use super::*;

#[test]
fn default_png_decodes_from_a_nonseekable_reader() {
    let pixels = [255, 0, 255, 255, 12, 34, 56, 78];
    let mut encoded = Vec::new();
    deadlib_image_png::encode_rgba8_writer(2, 1, &pixels, &Default::default(), &mut encoded)
        .unwrap();
    let mut input = encoded.as_slice();
    assert_eq!(file_format(&mut input, None).unwrap(), Format::Png);
    let decoded = decode_reader(&mut input, Format::Png, Use::Artwork, None).unwrap();
    assert_eq!(decoded.dimensions(), (2, 1));
    assert_eq!(decoded.as_ref(), &pixels);
}

#[test]
fn png_limits_bound_the_expanded_rgba_raster() {
    let image = Image {
        width: 3,
        height: 1,
        color_type: ColorType::Grayscale,
        bit_depth: 8,
        samples: Samples::U8(vec![11, 22, 33]),
    };
    let encoded =
        deadlib_image_png::encode(&deadlib_image_png::Png::new(image), &Default::default())
            .unwrap();
    let limits = Limits {
        max_width: 3,
        max_height: 1,
        max_bytes: 11,
    };
    assert!(matches!(
        load_with_limits(&encoded, limits),
        Err(ImageError::LimitExceeded)
    ));
}

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
                    deadlib_image::Rgba(palette[((x + y * width) % 5) as usize])
                });
                if off_pink && width > 0 && height > 0 {
                    image.put_pixel(width - 1, height - 1, deadlib_image::Rgba(palette[1]));
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
