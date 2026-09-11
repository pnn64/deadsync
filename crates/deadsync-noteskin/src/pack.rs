//! Optional noteskin packs and per-player asset selections.

use crate::itg::{IniData, NoteskinData, load_noteskin_data_cached};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::hash::Hasher;
use std::path::{Component, Path, PathBuf};
use twox_hash::XxHash64;

/// Customizable visual parts, in menu order.
pub const SLOTS: [&str; 11] = [
    "arrows",
    "receptors",
    "hold_active",
    "hold_inactive",
    "roll_active",
    "roll_inactive",
    "tap_explosions",
    "hold_explosions",
    "mines",
    "mine_size",
    "lifts",
];
// Bounds cover the workshop's catalog while keeping corrupt manifests finite.
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
// Each RGBA preview atlas costs 16 MiB; cap installed previews at 256 MiB.
const MAX_SKINS: usize = 16;
const MAX_CHOICES: usize = 1024;

/// A versioned, self-described collection of customizable noteskins.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub version: String,
    pub source: String,
    pub skins: Vec<Skin>,
}

/// A base noteskin with named choices and a compact preview atlas.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    pub id: String,
    pub base: String,
    pub preview: String,
    pub options: Vec<Choice>,
}

/// One replacement set for one visual part.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub slot: String,
    pub id: String,
    pub label: String,
    pub cell: u16,
    pub files: Vec<FileSwap>,
    #[serde(default)]
    pub metrics: Vec<MetricSwap>,
}

/// A base-relative filename replaced by a pack-relative source file.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileSwap {
    pub target: String,
    pub source: String,
}

/// A metric applied before noteskin compilation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetricSwap {
    pub section: String,
    pub key: String,
    pub value: String,
}

/// A validated pack installed outside the eager noteskin texture scan.
#[derive(Clone, Debug)]
pub struct InstalledPack {
    pub root: PathBuf,
    pub manifest: Manifest,
    pub fingerprint: String,
}

/// Stable profile identity; omitted parts use the pack's base assets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub skin: String,
    pub options: BTreeMap<String, String>,
}

/// Invalid pack data, selections, or inaccessible files.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Return the base name used by noteskin selectors.
pub fn base_name(selection: &str) -> &str {
    selection
        .split_once('?')
        .map_or(selection, |(base, _)| base)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

impl Selection {
    /// Parse a base name and optional `part=choice` pairs.
    ///
    /// # Errors
    /// Rejects malformed, duplicate, unknown, or excessively long parameters.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        if raw.len() > 2048 {
            return Err(Error::Invalid("noteskin selection is too long".into()));
        }
        let (skin, params) = raw.split_once('?').unwrap_or((raw, ""));
        if !valid_id(skin) {
            return Err(Error::Invalid("invalid pack skin ID".into()));
        }
        let mut options = BTreeMap::new();
        for pair in params.split('&').filter(|pair| !pair.is_empty()) {
            let Some((slot, id)) = pair.split_once('=') else {
                return Err(Error::Invalid("invalid noteskin option".into()));
            };
            if !SLOTS.contains(&slot)
                || !valid_id(id)
                || options.insert(slot.into(), id.into()).is_some()
            {
                return Err(Error::Invalid(format!(
                    "invalid or duplicate noteskin option: {slot}"
                )));
            }
        }
        Ok(Self {
            skin: skin.into(),
            options,
        })
    }
}

impl fmt::Display for Selection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.skin)?;
        for (index, (slot, id)) in self.options.iter().enumerate() {
            write!(f, "{}{slot}={id}", if index == 0 { '?' } else { '&' })?;
        }
        Ok(())
    }
}

/// Resolve a portable relative path and reject escapes, including symlinks.
pub fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::Invalid(format!("invalid pack path: {relative}")));
    }
    let resolved = root.join(path).canonicalize()?;
    if !resolved.starts_with(root.canonicalize()?) {
        return Err(Error::Invalid(format!(
            "pack path escapes its directory: {relative}"
        )));
    }
    Ok(resolved)
}

impl InstalledPack {
    /// Read and validate an installed manifest and every referenced file.
    ///
    /// # Errors
    /// Rejects unsupported schemas, duplicate IDs, unsafe paths, and missing assets.
    pub fn load(root: &Path) -> Result<Self, Error> {
        let path = root.join("pack.json");
        if fs::metadata(&path)?.len() > MAX_MANIFEST_BYTES {
            return Err(Error::Invalid("noteskin manifest exceeds 4 MiB".into()));
        }
        let bytes = fs::read(path)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        if manifest.schema != 1
            || !valid_id(&manifest.id)
            || manifest.version.is_empty()
            || manifest.skins.is_empty()
            || manifest.skins.len() > MAX_SKINS
        {
            return Err(Error::Invalid(
                "unsupported or invalid noteskin pack".into(),
            ));
        }
        let mut ids = HashSet::new();
        for skin in &manifest.skins {
            if !valid_id(&skin.id) || !ids.insert(&skin.id) || skin.options.len() > MAX_CHOICES {
                return Err(Error::Invalid("invalid or duplicate pack skin ID".into()));
            }
            let base = checked_path(root, &skin.base)?;
            if !base.join("metrics.ini").is_file() || !base.join("NoteSkin.lua").is_file() {
                return Err(Error::Invalid(format!(
                    "incomplete base noteskin: {}",
                    skin.id
                )));
            }
            let preview = checked_path(root, &skin.preview)?;
            if image::image_dimensions(preview).map_err(|e| Error::Invalid(e.to_string()))?
                != (2048, 2048)
            {
                return Err(Error::Invalid(
                    "pack preview must be a 2048x2048 atlas".into(),
                ));
            }
            validate_choices(root, &base, skin)?;
        }
        let root = root.canonicalize()?;
        let mut hash = XxHash64::default();
        hash.write(&bytes);
        hash.write(root.to_string_lossy().as_bytes());
        Ok(Self {
            root,
            manifest,
            fingerprint: format!("{:016x}", hash.finish()),
        })
    }

    /// Find an installed skin by its stable profile name.
    pub fn skin(&self, name: &str) -> Option<&Skin> {
        self.manifest
            .skins
            .iter()
            .find(|skin| skin.id == base_name(name))
    }

    /// Distinguish pack revisions and all selected parts in runtime caches.
    pub fn runtime_key(&self, selection: &Selection) -> String {
        let mut hash = XxHash64::default();
        hash.write(self.fingerprint.as_bytes());
        hash.write(selection.to_string().as_bytes());
        format!("{}-{:016x}", selection.skin, hash.finish())
    }

    /// PNG replacements change runtime textures, but not compiled Lua programs.
    /// Keep metric and all other file changes in the compiler identity.
    pub fn compiler_key(&self, selection: &Selection) -> String {
        let mut program = selection.clone();
        if let Some(skin) = self.skin(&selection.skin) {
            program.options.retain(|slot, id| {
                skin.options
                    .iter()
                    .find(|choice| choice.slot == *slot && choice.id == *id)
                    .is_none_or(|choice| {
                        !choice.metrics.is_empty()
                            || choice.files.iter().any(|swap| {
                                [&swap.source, &swap.target].into_iter().any(|path| {
                                    !Path::new(path)
                                        .extension()
                                        .and_then(|ext| ext.to_str())
                                        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                                })
                            })
                    })
            });
        }
        self.runtime_key(&program)
    }

    /// Resolve selected assets with the bundled common fallback.
    ///
    /// # Errors
    /// Fails on unavailable choices, conflicting replacements, or missing fallback data.
    pub fn resolve(&self, selection: &Selection, roots: &[PathBuf]) -> Result<NoteskinData, Error> {
        let skin = self
            .skin(&selection.skin)
            .ok_or_else(|| Error::Invalid("skin is not in this pack".into()))?;
        let base = self.root.join(&skin.base);
        let mut metrics = IniData::default();
        let mut overrides = Vec::new();
        for (slot, id) in &selection.options {
            let choice = skin
                .options
                .iter()
                .find(|c| c.slot == *slot && c.id == *id)
                .ok_or_else(|| Error::Invalid(format!("unavailable {slot} choice: {id}")))?;
            for swap in &choice.metrics {
                metrics.set(&swap.section, &swap.key, &swap.value);
            }
            for swap in &choice.files {
                let target = base.join(&swap.target).canonicalize()?;
                if overrides.iter().any(|(old, _)| *old == target) {
                    return Err(Error::Invalid(format!(
                        "overlapping noteskin replacement: {}",
                        swap.target
                    )));
                }
                overrides.push((target, self.root.join(&swap.source).canonicalize()?));
            }
        }
        metrics.merge_missing_from(
            &IniData::parse_file(&base.join("metrics.ini")).map_err(Error::Invalid)?,
        );
        let fallback = metrics
            .get("global", "fallbacknoteskin")
            .unwrap_or("common");
        let common = roots
            .iter()
            .find_map(|root| load_noteskin_data_cached(root, "dance", fallback).ok())
            .ok_or_else(|| Error::Invalid(format!("missing noteskin fallback: {fallback}")))?;
        metrics.merge_missing_from(&common.metrics);
        let mut search_dirs = Vec::with_capacity(common.search_dirs.len() + 1);
        search_dirs.push(base);
        search_dirs.extend(common.search_dirs.iter().cloned());
        Ok(NoteskinData {
            name: self.runtime_key(selection),
            metrics,
            search_dirs,
            overrides,
        })
    }
}

fn validate_choices(root: &Path, base: &Path, skin: &Skin) -> Result<(), Error> {
    let mut ids = HashSet::new();
    for choice in &skin.options {
        if !SLOTS.contains(&choice.slot.as_str())
            || !valid_id(&choice.id)
            || !ids.insert((&choice.slot, &choice.id))
            || choice.cell >= 1024
            || choice.label.is_empty()
            || choice.label.len() > 160
            || choice.files.len() > 32
            || choice.metrics.len() > 32
        {
            return Err(Error::Invalid(format!("invalid option in {}", skin.id)));
        }
        let mut targets = HashSet::new();
        for swap in &choice.files {
            let target = checked_path(base, &swap.target)?;
            let source = checked_path(root, &swap.source)?;
            if !source.is_file() || !target.is_file() || !targets.insert(target) {
                return Err(Error::Invalid(
                    "invalid or duplicate replacement file".into(),
                ));
            }
        }
        for metric in &choice.metrics {
            if !metric.section.eq_ignore_ascii_case("NoteDisplay")
                || metric.key.is_empty()
                || metric.key.contains(['\n', '\r', '='])
                || metric.value.contains(['\n', '\r'])
            {
                return Err(Error::Invalid("invalid NoteDisplay override".into()));
            }
        }
    }
    Ok(())
}

/// Discover validated packs in root order; earlier skin IDs take precedence.
pub fn discover(roots: &[PathBuf]) -> Vec<InstalledPack> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let mut dirs: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
                    && path.join("pack.json").is_file()
            })
            .collect();
        dirs.sort();
        for dir in dirs {
            if seen.len() == MAX_SKINS {
                return found;
            }
            match InstalledPack::load(&dir) {
                Ok(pack) if seen.len() + pack.manifest.skins.len() > MAX_SKINS => {
                    log::warn!(
                        "Skipping noteskin pack beyond the preview budget: {}",
                        dir.display()
                    );
                }
                Ok(pack)
                    if pack
                        .manifest
                        .skins
                        .iter()
                        .all(|skin| !seen.contains(&skin.id)) =>
                {
                    seen.extend(pack.manifest.skins.iter().map(|skin| skin.id.clone()));
                    found.push(pack);
                }
                Ok(_) => log::warn!(
                    "Skipping noteskin pack with duplicate skin IDs: {}",
                    dir.display()
                ),
                Err(error) => log::warn!("Cannot load noteskin pack {}: {error}", dir.display()),
            }
        }
    }
    found
}
