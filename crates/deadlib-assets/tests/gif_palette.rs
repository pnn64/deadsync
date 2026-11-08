use deadlib_assets::{TextureHints, decode_texture_image};

#[test]
fn local_gif_palette_works_without_any_global_color_table() {
    let path =
        std::env::temp_dir().join(format!("deadsync-local-palette-{}.gif", std::process::id()));
    for transparent in [false, true] {
        let mut encoder =
            gif::Encoder::new(std::fs::File::create(&path).unwrap(), 2, 1, &[]).unwrap();
        encoder
            .write_frame(&gif::Frame {
                width: 2,
                height: 1,
                palette: Some(vec![255, 0, 0, 0, 255, 0]),
                transparent: transparent.then_some(1),
                buffer: vec![0, 1].into(),
                ..Default::default()
            })
            .unwrap();
        drop(encoder);
        // The encoder emits a padded global table even for an empty input.
        let mut bytes = std::fs::read(&path).unwrap();
        assert_ne!(bytes[10] & 0x80, 0);
        let colors = 1usize << ((bytes[10] & 7) + 1);
        bytes[10] &= 0x7f;
        bytes.drain(13..13 + colors * 3);
        std::fs::write(&path, bytes).unwrap();
        let image = decode_texture_image(&path, &TextureHints::default()).unwrap();
        assert_eq!(image.dimensions(), (2, 1));
        assert_eq!(image.get_pixel(0, 0).0, [255, 0, 0, 255]);
        // Native hidden-alpha cleanup uses the sole visible pixel's RGB.
        assert_eq!(
            image.get_pixel(1, 0).0,
            if transparent {
                [255, 0, 0, 0]
            } else {
                [0, 255, 0, 255]
            }
        );
    }
    std::fs::remove_file(path).unwrap();
}
