use anyhow::{Context, Result};
use clap::Parser;
use common::DEFAULT_PORT;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "rnvim-server", version, about = "Thin nvim headless server launcher")]
struct Args {
    /// TCP port for nvim to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,

    /// Host/address for nvim to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let listen_addr = format!("{}:{}", args.host, args.port);

    // Try nvim from PATH, then fall back to common install locations.
    let nvim = find_nvim().context("nvim not found — ensure nvim is installed and in PATH")?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // exec() replaces this process image: no wrapper in ps, no extra memory.
        let err = Command::new(&nvim)
            .args(["--headless", "--listen", &listen_addr])
            .exec();
        Err(anyhow::Error::from(err))
            .with_context(|| format!("failed to exec {}", nvim))
    }

    #[cfg(not(unix))]
    {
        let status = Command::new(&nvim)
            .args(["--headless", "--listen", &listen_addr])
            .status()
            .with_context(|| format!("failed to spawn {}", nvim))?;
        if !status.success() {
            anyhow::bail!("nvim exited with status: {}", status);
        }
        Ok(())
    }
}

fn find_nvim() -> Option<String> {
    let candidates = [
        "nvim",
        "/usr/bin/nvim",
        "/usr/local/bin/nvim",
        "/opt/homebrew/bin/nvim",
        "/home/linuxbrew/.linuxbrew/bin/nvim",
    ];
    for candidate in candidates {
        if std::process::Command::new(candidate)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
        {
            return Some(candidate.to_string());
        }
    }
    None
}
