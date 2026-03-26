// config.rs - DragonFoxVPN: Persistent configuration management
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Handles loading and saving of user preferences (favorites, last location,
// auto-connect settings, network addresses) to a JSON file in the
// platform-appropriate config directory. JSON field names are kept
// compatible with the original Python version.

use directories::BaseDirs;
use log::error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Returns the platform-specific config file path.
/// - Linux:   ~/.config/dragonfox_vpn.json
/// - Windows: %APPDATA%\DragonFoxVPN\dragonfox_vpn.json
pub fn get_config_path() -> PathBuf {
    if std::env::consts::OS == "windows" {
        if let Some(base) = BaseDirs::new() {
            let dir = base.config_dir().join("DragonFoxVPN");
            let _ = std::fs::create_dir_all(&dir);
            return dir.join("dragonfox_vpn.json");
        }
    }

    // Linux / macOS fallback
    if let Some(base) = BaseDirs::new() {
        let dir = base.config_dir().to_path_buf();
        let _ = std::fs::create_dir_all(&dir);
        return dir.join("dragonfox_vpn.json");
    }

    // Last-resort fallback
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("dragonfox_vpn.json")
}

/// Returns the flag image cache directory (sibling of config file).
pub fn get_flags_dir() -> PathBuf {
    get_config_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("flags")
}

/// Persistent user configuration - JSON-compatible with the Python version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default)]
    pub auto_reconnect: bool,
    #[serde(default)]
    pub last_location: Option<String>,
    #[serde(default)]
    pub vpn_gateway: Option<String>,
    #[serde(default)]
    pub isp_gateway: Option<String>,
    #[serde(default)]
    pub dns_server: Option<String>,
    #[serde(default)]
    pub switcher_url: Option<String>,
    #[serde(default)]
    pub setup_complete: bool,
    /// Language override. None = auto-detect from system locale.
    #[serde(default)]
    pub language: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            favorites: Vec::new(),
            auto_connect: false,
            auto_reconnect: false,
            last_location: None,
            vpn_gateway: None,
            isp_gateway: None,
            dns_server: None,
            switcher_url: None,
            setup_complete: false,
            language: None,
        }
    }
}

impl AppConfig {
    /// Load config from disk, returning defaults on any failure.
    pub fn load() -> Self {
        let path = get_config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(contents) => match serde_json::from_str::<AppConfig>(&contents) {
                    Ok(cfg) => return cfg,
                    Err(e) => error!("Failed to parse config: {e}"),
                },
                Err(e) => error!("Failed to read config file: {e}"),
            }
        }
        AppConfig::default()
    }

    /// Save config to disk atomically (write to temp file then rename).
    pub fn save(&self) {
        let path = get_config_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                let tmp = path.with_extension("json.tmp");
                if let Err(e) = std::fs::write(&tmp, &json) {
                    error!("Failed to write config temp file: {e}");
                    return;
                }
                if let Err(e) = std::fs::rename(&tmp, &path) {
                    error!("Failed to rename config temp file: {e}");
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            Err(e) => error!("Failed to serialize config: {e}"),
        }
    }

    #[allow(dead_code)]
    pub fn is_favorite(&self, label: &str) -> bool {
        self.favorites.contains(&label.to_string())
    }

    pub fn toggle_favorite(&mut self, label: &str) {
        let s = label.to_string();
        if let Some(pos) = self.favorites.iter().position(|f| f == &s) {
            self.favorites.remove(pos);
        } else {
            self.favorites.push(s);
        }
        self.save();
    }
}
