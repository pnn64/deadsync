pub use crate::compiled::{CompiledActors, CompiledLoader, actor_manifest_key};

use crate::Quantization;
use crate::{
    actor as noteskin_actor, compiled as noteskin_compiled,
    compiled::{CompiledActorFile, CompiledLoaderEntry, CompiledNoteskinBundle},
    itg as noteskin_itg,
};
use deadlib_lua::{Error, HostCall, HostFunction, Root, Value as LuaValue, Vm};
use log::{info, warn};
use smallvec::SmallVec;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};
use twox_hash::XxHash64;

const COMPILER_VERSION: u32 = 30;
static COMPILED_HASH_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const PUMP_BUTTONS: [&str; 5] = ["DownLeft", "UpLeft", "Center", "UpRight", "DownRight"];
const DANCE_BUTTONS: [&str; 4] = ["Left", "Down", "Up", "Right"];
// Match NoteDisplay::Load exactly: Lua comparisons of Var "Element" are case
// sensitive even though file and compiled-entry lookup are case insensitive.
const CORE_ELEMENTS: [&str; 43] = [
    "Explosion",
    "Go Receptor",
    "HitMine Explosion",
    "Hold Body Active",
    "Hold Body Inactive",
    "Hold Bottomcap Active",
    "Hold Bottomcap Inactive",
    "Hold Explosion",
    "Hold Head Active",
    "Hold Head Inactive",
    "Hold Tail Active",
    "Hold Tail Inactive",
    "Hold Topcap Active",
    "Hold Topcap Inactive",
    "Ready Receptor",
    "Receptor",
    "Roll Body Active",
    "Roll Body Inactive",
    "Roll Bottomcap Active",
    "Roll Bottomcap Inactive",
    "Roll Explosion",
    "Roll Head Active",
    "Roll Head Inactive",
    "Roll Tail Active",
    "Roll Tail Inactive",
    "Roll Topcap Active",
    "Roll Topcap Inactive",
    "Tap Explosion Bright",
    "Tap Explosion Bright W1",
    "Tap Explosion Bright W2",
    "Tap Explosion Bright W3",
    "Tap Explosion Bright W4",
    "Tap Explosion Bright W5",
    "Tap Explosion Dim",
    "Tap Explosion Dim W1",
    "Tap Explosion Dim W2",
    "Tap Explosion Dim W3",
    "Tap Explosion Dim W4",
    "Tap Explosion Dim W5",
    "Tap Fake",
    "Tap Lift",
    "Tap Mine",
    "Tap Note",
];
const ASCII_CASE_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const ASCII_CASE_HASH_PRIME: u64 = 0x0100_0000_01b3;
// The built-in loader domain has 43 elements. Keeping 64 fingerprints inline
// also covers typical third-party additions without heap storage.
const INLINE_LOADER_DOMAIN_KEYS: usize = 64;
type LoaderFingerprints = SmallVec<[u64; INLINE_LOADER_DOMAIN_KEYS]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileOutcome {
    Reused,
    Built,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileAllItgSummary {
    pub total: usize,
    pub built: usize,
    pub reused: usize,
    pub failed: usize,
}

pub fn ensure_compiled(
    cache_dir: &Path,
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<CompileOutcome, String> {
    if let Some(path) = cached_bundle_path(cache_dir, game, &data.name)
        && noteskin_compiled::load_compiled_bundle(&path).is_some()
    {
        return Ok(CompileOutcome::Reused);
    }
    let source_hash = source_hash(game, data)?;
    remember_source_hash(game, &data.name, &source_hash);
    let path = compiled_bundle_path(cache_dir, game, &data.name, &source_hash);
    if noteskin_compiled::load_compiled_bundle(&path).is_some() {
        return Ok(CompileOutcome::Reused);
    }
    info!("compiling noteskin cache for '{game}/{}'", data.name);
    let bundle = compile_data(game, data, &source_hash, "")?;
    noteskin_compiled::save_compiled_bundle(&path, &bundle)?;
    Ok(CompileOutcome::Built)
}

#[must_use]
pub fn load_compiled(
    cache_dir: &Path,
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Option<CompiledNoteskinBundle> {
    let path = cached_bundle_path(cache_dir, game, &data.name)?;
    noteskin_compiled::load_compiled_bundle(&path)
}

pub fn load_or_compile(
    cache_dir: &Path,
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<CompiledNoteskinBundle, String> {
    if let Some(bundle) = load_compiled(cache_dir, game, data) {
        return Ok(bundle);
    }
    ensure_compiled(cache_dir, game, data).map_err(|err| {
        format!(
            "failed to compile noteskin cache for '{game}/{}': {err}",
            data.name
        )
    })?;
    load_compiled(cache_dir, game, data).ok_or_else(|| {
        format!(
            "compiled noteskin cache missing for '{game}/{}' after successful compilation",
            data.name
        )
    })
}

pub fn compile_all_itg_caches_with_progress<F>(
    cache_dir: &Path,
    roots: &[PathBuf],
    game: &str,
    mut on_progress: F,
) -> CompileAllItgSummary
where
    F: FnMut(usize, usize, &str, &str),
{
    let skins = noteskin_itg::discover_skins(roots, game);
    let total = skins.len();
    let mut summary = CompileAllItgSummary {
        total,
        ..CompileAllItgSummary::default()
    };

    for (idx, skin) in skins.iter().enumerate() {
        let label = format!("{game}/{skin}");
        let result = load_data_from_roots(roots, game, skin).and_then(|data| {
            ensure_compiled(cache_dir, game, &data).map(|outcome| (data, outcome))
        });
        match result {
            Ok((_data, CompileOutcome::Built)) => {
                summary.built += 1;
                on_progress(idx + 1, total, &label, "compiled");
            }
            Ok((_data, CompileOutcome::Reused)) => {
                summary.reused += 1;
                on_progress(idx + 1, total, &label, "");
            }
            Err(err) => {
                summary.failed += 1;
                warn!("noteskin cache compile failed for '{label}': {err}");
                on_progress(idx + 1, total, &label, "failed");
            }
        }
    }

    summary
}

fn load_data_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
) -> Result<noteskin_itg::NoteskinData, String> {
    let mut last_load_err = None;
    for root in roots {
        match noteskin_itg::load_noteskin_data(root, game, skin) {
            Ok(data) => return Ok(data),
            Err(err) => last_load_err = Some(err),
        }
    }
    Err(last_load_err.unwrap_or_else(|| format!("noteskin '{game}/{skin}' not found in any root")))
}

fn cached_bundle_path(cache_dir: &Path, game: &str, skin: &str) -> Option<PathBuf> {
    let key = compiled_hash_cache_key(game, skin);
    let hash = COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()?;
    Some(compiled_bundle_path(cache_dir, game, skin, &hash))
}

fn compiled_bundle_path(cache_dir: &Path, game: &str, skin: &str, source_hash: &str) -> PathBuf {
    noteskin_compiled::compiled_bundle_path(cache_dir, game, skin, source_hash)
}

fn remember_source_hash(game: &str, skin: &str, source_hash: &str) {
    let key = compiled_hash_cache_key(game, skin);
    COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, source_hash.to_string());
}

fn compiled_hash_cache_key(game: &str, skin: &str) -> String {
    let game = game.trim();
    let skin = skin.trim();
    let mut key = String::with_capacity(game.len() + 1 + skin.len());
    key.push_str(game);
    key.push('/');
    key.push_str(skin);
    key.make_ascii_lowercase();
    key
}

fn source_hash(game: &str, data: &noteskin_itg::NoteskinData) -> Result<String, String> {
    let sources = labeled_source_paths(data, source_paths(data));
    let mut hasher = XxHash64::default();
    hasher.write_u32(noteskin_compiled::CACHE_SCHEMA_VERSION);
    hasher.write_u32(COMPILER_VERSION);
    hasher.write(game.as_bytes());
    hasher.write(data.name.as_bytes());
    let mut bytes = Vec::new();
    for (label, path) in sources {
        hasher.write(label.as_bytes());
        bytes.clear();
        fs::File::open(&path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|err| format!("failed to read '{}' for hashing: {err}", path.display()))?;
        hasher.write(&bytes);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

fn labeled_source_paths(
    data: &noteskin_itg::NoteskinData,
    paths: Vec<PathBuf>,
) -> Vec<(String, PathBuf)> {
    let mut sources: Vec<_> = paths
        .into_iter()
        .map(|path| (source_label(data, &path), path))
        .collect();
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
}

fn source_paths(data: &noteskin_itg::NoteskinData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in &data.search_dirs {
        for name in ["metrics.ini", "NoteSkin.lua"] {
            let path = dir.join(name);
            if path.is_file() {
                out.push(path);
            }
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_actor_lua = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
                && path.file_name().is_none_or(|name| name != "NoteSkin.lua")
                && path.is_file();
            if is_actor_lua {
                out.push(path);
            }
        }
    }
    out
}

fn source_label(data: &noteskin_itg::NoteskinData, path: &Path) -> String {
    for dir in &data.search_dirs {
        if !path.starts_with(dir) {
            continue;
        }
        let game = dir
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let skin = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let rel = path.strip_prefix(dir).unwrap_or(path).to_string_lossy();
        let mut label = String::with_capacity(game.len() + skin.len() + rel.len() + 2);
        label.push_str(game);
        label.push('/');
        label.push_str(skin);
        label.push('/');
        push_normalized_path(&mut label, &rel);
        label.make_ascii_lowercase();
        return label;
    }
    let path = path.to_string_lossy();
    let mut label = String::with_capacity(path.len());
    push_normalized_path(&mut label, &path);
    label.make_ascii_lowercase();
    label
}

fn push_normalized_path(out: &mut String, path: &str) {
    let mut parts = path.split('\\');
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for part in parts {
        out.push('/');
        out.push_str(part);
    }
}

fn noteskin_paths(data: &noteskin_itg::NoteskinData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in data.search_dirs.iter().rev() {
        let path = dir.join("NoteSkin.lua");
        if path.is_file() {
            out.push(path);
        }
    }
    out
}

fn compile_actor_files_with(
    data: &noteskin_itg::NoteskinData,
    mut color_paths: impl FnMut(
        &str,
        Option<&str>,
        crate::Quantization,
        &mut noteskin_actor::ItgLuaActorDecl,
    ) -> Result<(), String>,
) -> Result<Vec<CompiledActorFile>, String> {
    let mut out = Vec::new();
    for dir in &data.search_dirs {
        let entries = fs::read_dir(dir)
            .map_err(|err| format!("failed to read '{}': {err}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_lua = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
                && path.file_name().is_none_or(|name| name != "NoteSkin.lua")
                && path.is_file();
            if !is_lua {
                continue;
            }
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
            let Some(key) = noteskin_compiled::actor_manifest_key_for_dir(dir, &path) else {
                continue;
            };
            let uses_button = uses_actor_var(&content, "Button");
            let uses_color = uses_actor_var(&content, "Color");
            let buttons: Vec<_> = if uses_button {
                DANCE_BUTTONS
                    .iter()
                    .chain(PUMP_BUTTONS.iter())
                    .copied()
                    .map(Some)
                    .collect()
            } else {
                vec![None]
            };
            for button in buttons {
                let key = button.map_or_else(|| key.clone(), |button| format!("{key}|{button}"));
                let decl = noteskin_actor::parse_actor_for_button(&content, &data.metrics, button);
                if uses_color {
                    for color in crate::Quantization::ALL {
                        let mut variant = decl.clone();
                        color_paths(&content, button, color, &mut variant).map_err(|err| {
                            format!(
                                "failed to compile '{}' ({}, {}): {err}",
                                path.display(),
                                button.unwrap_or("Down"),
                                color.color_name()
                            )
                        })?;
                        if color == crate::Quantization::Q4th {
                            out.push(CompiledActorFile {
                                key: key.clone(),
                                decl: variant.clone(),
                            });
                        }
                        out.push(CompiledActorFile {
                            key: format!("{key}|color={}", color.color_name()),
                            decl: variant,
                        });
                    }
                } else {
                    out.push(CompiledActorFile { key, decl });
                }
            }
            if uses_button {
                let mut decl = noteskin_actor::parse_actor_decl(&content, &data.metrics);
                if uses_color {
                    color_paths(&content, None, crate::Quantization::Q4th, &mut decl)
                        .map_err(|err| format!("failed to compile '{}': {err}", path.display()))?;
                }
                out.push(CompiledActorFile { key, decl });
            }
        }
    }
    out.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(out)
}

fn uses_actor_var(content: &str, name: &str) -> bool {
    let content = noteskin_actor::strip_lua_comments(content);
    content.match_indices("Var").any(|(offset, _)| {
        if offset > 0
            && (content.as_bytes()[offset - 1].is_ascii_alphanumeric()
                || content.as_bytes()[offset - 1] == b'_')
        {
            return false;
        }
        let arg = content[offset + 3..]
            .trim_start()
            .trim_start_matches('(')
            .trim_start();
        matches!(arg.as_bytes().first(), Some(b'\'' | b'"'))
            && crate::lua::itg_quoted_strings(arg).next() == Some(name)
    })
}

fn path_is_lua(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
}

#[derive(Debug, Eq, PartialEq)]
struct LoaderSortKey {
    normalized: String,
    button_len: usize,
}

impl LoaderSortKey {
    fn new(button: &str, element: &str) -> Self {
        let mut normalized = String::with_capacity(button.len() + element.len());
        normalized.push_str(button);
        let button_len = normalized.len();
        normalized.push_str(element);
        normalized.make_ascii_lowercase();
        Self {
            normalized,
            button_len,
        }
    }

    fn button(&self) -> &str {
        &self.normalized[..self.button_len]
    }

    fn element(&self) -> &str {
        &self.normalized[self.button_len..]
    }
}

impl Ord for LoaderSortKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.button()
            .cmp(other.button())
            .then_with(|| self.element().cmp(other.element()))
    }
}

impl PartialOrd for LoaderSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sort_compiled_loader_entries(entries: &mut [CompiledLoaderEntry]) {
    entries.sort_by_cached_key(|entry| {
        (
            LoaderSortKey::new(&entry.button, &entry.element),
            entry.color,
        )
    });
}

fn game_buttons(game: &str) -> &'static [&'static str] {
    if game.trim().eq_ignore_ascii_case("pump") {
        &PUMP_BUTTONS
    } else {
        &DANCE_BUTTONS
    }
}

fn collect_loader_domain(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> (Vec<String>, Vec<String>) {
    let game_buttons = game_buttons(game);
    let mut buttons = Vec::with_capacity(game_buttons.len());
    let mut button_seen = LoaderFingerprints::new();
    for &button in game_buttons {
        push_unique(&mut buttons, &mut button_seen, button);
    }
    let mut elements = Vec::with_capacity(CORE_ELEMENTS.len());
    let mut element_seen = LoaderFingerprints::new();
    for element in CORE_ELEMENTS {
        push_unique(&mut elements, &mut element_seen, element);
    }
    for dir in &data.search_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let stem = trim_variant_suffix(name);
            let Some((button, element)) = split_prefixed_stem(stem, game_buttons) else {
                continue;
            };
            if let Some(button) = button {
                push_unique(&mut buttons, &mut button_seen, button);
            }
            push_unique(&mut elements, &mut element_seen, element);
        }
    }
    (buttons, elements)
}

fn ascii_case_hash(value: &str) -> u64 {
    value.bytes().fold(ASCII_CASE_HASH_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(ASCII_CASE_HASH_PRIME)
    })
}

fn push_unique(out: &mut Vec<String>, seen: &mut LoaderFingerprints, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let fingerprint = ascii_case_hash(trimmed);
    for (index, &existing_fingerprint) in seen.iter().enumerate() {
        if existing_fingerprint == fingerprint && out[index].eq_ignore_ascii_case(trimmed) {
            return;
        }
    }
    seen.push(fingerprint);
    out.push(trimmed.to_string());
}

fn trim_variant_suffix(name: &str) -> &str {
    let stem = name.rsplit_once('.').map_or(name, |(head, _)| head).trim();
    let no_paren = stem
        .rsplit_once(" (")
        .map_or(stem, |(head, _)| head)
        .trim_end();
    match no_paren.rsplit_once(' ') {
        Some((head, tail))
            if tail
                .split_once('x')
                .is_some_and(|(w, h)| digits_only(w) && digits_only(h)) =>
        {
            head.trim_end()
        }
        _ => no_paren,
    }
}

fn split_prefixed_stem<'a>(stem: &'a str, buttons: &[&str]) -> Option<(Option<&'a str>, &'a str)> {
    let trimmed = stem.trim();
    if let Some(rest) = trimmed.strip_prefix("Fallback ") {
        return Some((None, rest.trim()));
    }
    for &button in buttons {
        let Some(rest) = trimmed.strip_prefix(button) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(' ') else {
            continue;
        };
        return Some((Some(&trimmed[..button.len()]), rest.trim()));
    }
    None
}

fn digits_only(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{compiled_bundle_path, noteskin_actor, noteskin_compiled, noteskin_itg};
    use std::{
        ffi::OsStr,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_noteskin_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-noteskin-compiler-{name}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compiled_bundle_path_omits_version_dir() {
        let path = compiled_bundle_path(Path::new("noteskins"), " Dance ", " Default ", "hash123");
        let suffix = Path::new("noteskins")
            .join("dance")
            .join("default")
            .join("hash123.bin");
        let version_dir = format!("v{}", noteskin_compiled::CACHE_SCHEMA_VERSION);
        assert!(path.ends_with(&suffix));
        assert!(
            path.components()
                .all(|component| component.as_os_str() != OsStr::new(&version_dir))
        );
    }

    #[test]
    fn game_aware_loader_keeps_startup_nil_and_request_redirects_through_cache() {
        let root = temp_noteskin_dir("game-aware");
        fs::write(
            root.join("NoteSkin.lua"),
            include_str!("../tests/fixtures/game-aware/NoteSkin.lua"),
        )
        .unwrap();
        let data = noteskin_itg::NoteskinData {
            overrides: Vec::new(),
            name: "game-aware".into(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone()],
        };
        for (game, buttons, rotation) in [
            ("dance", vec![("Left", "Outer"), ("Down", "Inner")], 0),
            ("pump", vec![("Center", "Outer"), ("UpLeft", "Inner")], 90),
        ] {
            let bundle = super::compile_data(game, &data, "fixture", "")
                .expect("compile synthetic game-aware loader");
            let path = root.join(format!("{game}.bin"));
            noteskin_compiled::save_compiled_bundle(&path, &bundle).unwrap();
            let cached = noteskin_compiled::load_compiled_bundle(&path).unwrap();
            for loader in [&bundle.loader, &cached.loader] {
                for (button, target) in &buttons {
                    for element in ["Tap Note", "Hold Head Active", "Roll Head Inactive"] {
                        let request = loader.load_request(button, element);
                        assert!(!request.blank);
                        assert_eq!(request.load_button, *target);
                        assert_eq!(request.load_element, "Tap Note");
                        assert_eq!(request.rotation_z, Some(rotation));
                    }
                    for element in ["Hold Tail Active", "Roll Tail Inactive"] {
                        assert!(loader.load_request(button, element).blank);
                    }
                }
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loader_reads_theme_author_and_metrics() {
        let root = temp_noteskin_dir("loader-metadata");
        fs::write(
            root.join("metrics.ini"),
            "[ReceptorArrow]\nFont=some author\nValue=local\n[NoteDisplay]\nValue=fallback\nBlank=\n",
        )
        .unwrap();
        fs::write(
            root.join("NoteSkin.lua"),
            r#"
return { Load = function()
    assert(THEME:GetThemeAuthor() == "")
    assert(NOTESKIN:GetMetric("receptorarrow", "value") == "local")
    assert(NOTESKIN:GetMetric("Left", "Value") == "fallback")
    assert(NOTESKIN:GetMetric("Left", "Blank") == "")
    assert(NOTESKIN:GetMetric("Left", "Missing") == "")
    local actor = LoadActor(NOTESKIN:GetPath("Down", "Tap Note"))
    if string.find(THEME:GetThemeAuthor(), NOTESKIN:GetMetric("ReceptorArrow", "Font")) then
        actor.BaseRotationY = 90
    end
    return actor
end }
"#,
        )
        .unwrap();
        let data = noteskin_itg::NoteskinData {
            overrides: Vec::new(),
            name: "loader-metadata".into(),
            metrics: noteskin_itg::IniData::parse_file(&root.join("metrics.ini")).unwrap(),
            search_dirs: vec![root.clone()],
        };
        let bundle = super::compile_data("dance", &data, "test", "").expect("compile loader");
        let path = root.join("compiled.bin");
        noteskin_compiled::save_compiled_bundle(&path, &bundle).unwrap();
        let bundle = noteskin_compiled::load_compiled_bundle(&path).unwrap();
        let request = bundle.loader.load_request("Left", "Tap Note");
        assert_eq!(request.load_button, "Down");
        assert_eq!(request.load_element, "Tap Note");
        assert_eq!(request.rotation_y, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loader_keeps_color_variants_through_cache() {
        use crate::Quantization;
        let root = temp_noteskin_dir("loader-colors");
        let fallback = root.join("fallback");
        fs::create_dir_all(&fallback).unwrap();
        fs::write(
            fallback.join("NoteSkin.lua"),
            r#"
return { Load = function()
    local element = Var "Element"
    if element == "Tap Fake" or string.find(element, "Head") then element = "Tap Note" end
    if Var('Color') == "16th" and element == "Tap Note" then return Def.Actor {} end
    return LoadActor(NOTESKIN:GetPath("Down", element))
end }
"#,
        )
        .unwrap();
        fs::write(root.join("NoteSkin.lua"), r#"
local skin = ...
local load = skin.Load
function skin.Load()
    local actor = load()
    local element = Var "Element"
    if Var "Color" == "8th" and (element == "Tap Note" or element == "Tap Fake" or string.find(element, "Head")) then
        actor.BaseRotationZ = ({Left=90, Down=0, Up=180, Right=-90})[Var "Button"]
        actor.InitCommand = function(self) self:zoom(0.75) end
    end
    if Var("Color") == "12th" and element == "Tap Note" then
        return LoadActor(NOTESKIN:GetPath("Down", "Alternate"))
    end
    return actor
end
return skin
"#).unwrap();
        let data = noteskin_itg::NoteskinData {
            overrides: Vec::new(),
            name: "loader-colors".into(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![root.clone(), fallback],
        };
        let bundle = super::compile_data("dance", &data, "fixture", "").unwrap();
        let path = root.join("compiled.bin");
        noteskin_compiled::save_compiled_bundle(&path, &bundle).unwrap();
        let mut bundle = noteskin_compiled::load_compiled_bundle(&path).unwrap();
        let loader = &bundle.loader;
        assert!(loader.has_color_variants());
        assert!(
            loader
                .entries
                .iter()
                .all(|entry| entry.color.is_none_or(|color| matches!(
                    color,
                    Quantization::Q8th | Quantization::Q12th | Quantization::Q16th
                )))
        );
        for (button, rotation) in [("left", 90), ("DOWN", 0), ("Up", 180), ("Right", -90)] {
            for color in Quantization::ALL {
                for element in [
                    "Tap Note",
                    "Tap Fake",
                    "Hold Head Active",
                    "Hold Head Inactive",
                    "Roll Head Active",
                    "Roll Head Inactive",
                ] {
                    let request = loader.load_request_color_ref(button, element, Some(color));
                    assert_eq!(
                        request.rotation_z,
                        (color == Quantization::Q8th).then_some(rotation)
                    );
                    assert_eq!(
                        request.init_command,
                        (color == Quantization::Q8th).then_some("zoom,0.75")
                    );
                    assert_eq!(request.blank, color == Quantization::Q16th);
                    if !request.blank {
                        assert_eq!(request.load_button, "Down");
                        assert_eq!(
                            request.load_element,
                            if color == Quantization::Q12th && element == "Tap Note" {
                                "Alternate"
                            } else {
                                "Tap Note"
                            }
                        );
                    }
                }
                assert_eq!(
                    loader.load_request_color_ref(button, "Receptor", Some(color)),
                    loader.load_request_ref(button, "Receptor")
                );
            }
            assert_eq!(
                loader.load_request_ref(button, "Tap Note"),
                loader.load_request_color_ref(button, "Tap Note", Some(Quantization::Q4th))
            );
        }
        bundle.version -= 1;
        let old_path = root.join("old.bin");
        noteskin_compiled::save_compiled_bundle(&old_path, &bundle).unwrap();
        assert!(noteskin_compiled::load_compiled_bundle(&old_path).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loader_loadfile_accepts_noteskin_path_tables() {
        let root = temp_noteskin_dir("loadfile-path-table");
        let skin_dir = root.join("dance/sch");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Left = "Down", Down = "Down", Up = "Down", Right = "Down" }
skin.PartsToRotate = { Receptor = true, ["Hold Body Active"] = true }
skin.Rotate = { Left = 90, Down = 0, Up = 180, Right = -90 }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local actor_file = loadfile(NOTESKIN:GetPath(load_button, element))
    local actor
    if type(actor_file) == "function" then
        actor = actor_file(nil)
    else
        actor = Def.Sprite {
            Texture = NOTESKIN:GetPath(load_button, element),
            BaseRotationX = 180,
            BaseRotationY = 180,
        }
    end
    if skin.PartsToRotate[element] then
        actor.BaseRotationZ = skin.Rotate[button]
    end
    return actor
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Receptor.lua"),
            r#"return Def.Sprite { Texture=NOTESKIN:GetPath("Down", "Receptor") }"#,
        )
        .unwrap();
        fs::write(skin_dir.join("Down Hold Body Active.png"), []).unwrap();
        fs::write(
            skin_dir.join("Fallback Explosion.lua"),
            r#"return Def.Actor {}"#,
        )
        .unwrap();

        let data = noteskin_itg::NoteskinData {
            overrides: Vec::new(),
            name: "sch".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![skin_dir],
        };
        let bundle = super::compile_data("dance", &data, "testhash", "").expect("compile data");
        let receptor = bundle.loader.load_request("Left", "Receptor");
        let hold_body = bundle.loader.load_request("Left", "Hold Body Active");
        let explosion = bundle.loader.load_request("Left", "Explosion");

        assert_eq!(receptor.load_button, "Down");
        assert_eq!(receptor.load_element, "Receptor");
        assert_eq!(receptor.rotation_z, Some(90));
        assert_eq!(hold_body.load_button, "Down");
        assert_eq!(hold_body.load_element, "Hold Body Active");
        assert_eq!(hold_body.rotation_x, Some(180));
        assert_eq!(hold_body.rotation_y, Some(180));
        assert_eq!(hold_body.rotation_z, Some(90));
        assert_eq!(explosion.load_button, "Down");
        assert_eq!(explosion.load_element, "Explosion");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn actor_decl_ignores_non_color_local_assignments() {
        let decl = noteskin_actor::parse_actor_decl(
            r#"
local button = Var "Button"
local path = NOTESKIN:GetPath(button, "Tap Note")
return Def.Sprite {
    Texture=path;
    InitCommand=cmd(diffusealpha,1);
}
"#,
            &noteskin_itg::IniData::default(),
        );

        let sprite = decl.sprites.first().expect("sprite should parse");
        assert_eq!(
            sprite.commands.get("initcommand").map(String::as_str),
            Some("diffusealpha,1")
        );
    }

    #[test]
    fn actor_decl_preserves_repeated_sprite_frame_states() {
        let decl = noteskin_actor::parse_actor_decl(
            r#"
return Def.Sprite {
    Texture=NOTESKIN:GetPath('_Down', 'roll body active');
    Frame0000=0;
    Delay0000=0.44;
    Frame0001=1;
    Delay0001=0.03;
    Frame0002=2;
    Delay0002=0.03;
    Frame0003=3;
    Delay0003=0.44;
    Frame0004=2;
    Delay0004=0.03;
    Frame0005=1;
    Delay0005=0.03;
};
"#,
            &noteskin_itg::IniData::default(),
        );

        let sprite = decl.sprites.first().expect("sprite should parse");
        assert_eq!(sprite.frame0, 0);
        assert_eq!(sprite.frame_count, 6);
        assert_eq!(
            sprite.frame_indices.as_deref(),
            Some([0, 1, 2, 3, 2, 1].as_slice())
        );
        let delays = sprite
            .frame_delays
            .as_deref()
            .expect("sprite frame delays should parse");
        assert_eq!(delays, [0.44, 0.03, 0.03, 0.44, 0.03, 0.03]);
        assert!((delays.iter().sum::<f32>() - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn actor_decl_expands_local_lua_command_helpers() {
        let decl = noteskin_actor::parse_actor_decl(
            r##"
local W2colour = color("#FFC917")
local Lastcolour = color("#00C8FF")

local function flashadd(thecolour, updatelast)
    return function(self)
        if updatelast then
            Lastcolour = thecolour
        end
        self:finishtweening()
        :diffuse(thecolour)
        :blend(Blend.Add)
        :diffusealpha(1.0)
        :linear(1/60)
        :diffusealpha(0.5)
        :linear(3/60)
        :diffusealpha(0.0)
    end
end

local function flashnormal(thecolour, uselast)
    return function(self)
        if uselast then
            self:finishtweening()
            :diffusealpha(1.0)
            :linear(10/60)
            :diffusealpha(0.0)
        else
            self:finishtweening()
            :diffuse(thecolour)
            :diffusealpha(1.0)
            :linear(10/60)
            :diffusealpha(0.0)
        end
    end
end

return Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath(Var "Button", "Flash");
        InitCommand=cmd(diffusealpha,0);
        W2Command=flashadd(W2colour,true);
        HeldCommand=flashnormal(Lastcolour,true);
        ECommand=function(self) self:blend("BlendMode_Normal"):diffusealpha(1.0):zoom(0.75):accelerate(64/60):diffusealpha(0.0):zoom(1.0):setstate(0):animate(true) end;
        JudgmentCommand=function(self) end;
    };
}
"##,
            &noteskin_itg::IniData::default(),
        );
        let sprite = decl.sprites.first().expect("sprite should parse");

        let w2 = sprite
            .commands
            .get("w2command")
            .expect("W2 command should compile");
        assert!(w2.contains("diffuse,1,0.7882353,0.09019608,1"));
        assert!(w2.contains("blend,Blend.Add"));
        assert!(w2.contains("linear,1/60"));
        assert!(!w2.contains("flashadd"));

        let held = sprite
            .commands
            .get("heldcommand")
            .expect("Held command should compile");
        assert!(held.contains("diffusealpha,1"));
        assert!(held.contains("linear,10/60"));
        assert!(!held.contains("diffuse,0,0.78431374,1,1"));
        assert!(!held.contains("flashnormal"));

        let e = sprite
            .commands
            .get("ecommand")
            .expect("E command should compile");
        assert!(e.contains("blend,\"BlendMode_Normal\""));
        assert!(e.contains("diffusealpha,1.0"));
        assert!(e.contains("zoom,0.75"));
        assert!(e.contains("accelerate,64/60"));
        assert!(e.contains("setstate,0"));
        assert!(e.contains("animate,true"));

        assert_eq!(
            sprite.commands.get("judgmentcommand").map(String::as_str),
            Some("")
        );
    }
}

#[cfg(test)]
#[path = "compiler_resources_perf.rs"]
mod resource_perf;

type CompileResult<T> = std::result::Result<T, String>;

/// Compile an isolated skin without consulting or populating the session cache.
/// An empty source hash is suitable for a song-lifetime, in-memory bundle.
pub fn compile_data(
    game: &str,
    data: &noteskin_itg::NoteskinData,
    source_hash: &str,
    player_options: &str,
) -> CompileResult<CompiledNoteskinBundle> {
    let mut session = Session::new()?;
    session.install(game, data, player_options)?;
    let mut current = LuaValue::Nil;
    let mut colors = false;
    let scripts = noteskin_paths(data);
    if scripts.is_empty() {
        return Err("no NoteSkin.lua files were found in fallback chain".into());
    }
    for path in scripts {
        let content = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
        colors |= uses_actor_var(&content, "Color");
        let function = session
            .compile(&content, &path.to_string_lossy())
            .map_err(|err| format!("failed to compile '{}': {err}", path.display()))?;
        let arguments = if current == LuaValue::Nil {
            vec![]
        } else {
            vec![current]
        };
        current = session
            .call(function, &arguments)
            .map_err(|err| format!("failed to execute '{}': {err}", path.display()))?;
        table(current)?;
    }
    let entries = session.entries(current, game, data, colors)?;
    let files = compile_actor_files_with(data, |content, button, color, decl| {
        session.scoped(|session| session.color_paths(content, button, color, decl))
    })?;
    Ok(CompiledNoteskinBundle {
        version: noteskin_compiled::CACHE_SCHEMA_VERSION,
        game: game.into(),
        skin: data.name.clone(),
        source_hash: source_hash.into(),
        loader: CompiledLoader {
            version: COMPILER_VERSION,
            game: game.into(),
            skin: data.name.clone(),
            entries,
        },
        actors: CompiledActors {
            version: COMPILER_VERSION,
            files,
        },
    })
}

fn error(error: Error) -> String {
    error.to_string()
}
fn table(value: LuaValue) -> CompileResult<deadlib_lua::TableId> {
    match value {
        LuaValue::Table(id) => Ok(id),
        _ => Err("noteskin table expected".into()),
    }
}

// Values held by preparation are explicitly rooted. The entire session, its
// handles and Lua state are released after producing the native bundle.
struct Session {
    vm: Vm,
    roots: Vec<Root>,
    globals: LuaValue,
    get: LuaValue,
    set: LuaValue,
    next: LuaValue,
    setmetatable: LuaValue,
    setfenv: LuaValue,
    tonumber: LuaValue,
}

impl Session {
    fn new() -> CompileResult<Self> {
        let mut vm = Vm::new();
        let globals = vm.global(b"_G");
        let get = vm.new_host_function(get_field, &[]).map_err(error)?;
        let set = vm.new_host_function(set_field, &[]).map_err(error)?;
        let next = vm.global(b"next");
        let setmetatable = vm.global(b"setmetatable");
        let setfenv = vm.global(b"setfenv");
        let tonumber = vm.global(b"tonumber");
        let mut session = Self {
            vm,
            roots: Vec::new(),
            globals,
            get,
            set,
            next,
            setmetatable,
            setfenv,
            tonumber,
        };
        for value in [globals, get, set, next, setmetatable, setfenv, tonumber] {
            session.keep(value)?;
        }
        Ok(session)
    }

    fn keep(&mut self, value: LuaValue) -> CompileResult<LuaValue> {
        if !matches!(
            value,
            LuaValue::Nil | LuaValue::Boolean(_) | LuaValue::Number(_)
        ) {
            self.roots.push(self.vm.pin(value).map_err(error)?);
        }
        Ok(value)
    }

    // Only native data may leave this scope. Lua values retained by scripts
    // remain reachable through ordinary Lua edges after temporary roots end.
    fn scoped<T>(&mut self, run: impl FnOnce(&mut Self) -> CompileResult<T>) -> CompileResult<T> {
        let start = self.roots.len();
        let result = run(self);
        for root in self.roots.drain(start..) {
            self.vm.unpin(root).map_err(error)?;
        }
        result
    }
    fn string(&mut self, value: &str) -> CompileResult<LuaValue> {
        let value = self.vm.try_intern(value.as_bytes()).map_err(error)?;
        self.keep(value)
    }
    fn text(&mut self, value: LuaValue) -> CompileResult<String> {
        let id = self
            .vm
            .coerce_string(value)
            .map_err(error)?
            .ok_or_else(|| "noteskin string expected".to_owned())?;
        std::str::from_utf8(self.vm.string_bytes(id).expect("live string"))
            .map(str::to_owned)
            .map_err(|err| err.to_string())
    }
    fn optional_text(&mut self, value: LuaValue) -> CompileResult<Option<String>> {
        if value == LuaValue::Nil {
            Ok(None)
        } else {
            self.text(value).map(Some)
        }
    }
    fn table(&mut self) -> CompileResult<LuaValue> {
        let value = LuaValue::Table(self.vm.try_new_table().map_err(error)?);
        self.keep(value)
    }
    fn call(&mut self, function: LuaValue, args: &[LuaValue]) -> CompileResult<LuaValue> {
        let value = self
            .vm
            .call(function, args)
            .map_err(error)?
            .first()
            .copied()
            .unwrap_or(LuaValue::Nil);
        self.keep(value)
    }
    fn field(&mut self, target: LuaValue, name: &str) -> CompileResult<LuaValue> {
        let key = self.string(name)?;
        self.call(self.get, &[target, key])
    }
    fn put(&mut self, target: LuaValue, name: &str, value: LuaValue) -> CompileResult<()> {
        let key = self.string(name)?;
        self.call(self.set, &[target, key, value])?;
        Ok(())
    }
    fn put_text(&mut self, target: LuaValue, name: &str, value: &str) -> CompileResult<()> {
        let value = self.string(value)?;
        self.put(target, name, value)
    }
    fn raw(&mut self, target: LuaValue, index: usize) -> CompileResult<LuaValue> {
        let value = self
            .vm
            .raw_table_get(table(target)?, LuaValue::Number(index as f64))
            .map_err(error)?;
        self.keep(value)
    }
    fn len(&self, target: LuaValue) -> CompileResult<usize> {
        self.vm.raw_table_len(table(target)?).map_err(error)
    }
    fn compile(&mut self, source: &str, name: &str) -> CompileResult<LuaValue> {
        let function = self
            .vm
            .try_load_source(source.as_bytes(), name.as_bytes())
            .map_err(error)?;
        self.keep(LuaValue::Function(function))
    }
    fn bind(
        &mut self,
        target: LuaValue,
        name: &str,
        binding: Binding,
        captures: &[LuaValue],
    ) -> CompileResult<LuaValue> {
        let function = self
            .vm
            .new_host_function_with_data(dispatch, captures, binding)
            .map_err(error)?;
        self.keep(function)?;
        self.put(target, name, function)?;
        Ok(function)
    }
    fn callback(
        &mut self,
        function: HostFunction,
        captures: &[LuaValue],
    ) -> CompileResult<LuaValue> {
        let function = self
            .vm
            .new_host_function(function, captures)
            .map_err(error)?;
        self.keep(function)
    }

    fn install(
        &mut self,
        game: &str,
        data: &noteskin_itg::NoteskinData,
        options: &str,
    ) -> CompileResult<()> {
        let globals = self.globals;
        // Match the safe standard-library subset previously opened by Lua::new.
        // Removing both references also prevents require from returning debug.
        self.put(globals, "debug", LuaValue::Nil)?;
        let package = self.field(globals, "package")?;
        let loaded = self.field(package, "loaded")?;
        self.put(loaded, "debug", LuaValue::Nil)?;
        let denied = self.callback(deny_loadlib, &[])?;
        self.put(package, "loadlib", denied)?;
        let loaders = self.field(package, "loaders")?;
        let diagnostic = self.string("\n\tcan't load C modules in safe mode")?;
        let c_loader = self
            .vm
            .new_host_function_with_data(dispatch, &[diagnostic], Binding::Constant)
            .map_err(error)?;
        self.keep(c_loader)?;
        self.vm
            .raw_table_set(table(loaders)?, LuaValue::Number(3.0), c_loader)
            .map_err(error)?;
        self.vm
            .raw_table_set(table(loaders)?, LuaValue::Number(4.0), LuaValue::Nil)
            .map_err(error)?;
        let player = self.table()?;
        let options = self.string(options)?;
        self.bind(
            player,
            "GetPlayerOptionsString",
            Binding::Constant,
            &[options],
        )?;
        let current_game = self.table()?;
        let game = self.string(&game.trim().to_ascii_lowercase())?;
        self.bind(current_game, "GetName", Binding::Constant, &[game])?;
        let state = self.table()?;
        self.bind(state, "GetCurrentGame", Binding::Constant, &[current_game])?;
        self.bind(state, "GetPlayerState", Binding::Constant, &[player])?;
        self.put(globals, "GAMESTATE", state)?;
        let theme = self.table()?;
        let empty = self.string("")?;
        self.bind(theme, "GetThemeAuthor", Binding::Constant, &[empty])?;
        self.put(globals, "THEME", theme)?;

        let actor_mt = self.table()?;
        let methods = self.table()?;
        for name in [
            "x",
            "y",
            "z",
            "addx",
            "addy",
            "addz",
            "rotationx",
            "rotationy",
            "rotationz",
            "addrotationx",
            "addrotationy",
            "addrotationz",
            "zoom",
            "zoomx",
            "zoomy",
            "zoomz",
            "diffuse",
            "diffusealpha",
            "glow",
            "vertalign",
            "valign",
            "blend",
            "visible",
            "SetTextureFiltering",
        ] {
            self.bind(methods, name, Binding::Command(name), &[])?;
        }
        self.put(actor_mt, "__index", methods)?;
        self.bind(actor_mt, "__concat", Binding::Identity, &[])?;
        let make_actor = self.callback(make_actor, &[actor_mt, self.setmetatable])?;
        let load_actor = self.bind(globals, "LoadActor", Binding::LoadActor, &[make_actor])?;
        self.bind(globals, "Var", Binding::Var, &[globals])?;
        self.bind(globals, "cmd", Binding::Constant, &[LuaValue::Nil])?;
        let noteskin = self.table()?;
        let data = Arc::new(data.clone());
        self.bind(noteskin, "GetMetric", Binding::Metric(data.clone()), &[])?;
        self.bind(noteskin, "GetPath", Binding::Path, &[])?;
        self.put(globals, "NOTESKIN", noteskin)?;
        let def = self.table()?;
        self.bind(def, "Actor", Binding::Actor, &[make_actor])?;
        self.bind(def, "Sprite", Binding::Sprite, &[make_actor, load_actor])?;
        self.put(globals, "Def", def)?;
        self.bind(globals, "loadfile", Binding::LoadFile(data), &[make_actor])?;
        Ok(())
    }

    fn normalize(
        &mut self,
        noteskin: LuaValue,
        buttons: &[String],
        elements: &[String],
    ) -> CompileResult<()> {
        for (names, canonical) in [
            (
                &["RedirTable", "ButtonRedir", "ButtonRedirs", "Rotate"][..],
                buttons,
            ),
            (
                &[
                    "ElementRedir",
                    "ElementRedirs",
                    "PartsToRotate",
                    "Blank",
                    "bBlanks",
                ][..],
                elements,
            ),
        ] {
            for name in names {
                let target = self.field(noteskin, name)?;
                if target == LuaValue::Nil {
                    continue;
                }
                table(target)?;
                let mut existing = Vec::new();
                let mut key = LuaValue::Nil;
                loop {
                    let values = self
                        .vm
                        .call(self.next, &[target, key])
                        .map_err(error)?
                        .to_vec();
                    key = values.first().copied().unwrap_or(LuaValue::Nil);
                    if key == LuaValue::Nil {
                        break;
                    }
                    self.keep(key)?;
                    self.keep(values[1])?;
                    if let LuaValue::String(id) = key
                        && let Ok(text) =
                            std::str::from_utf8(self.vm.string_bytes(id).expect("live key"))
                    {
                        existing.push((text.to_owned(), values[1]));
                    }
                }
                for want in canonical {
                    if self.field(target, want)? != LuaValue::Nil {
                        continue;
                    }
                    if let Some((_, value)) = existing
                        .iter()
                        .find(|(have, _)| have.eq_ignore_ascii_case(want))
                    {
                        self.put(target, want, *value)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn entries(
        &mut self,
        noteskin: LuaValue,
        game: &str,
        data: &noteskin_itg::NoteskinData,
        colors: bool,
    ) -> CompileResult<Vec<CompiledLoaderEntry>> {
        let (buttons, elements) = collect_loader_domain(game, data);
        self.normalize(noteskin, &buttons, &elements)?;
        let load = self.field(noteskin, "Load")?;
        if !matches!(load, LuaValue::Function(_) | LuaValue::Native(_)) {
            return Err("compiled noteskin is missing Load()".into());
        }
        let mut out = Vec::new();
        for button in &buttons {
            for element in &elements {
                self.put_text(self.globals, "__itg_button", button)?;
                self.put_text(self.globals, "__itg_element", element)?;
                self.put(self.globals, "__itg_sprite_only", LuaValue::Boolean(true))?;
                let default = out.len();
                let colors = if colors {
                    &Quantization::ALL[..]
                } else {
                    &Quantization::ALL[..1]
                };
                for color in colors {
                    let mut entry = self.scoped(|session| {
                        session.put_text(session.globals, "__itg_color", color.color_name())?;
                        let actor = session.call(load, &[]).map_err(|err| {
                            format!(
                                "Load() failed for '{button} {element}' ({}): {err}",
                                color.color_name()
                            )
                        })?;
                        session.read_entry(button, element, actor)
                    })?;
                    if *color == Quantization::Q4th {
                        out.push(entry);
                    } else if entry != out[default] {
                        entry.color = Some(*color);
                        out.push(entry);
                    }
                }
            }
        }
        self.put_text(self.globals, "__itg_color", "4th")?;
        sort_compiled_loader_entries(&mut out);
        Ok(out)
    }

    fn read_entry(
        &mut self,
        button: &str,
        element: &str,
        actor: LuaValue,
    ) -> CompileResult<CompiledLoaderEntry> {
        table(actor)?;
        let blank = !matches!(
            self.field(actor, "__blank").unwrap_or(LuaValue::Nil),
            LuaValue::Nil | LuaValue::Boolean(false)
        );
        let value = self.field(actor, "__load_button")?;
        let load_button = self.optional_text(value)?.unwrap_or_else(|| button.into());
        let value = self.field(actor, "__load_element")?;
        let load_element = self.optional_text(value)?.unwrap_or_else(|| element.into());
        let mut rotations = [None; 3];
        for (rotation, key) in
            rotations
                .iter_mut()
                .zip(["BaseRotationX", "BaseRotationY", "BaseRotationZ"])
        {
            if let Ok(value) = self.field(actor, key)
                && let Ok(LuaValue::Number(n)) = self.call(self.tonumber, &[value])
                && n.is_finite()
                && n.trunc() >= i32::MIN as f64
                && n.trunc() <= i32::MAX as f64
            {
                *rotation = Some(n as i32);
            }
        }
        let init = self.field(actor, "InitCommand")?;
        let mut init_command = None;
        match init {
            LuaValue::Function(_) | LuaValue::Native(_) => {
                self.call(init, &[actor])?;
            }
            LuaValue::String(_) => {
                let text = self.text(init)?;
                if !text.trim().is_empty() {
                    init_command = Some(text);
                }
            }
            _ => {}
        }
        if init_command.is_none() {
            let commands = self.field(actor, "__loader_commands")?;
            if commands != LuaValue::Nil {
                let mut out = Vec::new();
                let mut index = 1;
                loop {
                    let value = self.raw(commands, index)?;
                    if value == LuaValue::Nil {
                        break;
                    }
                    let command = self.text(value)?;
                    if !command.trim().is_empty() {
                        out.push(command);
                    }
                    index += 1;
                }
                if !out.is_empty() {
                    init_command = Some(out.join(";"));
                }
            }
        }
        Ok(CompiledLoaderEntry {
            button: button.into(),
            element: element.into(),
            color: None,
            load_button,
            load_element,
            blank,
            rotation_x: rotations[0],
            rotation_y: rotations[1],
            rotation_z: rotations[2],
            init_command,
        })
    }

    fn color_paths(
        &mut self,
        content: &str,
        button: Option<&str>,
        color: Quantization,
        decl: &mut noteskin_actor::ItgLuaActorDecl,
    ) -> CompileResult<()> {
        let env = self.table()?;
        let mt = self.table()?;
        self.put(mt, "__index", self.globals)?;
        self.call(self.setmetatable, &[env, mt])?;
        let original = self.field(self.globals, "Var")?;
        let color = self.string(color.color_name())?;
        let button = self.string(button.unwrap_or("Down"))?;
        self.bind(env, "Var", Binding::ColorVar, &[original, color, button])?;
        let def = self.table()?;
        let models = self.table()?;
        let sprites = self.table()?;
        self.bind(def, "Model", Binding::Capture, &[models])?;
        self.bind(def, "Sprite", Binding::Capture, &[sprites])?;
        self.bind(def, "ActorFrame", Binding::Identity, &[])?;
        self.bind(def, "Actor", Binding::Identity, &[])?;
        self.put(env, "Def", def)?;
        let function = self.compile(content, "=(noteskin actor)")?;
        self.call(self.setfenv, &[function, env])?;
        self.call(function, &[])?;
        if self.len(models)? != decl.models.len() || self.len(sprites)? != decl.sprites.len() {
            return Err("color-dependent actor structure is unsupported".into());
        }
        for (index, model) in decl.models.iter_mut().enumerate() {
            let props = self.raw(models, index + 1)?;
            let value = self.field(props, "Meshes")?;
            model.meshes_expr = self.path_expr(value)?;
            let value = self.field(props, "Materials")?;
            model.materials_expr = self.path_expr(value)?;
            let value = self.field(props, "Bones")?;
            model.bones_expr = self.path_expr(value)?;
            let value = self.field(props, "Texture")?;
            model.texture_expr = self.path_expr(value)?;
        }
        for (index, sprite) in decl.sprites.iter_mut().enumerate() {
            let props = self.raw(sprites, index + 1)?;
            let value = self.field(props, "Texture")?;
            if let Some(path) = self.path_expr(value)? {
                sprite.texture_expr = path;
            }
        }
        Ok(())
    }
    fn path_expr(&mut self, value: LuaValue) -> CompileResult<Option<String>> {
        Ok(match value {
            LuaValue::Nil => None,
            LuaValue::String(_) => Some(format!("{:?}", self.text(value)?)),
            LuaValue::Table(_) => {
                let button = self.field(value, "load_button")?;
                let button = self.text(button)?;
                let element = self.field(value, "load_element")?;
                let element = self.text(element)?;
                Some(format!("NOTESKIN:GetPath({button:?},{element:?})"))
            }
            _ => return Err("invalid noteskin resource path".into()),
        })
    }
}

#[cfg(test)]
mod session_tests;

#[derive(Clone)]
enum Binding {
    Constant,
    Identity,
    Actor,
    Sprite,
    LoadActor,
    Var,
    Path,
    Capture,
    ColorVar,
    Command(&'static str),
    Metric(Arc<noteskin_itg::NoteskinData>),
    LoadFile(Arc<noteskin_itg::NoteskinData>),
}
type HostResult = std::result::Result<(), Error>;
fn deny_loadlib(call: &mut HostCall<'_>) -> HostResult {
    Err(call.error(b"package.loadlib is disabled in safe mode"))
}
fn argument(call: &HostCall<'_>, index: usize) -> LuaValue {
    call.arguments()
        .get(index)
        .copied()
        .unwrap_or(LuaValue::Nil)
}
fn host_table(
    call: &HostCall<'_>,
    value: LuaValue,
) -> std::result::Result<deadlib_lua::TableId, Error> {
    match value {
        LuaValue::Table(id) => Ok(id),
        _ => Err(call.error(b"noteskin table expected")),
    }
}
fn host_text(call: &mut HostCall<'_>, value: LuaValue) -> std::result::Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"noteskin string expected"))?;
    std::str::from_utf8(call.string_bytes(id).expect("live string"))
        .map(str::to_owned)
        .map_err(|_| call.error(b"invalid UTF-8 noteskin string"))
}
fn optional_host_text(
    call: &mut HostCall<'_>,
    value: LuaValue,
) -> std::result::Result<Option<String>, Error> {
    if value == LuaValue::Nil {
        Ok(None)
    } else {
        host_text(call, value).map(Some)
    }
}
fn field(
    call: &mut HostCall<'_>,
    target: LuaValue,
    name: &str,
) -> std::result::Result<LuaValue, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(target, key)
}
fn put(call: &mut HostCall<'_>, target: LuaValue, name: &str, value: LuaValue) -> HostResult {
    let key = call.try_intern(name.as_bytes())?;
    call.table_set(target, key, value)
}
fn get_field(call: &mut HostCall<'_>) -> HostResult {
    let value = call.table_get(argument(call, 0), argument(call, 1))?;
    call.push(value)
}
fn set_field(call: &mut HostCall<'_>) -> HostResult {
    call.table_set(argument(call, 0), argument(call, 1), argument(call, 2))
}
fn make_actor(call: &mut HostCall<'_>) -> HostResult {
    let actor = LuaValue::Table(call.try_new_table()?);
    call.push(actor)?;
    put(call, actor, "__blank", argument(call, 0))?;
    for (index, name) in [(1, "__load_button"), (2, "__load_element")] {
        let value = argument(call, index);
        if value != LuaValue::Nil {
            put(call, actor, name, value)?;
        }
    }
    let mt = call.captures()[0];
    let setmetatable = call.captures()[1];
    call.call(setmetatable, &[actor, mt])?;
    Ok(())
}
fn deferred_actor(call: &mut HostCall<'_>) -> HostResult {
    let captures = [call.captures()[0], call.captures()[1], call.captures()[2]];
    call.call_and_return(
        captures[0],
        &[LuaValue::Boolean(false), captures[1], captures[2]],
    )
}
fn path_parts(
    call: &mut HostCall<'_>,
    value: LuaValue,
) -> std::result::Result<(Option<String>, Option<String>), Error> {
    match value {
        LuaValue::String(_) => Ok((None, Some(host_text(call, value)?))),
        LuaValue::Table(_) => {
            let button = field(call, value, "load_button")?;
            let button = optional_host_text(call, button)?;
            let element = field(call, value, "load_element")?;
            Ok((button, optional_host_text(call, element)?))
        }
        _ => Ok((None, None)),
    }
}
fn lua_text(call: &mut HostCall<'_>, value: Option<&str>) -> std::result::Result<LuaValue, Error> {
    value.map_or(Ok(LuaValue::Nil), |text| call.try_intern(text.as_bytes()))
}
fn dispatch(call: &mut HostCall<'_>) -> HostResult {
    let binding = call
        .function_data::<Binding>()
        .expect("noteskin binding")
        .clone();
    let first = argument(call, 0);
    match binding {
        Binding::Constant => call.push(call.captures()[0]),
        Binding::Identity => {
            host_table(call, first)?;
            call.push(first)
        }
        Binding::Actor => call.call_and_return(
            call.captures()[0],
            &[LuaValue::Boolean(true), LuaValue::Nil, LuaValue::Nil],
        ),
        Binding::LoadActor => {
            let (button, element) = path_parts(call, first)?;
            let blank = element
                .as_deref()
                .is_some_and(|text| text.eq_ignore_ascii_case("_blank"));
            let button = lua_text(call, button.as_deref())?;
            let element = lua_text(call, element.as_deref())?;
            call.call_and_return(
                call.captures()[0],
                &[LuaValue::Boolean(blank), button, element],
            )
        }
        Binding::Sprite => {
            let make = call.captures()[0];
            let load = call.captures()[1];
            let texture = if matches!(first, LuaValue::Table(_)) {
                field(call, first, "Texture")?
            } else {
                LuaValue::Nil
            };
            let actor = if texture == LuaValue::Nil {
                call.call(
                    make,
                    &[LuaValue::Boolean(false), LuaValue::Nil, LuaValue::Nil],
                )?[0]
            } else {
                call.call(load, &[texture])?[0]
            };
            call.push(actor)?;
            if matches!(first, LuaValue::Table(_)) {
                for name in [
                    "InitCommand",
                    "BaseRotationX",
                    "BaseRotationY",
                    "BaseRotationZ",
                ] {
                    let value = field(call, first, name)?;
                    if value != LuaValue::Nil {
                        put(call, actor, name, value)?;
                    }
                }
            }
            Ok(())
        }
        Binding::Var => {
            let name = host_text(call, first)?;
            let globals = call.captures()[0];
            let value = match name.as_str() {
                "Button" => field(call, globals, "__itg_button")?,
                "Element" => field(call, globals, "__itg_element")?,
                "Color" => {
                    let value = field(call, globals, "__itg_color")?;
                    let text = optional_host_text(call, value)?.unwrap_or_else(|| "4th".into());
                    call.try_intern(text.as_bytes())?
                }
                "SpriteOnly" => {
                    let value = field(call, globals, "__itg_sprite_only").unwrap_or(LuaValue::Nil);
                    LuaValue::Boolean(!matches!(value, LuaValue::Nil | LuaValue::Boolean(false)))
                }
                "Player" => LuaValue::Number(0.),
                _ => LuaValue::Nil,
            };
            call.push(value)
        }
        Binding::Path => {
            host_table(call, first)?;
            let button = host_text(call, argument(call, 1))?;
            let element = host_text(call, argument(call, 2))?;
            let path = LuaValue::Table(call.try_new_table()?);
            call.push(path)?;
            let button = call.try_intern(button.as_bytes())?;
            put(call, path, "load_button", button)?;
            let element = call.try_intern(element.as_bytes())?;
            put(call, path, "load_element", element)
        }
        Binding::Metric(data) => {
            host_table(call, first)?;
            let section = host_text(call, argument(call, 1))?;
            let key = host_text(call, argument(call, 2))?;
            let value = data
                .metrics
                .get(&section, &key)
                .or_else(|| data.metrics.get("NoteDisplay", &key))
                .unwrap_or("");
            let value = call.try_intern(value.as_bytes())?;
            call.push(value)
        }
        Binding::Command(command) => {
            host_table(call, first)?;
            let mut text = command.to_owned();
            for value in call.arguments().iter().skip(1).copied() {
                text.push(',');
                match value {
                    LuaValue::Boolean(value) => text.push_str(if value { "true" } else { "false" }),
                    LuaValue::Number(value) => text.push_str(&value.to_string()),
                    LuaValue::String(id) => text.push_str(
                        std::str::from_utf8(call.string_bytes(id).expect("live argument"))
                            .map_err(|_| call.error(b"invalid UTF-8 noteskin string"))?,
                    ),
                    _ => {}
                }
            }
            let mut commands = field(call, first, "__loader_commands")?;
            if commands == LuaValue::Nil {
                commands = LuaValue::Table(call.try_new_table()?);
                put(call, first, "__loader_commands", commands)?;
            }
            let id = host_table(call, commands)?;
            let index = call.raw_table_len(id)? + 1;
            let text = call.try_intern(text.as_bytes())?;
            call.raw_table_set(id, LuaValue::Number(index as f64), text)?;
            call.push(first)
        }
        Binding::LoadFile(data) => {
            let (button, element) = match first {
                LuaValue::Table(_) => {
                    let (button, element) = path_parts(call, first)?;
                    let exists = element
                        .as_deref()
                        .and_then(|element| {
                            data.resolve_path(button.as_deref().unwrap_or(""), element)
                        })
                        .is_some_and(|path| path_is_lua(&path));
                    if !exists {
                        return call.push(LuaValue::Nil);
                    }
                    (button, element)
                }
                LuaValue::String(_) => {
                    if !path_is_lua(Path::new(&host_text(call, first)?)) {
                        return call.push(LuaValue::Nil);
                    }
                    (None, None)
                }
                _ => return call.push(LuaValue::Nil),
            };
            let button = lua_text(call, button.as_deref())?;
            let element = lua_text(call, element.as_deref())?;
            let function =
                call.new_host_function(deferred_actor, &[call.captures()[0], button, element])?;
            call.push(function)
        }
        Binding::Capture => {
            host_table(call, first)?;
            let output = host_table(call, call.captures()[0])?;
            let index = call.raw_table_len(output)? + 1;
            call.raw_table_set(output, LuaValue::Number(index as f64), first)?;
            call.push(first)
        }
        Binding::ColorVar => match host_text(call, first)?.as_str() {
            "Color" => call.push(call.captures()[1]),
            "Button" => call.push(call.captures()[2]),
            _ => call.call_and_return(call.captures()[0], &[first]),
        },
    }
}
