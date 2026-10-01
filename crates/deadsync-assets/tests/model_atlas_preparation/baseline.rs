// Atlas-building block extracted verbatim from 57ece088f model_animation_source.
// Size/grid locals and Result return are the only harness adaptations.
use super::*;

pub(super) fn model_animation_atlas(
    animation: &ItgTextureAnimation,
    size: [u32; 2],
    grid: [u32; 2],
) -> Result<image::RgbaImage, String> {
    let [width, height] = size;
    let [columns, rows] = grid;
    let atlas_width = width * columns;
    let atlas_height = height * rows;
    let mut atlas = image::RgbaImage::new(atlas_width, atlas_height);
    for (index, frame) in animation.frames.iter().enumerate() {
        let image = assets::open_image_fallback(&frame.path)
            .map_err(|error| error.to_string())?
            .into_rgba8();
        let image = if image.dimensions() == (width, height) {
            image
        } else {
            image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
        };
        image::imageops::replace(
            &mut atlas,
            &image,
            i64::from(index as u32 % columns * width),
            i64::from(index as u32 / columns * height),
        );
    }
    Ok(atlas)
}
