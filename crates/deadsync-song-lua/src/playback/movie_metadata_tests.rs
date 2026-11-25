use deadsync_assets::song_lua::MEDIA_PROBES;
use super::*;

#[test]
#[ignore = "requires ffprobe on PATH or in the runtime bin directory"]
fn movie_source_dimensions_preserve_fullscreen_and_cropped_geometry() {
    crate::tests::init_paths();
    let dir = tempfile::tempdir().expect("movie source directory");
    for (height, crop) in [(240_u32, 0.0_f32), (288, 0.3)] {
        let path = dir.path().join(format!("movie-{height}.mpg"));
        // ffprobe identifies the actual container. A raw single-frame Y4M file
        // needs no encoder, while the ordinary image loader cannot read it.
        let mut movie =
            format!("YUV4MPEG2 W352 H{height} F30:1 Ip A1:1 C420jpeg\nFRAME\n").into_bytes();
        movie.resize(movie.len() + 352 * height as usize * 3 / 2, 128);
        std::fs::write(&path, movie).expect("write independent movie source");
        assert!(image::image_dimensions(&path).is_err());

        let key: Arc<str> = Arc::from(path.to_string_lossy().as_ref());
        let state = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            zoom_x: 854.0 / 352.0,
            zoom_y: 480.0 / height as f32,
            croptop: crop,
            cropbottom: crop,
            sprite_animate: false,
            ..Default::default()
        };
        let overlays: Vec<SongLuaOverlayActor<deadsync_assets::noteskin::SpriteSlot>> = vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Sprite {
                texture_path: path,
                texture_key: Arc::clone(&key),
                states: Arc::from([]),
                textures: Arc::from([]),
            },
            name: Some("Movie".into()),
            parent_index: None,
            initial_state: state,
            message_commands: Vec::new(),
        }];
        let mut composer = WholeSongComposer::new(&overlays, MEDIA_PROBES.movie_dimensions);
        let meta = deadlib_assets::texture_dims(&key).expect("registered movie dimensions");
        assert_eq!([meta.w, meta.h], [352, height]);
        let frame = composer.render_overlay(&overlays, &[state], 0, [854.0, 480.0], 0.0, 0.0);
        let [sprite] = frame.sprite_instances.as_slice() else {
            panic!("expected one movie Sprite: {:?}", frame.ops);
        };
        let expected_size = [854.0, 480.0 * (1.0 - 2.0 * crop)];
        for axis in 0..2 {
            assert!(
                (sprite.size[axis] - expected_size[axis]).abs() < 0.001,
                "height={height} crop={crop}: {:?} != {expected_size:?}",
                sprite.size,
            );
        }
    }
}
