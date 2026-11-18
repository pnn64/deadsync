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

    fn compare_note_draws(baked: &NoteDraws, live: &NoteDraws) -> Result<(), String> {
        if baked.draws.len() != live.draws.len() {
            return Err(format!(
                "draw count: baked {}, runtime {}",
                baked.draws.len(),
                live.draws.len()
            ));
        }
        for (index, (baked, live)) in baked.draws.iter().zip(&live.draws).enumerate() {
            if baked.receptor != live.receptor
                || baked.texture != live.texture
                || baked.blend != live.blend
                || (baked.alpha - live.alpha).abs() > 0.0001
            {
                return Err(format!(
                    "draw {index} has different kind, texture, blend or alpha"
                ));
            }
            // Compare submitted geometry after the final camera/Player transforms.
            // Sprite and transformed-quad winding can differ between composition
            // paths, so match unique vertices without depending on vertex order.
            if baked.vertices.len() != live.vertices.len() {
                return Err(format!("draw {index} has different vertex counts"));
            }
            let mut remaining = live.vertices.clone();
            for baked in &baked.vertices {
                let Some(matched) = remaining
                    .iter()
                    .position(|live| baked.iter().zip(live).all(|(a, b)| (a - b).abs() < 0.001))
                else {
                    return Err(format!("draw {index} differs at clip vertex {baked:?}"));
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

    // Native ITGmania's captured Paperclip run has no receptors at 14.340249 s,
    // all eight opaque receptors by 14.498889 s, and the first arrow at
    // 15.709864 s. Exercise the final gameplay draw stream around that reveal.
    #[test]
    fn paperclip_visibility_after_startup_in_baked_and_runtime_modes() {
        std::thread::Builder::new().name("paperclip-visibility".into())
            .stack_size(16 * 1024 * 1024).spawn(|| {
                let fixture = song_fixture::from_source("Paperclip/Paperclip.ssc");
                let mut failures = Vec::new();
                let mut baked_frames: [Vec<NoteDraws>; 2] = std::array::from_fn(|_| Vec::new());
                for live in [false, true] {
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
                            let mut state = build_test_state_with_lua_mode_and_clocks(&fixture.simfile,
                                GameplayViewport::new(854.0, 480.0), session, profiles,
                                "Vulkan (wgpu)", Some(70.0), live, None,
                                Some(GameplayConfig { global_offset_seconds: -0.008, ..Default::default() }));
                            assert!(state.mods.song_lua_compile_succeeded);
                            assert_eq!(state.song_lua.is_some(), live);
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
                                    failures.push(format!("live={live}, versus={versus}, t={second}: {} receptors / {} taps; expected {expected_receptors} receptors and arrows by 20 s", draws.receptors, draws.taps));
                                }
                                if live {
                                    if let Err(error) = compare_note_draws(&baked_frames[usize::from(versus)][probe], &draws) {
                                        failures.push(format!("Baked/Runtime geometry, versus={versus}, t={second}: {error}"));
                                    }
                                } else {
                                    baked_frames[usize::from(versus)].push(draws);
                                }
                            }
                        });
                    }
                }
                assert!(failures.is_empty(), "Paperclip visibility mismatches:\n{}", failures.join("\n"));
            }).unwrap().join().unwrap();
    }
}
