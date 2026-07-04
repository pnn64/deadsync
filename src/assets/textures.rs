use crate::assets::AssetManager;
pub use deadlib_assets::{
    TextureHints, apply_texture_hints, ascii_ci_hash, direct_texture_key_path, fix_hidden_alpha,
    open_image_fallback, parse_sprite_sheet_dims, parse_texture_hints, strip_sprite_hints,
    texture_filename_has_multiframe_hint, texture_source_dims_from_real,
    texture_source_frame_dims_from_real,
};
use deadlib_platform::dirs;
use deadlib_present::actors::TextureKeyHandle;
use deadlib_present::texture as present_texture;
use deadlib_render::{FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, SamplerWrap, TextureHandle};
use deadlib_renderer::Backend;
use image::RgbaImage;
use log::{debug, warn};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use super::{AssetError, PRESENT_TEXTURE_CONTEXT, visual_styles};

#[derive(Clone, Copy, Debug)]
pub struct TexMeta {
    pub w: u32,
    pub h: u32,
}

pub struct TextureChoice {
    pub key: Arc<str>,
    pub label: String,
    cached_handle: AtomicU64,
    cached_generation: AtomicU64,
}

impl TextureChoice {
    fn new(key: String, label: String) -> Self {
        Self {
            key: Arc::from(key),
            label,
            cached_handle: AtomicU64::new(INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        }
    }

    #[inline(always)]
    pub fn texture_key_handle(&self) -> TextureKeyHandle {
        present_texture::cached_texture_key_handle(
            &self.key,
            &self.cached_handle,
            &self.cached_generation,
            &PRESENT_TEXTURE_CONTEXT,
        )
    }
}

impl Clone for TextureChoice {
    fn clone(&self) -> Self {
        Self {
            key: Arc::clone(&self.key),
            label: self.label.clone(),
            cached_handle: AtomicU64::new(self.cached_handle.load(Ordering::Relaxed)),
            cached_generation: AtomicU64::new(self.cached_generation.load(Ordering::Relaxed)),
        }
    }
}

impl core::fmt::Debug for TextureChoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextureChoice")
            .field("key", &self.key)
            .field("label", &self.label)
            .finish()
    }
}

impl PartialEq for TextureChoice {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.label == other.label
    }
}

impl Eq for TextureChoice {}

#[derive(Clone, Debug)]
struct DiscoveredTexture {
    key: String,
    label: String,
    source_path: String,
}

static JUDGMENT_TEXTURE_CHOICES: OnceLock<Vec<TextureChoice>> = OnceLock::new();
static HOLD_JUDGMENT_TEXTURE_CHOICES: OnceLock<Vec<TextureChoice>> = OnceLock::new();
static HELD_MISS_TEXTURE_CHOICES: OnceLock<Vec<TextureChoice>> = OnceLock::new();
const NONE_TEXTURE_CHOICE_KEY: &str = "None";

#[inline(always)]
fn needs_repeat_sampler(key: &str) -> bool {
    matches!(
        key,
        "swoosh.png" | "graphics/menu_bg_technique/square.png" | "grades/goldstar (stretch).png"
    ) || visual_styles::is_shared_background_texture(key)
}

fn absolute_or_self(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn graphics_roots(folder: &str) -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    let mut roots = Vec::with_capacity(3);
    if !dirs.portable {
        let data_root = dirs.data_dir.join("assets").join("graphics").join(folder);
        if data_root.is_dir() {
            roots.push(data_root);
        }
    }

    let cwd_root = Path::new("assets").join("graphics").join(folder);
    if cwd_root.is_dir() {
        let cwd_root = absolute_or_self(&cwd_root);
        if !roots.iter().any(|root| root == &cwd_root) {
            roots.push(cwd_root);
        }
    }

    let exe_root = dirs.exe_dir.join("assets").join("graphics").join(folder);
    if exe_root.is_dir() && !roots.iter().any(|root| root == &exe_root) {
        roots.push(exe_root);
    }
    roots
}

fn is_png_file(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

fn discover_graphic_textures(
    folder: &str,
    love_first: bool,
    require_multiframe_hint: bool,
) -> Vec<DiscoveredTexture> {
    let mut discovered = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in graphics_roots(folder) {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if require_multiframe_hint && !texture_filename_has_multiframe_hint(file_name) {
                continue;
            }
            if !require_multiframe_hint && !is_png_file(file_name) {
                continue;
            }
            let key = format!("{folder}/{file_name}");
            if !seen_keys.insert(key.to_ascii_lowercase()) {
                continue;
            }
            let label = strip_sprite_hints(file_name);
            if label.eq_ignore_ascii_case(NONE_TEXTURE_CHOICE_KEY) {
                continue;
            }
            discovered.push(DiscoveredTexture {
                key,
                label,
                source_path: absolute_or_self(&path).to_string_lossy().replace('\\', "/"),
            });
        }
    }
    discovered.sort_by(|a, b| {
        let a_love = love_first && a.label.eq_ignore_ascii_case("Love");
        let b_love = love_first && b.label.eq_ignore_ascii_case("Love");
        match (a_love, b_love) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .label
                .to_ascii_lowercase()
                .cmp(&b.label.to_ascii_lowercase()),
        }
    });
    discovered
}

fn texture_choices_from_discovered(
    folder: &str,
    love_first: bool,
    include_none: bool,
    require_multiframe_hint: bool,
) -> Vec<TextureChoice> {
    let mut choices: Vec<TextureChoice> =
        discover_graphic_textures(folder, love_first, require_multiframe_hint)
            .into_iter()
            .map(|texture| TextureChoice::new(texture.key, texture.label))
            .collect();
    if include_none {
        choices.push(TextureChoice::new(
            NONE_TEXTURE_CHOICE_KEY.to_string(),
            NONE_TEXTURE_CHOICE_KEY.to_string(),
        ));
    }
    choices
}

pub fn judgment_texture_choices() -> &'static [TextureChoice] {
    JUDGMENT_TEXTURE_CHOICES
        .get_or_init(|| texture_choices_from_discovered("judgements", true, true, true))
        .as_slice()
}

pub fn hold_judgment_texture_choices() -> &'static [TextureChoice] {
    HOLD_JUDGMENT_TEXTURE_CHOICES
        .get_or_init(|| texture_choices_from_discovered("hold_judgements", false, true, true))
        .as_slice()
}

pub fn held_miss_texture_choices() -> &'static [TextureChoice] {
    HELD_MISS_TEXTURE_CHOICES
        .get_or_init(|| texture_choices_from_discovered("held_miss", false, true, false))
        .as_slice()
}

pub fn resolve_texture_choice<'a>(
    requested: Option<&str>,
    choices: &'a [TextureChoice],
) -> Option<&'a str> {
    resolve_texture_choice_entry(requested, choices).map(|choice| choice.key.as_ref())
}

pub fn resolve_texture_choice_entry<'a>(
    requested: Option<&str>,
    choices: &'a [TextureChoice],
) -> Option<&'a TextureChoice> {
    // When the caller explicitly opts out of a texture (e.g. user selected "None"),
    // honor that and render nothing. Only fall back to the first available choice
    // when a texture was requested but could not be located in the discovered set
    // (e.g. the user-customized file was removed).
    let key = requested?;
    choices
        .iter()
        .find(|choice| choice.key.as_ref().eq_ignore_ascii_case(key))
        .or_else(|| {
            choices.iter().find(|choice| {
                !choice
                    .key
                    .as_ref()
                    .eq_ignore_ascii_case(NONE_TEXTURE_CHOICE_KEY)
            })
        })
}

static TEX_META: std::sync::LazyLock<RwLock<HashMap<String, TexMeta>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static SHEET_DIMS: std::sync::LazyLock<RwLock<HashMap<String, (u32, u32)>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static TEXTURE_HANDLES: std::sync::LazyLock<RwLock<HashMap<String, TextureHandle>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static TEXTURE_HANDLE_ALIASES: std::sync::LazyLock<RwLock<FastU64Map<TextureHandle>>> =
    std::sync::LazyLock::new(|| RwLock::new(FastU64Map::default()));

#[derive(Clone)]
pub(crate) struct GeneratedTexture {
    pub image: Arc<RgbaImage>,
    pub sampler: SamplerDesc,
}

static GENERATED_TEXTURES: std::sync::LazyLock<RwLock<HashMap<String, GeneratedTexture>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));
static GENERATED_TEXTURES_PENDING: std::sync::LazyLock<Mutex<HashSet<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));
static TEXTURE_REGISTRY_GENERATION: AtomicU64 = AtomicU64::new(1);

#[inline(always)]
fn touch_texture_registry() {
    TEXTURE_REGISTRY_GENERATION.fetch_add(1, Ordering::Relaxed);
}

#[inline(always)]
pub fn texture_registry_generation() -> u64 {
    TEXTURE_REGISTRY_GENERATION.load(Ordering::Relaxed)
}

fn note_texture_handle_alias(
    aliases: &mut FastU64Map<TextureHandle>,
    key: &str,
    handle: TextureHandle,
) {
    let folded = ascii_ci_hash(key);
    match aliases.get_mut(&folded) {
        Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
        Some(_) => {}
        None => {
            aliases.insert(folded, handle);
        }
    }
}

fn rebuild_texture_handle_aliases(
    handles: &HashMap<String, TextureHandle>,
    aliases: &mut FastU64Map<TextureHandle>,
) {
    aliases.clear();
    aliases.reserve(handles.len());
    for (key, &handle) in handles {
        note_texture_handle_alias(aliases, key, handle);
    }
}

pub(crate) fn register_texture_handle(key: &str, handle: TextureHandle) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    let replaced = handles.insert(key.to_string(), handle);
    if replaced.is_some_and(|old| old != handle) {
        rebuild_texture_handle_aliases(&handles, &mut aliases);
        touch_texture_registry();
    } else if replaced.is_none() {
        note_texture_handle_alias(&mut aliases, key, handle);
        touch_texture_registry();
    }
}

pub(crate) fn remove_texture_handle(key: &str) {
    let mut handles = TEXTURE_HANDLES.write().unwrap();
    if handles.remove(key).is_none() {
        return;
    }
    let mut aliases = TEXTURE_HANDLE_ALIASES.write().unwrap();
    rebuild_texture_handle_aliases(&handles, &mut aliases);
    touch_texture_registry();
}

pub(crate) fn clear_texture_handles() {
    TEXTURE_HANDLES.write().unwrap().clear();
    TEXTURE_HANDLE_ALIASES.write().unwrap().clear();
    touch_texture_registry();
}

pub fn register_texture_dims(key: &str, w: u32, h: u32) {
    let sheet = parse_sprite_sheet_dims(key);
    let same_meta = TEX_META
        .read()
        .unwrap()
        .get(key)
        .is_some_and(|meta| meta.w == w && meta.h == h);
    if same_meta && SHEET_DIMS.read().unwrap().get(key).copied() == Some(sheet) {
        return;
    }

    let key = key.to_string();
    let mut m = TEX_META.write().unwrap();
    m.insert(key.clone(), TexMeta { w, h });
    drop(m);
    SHEET_DIMS.write().unwrap().insert(key, sheet);
    touch_texture_registry();
}

pub fn texture_dims(key: &str) -> Option<TexMeta> {
    TEX_META.read().unwrap().get(key).copied()
}

pub fn sprite_sheet_dims(key: &str) -> (u32, u32) {
    if let Some(dims) = SHEET_DIMS.read().unwrap().get(key).copied() {
        return dims;
    }
    let dims = parse_sprite_sheet_dims(key);
    SHEET_DIMS.write().unwrap().insert(key.to_string(), dims);
    dims
}

pub fn texture_handle(key: &str) -> TextureHandle {
    if let Some(handle) = TEXTURE_HANDLES.read().unwrap().get(key).copied() {
        return handle;
    }
    if let Some(handle) = TEXTURE_HANDLE_ALIASES
        .read()
        .unwrap()
        .get(&ascii_ci_hash(key))
        .copied()
        && handle != INVALID_TEXTURE_HANDLE
    {
        return handle;
    }
    TEXTURE_HANDLES
        .read()
        .unwrap()
        .iter()
        .find_map(|(candidate, handle)| candidate.eq_ignore_ascii_case(key).then_some(*handle))
        .unwrap_or(INVALID_TEXTURE_HANDLE)
}

pub fn register_generated_texture(key: &str, image: RgbaImage, sampler: SamplerDesc) {
    let (w, h) = (image.width(), image.height());
    GENERATED_TEXTURES.write().unwrap().insert(
        key.to_string(),
        GeneratedTexture {
            image: Arc::new(image),
            sampler,
        },
    );
    GENERATED_TEXTURES_PENDING
        .lock()
        .unwrap()
        .insert(key.to_string());
    register_texture_dims(key, w, h);
}

pub(crate) fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
}

pub(crate) fn take_pending_generated_texture_keys() -> Vec<String> {
    let mut pending = GENERATED_TEXTURES_PENDING.lock().unwrap();
    if pending.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(pending.len());
    out.extend(pending.drain());
    out
}

pub fn canonical_texture_key<P: AsRef<Path>>(p: P) -> String {
    let p = p.as_ref();
    // Try stripping data-dir or exe-dir asset prefix for absolute paths.
    if let Some(rel) = dirs::app_dirs().strip_asset_prefix(p) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    let rel = p.strip_prefix(Path::new("assets")).unwrap_or(p);
    rel.to_string_lossy().replace('\\', "/")
}

pub(crate) fn append_noteskins_pngs_recursive(list: &mut Vec<(String, String)>, folder: &str) {
    let roots = dirs::app_dirs().noteskin_roots();
    let mut seen_keys = HashSet::new();
    for root in &roots {
        let base = root.parent().expect("noteskin root has parent");
        let mut dirs = vec![base.join(folder)];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if !path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                {
                    continue;
                }
                let key = canonical_texture_key(&path);
                if key.starts_with("noteskins/") && seen_keys.insert(key.clone()) {
                    let file_path = path.to_string_lossy().replace('\\', "/");
                    list.push((key, file_path));
                }
            }
        }
    }
}

fn append_graphic_textures(
    list: &mut Vec<(String, String)>,
    folder: &str,
    love_first: bool,
    require_multiframe_hint: bool,
) {
    for texture in discover_graphic_textures(folder, love_first, require_multiframe_hint) {
        list.push((texture.key, texture.source_path));
    }
}

impl AssetManager {
    pub fn load_initial_textures(&mut self, backend: &mut Backend) -> Result<(), AssetError> {
        debug!("Loading initial textures...");

        #[inline(always)]
        fn fallback_rgba() -> RgbaImage {
            let data: [u8; 16] = [
                255, 0, 255, 255, 128, 128, 128, 255, 128, 128, 128, 255, 255, 0, 255, 255,
            ];
            RgbaImage::from_raw(2, 2, data.to_vec()).expect("fallback image")
        }

        let white_img = RgbaImage::from_raw(1, 1, vec![255, 255, 255, 255]).unwrap();
        let white_tex = backend.create_texture(&white_img, SamplerDesc::default())?;
        self.insert_texture("__white".to_string(), white_tex, 1, 1);
        register_texture_dims("__white", 1, 1);
        debug!("Loaded built-in texture: __white");

        let black_img = RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255]).unwrap();
        let black_tex = backend.create_texture(&black_img, SamplerDesc::default())?;
        self.insert_texture("__black".to_string(), black_tex, 1, 1);
        register_texture_dims("__black", 1, 1);
        debug!("Loaded built-in texture: __black");

        let mut textures_to_load: Vec<(String, String)> = vec![
            ("logo.png".to_string(), "logo.png".to_string()),
            ("init_arrow.png".to_string(), "init_arrow.png".to_string()),
            ("dance.png".to_string(), "dance.png".to_string()),
            (
                "select_mode/arrow-body.png".to_string(),
                "select_mode/arrow-body.png".to_string(),
            ),
            (
                "select_mode/arrow-border.png".to_string(),
                "select_mode/arrow-border.png".to_string(),
            ),
            (
                "select_mode/arrow-stripes.png".to_string(),
                "select_mode/arrow-stripes.png".to_string(),
            ),
            (
                "select_mode/center-body.png".to_string(),
                "select_mode/center-body.png".to_string(),
            ),
            (
                "select_mode/center-border.png".to_string(),
                "select_mode/center-border.png".to_string(),
            ),
            (
                "select_mode/center-feet.png".to_string(),
                "select_mode/center-feet.png".to_string(),
            ),
            (
                "test_input/dance.png".to_string(),
                "test_input/dance.png".to_string(),
            ),
            (
                "test_input/buttons.png".to_string(),
                "test_input/buttons.png".to_string(),
            ),
            (
                "test_input/highlight.png".to_string(),
                "test_input/highlight.png".to_string(),
            ),
            (
                "test_input/highlightgreen.png".to_string(),
                "test_input/highlightgreen.png".to_string(),
            ),
            (
                "test_input/highlightred.png".to_string(),
                "test_input/highlightred.png".to_string(),
            ),
            (
                "test_input/highlightarrow.png".to_string(),
                "test_input/highlightarrow.png".to_string(),
            ),
            (
                "test_lights/bass light (blue).png".to_string(),
                "test_lights/bass light (blue).png".to_string(),
            ),
            (
                "test_lights/blue.png".to_string(),
                "test_lights/blue.png".to_string(),
            ),
            (
                "test_lights/cabinet ITG2.png".to_string(),
                "test_lights/cabinet ITG2.png".to_string(),
            ),
            (
                "test_lights/dance.png".to_string(),
                "test_lights/dance.png".to_string(),
            ),
            (
                "test_lights/highlight.png".to_string(),
                "test_lights/highlight.png".to_string(),
            ),
            (
                "test_lights/pink.png".to_string(),
                "test_lights/pink.png".to_string(),
            ),
            (
                "test_lights/red.png".to_string(),
                "test_lights/red.png".to_string(),
            ),
            (
                "test_lights/white.png".to_string(),
                "test_lights/white.png".to_string(),
            ),
            ("meter_arrow.png".to_string(), "meter_arrow.png".to_string()),
            (
                "name_entry_cursor.png".to_string(),
                "name_entry_cursor.png".to_string(),
            ),
            ("has_lua.png".to_string(), "has_lua.png".to_string()),
            ("has_edit.png".to_string(), "has_edit.png".to_string()),
            (
                "rounded-square.png".to_string(),
                "rounded-square.png".to_string(),
            ),
            ("circle.png".to_string(), "circle.png".to_string()),
            ("swoosh.png".to_string(), "swoosh.png".to_string()),
            (
                "graphics/menu_bg_technique/arrow_tex.png".to_string(),
                "menu_bg_technique/arrow_tex.png".to_string(),
            ),
            (
                "graphics/menu_bg_technique/square.png".to_string(),
                "menu_bg_technique/square.png".to_string(),
            ),
            (
                "graphics/menu_bg_technique/white_tex.png".to_string(),
                "menu_bg_technique/white_tex.png".to_string(),
            ),
            ("fave-icon.png".to_string(), "fave-icon.png".to_string()),
            ("lock.png".to_string(), "lock.png".to_string()),
            (
                "folder-solid.png".to_string(),
                "folder-solid.png".to_string(),
            ),
            ("GrooveStats.png".to_string(), "GrooveStats.png".to_string()),
            ("nice.png".to_string(), "nice.png".to_string()),
            (
                "BoogieStatsEX.png".to_string(),
                "BoogieStatsEX.png".to_string(),
            ),
            ("arrowcloud.png".to_string(), "arrowcloud.png".to_string()),
            ("ITL.png".to_string(), "ITL.png".to_string()),
            ("crown.png".to_string(), "crown.png".to_string()),
            (
                "srpg9_logo_alt.png".to_string(),
                "srpg9_logo_alt.png".to_string(),
            ),
            (
                "srpg10_logo_alt.png".to_string(),
                "srpg10_logo_alt.png".to_string(),
            ),
            (
                visual_styles::SRPG10_TITLE_LOGO.to_string(),
                visual_styles::SRPG10_TITLE_LOGO.to_string(),
            ),
            (
                "combo_explosion.png".to_string(),
                "combo_explosion.png".to_string(),
            ),
            (
                "banner1.png".to_string(),
                "_fallback/banner1.png".to_string(),
            ),
            (
                "banner2.png".to_string(),
                "_fallback/banner2.png".to_string(),
            ),
            (
                "banner3.png".to_string(),
                "_fallback/banner3.png".to_string(),
            ),
            (
                "banner4.png".to_string(),
                "_fallback/banner4.png".to_string(),
            ),
            (
                "banner5.png".to_string(),
                "_fallback/banner5.png".to_string(),
            ),
            (
                "banner6.png".to_string(),
                "_fallback/banner6.png".to_string(),
            ),
            (
                "banner7.png".to_string(),
                "_fallback/banner7.png".to_string(),
            ),
            (
                "banner8.png".to_string(),
                "_fallback/banner8.png".to_string(),
            ),
            (
                "banner9.png".to_string(),
                "_fallback/banner9.png".to_string(),
            ),
            (
                "banner10.png".to_string(),
                "_fallback/banner10.png".to_string(),
            ),
            (
                "banner11.png".to_string(),
                "_fallback/banner11.png".to_string(),
            ),
            (
                "banner12.png".to_string(),
                "_fallback/banner12.png".to_string(),
            ),
            (
                "grades/grades 1x19.png".to_string(),
                "grades/grades 1x19.png".to_string(),
            ),
            (
                "evaluation/failed.png".to_string(),
                "evaluation/failed.png".to_string(),
            ),
            (
                "evaluation/cleared.png".to_string(),
                "evaluation/cleared.png".to_string(),
            ),
            (
                "feet-diagram.png".to_string(),
                "feet-diagram.png".to_string(),
            ),
            (
                "practice/snap_display_icon_9x1 (doubleres).png".to_string(),
                "practice/snap_display_icon_9x1 (doubleres).png".to_string(),
            ),
        ];

        for asset in visual_styles::all_assets() {
            textures_to_load.push((
                asset.select_color.to_string(),
                asset.select_color.to_string(),
            ));
            textures_to_load.push((
                asset.shared_background.to_string(),
                asset.shared_background.to_string(),
            ));
            for effect in [
                asset.effects.titlemenu_flycenter,
                asset.effects.titlemenu_flytop,
                asset.effects.titlemenu_flybottom,
                asset.effects.gameplayin_splode,
                asset.effects.gameplayin_minisplode,
                asset.effects.combo_100milestone_splode,
                asset.effects.combo_100milestone_minisplode,
                asset.effects.combo_1000milestone_swoosh,
            ] {
                textures_to_load.push((effect.to_string(), effect.to_string()));
            }
        }

        for p in visual_styles::SRPG10_EVAL_TEXTURES {
            textures_to_load.push((p.to_string(), p.to_string()));
        }

        for p in [
            "grades/star.png",
            "grades/s-plus.png",
            "grades/s.png",
            "grades/s-minus.png",
            "grades/a-plus.png",
            "grades/a.png",
            "grades/a-minus.png",
            "grades/b-plus.png",
            "grades/b.png",
            "grades/b-minus.png",
            "grades/c-plus.png",
            "grades/c.png",
            "grades/c-minus.png",
            "grades/d.png",
            "grades/f.png",
            "grades/q.png",
            "grades/affluent.png",
            "grades/goldstar (stretch).png",
        ] {
            textures_to_load.push((p.to_string(), p.to_string()));
        }

        for p in [
            "submit/LoadingSpinner_10x3.png",
            "submit/Hourglass_10x3.png",
            "submit/Check_1x1.png",
            "submit/Refresh_1x1.png",
            "submit/Rejected_1x1.png",
        ] {
            textures_to_load.push((p.to_string(), p.to_string()));
        }

        for p in deadsync_theme::step_stats_gifs::STEP_STATS_GIF_TEXTURES {
            textures_to_load.push((p.to_string(), p.to_string()));
        }

        append_noteskins_pngs_recursive(&mut textures_to_load, "noteskins");
        append_graphic_textures(&mut textures_to_load, "judgements", true, true);
        append_graphic_textures(&mut textures_to_load, "hold_judgements", false, true);
        append_graphic_textures(&mut textures_to_load, "held_miss", false, false);

        #[inline(always)]
        fn decode_rgba(
            key: String,
            relative_path: String,
        ) -> Result<(String, RgbaImage), (String, String)> {
            let rel = Path::new(&relative_path);
            let path = if rel.is_absolute() {
                rel.to_path_buf()
            } else if relative_path.starts_with("noteskins/") {
                Path::new("assets").join(&relative_path)
            } else {
                Path::new("assets/graphics").join(&relative_path)
            };
            let path = dirs::app_dirs().resolve_asset_path(&path.to_string_lossy());
            match open_image_fallback(&path) {
                Ok(img) => {
                    let mut rgba = img.to_rgba8();
                    fix_hidden_alpha(&mut rgba);
                    Ok((key, rgba))
                }
                Err(e) => Err((key, e.to_string())),
            }
        }

        let job_count = textures_to_load.len();
        let worker_count = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1)
            .min(job_count.max(1));

        let (job_tx, job_rx) = mpsc::channel::<(String, String)>();
        let job_rx = Arc::new(Mutex::new(job_rx));
        let (res_tx, res_rx) = mpsc::channel::<Result<(String, RgbaImage), (String, String)>>();

        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let job_rx = Arc::clone(&job_rx);
            let res_tx = res_tx.clone();
            workers.push(std::thread::spawn(move || {
                loop {
                    let job = {
                        let Ok(rx) = job_rx.lock() else { return };
                        rx.recv()
                    };
                    let Ok((key, relative_path)) = job else {
                        return;
                    };
                    let _ = res_tx.send(decode_rgba(key, relative_path));
                }
            }));
        }
        drop(res_tx);
        for (key, relative_path) in textures_to_load {
            let _ = job_tx.send((key, relative_path));
        }
        drop(job_tx);

        let fallback_image = Arc::new(fallback_rgba());
        for r in res_rx {
            match r {
                Ok((key, rgba)) => {
                    let sampler = if needs_repeat_sampler(&key) {
                        SamplerDesc {
                            wrap: SamplerWrap::Repeat,
                            ..SamplerDesc::default()
                        }
                    } else if key.starts_with("noteskins/") {
                        parse_texture_hints(&key).sampler_desc()
                    } else {
                        SamplerDesc::default()
                    };
                    let texture = backend.create_texture(&rgba, sampler)?;
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    debug!("Loaded texture: {key}");
                    self.insert_texture(key, texture, rgba.width(), rgba.height());
                }
                Err((key, msg)) => {
                    warn!("Failed to load texture for key '{key}': {msg}. Using fallback.");
                    let sampler = if needs_repeat_sampler(&key) {
                        SamplerDesc {
                            wrap: SamplerWrap::Repeat,
                            ..SamplerDesc::default()
                        }
                    } else if key.starts_with("noteskins/") {
                        parse_texture_hints(&key).sampler_desc()
                    } else {
                        SamplerDesc::default()
                    };
                    let texture = backend.create_texture(&fallback_image, sampler)?;
                    register_texture_dims(&key, fallback_image.width(), fallback_image.height());
                    self.insert_texture(
                        key,
                        texture,
                        fallback_image.width(),
                        fallback_image.height(),
                    );
                }
            }
        }

        for w in workers {
            w.join().expect("texture decode worker panicked");
        }

        Ok(())
    }

    pub(crate) fn ensure_texture_for_key(&mut self, backend: &mut Backend, texture_key: &str) {
        self.load_texture_key(backend, texture_key, None, false);
    }

    pub(crate) fn ensure_texture_for_key_with_sampler(
        &mut self,
        backend: &mut Backend,
        texture_key: &str,
        sampler: SamplerDesc,
    ) {
        self.load_texture_key(backend, texture_key, Some(sampler), true);
    }

    fn load_texture_key(
        &mut self,
        backend: &mut Backend,
        texture_key: &str,
        sampler_override: Option<SamplerDesc>,
        force_reload: bool,
    ) {
        if texture_key.is_empty() {
            return;
        }
        let key = canonical_texture_key(texture_key);
        if !force_reload && self.has_texture_key(&key) {
            return;
        }
        if let Some(generated) = generated_texture(&key) {
            let sampler = sampler_override.unwrap_or(generated.sampler);
            match backend.create_texture(generated.image.as_ref(), sampler) {
                Ok(texture) => {
                    self.set_texture_for_key(
                        backend,
                        key,
                        texture,
                        generated.image.width(),
                        generated.image.height(),
                    );
                }
                Err(e) => {
                    warn!("Failed to create GPU texture for generated key '{texture_key}': {e}");
                }
            }
            return;
        }
        if key.starts_with("__") {
            return;
        }

        let path = direct_texture_key_path(texture_key, &key).unwrap_or_else(|| {
            let dirs = dirs::app_dirs();
            let path = dirs.resolve_asset_path(&format!("assets/{key}"));
            if path.is_file() {
                path
            } else {
                dirs.resolve_asset_path(&format!("assets/graphics/{key}"))
            }
        });
        if !path.is_file() {
            warn!("Failed to resolve texture key '{key}' for preload.");
            return;
        }

        let hints = parse_texture_hints(&key);
        let sampler = sampler_override.unwrap_or_else(|| {
            if needs_repeat_sampler(&key) {
                SamplerDesc {
                    wrap: SamplerWrap::Repeat,
                    ..hints.sampler_desc()
                }
            } else {
                hints.sampler_desc()
            }
        });
        match open_image_fallback(&path) {
            Ok(img) => {
                let mut rgba = img.to_rgba8();
                if !hints.is_default() {
                    apply_texture_hints(&mut rgba, &hints);
                }
                fix_hidden_alpha(&mut rgba);
                match backend.create_texture(&rgba, sampler) {
                    Ok(texture) => {
                        self.set_texture_for_key(
                            backend,
                            key.clone(),
                            texture,
                            rgba.width(),
                            rgba.height(),
                        );
                        register_texture_dims(&key, rgba.width(), rgba.height());
                    }
                    Err(e) => {
                        warn!("Failed to create GPU texture for key '{key}': {e}");
                    }
                }
            }
            Err(e) => {
                warn!("Failed to open texture for key '{key}': {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goldstar_uses_repeat_sampler() {
        assert!(needs_repeat_sampler("grades/goldstar (stretch).png"));
    }

    #[test]
    fn texture_handle_lookup_tracks_registry_lifecycle() {
        clear_texture_handles();

        register_texture_handle("Graphics/Banner.png", 17);
        assert_eq!(texture_handle("Graphics/Banner.png"), 17);
        assert_eq!(texture_handle("graphics/banner.png"), 17);

        remove_texture_handle("Graphics/Banner.png");
        assert_eq!(
            texture_handle("graphics/banner.png"),
            deadlib_render::INVALID_TEXTURE_HANDLE
        );

        register_texture_handle("Other.png", 23);
        clear_texture_handles();
        assert_eq!(
            texture_handle("other.png"),
            deadlib_render::INVALID_TEXTURE_HANDLE
        );
    }
}
