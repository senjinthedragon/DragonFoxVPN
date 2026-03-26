// system.rs - DragonFoxVPN: OS abstraction layer
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Encapsulates all system-level command execution. Detects the operating
// system at runtime and dispatches the appropriate commands for routing
// table modification, DNS configuration, interface detection, and
// connectivity checks. Supports Linux (ip/resolvectl) and Windows
// (route.exe/netsh).

use log::{error, info, warn};
use std::process::Command;

/// Log the result of a routing/system command at the appropriate level.
fn log_cmd((stdout, stderr, code): (String, String, i32)) {
    if code == 0 {
        if !stdout.is_empty() {
            info!("  → ok: {stdout}");
        } else {
            info!("  → ok (exit 0)");
        }
    } else {
        warn!("  → exit {code}{}{}",
            if stdout.is_empty() { String::new() } else { format!(" stdout={stdout}") },
            if stderr.is_empty() { String::new() } else { format!(" stderr={stderr}") },
        );
    }
}

/// Set DNS via resolvectl. Downgrades the failure to info-level when
/// systemd-resolved is simply not running (e.g. Garuda/Arch), since
/// DNS will still flow correctly through the VPN tunnel in that case.
fn set_dns_resolvectl(adapter: &str, vpn_dns: &str) {
    let (_, stderr, code) =
        run_command(&format!("sudo resolvectl dns {adapter} {vpn_dns}"));
    if code == 0 {
        info!("  → DNS set via resolvectl ({vpn_dns})");
    } else if stderr.contains("org.freedesktop.resolve1")
        || stderr.contains("systemd-resolved")
        || stderr.contains("activation request failed")
    {
        info!(
            "  → resolvectl unavailable (systemd-resolved not running); \
             DNS will flow through VPN tunnel"
        );
    } else {
        warn!("  → resolvectl dns failed (exit {code}): {stderr}");
    }
}

/// Run a shell command, returning (stdout, stderr, exit_code).
/// Uses runtime OS detection to choose shell, matching the Python version.
pub fn run_command(cmd: &str) -> (String, String, i32) {
    let result = if std::env::consts::OS == "windows" {
        Command::new("cmd").args(["/C", cmd]).output()
    } else {
        Command::new("sh").args(["-c", cmd]).output()
    };

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let code = output.status.code().unwrap_or(-1);
            (stdout, stderr, code)
        }
        Err(e) => {
            error!("Command execution error: {e}");
            (String::new(), e.to_string(), -1)
        }
    }
}

pub struct SystemHandler;

impl SystemHandler {
    fn is_safe_adapter(adapter: &str) -> bool {
        regex_lite(r"^[a-zA-Z0-9._:-]+$").is_match(adapter)
    }

    fn is_safe_host(host: &str) -> bool {
        regex_lite(r"^[a-zA-Z0-9._:-]+$").is_match(host)
    }

    /// Detect the active network adapter name.
    pub fn get_active_adapter() -> String {
        let safe_re = regex_lite(r"^[a-zA-Z0-9._:-]+$");

        if std::env::consts::OS == "windows" {
            let (stdout, _, code) = run_command("netsh interface ipv4 show interfaces");
            if code == 0 {
                for line in stdout.lines() {
                    let lower = line.to_lowercase();
                    if lower.contains("connected") && !lower.contains("loopback") {
                        if let Some(candidate) = line.split_whitespace().last() {
                            if safe_re.is_match(candidate) {
                                return candidate.to_string();
                            }
                        }
                    }
                }
            }
            "Ethernet".to_string()
        } else {
            let (stdout, _, code) = run_command("ip route show default");
            if code == 0 && !stdout.is_empty() {
                // Look for "dev <name>"
                if let Some(pos) = stdout.find("dev ") {
                    let rest = &stdout[pos + 4..];
                    let candidate = rest.split_whitespace().next().unwrap_or("");
                    if safe_re.is_match(candidate) {
                        return candidate.to_string();
                    }
                }
            }
            "eno1".to_string()
        }
    }

    /// Flush the system DNS cache.
    pub fn flush_dns() {
        if std::env::consts::OS == "windows" {
            run_command("ipconfig /flushdns");
        } else {
            run_command("sudo systemd-resolve --flush-caches");
            run_command("sudo resolvectl flush-caches");
        }
    }

    /// Configure system routing to use the VPN gateway.
    /// Returns true on success.
    pub fn enable_routing(adapter: &str, vpn_gw: &str, vpn_dns: &str) -> bool {
        info!("enable_routing: adapter={adapter} vpn_gw={vpn_gw} vpn_dns={vpn_dns}");
        if !Self::is_safe_adapter(adapter) || !is_valid_ipv4(vpn_gw) || !is_valid_ipv4(vpn_dns) {
            error!("Refusing to run routing command with unsafe parameters.");
            return false;
        }

        if std::env::consts::OS == "windows" {
            run_command("route delete 0.0.0.0 mask 0.0.0.0");
            let (_, _, code) =
                run_command(&format!("route add 0.0.0.0 mask 0.0.0.0 {vpn_gw} metric 1"));
            run_command(&format!(
                "netsh interface ipv4 set dns name=\"{adapter}\" static {vpn_dns}"
            ));
            code == 0
        } else {
            log_cmd(run_command(&format!(
                "sudo sysctl -w net.ipv6.conf.{adapter}.disable_ipv6=1"
            )));
            set_dns_resolvectl(adapter, vpn_dns);
            log_cmd(run_command(&format!("sudo ip route del default dev {adapter}")));
            let result = run_command(&format!(
                "sudo ip route add default via {vpn_gw} dev {adapter} metric 50"
            ));
            let code = result.2;
            log_cmd(result);
            code == 0
        }
    }

    /// Restore default system routing, removing the VPN route.
    pub fn disable_routing(adapter: &str, vpn_gw: &str) {
        if !Self::is_safe_adapter(adapter) || !is_valid_ipv4(vpn_gw) {
            error!("Refusing to run routing command with unsafe parameters.");
            return;
        }

        if std::env::consts::OS == "windows" {
            run_command(&format!("route delete 0.0.0.0 mask 0.0.0.0 {vpn_gw}"));
            run_command(&format!(
                "netsh interface ipv4 set dns name=\"{adapter}\" source=dhcp"
            ));
        } else {
            run_command(&format!(
                "sudo ip route del default via {vpn_gw} dev {adapter}"
            ));
            run_command(&format!(
                "sudo sysctl -w net.ipv6.conf.{adapter}.disable_ipv6=0"
            ));
            let (_, stderr, code) =
                run_command(&format!("sudo resolvectl revert {adapter}"));
            if code != 0
                && !stderr.contains("org.freedesktop.resolve1")
                && !stderr.contains("systemd-resolved")
                && !stderr.contains("activation request failed")
            {
                warn!("  → resolvectl revert failed (exit {code}): {stderr}");
            }
        }
    }

    /// Kill-switch route deletion: forcibly remove the VPN default route.
    pub fn kill_switch_delete_route(vpn_gw: &str, adapter: &str) {
        if !Self::is_safe_adapter(adapter) || !is_valid_ipv4(vpn_gw) {
            error!("Refusing to run kill-switch command with unsafe parameters.");
            return;
        }

        if std::env::consts::OS == "windows" {
            run_command(&format!("route delete 0.0.0.0 mask 0.0.0.0 {vpn_gw}"));
        } else {
            run_command(&format!(
                "sudo ip route del default via {vpn_gw} dev {adapter}"
            ));
        }
    }

    /// Check if the first traceroute hop is NOT the ISP gateway
    /// (indicating traffic is flowing through the VPN).
    pub fn check_connection(vpn_gw: &str, isp_gw: &str) -> bool {
        if !is_valid_ipv4(isp_gw) {
            error!("Invalid ISP gateway configured; cannot verify VPN route.");
            return false;
        }

        let _ = vpn_gw; // vpn_gw not needed directly but kept for API symmetry
        let (stdout, _, code) = if std::env::consts::OS == "windows" {
            run_command("tracert -d -h 1 8.8.8.8")
        } else {
            run_command("traceroute -n -m 1 -w 1 8.8.8.8")
        };

        if code == 0 && !stdout.is_empty() {
            let ips = extract_ips(&stdout);
            if let Some(first_hop) = ips.first() {
                if first_hop == isp_gw {
                    return false;
                }
                return true;
            }
        }
        false
    }

    /// Ping a single host; returns true if it responds.
    pub fn ping_host(host: &str) -> bool {
        if !Self::is_safe_host(host) {
            error!("Refusing to ping unsafe host value.");
            return false;
        }

        let cmd = if std::env::consts::OS == "windows" {
            format!("ping -n 1 -w 1000 {host}")
        } else {
            format!("ping -c 1 -W 1 {host}")
        };
        let (_, _, code) = run_command(&cmd);
        code == 0
    }

    /// Check if the VPN default route is present in the routing table.
    pub fn is_route_active(vpn_gw: &str, adapter: &str) -> bool {
        if !Self::is_safe_adapter(adapter) || !is_valid_ipv4(vpn_gw) {
            error!("Refusing to query route with unsafe parameters.");
            return false;
        }

        if std::env::consts::OS == "windows" {
            let (stdout, _, code) = run_command("route print");
            code == 0 && stdout.contains(vpn_gw)
        } else {
            let (stdout, _, code) =
                run_command(&format!("ip route show default via {vpn_gw} dev {adapter}"));
            code == 0 && !stdout.trim().is_empty()
        }
    }
}

/// Extract all IPv4 addresses from a string.
pub fn extract_ips(text: &str) -> Vec<String> {
    let mut ips = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Simple octet scanner
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            let candidate = &text[start..i];
            if is_valid_ipv4(candidate) {
                ips.push(candidate.to_string());
            }
        } else {
            i += 1;
        }
    }
    ips
}

pub fn is_valid_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| {
        !p.is_empty()
            && p.len() <= 3
            && p.chars().all(|c| c.is_ascii_digit())
            && p.parse::<u8>().is_ok()
    })
}

/// Minimal regex-lite helper - matches whole string against a simple character-class pattern.
/// Only supports `^[chars]+$` patterns used in this module.
pub struct SimpleRegex {
    pattern: &'static str,
}

impl SimpleRegex {
    pub fn is_match(&self, s: &str) -> bool {
        if self.pattern == r"^[a-zA-Z0-9._:-]+$" {
            !s.is_empty()
                && s.chars().all(|c| {
                    c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == ':' || c == '-'
                })
        } else {
            true
        }
    }
}

pub fn regex_lite(pattern: &'static str) -> SimpleRegex {
    SimpleRegex { pattern }
}
