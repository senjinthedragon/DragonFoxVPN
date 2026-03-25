// vpn_runtime.rs - DragonFoxVPN: Shared runtime VPN operations
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Provides stateless free functions for enabling/disabling VPN routing
// and performing health checks. Called from background threads spawned
// by the main loop or the health_check_loop thread.

use crate::system::SystemHandler;

/// Enable VPN routing on the given adapter via the specified gateway and DNS.
/// Returns true on success, false if parameters are invalid or the OS command fails.
pub fn enable_vpn(adapter: &str, vpn_gateway: &str, dns_server: &str) -> bool {
    if vpn_gateway.is_empty() || dns_server.is_empty() {
        return false;
    }
    let ok = SystemHandler::enable_routing(adapter, vpn_gateway, dns_server);
    SystemHandler::flush_dns();
    ok
}

/// Disable VPN routing, restoring the default route.
pub fn disable_vpn(adapter: &str, vpn_gateway: &str) {
    if vpn_gateway.is_empty() {
        return;
    }
    SystemHandler::disable_routing(adapter, vpn_gateway);
    SystemHandler::flush_dns();
}

/// Health check result returned from `check_health`.
pub struct HealthCheck {
    pub vpn_active: bool,
    pub route_exists: bool,
    pub pi_reachable: bool,
}

/// Perform a single health check against the current VPN state.
pub fn check_health(adapter: &str, vpn_gateway: &str, isp_gateway: &str) -> HealthCheck {
    let route_exists = if vpn_gateway.is_empty() {
        false
    } else {
        SystemHandler::is_route_active(vpn_gateway, adapter)
    };
    let vpn_active = if route_exists && !isp_gateway.is_empty() {
        SystemHandler::check_connection(vpn_gateway, isp_gateway)
    } else {
        false
    };
    let pi_reachable = if vpn_gateway.is_empty() {
        false
    } else {
        SystemHandler::ping_host(vpn_gateway)
    };
    HealthCheck { vpn_active, route_exists, pi_reachable }
}
