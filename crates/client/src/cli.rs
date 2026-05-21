use clap::{Args, Parser, Subcommand};
use common::{DEFAULT_PORT, DEFAULT_SERVER_PATH};

#[derive(Parser, Debug)]
#[command(name = "rnvim", version, about = "Edit remote files with local Neovim over SSH")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Cmd>,

    #[command(flatten)]
    pub connect: ConnectArgs,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Manage saved sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionAction {
    /// List all saved sessions
    #[command(alias = "ls")]
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
        /// Enable config sync for this session
        #[arg(long)]
        sync_config: bool,
    },
    /// Remove a session — accepts index, name, or host:dir
    Rm {
        /// Index from `session list`, session name, or "host:dir"
        target: String,
    },
}

#[derive(Args, Debug)]
pub struct ConnectArgs {
    /// Remote host in [user@]host format
    pub host: Option<String>,

    /// TCP port used for the nvim socket
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Absolute path on the remote where the server binary is installed
    #[arg(long, default_value = DEFAULT_SERVER_PATH)]
    pub server_path: String,

    /// Force directory prompt even if saved sessions exist for this host
    #[arg(long)]
    pub new_session: bool,

    /// Do not save this connection as a session for future use
    #[arg(long, short = 'n')]
    pub no_save_session: bool,

    /// Sync local ~/.config/nvim to remote before connecting
    #[arg(long)]
    pub sync_config: bool,

    /// Skip config sync even if the session has sync enabled
    #[arg(long, conflicts_with = "sync_config")]
    pub no_sync_config: bool,

    /// Local nvim config directory to sync (default: ~/.config/nvim)
    #[arg(long)]
    pub local_config: Option<String>,

    /// Remote nvim config directory to sync to (default: ~/.config/nvim)
    #[arg(long)]
    pub remote_config: Option<String>,
}
