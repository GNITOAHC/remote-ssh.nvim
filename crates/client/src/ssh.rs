use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, info};

/// Run `ssh <target> <remote_cmd>` and return trimmed stdout.
/// Stderr is inherited so the user sees SSH prompts and errors.
pub async fn run_remote(target: &str, remote_cmd: &str) -> Result<String> {
    debug!("ssh {} {:?}", target, remote_cmd);
    let out = Command::new("ssh")
        .args([target, remote_cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .await
        .context("failed to spawn ssh")?;

    if !out.status.success() {
        bail!("ssh command failed (exit {}): {}", out.status, remote_cmd);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Returns true if the server binary exists on the remote.
pub async fn check_server_exists(target: &str, server_path: &str) -> Result<bool> {
    // Expand ~ manually because test -f doesn't expand tilde in all shells.
    let cmd = format!(
        r#"test -f {path} && echo EXISTS || echo MISSING"#,
        path = server_path
    );
    let output = run_remote(target, &cmd).await?;
    Ok(output.trim() == "EXISTS")
}

/// Returns the raw `uname -sm` string from the remote, e.g. "Linux x86_64".
pub async fn detect_remote_platform(target: &str) -> Result<String> {
    run_remote(target, "uname -sm").await
}

/// Stream `bytes` to `remote_path` over SSH stdin using `cat >`, then chmod +x.
/// Works on any remote with a POSIX shell — no scp required.
pub async fn upload_binary(target: &str, remote_path: &str, bytes: &[u8]) -> Result<()> {
    // Replace leading ~ with $HOME: tilde doesn't expand inside double quotes,
    // but $HOME does. Compute the parent dir in Rust to avoid dirname quoting issues.
    let shell_path = remote_path.replacen('~', "$HOME", 1);
    let parent = std::path::Path::new(remote_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".")
        .replacen('~', "$HOME", 1);
    let cmd = format!(
        "mkdir -p {parent} && cat > {path} && chmod +x {path}",
        parent = parent,
        path = shell_path,
    );

    info!("Uploading server binary to {}:{}", target, remote_path);
    let mut child = Command::new("ssh")
        .args([target, &cmd])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn ssh for upload")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(bytes).await.context("failed to write binary to ssh stdin")?;
        // Drop stdin to close the pipe so `cat` sees EOF.
    }

    let status = child.wait().await.context("ssh upload process error")?;
    if !status.success() {
        bail!("upload failed with exit status: {}", status);
    }
    info!("Upload complete.");
    Ok(())
}

/// Start the server on the remote as a detached background process via `ssh -f`.
pub async fn start_remote_server(target: &str, server_path: &str, port: u16) -> Result<()> {
    info!("Starting remote server on port {}...", port);
    let remote_cmd = format!("{} --port {}", server_path, port);

    let status = Command::new("ssh")
        .args([
            "-f",
            "-n",   // redirect stdin from /dev/null so ssh -f can background cleanly
            target,
            &remote_cmd,
        ])
        .status()
        .await
        .context("failed to spawn ssh -f")?;

    if !status.success() {
        bail!("failed to start remote server (exit status: {})", status);
    }
    Ok(())
}

/// Spawn a background SSH port-forward process.
/// Caller must keep the returned `Child` alive for the duration of the session.
pub async fn start_port_forward(target: &str, port: u16) -> Result<tokio::process::Child> {
    info!("Setting up SSH tunnel localhost:{port} -> {target}:{port}");
    let forward_spec = format!("{port}:localhost:{port}");
    let child = Command::new("ssh")
        .args([
            "-N",
            "-L", &forward_spec,
            "-o", "ExitOnForwardFailure=yes",
            target,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to spawn SSH port-forward")?;
    Ok(child)
}
