// api.rs - DragonFoxVPN: VPN backend HTTP API
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Interacts with the Raspberry Pi VPN switcher web backend. Fetches the
// list of available locations by scraping the switcher page HTML, and
// switches the active location via HTTP POST. SSL verification is disabled
// to support self-signed certificates on the local network.

use log::{error, warn};
use scraper::{Html, Selector};
use std::time::Duration;

/// A single available VPN location from the backend.
#[derive(Debug, Clone)]
pub struct Location {
    pub continent: String,
    pub value: String,
    pub label: String,
    pub country: String, // Lowercase country name for ISO lookup
}

pub struct VpnApi;

impl VpnApi {
    /// Fetch available VPN locations from the switcher web UI.
    /// Returns (locations, current_location_label).
    /// SSL verification is disabled (self-signed cert on Raspberry Pi).
    pub fn fetch_locations(switcher_url: &str) -> Result<(Vec<Location>, Option<String>), String> {
        let tls = ureq::tls::TlsConfig::builder()
            .disable_verification(true)
            .build();

        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(Duration::from_secs(5)))
            .build();
        let agent = ureq::Agent::new_with_config(config);

        let response = agent.get(switcher_url).call().map_err(|e| {
            error!("Failed to fetch VPN locations: {e}");
            e.to_string()
        })?;

        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok(parse_locations(&body))
    }

    /// POST to the switcher URL to change the active VPN location.
    /// Returns the confirmed active location label from the server response.
    ///
    /// ureq converts POST to GET when following 301/302 redirects, which
    /// causes PHP to see REQUEST_METHOD=GET and skip the switch handler.
    /// We disable automatic redirect following and re-POST manually so the
    /// body is preserved across the full redirect chain (e.g.
    /// http://host → http://host/ → https://host/).
    pub fn switch_location(switcher_url: &str, location_value: &str) -> Result<String, String> {
        let tls = ureq::tls::TlsConfig::builder()
            .disable_verification(true)
            .build();

        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(45)))
            .build();
        let agent = ureq::Agent::new_with_config(config);

        let mut url = ensure_trailing_slash(switcher_url);

        for hop in 0..5u8 {
            log::info!("switch_location: POST to {url} (hop {hop}) location={location_value}");

            match agent.post(&url).send_form([("location", location_value)]) {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    log::info!("switch_location: HTTP {status}");

                    // With max_redirects(0), ureq returns 3xx as Ok - handle manually.
                    if (301..=308).contains(&status) {
                        match response
                            .headers()
                            .get("location")
                            .and_then(|v| v.to_str().ok())
                        {
                            Some(loc) => {
                                log::info!("switch_location: redirect {status} → {loc}");
                                url = resolve_redirect(&url, loc);
                                continue;
                            }
                            None => {
                                return Err(format!("Redirect {status} with no Location header"))
                            }
                        }
                    }

                    let html = response
                        .body_mut()
                        .read_to_string()
                        .map_err(|e| e.to_string())?;

                    if html.contains("class='error'") || html.contains("class=\"error\"") {
                        let msg = extract_php_error(&html).unwrap_or_else(|| {
                            "Backend reported an error during location switch".to_string()
                        });
                        error!("Switch location backend error: {msg}");
                        return Err(msg);
                    }

                    let (_, current) = parse_locations(&html);
                    return match current {
                        Some(label) => Ok(label),
                        None => {
                            warn!("Switch POST succeeded but no active location in response HTML");
                            Err("Switch did not appear to take effect (no active location in response)".to_string())
                        }
                    };
                }
                Err(e) => {
                    error!("switch_location failed: {e}");
                    return Err(format!("Failed to switch location: {e}"));
                }
            }
        }

        Err("Too many redirects following switch POST".to_string())
    }
}

/// Add a trailing slash if the URL has no path component so Apache doesn't
/// issue a 301 that ureq would follow as a GET.
/// A bare `http://host` has 2 slashes (the scheme `://`); 3+ means a path
/// is already present (e.g. `http://host/`).
pub fn ensure_trailing_slash(url: &str) -> String {
    if url.matches('/').count() < 3 {
        format!("{}/", url.trim_end_matches('/'))
    } else {
        url.to_string()
    }
}

/// Resolve a redirect Location header value against the current URL.
pub fn resolve_redirect(current: &str, location: &str) -> String {
    if location.starts_with("http://") || location.starts_with("https://") {
        location.to_string()
    } else if location.starts_with('/') {
        // Absolute path - keep scheme+host from current URL.
        if let Some(idx) = current.find("://") {
            let rest = &current[idx + 3..];
            let host_end = rest.find('/').unwrap_or(rest.len());
            let base = &current[..idx + 3 + host_end];
            format!("{base}{location}")
        } else {
            location.to_string()
        }
    } else {
        location.to_string()
    }
}

/// Extract the text content of `<p class='error'><strong>...</strong></p>` from
/// the PHP backend's HTML response. Returns `None` if not present.
pub fn extract_php_error(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let sel = Selector::parse(".error strong").ok()?;
    let text: String = doc.select(&sel).next()?.text().collect();
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub fn parse_locations(html: &str) -> (Vec<Location>, Option<String>) {
    let document = Html::parse_document(html);

    let dropdown_sel = Selector::parse(".dropdown-content > *").unwrap();
    let mut locations = Vec::new();
    let mut current_location: Option<String> = None;
    let mut current_continent: Option<String> = None;

    for element in document.select(&dropdown_sel) {
        let classes: Vec<&str> = element
            .value()
            .attr("class")
            .unwrap_or("")
            .split_whitespace()
            .collect();

        if classes.contains(&"optgroup-label") {
            let text = element.text().collect::<String>();
            let continent = strip_continent_emojis(text.trim());
            current_continent = Some(continent);
        } else if classes.contains(&"dropdown-item") {
            let value = element.value().attr("data-value").unwrap_or("").to_string();
            let raw_label: String = element.text().collect::<String>();
            let label = strip_country_emojis(raw_label.trim());
            let is_active = classes.contains(&"active");

            if let Some(ref continent) = current_continent {
                if !value.is_empty() {
                    let country_name = label
                        .split(" - ")
                        .next()
                        .unwrap_or(&label)
                        .to_lowercase()
                        .trim()
                        .to_string();

                    let loc = Location {
                        continent: continent.clone(),
                        value,
                        label: label.clone(),
                        country: country_name,
                    };

                    if is_active {
                        current_location = Some(label);
                    }
                    locations.push(loc);
                }
            }
        }
    }

    (locations, current_location)
}

pub fn strip_continent_emojis(s: &str) -> String {
    // Remove common continent emoji prefixes (multi-byte Unicode)
    let emojis = ["🌍", "🌎", "🌏", "🌐"];
    let mut result = s.to_string();
    for e in &emojis {
        result = result.replace(e, "");
    }
    result.trim().to_string()
}

pub fn strip_country_emojis(s: &str) -> String {
    // Country flag emojis are regional indicator pairs (U+1F1xx).
    // Filter them out character by character.
    s.chars()
        .filter(|c| {
            let cp = *c as u32;
            // Regional indicator symbols: 0x1F1E6..0x1F1FF
            !(0x1F1E6..=0x1F1FF).contains(&cp)
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Map a lowercase country name to its ISO 3166-1 alpha-2 code.
/// Returns None if unknown.
pub fn country_to_iso(name: &str) -> Option<&'static str> {
    let name = name.trim();
    match name {
        "usa" => Some("us"),
        "uk" => Some("gb"),
        "south korea" => Some("kr"),
        "russia" => Some("ru"),
        "czech republic" => Some("cz"),
        "north macedonia" => Some("mk"),
        "moldova" => Some("md"),
        "laos" => Some("la"),
        "vietnam" => Some("vn"),
        "tanzania" => Some("tz"),
        "bolivia" => Some("bo"),
        "venezuela" => Some("ve"),
        "iran" => Some("ir"),
        "syria" => Some("sy"),
        "brunei" => Some("bn"),
        "cape verde" => Some("cv"),
        "congo" => Some("cg"),
        "democratic republic of the congo" => Some("cd"),
        "swaziland" => Some("sz"),
        "timor-leste" => Some("tl"),
        "vatican city" => Some("va"),
        "palestine" => Some("ps"),
        "taiwan" => Some("tw"),
        "hong kong" => Some("hk"),
        "macau" => Some("mo"),
        "india" => Some("in"),
        "india via singapore" => Some("in"),
        "india via uk" => Some("in"),
        "germany" => Some("de"),
        "france" => Some("fr"),
        "spain" => Some("es"),
        "italy" => Some("it"),
        "netherlands" => Some("nl"),
        "sweden" => Some("se"),
        "norway" => Some("no"),
        "denmark" => Some("dk"),
        "finland" => Some("fi"),
        "switzerland" => Some("ch"),
        "austria" => Some("at"),
        "belgium" => Some("be"),
        "poland" => Some("pl"),
        "portugal" => Some("pt"),
        "romania" => Some("ro"),
        "hungary" => Some("hu"),
        "bulgaria" => Some("bg"),
        "serbia" => Some("rs"),
        "croatia" => Some("hr"),
        "ukraine" => Some("ua"),
        "greece" => Some("gr"),
        "turkey" => Some("tr"),
        "israel" => Some("il"),
        "japan" => Some("jp"),
        "china" => Some("cn"),
        "australia" => Some("au"),
        "canada" => Some("ca"),
        "brazil" => Some("br"),
        "mexico" => Some("mx"),
        "argentina" => Some("ar"),
        "singapore" => Some("sg"),
        "thailand" => Some("th"),
        "indonesia" => Some("id"),
        "malaysia" => Some("my"),
        "philippines" => Some("ph"),
        "new zealand" => Some("nz"),
        "south africa" => Some("za"),
        "egypt" => Some("eg"),
        "nigeria" => Some("ng"),
        "kenya" => Some("ke"),
        "iraq" => Some("iq"),
        "saudi arabia" => Some("sa"),
        "uae" => Some("ae"),
        "pakistan" => Some("pk"),
        "bangladesh" => Some("bd"),
        "albania" => Some("al"),
        "algeria" => Some("dz"),
        "andorra" => Some("ad"),
        "armenia" => Some("am"),
        "azerbaijan" => Some("az"),
        "bahamas" => Some("bs"),
        "belarus" => Some("by"),
        "bermuda" => Some("bm"),
        "bhutan" => Some("bt"),
        "bosnia and herzegovina" => Some("ba"),
        "cambodia" => Some("kh"),
        "cayman islands" => Some("ky"),
        "chile" => Some("cl"),
        "colombia" => Some("co"),
        "costa rica" => Some("cr"),
        "cuba" => Some("cu"),
        "cyprus" => Some("cy"),
        "dominican republic" => Some("do"),
        "ecuador" => Some("ec"),
        "estonia" => Some("ee"),
        "georgia" => Some("ge"),
        "ghana" => Some("gh"),
        "guam" => Some("gu"),
        "guatemala" => Some("gt"),
        "honduras" => Some("hn"),
        "iceland" => Some("is"),
        "ireland" => Some("ie"),
        "isle of man" => Some("im"),
        "jamaica" => Some("jm"),
        "jersey" => Some("je"),
        "kazakhstan" => Some("kz"),
        "latvia" => Some("lv"),
        "lebanon" => Some("lb"),
        "liechtenstein" => Some("li"),
        "lithuania" => Some("lt"),
        "luxembourg" => Some("lu"),
        "malta" => Some("mt"),
        "monaco" => Some("mc"),
        "mongolia" => Some("mn"),
        "montenegro" => Some("me"),
        "morocco" => Some("ma"),
        "myanmar" => Some("mm"),
        "nepal" => Some("np"),
        "panama" => Some("pa"),
        "peru" => Some("pe"),
        "puerto rico" => Some("pr"),
        "slovakia" => Some("sk"),
        "slovenia" => Some("si"),
        "sri lanka" => Some("lk"),
        "trinidad and tobago" => Some("tt"),
        "uruguay" => Some("uy"),
        "uzbekistan" => Some("uz"),
        "united states" => Some("us"),
        "united kingdom" => Some("gb"),
        "south korea (korea, republic of)" => Some("kr"),
        _ => None,
    }
}
