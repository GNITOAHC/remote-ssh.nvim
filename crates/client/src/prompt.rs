use anyhow::Result;
use inquire::autocompletion::Replacement;
use inquire::{Autocomplete, Confirm, CustomUserError, Select, Text};
use std::borrow::Cow;
use std::process::Stdio;

use crate::session::Session;
use crate::ssh::SshConn;

// ---------------------------------------------------------------------------
// Remote directory completer
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct RemoteDirCompleter {
    target: String,
    socket: String,
    /// Remote $HOME — fetched once so ~ expansion is consistent.
    home_dir: String,
    /// (last_parent_queried, cached_results)
    /// SSH only called when the parent directory portion of the input changes.
    cache: Option<(String, Vec<String>)>,
}

impl RemoteDirCompleter {
    fn new(target: String, socket: String, home_dir: String) -> Self {
        Self { target, socket, home_dir, cache: None }
    }

    /// Expand a leading `~` to the remote $HOME so all paths are absolute.
    fn expand_home<'a>(&self, input: &'a str) -> Cow<'a, str> {
        if input.starts_with('~') && !self.home_dir.is_empty() {
            Cow::Owned(format!("{}{}", self.home_dir, &input[1..]))
        } else {
            Cow::Borrowed(input)
        }
    }

    /// Returns everything up to and including the last '/'.
    /// "/home/user" → "/home/",  "/home/" → "/home/",  "" → "".
    fn parent_of(input: &str) -> &str {
        match input.rfind('/') {
            Some(pos) => &input[..=pos],
            None => "",
        }
    }

    /// List all immediate subdirectories of `parent` on the remote.
    /// Uses `find` so hidden dirs (`.config`, `.local`) are included.
    /// Result is cached; re-queried only when `parent` changes.
    fn dirs_for_parent(&mut self, parent: &str) -> Vec<String> {
        if let Some((cached_parent, cached)) = &self.cache {
            if cached_parent == parent {
                return cached.clone();
            }
        }
        let cmd = Self::find_cmd(parent);
        let results = self.run_ssh(&cmd);
        self.cache = Some((parent.to_string(), results.clone()));
        results
    }

    /// Build the remote shell command to list immediate subdirectories.
    /// `find` is used instead of `ls */` so hidden directories are included.
    fn find_cmd(parent: &str) -> String {
        if parent.is_empty() {
            // Relative input, no leading '/': list cwd.
            r#"find . -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort | head -50 || true"#
                .to_string()
        } else {
            // Single-quote the path (strip any embedded quotes to avoid injection).
            let safe = parent.replace('\'', "");
            format!(
                r#"find '{safe}' -maxdepth 1 -mindepth 1 -type d 2>/dev/null | sort | head -50 || true"#
            )
        }
    }

    fn run_ssh(&self, cmd: &str) -> Vec<String> {
        let out = std::process::Command::new("ssh")
            .args([
                "-o",
                "ControlMaster=no",
                "-o",
                &format!("ControlPath={}", self.socket),
                &self.target,
                cmd,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();

        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.trim_end_matches('/').to_string() + "/")
                .collect(),
            Err(_) => vec![],
        }
    }
}

impl Autocomplete for RemoteDirCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        // Expand ~ before computing parent, so all queries use absolute paths.
        let expanded = self.expand_home(input);

        let parent = if expanded.is_empty() {
            "/".to_string()
        } else {
            Self::parent_of(&expanded).to_string()
        };

        let all = self.dirs_for_parent(&parent);

        // Client-side filter — instant, no SSH.
        let filtered: Vec<String> = if expanded.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|d| d.starts_with(expanded.as_ref()))
                .collect()
        };

        Ok(filtered)
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        // Use explicitly highlighted suggestion, or fallback to the first match.
        let target = if let Some(dir) = highlighted_suggestion {
            Some(dir)
        } else {
            let expanded = self.expand_home(input);
            let parent = if expanded.is_empty() {
                "/".to_string()
            } else {
                Self::parent_of(&expanded).to_string()
            };

            let all = self.dirs_for_parent(&parent);

            if expanded.is_empty() {
                all.first().cloned()
            } else {
                all.into_iter().find(|d| d.starts_with(expanded.as_ref()))
            }
        };

        // When Tab is pressed, we return the path WITHOUT the trailing slash. 
        // This forces the user to naturally type '/' to enter the directory, 
        // which acts as a fresh keystroke and triggers inquire's internal 
        // `on_change` event to refresh the dropdown with children.
        if let Some(dir) = target {
            // Pre-warm cache for the selected dir so the next get_suggestions
            // call is an instant cache hit instead of a blocking SSH round-trip.
            self.dirs_for_parent(&dir);
            Ok(Some(dir.trim_end_matches('/').to_string()))
        } else {
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Prompt user to type a remote working directory.
/// Shows a live dropdown of matching dirs (including hidden); SSH queried
/// only when parent dir changes, filtered client-side while typing.
pub async fn prompt_directory(conn: &SshConn) -> Result<String> {
    // Fetch remote $HOME once for consistent ~ expansion.
    let home_dir = conn.run_remote("echo $HOME").await.unwrap_or_default();

    let completer = RemoteDirCompleter::new(
        conn.target.clone(),
        conn.socket.to_string_lossy().into_owned(),
        home_dir,
    );

    tokio::task::block_in_place(|| {
        Text::new("Remote working directory:")
            .with_autocomplete(completer)
            .with_help_message("↑↓ navigate  ·  Tab select  ·  Enter confirm")
            .prompt()
            .map_err(|e| anyhow::anyhow!("{e}"))
    })
}

/// Prompt for an optional session name. Enter skips.
pub fn prompt_name() -> Option<String> {
    Text::new("Session name (optional — Enter to skip):")
        .prompt_skippable()
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

/// Ask whether to sync local nvim config to remote for this session.
pub fn prompt_sync_config() -> bool {
    Confirm::new("Sync local ~/.config/nvim to remote before connecting?")
        .with_default(false)
        .with_help_message("Uses rsync over the existing SSH connection")
        .prompt()
        .unwrap_or(false)
}

/// Show a global session list with "+ New session" at the bottom.
/// Returns `Some(Some(idx))` for an existing session, `Some(None)` for new, `None` on cancel.
pub fn pick_session(sessions: &[Session]) -> Option<Option<usize>> {
    let mut items: Vec<String> = sessions.iter().map(|s| s.label()).collect();
    items.push("+ New session".to_string());

    match Select::new("Select session:", items.clone()).prompt() {
        Ok(ans) if ans == "+ New session" => Some(None),
        Ok(ans) => Some(items.iter().position(|i| *i == ans)),
        Err(_) => None,
    }
}

/// Prompt for a remote host in [user@]host format.
pub fn prompt_host() -> Option<String> {
    Text::new("Remote host ([user@]host):")
        .prompt()
        .ok()
        .filter(|s| !s.is_empty())
}

/// Show a selection list of saved sessions + "New session" option.
/// Returns `Some(index)` for a saved session, `None` for "New session".
pub fn pick_or_new_session(sessions: &[Session]) -> Option<usize> {
    let mut items: Vec<String> = sessions.iter().map(|s| s.label()).collect();
    items.push("+ New session".to_string());

    let ans = Select::new("Select session:", items.clone())
        .prompt()
        .ok()?;

    if ans == "+ New session" {
        None
    } else {
        items.iter().position(|i| *i == ans)
    }
}
