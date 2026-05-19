# Contributing to remote-ssh.nvim

## Architecture

Rust workspace with three crates:

| Crate           | Binary         | Role                                        |
| --------------- | -------------- | ------------------------------------------- |
| `crates/common` | —              | Shared constants and platform detection     |
| `crates/server` | `rnvim-server` | Finds nvim and `exec()`s it headless        |
| `crates/client` | `rnvim`        | Full lifecycle orchestration (async, tokio) |

All SSH calls spawn the system `ssh` binary directly, so `~/.ssh/config`, ControlMaster, agent forwarding, and jump hosts all work as expected. Auth happens once at startup; subsequent operations reuse the mux socket.

## Building

```bash
# Build everything
cargo build --workspace

# Release binaries (stripped, LTO, size-optimized)
cargo build --workspace --release

# Run tests
cargo test --workspace
```

## Manual Server Installation

The client auto-installs the server on first connect. To build and install manually on the remote:

```bash
# On the remote machine
cargo build -p rnvim-server --release
cp ./target/release/rnvim-server ~/.local/bin/rnvim-server
```
