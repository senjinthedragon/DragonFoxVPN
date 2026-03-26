// daemon_ipc.rs - DragonFoxVPN: File-based IPC between tray daemon and UI subprocesses
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// The tray daemon writes DaemonStatus to daemon_status.json so UI subprocess
// windows can display live VPN state without sharing memory. UI subprocesses
// write DaemonCommand entries to daemon_command.json; the tray daemon polls
// and acts on them (reconnect after a location switch, reload config after
// settings are saved, etc.).

use serde::{Deserialize, Serialize};

/// Cap the command queue so a slow-consuming daemon can't accumulate an unbounded backlog.
const MAX_COMMAND_QUEUE: usize = 32;

// --------------------------------------------------------------------------
// Daemon status  (daemon → UI)
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    /// Human-readable VPN state: "Connected", "Disabled", "Enabling",
    /// "Dropped", "ServerUnreachable", or "SetupIncomplete".
    pub state: String,
    pub adapter: String,
    pub location: String,
    pub vpn_gateway: Option<String>,
    /// Unix timestamp of when the current connection was established.
    pub connected_since_unix: Option<u64>,
    pub message: Option<String>,
    #[serde(default)]
    pub updated_unix: u64,
}

// --------------------------------------------------------------------------
// Daemon commands  (UI → daemon)
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonCommand {
    /// UI saved new settings - daemon should reload AppConfig.
    ReloadConfig,
    /// UI switched VPN location while connected - daemon should reconnect.
    Reconnect,
    /// UI changed a setting that requires a full restart (e.g. language).
    /// Daemon spawns a fresh instance of itself then exits.
    Restart,
    /// User closed the setup window without completing setup.
    /// Daemon should exit cleanly.
    Quit,
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

pub fn current_unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn save_daemon_status(status: &DaemonStatus) {
    let mut s = status.clone();
    s.updated_unix = current_unix_ts();
    if let Ok(json) = serde_json::to_string_pretty(&s) {
        write_atomic(daemon_status_path(), &json);
    }
}

pub fn load_daemon_status() -> Option<DaemonStatus> {
    let content = std::fs::read_to_string(daemon_status_path()).ok()?;
    serde_json::from_str::<DaemonStatus>(&content).ok()
}

pub fn write_daemon_command(cmd: DaemonCommand) {
    let path = daemon_command_path();
    let mut queue = read_command_queue(&path);
    queue.push(cmd);
    if queue.len() > MAX_COMMAND_QUEUE {
        let overflow = queue.len() - MAX_COMMAND_QUEUE;
        queue.drain(0..overflow);
    }
    if let Ok(json) = serde_json::to_string(&queue) {
        write_atomic(path, &json);
    }
}

pub fn take_daemon_command() -> Option<DaemonCommand> {
    let path = daemon_command_path();
    let mut queue = read_command_queue(&path);
    if queue.is_empty() {
        return None;
    }
    let cmd = queue.remove(0);
    if queue.is_empty() {
        // Delete the file rather than writing an empty array so the daemon's
        // existence-check polling stays cheap (stat vs. read+parse).
        let _ = std::fs::remove_file(&path);
    } else if let Ok(json) = serde_json::to_string(&queue) {
        write_atomic(path, &json);
    }
    Some(cmd)
}

pub fn clear_daemon_command() {
    let _ = std::fs::remove_file(daemon_command_path());
}

// --------------------------------------------------------------------------
// Paths
// --------------------------------------------------------------------------

fn daemon_status_path() -> std::path::PathBuf {
    crate::config::get_config_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("daemon_status.json")
}

fn daemon_command_path() -> std::path::PathBuf {
    crate::config::get_config_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("daemon_command.json")
}

// --------------------------------------------------------------------------
// Internal helpers
// --------------------------------------------------------------------------

fn write_atomic(path: std::path::PathBuf, content: &str) {
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, content).is_ok() {
        let _ = std::fs::rename(tmp, path);
    }
}

fn read_command_queue(path: &std::path::Path) -> Vec<DaemonCommand> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    if let Ok(queue) = serde_json::from_str::<Vec<DaemonCommand>>(&content) {
        return queue;
    }
    // Backward-compat: single command payload.
    if let Ok(single) = serde_json::from_str::<DaemonCommand>(&content) {
        return vec![single];
    }
    Vec::new()
}
