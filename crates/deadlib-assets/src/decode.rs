use crate::{
    TextureHints, apply_texture_hints, discover_graphic_textures_in_roots, fix_hidden_alpha,
    initial_texture_sampler, initial_texture_source_path, noteskin_png_texture_entries,
    open_image_fallback, parse_texture_hints, texture_key_sampler, texture_key_source_path,
};
use crate::{black_texture_image, fallback_texture_image, generated_texture, white_texture_image};
use deadlib_render::SamplerDesc;
use image::RgbaImage;
use log::warn;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};

pub struct TextureDecodeJob {
    pub key: String,
    pub path: PathBuf,
}

pub enum TextureDecodeResult {
    Decoded { key: String, image: RgbaImage },
    Failed { key: String, message: String },
}

pub struct PreparedTextureImage {
    pub key: String,
    pub image: Arc<RgbaImage>,
    pub sampler: SamplerDesc,
    pub built_in: bool,
}

pub enum TextureKeyLoad {
    Skip,
    Missing {
        key: String,
    },
    DecodeFailed {
        key: String,
        message: String,
    },
    Image {
        key: String,
        image: Arc<RgbaImage>,
        sampler: SamplerDesc,
        register_dims: bool,
    },
}

#[derive(Clone, Copy)]
pub struct GraphicTextureDiscovery {
    pub folder: &'static str,
    pub love_first: bool,
    pub require_multiframe_hint: bool,
}

#[derive(Clone, Copy)]
pub struct TextureAssetSpec {
    pub key: &'static str,
    pub path: &'static str,
}

pub const fn texture_asset(path: &'static str) -> TextureAssetSpec {
    TextureAssetSpec { key: path, path }
}

fn decode_rgba(job: TextureDecodeJob) -> TextureDecodeResult {
    match decode_texture_image(&job.path, &TextureHints::default()) {
        Ok(image) => TextureDecodeResult::Decoded {
            key: job.key,
            image,
        },
        Err(e) => TextureDecodeResult::Failed {
            key: job.key,
            message: e.to_string(),
        },
    }
}

pub fn decode_texture_image(path: &Path, hints: &TextureHints) -> image::ImageResult<RgbaImage> {
    let mut image = open_image_fallback(path)?.to_rgba8();
    if !hints.is_default() {
        apply_texture_hints(&mut image, hints);
    }
    fix_hidden_alpha(&mut image);
    Ok(image)
}

pub fn initial_texture_decode_jobs(
    texture_assets: impl IntoIterator<Item = TextureAssetSpec>,
    noteskin_roots: &[PathBuf],
    canonical_key: impl Fn(&Path) -> String,
    graphic_folders: &[GraphicTextureDiscovery],
    graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    resolve_asset_path: impl Fn(&str) -> PathBuf,
) -> Vec<TextureDecodeJob> {
    let mut textures: Vec<(String, String)> = texture_assets
        .into_iter()
        .map(|asset| (asset.key.to_string(), asset.path.to_string()))
        .collect();
    textures.extend(noteskin_png_texture_entries(
        noteskin_roots,
        "noteskins",
        canonical_key,
    ));
    for spec in graphic_folders {
        for texture in discover_graphic_textures_in_roots(
            spec.folder,
            graphic_roots(spec.folder),
            spec.love_first,
            spec.require_multiframe_hint,
        ) {
            textures.push((texture.key, texture.source_path));
        }
    }
    textures
        .into_iter()
        .map(|(key, relative_path)| TextureDecodeJob {
            key,
            path: initial_texture_source_path(&relative_path, &resolve_asset_path),
        })
        .collect()
}

pub fn prepare_initial_texture_images(
    jobs: Vec<TextureDecodeJob>,
    needs_repeat_sampler: impl Fn(&str) -> bool,
) -> Vec<PreparedTextureImage> {
    let mut prepared = Vec::new();
    for built_in in [white_texture_image(), black_texture_image()] {
        prepared.push(PreparedTextureImage {
            key: built_in.key.to_string(),
            image: Arc::new(built_in.image),
            sampler: SamplerDesc::default(),
            built_in: true,
        });
    }

    let fallback_image = Arc::new(fallback_texture_image());
    for result in decode_texture_jobs_parallel(jobs) {
        match result {
            TextureDecodeResult::Decoded { key, image } => {
                let sampler = initial_texture_sampler(&key, needs_repeat_sampler(&key));
                prepared.push(PreparedTextureImage {
                    key,
                    image: Arc::new(image),
                    sampler,
                    built_in: false,
                });
            }
            TextureDecodeResult::Failed { key, message } => {
                warn!("Failed to load texture for key '{key}': {message}. Using fallback.");
                let sampler = initial_texture_sampler(&key, needs_repeat_sampler(&key));
                prepared.push(PreparedTextureImage {
                    key,
                    image: Arc::clone(&fallback_image),
                    sampler,
                    built_in: false,
                });
            }
        }
    }
    prepared
}

pub fn prepare_texture_key_load(
    texture_key: &str,
    sampler_override: Option<SamplerDesc>,
    force_reload: bool,
    has_texture_key: impl Fn(&str) -> bool,
    canonical_texture_key: impl Fn(&str) -> String,
    resolve_asset_path: impl Fn(&str) -> PathBuf,
    needs_repeat_sampler: impl Fn(&str) -> bool,
) -> TextureKeyLoad {
    if texture_key.is_empty() {
        return TextureKeyLoad::Skip;
    }

    let key = canonical_texture_key(texture_key);
    if !force_reload && has_texture_key(&key) {
        return TextureKeyLoad::Skip;
    }

    if let Some(generated) = generated_texture(&key) {
        return TextureKeyLoad::Image {
            key,
            image: generated.image,
            sampler: sampler_override.unwrap_or(generated.sampler),
            register_dims: false,
        };
    }
    if key.starts_with("__") {
        return TextureKeyLoad::Skip;
    }

    let path = texture_key_source_path(texture_key, &key, resolve_asset_path);
    if !path.is_file() {
        return TextureKeyLoad::Missing { key };
    }

    let hints = parse_texture_hints(&key);
    let sampler =
        sampler_override.unwrap_or_else(|| texture_key_sampler(&hints, needs_repeat_sampler(&key)));
    match decode_texture_image(&path, &hints) {
        Ok(image) => TextureKeyLoad::Image {
            key,
            image: Arc::new(image),
            sampler,
            register_dims: true,
        },
        Err(e) => TextureKeyLoad::DecodeFailed {
            key,
            message: e.to_string(),
        },
    }
}

pub fn decode_texture_jobs_parallel(jobs: Vec<TextureDecodeJob>) -> Vec<TextureDecodeResult> {
    let job_count = jobs.len();
    if job_count == 0 {
        return Vec::new();
    }

    let worker_count = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .min(job_count);

    let (job_tx, job_rx) = mpsc::channel::<TextureDecodeJob>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (res_tx, res_rx) = mpsc::channel::<TextureDecodeResult>();

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
                let Ok(job) = job else { return };
                let _ = res_tx.send(decode_rgba(job));
            }
        }));
    }
    drop(res_tx);

    for job in jobs {
        let _ = job_tx.send(job);
    }
    drop(job_tx);

    let mut results = Vec::with_capacity(job_count);
    for result in res_rx {
        results.push(result);
    }

    for worker in workers {
        worker.join().expect("texture decode worker panicked");
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_empty_job_list() {
        assert!(decode_texture_jobs_parallel(Vec::new()).is_empty());
    }

    #[test]
    fn prepare_initial_texture_images_includes_builtins() {
        let prepared = prepare_initial_texture_images(Vec::new(), |_| false);

        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].key, crate::WHITE_TEXTURE_KEY);
        assert!(prepared[0].built_in);
        assert_eq!(prepared[1].key, crate::BLACK_TEXTURE_KEY);
        assert!(prepared[1].built_in);
    }

    #[test]
    fn prepare_initial_texture_images_uses_fallback_for_failed_decode() {
        let prepared = prepare_initial_texture_images(
            vec![TextureDecodeJob {
                key: "grades/goldstar (stretch).png".to_string(),
                path: PathBuf::from("__missing_initial_texture__.png"),
            }],
            |_| true,
        );

        assert_eq!(prepared.len(), 3);
        assert_eq!(prepared[2].key, "grades/goldstar (stretch).png");
        assert!(!prepared[2].built_in);
        assert_eq!(prepared[2].image.width(), 2);
        assert_eq!(prepared[2].image.height(), 2);
        assert_eq!(
            prepared[2].sampler.wrap,
            deadlib_render::SamplerWrap::Repeat
        );
    }

    #[test]
    fn prepare_texture_key_load_skips_empty_and_internal_keys() {
        assert!(matches!(
            prepare_texture_key_load(
                "",
                None,
                false,
                |_| false,
                |key| key.to_string(),
                |path| PathBuf::from(path),
                |_| false
            ),
            TextureKeyLoad::Skip
        ));
        assert!(matches!(
            prepare_texture_key_load(
                "__white",
                None,
                false,
                |_| false,
                |key| key.to_string(),
                |path| PathBuf::from(path),
                |_| false
            ),
            TextureKeyLoad::Skip
        ));
    }

    #[test]
    fn prepare_texture_key_load_skips_cached_key_without_force() {
        assert!(matches!(
            prepare_texture_key_load(
                "cached.png",
                None,
                false,
                |key| key == "cached.png",
                |key| key.to_string(),
                |path| PathBuf::from(path),
                |_| false
            ),
            TextureKeyLoad::Skip
        ));
    }

    #[test]
    fn prepare_texture_key_load_reports_missing_source() {
        match prepare_texture_key_load(
            "missing.png",
            None,
            false,
            |_| false,
            str::to_string,
            |_| PathBuf::from("__missing_texture_key_source__.png"),
            |_| false,
        ) {
            TextureKeyLoad::Missing { key } => assert_eq!(key, "missing.png"),
            _ => panic!("missing source should be reported"),
        }
    }

    #[test]
    fn reports_missing_texture_decode_failure() {
        let results = decode_texture_jobs_parallel(vec![TextureDecodeJob {
            key: "missing".to_string(),
            path: PathBuf::from("__missing_texture__.png"),
        }]);

        assert_eq!(results.len(), 1);
        match &results[0] {
            TextureDecodeResult::Failed { key, message } => {
                assert_eq!(key, "missing");
                assert!(!message.is_empty());
            }
            TextureDecodeResult::Decoded { .. } => panic!("missing texture decoded"),
        }
    }

    #[test]
    fn decode_texture_image_reports_missing_file() {
        let err = decode_texture_image(
            Path::new("__missing_texture_decode_image__.png"),
            &TextureHints::default(),
        )
        .expect_err("missing image should fail");

        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn initial_texture_decode_jobs_maps_theme_assets() {
        let jobs = initial_texture_decode_jobs(
            [texture_asset("logo.png")],
            &[],
            |path| path.to_string_lossy().replace('\\', "/"),
            &[],
            |_| Vec::new(),
            |path| PathBuf::from(path),
        );

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].key, "logo.png");
        assert_eq!(jobs[0].path, PathBuf::from("assets/graphics/logo.png"));
    }
}
