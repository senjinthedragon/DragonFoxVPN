// notifications.rs - DragonFoxVPN: System notification helper
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Sends non-blocking system notifications for critical VPN events.
// Spawns a thread for each notification so the main loop is never stalled
// by the notification daemon. Only a small set of genuinely important
// events trigger notifications - routine state changes do not.

/// Send a system notification. Fire-and-forget; never blocks the caller.
pub fn notify(summary: &str, body: &str) {
    let summary = summary.to_string();
    let body = body.to_string();
    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        {
            let mut n = notify_rust::Notification::new();
            n.appname("DragonFoxVPN").summary(&summary).body(&body);
            n.show().ok();
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new("notify-send")
                .args(["-a", "DragonFoxVPN", "-i", "network-vpn", &summary, &body])
                .status()
                .ok();
        }
    });
}
