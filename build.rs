// build.rs - DragonFoxVPN: Cargo build script
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Embeds Windows version metadata and the application icon into the
// release executable when the `windows-resources` feature is enabled.

fn main() {
    #[cfg(target_os = "windows")]
    {
        {
            let mut res = winresource::WindowsResource::new();
            res.set("FileVersion", "2.0.0.0");
            res.set("ProductVersion", "2.0.0.0");
            res.set("ProductName", "DragonFoxVPN");
            res.set("FileDescription", "DragonFoxVPN System Tray Application");
            res.set("LegalCopyright", "Copyright (c) 2026 Senjin the Dragon");
            res.set_icon("app.ico");
            res.compile().expect("Failed to compile Windows resources");
        }
    }
}
