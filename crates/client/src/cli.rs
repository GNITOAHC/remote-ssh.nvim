use clap::Parser;
use common::{DEFAULT_PORT, DEFAULT_SERVER_PATH};

#[derive(Parser, Debug)]
#[command(
    name = "rnvim",
    version,
    about = "Edit remote files with local Neovim over SSH",
)]
pub struct Cli {
    /// Remote host: [user@]host
    pub host: String,

    /// TCP port used for the nvim socket.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Absolute path on the remote where the server binary is installed.
    #[arg(long, default_value = DEFAULT_SERVER_PATH)]
    pub server_path: String,
}

impl Cli {
    /// Return the ssh target string as provided (user@host or just host).
    pub fn ssh_target(&self) -> &str {
        &self.host
    }
}
