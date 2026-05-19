use anyhow::Result;
use common::platform_from_uname;
use reqwest::Client;
use tracing::info;

use crate::{github, ssh};

/// Ensure the server binary exists and is executable on the remote.
/// Downloads and installs it from GitHub Releases if missing.
pub async fn ensure_server(conn: &ssh::SshConn, server_path: &str, http: &Client) -> Result<()> {
    let exists = conn.check_server_exists(server_path).await?;
    if exists {
        info!("Server binary already present at {}", server_path);
        return Ok(());
    }

    info!("Server binary not found — bootstrapping...");

    let uname = conn.detect_remote_platform().await?;
    info!("Remote platform: {}", uname);

    let platform = platform_from_uname(&uname).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported remote platform: '{}'. Supported: Linux x86_64, Linux aarch64, Darwin x86_64, Darwin arm64",
            uname
        )
    })?;

    let binary = github::download_server_binary(http, platform).await?;
    conn.upload_binary(server_path, &binary).await?;

    info!("Bootstrap complete.");
    Ok(())
}
