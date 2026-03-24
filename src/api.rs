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

use log::error;
use scraper::{Html, Selector};

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
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string())?;

        let agent = ureq::AgentBuilder::new()
            .tls_connector(std::sync::Arc::new(tls))
            .build();

        let response = agent
            .get(switcher_url)
            .timeout(std::time::Duration::from_secs(5))
            .call()
            .map_err(|e| {
                error!("Failed to fetch VPN locations: {e}");
                e.to_string()
            })?;

        let body = response.into_string().map_err(|e| e.to_string())?;
        Ok(parse_locations(&body))
    }

    /// POST to the switcher URL to change the active VPN location.
    pub fn switch_location(switcher_url: &str, location_value: &str) -> Result<(), String> {
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| e.to_string())?;

        let agent = ureq::AgentBuilder::new()
            .tls_connector(std::sync::Arc::new(tls))
            .build();

        let body = format!("location={}", urlencoded(location_value));

        agent
            .post(switcher_url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&body)
            .map_err(|e| {
                error!("Failed to switch location: {e}");
                format!("Failed to switch location: {e}")
            })?;

        Ok(())
    }
}

pub fn urlencoded(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            c => {
                let mut buf = [0u8; 4];
                let bytes = c.encode_utf8(&mut buf).as_bytes();
                bytes.iter().map(|b| format!("%{:02X}", b)).collect()
            }
        })
        .collect()
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
