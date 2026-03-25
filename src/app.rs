// app.rs - DragonFoxVPN: Dialog windows for UI subprocesses
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Each dialog is launched as an independent subprocess (see main.rs). The
// subprocess has its own eframe event loop and no GTK connection, so the OS
// close button works reliably on every compositor. State is read from
// daemon_status.json; commands are written to daemon_command.json.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::time::Duration;

use log::{error, info, warn};

use crate::api::{country_to_iso, Location, VpnApi};
use crate::config::AppConfig;
use crate::daemon_ipc::{
    current_unix_ts, load_daemon_status, write_daemon_command, DaemonCommand,
};
use crate::system::SystemHandler;

// --------------------------------------------------------------------------
// UI lock: prevents opening the same dialog twice simultaneously
// --------------------------------------------------------------------------

struct UiLock {
    path: std::path::PathBuf,
}

impl Drop for UiLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_ui_lock(mode: &str) -> Option<UiLock> {
    let path = crate::config::get_config_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(format!("ui_{mode}.lock"));

    if std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .is_ok()
    {
        return Some(UiLock { path });
    }

    // Clear stale lock files left by crashed processes (older than 6 hours).
    let stale = Duration::from_secs(6 * 60 * 60);
    if let Ok(meta) = std::fs::metadata(&path) {
        if let Ok(modified) = meta.modified() {
            if modified.elapsed().unwrap_or_default() > stale {
                let _ = std::fs::remove_file(&path);
                if std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .is_ok()
                {
                    return Some(UiLock { path });
                }
            }
        }
    }

    None
}

// --------------------------------------------------------------------------
// Public entry points called from main.rs --ui dispatch
// --------------------------------------------------------------------------

/// Settings / initial-setup dialog.
pub fn run_settings_window() {
    let _lock = match acquire_ui_lock("settings") {
        Some(l) => l,
        None => return, // already open
    };

    let cfg = AppConfig::load();
    let first_run = !cfg.setup_complete;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(if first_run {
                "DragonFoxVPN - Initial Setup"
            } else {
                "DragonFoxVPN - Settings"
            })
            .with_inner_size([500.0, 360.0])
            .with_resizable(false),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "DragonFoxVPN Settings",
        options,
        Box::new(move |_cc| Ok(Box::new(SettingsWindow::new(first_run)))),
    );
}

/// Status dashboard dialog.
pub fn run_status_window() {
    let _lock = match acquire_ui_lock("status") {
        Some(l) => l,
        None => return,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DragonFoxVPN - Status")
            .with_inner_size([420.0, 300.0])
            .with_resizable(false),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "DragonFoxVPN Status",
        options,
        Box::new(|_cc| Ok(Box::new(StatusWindow::new()))),
    );
}

/// Location picker dialog.
pub fn run_location_window() {
    let _lock = match acquire_ui_lock("location") {
        Some(l) => l,
        None => return,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("DragonFoxVPN - Change Location")
            .with_inner_size([620.0, 700.0])
            .with_resizable(true),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "DragonFoxVPN Location",
        options,
        Box::new(|_cc| Ok(Box::new(LocationWindow::new()))),
    );
}

// --------------------------------------------------------------------------
// Settings window
// --------------------------------------------------------------------------

struct SettingsWindow {
    first_run: bool,
    vpn_gateway: String,
    isp_gateway: String,
    dns_server: String,
    switcher_url: String,
    message: Option<String>,
    saved: bool,
}

impl SettingsWindow {
    fn new(first_run: bool) -> Self {
        let cfg = AppConfig::load();
        Self {
            first_run,
            vpn_gateway: cfg.vpn_gateway.clone().unwrap_or_default(),
            isp_gateway: cfg.isp_gateway.clone().unwrap_or_default(),
            dns_server: cfg.dns_server.clone().unwrap_or_default(),
            switcher_url: cfg.switcher_url.clone().unwrap_or_default(),
            message: None,
            saved: false,
        }
    }
}

impl eframe::App for SettingsWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        // Block close on first run until setup is saved.
        if ctx.input(|i| i.viewport().close_requested()) && self.first_run && !self.saved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.message = Some("Setup is required to use DragonFoxVPN.".to_string());
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(if self.first_run {
                    "Initial Setup"
                } else {
                    "Network Settings"
                });
                if self.first_run {
                    ui.colored_label(
                        egui::Color32::GRAY,
                        "Enter your network details to get started.",
                    );
                }
            });
            ui.add_space(8.0);

            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.colored_label(egui::Color32::GRAY, "VPN Gateway IP");
                    ui.text_edit_singleline(&mut self.vpn_gateway);
                    ui.end_row();
                    ui.colored_label(egui::Color32::GRAY, "ISP Gateway IP");
                    ui.text_edit_singleline(&mut self.isp_gateway);
                    ui.end_row();
                    ui.colored_label(egui::Color32::GRAY, "DNS Server");
                    ui.text_edit_singleline(&mut self.dns_server);
                    ui.end_row();
                    ui.colored_label(egui::Color32::GRAY, "VPN Switcher URL");
                    ui.text_edit_singleline(&mut self.switcher_url);
                    ui.end_row();
                });

            if let Some(ref msg) = self.message.clone() {
                let color = if msg.starts_with("Saved") {
                    egui::Color32::LIGHT_GREEN
                } else {
                    egui::Color32::LIGHT_RED
                };
                ui.colored_label(color, msg);
            }

            ui.add_space(8.0);
            if ui.button("Save Settings").clicked() {
                self.try_save(ctx);
            }
        });
    }
}

impl SettingsWindow {
    fn try_save(&mut self, ctx: &egui::Context) {
        let vpn_gw = self.vpn_gateway.trim().to_string();
        let isp_gw = self.isp_gateway.trim().to_string();
        let dns = self.dns_server.trim().to_string();
        let url = self.switcher_url.trim().to_string();

        if vpn_gw.is_empty() || isp_gw.is_empty() || dns.is_empty() || url.is_empty() {
            self.message = Some("All fields are required.".to_string());
            return;
        }
        if !is_valid_ip(&vpn_gw) || !is_valid_ip(&isp_gw) || !is_valid_ip(&dns) {
            self.message =
                Some("Gateway and DNS fields must be valid IPv4 addresses.".to_string());
            return;
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.message =
                Some("Switcher URL must start with http:// or https://".to_string());
            return;
        }

        let mut cfg = AppConfig::load();
        cfg.vpn_gateway = Some(vpn_gw);
        cfg.isp_gateway = Some(isp_gw);
        cfg.dns_server = Some(dns);
        cfg.switcher_url = Some(url);
        cfg.setup_complete = true;
        cfg.save();

        // Tell the tray daemon to reload its config.
        write_daemon_command(DaemonCommand::ReloadConfig);

        let adapter = SystemHandler::get_active_adapter();
        info!("Settings saved. Active adapter: {adapter}");
        self.message = Some("Saved settings.".to_string());
        self.saved = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

// --------------------------------------------------------------------------
// Status window
// --------------------------------------------------------------------------

struct StatusWindow;

impl StatusWindow {
    fn new() -> Self {
        Self
    }
}

impl eframe::App for StatusWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0x00, 0x7A, 0xCC),
                    egui::RichText::new("DragonFox VPN").size(24.0).strong(),
                );
            });
            ui.add_space(12.0);

            if let Some(status) = load_daemon_status() {
                let (state_color, state_text) = match status.state.as_str() {
                    "Connected" => (egui::Color32::LIGHT_GREEN, "CONNECTED"),
                    "Enabling" => (egui::Color32::from_rgb(0x00, 0x7A, 0xCC), "CONNECTING…"),
                    "Dropped" => (egui::Color32::LIGHT_RED, "DROPPED"),
                    "ServerUnreachable" => (egui::Color32::GRAY, "SERVER UNREACHABLE"),
                    "SetupIncomplete" => (egui::Color32::GRAY, "SETUP INCOMPLETE"),
                    _ => (egui::Color32::YELLOW, "DISABLED"),
                };

                egui::Frame::none()
                    .fill(egui::Color32::from_rgb(0x25, 0x25, 0x26))
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.colored_label(
                                state_color,
                                egui::RichText::new(state_text).size(18.0).strong(),
                            );
                        });
                    });

                ui.add_space(8.0);
                ui.label(format!("Location: {}", status.location));
                ui.label(format!(
                    "Gateway: {}",
                    status.vpn_gateway.as_deref().unwrap_or("N/A")
                ));
                ui.label(format!("Adapter: {}", status.adapter));

                let duration_str = if let Some(since) = status.connected_since_unix {
                    let elapsed = current_unix_ts().saturating_sub(since);
                    format!(
                        "{:02}:{:02}:{:02}",
                        elapsed / 3600,
                        (elapsed % 3600) / 60,
                        elapsed % 60
                    )
                } else {
                    "--:--:--".to_string()
                };
                ui.label(format!("Duration: {duration_str}"));

                if let Some(ref msg) = status.message {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::GRAY, msg);
                }
            } else {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Waiting for daemon status…\nIs the tray daemon running?",
                );
            }

            ui.add_space(8.0);
        });

        // Refresh once per second so the duration counter updates.
        ctx.request_repaint_after(Duration::from_secs(1));
    }
}

// --------------------------------------------------------------------------
// Location window
// --------------------------------------------------------------------------

enum LocationMsg {
    LocationsFetched(Vec<Location>, Option<String>),
    FlagReady(String),
    SwitchDone(Result<String, String>),
}

struct LocationWindow {
    cfg: AppConfig,
    locations: Vec<Location>,
    selected_value: Option<String>,
    selected_label: Option<String>,
    search_text: String,
    is_loading: bool,
    is_switching: bool,
    switch_status: Option<String>,
    switch_ok: bool,
    // In-process flag cache (bytes from disk cache or download)
    flag_bytes: HashMap<String, Vec<u8>>,
    flag_textures: HashMap<String, egui::TextureHandle>,
    fetching_flags: HashSet<String>,
    msg_tx: mpsc::SyncSender<LocationMsg>,
    msg_rx: mpsc::Receiver<LocationMsg>,
}

impl LocationWindow {
    fn new() -> Self {
        let (msg_tx, msg_rx) = mpsc::sync_channel(32);
        let cfg = AppConfig::load();

        // Kick off location fetch immediately.
        if let Some(url) = cfg.switcher_url.clone() {
            let tx = msg_tx.clone();
            std::thread::spawn(move || match VpnApi::fetch_locations(&url) {
                Ok((locs, cur)) => {
                    let _ = tx.send(LocationMsg::LocationsFetched(locs, cur));
                }
                Err(e) => {
                    warn!("Location fetch failed: {e}");
                    let _ = tx.send(LocationMsg::LocationsFetched(vec![], None));
                }
            });
        } else {
            let tx = msg_tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(LocationMsg::LocationsFetched(vec![], None));
            });
        }

        Self {
            cfg,
            locations: vec![],
            selected_value: None,
            selected_label: None,
            search_text: String::new(),
            is_loading: true,
            is_switching: false,
            switch_status: None,
            switch_ok: false,
            flag_bytes: HashMap::new(),
            flag_textures: HashMap::new(),
            fetching_flags: HashSet::new(),
            msg_tx,
            msg_rx,
        }
    }

    fn ensure_flag(&mut self, iso_code: &str, ctx: &egui::Context) {
        if self.flag_textures.contains_key(iso_code) || self.fetching_flags.contains(iso_code) {
            return;
        }
        // Already have the bytes in memory - upload texture immediately.
        if let Some(bytes) = self.flag_bytes.get(iso_code).cloned() {
            self.upload_flag_texture(iso_code, &bytes, ctx);
            return;
        }
        // Fetch from disk cache or network.
        self.fetching_flags.insert(iso_code.to_string());
        let code = iso_code.to_string();
        let tx = self.msg_tx.clone();
        std::thread::spawn(move || {
            fetch_flag_bytes(&code); // caches to disk
            let _ = tx.send(LocationMsg::FlagReady(code));
        });
    }

    fn upload_flag_texture(&mut self, iso_code: &str, bytes: &[u8], ctx: &egui::Context) {
        if let Ok(img) = image::load_from_memory(bytes) {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let color_img =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
            let handle = ctx.load_texture(iso_code, color_img, egui::TextureOptions::LINEAR);
            self.flag_textures.insert(iso_code.to_string(), handle);
        }
    }
}

impl eframe::App for LocationWindow {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());

        // Drain background messages.
        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                LocationMsg::LocationsFetched(locs, current) => {
                    if let Some(ref cur_label) = current {
                        for loc in &locs {
                            if &loc.label == cur_label {
                                self.selected_value = Some(loc.value.clone());
                                self.selected_label = Some(loc.label.clone());
                                break;
                            }
                        }
                    }
                    self.locations = locs;
                    self.is_loading = false;
                }
                LocationMsg::FlagReady(code) => {
                    self.fetching_flags.remove(&code);
                    // Read from disk cache into memory, then upload texture.
                    let flags_dir = crate::config::get_flags_dir();
                    let path = flags_dir.join(format!("{code}.png"));
                    if let Ok(bytes) = std::fs::read(&path) {
                        self.upload_flag_texture(&code, &bytes, ctx);
                        self.flag_bytes.insert(code, bytes);
                    }
                    ctx.request_repaint();
                }
                LocationMsg::SwitchDone(Ok(confirmed_label)) => {
                    // Always sync last_location to what the backend actually reports.
                    self.cfg.last_location = Some(confirmed_label.clone());
                    self.cfg.save();
                    write_daemon_command(DaemonCommand::ReloadConfig);

                    // Verify the backend actually switched to the location we requested.
                    let requested = self.selected_label.as_deref().unwrap_or("");
                    if !confirmed_label.eq_ignore_ascii_case(requested) {
                        self.switch_status = Some(format!(
                            "Switch failed: backend is still on {confirmed_label}"
                        ));
                        self.switch_ok = false;
                        self.is_switching = false;
                        return;
                    }

                    // If currently connected, tell the daemon to reconnect.
                    if let Some(daemon) = load_daemon_status() {
                        if daemon.state == "Connected" {
                            write_daemon_command(DaemonCommand::Reconnect);
                        }
                    }
                    self.switch_ok = true;
                    self.is_switching = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                LocationMsg::SwitchDone(Err(e)) => {
                    self.switch_status = Some(format!("Error: {e}"));
                    self.switch_ok = false;
                    self.is_switching = false;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Select VPN Location");
            });
            ui.add_space(8.0);

            ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("Search countries or cities..."),
            );
            ui.add_space(4.0);

            if self.is_loading {
                ui.spinner();
                ui.label("Loading locations...");
            } else {
                let favorites = self.cfg.favorites.clone();
                let lower_search = self.search_text.to_lowercase();

                let mut sorted = self.locations.clone();
                sorted.sort_by(|a, b| {
                    let af = favorites.contains(&a.label);
                    let bf = favorites.contains(&b.label);
                    bf.cmp(&af)
                        .then(a.continent.cmp(&b.continent))
                        .then(a.label.cmp(&b.label))
                });

                let mut visible_iso: Vec<String> = Vec::new();

                egui::ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                    let mut last_section: Option<String> = None;
                    for loc in &sorted {
                        if !lower_search.is_empty()
                            && !loc.label.to_lowercase().contains(&lower_search)
                        {
                            continue;
                        }

                        let is_fav = favorites.contains(&loc.label);
                        let section = if is_fav {
                            "Favorites".to_string()
                        } else {
                            loc.continent.clone()
                        };
                        if last_section.as_deref() != Some(&section) {
                            last_section = Some(section.clone());
                            ui.add_space(4.0);
                            ui.colored_label(egui::Color32::GRAY, &section);
                            ui.separator();
                        }

                        let display_label = if is_fav {
                            format!("* {}", loc.label)
                        } else {
                            loc.label.clone()
                        };
                        let is_selected =
                            self.selected_value.as_deref() == Some(loc.value.as_str());
                        let iso = country_to_iso(&loc.country);

                        if let Some(code) = iso {
                            visible_iso.push(code.to_string());
                        }

                        let flag_tex = iso.and_then(|c| self.flag_textures.get(c));
                        let response = ui.horizontal(|ui| {
                            if let Some(tex) = flag_tex {
                                ui.image(egui::load::SizedTexture::new(
                                    tex.id(),
                                    egui::vec2(24.0, 18.0),
                                ));
                            } else {
                                ui.add_space(28.0);
                            }
                            ui.selectable_label(is_selected, &display_label)
                        });

                        // Left-click: immediately switch to this location.
                        if response.inner.clicked()
                            && !self.is_switching
                            && self.selected_value.as_deref() != Some(loc.value.as_str())
                        {
                            self.selected_value = Some(loc.value.clone());
                            self.selected_label = Some(loc.label.clone());
                            if let Some(url) = self.cfg.switcher_url.clone() {
                                let value = loc.value.clone();
                                self.is_switching = true;
                                self.switch_status = None;
                                let tx = self.msg_tx.clone();
                                std::thread::spawn(move || {
                                    let result = VpnApi::switch_location(&url, &value);
                                    let _ = tx.send(LocationMsg::SwitchDone(result));
                                });
                            }
                        }
                        // Right-click: toggle favorite.
                        if response.inner.secondary_clicked() {
                            self.cfg.toggle_favorite(&loc.label);
                        }
                    }
                });

                // Kick off flag fetches for all visible items.
                for code in visible_iso {
                    self.ensure_flag(&code, ctx);
                }

                if let Some(ref msg) = self.switch_status.clone() {
                    let color = if self.switch_ok {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::LIGHT_RED
                    };
                    ui.colored_label(color, msg);
                }

                if self.is_switching {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Switching to {}…",
                            self.selected_label.as_deref().unwrap_or("location")
                        ));
                    });
                }
            }
        });
    }
}

// --------------------------------------------------------------------------
// Flag image fetching (disk-cached)
// --------------------------------------------------------------------------

fn fetch_flag_bytes(iso_code: &str) -> Option<Vec<u8>> {
    let flags_dir = crate::config::get_flags_dir();
    let path = flags_dir.join(format!("{iso_code}.png"));
    if path.exists() {
        if let Ok(bytes) = std::fs::read(&path) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    let _ = std::fs::create_dir_all(&flags_dir);
    let url = format!("https://flagcdn.com/48x36/{iso_code}.png");
    match ureq::get(&url).timeout(Duration::from_secs(5)).call() {
        Ok(resp) => {
            use std::io::Read;
            let mut bytes = Vec::new();
            if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                let _ = std::fs::write(&path, &bytes);
                return Some(bytes);
            }
            None
        }
        Err(e) => {
            error!("Failed to fetch flag {iso_code}: {e}");
            None
        }
    }
}

// --------------------------------------------------------------------------
// Validation
// --------------------------------------------------------------------------

fn is_valid_ip(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

// --------------------------------------------------------------------------
// Legacy stubs kept for test compatibility
// --------------------------------------------------------------------------

/// Kept so existing test files that import it still compile.
#[derive(Default)]
pub struct SetupDialogState {
    pub vpn_gateway: String,
    pub isp_gateway: String,
    pub dns_server: String,
    pub switcher_url: String,
    pub error_msg: Option<String>,
    pub submitted: bool,
    pub cancelled: bool,
}

/// Kept so existing test files that import it still compile.
#[derive(Default)]
pub struct LocationDialogState {
    pub search_text: String,
    pub locations: Vec<Location>,
    pub selected_value: Option<String>,
    pub selected_label: Option<String>,
    pub is_loading: bool,
    pub is_switching: bool,
    pub switch_error: Option<String>,
    pub accepted: Option<String>,
    pub cancelled: bool,
}

pub use crate::autostart::AutoStartManager as AppAutoStart;
