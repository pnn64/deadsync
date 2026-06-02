//! User pad-config profiles (storage only).
//!
//! Each local profile can store several named pad configs in `padconfig.ini`
//! in its profile directory. A config holds a name, an optional pad **serial**
//! it was saved from, an `is_default` flag, and the threshold data as an opaque
//! hex blob (encoded/decoded by the engine layer — this module stays free of
//! `engine`/`config` dependencies per the architecture boundaries).

use crate::game::profile::local_profile_dir_for_id;
use log::warn;
use std::fmt::Write as _;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadConfigProfile {
    pub name: String,
    /// The pad serial this config was saved from (soft binding); `None` = generic.
    pub serial: Option<String>,
    pub is_default: bool,
    /// Threshold data as a hex blob (see `engine::smx::PadConfigData`).
    pub data_hex: String,
}

fn padconfig_path(profile_id: &str) -> PathBuf {
    local_profile_dir_for_id(profile_id).join("padconfig.ini")
}

/// Load all saved pad configs for a profile (empty if none / unreadable).
pub fn load(profile_id: &str) -> Vec<PadConfigProfile> {
    match std::fs::read_to_string(padconfig_path(profile_id)) {
        Ok(content) => parse(&content),
        Err(_) => Vec::new(),
    }
}

/// Persist the full set of pad configs for a profile.
pub fn save(profile_id: &str, profiles: &[PadConfigProfile]) {
    let path = padconfig_path(profile_id);
    if let Err(e) = std::fs::write(&path, serialize(profiles)) {
        warn!("Failed to save {}: {e}", path.display());
    }
}

/// Insert or replace a config by name. When `is_default`, clears the flag on the
/// others. Re-saving an existing config keeps its current default status unless
/// `is_default` is set. Empty names are ignored.
pub fn upsert(
    profile_id: &str,
    name: &str,
    serial: Option<String>,
    is_default: bool,
    data_hex: String,
) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut list = load(profile_id);
    if is_default {
        for p in &mut list {
            p.is_default = false;
        }
    }
    if let Some(existing) = list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
        existing.serial = serial;
        existing.is_default = is_default || existing.is_default;
        existing.data_hex = data_hex;
    } else {
        list.push(PadConfigProfile {
            name: name.to_string(),
            serial,
            is_default,
            data_hex,
        });
    }
    save(profile_id, &list);
}

/// Pick the config to apply for a pad: the serial-matching one first, else the
/// profile's default.
pub fn resolve<'a>(profiles: &'a [PadConfigProfile], serial: &str) -> Option<&'a PadConfigProfile> {
    profiles
        .iter()
        .find(|p| p.serial.as_deref() == Some(serial))
        .or_else(|| profiles.iter().find(|p| p.is_default))
}

fn serialize(profiles: &[PadConfigProfile]) -> String {
    let mut content = String::new();
    for (i, p) in profiles.iter().enumerate() {
        let _ = writeln!(content, "[PadProfile{i}]");
        let _ = writeln!(content, "Name={}", p.name);
        let _ = writeln!(content, "Serial={}", p.serial.as_deref().unwrap_or(""));
        let _ = writeln!(content, "Default={}", u8::from(p.is_default));
        let _ = writeln!(content, "Data={}", p.data_hex);
        content.push('\n');
    }
    content
}

fn parse(content: &str) -> Vec<PadConfigProfile> {
    let mut out = Vec::new();
    let mut in_section = false;
    let mut name = String::new();
    let mut serial = String::new();
    let mut default = false;
    let mut data_hex = String::new();

    let mut flush = |name: &mut String, serial: &mut String, default: &mut bool, data: &mut String, out: &mut Vec<PadConfigProfile>| {
        if !name.trim().is_empty() && !data.trim().is_empty() {
            out.push(PadConfigProfile {
                name: std::mem::take(name).trim().to_string(),
                serial: {
                    let s = serial.trim();
                    if s.is_empty() { None } else { Some(s.to_string()) }
                },
                is_default: *default,
                data_hex: std::mem::take(data).trim().to_string(),
            });
        }
        name.clear();
        serial.clear();
        *default = false;
        data.clear();
    };

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if in_section {
                flush(&mut name, &mut serial, &mut default, &mut data_hex, &mut out);
            }
            in_section = line[1..line.len() - 1].trim().starts_with("PadProfile");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            let val = line[eq + 1..].trim();
            match key {
                "Name" => name = val.to_string(),
                "Serial" => serial = val.to_string(),
                "Default" => default = val == "1",
                "Data" => data_hex = val.to_string(),
                _ => {}
            }
        }
    }
    if in_section {
        flush(&mut name, &mut serial, &mut default, &mut data_hex, &mut out);
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str, serial: Option<&str>, is_default: bool, data_hex: &str) -> PadConfigProfile {
        PadConfigProfile {
            name: name.to_string(),
            serial: serial.map(str::to_owned),
            is_default,
            data_hex: data_hex.to_string(),
        }
    }

    #[test]
    fn serialize_parse_round_trips() {
        let profiles = vec![
            sample("Alpha", Some("40ea1234"), true, "deadbeef"),
            sample("Beta", None, false, "0011223344"),
        ];
        // parse sorts by name; inputs are already alphabetical.
        assert_eq!(parse(&serialize(&profiles)), profiles);
    }

    #[test]
    fn parse_skips_entries_without_name_or_data() {
        let content = "[PadProfile0]\nName=Only\n\n[PadProfile1]\nName=Good\nData=abcd\n";
        let parsed = parse(content);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Good");
    }

    #[test]
    fn resolve_prefers_serial_then_default() {
        let profiles = vec![
            sample("Default", None, true, "00"),
            sample("PadB", Some("serialB"), false, "11"),
        ];
        assert_eq!(resolve(&profiles, "serialB").unwrap().name, "PadB");
        assert_eq!(resolve(&profiles, "unknown").unwrap().name, "Default");
        let no_default = vec![sample("X", Some("s"), false, "22")];
        assert!(resolve(&no_default, "other").is_none());
    }
}
