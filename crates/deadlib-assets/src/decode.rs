use crate::{TextureHints, apply_texture_hints, fix_hidden_alpha, open_image_fallback};
use deadlib_render_core::SamplerDesc;
use image::RgbaImage;
use std::{
    path::{Path, PathBuf},
    sync::{Condvar, Mutex},
};

/// Number of decode jobs claimed under one queue lock.
///
/// Eight amortizes synchronization while leaving enough batches to balance
/// differently sized texture files across the available workers.
const DECODE_JOB_BATCH_SIZE: usize = 8;

/// A fully resolved image load. Asset catalogs, path selection, and sampler policy
/// belong to the caller; workers apply only the supplied image options.
pub struct TextureDecodeJob {
    pub key: String,
    pub path: PathBuf,
    pub sampler: SamplerDesc,
    pub hints: TextureHints,
}

pub(crate) struct TextureDecodeResult {
    pub key: String,
    pub sampler: SamplerDesc,
    pub image: image::ImageResult<RgbaImage>,
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
    TextureDecodeResult {
        image: decode_texture_image(&job.path, &job.hints),
        key: job.key,
        sampler: job.sampler,
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
    use deadlib_render_core::{SamplerFilter, SamplerWrap};

    fn missing_job(index: usize) -> TextureDecodeJob {
        TextureDecodeJob {
            key: format!("missing-{index}"),
            path: PathBuf::from(format!("__missing_texture_{index}__.png")),
            sampler: SamplerDesc {
                filter: SamplerFilter::Nearest,
                wrap: SamplerWrap::Repeat,
                mipmaps: true,
            },
            hints: TextureHints::default(),
        }
    }

    #[test]
    fn empty_jobs_do_not_call_consumer() {
        decode_texture_jobs_with(Vec::new(), |_| -> Result<(), ()> {
            panic!("empty jobs must not call the consumer")
        })
        .expect("empty decode");
    }

    #[test]
    fn decode_failure_preserves_key_and_sampler() {
        let job = missing_job(0);
        let sampler = job.sampler;
        decode_texture_jobs_with(vec![job], |result| {
            assert_eq!(result.key, "missing-0");
            assert_eq!(result.sampler, sampler);
            assert!(result.image.is_err());
            Ok::<_, ()>(())
        })
        .expect("consumer accepts failures");
    }

    #[test]
    fn consumer_error_cancels_and_joins_workers() {
        let jobs = (0..32).map(missing_job).collect();
        let mut consumed = 0;
        let result = decode_texture_jobs_with(jobs, |_| {
            consumed += 1;
            Err("upload failed")
        });
        assert_eq!(result, Err("upload failed"));
        assert_eq!(consumed, 1);
    }

    #[test]
    fn workers_apply_resolved_options_without_interpreting_names() {
        let path = std::env::temp_dir().join(format!("resolved-decode-{}.png", std::process::id()));
        let source = RgbaImage::from_pixel(2, 2, image::Rgba([17, 83, 149, 255]));
        source.save(&path).expect("write decode fixture");
        let sampler = missing_job(0).sampler;
        let jobs = [false, true]
            .into_iter()
            .map(|grayscale| TextureDecodeJob {
                key: if grayscale {
                    "plain.png"
                } else {
                    "named (grayscale nearest).png"
                }
                .into(),
                path: path.clone(),
                sampler,
                hints: TextureHints {
                    non_default: grayscale,
                    grayscale,
                    ..Default::default()
                },
            })
            .collect();
        let mut consumed = 0;
        decode_texture_jobs_with(jobs, |result| {
            assert_eq!(result.sampler, sampler);
            let image = result.image.expect("decode fixture");
            if result.key == "plain.png" {
                let pixel = image.get_pixel(0, 0).0;
                assert_eq!(pixel[0], pixel[1]);
                assert_eq!(pixel[1], pixel[2]);
                assert_eq!(pixel[3], 255);
            } else {
                assert_eq!(image, source);
            }
            consumed += 1;
            Ok::<_, ()>(())
        })
        .expect("consume decoded images");
        assert_eq!(consumed, 2);
        std::fs::remove_file(path).expect("remove decode fixture");
    }
}
