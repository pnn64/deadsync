use crate::assets::AssetManager;
use crate::engine::gfx::{Backend, SamplerDesc, SamplerFilter, SamplerWrap};
use image::{ImageFormat, ImageReader, RgbaImage};
use log::{debug, warn};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, Mutex, RwLock, mpsc},
};

use super::AssetError;

#[derive(Clone, Copy, Debug)]
pub struct TexMeta {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TextureHints {
    pub raw: String,
    pub mipmaps: Option<bool>,
    pub grayscale: bool,
    pub alphamap: bool,
    pub doubleres: bool,
    pub stretch: bool,
    pub dither: bool,
    pub color_depth: Option<u32>,
    pub sampler_filter: Option<SamplerFilter>,
    pub sampler_wrap: Option<SamplerWrap>,
}

impl TextureHints {
    #[inline(always)]
    pub fn is_default(&self) -> bool {
        self.raw.is_empty() || self.raw.eq_ignore_ascii_case("default")
    }

    #[inline(always)]
    pub fn sampler_desc(&self) -> SamplerDesc {
        SamplerDesc {
            filter: self.sampler_filter.unwrap_or(SamplerFilter::Linear),
            wrap: self.sampler_wrap.unwrap_or(SamplerWrap::Clamp),
            mipmaps: self.mipmaps.unwrap_or(false),
        }
    }
}

#[inline(always)]
fn has_ascii_case_insensitive_substr(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[inline(always)]
fn parse_ascii_digits(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(b - b'0'))?;
    }
    Some(value)
}

#[inline(always)]
fn is_res_tag(bytes: &[u8], idx: usize) -> bool {
    idx + 4 <= bytes.len()
        && bytes[idx] == b'('
        && bytes[idx + 1].eq_ignore_ascii_case(&b'r')
        && bytes[idx + 2].eq_ignore_ascii_case(&b'e')
        && bytes[idx + 3].eq_ignore_ascii_case(&b's')
}

#[inline(always)]
fn skip_parenthetical(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0usize;
    let mut idx = start;
    while idx < bytes.len() {
        match bytes[idx] {
            b'(' => depth += 1,
            b')' => {
                if depth == 0 {
                    return idx + 1;
                }
                depth -= 1;
                if depth == 0 {
                    return idx + 1;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    bytes.len()
}

#[inline(always)]
fn trim_ascii_ws(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .map_or(start, |idx| idx + 1);
    &bytes[start..end]
}

fn parse_res_dims(section: &[u8]) -> Option<(u32, u32)> {
    let mut scan = 0usize;
    while scan + 4 <= section.len() {
        if !section[scan..scan + 4].eq_ignore_ascii_case(b"res ") {
            scan += 1;
            continue;
        }

        let mut width_start = scan + 4;
        while width_start < section.len() && section[width_start].is_ascii_whitespace() {
            width_start += 1;
        }

        let Some(x_rel) = section[width_start..]
            .iter()
            .position(|b| matches!(*b, b'x' | b'X'))
        else {
            break;
        };
        let x_idx = width_start + x_rel;
        let width = parse_ascii_digits(trim_ascii_ws(&section[width_start..x_idx]));

        let mut height_end = x_idx + 1;
        while height_end < section.len() && section[height_end].is_ascii_digit() {
            height_end += 1;
        }
        let height = parse_ascii_digits(&section[x_idx + 1..height_end]);

        if let (Some(width), Some(height)) = (width, height)
            && width > 0
            && height > 0
        {
            return Some((width, height));
        }

        scan = height_end.max(width_start + 1);
    }
    None
}

pub(crate) fn parse_texture_resolution_hint(raw: &str) -> Option<(u32, u32)> {
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'(' {
            i += 1;
            continue;
        }
        let end = skip_parenthetical(bytes, i);
        if end <= i + 2 {
            i = end.max(i + 1);
            continue;
        }
        if let Some(dims) = parse_res_dims(&bytes[i + 1..end - 1]) {
            return Some(dims);
        }
        i = end.max(i + 1);
    }
    None
}

pub fn texture_source_dims_from_real(texture_key: &str, real_w: u32, real_h: u32) -> (u32, u32) {
    let (mut source_w, mut source_h) =
        parse_texture_resolution_hint(texture_key).unwrap_or((real_w, real_h));
    if parse_texture_hints(texture_key).doubleres {
        source_w /= 2;
        source_h /= 2;
    }
    (source_w, source_h)
}

pub fn texture_source_frame_dims_from_real(
    texture_key: &str,
    real_w: u32,
    real_h: u32,
) -> (u32, u32) {
    let (source_w, source_h) = texture_source_dims_from_real(texture_key, real_w, real_h);
    let (frames_wide, frames_high) = parse_sprite_sheet_dims(texture_key);
    (source_w / frames_wide.max(1), source_h / frames_high.max(1))
}

pub(crate) fn ascii_ci_hash(input: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037u64;
    for &b in input.as_bytes() {
        hash ^= u64::from(b.to_ascii_lowercase());
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

pub fn parse_texture_hints(raw: &str) -> TextureHints {
    let mut hints = TextureHints::default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return hints;
    }
    hints.raw = trimmed.to_string();
    if trimmed.eq_ignore_ascii_case("default") {
        return hints;
    }

    let has = |sub: &[u8]| has_ascii_case_insensitive_substr(trimmed.as_bytes(), sub);

    if has(b"32bpp") {
        hints.color_depth = Some(32);
    } else if has(b"16bpp") {
        hints.color_depth = Some(16);
    }
    if has(b"dither") {
        hints.dither = true;
    }
    if has(b"stretch") {
        hints.stretch = true;
    }
    if has(b"mipmaps") {
        hints.mipmaps = Some(true);
    }
    if has(b"nomipmaps") {
        hints.mipmaps = Some(false);
    }
    if has(b"grayscale") {
        hints.grayscale = true;
    }
    if has(b"alphamap") {
        hints.alphamap = true;
    }
    if has(b"doubleres") {
        hints.doubleres = true;
    }
    if has(b"nearest") || has(b"point") {
        hints.sampler_filter = Some(SamplerFilter::Nearest);
    }
    if has(b"linear") {
        hints.sampler_filter = Some(SamplerFilter::Linear);
    }
    if has(b"wrap") || has(b"repeat") {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }
    if has(b"clamp") {
        hints.sampler_wrap = Some(SamplerWrap::Clamp);
    }
    if hints.mipmaps == Some(true) && hints.sampler_wrap.is_none() {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }

    hints
}

static TEX_META: std::sync::LazyLock<RwLock<HashMap<String, TexMeta>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static SHEET_DIMS: std::sync::LazyLock<RwLock<HashMap<String, (u32, u32)>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
pub(crate) struct GeneratedTexture {
    pub image: Arc<RgbaImage>,
    pub sampler: SamplerDesc,
}

static GENERATED_TEXTURES: std::sync::LazyLock<RwLock<HashMap<String, GeneratedTexture>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));
static GENERATED_TEXTURES_PENDING: std::sync::LazyLock<Mutex<HashSet<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn register_texture_dims(key: &str, w: u32, h: u32) {
    let sheet = parse_sprite_sheet_dims(key);
    let key = key.to_string();
    let mut m = TEX_META.write().unwrap();
    m.insert(key.clone(), TexMeta { w, h });
    drop(m);
    SHEET_DIMS.write().unwrap().insert(key, sheet);
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
    let rel = p.strip_prefix(Path::new("assets")).unwrap_or(p);
    rel.to_string_lossy().replace('\\', "/")
}

fn open_image_fallback_mode(
    path: &Path,
    warn_mismatch: bool,
) -> image::ImageResult<image::DynamicImage> {
    let hint = ImageFormat::from_path(path).ok();
    if let Some(fmt) = hint {
        let mut reader = ImageReader::open(path).map_err(image::ImageError::IoError)?;
        reader.set_format(fmt);
        if let Ok(img) = reader.decode() {
            return Ok(img);
        }
    }

    let guessed = ImageReader::open(path)
        .map_err(image::ImageError::IoError)?
        .with_guessed_format()?;
    let guessed_fmt = guessed.format();
    if let (Some(hint_fmt), Some(real_fmt)) = (hint, guessed_fmt)
        && hint_fmt != real_fmt
        && warn_mismatch
    {
        warn!(
            "Graphic file '{}' is really {:?}",
            path.to_string_lossy(),
            real_fmt
        );
    }
    guessed.decode()
}

pub fn open_image_fallback(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, true)
}

pub(crate) fn open_image_fallback_quiet(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, false)
}

pub(crate) fn append_noteskins_pngs_recursive(list: &mut Vec<(String, String)>, folder: &str) {
    let mut dirs = vec![Path::new("assets").join(folder)];
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
            if key.starts_with("noteskins/") {
                list.push((key.clone(), key));
            }
        }
    }
}

pub fn parse_sprite_sheet_dims(filename: &str) -> (u32, u32) {
    let bytes = filename.as_bytes();
    let mut dims: Option<(u32, u32)> = None;
    let mut i = 0usize;

    while i < bytes.len() {
        if is_res_tag(bytes, i) {
            i = skip_parenthetical(bytes, i);
            continue;
        }

        let b = bytes[i];
        if (b == b'x' || b == b'X') && i > 0 && bytes[i - 1].is_ascii_digit() {
            let mut left = i;
            while left > 0 && bytes[left - 1].is_ascii_digit() {
                left -= 1;
            }

            let mut right = i + 1;
            while right < bytes.len() && bytes[right].is_ascii_digit() {
                right += 1;
            }

            if left < i
                && i + 1 < right
                && let (Some(w), Some(h)) = (
                    parse_ascii_digits(&bytes[left..i]),
                    parse_ascii_digits(&bytes[i + 1..right]),
                )
                && w > 0
                && h > 0
            {
                dims = Some((w, h));
            }

            i = right;
            continue;
        }

        i += 1;
    }

    dims.unwrap_or((1, 1))
}

pub(crate) fn apply_texture_hints(image: &mut RgbaImage, hints: &TextureHints) {
    if !(hints.grayscale || hints.alphamap) {
        return;
    }

    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let lum = ((u16::from(r) * 30 + u16::from(g) * 59 + u16::from(b) * 11) / 100) as u8;
        if hints.alphamap {
            pixel.0 = [255, 255, 255, lum];
        } else {
            pixel.0 = [lum, lum, lum, a];
        }
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
        self.insert_texture("__white".to_string(), white_tex);
        register_texture_dims("__white", 1, 1);
        debug!("Loaded built-in texture: __white");

        let black_img = RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255]).unwrap();
        let black_tex = backend.create_texture(&black_img, SamplerDesc::default())?;
        self.insert_texture("__black".to_string(), black_tex);
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
            ("heart.png".to_string(), "heart.png".to_string()),
            ("GrooveStats.png".to_string(), "GrooveStats.png".to_string()),
            ("arrowcloud.png".to_string(), "arrowcloud.png".to_string()),
            ("ITL.png".to_string(), "ITL.png".to_string()),
            ("crown.png".to_string(), "crown.png".to_string()),
            (
                "srpg9_logo_alt.png".to_string(),
                "srpg9_logo_alt.png".to_string(),
            ),
            (
                "combo_explosion.png".to_string(),
                "combo_explosion.png".to_string(),
            ),
            (
                "combo_100milestone_splode.png".to_string(),
                "combo_100milestone_splode.png".to_string(),
            ),
            (
                "combo_100milestone_minisplode.png".to_string(),
                "combo_100milestone_minisplode.png".to_string(),
            ),
            (
                "gameplayin_splode.png".to_string(),
                "gameplayin_splode.png".to_string(),
            ),
            (
                "gameplayin_minisplode.png".to_string(),
                "gameplayin_minisplode.png".to_string(),
            ),
            (
                "combo_1000milestone_swoosh.png".to_string(),
                "combo_1000milestone_swoosh.png".to_string(),
            ),
            (
                "titlemenu_flycenter.png".to_string(),
                "titlemenu_flycenter.png".to_string(),
            ),
            (
                "titlemenu_flytop.png".to_string(),
                "titlemenu_flytop.png".to_string(),
            ),
            (
                "titlemenu_flybottom.png".to_string(),
                "titlemenu_flybottom.png".to_string(),
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
                "judgements/Love 2x7 (doubleres).png".to_string(),
                "judgements/Love 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Love Chroma 2x7 (doubleres).png".to_string(),
                "judgements/Love Chroma 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Rainbowmatic 2x7 (doubleres).png".to_string(),
                "judgements/Rainbowmatic 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/GrooveNights 2x7 (doubleres).png".to_string(),
                "judgements/GrooveNights 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Emoticon 2x7 (doubleres).png".to_string(),
                "judgements/Emoticon 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Censored 1x7 (doubleres).png".to_string(),
                "judgements/Censored 1x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Chromatic 2x7 (doubleres).png".to_string(),
                "judgements/Chromatic 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/ITG2 2x7 (doubleres).png".to_string(),
                "judgements/ITG2 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Bebas 2x7 (doubleres).png".to_string(),
                "judgements/Bebas 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Code 2x7 (doubleres).png".to_string(),
                "judgements/Code 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Comic Sans 2x7 (doubleres).png".to_string(),
                "judgements/Comic Sans 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Focus 2x7 (doubleres).png".to_string(),
                "judgements/Focus 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Grammar 2x7 (doubleres).png".to_string(),
                "judgements/Grammar 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Miso 2x7 (doubleres).png".to_string(),
                "judgements/Miso 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Papyrus 2x7 (doubleres).png".to_string(),
                "judgements/Papyrus 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Roboto 2x7 (doubleres).png".to_string(),
                "judgements/Roboto 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Shift 2x7 (doubleres).png".to_string(),
                "judgements/Shift 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Tactics 2x7 (doubleres).png".to_string(),
                "judgements/Tactics 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Wendy 2x7 (doubleres).png".to_string(),
                "judgements/Wendy 2x7 (doubleres).png".to_string(),
            ),
            (
                "judgements/Wendy Chroma 2x7 (doubleres).png".to_string(),
                "judgements/Wendy Chroma 2x7 (doubleres).png".to_string(),
            ),
            (
                "hold_judgements/Love 1x2 (doubleres).png".to_string(),
                "hold_judgements/Love 1x2 (doubleres).png".to_string(),
            ),
            (
                "hold_judgements/mute 1x2 (doubleres).png".to_string(),
                "hold_judgements/mute 1x2 (doubleres).png".to_string(),
            ),
            (
                "hold_judgements/ITG2 1x2 (doubleres).png".to_string(),
                "hold_judgements/ITG2 1x2 (doubleres).png".to_string(),
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
        ];

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

        append_noteskins_pngs_recursive(&mut textures_to_load, "noteskins");

        #[inline(always)]
        fn decode_rgba(
            key: String,
            relative_path: String,
        ) -> Result<(String, RgbaImage), (String, String)> {
            let path = if relative_path.starts_with("noteskins/") {
                Path::new("assets").join(&relative_path)
            } else {
                Path::new("assets/graphics").join(&relative_path)
            };
            match open_image_fallback(&path) {
                Ok(img) => Ok((key, img.to_rgba8())),
                Err(e) => Err((key, e.to_string())),
            }
        }

        let job_count = textures_to_load.len();
        let worker_count = std::thread::available_parallelism()
            .map(|n| n.get())
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
                    let sampler = if key == "swoosh.png" {
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
                    self.insert_texture(key, texture);
                }
                Err((key, msg)) => {
                    warn!("Failed to load texture for key '{key}': {msg}. Using fallback.");
                    let sampler = if key == "swoosh.png" {
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
                    self.insert_texture(key, texture);
                }
            }
        }

        for w in workers {
            w.join().expect("texture decode worker panicked");
        }

        Ok(())
    }

    pub(crate) fn ensure_texture_for_key(&mut self, backend: &mut Backend, texture_key: &str) {
        if texture_key.is_empty() {
            return;
        }
        let key = canonical_texture_key(texture_key);
        if self.has_texture_key(&key) {
            return;
        }
        if let Some(generated) = generated_texture(&key) {
            match backend.create_texture(generated.image.as_ref(), generated.sampler) {
                Ok(texture) => {
                    self.insert_texture(key, texture);
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

        let mut path = Path::new("assets").join(&key);
        if !path.is_file() {
            path = Path::new("assets/graphics").join(&key);
        }
        if !path.is_file() {
            warn!("Failed to resolve texture key '{key}' for preload.");
            return;
        }

        let hints = parse_texture_hints(&key);
        match open_image_fallback(&path) {
            Ok(img) => {
                let mut rgba = img.to_rgba8();
                if !hints.is_default() {
                    apply_texture_hints(&mut rgba, &hints);
                }
                match backend.create_texture(&rgba, hints.sampler_desc()) {
                    Ok(texture) => {
                        self.insert_texture(key.clone(), texture);
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

    pub(crate) fn upload_pending_generated_textures(&mut self, backend: &mut Backend) {
        for key in take_pending_generated_texture_keys() {
            let Some(generated) = generated_texture(&key) else {
                continue;
            };
            match backend.create_texture(generated.image.as_ref(), generated.sampler) {
                Ok(texture) => {
                    if let Some(old) = self.insert_texture(key.clone(), texture) {
                        let mut old_map = HashMap::with_capacity(1);
                        let handle = self.texture_handle_for_key(&key);
                        old_map.insert(handle, old);
                        backend.dispose_textures(&mut old_map);
                    }
                }
                Err(e) => {
                    warn!("Failed to create GPU texture for generated key '{key}': {e}");
                }
            }
        }
    }
}
