// tests/daemon_ipc_tests.rs - DragonFoxVPN: DaemonCommand and DaemonStatus IPC tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::daemon_ipc::{DaemonCommand, DaemonStatus};

// Tests that write to the command/status files must not run concurrently
// because they share the same file path on disk.
static FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_daemon_status_serialization_roundtrip() {
    let status = DaemonStatus {
        state: "Connected".to_string(),
        adapter: "eth0".to_string(),
        location: "Germany - Frankfurt".to_string(),
        vpn_gateway: Some("10.0.0.1".to_string()),
        connected_since_unix: Some(123456),
        message: Some("ok".to_string()),
        updated_unix: 999,
    };

    let json = serde_json::to_string(&status).expect("serialize status");
    let parsed: DaemonStatus = serde_json::from_str(&json).expect("deserialize status");

    assert_eq!(parsed.state, "Connected");
    assert_eq!(parsed.adapter, "eth0");
    assert_eq!(parsed.location, "Germany - Frankfurt");
    assert_eq!(parsed.vpn_gateway.as_deref(), Some("10.0.0.1"));
    assert_eq!(parsed.connected_since_unix, Some(123456));
    assert_eq!(parsed.message.as_deref(), Some("ok"));
    assert_eq!(parsed.updated_unix, 999);
}

#[test]
fn test_status_file_roundtrip() {
    let _guard = FILE_LOCK.lock().unwrap();
    let status = DaemonStatus {
        state: "Disabled".to_string(),
        adapter: "eth0".to_string(),
        location: "Unknown".to_string(),
        vpn_gateway: None,
        connected_since_unix: None,
        message: Some("status file test".to_string()),
        updated_unix: 1,
    };
    dragonfox_vpn::daemon_ipc::save_daemon_status(&status);
    let loaded = dragonfox_vpn::daemon_ipc::load_daemon_status().expect("status should load");
    assert_eq!(loaded.state, "Disabled");
    assert_eq!(loaded.adapter, "eth0");
    assert_eq!(loaded.vpn_gateway, None);
    assert_eq!(loaded.message.as_deref(), Some("status file test"));
    assert!(loaded.updated_unix > 0);
}

#[test]
fn test_daemon_command_serialization_roundtrip() {
    for cmd in [
        DaemonCommand::Reconnect,
        DaemonCommand::ReloadConfig,
        DaemonCommand::Restart,
        DaemonCommand::Quit,
    ] {
        let json = serde_json::to_string(&cmd).expect("serialize command");
        let parsed: DaemonCommand = serde_json::from_str(&json).expect("deserialize command");
        assert_eq!(cmd, parsed, "roundtrip failed for {:?}", cmd);
    }
}

#[test]
fn test_restart_command_file_roundtrip() {
    let _guard = FILE_LOCK.lock().unwrap();
    dragonfox_vpn::daemon_ipc::clear_daemon_command();
    dragonfox_vpn::daemon_ipc::write_daemon_command(DaemonCommand::Restart);
    let cmd = dragonfox_vpn::daemon_ipc::take_daemon_command();
    assert!(matches!(cmd, Some(DaemonCommand::Restart)));
    dragonfox_vpn::daemon_ipc::clear_daemon_command();
}

#[test]
fn test_current_unix_ts_is_nonzero() {
    let ts = dragonfox_vpn::daemon_ipc::current_unix_ts();
    assert!(ts > 0);
}

#[test]
fn test_command_file_roundtrip_and_clear() {
    let _guard = FILE_LOCK.lock().unwrap();
    dragonfox_vpn::daemon_ipc::clear_daemon_command();
    dragonfox_vpn::daemon_ipc::write_daemon_command(DaemonCommand::Reconnect);
    dragonfox_vpn::daemon_ipc::write_daemon_command(DaemonCommand::ReloadConfig);

    let cmd1 = dragonfox_vpn::daemon_ipc::take_daemon_command();
    assert!(matches!(cmd1, Some(DaemonCommand::Reconnect)));

    let cmd2 = dragonfox_vpn::daemon_ipc::take_daemon_command();
    assert!(matches!(cmd2, Some(DaemonCommand::ReloadConfig)));

    // Queue should now be empty.
    let none_again = dragonfox_vpn::daemon_ipc::take_daemon_command();
    assert!(none_again.is_none());

    dragonfox_vpn::daemon_ipc::clear_daemon_command();
}

#[test]
fn test_command_take_supports_legacy_single_command_json() {
    let _guard = FILE_LOCK.lock().unwrap();
    // Older versions wrote a single JSON object instead of an array.
    // take_daemon_command must still handle that format.
    let path = dragonfox_vpn::config::get_config_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("daemon_command.json");

    dragonfox_vpn::daemon_ipc::clear_daemon_command();
    let legacy = serde_json::to_string(&DaemonCommand::Reconnect).expect("serialize single cmd");
    std::fs::write(&path, legacy).expect("write legacy payload");

    let cmd = dragonfox_vpn::daemon_ipc::take_daemon_command();
    assert!(matches!(cmd, Some(DaemonCommand::Reconnect)));

    dragonfox_vpn::daemon_ipc::clear_daemon_command();
}

#[test]
fn test_command_queue_is_bounded() {
    let _guard = FILE_LOCK.lock().unwrap();
    dragonfox_vpn::daemon_ipc::clear_daemon_command();

    for _ in 0..40 {
        dragonfox_vpn::daemon_ipc::write_daemon_command(DaemonCommand::Reconnect);
    }

    let mut count = 0;
    while dragonfox_vpn::daemon_ipc::take_daemon_command().is_some() {
        count += 1;
    }

    assert_eq!(count, 32, "command queue should be capped at 32");
    dragonfox_vpn::daemon_ipc::clear_daemon_command();
}
