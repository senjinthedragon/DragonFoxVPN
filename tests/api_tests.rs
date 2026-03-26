// tests/api_tests.rs - DragonFoxVPN: VPN API parsing and utility tests
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.

use dragonfox_vpn::api::{
    country_to_iso, ensure_trailing_slash, extract_php_error, parse_locations, resolve_redirect,
    strip_continent_emojis, strip_country_emojis,
};

// ---------------------------------------------------------------------------
// country_to_iso
// ---------------------------------------------------------------------------

#[test]
fn test_iso_common_overrides() {
    assert_eq!(country_to_iso("usa"), Some("us"));
    assert_eq!(country_to_iso("uk"), Some("gb"));
    assert_eq!(country_to_iso("south korea"), Some("kr"));
    assert_eq!(country_to_iso("russia"), Some("ru"));
    assert_eq!(country_to_iso("taiwan"), Some("tw"));
    assert_eq!(country_to_iso("hong kong"), Some("hk"));
}

#[test]
fn test_iso_standard_countries() {
    assert_eq!(country_to_iso("germany"), Some("de"));
    assert_eq!(country_to_iso("france"), Some("fr"));
    assert_eq!(country_to_iso("japan"), Some("jp"));
    assert_eq!(country_to_iso("australia"), Some("au"));
    assert_eq!(country_to_iso("canada"), Some("ca"));
    assert_eq!(country_to_iso("brazil"), Some("br"));
    assert_eq!(country_to_iso("singapore"), Some("sg"));
}

#[test]
fn test_iso_india_via_variants() {
    // The backend can serve "India via Singapore" or "India via UK"
    assert_eq!(country_to_iso("india via singapore"), Some("in"));
    assert_eq!(country_to_iso("india via uk"), Some("in"));
    assert_eq!(country_to_iso("india"), Some("in"));
}

#[test]
fn test_iso_alternative_full_names() {
    assert_eq!(country_to_iso("united states"), Some("us"));
    assert_eq!(country_to_iso("united kingdom"), Some("gb"));
}

#[test]
fn test_iso_unknown_returns_none() {
    assert_eq!(country_to_iso("atlantis"), None);
    assert_eq!(country_to_iso(""), None);
    assert_eq!(country_to_iso("xyz"), None);
}

#[test]
fn test_iso_leading_trailing_whitespace_trimmed() {
    assert_eq!(country_to_iso("  usa  "), Some("us"));
    assert_eq!(country_to_iso("\tgermany\t"), Some("de"));
}

#[test]
fn test_iso_case_sensitive_lowercase_required() {
    // The function expects lowercase input (caller normalises before passing in)
    assert_eq!(country_to_iso("USA"), None);
    assert_eq!(country_to_iso("Germany"), None);
}

// ---------------------------------------------------------------------------
// parse_locations - HTML parsing
// ---------------------------------------------------------------------------

fn make_html(body: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><body><div class="dropdown-content">{body}</div></body></html>"#
    )
}

#[test]
fn test_parse_empty_html_returns_no_locations() {
    let (locs, current) = parse_locations(&make_html(""));
    assert!(locs.is_empty());
    assert!(current.is_none());
}

#[test]
fn test_parse_single_location() {
    let html = make_html(
        r#"<div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="uk-london">UK - London</div>"#,
    );
    let (locs, _) = parse_locations(&html);
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].label, "UK - London");
    assert_eq!(locs[0].value, "uk-london");
    assert_eq!(locs[0].continent, "Europe");
    assert_eq!(locs[0].country, "uk");
}

#[test]
fn test_parse_active_item_sets_current_location() {
    let html = make_html(
        r#"<div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="uk-london">UK - London</div>
           <div class="dropdown-item active" data-value="de-frankfurt">Germany - Frankfurt</div>"#,
    );
    let (locs, current) = parse_locations(&html);
    assert_eq!(locs.len(), 2);
    assert_eq!(current, Some("Germany - Frankfurt".to_string()));
}

#[test]
fn test_parse_no_active_item_current_is_none() {
    let html = make_html(
        r#"<div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="uk-london">UK - London</div>"#,
    );
    let (_, current) = parse_locations(&html);
    assert!(current.is_none());
}

#[test]
fn test_parse_multiple_continents() {
    let html = make_html(
        r#"<div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="uk-london">UK - London</div>
           <div class="optgroup-label">Asia</div>
           <div class="dropdown-item" data-value="jp-tokyo">Japan - Tokyo</div>
           <div class="dropdown-item" data-value="sg">Singapore</div>"#,
    );
    let (locs, _) = parse_locations(&html);
    assert_eq!(locs.len(), 3);
    assert_eq!(locs[0].continent, "Europe");
    assert_eq!(locs[1].continent, "Asia");
    assert_eq!(locs[2].continent, "Asia");
}

#[test]
fn test_parse_items_without_continent_are_skipped() {
    // Items before any optgroup-label should be ignored
    let html = make_html(
        r#"<div class="dropdown-item" data-value="orphan">Orphan</div>
           <div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="uk-london">UK - London</div>"#,
    );
    let (locs, _) = parse_locations(&html);
    assert_eq!(locs.len(), 1);
    assert_eq!(locs[0].value, "uk-london");
}

#[test]
fn test_parse_country_extracted_from_label() {
    // Country is the part before " - "
    let html = make_html(
        r#"<div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="de-berlin">Germany - Berlin</div>"#,
    );
    let (locs, _) = parse_locations(&html);
    assert_eq!(locs[0].country, "germany");
}

#[test]
fn test_parse_continent_emoji_stripped() {
    let html = make_html(
        r#"<div class="optgroup-label">🌍 Europe</div>
           <div class="dropdown-item" data-value="uk-london">UK - London</div>"#,
    );
    let (locs, _) = parse_locations(&html);
    assert_eq!(locs[0].continent, "Europe");
}

#[test]
fn test_parse_flag_emoji_stripped_from_label() {
    // 🇬🇧 is two regional indicator chars; the label should have them removed
    let html = make_html(
        r#"<div class="optgroup-label">Europe</div>
           <div class="dropdown-item" data-value="uk-london">🇬🇧 UK - London</div>"#,
    );
    let (locs, _) = parse_locations(&html);
    assert_eq!(locs[0].label, "UK - London");
}

// ---------------------------------------------------------------------------
// strip helpers
// ---------------------------------------------------------------------------

#[test]
fn test_strip_continent_emojis_removes_globe() {
    assert_eq!(strip_continent_emojis("🌍 Europe"), "Europe");
    assert_eq!(strip_continent_emojis("🌎 North America"), "North America");
    assert_eq!(strip_continent_emojis("🌏 Asia"), "Asia");
    assert_eq!(strip_continent_emojis("🌐 Other"), "Other");
}

#[test]
fn test_strip_continent_emojis_no_emoji_unchanged() {
    assert_eq!(strip_continent_emojis("Europe"), "Europe");
}

#[test]
fn test_strip_country_emojis_removes_flag() {
    // 🇬🇧 = U+1F1EC U+1F1E7
    let input = "🇬🇧 UK - London";
    let result = strip_country_emojis(input);
    assert_eq!(result, "UK - London");
}

#[test]
fn test_strip_country_emojis_plain_text_unchanged() {
    assert_eq!(
        strip_country_emojis("Germany - Frankfurt"),
        "Germany - Frankfurt"
    );
}

#[test]
fn test_strip_country_emojis_multiple_flags() {
    let input = "🇺🇸🇬🇧 Multi";
    let result = strip_country_emojis(input);
    assert_eq!(result, "Multi");
}

// ---------------------------------------------------------------------------
// ensure_trailing_slash
// ---------------------------------------------------------------------------

#[test]
fn test_trailing_slash_added_when_no_path() {
    assert_eq!(
        ensure_trailing_slash("http://10.0.0.20"),
        "http://10.0.0.20/"
    );
    assert_eq!(
        ensure_trailing_slash("https://vpn.example.com"),
        "https://vpn.example.com/"
    );
}

#[test]
fn test_trailing_slash_not_duplicated() {
    assert_eq!(
        ensure_trailing_slash("http://10.0.0.20/"),
        "http://10.0.0.20/"
    );
}

#[test]
fn test_trailing_slash_not_added_when_path_present() {
    assert_eq!(
        ensure_trailing_slash("http://10.0.0.20/switch"),
        "http://10.0.0.20/switch"
    );
    assert_eq!(
        ensure_trailing_slash("https://vpn.example.com/vpn/"),
        "https://vpn.example.com/vpn/"
    );
}

// ---------------------------------------------------------------------------
// resolve_redirect
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_redirect_absolute_url_returned_as_is() {
    assert_eq!(
        resolve_redirect("http://10.0.0.20/", "https://10.0.0.20/"),
        "https://10.0.0.20/"
    );
    assert_eq!(
        resolve_redirect("http://old.host/", "http://new.host/path"),
        "http://new.host/path"
    );
}

#[test]
fn test_resolve_redirect_absolute_path_uses_current_host() {
    assert_eq!(
        resolve_redirect("http://10.0.0.20/", "/switch"),
        "http://10.0.0.20/switch"
    );
    assert_eq!(
        resolve_redirect("https://vpn.example.com/foo", "/bar"),
        "https://vpn.example.com/bar"
    );
}

#[test]
fn test_resolve_redirect_relative_path_returned_as_is() {
    // Relative paths (no leading slash, no scheme) are passed through unchanged.
    assert_eq!(resolve_redirect("http://10.0.0.20/", "switch"), "switch");
}

// ---------------------------------------------------------------------------
// extract_php_error
// ---------------------------------------------------------------------------

#[test]
fn test_extract_php_error_returns_none_when_absent() {
    assert!(extract_php_error("<html><body><p>All good.</p></body></html>").is_none());
    assert!(extract_php_error("").is_none());
}

#[test]
fn test_extract_php_error_finds_error_message() {
    let html =
        "<html><body><p class='error'><strong>Invalid location value</strong></p></body></html>";
    assert_eq!(
        extract_php_error(html).as_deref(),
        Some("Invalid location value")
    );
}

#[test]
fn test_extract_php_error_trims_whitespace() {
    let html = "<html><body><p class=\"error\"><strong>  Bad request  </strong></p></body></html>";
    assert_eq!(extract_php_error(html).as_deref(), Some("Bad request"));
}

#[test]
fn test_extract_php_error_returns_none_for_empty_strong() {
    let html = "<html><body><p class='error'><strong>   </strong></p></body></html>";
    assert!(extract_php_error(html).is_none());
}
