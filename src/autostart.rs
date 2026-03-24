// autostart.rs - DragonFoxVPN: Windows registry autostart management
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Manages start-on-boot via the Windows registry Run key. All registry
// operations are gated behind #[cfg(target_os = "windows")]; on other
// platforms the methods return safe no-op defaults.

#[cfg(target_os = "windows")]
const APP_NAME: &str = "DragonFoxVPN";

pub struct AutoStartManager;

impl AutoStartManager {
    /// Returns true only on Windows where registry autostart is supported.
    pub fn is_available() -> bool {
        std::env::consts::OS == "windows"
    }

    /// Check if autostart is currently enabled.
    #[cfg(target_os = "windows")]
    pub fn is_enabled() -> bool {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match hkcu
            .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_READ)
        {
            Ok(k) => k,
            Err(_) => return false,
        };

        match run_key.get_value::<String, _>(APP_NAME) {
            Ok(reg_path) => {
                let reg_path = reg_path.trim_matches('"').trim_matches('\'').to_string();
                let exe = std::env::current_exe().unwrap_or_default();
                // Normalize comparison
                let p1 = reg_path.to_lowercase();
                let p2 = exe.to_string_lossy().to_lowercase();
                p1 == p2
            }
            Err(_) => false,
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn is_enabled() -> bool {
        false
    }

    /// Enable or disable autostart in the Windows registry.
    #[cfg(target_os = "windows")]
    pub fn set_autostart(enable: bool) {
        use log::error;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ};
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = match hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        ) {
            Ok(k) => k,
            Err(e) => {
                error!("Failed to open autostart registry key: {e}");
                return;
            }
        };

        if enable {
            let exe = std::env::current_exe().unwrap_or_default();
            let mut exe_path = exe.to_string_lossy().to_string();
            if exe_path.contains(' ') && !exe_path.starts_with('"') {
                exe_path = format!("\"{exe_path}\"");
            }
            if let Err(e) = run_key.set_value(APP_NAME, &exe_path) {
                error!("Failed to set autostart value: {e}");
            }
        } else {
            // Ignore "not found" errors
            let _ = run_key.delete_value(APP_NAME);
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn set_autostart(_enable: bool) {
        // No-op on Linux/macOS
    }
}
