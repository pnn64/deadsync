use deadsync_net as network;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadZipError {
    PrepareDir(String),
    Request(String),
    HttpStatus(u16),
    Io(String),
    NotZip { content_type: String },
}

impl Display for DownloadZipError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrepareDir(message) => write!(f, "Failed to prepare Downloads dir: {message}"),
            Self::Request(message) | Self::Io(message) => f.write_str(message),
            Self::HttpStatus(status) => write!(f, "Network Error {status}"),
            Self::NotZip { .. } => f.write_str("Download is not a Zip!"),
        }
    }
}

impl Error for DownloadZipError {}

pub fn download_zip_to_path<F>(
    url: &str,
    zip_path: &Path,
    mut report_progress: F,
) -> Result<(), DownloadZipError>
where
    F: FnMut(u64, u64),
{
    if let Some(parent) = zip_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| DownloadZipError::PrepareDir(error.to_string()))?;
    }

    let response = network::get_agent()
        .get(url)
        .call()
        .map_err(|error| DownloadZipError::Request(error.to_string()))?;
    let status = response.status().as_u16();
    if status != 200 {
        return Err(DownloadZipError::HttpStatus(status));
    }

    let content_type = response
        .headers()
        .get("Content-Type")
        .and_then(|value| value.to_str().ok())
        .map(|value| mime_token(value).to_string())
        .unwrap_or_default();
    let total_bytes = response
        .headers()
        .get("Content-Length")
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.parse::<u64>().ok())
        .unwrap_or(0);
    report_progress(0, total_bytes);

    let mut file =
        File::create(zip_path).map_err(|error| DownloadZipError::Io(error.to_string()))?;
    let mut body = response.into_body();
    let mut reader = body.as_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let read = reader
            .read(&mut buf)
            .map_err(|error| DownloadZipError::Io(error.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])
            .map_err(|error| DownloadZipError::Io(error.to_string()))?;
        downloaded = downloaded.saturating_add(read as u64);
        report_progress(downloaded, total_bytes);
    }
    report_progress(downloaded, total_bytes.max(downloaded));

    if content_type.as_str() != "application/zip" {
        return Err(DownloadZipError::NotZip { content_type });
    }

    Ok(())
}

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

    #[test]
    fn download_zip_error_preserves_download_messages() {
        assert_eq!(
            DownloadZipError::PrepareDir("denied".to_string()).to_string(),
            "Failed to prepare Downloads dir: denied"
        );
        assert_eq!(
            DownloadZipError::HttpStatus(404).to_string(),
            "Network Error 404"
        );
        assert_eq!(
            DownloadZipError::NotZip {
                content_type: "text/html".to_string()
            }
            .to_string(),
            "Download is not a Zip!"
        );
    }
}
