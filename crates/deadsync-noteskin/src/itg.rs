use hashbrown::{Equivalent, HashMap as BorrowMap};
use log::warn;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use crate::{
    NoteAnimPart, NoteColorType, NoteDisplayMetrics, NotePartAnimation, NotePartTextureTranslate,
    Style,
    actor::ITG_ARG0_TOKEN,
    lua::{itg_extract_quoted_strings, itg_parse_lua_quoted},
    script::{parse_script_bool, parse_script_number},
};

const MAX_FALLBACK_DEPTH: usize = 20;
const MAX_REDIR_DEPTH: usize = 100;
const DEFAULT_SKIN_NAME: &str = "default";
const DEFAULT_SKIN_CANDIDATES: &[&str] = &[DEFAULT_SKIN_NAME, "cel"];

type PathLookupCache = BorrowMap<IniKey, BorrowMap<IniKey, Option<PathBuf>>>;

static CHILD_DIR_CACHE: OnceLock<Mutex<PathLookupCache>> = OnceLock::new();
static FILE_PREFIX_CACHE: OnceLock<Mutex<PathLookupCache>> = OnceLock::new();
static NOTESKIN_DATA_CACHE: OnceLock<Mutex<HashMap<NoteskinDataCacheKey, Arc<NoteskinData>>>> =
    OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct NoteskinDataCacheKey {
    root: String,
    game: String,
    skin: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItgSkinCacheKey {
    num_cols: usize,
    num_players: usize,
    skin: String,
}

/// Process-wide lookup index for caller-owned noteskin runtimes.
///
/// The mutex makes lookup thread-safe, but entries are weak so the index never
/// extends a runtime beyond its screen/song owner. Player Options populates it
/// during entry; gameplay populates only its resolved skins before the song.
/// The touched-key count is bounded by installed skins times encountered play
/// styles and is cleared at the Player Options exit boundary or after source
/// changes. A miss loads synchronously and therefore must stay on transition
/// paths, never a live gameplay frame. There is no eviction or destruction
/// work here: the last owning `Arc` drops the runtime in that owner's context.
/// Existing loader warnings provide miss-failure instrumentation; a hit is one
/// bounded hash lookup plus `Weak::upgrade`.
pub struct ItgSkinRuntimeCache<T> {
    entries: Mutex<HashMap<ItgSkinCacheKey, Weak<T>>>,
}

#[derive(Debug, Clone)]
pub struct LoadedItgSkin<T> {
    pub skin: String,
    pub value: T,
    pub used_default_fallback: bool,
}

impl<T> Default for ItgSkinRuntimeCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<T> ItgSkinRuntimeCache<T> {
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    pub fn get_or_load<F>(&self, style: &Style, skin: &str, load: F) -> Result<Arc<T>, String>
    where
        F: FnOnce() -> Result<T, String>,
    {
        let key = itg_skin_cache_key(style, skin);
        if let Some(cached) = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .and_then(Weak::upgrade)
        {
            return Ok(cached);
        }

        let loaded = Arc::new(load()?);
        let mut guard = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = guard.get(&key).and_then(Weak::upgrade) {
            return Ok(cached);
        }
        guard.insert(key, Arc::downgrade(&loaded));
        Ok(loaded)
    }
}

pub fn clear_lookup_caches() {
    if let Some(cache) = CHILD_DIR_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    if let Some(cache) = FILE_PREFIX_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

pub fn clear_data_cache() {
    if let Some(cache) = NOTESKIN_DATA_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

#[inline(always)]
pub fn button_for_col(col: usize) -> &'static str {
    match col % 4 {
        0 => "Left",
        1 => "Down",
        2 => "Up",
        _ => "Right",
    }
}

pub fn down_col(num_cols: usize) -> usize {
    (0..num_cols)
        .find(|&col| button_for_col(col).eq_ignore_ascii_case("Down"))
        .unwrap_or(0)
}

pub fn normalized_game_name(game: &str) -> String {
    game.trim().to_ascii_lowercase()
}

pub fn normalized_skin_name(skin: &str) -> String {
    let skin = skin.trim();
    if skin.is_empty() {
        DEFAULT_SKIN_NAME.to_string()
    } else {
        skin.to_ascii_lowercase()
    }
}

pub fn skin_name_is_default(skin: &str) -> bool {
    normalized_skin_name(skin) == DEFAULT_SKIN_NAME
}

pub const fn default_skin_name() -> &'static str {
    DEFAULT_SKIN_NAME
}

pub const fn default_skin_candidates() -> &'static [&'static str] {
    DEFAULT_SKIN_CANDIDATES
}

#[inline(always)]
pub fn itg_skin_cache_key(style: &Style, skin: &str) -> ItgSkinCacheKey {
    ItgSkinCacheKey {
        num_cols: style.num_cols,
        num_players: style.num_players,
        skin: normalized_skin_name(skin),
    }
}

fn noteskin_data_cache_key(root: &Path, game: &str, skin: &str) -> NoteskinDataCacheKey {
    NoteskinDataCacheKey {
        root: root.to_string_lossy().to_ascii_lowercase(),
        game: normalized_game_name(game),
        skin: normalized_skin_name(skin),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IniKey(String);

impl IniKey {
    fn new(value: &str) -> Self {
        Self(value.to_ascii_lowercase())
    }
}

fn hash_ini_key(value: &str, state: &mut impl Hasher) {
    for byte in value.bytes() {
        state.write_u8(byte.to_ascii_lowercase());
    }
    state.write_u8(0xff);
}

impl Hash for IniKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_ini_key(&self.0, state);
    }
}

struct IniKeyRef<'a>(&'a str);

impl Hash for IniKeyRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_ini_key(self.0, state);
    }
}

impl Equivalent<IniKey> for IniKeyRef<'_> {
    fn equivalent(&self, key: &IniKey) -> bool {
        self.0.eq_ignore_ascii_case(&key.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IniData {
    sections: BorrowMap<IniKey, BorrowMap<IniKey, String>>,
}

impl IniData {
    pub fn parse_file(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| format!("failed to read ini '{}': {e}", path.display()))?;
        let mut out = Self::default();
        let mut section = IniKey::new("");

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
                section = IniKey::new(line[1..line.len() - 1].trim());
                out.sections.entry(section.clone()).or_default();
                continue;
            }
            let Some((key_raw, value_raw)) = line.split_once('=') else {
                continue;
            };
            let key = key_raw.trim();
            if key.is_empty() {
                continue;
            }
            let value = value_raw.trim().to_string();
            out.sections
                .entry(section.clone())
                .or_default()
                .insert(IniKey::new(key), value);
        }

        Ok(out)
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&IniKeyRef(section))
            .and_then(|values| values.get(&IniKeyRef(key)))
            .map(String::as_str)
    }

    pub fn merge_missing_from(&mut self, other: &Self) {
        for (section, values) in &other.sections {
            let dst = self.sections.entry(section.clone()).or_default();
            for (key, value) in values {
                dst.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoteskinData {
    pub name: String,
    pub metrics: IniData,
    pub search_dirs: Vec<PathBuf>,
}

impl NoteskinData {
    pub fn get_metric(&self, button: &str, value: &str) -> Option<&str> {
        self.metrics
            .get(button, value)
            .or_else(|| self.metrics.get("notedisplay", value))
    }

    pub fn resolve_path(&self, button: &str, element: &str) -> Option<PathBuf> {
        let mut path = self.resolve_path_once(button, element)?;

        for _ in 0..MAX_REDIR_DEPTH {
            if !is_redir(&path) {
                return Some(path);
            }

            let target = fs::read_to_string(&path).ok()?.trim().to_string();
            if target.is_empty() {
                warn!("noteskin redirect '{}' was empty", path.display());
                return None;
            }

            let Some(next) = self.resolve_file_from_search_dirs(&target) else {
                warn!(
                    "noteskin redirect '{}' -> '{}' did not resolve",
                    path.display(),
                    target
                );
                return None;
            };
            path = next;
        }

        warn!(
            "noteskin redirect depth exceeded while resolving '{} {}'",
            button, element
        );
        None
    }

    fn resolve_path_once(&self, button: &str, element: &str) -> Option<PathBuf> {
        let pref = if button.is_empty() {
            element.to_string()
        } else if element.is_empty() {
            button.to_string()
        } else {
            format!("{button} {element}")
        };

        if let Some(path) = self.resolve_file_from_search_dirs(&pref) {
            return Some(path);
        }

        if button.is_empty() {
            return None;
        }

        self.resolve_file_from_search_dirs(&format!("Fallback {element}"))
    }

    fn resolve_file_from_search_dirs(&self, prefix: &str) -> Option<PathBuf> {
        for dir in &self.search_dirs {
            if let Some(path) = find_file_with_prefix(dir, prefix) {
                return Some(path);
            }
        }
        None
    }
}

pub fn find_texture_with_prefix(data: &NoteskinData, prefix: &str) -> Option<PathBuf> {
    for dir in &data.search_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let matching = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .filter(|path| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|name| {
                        name.get(..prefix.len())
                            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
                            && name
                                .get(name.len().saturating_sub(4)..)
                                .is_some_and(|end| end.eq_ignore_ascii_case(".png"))
                    })
            })
            .min_by(|left, right| {
                let left = left
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                let right = right
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                left.bytes()
                    .map(|byte| byte.to_ascii_lowercase())
                    .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
            });
        if matching.is_some() {
            return matching;
        }
    }
    None
}

pub fn texture_key_for_path(
    asset_relative_path: Option<&Path>,
    path: &Path,
    path_is_file: bool,
) -> Option<String> {
    let key_path = if let Some(rel) = asset_relative_path {
        rel
    } else if path_is_file {
        path
    } else {
        return None;
    };
    let mut key = key_path.to_string_lossy().replace('\\', "/");
    if !path.is_absolute() {
        while key.starts_with('/') {
            key.remove(0);
        }
    }
    Some(key)
}

pub fn resolve_texture_expr(
    data: &NoteskinData,
    expr: &str,
    arg0_path: Option<&Path>,
) -> Option<PathBuf> {
    let value = expr.trim();
    if value == ITG_ARG0_TOKEN {
        return arg0_path.map(Path::to_path_buf);
    }
    if value.starts_with("NOTESKIN:GetPath(") {
        let args = itg_extract_quoted_strings(value);
        if args.len() >= 2 {
            return data
                .resolve_path(&args[0], &args[1])
                .or_else(|| data.resolve_path("", &args[1]));
        }
        if args.len() == 1 {
            return data
                .resolve_path(&args[0], "")
                .or_else(|| data.resolve_path("", &args[0]));
        }
    }
    let name = itg_parse_lua_quoted(value).unwrap_or_else(|| value.to_string());
    data.resolve_path(&name, "")
        .or_else(|| data.resolve_path("", &name))
        .or_else(|| {
            if value == "..." {
                arg0_path.map(Path::to_path_buf)
            } else {
                None
            }
        })
}

pub fn discover_skins(roots: &[PathBuf], game: &str) -> Vec<String> {
    let game = normalized_game_name(game);
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for root in roots {
        let game_dir = root.join(&game);
        let Ok(entries) = fs::read_dir(&game_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.is_empty() || name == "common" || name.starts_with('.') {
                continue;
            }
            let dir = entry.path();
            if noteskin_dir_has_itg_files(&dir) && seen.insert(name.clone()) {
                found.push(name);
            }
        }
    }
    order_discovered_skins(found)
}

fn noteskin_dir_has_itg_files(dir: &Path) -> bool {
    dir.join("NoteSkin.lua").is_file()
        || dir.join("metrics.ini").is_file()
        || fs::read_dir(dir)
            .ok()
            .and_then(|mut entries| entries.next())
            .is_some()
}

fn order_discovered_skins(mut found: Vec<String>) -> Vec<String> {
    found.sort();
    let mut ordered = Vec::with_capacity(found.len().max(2));
    for preferred in default_skin_candidates() {
        if let Some(pos) = found.iter().position(|skin| skin == preferred) {
            ordered.push(found.remove(pos));
        }
    }
    ordered.extend(found);
    if ordered.is_empty() {
        default_skin_candidates()
            .iter()
            .map(|skin| skin.to_string())
            .collect()
    } else {
        ordered
    }
}

pub fn load_itg_default_from_roots<T, F>(
    roots: &[PathBuf],
    game: &str,
    mut load: F,
) -> Result<LoadedItgSkin<T>, String>
where
    F: FnMut(&Path, &str, &str) -> Result<T, String>,
{
    for skin in default_skin_candidates() {
        for root in roots {
            if let Ok(value) = load(root, game, skin) {
                return Ok(LoadedItgSkin {
                    skin: (*skin).to_string(),
                    value,
                    used_default_fallback: *skin != default_skin_name(),
                });
            }
        }
    }
    Err(format!(
        "failed to load ITG default noteskin for game '{game}' from any root"
    ))
}

pub fn load_itg_skin_from_roots<T, F>(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
    mut load: F,
) -> Result<LoadedItgSkin<T>, String>
where
    F: FnMut(&Path, &str, &str) -> Result<T, String>,
{
    let requested = normalized_skin_name(skin);
    if skin_name_is_default(&requested) {
        return load_itg_default_from_roots(roots, game, load);
    }

    let mut last_err = format!("noteskin '{game}/{requested}' not found in any root");
    for root in roots {
        match load(root, game, &requested) {
            Ok(value) => {
                return Ok(LoadedItgSkin {
                    skin: requested,
                    value,
                    used_default_fallback: false,
                });
            }
            Err(error) => last_err = error,
        }
    }
    Err(last_err)
}

pub fn load_noteskin_data(root: &Path, game: &str, skin: &str) -> Result<NoteskinData, String> {
    let mut metrics = IniData::default();
    let mut search_dirs = Vec::new();

    let requested = normalized_skin_name(skin);
    let mut current = requested.clone();
    if current.is_empty() {
        return Err("noteskin name was empty".to_string());
    }

    let mut loaded_default = false;
    let mut loaded_common = false;
    let mut seen = HashSet::new();

    for _ in 0..MAX_FALLBACK_DEPTH {
        if !seen.insert(current.clone()) {
            return Err(format!(
                "circular noteskin fallback detected while loading '{skin}' (stuck on '{}')",
                current
            ));
        }

        let Some(dir) = resolve_skin_dir(root, game, &current) else {
            return Err(format!(
                "noteskin '{}' not found under '{}/{}' or '{}/common'",
                current,
                root.display(),
                game,
                root.display()
            ));
        };

        let ini = IniData::parse_file(&dir.join("metrics.ini"))?;
        metrics.merge_missing_from(&ini);
        search_dirs.push(dir);

        if current.eq_ignore_ascii_case("default") {
            loaded_default = true;
        }
        if current.eq_ignore_ascii_case("common") {
            loaded_common = true;
        }

        let next = match ini.get("global", "fallbacknoteskin") {
            Some(value) if !value.trim().is_empty() => Some(value.trim().to_ascii_lowercase()),
            _ if !loaded_default => Some("default".to_string()),
            _ if !loaded_common => Some("common".to_string()),
            _ => None,
        };

        let Some(next_skin) = next else {
            return Ok(NoteskinData {
                name: requested,
                metrics,
                search_dirs,
            });
        };
        if next_skin == current {
            return Ok(NoteskinData {
                name: requested,
                metrics,
                search_dirs,
            });
        }
        if seen.contains(&next_skin) {
            return Ok(NoteskinData {
                name: requested,
                metrics,
                search_dirs,
            });
        }
        current = next_skin;
    }

    Err(format!(
        "noteskin fallback depth exceeded while loading '{skin}'"
    ))
}

pub fn load_noteskin_data_cached(
    root: &Path,
    game: &str,
    skin: &str,
) -> Result<Arc<NoteskinData>, String> {
    let key = noteskin_data_cache_key(root, game, skin);
    let cache = NOTESKIN_DATA_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let loaded = Arc::new(load_noteskin_data(root, game, skin)?);
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = guard.entry(key).or_insert_with(|| loaded.clone());
    Ok(entry.clone())
}

pub fn load_noteskin_data_cached_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
) -> Option<Arc<NoteskinData>> {
    let skin = normalized_skin_name(skin);
    for root in roots {
        if let Ok(data) = load_noteskin_data_cached(root, game, &skin) {
            return Some(data);
        }
    }
    None
}

pub fn song_lua_noteskin_resolve_path_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
    button: &str,
    element: &str,
) -> Option<PathBuf> {
    load_noteskin_data_cached_from_roots(roots, game, skin)?.resolve_path(button, element)
}

pub fn song_lua_noteskin_metric_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
    element: &str,
    value: &str,
) -> Option<String> {
    load_noteskin_data_cached_from_roots(roots, game, skin)?
        .get_metric(element, value)
        .map(str::to_string)
}

pub fn song_lua_noteskin_metric_f_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
    element: &str,
    value: &str,
) -> Option<f32> {
    parse_script_number(
        song_lua_noteskin_metric_from_roots(roots, game, skin, element, value)?.as_str(),
    )
}

pub fn song_lua_noteskin_metric_b_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
    element: &str,
    value: &str,
) -> Option<bool> {
    Some(parse_script_bool(
        song_lua_noteskin_metric_from_roots(roots, game, skin, element, value)?.as_str(),
    ))
}

pub fn song_lua_noteskin_exists_from_roots(roots: &[PathBuf], game: &str, skin: &str) -> bool {
    load_noteskin_data_cached_from_roots(roots, game, skin).is_some()
}

pub fn song_lua_noteskin_names_from_roots(roots: &[PathBuf], game: &str) -> Vec<String> {
    discover_skins(roots, game)
}

pub fn note_display_metrics(metrics: &IniData) -> NoteDisplayMetrics {
    let mut out = NoteDisplayMetrics::default();
    let read_bool = |key: &str, default: bool| {
        metrics
            .get("NoteDisplay", key)
            .and_then(parse_ini_int)
            .map_or(default, |v| v != 0)
    };
    let read_float = |key: &str, default: f32| {
        metrics
            .get("NoteDisplay", key)
            .and_then(parse_ini_float)
            .unwrap_or(default)
    };
    let read_int = |key: &str, default: i32| {
        metrics
            .get("NoteDisplay", key)
            .and_then(parse_ini_int)
            .unwrap_or(default)
    };

    out.draw_hold_head_for_taps_on_same_row = read_bool(
        "DrawHoldHeadForTapsOnSameRow",
        out.draw_hold_head_for_taps_on_same_row,
    );
    out.draw_roll_head_for_taps_on_same_row = read_bool(
        "DrawRollHeadForTapsOnSameRow",
        out.draw_roll_head_for_taps_on_same_row,
    );
    out.tap_hold_roll_on_row_means_hold = read_bool(
        "TapHoldRollOnRowMeansHold",
        out.tap_hold_roll_on_row_means_hold,
    );
    out.hold_head_is_above_wavy_parts = read_bool(
        "HoldHeadIsAboveWavyParts",
        out.hold_head_is_above_wavy_parts,
    );
    out.hold_tail_is_above_wavy_parts = read_bool(
        "HoldTailIsAboveWavyParts",
        out.hold_tail_is_above_wavy_parts,
    );
    out.start_drawing_hold_body_offset_from_head = read_float(
        "StartDrawingHoldBodyOffsetFromHead",
        out.start_drawing_hold_body_offset_from_head,
    );
    out.stop_drawing_hold_body_offset_from_tail = read_float(
        "StopDrawingHoldBodyOffsetFromTail",
        out.stop_drawing_hold_body_offset_from_tail,
    );
    out.hold_let_go_gray_percent = read_float("HoldLetGoGrayPercent", out.hold_let_go_gray_percent);
    out.flip_head_and_tail_when_reverse = read_bool(
        "FlipHeadAndTailWhenReverse",
        out.flip_head_and_tail_when_reverse,
    );
    out.flip_hold_body_when_reverse =
        read_bool("FlipHoldBodyWhenReverse", out.flip_hold_body_when_reverse);
    out.top_hold_anchor_when_reverse =
        read_bool("TopHoldAnchorWhenReverse", out.top_hold_anchor_when_reverse);
    out.hold_active_is_add_layer = read_bool("HoldActiveIsAddLayer", out.hold_active_is_add_layer);
    for part in NoteAnimPart::ALL {
        let prefix = part.metric_prefix();
        let length_key = format!("{prefix}AnimationLength");
        let vivid_key = format!("{prefix}AnimationIsVivid");
        let add_x_key = format!("{prefix}AdditionTextureCoordOffsetX");
        let add_y_key = format!("{prefix}AdditionTextureCoordOffsetY");
        let spacing_x_key = format!("{prefix}NoteColorTextureCoordSpacingX");
        let spacing_y_key = format!("{prefix}NoteColorTextureCoordSpacingY");
        let count_key = format!("{prefix}NoteColorCount");
        let color_type_key = format!("{prefix}NoteColorType");
        let default_anim = out.part_animation[part as usize];
        let length = read_float(&length_key, default_anim.length).abs().max(1e-6);
        let vivid = read_bool(&vivid_key, default_anim.vivid);
        out.part_animation[part as usize] = NotePartAnimation { length, vivid };
        let default_translate = out.part_texture_translate[part as usize];
        let addition_offset = [
            read_float(&add_x_key, default_translate.addition_offset[0]),
            read_float(&add_y_key, default_translate.addition_offset[1]),
        ];
        let note_color_spacing = [
            read_float(&spacing_x_key, default_translate.note_color_spacing[0]),
            read_float(&spacing_y_key, default_translate.note_color_spacing[1]),
        ];
        let note_color_count = read_int(&count_key, default_translate.note_color_count);
        let note_color_type = metrics
            .get("NoteDisplay", &color_type_key)
            .and_then(NoteColorType::from_metric)
            .unwrap_or(default_translate.note_color_type);
        out.part_texture_translate[part as usize] = NotePartTextureTranslate {
            addition_offset,
            note_color_spacing,
            note_color_count,
            note_color_type,
        };
    }
    out
}

pub fn animation_is_beat_based(data: &NoteskinData) -> bool {
    data.metrics
        .get("NoteDisplay", "AnimationIsBeatBased")
        .or_else(|| data.metrics.get("Global", "AnimationIsBeatBased"))
        .and_then(parse_ini_float)
        .is_some_and(|v| v > 0.5)
}

fn parse_ini_value(raw: &str) -> Option<&str> {
    let trimmed = raw.split_once("//").map_or(raw, |(prefix, _)| prefix);
    let trimmed = trimmed
        .split_once(';')
        .map_or(trimmed, |(prefix, _)| prefix);
    let value = trimmed.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    Some(value)
}

fn parse_ini_int(raw: &str) -> Option<i32> {
    let value = parse_ini_value(raw)?;
    let bytes = value.as_bytes();
    let mut end = 0usize;
    if bytes.first().is_some_and(|b| *b == b'+' || *b == b'-') {
        end = 1;
    }
    let digit_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digit_start {
        return None;
    }
    let parsed = value[..end].parse::<i64>().ok()?;
    Some(parsed.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
}

pub fn parse_ini_float(raw: &str) -> Option<f32> {
    let value = parse_ini_value(raw)?;
    value.parse::<f32>().ok()
}

fn resolve_skin_dir(root: &Path, game: &str, skin: &str) -> Option<PathBuf> {
    find_child_dir_case_insensitive(&root.join(game), skin)
        .or_else(|| find_child_dir_case_insensitive(&root.join("common"), skin))
}

fn cached_path_lookup(
    cache: &Mutex<PathLookupCache>,
    parent: &Path,
    name: &str,
) -> Option<Option<PathBuf>> {
    let parent = parent.to_string_lossy();
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&IniKeyRef(parent.as_ref()))
        .and_then(|entries| entries.get(&IniKeyRef(name)))
        .cloned()
}

fn cache_path_lookup(
    cache: &Mutex<PathLookupCache>,
    parent: &Path,
    name: &str,
    value: Option<PathBuf>,
) {
    let parent = parent.to_string_lossy();
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry(IniKey::new(parent.as_ref()))
        .or_default()
        .insert(IniKey::new(name), value);
}

fn find_child_dir_case_insensitive(parent: &Path, name: &str) -> Option<PathBuf> {
    let cache = CHILD_DIR_CACHE.get_or_init(|| Mutex::new(BorrowMap::new()));
    if let Some(cached) = cached_path_lookup(cache, parent, name) {
        return cached;
    }
    let entries = fs::read_dir(parent).ok()?;
    let mut found = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let matches = entry
            .file_name()
            .to_str()
            .is_some_and(|entry_name| entry_name.eq_ignore_ascii_case(name));
        if matches {
            found = Some(path);
            break;
        }
    }
    cache_path_lookup(cache, parent, name, found.clone());
    found
}

fn find_file_with_prefix(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let cache = FILE_PREFIX_CACHE.get_or_init(|| Mutex::new(BorrowMap::new()));
    if let Some(cached) = cached_path_lookup(cache, dir, prefix) {
        return cached;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut match_count = 0usize;
    let mut chosen: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name
            .get(..prefix.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        {
            continue;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        match_count += 1;
        let comes_first = chosen.as_ref().is_none_or(|current| {
            let current = current
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name.bytes()
                .map(|byte| byte.to_ascii_lowercase())
                .lt(current.bytes().map(|byte| byte.to_ascii_lowercase()))
        });
        if comes_first {
            chosen = Some(path);
        }
    }

    if let Some(chosen) = chosen.as_ref().filter(|_| match_count > 1) {
        warn!(
            "multiple noteskin files matched prefix '{}' in '{}'; using '{}', ignoring {} others",
            prefix,
            dir.display(),
            chosen.display(),
            match_count - 1
        );
    }
    cache_path_lookup(cache, dir, prefix, chosen.clone());
    chosen
}

fn is_redir(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("redir"))
}

#[cfg(feature = "bench-support")]
pub struct ItgLookupBench {
    ini: IniData,
    legacy_ini: HashMap<String, HashMap<String, String>>,
    paths: PathLookupCache,
    legacy_paths: HashMap<(String, String), Option<PathBuf>>,
    path_queries: Vec<(PathBuf, String)>,
}

#[cfg(feature = "bench-support")]
impl ItgLookupBench {
    pub fn new() -> Self {
        let mut ini = IniData::default();
        let mut legacy_ini = HashMap::new();
        for section_index in 0..16 {
            let section = format!("Section-{section_index:02}");
            let mut current_values = BorrowMap::new();
            let mut legacy_values = HashMap::new();
            for key_index in 0..32 {
                let key = format!("Metric-{key_index:02}");
                let value = format!("{section_index}:{key_index}");
                current_values.insert(IniKey::new(&key), value.clone());
                legacy_values.insert(key.to_ascii_lowercase(), value);
            }
            ini.sections.insert(IniKey::new(&section), current_values);
            legacy_ini.insert(section.to_ascii_lowercase(), legacy_values);
        }

        let mut paths = PathLookupCache::new();
        let mut legacy_paths = HashMap::new();
        let mut path_queries = Vec::new();
        for index in 0..8 {
            let parent = PathBuf::from(format!("Assets/NoteSkins/dance/skin-{index:02}"));
            let name = format!("Tap Note {index:02}");
            let value = (index % 3 != 0).then(|| parent.join(format!("{name}.png")));
            paths
                .entry(IniKey::new(parent.to_string_lossy().as_ref()))
                .or_default()
                .insert(IniKey::new(&name), value.clone());
            legacy_paths.insert(
                (
                    parent.to_string_lossy().to_ascii_lowercase(),
                    name.to_ascii_lowercase(),
                ),
                value,
            );
            path_queries.push((parent, name));
        }

        Self {
            ini,
            legacy_ini,
            paths,
            legacy_paths,
            path_queries,
        }
    }

    #[inline(never)]
    pub fn current_ini_checksum(&self) -> u64 {
        ini_lookup_queries()
            .iter()
            .fold(0_u64, |checksum, &(section, key)| {
                checksum.rotate_left(5)
                    ^ self.ini.get(section, key).map_or(u64::MAX, string_checksum)
            })
    }

    #[inline(never)]
    pub fn legacy_ini_checksum(&self) -> u64 {
        ini_lookup_queries()
            .iter()
            .fold(0_u64, |checksum, &(section, key)| {
                let section = section.to_ascii_lowercase();
                let key = key.to_ascii_lowercase();
                checksum.rotate_left(5)
                    ^ self
                        .legacy_ini
                        .get(&section)
                        .and_then(|values| values.get(&key))
                        .map_or(u64::MAX, |value| string_checksum(value))
            })
    }

    #[inline(never)]
    pub fn current_path_checksum(&self) -> u64 {
        self.path_queries
            .iter()
            .fold(0_u64, |checksum, (parent, name)| {
                let value = self
                    .paths
                    .get(&IniKeyRef(parent.to_string_lossy().as_ref()))
                    .and_then(|entries| entries.get(&IniKeyRef(name)))
                    .cloned();
                checksum.rotate_left(5) ^ path_lookup_checksum(value)
            })
    }

    #[inline(never)]
    pub fn legacy_path_checksum(&self) -> u64 {
        self.path_queries
            .iter()
            .fold(0_u64, |checksum, (parent, name)| {
                let key = (
                    parent.to_string_lossy().to_ascii_lowercase(),
                    name.to_ascii_lowercase(),
                );
                let value = self.legacy_paths.get(&key).cloned();
                checksum.rotate_left(5) ^ path_lookup_checksum(value)
            })
    }
}

#[cfg(feature = "bench-support")]
impl Default for ItgLookupBench {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "bench-support")]
fn ini_lookup_queries() -> &'static [(&'static str, &'static str)] {
    &[
        ("SECTION-00", "METRIC-00"),
        ("section-03", "metric-17"),
        ("Section-07", "Metric-31"),
        ("SECTION-12", "metric-08"),
        ("section-15", "METRIC-31"),
        ("missing", "Metric-00"),
        ("Section-09", "missing"),
        ("Section-04", "Metric-20"),
    ]
}

#[cfg(feature = "bench-support")]
fn string_checksum(value: &str) -> u64 {
    value.bytes().fold(0_u64, |checksum, byte| {
        checksum.rotate_left(3) ^ u64::from(byte)
    })
}

#[cfg(feature = "bench-support")]
fn path_lookup_checksum(value: Option<Option<PathBuf>>) -> u64 {
    match value {
        Some(Some(path)) => string_checksum(path.to_string_lossy().as_ref()),
        Some(None) => 1,
        None => u64::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BorrowMap, IniData, IniKey, ItgSkinRuntimeCache, NoteskinData, animation_is_beat_based,
        button_for_col, clear_data_cache, clear_lookup_caches, default_skin_candidates,
        default_skin_name, down_col, find_file_with_prefix, find_texture_with_prefix,
        load_itg_skin_from_roots, load_noteskin_data_cached, load_noteskin_data_cached_from_roots,
        normalized_game_name, normalized_skin_name, note_display_metrics, parse_ini_float,
        resolve_skin_dir, resolve_texture_expr, skin_name_is_default,
        song_lua_noteskin_exists_from_roots, song_lua_noteskin_metric_b_from_roots,
        song_lua_noteskin_metric_f_from_roots, song_lua_noteskin_metric_from_roots,
        song_lua_noteskin_names_from_roots, song_lua_noteskin_resolve_path_from_roots,
        texture_key_for_path,
    };
    use crate::{NoteAnimPart, NoteColorType, Style};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    static LOOKUP_CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn temp_root(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-noteskin-itg-{name}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn button_for_col_cycles_dance_panels() {
        assert_eq!(button_for_col(0), "Left");
        assert_eq!(button_for_col(1), "Down");
        assert_eq!(button_for_col(2), "Up");
        assert_eq!(button_for_col(3), "Right");
        assert_eq!(button_for_col(4), "Left");
        assert_eq!(down_col(0), 0);
        assert_eq!(down_col(1), 0);
        assert_eq!(down_col(4), 1);
        assert_eq!(down_col(8), 1);
    }

    #[test]
    fn normalized_names_trim_case_and_default_empty_skins() {
        assert_eq!(normalized_game_name(" Dance "), "dance");
        assert_eq!(normalized_skin_name(" Cel "), "cel");
        assert_eq!(normalized_skin_name(" \t "), "default");
        assert_eq!(default_skin_name(), "default");
        assert_eq!(default_skin_candidates(), ["default", "cel"]);
        assert!(skin_name_is_default(""));
        assert!(skin_name_is_default(" DEFAULT "));
        assert!(!skin_name_is_default("cel"));
    }

    #[test]
    fn load_noteskin_data_uses_normalized_requested_name() {
        let _guard = LOOKUP_CACHE_TEST_LOCK.lock().unwrap();
        clear_lookup_caches();
        clear_data_cache();
        let root = temp_root("normalized-name");
        let skin_dir = root.join("dance/cel");
        let default_dir = root.join("dance/default");
        let common_dir = root.join("common/common");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::create_dir_all(&default_dir).unwrap();
        fs::create_dir_all(&common_dir).unwrap();

        let data = super::load_noteskin_data(&root, "dance", " CeL ").expect("noteskin data");

        assert_eq!(data.name, "cel");
        clear_data_cache();
    }

    #[test]
    fn clear_data_cache_reloads_noteskin_data() {
        let _guard = LOOKUP_CACHE_TEST_LOCK.lock().unwrap();
        clear_lookup_caches();
        clear_data_cache();
        let root = temp_root("data-cache");
        let skin_dir = root.join("dance/hot");
        fs::create_dir_all(&skin_dir).unwrap();
        let metrics = skin_dir.join("metrics.ini");
        fs::write(
            &metrics,
            "[Global]\nFallbackNoteSkin=hot\n[Down]\nFoo=old\n",
        )
        .unwrap();

        let loaded = load_noteskin_data_cached(&root, "dance", "hot").unwrap();
        assert_eq!(loaded.get_metric("Down", "Foo"), Some("old"));

        fs::write(
            &metrics,
            "[Global]\nFallbackNoteSkin=hot\n[Down]\nFoo=new\n",
        )
        .unwrap();
        let stale = load_noteskin_data_cached(&root, "dance", "hot").unwrap();
        assert_eq!(stale.get_metric("Down", "Foo"), Some("old"));

        clear_data_cache();
        let refreshed = load_noteskin_data_cached(&root, "dance", "hot").unwrap();
        assert_eq!(refreshed.get_metric("Down", "Foo"), Some("new"));

        let _ = fs::remove_dir_all(root);
        clear_lookup_caches();
        clear_data_cache();
    }

    #[test]
    fn discover_skins_orders_preferred_and_filters_internal_dirs() {
        let root = temp_root("discover");
        fs::create_dir_all(root.join("dance/default")).unwrap();
        fs::create_dir_all(root.join("dance/cel")).unwrap();
        fs::create_dir_all(root.join("dance/Zeta")).unwrap();
        fs::create_dir_all(root.join("dance/alpha")).unwrap();
        fs::create_dir_all(root.join("dance/common")).unwrap();
        fs::create_dir_all(root.join("dance/.hidden")).unwrap();
        fs::create_dir_all(root.join("dance/empty")).unwrap();
        fs::write(root.join("dance/default/metrics.ini"), b"").unwrap();
        fs::write(root.join("dance/cel/NoteSkin.lua"), b"").unwrap();
        fs::write(root.join("dance/Zeta/NoteSkin.lua"), b"").unwrap();
        fs::write(root.join("dance/alpha/sprite.png"), b"").unwrap();
        fs::write(root.join("dance/common/metrics.ini"), b"").unwrap();
        fs::write(root.join("dance/.hidden/metrics.ini"), b"").unwrap();

        assert_eq!(
            super::discover_skins(&[root], " Dance "),
            ["default", "cel", "alpha", "zeta"]
        );
    }

    #[test]
    fn discover_skins_returns_default_list_when_none_found() {
        let root = temp_root("discover-empty");

        assert_eq!(super::discover_skins(&[root], "dance"), ["default", "cel"]);
    }

    #[test]
    fn load_noteskin_data_cached_from_roots_uses_first_loadable_root() {
        let _guard = LOOKUP_CACHE_TEST_LOCK.lock().unwrap();
        clear_lookup_caches();
        clear_data_cache();
        let missing_root = temp_root("data-roots-missing");
        let root = temp_root("data-roots");
        let skin_dir = root.join("dance/cel");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            b"[Global]\nFallbackNoteSkin=cel\n[Down]\nFoo=bar\n",
        )
        .unwrap();

        let loaded = load_noteskin_data_cached_from_roots(
            &[missing_root.clone(), root.clone()],
            "dance",
            "cel",
        )
        .expect("load from second root");

        assert_eq!(loaded.name, "cel");
        assert_eq!(loaded.get_metric("Down", "Foo"), Some("bar"));

        let _ = fs::remove_dir_all(missing_root);
        let _ = fs::remove_dir_all(root);
        clear_lookup_caches();
        clear_data_cache();
    }

    #[test]
    fn song_lua_noteskin_helpers_use_cached_root_data() {
        let _guard = LOOKUP_CACHE_TEST_LOCK.lock().unwrap();
        clear_lookup_caches();
        clear_data_cache();
        let root = temp_root("song-lua");
        let skin_dir = root.join("dance/lambda");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(skin_dir.join("Down Tap Note.png"), b"fake").unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            b"[Global]\nFallbackNoteSkin=lambda\n[Down]\nFoo=bar\nZoom=1.25\nEnabled=1\nTap Note=Tap Note.png\n",
        )
        .unwrap();
        let roots = vec![root.clone()];

        assert!(song_lua_noteskin_exists_from_roots(
            &roots, "dance", "lambda"
        ));
        assert_eq!(
            song_lua_noteskin_names_from_roots(&roots, "dance"),
            ["lambda"]
        );
        assert_eq!(
            song_lua_noteskin_metric_from_roots(&roots, "dance", "lambda", "Down", "Foo"),
            Some("bar".to_string())
        );
        assert_eq!(
            song_lua_noteskin_metric_f_from_roots(&roots, "dance", "lambda", "Down", "Zoom"),
            Some(1.25)
        );
        assert_eq!(
            song_lua_noteskin_metric_b_from_roots(&roots, "dance", "lambda", "Down", "Enabled"),
            Some(true)
        );
        assert_eq!(
            song_lua_noteskin_resolve_path_from_roots(
                &roots, "dance", "lambda", "Down", "Tap Note"
            ),
            Some(skin_dir.join("Down Tap Note.png"))
        );

        let _ = fs::remove_dir_all(root);
        clear_lookup_caches();
        clear_data_cache();
    }

    #[test]
    fn load_itg_skin_from_roots_uses_default_fallback_candidates() {
        let missing_root = PathBuf::from("missing-root");
        let root = PathBuf::from("root");
        let mut calls = Vec::new();

        let loaded = load_itg_skin_from_roots(
            &[missing_root.clone(), root.clone()],
            "dance",
            "default",
            |root, game, skin| {
                calls.push((root.to_path_buf(), game.to_string(), skin.to_string()));
                if root == Path::new("root") && skin == "cel" {
                    Ok("loaded")
                } else {
                    Err("missing".to_string())
                }
            },
        )
        .expect("fallback skin should load");

        assert_eq!(loaded.value, "loaded");
        assert_eq!(loaded.skin, "cel");
        assert!(loaded.used_default_fallback);
        assert_eq!(
            calls,
            vec![
                (
                    missing_root.clone(),
                    "dance".to_string(),
                    "default".to_string()
                ),
                (root.clone(), "dance".to_string(), "default".to_string()),
                (missing_root, "dance".to_string(), "cel".to_string()),
                (root, "dance".to_string(), "cel".to_string()),
            ]
        );
    }

    #[test]
    fn itg_skin_runtime_cache_reuses_loaded_skin_by_style_and_name() {
        let cache = ItgSkinRuntimeCache::<String>::default();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let mut loads = 0usize;

        let first = cache
            .get_or_load(&style, " CeL ", || {
                loads += 1;
                Ok("loaded".to_string())
            })
            .unwrap();
        let second = cache
            .get_or_load(&style, "cel", || {
                loads += 1;
                Ok("other".to_string())
            })
            .unwrap();

        assert_eq!(loads, 1);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.as_str(), "loaded");

        drop(first);
        drop(second);
        let released = cache
            .get_or_load(&style, "cel", || {
                loads += 1;
                Ok("released".to_string())
            })
            .unwrap();
        assert_eq!(loads, 2);
        assert_eq!(released.as_str(), "released");

        cache.clear();
        let reloaded = cache
            .get_or_load(&style, "cel", || {
                loads += 1;
                Ok("reloaded".to_string())
            })
            .unwrap();
        assert_eq!(loads, 3);
        assert_eq!(reloaded.as_str(), "reloaded");
    }

    #[test]
    fn resolve_texture_expr_handles_getpath_quotes_and_arg0() {
        let _guard = LOOKUP_CACHE_TEST_LOCK.lock().unwrap();
        clear_lookup_caches();
        let root = temp_root("texture-expr");
        let skin_dir = root.join("dance/default");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(skin_dir.join("Down Tap Note.png"), b"").unwrap();
        fs::write(skin_dir.join("Fallback Explosion.png"), b"").unwrap();
        let data = NoteskinData {
            name: "default".to_string(),
            metrics: IniData::default(),
            search_dirs: vec![skin_dir.clone()],
        };
        let arg0 = skin_dir.join("Arg0.png");

        assert_eq!(
            resolve_texture_expr(&data, "NOTESKIN:GetPath('Down', 'Tap Note')", Some(&arg0),)
                .and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some("Down Tap Note.png".into())
        );
        assert_eq!(
            resolve_texture_expr(&data, "'Fallback Explosion'", Some(&arg0))
                .and_then(|path| path.file_name().map(|name| name.to_owned())),
            Some("Fallback Explosion.png".into())
        );
        assert_eq!(
            resolve_texture_expr(&data, crate::actor::ITG_ARG0_TOKEN, Some(&arg0)),
            Some(arg0.clone())
        );
        clear_lookup_caches();
    }

    fn ini_section(section: &str, values: &[(&str, &str)]) -> IniData {
        let section_values = values
            .iter()
            .map(|(key, value)| (IniKey::new(key), value.to_string()))
            .collect::<BorrowMap<_, _>>();
        IniData {
            sections: BorrowMap::from([(IniKey::new(section), section_values)]),
        }
    }

    #[test]
    fn note_display_metrics_parse_global_flags_and_offsets() {
        let metrics = ini_section(
            "NoteDisplay",
            &[
                ("DrawHoldHeadForTapsOnSameRow", "0"),
                ("FlipHoldBodyWhenReverse", "1"),
                ("HoldLetGoGrayPercent", "0.4"),
                ("TapNoteAnimationLength", "-2.5"),
                ("TapNoteAnimationIsVivid", "1"),
                ("TapNoteAdditionTextureCoordOffsetX", "0.125"),
                ("TapNoteAdditionTextureCoordOffsetY", "-0.25"),
                ("TapNoteNoteColorCount", "12"),
                ("TapNoteNoteColorType", "ProgressAlternate"),
            ],
        );

        let parsed = note_display_metrics(&metrics);
        let tap_anim = parsed.part_animation[NoteAnimPart::Tap as usize];
        let tap_translate = parsed.part_texture_translate[NoteAnimPart::Tap as usize];

        assert!(!parsed.draw_hold_head_for_taps_on_same_row);
        assert!(parsed.flip_hold_body_when_reverse);
        assert!((parsed.hold_let_go_gray_percent - 0.4).abs() <= f32::EPSILON);
        assert!((tap_anim.length - 2.5).abs() <= f32::EPSILON);
        assert!(tap_anim.vivid);
        assert_eq!(tap_translate.addition_offset, [0.125, -0.25]);
        assert_eq!(tap_translate.note_color_count, 12);
        assert_eq!(
            tap_translate.note_color_type,
            NoteColorType::ProgressAlternate
        );
    }

    #[test]
    fn note_display_metrics_keep_defaults_for_invalid_values() {
        let metrics = ini_section(
            "NoteDisplay",
            &[
                ("RollHeadAnimationLength", "nope"),
                ("RollHeadNoteColorCount", "nope"),
                ("RollHeadNoteColorType", "unknown"),
            ],
        );

        let parsed = note_display_metrics(&metrics);
        let roll_anim = parsed.part_animation[NoteAnimPart::RollHead as usize];
        let roll_translate = parsed.part_texture_translate[NoteAnimPart::RollHead as usize];

        assert_eq!(roll_anim.length, 1.0);
        assert_eq!(roll_translate.note_color_count, 8);
        assert_eq!(roll_translate.note_color_type, NoteColorType::Denominator);
    }

    #[test]
    fn parse_ini_float_trims_quotes_and_comments() {
        assert_eq!(parse_ini_float(" \"1.25\" ; comment"), Some(1.25));
        assert_eq!(parse_ini_float(" -0.5 // comment"), Some(-0.5));
        assert_eq!(parse_ini_float(" nope "), None);
    }

    #[test]
    fn ini_metric_lookup_remains_ascii_case_insensitive() {
        let metrics = ini_section("NoteDisplay", &[("TapNoteAnimationLength", "2")]);
        assert_eq!(
            metrics.get("notedisplay", "tapnoteanimationlength"),
            Some("2")
        );
        assert_eq!(
            metrics.get("NOTEDISPLAY", "TAPNOTEANIMATIONLENGTH"),
            Some("2")
        );
    }

    #[test]
    fn animation_is_beat_based_reads_notedisplay_then_global() {
        let global = NoteskinData {
            name: "global".to_string(),
            metrics: ini_section("Global", &[("AnimationIsBeatBased", "1")]),
            search_dirs: Vec::new(),
        };
        let override_off = NoteskinData {
            name: "override".to_string(),
            metrics: {
                let mut metrics = ini_section("Global", &[("AnimationIsBeatBased", "1")]);
                metrics.merge_missing_from(&ini_section(
                    "NoteDisplay",
                    &[("AnimationIsBeatBased", "0")],
                ));
                metrics
            },
            search_dirs: Vec::new(),
        };

        assert!(animation_is_beat_based(&global));
        assert!(!animation_is_beat_based(&override_off));
    }

    #[test]
    fn clear_lookup_caches_rechecks_missing_skin_dirs() {
        let _guard = LOOKUP_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_lookup_caches();
        let root = temp_root("skin-dir");
        fs::create_dir_all(root.join("dance")).unwrap();

        assert!(resolve_skin_dir(&root, "dance", "fresh").is_none());
        fs::create_dir_all(root.join("dance/Fresh")).unwrap();
        assert!(
            resolve_skin_dir(&root, "dance", "FRESH").is_none(),
            "missing directory result should remain cached until refresh"
        );

        clear_lookup_caches();
        assert_eq!(
            resolve_skin_dir(&root, "dance", "fresh"),
            Some(root.join("dance/Fresh"))
        );
        let _ = fs::remove_dir_all(&root);
        clear_lookup_caches();
    }

    #[test]
    fn clear_lookup_caches_rechecks_missing_file_prefixes() {
        let _guard = LOOKUP_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_lookup_caches();
        let root = temp_root("file-prefix");
        let dir = root.join("dance/default");
        fs::create_dir_all(&dir).unwrap();

        assert!(find_file_with_prefix(&dir, "Tap Note").is_none());
        let path = dir.join("tap note alpha.PNG");
        fs::write(&path, []).unwrap();
        fs::write(dir.join("Tap Note Zulu.png"), []).unwrap();
        assert!(
            find_file_with_prefix(&dir, "TAP NOTE").is_none(),
            "missing file prefix result should remain cached until refresh"
        );

        clear_lookup_caches();
        assert_eq!(find_file_with_prefix(&dir, "TAP NOTE"), Some(path));
        let _ = fs::remove_dir_all(&root);
        clear_lookup_caches();
    }

    #[test]
    fn lookup_cache_keeps_case_insensitive_filesystem_behavior() {
        let _guard = LOOKUP_CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_lookup_caches();
        let root = temp_root("lookup-case");
        let dir = root.join("dance");
        let skin = dir.join("MiXeD");
        fs::create_dir_all(&skin).unwrap();
        let path = skin.join("TaP NoTe.png");
        fs::write(&path, []).unwrap();

        assert_eq!(
            super::find_child_dir_case_insensitive(&dir, "mixed"),
            Some(skin.clone())
        );
        assert_eq!(
            super::find_child_dir_case_insensitive(&dir, "MIXED"),
            Some(skin.clone())
        );
        assert_eq!(find_file_with_prefix(&skin, "tap note"), Some(path.clone()));
        assert_eq!(find_file_with_prefix(&skin, "TAP NOTE"), Some(path));

        let _ = fs::remove_dir_all(&root);
        clear_lookup_caches();
    }

    #[test]
    fn find_texture_with_prefix_uses_png_matches_in_search_order() {
        let root = temp_root("texture-prefix");
        let first = root.join("dance/default");
        let second = root.join("common/default");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        fs::write(first.join("_arrow.ini"), []).unwrap();
        fs::write(first.join("_arrow z.png"), []).unwrap();
        fs::write(first.join("_arrow a.PNG"), []).unwrap();
        fs::write(second.join("_arrow first.png"), []).unwrap();
        let data = NoteskinData {
            name: "test".to_string(),
            metrics: IniData::default(),
            search_dirs: vec![first.clone(), second],
        };

        assert_eq!(
            find_texture_with_prefix(&data, "_ARROW"),
            Some(first.join("_arrow a.PNG"))
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn texture_key_for_path_prefers_asset_relative_path() {
        let path = Path::new("assets/noteskins/dance/default/Down Tap Note.png");
        let rel = Path::new("noteskins/dance/default/Down Tap Note.png");

        assert_eq!(
            texture_key_for_path(Some(rel), path, true),
            Some("noteskins/dance/default/Down Tap Note.png".to_string())
        );
    }

    #[test]
    fn texture_key_for_path_preserves_external_file_paths() {
        let path = std::env::current_dir()
            .unwrap()
            .join("external pack")
            .join("Tap Note.png");

        assert_eq!(
            texture_key_for_path(None, &path, true),
            Some(path.to_string_lossy().replace('\\', "/"))
        );
    }

    #[test]
    fn texture_key_for_path_rejects_missing_external_paths() {
        assert_eq!(
            texture_key_for_path(None, Path::new("missing.png"), false),
            None
        );
    }
}
