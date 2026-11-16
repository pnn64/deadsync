use super::*;


#[test]
fn wide_half_resize_preserves_float_coordinate_failure() {
    // Above 2^23, the native f32 center +/- 0.5 can span different texels.
    let width = (1 << 23) + 2;
    let input = RgbaImage::from_fn(width, 2, |x, y| {
        image::Rgba((x.wrapping_mul(1664525) ^ y.wrapping_mul(1013904223)).to_le_bytes())
    });
    // The original coordinates step past the last texel for this width.
    // Preserve that existing failure rather than silently changing its pixels.
    assert!(std::panic::catch_unwind(|| zoom_image(input, [width / 2, 1])).is_err());
}
