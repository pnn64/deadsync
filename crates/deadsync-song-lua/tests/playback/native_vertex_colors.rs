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
        deadlib_image::RgbaImage::from_pixel(1, 1, deadlib_image::Rgba([255; 4])),
    );
    assets.queue_texture_upload(
        "native-colors.png".to_owned(),
        deadlib_image::RgbaImage::new(40, 30),
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

fn multi_vertex_overlay() -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::ActorMultiVertex {
            vertices: Arc::from([
                SongLuaOverlayMeshVertex {
                    pos: [0.0, 0.0], z: 0.0, uv: [0.1, 0.2],
                    color: [0.1234, 0.501, 0.8761, 0.1234],
                },
                SongLuaOverlayMeshVertex {
                    pos: [40.0, 0.0], z: 2.0, uv: [0.8, 0.2],
                    color: [-0.125, 1.25, 0.501, 0.499],
                },
                SongLuaOverlayMeshVertex {
                    pos: [0.0, 30.0], z: 4.0, uv: [0.1, 0.9],
                    color: [0.1234, 0.501, 0.8761, 0.1234],
                },
            ]),
            texture_path: Some("native-colors.png".into()),
            texture_key: Some(Arc::from("native-colors.png")),
            capture_name: None,
        },
        name: None,
        parent_index: None,
        initial_state: Default::default(),
        message_commands: Vec::new(),
    }
}

fn multi_vertex_actors(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    assets: &AssetManager,
    path: usize,
) -> Vec<Actor> {
    if path == 2 {
        let overlays = std::slice::from_ref(overlay);
        let topology = SongLuaOverlayTopologyIndex::new(overlays);
        let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &overlay.kind else {
            unreachable!()
        };
        let vertices = song_lua_actor_multi_vertex_textured_mesh(vertices);
        let op = SongLuaDrawOp::Draw {
            source: SongLuaDrawSource::Overlay(0),
            state,
            parents: Arc::from([]),
            inherited_diffuse: [1.0; 4],
            inherited_glow: [0.0; 4],
            camera: None,
            vertices: None,
        };
        return vec![song_lua_manual_mesh(
            overlays, &topology, &op, &vertices, None.into(), assets, [640.0, 480.0], [0.0; 2], 0,
        ).expect("manual textured ActorMultiVertex")];
    }
    let mut scratch = SongLuaProjectedMeshScratch::textured(3);
    build_song_lua_overlay_actor_with_scratch(
        overlay, state, None, assets, 0, 640.0, 480.0, 0.0, 0.0, 0.0,
        None, (path == 1).then_some(&mut scratch),
    ).expect("textured ActorMultiVertex").into_iter().collect()
}

#[test]
fn song_lua_native_multi_vertex_colors_use_bytes_in_static_reusable_and_manual_draws() {
    let metrics = metrics();
    let assets = color_assets();
    let overlay = multi_vertex_overlay();
    let state = SongLuaOverlayState {
        x: 120.0, y: 160.0,
        diffuse: [0.8, 0.6, 0.75, 0.8],
        ..Default::default()
    };
    for path in 0..3 {
        let mut actors = multi_vertex_actors(&overlay, state, &assets, path);
        assert_eq!(actors.len(), 1);
        song_lua_style_capture_actor_in_place(
            &mut actors[0], [0.875, 0.875, 0.75, 0.875], None, 0,
        );
        let frame = deadlib_present::compose::build_screen_with_texture_context(
            &actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0,
            assets.texture_context(),
        );
        assert_eq!(submitted_colors(&frame, 0), vec![
            rgba([21, 67, 126, 21]), rgba([0, 133, 72, 88]), rgba([21, 67, 126, 21]),
        ], "draw path {path}");
        let DrawOp::TexturedMesh(run) = frame.ops[0] else { unreachable!() };
        let SongLuaOverlayKind::ActorMultiVertex { vertices, .. } = &overlay.kind else {
            unreachable!()
        };
        for (actual, source) in frame.tmesh_geometries[run.geometry as usize]
            .vertices.iter().zip(vertices.iter())
        {
            assert_eq!(actual.pos, [source.pos[0], source.pos[1], source.z]);
            assert_eq!(actual.uv, source.uv);
        }
        assert_eq!(vertices[0].color, [0.1234, 0.501, 0.8761, 0.1234]);
        assert_eq!(state.diffuse, [0.8, 0.6, 0.75, 0.8]);
    }
}

#[test]
fn song_lua_native_multi_vertex_colors_keep_glow_separate_from_diffuse_bytes() {
    let metrics = metrics();
    let assets = color_assets();
    let overlay = multi_vertex_overlay();
    let state = SongLuaOverlayState {
        x: 120.0, y: 160.0,
        diffuse: [1.0, 1.0, 1.0, 0.001],
        glow: [0.4375, 0.25, 0.125, 0.31339297],
        ..Default::default()
    };
    for path in 0..3 {
        let actors = multi_vertex_actors(&overlay, state, &assets, path);
        let frame = deadlib_present::compose::build_screen_with_texture_context(
            &actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0,
            assets.texture_context(),
        );
        assert_eq!(frame.ops.len(), 2, "draw path {path}");
        for color in submitted_colors(&frame, 0) {
            assert_eq!(color[3], 0.0, "positive float alpha keeps the diffuse pass");
        }
        for color in submitted_colors(&frame, 1) {
            assert_eq!(color, rgba([112, 64, 32, 80]), "draw path {path}");
        }
    }
}

#[test]
fn song_lua_native_multi_vertex_colors_pack_untextured_meshes_after_capture_tint() {
    let metrics = metrics();
    let assets = color_assets();
    let mut overlay = multi_vertex_overlay();
    let SongLuaOverlayKind::ActorMultiVertex { texture_path, texture_key, .. } = &mut overlay.kind else {
        unreachable!()
    };
    *texture_path = None;
    *texture_key = None;
    let state = SongLuaOverlayState {
        x: 120.0, y: 160.0, diffuse: [0.8, 0.6, 0.75, 0.8], ..Default::default()
    };
    for path in 0..2 {
        let mut actors = multi_vertex_actors(&overlay, state, &assets, path);
        assert_eq!(actors.len(), 1);
        song_lua_style_capture_actor_in_place(&mut actors[0], [0.875, 0.875, 0.75, 0.875], None, 0);
        let frame = deadlib_present::compose::build_screen(
            &actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0,
        );
        let colors = frame.mesh_vertices.iter().map(|vertex| vertex.color).collect::<Vec<_>>();
        assert_eq!(colors, [rgba([21,67,126,21]), rgba([0,133,72,88]), rgba([21,67,126,21])]);
        assert_eq!(frame.mesh_vertices.iter().map(|vertex| vertex.pos).collect::<Vec<_>>(),
            [[-200.0, 80.0], [-160.0, 80.0], [-200.0, 110.0]]);
        let source = match &actors[0] {
            Actor::Mesh { vertices, .. } => vertices.as_ref(),
            Actor::ReusableMesh { vertices, .. } => vertices.as_slice(),
            _ => panic!("expected an untextured mesh"),
        };
        assert_eq!(source[0].color, [0.1234, 0.501, 0.8761, 0.1234]);
    }
}

#[test]
fn song_lua_native_multi_vertex_colors_emit_untextured_glow_separately() {
    let metrics = metrics();
    let assets = color_assets();
    let mut overlay = multi_vertex_overlay();
    let SongLuaOverlayKind::ActorMultiVertex { texture_path, texture_key, .. } = &mut overlay.kind else {
        unreachable!()
    };
    *texture_path = None;
    *texture_key = None;
    for path in 0..2 {
        for alpha in [0.001, 0.0] {
            let state = SongLuaOverlayState {
                x: 120.0, y: 160.0,
                diffuse: [1.0, 1.0, 1.0, alpha],
                glow: [0.4375, 0.25, 0.125, 0.31339297], ..Default::default()
            };
            let actors = multi_vertex_actors(&overlay, state, &assets, path);
            let frame = deadlib_present::compose::build_screen(
                &actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0,
            );
            let diffuse_vertices = if alpha > 0.0 { 3 } else { 0 };
            assert_eq!(frame.mesh_vertices.len(), diffuse_vertices + 3);
            for vertex in &frame.mesh_vertices[..diffuse_vertices] {
                assert_eq!(vertex.color[3], 0.0, "positive float alpha retains the diffuse pass");
            }
            for vertex in &frame.mesh_vertices[diffuse_vertices..] {
                assert_eq!(vertex.color, rgba([112,64,32,80]));
            }
        }
    }
}
