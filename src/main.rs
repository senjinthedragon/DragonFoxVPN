// main.rs - DragonFoxVPN: System tray VPN management application
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Binary entry point. All application logic lives in the library crate
// (lib.rs and its modules); this file handles process startup only.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();

    #[cfg(target_os = "linux")]
    {
        // tray-icon uses GTK-backed menus on Linux and panics if GTK isn't initialized first.
        // Windows uses the Win32 event loop and does not require GTK initialization.
        if !gtk::is_initialized_main_thread() {
            gtk::init().unwrap_or_else(|e| {
                eprintln!("Failed to initialize GTK: {e}");
                std::process::exit(1);
            });
        }
    }

    if !dragonfox_vpn::single_instance_check() {
        log::warn!("Another instance of DragonFoxVPN is already running. Exiting.");
        return;
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_visible(false)
            .with_decorations(false)
            .with_taskbar(false)
            .with_inner_size([1.0, 1.0])
            .with_title("DragonFoxVPN"),
        ..Default::default()
    };

    eframe::run_native(
        "DragonFoxVPN",
        options,
        Box::new(|cc| Ok(dragonfox_vpn::app::DragonFoxApp::new(cc))),
    )
    .unwrap_or_else(|e| {
        eprintln!("Fatal error: {e}");
        std::process::exit(1);
    });
}
