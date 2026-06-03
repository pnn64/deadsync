//! User pad-config profiles (storage only).
//!
//! Each local profile can store several named pad configs in `padconfig.ini`
//! in its profile directory. A config holds a name, the **backend** it was saved
//! for (e.g. `smx`), an optional **pad type** (e.g. `fsr` / `loadcell`), an
//! optional pad **serial** it was saved from, an `is_default` flag, and the
//! threshold values as an opaque, human-readable key/value `settings` bag
//! (encoded/decoded by the engine layer — this module stays free of
//! `engine`/`config` dependencies per the architecture boundaries, so it never
//! interprets `settings`).

use crate::game::profile::local_profile_dir_for_id;
use log::warn;
use std::fmt::Write as _;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadConfigProfile {
    pub name: String,
    /// Backend the config targets (e.g. `"smx"`); configs only apply to a
    /// matching backend. Opaque to this module.
    pub backend: String,
    /// Pad sensor type the config was tuned for (e.g. `"fsr"` / `"loadcell"`);
    /// `None` = unspecified (applies to any pad of the backend).
    pub pad_type: Option<String>,
    /// The pad serial this config was saved from (soft binding); `None` = generic.
    pub serial: Option<String>,
    pub is_default: bool,
    /// Threshold values as a human-readable key/value list. Opaque here; the
    /// engine layer owns the schema (see `engine::smx::PadConfigData`).
    pub settings: Vec<(String, String)>,
}

/// Reserved section keys that map to struct fields rather than `settings`.
const META_KEYS: [&str; 5] = ["Name", "Backend", "PadType", "Serial", "Default"];

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
#[allow(clippy::too_many_arguments)]
pub fn upsert(
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    is_default: bool,
    settings: Vec<(String, String)>,
) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut list = load(profile_id);
    if is_default {
        // Default is scoped per pad (serial group): only clear the flag on other
        // configs that share this config's serial, so each pad keeps its own.
        for p in &mut list {
            if p.serial == serial {
                p.is_default = false;
            }
        }
    }
    if let Some(existing) = list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
        existing.backend = backend.to_string();
        existing.pad_type = pad_type;
        existing.serial = serial;
        existing.is_default = is_default || existing.is_default;
        existing.settings = settings;
    } else {
        list.push(PadConfigProfile {
            name: name.to_string(),
            backend: backend.to_string(),
            pad_type,
            serial,
            is_default,
            settings,
        });
    }
    save(profile_id, &list);
}

/// Mark one config as the profile's default (clearing it on the others). No-op
/// if the name isn't found.
pub fn set_default(profile_id: &str, name: &str) {
    let mut list = load(profile_id);
    if apply_set_default(&mut list, name) {
        save(profile_id, &list);
    }
}

/// Rename a config. No-op if `old` is missing, `new` is blank, or `new` already
/// names a different config.
pub fn rename(profile_id: &str, old: &str, new: &str) {
    let mut list = load(profile_id);
    if apply_rename(&mut list, old, new) {
        save(profile_id, &list);
    }
}

/// Delete a config by name. No-op if the name isn't found.
pub fn delete(profile_id: &str, name: &str) {
    let mut list = load(profile_id);
    if apply_delete(&mut list, name) {
        save(profile_id, &list);
    }
}

// Pure list mutations (testable without touching the filesystem). Each returns
// whether the list changed.
fn apply_set_default(list: &mut [PadConfigProfile], name: &str) -> bool {
    // Default is scoped per pad (serial group): mark `name` and clear the flag
    // only on other configs sharing its serial, so other pads keep their own.
    let Some(group) = list
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .map(|p| p.serial.clone())
    else {
        return false;
    };
    for p in list.iter_mut() {
        if p.serial == group {
            p.is_default = p.name.eq_ignore_ascii_case(name);
        }
    }
    true
}

fn apply_rename(list: &mut [PadConfigProfile], old: &str, new: &str) -> bool {
    let new = new.trim();
    if new.is_empty()
        || !list.iter().any(|p| p.name.eq_ignore_ascii_case(old))
        || list
            .iter()
            .any(|p| !p.name.eq_ignore_ascii_case(old) && p.name.eq_ignore_ascii_case(new))
    {
        return false;
    }
    if let Some(p) = list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(old)) {
        p.name = new.to_string();
    }
    true
}

fn apply_delete(list: &mut Vec<PadConfigProfile>, name: &str) -> bool {
    let before = list.len();
    list.retain(|p| !p.name.eq_ignore_ascii_case(name));
    list.len() != before
}

/// Whether a stored config is compatible with a pad of the given `backend` and
/// `pad_type`. A typed config only mismatches when both types are known and
/// differ (an unknown pad type is treated optimistically, so a config still
/// resolves while the pad's config is briefly unavailable).
pub fn config_matches(profile: &PadConfigProfile, backend: &str, pad_type: Option<&str>) -> bool {
    profile.backend == backend
        && match (profile.pad_type.as_deref(), pad_type) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

/// Pick the config to apply for a pad. Among configs compatible with the pad's
/// `backend` + `pad_type`, in order: this pad's serial group's **default**, then
/// any config matching this serial, then a serial-less **global default**
/// (a config bound to no pad, e.g. one the user hand-edited to drop its serial).
/// Defaults are per-serial, so another pad's default never applies here.
pub fn resolve<'a>(
    profiles: &'a [PadConfigProfile],
    backend: &str,
    pad_type: Option<&str>,
    serial: &str,
) -> Option<&'a PadConfigProfile> {
    let compatible = |p: &&PadConfigProfile| config_matches(p, backend, pad_type);
    let serial_match = |p: &&PadConfigProfile| p.serial.as_deref() == Some(serial);

    profiles
        .iter()
        .filter(compatible)
        .filter(serial_match)
        .find(|p| p.is_default)
        .or_else(|| profiles.iter().filter(compatible).find(serial_match))
        .or_else(|| {
            profiles
                .iter()
                .filter(compatible)
                .find(|p| p.serial.is_none() && p.is_default)
        })
}

fn serialize(profiles: &[PadConfigProfile]) -> String {
    let mut content = String::new();
    for (i, p) in profiles.iter().enumerate() {
        let _ = writeln!(content, "[PadProfile{i}]");
        let _ = writeln!(content, "Name={}", p.name);
        let _ = writeln!(content, "Backend={}", p.backend);
        let _ = writeln!(content, "PadType={}", p.pad_type.as_deref().unwrap_or(""));
        let _ = writeln!(content, "Serial={}", p.serial.as_deref().unwrap_or(""));
        let _ = writeln!(content, "Default={}", u8::from(p.is_default));
        for (k, v) in &p.settings {
            let _ = writeln!(content, "{k}={v}");
        }
        content.push('\n');
    }
    content
}

fn parse(content: &str) -> Vec<PadConfigProfile> {
    let mut out = Vec::new();
    let mut in_section = false;
    let mut name = String::new();
    let mut backend = String::new();
    let mut pad_type = String::new();
    let mut serial = String::new();
    let mut default = false;
    let mut settings: Vec<(String, String)> = Vec::new();

    let flush = |name: &mut String,
                 backend: &mut String,
                 pad_type: &mut String,
                 serial: &mut String,
                 default: &mut bool,
                 settings: &mut Vec<(String, String)>,
                 out: &mut Vec<PadConfigProfile>| {
        // Require the identifying meta and at least one setting; otherwise drop.
        if !name.trim().is_empty() && !backend.trim().is_empty() && !settings.is_empty() {
            out.push(PadConfigProfile {
                name: std::mem::take(name).trim().to_string(),
                backend: std::mem::take(backend).trim().to_string(),
                pad_type: {
                    let s = pad_type.trim();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                },
                serial: {
                    let s = serial.trim();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                },
                is_default: *default,
                settings: std::mem::take(settings),
            });
        }
        name.clear();
        backend.clear();
        pad_type.clear();
        serial.clear();
        *default = false;
        settings.clear();
    };

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if in_section {
                flush(
                    &mut name,
                    &mut backend,
                    &mut pad_type,
                    &mut serial,
                    &mut default,
                    &mut settings,
                    &mut out,
                );
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
                "Backend" => backend = val.to_string(),
                "PadType" => pad_type = val.to_string(),
                "Serial" => serial = val.to_string(),
                "Default" => default = val == "1",
                // Anything that isn't reserved meta is an opaque setting.
                _ if !META_KEYS.contains(&key) => settings.push((key.to_string(), val.to_string())),
                _ => {}
            }
        }
    }
    if in_section {
        flush(
            &mut name,
            &mut backend,
            &mut pad_type,
            &mut serial,
            &mut default,
            &mut settings,
            &mut out,
        );
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        name: &str,
        backend: &str,
        pad_type: Option<&str>,
        serial: Option<&str>,
        is_default: bool,
    ) -> PadConfigProfile {
        PadConfigProfile {
            name: name.to_string(),
            backend: backend.to_string(),
            pad_type: pad_type.map(str::to_owned),
            serial: serial.map(str::to_owned),
            is_default,
            settings: vec![
                ("Panel0.FsrLow".to_string(), "152 152 152 152".to_string()),
                ("DebounceMs".to_string(), "4".to_string()),
            ],
        }
    }

    #[test]
    fn serialize_parse_round_trips() {
        let profiles = vec![
            sample("Alpha", "smx", Some("fsr"), Some("40ea1234"), true),
            sample("Beta", "smx", None, None, false),
        ];
        // parse sorts by name; inputs are already alphabetical.
        assert_eq!(parse(&serialize(&profiles)), profiles);
    }

    #[test]
    fn parse_skips_entries_missing_name_backend_or_settings() {
        let content = "\
[PadProfile0]
Name=Only
Backend=smx

[PadProfile1]
Name=NoBackend
Panel0.FsrLow=1 2 3 4

[PadProfile2]
Name=Good
Backend=smx
PadType=fsr
Panel0.FsrLow=1 2 3 4
";
        let parsed = parse(content);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Good");
        assert_eq!(parsed[0].pad_type.as_deref(), Some("fsr"));
        assert_eq!(parsed[0].settings, vec![("Panel0.FsrLow".to_string(), "1 2 3 4".to_string())]);
    }

    #[test]
    fn resolve_prefers_serial_then_default() {
        let profiles = vec![
            sample("Default", "smx", Some("fsr"), None, true),
            sample("PadB", "smx", Some("fsr"), Some("serialB"), false),
        ];
        assert_eq!(resolve(&profiles, "smx", Some("fsr"), "serialB").unwrap().name, "PadB");
        assert_eq!(resolve(&profiles, "smx", Some("fsr"), "unknown").unwrap().name, "Default");
        let no_default = vec![sample("X", "smx", Some("fsr"), Some("s"), false)];
        assert!(resolve(&no_default, "smx", Some("fsr"), "other").is_none());
    }

    #[test]
    fn resolve_filters_by_backend_and_pad_type() {
        let profiles = vec![
            sample("FsrDefault", "smx", Some("fsr"), None, true),
            sample("LoadCellDefault", "smx", Some("loadcell"), None, true),
            sample("FsrioCfg", "fsrio", None, None, true),
        ];
        // A load-cell pad must not pick the FSR default (or the fsrio config).
        assert_eq!(
            resolve(&profiles, "smx", Some("loadcell"), "x").unwrap().name,
            "LoadCellDefault"
        );
        assert_eq!(resolve(&profiles, "smx", Some("fsr"), "x").unwrap().name, "FsrDefault");
        // Wrong backend never matches.
        assert!(resolve(&profiles, "smx", Some("fsr"), "x").unwrap().backend == "smx");
        // Unknown pad type resolves optimistically (here both smx defaults are
        // is_default; the first compatible one wins).
        assert!(resolve(&profiles, "smx", None, "x").is_some());
        // An untyped config matches any pad type of its backend.
        let untyped = vec![sample("Any", "smx", None, None, true)];
        assert!(resolve(&untyped, "smx", Some("loadcell"), "x").is_some());
    }

    #[test]
    fn default_is_scoped_per_serial() {
        let mut list = vec![
            sample("A1", "smx", Some("fsr"), Some("padA"), true), // padA's default
            sample("A2", "smx", Some("fsr"), Some("padA"), false),
            sample("B1", "smx", Some("fsr"), Some("padB"), true), // padB's default
        ];
        // Making A2 the default clears A1 (same serial) but leaves B1 alone.
        assert!(apply_set_default(&mut list, "A2"));
        assert!(!list[0].is_default); // A1
        assert!(list[1].is_default); // A2
        assert!(list[2].is_default); // B1 untouched
        // Each pad resolves to its own default.
        assert_eq!(resolve(&list, "smx", Some("fsr"), "padA").unwrap().name, "A2");
        assert_eq!(resolve(&list, "smx", Some("fsr"), "padB").unwrap().name, "B1");
    }

    #[test]
    fn resolve_serial_match_default_beats_other_serial_match() {
        let list = vec![
            sample("First", "smx", Some("fsr"), Some("pad"), false),
            sample("Chosen", "smx", Some("fsr"), Some("pad"), true),
        ];
        // Two configs share the serial; the one marked default wins (not the first).
        assert_eq!(resolve(&list, "smx", Some("fsr"), "pad").unwrap().name, "Chosen");
    }

    #[test]
    fn set_default_is_exclusive_and_case_insensitive() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), None, true),
            sample("B", "smx", Some("fsr"), None, false),
        ];
        assert!(apply_set_default(&mut list, "b"));
        assert!(!list[0].is_default);
        assert!(list[1].is_default);
        assert!(!apply_set_default(&mut list, "nope"));
        assert!(list[1].is_default);
    }

    #[test]
    fn rename_guards_blank_missing_and_duplicate() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), None, false),
            sample("B", "smx", Some("fsr"), None, false),
        ];
        assert!(!apply_rename(&mut list, "A", "  ")); // blank
        assert!(!apply_rename(&mut list, "missing", "C")); // missing
        assert!(!apply_rename(&mut list, "A", "b")); // duplicate (case-insensitive)
        assert!(apply_rename(&mut list, "A", "Alpha"));
        assert_eq!(list[0].name, "Alpha");
        assert!(apply_rename(&mut list, "Alpha", "ALPHA"));
        assert_eq!(list[0].name, "ALPHA");
    }

    #[test]
    fn delete_removes_matching() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), None, false),
            sample("B", "smx", Some("fsr"), None, false),
        ];
        assert!(apply_delete(&mut list, "a"));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "B");
        assert!(!apply_delete(&mut list, "missing"));
        assert_eq!(list.len(), 1);
    }
}
