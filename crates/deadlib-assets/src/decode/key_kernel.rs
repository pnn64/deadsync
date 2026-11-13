use super::*;


#[test]
fn key_preserves_edges_alpha_and_trailing_storage() {
    for width in [0, 1, 3, 17, 64] {
        for height in [0, 1, 2, 7] {
            for off_pink in [false, true] {
                let palette = [
                    [255, 0, 255, 255],
                    [248, 0, 248, 255],
                    [255, 0, 255, 254],
                    [248, 0, 248, 0],
                    [12, 34, 56, 78],
                ];
                let mut image = RgbaImage::from_fn(width, height, |x, y| {
                    image::Rgba(palette[((x + y * width) % 5) as usize])
                });
                if off_pink && width > 0 && height > 0 {
                    image.put_pixel(width - 1, height - 1, image::Rgba(palette[1]));
                }
                let uses_off_pink = image
                    .enumerate_pixels()
                    .any(|(_, y, p)| (y == 0 || y + 1 == height) && p.0 == palette[1]);
                let key = palette[usize::from(uses_off_pink)];
                let expected: Vec<_> = image
                    .pixels()
                    .flat_map(|p| if p.0 == key { [0; 4] } else { p.0 })
                    .chain([99; 12])
                    .collect();
                let mut bytes = image.into_raw();
                bytes.extend([99; 12]);
                let mut image = RgbaImage::from_raw(width, height, bytes).unwrap();
                key_rgba(&mut image);
                assert_eq!(image.into_raw(), expected);
            }
        }
    }
}
