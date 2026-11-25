use super::*;
use deadlib_assets::AssetManager;
use deadlib_present::compose::{
    ComposeScratch, build_screen_cached_with_scratch_and_texture_context,
};
use deadlib_present::space::Metrics;
use deadlib_render_core::{DrawOp, RenderFrame};
use deadsync_assets::song_lua::{SongLuaOverlayActor, SongLuaOverlayKind};

fn metrics() -> Metrics {
    let metrics = Metrics::centered(640.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    deadlib_present::space::set_current_window_px(640, 480);
    metrics
}

struct GlyphTextures;

impl deadlib_present::texture::TextureContext for GlyphTextures {
    fn texture_registry_generation(&self) -> u64 {
        1
    }
    fn texture_dims(&self, _: &str) -> Option<deadlib_present::texture::TextureMeta> {
        Some(deadlib_present::texture::TextureMeta { w: 32, h: 42 })
    }
    fn sprite_sheet_dims(&self, _: &str) -> (u32, u32) {
        (1, 1)
    }
    fn texture_handle(&self, key: &str) -> u64 {
        if key == "count-stroke" { 18 } else { 17 }
    }
}

fn fonts() -> font::FontMap {
    let glyph = font::Glyph {
        texture_key: Arc::from("count-fill"),
        stroke_texture_key: Some(Arc::from("count-stroke")),
        tex_rect: [0.0, 0.0, 32.0, 42.0],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        size: [32.0, 42.0],
        offset: [0.0, -42.0],
        advance: 32.0,
        advance_i32: 32,
    };
    let mut ascii = std::array::from_fn(|_| None);
    ascii[b'2' as usize] = Some(glyph.clone());
    font::FontMap::from_iter([(
        "miso",
        font::Font {
            glyph_map: font::GlyphMap::from_iter([('2', glyph)]),
            ascii_glyphs: Box::new(ascii),
            default_glyph: None,
            line_spacing: 42,
            height: 42,
            fallback_font_name: None,
            cache_tag: 1,
            chain_key: 1,
            default_stroke_color: [0.0; 4],
            stroke_texture_map: [("count-fill".to_owned(), "count-stroke".to_owned())]
                .into_iter()
                .collect(),
            texture_hints_map: Default::default(),
        },
    )])
}

fn countdown(attributed: bool, stroke: bool) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::BitmapText {
            font_name: "miso",
            font_path: "Fonts/Common Normal.ini".into(),
            text: Arc::from("22"),
            text_changes: Arc::from([]),
            text_changes_in_seconds: false,
            stroke_color: stroke.then_some([0.0, 0.0, 0.0, 1.0]),
            attributes: if attributed {
                vec![TextAttribute {
                    start: 0,
                    length: 2,
                    color: [0.25, 0.5, 0.75, 1.0],
                    vertex_colors: Some([[0.3, 0.6, 0.9, 1.0]; 4]),
                    glow: None,
                }]
                .into()
            } else {
                Arc::from([])
            },
        },
        name: Some("MultitapTextP1_1".into()),
        parent_index: None,
        initial_state: SongLuaOverlayState::default(),
        message_commands: Vec::new(),
    }
}

fn draw_text(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    camera: impl Into<SongLuaDrawCamera>,
) -> SongLuaActorList {
    build_song_lua_overlay_actor_with_scratch(
        overlay,
        state,
        camera,
        &AssetManager::<()>::new(),
        20,
        640.0,
        480.0,
        0.0,
        0.0,
        0.0,
        None,
        None,
    )
    .expect("visible countdown output")
}

fn render(actors: &[Actor], prewarm: bool) -> RenderFrame {
    let fonts = fonts();
    let mut text = TextLayoutCache::default();
    if prewarm {
        text.prewarm_text(&fonts, "miso", "22", None);
    }
    build_screen_cached_with_scratch_and_texture_context(
        actors,
        [0.0; 4],
        &metrics(),
        &fonts,
        0.0,
        &mut text,
        &mut ComposeScratch::default(),
        &GlyphTextures,
    )
}

fn assert_native_glyphs(frame: &RenderFrame, expected: Matrix4) {
    let mut checked = 0;
    for op in &frame.ops {
        let DrawOp::TexturedMesh(run) = op else {
            panic!("glyph mesh draw: {op:?}")
        };
        assert!(
            !run.depth_test && !run.depth_write,
            "native countdown keeps default depth flags"
        );
        let camera = frame.cameras[run.camera as usize];
        for instance in &frame.tmesh_instances
            [run.instance_start as usize..(run.instance_start + run.instance_count) as usize]
        {
            let model = Matrix4::from_cols_array_2d(&[
                instance.model_col0,
                instance.model_col1,
                instance.model_col2,
                instance.model_col3,
            ]);
            for vertex in frame.tmesh_geometries[run.geometry as usize]
                .vertices
                .iter()
            {
                let local = Vector3::from_array(vertex.pos).extend(1.0);
                let actual = camera * model * local;
                let native = expected * local;
                assert!(actual.is_finite() && actual.w > 0.0);
                for axis in 0..4 {
                    let tolerance = 0.002 + native[axis].abs() * 0.00002;
                    assert!(
                        (actual[axis] - native[axis]).abs() <= tolerance,
                        "glyph={local:?} axis={axis}: actual={actual:?}, native={native:?}"
                    );
                }
                assert!(
                    actual.z.abs() <= actual.w,
                    "visible native glyph must survive homogeneous depth clipping: {actual:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 12,
        "both countdown digits must emit real glyph triangles"
    );
}

#[test]
fn countdown_glyphs_follow_distant_hallway_and_overhead_fields() {
    metrics();
    for tilt in [-1.0_f32, 0.0, 1.0] {
        for mini in [0.0_f32, 0.6] {
            let root = SongLuaOverlayState {
                x: 160.0,
                y: 240.0,
                fov: Some(45.0),
                vanishpoint: Some([160.0 - 160.0 / 9.0, 240.0]),
                ..Default::default()
            };
            let mini_zoom = 1.0 - 0.5 * mini;
            let field_zoom = mini_zoom * (1.0 - 0.1 * tilt.abs());
            let field_y = if tilt > 0.0 {
                -45.0 * tilt
            } else {
                20.0 * tilt
            };
            let rotation = -30.0 * tilt;
            let field = SongLuaOverlayState {
                y: field_y,
                rot_x_deg: rotation,
                zoom: field_zoom,
                ..Default::default()
            };
            let note = SongLuaOverlayState {
                x: -32.0,
                y: -135.0 / mini_zoom,
                zoom_y: 1.25,
                ..Default::default()
            };
            let count = SongLuaOverlayState {
                z: 10.0,
                ..Default::default()
            };
            let mut state = root;
            for child in [field, note, count] {
                state = song_lua_overlay_compose_state(
                    &SongLuaOverlayKind::ActorFrame,
                    state,
                    child,
                    640.0,
                    480.0,
                    [0.0; 2],
                );
            }
            // Actor::BeginDraw / ActorFrame::BeginDraw: native parent XYZ,
            // rotation then scale, the bouncing note, and the text's local Z.
            // Rage's X rotation has the opposite sine sign to glam's standard X.
            let model = Matrix4::from_translation(Vector3::new(160.0, 240.0, 0.0))
                * Matrix4::from_translation(Vector3::new(0.0, field_y, 0.0))
                * Matrix4::from_rotation_x(-rotation.to_radians())
                * Matrix4::from_scale(Vector3::splat(field_zoom))
                * Matrix4::from_translation(Vector3::new(note.x, note.y, 0.0))
                * Matrix4::from_scale(Vector3::new(1.0, 1.25, 1.0))
                * Matrix4::from_translation(Vector3::new(0.0, 0.0, 10.0));
            let (view, projection) = song_lua_overlay_view_proj(root, 640.0, 480.0).unwrap();
            for attributed in [false, true] {
                let actors = draw_text(&countdown(attributed, true), state, Some(root));
                let frame = render(&actors, !attributed);
                assert!(frame.ops.len() >= 2, "stroke and fill remain present");
                assert_native_glyphs(&frame, projection * view * model);
            }
        }
    }
}

#[test]
fn projected_glyphs_preserve_alignment_fit_attributes_and_screen_world() {
    metrics();
    let state = SongLuaOverlayState {
        x: 280.0,
        y: 220.0,
        z: 10.0,
        rot_x_deg: 27.0,
        zoom_x: 1.25,
        zoom_y: 0.8,
        halign: 0.0,
        valign: 1.0,
        size: Some([96.0, 200.0]),
        max_width: Some(64.0),
        max_height: Some(30.0),
        ..Default::default()
    };
    let camera_state = SongLuaOverlayState {
        fov: Some(45.0),
        vanishpoint: Some([320.0, 240.0]),
        ..Default::default()
    };
    let screen_world = Matrix4::from_translation(Vector3::new(13.0, -9.0, 0.0))
        * Matrix4::from_scale(Vector3::new(1.1, 0.9, 1.0));
    let camera = SongLuaDrawCamera {
        state: Some(camera_state),
        world: screen_world,
        resolved: None,
    };
    let overlay = countdown(true, true);
    let mut actors = Vec::new();
    append_song_lua_overlay_output(
        &mut actors,
        &overlay,
        state,
        camera,
        &AssetManager::<()>::new(),
        20,
        [640.0, 480.0],
        [0.0; 2],
        0.0,
        None,
        None,
    );
    assert_eq!(
        actors
            .iter()
            .filter(|actor| matches!(actor, Actor::CameraPush { .. }))
            .count(),
        1,
        "the text branch owns one camera; the outer emitter must not apply it twice"
    );
    let text = actors
        .iter()
        .find(|actor| matches!(actor, Actor::Text { .. }))
        .unwrap();
    let Actor::Text {
        content,
        font,
        attributes,
        stroke_color,
        ..
    } = text
    else {
        unreachable!()
    };
    assert_eq!(content.as_str(), "22");
    assert_eq!(*font, "miso");
    assert_eq!(attributes[0].color, [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(*stroke_color, Some([0.0, 0.0, 0.0, 1.0]));
    let model = Matrix4::from_translation(Vector3::new(280.0, 220.0, 10.0))
        * Matrix4::from_rotation_x(-27.0_f32.to_radians())
        * Matrix4::from_scale(Vector3::new(1.25, 0.8, 1.0));
    // Two 32x42 glyph cells: fit to 96px first, then constrain the zoomed block
    // to 64x30. The left/bottom actor alignment becomes a local layout offset.
    let layout = Matrix4::from_scale(Vector3::new(64.0 / (64.0 * 1.25), 30.0 / (42.0 * 0.8), 1.0))
        * Matrix4::from_translation(Vector3::new(32.0, -21.0, 0.0));
    let (view, projection) = song_lua_overlay_view_proj(camera_state, 640.0, 480.0).unwrap();
    assert_native_glyphs(
        &render(&actors, false),
        projection * view * screen_world * model * layout,
    );
}

#[test]
fn tilted_text_without_an_explicit_camera_uses_native_menu_depth() {
    metrics();
    let state = SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        z: 10.0,
        rot_x_deg: -30.0,
        ..Default::default()
    };
    let actors = draw_text(&countdown(false, false), state, None);
    let model = Matrix4::from_translation(Vector3::new(320.0, 240.0, 10.0))
        * Matrix4::from_rotation_x(30.0_f32.to_radians());
    assert_native_glyphs(&render(&actors, true), song_lua_screen_proj(1000.0) * model);
}

#[test]
fn plain_overhead_text_keeps_the_direct_text_path() {
    metrics();
    let actors = draw_text(
        &countdown(true, true),
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            ..Default::default()
        },
        None,
    );
    let [
        Actor::Text {
            offset,
            local_transform,
            content,
            ..
        },
    ] = actors.as_slice()
    else {
        panic!("ordinary overhead text must stay on its existing direct path");
    };
    assert_eq!(*offset, [320.0, 240.0]);
    assert_eq!(*local_transform, Matrix4::IDENTITY);
    assert_eq!(content.as_str(), "22");
    let expected =
        song_lua_screen_proj(1.0) * Matrix4::from_translation(Vector3::new(320.0, 240.0, 0.0));
    assert_native_glyphs(&render(&actors, false), expected);
}

#[test]
fn projected_countdown_preserves_arrow_order_in_captured_sources() {
    metrics();
    let mesh = |z| Actor::Mesh {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Fill; 2],
        tint: [1.0; 4],
        vertex_color_encoding: Default::default(),
        vertices: Arc::from([]),
        visible: true,
        blend: BlendMode::Alpha,
        z,
    };
    let mut captured = vec![mesh(10)];
    captured.extend(draw_text(
        &countdown(false, false),
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            z: 10.0,
            rot_x_deg: 30.0,
            ..Default::default()
        },
        None,
    ));
    captured.push(mesh(30));
    song_lua_proxy_local_children_in_place(&mut captured);
    assert!(
        matches!(
            captured.as_slice(),
            [
                Actor::Mesh { z: 0, .. },
                Actor::CameraPush { .. },
                Actor::Text { z: 0, .. },
                Actor::CameraPop,
                Actor::Mesh { z: 0, .. },
            ]
        ),
        "the projected countdown must stay after its arrow and before later actors"
    );
}

fn proxy(actors: Vec<Actor>, state: SongLuaOverlayState, source_offset: [f32; 2]) -> Actor {
    let source = [Arc::<[Actor]>::from(actors)];
    song_lua_build_proxy_actor_in_space_with_scratch(
        state,
        30,
        SongLuaProxySource::offset(&source, source_offset),
        1280.0,
        960.0,
        640.0,
        480.0,
        None,
    )
    .expect("visible countdown proxy")
}

#[test]
fn translated_proxy_moves_projected_countdown_outside_its_rotation() {
    metrics();
    let state = SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        z: 10.0,
        rot_x_deg: 30.0,
        ..Default::default()
    };
    let model = Matrix4::from_translation(Vector3::new(320.0, 240.0, 10.0))
        * Matrix4::from_rotation_x(-30.0_f32.to_radians());
    for nested in [false, true] {
        let mut actors = draw_text(&countdown(false, true), state, None).into_vec();
        if nested {
            actors = vec![Actor::Frame {
                align: [0.0; 2],
                offset: [0.0; 2],
                size: [SizeSpec::Fill; 2],
                children: actors,
                background: None,
                z: 0,
            }];
        }
        // Actor XY is scaled into screen space before adding the source offset.
        // The resulting (15, 13) translation must not acquire source rotation/Z.
        let actor = proxy(
            actors,
            SongLuaOverlayState {
                x: 20.0,
                y: 40.0,
                ..Default::default()
            },
            [5.0, -7.0],
        );
        assert!(matches!(actor, Actor::SharedTransform { .. }));
        let expected = song_lua_screen_proj(1000.0)
            * Matrix4::from_translation(Vector3::new(15.0, 13.0, 0.0))
            * model;
        assert_native_glyphs(&render(&[actor], true), expected);
    }
}

#[test]
fn cancelling_proxy_offsets_preserve_the_projected_source_camera() {
    metrics();
    let state = SongLuaOverlayState {
        x: 320.0,
        y: 240.0,
        z: 10.0,
        rot_x_deg: 30.0,
        ..Default::default()
    };
    let actor = proxy(
        draw_text(&countdown(false, true), state, None).into_vec(),
        SongLuaOverlayState {
            x: 40.0,
            y: 20.0,
            ..Default::default()
        },
        [-20.0, -10.0],
    );
    assert!(
        matches!(&actor, Actor::SharedFrame { offset, .. } if *offset == [0.0; 2]),
        "cancelling offsets must not introduce an identity camera remap"
    );
    let expected = song_lua_screen_proj(1000.0)
        * Matrix4::from_translation(Vector3::new(320.0, 240.0, 10.0))
        * Matrix4::from_rotation_x(-30.0_f32.to_radians());
    assert_native_glyphs(&render(&[actor], true), expected);
}

#[test]
fn translation_only_flat_text_proxy_keeps_the_shared_frame_path() {
    metrics();
    let actor = proxy(
        draw_text(
            &countdown(false, true),
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                ..Default::default()
            },
            None,
        )
        .into_vec(),
        SongLuaOverlayState {
            x: 20.0,
            y: 40.0,
            ..Default::default()
        },
        [5.0, -7.0],
    );
    assert!(matches!(&actor, Actor::SharedFrame { offset, .. } if *offset == [15.0, 13.0]));
    let expected =
        song_lua_screen_proj(1.0) * Matrix4::from_translation(Vector3::new(335.0, 253.0, 0.0));
    assert_native_glyphs(&render(&[actor], true), expected);
}

#[test]
fn projected_countdown_fill_and_glow_stay_in_inline_storage() {
    metrics();
    let actors = draw_text(
        &countdown(false, true),
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            z: 10.0,
            rot_x_deg: 30.0,
            glow: [0.1, 0.2, 0.3, 1.0],
            ..Default::default()
        },
        None,
    );
    assert!(
        matches!(
            actors.as_slice(),
            [
                Actor::CameraPush { .. },
                Actor::Text {
                    blend: BlendMode::Alpha,
                    ..
                },
                Actor::Text {
                    blend: BlendMode::Add,
                    ..
                },
                Actor::CameraPop,
            ]
        ),
        "fill and glow must share the native text camera"
    );
    assert!(
        !actors.spilled(),
        "projected text must not allocate an actor list every frame"
    );
    let expected = song_lua_screen_proj(1000.0)
        * Matrix4::from_translation(Vector3::new(320.0, 240.0, 10.0))
        * Matrix4::from_rotation_x(-30.0_f32.to_radians());
    assert_native_glyphs(&render(&actors, true), expected);
}

#[test]
fn translated_proxy_respects_text_camera_scopes_and_declared_player_roots() {
    metrics();
    let text = draw_text(
        &countdown(false, false),
        SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            ..Default::default()
        },
        None,
    )
    .into_vec();
    for text_in_camera in [false, true] {
        for declared_player_root in [false, true] {
            let mut last_segment = Vec::new();
            if !text_in_camera {
                last_segment.push(Actor::CameraPop);
            }
            last_segment.extend(text.iter().cloned());
            if text_in_camera {
                last_segment.push(Actor::CameraPop);
            }
            // The inner scope closes in another segment; the outer scope must
            // remain active until its own pop, without affecting later text.
            let segments = [
                Arc::<[Actor]>::from(vec![
                    Actor::CameraPush {
                        view_proj: Matrix4::IDENTITY,
                    },
                    Actor::Mesh {
                        align: [0.0; 2],
                        offset: [0.0; 2],
                        size: [SizeSpec::Fill; 2],
                        tint: [1.0; 4],
                        vertex_color_encoding: Default::default(),
                        vertices: Arc::from([]),
                        visible: true,
                        blend: BlendMode::Alpha,
                        z: 0,
                    },
                    Actor::CameraPush {
                        view_proj: Matrix4::IDENTITY,
                    },
                ]),
                Arc::<[Actor]>::from(vec![Actor::CameraPop]),
                Arc::<[Actor]>::from(last_segment),
            ];
            let source = SongLuaProxySource {
                source_view_proj: declared_player_root
                    .then(|| song_lua_player_root_camera(Matrix4::IDENTITY)),
                ..SongLuaProxySource::offset(&segments, [5.0, -7.0])
            };
            let actor = song_lua_build_proxy_actor_in_space_with_scratch(
                SongLuaOverlayState {
                    x: 20.0,
                    y: 40.0,
                    ..Default::default()
                },
                30,
                source,
                1280.0,
                960.0,
                640.0,
                480.0,
                None,
            )
            .expect("mixed proxy source should render");
            assert_eq!(
                matches!(&actor, Actor::SharedTransform { .. }),
                text_in_camera && !declared_player_root,
                "text_in_camera={text_in_camera}, declared_player_root={declared_player_root}"
            );
            if !text_in_camera {
                assert!(
                    matches!(&actor, Actor::SharedFrame { offset, .. } if *offset == [15.0, 13.0])
                );
            }
        }
    }
}
