use deadlib_render_core::SamplerDesc;
use deadsync_online::arrowcloud::ArrowCloudResultDialogDownload;
use deadsync_profile as profile_data;
use image::{DynamicImage, ImageReader, Limits};
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread;

const RESULT_DIALOG_QUEUE_CAPACITY: usize = 2;
/// Bounds transient decoder allocations to at most one 4096x4096 RGBA image.
const RESULT_DIALOG_MAX_DECODE_DIMENSION: u32 = 4096;
const RESULT_DIALOG_MAX_DECODE_ALLOCATION_BYTES: u64 = 96 * 1024 * 1024;
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
    pub texture_keys: Box<[Arc<str>]>,
}

#[derive(Debug)]
pub(super) struct Service {
    request_tx: SyncSender<ArrowCloudResultDialogDownload>,
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
            .name("arrowcloud-result-decode".to_string())
            .spawn(move || {
                while let Ok(download) = request_rx.recv() {
                    if let Some(ready) = decode_download(download) {
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
            log::warn!("Could not start the ArrowCloud result decoder: {error}.");
        }
        Self {
            request_tx,
            ready_rx,
            ready_pending,
        }
    }

    pub(super) fn submit(&self, download: ArrowCloudResultDialogDownload) {
        match self.request_tx.try_send(download) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                log::warn!(
                    "Dropped an ArrowCloud result dialog because the decoder queue is full."
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                log::warn!(
                    "Could not decode an ArrowCloud result dialog because its worker stopped."
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

fn decode_download(download: ArrowCloudResultDialogDownload) -> Option<ReadyResultDialog> {
    let side_index = profile_data::player_side_index(download.side);
    let mut texture_keys = Vec::with_capacity(download.images.len());
    for (image_index, bytes) in download.images.into_vec().into_iter().enumerate() {
        let Some(key) = RESULT_DIALOG_TEXTURE_KEYS
            .get(side_index)
            .and_then(|keys| keys.get(image_index))
        else {
            break;
        };
        match decode_image(bytes.as_ref()) {
            Ok(image) => {
                deadlib_assets::register_generated_texture(
                    key,
                    image.into_rgba8(),
                    SamplerDesc::default(),
                );
                texture_keys.push(Arc::from(*key));
            }
            Err(error) => log::warn!(
                "ArrowCloud result dialog image {} could not be decoded: {error}.",
                image_index + 1
            ),
        }
    }
    if texture_keys.is_empty() {
        return None;
    }
    Some(ReadyResultDialog {
        side: download.side,
        chart_hash: download.chart_hash,
        token: download.token,
        texture_keys: texture_keys.into_boxed_slice(),
    })
}

fn decode_image(bytes: &[u8]) -> Result<DynamicImage, image::ImageError> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(RESULT_DIALOG_MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(RESULT_DIALOG_MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(RESULT_DIALOG_MAX_DECODE_ALLOCATION_BYTES);
    reader.limits(limits);
    let image = reader.decode()?;
    if image.width() > RESULT_DIALOG_MAX_TEXTURE_DIMENSION
        || image.height() > RESULT_DIALOG_MAX_TEXTURE_DIMENSION
    {
        Ok(image.thumbnail(
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
    use image::{ImageFormat, Rgba, RgbaImage};
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
