use crate::{
    TextureHints, apply_texture_hints, discover_graphic_textures_in_roots, fix_hidden_alpha,
    initial_texture_source_path, noteskin_png_texture_entries, open_image_fallback,
};
use image::RgbaImage;
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

#[derive(Clone, Copy)]
pub struct GraphicTextureDiscovery {
    pub folder: &'static str,
    pub love_first: bool,
    pub require_multiframe_hint: bool,
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
    texture_assets: impl IntoIterator<Item = (String, String)>,
    noteskin_roots: &[PathBuf],
    canonical_key: impl Fn(&Path) -> String,
    graphic_folders: &[GraphicTextureDiscovery],
    graphic_roots: impl Fn(&str) -> Vec<PathBuf>,
    resolve_asset_path: impl Fn(&str) -> PathBuf,
) -> Vec<TextureDecodeJob> {
    let mut textures: Vec<(String, String)> = texture_assets.into_iter().collect();
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
            [("logo.png".to_string(), "logo.png".to_string())],
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
