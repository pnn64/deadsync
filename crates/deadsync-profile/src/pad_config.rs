//! User pad-config profile data and serialization.
//!
//! Each local profile can store several named pad configs in `padconfig.ini`.
//! A config holds the backend, optional pad type, provenance serial, default
//! serial associations, an optional global default, and opaque key/value
//! settings owned by the input backend.

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
    serialize_with::<true, true, true>(profiles)
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

fn serialize_with<const RESERVE: bool, const STREAM_DEFAULTS: bool, const DIRECT_FIELDS: bool>(
    profiles: &[PadConfigProfile],
) -> String {
    let mut content = if RESERVE {
        String::with_capacity(serialized_len(profiles))
    } else {
        String::new()
    };
    for (i, p) in profiles.iter().enumerate() {
        if DIRECT_FIELDS {
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
            content.push('\n');
        } else {
            let _ = writeln!(content, "[PadProfile{i}]");
            let _ = writeln!(content, "Name={}", p.name);
            let _ = writeln!(content, "Backend={}", p.backend);
            let _ = writeln!(content, "PadType={}", p.pad_type.as_deref().unwrap_or(""));
            let _ = writeln!(content, "Serial={}", p.serial.as_deref().unwrap_or(""));
        }
        content.push_str("DefaultFor=");
        if STREAM_DEFAULTS {
            if let Some((first, rest)) = p.default_for_serials.split_first() {
                content.push_str(first);
                for serial in rest {
                    content.push(' ');
                    content.push_str(serial);
                }
            }
        } else {
            content.push_str(&p.default_for_serials.join(" "));
        }
        content.push('\n');
        if DIRECT_FIELDS {
            content.push_str("GlobalDefault=");
            content.push(if p.global_default { '1' } else { '0' });
            content.push('\n');
            for (key, value) in &p.settings {
                content.push_str(key);
                content.push('=');
                content.push_str(value);
                content.push('\n');
            }
        } else {
            let _ = writeln!(content, "GlobalDefault={}", u8::from(p.global_default));
            for (key, value) in &p.settings {
                let _ = writeln!(content, "{key}={value}");
            }
        }
        content.push('\n');
    }
    content
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

#[cfg(any(test, feature = "bench-support"))]
fn parse_owned(content: &str) -> Vec<PadConfigProfile> {
    let mut out = Vec::new();
    let mut in_section = false;
    let mut name = String::new();
    let mut backend = String::new();
    let mut pad_type = String::new();
    let mut serial = String::new();
    let mut default_for = String::new();
    let mut global_default = false;
    let mut settings: Vec<(String, String)> = Vec::new();

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if in_section {
                flush_profile(
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
                _ if !META_KEYS.contains(&key) => settings.push((key.to_string(), val.to_string())),
                _ => {}
            }
        }
    }
    if in_section {
        flush_profile(
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

    out.sort_by(|a, b| unicode_case_insensitive_cmp(&a.name, &b.name));
    out
}

pub fn load_path(path: &Path) -> std::io::Result<Vec<PadConfigProfile>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse(&content))
}

#[must_use]
pub fn load_dir(profile_dir: &Path) -> Vec<PadConfigProfile> {
    load_path(&pad_config_path(profile_dir)).unwrap_or_default()
}

pub fn pad_config_path_for_profile_id(
    root: &Path,
    profile_id: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> PathBuf {
    pad_config_path(&crate::runtime_profile_dir_for_id(
        root, profile_id, duplicate,
    ))
}

pub fn load_profile_id(
    root: &Path,
    profile_id: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Vec<PadConfigProfile> {
    load_path(&pad_config_path_for_profile_id(root, profile_id, duplicate)).unwrap_or_default()
}

pub fn save_path(path: &Path, profiles: &[PadConfigProfile]) -> std::io::Result<()> {
    std::fs::write(path, serialize(profiles))
}

pub fn save_dir(profile_dir: &Path, profiles: &[PadConfigProfile]) -> std::io::Result<()> {
    save_path(&pad_config_path(profile_dir), profiles)
}

pub fn save_profile_id(
    root: &Path,
    profile_id: &str,
    profiles: &[PadConfigProfile],
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> std::io::Result<()> {
    save_path(
        &pad_config_path_for_profile_id(root, profile_id, duplicate),
        profiles,
    )
}

#[derive(Debug)]
pub struct PadConfigIoError {
    pub path: PathBuf,
    pub error: std::io::Error,
}

const fn pad_config_io_error(path: PathBuf, error: std::io::Error) -> PadConfigIoError {
    PadConfigIoError { path, error }
}

pub fn save_profile_id_report(
    root: &Path,
    profile_id: &str,
    profiles: &[PadConfigProfile],
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Result<(), PadConfigIoError> {
    let path = pad_config_path_for_profile_id(root, profile_id, duplicate);
    save_path(&path, profiles).map_err(|error| pad_config_io_error(path, error))
}

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
pub fn upsert_dir(
    profile_dir: &Path,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) -> std::io::Result<bool> {
    upsert_path(
        &pad_config_path(profile_dir),
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_profile_id(
    root: &Path,
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> std::io::Result<bool> {
    upsert_path(
        &pad_config_path_for_profile_id(root, profile_id, duplicate),
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_profile_id_report(
    root: &Path,
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Result<bool, PadConfigIoError> {
    let path = pad_config_path_for_profile_id(root, profile_id, duplicate);
    upsert_path(
        &path,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    )
    .map_err(|error| pad_config_io_error(path, error))
}

pub fn set_default_path(path: &Path, serial: &str, name: &str) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = set_default_config(&mut list, serial, name);
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
}

pub fn set_default_dir(profile_dir: &Path, serial: &str, name: &str) -> std::io::Result<bool> {
    set_default_path(&pad_config_path(profile_dir), serial, name)
}

pub fn set_default_profile_id(
    root: &Path,
    profile_id: &str,
    serial: &str,
    name: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> std::io::Result<bool> {
    set_default_path(
        &pad_config_path_for_profile_id(root, profile_id, duplicate),
        serial,
        name,
    )
}

pub fn set_default_profile_id_report(
    root: &Path,
    profile_id: &str,
    serial: &str,
    name: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Result<bool, PadConfigIoError> {
    let path = pad_config_path_for_profile_id(root, profile_id, duplicate);
    set_default_path(&path, serial, name).map_err(|error| pad_config_io_error(path, error))
}

pub fn rename_path(path: &Path, old: &str, new: &str) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = rename_config(&mut list, old, new);
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
}

pub fn rename_dir(profile_dir: &Path, old: &str, new: &str) -> std::io::Result<bool> {
    rename_path(&pad_config_path(profile_dir), old, new)
}

pub fn rename_profile_id(
    root: &Path,
    profile_id: &str,
    old: &str,
    new: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> std::io::Result<bool> {
    rename_path(
        &pad_config_path_for_profile_id(root, profile_id, duplicate),
        old,
        new,
    )
}

pub fn rename_profile_id_report(
    root: &Path,
    profile_id: &str,
    old: &str,
    new: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Result<bool, PadConfigIoError> {
    let path = pad_config_path_for_profile_id(root, profile_id, duplicate);
    rename_path(&path, old, new).map_err(|error| pad_config_io_error(path, error))
}

pub fn delete_path(path: &Path, name: &str) -> std::io::Result<bool> {
    let mut list = load_path(path).unwrap_or_default();
    let changed = delete_config(&mut list, name);
    if changed {
        save_path(path, &list)?;
    }
    Ok(changed)
}

pub fn delete_dir(profile_dir: &Path, name: &str) -> std::io::Result<bool> {
    delete_path(&pad_config_path(profile_dir), name)
}

pub fn delete_profile_id(
    root: &Path,
    profile_id: &str,
    name: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> std::io::Result<bool> {
    delete_path(
        &pad_config_path_for_profile_id(root, profile_id, duplicate),
        name,
    )
}

pub fn delete_profile_id_report(
    root: &Path,
    profile_id: &str,
    name: &str,
    duplicate: impl FnMut(&str, &Path, &Path, &Path),
) -> Result<bool, PadConfigIoError> {
    let path = pad_config_path_for_profile_id(root, profile_id, duplicate);
    delete_path(&path, name).map_err(|error| pad_config_io_error(path, error))
}

#[cfg(any(test, feature = "bench-support"))]
#[allow(clippy::too_many_arguments)]
fn flush_profile(
    name: &mut String,
    backend: &mut String,
    pad_type: &mut String,
    serial: &mut String,
    default_for: &mut String,
    global_default: &mut bool,
    settings: &mut Vec<(String, String)>,
    out: &mut Vec<PadConfigProfile>,
) {
    if !name.trim().is_empty() && !backend.trim().is_empty() && !settings.is_empty() {
        out.push(PadConfigProfile {
            name: std::mem::take(name).trim().to_string(),
            backend: std::mem::take(backend).trim().to_string(),
            pad_type: opt_string(pad_type),
            serial: opt_string(serial),
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

#[cfg(any(test, feature = "bench-support"))]
fn opt_string(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn serialize_unreserved_for_bench(profiles: &[PadConfigProfile]) -> String {
    serialize_with::<false, true, false>(profiles)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn serialize_joined_defaults_for_bench(profiles: &[PadConfigProfile]) -> String {
    serialize_with::<true, false, true>(profiles)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn parse_owned_for_bench(content: &str) -> Vec<PadConfigProfile> {
    parse_owned(content)
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
    fn optimized_serializers_preserve_pad_config_wire_format() {
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
        assert_eq!(serialize_with::<false, true, false>(&profiles), expected);
        assert_eq!(serialize_with::<true, false, true>(&profiles), expected);
        assert_eq!(serialized_len(&profiles), expected.len());
    }

    #[test]
    fn borrowed_parser_matches_owned_parser_for_mixed_sections() {
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

        let expected = parse_owned(content);
        let actual = parse(content);
        assert_eq!(actual, expected);
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
