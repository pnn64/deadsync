use crate::generated_texture;
use crate::{
    TextureHints, apply_texture_hints, discover_graphic_textures_in_roots, fix_hidden_alpha,
    initial_texture_source_path, noteskin_png_texture_entries, open_image_fallback,
    parse_texture_hints, texture_key_sampler, texture_key_source_path,
};
use deadlib_render_core::SamplerDesc;
use image::RgbaImage;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

/// Number of decode jobs claimed under one queue lock.
///
/// Eight amortizes synchronization while leaving enough batches to balance
/// differently sized texture files across the available workers.
const DECODE_JOB_BATCH_SIZE: usize = 8;

pub struct TextureDecodeJob {
    pub key: String,
    pub path: PathBuf,
}

pub enum TextureDecodeResult {
    Decoded { key: String, image: RgbaImage },
    Failed { key: String, message: String },
}

struct DecodeSlot {
    state: Mutex<DecodeSlotState>,
    ready: Condvar,
    empty: Condvar,
}

struct DecodeSlotState {
    result: Option<TextureDecodeResult>,
    active_workers: usize,
    cancelled: bool,
}

impl DecodeSlot {
    fn new(active_workers: usize) -> Self {
        Self {
            state: Mutex::new(DecodeSlotState {
                result: None,
                active_workers,
                cancelled: false,
            }),
            ready: Condvar::new(),
            empty: Condvar::new(),
        }
    }

    fn send(&self, result: TextureDecodeResult) -> bool {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        while state.result.is_some() && !state.cancelled {
            state = self
                .empty
                .wait(state)
                .expect("texture decode slot poisoned");
        }
        if state.cancelled {
            return false;
        }
        state.result = Some(result);
        self.ready.notify_one();
        true
    }

    fn receive(&self) -> Option<TextureDecodeResult> {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        loop {
            if let Some(result) = state.result.take() {
                self.empty.notify_one();
                return Some(result);
            }
            if state.active_workers == 0 {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .expect("texture decode slot poisoned");
        }
    }

    fn finish_worker(&self) {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        state.active_workers -= 1;
        self.ready.notify_one();
    }

    fn cancel(&self) {
        let mut state = self.state.lock().expect("texture decode slot poisoned");
        state.cancelled = true;
        self.empty.notify_all();
    }
}

struct DecodeWorker<'a>(&'a DecodeSlot);

impl Drop for DecodeWorker<'_> {
    fn drop(&mut self) {
        self.0.finish_worker();
    }
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

#[must_use]
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
    let mut image = open_image_fallback(path)?.into_rgba8();
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
    let textures = texture_assets
        .into_iter()
        .map(|asset| (asset.key.to_string(), asset.path.to_string()))
        .chain(noteskin_png_texture_entries(
            noteskin_roots,
            "noteskins",
            canonical_key,
        ))
        .chain(graphic_folders.iter().flat_map(|spec| {
            discover_graphic_textures_in_roots(
                spec.folder,
                graphic_roots(spec.folder),
                spec.love_first,
                spec.require_multiframe_hint,
            )
            .into_iter()
            .map(|texture| (texture.key, texture.source_path))
        }));
    textures
        .map(|(key, relative_path)| TextureDecodeJob {
            key,
            path: initial_texture_source_path(&relative_path, &resolve_asset_path),
        })
        .collect()
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

/// Decodes on workers while the caller consumes completed images immediately.
///
/// The bounded handoff limits retained decoded pixels to roughly one image per
/// worker instead of the full startup corpus.
///
/// # Panics
///
/// Panics if an internal worker fails.
pub(crate) fn decode_texture_jobs_with<E>(
    jobs: Vec<TextureDecodeJob>,
    mut consume: impl FnMut(TextureDecodeResult) -> Result<(), E>,
) -> Result<(), E> {
    let job_count = jobs.len();
    if job_count == 0 {
        return Ok(());
    }

    let worker_count = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1)
        .min(job_count);
    if worker_count == 1 {
        for job in jobs {
            consume(decode_rgba(job))?;
        }
        return Ok(());
    }

    let jobs = Mutex::new(jobs.into_iter());
    let slot = DecodeSlot::new(worker_count);
    std::thread::scope(|scope| {
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let jobs = &jobs;
            let slot = &slot;
            workers.push(scope.spawn(move || {
                let _worker = DecodeWorker(slot);
                let mut batch = Vec::with_capacity(DECODE_JOB_BATCH_SIZE);
                loop {
                    {
                        let mut jobs = jobs.lock().expect("texture decode job queue poisoned");
                        batch.extend(jobs.by_ref().take(DECODE_JOB_BATCH_SIZE));
                    }
                    if batch.is_empty() {
                        return;
                    }
                    for job in batch.drain(..) {
                        if !slot.send(decode_rgba(job)) {
                            return;
                        }
                    }
                }
            }));
        }

        let mut result = Ok(());
        while let Some(decoded) = slot.receive() {
            if let Err(error) = consume(decoded) {
                slot.cancel();
                result = Err(error);
                break;
            }
        }
        for worker in workers {
            worker.join().expect("texture decode worker panicked");
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_empty_job_list() {
        let mut called = false;
        decode_texture_jobs_with(Vec::new(), |_| {
            called = true;
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();

        assert!(!called);
    }

    #[test]
    fn prepare_texture_key_load_skips_empty_and_internal_keys() {
        assert!(matches!(
            prepare_texture_key_load(
                "",
                None,
                false,
                |_| false,
                std::string::ToString::to_string,
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
                std::string::ToString::to_string,
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
                std::string::ToString::to_string,
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
        let mut results = Vec::new();
        decode_texture_jobs_with(
            vec![TextureDecodeJob {
                key: "missing".to_string(),
                path: PathBuf::from("__missing_texture__.png"),
            }],
            |result| {
                results.push(result);
                Ok::<_, std::convert::Infallible>(())
            },
        )
        .unwrap();

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
