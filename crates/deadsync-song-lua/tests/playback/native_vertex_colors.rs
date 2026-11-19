use super::*;
use deadlib_present::actors::VertexColorEncoding;
use deadlib_render_core::{DrawOp, RenderFrame};

fn metrics() -> deadlib_present::space::Metrics {
    let metrics = deadlib_present::space::Metrics::centered(640.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    deadlib_present::space::set_current_window_px(640, 480);
    metrics
}

fn state() -> SongLuaOverlayState {
    SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        size: Some([40.0, 30.0]),
        diffuse: [0.875, 0.5, 0.25, 0.5],
        vertex_colors: Some([[0.5, 0.25, 0.875, 0.875]; 4]),
        fadeleft: 0.8,
        faderight: 0.8,
        ..Default::default()
    }
}

fn submitted_colors(frame: &RenderFrame, op: usize) -> Vec<[f32; 4]> {
    let DrawOp::TexturedMesh(run) = frame.ops[op] else {
        panic!("expected sprite color mesh")
    };
    let tint = frame.tmesh_instances[run.instance_start as usize].tint;
    frame.tmesh_geometries[run.geometry as usize]
        .vertices
        .iter()
        .map(|vertex| std::array::from_fn(|i| vertex.color[i] * tint[i]))
        .collect()
}

fn rgba(bytes: [u8; 4]) -> [f32; 4] {
    bytes.map(|byte| f32::from(byte) / 255.0)
}

fn color_assets() -> AssetManager {
    let mut assets = AssetManager::new();
    assets.queue_texture_upload(
        white_texture_key().to_string(),
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
    );
    assets.queue_texture_upload(
        "native-colors.png".to_owned(),
        image::RgbaImage::new(40, 30),
    );
    assets
}

#[test]
fn song_lua_native_vertex_colors_pack_after_fade_corners_and_capture_style() {
    let metrics = metrics();
    let assets = color_assets();
    for kind in [
        test_sprite_kind("native-colors.png"),
        SongLuaOverlayKind::Quad,
    ] {
        let overlay = SongLuaOverlayActor {
            kind,
            name: None,
            parent_index: None,
            initial_state: Default::default(),
            message_commands: Vec::new(),
        };
        for camera in [
            None,
            Some(SongLuaOverlayState {
                fov: Some(45.0),
                vanishpoint: Some([320.0, 240.0]),
                ..Default::default()
            }),
        ] {
            for cached in [false, true] {
                let state = state();
                let mut scratch = SongLuaProjectedMeshScratch::default();
                let mut actors = build_song_lua_overlay_actor_with_scratch(
                    &overlay,
                    state,
                    camera,
                    &assets,
                    0,
                    640.0,
                    480.0,
                    0.0,
                    0.0,
                    0.0,
                    None,
                    cached.then_some(&mut scratch),
                )
                .expect("visible sprite");
                assert_eq!(actors.len(), 1);
                assert!(song_lua_has_native_sprite_colors(&actors[0]));
                song_lua_style_capture_actor_in_place(&mut actors[0], [0.875; 4], None, 0);
                let frame = deadlib_present::compose::build_screen_with_texture_context(
                    &actors,
                    [0.0; 4],
                    &metrics,
                    &font::FontMap::default(),
                    0.0,
                    assets.texture_context(),
                );
                let colors = submitted_colors(&frame, 0);
                assert_eq!(colors.len(), 12, "overlapping fades emit two edge quads");
                // Sprite.cpp scales each .8 fade to .5, leaving inner alpha .625.
                // Combine diffuse, corner, .625 fade and .875 capture before FTOC.
                let expected_alpha = [0, 61, 61, 0, 61, 0, 61, 0, 0, 61, 0, 61];
                for (color, alpha) in colors.iter().zip(expected_alpha) {
                    assert_eq!(
                        *color,
                        rgba([98, 28, 49, alpha]),
                        "camera={camera:?}, cached={cached}"
                    );
                }
                assert_eq!(state.diffuse, [0.875, 0.5, 0.25, 0.5]);
                assert_eq!(state.vertex_colors.unwrap()[0][3], 0.875);
            }
        }
    }
}

#[test]
fn song_lua_native_vertex_colors_clamp_after_all_color_multiplication() {
    let metrics = metrics();
    let assets = color_assets();
    for kind in [test_sprite_kind("native-colors.png"), SongLuaOverlayKind::Quad] {
        let overlay = SongLuaOverlayActor {
            kind,
            name: None,
            parent_index: None,
            initial_state: Default::default(),
            message_commands: Vec::new(),
        };
        let mut actors = build_song_lua_overlay_actor_with_scratch(
            &overlay,
            SongLuaOverlayState {
                diffuse: [2.0, -1.0, 0.5, 2.0],
                glow: [2.0, -1.0, 0.5, 2.0],
                vertex_colors: Some([[0.5, -0.5, 2.0, 0.25]; 4]),
                fadeleft: 0.0,
                faderight: 0.0,
                ..state()
            },
            None,
            &assets,
            0,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
            None,
            None,
        ).expect("visible native color mesh");
        for actor in &mut actors {
            song_lua_style_capture_actor_in_place(actor, [0.875; 4], None, 0);
        }
        let frame = deadlib_present::compose::build_screen_with_texture_context(
            &actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0,
            assets.texture_context(),
        );
        assert_eq!(frame.ops.len(), 2);
        for color in submitted_colors(&frame, 0) {
            assert_eq!(color, rgba([224, 112, 224, 112]));
        }
        for color in submitted_colors(&frame, 1) {
            assert_eq!(color, rgba([255, 0, 112, 255]));
        }
    }
}

#[test]
fn song_lua_native_vertex_colors_keep_shadow_and_glow_independent_of_corner_rgb() {
    let metrics = metrics();
    let assets = color_assets();
    let overlay: SongLuaOverlayActor = SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Quad,
        name: None,
        parent_index: None,
        initial_state: Default::default(),
        message_commands: Vec::new(),
    };
    let mut actors = build_song_lua_overlay_actor_with_scratch(
        &overlay,
        SongLuaOverlayState {
            glow: [0.875; 4],
            shadow_len: [3.0, 4.0],
            shadow_color: [0.875, 0.5, 0.25, 0.5],
            ..state()
        },
        None,
        &assets,
        0,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        None,
        None,
    )
    .expect("visible quad");
    for actor in &mut actors {
        song_lua_style_capture_actor_in_place(actor, [0.875; 4], None, 0);
    }
    let frame = deadlib_present::compose::build_screen_with_texture_context(
        &actors,
        [0.0; 4],
        &metrics,
        &font::FontMap::default(),
        0.0,
        assets.texture_context(),
    );
    assert_eq!(frame.ops.len(), 3, "shadow, diffuse, and glow");
    for (index, color) in submitted_colors(&frame, 0).into_iter().enumerate() {
        assert_eq!(color, rgba([224, 128, 64, if index < 6 { 0 } else { 30 }]));
    }
    for color in submitted_colors(&frame, 2) {
        assert_eq!(
            color,
            rgba([196; 4]),
            "glow ignores diffuse corners and fade alpha"
        );
    }
}

#[test]
fn song_lua_native_vertex_colors_include_aft_sprites() {
    let metrics = metrics();
    let handle = deadlib_render_core::render_target_texture_handle(17);
    let actors = build_song_lua_aft_sprite_actor(
        state(),
        handle,
        [40.0, 30.0],
        0,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        None,
    )
    .expect("visible AFT sprite");
    let Actor::TexturedMesh {
        vertex_color_encoding,
        texture,
        ..
    } = &actors[0]
    else {
        panic!("faded AFT sprite uses explicit native vertex colors")
    };
    assert_eq!(*vertex_color_encoding, VertexColorEncoding::RageVColor);
    assert_eq!(
        texture.texture_handle(),
        Some(deadlib_render_core::render_target_sample_handle(
            handle, false
        ))
    );
    let frame = deadlib_present::compose::build_screen_with_texture_context(
        &actors,
        [0.0; 4],
        &metrics,
        &font::FontMap::default(),
        0.0,
        &NullTextureContext,
    );
    let colors = submitted_colors(&frame, 0);
    assert_eq!(colors.len(), 12);
    assert_eq!(colors[0], rgba([112, 32, 56, 0]));
    assert_eq!(colors[1], rgba([112, 32, 56, 70]));
}
