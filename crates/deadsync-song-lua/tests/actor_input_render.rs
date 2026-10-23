#![cfg(target_os = "windows")]
//! One focused production playback/pixel differential, without the Song lib harness.
pub use deadsync_song_lua::*;
#[path = "../src/actor_quaternion.rs"]
mod actor_quaternion;
#[allow(dead_code)]
mod renderer {
    include!("../src/playback.rs");

    struct WhiteTextureContext;
    impl deadlib_present::texture::TextureContext for WhiteTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            1
        }
        fn texture_dims(&self, key: &str) -> Option<deadlib_present::texture::TextureMeta> {
            (key == "__white").then_some(deadlib_present::texture::TextureMeta { w: 1, h: 1 })
        }
        fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
            (1, 1)
        }
        fn texture_handle(&self, key: &str) -> u64 {
            if key == "__white" {
                1
            } else {
                deadlib_render_core::INVALID_TEXTURE_HANDLE
            }
        }
    }

    fn frame(
        overlays: &[SongLuaOverlayActor<deadsync_assets::noteskin::SpriteSlot>],
        states: &[SongLuaOverlayState],
    ) -> deadlib_render_core::RenderFrame {
        use deadlib_present::actors::ActorResourceArena;
        use deadlib_present::compose::{
            ComposeScratch,
            build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources,
        };
        let mut overlays = overlays.to_vec();
        for (overlay, state) in overlays.iter_mut().zip(states) {
            overlay.initial_state = *state;
        }
        let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut caches = Vec::new();
        let mut local = Vec::new();
        let mut composed = Vec::new();
        song_lua_overlay_state_sets_from_into::<deadsync_assets::noteskin::SpriteSlot>(
            0.0,
            &overlays,
            &[],
            &[],
            &vec![0..0; overlays.len()],
            64.0,
            48.0,
            &mut order,
            &mut caches,
            &mut local,
            &mut composed,
            [0.0; 2],
        );
        let assets = AssetManager::new();
        let mut actors = Vec::new();
        for (index, overlay) in overlays.iter().enumerate() {
            if !matches!(overlay.kind, SongLuaOverlayKind::Quad) {
                continue;
            }
            if let Some(draws) = build_song_lua_overlay_actor(
                overlay,
                composed[index],
                None,
                &assets,
                index as i16,
                64.0,
                48.0,
                0.0,
                0.0,
                0.0,
            ) {
                actors.extend(draws);
            }
        }
        build_screen_segments_cached_with_scratch_and_texture_context_and_actor_resources(
            &[ActorSegment::new(&actors)],
            [0.0, 0.0, 0.0, 1.0],
            &deadlib_present::space::Metrics::centered(64.0, 48.0),
            &font::FontMap::default(),
            0.0,
            &mut TextLayoutCache::default(),
            &mut ComposeScratch::default(),
            &WhiteTextureContext,
            &ActorResourceArena::new(0),
        )
    }

    #[test]
    #[ignore = "requires a local OpenGL device and a hidden window"]
    fn native_actor_inputs_preserve_gameplay_frame_bits_and_opengl_pixels() {
        use deadlib_render::{Backend, Texture, create_backend};
        use deadlib_render_core::{
            BackendType, PresentModePolicy, ProjectionMatrix, TextureHandleMap,
        };
        use deadsync_song_lua::{
            SongLuaCompileContext, SongLuaInputEvent, SongLuaInputType, SongLuaNoteskinResolver,
            compile_song_lua_layers_with_default_host,
        };
        use winit::{
            dpi::PhysicalSize, event_loop::EventLoop,
            platform::windows::EventLoopBuilderExtWindows, window::Window,
        };
        let metrics = deadlib_present::space::Metrics::centered(64.0, 48.0);
        deadlib_present::space::set_current_metrics(metrics);
        deadlib_present::space::set_current_window_px(64, 48);
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/actor-input.lua");
        let mut context = SongLuaCompileContext::new(path.parent().unwrap(), "input pixel test");
        context.live = true;
        context.music_length_seconds = 1.0;
        context.screen_width = 64.0;
        context.screen_height = 48.0;
        let mut compiled = compile_song_lua_layers_with_default_host(
            &[&path],
            0,
            &context,
            SongLuaNoteskinResolver::default(),
            |_: &Path, _: &Path, _: &Path| {
                Ok(Arc::<[deadsync_assets::noteskin::SpriteSlot]>::from([]))
            },
            |_: &deadsync_assets::noteskin::SpriteSlot| None::<SongLuaOverlayModelLayer>,
            |_: &SongLuaCompileContext, _: &str| {
                None::<(
                    SongLuaOverlayKind<deadsync_assets::noteskin::SpriteSlot>,
                    SongLuaOverlayState,
                )>
            },
        )
        .unwrap();
        let mut scene = compiled[0].live_scene.take().unwrap();
        let mut native = scene.compile_actor_inputs().unwrap();
        let event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
        #[expect(deprecated, reason = "hidden pixel fixture needs no event dispatch")]
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(PhysicalSize::new(64, 48)),
                )
                .unwrap(),
        );
        let mut backend = create_backend(
            BackendType::OpenGL,
            window,
            ProjectionMatrix::from_cols_array(&metrics.projection().to_cols_array()),
            false,
            PresentModePolicy::Immediate,
            false,
            true,
        )
        .unwrap();
        let mut textures = TextureHandleMap::<Texture>::default();
        textures.insert(
            1,
            backend
                .create_texture(
                    &deadlib_assets::white_texture_image().image,
                    Default::default(),
                )
                .unwrap(),
        );
        fn pixels(
            backend: &mut Backend,
            frame: &deadlib_render_core::RenderFrame,
            textures: &TextureHandleMap<Texture>,
        ) -> image::RgbaImage {
            backend.request_screenshot();
            backend.draw(frame, textures, false).unwrap();
            backend.capture_frame().unwrap()
        }
        let mut visible_frames = 0;
        for index in 0..30 {
            let input = SongLuaInputEvent {
                player: Some(index % 2),
                controller: Some(0),
                button: ["Left", "Right", "Other"][index % 3],
                game_button: ["Left", "Right", "Other"][index % 3],
                kind: [
                    SongLuaInputType::FirstPress,
                    SongLuaInputType::Repeat,
                    SongLuaInputType::Release,
                ][index % 3],
                device: "InputDevice_Key",
                device_button: "DeviceButton_left",
                level: [0.25, 0.5, 1.0][index % 3],
                z: 0,
                down: index % 2 == 0,
                ago: 0.0,
                is_joystick: false,
                is_mouse: false,
            };
            assert_eq!(native.input(&input), scene.input(&input).unwrap().0);
            let before = frame(&compiled[0].overlays, scene.states());
            let after = frame(&compiled[0].overlays, native.states());
            assert_eq!(
                deadlib_render_core::frame_compare::compare_render_frames(&before, &after),
                Ok(())
            );
            let before = pixels(&mut backend, &before, &textures);
            let after = pixels(&mut backend, &after, &textures);
            assert_eq!(before.as_raw(), after.as_raw(), "input {index}");
            visible_frames += usize::from(before.pixels().any(|p| p.0[..3] != [0, 0, 0]));
        }
        assert!(visible_frames > 0, "fixture must draw visible gameplay");
        backend.cleanup();
    }
}
