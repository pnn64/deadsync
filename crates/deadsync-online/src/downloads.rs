use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const ITL_UNLOCK_PACK_YEAR: u32 = 2026;
const INVALID_PACK_CHARS: [char; 9] = ['/', '<', '>', ':', '"', '\\', '|', '?', '*'];
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub type UnlockCache = HashMap<String, HashMap<String, bool>>;

#[derive(Clone, Debug)]
pub struct DownloadSnapshot {
    pub name: String,
    pub current_bytes: u64,
    pub total_bytes: u64,
    pub complete: bool,
    pub error_message: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UnlockCacheFile(pub UnlockCache);

pub fn sanitize_pack_name(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if INVALID_PACK_CHARS.contains(&ch) {
            continue;
        }
        sanitized.push(ch);
    }
    if sanitized.trim().is_empty() {
        sanitized = "Unlocks".to_string();
    }
    if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(sanitized.trim()))
    {
        return format!(" {} ", sanitized.trim());
    }
    sanitized
}

pub fn mime_token(value: &str) -> &str {
    value.split(';').next().unwrap_or("").trim()
}

pub fn cache_has_destination(cache: &UnlockCache, url: &str, destination: &str) -> bool {
    cache
        .get(url)
        .and_then(|packs| packs.get(destination))
        .copied()
        .unwrap_or(false)
}

pub fn itl_unlock_pack_ini_content(pack_name: &str) -> Option<String> {
    let lower = pack_name.to_ascii_lowercase();
    if !lower.contains(&format!("itl online {ITL_UNLOCK_PACK_YEAR} unlocks")) {
        return None;
    }
    Some(format!(
        "[Group]\nVersion=1\nDisplayTitle={pack_name}\nTranslitTitle={pack_name}\nSortTitle={pack_name}\nSeries=ITL Online\nYear={ITL_UNLOCK_PACK_YEAR}\nBanner=\nSyncOffset=NULL\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_pack_name_strips_invalid_chars() {
        assert_eq!(sanitize_pack_name("ITL/Unlocks:*?"), "ITLUnlocks");
    }

    #[test]
    fn sanitize_pack_name_avoids_windows_reserved_names() {
        assert_eq!(sanitize_pack_name("CON"), " CON ");
    }

    #[test]
    fn sanitize_pack_name_falls_back_when_empty() {
        assert_eq!(sanitize_pack_name("///"), "Unlocks");
    }

    #[test]
    fn mime_token_strips_parameters() {
        assert_eq!(
            mime_token("application/zip; charset=binary"),
            "application/zip"
        );
    }

    #[test]
    fn cache_has_destination_reads_nested_success_flag() {
        let mut cache = UnlockCache::new();
        cache
            .entry("https://example.com/unlock.zip".to_string())
            .or_default()
            .insert("ITL Unlocks".to_string(), true);

        assert!(cache_has_destination(
            &cache,
            "https://example.com/unlock.zip",
            "ITL Unlocks"
        ));
        assert!(!cache_has_destination(
            &cache,
            "https://example.com/unlock.zip",
            "Other Pack"
        ));
    }

    #[test]
    fn itl_unlock_pack_ini_content_matches_pack_ini_shape() {
        let content =
            itl_unlock_pack_ini_content("ITL Online 2026 Unlocks").expect("pack ini content");
        assert!(content.contains("DisplayTitle=ITL Online 2026 Unlocks"));
        assert!(content.contains("Series=ITL Online"));
        assert!(content.contains("Year=2026"));
    }

    #[test]
    fn itl_unlock_pack_ini_content_skips_other_packs() {
        assert!(itl_unlock_pack_ini_content("Other Pack").is_none());
    }
}
