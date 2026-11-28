use deadsync_assets::song_lua::MEDIA_PROBES;
use super::*;
use deadlib_present::actors::{Background, RenderTarget};
use deadlib_present::space::{Metrics, current_metrics, with_current_metrics};
use deadlib_render_core::{DrawOp, RenderFrame, RenderTargetFrame};
use deadsync_song_lua::playback::actor_conformance::WholeSongComposer;

const SCREEN: [f32; 2] = [854.0, 480.0];

fn actor(kind: SongLuaOverlayKind, state: SongLuaOverlayState) -> SongLuaOverlayActor {
    SongLuaOverlayActor {
        kind,
        name: None,
        parent_index: None,
        initial_state: state,
        message_commands: Vec::new(),
    }
}

fn scene(size: [f32; 2]) -> Vec<SongLuaOverlayActor> {
    let capture = |name: &str, size| {
        actor(
            SongLuaOverlayKind::ActorFrameTexture {
                capture_name: name.into(),
                alpha_buffer: true,
                depth_buffer: true,
                float_buffer: false,
                capture_size: Some(size),
            },
            SongLuaOverlayState {
                aft_created: true,
                ..Default::default()
            },
        )
    };
    vec![
        actor(SongLuaOverlayKind::ActorFrame, Default::default()),
        capture("outer", size),
        capture("inner", [5.0, 3.0]),
        actor(
            SongLuaOverlayKind::ActorMultiVertex {
                vertices: triangle([1.0, 1.0], 0.0),
                texture_path: None,
                texture_key: None,
                capture_name: Some("outer".into()),
            },
            Default::default(),
        ),
        actor(SongLuaOverlayKind::Quad, Default::default()),
    ]
}

fn triangle(size: [f32; 2], z: f32) -> Arc<[SongLuaOverlayMeshVertex]> {
    [[0.0, 0.0], [size[0], 0.0], [0.0, size[1]]]
        .map(|pos| SongLuaOverlayMeshVertex {
            pos,
            z,
            uv: [0.0; 2],
            color: [1.0; 4],
        })
        .into()
}

fn draw(
    source: SongLuaDrawSource,
    state: SongLuaOverlayState,
    camera: Option<SongLuaOverlayState>,
    vertices: Option<Arc<[SongLuaOverlayMeshVertex]>>,
) -> SongLuaDrawOp {
    SongLuaDrawOp::Draw {
        source,
        state,
        parents: Arc::from([]),
        camera,
        vertices,
        inherited_diffuse: [1.0; 4],
        inherited_glow: [0.0; 4],
    }
}

fn program(ops: Vec<SongLuaDrawOp>) -> SongLuaDrawFrame {
    SongLuaDrawFrame {
        second: 0.0,
        immediate: false,
        owners: vec![0],
        owner_ends: vec![ops.len()],
        ops: ops.into(),
    }
}

fn capture_program(draw: SongLuaDrawOp) -> SongLuaDrawFrame {
    program(vec![
        SongLuaDrawOp::Begin {
            capture: 1,
            preserve: false,
        },
        draw,
        SongLuaDrawOp::Finish { capture: 1 },
    ])
}

fn assets() -> AssetManager {
    let mut assets = AssetManager::new();
    assets.queue_texture_upload(
        white_texture_key().to_string(),
        deadlib_image::RgbaImage::from_pixel(1, 1, deadlib_image::Rgba([255; 4])),
    );
    assets
}

fn render_with_scratch(
    overlays: &[SongLuaOverlayActor],
    scratch: &mut SongLuaManualScratch,
    sources: &SongLuaScreenProxySources<'_>,
) -> RenderFrame {
    let metrics = Metrics::centered(SCREEN[0], SCREEN[1]);
    with_current_metrics(metrics, || {
        let states = overlays
            .iter()
            .map(|actor| actor.initial_state)
            .collect::<Vec<_>>();
        let topology = SongLuaOverlayTopologyIndex::new(overlays);
        let mut actors = Vec::new();
        let mut targets = Vec::<RenderTarget>::new();
        let assets = assets();
        scratch.begin_frame();
        song_lua_draw_owner(
            &mut actors,
            &mut targets,
            overlays,
            &states,
            &topology,
            scratch,
            0,
            0,
            sources,
            &assets,
            SCREEN,
            [0.0; 2],
            0.0,
            0,
        );
        assert_eq!([screen_width(), screen_height()], SCREEN);
        deadlib_present::compose::build_passes(
            std::iter::once(ActorSegment::new(&actors)),
            &targets,
            [0.0; 4],
            &metrics,
            &font::FontMap::default(),
            0.0,
            &mut TextLayoutCache::default(),
            &mut ComposeScratch::default(),
            assets.texture_context(),
            None,
        )
    })
}

fn render(overlays: &[SongLuaOverlayActor], frame: &SongLuaDrawFrame) -> RenderFrame {
    let mut scratch = SongLuaManualScratch::with_frames(overlays, Arc::from([frame.clone()]));
    render_with_scratch(
        overlays,
        &mut scratch,
        &SongLuaScreenProxySources::default(),
    )
}

fn recorded(overlays: &[SongLuaOverlayActor], frame: &SongLuaDrawFrame) -> Vec<RenderFrame> {
    let states = overlays
        .iter()
        .map(|actor| actor.initial_state)
        .collect::<Vec<_>>();
    WholeSongComposer::new(overlays, MEDIA_PROBES.movie_dimensions)
        .render_recorded_frame(overlays, &states, frame, SCREEN, 0.0, 0.0)
        .into_iter()
        .map(|(_, frame)| frame)
        .collect()
}

fn target_frame(target: &RenderTargetFrame) -> RenderFrame {
    RenderFrame {
        clear_color: [0.0; 4],
        render_targets: Vec::new(),
        cameras: target.cameras.clone(),
        sprite_instances: target.sprite_instances.clone(),
        mesh_vertices: target.mesh_vertices.clone(),
        tmesh_instances: target.tmesh_instances.clone(),
        tmesh_geometries: target.tmesh_geometries.clone(),
        ops: target.ops.clone(),
    }
}

fn mesh_clips(frame: &RenderFrame) -> Vec<Vec<Vector4>> {
    frame
        .ops
        .iter()
        .filter_map(|op| {
            let DrawOp::TexturedMesh(run) = op else {
                return None;
            };
            assert_eq!(run.instance_count, 1);
            let transform = frame.cameras[run.camera as usize]
                * frame.tmesh_instances[run.instance_start as usize].transform();
            Some(
                frame.tmesh_geometries[run.geometry as usize]
                    .vertices
                    .iter()
                    .map(|vertex| {
                        transform * Vector4::new(vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0)
                    })
                    .collect(),
            )
        })
        .collect()
}

fn native_ortho(pos: [f32; 3], size: [f32; 2]) -> Vector4 {
    // RageDisplay::LoadMenuPerspective(0), independent of playback helpers.
    Vector4::new(
        2.0 * pos[0] / size[0] - 1.0,
        1.0 - 2.0 * pos[1] / size[1],
        -pos[2] / 1000.0,
        1.0,
    )
}

fn assert_clip(actual: Vector4, expected: Vector4) {
    for axis in 0..4 {
        assert!(
            (actual[axis] - expected[axis]).abs() <= 0.0001 + expected[axis].abs() * 0.000002,
            "axis {axis}: {actual:?} != native {expected:?}"
        );
    }
}

fn assert_triangle(actual: &[Vector4], size: [f32; 2], offset: [f32; 3], viewport: [f32; 2]) {
    assert_eq!(actual.len(), 3);
    for (actual, pos) in actual
        .iter()
        .zip([[0.0, 0.0], [size[0], 0.0], [0.0, size[1]]])
    {
        assert_clip(
            *actual,
            native_ortho(
                [pos[0] + offset[0], pos[1] + offset[1], offset[2]],
                viewport,
            ),
        );
    }
}

fn assert_fill_quad(frame: &RenderFrame) {
    let [DrawOp::Sprite(run)] = frame.ops.as_slice() else {
        panic!("expected one plain Quad")
    };
    let instance = &frame.sprite_instances[run.instance_start as usize];
    assert_eq!(instance.rot_sin_cos, [0.0, 1.0]);
    assert_eq!(instance.local_offset, [0.0; 2]);
    let camera = frame.cameras[run.camera as usize];
    for (x, y) in [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
        let clip = camera
            * Vector4::new(
                instance.center[0] + x * instance.size[0] / 2.0,
                instance.center[1] + y * instance.size[1] / 2.0,
                instance.center[2],
                1.0,
            );
        assert_clip(clip, Vector4::new(x, y, 0.0, 1.0));
    }
}

#[test]
fn manual_target_space_small_quad_fills_image_viewport_in_both_paths() {
    for size in [[8.0, 8.0], [13.0, 7.0]] {
        let overlays = scene(size);
        let frame = capture_program(draw(
            SongLuaDrawSource::Overlay(4),
            SongLuaOverlayState {
                x: size[0] / 2.0,
                y: size[1] / 2.0,
                size: Some(size),
                ..Default::default()
            },
            None,
            None,
        ));
        let production = render(&overlays, &frame);
        assert_eq!(production.render_targets.len(), 1);
        let target = &production.render_targets[0];
        assert_eq!(target.viewport, size.map(|value| value as u32));
        assert_eq!(
            [target.width, target.height],
            size.map(|value| (value as u32).next_power_of_two())
        );
        assert_fill_quad(&target_frame(target));
        let adapter = recorded(&overlays, &frame);
        assert_eq!(adapter.len(), 1);
        assert_fill_quad(&adapter[0]);
    }
}

#[test]
fn manual_target_space_nested_targets_restore_outer_and_screen_dimensions() {
    let size = [13.0, 7.0];
    let overlays = scene(size);
    let mesh = |size, x, y| {
        draw(
            SongLuaDrawSource::Overlay(3),
            SongLuaOverlayState {
                x,
                y,
                ..Default::default()
            },
            None,
            Some(triangle(size, 0.0)),
        )
    };
    let frame = program(vec![
        SongLuaDrawOp::Begin {
            capture: 1,
            preserve: false,
        },
        mesh(size, 0.0, 0.0),
        SongLuaDrawOp::Begin {
            capture: 2,
            preserve: false,
        },
        mesh([5.0, 3.0], 0.0, 0.0),
        SongLuaDrawOp::Finish { capture: 2 },
        mesh(size, 2.0, 1.0),
        SongLuaDrawOp::Finish { capture: 1 },
        mesh(SCREEN, 0.0, 0.0),
    ]);
    let original = Metrics {
        left: -100.0,
        right: 754.0,
        top: 200.0,
        bottom: -280.0,
    };
    with_current_metrics(original, || {
        let production = render(&overlays, &frame);
        let restored = current_metrics();
        assert_eq!(
            [restored.left, restored.right, restored.top, restored.bottom],
            [original.left, original.right, original.top, original.bottom]
        );
        assert_eq!(production.render_targets.len(), 2);
        let inner = &production.render_targets[0];
        let outer = &production.render_targets[1];
        assert_eq!(inner.viewport, [5, 3]);
        assert_eq!(outer.viewport, [13, 7]);
        assert_triangle(
            &mesh_clips(&target_frame(inner))[0],
            [5.0, 3.0],
            [0.0; 3],
            [5.0, 3.0],
        );
        let outer_clips = mesh_clips(&target_frame(outer));
        assert_eq!(outer_clips.len(), 2);
        assert_triangle(&outer_clips[0], size, [0.0; 3], size);
        assert_triangle(&outer_clips[1], size, [2.0, 1.0, 0.0], size);
        assert_triangle(&mesh_clips(&production)[0], SCREEN, [0.0; 3], SCREEN);
    });
    let adapter = recorded(&overlays, &frame);
    assert_eq!(adapter.len(), 4);
    for (draw, triangle_size, offset, viewport) in [
        (0, size, [0.0; 3], size),
        (1, [5.0, 3.0], [0.0; 3], [5.0, 3.0]),
        (2, size, [2.0, 1.0, 0.0], size),
        (3, SCREEN, [0.0; 3], SCREEN),
    ] {
        assert_triangle(
            &mesh_clips(&adapter[draw])[0],
            triangle_size,
            offset,
            viewport,
        );
    }
}

#[test]
fn manual_target_space_nonzero_mesh_depth_uses_native_range_once() {
    let size = [8.0, 8.0];
    let overlays = scene(size);
    for depth in [false, true] {
        let frame = capture_program(draw(
            SongLuaDrawSource::Overlay(3),
            SongLuaOverlayState {
                z: 100.0,
                depth_test: depth,
                depth_write: depth,
                z_bias: 0.3,
                ..Default::default()
            },
            None,
            Some(triangle(size, 80.0)),
        ));
        let production = render(&overlays, &frame);
        let adapter = recorded(&overlays, &frame);
        for frame in [
            target_frame(&production.render_targets[0]),
            adapter.into_iter().next().unwrap(),
        ] {
            let clips = mesh_clips(&frame);
            for (actual, pos) in clips[0].iter().zip([[0.0, 0.0], [8.0, 0.0], [0.0, 8.0]]) {
                let mut expected = native_ortho([pos[0], pos[1], 180.0], size);
                if depth {
                    expected.z = 0.95 * expected.z + 0.05 - 0.1 * 0.3;
                }
                assert_clip(*actual, expected);
            }
        }
    }
}

#[test]
fn manual_target_space_explicit_zero_and_perspective_fov_keep_original_screen() {
    let size = [8.0, 8.0];
    let overlays = scene(size);
    for fov in [0.0, 45.0] {
        let camera = SongLuaOverlayState {
            fov: Some(fov),
            vanishpoint: Some([SCREEN[0] / 2.0, SCREEN[1] / 2.0]),
            ..Default::default()
        };
        let frame = capture_program(draw(
            SongLuaDrawSource::Overlay(3),
            SongLuaOverlayState {
                x: 2.0,
                y: 3.0,
                z: 100.0,
                ..Default::default()
            },
            Some(camera),
            Some(triangle(size, 80.0)),
        ));
        let production = render(&overlays, &frame);
        let adapter = recorded(&overlays, &frame);
        for frame in [
            target_frame(&production.render_targets[0]),
            adapter.into_iter().next().unwrap(),
        ] {
            for (actual, pos) in
                mesh_clips(&frame)[0]
                    .iter()
                    .zip([[2.0, 3.0], [10.0, 3.0], [2.0, 11.0]])
            {
                let expected = if fov == 0.0 {
                    native_ortho([pos[0], pos[1], 180.0], SCREEN)
                } else {
                    // RageDisplay's centered menu frustum with near=1 and
                    // far=distance+1000; no playback projection helper is used.
                    let distance = SCREEN[0] / 2.0 / (45.0f32.to_radians() / 2.0).tan();
                    let far = distance + 1000.0;
                    let eye_z = 180.0 - distance;
                    Vector4::new(
                        2.0 * distance / SCREEN[0] * (pos[0] - SCREEN[0] / 2.0),
                        -2.0 * distance / SCREEN[1] * (pos[1] - SCREEN[1] / 2.0),
                        -(far + 1.0) / (far - 1.0) * eye_z - 2.0 * far / (far - 1.0),
                        -eye_z,
                    )
                };
                assert_clip(*actual, expected);
            }
        }
    }
}

fn source_children(origin: [f32; 2]) -> Arc<[Actor]> {
    Arc::from([
        Actor::Frame {
            align: [0.0; 2],
            offset: [origin[0] + 1.0, origin[1] + 2.0],
            size: [SizeSpec::Px(4.0), SizeSpec::Px(2.0)],
            children: Vec::new(),
            background: Some(Background::Color([1.0; 4])),
            z: 0,
        },
        Actor::Frame {
            align: [0.0; 2],
            offset: [0.0; 2],
            size: [SizeSpec::Fill; 2],
            children: Vec::new(),
            background: Some(Background::Color([1.0; 4])),
            z: 1,
        },
        // A composable source camera preserves layout and SharedTransform remapping.
        // Retained SharedCamera deliberately bypasses both for absolute replay.
        Actor::Camera {
            view_proj: Metrics::centered(SCREEN[0], SCREEN[1]).projection(),
            children: vec![Actor::Mesh {
                align: [0.0; 2],
                offset: [0.0; 2],
                size: [SizeSpec::Fill; 2],
                tint: [1.0; 4],
                vertex_color_encoding: Default::default(),
                vertices: [[3.0, 4.0], [4.0, 4.0], [3.0, 5.0]]
                    .map(|pos| MeshVertex {
                        pos: [pos[0] + origin[0], pos[1] + origin[1]],
                        color: [1.0; 4],
                    })
                    .into(),
                visible: true,
                blend: BlendMode::Alpha,
                z: 2,
            }],
        },
    ])
}

#[test]
fn manual_target_space_player_and_screen_sources_keep_original_layout_and_camera() {
    let size = [13.0, 7.0];
    let overlays = scene(size);
    for player in [false, true] {
        let origin = if player {
            [SCREEN[0] / 2.0, SCREEN[1] / 2.0]
        } else {
            [0.0; 2]
        };
        let source = [source_children(origin)];
        let raw = SongLuaManualPlayerSource {
            field: None,
            combo: None,
            judgment: Some(source.clone()),
            origin,
            camera: NotefieldCameraCache::default(),
        };
        let sources = if player {
            SongLuaScreenProxySources {
                manual_players: [Some(&raw), None],
                ..Default::default()
            }
        } else {
            SongLuaScreenProxySources {
                underlay: Some(&source),
                ..Default::default()
            }
        };
        let frame = capture_program(draw(
            if player {
                SongLuaDrawSource::Player(0)
            } else {
                SongLuaDrawSource::ScreenLayer(0)
            },
            SongLuaOverlayState {
                x: 2.0,
                y: 1.0,
                ..Default::default()
            },
            None,
            None,
        ));
        let mut scratch = SongLuaManualScratch::with_frames(&overlays, Arc::from([frame]));
        for _ in 0..4 {
            let result = render_with_scratch(&overlays, &mut scratch, &sources);
            let target = &result.render_targets[0];
            let sprites = target
                .ops
                .iter()
                .filter_map(|op| match op {
                    DrawOp::Sprite(run) => Some(run),
                    _ => None,
                })
                .flat_map(|run| {
                    let start = run.instance_start as usize;
                    let end = start + run.instance_count as usize;
                    target.sprite_instances[start..end]
                        .iter()
                        .map(move |instance| (run.camera, instance))
                })
                .collect::<Vec<_>>();
            assert_eq!(sprites.len(), 2);
            let (camera, first) = sprites[0];
            assert_eq!(first.size, [4.0, 2.0]);
            let clip = target.cameras[camera as usize]
                * Vector4::new(first.center[0], first.center[1], first.center[2], 1.0);
            assert_clip(clip, native_ortho([5.0, 4.0, 0.0], size));
            let (_, fill) = sprites[1];
            assert_eq!(
                fill.size, SCREEN,
                "Fill resolves in the source's original screen extent"
            );
            let mesh = target
                .ops
                .iter()
                .find_map(|op| match op {
                    DrawOp::Mesh(run) => Some(run),
                    _ => None,
                })
                .expect("nested source camera mesh");
            let pos = target.mesh_vertices[mesh.vertex_start as usize].pos;
            let clip =
                target.cameras[mesh.camera as usize] * Vector4::new(pos[0], pos[1], 0.0, 1.0);
            assert_clip(clip, native_ortho([5.0, 5.0, 0.0], size));
        }
        for bank in &scratch.banks {
            let stats = bank[1]
                .source_layout
                .as_ref()
                .expect("pooled source layout")
                .stats();
            assert_eq!(stats.growths, 0);
            assert_eq!(
                stats.replacements, 0,
                "released frames reuse source layout storage"
            );
        }
    }
}
