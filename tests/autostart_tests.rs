// tests/autostart_tests.rs - DragonFoxVPN: AutoStartManager tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::autostart::AutoStartManager;

#[cfg(not(target_os = "windows"))]
#[test]
fn test_not_available_on_non_windows() {
    assert!(
        !AutoStartManager::is_available(),
        "autostart should not be available on non-Windows"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_not_enabled_on_non_windows() {
    assert!(
        !AutoStartManager::is_enabled(),
        "autostart should report disabled on non-Windows"
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_set_autostart_enable_does_not_panic() {
    // On non-Windows this is a no-op; must not panic.
    AutoStartManager::set_autostart(true);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_set_autostart_disable_does_not_panic() {
    AutoStartManager::set_autostart(false);
}

#[cfg(target_os = "windows")]
#[test]
fn test_available_on_windows() {
    assert!(AutoStartManager::is_available());
}
