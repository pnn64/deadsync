//! Frozen byte writer from 04e4b89ec, before forcing its closure to inline.
use super::*;

pub(super) fn byte_writer(buffer: &mut [u32]) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
    move |index, color, mode| {
        if mode == BlendMode::Alpha && color[3] >= 1.0 {
            buffer[index] = pack_rgba([color[0], color[1], color[2], 1.0]);
            return;
        }
        if mode == BlendMode::NoEffect {
            // Preserve the original bounds check while skipping color work.
            let _ = buffer[index];
            return;
        }
        let dst = buffer[index];
        let dst = [
            ((dst >> 16) & 255) as f32 * U8_TO_F32,
            ((dst >> 8) & 255) as f32 * U8_TO_F32,
            (dst & 255) as f32 * U8_TO_F32,
            (dst >> 24) as f32 * U8_TO_F32,
        ];
        // Normalized attachments clamp fragment inputs before blending.
        buffer[index] = pack_rgba(blend_color(dst, color.map(clamp01), mode));
    }
}
