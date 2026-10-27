// Frozen from 55ca4c72e for exact-output comparisons.
#[inline]
fn original_opaque(image: &RgbaImage) -> bool {
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
