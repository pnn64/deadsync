use crate::assets::AssetManager;
use crate::core::{gfx::Backend, video};
use image::RgbaImage;
use log::{debug, warn};
use std::{
    collections::HashSet,
    fs,
    hash::Hasher,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};
use twox_hash::XxHash64;

use super::textures::{open_image_fallback, open_image_fallback_quiet};

const BANNER_CACHE_DIR: &str = "cache/banner";
const CDTITLE_CACHE_DIR: &str = "cache/cdtitle";
static BANNER_CACHE_TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug)]
pub(crate) struct BannerCacheOptions {
    pub(crate) enabled: bool,
}

impl BannerCacheOptions {
    #[inline(always)]
    pub(crate) fn from_banner_config(cfg: &crate::config::Config) -> Self {
        Self {
            enabled: cfg.banner_cache,
        }
    }

    #[inline(always)]
    pub(crate) fn from_cdtitle_config(cfg: &crate::config::Config) -> Self {
        Self {
            enabled: cfg.cdtitle_cache,
        }
    }
}

fn banner_cache_opthash(opts: BannerCacheOptions) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write_u8(1);
    hasher.write_u8(u8::from(opts.enabled));
    hasher.finish()
}

const BANNER_CACHE_MAGIC: [u8; 8] = *b"DSBNR02\0";
const BANNER_CACHE_HEADER_SIZE: usize = 16;

pub(crate) fn dynamic_image_cache_path_for(
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

pub(crate) fn save_raw_cached_banner_image(cache_path: &Path, rgba: &RgbaImage) -> bool {
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

pub(crate) fn save_cached_banner_image(cache_path: &Path, path_hex: &str, rgba: &RgbaImage) {
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

pub(crate) fn load_or_build_cached_dynamic_image(
    path: &Path,
    opts: BannerCacheOptions,
    cache_dir: &str,
) -> image::ImageResult<RgbaImage> {
    let Some((cache_path, path_hex)) = dynamic_image_cache_path_for(path, opts, cache_dir) else {
        return build_cached_banner_rgba(path, opts);
    };

    if let Some(rgba) = load_cached_banner_image(&cache_path, path) {
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
pub(crate) fn is_dynamic_video_path(path: &Path) -> bool {
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

pub(crate) fn load_banner_source_rgba(
    path: &Path,
    opts: BannerCacheOptions,
) -> Result<RgbaImage, String> {
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

pub(crate) fn load_cdtitle_source_rgba(
    path: &Path,
    opts: BannerCacheOptions,
) -> Result<RgbaImage, String> {
    if opts.enabled {
        return load_or_build_cached_cdtitle(path, opts).map_err(|e| e.to_string());
    }
    open_image_fallback(path)
        .map(|img| img.to_rgba8())
        .map_err(|e| e.to_string())
}

pub(crate) fn collect_stale_dynamic_keys<'a>(
    current: impl Iterator<Item = &'a String>,
    desired: &HashSet<String>,
) -> Vec<String> {
    current
        .filter(|key| !desired.contains(*key))
        .cloned()
        .collect()
}

pub(crate) fn dedupe_dynamic_keys(keys: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::with_capacity(keys.len());
    let mut out = Vec::with_capacity(keys.len());
    for key in keys {
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}

impl AssetManager {
    pub(crate) fn ensure_texture_from_path(&mut self, backend: &mut Backend, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        if self.has_texture_key(&key) {
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
        }
    }
}
