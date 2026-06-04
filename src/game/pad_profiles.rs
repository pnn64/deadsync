//! User pad-config profiles (storage only).
//!
//! Each local profile can store several named pad configs in `padconfig.ini`
//! in its profile directory. A config holds a name, the **backend** it was saved
//! for (e.g. `smx`), an optional **pad type** (e.g. `fsr` / `loadcell`), the pad
//! **serial** it was captured from (provenance), the set of pad serials it is the
//! **default** for, an optional **global default** flag, and the threshold values
//! as an opaque, human-readable key/value `settings` bag (encoded/decoded by the
//! engine layer — this module stays free of `engine`/`config` dependencies per
//! the architecture boundaries, so it never interprets `settings`).
//!
//! Defaults are **per pad**: any config can be the default for any pad (keyed by
//! that pad's serial), so two pads can point at the same or different configs.

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
    /// The pad serial this config was captured from (provenance / overwrite
    /// target); `None` = generic.
    pub serial: Option<String>,
    /// Pad serials this config is the default for. A config can be the default
    /// for several pads; each pad has at most one default config.
    pub default_for_serials: Vec<String>,
    /// Fallback default applied to a pad that has no per-pad default of its own.
    pub global_default: bool,
    /// Threshold values as a human-readable key/value list. Opaque here; the
    /// engine layer owns the schema (see `engine::smx::PadConfigData`).
    pub settings: Vec<(String, String)>,
}

/// Reserved section keys that map to struct fields rather than `settings`.
const META_KEYS: [&str; 6] = [
    "Name",
    "Backend",
    "PadType",
    "Serial",
    "DefaultFor",
    "GlobalDefault",
];

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

/// Insert or replace a config by name, preserving its existing default
/// associations. When `make_default` and the config has a serial, it also becomes
/// that pad's (serial's) default. Empty names are ignored.
#[allow(clippy::too_many_arguments)]
pub fn upsert(
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut list = load(profile_id);
    if let Some(existing) = list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
        existing.backend = backend.to_string();
        existing.pad_type = pad_type;
        existing.serial = serial.clone();
        existing.settings = settings;
        // default_for_serials / global_default are preserved across a re-save.
    } else {
        list.push(PadConfigProfile {
            name: name.to_string(),
            backend: backend.to_string(),
            pad_type,
            serial: serial.clone(),
            default_for_serials: Vec::new(),
            global_default: false,
            settings,
        });
    }
    // "Set as default" means: default for the pad it was saved on.
    if make_default && let Some(s) = serial {
        apply_set_default(&mut list, &s, name);
    }
    save(profile_id, &list);
}

/// Make `name` the default config for the pad identified by `serial` (clearing
/// that serial from every other config). No-op if the name isn't found.
pub fn set_default(profile_id: &str, serial: &str, name: &str) {
    let mut list = load(profile_id);
    if apply_set_default(&mut list, serial, name) {
        save(profile_id, &list);
    }
}

/// Rename a config. No-op if `old` is missing, `new` is blank, or `new` already
/// names a different config. Default associations travel with the config.
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
fn apply_set_default(list: &mut [PadConfigProfile], serial: &str, name: &str) -> bool {
    if !list.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
        return false;
    }
    for p in list.iter_mut() {
        let should = p.name.eq_ignore_ascii_case(name);
        p.default_for_serials.retain(|s| s != serial);
        if should {
            p.default_for_serials.push(serial.to_string());
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

/// Whether `profile` is the default config for the pad identified by `serial`.
pub fn is_default_for(profile: &PadConfigProfile, serial: &str) -> bool {
    profile.default_for_serials.iter().any(|s| s == serial)
}

/// Pick the config to apply for a pad. Among configs compatible with the pad's
/// `backend` + `pad_type`: this pad's per-pad default first, else a global
/// default. Defaults are per-serial, so another pad's default never applies here.
pub fn resolve<'a>(
    profiles: &'a [PadConfigProfile],
    backend: &str,
    pad_type: Option<&str>,
    serial: &str,
) -> Option<&'a PadConfigProfile> {
    let compatible = |p: &&PadConfigProfile| config_matches(p, backend, pad_type);
    profiles
        .iter()
        .filter(compatible)
        .find(|p| is_default_for(p, serial))
        .or_else(|| {
            profiles
                .iter()
                .filter(compatible)
                .find(|p| p.global_default)
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
        let _ = writeln!(content, "DefaultFor={}", p.default_for_serials.join(" "));
        let _ = writeln!(content, "GlobalDefault={}", u8::from(p.global_default));
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
    let mut default_for = String::new();
    let mut global_default = false;
    let mut settings: Vec<(String, String)> = Vec::new();

    #[allow(clippy::too_many_arguments)]
    fn flush(
        name: &mut String,
        backend: &mut String,
        pad_type: &mut String,
        serial: &mut String,
        default_for: &mut String,
        global_default: &mut bool,
        settings: &mut Vec<(String, String)>,
        out: &mut Vec<PadConfigProfile>,
    ) {
        // Require the identifying meta and at least one setting; otherwise drop.
        if !name.trim().is_empty() && !backend.trim().is_empty() && !settings.is_empty() {
            out.push(PadConfigProfile {
                name: std::mem::take(name).trim().to_string(),
                backend: std::mem::take(backend).trim().to_string(),
                pad_type: opt(pad_type),
                serial: opt(serial),
                default_for_serials: default_for.split_whitespace().map(str::to_string).collect(),
                global_default: *global_default,
                settings: std::mem::take(settings),
            });
        }
        name.clear();
        backend.clear();
        pad_type.clear();
        serial.clear();
        default_for.clear();
        *global_default = false;
        settings.clear();
    }

    fn opt(s: &mut String) -> Option<String> {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    }

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
                    &mut default_for,
                    &mut global_default,
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
                "DefaultFor" => default_for = val.to_string(),
                "GlobalDefault" => global_default = val == "1",
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
            &mut default_for,
            &mut global_default,
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
        default_for: &[&str],
    ) -> PadConfigProfile {
        PadConfigProfile {
            name: name.to_string(),
            backend: backend.to_string(),
            pad_type: pad_type.map(str::to_owned),
            serial: serial.map(str::to_owned),
            default_for_serials: default_for.iter().map(|s| s.to_string()).collect(),
            global_default: false,
            settings: vec![
                ("Panel0.FsrLow".to_string(), "152 152 152 152".to_string()),
                ("DebounceMs".to_string(), "4".to_string()),
            ],
        }
    }

    #[test]
    fn serialize_parse_round_trips() {
        let profiles = vec![
            sample("Alpha", "smx", Some("fsr"), Some("S1"), &["S1", "S2"]),
            sample("Beta", "smx", None, None, &[]),
        ];
        assert_eq!(parse(&serialize(&profiles)), profiles);
    }

    #[test]
    fn parse_skips_entries_missing_name_backend_or_settings() {
        let content = "\
[PadProfile0]
Name=Only
Backend=smx

[PadProfile2]
Name=Good
Backend=smx
PadType=fsr
DefaultFor=S1 S2
Panel0.FsrLow=1 2 3 4
";
        let parsed = parse(content);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Good");
        assert_eq!(parsed[0].default_for_serials, vec!["S1", "S2"]);
    }

    #[test]
    fn default_is_per_pad_any_config() {
        // One config can be the default for two pads; or each pad a different one.
        let mut list = vec![
            sample("Soft", "smx", Some("fsr"), Some("S1"), &[]),
            sample("Hard", "smx", Some("fsr"), Some("S2"), &[]),
        ];
        // Pad S1 -> Soft, Pad S2 -> Soft (same config for both pads).
        assert!(apply_set_default(&mut list, "S1", "Soft"));
        assert!(apply_set_default(&mut list, "S2", "Soft"));
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S1").unwrap().name,
            "Soft"
        );
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S2").unwrap().name,
            "Soft"
        );
        // Re-point pad S2 at Hard; S1 stays on Soft.
        assert!(apply_set_default(&mut list, "S2", "Hard"));
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S1").unwrap().name,
            "Soft"
        );
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S2").unwrap().name,
            "Hard"
        );
        // list is in insertion order here (parse sorts; these are built directly).
        assert!(is_default_for(&list[0], "S1")); // Soft -> S1
        assert!(!is_default_for(&list[0], "S2")); // Soft no longer S2 (Hard took it)
        assert!(is_default_for(&list[1], "S2")); // Hard -> S2
    }

    #[test]
    fn resolve_falls_back_to_global_default_then_none() {
        let mut list = vec![sample("Any", "smx", None, None, &[])];
        // No per-pad default and no global -> nothing.
        assert!(resolve(&list, "smx", Some("fsr"), "S9").is_none());
        list[0].global_default = true;
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S9").unwrap().name,
            "Any"
        );
    }

    #[test]
    fn resolve_filters_by_backend_and_pad_type() {
        let list = vec![
            sample("Fsr", "smx", Some("fsr"), Some("S1"), &["S1"]),
            sample("LoadCell", "smx", Some("loadcell"), Some("S1"), &["S1"]),
            sample("Fsrio", "fsrio", None, Some("S1"), &["S1"]),
        ];
        // A load-cell pad must not pick the FSR default (or the fsrio config).
        assert_eq!(
            resolve(&list, "smx", Some("loadcell"), "S1").unwrap().name,
            "LoadCell"
        );
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S1").unwrap().name,
            "Fsr"
        );
    }

    #[test]
    fn set_default_is_exclusive_per_serial_and_case_insensitive() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), Some("S1"), &["S1"]),
            sample("B", "smx", Some("fsr"), Some("S1"), &[]),
        ];
        assert!(apply_set_default(&mut list, "S1", "b"));
        assert!(!is_default_for(&list[0], "S1")); // A lost S1
        assert!(is_default_for(&list[1], "S1")); // B gained S1
        assert!(!apply_set_default(&mut list, "S1", "nope"));
        assert!(is_default_for(&list[1], "S1"));
    }

    #[test]
    fn rename_keeps_default_associations() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), Some("S1"), &["S1"]),
            sample("B", "smx", Some("fsr"), Some("S2"), &[]),
        ];
        assert!(!apply_rename(&mut list, "A", "  ")); // blank
        assert!(!apply_rename(&mut list, "missing", "C")); // missing
        assert!(!apply_rename(&mut list, "A", "b")); // duplicate
        assert!(apply_rename(&mut list, "A", "Alpha"));
        assert_eq!(list[0].name, "Alpha");
        assert!(is_default_for(&list[0], "S1")); // association survived the rename
    }

    #[test]
    fn delete_removes_matching() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), Some("S1"), &["S1"]),
            sample("B", "smx", Some("fsr"), Some("S2"), &["S2"]),
        ];
        assert!(apply_delete(&mut list, "a"));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "B");
        assert!(!apply_delete(&mut list, "missing"));
    }
}
