// locale.rs - DragonFoxVPN: Localization helpers
// Copyright (c) 2026 Senjin the Dragon.
// https://github.com/senjinthedragon/DragonFoxVPN
// Licensed under the MIT License.
// See LICENSE for full license information.
//
// Locale files are embedded at compile time via include_str!.
// The English file acts as the base (complete); every other language only needs
// to contain the keys it actually translates - missing keys fall back to English.
// Call init() once at startup before any t() / t_fmt() calls.

use std::collections::HashMap;
use std::sync::OnceLock;

static DETECTED_LANG: OnceLock<String> = OnceLock::new();
static TRANSLATIONS: OnceLock<HashMap<String, String>> = OnceLock::new();

const EN: &str = include_str!("../locales/en.json");
const DE: &str = include_str!("../locales/de.json");
const FR: &str = include_str!("../locales/fr.json");
const ES: &str = include_str!("../locales/es.json");
const PT_BR: &str = include_str!("../locales/pt_BR.json");
const IT: &str = include_str!("../locales/it.json");
const RU: &str = include_str!("../locales/ru.json");
const ZH_CN: &str = include_str!("../locales/zh_CN.json");
const JA: &str = include_str!("../locales/ja.json");
const KO: &str = include_str!("../locales/ko.json");

fn json_for_lang(lang: &str) -> &'static str {
    match lang {
        "de" => DE,
        "fr" => FR,
        "es" => ES,
        "pt_BR" => PT_BR,
        "it" => IT,
        "ru" => RU,
        "zh_CN" => ZH_CN,
        "ja" => JA,
        "ko" => KO,
        _ => EN,
    }
}

/// Returns the system locale string (e.g. "de_DE.UTF-8" or "zh-CN").
/// On Linux reads the standard LC_ALL / LANG / LANGUAGE env vars.
/// On Windows reads LocaleName from HKCU\Control Panel\International via
/// the winreg crate (already a dependency for dark-mode detection).
fn system_locale() -> String {
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Control Panel\\International")
        {
            if let Ok(name) = key.get_value::<String, _>("LocaleName") {
                return name; // e.g. "de-DE" or "zh-CN"
            }
        }
    }
    // Linux / macOS: honour the standard locale env vars in priority order.
    for var in &["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() && val != "C" && val != "POSIX" {
                return val;
            }
        }
    }
    String::new()
}

fn detected_language() -> &'static str {
    DETECTED_LANG.get_or_init(|| {
        let locale = system_locale().to_lowercase();
        if locale.starts_with("zh") {
            "zh_CN".to_string()
        } else if locale.starts_with("ja") {
            "ja".to_string()
        } else if locale.starts_with("ko") {
            "ko".to_string()
        } else if locale.starts_with("de") {
            "de".to_string()
        } else if locale.starts_with("fr") {
            "fr".to_string()
        } else if locale.starts_with("es") {
            "es".to_string()
        } else if locale.starts_with("pt") {
            "pt_BR".to_string()
        } else if locale.starts_with("ru") {
            "ru".to_string()
        } else if locale.starts_with("it") {
            "it".to_string()
        } else {
            "en".to_string()
        }
    })
}

/// Initialize the translation table. Call once at startup before any t() calls.
pub fn init() {
    // Start with every English key as a baseline fallback.
    let mut map: HashMap<String, String> =
        serde_json::from_str(EN).unwrap_or_default();
    let lang = detected_language();
    if lang != "en" {
        let translated: HashMap<String, String> =
            serde_json::from_str(json_for_lang(lang)).unwrap_or_default();
        map.extend(translated);
    }
    TRANSLATIONS.set(map).ok();
}

/// Look up a locale key. Returns the key itself if not found (never panics).
pub fn t(key: &str) -> String {
    TRANSLATIONS
        .get()
        .and_then(|m| m.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Look up a locale key and substitute named placeholders.
/// Placeholders are written as `{name}` in the JSON values.
/// Example: t_fmt("tray.connected_location", &[("location", "Belgium")])
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = t(key);
    for (name, value) in args {
        s = s.replace(&format!("{{{name}}}"), value);
    }
    s
}

/// If the active language requires CJK characters (Chinese, Japanese, Korean),
/// attempt to load a system CJK font and register it with egui so the
/// characters render correctly. On systems without such a font this is a no-op.
pub fn apply_cjk_font_if_needed(ctx: &egui::Context) {
    let lang = detected_language();
    if !matches!(lang, "zh_CN" | "ja" | "ko") {
        return;
    }
    if let Some(path) = find_cjk_font(lang) {
        if let Ok(bytes) = std::fs::read(&path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("cjk".to_owned());
            ctx.set_fonts(fonts);
        }
    }
}

fn find_cjk_font(lang: &str) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let windir =
            std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let fonts_dir = std::path::Path::new(&windir).join("Fonts");
        let candidates: &[&str] = match lang {
            "zh_CN" => &["msyh.ttc", "msyhbd.ttc", "simhei.ttf", "simsun.ttc"],
            "ja" => &["meiryo.ttc", "YuGothR.ttc", "msgothic.ttc"],
            "ko" => &["malgun.ttf", "gulim.ttc"],
            _ => &[],
        };
        for name in candidates {
            let p = fonts_dir.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let candidates: &[&str] = match lang {
            "zh_CN" => &[
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJKsc-Regular.otf",
                "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            ],
            "ja" => &[
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJKjp-Regular.otf",
            ],
            "ko" => &[
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJKkr-Regular.otf",
            ],
            _ => &[],
        };
        for path in candidates {
            let p = std::path::Path::new(path);
            if p.exists() {
                return Some(p.to_path_buf());
            }
        }
    }
    None
}
