// tests/state_tests.rs - DragonFoxVPN: VpnState and AppState unit tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::state::{AppState, VpnState};

// ---------------------------------------------------------------------------
// VpnState::as_str
// ---------------------------------------------------------------------------

#[test]
fn test_as_str_disabled() {
    assert_eq!(VpnState::Disabled.as_str(), "Disabled");
}

#[test]
fn test_as_str_enabling() {
    assert_eq!(VpnState::Enabling.as_str(), "Enabling...");
}

#[test]
fn test_as_str_connected() {
    assert_eq!(VpnState::Connected.as_str(), "Connected");
}

#[test]
fn test_as_str_dropped() {
    assert_eq!(VpnState::Dropped.as_str(), "Dropped");
}

#[test]
fn test_as_str_server_unreachable() {
    assert_eq!(VpnState::ServerUnreachable.as_str(), "Server Unreachable");
}

// ---------------------------------------------------------------------------
// VpnState::color - just verify each variant returns a distinct, non-black colour
// ---------------------------------------------------------------------------

#[test]
fn test_colors_are_non_black() {
    for state in [
        VpnState::Disabled,
        VpnState::Enabling,
        VpnState::Connected,
        VpnState::Dropped,
        VpnState::ServerUnreachable,
    ] {
        let c = state.color();
        assert!(
            c.r() > 0 || c.g() > 0 || c.b() > 0,
            "{:?} returned a black colour",
            state
        );
    }
}

#[test]
fn test_connected_color_is_green() {
    let c = VpnState::Connected.color();
    assert!(c.g() > c.r(), "Connected should be green-dominant");
    assert!(c.g() > c.b(), "Connected should be green-dominant");
}

#[test]
fn test_dropped_color_is_red() {
    let c = VpnState::Dropped.color();
    assert!(c.r() > c.g(), "Dropped should be red-dominant");
    assert!(c.r() > c.b(), "Dropped should be red-dominant");
}

#[test]
fn test_all_state_colors_are_distinct() {
    let colors: Vec<_> = [
        VpnState::Disabled,
        VpnState::Enabling,
        VpnState::Connected,
        VpnState::Dropped,
        VpnState::ServerUnreachable,
    ]
    .iter()
    .map(|s| s.color())
    .collect();

    for i in 0..colors.len() {
        for j in (i + 1)..colors.len() {
            assert_ne!(
                colors[i], colors[j],
                "States {} and {} share a colour",
                i, j
            );
        }
    }
}

// ---------------------------------------------------------------------------
// VpnState equality
// ---------------------------------------------------------------------------

#[test]
fn test_equality_same_variant() {
    assert_eq!(VpnState::Connected, VpnState::Connected);
    assert_eq!(VpnState::Disabled, VpnState::Disabled);
}

#[test]
fn test_inequality_different_variants() {
    assert_ne!(VpnState::Connected, VpnState::Disabled);
    assert_ne!(VpnState::Dropped, VpnState::ServerUnreachable);
}

#[test]
fn test_clone() {
    let original = VpnState::Connected;
    let cloned = original.clone();
    assert_eq!(original, cloned);
}

// ---------------------------------------------------------------------------
// AppState defaults
// ---------------------------------------------------------------------------

#[test]
fn test_appstate_default_is_disabled() {
    assert_eq!(AppState::default().vpn_state, VpnState::Disabled);
}

#[test]
fn test_appstate_default_location_is_unknown() {
    assert_eq!(AppState::default().vpn_location, "Unknown");
}

#[test]
fn test_appstate_default_no_connection_start_time() {
    assert!(AppState::default().connection_start_time.is_none());
}

#[test]
fn test_appstate_default_manual_disable_is_true() {
    // On startup the user has not explicitly connected, so manual_disable = true
    // prevents the kill switch from firing before any connection is made.
    assert!(AppState::default().manual_disable);
}

#[test]
fn test_appstate_default_adapter_is_auto() {
    assert_eq!(AppState::default().adapter_name, "auto");
}
