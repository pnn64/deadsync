use crate::open_image_fallback;
use deadlib_assets::{AssetManager, dynamic};
use deadlib_render_core::TextureBackend;
use deadlib_video as video;
use deadlib_image::RgbaImage;
use log::warn;
use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};

fn decode_artwork_source(path: &Path) -> deadlib_image_io::ImageResult<RgbaImage> {
    if dynamic::is_dynamic_video_path(path) {
        return video::load_poster(path)
            .map_err(|e| deadlib_image_io::ImageError::Io(std::io::Error::other(e)));
    }
    deadlib_assets::open_image_fallback_quiet(path)
}

pub fn load_banner_source_rgba(
    path: &Path,
    opts: dynamic::BannerCacheOptions,
) -> Result<RgbaImage, String> {
    if opts.enabled {
        return dynamic::load_or_build_cached_dynamic_image(
            path,
            opts,
            &crate::paths().banner_cache,
            decode_artwork_source,
        )
        .map_err(|e| e.to_string());
    }
    if dynamic::is_dynamic_video_path(path) {
        return video::load_poster(path);
    }
    open_image_fallback(path)
        .map_err(|e| e.to_string())
}

pub fn load_cdtitle_source_rgba(
    path: &Path,
    opts: dynamic::BannerCacheOptions,
) -> Result<RgbaImage, String> {
    if opts.enabled {
        return dynamic::load_or_build_cached_dynamic_image(
            path,
            opts,
            &crate::paths().cdtitle_cache,
            decode_artwork_source,
        )
        .map_err(|e| e.to_string());
    }
    open_image_fallback(path)
        .map_err(|e| e.to_string())
}

pub fn ensure_banner_texture<B: TextureBackend<RgbaImage = RgbaImage>>(
    assets: &mut AssetManager<B::Texture>,
    backend: &mut B,
    path: &Path,
    cache_options: impl FnOnce() -> dynamic::BannerCacheOptions,
) {
    let key = path.to_string_lossy().into_owned();
    if assets.has_texture_key(&key) {
        return;
    }

    let rgba = match load_banner_source_rgba(path, cache_options()) {
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

/// One-shot artwork work list retained between progress setup and execution.
///
/// Building the plan performs path canonicalization and deduplication once;
/// consuming it guarantees that the progress total matches the executed jobs.
#[must_use = "an artwork plan does no work until it is prewarmed"]
pub struct ArtworkCachePlan {
    total_paths: usize,
    duplicate: usize,
    jobs: Vec<dynamic::DynamicImagePrewarmJob>,
}

impl ArtworkCachePlan {
    #[inline(always)]
    #[must_use]
    pub const fn job_count(&self) -> usize {
        self.jobs.len()
    }
}

fn build_artwork_cache_plan(
    banner_paths: &[PathBuf],
    cdtitle_paths: &[PathBuf],
    banner_opts: dynamic::BannerCacheOptions,
    cdtitle_opts: dynamic::BannerCacheOptions,
    banner_cache_dir: &Path,
    cdtitle_cache_dir: &Path,
) -> ArtworkCachePlan {
    let total_paths = banner_paths.len().saturating_add(cdtitle_paths.len());
    let mut unique =
        FxHashSet::<String>::with_capacity_and_hasher(total_paths, rustc_hash::FxBuildHasher);
    let mut jobs = Vec::<dynamic::DynamicImagePrewarmJob>::with_capacity(total_paths);
    let mut duplicate = dynamic::push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        banner_paths,
        banner_opts,
        banner_cache_dir,
        "Banner",
    );
    duplicate = duplicate.saturating_add(dynamic::push_dynamic_image_prewarm_jobs(
        &mut jobs,
        &mut unique,
        cdtitle_paths,
        cdtitle_opts,
        cdtitle_cache_dir,
        "CDTitle",
    ));
    ArtworkCachePlan {
        total_paths,
        duplicate,
        jobs,
    }
}

#[must_use]
pub fn artwork_cache_plan(
    banner_paths: &[PathBuf],
    cdtitle_paths: &[PathBuf],
    banner_opts: dynamic::BannerCacheOptions,
    cdtitle_opts: dynamic::BannerCacheOptions,
) -> ArtworkCachePlan {
    let bcache = &crate::paths().banner_cache;
    let ccache = &crate::paths().cdtitle_cache;
    build_artwork_cache_plan(
        banner_paths,
        cdtitle_paths,
        banner_opts,
        cdtitle_opts,
        bcache,
        ccache,
    )
}

pub fn prewarm_artwork_cache_with_progress<F>(plan: ArtworkCachePlan, progress: &mut F)
where
    F: FnMut(usize, usize, Option<&Path>),
{
    dynamic::prewarm_dynamic_image_jobs_with_progress(
        plan.total_paths,
        plan.jobs,
        plan.duplicate,
        "Artwork",
        decode_artwork_source,
        progress,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artwork_plan_preserves_order_labels_and_duplicate_count() {
        let banners = vec!["a.png".into(), "b.png".into(), "a.png".into()];
        let cdtitles = vec!["c.png".into(), "c.png".into()];
        let opts = dynamic::BannerCacheOptions { enabled: true };
        let plan = build_artwork_cache_plan(
            &banners,
            &cdtitles,
            opts,
            opts,
            Path::new("banner-cache"),
            Path::new("cdtitle-cache"),
        );

        assert_eq!(plan.total_paths, 5);
        assert_eq!(plan.duplicate, 2);
        assert_eq!(plan.job_count(), 3);
        assert_eq!(
            plan.jobs
                .iter()
                .map(|job| (job.label, job.path.as_path()))
                .collect::<Vec<_>>(),
            vec![
                ("Banner", Path::new("a.png")),
                ("Banner", Path::new("b.png")),
                ("CDTitle", Path::new("c.png")),
            ]
        );
    }

    #[test]
    fn artwork_plan_excludes_disabled_cache_classes() {
        let banners = vec!["banner.png".into()];
        let cdtitles = vec!["cdtitle.png".into()];
        let plan = build_artwork_cache_plan(
            &banners,
            &cdtitles,
            dynamic::BannerCacheOptions { enabled: false },
            dynamic::BannerCacheOptions { enabled: true },
            Path::new("banner-cache"),
            Path::new("cdtitle-cache"),
        );

        assert_eq!(plan.total_paths, 2);
        assert_eq!(plan.duplicate, 0);
        assert_eq!(plan.job_count(), 1);
        assert_eq!(plan.jobs[0].label, "CDTitle");
    }
}
