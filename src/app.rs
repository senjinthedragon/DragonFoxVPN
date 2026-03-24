// app.rs - DragonFoxVPN: Main eframe application
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Central application controller. Owns the system tray icon, context menu,
// all dialog state, and the background network monitor thread. Drives the
// VPN state machine (Disabled → Enabling → Connected → Dropped) and the
// kill switch (fires after two consecutive failed network checks).

use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use egui::Context;
use log::{error, info, warn};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::api::{country_to_iso, Location, VpnApi};
use crate::autostart::AutoStartManager;
use crate::config::AppConfig;
use crate::icons::Icons;
use crate::state::{AppState, NetworkCheckResult, VpnState};
use crate::system::SystemHandler;

// --------------------------------------------------------------------------
// Shared dialog state (Arc<Mutex<>> so viewport callbacks can mutate it)
// --------------------------------------------------------------------------

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

#[derive(Default)]
pub struct LocationDialogState {
    pub search_text: String,
    pub locations: Vec<Location>,
    pub selected_value: Option<String>,
    pub selected_label: Option<String>,
    pub is_loading: bool,
    pub is_switching: bool,
    pub switch_error: Option<String>,
    /// Accepted with a new location label
    pub accepted: Option<String>,
    pub cancelled: bool,
}

// --------------------------------------------------------------------------
// Background thread message types
// --------------------------------------------------------------------------

pub enum BgMsg {
    NetworkCheck(NetworkCheckResult),
    LocationsFetched(Vec<Location>, Option<String>),
    LocationSwitchDone(Result<String, String>), // Ok(label) or Err(msg)
    FlagReady(String),                          // iso_code
}

// --------------------------------------------------------------------------
// Main App
// --------------------------------------------------------------------------

pub struct DragonFoxApp {
    // Shared state
    state: Arc<Mutex<AppState>>,
    config: Arc<Mutex<AppConfig>>,

    // Tray icon (kept alive)
    _tray: TrayIcon,

    // Menu item IDs for event matching
    menu_dashboard: MenuItem,
    menu_enable: MenuItem,
    menu_disable: MenuItem,
    menu_location: MenuItem,
    menu_autoconnect: CheckMenuItem,
    menu_autostart: CheckMenuItem,
    menu_settings: MenuItem,
    menu_exit: MenuItem,

    // Background channel
    bg_tx: mpsc::SyncSender<BgMsg>,
    bg_rx: mpsc::Receiver<BgMsg>,

    // Kill-switch drop counter
    drop_count: u32,

    // Next scheduled network check
    next_check: Instant,

    // Whether a network check is already running
    check_in_flight: bool,
    exiting: bool,

    // Dialog visibility flags
    show_setup_dialog: bool,
    show_location_dialog: bool,
    show_dashboard: bool,
    first_run: bool,

    // Dialog state
    setup_state: Arc<Mutex<SetupDialogState>>,
    location_state: Arc<Mutex<LocationDialogState>>,

    // Flag image textures: iso_code → egui TextureHandle
    flag_textures: std::collections::HashMap<String, egui::TextureHandle>,
}

impl DragonFoxApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Box<Self> {
        // Load config
        let config = AppConfig::load();
        let first_run = !config.setup_complete;
        let config = Arc::new(Mutex::new(config));

        // Load app state
        let mut state = AppState::default();
        if let Ok(cfg) = config.lock() {
            if let Some(loc) = &cfg.last_location {
                state.vpn_location = loc.clone();
            }
        }

        // Detect active adapter
        state.adapter_name = SystemHandler::get_active_adapter();
        info!("Active adapter: {}", state.adapter_name);

        let state = Arc::new(Mutex::new(state));

        // Build icons
        let icons = Icons::load();

        // Build tray menu
        let menu = Menu::new();
        let menu_dashboard = MenuItem::new("Status Dashboard", true, None);
        let menu_sep1 = PredefinedMenuItem::separator();
        let menu_enable = MenuItem::new("Enable VPN", true, None);
        let menu_disable = MenuItem::new("Disable VPN", false, None);
        let menu_sep2 = PredefinedMenuItem::separator();
        let menu_location = MenuItem::new("Change Location...", true, None);

        let auto_connect_checked = config.lock().map(|c| c.auto_connect).unwrap_or(false);
        let menu_autoconnect =
            CheckMenuItem::new("Auto-Connect on Start", true, auto_connect_checked, None);

        let autostart_avail = AutoStartManager::is_available();
        let autostart_checked = AutoStartManager::is_enabled();
        let menu_autostart = if autostart_avail {
            CheckMenuItem::new("Run on Startup", true, autostart_checked, None)
        } else {
            CheckMenuItem::new("Run on Startup (Windows only)", false, false, None)
        };

        let menu_sep3 = PredefinedMenuItem::separator();
        let menu_settings = MenuItem::new("Settings...", true, None);
        let menu_sep4 = PredefinedMenuItem::separator();
        let menu_exit = MenuItem::new("Exit", true, None);

        let _ = menu.append(&menu_dashboard);
        let _ = menu.append(&menu_sep1);
        let _ = menu.append(&menu_enable);
        let _ = menu.append(&menu_disable);
        let _ = menu.append(&menu_sep2);
        let _ = menu.append(&menu_location);
        let _ = menu.append(&menu_autoconnect);
        let _ = menu.append(&menu_autostart);
        let _ = menu.append(&menu_sep3);
        let _ = menu.append(&menu_settings);
        let _ = menu.append(&menu_sep4);
        let _ = menu.append(&menu_exit);

        // Build initial tray icon (yellow = disabled)
        let initial_icon = icons.disabled.unwrap_or_else(|| {
            // Fallback: create a tiny 1x1 icon
            tray_icon::Icon::from_rgba(vec![0xFF, 0xC1, 0x07, 0xFF], 1, 1).unwrap()
        });

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(initial_icon)
            .with_tooltip("DragonFoxVPN: Disabled")
            .build()
            .expect("Failed to create tray icon");

        // Background channel (bounded to avoid unbounded memory growth)
        let (bg_tx, bg_rx) = mpsc::sync_channel::<BgMsg>(32);

        // Setup dialog pre-fill from config
        let setup_state = {
            let cfg = config.lock().unwrap();
            SetupDialogState {
                vpn_gateway: cfg.vpn_gateway.clone().unwrap_or_default(),
                isp_gateway: cfg.isp_gateway.clone().unwrap_or_default(),
                dns_server: cfg.dns_server.clone().unwrap_or_default(),
                switcher_url: cfg.switcher_url.clone().unwrap_or_default(),
                error_msg: None,
                submitted: false,
                cancelled: false,
            }
        };

        let app = Box::new(DragonFoxApp {
            state,
            config,
            _tray: tray,
            menu_dashboard,
            menu_enable,
            menu_disable,
            menu_location,
            menu_autoconnect,
            menu_autostart,
            menu_settings,
            menu_exit,
            bg_tx,
            bg_rx,
            drop_count: 0,
            next_check: Instant::now() + Duration::from_secs(3),
            check_in_flight: false,
            exiting: false,
            show_setup_dialog: first_run,
            show_location_dialog: false,
            show_dashboard: false,
            first_run,
            setup_state: Arc::new(Mutex::new(setup_state)),
            location_state: Arc::new(Mutex::new(LocationDialogState::default())),
            flag_textures: std::collections::HashMap::new(),
        });

        // If auto-connect is configured, trigger enable after a short delay
        let should_auto_connect = app
            .config
            .lock()
            .map(|c| c.auto_connect && !first_run)
            .unwrap_or(false);
        if should_auto_connect {
            let state = Arc::clone(&app.state);
            let config = Arc::clone(&app.config);
            let tx = app.bg_tx.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1000));
                do_enable_vpn(&state, &config);
                // Trigger an immediate check
                let _ = tx.try_send(BgMsg::NetworkCheck(NetworkCheckResult {
                    vpn_active: false,
                    route_exists: false,
                    pi_reachable: true,
                }));
            });
        } else if !first_run {
            // Fetch current location in background
            let cfg_url = app.config.lock().ok().and_then(|c| c.switcher_url.clone());
            if let Some(url) = cfg_url {
                let tx = app.bg_tx.clone();
                std::thread::spawn(move || match VpnApi::fetch_locations(&url) {
                    Ok((locs, cur)) => {
                        let _ = tx.send(BgMsg::LocationsFetched(locs, cur));
                    }
                    Err(e) => warn!("Initial location fetch failed: {e}"),
                });
            }
        }

        // Request continuous repaints for timer-driven checks
        cc.egui_ctx.request_repaint_after(Duration::from_secs(3));

        app
    }

    // --------------------------------------------------------------------------
    // Tray icon update
    // --------------------------------------------------------------------------

    fn update_tray_icon(&self) {
        let (icon_rgba, tooltip) = {
            let st = self.state.lock().unwrap();
            let color = match st.vpn_state {
                VpnState::Connected => crate::icons::COLOR_GREEN,
                VpnState::Dropped => crate::icons::COLOR_RED,
                VpnState::ServerUnreachable => crate::icons::COLOR_GRAY,
                VpnState::Enabling => crate::icons::COLOR_BLUE,
                VpnState::Disabled => crate::icons::COLOR_YELLOW,
            };
            let tip = format!(
                "DragonFoxVPN: {}\nLocation: {}",
                st.vpn_state.as_str(),
                st.vpn_location
            );
            (crate::icons::create_status_icon_rgba(&color), tip)
        };

        if let Ok(icon) = tray_icon::Icon::from_rgba(icon_rgba, 64, 64) {
            let _ = self._tray.set_icon(Some(icon));
        }
        let _ = self._tray.set_tooltip(Some(&tooltip));

        // Update menu item enabled states
        let is_connected = self
            .state
            .lock()
            .map(|s| s.vpn_state == VpnState::Connected)
            .unwrap_or(false);
        self.menu_enable.set_enabled(!is_connected);
        self.menu_disable.set_enabled(is_connected);
    }

    // --------------------------------------------------------------------------
    // Network check scheduling
    // --------------------------------------------------------------------------

    fn maybe_start_check(&mut self, ctx: &Context) {
        if self.check_in_flight || Instant::now() < self.next_check {
            return;
        }
        // Don't check if manually disabled or setup incomplete
        let skip = self
            .config
            .lock()
            .map(|c| !c.setup_complete)
            .unwrap_or(true);
        if skip || self.show_setup_dialog {
            self.next_check = Instant::now() + Duration::from_secs(3);
            return;
        }

        self.check_in_flight = true;
        self.next_check = Instant::now() + Duration::from_secs(3);

        let vpn_gw = self.config.lock().ok().and_then(|c| c.vpn_gateway.clone());
        let isp_gw = self.config.lock().ok().and_then(|c| c.isp_gateway.clone());
        let adapter = self
            .state
            .lock()
            .map(|s| s.adapter_name.clone())
            .unwrap_or_default();
        let tx = self.bg_tx.clone();
        let ctx2 = ctx.clone();

        std::thread::spawn(move || {
            let vpn_gw = vpn_gw.unwrap_or_default();
            let isp_gw = isp_gw.unwrap_or_default();

            let route_exists = SystemHandler::is_route_active(&vpn_gw, &adapter);
            let vpn_active = if route_exists {
                SystemHandler::check_connection(&vpn_gw, &isp_gw)
            } else {
                false
            };
            let pi_reachable = SystemHandler::ping_host(&vpn_gw);

            let _ = tx.send(BgMsg::NetworkCheck(NetworkCheckResult {
                vpn_active,
                route_exists,
                pi_reachable,
            }));
            ctx2.request_repaint();
        });
    }

    // --------------------------------------------------------------------------
    // Process background messages
    // --------------------------------------------------------------------------

    fn process_bg_messages(&mut self, ctx: &Context) {
        while let Ok(msg) = self.bg_rx.try_recv() {
            match msg {
                BgMsg::NetworkCheck(result) => {
                    self.check_in_flight = false;
                    self.handle_network_result(result, ctx);
                }
                BgMsg::LocationsFetched(locs, current) => {
                    if let Some(cur) = &current {
                        if let Ok(mut st) = self.state.lock() {
                            st.vpn_location = cur.clone();
                        }
                        if let Ok(mut cfg) = self.config.lock() {
                            cfg.last_location = Some(cur.clone());
                            cfg.save();
                        }
                    }
                    if let Ok(mut ls) = self.location_state.lock() {
                        ls.locations = locs;
                        ls.is_loading = false;
                    }
                    self.update_tray_icon();
                }
                BgMsg::LocationSwitchDone(result) => {
                    if let Ok(mut ls) = self.location_state.lock() {
                        ls.is_switching = false;
                        match result {
                            Ok(label) => {
                                ls.accepted = Some(label.clone());
                            }
                            Err(msg) => {
                                ls.switch_error = Some(msg);
                            }
                        }
                    }
                }
                BgMsg::FlagReady(iso_code) => {
                    // Load the PNG from cache and upload to egui
                    let flags_dir = crate::config::get_flags_dir();
                    let path = flags_dir.join(format!("{iso_code}.png"));
                    if path.exists() {
                        if let Ok(img) = image::open(&path) {
                            let img = img.to_rgba8();
                            let (w, h) = img.dimensions();
                            let pixels: Vec<egui::Color32> = img
                                .pixels()
                                .map(|p| {
                                    egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
                                })
                                .collect();
                            let color_image = egui::ColorImage {
                                size: [w as usize, h as usize],
                                pixels,
                            };
                            let handle = ctx.load_texture(
                                &iso_code,
                                color_image,
                                egui::TextureOptions::LINEAR,
                            );
                            self.flag_textures.insert(iso_code, handle);
                        }
                    }
                }
            }
        }
    }

    fn handle_network_result(&mut self, result: NetworkCheckResult, ctx: &Context) {
        let manual_disable = self.state.lock().map(|s| s.manual_disable).unwrap_or(true);

        if result.vpn_active && result.route_exists {
            self.drop_count = 0;
            if let Ok(mut st) = self.state.lock() {
                if st.vpn_state != VpnState::Connected {
                    st.vpn_state = VpnState::Connected;
                    if st.connection_start_time.is_none() {
                        st.connection_start_time = Some(Instant::now());
                    }
                }
            }
        } else if result.vpn_active && !result.route_exists && !manual_disable {
            // Route dropped externally while VPN is still up - recover
            info!("VPN route missing but connection active, recovering...");
            self.do_enable(ctx);
        } else if !result.vpn_active && result.route_exists {
            self.drop_count += 1;
            if self.drop_count >= 2 {
                warn!("VPN connection dropped! Triggering kill switch.");
                let vpn_gw = self
                    .config
                    .lock()
                    .ok()
                    .and_then(|c| c.vpn_gateway.clone())
                    .unwrap_or_default();
                let adapter = self
                    .state
                    .lock()
                    .map(|s| s.adapter_name.clone())
                    .unwrap_or_default();
                crate::system::SystemHandler::kill_switch_delete_route(&vpn_gw, &adapter);

                if let Ok(mut st) = self.state.lock() {
                    st.vpn_state = VpnState::Dropped;
                    st.connection_start_time = None;
                }
                self.drop_count = 0;
                self.show_tray_notification("CONNECTION DROPPED. Kill switch active.");
            } else {
                if let Ok(mut st) = self.state.lock() {
                    if st.vpn_state == VpnState::Connected {
                        st.vpn_state = VpnState::Dropped;
                    }
                }
            }
        } else {
            // Not routing through VPN
            let current_state = self
                .state
                .lock()
                .map(|s| s.vpn_state.clone())
                .unwrap_or(VpnState::Disabled);
            if matches!(
                current_state,
                VpnState::Disabled | VpnState::ServerUnreachable
            ) {
                if !result.pi_reachable && current_state != VpnState::ServerUnreachable {
                    warn!("VPN server unreachable.");
                    if let Ok(mut st) = self.state.lock() {
                        st.vpn_state = VpnState::ServerUnreachable;
                    }
                } else if result.pi_reachable && current_state == VpnState::ServerUnreachable {
                    info!("VPN server reachable again.");
                    if let Ok(mut st) = self.state.lock() {
                        st.vpn_state = VpnState::Disabled;
                    }
                }
            }
        }

        self.update_tray_icon();
        ctx.request_repaint();
    }

    fn show_tray_notification(&self, _msg: &str) {
        // tray-icon 0.13 notification support varies by platform;
        // log it as a fallback.
        warn!("Tray notification: {_msg}");
    }

    // --------------------------------------------------------------------------
    // VPN enable / disable
    // --------------------------------------------------------------------------

    fn do_enable(&mut self, ctx: &Context) {
        info!("Enabling VPN routing...");
        if let Ok(mut st) = self.state.lock() {
            st.manual_disable = false;
            st.vpn_state = VpnState::Enabling;
        }
        self.update_tray_icon();

        let state = Arc::clone(&self.state);
        let config = Arc::clone(&self.config);
        let ctx2 = ctx.clone();

        std::thread::spawn(move || {
            do_enable_vpn(&state, &config);
            ctx2.request_repaint();
        });
    }

    fn do_disable(&mut self) {
        info!("Disabling VPN routing...");
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

        if let Ok(mut st) = self.state.lock() {
            st.manual_disable = true;
        }

        SystemHandler::disable_routing(&adapter, &vpn_gw);
        SystemHandler::flush_dns();

        if let Ok(mut st) = self.state.lock() {
            st.vpn_state = VpnState::Disabled;
            st.connection_start_time = None;
        }
        self.update_tray_icon();
    }

    // --------------------------------------------------------------------------
    // Dialog rendering helpers
    // --------------------------------------------------------------------------

    fn draw_setup_dialog(&mut self, ctx: &Context) {
        let first_run = self.first_run;
        let ss = Arc::clone(&self.setup_state);
        let cfg = Arc::clone(&self.config);

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("setup_dialog"),
            egui::ViewportBuilder::default()
                .with_title(if first_run {
                    "DragonFoxVPN — Initial Setup"
                } else {
                    "DragonFoxVPN — Settings"
                })
                .with_inner_size([500.0, 360.0])
                .with_resizable(false)
                .with_maximize_button(false),
            move |ctx, _class| {
                ctx.set_visuals(egui::Visuals::dark());

                // Block X-button close during mandatory first-run setup.
                if ctx.input(|i| i.viewport().close_requested()) {
                    if first_run {
                        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        if let Ok(mut s) = ss.lock() {
                            s.error_msg =
                                Some("Setup is required to use DragonFoxVPN.".to_string());
                        }
                    } else if let Ok(mut s) = ss.lock() {
                        s.cancelled = true;
                    }
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading(if first_run { "Initial Setup" } else { "Network Settings" });
                        if first_run {
                            ui.colored_label(
                                egui::Color32::GRAY,
                                "Enter your network details to get started.",
                            );
                        }
                    });
                    ui.add_space(8.0);

                    if let Ok(mut s) = ss.lock() {
                        egui::Grid::new("setup_grid")
                            .num_columns(2)
                            .spacing([8.0, 6.0])
                            .show(ui, |ui| {
                                ui.colored_label(egui::Color32::GRAY, "VPN Gateway IP");
                                ui.text_edit_singleline(&mut s.vpn_gateway);
                                ui.end_row();
                                ui.colored_label(egui::Color32::GRAY, "ISP Gateway IP");
                                ui.text_edit_singleline(&mut s.isp_gateway);
                                ui.end_row();
                                ui.colored_label(egui::Color32::GRAY, "DNS Server");
                                ui.text_edit_singleline(&mut s.dns_server);
                                ui.end_row();
                                ui.colored_label(egui::Color32::GRAY, "VPN Switcher URL");
                                ui.text_edit_singleline(&mut s.switcher_url);
                                ui.end_row();
                            });

                        if let Some(ref err) = s.error_msg.clone() {
                            ui.colored_label(egui::Color32::RED, err);
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if !first_run && ui.button("Cancel").clicked() {
                                s.cancelled = true;
                            }
                            if ui.button("Save Settings").clicked() {
                                validate_and_save(&mut s, &cfg);
                            }
                        });
                    }
                });
            },
        );

        // Apply results now that the viewport has rendered.
        if let Ok(mut s) = self.setup_state.lock() {
            if s.submitted {
                s.submitted = false;
                self.show_setup_dialog = false;
                self.first_run = false;
                let adapter = SystemHandler::get_active_adapter();
                if let Ok(mut st) = self.state.lock() {
                    st.adapter_name = adapter;
                }
            } else if s.cancelled {
                s.cancelled = false;
                self.show_setup_dialog = false;
            }
        }
    }

    fn draw_location_dialog(&mut self, ctx: &Context) {
        // Poll for location-switch acceptance
        // Handle accepted selection — close the dialog and optionally reconnect.
        let accepted = self.location_state.lock().ok().and_then(|ls| ls.accepted.clone());
        if let Some(label) = accepted {
            self.show_location_dialog = false;
            let was_connected = self
                .state
                .lock()
                .map(|s| s.vpn_state == VpnState::Connected)
                .unwrap_or(false);
            if let Ok(mut st) = self.state.lock() {
                st.vpn_location = label.clone();
            }
            if let Ok(mut cfg) = self.config.lock() {
                cfg.last_location = Some(label);
                cfg.save();
            }
            if let Ok(mut ls) = self.location_state.lock() {
                ls.accepted = None;
            }
            if was_connected {
                info!("Location changed, reconnecting...");
                self.do_disable();
                let state = Arc::clone(&self.state);
                let config = Arc::clone(&self.config);
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    do_enable_vpn(&state, &config);
                    ctx2.request_repaint();
                });
            }
            return;
        }

        let ls_arc = Arc::clone(&self.location_state);
        let config_arc = Arc::clone(&self.config);
        let bg_tx = self.bg_tx.clone();
        // Clone textures for read access inside the closure; new ones arrive via
        // BgMsg::FlagReady and are added to self.flag_textures by process_bg_messages
        // before this method is called next frame.
        let flag_textures = self.flag_textures.clone();
        let close = Arc::new(Mutex::new(false));
        let close_inner = Arc::clone(&close);

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("location_dialog"),
            egui::ViewportBuilder::default()
                .with_title("Change VPN Location")
                .with_inner_size([620.0, 700.0])
                .with_resizable(true),
            move |ctx, _class| {
                ctx.set_visuals(egui::Visuals::dark());

                if ctx.input(|i| i.viewport().close_requested()) {
                    *close_inner.lock().unwrap() = true;
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Select VPN Location");
                    });
                    ui.add_space(8.0);

                    let mut search =
                        ls_arc.lock().map(|ls| ls.search_text.clone()).unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut search)
                                .hint_text("Search countries or cities..."),
                        )
                        .changed()
                    {
                        if let Ok(mut ls) = ls_arc.lock() {
                            ls.search_text = search.clone();
                        }
                    }
                    ui.add_space(4.0);

                    let is_loading = ls_arc.lock().map(|ls| ls.is_loading).unwrap_or(false);
                    let is_switching = ls_arc.lock().map(|ls| ls.is_switching).unwrap_or(false);

                    if is_loading {
                        ui.spinner();
                        ui.label("Loading locations...");
                    } else {
                        let (locations, favorites, selected_value, switch_error) = {
                            let ls = ls_arc.lock().unwrap();
                            (
                                ls.locations.clone(),
                                config_arc.lock().map(|c| c.favorites.clone()).unwrap_or_default(),
                                ls.selected_value.clone(),
                                ls.switch_error.clone(),
                            )
                        };

                        let lower_search = search.to_lowercase();
                        let mut sorted = locations.clone();
                        sorted.sort_by(|a, b| {
                            let af = favorites.contains(&a.label);
                            let bf = favorites.contains(&b.label);
                            bf.cmp(&af)
                                .then(a.continent.cmp(&b.continent))
                                .then(a.label.cmp(&b.label))
                        });

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
                                    selected_value.as_deref() == Some(loc.value.as_str());
                                let iso = country_to_iso(&loc.country);
                                let flag_tex = iso.and_then(|code| flag_textures.get(code));
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
                                    if let Ok(mut ls) = ls_arc.lock() {
                                        ls.selected_value = Some(loc.value.clone());
                                        ls.selected_label = Some(loc.label.clone());
                                    }
                                }
                                if response.inner.secondary_clicked() {
                                    if let Ok(mut cfg) = config_arc.lock() {
                                        cfg.toggle_favorite(&loc.label);
                                    }
                                }
                                // Enqueue flag fetch if not yet in cache.
                                if let Some(code) = iso {
                                    if !flag_textures.contains_key(code) {
                                        let code = code.to_string();
                                        let flags_dir = crate::config::get_flags_dir();
                                        let tx = bg_tx.clone();
                                        let ctx2 = ctx.clone();
                                        std::thread::spawn(move || {
                                            let path = flags_dir.join(format!("{code}.png"));
                                            if !path.exists() {
                                                fetch_flag(&code, &flags_dir);
                                            }
                                            let _ = tx.send(BgMsg::FlagReady(code));
                                            ctx2.request_repaint();
                                        });
                                    }
                                }
                            }
                        });

                        if let Some(err) = switch_error {
                            ui.colored_label(egui::Color32::RED, &err);
                        }
                        ui.add_space(8.0);

                        let has_selection =
                            ls_arc.lock().map(|ls| ls.selected_value.is_some()).unwrap_or(false);
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                if let Ok(mut ls) = ls_arc.lock() {
                                    ls.cancelled = true;
                                }
                            }
                            let switch_label = ls_arc
                                .lock()
                                .ok()
                                .and_then(|ls| ls.selected_label.clone())
                                .map(|l| format!("Switch to {l}"))
                                .unwrap_or_else(|| "Switch Location".to_string());
                            if ui
                                .add_enabled(
                                    has_selection && !is_switching,
                                    egui::Button::new(&switch_label),
                                )
                                .clicked()
                            {
                                let (val, lbl, url) = {
                                    let ls = ls_arc.lock().unwrap();
                                    let cfg = config_arc.lock().unwrap();
                                    (
                                        ls.selected_value.clone(),
                                        ls.selected_label.clone(),
                                        cfg.switcher_url.clone(),
                                    )
                                };
                                if let (Some(value), Some(label), Some(url)) = (val, lbl, url) {
                                    if let Ok(mut ls) = ls_arc.lock() {
                                        ls.is_switching = true;
                                        ls.switch_error = None;
                                    }
                                    let tx = bg_tx.clone();
                                    let ctx2 = ctx.clone();
                                    std::thread::spawn(move || {
                                        let result = match VpnApi::switch_location(&url, &value) {
                                            Ok(_) => {
                                                std::thread::sleep(Duration::from_secs(2));
                                                Ok(label)
                                            }
                                            Err(e) => Err(e),
                                        };
                                        let _ = tx.send(BgMsg::LocationSwitchDone(result));
                                        ctx2.request_repaint();
                                    });
                                }
                            }
                            if is_switching {
                                ui.spinner();
                            }
                        });
                    }
                });
            },
        );

        if *close.lock().unwrap() {
            self.show_location_dialog = false;
            if let Ok(mut ls) = self.location_state.lock() {
                ls.cancelled = false;
            }
            return;
        }
        let cancelled = self.location_state.lock().map(|ls| ls.cancelled).unwrap_or(false);
        if cancelled {
            self.show_location_dialog = false;
            if let Ok(mut ls) = self.location_state.lock() {
                ls.cancelled = false;
            }
        }
    }

    fn draw_dashboard(&mut self, ctx: &Context) {
        let state_arc = Arc::clone(&self.state);
        let config_arc = Arc::clone(&self.config);
        let close = Arc::new(Mutex::new(false));
        let close_inner = Arc::clone(&close);

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("dashboard"),
            egui::ViewportBuilder::default()
                .with_title("DragonFox Status")
                .with_inner_size([420.0, 340.0])
                .with_resizable(false)
                .with_maximize_button(false),
            move |ctx, _class| {
                ctx.set_visuals(egui::Visuals::dark());

                if ctx.input(|i| i.viewport().close_requested()) {
                    *close_inner.lock().unwrap() = true;
                }

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x00, 0x7A, 0xCC),
                            egui::RichText::new("DragonFox VPN").size(24.0).strong(),
                        );
                    });
                    ui.add_space(12.0);

                    let (state_str, state_color, location, gateway, start_time) = {
                        let st = state_arc.lock().unwrap();
                        let cfg = config_arc.lock().unwrap();
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
                            *close_inner.lock().unwrap() = true;
                        }
                    });
                });

                // Tick every second so the duration counter updates.
                ctx.request_repaint_after(Duration::from_secs(1));
            },
        );

        if *close.lock().unwrap() {
            self.show_dashboard = false;
        }
    }

    // --------------------------------------------------------------------------
    // Open location dialog and kick off fetch
    // --------------------------------------------------------------------------

    fn open_location_dialog(&mut self, ctx: &Context) {
        {
            let mut ls = self.location_state.lock().unwrap();
            *ls = LocationDialogState::default();
            ls.is_loading = true;
        }

        self.show_location_dialog = true;

        let url = self.config.lock().ok().and_then(|c| c.switcher_url.clone());
        if let Some(url) = url {
            let tx = self.bg_tx.clone();
            let ctx2 = ctx.clone();
            std::thread::spawn(move || {
                match VpnApi::fetch_locations(&url) {
                    Ok((locs, cur)) => {
                        let _ = tx.send(BgMsg::LocationsFetched(locs, cur));
                    }
                    Err(e) => {
                        error!("Location fetch failed: {e}");
                        let _ = tx.send(BgMsg::LocationsFetched(vec![], None));
                    }
                }
                ctx2.request_repaint();
            });
        } else {
            if let Ok(mut ls) = self.location_state.lock() {
                ls.is_loading = false;
            }
        }
    }

}

// --------------------------------------------------------------------------
// Validation helper (called from the setup viewport closure)
// --------------------------------------------------------------------------

fn validate_and_save(ss: &mut SetupDialogState, config: &Arc<Mutex<AppConfig>>) {
    let vpn_gw = ss.vpn_gateway.trim().to_string();
    let isp_gw = ss.isp_gateway.trim().to_string();
    let dns = ss.dns_server.trim().to_string();
    let url = ss.switcher_url.trim().to_string();

    if vpn_gw.is_empty() || isp_gw.is_empty() || dns.is_empty() || url.is_empty() {
        ss.error_msg = Some("All fields are required.".to_string());
        return;
    }
    if !is_valid_ip(&vpn_gw) || !is_valid_ip(&isp_gw) || !is_valid_ip(&dns) {
        ss.error_msg = Some("Gateway and DNS fields must be valid IP addresses.".to_string());
        return;
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        ss.error_msg = Some("Switcher URL must start with http:// or https://".to_string());
        return;
    }

    if let Ok(mut cfg) = config.lock() {
        cfg.vpn_gateway = Some(vpn_gw);
        cfg.isp_gateway = Some(isp_gw);
        cfg.dns_server = Some(dns);
        cfg.switcher_url = Some(url);
        cfg.setup_complete = true;
        cfg.save();
    }

    ss.error_msg = None;
    ss.submitted = true;
}

impl eframe::App for DragonFoxApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Pump GTK events so the tray icon and its menu receive updates.
        // eframe uses a winit event loop, not GTK's, so we must do this manually.
        #[cfg(target_os = "linux")]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Apply dark visuals
        ctx.set_visuals(egui::Visuals::dark());

        // The main viewport is a permanent 1×1 invisible anchor — it must never
        // close unless the user chose Exit. Cancel any unexpected close events.
        if ctx.input(|i| i.viewport().close_requested()) && !self.exiting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        // Process background messages
        self.process_bg_messages(ctx);

        // Poll tray icon events
        if let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if event.click_type == tray_icon::ClickType::Double {
                self.show_dashboard = true;
            }
        }

        // Poll menu events
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id;
            if id == self.menu_dashboard.id() {
                self.show_dashboard = true;
            } else if id == self.menu_enable.id() {
                self.do_enable(ctx);
            } else if id == self.menu_disable.id() {
                self.do_disable();
            } else if id == self.menu_location.id() {
                self.open_location_dialog(ctx);
            } else if id == self.menu_autoconnect.id() {
                let checked = self.menu_autoconnect.is_checked();
                if let Ok(mut cfg) = self.config.lock() {
                    cfg.auto_connect = checked;
                    cfg.save();
                }
            } else if id == self.menu_autostart.id() {
                let checked = self.menu_autostart.is_checked();
                AutoStartManager::set_autostart(checked);
            } else if id == self.menu_settings.id() {
                if let Ok(cfg) = self.config.lock() {
                    *self.setup_state.lock().unwrap() = SetupDialogState {
                        vpn_gateway: cfg.vpn_gateway.clone().unwrap_or_default(),
                        isp_gateway: cfg.isp_gateway.clone().unwrap_or_default(),
                        dns_server: cfg.dns_server.clone().unwrap_or_default(),
                        switcher_url: cfg.switcher_url.clone().unwrap_or_default(),
                        error_msg: None,
                        submitted: false,
                        cancelled: false,
                    };
                }
                self.show_setup_dialog = true;
            } else if id == self.menu_exit.id() {
                self.exiting = true;
                self.do_disable();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Network check timer
        self.maybe_start_check(ctx);

        // Each dialog lives in its own immediate viewport (a separate OS window).
        // When a flag is false, show_viewport_immediate is not called and eframe
        // closes that window automatically — no visibility commands needed.
        if self.show_setup_dialog {
            self.draw_setup_dialog(ctx);
        }
        if self.show_location_dialog {
            self.draw_location_dialog(ctx);
        }
        if self.show_dashboard {
            self.draw_dashboard(ctx);
        }

        // Keep the 1×1 anchor ticking for the network-check timer.
        // Dialog viewports repaint themselves internally.
        ctx.request_repaint_after(Duration::from_secs(3));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.do_disable();
    }
}

// --------------------------------------------------------------------------
// Free functions (called from threads, so no &self)
// --------------------------------------------------------------------------

fn do_enable_vpn(state: &Arc<Mutex<AppState>>, config: &Arc<Mutex<AppConfig>>) {
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

    let success = SystemHandler::enable_routing(&adapter, &vpn_gw, &dns);
    SystemHandler::flush_dns();

    if let Ok(mut st) = state.lock() {
        if success {
            st.vpn_state = VpnState::Connected;
            st.connection_start_time = Some(Instant::now());
        } else {
            error!("Failed to enable routing.");
            st.vpn_state = VpnState::Disabled;
            st.manual_disable = true;
            // Attempt cleanup
            drop(st); // release lock before calling disable
            SystemHandler::disable_routing(&adapter, &vpn_gw);
        }
    }
}

fn fetch_flag(iso_code: &str, flags_dir: &std::path::Path) {
    let path = flags_dir.join(format!("{iso_code}.png"));
    if path.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(flags_dir);
    let url = format!("https://flagcdn.com/48x36/{iso_code}.png");
    match ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Ok(resp) => {
            use std::io::Read;
            let mut bytes = Vec::new();
            let mut reader = resp.into_reader();
            if let Err(e) = reader.read_to_end(&mut bytes) {
                error!("Failed to read flag body for {iso_code}: {e}");
                return;
            }
            if !bytes.is_empty() {
                if let Err(e) = std::fs::write(&path, &bytes) {
                    error!("Failed to write flag {iso_code}: {e}");
                }
            }
        }
        Err(e) => error!("Failed to fetch flag {iso_code}: {e}"),
    }
}

fn is_valid_ip(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}
