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
    // Use a named mutex — the correct Windows single-instance pattern.
    // Only the daemon creates/holds this mutex; --ui subprocesses never call
    // single_instance_check, so they don't interfere. tasklist-based counting
    // incorrectly includes subprocesses and breaks on restart.
    use std::os::windows::ffi::OsStrExt;
    extern "system" {
        fn CreateMutexW(
            lp_mutex_attributes: *mut std::ffi::c_void,
            b_initial_owner: i32,
            lp_name: *const u16,
        ) -> *mut std::ffi::c_void;
        fn GetLastError() -> u32;
    }
    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name: Vec<u16> = std::ffi::OsStr::new("Local\\DragonFoxVPN_Daemon")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = CreateMutexW(std::ptr::null_mut(), 0, name.as_ptr());
        if handle.is_null() {
            return true; // couldn't create mutex, allow startup
        }
        // Intentionally leak the handle — it keeps the mutex alive until the
        // process exits, at which point the OS releases it automatically.
        GetLastError() != ERROR_ALREADY_EXISTS
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
