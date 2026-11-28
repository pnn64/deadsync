mod paperclip {
    use super::*;
    use std::collections::HashSet;

    #[derive(Debug)]
    struct NoteDraw {
        receptor: bool,
        texture: deadlib_render_core::TextureHandle,
        blend: deadlib_render_core::BlendMode,
        alpha: f32,
        vertices: Vec<[f32; 4]>,
    }

    #[derive(Debug, Default)]
    struct NoteDraws {
        receptors: usize,
        taps: usize,
        draws: Vec<NoteDraw>,
    }

    fn note_draws(
        frame: &deadlib_render_core::RenderFrame,
        metrics: &space::Metrics,
        receptors: &HashSet<deadlib_render_core::TextureHandle>,
        taps: &HashSet<deadlib_render_core::TextureHandle>,
    ) -> NoteDraws {
        let mut result = NoteDraws::default();
        for op in &frame.ops {
            let (texture, blend, camera) = match op {
                DrawOp::Sprite(run) => (run.texture_handle, run.blend, run.camera),
                DrawOp::TexturedMesh(run) => (run.texture_handle, run.blend, run.camera),
                _ => continue,
            };
            let receptor = receptors.contains(&texture);
            if !receptor && !taps.contains(&texture) {
                continue;
            }
            let camera = frame
                .cameras
                .get(camera as usize)
                .copied()
                .unwrap_or_else(|| metrics.projection());
            let mut push = |alpha: f32, mut vertices: Vec<[f32; 4]>| {
                if alpha <= 0.0 {
                    return;
                }
                assert!(alpha.is_finite());
                assert!(vertices.iter().flatten().all(|value| value.is_finite()));
                vertices.sort_by(|a, b| {
                    a.iter()
                        .zip(b)
                        .map(|(a, b)| a.total_cmp(b))
                        .find(|order| !order.is_eq())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                vertices.dedup();
                result.receptors += usize::from(receptor);
                result.taps += usize::from(!receptor);
                result.draws.push(NoteDraw {
                    receptor,
                    texture,
                    blend,
                    alpha,
                    vertices,
                });
            };
            match op {
                DrawOp::Sprite(run) => {
                    for sprite in &frame.sprite_instances[run.instance_start as usize
                        ..(run.instance_start + run.instance_count) as usize]
                    {
                        let [sin, cos] = sprite.rot_sin_cos;
                        let [offset_sin, offset_cos] = sprite.local_offset_rot_sin_cos;
                        let [offset_x, offset_y] = sprite.local_offset;
                        let x = sprite.center[0] + offset_cos * offset_x - offset_sin * offset_y;
                        let y = sprite.center[1] + offset_sin * offset_x + offset_cos * offset_y;
                        let vertices = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]]
                            .map(|[dx, dy]| {
                                let px = dx * sprite.size[0];
                                let py = dy * sprite.size[1];
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
                            })
                            .to_vec();
                        push(sprite.tint[3], vertices);
                    }
                }
                DrawOp::TexturedMesh(run) => {
                    let geometry = &frame.tmesh_geometries[run.geometry as usize];
                    for instance in &frame.tmesh_instances[run.instance_start as usize
                        ..(run.instance_start + run.instance_count) as usize]
                    {
                        let matrix = camera * instance.transform();
                        let vertices = geometry
                            .vertices
                            .iter()
                            .map(|vertex| {
                                matrix
                                    .mul_vec4(
                                        [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0].into(),
                                    )
                                    .to_array()
                            })
                            .collect();
                        push(instance.tint[3], vertices);
                    }
                }
                _ => unreachable!(),
            }
        }
        result
    }

    fn compare_note_draws(reference: &NoteDraws, actual: &NoteDraws) -> Result<(), String> {
        if reference.draws.len() != actual.draws.len() {
            return Err(format!(
                "draw count: reference {}, actual {}",
                reference.draws.len(),
                actual.draws.len()
            ));
        }
        for (index, (reference, actual)) in reference.draws.iter().zip(&actual.draws).enumerate() {
            if reference.receptor != actual.receptor
                || reference.texture != actual.texture
                || reference.blend != actual.blend
                || (reference.alpha - actual.alpha).abs() > 0.0001
            {
                return Err(format!(
                    "draw {index} has different kind, texture, blend or alpha"
                ));
            }
            // Compare submitted geometry after the final camera/Player transforms.
            // Sprite and transformed-quad winding can differ between composition
            // paths, so match unique vertices without depending on vertex order.
            if reference.vertices.len() != actual.vertices.len() {
                return Err(format!("draw {index} has different vertex counts"));
            }
            let mut remaining = actual.vertices.clone();
            for reference in &reference.vertices {
                let Some(matched) = remaining.iter().position(|actual| {
                    reference
                        .iter()
                        .zip(actual)
                        .all(|(a, b)| (a - b).abs() < 0.001)
                }) else {
                    return Err(format!("draw {index} differs at clip vertex {reference:?}"));
                };
                remaining.swap_remove(matched);
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires a window system; run alone with the software renderer"]
    fn live_song_images_upload_after_delayed_load_and_replacement() {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        std::thread::Builder::new().stack_size(16 * 1024 * 1024).spawn(|| {
            with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
                true, false, || {
                let metrics = space::Metrics::centered(640.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(640, 480);
                let dir = tempfile::tempdir().unwrap();
                let simfile = dir.path().join("song.ssc");
                fs::write(&simfile, "#TITLE:Delayed Sprite images;\n#BPMS:0=120;\n#FGCHANGES:0=foreground.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n0000\n0000\n0000\n1000\n;\n").unwrap();
                // Foreground.cpp fixes the stop beat from GetTweenTimeLeft just
                // after On. Keep this layer alive while child callbacks queue
                // their later delays; those cannot extend the native lifetime.
                fs::write(dir.path().join("foreground.lua"), r#"
return Def.ActorFrame {
    OnCommand=function(self) self:sleep(2) end,
    Def.Sprite {
    Name='LateImage',
    InitCommand=function(self) self:xy(SCREEN_CENTER_X,SCREEN_CENTER_Y) end,
    OnCommand=function(self) self:sleep(.5):queuecommand('First') end,
    FirstCommand=function(self)
        self:Load(GAMESTATE:GetCurrentSong():GetSongDir()..'first.png')
        self:sleep(.5):queuecommand('Second')
    end,
    SecondCommand=function(self)
        self:Load(GAMESTATE:GetCurrentSong():GetSongDir()..'second.png')
    end,
} }
"#).unwrap();
                image::RgbaImage::from_pixel(8, 8, image::Rgba([240, 20, 30, 255]))
                    .save(dir.path().join("first.png")).unwrap();
                image::RgbaImage::from_pixel(12, 6, image::Rgba([20, 230, 40, 255]))
                    .save(dir.path().join("second.png")).unwrap();
                let mut state = build_test_state_with_lua_mode(&simfile,
                    GameplayViewport::new(640.0, 480.0), GameplaySession::default(),
                    std::array::from_fn(|_| profile_data::Profile::default()),
                    "Software", Some(4.0), true);
                assert!(state.song_lua.is_some());
                let event_loop = winit::event_loop::EventLoop::builder().with_any_thread(true)
                    .build().unwrap();
                #[expect(deprecated, reason = "hidden software upload fixture needs no event loop")]
                let window = event_loop.create_window(winit::window::Window::default_attributes()
                    .with_visible(false).with_inner_size(winit::dpi::PhysicalSize::new(640, 480))).unwrap();
                let mut backend = deadlib_render::create_backend(
                    deadlib_render_core::BackendType::Software, Arc::new(window), metrics.projection(),
                    false, deadlib_render_core::PresentModePolicy::Immediate, false, false).unwrap();
                let mut assets = fixture_assets();
                let mut images = crate::gameplay_prewarm::GameplayLiveTextures::default();
                let mut actors = Vec::new();
                let mut text = compose::TextLayoutCache::default();
                let mut scratch = compose::ComposeScratch::default();
                let mut uploads = Vec::new();
                let mut old_handle = None;
                for tick in 0..=90 {
                    set_fixture_time(&mut state, tick as f32 / 60.0);
                    let visuals = state.song_lua_visuals();
                    let overlays = || visuals.overlays.iter().chain(visuals.background_visual_layers.iter()
                        .chain(&visuals.foreground_visual_layers).flat_map(|layer| &layer.overlays));
                    let resource = overlays().find(|actor| actor.name.as_deref() == Some("LateImage"))
                        .and_then(|actor| match &actor.kind {
                            deadsync_song_lua::SongLuaOverlayKind::Sprite { texture_path, texture_key, .. } =>
                                Some((texture_path.clone(), Arc::clone(texture_key))),
                            _ => None,
                        });
                    let newly_bound = resource.as_ref().is_some_and(|(_, key)| !assets.has_uploaded_texture_key(key));
                    images.sync(&mut assets, &mut backend, overlays());
                    if let Some((path, key)) = &resource {
                        let bound = assets.texture_context().bind_texture(key).expect("image binding before composition");
                        let deadlib_render::Texture::Software(texture) = assets.textures().get(&bound.handle)
                            .expect("image uploaded to actual renderer") else { panic!("software texture"); };
                        let (size, color) = if path.file_name().unwrap() == "first.png" {
                            ((8, 8), [240, 20, 30, 255])
                        } else { ((12, 6), [20, 230, 40, 255]) };
                        assert_eq!(texture.image.dimensions(), size);
                        assert!(texture.image.pixels().all(|pixel| pixel.0 == color));
                        if newly_bound { uploads.push(path.file_name().unwrap().to_owned()); }
                    }
                    if ![15, 45, 75, 90].contains(&tick) { continue; }
                    let frame = compose_fixture_frame_with_textures(&mut state, &assets, &metrics,
                        &mut actors, &mut text, &mut scratch, assets.texture_context());
                    if let Some((path, key)) = resource {
                        let handle = assets.texture_context().texture_handle(&key);
                        let draws = frame.ops.iter().filter_map(|op| match op {
                            DrawOp::Sprite(run) if run.texture_handle == handle => Some(run),
                            _ => None,
                        }).collect::<Vec<_>>();
                        assert_eq!(draws.iter().map(|run| run.instance_count).sum::<u32>(), 1,
                            "published image reaches the real frame at tick {tick}, handle {handle:?}; ops {:?}",
                            frame.ops.iter().map(|op| match op {
                                DrawOp::Sprite(run) => format!("sprite {:?} x{}", run.texture_handle, run.instance_count),
                                DrawOp::TexturedMesh(run) => format!("textured mesh {:?} x{}", run.texture_handle, run.instance_count),
                                DrawOp::Mesh(_) => "mesh".into(),
                            }).collect::<Vec<_>>());
                        let sprite = &frame.sprite_instances[draws[0].instance_start as usize];
                        assert_eq!(sprite.size, if path.file_name().unwrap() == "first.png" { [8.0, 8.0] } else { [12.0, 6.0] });
                        if tick == 45 { old_handle = Some(handle); }
                        if tick >= 75 {
                            assert!(frame.ops.iter().all(|op| !matches!(op,
                                DrawOp::Sprite(run) if Some(run.texture_handle) == old_handle)),
                                "replacement retires the previous Sprite binding from the frame");
                        }
                        backend.draw(&frame, assets.textures(), false).expect("draw uploaded live image");
                    } else {
                        assert_eq!(tick, 15, "image absent only before the delayed Load");
                    }
                }
                assert_eq!(uploads, [std::ffi::OsString::from("first.png"), std::ffi::OsString::from("second.png")]);
            });
        }).unwrap().join().unwrap();
    }

    fn published_overlay<'a>(
        state: &'a screen_gameplay::State,
        name: &str,
    ) -> &'a deadsync_assets::song_lua::SongLuaOverlayActor {
        let visuals = state.song_lua_visuals();
        visuals
            .overlays
            .iter()
            .chain(
                visuals
                    .background_visual_layers
                    .iter()
                    .chain(&visuals.foreground_visual_layers)
                    .flat_map(|layer| &layer.overlays),
            )
            .find(|actor| actor.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("Paperclip actor {name:?} is present"))
    }

    fn image_draw_count(
        frame: &deadlib_render_core::RenderFrame,
        handle: deadlib_render_core::TextureHandle,
    ) -> usize {
        frame
            .ops
            .iter()
            .map(|op| match op {
                DrawOp::Sprite(run) if run.texture_handle == handle => frame.sprite_instances[run
                    .instance_start
                    as usize
                    ..(run.instance_start + run.instance_count) as usize]
                    .iter()
                    .filter(|instance| {
                        instance.tint[3] > 0.0 && instance.size.iter().all(|size| size.abs() > 0.0)
                    })
                    .count(),
                DrawOp::TexturedMesh(run) if run.texture_handle == handle => frame.tmesh_instances
                    [run.instance_start as usize
                        ..(run.instance_start + run.instance_count) as usize]
                    .iter()
                    .filter(|instance| instance.tint[3] > 0.0)
                    .count(),
                _ => 0,
            })
            .sum()
    }

    fn life_meter_glyph_colors(
        frame: &deadlib_render_core::RenderFrame,
        metrics: &space::Metrics,
        font_atlas: deadlib_render_core::TextureHandle,
        versus: bool,
        player: usize,
    ) -> Vec<[[f32; 4]; 6]> {
        // Paperclip places the meters on the header at y=40. Use the actual
        // Wendy atlas and projected positions to isolate them in the complete
        // submitted frame, including retained actors and text-layout caches.
        let start_x = if versus {
            if player == 0 { 15.0 } else { 442.0 }
        } else {
            117.0
        };
        let mut glyphs = Vec::new();
        for op in &frame.ops {
            let DrawOp::TexturedMesh(run) = op else {
                continue;
            };
            if run.texture_handle != font_atlas {
                continue;
            }
            let camera = frame
                .cameras
                .get(run.camera as usize)
                .copied()
                .unwrap_or_else(|| metrics.projection());
            let geometry = &frame.tmesh_geometries[run.geometry as usize];
            for instance in &frame.tmesh_instances
                [run.instance_start as usize..(run.instance_start + run.instance_count) as usize]
            {
                let transform = camera * instance.transform();
                for quad in geometry.vertices.chunks_exact(6) {
                    let mut center = [0.0; 2];
                    for vertex in quad {
                        let clip = transform
                            .mul_vec4([vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0].into());
                        center[0] +=
                            (clip.x / clip.w + 1.0) * 0.5 * (metrics.right - metrics.left) / 6.0;
                        center[1] +=
                            (1.0 - clip.y / clip.w) * 0.5 * (metrics.top - metrics.bottom) / 6.0;
                    }
                    if !(start_x - 2.0..start_x + 130.0).contains(&center[0])
                        || !(25.0..55.0).contains(&center[1])
                    {
                        continue;
                    }
                    let colors = std::array::from_fn(|vertex| {
                        std::array::from_fn(|channel| {
                            quad[vertex].color[channel] * instance.tint[channel]
                        })
                    });
                    glyphs.push((center[0], colors));
                }
            }
        }
        glyphs.sort_by(|left, right| left.0.total_cmp(&right.0));
        glyphs.into_iter().map(|(_, colors)| colors).collect()
    }

    #[test]
    fn paperclip_score_life_and_failure_follow_gameplay_in_requested_modes() {
        std::thread::Builder::new().name("paperclip-gameplay-stats".into())
            .stack_size(16 * 1024 * 1024).spawn(|| {
                let fixture = song_fixture::from_source("Paperclip/Paperclip.ssc");
                let native: serde_json::Value = serde_json::from_str(include_str!(
                    "../../../tests/fixtures/interactive_song_lua/paperclip-live-stats-2026-11-18/rainbow-observations.json"
                )).expect("native Paperclip glyph-color observations");
                let sample = |sequence| native["samples"].as_array().unwrap().iter()
                    .find(|sample| sample["update_sequence"].as_u64() == Some(sequence))
                    .expect("retained native color sample");
                let full_life_samples = [1122, 1128].map(|sequence| {
                    let sample = sample(sequence);
                    assert_eq!(sample["glyph_count"].as_u64(), Some(18));
                    assert_eq!(sample["all_quad_corner_colors_equal"].as_bool(), Some(true));
                    let colors: Vec<[f32; 4]> = serde_json::from_value(sample["glyph_quad_rgba"].clone()).unwrap();
                    assert_eq!(colors.len(), 18);
                    (sample["update_wall_seconds"].as_f64().unwrap(), colors)
                });
                assert_ne!(full_life_samples[0].1, full_life_samples[1].1,
                    "native samples span a rainbow color change");
                let normal_meter_color: [f32; 4] =
                    serde_json::from_value(sample(1134)["glyph_quad_rgba"][0].clone()).unwrap();
                let mut failures = Vec::new();
                for runtime in [false, true] {
                    for (style, side, joined) in [
                        (profile_data::PlayStyle::Single, profile_data::PlayerSide::P1, [true, false]),
                        (profile_data::PlayStyle::Single, profile_data::PlayerSide::P2, [false, true]),
                        (profile_data::PlayStyle::Versus, profile_data::PlayerSide::P1, [true, true]),
                    ] {
                        with_session(style, side, joined[0], joined[1], || {
                            let versus = style == profile_data::PlayStyle::Versus;
                            let label = format!("requested_runtime={runtime}, style={style:?}, side={side:?}");
                            let metrics = space::Metrics::centered(854.0, 480.0);
                            space::set_current_metrics(metrics);
                            space::set_current_window_px(854, 480);
                            space::set_overscan(0, 0, 0, 0);
                            let session = GameplaySession {
                                play_style: if versus { deadsync_gameplay::GameplayInputPlayStyle::Versus }
                                    else { deadsync_gameplay::GameplayInputPlayStyle::Single },
                                player_side: if side == profile_data::PlayerSide::P2 {
                                    deadsync_gameplay::GameplayInputPlayerSide::P2
                                } else { deadsync_gameplay::GameplayInputPlayerSide::P1 },
                                joined_sides: joined,
                                ..Default::default()
                            };
                            let profiles = std::array::from_fn(|_| profile_data::Profile {
                                noteskin: profile_data::NoteSkin::new("cel"),
                                attack_mode: profile_data::AttackMode::On,
                                ..Default::default()
                            });
                            let mut state = build_test_state_with_lua_preparation(&fixture.simfile,
                                GameplayViewport::new(854.0, 480.0), session, profiles,
                                "Software", Some(60.0), runtime, None,
                                Some(GameplayConfig { global_offset_seconds: -0.008, ..Default::default() }), true);
                            assert!(state.mods.song_lua_compile_succeeded, "{label}: preparation");
                            if state.song_lua.is_none() {
                                failures.push(format!("{label}: mutable stage-stat reads require retained runtime"));
                            }
                            let mut assets = fixture_assets();
                            let visuals = state.song_lua_visuals();
                            let (bsod_path, bsod_key) = visuals.overlays.iter().chain(
                                visuals.background_visual_layers.iter().chain(&visuals.foreground_visual_layers)
                                    .flat_map(|layer| &layer.overlays),
                            ).find_map(|actor| match &actor.kind {
                                deadsync_assets::song_lua::SongLuaOverlayKind::Sprite { texture_path, texture_key, .. }
                                    if texture_path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.eq_ignore_ascii_case("BSOD.png")) =>
                                    Some((texture_path.clone(), Arc::clone(texture_key))),
                                _ => None,
                            }).expect("Paperclip includes the failure background");
                            assets.queue_texture_upload(bsod_key.to_string(), deadlib_image_io::open(&bsod_path).unwrap());
                            let bsod_handle = assets.texture_context().texture_handle(&bsod_key);
                            let meter_atlas_key = Arc::clone(&assets.fonts()["wendy"].glyph_map[&'1'].texture_key);
                            let meter_atlas_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("../../assets").join(meter_atlas_key.as_ref());
                            assets.queue_texture_upload(meter_atlas_key.to_string(),
                                deadlib_image_io::open(&meter_atlas_path).unwrap());
                            let meter_atlas = assets.texture_context().texture_handle(&meter_atlas_key);
                            assert_ne!(meter_atlas, 0, "the real life-meter font atlas is bound");
                            let mut actors = Vec::new();
                            let mut text = compose::TextLayoutCache::default();
                            let mut scratch = compose::ComposeScratch::default();
                            let advance = |state: &mut screen_gameplay::State, ticks: std::ops::RangeInclusive<u32>| {
                                for tick in ticks { set_fixture_time(state, tick as f32 / 60.0); }
                            };
                            let check_text = |state: &screen_gameplay::State, lives: [f32; 2], full_score: [bool; 2], failures: &mut Vec<String>| {
                                for player in 0..if versus { 2 } else { 1 } {
                                    let meter = published_overlay(state, &format!("{}P meter", player + 1));
                                    let score = published_overlay(state, &format!("{}P score", player + 1));
                                    let deadsync_assets::song_lua::SongLuaOverlayKind::BitmapText { text: meter_text, .. } = &meter.kind
                                        else { panic!("life meter is BitmapText"); };
                                    let deadsync_assets::song_lua::SongLuaOverlayKind::BitmapText { text: score_text, .. } = &score.kind
                                        else { panic!("score is BitmapText"); };
                                    let expected_meter = "1".repeat((18.0 * lives[player]) as usize);
                                    let expected_score = if full_score[player] { "100.00" } else { "00.00" };
                                    if meter_text.as_ref() != expected_meter || meter.initial_state.rainbow_scroll != (lives[player] == 1.0)
                                        || score_text.as_ref() != expected_score {
                                        failures.push(format!("{label}, t={}, player{player}: meter={meter_text:?}, rainbow={}, score={score_text:?}; expected {expected_meter:?}, {}, {expected_score:?}",
                                            state.current_music_time_display(), meter.initial_state.rainbow_scroll, lives[player] == 1.0));
                                    }
                                }
                            };
                            let set_stats = |state: &mut screen_gameplay::State, player: usize, life: f32, full_score: bool| {
                                let possible = state.stage_itg_score_inputs(player).unwrap().possible_grade_points;
                                // Use the gameplay setter to keep life history in
                                // sync while bypassing judgment recovery gating.
                                state.set_player_life_from_script(player, life);
                                let player = &mut state.players_runtime.players[player];
                                player.earned_grade_points = if full_score { possible } else { 0 };
                                assert_eq!(player.life, life);
                                assert_eq!(player.life_history.last().unwrap().1, life);
                            };
                            let check_meter_colors = |frame: &deadlib_render_core::RenderFrame, lives: [f32; 2], uptime: f64, native_full_colors: &[[f32; 4]], failures: &mut Vec<String>| {
                                // ITGmania BitmapText::DrawPrimitives replaces glyph
                                // diffuse every 0.2s of process uptime. Compare all
                                // submitted corners against captured native RGBA,
                                // allowing the native byte-color quantization.
                                for player in 0..if versus { 2 } else { 1 } {
                                    let glyphs = life_meter_glyph_colors(frame, &metrics, meter_atlas, versus, player);
                                    let count = (18.0 * lives[player]) as usize;
                                    if glyphs.len() != count {
                                        failures.push(format!("{label}, uptime={uptime}, player{player}: expected {count} submitted life-meter glyphs, got {}", glyphs.len()));
                                    }
                                    for (index, colors) in glyphs.iter().enumerate() {
                                        let expected = if lives[player] == 1.0 {
                                            native_full_colors.get(index).copied().unwrap_or(normal_meter_color)
                                        } else { normal_meter_color };
                                        if colors.iter().any(|color| color.iter().zip(expected)
                                            .any(|(actual, expected)| (actual - expected).abs() > 1.0 / 255.0 + 1e-5)) {
                                            failures.push(format!("{label}, uptime={uptime}, player{player}, glyph{index}: submitted colors={colors:?}, expected {expected:?} at life={}", lives[player]));
                                            break;
                                        }
                                    }
                                }
                            };
                            advance(&mut state, 0..=900);
                            check_text(&state, [0.5, 0.5], [false, false], &mut failures);
                            let first_uptime = full_life_samples[0].0;
                            let second_uptime = full_life_samples[1].0;
                            let frame = compose_fixture_frame_with_view_and_clock(&mut state, &assets, &metrics,
                                &mut actors, &mut text, &mut scratch, screen_gameplay::ActorViewOverride::default(),
                                assets.texture_context(), Some(first_uptime - 0.25));
                            check_meter_colors(&frame, [0.5, 0.5], first_uptime - 0.25, &full_life_samples[0].1, &mut failures);
                            for (ticks, lives, scores) in [
                                (901..=915, [1.0, 1.0], [true, false]),
                                (916..=930, [0.25, 0.75], [false, true]),
                            ] {
                                for player in 0..if versus { 2 } else { 1 } {
                                    set_stats(&mut state, player, lives[player], scores[player]);
                                }
                                advance(&mut state, ticks);
                                check_text(&state, lives, scores, &mut failures);
                                let uptime = if lives[0] == 1.0 { first_uptime } else { second_uptime + 0.25 };
                                let frame = compose_fixture_frame_with_view_and_clock(&mut state, &assets, &metrics,
                                    &mut actors, &mut text, &mut scratch, screen_gameplay::ActorViewOverride::default(),
                                    assets.texture_context(), Some(uptime));
                                check_meter_colors(&frame, lives, uptime, &full_life_samples[0].1, &mut failures);
                                if image_draw_count(&frame, bsod_handle) != 0 {
                                    failures.push(format!("{label}: BSOD is drawn while all joined players have life"));
                                }
                                if lives[0] == 1.0 {
                                    // Keep music time and the published Lua state
                                    // unchanged. The warmed frame must still cycle
                                    // every glyph at the next native uptime sample.
                                    let paused_music_time = state.current_music_time_display();
                                    let frame = compose_fixture_frame_with_view_and_clock(&mut state, &assets, &metrics,
                                        &mut actors, &mut text, &mut scratch, screen_gameplay::ActorViewOverride::default(),
                                        assets.texture_context(), Some(second_uptime));
                                    assert_eq!(state.current_music_time_display(), paused_music_time);
                                    check_meter_colors(&frame, lives, second_uptime, &full_life_samples[1].1, &mut failures);
                                }
                            }
                            set_stats(&mut state, 0, 0.0, false);
                            advance(&mut state, 931..=966);
                            if versus {
                                check_text(&state, [0.0, 0.75], [false, true], &mut failures);
                                let frame = compose_fixture_frame_with_textures(&mut state, &assets, &metrics,
                                    &mut actors, &mut text, &mut scratch, assets.texture_context());
                                if image_draw_count(&frame, bsod_handle) != 0
                                    || !published_overlay(&state, "Microsoft Word").initial_state.visible {
                                    failures.push(format!("{label}: one failed player must leave Word visible and BSOD hidden"));
                                }
                                set_stats(&mut state, 1, 0.0, true);
                                advance(&mut state, 967..=1002);
                            }
                            let frame = compose_fixture_frame_with_textures(&mut state, &assets, &metrics,
                                &mut actors, &mut text, &mut scratch, assets.texture_context());
                            let draws = image_draw_count(&frame, bsod_handle);
                            if draws != 1 || published_overlay(&state, "Microsoft Word").initial_state.visible {
                                failures.push(format!("{label}: draining every joined meter must hide Word and submit one visible BSOD draw; got {draws}"));
                            }
                        });
                    }
                }
                assert!(failures.is_empty(), "Paperclip gameplay-stat mismatches:\n{}", failures.join("\n"));
            }).unwrap().join().unwrap();
    }

    #[test]
    fn finite_player_reveal_preserves_visibility_alpha_and_transform() {
        std::thread::Builder::new().name("finite-player-reveal".into())
            .stack_size(16 * 1024 * 1024).spawn(|| {
                with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
                    true, false, || {
                    let metrics = space::Metrics::centered(854.0, 480.0);
                    space::set_current_metrics(metrics);
                    space::set_current_window_px(854, 480);
                    space::set_overscan(0, 0, 0, 0);
                    let directory = tempfile::tempdir().unwrap();
                    let simfile = directory.path().join("song.ssc");
                    fs::write(&simfile, "#TITLE:Finite Player reveal;\n#BPMS:0=120;\n#FGCHANGES:0=foreground.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n0000\n0000\n0000\n0000\n,\n1000\n0100\n0010\n0001\n;\n").unwrap();
                    // Preserve finite Player render-state coverage independently
                    // of Paperclip, whose stage-stat queries require a live VM.
                    fs::write(directory.path().join("foreground.lua"), r#"
return Def.ActorFrame {
    OnCommand=function(self)
        for name, child in pairs(SCREENMAN:GetTopScreen():GetChildren()) do
            if name ~= 'SongBackground' then child:visible(false) end
        end
        self:sleep(.5):queuecommand('Reveal'):sleep(3)
    end,
    RevealCommand=function(self)
        for _, child in pairs(SCREENMAN:GetTopScreen():GetChildren()) do
            child:visible(true)
        end
        SCREENMAN:GetTopScreen():GetChild('PlayerP1'):diffusealpha(.6):addx(13):addy(9)
    end,
}
"#).unwrap();
                    let assets = fixture_assets();
                    let mut baked = Vec::new();
                    for runtime in [false, true] {
                        let profiles = std::array::from_fn(|_| profile_data::Profile {
                            noteskin: profile_data::NoteSkin::new("cel"),
                            scroll_speed: ScrollSpeedSetting::MMod(600.0),
                            attack_mode: profile_data::AttackMode::On,
                            ..Default::default()
                        });
                        let mut state = build_test_state_with_lua_mode(&simfile,
                            GameplayViewport::new(854.0, 480.0), GameplaySession::default(),
                            profiles, "Software", Some(8.0), runtime);
                        assert!(state.mods.song_lua_compile_succeeded);
                        assert_eq!(state.song_lua.is_some(), runtime,
                            "noninteractive Player visibility must remain finitely compilable");
                        let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
                        let taps: HashSet<_> = ns.notes.iter()
                            .chain(ns.note_layers.iter().flat_map(|layers| layers.iter()))
                            .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                        let receptors: HashSet<_> = state.noteskin_assets.receptor_noteskin[0].as_ref().unwrap_or(ns)
                            .receptor_off.iter().map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                        let mut actors = Vec::new();
                        let mut text = compose::TextLayoutCache::default();
                        let mut scratch = compose::ComposeScratch::default();
                        for tick in 0..=90 {
                            set_fixture_time(&mut state, tick as f32 / 60.0);
                            refresh_active_attack_masks(&mut state.gameplay, 1.0 / 60.0);
                            let Some(probe) = [15, 90].iter().position(|&at| at == tick) else { continue; };
                            let frame = compose_fixture_frame(&mut state, &assets, &metrics,
                                &mut actors, &mut text, &mut scratch);
                            let draws = note_draws(&frame, &metrics, &receptors, &taps);
                            assert_eq!(draws.receptors, if tick == 15 { 0 } else { 4 },
                                "runtime={runtime}, tick={tick}: Player reveal");
                            if tick == 90 {
                                assert!(draws.taps > 0, "arrows are visible after the queued reveal");
                                assert!(draws.draws.iter().filter(|draw| draw.receptor)
                                    .all(|draw| (draw.alpha - 0.6).abs() < 0.0001),
                                    "Player render-state alpha reaches the final receptor draw");
                            }
                            if runtime {
                                assert_eq!(compare_note_draws(&baked[probe], &draws), Ok(()),
                                    "finite Player transforms apply once, tick={tick}");
                            } else {
                                baked.push(draws);
                            }
                        }
                    }
                });
            }).unwrap().join().unwrap();
    }

    // Native ITGmania's captured Paperclip run has no receptors at 14.340249 s,
    // all eight opaque receptors by 14.498889 s, and the first arrow at
    // 15.709864 s. Exercise the final gameplay draw stream around that reveal.
    #[test]
    fn paperclip_visibility_after_startup_in_requested_modes() {
        std::thread::Builder::new().name("paperclip-visibility".into())
            .stack_size(16 * 1024 * 1024).spawn(|| {
                let fixture = song_fixture::from_source("Paperclip/Paperclip.ssc");
                let mut failures = Vec::new();
                let mut requested_baked_frames: [Vec<NoteDraws>; 2] = std::array::from_fn(|_| Vec::new());
                for requested_runtime in [false, true] {
                    for versus in [false, true] {
                        let style = if versus { profile_data::PlayStyle::Versus }
                            else { profile_data::PlayStyle::Single };
                        with_session(style, profile_data::PlayerSide::P1, true, versus, || {
                            let metrics = space::Metrics::centered(854.0, 480.0);
                            space::set_current_metrics(metrics);
                            space::set_current_window_px(854, 480);
                            space::set_overscan(0, 0, 0, 0);
                            let profiles = std::array::from_fn(|_| profile_data::Profile {
                                noteskin: profile_data::NoteSkin::new("cel"),
                                scroll_speed: ScrollSpeedSetting::MMod(600.0),
                                attack_mode: profile_data::AttackMode::On,
                                ..Default::default()
                            });
                            let session = GameplaySession {
                                play_style: if versus { deadsync_gameplay::GameplayInputPlayStyle::Versus }
                                    else { deadsync_gameplay::GameplayInputPlayStyle::Single },
                                joined_sides: [true, versus],
                                ..Default::default()
                            };
                            let mut state = build_test_state_with_lua_preparation(&fixture.simfile,
                                GameplayViewport::new(854.0, 480.0), session, profiles,
                                "Vulkan (wgpu)", Some(70.0), requested_runtime, None,
                                Some(GameplayConfig { global_offset_seconds: -0.008, ..Default::default() }), true);
                            assert!(state.mods.song_lua_compile_succeeded);
                            assert!(state.song_lua.is_some(), "Paperclip's mutable stage-stat reads retain the VM in either requested mode");
                            let assets = fixture_assets();
                            let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
                            let taps: HashSet<_> = ns.notes.iter()
                                .chain(ns.note_layers.iter().flat_map(|layers| layers.iter()))
                                .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                            let receptors: HashSet<_> = state.noteskin_assets.receptor_noteskin[0].as_ref().unwrap_or(ns)
                                .receptor_off.iter().map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                            let mut actors = Vec::new();
                            let mut text = compose::TextLayoutCache::default();
                            let mut scratch = compose::ComposeScratch::default();
                            for tick in 0..=1200 {
                                let second = tick as f32 / 60.0;
                                set_fixture_time(&mut state, second);
                                refresh_active_attack_masks(&mut state.gameplay, 1.0 / 60.0);
                                let Some(probe) = [810, 900, 1200].iter().position(|&at| at == tick) else { continue; };
                                let frame = compose_fixture_frame(&mut state, &assets, &metrics,
                                    &mut actors, &mut text, &mut scratch);
                                let draws = note_draws(&frame, &metrics, &receptors, &taps);
                                let expected_receptors = if second < 14.34 { 0 } else if versus { 8 } else { 4 };
                                if draws.receptors != expected_receptors || (second == 20.0 && draws.taps == 0) {
                                    failures.push(format!("requested_runtime={requested_runtime}, versus={versus}, t={second}: {} receptors / {} taps; expected {expected_receptors} receptors and arrows by 20 s", draws.receptors, draws.taps));
                                }
                                if requested_runtime {
                                    if let Err(error) = compare_note_draws(&requested_baked_frames[usize::from(versus)][probe], &draws) {
                                        failures.push(format!("requested-mode geometry, versus={versus}, t={second}: {error}"));
                                    }
                                } else {
                                    requested_baked_frames[usize::from(versus)].push(draws);
                                }
                            }
                        });
                    }
                }
                assert!(failures.is_empty(), "Paperclip visibility mismatches:\n{}", failures.join("\n"));
            }).unwrap().join().unwrap();
    }
}
