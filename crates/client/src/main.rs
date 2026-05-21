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
    } else if let Some(target) = cli.connect.host.clone() {
        // Try to resolve as saved session (by index or name), fall back to host connect.
        if let Ok(idx) = session::resolve(&target) {
            let sess = session::load()[idx].clone();
            run_connect_session(sess, cli.connect).await
        } else {
            run_connect(target, cli.connect).await
        }
    } else {
        let sessions = session::load();
        if sessions.is_empty() {
            Cli::command().print_help()?;
            println!();
            Ok(())
        } else {
            run_global_session_picker(sessions, cli.connect).await
        }
    }
}

/// Return the default local nvim config directory for the current OS.
/// Linux/macOS: ~/.config/nvim (XDG spec — macOS nvim follows XDG, not ~/Library)
/// Windows:     %LOCALAPPDATA%\nvim
fn default_nvim_config_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .map(|p| p.join("nvim").to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                format!(
                    "{}/AppData/Local/nvim",
                    std::env::var("USERPROFILE").unwrap_or_default()
                )
            })
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("{}/.config/nvim", std::env::var("HOME").unwrap_or_default())
    }
}

fn rsync_available() -> bool {
    std::process::Command::new("rsync")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Determine sync paths for this run. Returns None if sync is disabled.
/// Precedence: --no-sync-config > --sync-config > session preference.
/// Paths: CLI arg > session stored > default (~/.config/nvim).
fn resolve_sync(
    args: &ConnectArgs,
    session_sync: bool,
    session_local: Option<&str>,
    session_remote: Option<&str>,
) -> Option<(String, String)> {
    if args.no_sync_config {
        return None;
    }
    if !args.sync_config && !session_sync {
        return None;
    }

    let local_path = args.local_config.as_deref()
        .or(session_local)
        .map(str::to_string)
        .unwrap_or_else(default_nvim_config_dir);

    let remote_path = args.remote_config.as_deref()
        .or(session_remote)
        .map(str::to_string)
        .unwrap_or_else(|| "~/.config/nvim".to_string());

    Some((local_path, remote_path))
}

async fn run_global_session_picker(sessions: Vec<session::Session>, args: ConnectArgs) -> Result<()> {
    let choice = tokio::task::block_in_place(|| prompt::pick_session(&sessions));
    match choice {
        Some(idx) => run_connect_session(sessions[idx].clone(), args).await,
        None => Ok(()),
    }
}

async fn run_connect_session(sess: session::Session, args: ConnectArgs) -> Result<()> {
    info!("remote-ssh v{}", common::VERSION);
    info!("target: {}  port: {}  server: {}", sess.host, sess.port, sess.server_path);

    let has_rsync = rsync_available();
    let conn = ssh::SshConn::connect(&sess.host).await?;

    let http = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("failed to build HTTP client")?;

    bootstrap::ensure_server(&conn, &sess.server_path, &http).await?;

    session::mark_used(&sess.host, &sess.directory);
    info!("Working directory: {}", sess.directory);

    if let Some((local_path, remote_path)) = resolve_sync(
        &args,
        sess.sync_config,
        sess.local_config.as_deref(),
        sess.remote_config.as_deref(),
    ) {
        if has_rsync {
            conn.sync_config(&local_path, &remote_path)
                .await
                .context("config sync failed")?;
        } else {
            eprintln!(
                "Warning: rsync not found — skipping config sync. \
                 Install rsync or pass --no-sync-config to suppress this warning."
            );
        }
    }

    conn.start_remote_server(&sess.server_path, sess.port, Some(&sess.directory)).await?;

    let mut tunnel = conn.start_port_forward(sess.port).await?;
    info!("tunnel ready: localhost:{} -> {}:{}", sess.port, sess.host, sess.port);

    sleep(Duration::from_secs(1)).await;

    let server_addr = format!("localhost:{}", sess.port);
    info!("connecting: nvim --remote-ui --server {}", server_addr);

    let status = tokio::process::Command::new("nvim")
        .args(["--remote-ui", "--server", &server_addr])
        .status()
        .await
        .context("failed to spawn nvim — is nvim installed?")?;

    tunnel.kill().await.ok();
    tunnel.wait().await.ok();
    info!("tunnel closed.");

    if !status.success() {
        anyhow::bail!("nvim exited: {}", status);
    }
    Ok(())
}

async fn run_connect(host: String, args: ConnectArgs) -> Result<()> {
    let target = &host;
    let port = args.port;
    let server_path = &args.server_path;

    info!("remote-ssh v{}", common::VERSION);
    info!("target: {}  port: {}  server: {}", target, port, server_path);

    let has_rsync = rsync_available();

    // Step 1: Establish SSH connection (handles password / host-key prompts interactively).
    let conn = ssh::SshConn::connect(target).await?;

    let http = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .context("failed to build HTTP client")?;

    // Step 2: Install server binary if missing.
    bootstrap::ensure_server(&conn, server_path, &http).await?;

    // Step 3: Determine working directory via session memory or interactive prompt.
    let (working_dir, session_sync, session_local, session_remote) =
        resolve_working_dir(&conn, target, &args, has_rsync).await?;
    info!("Working directory: {}", working_dir);

    // Step 3.5: Sync local nvim config if requested.
    if let Some((local_path, remote_path)) = resolve_sync(
        &args,
        session_sync,
        session_local.as_deref(),
        session_remote.as_deref(),
    ) {
        if has_rsync {
            conn.sync_config(&local_path, &remote_path)
                .await
                .context("config sync failed")?;
        } else {
            eprintln!(
                "Warning: rsync not found — skipping config sync. \
                 Install rsync or pass --no-sync-config to suppress this warning."
            );
        }
    }

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
/// Returns (dir, sync_config, local_config, remote_config).
/// - Existing sessions for this host → show picker (unless --new-session).
/// - No sessions or user chose "New session" → interactive prompt with autocomplete.
async fn resolve_working_dir(
    conn: &ssh::SshConn,
    host: &str,
    args: &ConnectArgs,
    has_rsync: bool,
) -> Result<(String, bool, Option<String>, Option<String>)> {
    let existing = session::sessions_for_host(host);

    if !existing.is_empty() && !args.new_session {
        let choice = tokio::task::block_in_place(|| prompt::pick_or_new_session(&existing));
        if let Some(idx) = choice {
            let sess = &existing[idx];
            let dir = sess.directory.clone();
            let sync = sess.sync_config;
            let local = sess.local_config.clone();
            let remote = sess.remote_config.clone();
            if !args.no_save_session {
                session::mark_used(host, &dir);
            }
            return Ok((dir, sync, local, remote));
        }
        // User chose "New session" — fall through to prompt.
    }

    // Interactive directory prompt with remote autocomplete.
    let dir = prompt::prompt_directory(conn).await?;

    if args.no_save_session {
        return Ok((dir, false, None, None));
    }

    let want_sync = tokio::task::block_in_place(|| {
        if has_rsync {
            prompt::prompt_sync_config()
        } else {
            eprintln!("Note: rsync not found on this machine — config sync unavailable.");
            false
        }
    });

    let name = tokio::task::block_in_place(prompt::prompt_name);
    let local = args.local_config.clone();
    let remote = args.remote_config.clone();

    session::add(
        host,
        &dir,
        args.port,
        &args.server_path,
        name.as_deref(),
        want_sync,
        local.clone(),
        remote.clone(),
    );

    Ok((dir, want_sync, local, remote))
}
