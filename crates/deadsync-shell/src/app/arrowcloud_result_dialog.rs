use deadlib_render_core::SamplerDesc;
use deadsync_online::arrowcloud::ArrowCloudResultDialogRequest;
use deadsync_profile as profile_data;
use deadlib_image::{RgbaImage, imageops};
use deadlib_image_io::Limits;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread;

const RESULT_DIALOG_QUEUE_CAPACITY: usize = 2;
/// Bounds transient decoder allocations to at most one 4096x4096 RGBA image.
const RESULT_DIALOG_MAX_DECODE_DIMENSION: u32 = 4096;
const RESULT_DIALOG_MAX_DECODE_ALLOCATION_BYTES: usize = 96 * 1024 * 1024;
/// Generated textures are capped at four MiB each, matching the main-frame
/// upload budget's intended bounded work.
const RESULT_DIALOG_MAX_TEXTURE_DIMENSION: u32 = 1024;
const RESULT_DIALOG_TEXTURE_KEYS: [[&str; 4]; 2] = [
    [
        "generated/arrowcloud-result-dialog-p1-0",
        "generated/arrowcloud-result-dialog-p1-1",
        "generated/arrowcloud-result-dialog-p1-2",
        "generated/arrowcloud-result-dialog-p1-3",
    ],
    [
        "generated/arrowcloud-result-dialog-p2-0",
        "generated/arrowcloud-result-dialog-p2-1",
        "generated/arrowcloud-result-dialog-p2-2",
        "generated/arrowcloud-result-dialog-p2-3",
    ],
];

#[derive(Debug)]
pub(super) struct ReadyResultDialog {
    pub side: profile_data::PlayerSide,
    pub chart_hash: Box<str>,
    pub token: u64,
    textures: Vec<(&'static str, RgbaImage)>,
}

impl ReadyResultDialog {
    pub(super) fn register_textures(self) -> Box<[Arc<str>]> {
        self.textures
            .into_iter()
            .map(|(key, image)| {
                deadlib_assets::register_generated_texture(key, image, SamplerDesc::default());
                Arc::from(key)
            })
            .collect()
    }
}

#[derive(Debug)]
pub(super) struct Service {
    request_tx: SyncSender<ArrowCloudResultDialogRequest>,
    ready_rx: Receiver<ReadyResultDialog>,
    ready_pending: Arc<AtomicBool>,
}

impl Service {
    pub(super) fn new() -> Self {
        let (request_tx, request_rx) = sync_channel(RESULT_DIALOG_QUEUE_CAPACITY);
        let (ready_tx, ready_rx) = sync_channel(RESULT_DIALOG_QUEUE_CAPACITY);
        let ready_pending = Arc::new(AtomicBool::new(false));
        let worker_pending = Arc::clone(&ready_pending);
        let spawn = thread::Builder::new()
            .name("arrowcloud-result-media".to_string())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    if let Some(ready) = download_and_decode(request) {
                        match ready_tx.try_send(ready) {
                            Ok(()) => worker_pending.store(true, Ordering::Release),
                            Err(TrySendError::Full(_)) => {
                                log::warn!("Dropped an ArrowCloud result dialog because the ready queue is full.");
                            }
                            Err(TrySendError::Disconnected(_)) => break,
                        }
                    }
                }
            });
        if let Err(error) = spawn {
            log::warn!("Could not start the ArrowCloud result media worker: {error}.");
        }
        Self {
            request_tx,
            ready_rx,
            ready_pending,
        }
    }

    pub(super) fn submit(&self, request: ArrowCloudResultDialogRequest) {
        match self.request_tx.try_send(request) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                log::warn!(
                    "Dropped an ArrowCloud result dialog because the media queue is full."
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                log::warn!(
                    "Could not prepare an ArrowCloud result dialog because its worker stopped."
                );
            }
        }
    }

    /// Drains ready values into app-owned scratch storage. The atomic gate
    /// avoids touching the channel on stable frames.
    pub(super) fn drain_ready(&self, out: &mut Vec<ReadyResultDialog>) {
        if !self.ready_pending.swap(false, Ordering::AcqRel) {
            return;
        }
        while let Ok(ready) = self.ready_rx.try_recv() {
            out.push(ready);
        }
    }
}

fn download_and_decode(request: ArrowCloudResultDialogRequest) -> Option<ReadyResultDialog> {
    let side_index = profile_data::player_side_index(request.side);
    if !deadsync_score::arrowcloud_submit_ui_token_matches(
        side_index,
        &request.chart_hash,
        request.token,
    ) {
        return None;
    }
    let keys = RESULT_DIALOG_TEXTURE_KEYS.get(side_index)?;
    let images = request.download_images();
    let mut textures = Vec::with_capacity(images.len());
    for (image_index, (&key, bytes)) in keys.iter().zip(images.into_vec()).enumerate() {
        match decode_image(bytes.as_ref()) {
            Ok(image) => textures.push((key, image)),
            Err(error) => log::warn!(
                "ArrowCloud result dialog image {} could not be decoded: {error}.",
                image_index + 1
            ),
        }
    }
    if textures.is_empty() {
        return None;
    }
    Some(ReadyResultDialog {
        side: request.side,
        chart_hash: request.chart_hash,
        token: request.token,
        textures,
    })
}

fn decode_image(bytes: &[u8]) -> Result<RgbaImage, deadlib_image_io::ImageError> {
    let limits = Limits {
        max_width: RESULT_DIALOG_MAX_DECODE_DIMENSION,
        max_height: RESULT_DIALOG_MAX_DECODE_DIMENSION,
        max_bytes: RESULT_DIALOG_MAX_DECODE_ALLOCATION_BYTES,
    };
    let image = deadlib_image_io::load_with_limits(bytes, limits)?;
    if image.width() > RESULT_DIALOG_MAX_TEXTURE_DIMENSION
        || image.height() > RESULT_DIALOG_MAX_TEXTURE_DIMENSION
    {
        Ok(imageops::thumbnail_fit(
            &image,
            RESULT_DIALOG_MAX_TEXTURE_DIMENSION,
            RESULT_DIALOG_MAX_TEXTURE_DIMENSION,
        ))
    } else {
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    #[test]
    fn decode_image_preserves_small_images_and_bounds_large_output() {
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 8, Rgba([1, 2, 3, 255])))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        let decoded = decode_image(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (16, 8));

        bytes.clear();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2048, 1024, Rgba([1, 2, 3, 255])))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        let decoded = decode_image(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1024, 512));
    }
}
