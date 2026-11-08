use super::*;

struct DecodedGif {
    images: Vec<RgbaImage>,
    durations: Vec<f32>,
}

/// Decode GIF bytes into RGBA frames with per-frame durations. Mirrors the
/// SDK: a delay of 0 or anything in 28..=42ms snaps to exactly 1/30s, else
/// the GIF's own delay is kept.
fn decode_gif(data: &[u8]) -> Result<DecodedGif, &'static str> {
    let decoder = GifDecoder::new(Cursor::new(data)).map_err(|_| "the GIF couldn't be read")?;
    let mut images = Vec::new();
    let mut durations = Vec::new();
    for frame in decoder.into_frames().filter_map(std::result::Result::ok) {
        let (numer, denom) = frame.delay().numer_denom_ms();
        let ms = numer as f32 / denom as f32;
        durations.push(if ms <= 0.0 || (28.0..=42.0).contains(&ms) {
            1.0 / 30.0
        } else {
            ms / 1000.0
        });
        images.push(frame.into_buffer());
    }
    if images.is_empty() {
        return Err("the GIF has no frames");
    }
    Ok(DecodedGif { images, durations })
}

/// First frame whose marker pixel at `(x, marker_y)` is white-ish (alpha 255,
/// R >= 128). The marker row carries one flag pixel per column: x 0 is the
/// loop start, x 1 the loop end.
fn marked_frame(images: &[RgbaImage], x: u32, marker_y: u32) -> Option<usize> {
    images.iter().position(|img| {
        let px = img.get_pixel(x, marker_y);
        px[3] == 255 && px[0] >= 128
    })
}

pub fn decode_full_pad(data: &[u8]) -> Result<(FullPadAnim, PadSize), &'static str> {
    let gif = decode_gif(data)?;
    let first = &gif.images[0];
    let (w, h) = (first.width(), first.height());
    let size = full_pad_size(w, h).ok_or("a full-pad GIF must be 23x24 or 14x15")?;
    let loop_frame = marked_frame(&gif.images, 0, h - 1).unwrap_or(0);
    let mut panels: [Vec<PanelFrame>; PANELS] =
        std::array::from_fn(|_| Vec::with_capacity(gif.images.len()));
    for img in &gif.images {
        for (panel, frames) in panels.iter_mut().enumerate() {
            frames.push(sample_full_pad_panel(img, panel, size));
        }
    }
    Ok((
        FullPadAnim {
            panels,
            durations: gif.durations,
            loop_frame,
            beats_per_loop: None,
        },
        size,
    ))
}

/// Decode a per-panel judgement GIF (7x8, 7x7, 4x5, or 4x4). Bare 7x7 / 4x4
/// canvases have no marker row, so they loop from frame 0 with no outro.
pub fn decode_panel(data: &[u8]) -> Result<(PanelAnim, PadSize), &'static str> {
    let gif = decode_gif(data)?;
    let first = &gif.images[0];
    let (w, h) = (first.width(), first.height());
    let (size, has_marker_row) =
        panel_canvas(w, h).ok_or("a per-panel GIF must be 7x8, 7x7, 4x5, or 4x4")?;
    let (loop_frame, loop_end) = if has_marker_row {
        let loop_frame = marked_frame(&gif.images, 0, h - 1).unwrap_or(0);
        // A loop end marked before the loop start is author error; ignore it.
        let loop_end = marked_frame(&gif.images, 1, h - 1)
            .filter(|&end| end >= loop_frame)
            .unwrap_or(gif.images.len() - 1);
        (loop_frame, loop_end)
    } else {
        (0, gif.images.len() - 1)
    };
    let frames = gif
        .images
        .iter()
        .map(|img| match size {
            PadSize::Leds16 => sample_block_16(img, 0, 0),
            PadSize::Leds25 => sample_block_25(img, 0, 0),
        })
        .collect();
    Ok((
        PanelAnim {
            frames,
            durations: gif.durations,
            loop_frame,
            loop_end,
        },
        size,
    ))
}
