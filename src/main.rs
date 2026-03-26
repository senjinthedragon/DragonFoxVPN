// main.rs - DragonFoxVPN: Tray daemon entry point and UI subprocess dispatcher
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Architecture: a single long-running tray daemon process owns the GTK event
// loop, the tray icon, the VPN state machine, and the health-check thread.
// Dialog windows (Settings, Status, Location) are launched as independent
// subprocesses via `--ui <mode>`. Each subprocess runs its own eframe event
// loop in isolation - no shared Wayland connection, no GTK - so the OS close
// button works reliably on every platform and compositor.
//
// IPC uses two JSON files in the config directory:
//   daemon_status.json  - daemon writes, UI subprocesses read
//   daemon_command.json - UI subprocesses write, daemon reads and acts on

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::time::{Duration, Instant};

use log::{error, info, warn};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use dragonfox_vpn::config::AppConfig;
use dragonfox_vpn::locale::{t, t_fmt};
use dragonfox_vpn::daemon_ipc::{
    clear_daemon_command, current_unix_ts, save_daemon_status, take_daemon_command, DaemonCommand,
    DaemonStatus,
};
use dragonfox_vpn::icons::{
    create_status_icon_rgba, COLOR_BLUE, COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_YELLOW,
};
use dragonfox_vpn::state::VpnState;
use dragonfox_vpn::system::SystemHandler;
use dragonfox_vpn::vpn_runtime;

// --------------------------------------------------------------------------
// Emergency VPN cleanup (used by signal handler and panic hook)
// --------------------------------------------------------------------------

/// Stores (adapter, vpn_gateway) while the VPN is active so that signal
/// handlers and panic hooks can restore normal routing without access to
/// the daemon's local state.
static VPN_ACTIVE: std::sync::OnceLock<std::sync::Mutex<Option<(String, String)>>> =
    std::sync::OnceLock::new();

fn vpn_active_lock() -> &'static std::sync::Mutex<Option<(String, String)>> {
    VPN_ACTIVE.get_or_init(|| std::sync::Mutex::new(None))
}

fn set_vpn_active(adapter: &str, vpn_gateway: &str) {
    if let Ok(mut g) = vpn_active_lock().lock() {
        *g = Some((adapter.to_string(), vpn_gateway.to_string()));
    }
}

fn clear_vpn_active() {
    if let Ok(mut g) = vpn_active_lock().lock() {
        *g = None;
    }
}

/// Restore normal routing if the VPN is active. Safe to call from a signal
/// handler or panic hook - reads from the static and issues OS commands only.
fn emergency_vpn_restore() {
    if let Ok(g) = vpn_active_lock().lock() {
        if let Some((adapter, vpn_gateway)) = g.as_ref() {
            dragonfox_vpn::vpn_runtime::disable_vpn(adapter, vpn_gateway);
        }
    }
}

// --------------------------------------------------------------------------
// Entry point
// --------------------------------------------------------------------------

fn main() {
    let mut logger = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    logger.format_timestamp_secs();
    // On Windows the console is suppressed, so also write logs to a file
    // next to the executable so they can be inspected for debugging.
    #[cfg(target_os = "windows")]
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let log_path = dir.join("dragonfoxvpn.log");
            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true).append(true).open(&log_path)
            {
                logger.target(env_logger::Target::Pipe(Box::new(file)));
            }
        }
    }
    logger.init();

    // Initialise localisation before any user-visible strings are produced.
    dragonfox_vpn::locale::init();

    // Restore normal routing on SIGINT / SIGTERM (e.g. system shutdown or kill).
    ctrlc::set_handler(|| {
        emergency_vpn_restore();
        std::process::exit(0);
    })
    .unwrap_or_else(|e| warn!("Failed to register signal handler: {e}"));

    // Restore normal routing on panic before the process unwinds.
    std::panic::set_hook(Box::new(|info| {
        emergency_vpn_restore();
        error!("Panic: {info}");
    }));

    // UI subprocess mode: launched by the tray daemon for each dialog window.
    // GTK is NOT initialised here - each subprocess has a clean eframe event
    // loop with no competing Wayland connections, which is why close works.
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--ui" {
        match args[2].as_str() {
            "settings" => dragonfox_vpn::app::run_settings_window(),
            "status" => dragonfox_vpn::app::run_status_window(),
            "location" => dragonfox_vpn::app::run_location_window(),
            "about" => dragonfox_vpn::app::run_about_window(),
            _ => {}
        }
        return;
    }

    // Tray daemon path.
    #[cfg(target_os = "linux")]
    {
        if !gtk::is_initialized_main_thread() {
            gtk::init().unwrap_or_else(|e| {
                eprintln!("Failed to initialise GTK: {e}");
                std::process::exit(1);
            });
        }
    }

    if !dragonfox_vpn::single_instance_check() {
        warn!("Another instance of DragonFoxVPN is already running. Exiting.");
        return;
    }

    run_tray_daemon();
}

// --------------------------------------------------------------------------
// Tray daemon
// --------------------------------------------------------------------------

fn run_tray_daemon() {
    let mut config = AppConfig::load();
    let adapter = SystemHandler::get_active_adapter();
    info!("Active adapter: {adapter}");

    let setup_complete = config.setup_complete;

    // Persist initial status so UI subprocesses can display something
    // immediately even before any VPN operation has occurred.
    let mut daemon_status = DaemonStatus {
        state: if setup_complete {
            "Disabled".to_string()
        } else {
            "SetupIncomplete".to_string()
        },
        adapter: adapter.clone(),
        location: config
            .last_location
            .clone()
            .unwrap_or_else(|| "Unknown".to_string()),
        vpn_gateway: config.vpn_gateway.clone(),
        connected_since_unix: None,
        message: None,
        updated_unix: current_unix_ts(),
    };
    clear_daemon_command();
    save_daemon_status(&daemon_status);

    // Build tray icon and menu.
    let (tray, items) = build_tray(&config);

    // Health-check notification channel: background thread → main loop.
    let (hc_tx, hc_rx) = std::sync::mpsc::channel::<HcEvent>();
    {
        let cfg_path = dragonfox_vpn::config::get_config_path();
        let tx = hc_tx.clone();
        std::thread::spawn(move || health_check_loop(cfg_path, tx));
    }

    // Fetch the actual current location from the backend on startup so the
    // status reflects what the Pi is set to, not just what the app last saved.
    if let Some(url) = config.switcher_url.clone() {
        let tx = hc_tx.clone();
        std::thread::spawn(move || {
            if let Ok((_, Some(label))) = dragonfox_vpn::api::VpnApi::fetch_locations(&url) {
                let _ = tx.send(HcEvent::LocationFetched(label));
            }
        });
    }

    // Track local VPN state for the daemon loop.
    let mut vpn_state = VpnState::Disabled;
    let mut connected_since: Option<Instant> = None;
    update_tray_icon(&tray, &items, &vpn_state, None);

    // Auto-connect on startup if configured; otherwise ensure routing is in
    // the non-VPN state. This recovers from a previous crash (SIGSEGV, etc.)
    // that left VPN routing active, or any other situation where the routes
    // are in an unexpected state.
    if setup_complete && config.auto_connect {
        set_vpn_enabling(&tray, &items, &mut vpn_state, &mut daemon_status);
        if do_enable_vpn(&adapter, &config) {
            set_vpn_connected(
                &tray,
                &items,
                &mut vpn_state,
                &mut connected_since,
                &mut daemon_status,
                &config,
                None,
            );
        } else {
            set_vpn_failed(&tray, &items, &mut vpn_state, &mut daemon_status);
        }
    } else if setup_complete {
        do_disable_vpn(&adapter, &config);
    }

    // Open settings on first run.
    if !setup_complete {
        spawn_ui("settings");
    }

    // -----------------------------------------------------------------------
    // Main tray event loop
    // -----------------------------------------------------------------------
    let mut dialog_was_open = false;

    loop {
        // Service the platform event queue so the tray icon stays responsive.
        #[cfg(target_os = "linux")]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }
        // On Windows, pump the Win32 message queue so tray and menu events
        // are dispatched. Without this the tray icon renders but all input
        // is ignored.
        #[cfg(target_os = "windows")]
        {
            use std::ffi::c_void;
            #[repr(C)]
            struct Msg {
                hwnd: *mut c_void, message: u32, w_param: usize,
                l_param: isize, time: u32, pt_x: i32, pt_y: i32, _private: u32,
            }
            extern "system" {
                fn PeekMessageW(lp: *mut Msg, hwnd: *mut c_void, min: u32, max: u32, remove: u32) -> i32;
                fn TranslateMessage(lp: *const Msg) -> i32;
                fn DispatchMessageW(lp: *const Msg) -> isize;
            }
            unsafe {
                let mut msg: Msg = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 1) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        // Modal behaviour: while a dialog is open, replace the tray menu with
        // a locked placeholder so right-click shows nothing actionable.
        // libappindicator on Linux always shows a menu on right-click and
        // cannot suppress it, so set_menu(None) has no effect - swapping to a
        // disabled placeholder is the only reliable way to block interaction.
        let dialog_open = any_ui_open();
        if dialog_open != dialog_was_open {
            if dialog_open {
                items.dashboard.set_enabled(false);
                items.toggle.set_enabled(false);
                items.location.set_enabled(false);
                items.settings.set_enabled(false);
                items.about.set_enabled(false);
                items.exit.set_enabled(false);
            } else {
                // Restore each item to its correct state.
                items.dashboard.set_enabled(true);
                items.settings.set_enabled(true);
                items.about.set_enabled(true);
                items.exit.set_enabled(true);
                let setup_done = config.setup_complete;
                items.toggle.set_text(&if vpn_state == VpnState::Connected {
                    t("tray.disable_vpn")
                } else {
                    t("tray.enable_vpn")
                });
                items.toggle.set_enabled(setup_done && vpn_state != VpnState::Enabling);
                items.location.set_enabled(setup_done);
            }
            dialog_was_open = dialog_open;
        }

        // Process health-check events from background thread.
        while let Ok(ev) = hc_rx.try_recv() {
            handle_hc_event(
                ev,
                &tray,
                &items,
                &adapter,
                &config,
                &mut vpn_state,
                &mut connected_since,
                &mut daemon_status,
            );
        }

        // Update connected_since timestamp in status ~every second.
        if vpn_state == VpnState::Connected {
            let ts = connected_since
                .map(|s| current_unix_ts() - s.elapsed().as_secs())
                .or(daemon_status.connected_since_unix);
            if daemon_status.connected_since_unix != ts {
                daemon_status.connected_since_unix = ts;
                save_daemon_status(&daemon_status);
            }
        }

        // Process daemon commands from UI subprocesses.
        while let Some(cmd) = take_daemon_command() {
            match cmd {
                DaemonCommand::ReloadConfig => {
                    config = AppConfig::load();
                    daemon_status.vpn_gateway = config.vpn_gateway.clone();
                    daemon_status.location = config
                        .last_location
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string());
                    let setup_now = config.setup_complete;
                    if setup_now {
                        items.toggle.set_text(&if vpn_state == VpnState::Connected {
                            t("tray.disable_vpn")
                        } else {
                            t("tray.enable_vpn")
                        });
                        items.toggle.set_enabled(
                            vpn_state != VpnState::Enabling,
                        );
                        items.location.set_enabled(true);
                        items.settings.set_enabled(true);
                        daemon_status.state = if vpn_state == VpnState::Disabled {
                            "Disabled".to_string()
                        } else {
                            daemon_status.state.clone()
                        };
                    }
                    // If location is still unknown and we now have a URL,
                    // fetch the current location from the backend.
                    if config.last_location.is_none() {
                        if let Some(url) = config.switcher_url.clone() {
                            let tx = hc_tx.clone();
                            std::thread::spawn(move || {
                                if let Ok((_, Some(label))) =
                                    dragonfox_vpn::api::VpnApi::fetch_locations(&url)
                                {
                                    let _ = tx.send(HcEvent::LocationFetched(label));
                                }
                            });
                        }
                    }
                    save_daemon_status(&daemon_status);
                    info!("Config reloaded from daemon command.");
                }
                DaemonCommand::Restart => {
                    info!("Restart requested by settings subprocess.");
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(exe).spawn();
                    }
                    std::process::exit(0);
                }
                DaemonCommand::Reconnect => {
                    info!("Reconnect requested by UI subprocess.");
                    if vpn_state == VpnState::Connected {
                        do_disable_vpn(&adapter, &config);
                    }
                    config = AppConfig::load();
                    set_vpn_enabling(&tray, &items, &mut vpn_state, &mut daemon_status);
                    if do_enable_vpn(&adapter, &config) {
                        set_vpn_connected(
                            &tray,
                            &items,
                            &mut vpn_state,
                            &mut connected_since,
                            &mut daemon_status,
                            &config,
                            Some(t("status.reconnected")),
                        );
                    } else {
                        set_vpn_failed(&tray, &items, &mut vpn_state, &mut daemon_status);
                    }
                }
            }
        }

        // Poll tray icon events (double-click opens status).
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if matches!(ev, TrayIconEvent::DoubleClick { .. }) {
                spawn_ui("status");
            }
        }

        // Poll menu events.
        let mut should_exit = false;
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            handle_menu_event(
                ev.id,
                &items,
                &tray,
                &adapter,
                &config,
                &mut vpn_state,
                &mut connected_since,
                &mut daemon_status,
                &mut should_exit,
            );
        }

        if should_exit {
            break;
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    // Clean up on exit.
    if vpn_state == VpnState::Connected {
        do_disable_vpn(&adapter, &config);
    }
    drop(tray);
}

// --------------------------------------------------------------------------
// VPN state helpers (update tray + daemon_status atomically)
// --------------------------------------------------------------------------

fn set_vpn_enabling(
    tray: &TrayIcon,
    items: &MenuItems,
    vpn_state: &mut VpnState,
    status: &mut DaemonStatus,
) {
    *vpn_state = VpnState::Enabling;
    items.toggle.set_text(&t("tray.enable_vpn"));
    items.toggle.set_enabled(false);
    update_tray_icon(tray, items, vpn_state, None);
    status.state = "Enabling".to_string();
    status.message = Some(t("status.connecting"));
    save_daemon_status(status);
}

fn set_vpn_connected(
    tray: &TrayIcon,
    items: &MenuItems,
    vpn_state: &mut VpnState,
    connected_since: &mut Option<Instant>,
    status: &mut DaemonStatus,
    config: &AppConfig,
    message: Option<String>,
) {
    *vpn_state = VpnState::Connected;
    *connected_since = Some(Instant::now());
    items.toggle.set_text(&t("tray.disable_vpn"));
    items.toggle.set_enabled(true);
    status.state = "Connected".to_string();
    status.connected_since_unix = Some(current_unix_ts());
    // Prefer saved config location; fall back to whatever was fetched at
    // startup (status.location) before defaulting to "Unknown".
    let existing = status.location.clone();
    status.location = config.last_location.clone().unwrap_or_else(|| {
        if !existing.is_empty() && existing != "Unknown" {
            existing
        } else {
            "Unknown".to_string() // internal sentinel; never shown raw
        }
    });
    status.message = message;
    update_tray_icon(tray, items, vpn_state, Some(&status.location));
    save_daemon_status(status);
}

fn set_vpn_disabled(
    tray: &TrayIcon,
    items: &MenuItems,
    vpn_state: &mut VpnState,
    connected_since: &mut Option<Instant>,
    status: &mut DaemonStatus,
) {
    *vpn_state = VpnState::Disabled;
    *connected_since = None;
    items.toggle.set_text(&t("tray.enable_vpn"));
    items.toggle.set_enabled(true);
    update_tray_icon(tray, items, vpn_state, None);
    status.state = "Disabled".to_string();
    status.connected_since_unix = None;
    status.message = None;
    save_daemon_status(status);
}

fn set_vpn_failed(
    tray: &TrayIcon,
    items: &MenuItems,
    vpn_state: &mut VpnState,
    status: &mut DaemonStatus,
) {
    *vpn_state = VpnState::Disabled;
    items.toggle.set_text(&t("tray.enable_vpn"));
    items.toggle.set_enabled(true);
    update_tray_icon(tray, items, vpn_state, None);
    status.state = "Disabled".to_string();
    status.connected_since_unix = None;
    status.message = Some(t("status.failed"));
    save_daemon_status(status);
}

fn set_vpn_dropped(
    tray: &TrayIcon,
    items: &MenuItems,
    vpn_state: &mut VpnState,
    connected_since: &mut Option<Instant>,
    status: &mut DaemonStatus,
    message: Option<String>,
) {
    *vpn_state = VpnState::Dropped;
    *connected_since = None;
    items.toggle.set_text(&t("tray.enable_vpn"));
    items.toggle.set_enabled(true);
    update_tray_icon(tray, items, vpn_state, None);
    status.state = "Dropped".to_string();
    status.connected_since_unix = None;
    status.message = message;
    save_daemon_status(status);
}

fn set_vpn_unreachable(
    tray: &TrayIcon,
    items: &MenuItems,
    vpn_state: &mut VpnState,
    connected_since: &mut Option<Instant>,
    status: &mut DaemonStatus,
) {
    *vpn_state = VpnState::ServerUnreachable;
    *connected_since = None;
    items.toggle.set_text(&t("tray.enable_vpn"));
    items.toggle.set_enabled(true);
    update_tray_icon(tray, items, vpn_state, None);
    status.state = "ServerUnreachable".to_string();
    status.connected_since_unix = None;
    status.message = Some(t("status.vpn_unreachable"));
    save_daemon_status(status);
}

// --------------------------------------------------------------------------
// VPN operations (synchronous, called from the daemon loop)
// --------------------------------------------------------------------------

fn do_enable_vpn(adapter: &str, config: &AppConfig) -> bool {
    let vpn_gw = config.vpn_gateway.clone().unwrap_or_default();
    let dns = config.dns_server.clone().unwrap_or_default();
    let ok = vpn_runtime::enable_vpn(adapter, &vpn_gw, &dns);
    if !ok {
        return false;
    }
    // Verify the route is actually present in the table.
    // The routing commands may return exit 0 yet the route may have been
    // immediately reverted (e.g. by NetworkManager).
    if !SystemHandler::is_route_active(&vpn_gw, adapter) {
        warn!("enable_vpn returned ok but route is not present - treating as failure.");
        return false;
    }
    // Verify traffic is actually flowing through the VPN gateway by checking
    // the first traceroute hop. The route existing is necessary but not
    // sufficient - the gateway may not be forwarding through the tunnel.
    let isp_gw = config.isp_gateway.clone().unwrap_or_default();
    if !SystemHandler::check_connection(&vpn_gw, &isp_gw) {
        warn!("enable_vpn: route present but traffic check failed - not going green.");
        return false;
    }
    set_vpn_active(adapter, &vpn_gw);
    true
}

fn do_disable_vpn(adapter: &str, config: &AppConfig) {
    let vpn_gw = config.vpn_gateway.clone().unwrap_or_default();
    vpn_runtime::disable_vpn(adapter, &vpn_gw);
    clear_vpn_active();
}

// --------------------------------------------------------------------------
// Spawn a UI subprocess
// --------------------------------------------------------------------------

fn any_ui_open() -> bool {
    let config_dir = dragonfox_vpn::config::get_config_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    ["settings", "status", "location", "about"]
        .iter()
        .any(|m| config_dir.join(format!("ui_{m}.lock")).exists())
}

fn spawn_ui(mode: &str) {
    if any_ui_open() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg("--ui")
            .arg(mode)
            .spawn();
    }
}

// --------------------------------------------------------------------------
// Health-check background thread
// --------------------------------------------------------------------------

enum HcEvent {
    Dropped { kill_switched: bool },
    Unreachable,
    Recovered,
    Healthy,
    LocationFetched(String),
    AutoReconnect,
}

fn health_check_loop(
    _cfg_path: std::path::PathBuf,
    tx: std::sync::mpsc::Sender<HcEvent>,
) {
    let mut drop_count: u32 = 0;
    let mut location_tick: u32 = 0;

    loop {
        std::thread::sleep(Duration::from_secs(3));

        // Refresh location from backend every 5 minutes so external
        // location switches (e.g. via the Pi's web UI) are picked up.
        // Each request triggers a sudo shell spawn on the Pi, so polling
        // too aggressively (especially across multiple clients) adds up.
        location_tick += 1;
        if location_tick >= 100 {
            location_tick = 0;
            let config = AppConfig::load();
            if let Some(url) = config.switcher_url {
                let tx2 = tx.clone();
                std::thread::spawn(move || {
                    if let Ok((_, Some(label))) =
                        dragonfox_vpn::api::VpnApi::fetch_locations(&url)
                    {
                        let _ = tx2.send(HcEvent::LocationFetched(label));
                    }
                });
            }
        }

        // Read current status from disk so we don't need shared memory.
        let status = match dragonfox_vpn::daemon_ipc::load_daemon_status() {
            Some(s) => s,
            None => continue,
        };

        let should_check = matches!(
            status.state.as_str(),
            "Connected" | "Dropped" | "ServerUnreachable"
        );
        if !should_check {
            drop_count = 0;
            continue;
        }

        let config = AppConfig::load();
        let adapter = status.adapter.clone();
        let vpn_gw = config.vpn_gateway.clone().unwrap_or_default();
        let isp_gw = config.isp_gateway.clone().unwrap_or_default();

        let result = vpn_runtime::check_health(&adapter, &vpn_gw, &isp_gw);

        if result.vpn_active && result.route_exists {
            drop_count = 0;
            if status.state != "Connected" {
                let _ = tx.send(HcEvent::Recovered);
            } else {
                let _ = tx.send(HcEvent::Healthy);
            }
        } else if !result.vpn_active && result.route_exists {
            drop_count += 1;
            if drop_count >= 2 {
                warn!("VPN dropped after {drop_count} checks - triggering kill switch.");
                SystemHandler::kill_switch_delete_route(&vpn_gw, &adapter);
                SystemHandler::flush_dns();
                drop_count = 0;
                let _ = tx.send(HcEvent::Dropped { kill_switched: true });
            }
        } else if !result.route_exists {
            drop_count = 0;
            if result.pi_reachable {
                let config = AppConfig::load();
                if config.auto_reconnect {
                    let _ = tx.send(HcEvent::AutoReconnect);
                } else {
                    let _ = tx.send(HcEvent::Dropped { kill_switched: false });
                }
            } else {
                let _ = tx.send(HcEvent::Unreachable);
            }
        }
    }
}

fn handle_hc_event(
    ev: HcEvent,
    tray: &TrayIcon,
    items: &MenuItems,
    adapter: &str,
    config: &AppConfig,
    vpn_state: &mut VpnState,
    connected_since: &mut Option<Instant>,
    status: &mut DaemonStatus,
) {
    match ev {
        HcEvent::Healthy => {}
        HcEvent::LocationFetched(label) => {
            // Notify if the location changed externally while connected.
            if *vpn_state == VpnState::Connected
                && !status.location.is_empty()
                && status.location != "Unknown"
                && status.location != label
            {
                dragonfox_vpn::notifications::notify(
                    &t("notify.location_changed_title"),
                    &t_fmt("notify.location_changed_body", &[("location", &label)]),
                );
            }
            status.location = label.clone();
            save_daemon_status(status);
            if *vpn_state == VpnState::Connected {
                update_tray_icon(tray, items, vpn_state, Some(&status.location));
            }
            info!("Startup location sync from backend: {label}");
        }
        HcEvent::Recovered => {
            if *vpn_state != VpnState::Connected {
                *vpn_state = VpnState::Connected;
                if connected_since.is_none() {
                    *connected_since = Some(Instant::now());
                    status.connected_since_unix = Some(current_unix_ts());
                }
                items.toggle.set_text(&t("tray.disable_vpn"));
                items.toggle.set_enabled(true);
                update_tray_icon(tray, items, vpn_state, Some(&status.location));
                status.state = "Connected".to_string();
                status.message = None;
                save_daemon_status(status);
                info!("VPN recovered.");
            }
        }
        HcEvent::Dropped { kill_switched } => {
            if *vpn_state != VpnState::Dropped {
                let msg = if kill_switched {
                    t("status.kill_switch")
                } else {
                    t("status.route_lost")
                };
                if kill_switched {
                    dragonfox_vpn::notifications::notify(
                        &t("notify.kill_switch_title"),
                        &t("notify.kill_switch_body"),
                    );
                } else {
                    dragonfox_vpn::notifications::notify(
                        &t("notify.dropped_title"),
                        &t("notify.dropped_body"),
                    );
                }
                error!("{msg}");
                set_vpn_dropped(tray, items, vpn_state, connected_since, status, Some(msg));
            }
        }
        HcEvent::Unreachable => {
            if *vpn_state != VpnState::ServerUnreachable {
                dragonfox_vpn::notifications::notify(
                    &t("notify.unreachable_title"),
                    &t("notify.unreachable_body"),
                );
                set_vpn_unreachable(tray, items, vpn_state, connected_since, status);
                warn!("VPN server unreachable.");
            }
        }
        HcEvent::AutoReconnect => {
            if matches!(*vpn_state, VpnState::Dropped | VpnState::ServerUnreachable) {
                info!("Auto-reconnect: VPN server is back, re-enabling VPN.");
                set_vpn_enabling(tray, items, vpn_state, status);
                if do_enable_vpn(adapter, config) {
                    set_vpn_connected(tray, items, vpn_state, connected_since, status, config, Some(t("status.auto_reconnected")));
                } else {
                    set_vpn_dropped(tray, items, vpn_state, connected_since, status, Some(t("status.auto_reconnect_failed")));
                }
            }
        }
    }
}

// --------------------------------------------------------------------------
// Menu event dispatch
// --------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_menu_event(
    id: tray_icon::menu::MenuId,
    items: &MenuItems,
    tray: &TrayIcon,
    adapter: &str,
    config: &AppConfig,
    vpn_state: &mut VpnState,
    connected_since: &mut Option<Instant>,
    daemon_status: &mut DaemonStatus,
    should_exit: &mut bool,
) {
    if id == items.dashboard.id() {
        spawn_ui("status");
    } else if id == items.toggle.id() {
        if *vpn_state == VpnState::Connected {
            do_disable_vpn(adapter, config);
            set_vpn_disabled(tray, items, vpn_state, connected_since, daemon_status);
        } else {
            set_vpn_enabling(tray, items, vpn_state, daemon_status);
            if do_enable_vpn(adapter, config) {
                set_vpn_connected(tray, items, vpn_state, connected_since, daemon_status, config, None);
            } else {
                set_vpn_failed(tray, items, vpn_state, daemon_status);
            }
        }
    } else if id == items.location.id() {
        spawn_ui("location");
    } else if id == items.settings.id() {
        spawn_ui("settings");
    } else if id == items.about.id() {
        spawn_ui("about");
    } else if id == items.exit.id() {
        *should_exit = true;
    }
}

// --------------------------------------------------------------------------
// Tray construction
// --------------------------------------------------------------------------

struct MenuItems {
    status_label: Option<MenuItem>, // Linux only; Windows uses the tray tooltip instead
    dashboard: MenuItem,
    toggle: MenuItem, // "Enable VPN" or "Disable VPN" depending on state
    location: MenuItem,
    settings: MenuItem,
    about: MenuItem,
    exit: MenuItem,
}

fn build_tray(config: &AppConfig) -> (TrayIcon, MenuItems) {
    let menu = Menu::new();

    let setup_complete = config.setup_complete;
    // On Linux, libappindicator doesn't show tray tooltips, so a status
    // label at the top of the menu is the only way to surface state there.
    // On Windows the tooltip works, so the menu label is redundant.
    let status_label = if cfg!(not(target_os = "windows")) {
        Some(MenuItem::new(&t("tray.disconnected"), false, None))
    } else {
        None
    };
    let sep0 = PredefinedMenuItem::separator();
    let dashboard = MenuItem::new(&t("tray.dashboard"), true, None);
    let sep1 = PredefinedMenuItem::separator();
    let toggle = MenuItem::new(&t("tray.enable_vpn"), setup_complete, None);
    let sep2 = PredefinedMenuItem::separator();
    let location = MenuItem::new(&t("tray.change_location"), setup_complete, None);
    let sep3 = PredefinedMenuItem::separator();
    let settings = MenuItem::new(&t("tray.settings"), true, None);
    let about = MenuItem::new(&t("tray.about"), true, None);
    let sep4 = PredefinedMenuItem::separator();
    let exit = MenuItem::new(&t("tray.exit"), true, None);

    if let Some(ref sl) = status_label {
        let _ = menu.append(sl);
        let _ = menu.append(&sep0);
    }
    let _ = menu.append(&dashboard);
    let _ = menu.append(&sep1);
    let _ = menu.append(&toggle);
    let _ = menu.append(&sep2);
    let _ = menu.append(&location);
    let _ = menu.append(&sep3);
    let _ = menu.append(&settings);
    let _ = menu.append(&about);
    let _ = menu.append(&sep4);
    let _ = menu.append(&exit);

    let initial_rgba = create_status_icon_rgba(&COLOR_YELLOW);
    let initial_icon = tray_icon::Icon::from_rgba(initial_rgba, 64, 64)
        .unwrap_or_else(|_| {
            tray_icon::Icon::from_rgba(vec![0xFF, 0xC1, 0x07, 0xFF], 1, 1).unwrap()
        });

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_icon(initial_icon)
        .with_tooltip(&format!("DragonFoxVPN: {}", t("tray.disconnected")))
        .build()
        .expect("Failed to create tray icon");

    let items = MenuItems {
        status_label,
        dashboard,
        toggle,
        location,
        settings,
        about,
        exit,
    };
    (tray, items)
}

// --------------------------------------------------------------------------
// Tray icon colour
// --------------------------------------------------------------------------

fn update_tray_icon(tray: &TrayIcon, items: &MenuItems, vpn_state: &VpnState, location: Option<&str>) {
    let color = match vpn_state {
        VpnState::Connected => &COLOR_GREEN,
        VpnState::Dropped => &COLOR_RED,
        VpnState::ServerUnreachable => &COLOR_GRAY,
        VpnState::Enabling => &COLOR_BLUE,
        VpnState::Disabled => &COLOR_YELLOW,
    };
    let label = match vpn_state {
        VpnState::Connected => t_fmt(
            "tray.connected_location",
            &[("location", location.unwrap_or("Unknown"))],
        ),
        VpnState::Dropped => t("tray.dropped"),
        VpnState::ServerUnreachable => t("tray.unreachable"),
        VpnState::Enabling => t("tray.connecting"),
        VpnState::Disabled => t("tray.disconnected"),
    };
    let rgba = create_status_icon_rgba(color);
    if let Ok(icon) = tray_icon::Icon::from_rgba(rgba, 64, 64) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(&label));
    if let Some(ref sl) = items.status_label {
        sl.set_text(&label);
    }
}
