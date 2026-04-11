use crate::assets::{AssetManager, dynamic, open_image_fallback};
use crate::config::dirs;
use crate::engine::{gfx::Backend, video};
use image::RgbaImage;
use log::{debug, warn};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

#[inline(always)]
pub(crate) fn banner_cache_options() -> dynamic::BannerCacheOptions {
    dynamic::BannerCacheOptions {
        enabled: crate::config::get().banner_cache,
    }
}

#[inline(always)]
pub(crate) fn cdtitle_cache_options() -> dynamic::BannerCacheOptions {
    dynamic::BannerCacheOptions {
        enabled: crate::config::get().cdtitle_cache,
    }
}

pub(crate) fn load_banner_source_rgba(path: &Path) -> Result<RgbaImage, String> {
    let opts = banner_cache_options();
    if opts.enabled {
        return dynamic::load_or_build_cached_dynamic_image(
            path,
            opts,
            &dirs::app_dirs().banner_cache_dir(),
        )
        .map_err(|e| e.to_string());
    }
    if dynamic::is_dynamic_video_path(path) {
        return video::load_poster(path);
    }
    open_image_fallback(path)
        .map(|img| img.to_rgba8())
        .map_err(|e| e.to_string())
}

pub(crate) fn load_cdtitle_source_rgba(path: &Path) -> Result<RgbaImage, String> {
    let opts = cdtitle_cache_options();
    if opts.enabled {
        return dynamic::load_or_build_cached_dynamic_image(
            path,
            opts,
            &dirs::app_dirs().cdtitle_cache_dir(),
        )
        .map_err(|e| e.to_string());
    }
    open_image_fallback(path)
        .map(|img| img.to_rgba8())
        .map_err(|e| e.to_string())
}

pub(crate) fn ensure_banner_texture(assets: &mut AssetManager, backend: &mut Backend, path: &Path) {
    let key = path.to_string_lossy().into_owned();
    if assets.has_texture_key(&key) {
        return;
    }

    let rgba = match load_banner_source_rgba(path) {
        Ok(rgba) => rgba,
        Err(e) => {
            warn!("Failed to load banner source {path:?}: {e}. Skipping.");
            return;
        }
    };

    if let Err(e) = assets.update_texture_for_key(backend, &key, &rgba) {
        warn!("Failed to create GPU texture for image {path:?}: {e}. Skipping.");
    }
}

pub(crate) fn queue_banner_texture(assets: &mut AssetManager, path: &Path) {
    let key = path.to_string_lossy().into_owned();
    if assets.has_texture_key(&key) {
        return;
    }

    let rgba = match load_banner_source_rgba(path) {
        Ok(rgba) => rgba,
        Err(e) => {
            warn!("Failed to load banner source {path:?}: {e}. Skipping.");
            return;
        }
    };

    assets.queue_texture_upload(key, rgba);
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
    opts: dynamic::BannerCacheOptions,
    cache_dir: PathBuf,
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
    opts: dynamic::BannerCacheOptions,
    cache_dir: &Path,
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
                cache_dir: cache_dir.to_path_buf(),
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
    opts: dynamic::BannerCacheOptions,
    cache_dir: &Path,
) -> DynamicImagePrewarmOutcome {
    if !path.is_file() {
        return DynamicImagePrewarmOutcome::SkippedNonFile { path };
    }
    if !dynamic::is_cacheable_dynamic_image_path(&path) {
        return DynamicImagePrewarmOutcome::SkippedNonImage { path };
    }

    let started = Instant::now();
    match dynamic::ensure_cached_dynamic_image_on_disk(&path, opts, cache_dir) {
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
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .min(job_count)
}

#[inline(always)]
fn dynamic_image_prewarm_dedupe_key(
    path: &Path,
    opts: dynamic::BannerCacheOptions,
    cache_dir: &Path,
) -> String {
    dynamic::dynamic_image_cache_path_for(path, opts, cache_dir).map_or_else(
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
                let outcome = prewarm_one_dynamic_image(job.path, job.opts, &job.cache_dir);
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

pub(crate) fn artwork_cache_jobs(banner_paths: &[PathBuf], cdtitle_paths: &[PathBuf]) -> usize {
    let banner_opts = banner_cache_options();
    let cdtitle_opts = cdtitle_cache_options();
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique = HashSet::<String>::with_capacity(total_paths);
    let bcache = dirs::app_dirs().banner_cache_dir();
    let ccache = dirs::app_dirs().cdtitle_cache_dir();
    if banner_opts.enabled {
        for path in banner_paths {
            unique.insert(dynamic_image_prewarm_dedupe_key(path, banner_opts, &bcache));
        }
    }
    if cdtitle_opts.enabled {
        for path in cdtitle_paths {
            unique.insert(dynamic_image_prewarm_dedupe_key(
                path,
                cdtitle_opts,
                &ccache,
            ));
        }
    }
    unique.len()
}

pub(crate) fn prewarm_artwork_cache_with_progress<F>(
    banner_paths: &[PathBuf],
    cdtitle_paths: &[PathBuf],
    progress: &mut F,
) where
    F: FnMut(usize, usize, Option<&Path>),
{
    let banner_opts = banner_cache_options();
    let cdtitle_opts = cdtitle_cache_options();
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique = HashSet::<String>::with_capacity(total_paths);
    let mut jobs = Vec::<DynamicImagePrewarmJob>::with_capacity(total_paths);
    let mut duplicate = 0usize;
    let bcache = dirs::app_dirs().banner_cache_dir();
    let ccache = dirs::app_dirs().cdtitle_cache_dir();
    duplicate = duplicate.saturating_add(push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        banner_paths,
        banner_opts,
        &bcache,
        "Banner",
    ));
    duplicate = duplicate.saturating_add(push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        cdtitle_paths,
        cdtitle_opts,
        &ccache,
        "CDTitle",
    ));
    prewarm_dynamic_image_jobs_with_progress(total_paths, jobs, duplicate, "Artwork", progress);
}
