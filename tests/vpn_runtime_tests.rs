use dragonfox_vpn::vpn_runtime;

#[test]
fn test_enable_vpn_rejects_empty_gateway_or_dns() {
    assert!(!vpn_runtime::enable_vpn("eth0", "", "1.1.1.1"));
    assert!(!vpn_runtime::enable_vpn("eth0", "10.0.0.1", ""));
    assert!(!vpn_runtime::enable_vpn("eth0", "", ""));
}

#[test]
fn test_disable_vpn_noop_on_empty_gateway() {
    // Should not panic when no gateway is configured.
    vpn_runtime::disable_vpn("eth0", "");
}

#[test]
fn test_check_health_empty_inputs_are_all_false() {
    let health = vpn_runtime::check_health("eth0", "", "");
    assert!(!health.route_exists);
    assert!(!health.vpn_active);
    assert!(!health.pi_reachable);
}
