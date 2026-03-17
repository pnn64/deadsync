use crate::core::gfx::{
    Backend, INVALID_TEXTURE_HANDLE, ObjectType, RenderList, SamplerDesc, SamplerFilter,
    SamplerWrap, Texture as GfxTexture, TextureHandle,
};
use crate::core::video;
use crate::game::profile;
use crate::ui::font::{self, Font, FontLoadData};
use image::{ImageFormat, ImageReader, RgbaImage};
use log::{debug, warn};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};
use twox_hash::XxHash64;

// --- Texture Metadata ---

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

fn parse_texture_resolution_hint(raw: &str) -> Option<(u32, u32)> {
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
        let section = raw.get(i + 1..end - 1)?.to_ascii_lowercase();
        let mut scan = 0usize;
        while let Some(rel) = section[scan..].find("res ") {
            let start = scan + rel + 4;
            let tail = section[start..].trim_start();
            let Some(x_pos) = tail.find('x') else {
                break;
            };
            let width_txt = tail[..x_pos].trim();
            if width_txt.is_empty() || !width_txt.bytes().all(|b| b.is_ascii_digit()) {
                scan = start + 1;
                continue;
            }
            let after_x = &tail[x_pos + 1..];
            let height_len = after_x.bytes().take_while(|b| b.is_ascii_digit()).count();
            if height_len == 0 {
                scan = start + x_pos + 1;
                continue;
            }
            let height_txt = &after_x[..height_len];
            let (Ok(width), Ok(height)) = (width_txt.parse::<u32>(), height_txt.parse::<u32>())
            else {
                scan = start + x_pos + 1 + height_len;
                continue;
            };
            if width > 0 && height > 0 {
                return Some((width, height));
            }
            scan = start + x_pos + 1 + height_len;
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

#[inline(always)]
fn ascii_ci_hash(input: &str) -> u64 {
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

    // Zero-allocation case-insensitive substring check
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
        // ITG noteskin "(mipmaps)" sheets are typically authored for scrolling/repeating UVs.
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }

    hints
}

impl TextureHints {
    #[inline(always)]
    pub fn sampler_desc(&self) -> SamplerDesc {
        SamplerDesc {
            filter: self.sampler_filter.unwrap_or(SamplerFilter::Linear),
            wrap: self.sampler_wrap.unwrap_or(SamplerWrap::Clamp),
            mipmaps: self.mipmaps.unwrap_or(false),
        }
    }
}

static TEX_META: std::sync::LazyLock<RwLock<HashMap<String, TexMeta>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

static SHEET_DIMS: std::sync::LazyLock<RwLock<HashMap<String, (u32, u32)>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
struct GeneratedTexture {
    image: Arc<RgbaImage>,
    sampler: SamplerDesc,
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

fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
}

fn take_pending_generated_texture_keys() -> Vec<String> {
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

pub(crate) fn open_image_fallback(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, true)
}

fn open_image_fallback_quiet(path: &Path) -> image::ImageResult<image::DynamicImage> {
    open_image_fallback_mode(path, false)
}

const BANNER_CACHE_DIR: &str = "cache/banner";
const CDTITLE_CACHE_DIR: &str = "cache/cdtitle";
static BANNER_CACHE_TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug)]
struct BannerCacheOptions {
    enabled: bool,
}

impl BannerCacheOptions {
    #[inline(always)]
    fn from_banner_config(cfg: &crate::config::Config) -> Self {
        Self {
            enabled: cfg.banner_cache,
        }
    }

    #[inline(always)]
    fn from_cdtitle_config(cfg: &crate::config::Config) -> Self {
        Self {
            enabled: cfg.cdtitle_cache,
        }
    }
}

fn banner_cache_opthash(opts: BannerCacheOptions) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write_u8(1); // cache format version
    hasher.write_u8(u8::from(opts.enabled));
    hasher.finish()
}

const BANNER_CACHE_MAGIC: [u8; 8] = *b"DSBNR02\0";
const BANNER_CACHE_HEADER_SIZE: usize = 16;

fn dynamic_image_cache_path_for(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> Option<(PathBuf, String)> {
    let canonical = path.canonicalize().ok()?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical.to_string_lossy().replace('\\', "/").as_bytes());
    let path_hash = hasher.finish();
    let path_hex = format!("{path_hash:016x}");
    let opt_hash = banner_cache_opthash(opts);
    let shard2 = &path_hex[..2];
    let stem = format!("{path_hex}-{opt_hash:016x}");
    let dir = Path::new(cache_dir).join(shard2);
    Some((dir.join(format!("{stem}.rgba")), path_hex))
}

fn source_newer_than_cache(src: &Path, cache: &Path) -> bool {
    let src_m = fs::metadata(src).ok().and_then(|m| m.modified().ok());
    let cache_m = fs::metadata(cache).ok().and_then(|m| m.modified().ok());
    match (src_m, cache_m) {
        (Some(src_m), Some(cache_m)) => src_m > cache_m,
        (Some(_), None) => true,
        _ => false,
    }
}

fn ensure_cache_parent(cache_path: &Path) -> bool {
    if let Some(parent) = cache_path.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        warn!(
            "Failed to create banner cache directory '{}': {e}",
            parent.display()
        );
        return false;
    }
    true
}

fn load_raw_cached_banner_image(cache_path: &Path) -> Option<RgbaImage> {
    let mut bytes = fs::read(cache_path).ok()?;
    if bytes.len() < BANNER_CACHE_HEADER_SIZE || bytes[..8] != BANNER_CACHE_MAGIC {
        return None;
    }
    let width = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let height = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let payload_len = usize::try_from(width.checked_mul(height)?.checked_mul(4)?).ok()?;
    if bytes.len() != BANNER_CACHE_HEADER_SIZE.saturating_add(payload_len) {
        return None;
    }
    let payload = bytes.split_off(BANNER_CACHE_HEADER_SIZE);
    RgbaImage::from_raw(width, height, payload)
}

fn save_raw_cached_banner_image(cache_path: &Path, rgba: &RgbaImage) -> bool {
    if !ensure_cache_parent(cache_path) {
        return false;
    }
    let raw = rgba.as_raw();
    let mut out = Vec::<u8>::with_capacity(BANNER_CACHE_HEADER_SIZE.saturating_add(raw.len()));
    out.extend_from_slice(&BANNER_CACHE_MAGIC);
    out.extend_from_slice(&rgba.width().to_le_bytes());
    out.extend_from_slice(&rgba.height().to_le_bytes());
    out.extend_from_slice(raw);

    let parent = cache_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = cache_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("banner.rgba");
    let tmp_seq = BANNER_CACHE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = parent.join(format!(".{file_name}.tmp-{}-{tmp_seq}", std::process::id()));

    if let Err(e) = fs::write(&tmp_path, out) {
        warn!(
            "Failed to save raw banner cache '{}': {e}",
            cache_path.to_string_lossy()
        );
        let _ = fs::remove_file(&tmp_path);
        return false;
    }

    if fs::rename(&tmp_path, cache_path).is_err() {
        let _ = fs::remove_file(cache_path);
        if let Err(e) = fs::rename(&tmp_path, cache_path) {
            warn!(
                "Failed to finalize raw banner cache '{}': {e}",
                cache_path.to_string_lossy()
            );
            let _ = fs::remove_file(&tmp_path);
            return false;
        }
    }

    true
}

fn load_cached_banner_image(cache_path: &Path, source_path: &Path) -> Option<RgbaImage> {
    if cache_path.is_file() && !source_newer_than_cache(source_path, cache_path) {
        if let Some(rgba) = load_raw_cached_banner_image(cache_path) {
            return Some(rgba);
        }
        let _ = fs::remove_file(cache_path);
        debug!(
            "Invalid raw banner cache '{}'; rebuilding.",
            cache_path.to_string_lossy()
        );
    }
    None
}

fn prune_stale_banner_cache_variants(cache_path: &Path, path_hex: &str) {
    let Some(parent) = cache_path.parent() else {
        return;
    };
    let Some(current_name) = cache_path.file_name().and_then(|n| n.to_str()) else {
        return;
    };

    let prefix = format!("{path_hex}-");
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == current_name || !name.starts_with(&prefix) {
            continue;
        }
        if !(name.ends_with(".rgba") || name.ends_with(".png")) {
            continue;
        }
        if let Err(e) = fs::remove_file(&path) {
            warn!(
                "Failed to remove stale banner cache variant '{}': {e}",
                path.display()
            );
        }
    }
}

fn save_cached_banner_image(cache_path: &Path, path_hex: &str, rgba: &RgbaImage) {
    if !save_raw_cached_banner_image(cache_path, rgba) {
        return;
    }
    prune_stale_banner_cache_variants(cache_path, path_hex);
}

fn load_or_build_cached_banner(
    path: &Path,
    opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    load_or_build_cached_dynamic_image(path, opts, BANNER_CACHE_DIR)
}

fn load_or_build_cached_cdtitle(
    path: &Path,
    opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    load_or_build_cached_dynamic_image(path, opts, CDTITLE_CACHE_DIR)
}

fn load_or_build_cached_dynamic_image(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> image::ImageResult<RgbaImage> {
    let Some((cache_path, path_hex)) = dynamic_image_cache_path_for(path, opts, cache_dir) else {
        return build_cached_banner_rgba(path, opts);
    };

    if let Some(rgba) = load_cached_banner_image(&cache_path, path) {
        prune_stale_banner_cache_variants(&cache_path, &path_hex);
        return Ok(rgba);
    }

    let rgba = build_cached_banner_rgba(path, opts)?;
    save_cached_banner_image(&cache_path, &path_hex, &rgba);
    Ok(rgba)
}

#[inline(always)]
fn is_cacheable_dynamic_image_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "tga"
            | "tif"
            | "tiff"
            | "mp4"
            | "avi"
            | "m4v"
            | "mov"
            | "webm"
            | "mkv"
            | "mpg"
            | "mpeg"
    )
}

#[inline(always)]
fn is_dynamic_video_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp4" | "avi" | "m4v" | "mov" | "webm" | "mkv" | "mpg" | "mpeg"
    )
}

fn ensure_cached_dynamic_image_on_disk(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> image::ImageResult<bool> {
    let Some((cache_path, path_hex)) = dynamic_image_cache_path_for(path, opts, cache_dir) else {
        return Ok(false);
    };
    if load_cached_banner_image(&cache_path, path).is_some() {
        prune_stale_banner_cache_variants(&cache_path, &path_hex);
        return Ok(false);
    }
    let rgba = build_cached_banner_rgba(path, opts)?;
    save_cached_banner_image(&cache_path, &path_hex, &rgba);
    Ok(true)
}

enum DynamicImagePrewarmOutcome {
    Built { path: PathBuf, millis: f64 },
    Reused { path: PathBuf, millis: f64 },
    SkippedNonFile { path: PathBuf },
    SkippedNonImage { path: PathBuf },
    Failed { path: PathBuf, msg: String },
}

#[derive(Clone)]
struct DynamicImagePrewarmJob {
    path: PathBuf,
    opts: BannerCacheOptions,
    cache_dir: &'static str,
    label: &'static str,
}

struct DynamicImagePrewarmResult {
    label: &'static str,
    outcome: DynamicImagePrewarmOutcome,
}

fn push_dynamic_image_prewarm_jobs(
    jobs: &mut Vec<DynamicImagePrewarmJob>,
    unique: &mut HashSet<String>,
    paths: &[PathBuf],
    opts: BannerCacheOptions,
    cache_dir: &'static str,
    label: &'static str,
) -> usize {
    if !opts.enabled {
        return 0;
    }
    let mut duplicate = 0usize;
    for path in paths {
        let dedupe_key = dynamic_image_prewarm_dedupe_key(path, opts, cache_dir);
        if unique.insert(dedupe_key) {
            jobs.push(DynamicImagePrewarmJob {
                path: path.clone(),
                opts,
                cache_dir,
                label,
            });
        } else {
            duplicate += 1;
        }
    }
    duplicate
}

#[inline(always)]
fn prewarm_one_dynamic_image(
    path: PathBuf,
    opts: BannerCacheOptions,
    cache_dir: &'static str,
) -> DynamicImagePrewarmOutcome {
    if !path.is_file() {
        return DynamicImagePrewarmOutcome::SkippedNonFile { path };
    }
    if !is_cacheable_dynamic_image_path(&path) {
        return DynamicImagePrewarmOutcome::SkippedNonImage { path };
    }

    let started = Instant::now();
    match ensure_cached_dynamic_image_on_disk(&path, opts, cache_dir) {
        Ok(true) => DynamicImagePrewarmOutcome::Built {
            path,
            millis: started.elapsed().as_secs_f64() * 1000.0,
        },
        Ok(false) => DynamicImagePrewarmOutcome::Reused {
            path,
            millis: started.elapsed().as_secs_f64() * 1000.0,
        },
        Err(e) => DynamicImagePrewarmOutcome::Failed {
            path,
            msg: e.to_string(),
        },
    }
}

#[inline(always)]
fn dynamic_image_prewarm_workers(job_count: usize) -> usize {
    if job_count == 0 {
        return 0;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(job_count)
}

#[inline(always)]
fn dynamic_image_prewarm_dedupe_key(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &'static str,
) -> String {
    dynamic_image_cache_path_for(path, opts, cache_dir).map_or_else(
        || path.to_string_lossy().replace('\\', "/"),
        |(cache_path, _)| cache_path.to_string_lossy().replace('\\', "/"),
    )
}

fn prewarm_dynamic_image_jobs_with_progress<F>(
    input_count: usize,
    jobs: Vec<DynamicImagePrewarmJob>,
    duplicate: usize,
    label: &'static str,
    progress: &mut F,
) where
    F: FnMut(usize, usize, Option<&Path>),
{
    let started = Instant::now();
    let worker_count = dynamic_image_prewarm_workers(jobs.len());
    let total_jobs = jobs.len();
    progress(0, total_jobs, None);
    debug!(
        "{} cache prewarm start: {} input, {} unique, {} duplicate, {} worker threads.",
        label, input_count, total_jobs, duplicate, worker_count
    );

    let (job_tx, job_rx) = mpsc::channel::<DynamicImagePrewarmJob>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (res_tx, res_rx) = mpsc::channel::<DynamicImagePrewarmResult>();
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
                let Ok(job) = job else {
                    return;
                };
                let outcome = prewarm_one_dynamic_image(job.path, job.opts, job.cache_dir);
                let _ = res_tx.send(DynamicImagePrewarmResult {
                    label: job.label,
                    outcome,
                });
            }
        }));
    }
    drop(res_tx);
    for job in jobs {
        let _ = job_tx.send(job);
    }
    drop(job_tx);

    let mut prepared = 0usize;
    let mut built = 0usize;
    let mut reused = 0usize;
    let mut skipped_non_file = 0usize;
    let mut skipped_non_image = 0usize;
    let mut failed = 0usize;
    let mut built_ms = 0.0f64;
    let mut reused_ms = 0.0f64;
    let mut completed = 0usize;
    for result in res_rx {
        let current_path = match &result.outcome {
            DynamicImagePrewarmOutcome::Built { path, .. }
            | DynamicImagePrewarmOutcome::Reused { path, .. }
            | DynamicImagePrewarmOutcome::SkippedNonFile { path }
            | DynamicImagePrewarmOutcome::SkippedNonImage { path }
            | DynamicImagePrewarmOutcome::Failed { path, .. } => Some(path.as_path()),
        };
        completed = completed.saturating_add(1);
        progress(completed, total_jobs, current_path);
        match result.outcome {
            DynamicImagePrewarmOutcome::Built { millis, .. } => {
                prepared += 1;
                built += 1;
                built_ms += millis;
            }
            DynamicImagePrewarmOutcome::Reused { millis, .. } => {
                prepared += 1;
                reused += 1;
                reused_ms += millis;
            }
            DynamicImagePrewarmOutcome::SkippedNonFile { .. } => {
                skipped_non_file += 1;
            }
            DynamicImagePrewarmOutcome::SkippedNonImage { .. } => {
                skipped_non_image += 1;
            }
            DynamicImagePrewarmOutcome::Failed { path, msg } => {
                failed += 1;
                warn!(
                    "{} cache prewarm failed for '{}': {}",
                    result.label,
                    path.display(),
                    msg
                );
            }
        }
    }

    for worker in workers {
        let _ = worker.join();
    }

    let elapsed = started.elapsed().as_secs_f64();
    let prep_per_sec = if elapsed > 0.0 {
        prepared as f64 / elapsed
    } else {
        0.0
    };
    let built_avg_ms = if built > 0 {
        built_ms / built as f64
    } else {
        0.0
    };
    let reused_avg_ms = if reused > 0 {
        reused_ms / reused as f64
    } else {
        0.0
    };
    debug!(
        "{} cache prewarm complete in {:.2}s: prepared={} (built={}, reused={}), \
         skipped={} (non-file={}, non-image={}, duplicate={}), failed={}, workers={}, \
         throughput={:.1}/s, avg_ms={{built:{:.2}, reused:{:.2}}}.",
        label,
        elapsed,
        prepared,
        built,
        reused,
        skipped_non_file + skipped_non_image + duplicate,
        skipped_non_file,
        skipped_non_image,
        duplicate,
        failed,
        worker_count,
        prep_per_sec,
        built_avg_ms,
        reused_avg_ms
    );
}

pub fn artwork_cache_jobs(banner_paths: &[PathBuf], cdtitle_paths: &[PathBuf]) -> usize {
    let cfg = crate::config::get();
    let banner_opts = BannerCacheOptions::from_banner_config(&cfg);
    let cdtitle_opts = BannerCacheOptions::from_cdtitle_config(&cfg);
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique = HashSet::<String>::with_capacity(total_paths);
    if banner_opts.enabled {
        for path in banner_paths {
            unique.insert(dynamic_image_prewarm_dedupe_key(
                path,
                banner_opts,
                BANNER_CACHE_DIR,
            ));
        }
    }
    if cdtitle_opts.enabled {
        for path in cdtitle_paths {
            unique.insert(dynamic_image_prewarm_dedupe_key(
                path,
                cdtitle_opts,
                CDTITLE_CACHE_DIR,
            ));
        }
    }
    unique.len()
}

pub fn prewarm_artwork_cache_with_progress<F>(
    banner_paths: &[PathBuf],
    cdtitle_paths: &[PathBuf],
    progress: &mut F,
) where
    F: FnMut(usize, usize, Option<&Path>),
{
    let cfg = crate::config::get();
    let banner_opts = BannerCacheOptions::from_banner_config(&cfg);
    let cdtitle_opts = BannerCacheOptions::from_cdtitle_config(&cfg);
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique = HashSet::<String>::with_capacity(total_paths);
    let mut jobs = Vec::<DynamicImagePrewarmJob>::with_capacity(total_paths);
    let mut duplicate = 0usize;
    duplicate = duplicate.saturating_add(push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        banner_paths,
        banner_opts,
        BANNER_CACHE_DIR,
        "Banner",
    ));
    duplicate = duplicate.saturating_add(push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        cdtitle_paths,
        cdtitle_opts,
        CDTITLE_CACHE_DIR,
        "CDTitle",
    ));
    prewarm_dynamic_image_jobs_with_progress(total_paths, jobs, duplicate, "Artwork", progress);
}

fn build_cached_banner_rgba(
    path: &Path,
    _opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    if is_dynamic_video_path(path) {
        return video::load_poster(path)
            .map_err(|e| image::ImageError::IoError(std::io::Error::other(e)));
    }
    Ok(open_image_fallback_quiet(path)?.to_rgba8())
}

fn load_banner_source_rgba(path: &Path, opts: BannerCacheOptions) -> Result<RgbaImage, String> {
    if opts.enabled && is_cacheable_dynamic_image_path(path) {
        return load_or_build_cached_banner(path, opts).map_err(|e| e.to_string());
    }
    if is_dynamic_video_path(path) {
        return video::load_poster(path);
    }
    open_image_fallback(path)
        .map(|img| img.to_rgba8())
        .map_err(|e| e.to_string())
}

fn append_noteskins_pngs_recursive(list: &mut Vec<(String, String)>, folder: &str) {
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

#[inline(always)]
fn apply_texture_hints(image: &mut RgbaImage, hints: &TextureHints) {
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

// --- Asset Manager ---

struct DynamicVideoState {
    player: video::Player,
    started_at: Instant,
}

struct DynamicBannerState {
    key: String,
    path: PathBuf,
    high_res_loaded: bool,
}

struct DynamicBackgroundState {
    key: String,
    path: PathBuf,
    video: Option<video::Player>,
}

pub struct AssetManager {
    textures: HashMap<TextureHandle, GfxTexture>,
    texture_handles: HashMap<String, TextureHandle>,
    texture_handles_ascii_ci: HashMap<u64, TextureHandle>,
    next_texture_handle: TextureHandle,
    fonts: HashMap<&'static str, Font>,
    current_dynamic_banner: Option<DynamicBannerState>,
    active_banner_videos: HashMap<String, DynamicVideoState>,
    current_dynamic_cdtitle: Option<(String, PathBuf)>,
    current_dynamic_pack_banner: Option<(String, PathBuf)>,
    dynamic_pack_banner_keys: HashSet<String>,
    current_dynamic_background: Option<DynamicBackgroundState>,
    current_profile_avatars: [Option<(String, PathBuf)>; 2],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}

#[derive(Debug, Clone)]
pub struct DensityGraphSource {
    pub max_nps: f64,
    pub measure_nps_vec: Vec<f64>,
    pub timing: crate::game::timing::TimingData,
    pub first_second: f32,
    pub last_second: f32,
}

impl AssetManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            texture_handles: HashMap::new(),
            texture_handles_ascii_ci: HashMap::new(),
            next_texture_handle: 1,
            fonts: HashMap::new(),
            current_dynamic_banner: None,
            active_banner_videos: HashMap::new(),
            current_dynamic_cdtitle: None,
            current_dynamic_pack_banner: None,
            dynamic_pack_banner_keys: HashSet::new(),
            current_dynamic_background: None,
            current_profile_avatars: std::array::from_fn(|_| None),
        }
    }

    // --- Font Management ---

    pub fn register_font(&mut self, name: &'static str, font: Font) {
        self.fonts.insert(name, font);
    }

    pub const fn fonts(&self) -> &HashMap<&'static str, Font> {
        &self.fonts
    }

    #[inline(always)]
    pub fn textures(&self) -> &HashMap<TextureHandle, GfxTexture> {
        &self.textures
    }

    #[inline(always)]
    pub fn has_texture_key(&self, key: &str) -> bool {
        self.texture_handles.contains_key(key)
    }

    pub fn take_textures(&mut self) -> HashMap<TextureHandle, GfxTexture> {
        self.texture_handles.clear();
        self.texture_handles_ascii_ci.clear();
        std::mem::take(&mut self.textures)
    }

    #[inline(always)]
    pub fn texture_handle_for_key(&self, key: &str) -> TextureHandle {
        if let Some(handle) = self.texture_handles.get(key) {
            return *handle;
        }
        if let Some(handle) = self.texture_handles_ascii_ci.get(&ascii_ci_hash(key))
            && *handle != INVALID_TEXTURE_HANDLE
        {
            return *handle;
        }
        self.texture_handles
            .iter()
            .find_map(|(candidate, handle)| candidate.eq_ignore_ascii_case(key).then_some(*handle))
            .unwrap_or(INVALID_TEXTURE_HANDLE)
    }

    pub fn resolve_render_textures(&self, render: &mut RenderList<'_>) {
        #[inline(always)]
        fn texture_key<'a>(obj: &'a crate::core::gfx::RenderObject<'a>) -> Option<&'a str> {
            match &obj.object_type {
                ObjectType::Sprite { texture_id, .. }
                | ObjectType::TexturedMesh { texture_id, .. } => Some(texture_id.as_ref()),
                ObjectType::Mesh { .. } => None,
            }
        }

        let objects = &mut render.objects;
        let mut last_handle = INVALID_TEXTURE_HANDLE;
        for idx in 0..objects.len() {
            let handle = match texture_key(&objects[idx]) {
                Some(key) if idx > 0 && texture_key(&objects[idx - 1]) == Some(key) => last_handle,
                Some(key) => self.texture_handle_for_key(key),
                None => INVALID_TEXTURE_HANDLE,
            };
            objects[idx].texture_handle = handle;
            last_handle = handle;
        }
    }

    pub fn with_fonts<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HashMap<&'static str, Font>) -> R,
    {
        f(&self.fonts)
    }

    pub fn with_font<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Font) -> R,
    {
        self.fonts.get(name).map(f)
    }

    #[inline(always)]
    fn alloc_texture_handle(&mut self) -> TextureHandle {
        let handle = self.next_texture_handle;
        self.next_texture_handle = self.next_texture_handle.wrapping_add(1).max(1);
        handle
    }

    pub(crate) fn reserve_texture_handle(&mut self, key: String) -> TextureHandle {
        match self.texture_handles.get(&key).copied() {
            Some(handle) => handle,
            None => {
                let handle = self.alloc_texture_handle();
                self.texture_handles.insert(key.clone(), handle);
                self.note_texture_handle_alias(&key, handle);
                handle
            }
        }
    }

    pub(crate) fn insert_texture(
        &mut self,
        key: String,
        texture: GfxTexture,
    ) -> Option<GfxTexture> {
        let handle = self.reserve_texture_handle(key);
        self.textures.insert(handle, texture)
    }

    pub(crate) fn remove_texture(&mut self, key: &str) -> Option<(TextureHandle, GfxTexture)> {
        let handle = self.texture_handles.remove(key)?;
        self.rebuild_texture_handle_aliases();
        self.textures
            .remove(&handle)
            .map(|texture| (handle, texture))
    }

    fn dispose_texture(
        &mut self,
        backend: &mut Backend,
        handle: TextureHandle,
        texture: GfxTexture,
    ) {
        let mut textures = HashMap::with_capacity(1);
        textures.insert(handle, texture);
        backend.dispose_textures(&mut textures);
    }

    fn set_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: String,
        texture: GfxTexture,
    ) -> TextureHandle {
        let handle = self.reserve_texture_handle(key);
        if let Some(old) = self.textures.insert(handle, texture) {
            self.dispose_texture(backend, handle, old);
        }
        handle
    }

    fn update_texture_for_key(
        &mut self,
        backend: &mut Backend,
        key: &str,
        rgba: &RgbaImage,
    ) -> Result<(), Box<dyn Error>> {
        let dims = texture_dims(key);
        let handle = self.texture_handles.get(key).copied();
        if let (Some(meta), Some(handle)) = (dims, handle)
            && meta.w == rgba.width()
            && meta.h == rgba.height()
            && let Some(texture) = self.textures.get_mut(&handle)
        {
            backend.update_texture(texture, rgba)?;
            return Ok(());
        }

        let texture = backend.create_texture(rgba, SamplerDesc::default())?;
        self.set_texture_for_key(backend, key.to_string(), texture);
        register_texture_dims(key, rgba.width(), rgba.height());
        Ok(())
    }

    fn remove_texture_and_dispose(&mut self, backend: &mut Backend, key: &str) {
        if let Some((handle, texture)) = self.remove_texture(key) {
            self.dispose_texture(backend, handle, texture);
        }
    }

    fn note_texture_handle_alias(&mut self, key: &str, handle: TextureHandle) {
        let folded = ascii_ci_hash(key);
        match self.texture_handles_ascii_ci.get_mut(&folded) {
            Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
            Some(_) => {}
            None => {
                self.texture_handles_ascii_ci.insert(folded, handle);
            }
        }
    }

    fn rebuild_texture_handle_aliases(&mut self) {
        self.texture_handles_ascii_ci.clear();
        self.texture_handles_ascii_ci
            .reserve(self.texture_handles.len());
        for (key, &handle) in &self.texture_handles {
            let folded = ascii_ci_hash(key);
            match self.texture_handles_ascii_ci.get_mut(&folded) {
                Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
                Some(_) => {}
                None => {
                    self.texture_handles_ascii_ci.insert(folded, handle);
                }
            }
        }
    }

    // --- Loading Logic ---

    pub fn load_initial_assets(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        self.load_initial_textures(backend)?;
        self.load_initial_fonts(backend)?;
        Ok(())
    }

    fn load_initial_textures(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        debug!("Loading initial textures...");

        #[inline(always)]
        fn fallback_rgba() -> RgbaImage {
            let data: [u8; 16] = [
                255, 0, 255, 255, 128, 128, 128, 255, 128, 128, 128, 255, 255, 0, 255, 255,
            ];
            RgbaImage::from_raw(2, 2, data.to_vec()).expect("fallback image")
        }

        // Load __white texture
        let white_img = RgbaImage::from_raw(1, 1, vec![255, 255, 255, 255]).unwrap();
        let white_tex = backend.create_texture(&white_img, SamplerDesc::default())?;
        self.insert_texture("__white".to_string(), white_tex);
        register_texture_dims("__white", 1, 1);
        debug!("Loaded built-in texture: __white");

        // Load __black texture for missing/background-off fallbacks.
        let black_img = RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255]).unwrap();
        let black_tex = backend.create_texture(&black_img, SamplerDesc::default())?;
        self.insert_texture("__black".to_string(), black_tex);
        register_texture_dims("__black", 1, 1);
        debug!("Loaded built-in texture: __black");

        let mut textures_to_load: Vec<(String, String)> = vec![
            ("logo.png".to_string(), "logo.png".to_string()),
            ("init_arrow.png".to_string(), "init_arrow.png".to_string()),
            ("dance.png".to_string(), "dance.png".to_string()),
            // ScreenSelectPlayMode demo arrows (ported from Simply Love).
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
            // Test Input pad assets (Simply Love-style)
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

        // Simply Love-style grade assets (used by `screens::components::evaluation::eval_grades`).
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

        let profile = profile::get();
        // Preload all local profile avatars so ScreenSelectProfile can preview them.
        // These are user-generated assets; local profile counts are usually small.
        for p in profile::scan_local_profiles() {
            if let Some(path) = p.avatar_path {
                self.ensure_texture_from_path(backend, &path);
            }
        }
        self.set_profile_avatar(backend, profile.avatar_path);

        Ok(())
    }

    fn load_initial_fonts(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        for &name in &[
            "wendy",
            "miso",
            "cjk",
            "emoji",
            "game",
            "wendy_monospace_numbers",
            "wendy_screenevaluation",
            "wendy_combo",
            "combo_arial_rounded",
            "combo_asap",
            "combo_bebas_neue",
            "combo_source_code",
            "combo_work",
            "combo_wendy_cursed",
            "wendy_white",
        ] {
            let ini_path_str = match name {
                "wendy" => "assets/fonts/wendy/_wendy small.ini",
                "miso" => "assets/fonts/miso/_miso light.ini",
                "cjk" => "assets/fonts/cjk/_jfonts 16px.ini",
                "emoji" => "assets/fonts/emoji/_emoji 16px.ini",
                "game" => "assets/fonts/game/_game chars 16px.ini",
                "wendy_monospace_numbers" => "assets/fonts/wendy/_wendy monospace numbers.ini",
                "wendy_screenevaluation" => "assets/fonts/wendy/_ScreenEvaluation numbers.ini",
                "wendy_combo" => "assets/fonts/_combo/wendy/Wendy.ini",
                "combo_arial_rounded" => "assets/fonts/_combo/Arial Rounded/Arial Rounded.ini",
                "combo_asap" => "assets/fonts/_combo/Asap/Asap.ini",
                "combo_bebas_neue" => "assets/fonts/_combo/Bebas Neue/Bebas Neue.ini",
                "combo_source_code" => "assets/fonts/_combo/Source Code/Source Code.ini",
                "combo_work" => "assets/fonts/_combo/Work/Work.ini",
                "combo_wendy_cursed" => "assets/fonts/_combo/Wendy (Cursed)/Wendy (Cursed).ini",
                "wendy_white" => "assets/fonts/wendy/_wendy white.ini",
                _ => return Err(format!("Unknown font name: {name}").into()),
            };

            let FontLoadData {
                mut font,
                required_textures,
            } = font::parse(ini_path_str)?;

            if name == "miso" {
                font.fallback_font_name = Some("game");
                debug!("Font 'miso' configured to use 'game' as fallback.");
            }

            if name == "game" {
                font.fallback_font_name = Some("cjk");
                debug!("Font 'game' configured to use 'cjk' as fallback.");
            }

            if name == "cjk" {
                font.fallback_font_name = Some("emoji");
                debug!("Font 'cjk' configured to use 'emoji' as fallback.");
            }

            for tex_path in &required_textures {
                let key = canonical_texture_key(tex_path);
                if !self.has_texture_key(&key) {
                    let hints = font
                        .texture_hints_map
                        .get(&key)
                        .map(|s| parse_texture_hints(s))
                        .unwrap_or_default();
                    let mut image_data = open_image_fallback(tex_path)?.to_rgba8();
                    if !hints.is_default() {
                        apply_texture_hints(&mut image_data, &hints);
                    }
                    let texture = backend.create_texture(&image_data, hints.sampler_desc())?;
                    register_texture_dims(&key, image_data.width(), image_data.height());
                    self.insert_texture(key.clone(), texture);
                    debug!("Loaded font texture: {key}");
                }
            }
            self.register_font(name, font);
            debug!("Loaded font '{name}' from '{ini_path_str}'");
        }
        Ok(())
    }

    // --- Dynamic Asset Management ---

    pub fn destroy_dynamic_assets(&mut self, backend: &mut Backend) {
        if self.current_dynamic_banner.is_some()
            || !self.active_banner_videos.is_empty()
            || self.current_dynamic_cdtitle.is_some()
            || self.current_dynamic_pack_banner.is_some()
            || !self.dynamic_pack_banner_keys.is_empty()
            || self.current_dynamic_background.is_some()
        {
            if let Some(state) = self.current_dynamic_banner.take() {
                self.remove_texture_and_dispose(backend, &state.key);
            }
            self.active_banner_videos.clear();
            if let Some((key, _)) = self.current_dynamic_cdtitle.take() {
                self.remove_texture_and_dispose(backend, &key);
            }
            if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
                self.dynamic_pack_banner_keys.remove(&key);
                self.remove_texture_and_dispose(backend, &key);
            }
            let drained_pack_keys: Vec<_> = self.dynamic_pack_banner_keys.drain().collect();
            for key in drained_pack_keys {
                self.remove_texture_and_dispose(backend, &key);
            }
            if let Some(state) = self.current_dynamic_background.take() {
                self.remove_texture_and_dispose(backend, &state.key);
            }
        }
    }

    pub fn destroy_dynamic_banner(&mut self, backend: &mut Backend) {
        self.destroy_current_dynamic_banner(backend);
    }

    pub fn set_dynamic_cdtitle(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> Option<String> {
        let cache_opts = BannerCacheOptions::from_cdtitle_config(&crate::config::get());
        if let Some(path) = path_opt {
            if let Some((key, current_path)) = self.current_dynamic_cdtitle.as_ref()
                && current_path == &path
                && self.has_texture_key(key)
            {
                return Some(key.clone());
            }

            self.destroy_current_dynamic_cdtitle(backend);
            let rgba = if cache_opts.enabled {
                match load_or_build_cached_cdtitle(&path, cache_opts) {
                    Ok(cached) => cached,
                    Err(e) => {
                        warn!(
                            "Failed to load cached CDTitle '{}': {e}. Skipping.",
                            path.display()
                        );
                        return None;
                    }
                }
            } else {
                match open_image_fallback(&path) {
                    Ok(img) => img.to_rgba8(),
                    Err(e) => {
                        warn!("Failed to open CDTitle image {path:?}: {e}. Skipping.");
                        return None;
                    }
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    let path_key = path.to_string_lossy();
                    let key = format!("__cdtitle::{path_key}");
                    self.insert_texture(key.clone(), texture);
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_cdtitle = Some((key.clone(), path));
                    Some(key)
                }
                Err(e) => {
                    warn!(
                        "Failed to create GPU texture for CDTitle image {path:?}: {e}. Skipping."
                    );
                    None
                }
            }
        } else {
            self.destroy_current_dynamic_cdtitle(backend);
            None
        }
    }

    pub fn set_dynamic_pack_banner(&mut self, backend: &mut Backend, path_opt: Option<PathBuf>) {
        let banner_cache_opts = BannerCacheOptions::from_banner_config(&crate::config::get());
        if let Some(path) = path_opt {
            if self
                .current_dynamic_pack_banner
                .as_ref()
                .is_some_and(|(key, p)| p == &path && self.has_texture_key(key))
            {
                return;
            }

            let key = path.to_string_lossy().into_owned();
            if banner_cache_opts.enabled
                && self.dynamic_pack_banner_keys.contains(&key)
                && self.has_texture_key(&key)
            {
                self.current_dynamic_pack_banner = Some((key, path));
                return;
            }

            if banner_cache_opts.enabled {
                self.current_dynamic_pack_banner = None;
            } else {
                if let Some((old_key, _)) = self.current_dynamic_pack_banner.take() {
                    self.dynamic_pack_banner_keys.remove(&old_key);
                    self.remove_texture_and_dispose(backend, &old_key);
                }
            }

            let rgba = match load_banner_source_rgba(&path, banner_cache_opts) {
                Ok(rgba) => rgba,
                Err(e) => {
                    warn!(
                        "Failed to load pack banner '{}': {e}. Skipping.",
                        path.display()
                    );
                    return;
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    self.insert_texture(key.clone(), texture);
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    if banner_cache_opts.enabled {
                        self.dynamic_pack_banner_keys.insert(key.clone());
                    }
                    self.current_dynamic_pack_banner = Some((key, path));
                }
                Err(e) => {
                    warn!("Failed to create GPU texture for pack banner {path:?}: {e}. Skipping.");
                }
            }
        } else {
            if banner_cache_opts.enabled {
                self.current_dynamic_pack_banner = None;
            } else {
                if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
                    self.dynamic_pack_banner_keys.remove(&key);
                    self.remove_texture_and_dispose(backend, &key);
                }
            }
        }
    }

    pub fn set_dynamic_banner(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "banner1.png";
        let banner_cache_opts = BannerCacheOptions::from_banner_config(&crate::config::get());

        if let Some(path) = path_opt {
            let key = path.to_string_lossy().into_owned();
            if let Some(current) = self.current_dynamic_banner.as_ref()
                && current.path == path
                && self.has_texture_key(&current.key)
            {
                return current.key.clone();
            }
            self.destroy_current_dynamic_banner(backend);
            let rgba = match load_banner_source_rgba(&path, banner_cache_opts) {
                Ok(rgba) => rgba,
                Err(e) => {
                    warn!(
                        "Failed to load banner '{}': {e}. Using fallback.",
                        path.display()
                    );
                    return FALLBACK_KEY.to_string();
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    self.set_texture_for_key(backend, key.clone(), texture);
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_banner = Some(DynamicBannerState {
                        key: key.clone(),
                        path,
                        high_res_loaded: true,
                    });
                    key
                }
                Err(e) => {
                    warn!(
                        "Failed to create GPU texture for banner '{}': {e}. Using fallback.",
                        key
                    );
                    FALLBACK_KEY.to_string()
                }
            }
        } else {
            self.destroy_current_dynamic_banner(backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub fn sync_active_banner_videos(&mut self, backend: &mut Backend, desired_paths: &[PathBuf]) {
        let mut desired = HashSet::<String>::with_capacity(desired_paths.len());
        for path in desired_paths {
            if !is_dynamic_video_path(path) {
                continue;
            }
            desired.insert(path.to_string_lossy().into_owned());
        }
        self.active_banner_videos
            .retain(|key, _| desired.contains(key));
        for path in desired_paths {
            if !is_dynamic_video_path(path) {
                continue;
            }
            let key = path.to_string_lossy().into_owned();
            if self.active_banner_videos.contains_key(&key) {
                continue;
            }
            self.ensure_texture_from_path(backend, path);
            if !self.has_texture_key(&key) {
                continue;
            }
            match video::open_player(path, true) {
                Ok(player) => {
                    self.active_banner_videos.insert(
                        key,
                        DynamicVideoState {
                            player,
                            started_at: Instant::now(),
                        },
                    );
                }
                Err(e) => {
                    warn!("Failed to start banner video '{}': {e}", path.display());
                }
            }
        }
    }

    pub fn set_dynamic_background(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "__black";

        if let Some(path) = path_opt {
            let animate_video = crate::config::get().show_video_backgrounds;
            if self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|state| {
                    state.path == path
                        && self.has_texture_key(&state.key)
                        && (state.video.is_some()
                            == (animate_video && is_dynamic_video_path(&path)))
                })
            {
                return self
                    .current_dynamic_background
                    .as_ref()
                    .unwrap()
                    .key
                    .clone();
            }

            self.destroy_current_dynamic_background(backend);

            if is_dynamic_video_path(&path) {
                let key = path.to_string_lossy().into_owned();
                if animate_video {
                    match video::open(&path, true) {
                        Ok(video) => {
                            match backend.create_texture(&video.poster, SamplerDesc::default()) {
                                Ok(texture) => {
                                    self.set_texture_for_key(backend, key.clone(), texture);
                                    register_texture_dims(
                                        &key,
                                        video.info.width,
                                        video.info.height,
                                    );
                                    self.current_dynamic_background =
                                        Some(DynamicBackgroundState {
                                            key: key.clone(),
                                            path,
                                            video: Some(video.player),
                                        });
                                    return key;
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to create GPU texture for video background {path:?}: {e}. Using fallback."
                                    );
                                    return FALLBACK_KEY.to_string();
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to open video background '{}': {e}. Using fallback.",
                                path.display()
                            );
                            return FALLBACK_KEY.to_string();
                        }
                    }
                }
                match video::load_poster(&path) {
                    Ok(rgba) => match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            self.set_texture_for_key(backend, key.clone(), texture);
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_dynamic_background = Some(DynamicBackgroundState {
                                key: key.clone(),
                                path,
                                video: None,
                            });
                            return key;
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for video background poster {path:?}: {e}. Using fallback."
                            );
                            return FALLBACK_KEY.to_string();
                        }
                    },
                    Err(e) => {
                        warn!(
                            "Failed to load video background poster '{}': {e}. Using fallback.",
                            path.display()
                        );
                        return FALLBACK_KEY.to_string();
                    }
                }
            }

            let rgba = match open_image_fallback(&path) {
                Ok(img) => img.to_rgba8(),
                Err(e) => {
                    warn!("Failed to open background image {path:?}: {e}. Using fallback.");
                    return FALLBACK_KEY.to_string();
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    let key = path.to_string_lossy().into_owned();
                    self.set_texture_for_key(backend, key.clone(), texture);
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_background = Some(DynamicBackgroundState {
                        key: key.clone(),
                        path,
                        video: None,
                    });
                    key
                }
                Err(e) => {
                    warn!(
                        "Failed to create GPU texture for background {path:?}: {e}. Using fallback."
                    );
                    FALLBACK_KEY.to_string()
                }
            }
        } else {
            self.destroy_current_dynamic_background(backend);
            FALLBACK_KEY.to_string()
        }
    }

    pub fn set_profile_avatar(&mut self, backend: &mut Backend, path_opt: Option<PathBuf>) {
        let side = profile::get_session_player_side();
        self.set_profile_avatar_for_side(backend, side, path_opt);
    }

    pub fn set_profile_avatar_for_side(
        &mut self,
        backend: &mut Backend,
        side: profile::PlayerSide,
        path_opt: Option<PathBuf>,
    ) {
        let ix = match side {
            profile::PlayerSide::P1 => 0,
            profile::PlayerSide::P2 => 1,
        };

        if let Some(path) = path_opt {
            let key = path.to_string_lossy().into_owned();
            self.ensure_texture_from_path(backend, &path);
            self.current_profile_avatars[ix] = Some((key.clone(), path));
            if self.has_texture_key(&key) {
                profile::set_avatar_texture_key_for_side(side, Some(key));
            } else {
                profile::set_avatar_texture_key_for_side(side, None);
            }
        } else {
            self.destroy_current_profile_avatar_for_side(backend, side);
        }
    }

    pub fn update_dynamic_video_frames(
        &mut self,
        backend: &mut Backend,
        gameplay_time_sec: Option<f32>,
    ) {
        let banner_frames: Vec<_> = self
            .active_banner_videos
            .iter_mut()
            .filter_map(|(key, video)| {
                let play_time = video.started_at.elapsed().as_secs_f32();
                video
                    .player
                    .take_due_frame(play_time)
                    .map(|frame| (key.clone(), frame))
            })
            .collect();
        for (key, frame) in banner_frames {
            if let Err(e) = self.update_texture_for_key(backend, &key, &frame) {
                warn!("Failed to update dynamic video banner '{}': {e}", key);
            }
        }

        let background_frame = self.current_dynamic_background.as_mut().and_then(|state| {
            let video = state.video.as_mut()?;
            let play_time = gameplay_time_sec.unwrap_or(0.0).max(0.0);
            video
                .take_due_frame(play_time)
                .map(|frame| (state.key.clone(), frame))
        });
        if let Some((key, frame)) = background_frame
            && let Err(e) = self.update_texture_for_key(backend, &key, &frame)
        {
            warn!("Failed to update dynamic video background '{}': {e}", key);
        }
    }

    fn destroy_current_dynamic_banner(&mut self, backend: &mut Backend) {
        if let Some(state) = self.current_dynamic_banner.take() {
            self.active_banner_videos.remove(&state.key);
            self.remove_texture_and_dispose(backend, &state.key);
        }
    }

    fn destroy_current_dynamic_cdtitle(&mut self, backend: &mut Backend) {
        if let Some((key, _)) = self.current_dynamic_cdtitle.take() {
            self.remove_texture_and_dispose(backend, &key);
        }
    }

    fn destroy_current_dynamic_background(&mut self, backend: &mut Backend) {
        if let Some(state) = self.current_dynamic_background.take() {
            self.remove_texture_and_dispose(backend, &state.key);
        }
    }

    fn destroy_current_profile_avatar_for_side(
        &mut self,
        backend: &mut Backend,
        side: profile::PlayerSide,
    ) {
        let _ = backend;
        let ix = match side {
            profile::PlayerSide::P1 => 0,
            profile::PlayerSide::P2 => 1,
        };
        self.current_profile_avatars[ix] = None;
        profile::set_avatar_texture_key_for_side(side, None);
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

    pub(crate) fn ensure_texture_from_path(&mut self, backend: &mut Backend, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        let has_existing = self.has_texture_key(&key);
        let needs_high_res_upgrade = self
            .current_dynamic_banner
            .as_ref()
            .is_some_and(|state| state.key == key && state.path == path && !state.high_res_loaded);

        if has_existing && !needs_high_res_upgrade {
            return;
        }

        let banner_cache_opts = BannerCacheOptions::from_banner_config(&crate::config::get());
        let rgba = match load_banner_source_rgba(path, banner_cache_opts) {
            Ok(rgba) => rgba,
            Err(e) => {
                warn!("Failed to load banner source {path:?}: {e}. Skipping.");
                return;
            }
        };

        if let Err(e) = self.update_texture_for_key(backend, &key, &rgba) {
            warn!("Failed to create GPU texture for image {path:?}: {e}. Skipping.");
            return;
        }
        if needs_high_res_upgrade
            && let Some(state) = self.current_dynamic_banner.as_mut()
            && state.key == key
            && state.path == path
        {
            state.high_res_loaded = true;
        }
    }
}
