use image::RgbaImage;
use log::{debug, warn};
use std::{collections::VecDeque, sync::mpsc, time::Instant};

pub(super) struct CaptureTarget {
    texture: wgpu::Texture,
}

impl CaptureTarget {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }
}

pub(super) struct RenderTarget<'a> {
    texture: &'a wgpu::Texture,
    view: wgpu::TextureView,
}

impl RenderTarget<'_> {
    pub(super) fn texture(&self) -> &wgpu::Texture {
        self.texture
    }

    pub(super) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }
}

pub(super) fn render_target<'a>(
    capture: Option<&'a CaptureTarget>,
    surface: Option<&'a wgpu::SurfaceTexture>,
) -> RenderTarget<'a> {
    let texture = capture
        .map(CaptureTarget::texture)
        .or_else(|| surface.map(|frame| &frame.texture))
        .expect("surface or screenshot target is available");
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    RenderTarget { texture, view }
}

pub(super) struct Readback {
    buffer: wgpu::Buffer,
    width: usize,
    height: usize,
    padded_row_bytes: usize,
    format: wgpu::TextureFormat,
}

pub(super) struct PendingReadback {
    readback: Readback,
    receiver: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
    submission_index: wgpu::SubmissionIndex,
}

pub(super) struct OfflineScreenshot {
    requested: bool,
    pending_readbacks: VecDeque<PendingReadback>,
    readback_buffers: Vec<wgpu::Buffer>,
    captured_frame: Option<RgbaImage>,
}

impl OfflineScreenshot {
    pub(super) fn new() -> Self {
        Self {
            requested: false,
            pending_readbacks: VecDeque::with_capacity(2),
            readback_buffers: Vec::with_capacity(2),
            captured_frame: None,
        }
    }

    pub(super) fn request(&mut self, requested: bool) {
        self.requested = requested;
    }

    pub(super) fn take_target(
        &mut self,
        requested: bool,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> Option<CaptureTarget> {
        if !requested {
            return None;
        }
        if !std::mem::take(&mut self.requested) {
            return None;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wgpu screenshot target"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Some(CaptureTarget { texture })
    }

    pub(super) fn encode_readback(
        &mut self,
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        requested: bool,
    ) -> Option<Readback> {
        if !requested {
            return None;
        }
        let format = config.format;
        debug!(
            "wgpu screenshot: format={:?} size={}x{}",
            format, config.width, config.height
        );
        let width = config.width.max(1);
        let height = config.height.max(1);
        let bytes_per_row = 4 * width;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row_bytes = bytes_per_row.div_ceil(align) * align;
        let readback_size = u64::from(padded_row_bytes) * u64::from(height);
        let buffer = self
            .readback_buffers
            .pop()
            .filter(|buffer| buffer.size() >= readback_size)
            .unwrap_or_else(|| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("wgpu screenshot readback"),
                    size: readback_size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Some(Readback {
            buffer,
            width: width as usize,
            height: height as usize,
            padded_row_bytes: padded_row_bytes as usize,
            format,
        })
    }

    pub(super) fn submit_readback(
        &mut self,
        device: &wgpu::Device,
        readback: Option<Readback>,
        submission_index: wgpu::SubmissionIndex,
        offscreen_capture: bool,
    ) -> (u32, bool) {
        let Some(readback) = readback else {
            return (0, false);
        };
        let pending_readback = start_readback(readback, submission_index);
        if offscreen_capture {
            self.pending_readbacks.push_back(pending_readback);
            (0, false)
        } else {
            self.finish_readback(device, pending_readback)
        }
    }

    pub(super) fn capture_frame(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<RgbaImage, std::io::Error> {
        if let Some(readback) = self.pending_readbacks.pop_front() {
            self.finish_readback(device, readback);
        }
        self.captured_frame
            .take()
            .ok_or_else(|| std::io::Error::other("No captured screenshot frame available"))
    }

    fn finish_readback(&mut self, device: &wgpu::Device, pending: PendingReadback) -> (u32, bool) {
        let PendingReadback {
            readback,
            receiver,
            submission_index,
        } = pending;
        let slice = readback.buffer.slice(..);
        let wait_started = Instant::now();
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission_index),
            timeout: None,
        });
        let elapsed = wait_started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
        let captured_frame = if receiver.recv().is_ok_and(|result| result.is_ok()) {
            match slice.get_mapped_range() {
                Ok(data) => {
                    let row_bytes = readback.width * 4;
                    let mut rgba = vec![0u8; row_bytes * readback.height];
                    let swap_rb = matches!(
                        readback.format,
                        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                    );
                    for y in 0..readback.height {
                        let src = y * readback.padded_row_bytes;
                        // Copy rows are already top-to-bottom.
                        let dst = y * row_bytes;
                        if swap_rb {
                            for x in 0..readback.width {
                                let s = src + x * 4;
                                let d = dst + x * 4;
                                rgba[d] = data[s + 2];
                                rgba[d + 1] = data[s + 1];
                                rgba[d + 2] = data[s];
                                rgba[d + 3] = data[s + 3];
                            }
                        } else {
                            rgba[dst..dst + row_bytes].copy_from_slice(&data[src..src + row_bytes]);
                        }
                    }
                    drop(data);
                    RgbaImage::from_raw(readback.width as u32, readback.height as u32, rgba)
                }
                Err(error) => {
                    warn!("wgpu screenshot readback failed: {error}");
                    None
                }
            }
        } else {
            warn!("wgpu screenshot readback failed: map_async returned error");
            None
        };
        readback.buffer.unmap();
        self.readback_buffers.push(readback.buffer);
        self.captured_frame = captured_frame;
        (elapsed, elapsed != 0)
    }
}

fn start_readback(readback: Readback, submission_index: wgpu::SubmissionIndex) -> PendingReadback {
    let slice = readback.buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    PendingReadback {
        readback,
        receiver: rx,
        submission_index,
    }
}
