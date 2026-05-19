use anyhow::{Context, Result};
use common::{GITHUB_API_BASE, GITHUB_REPO};
use reqwest::Client;
use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

pub async fn fetch_latest_release(client: &Client) -> Result<Release> {
    let url = format!("{}/repos/{}/releases/latest", GITHUB_API_BASE, GITHUB_REPO);
    info!("Fetching latest release from {}", url);

    client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(
            "User-Agent",
            concat!("remote-ssh.nvim/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("failed to reach GitHub API")?
        .error_for_status()
        .context("GitHub API returned error status")?
        .json::<Release>()
        .await
        .context("failed to parse GitHub release JSON")
}

/// Download the server binary for `platform` (e.g. "linux-x86_64").
/// Returns the raw bytes.
pub async fn download_server_binary(client: &Client, platform: &str) -> Result<Vec<u8>> {
    let release = fetch_latest_release(client).await?;
    let asset_name = format!("rnvim-server-{}", platform);

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| {
            let available: Vec<_> = release.assets.iter().map(|a| &a.name).collect();
            format!(
                "no asset '{}' in release {}. Available: {:?}",
                asset_name, release.tag_name, available
            )
        })?;

    info!("Downloading {} ...", asset.browser_download_url);

    let bytes = client
        .get(&asset.browser_download_url)
        .header(
            "User-Agent",
            concat!("remote-ssh.nvim/", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await
        .context("failed to start binary download")?
        .error_for_status()
        .context("download URL returned error status")?
        .bytes()
        .await
        .context("failed to read download body")?;

    info!("Downloaded {} bytes.", bytes.len());
    Ok(bytes.to_vec())
}
