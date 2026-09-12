//! Machine-global pad-config data, serialization, and legacy migration.
//!
//! The machine stores several named pad configs in one `padconfig.ini`,
//! shared by every player (pad thresholds describe the physical pads, not a
//! player). A config holds the backend, optional pad type, provenance serial,
//! default serial associations, an optional global default, and opaque
//! key/value settings owned by the input backend. Configs previously lived in
//! each local profile's `padconfig.ini`; [`migrate_machine_store`] merges
//! those into the machine store once.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::favorites_view::unicode_case_insensitive_cmp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadConfigProfile {
    pub name: String,
    pub backend: String,
    pub pad_type: Option<String>,
    pub serial: Option<String>,
    pub default_for_serials: Vec<String>,
    pub global_default: bool,
    pub settings: Vec<(String, String)>,
}

const META_KEYS: [&str; 6] = [
    "Name",
    "Backend",
    "PadType",
    "Serial",
    "DefaultFor",
    "GlobalDefault",
];

pub const PAD_CONFIG_FILE: &str = "padconfig.ini";

/// Legacy per-profile store location, read only by [`migrate_machine_store`].
#[inline(always)]
#[must_use]
pub fn pad_config_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(PAD_CONFIG_FILE)
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_config(
    list: &mut Vec<PadConfigProfile>,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return false;
    }
    if let Some(existing) = list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(name)) {
        existing.backend = backend.to_string();
        existing.pad_type = pad_type;
        existing.serial.clone_from(&serial);
        existing.settings = settings;
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
    if make_default && let Some(s) = serial {
        set_default_config(list, &s, name);
    }
    true
}

pub fn set_default_config(list: &mut [PadConfigProfile], serial: &str, name: &str) -> bool {
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

pub fn rename_config(list: &mut [PadConfigProfile], old: &str, new: &str) -> bool {
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

pub fn delete_config(list: &mut Vec<PadConfigProfile>, name: &str) -> bool {
    let before = list.len();
    list.retain(|p| !p.name.eq_ignore_ascii_case(name));
    list.len() != before
}

#[must_use]
pub fn config_matches(profile: &PadConfigProfile, backend: &str, pad_type: Option<&str>) -> bool {
    profile.backend == backend
        && match (profile.pad_type.as_deref(), pad_type) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

#[must_use]
pub fn is_default_for(profile: &PadConfigProfile, serial: &str) -> bool {
    profile.default_for_serials.iter().any(|s| s == serial)
}

#[must_use]
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

#[must_use]
pub fn serialize(profiles: &[PadConfigProfile]) -> String {
    let mut content = String::with_capacity(serialized_len(profiles));
    for (i, p) in profiles.iter().enumerate() {
        content.push_str("[PadProfile");
        let _ = write!(content, "{i}");
        content.push_str("]\nName=");
        content.push_str(&p.name);
        content.push_str("\nBackend=");
        content.push_str(&p.backend);
        content.push_str("\nPadType=");
        content.push_str(p.pad_type.as_deref().unwrap_or(""));
        content.push_str("\nSerial=");
        content.push_str(p.serial.as_deref().unwrap_or(""));
        content.push_str("\nDefaultFor=");
        if let Some((first, rest)) = p.default_for_serials.split_first() {
            content.push_str(first);
            for serial in rest {
                content.push(' ');
                content.push_str(serial);
            }
        }
        content.push_str("\nGlobalDefault=");
        content.push(if p.global_default { '1' } else { '0' });
        content.push('\n');
        for (key, value) in &p.settings {
            content.push_str(key);
            content.push('=');
            content.push_str(value);
            content.push('\n');
        }
        content.push('\n');
    }
    content
}

const fn decimal_digits(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn serialized_len(profiles: &[PadConfigProfile]) -> usize {
    profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            let default_for_len = profile
                .default_for_serials
                .iter()
                .map(String::len)
                .sum::<usize>()
                .saturating_add(profile.default_for_serials.len().saturating_sub(1));
            let settings_len = profile
                .settings
                .iter()
                .map(|(key, value)| key.len().saturating_add(value.len()).saturating_add(2))
                .sum::<usize>();

            decimal_digits(index)
                .saturating_add(profile.name.len())
                .saturating_add(profile.backend.len())
                .saturating_add(profile.pad_type.as_deref().map_or(0, str::len))
                .saturating_add(profile.serial.as_deref().map_or(0, str::len))
                .saturating_add(default_for_len)
                .saturating_add(settings_len)
                // Fixed punctuation, field names, newlines, and the section gap.
                .saturating_add(74)
        })
        .fold(0, usize::saturating_add)
}

#[must_use]
pub fn parse(content: &str) -> Vec<PadConfigProfile> {
    parse_borrowed(content)
}

#[derive(Default)]
struct BorrowedPadConfig<'a> {
    name: &'a str,
    backend: &'a str,
    pad_type: &'a str,
    serial: &'a str,
    default_for: &'a str,
    global_default: bool,
}

fn parse_borrowed(content: &str) -> Vec<PadConfigProfile> {
    let mut out = Vec::new();
    let mut in_section = false;
    let mut fields = BorrowedPadConfig::default();
    let mut settings = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if in_section {
                flush_borrowed_profile(&mut fields, &mut settings, &mut out);
            }
            in_section = line[1..line.len() - 1].trim().starts_with("PadProfile");
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim();
            let value = line[eq + 1..].trim();
            match key {
                "Name" => fields.name = value,
                "Backend" => fields.backend = value,
                "PadType" => fields.pad_type = value,
                "Serial" => fields.serial = value,
                "DefaultFor" => fields.default_for = value,
                "GlobalDefault" => fields.global_default = value == "1",
                _ if !META_KEYS.contains(&key) => settings.push((key, value)),
                _ => {}
            }
        }
    }
    if in_section {
        flush_borrowed_profile(&mut fields, &mut settings, &mut out);
    }

    out.sort_by(|a, b| unicode_case_insensitive_cmp(&a.name, &b.name));
    out
}

fn flush_borrowed_profile<'a>(
    fields: &mut BorrowedPadConfig<'a>,
    settings: &mut Vec<(&'a str, &'a str)>,
    out: &mut Vec<PadConfigProfile>,
) {
    if !fields.name.is_empty() && !fields.backend.is_empty() && !settings.is_empty() {
        out.push(PadConfigProfile {
            name: fields.name.to_owned(),
            backend: fields.backend.to_owned(),
            pad_type: owned_nonempty(fields.pad_type),
            serial: owned_nonempty(fields.serial),
            default_for_serials: fields
                .default_for
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
            global_default: fields.global_default,
            settings: settings
                .iter()
                .map(|&(key, value)| (key.to_owned(), value.to_owned()))
                .collect(),
        });
    }
    *fields = BorrowedPadConfig::default();
    settings.clear();
}

fn owned_nonempty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

pub fn load_path(path: &Path) -> std::io::Result<Vec<PadConfigProfile>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse(&content))
}

pub fn save_path(path: &Path, profiles: &[PadConfigProfile]) -> std::io::Result<()> {
    deadlib_platform::atomic_write::write_atomic(path, serialize(profiles).as_bytes())
}

/// Merge per-profile pad-config lists into one machine-global list.
///
/// `sources` is ordered newest-first (file mtime); earlier sources win every
/// conflict:
/// - A config that matches an already-merged one (same name ignoring case,
///   backend, pad type, and settings) folds into it, contributing any serial
///   defaults not already claimed.
/// - A name collision between *different* configs keeps both, renaming the
///   later one with a " (2)"-style suffix.
/// - Each serial keeps one default and each backend one global default: the
///   newest source claiming it.
#[must_use]
/// Name a migrated config after the profile it came from, so the merged
/// list still says whose tuning each entry is ("ddrcoder - Left"). An empty
/// owner leaves the name alone.
#[must_use]
pub fn migrated_config_name(owner: &str, name: &str) -> String {
    let owner = owner.trim();
    if owner.is_empty() {
        name.to_owned()
    } else {
        format!("{owner} - {name}")
    }
}

/// Merge legacy per-profile config lists, newest source first, into one
/// machine list. Each source is `(owner, configs)`; every config is renamed
/// via [`migrated_config_name`] so it keeps its provenance.
pub fn merge_for_migration(sources: Vec<(String, Vec<PadConfigProfile>)>) -> Vec<PadConfigProfile> {
    let mut merged: Vec<PadConfigProfile> = Vec::new();
    for (owner, source) in sources {
        for mut profile in source {
            profile.name = migrated_config_name(&owner, &profile.name);
            profile
                .default_for_serials
                .retain(|serial| !merged.iter().any(|m| is_default_for(m, serial)));
            if profile.global_default
                && merged
                    .iter()
                    .any(|m| m.global_default && m.backend == profile.backend)
            {
                profile.global_default = false;
            }
            if let Some(existing) = merged.iter_mut().find(|m| {
                m.name.eq_ignore_ascii_case(&profile.name)
                    && m.backend == profile.backend
                    && m.pad_type == profile.pad_type
                    && m.settings == profile.settings
            }) {
                existing
                    .default_for_serials
                    .append(&mut profile.default_for_serials);
                existing.global_default |= profile.global_default;
                continue;
            }
            let mut name = profile.name.clone();
            let mut n = 1;
            while merged.iter().any(|m| m.name.eq_ignore_ascii_case(&name)) {
                n += 1;
                name = format!("{} ({n})", profile.name);
            }
            profile.name = name;
            merged.push(profile);
        }
    }
    merged.sort_by(|a, b| unicode_case_insensitive_cmp(&a.name, &b.name));
    merged
}

/// One-time migration into the machine-global store at `machine_path`: merge
/// every per-profile `padconfig.ini` under `profiles_root` (newest file
/// first) and write the result, each config renamed after its profile (the
/// `profile.ini` display name, else the folder name). The machine file's
/// existence marks the migration done, so an empty file is written even when
/// there is nothing to migrate. Legacy per-profile files are left in place
/// untouched, so rolling back to a per-profile build loses nothing.
///
/// Returns `None` when the machine store already exists, otherwise the number
/// of migrated configs.
pub fn migrate_machine_store(
    machine_path: &Path,
    profiles_root: &Path,
) -> std::io::Result<Option<usize>> {
    if machine_path.exists() {
        return Ok(None);
    }
    let mut sources: Vec<(
        std::time::SystemTime,
        std::ffi::OsString,
        String,
        Vec<PadConfigProfile>,
    )> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(profiles_root) {
        for entry in entries.flatten() {
            let path = pad_config_path(&entry.path());
            let Ok(list) = load_path(&path) else { continue };
            if list.is_empty() {
                continue;
            }
            let modified = std::fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            let owner = crate::read_profile_identity_dir(&entry.path())
                .1
                .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
            sources.push((modified, entry.file_name(), owner, list));
        }
    }
    // Newest profile first; directory name breaks mtime ties so the merge is
    // deterministic regardless of read_dir order.
    sources.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let merged = merge_for_migration(
        sources
            .into_iter()
            .map(|(_, _, owner, list)| (owner, list))
            .collect(),
    );
    if let Some(parent) = machine_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    save_path(machine_path, &merged)?;
    Ok(Some(merged.len()))
}

pub fn upsert_path(
    path: &Path,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = upsert_config(
        &mut list,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    );
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
}

pub fn set_default_path(path: &Path, serial: &str, name: &str) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = set_default_config(&mut list, serial, name);
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
}

pub fn rename_path(path: &Path, old: &str, new: &str) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = rename_config(&mut list, old, new);
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
}

pub fn delete_path(path: &Path, name: &str) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = delete_config(&mut list, name);
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
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
            default_for_serials: default_for
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
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
    fn serializer_preserves_pad_config_wire_format() {
        let mut profile = sample("Alpha", "smx", Some("fsr"), Some("S1"), &["S1", "S2"]);
        profile.global_default = true;
        let profiles = [profile];
        let expected = "\
[PadProfile0]
Name=Alpha
Backend=smx
PadType=fsr
Serial=S1
DefaultFor=S1 S2
GlobalDefault=1
Panel0.FsrLow=152 152 152 152
DebounceMs=4

";

        assert_eq!(serialize(&profiles), expected);
        assert_eq!(serialized_len(&profiles), expected.len());
    }

    #[test]
    fn borrowed_parser_handles_mixed_sections() {
        let content = "\
; ignored comment
[PadProfile0]
Name=First
Name=  Beta
Backend= smx
PadType= fsr
Serial= S2
DefaultFor= S2   S3
GlobalDefault=1
Panel0.FsrLow= 10 20 30 40

[Other]
Name=Not a pad profile
Backend=ignored
Setting=ignored

[PadProfile1]
Name=Rejected
Backend=smx

[PadProfile2]
Name=alpha
Backend=fsrio
Custom= value
";

        let actual = parse(content);
        assert_eq!(actual.len(), 2);
        assert_eq!(actual[0].name, "alpha");
        assert_eq!(actual[1].name, "Beta");
        assert_eq!(actual[1].default_for_serials, ["S2", "S3"]);
        assert_eq!(
            actual[1].settings[0],
            ("Panel0.FsrLow".into(), "10 20 30 40".into())
        );
    }

    #[test]
    fn pad_config_path_uses_profile_dir() {
        assert_eq!(
            pad_config_path(Path::new("Profiles/abc")),
            Path::new("Profiles/abc").join(PAD_CONFIG_FILE)
        );
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
        let mut list = vec![
            sample("Soft", "smx", Some("fsr"), Some("S1"), &[]),
            sample("Hard", "smx", Some("fsr"), Some("S2"), &[]),
        ];
        assert!(set_default_config(&mut list, "S1", "Soft"));
        assert!(set_default_config(&mut list, "S2", "Soft"));
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S1").unwrap().name,
            "Soft"
        );
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S2").unwrap().name,
            "Soft"
        );

        assert!(set_default_config(&mut list, "S2", "Hard"));
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S1").unwrap().name,
            "Soft"
        );
        assert_eq!(
            resolve(&list, "smx", Some("fsr"), "S2").unwrap().name,
            "Hard"
        );
        assert!(is_default_for(&list[0], "S1"));
        assert!(!is_default_for(&list[0], "S2"));
        assert!(is_default_for(&list[1], "S2"));
    }

    #[test]
    fn resolve_falls_back_to_global_default_then_none() {
        let mut list = vec![sample("Any", "smx", None, None, &[])];
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
        assert!(set_default_config(&mut list, "S1", "b"));
        assert!(!is_default_for(&list[0], "S1"));
        assert!(is_default_for(&list[1], "S1"));
        assert!(!set_default_config(&mut list, "S1", "nope"));
        assert!(is_default_for(&list[1], "S1"));
    }

    #[test]
    fn rename_keeps_default_associations() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), Some("S1"), &["S1"]),
            sample("B", "smx", Some("fsr"), Some("S2"), &[]),
        ];
        assert!(!rename_config(&mut list, "A", "  "));
        assert!(!rename_config(&mut list, "missing", "C"));
        assert!(!rename_config(&mut list, "A", "b"));
        assert!(rename_config(&mut list, "A", "Alpha"));
        assert_eq!(list[0].name, "Alpha");
        assert!(is_default_for(&list[0], "S1"));
    }

    #[test]
    fn delete_removes_matching() {
        let mut list = vec![
            sample("A", "smx", Some("fsr"), Some("S1"), &["S1"]),
            sample("B", "smx", Some("fsr"), Some("S2"), &["S2"]),
        ];
        assert!(delete_config(&mut list, "a"));
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "B");
        assert!(!delete_config(&mut list, "missing"));
    }

    #[test]
    fn migration_merge_unions_identical_configs_and_their_defaults() {
        let newest = vec![sample("Soft", "smx", Some("fsr"), Some("S1"), &["S1"])];
        let older = vec![sample("soft", "smx", Some("fsr"), Some("S1"), &["S2"])];
        let merged = merge_for_migration(vec![(String::new(), newest), (String::new(), older)]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "Soft");
        assert_eq!(merged[0].default_for_serials, ["S1", "S2"]);
    }

    #[test]
    fn migration_merge_keeps_conflicting_configs_under_suffixed_names() {
        let newest = vec![sample("Soft", "smx", Some("fsr"), Some("S1"), &[])];
        let mut conflicting = sample("Soft", "smx", Some("fsr"), Some("S1"), &[]);
        conflicting.settings[1].1 = "9".to_owned();
        let merged = merge_for_migration(vec![
            (String::new(), newest),
            (String::new(), vec![conflicting]),
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].name, "Soft");
        assert_eq!(merged[1].name, "Soft (2)");
        assert_eq!(merged[1].settings[1].1, "9");
    }

    #[test]
    fn migration_merge_gives_each_serial_and_backend_one_default() {
        // Both profiles claim S1; the newest source wins. Both carry a global
        // default for the same backend; again the newest wins.
        let mut newest = sample("New", "smx", Some("fsr"), Some("S1"), &["S1"]);
        newest.global_default = true;
        let mut older = sample("Old", "smx", Some("fsr"), Some("S1"), &["S1", "S2"]);
        older.global_default = true;
        let mut other_backend = sample("Fsrio", "fsrio", None, None, &[]);
        other_backend.global_default = true;
        let merged = merge_for_migration(vec![
            (String::new(), vec![newest]),
            (String::new(), vec![older, other_backend]),
        ]);
        assert_eq!(merged.len(), 3);
        let by_name = |name: &str| merged.iter().find(|p| p.name == name).unwrap();
        assert_eq!(by_name("New").default_for_serials, ["S1"]);
        assert!(by_name("New").global_default);
        assert_eq!(by_name("Old").default_for_serials, ["S2"]);
        assert!(!by_name("Old").global_default);
        // A different backend keeps its own global default.
        assert!(by_name("Fsrio").global_default);
    }

    #[test]
    fn migration_names_configs_after_their_profile() {
        assert_eq!(migrated_config_name("ddrcoder", "Left"), "ddrcoder - Left");
        assert_eq!(migrated_config_name("  ", "Left"), "Left");
        // Identical tunings from two profiles stay apart under their owners'
        // names; the serial default still goes to the newest source only.
        let newest = vec![sample("Soft", "smx", Some("fsr"), Some("S1"), &["S1"])];
        let older = vec![sample("Soft", "smx", Some("fsr"), Some("S1"), &["S1"])];
        let merged = merge_for_migration(vec![
            ("bob".to_owned(), newest),
            ("alice".to_owned(), older),
        ]);
        let names: Vec<&str> = merged.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["alice - Soft", "bob - Soft"]);
        assert!(merged[0].default_for_serials.is_empty());
        assert_eq!(merged[1].default_for_serials, ["S1"]);
    }

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "deadsync-padcfg-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time before unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn migrate_machine_store_merges_profile_files_newest_first() {
        let root = unique_temp_dir("migrate");
        let profiles_root = root.join("profiles");
        let machine = root.join("save").join(PAD_CONFIG_FILE);

        let older_dir = profiles_root.join("alice");
        std::fs::create_dir_all(&older_dir).expect("create profile dir");
        // The display name in profile.ini names the migrated configs; a folder
        // without one (bob, below) falls back to its folder name.
        std::fs::write(
            crate::profile_ini_path(&older_dir),
            "[UserProfile]\nDisplayName=Alice B\n",
        )
        .expect("write profile.ini");
        save_path(
            &pad_config_path(&older_dir),
            &[sample("Soft", "smx", Some("fsr"), Some("S1"), &["S1"])],
        )
        .expect("write older configs");
        // Ensure a strictly newer mtime for the second file: filetimes on some
        // filesystems are coarse, so set it explicitly via a future mtime.
        let newer_dir = profiles_root.join("bob");
        std::fs::create_dir_all(&newer_dir).expect("create profile dir");
        save_path(
            &pad_config_path(&newer_dir),
            &[sample("Hard", "smx", Some("fsr"), Some("S1"), &["S1"])],
        )
        .expect("write newer configs");
        let newer = std::fs::File::options()
            .append(true)
            .open(pad_config_path(&newer_dir))
            .expect("open newer file");
        newer
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(60))
            .expect("bump mtime");

        let migrated = migrate_machine_store(&machine, &profiles_root).expect("migrate");
        assert_eq!(migrated, Some(2));
        let merged = load_path(&machine).expect("read machine store");
        assert_eq!(merged.len(), 2);
        // The newer file's claim on S1 wins; names carry their profile.
        let hard = merged.iter().find(|p| p.name == "bob - Hard").unwrap();
        let soft = merged.iter().find(|p| p.name == "Alice B - Soft").unwrap();
        assert_eq!(hard.default_for_serials, ["S1"]);
        assert!(soft.default_for_serials.is_empty());

        // A second call sees the machine store and does nothing.
        assert_eq!(
            migrate_machine_store(&machine, &profiles_root).expect("re-run"),
            None
        );
        // Legacy files stay in place for rollback.
        assert!(pad_config_path(&older_dir).exists());

        std::fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn migrate_machine_store_writes_an_empty_marker_without_legacy_files() {
        let root = unique_temp_dir("empty");
        let machine = root.join("save").join(PAD_CONFIG_FILE);
        let migrated =
            migrate_machine_store(&machine, &root.join("missing-profiles")).expect("migrate");
        assert_eq!(migrated, Some(0));
        assert!(machine.exists());
        assert_eq!(load_path(&machine).expect("read machine store"), Vec::new());
        std::fs::remove_dir_all(root).expect("cleanup temp dir");
    }

    #[test]
    fn upsert_preserves_defaults_and_can_make_serial_default() {
        let mut list = vec![sample("A", "smx", Some("fsr"), Some("S1"), &["S1"])];
        assert!(upsert_config(
            &mut list,
            "A",
            "smx",
            Some("loadcell".to_string()),
            Some("S2".to_string()),
            true,
            vec![("DebounceMs".to_string(), "5".to_string())],
        ));

        assert_eq!(list.len(), 1);
        assert_eq!(list[0].pad_type.as_deref(), Some("loadcell"));
        assert!(is_default_for(&list[0], "S1"));
        assert!(is_default_for(&list[0], "S2"));
    }
}
