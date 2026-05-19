use anyhow::Result;
use common::platform_from_uname;
use reqwest::Client;
use tracing::info;

use crate::{github, ssh};

/// Ensure the server binary exists and is executable on the remote.
/// Downloads and installs it from GitHub Releases if missing.
pub async fn ensure_server(target: &str, server_path: &str, http: &Client) -> Result<()> {
    let exists = ssh::check_server_exists(target, server_path).await?;
    if exists {
        info!("Server binary already present at {}:{}", target, server_path);
        return Ok(());
    }

    info!("Server binary not found — bootstrapping...");

    let uname = ssh::detect_remote_platform(target).await?;
    info!("Remote platform: {}", uname);

    let platform = platform_from_uname(&uname).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported remote platform: '{}'. Supported: Linux x86_64, Linux aarch64, Darwin x86_64, Darwin arm64",
            uname
        )
    })?;

    let binary = github::download_server_binary(http, platform).await?;
    ssh::upload_binary(target, server_path, &binary).await?;

    info!("Bootstrap complete.");
    Ok(())
}
