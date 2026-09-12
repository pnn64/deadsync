// Frozen from 0dfc476f4 (0.5.1143); visibility widened for comparisons.
use deadlib_render_core::Yuv420Upload;
use image::RgbaImage;
use std::error::Error;

pub fn yuv420_to_rgba(upload: Yuv420Upload<'_>) -> Result<RgbaImage, Box<dyn Error>> {
    if !upload.is_valid() {
        return Err(std::io::Error::other("invalid YUV420 planes").into());
    }

    let width = upload.width as usize;
    let height = upload.height as usize;
    let luma_len = width * height;

    let mut rgba = vec![0; luma_len * 4];
    for row in 0..height {
        let chroma_row = row / 2 * (width / 2);
        for col in 0..width {
            let pixel = row * width + col;
            let chroma = chroma_row + col / 2;
            let y =
                (f32::from(upload.y[pixel]) / 255.0).mul_add(upload.levels[0], upload.levels[1]);
            let u =
                (f32::from(upload.u[chroma]) / 255.0).mul_add(upload.levels[2], upload.levels[3]);
            let v =
                (f32::from(upload.v[chroma]) / 255.0).mul_add(upload.levels[2], upload.levels[3]);
            let out = &mut rgba[pixel * 4..pixel * 4 + 4];
            out[0] = (upload.coeffs[0].mul_add(v, y).clamp(0.0, 1.0) * 255.0).round() as u8;
            out[1] = (upload.coeffs[2]
                .mul_add(v, upload.coeffs[1].mul_add(u, y))
                .clamp(0.0, 1.0)
                * 255.0)
                .round() as u8;
            out[2] = (upload.coeffs[3].mul_add(u, y).clamp(0.0, 1.0) * 255.0).round() as u8;
            out[3] = 255;
        }
    }
    RgbaImage::from_raw(upload.width, upload.height, rgba)
        .ok_or_else(|| std::io::Error::other("invalid converted YUV420 image").into())
}

#[inline]
pub fn texture_is_opaque(image: &RgbaImage) -> bool {
    image.width() != 0
        && image.height() != 0
        && image
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255)
}
