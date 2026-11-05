//! Frozen texture preparation from 294b5139a0b28c46973abcbd1a617431ea4c1fac.
use super::*;
pub fn decode_texture(path: &Path, hints: &TextureHints) -> image::ImageResult<DecodedTexture> {
    let mut image = decode_image(path, hints.hot_pink_color_key)?;
    let source_size = [image.width(), image.height()];
    let size = texture_image_size(source_size, hints)?;
    if size != [image.width(), image.height()] {
        image = zoom_image(image, size);
    }
    if !hints.is_default() {
        apply_texture_hints(&mut image, hints);
    }
    // Native cleanup follows color keying and the final resize, including
    // sources that gain an alpha channel during preparation.
    fix_hidden_alpha(&mut image);
    Ok(DecodedTexture { image, source_size })
}
pub(super) fn finish(image: &mut RgbaImage, hints: &TextureHints) {
    if !hints.is_default() {
        apply_texture_hints(image, hints);
    }
    // Native cleanup follows color keying and the final resize, including
    // sources that gain an alpha channel during preparation.
    fix_hidden_alpha(image);
}
