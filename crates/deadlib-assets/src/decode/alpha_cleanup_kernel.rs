//! Preparation-only kernel derived from decode_texture after resize.
use super::*;
pub(super) fn finish(image: &mut RgbaImage, hints: &TextureHints) {
    if !hints.is_default() {
        apply_texture_hints(image, hints);
    }
    // Native cleanup follows color keying and the final resize, including
    // sources that gain an alpha channel during preparation.
    if !hints.is_default() && hints.alphamap {
        // Alphamaps have white RGB in every complete pixel. Only an entirely
        // transparent result needs the native hidden-color cleanup.
        if !image
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] != 0)
        {
            image.as_mut().as_chunks_mut::<4>().0.fill([0; 4]);
        }
    } else {
        fix_hidden_alpha(image);
    }
}
