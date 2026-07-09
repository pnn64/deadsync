use crate::{AssetManager, open_image_fallback};
use deadlib_assets::dynamic;
use deadlib_platform::dirs;
use deadlib_renderer::Backend;
use deadlib_video as video;
use image::RgbaImage;
use log::warn;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[inline(always)]
pub fn banner_cache_options() -> dynamic::BannerCacheOptions {
    dynamic::BannerCacheOptions {
        enabled: deadsync_config::runtime::get().banner_cache,
    }
}

#[inline(always)]
pub fn cdtitle_cache_options() -> dynamic::BannerCacheOptions {
    dynamic::BannerCacheOptions {
        enabled: deadsync_config::runtime::get().cdtitle_cache,
    }
}

pub fn load_banner_source_rgba(path: &Path) -> Result<RgbaImage, String> {
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

pub fn load_cdtitle_source_rgba(path: &Path) -> Result<RgbaImage, String> {
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

pub fn ensure_banner_texture(assets: &mut AssetManager, backend: &mut Backend, path: &Path) {
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

pub fn artwork_cache_jobs(banner_paths: &[PathBuf], cdtitle_paths: &[PathBuf]) -> usize {
    let banner_opts = banner_cache_options();
    let cdtitle_opts = cdtitle_cache_options();
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique = HashSet::<String>::with_capacity(total_paths);
    let bcache = dirs::app_dirs().banner_cache_dir();
    let ccache = dirs::app_dirs().cdtitle_cache_dir();
    if banner_opts.enabled {
        for path in banner_paths {
            unique.insert(dynamic::dynamic_image_prewarm_dedupe_key(
                path,
                banner_opts,
                &bcache,
            ));
        }
    }
    if cdtitle_opts.enabled {
        for path in cdtitle_paths {
            unique.insert(dynamic::dynamic_image_prewarm_dedupe_key(
                path,
                cdtitle_opts,
                &ccache,
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
    let banner_opts = banner_cache_options();
    let cdtitle_opts = cdtitle_cache_options();
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique = HashSet::<String>::with_capacity(total_paths);
    let mut jobs = Vec::<dynamic::DynamicImagePrewarmJob>::with_capacity(total_paths);
    let mut duplicate = 0usize;
    let bcache = dirs::app_dirs().banner_cache_dir();
    let ccache = dirs::app_dirs().cdtitle_cache_dir();
    duplicate = duplicate.saturating_add(dynamic::push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        banner_paths,
        banner_opts,
        &bcache,
        "Banner",
    ));
    duplicate = duplicate.saturating_add(dynamic::push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        cdtitle_paths,
        cdtitle_opts,
        &ccache,
        "CDTitle",
    ));
    dynamic::prewarm_dynamic_image_jobs_with_progress(
        total_paths,
        jobs,
        duplicate,
        "Artwork",
        progress,
    );
}
