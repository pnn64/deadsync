//! CPU texture upload work shared by creation and retained-frame updates.
use deadlib_render_core::Yuv420Upload;
use image::RgbaImage;
use std::error::Error;

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
fn rgba_pixel(y: u8, u: f32, v: f32, levels: [f32; 4], coeffs: [f32; 4]) -> [u8; 4] {
    let y = (f32::from(y) / 255.0).mul_add(levels[0], levels[1]);
    [
        (coeffs[0].mul_add(v, y).clamp(0.0, 1.0) * 255.0).round() as u8,
        (coeffs[2]
            .mul_add(v, coeffs[1].mul_add(u, y))
            .clamp(0.0, 1.0)
            * 255.0)
            .round() as u8,
        (coeffs[3].mul_add(u, y).clamp(0.0, 1.0) * 255.0).round() as u8,
        255,
    ]
}

fn write_yuv420(upload: Yuv420Upload<'_>, rgba: &mut [u8]) {
    let width = upload.width as usize;
    // YUV420 shares each chroma pair across a 2x2 block. Normalize U/V once
    // for those four pixels, retaining every original fused operation's order.
    for (row_pair, output) in rgba.chunks_exact_mut(width * 8).enumerate() {
        let (top, bottom) = output.split_at_mut(width * 4);
        let y_start = row_pair * width * 2;
        let chroma_start = row_pair * (width / 2);
        for col_pair in 0..width / 2 {
            let chroma = chroma_start + col_pair;
            let u =
                (f32::from(upload.u[chroma]) / 255.0).mul_add(upload.levels[2], upload.levels[3]);
            let v =
                (f32::from(upload.v[chroma]) / 255.0).mul_add(upload.levels[2], upload.levels[3]);
            let col = col_pair * 2;
            for (y_row, out) in [(y_start, &mut *top), (y_start + width, &mut *bottom)] {
                let pixel = col * 4;
                out[pixel..pixel + 4].copy_from_slice(&rgba_pixel(
                    upload.y[y_row + col],
                    u,
                    v,
                    upload.levels,
                    upload.coeffs,
                ));
                out[pixel + 4..pixel + 8].copy_from_slice(&rgba_pixel(
                    upload.y[y_row + col + 1],
                    u,
                    v,
                    upload.levels,
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
    let (groups, tail) = image.as_raw().as_chunks::<16>();
    groups
        .iter()
        .all(|group| u128::from_ne_bytes(*group) & ALPHA_MASK == ALPHA_MASK)
        && tail.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255)
}
