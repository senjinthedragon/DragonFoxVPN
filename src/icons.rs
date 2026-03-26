// icons.rs - DragonFoxVPN: Programmatic icon generation
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Generates status indicator icons at runtime as 64×64 RGBA bitmaps -
// no external image assets required. Each icon is a coloured circle with
// a radial gradient and a specular highlight, matching the style of the
// original Python QPainter-drawn icons.

use tray_icon::Icon;

/// Colour definitions for each VPN state.
pub struct IconColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl IconColor {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

pub const COLOR_GREEN: IconColor = IconColor::new(0x4C, 0xAF, 0x50);
pub const COLOR_YELLOW: IconColor = IconColor::new(0xFF, 0xC1, 0x07);
pub const COLOR_RED: IconColor = IconColor::new(0xF4, 0x43, 0x36);
pub const COLOR_BLUE: IconColor = IconColor::new(0x21, 0x96, 0xF3);
pub const COLOR_GRAY: IconColor = IconColor::new(0x9E, 0x9E, 0x9E);

/// Draw a filled circle with a subtle radial-gradient-like effect into a 64x64 RGBA buffer.
/// Returns raw RGBA bytes (width=64, height=64).
pub fn create_status_icon_rgba(color: &IconColor) -> Vec<u8> {
    const SIZE: usize = 64;
    const CENTER: f32 = 32.0;
    const RADIUS: f32 = 28.0;
    const BORDER: f32 = 2.0;

    let mut buf = vec![0u8; SIZE * SIZE * 4];

    for y in 0..SIZE {
        for x in 0..SIZE {
            let fx = x as f32 - CENTER;
            let fy = y as f32 - CENTER;
            let dist = (fx * fx + fy * fy).sqrt();

            if dist <= RADIUS - BORDER {
                // Interior: apply a simple radial gradient (lighter toward top-left)
                let t = 1.0 - (dist / (RADIUS - BORDER)).powi(2);
                let shine_dist = ((fx - (-8.0)).powi(2) + (fy - (-12.0)).powi(2)).sqrt();
                let shine = ((1.0 - (shine_dist / 22.0).min(1.0)) * 0.35).max(0.0);

                let r = lerp(color.r as f32, 255.0, shine * t).min(255.0) as u8;
                let g = lerp(color.g as f32, 255.0, shine * t).min(255.0) as u8;
                let b = lerp(color.b as f32, 255.0, shine * t).min(255.0) as u8;

                let idx = (y * SIZE + x) * 4;
                buf[idx] = r;
                buf[idx + 1] = g;
                buf[idx + 2] = b;
                buf[idx + 3] = 255;
            } else if dist <= RADIUS {
                // Border ring: darker shade
                let br = (color.r as f32 * 0.7) as u8;
                let bg = (color.g as f32 * 0.7) as u8;
                let bb = (color.b as f32 * 0.7) as u8;
                // Anti-alias the border
                let alpha = ((RADIUS - dist) / BORDER * 255.0).clamp(0.0, 255.0) as u8;
                let idx = (y * SIZE + x) * 4;
                buf[idx] = br;
                buf[idx + 1] = bg;
                buf[idx + 2] = bb;
                buf[idx + 3] = alpha;
            }
            // Outside circle: transparent (already 0)
        }
    }

    buf
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Create a `tray_icon::Icon` for the given colour.
/// Returns None if icon creation fails (logs error).
pub fn make_tray_icon(color: &IconColor) -> Option<Icon> {
    let rgba = create_status_icon_rgba(color);
    match Icon::from_rgba(rgba, 64, 64) {
        Ok(icon) => Some(icon),
        Err(e) => {
            log::error!("Failed to create tray icon: {e}");
            None
        }
    }
}

/// Icon set for all VPN states.
#[allow(dead_code)]
pub struct Icons {
    pub connected: Option<Icon>,
    pub disabled: Option<Icon>,
    pub dropped: Option<Icon>,
    pub unreachable: Option<Icon>,
    pub info: Option<Icon>,
}

impl Icons {
    pub fn load() -> Self {
        Self {
            connected: make_tray_icon(&COLOR_GREEN),
            disabled: make_tray_icon(&COLOR_YELLOW),
            dropped: make_tray_icon(&COLOR_RED),
            unreachable: make_tray_icon(&COLOR_GRAY),
            info: make_tray_icon(&COLOR_BLUE),
        }
    }
}
