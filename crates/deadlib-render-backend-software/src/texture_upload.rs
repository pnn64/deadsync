//! CPU texture upload work shared by creation and retained-frame updates.
use deadlib_render_core::Yuv420Upload;
use image::RgbaImage;
use std::error::Error;

// Preserve division's rounding exactly; multiplying by 1/255 can differ by one ULP.
const NORMALIZED_U8: [f32; 256] = {
    let mut values = [0.0; 256];
    let mut i = 0;
    while i < values.len() {
        values[i] = i as f32 / 255.0;
        i += 1;
    }
    values
};

pub(super) fn yuv420_to_rgba(upload: Yuv420Upload<'_>) -> Result<RgbaImage, Box<dyn Error>> {
    if !upload.is_valid() {
        return Err(std::io::Error::other("invalid YUV420 planes").into());
    }
    let mut rgba = vec![0; upload.width as usize * upload.height as usize * 4];
    write_yuv420(upload, &mut rgba);
    RgbaImage::from_raw(upload.width, upload.height, rgba)
        .ok_or_else(|| std::io::Error::other("invalid converted YUV420 image").into())
}

pub(super) fn update_yuv420_image(
    image: &mut RgbaImage,
    upload: Yuv420Upload<'_>,
) -> Result<(), Box<dyn Error>> {
    // Validate before touching the current frame, including on resize failures.
    if !upload.is_valid() {
        return Err(std::io::Error::other("invalid YUV420 planes").into());
    }
    if image.dimensions() == (upload.width, upload.height)
        && image.as_raw().len() == upload.width as usize * upload.height as usize * 4
    {
        write_yuv420(upload, image.as_mut());
    } else {
        *image = yuv420_to_rgba(upload)?;
    }
    Ok(())
}

#[inline]
fn rgba_pixel(y: f32, u: f32, v: f32, coeffs: [f32; 4]) -> [u8; 4] {
    // Widen the rounded f32 product before adding the half: f32 addition can
    // otherwise round a value just below a half-integer across the boundary.
    // The u8 cast already saturates negative values, infinities and NaNs.
    [
        (f64::from(coeffs[0].mul_add(v, y) * 255.0) + 0.5) as u8,
        (f64::from(coeffs[2].mul_add(v, coeffs[1].mul_add(u, y)) * 255.0) + 0.5) as u8,
        (f64::from(coeffs[3].mul_add(u, y) * 255.0) + 0.5) as u8,
        255,
    ]
}

fn write_yuv420(upload: Yuv420Upload<'_>, rgba: &mut [u8]) {
    let width = upload.width as usize;
    // Preparing 512 values pays off for video frames, but not tiny textures.
    // These per-upload stack tables retain the exact division/FMA order.
    let prepare = rgba.len() >= 4096;
    let luma =
        prepare.then(|| NORMALIZED_U8.map(|y| y.mul_add(upload.levels[0], upload.levels[1])));
    let chroma =
        prepare.then(|| NORMALIZED_U8.map(|uv| uv.mul_add(upload.levels[2], upload.levels[3])));
    let y_value = |byte: u8| {
        luma.as_ref().map_or_else(
            || NORMALIZED_U8[byte as usize].mul_add(upload.levels[0], upload.levels[1]),
            |values| values[byte as usize],
        )
    };
    let uv_value = |byte: u8| {
        chroma.as_ref().map_or_else(
            || NORMALIZED_U8[byte as usize].mul_add(upload.levels[2], upload.levels[3]),
            |values| values[byte as usize],
        )
    };
    // YUV420 shares each chroma pair across a 2x2 block.
    for (row_pair, output) in rgba.chunks_exact_mut(width * 8).enumerate() {
        let (top, bottom) = output.split_at_mut(width * 4);
        let y_start = row_pair * width * 2;
        let chroma_start = row_pair * (width / 2);
        for col_pair in 0..width / 2 {
            let index = chroma_start + col_pair;
            let u = uv_value(upload.u[index]);
            let v = uv_value(upload.v[index]);
            let col = col_pair * 2;
            for (y_row, out) in [(y_start, &mut *top), (y_start + width, &mut *bottom)] {
                let pixel = col * 4;
                out[pixel..pixel + 4].copy_from_slice(&rgba_pixel(
                    y_value(upload.y[y_row + col]),
                    u,
                    v,
                    upload.coeffs,
                ));
                out[pixel + 4..pixel + 8].copy_from_slice(&rgba_pixel(
                    y_value(upload.y[y_row + col + 1]),
                    u,
                    v,
                    upload.coeffs,
                ));
            }
        }
    }
}

#[inline]
pub(super) fn texture_is_opaque(image: &RgbaImage) -> bool {
    if image.width() == 0 || image.height() == 0 {
        return false;
    }
    const ALPHA_MASK: u128 =
        u128::from_ne_bytes([0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]);
    // Reduce four words before branching, retaining an immediate first-pixel exit.
    let bytes = image.as_raw();
    if bytes[3] != 255 {
        return false;
    }
    let (blocks, tail) = bytes.as_chunks::<64>();
    blocks.iter().all(|block| {
        let alpha = block
            .as_chunks::<16>()
            .0
            .iter()
            .fold(u128::MAX, |alpha, word| alpha & u128::from_ne_bytes(*word));
        alpha & ALPHA_MASK == ALPHA_MASK
    }) && tail.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255)
}

#[cfg(test)]
#[path = "opacity_kernel.rs"]
mod opacity_kernel;

#[cfg(test)]
#[path = "yuv_quantization.rs"]
mod yuv_quantization;
