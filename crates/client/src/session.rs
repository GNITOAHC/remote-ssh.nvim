use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub name: Option<String>,
    pub host: String,
    pub directory: String,
    pub port: u16,
    pub server_path: String,
    pub last_used: u64,
}

impl Session {
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => format!("{} ({}:{})", n, self.host, self.directory),
            None => format!("{}:{}", self.host, self.directory),
        }
    }
}

fn sessions_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from(
            std::env::var("HOME").unwrap_or_default() + "/.local/share"
        ))
        .join("rnvim")
        .join("sessions.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load() -> Vec<Session> {
    let path = sessions_path();
    let Ok(data) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save(sessions: &[Session]) {
    let path = sessions_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(sessions) {
        let _ = std::fs::write(&path, json);
    }
}

pub fn sessions_for_host(host: &str) -> Vec<Session> {
    let mut sessions: Vec<_> = load()
        .into_iter()
        .filter(|s| s.host == host)
        .collect();
    sessions.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    sessions
}

pub fn add(host: &str, dir: &str, port: u16, server_path: &str, name: Option<&str>) {
    let mut sessions = load();
    if let Some(existing) = sessions.iter_mut().find(|s| s.host == host && s.directory == dir) {
        existing.last_used = now_secs();
        if name.is_some() {
            existing.name = name.map(str::to_string);
        }
    } else {
        sessions.push(Session {
            name: name.map(str::to_string),
            host: host.to_string(),
            directory: dir.to_string(),
            port,
            server_path: server_path.to_string(),
            last_used: now_secs(),
        });
    }
    save(&sessions);
}

/// Resolve target string to an index in the full session list.
/// Accepts: integer index, session name, or "host:dir".
pub fn resolve(target: &str) -> Result<usize> {
    let sessions = load();

    // Try integer index first.
    if let Ok(idx) = target.parse::<usize>() {
        if idx < sessions.len() {
            return Ok(idx);
        }
        return Err(anyhow!("index {} out of range (have {} sessions)", idx, sessions.len()));
    }

    // Try name match.
    if let Some(idx) = sessions.iter().position(|s| s.name.as_deref() == Some(target)) {
        return Ok(idx);
    }

    // Try "host:dir" match — split on first colon.
    if let Some(colon) = target.find(':') {
        let host = &target[..colon];
        let dir = &target[colon + 1..];
        if let Some(idx) = sessions.iter().position(|s| s.host == host && s.directory == dir) {
            return Ok(idx);
        }
    }

    Err(anyhow!("no session matching '{}'", target))
}

pub fn remove(target: &str) -> Result<()> {
    let idx = resolve(target)?;
    let mut sessions = load();
    sessions.remove(idx);
    save(&sessions);
    Ok(())
}

pub fn mark_used(host: &str, dir: &str) {
    let mut sessions = load();
    if let Some(s) = sessions.iter_mut().find(|s| s.host == host && s.directory == dir) {
        s.last_used = now_secs();
    }
    save(&sessions);
}

pub fn handle_action(action: crate::cli::SessionAction) -> Result<()> {
    use crate::cli::SessionAction;
    match action {
        SessionAction::List => {
            let sessions = load();
            if sessions.is_empty() {
                println!("No saved sessions.");
            } else {
                for (i, s) in sessions.iter().enumerate() {
                    println!("[{}] {}", i, s.label());
                }
            }
        }
        SessionAction::Add { host, dir, name, port, server_path } => {
            add(&host, &dir, port, &server_path, name.as_deref());
            println!("Session saved: {} → {}", host, dir);
        }
        SessionAction::Rm { target } => {
            remove(&target)?;
            println!("Session '{}' removed.", target);
        }
    }
    Ok(())
}
