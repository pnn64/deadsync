// Frozen pixel paths from ecfd04050aa748c724b182cf6835924ca7baa9da.
use super::*;
pub(super) fn unpack_half(pixel: u64) -> [f32; 4] {
    std::array::from_fn(|i| decode_half((pixel >> (i * 16)) as u16))
}
pub(super) fn blend_color(dst: [f32; 4], src: [f32; 4], mode: BlendMode) -> [f32; 4] {
    let alpha = clamp01(src[3]);
    let inv = 1.0 - alpha;
    let mut color = std::array::from_fn(|i| match mode {
        BlendMode::NoEffect => dst[i],
        BlendMode::Alpha => src[i].mul_add(alpha, dst[i] * inv),
        BlendMode::Add => src[i].mul_add(alpha, dst[i]),
        BlendMode::Multiply => src[i] * dst[i],
        BlendMode::Subtract => dst[i].mul_add(inv, -(src[i] * alpha)),
    });
    color[3] = if mode == BlendMode::NoEffect {
        dst[3]
    } else if mode == BlendMode::Subtract {
        dst[3].mul_add(inv, -src[3])
    } else {
        src[3] + dst[3] * inv
    };
    color
}
pub(super) fn copy_target_pixels<const PRESERVE_ALPHA: bool>(target: &mut OffscreenTarget) {
    if target.with_depth {
        for (source, dest) in target
            .depth
            .chunks_exact(target.viewport[0] as usize)
            .zip(target.depth_image.chunks_exact_mut(target.width as usize))
        {
            dest[..source.len()].copy_from_slice(source);
        }
    }

    if target.float_color {
        for (source, dest) in target
            .half_pixels
            .chunks_exact(target.viewport[0] as usize)
            .zip(
                target
                    .texture
                    .half_pixels
                    .chunks_exact_mut(target.width as usize),
            )
        {
            dest[..source.len()].copy_from_slice(source);
        }
        for (rgba, &pixel) in target
            .texture
            .image
            .as_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(&target.texture.half_pixels)
        {
            *rgba = std::array::from_fn(|i| half_to_byte((pixel >> (i * 16)) as u16));
        }
        return;
    }

    // A full viewport writes every alpha byte below; padding still needs filling.
    if !PRESERVE_ALPHA && target.viewport != [target.width, target.height] {
        for rgba in target.texture.image.as_mut().as_chunks_mut::<4>().0 {
            rgba[3] = 255;
        }
    }
    for (row, pixels) in target
        .texture
        .image
        .as_mut()
        .chunks_exact_mut(target.width as usize * 4)
        .zip(target.pixels.chunks_exact_mut(target.viewport[0] as usize))
    {
        for (rgba, pixel) in row.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            if !PRESERVE_ALPHA {
                *pixel |= 0xff00_0000;
            }
            let pixel = *pixel;
            rgba[0] = (pixel >> 16) as u8;
            rgba[1] = (pixel >> 8) as u8;
            rgba[2] = pixel as u8;
            rgba[3] = (pixel >> 24) as u8;
        }
    }
}
pub(super) fn half_writer(
    buffer: &mut [u64],
    alpha: bool,
) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
    move |index, color, mode| {
        let mut color = blend_color(unpack_half(buffer[index]), color, mode);
        if !alpha {
            color[3] = 1.0;
        }
        buffer[index] = pack_half(color);
    }
}
pub(super) fn pack_half(color: [f32; 4]) -> u64 {
    let [r, g, b, a] = color.map(encode_half).map(u64::from);
    r | (g << 16) | (b << 32) | (a << 48)
}
pub(super) fn decode_half(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = bits & 0x7c00;
    let magnitude = if exponent == 0 {
        // All binary16 subnormals are normal f32 values: scale the integer
        // significand exactly, including zero, instead of renormalizing bits.
        (f32::from(bits & 0x03ff) * (1.0 / 16_777_216.0)).to_bits()
    } else {
        let normal = (u32::from(bits & 0x7fff) << 13) + 0x3800_0000;
        normal | if exponent == 0x7c00 { 0x7f80_0000 } else { 0 }
    };
    f32::from_bits(sign | magnitude)
}
pub(super) fn encode_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let magnitude = bits & 0x7fff_ffff;
    // One unsigned check covers normal binary16 exponents: [2^-14, 2^16).
    if magnitude.wrapping_sub(0x3880_0000) < 0x0f00_0000 {
        let rounded = magnitude + 0x0fff + ((magnitude >> 13) & 1);
        return sign | ((rounded >> 13) - 0x1c000) as u16;
    }
    if magnitude >= 0x4780_0000 {
        return sign | 0x7c00 | if magnitude > 0x7f80_0000 { 0x0200 } else { 0 };
    }
    // Below 2^-25, even rounding cannot reach a subnormal.
    if magnitude < 0x3300_0000 {
        return sign;
    }
    let mantissa = (magnitude & 0x007f_ffff) | 0x0080_0000;
    let shift = 126 - (magnitude >> 23);
    let rounded = mantissa + ((1 << (shift - 1)) - 1) + ((mantissa >> shift) & 1);
    sign | (rounded >> shift) as u16
}
pub(super) fn half_to_byte(bits: u16) -> u8 {
    // Negative values and NaNs cast to zero after clamping; +infinity saturates.
    if bits > 0x7c00 {
        return 0;
    }
    if bits >= 0x3c00 {
        return 255;
    }
    // Values below 2^-9 cannot round up to one byte, including all subnormals.
    if bits < 0x1800 {
        return 0;
    }
    // The 11-bit significand times 255 is exact in f32. Round that integer
    // product directly, removing the float decode, clamp, scale and round.
    let scaled = (u32::from(bits & 0x03ff) | 0x0400) * 255;
    let shift = 25 - (bits >> 10);
    ((scaled + (1 << (shift - 1))) >> shift) as u8
}
pub(super) fn byte_writer(buffer: &mut [u32]) -> impl FnMut(usize, [f32; 4], BlendMode) + '_ {
    move |index, color, mode| {
        if mode == BlendMode::Alpha && color[3] >= 1.0 {
            buffer[index] = pack_rgba([color[0], color[1], color[2], 1.0]);
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
