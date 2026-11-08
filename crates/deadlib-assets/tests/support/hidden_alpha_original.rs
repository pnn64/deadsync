// Frozen from 241a16adc for differential checks and paired benchmarks.
use image::RgbaImage;
pub fn fix_hidden_alpha(image: &mut RgbaImage) {
    // RageSurfaceUtils::FindAlphaRGB defaults to black when the surface has
    // no visible pixel; native SetAlphaRGB still clears its hidden RGB.
    let first = image
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .find(|pixel| pixel[3] != 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .unwrap_or([0; 3]);
    let last = image
        .as_raw()
        .as_chunks::<4>()
        .0
        .iter()
        .rev()
        .find(|pixel| pixel[3] != 0)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .unwrap_or([0; 3]);
    let [r, g, b] = if first == last { first } else { [0, 0, 0] };
    for pixel in image.as_mut().as_chunks_mut::<4>().0 {
        if pixel[3] == 0 {
            pixel[0] = r;
            pixel[1] = g;
            pixel[2] = b;
        }
    }
}
