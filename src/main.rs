// main.rs - DragonFoxVPN: Pure tray application entry point
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// The main() function runs entirely without a persistent eframe window.
// The application lives in the system tray. Dialogs are opened on-demand
// via eframe::run_native(), which blocks until the dialog is closed. A
// background health_check_loop thread handles kill-switch logic
// independently. VPN enable/disable operations are dispatched as threads
// and update the shared AppState.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use log::{error, info, warn};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use dragonfox_vpn::app::{DashboardDialog, FlagCache, LocationDialog, SetupDialog};
use dragonfox_vpn::autostart::AutoStartManager;
use dragonfox_vpn::config::AppConfig;
use dragonfox_vpn::icons::{
    create_status_icon_rgba, COLOR_BLUE, COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_YELLOW,
};
use dragonfox_vpn::state::{AppState, VpnState};
use dragonfox_vpn::system::SystemHandler;
use dragonfox_vpn::vpn_runtime;

// --------------------------------------------------------------------------
// Dialog kind
// --------------------------------------------------------------------------

enum DialogKind {
    Setup { first_run: bool },
    Dashboard,
    Location,
}

// --------------------------------------------------------------------------
// Menu item handles
// --------------------------------------------------------------------------

struct MenuItems {
    dashboard: MenuItem,
    enable: MenuItem,
    disable: MenuItem,
    location: MenuItem,
    autoconnect: CheckMenuItem,
    autostart: CheckMenuItem,
    settings: MenuItem,
    exit: MenuItem,
}

// --------------------------------------------------------------------------
// main()
// --------------------------------------------------------------------------

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    #[cfg(target_os = "linux")]
    {
        if !gtk::is_initialized_main_thread() {
            gtk::init().unwrap_or_else(|e| {
                eprintln!("Failed to initialize GTK: {e}");
                std::process::exit(1);
            });
        }
    }

    if !dragonfox_vpn::single_instance_check() {
        warn!("Another instance of DragonFoxVPN is already running. Exiting.");
        return;
    }

    // Load config and state.
    let config = Arc::new(Mutex::new(AppConfig::load()));
    let state = Arc::new(Mutex::new(AppState::default()));
    let flag_cache: FlagCache = Arc::new(Mutex::new(HashMap::new()));

    // Detect active network adapter.
    {
        let adapter = SystemHandler::get_active_adapter();
        info!("Active adapter: {adapter}");
        state.lock().unwrap().adapter_name = adapter;
    }

    // Restore last known location from config.
    {
        if let (Ok(mut st), Ok(cfg)) = (state.lock(), config.lock()) {
            if let Some(loc) = &cfg.last_location {
                st.vpn_location = loc.clone();
            }
        }
    }

    // Health check notification channel (main thread drains this to know when to refresh tray).
    let (hc_tx, hc_rx) = mpsc::channel::<()>();

    // Spawn health check background thread.
    {
        let s = Arc::clone(&state);
        let c = Arc::clone(&config);
        let tx = hc_tx.clone();
        std::thread::spawn(move || health_check_loop(s, c, tx));
    }

    // Build tray icon and menu.
    let (tray, items) = build_tray(&config.lock().unwrap());

    // Determine initial dialog.
    let first_run = !config.lock().unwrap().setup_complete;
    let mut pending_dialog: Option<DialogKind> = if first_run {
        Some(DialogKind::Setup { first_run: true })
    } else {
        None
    };

    // Auto-connect if configured and not first run.
    if !first_run && config.lock().unwrap().auto_connect {
        do_enable_vpn(Arc::clone(&state), Arc::clone(&config));
    }

    // Track tray icon state to avoid redundant updates.
    let mut last_tray_state = VpnState::Disabled;
    update_tray_icon(&tray, &VpnState::Disabled);

    // --------------------------------------------------------------------------
    // Main event loop
    // --------------------------------------------------------------------------
    loop {
        // Pump GTK events so tray menus stay responsive between dialogs.
        #[cfg(target_os = "linux")]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Drain health-check pings (actual state is in the shared AppState).
        while hc_rx.try_recv().is_ok() {}

        // Update tray icon if VPN state changed.
        let current_state = state.lock().unwrap().vpn_state.clone();
        if current_state != last_tray_state {
            update_tray_icon(&tray, &current_state);
            update_menu_enabled(&items, &current_state);
            last_tray_state = current_state;
        }

        // Poll tray icon events (double-click → Dashboard).
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if ev.click_type == tray_icon::ClickType::Double {
                pending_dialog = Some(DialogKind::Dashboard);
            }
        }

        // Poll menu events.
        let mut should_exit = false;
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            handle_menu_event(
                ev.id,
                &items,
                &state,
                &config,
                &tray,
                &mut last_tray_state,
                &mut pending_dialog,
                &mut should_exit,
            );
        }

        if should_exit {
            break;
        }

        // Open pending dialog. eframe::run_native blocks until the dialog closes.
        if let Some(kind) = pending_dialog.take() {
            let setup_was_incomplete = !config.lock().unwrap().setup_complete;

            run_dialog(kind, Arc::clone(&state), Arc::clone(&config), Arc::clone(&flag_cache));

            // After Setup dialog closes, refresh adapter if setup just completed.
            if setup_was_incomplete && config.lock().unwrap().setup_complete {
                let adapter = SystemHandler::get_active_adapter();
                state.lock().unwrap().adapter_name = adapter;
                items.enable.set_enabled(true);
                items.location.set_enabled(true);
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    // Clean exit: disable VPN and drop tray.
    {
        let (adapter, vpn_gw) = {
            let st = state.lock().unwrap();
            let cfg = config.lock().unwrap();
            (
                st.adapter_name.clone(),
                cfg.vpn_gateway.clone().unwrap_or_default(),
            )
        };
        vpn_runtime::disable_vpn(&adapter, &vpn_gw);
    }
    drop(tray);
}

// --------------------------------------------------------------------------
// Tray construction
// --------------------------------------------------------------------------

fn build_tray(config: &AppConfig) -> (TrayIcon, MenuItems) {
    let menu = Menu::new();

    let dashboard = MenuItem::new("Status Dashboard", true, None);
    let sep1 = PredefinedMenuItem::separator();
    let setup_complete = config.setup_complete;
    let enable = MenuItem::new("Enable VPN", setup_complete, None);
    let disable = MenuItem::new("Disable VPN", false, None);
    let sep2 = PredefinedMenuItem::separator();
    let location = MenuItem::new("Change Location...", setup_complete, None);
    let autoconnect =
        CheckMenuItem::new("Auto-Connect on Start", true, config.auto_connect, None);
    let autostart_avail = AutoStartManager::is_available();
    let autostart_checked = AutoStartManager::is_enabled();
    let autostart = if autostart_avail {
        CheckMenuItem::new("Run on Startup", true, autostart_checked, None)
    } else {
        CheckMenuItem::new("Run on Startup (Windows only)", false, false, None)
    };
    let sep3 = PredefinedMenuItem::separator();
    let settings = MenuItem::new("Settings...", true, None);
    let sep4 = PredefinedMenuItem::separator();
    let exit = MenuItem::new("Exit", true, None);

    let _ = menu.append(&dashboard);
    let _ = menu.append(&sep1);
    let _ = menu.append(&enable);
    let _ = menu.append(&disable);
    let _ = menu.append(&sep2);
    let _ = menu.append(&location);
    let _ = menu.append(&autoconnect);
    let _ = menu.append(&autostart);
    let _ = menu.append(&sep3);
    let _ = menu.append(&settings);
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
        .with_tooltip("DragonFoxVPN: Disabled")
        .build()
        .expect("Failed to create tray icon");

    let items = MenuItems { dashboard, enable, disable, location, autoconnect, autostart, settings, exit };
    (tray, items)
}

// --------------------------------------------------------------------------
// Tray icon update
// --------------------------------------------------------------------------

fn update_tray_icon(tray: &TrayIcon, vpn_state: &VpnState) {
    let color = match vpn_state {
        VpnState::Connected => &COLOR_GREEN,
        VpnState::Dropped => &COLOR_RED,
        VpnState::ServerUnreachable => &COLOR_GRAY,
        VpnState::Enabling => &COLOR_BLUE,
        VpnState::Disabled => &COLOR_YELLOW,
    };
    let tooltip = match vpn_state {
        VpnState::Connected => "DragonFoxVPN: Connected",
        VpnState::Dropped => "DragonFoxVPN: Connection Dropped",
        VpnState::ServerUnreachable => "DragonFoxVPN: Server Unreachable",
        VpnState::Enabling => "DragonFoxVPN: Connecting...",
        VpnState::Disabled => "DragonFoxVPN: Disabled",
    };
    let rgba = create_status_icon_rgba(color);
    if let Ok(icon) = tray_icon::Icon::from_rgba(rgba, 64, 64) {
        let _ = tray.set_icon(Some(icon));
    }
    let _ = tray.set_tooltip(Some(tooltip));
}

fn update_menu_enabled(items: &MenuItems, vpn_state: &VpnState) {
    let connected = *vpn_state == VpnState::Connected;
    items.enable.set_enabled(!connected);
    items.disable.set_enabled(connected);
}


// --------------------------------------------------------------------------
// Menu event dispatch
// --------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn handle_menu_event(
    id: tray_icon::menu::MenuId,
    items: &MenuItems,
    state: &Arc<Mutex<AppState>>,
    config: &Arc<Mutex<AppConfig>>,
    tray: &TrayIcon,
    last_tray_state: &mut VpnState,
    pending_dialog: &mut Option<DialogKind>,
    should_exit: &mut bool,
) {
    if id == items.dashboard.id() {
        *pending_dialog = Some(DialogKind::Dashboard);
    } else if id == items.enable.id() {
        do_enable_vpn(Arc::clone(state), Arc::clone(config));
    } else if id == items.disable.id() {
        do_disable_vpn(Arc::clone(state), Arc::clone(config));
    } else if id == items.location.id() {
        *pending_dialog = Some(DialogKind::Location);
    } else if id == items.autoconnect.id() {
        let checked = items.autoconnect.is_checked();
        if let Ok(mut cfg) = config.lock() {
            cfg.auto_connect = checked;
            cfg.save();
        }
    } else if id == items.autostart.id() {
        let checked = items.autostart.is_checked();
        AutoStartManager::set_autostart(checked);
    } else if id == items.settings.id() {
        *pending_dialog = Some(DialogKind::Setup { first_run: false });
    } else if id == items.exit.id() {
        *should_exit = true;
        // Disable VPN synchronously on exit.
        let (adapter, vpn_gw) = {
            let st = state.lock().unwrap();
            let cfg = config.lock().unwrap();
            (
                st.adapter_name.clone(),
                cfg.vpn_gateway.clone().unwrap_or_default(),
            )
        };
        vpn_runtime::disable_vpn(&adapter, &vpn_gw);
    }

    let _ = (tray, last_tray_state); // suppress unused warnings
}

// --------------------------------------------------------------------------
// run_dialog: open a dialog via eframe::run_native (blocks until closed)
// --------------------------------------------------------------------------

fn run_dialog(
    kind: DialogKind,
    state: Arc<Mutex<AppState>>,
    config: Arc<Mutex<AppConfig>>,
    flag_cache: FlagCache,
) {
    let (title, width, height, resizable) = match &kind {
        DialogKind::Setup { first_run } => (
            if *first_run { "DragonFoxVPN — Initial Setup" } else { "DragonFoxVPN — Settings" },
            500.0_f32,
            360.0_f32,
            false,
        ),
        DialogKind::Dashboard => ("DragonFoxVPN — Status", 420.0, 300.0, false),
        DialogKind::Location => ("DragonFoxVPN — Change Location", 620.0, 700.0, true),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([width, height])
            .with_resizable(resizable),
        // vsync blocks eglSwapBuffers waiting for the KDE/Wayland compositor,
        // which stalls the winit event loop and prevents close events from
        // being processed. Disabling it keeps the event loop responsive.
        vsync: false,
        ..Default::default()
    };

    let _ = eframe::run_native(
        "DragonFoxVPN",
        options,
        Box::new(move |_cc| {
            Ok(match kind {
                DialogKind::Setup { first_run } => {
                    Box::new(SetupDialog::new(config, first_run)) as Box<dyn eframe::App>
                }
                DialogKind::Dashboard => {
                    Box::new(DashboardDialog::new(state, config)) as Box<dyn eframe::App>
                }
                DialogKind::Location => {
                    Box::new(LocationDialog::new(state, config, flag_cache)) as Box<dyn eframe::App>
                }
            })
        }),
    );
}

// --------------------------------------------------------------------------
// VPN enable / disable (spawn threads, update shared state)
// --------------------------------------------------------------------------

fn do_enable_vpn(state: Arc<Mutex<AppState>>, config: Arc<Mutex<AppConfig>>) {
    // Immediately mark as Enabling so the tray updates without waiting for the thread.
    if let Ok(mut st) = state.lock() {
        st.vpn_state = VpnState::Enabling;
        st.manual_disable = false;
    }

    std::thread::spawn(move || {
        let (adapter, vpn_gw, dns) = {
            let st = state.lock().unwrap();
            let cfg = config.lock().unwrap();
            (
                st.adapter_name.clone(),
                cfg.vpn_gateway.clone().unwrap_or_default(),
                cfg.dns_server.clone().unwrap_or_default(),
            )
        };

        let success = vpn_runtime::enable_vpn(&adapter, &vpn_gw, &dns);

        if let Ok(mut st) = state.lock() {
            if success {
                st.vpn_state = VpnState::Connected;
                st.connection_start_time = Some(Instant::now());
                info!("VPN enabled successfully.");
            } else {
                error!("Failed to enable VPN routing.");
                st.vpn_state = VpnState::Disabled;
                st.manual_disable = true;
            }
        }
    });
}

fn do_disable_vpn(state: Arc<Mutex<AppState>>, config: Arc<Mutex<AppConfig>>) {
    std::thread::spawn(move || {
        let (adapter, vpn_gw) = {
            let st = state.lock().unwrap();
            let cfg = config.lock().unwrap();
            (
                st.adapter_name.clone(),
                cfg.vpn_gateway.clone().unwrap_or_default(),
            )
        };

        vpn_runtime::disable_vpn(&adapter, &vpn_gw);

        if let Ok(mut st) = state.lock() {
            st.vpn_state = VpnState::Disabled;
            st.connection_start_time = None;
            st.manual_disable = true;
            info!("VPN disabled.");
        }
    });
}

// --------------------------------------------------------------------------
// Health check loop (independent background thread)
// --------------------------------------------------------------------------

fn health_check_loop(
    state: Arc<Mutex<AppState>>,
    config: Arc<Mutex<AppConfig>>,
    tx: mpsc::Sender<()>,
) {
    let mut drop_count: u32 = 0;

    loop {
        std::thread::sleep(Duration::from_secs(3));

        // Only check when connected or unreachable.
        let current_vpn_state = state.lock().unwrap().vpn_state.clone();
        let setup_complete = config.lock().unwrap().setup_complete;

        if !setup_complete {
            continue;
        }

        let should_check = matches!(
            current_vpn_state,
            VpnState::Connected | VpnState::ServerUnreachable | VpnState::Dropped
        );
        if !should_check {
            drop_count = 0;
            continue;
        }

        let (adapter, vpn_gw, isp_gw) = {
            let st = state.lock().unwrap();
            let cfg = config.lock().unwrap();
            (
                st.adapter_name.clone(),
                cfg.vpn_gateway.clone().unwrap_or_default(),
                cfg.isp_gateway.clone().unwrap_or_default(),
            )
        };

        let result = vpn_runtime::check_health(&adapter, &vpn_gw, &isp_gw);
        let manual_disable = state.lock().unwrap().manual_disable;

        if result.vpn_active && result.route_exists {
            // Connection healthy.
            drop_count = 0;
            if let Ok(mut st) = state.lock() {
                if st.vpn_state != VpnState::Connected {
                    st.vpn_state = VpnState::Connected;
                    if st.connection_start_time.is_none() {
                        st.connection_start_time = Some(Instant::now());
                    }
                }
            }
        } else if !result.vpn_active && result.route_exists && !manual_disable {
            // Route is up but traffic isn't flowing through VPN.
            drop_count += 1;
            if drop_count >= 2 {
                warn!("VPN connection dropped after {drop_count} checks. Triggering kill switch.");
                SystemHandler::kill_switch_delete_route(&vpn_gw, &adapter);
                SystemHandler::flush_dns();
                if let Ok(mut st) = state.lock() {
                    st.vpn_state = VpnState::Dropped;
                    st.connection_start_time = None;
                }
                drop_count = 0;
            } else {
                if let Ok(mut st) = state.lock() {
                    if st.vpn_state == VpnState::Connected {
                        st.vpn_state = VpnState::Dropped;
                    }
                }
            }
        } else if !result.route_exists {
            // No route at all.
            drop_count = 0;
            let vpn_state_now = state.lock().unwrap().vpn_state.clone();
            if vpn_state_now == VpnState::Connected || vpn_state_now == VpnState::Dropped {
                // Route vanished unexpectedly.
                if !manual_disable {
                    if !result.pi_reachable {
                        if let Ok(mut st) = state.lock() {
                            st.vpn_state = VpnState::ServerUnreachable;
                        }
                    } else {
                        if let Ok(mut st) = state.lock() {
                            st.vpn_state = VpnState::Dropped;
                        }
                    }
                }
            } else if vpn_state_now == VpnState::ServerUnreachable {
                if result.pi_reachable {
                    info!("VPN server reachable again.");
                    if let Ok(mut st) = state.lock() {
                        st.vpn_state = VpnState::Disabled;
                    }
                }
            }
        }

        // Notify main loop that state may have changed.
        let _ = tx.send(());
    }
}
