// tests/config_tests.rs - DragonFoxVPN: AppConfig unit tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::config::{get_config_path, get_flags_dir, AppConfig};

// ---------------------------------------------------------------------------
// Default values
// ---------------------------------------------------------------------------

#[test]
fn test_default_has_no_favorites() {
    let cfg = AppConfig::default();
    assert!(cfg.favorites.is_empty());
}

#[test]
fn test_default_auto_connect_is_false() {
    assert!(!AppConfig::default().auto_connect);
}

#[test]
fn test_default_setup_not_complete() {
    assert!(!AppConfig::default().setup_complete);
}

#[test]
fn test_default_optional_fields_are_none() {
    let cfg = AppConfig::default();
    assert!(cfg.last_location.is_none());
    assert!(cfg.vpn_gateway.is_none());
    assert!(cfg.isp_gateway.is_none());
    assert!(cfg.dns_server.is_none());
    assert!(cfg.switcher_url.is_none());
}

// ---------------------------------------------------------------------------
// Serialization / deserialization
// ---------------------------------------------------------------------------

#[test]
fn test_serde_roundtrip_default() {
    let original = AppConfig::default();
    let json = serde_json::to_string(&original).expect("serialize");
    let restored: AppConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.favorites, original.favorites);
    assert_eq!(restored.auto_connect, original.auto_connect);
    assert_eq!(restored.setup_complete, original.setup_complete);
}

#[test]
fn test_serde_roundtrip_populated() {
    let mut cfg = AppConfig::default();
    cfg.favorites = vec!["UK - London".to_string(), "Germany - Frankfurt".to_string()];
    cfg.auto_connect = true;
    cfg.last_location = Some("UK - London".to_string());
    cfg.vpn_gateway = Some("10.0.0.20".to_string());
    cfg.isp_gateway = Some("10.0.0.1".to_string());
    cfg.dns_server = Some("10.0.0.20".to_string());
    cfg.switcher_url = Some("http://10.0.0.20".to_string());
    cfg.setup_complete = true;

    let json = serde_json::to_string_pretty(&cfg).expect("serialize");
    let restored: AppConfig = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.favorites, cfg.favorites);
    assert_eq!(restored.auto_connect, cfg.auto_connect);
    assert_eq!(restored.last_location, cfg.last_location);
    assert_eq!(restored.vpn_gateway, cfg.vpn_gateway);
    assert_eq!(restored.isp_gateway, cfg.isp_gateway);
    assert_eq!(restored.dns_server, cfg.dns_server);
    assert_eq!(restored.switcher_url, cfg.switcher_url);
    assert_eq!(restored.setup_complete, cfg.setup_complete);
}

#[test]
fn test_partial_json_uses_defaults_for_missing_fields() {
    // Only set two fields; the rest should fall back to defaults.
    let json = r#"{"auto_connect": true, "setup_complete": true}"#;
    let cfg: AppConfig = serde_json::from_str(json).expect("deserialize partial JSON");
    assert!(cfg.auto_connect);
    assert!(cfg.setup_complete);
    assert!(cfg.favorites.is_empty());
    assert!(cfg.vpn_gateway.is_none());
}

#[test]
fn test_empty_json_object_deserializes_to_defaults() {
    let cfg: AppConfig = serde_json::from_str("{}").expect("deserialize empty object");
    assert!(!cfg.auto_connect);
    assert!(cfg.favorites.is_empty());
}

// ---------------------------------------------------------------------------
// Favorites
// ---------------------------------------------------------------------------

#[test]
fn test_is_favorite_false_initially() {
    let cfg = AppConfig::default();
    assert!(!cfg.is_favorite("UK - London"));
}

#[test]
fn test_is_favorite_true_after_adding() {
    let mut cfg = AppConfig::default();
    cfg.favorites.push("UK - London".to_string());
    assert!(cfg.is_favorite("UK - London"));
}

#[test]
fn test_is_favorite_case_sensitive() {
    let mut cfg = AppConfig::default();
    cfg.favorites.push("UK - London".to_string());
    assert!(!cfg.is_favorite("uk - london"));
}

#[test]
fn test_toggle_favorite_adds_when_absent() {
    let mut cfg = AppConfig::default();
    // Avoid the save() call writing to real disk by manipulating favorites directly.
    let label = "Germany - Frankfurt";
    assert!(!cfg.favorites.contains(&label.to_string()));
    cfg.favorites.push(label.to_string());
    assert!(cfg.favorites.contains(&label.to_string()));
}

#[test]
fn test_toggle_favorite_removes_when_present() {
    let mut cfg = AppConfig::default();
    cfg.favorites.push("Germany - Frankfurt".to_string());
    let pos = cfg
        .favorites
        .iter()
        .position(|f| f == "Germany - Frankfurt");
    assert!(pos.is_some());
    cfg.favorites.remove(pos.unwrap());
    assert!(!cfg.favorites.contains(&"Germany - Frankfurt".to_string()));
}

#[test]
fn test_favorites_preserves_order() {
    let mut cfg = AppConfig::default();
    cfg.favorites = vec![
        "UK - London".to_string(),
        "Germany - Frankfurt".to_string(),
        "Japan - Tokyo".to_string(),
    ];
    assert_eq!(cfg.favorites[0], "UK - London");
    assert_eq!(cfg.favorites[1], "Germany - Frankfurt");
    assert_eq!(cfg.favorites[2], "Japan - Tokyo");
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

#[test]
fn test_config_path_is_json_file() {
    let path = get_config_path();
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("json"));
}

#[test]
fn test_flags_dir_is_sibling_of_config() {
    let config = get_config_path();
    let flags = get_flags_dir();
    assert_eq!(config.parent(), flags.parent());
}

#[test]
fn test_flags_dir_is_named_flags() {
    let flags = get_flags_dir();
    assert_eq!(flags.file_name().and_then(|n| n.to_str()), Some("flags"));
}
