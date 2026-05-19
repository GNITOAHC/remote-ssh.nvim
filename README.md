# remote-ssh.nvim

The effortless Neovim remote development experience.

**remote-ssh.nvim** brings a true VSCode-like Remote-SSH experience to Neovim. No local plugins, no complex configuration. Just run `rnvim user@host` and start coding with your local UI connected to a high-performance remote backend.

## Why remote-ssh.nvim?

- **Zero Local Footprint**: Unlike other solutions, it requires **no local Neovim plugins**. Your local `init.lua` stays clean and untouched.
- **True Remote Execution**: All LSPs, plugins, and heavy indexing run on the remote server. Your local machine only renders the UI—saving battery and keeping your fans quiet.
- **Native & Blazing Fast**: Leverages Neovim's built-in RPC and persistent SSH ControlMaster for a low-latency, responsive editing experience.
- **Automatic Server Management**: The client automatically bootstraps the server binary on the remote host—no manual installation needed.

## Comparison with existing tools

| Feature              | `remote-sshfs.nvim`   | `remote-nvim.nvim` | `distant.nvim`  | **remote-ssh.nvim**      |
| :------------------- | :-------------------- | :----------------- | :-------------- | :----------------------- |
| **Local Plugins**    | Required              | Required           | Required        | **None**                 |
| **LSP Location**     | Local (Slow indexing) | Remote             | Remote          | **Remote**               |
| **Protocol**         | SSHFS (High latency)  | Neovim RPC         | Custom Protocol | **Neovim RPC + SSH**     |
| **Remote Footprint** | None                  | Auto-installs Nvim | Needs daemon    | **Auto-installs server** |

- **vs. [remote-sshfs.nvim](https://github.com/nosduco/remote-sshfs.nvim)**: `remote-sshfs` mounts remote files locally, forcing your local LSPs to index files over the network. `remote-ssh.nvim` runs everything remotely, sending only UI updates—making it far superior for large projects.
- **vs. [remote-nvim.nvim](https://github.com/amitds1997/remote-nvim.nvim)**: While both use Neovim's remote UI, `remote-nvim.nvim` is a local Neovim plugin that manages connections. `remote-ssh.nvim` is a standalone CLI tool, ensuring your local Neovim configuration remains independent and portable.
- **vs. [distant.nvim](https://github.com/chipsenkbeil/distant.nvim)**: `distant` requires a custom daemon and optimized protocol. `remote-ssh.nvim` sticks to standard SSH and native Neovim features, making it simpler to use and maintain without background processes.

## How it works

`rnvim user@host` orchestrates the entire lifecycle:

```
rnvim user@host
  │
  ├─ 1. SSH ControlMaster — Auth once, reuse for everything
  ├─ 2. Auto-Bootstrap — Installs rnvim-server on remote if missing
  ├─ 3. Session Picker — Pick a saved workspace or start a new one
  ├─ 4. Start Server — Launches headless Neovim on the server
  ├─ 5. Tunneling — Securely forwards the RPC port to localhost
  └─ 6. Connect — Hooks up your local nvim UI to the remote instance
```

## Quick Start

### Installation

#### 1. Download from GitHub Releases (Recommended)

Download the latest `rnvim` binary for your platform from the [Releases](https://github.com/GNITOAHC/remote-ssh.nvim/releases) page and place it in your PATH.

#### 2. Build from Source

If you have the Rust toolchain installed, you can build it yourself:

```bash
cargo build -p rnvim --release
cp ./target/release/rnvim ~/.local/bin/rnvim
```

### Basic Usage

```bash
# Connect and start a session
rnvim user@host
```

### Session Management

`rnvim` remembers your remote workspaces so you can jump back in instantly.

```bash
# List all saved sessions
rnvim session list

# Remove a session
rnvim session rm <id/name>
```

## Supported platforms (remote)

| OS    | Architecture                  |
| ----- | ----------------------------- |
| Linux | x86_64, aarch64               |
| macOS | x86_64, arm64 (Apple Silicon) |

## Development

For architecture details and build instructions, see [CONTRIBUTING.md](./CONTRIBUTING.md).
