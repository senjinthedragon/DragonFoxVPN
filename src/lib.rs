// lib.rs - DragonFoxVPN: Library crate root
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Exposes all application modules as a library so that integration tests
// in tests/ can import them. The binary (main.rs) calls into this crate.

pub mod api;
pub mod locale;
pub mod notifications;
pub mod app;
pub mod autostart;
pub mod daemon_ipc;
pub mod config;
pub mod icons;
pub mod state;
pub mod system;
pub mod vpn_runtime;

/// Returns false if another instance of DragonFoxVPN is already running.
pub fn single_instance_check() -> bool {
    #[cfg(target_os = "windows")]
    {
        return single_instance_check_windows();
    }
    #[cfg(not(target_os = "windows"))]
    {
        single_instance_check_unix()
    }
}

#[cfg(target_os = "windows")]
fn single_instance_check_windows() -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq DragonFoxVPN.exe", "/NH", "/FO", "CSV"])
        .output();
    match output {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let count = text
                .lines()
                .filter(|l| l.contains("DragonFoxVPN.exe"))
                .count();
            count <= 1
        }
        Err(_) => true,
    }
}

#[cfg(not(target_os = "windows"))]
fn single_instance_check_unix() -> bool {
    match std::process::Command::new("pgrep")
        .args(["-x", "DragonFoxVPN"])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let my_pid = std::process::id().to_string();
            let other_pids: Vec<&str> = stdout
                .lines()
                .filter(|pid| pid.trim() != my_pid.as_str())
                .collect();
            other_pids.is_empty()
        }
        Err(_) => true,
    }
}
