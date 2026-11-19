use super::*;

#[test]
fn image_content_dimensions_preserve_sprite_geometry() {
    crate::tests::init_paths();
    let dir = tempfile::tempdir().expect("image source directory");
    for (name, format) in [
        ("jpeg-content.png", image::ImageFormat::Jpeg),
        ("png-content.jpg", image::ImageFormat::Png),
    ] {
        let path = dir.path().join(name);
        image::RgbImage::from_pixel(37, 23, image::Rgb([64, 128, 192]))
            .save_with_format(&path, format)
            .expect("write image with a mismatched extension");
        let key: Arc<str> = Arc::from(path.to_string_lossy().as_ref());
        let state = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            zoom_x: 2.0,
            zoom_y: 3.0,
            sprite_animate: false,
            ..Default::default()
        };
        let overlays: Vec<SongLuaOverlayActor<deadsync_assets::noteskin::SpriteSlot>> =
            vec![SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Sprite {
                    texture_path: path,
                    texture_key: Arc::clone(&key),
                    states: Arc::from([]),
                    textures: Arc::from([]),
                },
                name: Some("Image".into()),
                parent_index: None,
                initial_state: state,
                message_commands: Vec::new(),
            }];
        let mut composer = WholeSongComposer::new(&overlays);
        let meta = deadlib_assets::texture_dims(&key).expect("registered source dimensions");
        assert_eq!([meta.w, meta.h], [37, 23], "source {name}");
        let frame = composer.render_overlay(&overlays, &[state], 0, [854.0, 480.0], 0.0, 0.0);
        let [sprite] = frame.sprite_instances.as_slice() else {
            panic!("expected one image Sprite for {name}: {:?}", frame.ops);
        };
        assert_eq!(sprite.size, [74.0, 69.0], "source {name}");
    }
}
