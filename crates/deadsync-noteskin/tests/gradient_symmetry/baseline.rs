// Frozen from ea7f72dae (0.5.1656); unchanged color/profile helpers are shared.
use super::*;

#[must_use]
pub(super) fn mine_gradient_texture(colors: &[[f32; 4]]) -> RgbaImage {
    assert!(
        !colors.is_empty(),
        "mine gradient requires at least one color"
    );
    let frame_count = colors.len();
    let frame_size = MINE_GRADIENT_FRAME_SIZE.max(2);
    let mut image = RgbaImage::new(frame_size * frame_count as u32, frame_size);
    let colors = mine_gradient_colors(colors);
    let profile = &*MINE_GRADIENT_PROFILE;
    for frame in 0..frame_count {
        let x_offset = frame as u32 * frame_size;
        // Rotation depends on frame and radial layer, not the individual pixel.
        let layer_colors: [MineGradientColor; MINE_FILL_LAYERS] = std::array::from_fn(|layer| {
            colors[(frame + colors.len() - (layer % colors.len())) % colors.len()]
        });
        for y in 0..frame_size {
            for x in 0..frame_size {
                let profile_index = y as usize * frame_size as usize + x as usize;
                let layer = profile.layers[profile_index];
                if layer == MINE_GRADIENT_OUTSIDE_LAYER {
                    continue;
                }
                image.put_pixel(
                    x_offset + x,
                    y,
                    Rgba(mine_gradient_pixel(
                        layer_colors[usize::from(layer)],
                        profile.edge_alpha[profile_index],
                    )),
                );
            }
        }
    }

    image
}
