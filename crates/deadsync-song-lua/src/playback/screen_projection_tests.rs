use super::*;
use deadlib_assets::AssetManager;
use deadlib_present::compose::{ComposeScratch, NullTextureContext};
use deadlib_present::space::Metrics;

#[test]
fn screen_zoom_precedes_child_camera_and_depth_range() {
    let size = [854.0, 480.0];
    deadlib_present::space::set_current_metrics(Metrics::centered(size[0], size[1]));
    let child_camera = SongLuaOverlayState {
        fov: Some(60.0),
        ..Default::default()
    };
    let (view, projection) = song_lua_overlay_view_proj(child_camera, size[0], size[1]).unwrap();
    for screen_fov in [None, Some(90.0)] {
        for zoom_z in [1.0, 0.0] {
            let screen = SongLuaOverlayState {
                x: -42.7,
                y: -24.0,
                zoom: 1.1,
                zoom_z,
                fov: screen_fov,
                ..Default::default()
            };
            let screen_model = Matrix4::from_translation(Vector3::new(-42.7, -24.0, 0.0))
                * Matrix4::from_scale(Vector3::new(
                    1.1,
                    1.1,
                    if zoom_z == 0.0 { 0.0 } else { 1.1 },
                ));
            for depth in [false, true] {
                let draw = SongLuaOverlayState {
                    depth_test: depth,
                    depth_write: depth,
                    z_bias: 0.7,
                    ..Default::default()
                };
                let camera = SongLuaDrawCamera::with_screen(
                    Some(child_camera), Some(screen), [0.0; 2],
                ).view_proj(size[0], size[1]).unwrap();
                // Native keeps ScreenGameplay's model on the world stack when
                // the child ActorFrame replaces the camera with its own FOV.
                let depth_range = song_lua_depth_range(draw);
                for world in [
                    Vector4::new(427.0, 240.0, 0.0, 1.0),
                    Vector4::new(100.0, 60.0, 150.0, 1.0),
                    Vector4::new(850.0, 470.0, -80.0, 1.0),
                ] {
                    let native = depth_range * projection * (view * screen_model) * world;
                    let rendered = song_lua_screen_proj(1.0)
                        * song_lua_projected_local_transform(camera, Matrix4::IDENTITY, draw)
                        * world;
                    assert!(rendered.is_finite());
                    for axis in 0..4 {
                        // Match the native drawable comparator, including its
                        // strict absolute bound around clip-space X/Y zero.
                        let tolerance = 0.002 + native[axis].abs() * 0.00002;
                        assert!(
                            (native[axis] - rendered[axis]).abs() <= tolerance,
                            "screen_fov={screen_fov:?} zoom_z={zoom_z} depth={depth} axis={axis}: {native:?} != {rendered:?}",
                        );
                    }
                }
            }
        }
    }
}

fn assert_clip_camera(actual: Matrix4, expected: Matrix4) {
    assert!(actual.is_finite());
    for point in [
        Vector4::W,
        Vector4::new(32.0, 0.0, 0.0, 1.0),
        Vector4::new(0.0, -32.0, 0.0, 1.0),
        Vector4::new(0.0, 0.0, 32.0, 1.0),
    ] {
        let actual = actual * point;
        let expected = expected * point;
        for axis in 0..4 {
            // The stored f32 conjugation contains nearly cancelling clip-Z/W
            // terms. Bound that XY rounding separately from native depth, then
            // also require its projected error to stay below 1/16 of a pixel.
            let tolerance = if axis < 2 { 0.125 } else { 0.005 };
            assert!(
                (actual[axis] - expected[axis]).abs() < tolerance,
                "axis={axis}, point={point:?}: {actual:?} != {expected:?}"
            );
        }
        let actual_pixel = Vector2::new(actual.x / actual.w * 427.0, actual.y / actual.w * 240.0);
        let expected_pixel = Vector2::new(
            expected.x / expected.w * 427.0,
            expected.y / expected.w * 240.0,
        );
        let pixel_error = (actual_pixel - expected_pixel).length();
        assert!(
            pixel_error < 1.0 / 16.0,
            "point={point:?}: projected error {pixel_error} pixels"
        );
    }
}

fn screen_player_transform(camera_parts: Option<(Matrix4, Matrix4)>) -> SongLuaCaptureTransform {
    SongLuaCaptureTransform {
        z_shift: 900,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: 427.0,
        target_x: -32.0,
        target_y: 0.0,
        rotation_x: 17.0,
        rotation_y: -25.0,
        rotation_z: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 2.0,
        target_z: 0.0,
        wrappers: Matrix4::IDENTITY,
        field_camera_parts: camera_parts,
    }
}

#[test]
fn screen_world_precedes_player_camera_with_singular_field_zoom() {
    let size = [854.0, 480.0];
    deadlib_present::space::set_current_metrics(Metrics::centered(size[0], size[1]));
    let mut cache = NotefieldCameraCache::default();
    cache.set_vanish_center(Some([-32.0, 0.0]));
    for child_zoom_z in [0.0, 1.0, 4.0] {
        cache.set_wrapper(Matrix4::from_scale(Vector3::new(1.0, 1.0, child_zoom_z)));
        let field = cache
            .resolve(854.0, 480.0, 427.0, 240.0, 0.0, 0.0, false)
            .unwrap();
        let (camera, model) = cache.camera_parts().unwrap();
        let transform = screen_player_transform(cache.camera_parts());
        let player = song_lua_player_camera_suffix(transform).unwrap();
        for fov in [None, Some(90.0)] {
            let screen = SongLuaOverlayState {
                x: 427.0,
                y: 240.0,
                fov,
                ..Default::default()
            };
            let world = deadsync_notefield::song_lua_screen_matrix_to_world(
                song_lua_draw_matrix(screen, [0.0; 2]).0,
                size[0],
                size[1],
            );
            let prefix = song_lua_camera_world_projection(camera, world);
            // Actor::BeginDraw retains ScreenGameplay on the world stack;
            // Player::PushPlayerMatrix replaces only the camera. The native
            // order is camera * Screen * Player * NoteField, even when the
            // child zoom is zero and cannot be inverted.
            let expected = camera * (world * (player * model));
            let direct =
                song_lua_field_camera_with_player(Some(field), cache.camera_parts(), Some(player))
                    .unwrap();
            assert_clip_camera(prefix * direct, expected);
            if child_zoom_z != 0.0 && fov.is_none() {
                let old = song_lua_screen_projection(screen, size, [0.0; 2]) * direct;
                assert!(
                    (old * Vector4::new(0.0, 0.0, 32.0, 1.0)
                        - expected * Vector4::new(0.0, 0.0, 32.0, 1.0))
                    .abs()
                    .max_element()
                        > 1.0,
                    "a screen-space translation cannot substitute for the native world order"
                );
            }
        }
    }
}

#[test]
fn screen_projection_separates_field_and_hud_for_direct_and_captured_players() {
    let metrics = Metrics::centered(854.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    let screen = SongLuaOverlayState {
        x: 427.0,
        y: 240.0,
        ..Default::default()
    };
    let world = Matrix4::from_translation(Vector3::new(427.0, -240.0, 0.0));
    let screen_projection = song_lua_screen_projection(screen, [854.0, 480.0], [0.0; 2]);
    let mut cache = NotefieldCameraCache::default();
    cache.set_vanish_center(Some([-32.0, 0.0]));
    cache.set_wrapper(Matrix4::from_scale(Vector3::new(1.0, 1.0, 3.0)));
    let field = cache
        .resolve(854.0, 480.0, 427.0, 240.0, 0.0, 0.0, false)
        .unwrap();
    let (camera, model) = cache.camera_parts().unwrap();
    let transform = screen_player_transform(cache.camera_parts());
    let player = song_lua_player_camera_suffix(transform).unwrap();
    let local_field = camera * (player * model);
    let expected_field = camera * (world * (player * model));
    let expected_hud = song_lua_player_root_camera(Matrix4::IDENTITY) * (world * player);
    for captured in [false, true] {
        let mut scratch = FrameScratch::default();
        scratch.notefield_actor_scratch[0] = vec![
            Actor::CameraPush { view_proj: field },
            mesh(),
            Actor::CameraPop,
        ];
        scratch.notefield_hud_actor_scratch[0] = vec![mesh(), mesh()];
        let mut assembly = player_actor_assembly_for_transform(captured, true, transform);
        let mut field_camera = prepare_player_field_camera(
            &mut assembly,
            Some(field),
            cache.camera_parts(),
            &mut scratch.notefield_actor_scratch[0],
        );
        if let PlayerActorAssembly::DirectTransform {
            root_camera,
            field_camera_suffix,
            ..
        } = assembly
        {
            field_camera = Some(PlayerFieldCameraCache::default().resolve(
                0,
                0,
                field_camera,
                root_camera,
                field_camera_suffix,
            ));
        }
        let mut captured_field_range = None;
        if captured {
            let mut proxies = SongLuaProxyActorScratch::new(1);
            let source = capture_player_source(
                &mut scratch.notefield_actor_scratch[0],
                &mut scratch.notefield_hud_actor_scratch[0],
                1,
                transform,
                proxies.player(0, SONG_LUA_PLAYER_PROXY_SOURCE).unwrap(),
            )
            .unwrap();
            captured_field_range = song_lua_captured_camera_range(
                song_lua_captured_segment_actors(&source[0]),
                local_field,
            );
            assert!(
                captured_field_range.is_some(),
                "capture retains one balanced field scope"
            );
            scratch.song_lua_proxy_actor_scratch = Some(proxies);
        }
        let owner = GameplayActorSegments {
            insert: 0,
            screen_actor_end: 0,
            players: [
                Some(PlayerActorSegment {
                    player: 0,
                    assembly,
                    field_camera,
                    field_screen_projection: song_lua_camera_world_projection(camera, world),
                    captured_field_range,
                    manual_hud_draw: false,
                    hud_parts: deadsync_notefield::NotefieldHudParts {
                        combo_actors: 1,
                        ..Default::default()
                    },
                }),
                None,
            ],
            direct_proxy_len: 0,
            underlay_visible: false,
            screen_projection,
        };
        let frame = deadlib_present::compose::build_passes(
            owner.segments(&scratch, &[]),
            &[],
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            0.0,
            &mut TextLayoutCache::default(),
            &mut ComposeScratch::default(),
            &NullTextureContext,
            None,
        );
        let cameras = frame
            .ops
            .iter()
            .map(|op| {
                let deadlib_render_core::DrawOp::Mesh(run) = op else {
                    panic!("mesh draw")
                };
                frame.cameras[run.camera as usize]
            })
            .collect::<Vec<_>>();
        assert_eq!(
            cameras.len(),
            3,
            "Combo, NoteField, Judgment retain their order"
        );
        for (actual, expected) in
            cameras
                .into_iter()
                .zip([expected_hud, expected_field, expected_hud])
        {
            assert_clip_camera(actual, expected);
        }
        if let Some([start, _]) = captured_field_range {
            let source = scratch
                .song_lua_proxy_actor_scratch
                .as_ref()
                .unwrap()
                .player_source(0, SONG_LUA_PLAYER_PROXY_SOURCE)
                .unwrap();
            let Actor::CameraPush { view_proj } = song_lua_captured_segment_actors(source)[start]
            else {
                panic!("retained local field camera");
            };
            assert_eq!(
                view_proj, local_field,
                "ActorProxy source stays independent of Screen"
            );
        }
    }
}

fn mesh() -> Actor {
    Actor::Mesh {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Fill; 2],
        tint: [1.0; 4],
        vertex_color_encoding: Default::default(),
        vertices: Arc::from([
            MeshVertex {
                pos: [10.0, 10.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [20.0, 10.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [10.0, 20.0],
                color: [1.0; 4],
            },
        ]),
        visible: true,
        blend: BlendMode::Alpha,
        z: 0,
    }
}

#[test]
fn screen_projection_ranges_preserve_player_and_system_boundaries() {
    let metrics = Metrics::centered(640.0, 480.0);
    let actors = [mesh(), mesh(), mesh(), mesh(), mesh(), mesh()];
    let mut scratch = FrameScratch::default();
    scratch.song_lua_screen_projection_ranges = vec![
        SongLuaScreenProjectionRange {
            start: 1,
            end: 2,
        },
        SongLuaScreenProjectionRange {
            start: 3,
            end: 4,
        },
    ];
    let owner = GameplayActorSegments {
        insert: 2,
        screen_actor_end: 4,
        players: [None, None],
        direct_proxy_len: 0,
        underlay_visible: false,
        screen_projection: Matrix4::from_scale(Vector3::splat(0.9)),
    };
    let compose = |segments: Vec<ActorSegment<'_>>| {
        deadlib_present::compose::build_passes(
            segments.into_iter(),
            &[],
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            0.0,
            &mut TextLayoutCache::default(),
            &mut ComposeScratch::default(),
            &NullTextureContext,
            None,
        )
    };
    let actual = compose(owner.segments(&scratch, &actors).collect());
    let expected = compose(vec![
        ActorSegment::new(&actors[0..1]).with_projection(&owner.screen_projection),
        ActorSegment::new(&actors[1..2]),
        ActorSegment::new(&actors[2..3]).with_projection(&owner.screen_projection),
        ActorSegment::new(&actors[3..4]),
        ActorSegment::new(&actors[4..6]),
    ]);
    assert_eq!(
        deadlib_render_core::frame_compare::compare_render_frames_semantic(&actual, &expected),
        Ok(()),
    );
    let unbounded = GameplayActorSegments {
        screen_actor_end: actors.len(),
        ..owner
    };
    let inherited = compose(unbounded.segments(&scratch, &actors).collect());
    assert!(
        deadlib_render_core::frame_compare::compare_render_frames_semantic(&inherited, &expected)
            .is_err(),
        "including application overlays in the chart span must change their rendered positions"
    );
}

#[test]
fn screen_projection_ranges_survive_background_capture() {
    for retain in [true, false] {
        let mut actors = vec![mesh(), mesh()];
        let mut source = Some(SongLuaActorSegments::new());
        let mut projections = vec![SongLuaScreenProjectionRange {
            start: 1,
            end: 2,
        }];
        song_lua_capture_screen_actors(&mut source, &mut actors, 0, None, retain, &mut projections,
            &SongLuaScreenCaptureOriginals::default());
        assert!(!source.unwrap().is_empty());
        assert_eq!(actors.len(), if retain { 2 } else { 0 });
        assert_eq!(projections.len(), usize::from(retain));
    }
}

#[test]
fn background_capture_replaces_only_world_transformed_leaf_spans() {
    let tagged_mesh = |tag| {
        let mut actor = mesh();
        let Actor::Mesh { z, .. } = &mut actor else { unreachable!() };
        *z = tag;
        actor
    };
    for retain in [true, false] {
        for pooled in [true, false] {
            let mut actors = (0..7).map(tagged_mesh).collect::<Vec<_>>();
            let originals = SongLuaScreenCaptureOriginals {
                actors: [22, 55, 56].into_iter().map(tagged_mesh).collect(),
                ranges: vec![
                    SongLuaScreenCaptureRange {
                        start: 2, end: 4, original_start: 0, original_end: 1,
                    },
                    SongLuaScreenCaptureRange {
                        start: 5, end: 6, original_start: 1, original_end: 3,
                    },
                ],
            };
            let mut projections = [(0, 1), (2, 4), (5, 6)].map(|(start, end)| {
                SongLuaScreenProjectionRange { start, end }
            }).to_vec();
            let mut scratch = SongLuaProxyActorScratch::with_proxy_counts(
                0, [0; SONG_LUA_PROXY_POOL_CLASSES]);
            let mut source = Some(SongLuaActorSegments::new());
            song_lua_capture_screen_actors(&mut source, &mut actors, 1,
                pooled.then_some(&mut scratch), retain, &mut projections, &originals);
            let source = source.unwrap();
            assert_eq!(source.len(), 1);
            let captured = match source[0].as_ref() {
                [Actor::Frame { children, .. }] => children.as_slice(),
                actors => actors,
            };
            let tags = captured.iter().map(|actor| {
                let Actor::Mesh { z, .. } = actor else { panic!("expected captured leaf") };
                *z
            }).collect::<Vec<_>>();
            assert_eq!(tags, [1, 22, 4, 55, 56, 6]);
            assert_eq!(actors.len(), if retain { 7 } else { 1 });
            for (expected, actor) in actors.iter().enumerate() {
                let Actor::Mesh { z, .. } = actor else { unreachable!() };
                assert_eq!(*z as usize, expected, "ordinary actor indices stay unchanged");
            }
            assert_eq!(projections.len(), if retain { 3 } else { 1 });
            assert_eq!((projections[0].start, projections[0].end), (0, 1));
        }
    }
}

#[test]
fn screen_transform_preserves_generic_sprite_and_quad_masks() {
    use deadlib_render_core::DrawOp;

    let metrics = Metrics::centered(640.0, 480.0);
    deadlib_present::space::set_current_metrics(metrics);
    deadlib_present::space::set_current_window_px(640, 480);
    let mut assets = AssetManager::<()>::new();
    for key in [white_texture_key(), Arc::from("screen-mask.png")] {
        assets.queue_texture_upload(
            key.to_string(), deadlib_image::RgbaImage::from_pixel(1, 1, deadlib_image::Rgba([255; 4])),
        );
    }
    let screen = SongLuaOverlayState {
        x: 30.0,
        y: -12.0,
        zoom: 2.0,
        ..Default::default()
    };
    for kind in [
        SongLuaOverlayKind::Quad,
        SongLuaOverlayKind::Sprite {
            texture_path: "screen-mask.png".into(),
            texture_key: Arc::from("screen-mask.png"),
            states: Arc::from([]),
            textures: Arc::from([]),
        },
    ] {
        let overlay: SongLuaOverlayActor<deadsync_assets::noteskin::SpriteSlot> = SongLuaOverlayActor {
            kind,
            name: None,
            parent_index: None,
            initial_state: Default::default(),
            message_commands: Vec::new(),
        };
        for (fov, generic_mask) in [
            (None, true),
            (Some(-1.0), true),
            (Some(f32::NAN), true),
            (Some(0.0), false),
            (Some(60.0), false),
        ] {
            let camera = SongLuaDrawCamera::with_screen(
                fov.map(|fov| SongLuaOverlayState { fov: Some(fov), ..Default::default() }),
                Some(screen), [0.0; 2],
            );
            let mut actors = Vec::new();
            for state in [
                SongLuaOverlayState {
                    x: 160.0,
                    y: 120.0,
                    size: Some([40.0, 30.0]),
                    mask_source: true,
                    ..Default::default()
                },
                SongLuaOverlayState {
                    x: 170.0,
                    y: 125.0,
                    size: Some([100.0, 100.0]),
                    mask_dest: true,
                    ..Default::default()
                },
            ] {
                append_song_lua_overlay_output(
                    &mut actors, &overlay, state, camera, &assets, 0,
                    [640.0, 480.0], [0.0; 2], 0.0, None, None,
                );
            }
            if !generic_mask {
                // Authored FOV cameras retain their existing projected path.
                assert_eq!(actors.len(), 2);
                assert!(actors.iter().all(|actor| matches!(actor, Actor::TexturedMesh { .. })));
                continue;
            }
            assert_eq!(actors.len(), 6);
            assert!(matches!(&actors[1], Actor::Sprite { mask_source: true, mask_dest: false, .. }));
            assert!(matches!(&actors[4], Actor::Sprite { mask_source: false, mask_dest: true, .. }));
            let frame = deadlib_present::compose::build_screen_with_texture_context(
                &actors, [0.0; 4], &metrics, &font::FontMap::default(), 0.0,
                assets.texture_context(),
            );
            let [DrawOp::Sprite(run)] = frame.ops.as_slice() else {
                panic!("only the clipped destination should draw: {:?}", frame.ops);
            };
            assert_eq!(run.instance_count, 1);
            let sprite = frame.sprite_instances[run.instance_start as usize];
            let projection = frame.cameras[run.camera as usize];
            // Clip to the source's [140,105]..[180,135] rectangle before
            // applying the screen's twofold scale and [30,-12] translation.
            for (signs, expected) in [([-1.0, 1.0], [310.0, 198.0]), ([1.0, -1.0], [390.0, 258.0])] {
                let clip = projection * Vector4::new(
                    sprite.center[0] + signs[0] * sprite.size[0] * 0.5,
                    sprite.center[1] + signs[1] * sprite.size[1] * 0.5,
                    sprite.center[2], 1.0,
                );
                let actual = [(clip.x / clip.w + 1.0) * 320.0, (1.0 - clip.y / clip.w) * 240.0];
                for axis in 0..2 {
                    assert!((actual[axis] - expected[axis]).abs() < 0.001,
                        "fov={fov:?}, axis={axis}: {actual:?} != {expected:?}");
                }
            }
        }
    }
}
