use deadlib_assets::{
    TextureHints, apply_texture_hints, decode_texture_image, fix_hidden_alpha, open_image_fallback,
};
use image::{DynamicImage, GrayAlphaImage, GrayImage, RgbImage, RgbaImage};

#[test]
fn texture_cleanup_preserves_8bit_sources_and_hint_combinations() {
    let dir = std::env::temp_dir().join(format!(
        "deadsync-texture-cleanup-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let sources = [
        DynamicImage::ImageRgb8(RgbImage::from_fn(13, 7, |x, y| {
            image::Rgb([(x * 19) as u8, (y * 37) as u8, 0])
        })),
        DynamicImage::ImageRgba8(RgbaImage::from_fn(13, 7, |x, y| {
            image::Rgba([
                (x * 19) as u8,
                (y * 37) as u8,
                0,
                [0, 1, 128, 255][x as usize % 4],
            ])
        })),
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(13, 7, image::Rgba([17, 83, 149, 0]))),
        DynamicImage::ImageLuma8(GrayImage::from_fn(13, 7, |x, y| {
            image::Luma([(x * 19 + y * 37) as u8])
        })),
        DynamicImage::ImageLumaA8(GrayAlphaImage::from_fn(13, 7, |x, y| {
            image::LumaA([(x * 19) as u8, (y * 37) as u8])
        })),
    ];
    for (index, source) in sources.into_iter().enumerate() {
        let path = dir.join(format!("{index}.png"));
        source.save(&path).unwrap();
        for non_default in [false, true] {
            for grayscale in [false, true] {
                for alphamap in [false, true] {
                    let hints = TextureHints {
                        non_default,
                        grayscale,
                        alphamap,
                        ..Default::default()
                    };
                    let mut expected = open_image_fallback(&path).unwrap().into_rgba8();
                    if !hints.is_default() {
                        apply_texture_hints(&mut expected, &hints);
                    }
                    fix_hidden_alpha(&mut expected);
                    assert_eq!(
                        decode_texture_image(&path, &hints).unwrap(),
                        expected,
                        "source={index}, hints={hints:?}"
                    );
                }
            }
        }
        std::fs::remove_file(path).unwrap();
    }
    std::fs::remove_dir(dir).unwrap();
}
