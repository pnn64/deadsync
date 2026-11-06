use super::State;
use image::RgbaImage;
use log::{debug, warn};
use std::{sync::mpsc, time::Instant};

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

pub(super) fn take_target(state: &mut State) -> Option<CaptureTarget> {
    if !state.screenshot_requested {
        return None;
    }
    let offscreen_requested = state.screenshot_offscreen_requested;
    state.screenshot_requested = false;
    state.screenshot_offscreen_requested = false;
    if !offscreen_requested {
        return None;
    }
    let texture = state.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wgpu screenshot target"),
        size: wgpu::Extent3d {
            width: state.config.width.max(1),
            height: state.config.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: state.config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    Some(CaptureTarget { texture })
}

pub(super) fn encode_readback(
    state: &State,
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
    requested: bool,
) -> Option<Readback> {
    if !requested {
        return None;
    }
    let format = state.config.format;
    debug!(
        "wgpu screenshot: format={:?} size={}x{}",
        format, state.config.width, state.config.height
    );
    let width = state.config.width.max(1);
    let height = state.config.height.max(1);
    let bytes_per_row = 4 * width;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_row_bytes = bytes_per_row.div_ceil(align) * align;
    let readback_size = u64::from(padded_row_bytes) * u64::from(height);
    let buffer = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu screenshot readback"),
        size: readback_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
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

pub(super) fn finish_readback(state: &mut State, readback: Option<Readback>) -> (u32, bool) {
    let Some(readback) = readback else {
        return (0, false);
    };
    let slice = readback.buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let wait_started = Instant::now();
    let _ = state.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let elapsed = wait_started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
    if rx.recv().is_ok_and(|result| result.is_ok()) {
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
                readback.buffer.unmap();
                state.captured_frame =
                    RgbaImage::from_raw(readback.width as u32, readback.height as u32, rgba);
            }
            Err(error) => {
                readback.buffer.unmap();
                state.captured_frame = None;
                warn!("wgpu screenshot readback failed: {error}");
            }
        }
    } else {
        readback.buffer.unmap();
        state.captured_frame = None;
        warn!("wgpu screenshot readback failed: map_async returned error");
    }
    (elapsed, elapsed != 0)
}
