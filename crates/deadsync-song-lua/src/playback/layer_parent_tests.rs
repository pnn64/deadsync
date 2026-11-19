use super::*;

type Slot = deadsync_assets::noteskin::SpriteSlot;
type Overlay = SongLuaOverlayActor<Slot>;

fn overlay(
    kind: SongLuaOverlayKind<Slot>,
    parent: Option<usize>,
    state: SongLuaOverlayState,
) -> Overlay {
    Overlay {
        kind,
        name: None,
        parent_index: parent,
        initial_state: state,
        message_commands: Vec::new(),
    }
}

fn clip_bounds(frame: &deadlib_render_core::RenderFrame) -> [f32; 4] {
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut count = 0;
    for op in &frame.ops {
        let deadlib_render_core::DrawOp::TexturedMesh(run) = op else {
            panic!("screen transform must use projected geometry");
        };
        for instance in
            &frame.tmesh_instances[run.instance_start as usize..][..run.instance_count as usize]
        {
            let transform = frame.cameras[run.camera as usize] * instance.transform();
            for vertex in frame.tmesh_geometries[run.geometry as usize]
                .vertices
                .iter()
            {
                let clip =
                    transform * Vector4::new(vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0);
                assert!(clip.w > 0.0);
                let xy = [clip.x / clip.w, clip.y / clip.w];
                for axis in 0..2 {
                    bounds[axis] = bounds[axis].min(xy[axis]);
                    bounds[axis + 2] = bounds[axis + 2].max(xy[axis]);
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 6);
    bounds
}

#[test]
fn retained_layer_parent_compensates_screen_translation_in_production_and_replay() {
    crate::tests::init_paths();
    let size = [854.0, 480.0];
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        size[0], size[1],
    ));
    let local = [SongLuaOverlayState {
        x: size[0] * 0.5,
        y: size[1] * 0.5,
        size: Some(size),
        ..Default::default()
    }];
    let overlays = [overlay(SongLuaOverlayKind::Quad, None, local[0])];
    let parents = [
        SongLuaOverlayState {
            x: -size[0] * 0.5,
            y: -size[1] * 0.5,
            ..Default::default()
        },
        SongLuaOverlayState {
            x: 10.0,
            y: 20.0,
            ..Default::default()
        },
    ];
    let parent = song_lua_layer_parent_state::<Slot>(parents[0], parents[1], size, [0.0; 2]);
    let states = compose_overlay_states(&overlays, &local, size, [0.0; 2]);
    let replay_states = compose_layer_overlay_states(&overlays, &local, size, [0.0; 2], parents);
    let mut composer = WholeSongComposer::new(&overlays);
    for fov in [None, Some(0.0)] {
        let screen = SongLuaOverlayState {
            x: size[0] * 0.5,
            y: size[1] * 0.5,
            fov,
            ..Default::default()
        };
        let mut actors = Vec::new();
        let mut targets = Vec::new();
        let mut ranges = Vec::new();
        let mut order = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut aft = SongLuaAftCaptureScratch::new(&overlays, &composer.topology);
        push_song_lua_layer_actors(
            &mut actors,
            &mut targets,
            &overlays,
            &mut order,
            &composer.topology,
            &local,
            &states,
            parent,
            Some(screen),
            &mut ranges,
            None,
            &SongLuaScreenProxySources::default(),
            None,
            None,
            &composer.assets,
            size[0],
            size[1],
            0.0,
            0.0,
            0.0,
            &mut Vec::new(),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut aft,
            &mut composer.mesh_scratch,
            SONG_LUA_BACKGROUND_DEPTH,
        );
        let production = deadlib_present::compose::build_screen_with_texture_context(
            &actors,
            [0.0; 4],
            &deadlib_present::space::Metrics::centered(size[0], size[1]),
            &font::FontMap::default(),
            0.0,
            composer.assets.texture_context(),
        );
        let replay = composer.render_screen_overlay(
            &overlays,
            &replay_states,
            0,
            size,
            0.0,
            0.0,
            Some(screen),
        );
        let expected = [
            -1.0 + 20.0 / size[0],
            -1.0 - 40.0 / size[1],
            1.0 + 20.0 / size[0],
            1.0 - 40.0 / size[1],
        ];
        for frame in [&production, &replay] {
            let actual = clip_bounds(frame);
            for axis in 0..4 {
                assert!(
                    (actual[axis] - expected[axis]).abs() < 0.00001,
                    "fov={fov:?}: {actual:?} != {expected:?}"
                );
            }
        }
        assert!(targets.is_empty());
        assert_eq!(ranges.len(), 1);
    }
}

#[test]
fn retained_layer_parent_stops_at_aft_capture_contents() {
    let size = [854.0, 480.0];
    let overlays = [
        overlay(
            SongLuaOverlayKind::ActorFrame,
            None,
            SongLuaOverlayState::default(),
        ),
        overlay(
            SongLuaOverlayKind::Quad,
            Some(0),
            SongLuaOverlayState {
                x: 30.0,
                y: 20.0,
                size: Some([8.0, 4.0]),
                ..Default::default()
            },
        ),
        overlay(
            SongLuaOverlayKind::ActorFrameTexture {
                capture_name: "capture".into(),
                capture_size: Some([64.0, 32.0]),
                float_buffer: false,
                alpha_buffer: true,
                depth_buffer: false,
            },
            Some(0),
            SongLuaOverlayState {
                size: Some([64.0, 32.0]),
                aft_created: true,
                ..Default::default()
            },
        ),
        overlay(
            SongLuaOverlayKind::Quad,
            Some(2),
            SongLuaOverlayState {
                x: 16.0,
                y: 12.0,
                size: Some([8.0, 4.0]),
                ..Default::default()
            },
        ),
    ];
    let local = overlays
        .iter()
        .map(|actor| actor.initial_state)
        .collect::<Vec<_>>();
    let baseline = compose_overlay_states(&overlays, &local, size, [0.0; 2]);
    let parent = SongLuaOverlayState {
        x: -427.0,
        y: -240.0,
        ..Default::default()
    };
    let actual = compose_layer_overlay_states(
        &overlays,
        &local,
        size,
        [0.0; 2],
        [parent, SongLuaOverlayState::default()],
    );
    assert_eq!([actual[1].x, actual[1].y], [-397.0, -220.0]);
    assert_eq!(
        actual[3], baseline[3],
        "render-target contents keep their texture-space pose"
    );
}
