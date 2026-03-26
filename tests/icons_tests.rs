// tests/icons_tests.rs - DragonFoxVPN: Icon generation tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::icons::{
    create_status_icon_rgba, COLOR_BLUE, COLOR_GRAY, COLOR_GREEN, COLOR_RED, COLOR_YELLOW,
};

const SIZE: usize = 64;

fn pixel(buf: &[u8], x: usize, y: usize) -> (u8, u8, u8, u8) {
    let i = (y * SIZE + x) * 4;
    (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
}

// ---------------------------------------------------------------------------
// Buffer dimensions
// ---------------------------------------------------------------------------

#[test]
fn test_rgba_buffer_is_correct_size() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    assert_eq!(buf.len(), SIZE * SIZE * 4);
}

#[test]
fn test_rgba_buffer_length_is_same_for_all_colors() {
    for color in [&COLOR_GREEN, &COLOR_RED, &COLOR_YELLOW, &COLOR_BLUE, &COLOR_GRAY] {
        assert_eq!(create_status_icon_rgba(color).len(), SIZE * SIZE * 4);
    }
}

// ---------------------------------------------------------------------------
// Transparency - corners are outside the circle
// ---------------------------------------------------------------------------

#[test]
fn test_top_left_corner_is_transparent() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let (_, _, _, a) = pixel(&buf, 0, 0);
    assert_eq!(a, 0, "top-left corner should be transparent");
}

#[test]
fn test_top_right_corner_is_transparent() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let (_, _, _, a) = pixel(&buf, 63, 0);
    assert_eq!(a, 0, "top-right corner should be transparent");
}

#[test]
fn test_bottom_left_corner_is_transparent() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let (_, _, _, a) = pixel(&buf, 0, 63);
    assert_eq!(a, 0, "bottom-left corner should be transparent");
}

#[test]
fn test_bottom_right_corner_is_transparent() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let (_, _, _, a) = pixel(&buf, 63, 63);
    assert_eq!(a, 0, "bottom-right corner should be transparent");
}

// ---------------------------------------------------------------------------
// Opacity - centre is inside the circle
// ---------------------------------------------------------------------------

#[test]
fn test_centre_pixel_is_fully_opaque() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let (_, _, _, a) = pixel(&buf, 32, 32);
    assert_eq!(a, 255, "centre pixel should be fully opaque");
}

#[test]
fn test_centre_is_opaque_for_all_colors() {
    for color in [&COLOR_GREEN, &COLOR_RED, &COLOR_YELLOW, &COLOR_BLUE, &COLOR_GRAY] {
        let buf = create_status_icon_rgba(color);
        let (_, _, _, a) = pixel(&buf, 32, 32);
        assert_eq!(a, 255, "centre should be opaque for every colour");
    }
}

// ---------------------------------------------------------------------------
// Colour correctness at centre
// ---------------------------------------------------------------------------

#[test]
fn test_green_icon_centre_is_green_dominant() {
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let (r, g, b, _) = pixel(&buf, 32, 32);
    assert!(g > r, "green icon: g ({g}) should exceed r ({r})");
    assert!(g > b, "green icon: g ({g}) should exceed b ({b})");
}

#[test]
fn test_red_icon_centre_is_red_dominant() {
    let buf = create_status_icon_rgba(&COLOR_RED);
    let (r, g, b, _) = pixel(&buf, 32, 32);
    assert!(r > g, "red icon: r ({r}) should exceed g ({g})");
    assert!(r > b, "red icon: r ({r}) should exceed b ({b})");
}

#[test]
fn test_blue_icon_centre_is_blue_dominant() {
    let buf = create_status_icon_rgba(&COLOR_BLUE);
    let (r, _g, b, _) = pixel(&buf, 32, 32);
    assert!(b > r, "blue icon: b ({b}) should exceed r ({r})");
}

// ---------------------------------------------------------------------------
// Different colours produce different output
// ---------------------------------------------------------------------------

#[test]
fn test_green_and_red_icons_differ() {
    let green = create_status_icon_rgba(&COLOR_GREEN);
    let red = create_status_icon_rgba(&COLOR_RED);
    assert_ne!(green, red);
}

#[test]
fn test_all_five_icons_are_distinct() {
    let bufs: Vec<Vec<u8>> = [&COLOR_GREEN, &COLOR_RED, &COLOR_YELLOW, &COLOR_BLUE, &COLOR_GRAY]
        .iter()
        .map(|c| create_status_icon_rgba(c))
        .collect();

    for i in 0..bufs.len() {
        for j in (i + 1)..bufs.len() {
            assert_ne!(bufs[i], bufs[j], "icons {i} and {j} should differ");
        }
    }
}

// ---------------------------------------------------------------------------
// Structural sanity - circle ring
// ---------------------------------------------------------------------------

#[test]
fn test_many_non_corner_pixels_have_some_opacity() {
    // At least some pixels across the icon should be visible (sanity check
    // that the drawing loop actually ran).
    let buf = create_status_icon_rgba(&COLOR_GREEN);
    let opaque_count = buf.chunks(4).filter(|p| p[3] > 0).count();
    assert!(
        opaque_count > 100,
        "expected many opaque pixels, got {opaque_count}"
    );
}
