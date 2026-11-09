pub use crate::compiled::{CompiledActors, CompiledLoader, actor_manifest_key};
mod rust_engine;
use crate::{
    actor as noteskin_actor, compiled as noteskin_compiled,
    compiled::{CompiledActorFile, CompiledLoaderEntry, CompiledNoteskinBundle},
    itg as noteskin_itg,
};
use log::{info, warn};
pub use rust_engine::compile_data;
use smallvec::SmallVec;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::Read;
use std::path::{Path, PathBuf};
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
