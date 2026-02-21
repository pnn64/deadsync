use crate::core::gfx::{Backend, SamplerDesc, SamplerFilter, SamplerWrap, Texture as GfxTexture};
use crate::game::profile;
use crate::ui::font::{self, Font, FontLoadData};
use image::{ImageFormat, ImageReader, RgbaImage};
use log::{info, warn};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock, mpsc},
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
    register_texture_dims(key, w, h);
}

fn generated_texture(key: &str) -> Option<GeneratedTexture> {
    GENERATED_TEXTURES.read().unwrap().get(key).cloned()
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

#[derive(Clone, Copy, Debug)]
struct BannerCacheOptions {
    enabled: bool,
    scale_divisor: u32,
    min_dimension: u32,
    round_pow2: bool,
    color_depth: u8,
}

impl BannerCacheOptions {
    #[inline(always)]
    fn from_config(cfg: &crate::config::Config) -> Self {
        Self {
            enabled: cfg.banner_cache,
            scale_divisor: u32::from(cfg.banner_cache_scale_divisor.max(1)),
            min_dimension: u32::from(cfg.banner_cache_min_dimension.max(1)),
            round_pow2: cfg.banner_cache_pow2,
            color_depth: cfg.banner_cache_color_depth,
        }
    }
}

#[inline(always)]
fn pow2_or_one(v: u32) -> u32 {
    if v <= 1 { 1 } else { v.next_power_of_two() }
}

#[inline(always)]
fn closest_pow2(value: u32) -> u32 {
    let hi = pow2_or_one(value);
    let lo = (hi / 2).max(1);
    let d_hi = hi.abs_diff(value);
    let d_lo = lo.abs_diff(value);
    if d_hi < d_lo { hi } else { lo }
}

fn compute_cached_banner_dims(src_w: u32, src_h: u32, opts: BannerCacheOptions) -> (u32, u32) {
    let mut w = (src_w / opts.scale_divisor).max(1);
    let mut h = (src_h / opts.scale_divisor).max(1);

    if opts.round_pow2 {
        w = closest_pow2(w);
        h = closest_pow2(h);
    }

    let w_floor = opts.min_dimension.min(pow2_or_one(src_w));
    let h_floor = opts.min_dimension.min(pow2_or_one(src_h));
    (w.max(w_floor), h.max(h_floor))
}

fn apply_banner_cache_color_depth(rgba: &mut RgbaImage, color_depth: u8) {
    if color_depth >= 32 {
        return;
    }
    if color_depth <= 8 {
        for px in rgba.pixels_mut() {
            let [r, g, b, a] = px.0;
            let r3 = ((u16::from(r) * 7 + 127) / 255) as u8;
            let g3 = ((u16::from(g) * 7 + 127) / 255) as u8;
            let b2 = ((u16::from(b) * 3 + 127) / 255) as u8;
            px.0 = [
                ((u16::from(r3) * 255) / 7) as u8,
                ((u16::from(g3) * 255) / 7) as u8,
                ((u16::from(b2) * 255) / 3) as u8,
                if a < 128 { 0 } else { 255 },
            ];
        }
        return;
    }

    for px in rgba.pixels_mut() {
        let [r, g, b, a] = px.0;
        let r5 = ((u16::from(r) * 31 + 127) / 255) as u8;
        let g5 = ((u16::from(g) * 31 + 127) / 255) as u8;
        let b5 = ((u16::from(b) * 31 + 127) / 255) as u8;
        px.0 = [
            ((u16::from(r5) * 255) / 31) as u8,
            ((u16::from(g5) * 255) / 31) as u8,
            ((u16::from(b5) * 255) / 31) as u8,
            if a < 128 { 0 } else { 255 },
        ];
    }
}

fn banner_cache_opthash(opts: BannerCacheOptions) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write_u8(1); // cache format version
    hasher.write_u8(opts.color_depth);
    hasher.write_u8(u8::from(opts.round_pow2));
    hasher.write_u32(opts.scale_divisor);
    hasher.write_u32(opts.min_dimension);
    hasher.finish()
}

fn banner_cache_path_for(path: &Path, opts: BannerCacheOptions) -> Option<(PathBuf, String)> {
    let canonical = path.canonicalize().ok()?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical.to_string_lossy().replace('\\', "/").as_bytes());
    let path_hash = hasher.finish();
    let path_hex = format!("{path_hash:016x}");
    let opt_hash = banner_cache_opthash(opts);
    let shard2 = &path_hex[..2];
    Some((
        Path::new(BANNER_CACHE_DIR)
            .join(shard2)
            .join(format!("{path_hex}-{opt_hash:016x}.png")),
        path_hex,
    ))
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

fn load_cached_banner_image(cache_path: &Path, source_path: &Path) -> Option<RgbaImage> {
    if !cache_path.is_file() || source_newer_than_cache(source_path, cache_path) {
        return None;
    }
    open_image_fallback_quiet(cache_path)
        .ok()
        .map(|img| img.to_rgba8())
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
        if name == current_name || !name.starts_with(&prefix) || !name.ends_with(".png") {
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
    if !ensure_cache_parent(cache_path) {
        return;
    }
    if let Err(e) = rgba.save(cache_path) {
        warn!(
            "Failed to save cached banner '{}': {e}",
            cache_path.to_string_lossy()
        );
        return;
    }
    prune_stale_banner_cache_variants(cache_path, path_hex);
}

fn load_or_build_cached_banner(
    path: &Path,
    opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    let Some((cache_path, path_hex)) = banner_cache_path_for(path, opts) else {
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
fn is_cacheable_banner_path(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tga" | "tif" | "tiff"
    )
}

fn ensure_cached_banner_on_disk(path: &Path, opts: BannerCacheOptions) -> image::ImageResult<bool> {
    let Some((cache_path, path_hex)) = banner_cache_path_for(path, opts) else {
        return Ok(false);
    };
    if cache_path.is_file() && !source_newer_than_cache(path, &cache_path) {
        prune_stale_banner_cache_variants(&cache_path, &path_hex);
        return Ok(false);
    }
    let rgba = build_cached_banner_rgba(path, opts)?;
    save_cached_banner_image(&cache_path, &path_hex, &rgba);
    Ok(true)
}

enum BannerPrewarmOutcome {
    Built { millis: f64 },
    Reused { millis: f64 },
    SkippedNonFile,
    SkippedNonImage,
    Failed { path: PathBuf, msg: String },
}

#[inline(always)]
fn prewarm_one_banner(path: PathBuf, opts: BannerCacheOptions) -> BannerPrewarmOutcome {
    if !path.is_file() {
        return BannerPrewarmOutcome::SkippedNonFile;
    }
    if !is_cacheable_banner_path(&path) {
        return BannerPrewarmOutcome::SkippedNonImage;
    }

    let started = Instant::now();
    match ensure_cached_banner_on_disk(&path, opts) {
        Ok(true) => BannerPrewarmOutcome::Built {
            millis: started.elapsed().as_secs_f64() * 1000.0,
        },
        Ok(false) => BannerPrewarmOutcome::Reused {
            millis: started.elapsed().as_secs_f64() * 1000.0,
        },
        Err(e) => BannerPrewarmOutcome::Failed {
            path,
            msg: e.to_string(),
        },
    }
}

#[inline(always)]
fn banner_prewarm_workers(job_count: usize) -> usize {
    if job_count == 0 {
        return 0;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(job_count)
}

pub fn prewarm_banner_cache(paths: &[PathBuf]) {
    let opts = BannerCacheOptions::from_config(&crate::config::get());
    if !opts.enabled {
        return;
    }

    let started = Instant::now();
    let mut unique = HashSet::with_capacity(paths.len());
    let mut deduped = Vec::with_capacity(paths.len());
    let mut duplicate = 0usize;
    for path in paths {
        if unique.insert(path.clone()) {
            deduped.push(path.clone());
        } else {
            duplicate += 1;
        }
    }

    let worker_count = banner_prewarm_workers(deduped.len());
    info!(
        "Banner cache prewarm start: {} input, {} unique, {} duplicate, {} worker threads.",
        paths.len(),
        deduped.len(),
        duplicate,
        worker_count
    );

    let (job_tx, job_rx) = mpsc::channel::<PathBuf>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (res_tx, res_rx) = mpsc::channel::<BannerPrewarmOutcome>();
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
                let Ok(path) = job else {
                    return;
                };
                let _ = res_tx.send(prewarm_one_banner(path, opts));
            }
        }));
    }
    drop(res_tx);
    for path in deduped {
        let _ = job_tx.send(path);
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
    for result in res_rx {
        match result {
            BannerPrewarmOutcome::Built { millis } => {
                prepared += 1;
                built += 1;
                built_ms += millis;
            }
            BannerPrewarmOutcome::Reused { millis } => {
                prepared += 1;
                reused += 1;
                reused_ms += millis;
            }
            BannerPrewarmOutcome::SkippedNonFile => {
                skipped_non_file += 1;
            }
            BannerPrewarmOutcome::SkippedNonImage => {
                skipped_non_image += 1;
            }
            BannerPrewarmOutcome::Failed { path, msg } => {
                failed += 1;
                warn!(
                    "Banner cache prewarm failed for '{}': {}",
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
    info!(
        "Banner cache prewarm complete in {:.2}s: prepared={} (built={}, reused={}), \
         skipped={} (non-file={}, non-image={}, duplicate={}), failed={}, workers={}, \
         throughput={:.1}/s, avg_ms={{built:{:.2}, reused:{:.2}}}.",
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

fn build_cached_banner_rgba(
    path: &Path,
    opts: BannerCacheOptions,
) -> image::ImageResult<RgbaImage> {
    let src = open_image_fallback_quiet(path)?.to_rgba8();
    let (dst_w, dst_h) = compute_cached_banner_dims(src.width(), src.height(), opts);
    let mut out = if src.width() == dst_w && src.height() == dst_h {
        src
    } else {
        image::imageops::resize(&src, dst_w, dst_h, image::imageops::FilterType::Triangle)
    };
    apply_banner_cache_color_depth(&mut out, opts.color_depth);
    Ok(out)
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

#[derive(Debug, Clone)]
struct DynamicBannerState {
    key: String,
    path: PathBuf,
    high_res_loaded: bool,
}

pub struct AssetManager {
    pub textures: HashMap<String, GfxTexture>,
    fonts: HashMap<&'static str, Font>,
    current_dynamic_banner: Option<DynamicBannerState>,
    current_dynamic_pack_banner: Option<(String, PathBuf)>,
    current_dynamic_background: Option<(String, PathBuf)>,
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
            fonts: HashMap::new(),
            current_dynamic_banner: None,
            current_dynamic_pack_banner: None,
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

    // --- Loading Logic ---

    pub fn load_initial_assets(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        self.load_initial_textures(backend)?;
        self.load_initial_fonts(backend)?;
        Ok(())
    }

    fn load_initial_textures(&mut self, backend: &mut Backend) -> Result<(), Box<dyn Error>> {
        info!("Loading initial textures...");

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
        self.textures.insert("__white".to_string(), white_tex);
        register_texture_dims("__white", 1, 1);
        info!("Loaded built-in texture: __white");

        // Load __black texture for missing/background-off fallbacks.
        let black_img = RgbaImage::from_raw(1, 1, vec![0, 0, 0, 255]).unwrap();
        let black_tex = backend.create_texture(&black_img, SamplerDesc::default())?;
        self.textures.insert("__black".to_string(), black_tex);
        register_texture_dims("__black", 1, 1);
        info!("Loaded built-in texture: __black");

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
            ("ITL.png".to_string(), "ITL.png".to_string()),
            ("crown.png".to_string(), "crown.png".to_string()),
            (
                "SRPG9_logo_alt (doubleres).png".to_string(),
                "SRPG9_logo_alt (doubleres).png".to_string(),
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
        ];

        // Simply Love-style grade assets (used by `screens::components::eval_grades`).
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
                    info!("Loaded texture: {key}");
                    self.textures.insert(key, texture);
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
                    self.textures.insert(key, texture);
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
                info!("Font 'miso' configured to use 'game' as fallback.");
            }

            if name == "game" {
                font.fallback_font_name = Some("cjk");
                info!("Font 'game' configured to use 'cjk' as fallback.");
            }

            if name == "cjk" {
                font.fallback_font_name = Some("emoji");
                info!("Font 'cjk' configured to use 'emoji' as fallback.");
            }

            for tex_path in &required_textures {
                let key = canonical_texture_key(tex_path);
                if !self.textures.contains_key(&key) {
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
                    self.textures.insert(key.clone(), texture);
                    info!("Loaded font texture: {key}");
                }
            }
            self.register_font(name, font);
            info!("Loaded font '{name}' from '{ini_path_str}'");
        }
        Ok(())
    }

    // --- Dynamic Asset Management ---

    pub fn destroy_dynamic_assets(&mut self, backend: &mut Backend) {
        if self.current_dynamic_banner.is_some()
            || self.current_dynamic_pack_banner.is_some()
            || self.current_dynamic_background.is_some()
        {
            backend.wait_for_idle(); // Wait for GPU to finish using old textures
            if let Some(state) = self.current_dynamic_banner.take() {
                self.textures.remove(&state.key);
            }
            if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
                self.textures.remove(&key);
            }
            if let Some((key, _)) = self.current_dynamic_background.take() {
                self.textures.remove(&key);
            }
        }
    }

    pub fn destroy_dynamic_banner(&mut self, backend: &mut Backend) {
        self.destroy_current_dynamic_banner(backend);
    }

    pub fn set_dynamic_pack_banner(&mut self, backend: &mut Backend, path_opt: Option<PathBuf>) {
        if let Some(path) = path_opt {
            if self
                .current_dynamic_pack_banner
                .as_ref()
                .is_some_and(|(_, p)| p == &path)
            {
                return;
            }

            backend.wait_for_idle();
            if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
                self.textures.remove(&key);
            }

            match open_image_fallback(&path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            let key = path.to_string_lossy().into_owned();
                            self.textures.insert(key.clone(), texture);
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_dynamic_pack_banner = Some((key, path));
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for pack banner {path:?}: {e}. Skipping."
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to open pack banner image {path:?}: {e}. Skipping.");
                }
            }
        } else {
            backend.wait_for_idle();
            if let Some((key, _)) = self.current_dynamic_pack_banner.take() {
                self.textures.remove(&key);
            }
        }
    }

    pub fn set_dynamic_banner(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "banner1.png";
        let banner_cache_opts = BannerCacheOptions::from_config(&crate::config::get());

        if let Some(path) = path_opt {
            if let Some(current) = self.current_dynamic_banner.as_ref()
                && current.path == path
            {
                let key = current.key.clone();
                if banner_cache_opts.enabled && !current.high_res_loaded {
                    match open_image_fallback(&path) {
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            match backend.create_texture(&rgba, SamplerDesc::default()) {
                                Ok(texture) => {
                                    backend.wait_for_idle();
                                    self.textures.insert(key.clone(), texture);
                                    register_texture_dims(&key, rgba.width(), rgba.height());
                                    if let Some(state) = self.current_dynamic_banner.as_mut() {
                                        state.high_res_loaded = true;
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to upgrade banner '{}' to high-res: {e}",
                                        path.display()
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Failed to open high-res banner '{}' during upgrade: {e}",
                                path.display()
                            );
                        }
                    }
                }
                return key;
            }

            self.destroy_current_dynamic_banner(backend);
            let key = path.to_string_lossy().into_owned();

            let (rgba, high_res_loaded) = if banner_cache_opts.enabled
                && is_cacheable_banner_path(&path)
            {
                match load_or_build_cached_banner(&path, banner_cache_opts) {
                    Ok(cached) => (cached, false),
                    Err(e) => {
                        warn!(
                            "Failed to use low-res cached banner '{}': {e}; trying high-res.",
                            path.display()
                        );
                        match open_image_fallback(&path) {
                            Ok(full) => (full.to_rgba8(), true),
                            Err(e) => {
                                warn!("Failed to open banner image {path:?}: {e}. Using fallback.");
                                return FALLBACK_KEY.to_string();
                            }
                        }
                    }
                }
            } else {
                match open_image_fallback(&path) {
                    Ok(full) => (full.to_rgba8(), true),
                    Err(e) => {
                        warn!("Failed to open banner image {path:?}: {e}. Using fallback.");
                        return FALLBACK_KEY.to_string();
                    }
                }
            };

            match backend.create_texture(&rgba, SamplerDesc::default()) {
                Ok(texture) => {
                    self.textures.insert(key.clone(), texture);
                    register_texture_dims(&key, rgba.width(), rgba.height());
                    self.current_dynamic_banner = Some(DynamicBannerState {
                        key: key.clone(),
                        path,
                        high_res_loaded,
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

    pub fn set_dynamic_background(
        &mut self,
        backend: &mut Backend,
        path_opt: Option<PathBuf>,
    ) -> String {
        const FALLBACK_KEY: &str = "__black";

        if let Some(path) = path_opt {
            if self
                .current_dynamic_background
                .as_ref()
                .is_some_and(|(_, p)| p == &path)
            {
                return self.current_dynamic_background.as_ref().unwrap().0.clone();
            }

            self.destroy_current_dynamic_background(backend);

            match open_image_fallback(&path) {
                Ok(img) => {
                    let rgba = img.to_rgba8();
                    match backend.create_texture(&rgba, SamplerDesc::default()) {
                        Ok(texture) => {
                            let key = path.to_string_lossy().into_owned();
                            self.textures.insert(key.clone(), texture);
                            register_texture_dims(&key, rgba.width(), rgba.height());
                            self.current_dynamic_background = Some((key.clone(), path));
                            key
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create GPU texture for background {path:?}: {e}. Using fallback."
                            );
                            FALLBACK_KEY.to_string()
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to open background image {path:?}: {e}. Using fallback.");
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
            if self.textures.contains_key(&key) {
                profile::set_avatar_texture_key_for_side(side, Some(key));
            } else {
                profile::set_avatar_texture_key_for_side(side, None);
            }
        } else {
            self.destroy_current_profile_avatar_for_side(backend, side);
        }
    }

    fn destroy_current_dynamic_banner(&mut self, backend: &mut Backend) {
        if let Some(state) = self.current_dynamic_banner.take() {
            backend.wait_for_idle();
            self.textures.remove(&state.key);
        }
    }

    fn destroy_current_dynamic_background(&mut self, backend: &mut Backend) {
        if let Some((key, _)) = self.current_dynamic_background.take() {
            backend.wait_for_idle();
            self.textures.remove(&key);
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
        if self.textures.contains_key(&key) {
            return;
        }
        if let Some(generated) = generated_texture(&key) {
            match backend.create_texture(generated.image.as_ref(), generated.sampler) {
                Ok(texture) => {
                    self.textures.insert(key, texture);
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
                        self.textures.insert(key.clone(), texture);
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

    pub(crate) fn ensure_texture_from_path(&mut self, backend: &mut Backend, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        if self.textures.contains_key(&key) {
            return;
        }

        match open_image_fallback(path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                match backend.create_texture(&rgba, SamplerDesc::default()) {
                    Ok(texture) => {
                        self.textures.insert(key.clone(), texture);
                        register_texture_dims(&key, rgba.width(), rgba.height());
                    }
                    Err(e) => {
                        warn!("Failed to create GPU texture for image {path:?}: {e}. Skipping.");
                    }
                }
            }
            Err(e) => {
                warn!("Failed to open image {path:?}: {e}. Skipping.");
            }
        }
    }
}
