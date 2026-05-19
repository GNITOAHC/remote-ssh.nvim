use std::ffi::OsString;
use clap::{Parser, Subcommand};
use common::{DEFAULT_PORT, DEFAULT_SERVER_PATH};

#[derive(Parser, Debug)]
#[command(name = "rnvim", version, about = "Edit remote files with local Neovim over SSH")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Manage saved sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Connect to [user@]host — any unrecognized subcommand is treated as the host
    #[command(external_subcommand)]
    Connect(Vec<OsString>),
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// List all saved sessions
    List,
    /// Add a session manually
    Add {
        /// Remote host in [user@]host format
        host: String,
        /// Working directory on the remote
        #[arg(long)]
        dir: String,
        /// Optional friendly name for this session
        #[arg(long)]
        name: Option<String>,
        /// TCP port for the nvim socket
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        /// Path to rnvim-server on the remote
        #[arg(long, default_value = DEFAULT_SERVER_PATH)]
        server_path: String,
    },
    /// Remove a session — accepts index, name, or host:dir
    Rm {
        /// Index from `session list`, session name, or "host:dir"
        target: String,
    },
}

/// Parsed from the external_subcommand catch-all args.
#[derive(Parser, Debug)]
#[command(name = "rnvim")]
pub struct ConnectArgs {
    /// Remote host in [user@]host format
    pub host: String,

    /// TCP port used for the nvim socket
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Absolute path on the remote where the server binary is installed
    #[arg(long, default_value = DEFAULT_SERVER_PATH)]
    pub server_path: String,

    /// Force directory prompt even if saved sessions exist for this host
    #[arg(long)]
    pub new_session: bool,
}
