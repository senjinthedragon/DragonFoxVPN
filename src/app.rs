// app.rs - DragonFoxVPN: Dialog structs for eframe::run_native
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Defines three dialog structs — SetupDialog, DashboardDialog, and
// LocationDialog — each implementing eframe::App and opened on demand by
// main() via eframe::run_native(). No persistent main window exists; each
// dialog is a proper OS window that opens, does its job, and closes.

use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use log::{error, info, warn};
#[cfg(target_os = "linux")]
use tray_icon::{menu::MenuEvent, TrayIconEvent};

use crate::api::{country_to_iso, Location, VpnApi};
use crate::config::AppConfig;
use crate::state::{AppState, VpnState};
use crate::system::SystemHandler;

/// Shared flag image cache: iso_code → raw PNG bytes.
/// Shared across successive opens of LocationDialog so re-upload is fast.
pub type FlagCache = Arc<Mutex<HashMap<String, Vec<u8>>>>;

/// Service the GTK event loop so the GTK Wayland socket doesn't fill up.
///
/// On Wayland, GTK and winit each hold their own Wayland connection. If GTK's
/// connection is never serviced, its socket buffer fills and the compositor
/// stops sending events to ALL connections from this process — including
/// winit's — causing the dialog window to freeze and eventually be killed.
///
/// We also drain the tray-icon event channels so that any tray clicks made
/// while a dialog is open are silently discarded rather than queued.
#[cfg(target_os = "linux")]
fn service_gtk() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
    while TrayIconEvent::receiver().try_recv().is_ok() {}
    while MenuEvent::receiver().try_recv().is_ok() {}
}

// --------------------------------------------------------------------------
// Internal background message types for LocationDialog
// --------------------------------------------------------------------------

enum LocationMsg {
    LocationsFetched(Vec<Location>, Option<String>),
    FlagReady(String),
    SwitchDone(Result<String, String>),
}

// --------------------------------------------------------------------------
// SetupDialog
// --------------------------------------------------------------------------

/// Initial-setup / Settings dialog.
pub struct SetupDialog {
    config: Arc<Mutex<AppConfig>>,
    first_run: bool,
    vpn_gateway: String,
    isp_gateway: String,
    dns_server: String,
    switcher_url: String,
    error_msg: Option<String>,
    saved: bool,
}

impl SetupDialog {
    pub fn new(config: Arc<Mutex<AppConfig>>, first_run: bool) -> Self {
        let (vpn_gw, isp_gw, dns, url) = {
            let cfg = config.lock().unwrap();
            (
                cfg.vpn_gateway.clone().unwrap_or_default(),
                cfg.isp_gateway.clone().unwrap_or_default(),
                cfg.dns_server.clone().unwrap_or_default(),
                cfg.switcher_url.clone().unwrap_or_default(),
            )
        };
        Self {
            config,
            first_run,
            vpn_gateway: vpn_gw,
            isp_gateway: isp_gw,
            dns_server: dns,
            switcher_url: url,
            error_msg: None,
            saved: false,
        }
    }
}

impl eframe::App for SetupDialog {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        service_gtk();

        ctx.set_visuals(egui::Visuals::dark());

        // Block the close button only on first run (setup is mandatory).
        if ctx.input(|i| i.viewport().close_requested()) && self.first_run && !self.saved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.error_msg = Some("Setup is required to use DragonFoxVPN.".to_string());
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

            egui::Grid::new("setup_grid")
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

            if let Some(ref err) = self.error_msg.clone() {
                ui.colored_label(egui::Color32::RED, err);
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if !self.first_run && ui.button("Cancel").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("Save Settings").clicked() {
                    self.try_save(ctx);
                }
            });
        });
    }
}

impl SetupDialog {
    fn try_save(&mut self, ctx: &egui::Context) {
        let vpn_gw = self.vpn_gateway.trim().to_string();
        let isp_gw = self.isp_gateway.trim().to_string();
        let dns = self.dns_server.trim().to_string();
        let url = self.switcher_url.trim().to_string();

        if vpn_gw.is_empty() || isp_gw.is_empty() || dns.is_empty() || url.is_empty() {
            self.error_msg = Some("All fields are required.".to_string());
            return;
        }
        if !is_valid_ip(&vpn_gw) || !is_valid_ip(&isp_gw) || !is_valid_ip(&dns) {
            self.error_msg =
                Some("Gateway and DNS fields must be valid IP addresses.".to_string());
            return;
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.error_msg =
                Some("Switcher URL must start with http:// or https://".to_string());
            return;
        }

        if let Ok(mut cfg) = self.config.lock() {
            cfg.vpn_gateway = Some(vpn_gw);
            cfg.isp_gateway = Some(isp_gw);
            cfg.dns_server = Some(dns);
            cfg.switcher_url = Some(url);
            cfg.setup_complete = true;
            cfg.save();
        }

        // Refresh adapter after setup.
        let adapter = SystemHandler::get_active_adapter();
        info!("Setup saved. Active adapter: {adapter}");
        self.error_msg = None;
        self.saved = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

// --------------------------------------------------------------------------
// DashboardDialog
// --------------------------------------------------------------------------

/// Status dashboard dialog.
pub struct DashboardDialog {
    state: Arc<Mutex<AppState>>,
    config: Arc<Mutex<AppConfig>>,
    closing: bool,
}

impl DashboardDialog {
    pub fn new(state: Arc<Mutex<AppState>>, config: Arc<Mutex<AppConfig>>) -> Self {
        Self { state, config, closing: false }
    }
}

impl eframe::App for DashboardDialog {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        service_gtk();

        ctx.set_visuals(egui::Visuals::dark());

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0x00, 0x7A, 0xCC),
                    egui::RichText::new("DragonFox VPN").size(24.0).strong(),
                );
            });
            ui.add_space(12.0);

            let (state_str, state_color, location, gateway, start_time) = {
                let st = self.state.lock().unwrap();
                let cfg = self.config.lock().unwrap();
                (
                    st.vpn_state.as_str(),
                    st.vpn_state.color(),
                    st.vpn_location.clone(),
                    cfg.vpn_gateway.clone().unwrap_or_else(|| "N/A".to_string()),
                    st.connection_start_time,
                )
            };

            egui::Frame::none()
                .fill(egui::Color32::from_rgb(0x25, 0x25, 0x26))
                .rounding(egui::Rounding::same(8.0))
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.colored_label(
                            state_color,
                            egui::RichText::new(state_str.to_uppercase())
                                .size(18.0)
                                .strong(),
                        );
                    });
                });

            ui.add_space(8.0);
            ui.label(format!("Location: {location}"));
            ui.label(format!("Gateway: {gateway}"));

            let duration_str = if let Some(start) = start_time {
                let secs = start.elapsed().as_secs();
                format!("{:02}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
            } else {
                "--:--:--".to_string()
            };
            ui.label(format!("Duration: {duration_str}"));

            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                if ui.button("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });

        // Stop scheduling repaints once close has been requested so the
        // 1-second timer cannot fire after the event loop begins shutting
        // down (which would race with eframe's cleanup and cause a hang).
        if ctx.input(|i| i.viewport().close_requested()) {
            self.closing = true;
        }
        if !self.closing {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}

// --------------------------------------------------------------------------
// LocationDialog
// --------------------------------------------------------------------------

/// Change-location dialog with live search, favorites, and flag images.
pub struct LocationDialog {
    state: Arc<Mutex<AppState>>,
    config: Arc<Mutex<AppConfig>>,
    flag_cache: FlagCache,
    flag_textures: HashMap<String, egui::TextureHandle>,
    fetching_flags: HashSet<String>,
    locations: Vec<Location>,
    selected_value: Option<String>,
    selected_label: Option<String>,
    search_text: String,
    is_loading: bool,
    is_switching: bool,
    switch_error: Option<String>,
    msg_tx: mpsc::SyncSender<LocationMsg>,
    msg_rx: mpsc::Receiver<LocationMsg>,
}

impl LocationDialog {
    pub fn new(
        state: Arc<Mutex<AppState>>,
        config: Arc<Mutex<AppConfig>>,
        flag_cache: FlagCache,
    ) -> Self {
        let (msg_tx, msg_rx) = mpsc::sync_channel(32);
        let dialog = Self {
            state,
            config,
            flag_cache,
            flag_textures: HashMap::new(),
            fetching_flags: HashSet::new(),
            locations: vec![],
            selected_value: None,
            selected_label: None,
            search_text: String::new(),
            is_loading: true,
            is_switching: false,
            switch_error: None,
            msg_tx,
            msg_rx,
        };
        dialog.fetch_locations();
        dialog
    }

    fn fetch_locations(&self) {
        let url = self.config.lock().ok().and_then(|c| c.switcher_url.clone());
        if let Some(url) = url {
            let tx = self.msg_tx.clone();
            std::thread::spawn(move || {
                match VpnApi::fetch_locations(&url) {
                    Ok((locs, cur)) => {
                        let _ = tx.send(LocationMsg::LocationsFetched(locs, cur));
                    }
                    Err(e) => {
                        warn!("Location fetch failed: {e}");
                        let _ = tx.send(LocationMsg::LocationsFetched(vec![], None));
                    }
                }
            });
        } else {
            // No URL configured; signal load complete immediately.
            let tx = self.msg_tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(LocationMsg::LocationsFetched(vec![], None));
            });
        }
    }

    fn ensure_flag(&mut self, iso_code: &str, ctx: &egui::Context) {
        if self.flag_textures.contains_key(iso_code) || self.fetching_flags.contains(iso_code) {
            return;
        }

        // If the bytes are already in the shared cache, load them immediately.
        if let Some(bytes) =
            self.flag_cache.lock().ok().and_then(|c| c.get(iso_code).cloned())
        {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let color_img = egui::ColorImage::from_rgba_unmultiplied(
                    [w as usize, h as usize],
                    &rgba,
                );
                let handle =
                    ctx.load_texture(iso_code, color_img, egui::TextureOptions::LINEAR);
                self.flag_textures.insert(iso_code.to_string(), handle);
                return;
            }
        }

        // Mark in-flight and spawn a fetch thread.
        self.fetching_flags.insert(iso_code.to_string());
        let code = iso_code.to_string();
        let tx = self.msg_tx.clone();
        let cache = Arc::clone(&self.flag_cache);
        std::thread::spawn(move || {
            let bytes = fetch_flag_bytes(&code);
            if let Some(bytes) = bytes {
                if let Ok(mut c) = cache.lock() {
                    c.insert(code.clone(), bytes);
                }
            }
            let _ = tx.send(LocationMsg::FlagReady(code));
        });
    }
}

impl eframe::App for LocationDialog {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(target_os = "linux")]
        service_gtk();

        ctx.set_visuals(egui::Visuals::dark());

        // Process background messages.
        while let Ok(msg) = self.msg_rx.try_recv() {
            match msg {
                LocationMsg::LocationsFetched(locs, current) => {
                    if let Some(ref cur_label) = current {
                        // Pre-select the currently active location.
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
                    // Load bytes from cache into a texture.
                    if let Some(bytes) =
                        self.flag_cache.lock().ok().and_then(|c| c.get(&code).cloned())
                    {
                        if let Ok(img) = image::load_from_memory(&bytes) {
                            let rgba = img.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            let color_img = egui::ColorImage::from_rgba_unmultiplied(
                                [w as usize, h as usize],
                                &rgba,
                            );
                            let handle = ctx.load_texture(
                                &code,
                                color_img,
                                egui::TextureOptions::LINEAR,
                            );
                            self.flag_textures.insert(code, handle);
                        }
                    }
                    ctx.request_repaint();
                }
                LocationMsg::SwitchDone(Ok(label)) => {
                    // Update state and config.
                    if let Ok(mut st) = self.state.lock() {
                        st.vpn_location = label.clone();
                    }
                    if let Ok(mut cfg) = self.config.lock() {
                        cfg.last_location = Some(label.clone());
                        cfg.save();
                    }

                    // Check if we were connected; if so reconnect in a thread.
                    let was_connected = self
                        .state
                        .lock()
                        .map(|s| s.vpn_state == VpnState::Connected)
                        .unwrap_or(false);

                    if was_connected {
                        info!("Location changed, reconnecting...");
                        // Disable first.
                        {
                            let adapter = self
                                .state
                                .lock()
                                .map(|s| s.adapter_name.clone())
                                .unwrap_or_default();
                            let vpn_gw = self
                                .config
                                .lock()
                                .ok()
                                .and_then(|c| c.vpn_gateway.clone())
                                .unwrap_or_default();
                            crate::vpn_runtime::disable_vpn(&adapter, &vpn_gw);
                            if let Ok(mut st) = self.state.lock() {
                                st.vpn_state = VpnState::Disabled;
                                st.connection_start_time = None;
                                st.manual_disable = true;
                            }
                        }
                        let state2 = Arc::clone(&self.state);
                        let config2 = Arc::clone(&self.config);
                        std::thread::spawn(move || {
                            std::thread::sleep(Duration::from_millis(1500));
                            do_enable_vpn_thread(state2, config2);
                        });
                    }

                    self.is_switching = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                LocationMsg::SwitchDone(Err(e)) => {
                    self.is_switching = false;
                    self.switch_error = Some(e);
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("Select VPN Location");
            });
            ui.add_space(8.0);

            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.search_text)
                        .hint_text("Search countries or cities..."),
                )
                .changed()
            {}
            ui.add_space(4.0);

            if self.is_loading {
                ui.spinner();
                ui.label("Loading locations...");
            } else {
                let favorites = self
                    .config
                    .lock()
                    .map(|c| c.favorites.clone())
                    .unwrap_or_default();
                let lower_search = self.search_text.to_lowercase();

                let mut sorted = self.locations.clone();
                sorted.sort_by(|a, b| {
                    let af = favorites.contains(&a.label);
                    let bf = favorites.contains(&b.label);
                    bf.cmp(&af)
                        .then(a.continent.cmp(&b.continent))
                        .then(a.label.cmp(&b.label))
                });

                // Collect iso codes for visible items so we can kick off flag fetches.
                let mut visible_iso: Vec<String> = Vec::new();

                egui::ScrollArea::vertical().max_height(440.0).show(ui, |ui| {
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

                        let flag_tex = iso.and_then(|code| self.flag_textures.get(code));
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

                        if response.inner.clicked() {
                            self.selected_value = Some(loc.value.clone());
                            self.selected_label = Some(loc.label.clone());
                        }

                        if response.inner.secondary_clicked() {
                            if let Ok(mut cfg) = self.config.lock() {
                                cfg.toggle_favorite(&loc.label);
                            }
                        }
                    }
                });

                // Kick off flag fetches for visible iso codes.
                for code in visible_iso {
                    self.ensure_flag(&code, ctx);
                }

                if let Some(ref err) = self.switch_error.clone() {
                    ui.colored_label(egui::Color32::RED, err);
                }
                ui.add_space(8.0);

                let has_selection = self.selected_value.is_some();
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    let switch_label = self
                        .selected_label
                        .as_deref()
                        .map(|l| format!("Switch to {l}"))
                        .unwrap_or_else(|| "Switch Location".to_string());
                    if ui
                        .add_enabled(
                            has_selection && !self.is_switching,
                            egui::Button::new(&switch_label),
                        )
                        .clicked()
                    {
                        let val = self.selected_value.clone();
                        let lbl = self.selected_label.clone();
                        let url =
                            self.config.lock().ok().and_then(|c| c.switcher_url.clone());

                        if let (Some(value), Some(label), Some(url)) = (val, lbl, url) {
                            self.is_switching = true;
                            self.switch_error = None;
                            let tx = self.msg_tx.clone();
                            std::thread::spawn(move || {
                                let result = match VpnApi::switch_location(&url, &value) {
                                    Ok(_) => {
                                        std::thread::sleep(Duration::from_secs(2));
                                        Ok(label)
                                    }
                                    Err(e) => Err(e),
                                };
                                let _ = tx.send(LocationMsg::SwitchDone(result));
                            });
                        }
                    }
                    if self.is_switching {
                        ui.spinner();
                    }
                });
            }
        });
    }
}

// --------------------------------------------------------------------------
// Helper: enable VPN in a thread (used by LocationDialog reconnect)
// --------------------------------------------------------------------------

fn do_enable_vpn_thread(state: Arc<Mutex<AppState>>, config: Arc<Mutex<AppConfig>>) {
    let (adapter, vpn_gw, dns) = {
        let st = state.lock().unwrap();
        let cfg = config.lock().unwrap();
        (
            st.adapter_name.clone(),
            cfg.vpn_gateway.clone().unwrap_or_default(),
            cfg.dns_server.clone().unwrap_or_default(),
        )
    };

    if let Ok(mut st) = state.lock() {
        st.vpn_state = VpnState::Enabling;
        st.manual_disable = false;
    }

    let success = crate::vpn_runtime::enable_vpn(&adapter, &vpn_gw, &dns);

    if let Ok(mut st) = state.lock() {
        if success {
            st.vpn_state = VpnState::Connected;
            st.connection_start_time = Some(std::time::Instant::now());
        } else {
            error!("Failed to enable routing after location switch.");
            st.vpn_state = VpnState::Disabled;
            st.manual_disable = true;
        }
    }
}

// --------------------------------------------------------------------------
// Helper: fetch flag PNG bytes from flagcdn.com
// --------------------------------------------------------------------------

fn fetch_flag_bytes(iso_code: &str) -> Option<Vec<u8>> {
    // Check disk cache first.
    let flags_dir = crate::config::get_flags_dir();
    let path = flags_dir.join(format!("{iso_code}.png"));
    if path.exists() {
        if let Ok(bytes) = std::fs::read(&path) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }

    // Download from flagcdn.com.
    let _ = std::fs::create_dir_all(&flags_dir);
    let url = format!("https://flagcdn.com/48x36/{iso_code}.png");
    match ureq::get(&url)
        .timeout(Duration::from_secs(5))
        .call()
    {
        Ok(resp) => {
            use std::io::Read;
            let mut bytes = Vec::new();
            if resp.into_reader().read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
                // Cache to disk.
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
// Validation helper
// --------------------------------------------------------------------------

fn is_valid_ip(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

// --------------------------------------------------------------------------
// Re-exports kept for test compatibility
// --------------------------------------------------------------------------

/// Legacy type kept so existing test files that import it still compile.
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

/// Legacy type kept so existing test files that import it still compile.
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

// Keep AutoStartManager accessible via app module (was used from tests)
pub use crate::autostart::AutoStartManager as AppAutoStart;
