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
use crate::daemon_ipc::{current_unix_ts, load_daemon_status, write_daemon_command, DaemonCommand};
use crate::locale::{t, t_fmt};
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

    // Helper: try to create the lock file, writing our PID into it.
    let try_create = |p: &std::path::PathBuf| -> bool {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(p)
        {
            let _ = write!(f, "{}", std::process::id());
            true
        } else {
            false
        }
    };

    if try_create(&path) {
        return Some(UiLock { path });
    }

    // Read the PID from the existing lock file and check if that process is
    // still running. If the process is gone, the lock is stale.
    let stale = if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            !pid_is_running(pid)
        } else {
            // Unreadable/empty lock - treat as stale.
            true
        }
    } else {
        // Can't read the file - treat as stale.
        true
    };

    if stale {
        let _ = std::fs::remove_file(&path);
        if try_create(&path) {
            return Some(UiLock { path });
        }
    }

    None
}

/// Returns true if a process with the given PID is currently running.
#[cfg(target_os = "linux")]
fn pid_is_running(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(target_os = "windows")]
fn pid_is_running(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn pid_is_running(_pid: u32) -> bool {
    false // Treat as stale on unknown platforms so locks don't stick forever.
}

// --------------------------------------------------------------------------
// Renderer helper
// --------------------------------------------------------------------------

/// Run an eframe window using the glow (OpenGL) renderer.
fn run_native_with_fallback(
    title: &str,
    mut options: eframe::NativeOptions,
    make_app: impl Fn() -> Box<dyn eframe::App> + 'static,
) {
    let visuals = platform_visuals();
    options.renderer = eframe::Renderer::Glow;
    if let Err(e) = eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(visuals);
            crate::locale::apply_cjk_font_if_needed(&cc.egui_ctx);
            Ok(make_app())
        }),
    ) {
        log::error!("renderer failed: {e}");
    }
}

// --------------------------------------------------------------------------
// Platform theming
// --------------------------------------------------------------------------

/// Returns egui visuals appropriate for the current platform and system theme.
/// On Windows, reads the dark/light mode preference from the registry and
/// applies Windows 11-style colors and rounding. On other platforms returns
/// the default dark theme.
fn platform_visuals() -> egui::Visuals {
    #[cfg(target_os = "windows")]
    {
        if windows_is_dark_mode() {
            windows_dark_visuals()
        } else {
            windows_light_visuals()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        egui::Visuals::dark()
    }
}

#[cfg(target_os = "windows")]
fn windows_is_dark_mode() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v: u32| v == 0)
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn windows_dark_visuals() -> egui::Visuals {
    use egui::{Color32, CornerRadius, Stroke, Visuals};
    let accent = Color32::from_rgb(0, 120, 212); // #0078D4 Windows blue
    let mut v = Visuals::dark();
    v.window_corner_radius = CornerRadius::same(8);
    v.window_fill = Color32::from_rgb(32, 32, 32);
    v.panel_fill = Color32::from_rgb(45, 45, 45);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(60, 60, 60));
    v.selection.bg_fill = accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = Color32::from_rgb(77, 166, 255);
    let r = CornerRadius::same(4);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(62, 62, 62);
    v.widgets.inactive.bg_fill = Color32::from_rgb(62, 62, 62);
    v.widgets.hovered.corner_radius = r;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(75, 75, 75);
    v.widgets.hovered.bg_fill = Color32::from_rgb(75, 75, 75);
    v.widgets.active.corner_radius = r;
    v.widgets.active.weak_bg_fill = Color32::from_rgb(90, 90, 90);
    v.widgets.active.bg_fill = Color32::from_rgb(90, 90, 90);
    v.widgets.open.corner_radius = r;
    v
}

#[cfg(target_os = "windows")]
fn windows_light_visuals() -> egui::Visuals {
    use egui::{Color32, CornerRadius, Stroke, Visuals};
    let accent = Color32::from_rgb(0, 120, 212);
    let mut v = Visuals::light();
    v.window_corner_radius = CornerRadius::same(8);
    v.window_fill = Color32::from_rgb(243, 243, 243);
    v.panel_fill = Color32::from_rgb(255, 255, 255);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(200, 200, 200));
    v.selection.bg_fill = accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = accent;
    let r = CornerRadius::same(4);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(230, 230, 230);
    v.widgets.inactive.bg_fill = Color32::from_rgb(230, 230, 230);
    v.widgets.hovered.corner_radius = r;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(214, 214, 214);
    v.widgets.hovered.bg_fill = Color32::from_rgb(214, 214, 214);
    v.widgets.active.corner_radius = r;
    v.widgets.active.weak_bg_fill = Color32::from_rgb(200, 200, 200);
    v.widgets.active.bg_fill = Color32::from_rgb(200, 200, 200);
    v.widgets.open.corner_radius = r;
    v
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

    let title = if first_run {
        t("settings.title_setup")
    } else {
        t("settings.title_settings")
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([340.0, 360.0])
            .with_min_inner_size([300.0, 150.0])
            .with_resizable(false),
        ..Default::default()
    };

    run_native_with_fallback(&title, options, move || {
        Box::new(SettingsWindow::new(first_run))
    });
}

/// About dialog.
pub fn run_about_window() {
    let _lock = match acquire_ui_lock("about") {
        Some(l) => l,
        None => return,
    };

    let title = format!("DragonFoxVPN - {}", t("tray.about").trim_end_matches('.'));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([360.0, 480.0])
            .with_resizable(false),
        ..Default::default()
    };

    run_native_with_fallback(&title, options, || Box::new(AboutWindow::new()));
}

/// Status dashboard dialog.
pub fn run_status_window() {
    let _lock = match acquire_ui_lock("status") {
        Some(l) => l,
        None => return,
    };

    let title = t("status_win.title");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([360.0, 220.0])
            .with_min_inner_size([300.0, 80.0])
            .with_resizable(false),
        ..Default::default()
    };

    run_native_with_fallback(&title, options, || Box::new(StatusWindow::new()));
}

/// Location picker dialog.
pub fn run_location_window() {
    let _lock = match acquire_ui_lock("location") {
        Some(l) => l,
        None => return,
    };

    let title = t("location_win.title");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(&title)
            .with_inner_size([400.0, 550.0])
            .with_resizable(true),
        ..Default::default()
    };

    run_native_with_fallback(&title, options, || Box::new(LocationWindow::new()));
}

// --------------------------------------------------------------------------
// UI helpers
// --------------------------------------------------------------------------

/// Draws a coloured section label followed by a full-width separator rule.
/// Produces a clean visual divider that groups related settings.
fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.add_space(6.0);
    if label.is_empty() {
        ui.separator();
    } else {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(0x00, 0x7A, 0xCC), label);
            ui.separator();
        });
    }
    ui.add_space(2.0);
}

// --------------------------------------------------------------------------
// Settings window
// --------------------------------------------------------------------------

struct SettingsWindow {
    first_run: bool,
    vpn_gateway: IpInput,
    isp_gateway: IpInput,
    switcher_url: String,
    message: Option<String>,
    saved: bool,
    // Behavior toggles (moved from tray menu)
    auto_connect: bool,
    auto_reconnect: bool,
    #[cfg(target_os = "windows")]
    run_on_startup: bool,
    // Language selection
    language: String, // locale code, e.g. "en", "de"; empty = system auto
    // Auto-resolve VPN server IP from the switcher URL.
    last_resolved_url: String,
    resolving: bool,
    resolve_rx: Option<std::sync::mpsc::Receiver<Option<String>>>,
    // Test Connection
    testing: bool,
    test_rx: Option<std::sync::mpsc::Receiver<Vec<(String, bool)>>>,
    test_results: Vec<(String, bool)>,
    last_height: f32,
}

impl SettingsWindow {
    fn new(first_run: bool) -> Self {
        let cfg = AppConfig::load();
        Self {
            first_run,
            vpn_gateway: IpInput::from_str(cfg.vpn_gateway.as_deref().unwrap_or("")),
            isp_gateway: IpInput::from_str(cfg.isp_gateway.as_deref().unwrap_or("")),
            switcher_url: cfg.switcher_url.clone().unwrap_or_default(),
            message: None,
            saved: false,
            auto_connect: cfg.auto_connect,
            auto_reconnect: cfg.auto_reconnect,
            #[cfg(target_os = "windows")]
            run_on_startup: crate::autostart::AutoStartManager::is_enabled(),
            language: cfg
                .language
                .clone()
                .unwrap_or_else(|| crate::locale::active_language().to_string()),
            last_resolved_url: cfg.switcher_url.unwrap_or_default(),
            resolving: false,
            resolve_rx: None,
            testing: false,
            test_rx: None,
            test_results: Vec::new(),
            last_height: 0.0,
        }
    }
}

impl eframe::App for SettingsWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // On first run, closing without saving exits the whole application.
        if ctx.input(|i| i.viewport().close_requested()) && self.first_run && !self.saved {
            write_daemon_command(DaemonCommand::Quit);
        }

        // Trigger VPN IP auto-resolve when the switcher URL changes.
        let url_valid =
            self.switcher_url.starts_with("http://") || self.switcher_url.starts_with("https://");
        if url_valid && self.switcher_url != self.last_resolved_url && !self.resolving {
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let url = self.switcher_url.clone();
            std::thread::spawn(move || {
                let _ = tx.send(resolve_host_from_url(&url));
            });
            self.resolve_rx = Some(rx);
            self.resolving = true;
            self.last_resolved_url = self.switcher_url.clone();
        }
        // Drain resolve result.
        if let Some(ref rx) = self.resolve_rx {
            if let Ok(result) = rx.try_recv() {
                if let Some(ip) = result {
                    self.vpn_gateway = IpInput::from_str(&ip);
                }
                self.resolving = false;
                self.resolve_rx = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        // Drain test result.
        if let Some(ref rx) = self.test_rx {
            if let Ok(results) = rx.try_recv() {
                self.test_results = results;
                self.testing = false;
                self.test_rx = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        let panel = egui::Panel::top("settings_content").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                ui.heading(if self.first_run {
                    t("settings.heading_setup")
                } else {
                    t("settings.heading_settings")
                });
                if self.first_run {
                    ui.colored_label(egui::Color32::GRAY, t("settings.subtitle_setup"));
                }
            });

            // ── Network ──────────────────────────────────────────────
            section_header(ui, &t("settings.section_network"));
            egui::Grid::new("settings_grid")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.colored_label(egui::Color32::GRAY, t("settings.field_url"));
                    ui.add(egui::TextEdit::singleline(&mut self.switcher_url).desired_width(175.0));
                    ui.end_row();
                    ui.colored_label(egui::Color32::GRAY, t("settings.field_vpn_ip"));
                    ui.horizontal(|ui| {
                        self.vpn_gateway.show(ui, "vpn_gw");
                        if self.resolving {
                            ui.spinner();
                        }
                    });
                    ui.end_row();
                    ui.colored_label(egui::Color32::GRAY, t("settings.field_router_ip"));
                    self.isp_gateway.show(ui, "isp_gw");
                    ui.end_row();
                });

            // ── Behavior ─────────────────────────────────────────────
            section_header(ui, &t("settings.section_behavior"));
            ui.checkbox(&mut self.auto_connect, t("tray.autoconnect"));
            ui.checkbox(&mut self.auto_reconnect, t("tray.autoreconnect"));
            #[cfg(target_os = "windows")]
            ui.checkbox(&mut self.run_on_startup, t("tray.run_on_startup"));

            // ── Language ─────────────────────────────────────────────
            section_header(ui, &t("settings.section_language"));
            let current_name = crate::locale::available_languages()
                .iter()
                .find(|(c, _)| *c == self.language.as_str())
                .map(|(_, n)| *n)
                .unwrap_or("English");
            egui::ComboBox::from_id_salt("lang_select")
                .selected_text(current_name)
                .width(200.0)
                .show_ui(ui, |ui| {
                    for (code, name) in crate::locale::available_languages() {
                        ui.selectable_value(&mut self.language, code.to_string(), *name);
                    }
                });

            // ── Test results (when available) ─────────────────────────
            if !self.test_results.is_empty() {
                section_header(ui, "");
                for (label, ok) in &self.test_results {
                    let color = if *ok {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::LIGHT_RED
                    };
                    let prefix = if *ok { "✓" } else { "✗" };
                    ui.colored_label(color, format!("{prefix}  {label}"));
                }
            }

            if let Some(ref msg) = self.message.clone() {
                ui.add_space(4.0);
                let color = if self.saved {
                    egui::Color32::LIGHT_GREEN
                } else {
                    egui::Color32::LIGHT_RED
                };
                ui.colored_label(color, msg);
            }

            ui.add_space(8.0);
            let busy = self.testing || self.resolving;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new(t("settings.btn_test")))
                    .clicked()
                {
                    self.start_test();
                }
                if self.testing {
                    ui.spinner();
                }
                if ui.button(t("settings.btn_save")).clicked() {
                    self.try_save(&ctx);
                }
            });
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show_inside(ui, |_| {});
        let h = panel.response.rect.height();
        if (h - self.last_height).abs() > 1.0 {
            self.last_height = h;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(340.0, h)));
        }
    }
}

impl SettingsWindow {
    fn start_test(&mut self) {
        let url = self.switcher_url.trim().trim_end_matches('/').to_string();
        // Update the field so the user sees the normalised URL.
        self.switcher_url = url.clone();
        let vpn_ip = self.vpn_gateway.to_ip_string();
        let router_ip = self.isp_gateway.to_ip_string();

        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(run_connection_test(url, vpn_ip, router_ip));
        });
        self.test_rx = Some(rx);
        self.test_results.clear();
        self.testing = true;
    }

    fn try_save(&mut self, ctx: &egui::Context) {
        let url = self.switcher_url.trim().trim_end_matches('/').to_string();

        let mut cfg = AppConfig::load();
        let language_changed = cfg.language.as_deref() != Some(self.language.as_str());

        let ip_valid = self.vpn_gateway.is_valid() && self.isp_gateway.is_valid();
        let url_valid =
            !url.is_empty() && (url.starts_with("http://") || url.starts_with("https://"));

        // If network fields are incomplete but the language changed, save just
        // the language and restart so the user can continue setup in their language.
        // setup_complete is intentionally left untouched so the setup dialog reappears.
        if !ip_valid || !url_valid {
            if language_changed {
                cfg.language = Some(self.language.clone());
                cfg.save();
                write_daemon_command(DaemonCommand::Restart);
                self.saved = true; // prevent close handler from blocking the restart
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.message = Some(if !ip_valid {
                t("settings.msg_invalid_ip")
            } else {
                t("settings.msg_invalid_url")
            });
            self.saved = false;
            return;
        }

        let vpn_gw = self.vpn_gateway.to_ip_string();
        let isp_gw = self.isp_gateway.to_ip_string();

        cfg.vpn_gateway = Some(vpn_gw.clone());
        cfg.isp_gateway = Some(isp_gw);
        cfg.dns_server = Some(vpn_gw); // DNS is always the VPN server IP
        cfg.switcher_url = Some(url);
        cfg.setup_complete = true;
        cfg.auto_connect = self.auto_connect;
        cfg.auto_reconnect = self.auto_reconnect;
        cfg.language = Some(self.language.clone());
        cfg.save();

        // Apply run-on-startup (Windows registry, not stored in config).
        #[cfg(target_os = "windows")]
        crate::autostart::AutoStartManager::set_autostart(self.run_on_startup);

        // Language change requires a full restart to re-initialise the locale.
        // Other changes only need a config reload.
        if language_changed {
            write_daemon_command(DaemonCommand::Restart);
        } else {
            write_daemon_command(DaemonCommand::ReloadConfig);
        }

        let adapter = SystemHandler::get_active_adapter();
        info!("Settings saved. Active adapter: {adapter}");
        self.message = Some(t("settings.msg_saved"));
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0x00, 0x7A, 0xCC),
                    egui::RichText::new("DragonFox VPN").size(24.0).strong(),
                );
            });
            ui.add_space(12.0);

            if let Some(status) = load_daemon_status() {
                let (state_color, state_text) = match status.state.as_str() {
                    "Connected" => (egui::Color32::LIGHT_GREEN, t("status_win.state_connected")),
                    "Enabling" => (
                        egui::Color32::from_rgb(0x00, 0x7A, 0xCC),
                        t("status_win.state_connecting"),
                    ),
                    "Dropped" => (egui::Color32::LIGHT_RED, t("status_win.state_dropped")),
                    "ServerUnreachable" => (egui::Color32::GRAY, t("status_win.state_unreachable")),
                    "SetupIncomplete" => {
                        (egui::Color32::GRAY, t("status_win.state_setup_incomplete"))
                    }
                    _ => (egui::Color32::YELLOW, t("status_win.state_disabled")),
                };

                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(0x25, 0x25, 0x26))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.colored_label(
                                state_color,
                                egui::RichText::new(state_text).size(18.0).strong(),
                            );
                        });
                    });

                ui.add_space(8.0);
                ui.label(t_fmt("status_win.location", &[("value", &status.location)]));
                ui.label(t_fmt(
                    "status_win.gateway",
                    &[("value", status.vpn_gateway.as_deref().unwrap_or("N/A"))],
                ));
                ui.label(t_fmt("status_win.adapter", &[("value", &status.adapter)]));

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
                ui.label(t_fmt("status_win.duration", &[("value", &duration_str)]));

                if let Some(ref msg) = status.message {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::GRAY, msg);
                }
            } else {
                ui.colored_label(egui::Color32::YELLOW, t("status_win.waiting"));
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
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
                        self.upload_flag_texture(&code, &bytes, &ctx);
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
                        self.switch_status = Some(t_fmt(
                            "location_win.switch_failed",
                            &[("location", &confirmed_label)],
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
                    self.switch_status = Some(t_fmt("location_win.switch_error", &[("msg", &e)]));
                    self.switch_ok = false;
                    self.is_switching = false;
                }
            }
        }

        // Heading, search bar, and status always visible at the top.
        egui::Panel::top("loc_top").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.vertical_centered(|ui| {
                ui.heading(t("location_win.heading"));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search_text)
                        .desired_width(f32::INFINITY)
                        .hint_text(t("location_win.search_hint")),
                );
                if self.is_loading || self.is_switching {
                    ui.spinner();
                }
            });
            if self.is_switching {
                ui.label(t_fmt(
                    "location_win.switching",
                    &[("location", self.selected_label.as_deref().unwrap_or("…"))],
                ));
            } else if let Some(ref msg) = self.switch_status.clone() {
                let color = if self.switch_ok {
                    egui::Color32::LIGHT_GREEN
                } else {
                    egui::Color32::LIGHT_RED
                };
                ui.colored_label(color, msg);
            }
            ui.add_space(2.0);
        });

        // List fills all remaining space.
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.is_loading {
                ui.add_space(12.0);
                ui.label(t("location_win.loading"));
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

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut last_section: Option<String> = None;
                    for loc in &sorted {
                        if !lower_search.is_empty()
                            && !loc.label.to_lowercase().contains(&lower_search)
                        {
                            continue;
                        }

                        let is_fav = favorites.contains(&loc.label);
                        let section = if is_fav {
                            t("location_win.favorites")
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
                    self.ensure_flag(&code, &ctx);
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
    match ureq::get(&url).call() {
        Ok(mut resp) => {
            if let Ok(bytes) = resp.body_mut().read_to_vec() {
                if !bytes.is_empty() {
                    let _ = std::fs::write(&path, &bytes);
                    return Some(bytes);
                }
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
// IP address segmented input widget
// --------------------------------------------------------------------------

struct IpInput {
    octets: [String; 4],
}

impl IpInput {
    fn from_str(ip: &str) -> Self {
        let mut parts = ip.splitn(4, '.');
        Self {
            octets: std::array::from_fn(|_| parts.next().unwrap_or("").to_string()),
        }
    }

    fn to_ip_string(&self) -> String {
        format!(
            "{}.{}.{}.{}",
            self.octets[0], self.octets[1], self.octets[2], self.octets[3]
        )
    }

    fn is_valid(&self) -> bool {
        self.octets
            .iter()
            .all(|o| !o.is_empty() && o.parse::<u8>().is_ok())
    }

    /// Render four narrow octet fields separated by dots.
    /// Auto-advances focus to the next field when 3 digits are entered or a
    /// dot is typed (matching the Windows IP-address common control behaviour).
    fn show(&mut self, ui: &mut egui::Ui, id_salt: &str) {
        let base_id = egui::Id::new(id_salt);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;

            // Collect which field should receive focus after all fields are
            // rendered. Advancing focus mid-loop causes the dot event to
            // propagate into the next field's TextEdit in the same frame,
            // which then advances again - skipping all the way to the last box.
            let mut focus_next: Option<usize> = None;

            for i in 0..4usize {
                let field_id = base_id.with(i);
                let prev_len = self.octets[i].len();

                // Snapshot focus state BEFORE rendering so subsequent fields
                // that haven't been focused yet don't falsely trigger.
                let had_focus = ui.memory(|m| m.has_focus(field_id));

                ui.add(
                    egui::TextEdit::singleline(&mut self.octets[i])
                        .id(field_id)
                        .desired_width(30.0)
                        .char_limit(4),
                );

                // If the user typed a '.' advance to the next octet.
                // Gate on had_focus so only the active field triggers this.
                let has_dot = had_focus && self.octets[i].contains('.');
                // Keep only digits and cap at 3 chars.
                self.octets[i].retain(|c| c.is_ascii_digit());
                if self.octets[i].len() > 3 {
                    self.octets[i].truncate(3);
                }

                // Auto-advance when the field just became 3 digits, or on dot.
                let just_filled = self.octets[i].len() == 3 && prev_len < 3;
                if (just_filled || has_dot) && i < 3 && focus_next.is_none() {
                    focus_next = Some(i + 1);
                }

                if i < 3 {
                    ui.label(".");
                }
            }

            // Apply the focus change after all fields are rendered.
            if let Some(next) = focus_next {
                ui.memory_mut(|m| m.request_focus(base_id.with(next)));
            }
        });
    }
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

/// Run the three connection checks for the Settings "Test Connection" button.
/// Blocking - intended to be called from a background thread.
fn simplify_network_error(e: &str) -> String {
    let lower = e.to_lowercase();
    if lower.contains("timed out") || lower.contains("10060") || lower.contains("timeout") {
        "connection timed out".to_string()
    } else if lower.contains("refused") || lower.contains("10061") {
        "connection refused".to_string()
    } else if lower.contains("dns") || lower.contains("resolve") || lower.contains("no such host") {
        "hostname could not be resolved".to_string()
    } else if lower.contains("certificate") || lower.contains("ssl") || lower.contains("tls") {
        "SSL/TLS certificate error".to_string()
    } else if lower.contains("network") || lower.contains("unreachable") {
        "network unreachable".to_string()
    } else {
        "could not connect".to_string()
    }
}

fn run_connection_test(url: String, vpn_ip: String, router_ip: String) -> Vec<(String, bool)> {
    let mut results = Vec::new();

    // 1. Switcher URL - fetch locations to confirm the page is the real switcher.
    if url.starts_with("http://") || url.starts_with("https://") {
        match VpnApi::fetch_locations(&url) {
            Ok((locs, _)) if !locs.is_empty() => {
                results.push((
                    t_fmt("test.url_ok", &[("count", &locs.len().to_string())]),
                    true,
                ));
            }
            Ok(_) => {
                results.push((t("test.url_wrong_page"), false));
            }
            Err(e) => {
                let msg = simplify_network_error(&e);
                results.push((t_fmt("test.url_error", &[("msg", &msg)]), false));
            }
        }
    } else {
        results.push((t("test.url_not_set"), false));
    }

    // 2. VPN Server IP - ping.
    if vpn_ip.split('.').count() == 4 && !vpn_ip.starts_with('.') {
        let ok = SystemHandler::ping_host(&vpn_ip);
        let status_str = if ok {
            t("test.reachable")
        } else {
            t("test.unreachable")
        };
        results.push((
            t_fmt(
                "test.vpn_server",
                &[("ip", &vpn_ip), ("status", &status_str)],
            ),
            ok,
        ));
    } else {
        results.push((t("test.vpn_server_not_set"), false));
    }

    // 3. Router IP - ping.
    if router_ip.split('.').count() == 4 && !router_ip.starts_with('.') {
        let ok = SystemHandler::ping_host(&router_ip);
        let status_str = if ok {
            t("test.reachable")
        } else {
            t("test.unreachable")
        };
        results.push((
            t_fmt(
                "test.router",
                &[("ip", &router_ip), ("status", &status_str)],
            ),
            ok,
        ));
    } else {
        results.push((t("test.router_not_set"), false));
    }

    results
}

/// Resolve the hostname in a URL to an IP address string.
/// Returns None if the URL is malformed or DNS lookup fails.
fn resolve_host_from_url(url: &str) -> Option<String> {
    use std::net::ToSocketAddrs;
    // Strip scheme.
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    // Take only the host[:port] part.
    let host_part = rest.split('/').next()?;
    // Remove any explicit port for the lookup key, then add :80 for ToSocketAddrs.
    let host = host_part.split(':').next()?;
    let addr_str = format!("{host}:80");
    let mut addrs = addr_str.to_socket_addrs().ok()?;
    let ip = addrs.next()?.ip().to_string();
    Some(ip)
}

// --------------------------------------------------------------------------
// About window
// --------------------------------------------------------------------------

struct AboutWindow {
    logo: Option<egui::TextureHandle>,
}

impl AboutWindow {
    fn new() -> Self {
        Self { logo: None }
    }
}

const GITHUB_URL: &str = "https://github.com/sponsors/senjinthedragon/";
const KOFI_URL: &str = "https://ko-fi.com/senjinthedragon";
const BITCOIN_ADDRESS: &str = "bc1qjsaqw6rjcmhv6ywv2a97wfd4zxnae3ncrn8mf9";

impl eframe::App for AboutWindow {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Load and cache the logo texture on the first frame.
        if self.logo.is_none() {
            let bytes = include_bytes!("../assets/senjin_logo.png");
            if let Ok(img) = image::load_from_memory(bytes) {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                self.logo =
                    Some(ctx.load_texture("about_logo", color_image, egui::TextureOptions::LINEAR));
            }
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);

                    // Logo
                    if let Some(tex) = &self.logo {
                        ui.add(egui::Image::new((tex.id(), egui::vec2(119.0, 128.0))));
                        ui.add_space(10.0);
                    }

                    // Title and version
                    ui.colored_label(
                        egui::Color32::from_rgb(0x00, 0x7A, 0xCC),
                        egui::RichText::new("DragonFoxVPN").size(26.0).strong(),
                    );
                    ui.label(
                        egui::RichText::new(concat!("Version ", env!("CARGO_PKG_VERSION")))
                            .size(13.0)
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(8.0);

                    ui.label("© 2026 Senjin the Dragon");
                    ui.label(
                        egui::RichText::new("Released under the MIT License")
                            .color(egui::Color32::GRAY),
                    );

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Support links
                    ui.label(egui::RichText::new("Support Development").strong());
                    ui.add_space(6.0);
                    ui.add(egui::Hyperlink::from_label_and_url(
                        "⭐  GitHub Sponsors",
                        GITHUB_URL,
                    ));
                    ui.add_space(4.0);
                    ui.add(egui::Hyperlink::from_label_and_url("☕  Ko-fi", KOFI_URL));

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Bitcoin donation
                    ui.label(egui::RichText::new("Bitcoin (BTC)").strong());
                    ui.add_space(4.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(BITCOIN_ADDRESS).monospace().size(11.0),
                        )
                        .selectable(true),
                    );
                    ui.add_space(4.0);
                    if ui.small_button("Copy address").clicked() {
                        ctx.copy_text(BITCOIN_ADDRESS.to_owned());
                    }

                    ui.add_space(12.0);
                });
            });
        });
    }
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
