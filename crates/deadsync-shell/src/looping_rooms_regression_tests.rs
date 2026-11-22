mod looping_rooms {
    use super::*;
    use serde::Deserialize;
    use std::collections::{BTreeMap, HashMap, HashSet};

    #[derive(Deserialize)]
    struct OptionDeltas {
        song: f32,
        players: [f32; 2],
    }

    #[derive(Deserialize)]
    struct Update {
        sequence: u64,
        delta: f32,
        seconds: f32,
        visible_seconds: f32,
        visible_beat: f32,
        wall_seconds: f64,
        option_deltas: OptionDeltas,
    }

    #[derive(Deserialize)]
    struct Context {
        kind: String,
        player: usize,
        column: usize,
        part: i32,
    }

    #[derive(Deserialize)]
    struct ReceptorState {
        render_target: u64,
        base_alpha: f32,
        note_context: Context,
    }

    #[derive(Deserialize)]
    struct ReceptorVisit {
        update_sequence: u64,
        state: ReceptorState,
    }

    #[derive(Deserialize)]
    struct SpriteState {
        render_target: u64,
        blend: u32,
        note_context: Option<Context>,
    }

    #[derive(Deserialize)]
    struct SpriteSample {
        update_sequence: u64,
        state: SpriteState,
        vertices: Vec<[f32; 13]>,
    }

    #[derive(Deserialize)]
    struct ModelState {
        render_target: u64,
        mesh_index: usize,
        mesh_name: String,
        note_context: Option<Context>,
        anchors_clip: [[f32; 4]; 4],
        render_state: serde_json::Value,
    }

    #[derive(Deserialize)]
    struct ModelSample {
        update_sequence: u64,
        geometry: u64,
        state: ModelState,
    }

    #[derive(Deserialize)]
    struct Geometry {
        id: u64,
        vertices: Vec<[f32; 3]>,
    }

    #[derive(Deserialize)]
    struct Capture {
        oracle: String,
        input: serde_json::Value,
        display: serde_json::Value,
        capabilities: serde_json::Value,
        provenance: serde_json::Value,
        dropped_model_geometry_samples: u64,
        dropped_drawable_samples: u64,
        dropped_receptor_visits: u64,
        dropped_script_errors: u64,
        script_errors: Vec<serde_json::Value>,
        update_frames: Vec<Update>,
        model_geometry_meshes: Vec<Geometry>,
        model_geometry_samples: Vec<ModelSample>,
        drawable_samples: Vec<SpriteSample>,
        receptor_visits: Vec<ReceptorVisit>,
    }

    #[derive(Clone, Debug)]
    struct Draw {
        // Zero is a receptor quad; one and two are Cel's Model meshes.
        kind: usize,
        blend: deadlib_render_core::BlendMode,
        texture_mask: bool,
        alpha: f32,
        clip: [[f32; 4]; 4],
    }

    #[derive(Default)]
    struct NativeFrame {
        visits: [Option<f32>; 4],
        draws: Vec<Draw>,
    }

    fn section(seconds: f32) -> Option<usize> {
        [(35.4, 37.4), (69.4, 71.4), (82.8, 84.5), (129.8, 131.5)]
            .iter()
            .position(|&(start, end)| seconds >= start && seconds <= end)
    }

    fn clip_error(actual: &[f32; 4], expected: &[f32; 4]) -> f32 {
        if !actual.iter().chain(expected).all(|value| value.is_finite())
            || actual[3].signum() != expected[3].signum()
        {
            return f32::INFINITY;
        }
        actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max)
    }

    fn draw_error(actual: &Draw, expected: &Draw) -> f32 {
        let alpha_tolerance = if expected.kind == 0 {
            1.0 / 255.0 + 0.00001
        } else {
            0.0001
        };
        if actual.kind != expected.kind
            || actual.blend != expected.blend
            || actual.texture_mask != expected.texture_mask
            || !actual.alpha.is_finite()
            || !expected.alpha.is_finite()
            || (actual.alpha - expected.alpha).abs() > alpha_tolerance
        {
            return f32::INFINITY;
        }
        if actual.kind != 0 {
            return actual
                .clip
                .iter()
                .zip(&expected.clip)
                .map(|(a, b)| clip_error(a, b))
                .fold(0.0, f32::max);
        }
        // Sprite winding differs between backends. Match the four actual
        // submitted corners bijectively, retaining Z/W and perspective tilt.
        let mut best = f32::INFINITY;
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    if a == b || a == c || b == c {
                        continue;
                    }
                    let d = 6 - a - b - c;
                    let error = [a, b, c, d]
                        .into_iter()
                        .enumerate()
                        .map(|(i, j)| clip_error(&actual.clip[i], &expected.clip[j]))
                        .fold(0.0, f32::max);
                    best = best.min(error);
                }
            }
        }
        best
    }

    fn compare_draws(expected: &[Draw], mut actual: Vec<Draw>) -> Result<f32, String> {
        if actual.len() != expected.len() {
            let mut unexpected = actual.clone();
            let missing: Vec<_> = expected
                .iter()
                .filter(|native| {
                    let candidate = unexpected.iter().enumerate()
                        .map(|(index, draw)| (index, draw_error(draw, native)))
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    if let Some((index, error)) = candidate && error < 0.5 {
                        unexpected.swap_remove(index);
                        false
                    } else {
                        true
                    }
                })
                .collect();
            return Err(format!(
                "submitted draws: actual {}, native {}; unexpected {unexpected:?}; missing {missing:?}",
                actual.len(),
                expected.len()
            ));
        }
        let mut max_error = 0.0f32;
        for native in expected {
            let (index, error) = actual
                .iter()
                .enumerate()
                .map(|(index, draw)| (index, draw_error(draw, native)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("remaining native draw");
            let draw = actual.swap_remove(index);
            if error >= 0.5 {
                return Err(format!(
                    "native {native:?}, actual {draw:?}, clip error {error}"
                ));
            }
            max_error = max_error.max(error);
        }
        Ok(max_error)
    }

    fn actual_draws(
        frame: &deadlib_render_core::RenderFrame,
        metrics: &space::Metrics,
        receptor_handles: &HashSet<deadlib_render_core::TextureHandle>,
        tap_handles: &HashSet<deadlib_render_core::TextureHandle>,
        native_vertices: &[Vec<[f32; 3]>; 2],
    ) -> Vec<Draw> {
        let meshes: Vec<_> = frame
            .tmesh_geometries
            .iter()
            .map(|geometry| {
                native_vertices.iter().position(|native| {
                    geometry.vertices.len() == native.len()
                        && geometry
                            .vertices
                            .iter()
                            .zip(native)
                            .all(|(actual, native)| {
                                (0..3).all(|axis| (actual.pos[axis] - native[axis]).abs() < 0.00001)
                            })
                })
            })
            .collect();
        let mut draws = Vec::new();
        for op in &frame.ops {
            match op {
                DrawOp::Sprite(run)
                    if receptor_handles.contains(&run.texture_handle)
                        && run.blend == deadlib_render_core::BlendMode::Alpha =>
                {
                    let camera = frame
                        .cameras
                        .get(run.camera as usize)
                        .copied()
                        .unwrap_or_else(|| metrics.projection());
                    for sprite in &frame.sprite_instances[run.instance_start as usize
                        ..(run.instance_start + run.instance_count) as usize]
                    {
                        let [sin, cos] = sprite.rot_sin_cos;
                        let [offset_sin, offset_cos] = sprite.local_offset_rot_sin_cos;
                        let [offset_x, offset_y] = sprite.local_offset;
                        let x = sprite.center[0] + offset_cos * offset_x - offset_sin * offset_y;
                        let y = sprite.center[1] + offset_sin * offset_x + offset_cos * offset_y;
                        let clip = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]].map(|p| {
                            let px = p[0] * sprite.size[0];
                            let py = p[1] * sprite.size[1];
                            camera
                                .mul_vec4(
                                    [
                                        x + cos * px - sin * py,
                                        y + sin * px + cos * py,
                                        sprite.center[2],
                                        1.0,
                                    ]
                                    .into(),
                                )
                                .to_array()
                        });
                        draws.push(Draw {
                            kind: 0,
                            blend: run.blend,
                            texture_mask: sprite.texture_mask != 0.0,
                            alpha: sprite.tint[3],
                            clip,
                        });
                    }
                }
                DrawOp::TexturedMesh(run) => {
                    let receptor = receptor_handles.contains(&run.texture_handle)
                        && run.blend == deadlib_render_core::BlendMode::Alpha;
                    let mesh = tap_handles
                        .contains(&run.texture_handle)
                        .then_some(meshes[run.geometry as usize])
                        .flatten();
                    if !receptor && mesh.is_none() {
                        continue;
                    }
                    let camera = frame
                        .cameras
                        .get(run.camera as usize)
                        .copied()
                        .unwrap_or_else(|| metrics.projection());
                    for instance in &frame.tmesh_instances[run.instance_start as usize
                        ..(run.instance_start + run.instance_count) as usize]
                    {
                        let matrix = camera * instance.transform();
                        let clip = if receptor {
                            let mut corners = Vec::new();
                            for vertex in frame.tmesh_geometries[run.geometry as usize]
                                .vertices
                                .iter()
                            {
                                let point = matrix
                                    .mul_vec4(
                                        [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0].into(),
                                    )
                                    .to_array();
                                if !corners.contains(&point) {
                                    corners.push(point);
                                }
                            }
                            corners
                                .try_into()
                                .expect("rotated receptor retains four quad corners")
                        } else {
                            [
                                matrix.w_axis.to_array(),
                                (matrix.w_axis + 32.0 * matrix.x_axis).to_array(),
                                (matrix.w_axis + 32.0 * matrix.y_axis).to_array(),
                                (matrix.w_axis + 32.0 * matrix.z_axis).to_array(),
                            ]
                        };
                        draws.push(Draw {
                            kind: if receptor { 0 } else { mesh.unwrap() + 1 },
                            blend: run.blend,
                            texture_mask: instance.texture_mask != 0.0,
                            alpha: instance.tint[3],
                            clip,
                        });
                    }
                }
                _ => {}
            }
        }
        draws
    }

    /// Only observed clock inputs enter gameplay. Options, receptor positions,
    /// opacity, note visibility and Model transforms must come from the SSC and
    /// ordinary non-Lua update/composition paths.
    #[test]
    fn looping_rooms_matches_native_attack_transitions_and_final_tilt() {
        use sha2::{Digest, Sha256};
        let decoder = zstd::stream::read::Decoder::new(
            include_bytes!(
                "../../../tests/fixtures/itgmania-song-lua-micro/looping-rooms-native.json.zst"
            )
            .as_slice(),
        )
        .unwrap();
        let native: Capture = serde_json::from_reader(std::io::BufReader::new(decoder)).unwrap();
        assert_eq!(native.oracle, "itgmania_notefield_render_trace");
        assert_eq!(native.input["player_options"], "m690, -10% mini, cel");
        assert_eq!(
            native.input["native_source"]["revision"],
            "663525dcb86eb570d0ef6a2e209678436660ae26"
        );
        assert_eq!(native.input["native_source"]["modified_src"], false);
        assert_eq!(native.input["autoplay"], 1);
        assert_eq!(native.input["window_visible"], false);
        assert_eq!(native.input["players"].as_array().unwrap().len(), 1);
        assert_eq!(native.input["players"][0]["player"], "PlayerNumber_P1");
        assert_eq!(native.input["players"][0]["noteskin"], "cel");
        assert!((native.input["players"][0]["mini"].as_f64().unwrap() + 0.1).abs() < 1e-6);
        assert_eq!(
            native.input["players"][0]["preferred_visual_delay_seconds"],
            0
        );
        for key in [
            "fast_note_rendering_requested",
            "fast_note_rendering",
            "fast_note_rendering_at_end",
        ] {
            assert_eq!(native.input[key], true, "native {key}");
        }
        for key in [
            "native_option_clock_deltas",
            "native_song_position_clocks",
            "native_notefield_geometry",
            "native_fast_note_rendering",
            "native_pre_theme_preferences",
        ] {
            assert_eq!(native.capabilities[key], true, "native {key}");
        }
        assert_eq!(native.capabilities["native_model_render_state_protocol"], 1);
        assert_eq!(
            native.capabilities["native_model_render_state_backend"],
            "opengl"
        );
        assert_eq!(native.display["logical_width"], 854);
        assert_eq!(native.display["logical_height"], 480);
        let prefs = native.input["preferences"].clone();
        assert_eq!(prefs["music_rate"], 1);
        assert_eq!(prefs["visual_delay_seconds"], 0);
        assert_eq!(prefs["default_sync_offset"], "SyncOffset_ITG");
        assert!((prefs["global_offset_seconds"].as_f64().unwrap() + 0.008).abs() < 1e-6);
        assert_eq!(
            [
                native.dropped_model_geometry_samples,
                native.dropped_drawable_samples,
                native.dropped_receptor_visits,
                native.dropped_script_errors
            ],
            [0; 4]
        );
        assert!(native.script_errors.is_empty());
        let source =
            include_bytes!("../../../tests/fixtures/itgmania-song-lua-micro/looping-rooms.ssc");
        let hash = Sha256::digest(source)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(native.provenance["simfile_sha256"], hash);
        let updates = native.update_frames;
        assert_eq!(updates.len(), 4392, "retain the complete approach history");
        for (index, update) in updates.iter().enumerate() {
            assert_eq!(update.sequence, index as u64 + 1);
            assert!(
                [update.seconds, update.visible_seconds, update.visible_beat]
                    .into_iter()
                    .all(f32::is_finite)
            );
            assert!(update.wall_seconds.is_finite() && update.wall_seconds >= 0.0);
            assert!(
                [
                    update.delta,
                    update.option_deltas.song,
                    update.option_deltas.players[0],
                    update.option_deltas.players[1]
                ]
                .into_iter()
                .all(|delta| delta.is_finite() && delta >= 0.0)
            );
        }
        let mut frames = BTreeMap::<u64, NativeFrame>::new();
        for visit in native.receptor_visits {
            assert_eq!(visit.state.render_target, 0);
            assert_eq!(visit.state.note_context.player, 0);
            let old = frames.entry(visit.update_sequence).or_default().visits
                [visit.state.note_context.column]
                .replace(visit.state.base_alpha);
            assert!(
                old.is_none(),
                "one native receptor visit per lane and frame"
            );
        }
        assert_eq!(frames.len(), 235);
        assert!(
            frames
                .values()
                .all(|frame| frame.visits.iter().all(Option::is_some))
        );
        for sample in native.drawable_samples {
            let Some(context) = sample.state.note_context else {
                continue;
            };
            if context.kind != "receptor"
                || sample.state.render_target != 0
                || sample.state.blend != 0
            {
                continue;
            }
            assert_eq!(context.player, 0);
            assert_eq!(sample.vertices.len(), 4);
            let clip = std::array::from_fn(|vertex| {
                std::array::from_fn(|axis| sample.vertices[vertex][3 + axis])
            });
            let alpha = sample.vertices[0][12];
            assert!(sample.vertices.iter().all(|vertex| vertex[12] == alpha));
            frames
                .get_mut(&sample.update_sequence)
                .unwrap()
                .draws
                .push(Draw {
                    kind: 0,
                    blend: deadlib_render_core::BlendMode::Alpha,
                    texture_mask: false,
                    alpha,
                    clip,
                });
        }
        let geometries: HashMap<_, _> = native
            .model_geometry_meshes
            .into_iter()
            .map(|mesh| (mesh.id, mesh.vertices))
            .collect();
        let mut native_vertices: [Option<Vec<[f32; 3]>>; 2] = [None, None];
        let mut fake_submissions = 0;
        for sample in native.model_geometry_samples {
            let state = sample.state;
            let Some(context) = state.note_context else {
                continue;
            };
            // Native NotePart_Tap, NotePart_Fake and NotePart_HoldHead all use
            // Cel's tap Model, and share its render texture handles.
            if context.kind != "actor"
                || !matches!(context.part, 0 | 3 | 4)
                || state.render_target != 0
            {
                continue;
            }
            fake_submissions += usize::from(context.part == 3);
            assert_eq!(context.player, 0);
            assert_eq!(state.mesh_name, ["cells", "scroller"][state.mesh_index]);
            let vertices = &geometries[&sample.geometry];
            if let Some(expected) = &native_vertices[state.mesh_index] {
                assert_eq!(vertices, expected);
            } else {
                native_vertices[state.mesh_index] = Some(vertices.clone());
            }
            let render = state.render_state;
            assert_eq!(render["backend"], "opengl");
            assert_eq!(render["lighting"], false);
            let alpha = (render["current_color"][3].as_f64().unwrap() as f32).clamp(0.0, 1.0);
            // Native unlit SetMaterial uses diffuse alpha, while glow may exceed
            // one until the fixed-function pipeline clamps the current color.
            assert!(
                (alpha
                    - (render["material"]["diffuse"][3].as_f64().unwrap() as f32).clamp(0.0, 1.0))
                .abs()
                    < 0.00001
            );
            let texture = &render["textures"][0];
            assert_eq!(texture["unit"], 0);
            let blend = match render["blend"]["dst_rgb"].as_u64().unwrap() {
                771 => deadlib_render_core::BlendMode::Alpha,
                1 => deadlib_render_core::BlendMode::Add,
                other => panic!("native Model blend {other}"),
            };
            let clip = std::array::from_fn(|anchor| {
                std::array::from_fn(|axis| {
                    state.anchors_clip[0][axis]
                        + 32.0 * (state.anchors_clip[anchor][axis] - state.anchors_clip[0][axis])
                })
            });
            frames
                .get_mut(&sample.update_sequence)
                .unwrap()
                .draws
                .push(Draw {
                    kind: state.mesh_index + 1,
                    blend,
                    texture_mask: texture["mode"] == 1,
                    alpha,
                    clip,
                });
        }
        assert_eq!(fake_submissions, 76, "retain native fake-arrow submissions");
        let native_vertices = native_vertices.map(|mesh| mesh.expect("both native Cel meshes"));
        std::thread::Builder::new().stack_size(64 << 20).spawn(move || {
            with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1, true, false, || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(854, 480);
                space::set_overscan(0, 0, 0, 0);
                let simfile = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/itgmania-song-lua-micro/looping-rooms.ssc");
                let profiles = std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"),
                    scroll_speed: ScrollSpeedSetting::MMod(690.0), mini_percent: -10,
                    attack_mode: profile_data::AttackMode::On, ..Default::default()
                });
                let config = GameplayConfig {
                    global_offset_seconds: prefs["global_offset_seconds"].as_f64().unwrap() as f32,
                    visual_delay_seconds: prefs["visual_delay_seconds"].as_f64().unwrap() as f32,
                    machine_pack_ini_offsets: true, machine_default_sync_pref: deadsync_chart::SyncPref::Itg,
                    ..Default::default()
                };
                let mut state = build_test_state_with_lua_mode_and_clocks(&simfile,
                    GameplayViewport::design(), GameplaySession::default(), profiles,
                    "Vulkan (wgpu)", None, false, None, Some(config));
                assert!(!state.song().has_lua && state.song_lua.is_none(), "exercise the non-Lua compositor");
                assert!((state.gameplay.timing().get_time_for_beat_exact(0.0) - 0.017).abs() < 0.000001);
                state.set_live_autoplay_enabled(true);
                let assets = fixture_assets();
                let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
                let tap_handles: HashSet<_> = ns.notes.iter()
                    .chain(ns.note_layers.iter().flat_map(|layers| layers.iter()))
                    .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                let receptor_handles: HashSet<_> = state.noteskin_assets.receptor_noteskin[0].as_ref().unwrap_or(ns)
                    .receptor_off.iter().map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                #[cfg(target_os = "windows")]
                let mut capture = std::env::var_os("DEADSYNC_LOOPING_CAPTURE_DIR")
                    .map(|output| SongLuaNotePixelCapture::new(&state, &metrics, PathBuf::from(output)));
                #[cfg(target_os = "windows")]
                let mut captured = [false; 4];
                let mut counts = [0usize; 4];
                let mut controls = [false; 4];
                let mut hidden = 0;
                let mut fading = 0;
                let mut tilted = 0;
                let mut tilted_note_models = 0;
                let mut max_error = 0.0f32;
                let mut failures = Vec::new();
                state.set_current_music_time_ns(deadsync_core::song_time::song_time_ns_from_seconds(updates[0].seconds));
                for update in updates {
                    let music = deadsync_core::song_time::song_time_ns_from_seconds(update.seconds);
                    let host = (update.wall_seconds * 1e9) as u64;
                    state.gameplay.set_player_option_clock_deltas(update.option_deltas.players);
                    screen_gameplay::update(&mut state, update.delta, deadsync_gameplay::GameplayAudioSnapshot {
                        stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                            music_nanos: music, has_music_mapping: true, valid_at_host_nanos: host, ..Default::default()
                        }, ..Default::default()
                    }, || host, &mut Vec::new());
                    state.set_current_music_time_ns(music);
                    let Some(expected) = frames.get(&update.sequence) else { continue; };
                    assert_eq!(update.seconds, update.visible_seconds);
                    let section = section(update.seconds).expect("retained native window");
                    counts[section] += 1;
                    let frame = compose_fixture_frame_with_view_and_clock(&mut state, &assets, &metrics,
                        &mut Vec::new(), &mut compose::TextLayoutCache::default(), &mut compose::ComposeScratch::default(),
                        screen_gameplay::ActorViewOverride::default(), &FIXTURE_TEXTURES, Some(update.wall_seconds));
                    assert!(frame.render_targets.is_empty());
                    let actual = actual_draws(&frame, &metrics, &receptor_handles, &tap_handles, &native_vertices);
                    let mut errors = Vec::new();
                    // Report both channels so a receptor mismatch cannot hide
                    // an independent note Model regression in the same frame.
                    for (label, receptors) in [("receptors", true), ("models", false)] {
                        let expected: Vec<_> = expected.draws.iter()
                            .filter(|draw| (draw.kind == 0) == receptors).cloned().collect();
                        let actual = actual.iter()
                            .filter(|draw| (draw.kind == 0) == receptors).cloned().collect();
                        match compare_draws(&expected, actual) {
                            Ok(error) => max_error = max_error.max(error),
                            Err(error) => errors.push(format!("{label}: {error}")),
                        }
                    }
                    let frame_matches = errors.is_empty();
                    if !frame_matches { failures.push((update.seconds, update.visible_beat, errors.join("; "))); }
                    if expected.visits.iter().all(|alpha| *alpha == Some(0.0)) {
                        assert!(!expected.draws.iter().any(|draw| draw.kind == 0));
                        hidden += 1;
                    }
                    if expected.visits[0].is_some_and(|alpha| alpha > 0.05 && alpha < 0.95) { fading += 1; }
                    if section == 3 && expected.draws.iter().any(|draw| draw.kind == 0
                        && draw.clip.iter().map(|p| p[3]).fold(f32::NEG_INFINITY, f32::max)
                            - draw.clip.iter().map(|p| p[3]).fold(f32::INFINITY, f32::min) > 10.0) { tilted += 1; }
                    // Native autoplay's tilted note Models in this window are
                    // the down-column hold head at beat 351. Standalone taps
                    // finish before their ending attacks activate; receptor
                    // geometry also covers the later columns' perspective.
                    if section == 3 && expected.draws.iter().any(|draw| draw.kind != 0
                        && (draw.clip[2][3] - draw.clip[0][3]).abs() > 10.0) { tilted_note_models += 1; }
                    if !controls[section] && update.seconds >= [36.2, 70.3, 83.6, 130.1][section] {
                        let control = compose_fixture_frame_with_view_and_clock(&mut state, &assets, &metrics,
                            &mut Vec::new(), &mut compose::TextLayoutCache::default(), &mut compose::ComposeScratch::default(),
                            FixtureFrameView {
                                presentation: screen_gameplay::ActorViewOverride::default(),
                                preview: Some(SongFrameContent::preview(false, true)),
                            },
                            &FIXTURE_TEXTURES, Some(update.wall_seconds));
                        assert!(compare_draws(&expected.draws,
                            actual_draws(&control, &metrics, &receptor_handles, &tap_handles, &native_vertices)).is_err(),
                            "each reported section must reject a render with attacks disabled");
                        controls[section] = true;
                    }
                    #[cfg(target_os = "windows")]
                    if let Some(capture) = capture.as_mut()
                        && !captured[section] && frame_matches
                        && update.seconds >= [36.2, 70.3, 83.6, 130.1][section]
                    {
                        capture.capture_named_at_game_time(&mut state, &metrics, update.visible_beat,
                            "looping-rooms-native-clock", Some(update.wall_seconds));
                        captured[section] = true;
                    }
                }
                assert!(counts.iter().all(|count| *count > 40), "four transition windows: {counts:?}");
                assert_eq!(controls, [true; 4]);
                assert!(hidden > 10 && fading > 20 && tilted > 20 && tilted_note_models >= 10,
                    "native positive/negative coverage: hidden={hidden}, fading={fading}, tilted={tilted}, tilted note Models={tilted_note_models}");
                assert!(failures.is_empty(), "Looping the Rooms mismatched {} frames; first per section: {:?}", failures.len(),
                    [35.4..37.4, 69.4..71.4, 82.8..84.5, 129.8..131.5].into_iter()
                        .filter_map(|window| failures.iter().find(|(time, _, _)| window.contains(time))).collect::<Vec<_>>());
                #[cfg(target_os = "windows")]
                if capture.is_some() { assert_eq!(captured, [true; 4]); }
                eprintln!("Looping the Rooms matched {} native frames, {hidden} hidden/{fading} fading/{tilted} tilted receptor frames and {tilted_note_models} tilted note Model frames; max clip residual {max_error}", counts.iter().sum::<usize>());
            });
        }).unwrap().join().unwrap();
    }
}
