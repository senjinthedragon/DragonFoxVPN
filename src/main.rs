// main.rs - DragonFoxVPN: System tray VPN management application
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
//
// System tray utility for managing VPN connections through a Raspberry Pi
// OpenVPN gateway. Features a dark UI, location switching with flags,
// auto-connect, kill switch, and DNS leak protection.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod api;
mod app;
mod autostart;
mod config;
mod icons;
mod state;
mod system;

use log::warn;

fn main() {
    // Initialise logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    // Single-instance guard
    if !single_instance_check() {
        warn!("Another instance of DragonFoxVPN is already running. Exiting.");
        return;
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_visible(false)
            .with_decorations(false)
            .with_inner_size([1.0, 1.0])
            .with_title("DragonFoxVPN"),
        ..Default::default()
    };

    eframe::run_native(
        "DragonFoxVPN",
        options,
        Box::new(|cc| Ok(app::DragonFoxApp::new(cc))),
    )
    .unwrap_or_else(|e| {
        eprintln!("Fatal error: {e}");
        std::process::exit(1);
    });
}

/// Returns false if another instance is already running.
fn single_instance_check() -> bool {
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
    // On Windows, use a named mutex via winreg-adjacent approach:
    // We write a lock file to the temp directory and check it.
    // A proper implementation would use CreateMutexW from the Win32 API,
    // but to avoid an extra dependency we use a simpler approach:
    // check if another process with our name appears in the task list.
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
            // More than one means another instance (we're already running)
            count <= 1
        }
        Err(_) => true,
    }
}

#[cfg(not(target_os = "windows"))]
fn single_instance_check_unix() -> bool {
    // Linux: use pgrep to check for another process with our binary name
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
        Err(_) => true, // pgrep not available - assume we're alone
    }
}
