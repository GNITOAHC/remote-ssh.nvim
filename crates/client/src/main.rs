mod bootstrap;
mod cli;
mod github;
mod ssh;

use anyhow::{Context, Result};
use clap::Parser;
use cli::Cli;
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

    let args = Cli::parse();
    let target = args.ssh_target();
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

    // Step 3: Start the remote headless nvim server.
    conn.start_remote_server(server_path, port).await?;

    // Step 4: Open the SSH port-forward tunnel.
    let mut tunnel = conn.start_port_forward(port).await?;
    info!("tunnel ready: localhost:{} -> {}:{}", port, target, port);

    // Step 5: Give nvim a moment to start listening.
    sleep(Duration::from_secs(1)).await;

    // Step 6: Spawn (not exec) the local nvim UI so we can kill the tunnel after it exits.
    let server_addr = format!("localhost:{}", port);
    info!("connecting: nvim --remote-ui --server {}", server_addr);

    let status = tokio::process::Command::new("nvim")
        .args(["--remote-ui", "--server", &server_addr])
        .status()
        .await
        .context("failed to spawn nvim — is nvim installed?")?;

    // Step 7: nvim exited — tear down the SSH tunnel and connection.
    tunnel.kill().await.ok();
    tunnel.wait().await.ok();
    info!("tunnel closed.");

    if !status.success() {
        anyhow::bail!("nvim exited: {}", status);
    }
    Ok(())
}
