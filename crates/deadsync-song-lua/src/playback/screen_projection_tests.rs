use super::*;
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

fn mesh() -> Actor {
    Actor::Mesh {
        align: [0.0; 2],
        offset: [0.0; 2],
        size: [SizeSpec::Fill; 2],
        tint: [1.0; 4],
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
fn screen_projection_ranges_preserve_default_spans_and_player_boundary() {
    let metrics = Metrics::centered(640.0, 480.0);
    let actors = [mesh(), mesh(), mesh(), mesh()];
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
    ]);
    assert_eq!(
        deadlib_render_core::frame_compare::compare_render_frames_semantic(&actual, &expected),
        Ok(()),
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
    let mut assets = AssetManager::new();
    for key in [white_texture_key(), Arc::from("screen-mask.png")] {
        assets.queue_texture_upload(
            key.to_string(), image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
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
