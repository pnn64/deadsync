use deadlib_assets::{
    AssetManager, TextureAssetSpec, TextureChoice, TextureDecodeJob, TextureHints,
    canonical_texture_key_with_asset_roots, parse_texture_hints, strip_sprite_hints,
    texture_filename_has_multiframe_hint, texture_key_sampler,
};
use deadlib_render::Backend;
use deadlib_render_core::{SamplerDesc, SamplerWrap};
use deadsync_config::dirs::AssetPaths;
use log::warn;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Clone, Copy)]
struct GraphicTextureDiscovery {
    folder: &'static str,
    love_first: bool,
    require_multiframe_hint: bool,
}

// Session catalog snapshots, populated on first menu lookup. Discovery and sorting
// run once per catalog; subsequent reads and actor bindings reuse retained storage.
// The menu thread owns warmup; OnceLock publishes immutable results. Capacity is
// the discovered file set, with no later insertion, eviction, or gameplay scanning.
static GRAPHIC_TEXTURE_CHOICES: GraphicTextureChoiceCache = GraphicTextureChoiceCache::new();

#[must_use]
pub fn graphic_texture_roots(folder: &str) -> Vec<PathBuf> {
    let dirs = crate::paths();
    graphic_roots_in_dirs(folder, &dirs.graphic_roots)
}

pub fn judgment_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.judgment_texture_choices(graphic_texture_roots)
}

pub fn hold_judgment_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.hold_judgment_texture_choices(graphic_texture_roots)
}

pub fn held_miss_texture_choices() -> &'static [TextureChoice] {
    GRAPHIC_TEXTURE_CHOICES.held_miss_texture_choices(graphic_texture_roots)
}

pub fn canonical_texture_key<P: AsRef<Path>>(p: P) -> String {
    let dirs = crate::paths();
    let raw = p.as_ref().to_string_lossy();
    let source = texture_source_key(&raw);
    let key = canonical_texture_key_with_asset_roots(Path::new(source), &dirs.texture_roots);
    if source.len() != raw.len() {
        model_texture_key(&key)
    } else {
        key
    }
}

const MODEL_TEXTURE_SUFFIX: &str = "#itg-model";

/// A native Model load requests a distinct RageTextureID from a Sprite load.
/// Keep its prepared image resident separately; source lookup strips this tag.
#[must_use]
pub fn model_texture_key(key: &str) -> String {
    if key.starts_with("__") || key.ends_with(MODEL_TEXTURE_SUFFIX) {
        key.to_owned()
    } else {
        format!("{key}{MODEL_TEXTURE_SUFFIX}")
    }
}

#[must_use]
pub fn texture_source_key(key: &str) -> &str {
    key.strip_suffix(MODEL_TEXTURE_SUFFIX).unwrap_or(key)
}

fn decode_texture_hints(key: &str) -> TextureHints {
    let source = texture_source_key(key);
    let mut hints = parse_texture_hints(source);
    // The native game profile caps images to 2048; allocations below 8 force
    // both axes to stretch. Logical Sprite sizes retain their original source.
    hints.max_size = Some(2048);
    hints.min_size = Some(8);
    if source.len() != key.len() {
        hints.non_default = true;
        hints.stretch = true;
        hints.hot_pink_color_key = true;
        hints.max_size = Some(2048); // Native RageTextureID::Init default.
    }
    hints
}

#[must_use]
pub fn model_texture_sampler(key: &str) -> SamplerDesc {
    SamplerDesc {
        wrap: SamplerWrap::Repeat,
        ..parse_texture_hints(texture_source_key(key)).sampler_desc()
    }
}

/// Resolve a native source for a worker without decoding or uploading it.
pub fn texture_decode_job(key: &str, model: bool) -> TextureDecodeJob {
    let path = texture_key_source_path(key, key, |path| crate::paths().resolve_asset_path(path));
    let hints = decode_texture_hints(key);
    TextureDecodeJob {
        key: key.to_owned(),
        path,
        sampler: texture_key_sampler(&hints, model),
        hints,
    }
}

/// Decode and upload resolved native textures before entering a screen.
/// Shared source keys are already deduplicated by the caller; resident textures
/// are reused. The ordinary bounded decode workers stream directly to the GPU.
///
/// # Errors
/// Returns an error if a GPU texture cannot be created.
pub fn preload_texture_keys(
    assets: &mut AssetManager,
    backend: &mut Backend,
    textures: impl IntoIterator<Item = (std::sync::Arc<str>, bool)>,
) -> Result<(), deadlib_assets::AssetError> {
    let mut jobs = Vec::new();
    for (key, model) in textures {
        if assets.has_uploaded_texture_key(&key)
            || assets.load_generated_texture(backend, &key, None)?
        {
            continue;
        }
        jobs.push(texture_decode_job(&key, model));
    }
    if !jobs.is_empty() {
        assets.load_textures(backend, jobs)?;
    }
    Ok(())
}

pub fn initial_texture_jobs(
    texture_assets: impl IntoIterator<Item = TextureAssetSpec>,
    dirs: &AssetPaths,
    needs_repeat: fn(&str) -> bool,
) -> Vec<TextureDecodeJob> {
    // Installed noteskins stay on disk. Options loads bounded preview textures;
    // gameplay prewarms the selected skins before starting a song.
    let textures = texture_assets
        .into_iter()
        .map(|asset| {
            (
                asset.key.to_string(),
                initial_texture_source_path(asset.path, |path| dirs.resolve_asset_path(path)),
            )
        })
        .chain(INITIAL_GRAPHIC_TEXTURES.iter().flat_map(|spec| {
            discover_graphic_textures_in_roots(
                spec.folder,
                graphic_roots_in_dirs(spec.folder, &dirs.graphic_roots),
                spec.love_first,
                spec.require_multiframe_hint,
            )
            .into_iter()
            .map(|texture| (texture.key, texture.source_path))
        }));
    textures
        .map(|(key, path)| TextureDecodeJob {
            sampler: initial_texture_sampler(&key, needs_repeat(&key)),
            hints: decode_texture_hints(&key),
            key,
            path,
        })
        .collect()
}

pub fn ensure_texture_for_key(
    assets: &mut AssetManager,
    backend: &mut Backend,
    texture_key: &str,
    needs_repeat: fn(&str) -> bool,
) {
    load_texture_key(assets, backend, texture_key, None, needs_repeat);
}

/// An explicit sampler forces replacement, even if this key is already loaded.
pub fn ensure_texture_for_key_with_sampler(
    assets: &mut AssetManager,
    backend: &mut Backend,
    texture_key: &str,
    sampler: SamplerDesc,
) {
    load_texture_key(assets, backend, texture_key, Some(sampler), |_| false);
}

fn load_texture_key(
    assets: &mut AssetManager,
    backend: &mut Backend,
    texture_key: &str,
    sampler_override: Option<SamplerDesc>,
    needs_repeat: fn(&str) -> bool,
) {
    if texture_key.is_empty() {
        return;
    }
    let key = canonical_texture_key(texture_key);
    if sampler_override.is_none() && assets.has_texture_key(&key) {
        return;
    }
    match assets.load_generated_texture(backend, &key, sampler_override) {
        Ok(true) => return,
        Err(error) => {
            warn!("Failed to create generated GPU texture for key '{key}': {error}");
            return;
        }
        Ok(false) => {}
    }
    if key.starts_with("__") {
        return;
    }
    let path = texture_key_source_path(texture_key, &key, |path| {
        crate::paths().resolve_asset_path(path)
    });
    if !path.is_file() {
        warn!("Failed to resolve texture key '{key}' for preload.");
        return;
    }
    let hints = decode_texture_hints(&key);
    let sampler =
        sampler_override.unwrap_or_else(|| texture_key_sampler(&hints, needs_repeat(&key)));
    let job = TextureDecodeJob {
        key,
        path,
        sampler,
        hints,
    };
    if let Err(error) = assets.load_texture(backend, &job) {
        warn!("Failed to load texture for key '{}': {error}", job.key);
    }
}

fn initial_texture_sampler(key: &str, needs_repeat: bool) -> SamplerDesc {
    if needs_repeat {
        SamplerDesc {
            wrap: SamplerWrap::Repeat,
            ..SamplerDesc::default()
        }
    } else if key.starts_with("noteskins/") {
        parse_texture_hints(key).sampler_desc()
    } else {
        SamplerDesc::default()
    }
}

const NONE_TEXTURE_CHOICE_KEY: &str = "None";
const INITIAL_GRAPHIC_TEXTURES: [GraphicTextureDiscovery; 4] = [
    GraphicTextureDiscovery {
        folder: "judgements",
        love_first: true,
        require_multiframe_hint: true,
    },
    GraphicTextureDiscovery {
        folder: "hold_judgements",
        love_first: false,
        require_multiframe_hint: true,
    },
    GraphicTextureDiscovery {
        folder: "held_miss",
        love_first: false,
        require_multiframe_hint: false,
    },
    GraphicTextureDiscovery {
        folder: "step_stats_gifs",
        love_first: false,
        require_multiframe_hint: true,
    },
];

struct GraphicTextureChoiceCache {
    judgment: OnceLock<Vec<TextureChoice>>,
    hold_judgment: OnceLock<Vec<TextureChoice>>,
    held_miss: OnceLock<Vec<TextureChoice>>,
}

impl GraphicTextureChoiceCache {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            judgment: OnceLock::new(),
            hold_judgment: OnceLock::new(),
            held_miss: OnceLock::new(),
        }
    }

    pub fn judgment_texture_choices(
        &self,
        graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    ) -> &[TextureChoice] {
        self.judgment
            .get_or_init(|| {
                texture_choices_from_folder(INITIAL_GRAPHIC_TEXTURES[0], true, graphic_roots)
            })
            .as_slice()
    }

    pub fn hold_judgment_texture_choices(
        &self,
        graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    ) -> &[TextureChoice] {
        self.hold_judgment
            .get_or_init(|| {
                texture_choices_from_folder(INITIAL_GRAPHIC_TEXTURES[1], true, graphic_roots)
            })
            .as_slice()
    }

    pub fn held_miss_texture_choices(
        &self,
        graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    ) -> &[TextureChoice] {
        self.held_miss
            .get_or_init(|| {
                texture_choices_from_folder(INITIAL_GRAPHIC_TEXTURES[2], true, graphic_roots)
            })
            .as_slice()
    }
}

fn texture_choices_from_folder(
    spec: GraphicTextureDiscovery,
    include_none: bool,
    graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
) -> Vec<TextureChoice> {
    let discovered = discover_graphic_textures_in_roots(
        spec.folder,
        graphic_roots(spec.folder),
        spec.love_first,
        spec.require_multiframe_hint,
    );
    texture_choices_from_discovered(discovered, include_none)
}

#[derive(Clone, Debug)]
struct DiscoveredTexture {
    pub key: String,
    pub label: String,
    pub source_path: PathBuf,
}

fn absolute_or_self(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

#[must_use]
fn graphic_roots_in_dirs(folder: &str, bases: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::with_capacity(bases.len());
    for base in bases {
        let root = base.join(folder);
        if root.is_dir() && !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

fn is_png_file(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

fn discover_graphic_textures_in_roots(
    folder: &str,
    roots: impl IntoIterator<Item = PathBuf>,
    love_first: bool,
    require_multiframe_hint: bool,
) -> Vec<DiscoveredTexture> {
    let mut discovered = Vec::new();
    let mut seen_keys = HashSet::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            if require_multiframe_hint && !texture_filename_has_multiframe_hint(file_name) {
                continue;
            }
            if !require_multiframe_hint && !is_png_file(file_name) {
                continue;
            }
            let is_file = match entry.file_type() {
                Ok(kind) if !kind.is_symlink() => kind.is_file(),
                _ => entry.path().is_file(),
            };
            if !is_file {
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
                source_path: absolute_or_self(&entry.path()),
            });
        }
    }
    sort_discovered_textures(&mut discovered, love_first);
    discovered
}

#[inline]
fn ascii_case_insensitive_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

fn sort_discovered_textures(discovered: &mut [DiscoveredTexture], love_first: bool) {
    discovered.sort_by(|a, b| {
        let a_love = love_first && a.label.eq_ignore_ascii_case("Love");
        let b_love = love_first && b.label.eq_ignore_ascii_case("Love");
        match (a_love, b_love) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => ascii_case_insensitive_cmp(&a.label, &b.label),
        }
    });
}

fn texture_choices_from_discovered(
    discovered: impl IntoIterator<Item = DiscoveredTexture>,
    include_none: bool,
) -> Vec<TextureChoice> {
    let mut choices: Vec<TextureChoice> = discovered
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

fn initial_texture_source_path(
    relative_path: &str,
    resolve_asset_path: impl FnOnce(&str) -> PathBuf,
) -> PathBuf {
    let rel = Path::new(relative_path);
    let path = if rel.is_absolute() {
        rel.to_path_buf()
    } else if relative_path.starts_with("noteskins/") {
        Path::new("assets").join(relative_path)
    } else {
        Path::new("assets/graphics").join(relative_path)
    };
    resolve_asset_path(&path.to_string_lossy())
}

pub fn texture_key_source_path(
    raw: &str,
    key: &str,
    resolve_asset_path: impl Fn(&str) -> PathBuf,
) -> PathBuf {
    let raw = texture_source_key(raw);
    let key = texture_source_key(key);
    if let Some(path) = deadlib_assets::direct_texture_key_path(raw, key) {
        return path;
    }
    let asset_path = resolve_asset_path(&format!("assets/{key}"));
    if asset_path.is_file() {
        asset_path
    } else {
        resolve_asset_path(&format!("assets/graphics/{key}"))
    }
}

pub fn resolve_texture_choice_key<'a>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_render_core::SamplerFilter;

    #[test]
    fn native_sprite_prepared_pixels() {
        crate::init_asset_paths();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/texture-sprite-preparation");
        let control: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("control.json")).expect("native Sprite controls"),
        ).expect("Sprite control JSON");
        let native: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("native.json")).expect("native Sprite pixels"),
        ).expect("native Sprite JSON");
        let mut failures = Vec::new();
        let mut checked = 0;
        for (spec, case) in control["texture_files"].as_array().expect("controls").iter()
            .zip(native["cases"].as_array().expect("native cases")) {
            // The production headless profile resolves high-resolution textures on.
            if spec["high_resolution"] == false { continue; }
            let name = case["name"].as_str().expect("case name");
            let source = root.join(spec["file"].as_str().expect("source file"));
            let key = canonical_texture_key(&source);
            let job = texture_decode_job(&key, false);
            let image = deadlib_assets::decode_texture_image(&job.path, &job.hints)
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            checked += 1;
            if serde_json::json!([image.width(), image.height()]) != case["dimensions"]["image"] {
                failures.push(format!("{name}: actual {:?}, native {}", image.dimensions(), case["dimensions"]["image"]));
                continue;
            }
            if case["upload"]["pixels_captured"] == true {
                let expected: Vec<u8> = case["upload"]["pixels"].as_array().expect("native pixels").iter()
                    .flat_map(|pixel| pixel.as_array().expect("RGBA").iter().map(|byte| byte.as_u64().expect("byte") as u8)).collect();
                if image.as_raw() != &expected {
                    let difference = image.as_raw().iter().zip(&expected).position(|(actual, expected)| actual != expected);
                    failures.push(format!("{name}: native pixel difference at {difference:?}"));
                }
            }
        }
        assert_eq!(checked, 9);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn native_indexed_bitmap_pixels() {
        crate::init_asset_paths();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files");
        let cases: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("cases.json")).expect("native indexed manifest"),
        )
        .expect("native indexed case JSON");
        let cases = cases.as_array().expect("native indexed cases");
        assert_eq!(cases.len(), 33);
        let mut failures = Vec::new();
        for case in cases {
            let name = case["name"].as_str().expect("case name");
            let source = root.join(case["file"].as_str().expect("source file"));
            let (width, height) =
                deadlib_assets::texture_source_size(&source).expect("native source dimensions");
            assert_eq!(
                serde_json::json!([width, height]),
                case["source"],
                "{name}: source metadata"
            );
            let model = case["kind"] == "model";
            let key = canonical_texture_key(&source);
            let key = if model { model_texture_key(&key) } else { key };
            let job = texture_decode_job(&key, model);
            let image = match deadlib_assets::decode_texture_image(&job.path, &job.hints) {
                Ok(image) => image,
                Err(error) => {
                    failures.push(format!("{name}: {error}"));
                    continue;
                }
            };
            if serde_json::json!([image.width(), image.height()]) != case["output"] {
                failures.push(format!(
                    "{name}: dimensions {:?}, native {}",
                    image.dimensions(),
                    case["output"]
                ));
                continue;
            }
            let native = fs::read(root.join(format!("{name}.rgba"))).expect("native upload bytes");
            if let Some((index, (actual, expected))) = image
                .as_raw()
                .iter()
                .zip(&native)
                .enumerate()
                .find(|(_, (actual, expected))| actual != expected)
            {
                failures.push(format!(
                    "{name}: byte {index}, actual={actual}, native={expected}"
                ));
            }
            assert_eq!(image.as_raw().len(), native.len(), "{name}: byte count");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn native_indexed_bitmap_rejections() {
        crate::init_asset_paths();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files");
        let cases: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("native-rejections.json")).expect("native rejection manifest"),
        )
        .expect("native rejection cases");
        let cases = cases.as_array().expect("rejection array");
        assert_eq!(cases.len(), 2);
        for case in cases {
            let name = case["name"].as_str().expect("rejection name");
            let source = root.join(case["file"].as_str().expect("native rejected source"));
            let model = case["kind"] == "model";
            let key = canonical_texture_key(&source);
            let key = if model { model_texture_key(&key) } else { key };
            let job = texture_decode_job(&key, model);
            assert!(
                deadlib_assets::decode_texture_image(&job.path, &job.hints).is_err(),
                "{name}: native rejected this source"
            );
            assert!(
                deadlib_assets::texture_source_size(&source).is_err(),
                "{name}: native rejected its source dimensions"
            );
        }
    }

    #[test]
    fn native_indexed_model_dimensions() {
        crate::init_asset_paths();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/indexed-bitmap-files");
        let piece = root.join("model.txt");
        let slots = crate::noteskin::load_itg_model_slots(&piece, &piece, &piece)
            .expect("native first-frame GIF Model material");
        let key = slots[0].texture_key();
        assert_eq!(
            deadlib_assets::texture_dims(key).map(|meta| (meta.w, meta.h)),
            Some((8, 8))
        );
        let job = texture_decode_job(key, true);
        let image = deadlib_assets::decode_texture_image(&job.path, &job.hints)
            .expect("prepared GIF Model image");
        let native =
            fs::read(root.join("gif-offset-model.rgba")).expect("native GIF upload pixels");
        assert_eq!(image.dimensions(), (8, 8));
        assert_eq!(image.as_raw(), &native);
    }

    #[test]
    fn native_prepared_pixels() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/model-texture-preparation");
        let cases: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("cases.json")).expect("native preparation controls"),
        )
        .expect("valid native case manifest");
        let cases = cases.as_array().expect("native case array");
        assert_eq!(cases.len(), 21);
        for case in cases {
            let name = case["name"].as_str().expect("case name");
            let hints = TextureHints {
                non_default: true,
                hot_pink_color_key: case["hot_pink_color_key"].as_bool().expect("key policy"),
                stretch: case["stretch"].as_bool().expect("resize policy"),
                max_size: case["max_size"].as_u64().map(|size| size as u32),
                ..Default::default()
            };
            let image =
                deadlib_assets::decode_texture_image(&root.join(format!("{name}.png")), &hints)
                    .unwrap_or_else(|error| panic!("{name}: {error}"));
            let size = [image.width(), image.height()];
            assert_eq!(
                serde_json::json!(size),
                case["output"],
                "{name}: native dimensions"
            );
            let native =
                fs::read(root.join(format!("{name}.rgba"))).expect("compiled native bytes");
            assert_eq!(
                image.as_raw().len(),
                native.len(),
                "{name}: native pixel count"
            );
            if let Some((index, (actual, expected))) = image
                .as_raw()
                .iter()
                .zip(&native)
                .enumerate()
                .find(|(_, (actual, expected))| actual != expected)
            {
                panic!("{name}: native byte {index}, actual={actual}, expected={expected}");
            }
        }
    }

    #[test]
    fn native_model_pixels() {
        crate::init_asset_paths();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/model-texture-request");
        let piece = root.join("model.txt");
        let slots = crate::noteskin::load_itg_model_slots(&piece, &piece, &piece)
            .expect("native Model material control");
        let job = texture_decode_job(slots[0].texture_key(), true);
        let image = deadlib_assets::decode_texture_image(&job.path, &job.hints)
            .expect("decode actual Model source");
        let native = fs::read(root.join("native.rgba")).expect("compiled native surface bytes");
        assert_eq!(image.dimensions(), (8, 16));
        assert_eq!(image.as_raw(), &native);
    }

    #[test]
    fn model_views_keep_sprites() {
        crate::init_asset_paths();
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/itgmania-song-lua-micro/model-texture-request");
        let piece = root.join("model.txt");
        let slots = crate::noteskin::load_itg_model_slots(&piece, &piece, &piece)
            .expect("shared native Model material");
        let model = texture_decode_job(slots[0].texture_key(), true);
        let sprite = texture_decode_job(&canonical_texture_key(root.join("npot.png")), false);
        assert_eq!(canonical_texture_key(&model.key), model.key);
        assert_eq!(model_texture_key(&model.key), model.key);
        assert!(model.hints.stretch && model.hints.hot_pink_color_key);
        assert_eq!(model.hints.max_size, Some(2048));
        assert_eq!(
            deadlib_assets::texture_dims(&model.key).map(|meta| (meta.w, meta.h)),
            Some((8, 16))
        );
        assert_ne!(
            model.key, sprite.key,
            "native RageTextureID separates these requests"
        );
        assert_eq!(model.path, sprite.path);
        let image = deadlib_assets::decode_texture_image(&sprite.path, &sprite.hints)
            .expect("decode independent Sprite view");
        assert_eq!(image.dimensions(), (5, 9));
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 255, 255]);
    }

    #[test]
    fn initial_sampler_keeps_startup_policy() {
        assert_eq!(
            initial_texture_sampler("noteskins/foo (nearest).png", false).filter,
            SamplerFilter::Nearest
        );
        assert_eq!(
            initial_texture_sampler("graphics/foo (nearest).png", false).filter,
            SamplerFilter::Linear
        );
        let repeat = initial_texture_sampler("noteskins/foo (nearest).png", true);
        assert_eq!(repeat.wrap, SamplerWrap::Repeat);
        assert_eq!(repeat.filter, SamplerFilter::Linear);
    }

    #[test]
    fn discovery_keeps_overlay_precedence_and_filters() {
        let root = std::env::temp_dir().join(format!("asset-catalog-{}", std::process::id()));
        let user = root.join("user");
        let bundled = root.join("bundled");
        for (dir, names) in [
            (
                &user,
                &["Metal 2x7.png", "Love 2x7.png", "None 2x7.png", "plain.png"][..],
            ),
            (&bundled, &["LOVE 2x7.PNG", "Alpha 2x7.png"][..]),
        ] {
            fs::create_dir_all(dir).expect("create catalog fixture");
            for name in names {
                fs::write(dir.join(name), [0u8]).expect("write catalog fixture");
            }
        }
        let found =
            discover_graphic_textures_in_roots("judgements", [user.clone(), bundled], true, true);
        assert_eq!(
            found
                .iter()
                .map(|texture| texture.label.as_str())
                .collect::<Vec<_>>(),
            ["Love", "Alpha", "Metal"]
        );
        assert_eq!(found[0].source_path, user.join("Love 2x7.png"));
        fs::remove_dir_all(root).expect("remove catalog fixture");
    }

    #[test]
    fn jobs_resolve_noteskins_and_carry_startup_options() {
        let root = std::env::temp_dir().join(format!("asset-jobs-{}", std::process::id()));
        let dirs = deadsync_config::dirs::AppDirs {
            data_dir: root.join("user"),
            exe_dir: root.join("bundled"),
            cache_dir: root.join("cache"),
            portable: false,
        };
        let key = "noteskins/dance/boundary/note (nearest grayscale).PNG";
        for base in [&dirs.data_dir, &dirs.exe_dir] {
            let path = base.join("assets").join(key);
            fs::create_dir_all(path.parent().expect("fixture parent"))
                .expect("create noteskin fixture");
            fs::write(path, [0u8]).expect("write noteskin fixture");
            fs::write(base.join("assets/noteskins/ignored.txt"), [0u8])
                .expect("write non-image fixture");
        }
        let jobs = initial_texture_jobs(
            [
                deadlib_assets::texture_asset("boundary (nearest).png"),
                deadlib_assets::texture_asset(key),
            ],
            &dirs.asset_paths(None),
            |key| key == "boundary (nearest).png",
        );
        let skin_jobs = jobs.iter().filter(|job| job.key == key).collect::<Vec<_>>();
        assert_eq!(skin_jobs.len(), 1);
        assert_eq!(skin_jobs[0].path, dirs.data_dir.join("assets").join(key));
        assert_eq!(skin_jobs[0].sampler.filter, SamplerFilter::Nearest);
        assert!(skin_jobs[0].hints.grayscale);
        assert_eq!(skin_jobs[0].hints.min_size, Some(8));
        assert_eq!(skin_jobs[0].hints.max_size, Some(2048));
        assert!(!jobs.iter().any(|job| job.key.ends_with("ignored.txt")));
        let manifest_job = &jobs[0];
        assert_eq!(manifest_job.key, "boundary (nearest).png");
        assert_eq!(manifest_job.sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(manifest_job.sampler.filter, SamplerFilter::Linear);
        assert_eq!(
            manifest_job.path,
            dirs.asset_paths(None)
                .resolve_asset_path("assets/graphics/boundary (nearest).png")
        );
        fs::remove_dir_all(root).expect("remove noteskin fixture");
    }
    #[test]
    fn model_sampler_forces_repeat_for_plain_textures() {
        let key = "noteskins/dance/custom/textures/Tap Note parts.png";
        let sampler = model_texture_sampler(key);

        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(sampler.filter, SamplerFilter::Linear);
    }

    #[test]
    fn model_sampler_preserves_texture_hints() {
        let key = "noteskins/dance/custom/textures/Tap Note parts (nearest mipmaps).png";
        let sampler = model_texture_sampler(key);

        assert_eq!(sampler.wrap, SamplerWrap::Repeat);
        assert_eq!(sampler.filter, SamplerFilter::Nearest);
        assert!(sampler.mipmaps);
    }

    fn choice(key: &str) -> TextureChoice {
        TextureChoice::new(key.to_owned(), String::new())
    }

    #[test]
    fn resolves_requested_texture_choice_case_insensitively() {
        let choices = [choice("Love"), choice("Metal")];

        assert_eq!(
            resolve_texture_choice_entry(Some("metal"), &choices),
            Some(&choices[1])
        );
    }

    #[test]
    fn falls_back_to_first_non_none_texture_choice() {
        let choices = [choice(NONE_TEXTURE_CHOICE_KEY), choice("Love")];

        assert_eq!(
            resolve_texture_choice_key(Some("missing"), &choices),
            Some("Love")
        );
    }

    #[test]
    fn explicit_none_request_keeps_none_choice() {
        let choices = [choice(NONE_TEXTURE_CHOICE_KEY), choice("Love")];

        assert_eq!(
            resolve_texture_choice_key(Some(NONE_TEXTURE_CHOICE_KEY), &choices),
            Some(NONE_TEXTURE_CHOICE_KEY)
        );
    }

    #[test]
    fn missing_request_resolves_to_no_choice() {
        let choices = [choice("Love")];

        assert_eq!(resolve_texture_choice_key(None, &choices), None);
    }

    #[test]
    fn texture_choices_from_discovered_appends_none_choice() {
        let choices = texture_choices_from_discovered(
            [DiscoveredTexture {
                key: "judgements/Love 2x6.png".to_string(),
                label: "Love".to_string(),
                source_path: PathBuf::from("assets/graphics/judgements/Love 2x6.png"),
            }],
            true,
        );

        assert_eq!(
            choices,
            [
                TextureChoice::new("judgements/Love 2x6.png".to_string(), "Love".to_string()),
                TextureChoice::new(
                    NONE_TEXTURE_CHOICE_KEY.to_string(),
                    NONE_TEXTURE_CHOICE_KEY.to_string()
                ),
            ]
        );
    }

    #[test]
    fn texture_choice_exposes_key_for_resolution() {
        let choice = TextureChoice::new("key.png".to_string(), "Key".to_string());

        assert_eq!(choice.key.as_ref(), "key.png");
    }

    #[test]
    fn graphic_texture_choice_cache_discovers_judgments() {
        let root = std::env::temp_dir().join(format!(
            "deadsync-graphic-choice-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("Metal 2x7.png"), [0u8]).unwrap();
        std::fs::write(root.join("Love 2x7.png"), [0u8]).unwrap();

        let cache = GraphicTextureChoiceCache::new();
        let choices = cache.judgment_texture_choices(|folder| {
            if folder == "judgements" {
                vec![root.clone()]
            } else {
                Vec::new()
            }
        });

        assert_eq!(choices[0].key.as_ref(), "judgements/Love 2x7.png");
        assert_eq!(choices[1].key.as_ref(), "judgements/Metal 2x7.png");
        assert_eq!(choices[2].key.as_ref(), NONE_TEXTURE_CHOICE_KEY);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn initial_texture_source_path_maps_relative_asset_roots() {
        let resolved =
            initial_texture_source_path("judgements/Love 2x6.png", |path| PathBuf::from(path));
        assert_eq!(
            resolved,
            PathBuf::from("assets/graphics/judgements/Love 2x6.png")
        );

        let resolved =
            initial_texture_source_path("noteskins/dance/foo.png", |path| PathBuf::from(path));
        assert_eq!(resolved, PathBuf::from("assets/noteskins/dance/foo.png"));
    }

    #[test]
    fn initial_texture_source_path_keeps_absolute_paths() {
        let path = if cfg!(windows) {
            PathBuf::from("C:/tmp/texture.png")
        } else {
            PathBuf::from("/tmp/texture.png")
        };
        let resolved =
            initial_texture_source_path(&path.to_string_lossy(), |path| PathBuf::from(path));
        assert_eq!(resolved, path);
    }

    #[test]
    fn texture_key_source_path_prefers_assets_root_when_present() {
        let dir =
            std::env::temp_dir().join(format!("deadsync-texture-source-{}", std::process::id()));
        let asset_path = dir.join("assets").join("foo.png");
        std::fs::create_dir_all(asset_path.parent().unwrap()).expect("create fixture dir");
        std::fs::write(&asset_path, [0u8]).expect("write fixture");

        let resolved = texture_key_source_path("foo.png", "foo.png", |path| dir.join(path));

        assert_eq!(resolved, asset_path);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn texture_key_source_path_falls_back_to_graphics_root() {
        let resolved = texture_key_source_path("foo.png", "foo.png", |path| PathBuf::from(path));

        assert_eq!(resolved, PathBuf::from("assets/graphics/foo.png"));
    }
}
