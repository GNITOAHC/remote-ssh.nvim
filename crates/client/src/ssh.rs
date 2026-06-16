use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, info};

/// A live SSH connection via ControlMaster.
/// All commands reuse the mux socket — auth happens once at `connect()`.
pub struct SshConn {
    pub(crate) target: String,
    pub(crate) socket: PathBuf,
}

impl SshConn {
    /// Establish the ControlMaster connection.
    /// All stdio is inherited so SSH can prompt for passwords and host-key confirmation.
    pub async fn connect(target: &str) -> Result<Self> {
        let socket = std::env::temp_dir().join(format!("rnvim-{}.sock", std::process::id()));
        let socket_arg = format!("ControlPath={}", socket.to_string_lossy());

        info!("Connecting to {}...", target);
        let status = Command::new("ssh")
            .args([
                "-o", "ControlMaster=yes",
                "-o", &socket_arg,
                "-o", "ControlPersist=yes",
                target,
                "true",
            ])
            // All stdio inherited: SSH can open /dev/tty for password / host-key prompts.
            .status()
            .await
            .context("failed to spawn ssh")?;

        if !status.success() {
            bail!(
                "SSH connection to '{}' failed (exit {}).\nCheck hostname, port, and credentials.",
                target,
                status
            );
        }

        info!("Connected.");
        Ok(Self { target: target.to_string(), socket })
    }

    /// Returns the four SSH args that route through the ControlMaster socket.
    /// Reused by every subsequent SSH/rsync call so auth only happens once.
    fn ctl_args(&self) -> [String; 4] {
        [
            "-o".into(),
            "ControlMaster=no".into(),
            "-o".into(),
            format!("ControlPath={}", self.socket.to_string_lossy()),
        ]
    }

    /// Run a command on the remote and return trimmed stdout.
    pub async fn run_remote(&self, remote_cmd: &str) -> Result<String> {
        debug!("ssh {} {:?}", self.target, remote_cmd);
        let [a, b, c, d] = self.ctl_args();
        let out = Command::new("ssh")
            .args([&a, &b, &c, &d, &self.target, remote_cmd])
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
    pub async fn check_server_exists(&self, server_path: &str) -> Result<bool> {
        let cmd = format!(
            r#"test -f {path} && echo EXISTS || echo MISSING"#,
            path = server_path
        );
        let output = self.run_remote(&cmd).await?;
        Ok(output.trim() == "EXISTS")
    }

    /// Returns the raw `uname -sm` string from the remote, e.g. "Linux x86_64".
    pub async fn detect_remote_platform(&self) -> Result<String> {
        self.run_remote("uname -sm").await
    }

    /// Stream `bytes` to `remote_path` over SSH stdin using `cat >`, then chmod +x.
    pub async fn upload_binary(&self, remote_path: &str, bytes: &[u8]) -> Result<()> {
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

        info!("Uploading server binary to {}:{}", self.target, remote_path);
        let [a, b, c, d] = self.ctl_args();
        let mut child = Command::new("ssh")
            .args([&a, &b, &c, &d, &self.target, &cmd])
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to spawn ssh for upload")?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(bytes).await.context("failed to write binary to ssh stdin")?;
        }

        let status = child.wait().await.context("ssh upload process error")?;
        if !status.success() {
            bail!("upload failed with exit status: {}", status);
        }
        info!("Upload complete.");
        Ok(())
    }

    /// Start the server on the remote as a detached background process via `ssh -f`.
    /// If `working_dir` is set, the server runs inside that directory.
    /// If `appname` is set, `NVIM_APPNAME=<appname>` is prepended to the shell command so
    /// the server process (and the nvim it exec()s) inherits the correct config directory.
    pub async fn start_remote_server(
        &self,
        server_path: &str,
        port: u16,
        working_dir: Option<&str>,
        appname: Option<&str>,
    ) -> Result<()> {
        info!("Starting remote server on port {}...", port);
        let env_prefix = appname
            .map(|a| format!("NVIM_APPNAME={} ", a))
            .unwrap_or_default();
        let base_cmd = format!("{}{} --port {}", env_prefix, server_path, port);
        let inner = if let Some(dir) = working_dir {
            let dir = dir.replacen('~', "$HOME", 1);
            format!("cd {} && {}", dir, base_cmd)
        } else {
            base_cmd
        };
        // Interactive login shell: -l sources ~/.bash_profile, -i forces ~/.bashrc past the
        // common `case $- in *i*) ;; *) return;; esac` guard so nvm/mise/asdf init actually runs.
        let escaped = inner.replace('\'', r"'\''");
        let remote_cmd = format!("bash -ilc '{}'", escaped);
        let [a, b, c, d] = self.ctl_args();

        let status = Command::new("ssh")
            .args([&a, &b, &c, &d, "-f", "-n", &self.target, &remote_cmd])
            .status()
            .await
            .context("failed to spawn ssh -f")?;

        if !status.success() {
            bail!("failed to start remote server (exit status: {})", status);
        }
        Ok(())
    }

    /// Rsync local nvim config to the remote using the existing ControlMaster socket.
    pub async fn sync_config(&self, local_path: &str, remote_path: &str) -> Result<()> {
        let local_src = format!("{}/", local_path.trim_end_matches('/'));
        let dest = format!("{}:{}", self.target, remote_path);
        let ssh_cmd = format!(
            "ssh -o ControlMaster=no -o ControlPath={}",
            self.socket.to_string_lossy()
        );

        info!("Syncing config: {} -> {}:{}", local_src, self.target, remote_path);
        let status = tokio::process::Command::new("rsync")
            .args(["-az", "--delete", "-e", &ssh_cmd, &local_src, &dest])
            .status()
            .await
            .context("failed to spawn rsync — is rsync installed on this machine?")?;

        if !status.success() {
            bail!("config sync failed: rsync exited with status {}", status);
        }
        info!("Config sync complete.");
        Ok(())
    }

    /// Spawn a background SSH port-forward process.
    /// Caller must keep the returned `Child` alive for the duration of the session.
    pub async fn start_port_forward(&self, port: u16) -> Result<tokio::process::Child> {
        info!("Setting up SSH tunnel localhost:{port} -> {}:{port}", self.target);
        let forward_spec = format!("{port}:localhost:{port}");
        let [a, b, c, d] = self.ctl_args();

        let child = Command::new("ssh")
            .args([
                &a, &b, &c, &d,
                "-N",
                "-L", &forward_spec,
                "-o", "ExitOnForwardFailure=yes",
                &self.target,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to spawn SSH port-forward")?;
        Ok(child)
    }
}

impl Drop for SshConn {
    fn drop(&mut self) {
        let socket_arg = format!("ControlPath={}", self.socket.to_string_lossy());
        let _ = std::process::Command::new("ssh")
            .args(["-O", "exit", "-o", &socket_arg, &self.target])
            .output();
    }
}
