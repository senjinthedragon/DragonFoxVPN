// state.rs - DragonFoxVPN: Global application state
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Defines the VPN connection state machine and the shared AppState struct
// that is passed between the UI thread and background monitor via Arc<Mutex<>>.

use std::time::Instant;

/// VPN connection state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VpnState {
    Disabled,
    Enabling,
    Connected,
    Dropped,
    ServerUnreachable,
}

impl VpnState {
    pub fn as_str(&self) -> &'static str {
        match self {
            VpnState::Disabled => "Disabled",
            VpnState::Enabling => "Enabling...",
            VpnState::Connected => "Connected",
            VpnState::Dropped => "Dropped",
            VpnState::ServerUnreachable => "Server Unreachable",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            VpnState::Connected => egui::Color32::from_rgb(0x4C, 0xAF, 0x50),
            VpnState::Dropped => egui::Color32::from_rgb(0xF4, 0x43, 0x36),
            VpnState::ServerUnreachable => egui::Color32::from_rgb(0x9E, 0x9E, 0x9E),
            VpnState::Enabling => egui::Color32::from_rgb(0x21, 0x96, 0xF3),
            VpnState::Disabled => egui::Color32::from_rgb(0xFF, 0xC1, 0x07),
        }
    }
}

/// Global runtime state shared between UI and background threads.
pub struct AppState {
    pub vpn_state: VpnState,
    pub vpn_location: String,
    pub connection_start_time: Option<Instant>,
    pub adapter_name: String,
    pub manual_disable: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            vpn_state: VpnState::Disabled,
            vpn_location: "Unknown".to_string(),
            connection_start_time: None,
            adapter_name: "auto".to_string(),
            manual_disable: true,
        }
    }
}

/// Result from the background network monitor thread.
#[derive(Debug, Clone)]
pub struct NetworkCheckResult {
    pub vpn_active: bool,
    pub route_exists: bool,
    pub pi_reachable: bool,
}
