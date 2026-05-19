mod bootstrap;
mod cli;
mod github;
mod prompt;
mod session;
mod ssh;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use cli::{Cli, Cmd, ConnectArgs};
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        match cmd {
            Cmd::Session { action } => session::handle_action(action),
        }
    } else if let Some(host) = cli.connect.host.clone() {
        run_connect(host, cli.connect).await
    } else {
        Cli::command().print_help()?;
        println!(); // Add a newline after help
        Ok(())
    }
}

async fn run_connect(host: String, args: ConnectArgs) -> Result<()> {
    let target = &host;
    let port = args.port;
    let server_path = &args.server_path;

    info!("remote-ssh v{}", common::VERSION);
    info!("target: {}  port: {}  server: {}", target, port, server_path);

    // Step 1: Establish SSH connection (handles password / host-key prompts interactively).
    let conn = ssh::SshConn::connect(target).await?;

    let http = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("failed to build HTTP client")?;

    // Step 2: Install server binary if missing.
    bootstrap::ensure_server(&conn, server_path, &http).await?;

    // Step 3: Determine working directory via session memory or interactive prompt.
    let working_dir = resolve_working_dir(&conn, target, port, server_path, args.new_session, args.no_save_session).await?;
    info!("Working directory: {}", working_dir);

    // Step 4: Start the remote headless nvim server in the chosen directory.
    conn.start_remote_server(server_path, port, Some(&working_dir)).await?;

    // Step 5: Open the SSH port-forward tunnel.
    let mut tunnel = conn.start_port_forward(port).await?;
    info!("tunnel ready: localhost:{} -> {}:{}", port, target, port);

    // Step 6: Give nvim a moment to start listening.
    sleep(Duration::from_secs(1)).await;

    // Step 7: Spawn (not exec) the local nvim UI so we can kill the tunnel after it exits.
    let server_addr = format!("localhost:{}", port);
    info!("connecting: nvim --remote-ui --server {}", server_addr);

    let status = tokio::process::Command::new("nvim")
        .args(["--remote-ui", "--server", &server_addr])
        .status()
        .await
        .context("failed to spawn nvim — is nvim installed?")?;

    // Step 8: nvim exited — tear down tunnel and SSH connection.
    tunnel.kill().await.ok();
    tunnel.wait().await.ok();
    info!("tunnel closed.");

    if !status.success() {
        anyhow::bail!("nvim exited: {}", status);
    }
    Ok(())
}

/// Pick or prompt for the remote working directory.
/// - Existing sessions for this host → show picker (unless --new-session).
/// - No sessions or user chose "New session" → interactive prompt with autocomplete.
async fn resolve_working_dir(
    conn: &ssh::SshConn,
    host: &str,
    port: u16,
    server_path: &str,
    force_new: bool,
    no_save: bool,
) -> Result<String> {
    let existing = session::sessions_for_host(host);

    if !existing.is_empty() && !force_new {
        let choice = tokio::task::block_in_place(|| prompt::pick_or_new_session(&existing));
        if let Some(idx) = choice {
            let dir = existing[idx].directory.clone();
            if !no_save {
                session::mark_used(host, &dir);
            }
            return Ok(dir);
        }
        // User chose "New session" — fall through to prompt.
    }

    // Interactive directory prompt with remote autocomplete.
    let dir = prompt::prompt_directory(conn).await?;
    if !no_save {
        let name = tokio::task::block_in_place(prompt::prompt_name);
        session::add(host, &dir, port, server_path, name.as_deref());
    }
    Ok(dir)
}
