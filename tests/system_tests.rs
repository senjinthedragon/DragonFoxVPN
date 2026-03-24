// tests/system_tests.rs - DragonFoxVPN: SystemHandler utility tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::system::{extract_ips, is_valid_ipv4, regex_lite};

// ---------------------------------------------------------------------------
// extract_ips
// ---------------------------------------------------------------------------

#[test]
fn test_extract_ips_from_linux_traceroute() {
    // Typical `traceroute -n -m 1 -w 1 8.8.8.8` output
    let output = "traceroute to 8.8.8.8 (8.8.8.8), 1 hops max\n 1  10.0.0.20  1.234 ms";
    let ips = extract_ips(output);
    assert!(ips.contains(&"10.0.0.20".to_string()));
}

#[test]
fn test_extract_ips_from_windows_tracert() {
    // Typical `tracert -d -h 1 8.8.8.8` output
    let output =
        "Tracing route to 8.8.8.8 over a maximum of 1 hops:\r\n  1    <1 ms   10.0.0.1\r\n";
    let ips = extract_ips(output);
    assert!(ips.contains(&"10.0.0.1".to_string()));
}

#[test]
fn test_extract_ips_preserves_order() {
    let output = "first 192.168.1.1 second 10.0.0.1";
    let ips = extract_ips(output);
    assert_eq!(ips[0], "192.168.1.1");
    assert_eq!(ips[1], "10.0.0.1");
}

#[test]
fn test_extract_ips_empty_string() {
    assert!(extract_ips("").is_empty());
}

#[test]
fn test_extract_ips_no_ips_in_string() {
    assert!(extract_ips("traceroute: no route to host").is_empty());
}

#[test]
fn test_extract_ips_ignores_incomplete_octets() {
    // "256.0.0.1" is not a valid IP; should not be returned
    let ips = extract_ips("256.0.0.1 and 10.0.0.1");
    assert!(!ips.contains(&"256.0.0.1".to_string()));
    assert!(ips.contains(&"10.0.0.1".to_string()));
}

#[test]
fn test_extract_ips_multiple_on_same_line() {
    let output = "10.0.0.1 via 192.168.1.254";
    let ips = extract_ips(output);
    assert_eq!(ips.len(), 2);
}

#[test]
fn test_extract_ips_star_hop_returns_no_ip() {
    // Timeout hops print "* * *" with no IP
    let output = "  1  * * *";
    assert!(extract_ips(output).is_empty());
}

// ---------------------------------------------------------------------------
// is_valid_ipv4
// ---------------------------------------------------------------------------

#[test]
fn test_valid_ips() {
    assert!(is_valid_ipv4("192.168.1.1"));
    assert!(is_valid_ipv4("10.0.0.1"));
    assert!(is_valid_ipv4("0.0.0.0"));
    assert!(is_valid_ipv4("255.255.255.255"));
    assert!(is_valid_ipv4("8.8.8.8"));
}

#[test]
fn test_invalid_too_many_octets() {
    assert!(!is_valid_ipv4("1.2.3.4.5"));
}

#[test]
fn test_invalid_too_few_octets() {
    assert!(!is_valid_ipv4("192.168.1"));
    assert!(!is_valid_ipv4("10.0"));
}

#[test]
fn test_invalid_octet_out_of_range() {
    assert!(!is_valid_ipv4("256.0.0.1"));
    assert!(!is_valid_ipv4("192.168.1.256"));
}

#[test]
fn test_invalid_non_numeric() {
    assert!(!is_valid_ipv4("abc.def.ghi.jkl"));
    assert!(!is_valid_ipv4("192.168.1.x"));
}

#[test]
fn test_invalid_empty_string() {
    assert!(!is_valid_ipv4(""));
}

#[test]
fn test_invalid_empty_octet() {
    assert!(!is_valid_ipv4("192..1.1"));
}

#[test]
fn test_invalid_leading_dot() {
    assert!(!is_valid_ipv4(".192.168.1.1"));
}

// ---------------------------------------------------------------------------
// SimpleRegex (adapter name validator)
// ---------------------------------------------------------------------------

#[test]
fn test_adapter_regex_valid_names() {
    let re = regex_lite(r"^[a-zA-Z0-9._:-]+$");
    assert!(re.is_match("eth0"));
    assert!(re.is_match("eno1"));
    assert!(re.is_match("wlan0"));
    assert!(re.is_match("Ethernet"));
    assert!(re.is_match("Wi-Fi"));
    assert!(re.is_match("lo"));
    assert!(re.is_match("enp3s0"));
    assert!(re.is_match("tun0"));
}

#[test]
fn test_adapter_regex_invalid_names() {
    let re = regex_lite(r"^[a-zA-Z0-9._:-]+$");
    // Spaces are not allowed
    assert!(!re.is_match("Wi Fi"));
    assert!(!re.is_match("Local Area Connection"));
    // Slashes are not allowed
    assert!(!re.is_match("dev/eth0"));
    // Empty string is not allowed
    assert!(!re.is_match(""));
}

#[test]
fn test_adapter_regex_allows_all_valid_chars() {
    let re = regex_lite(r"^[a-zA-Z0-9._:-]+$");
    assert!(re.is_match("a-b.c_d:e0"));
}
