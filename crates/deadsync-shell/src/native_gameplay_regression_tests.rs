// Full gameplay composition checks for the installed native presentation.

const NATIVE_GAMEPLAY_SIMFILE: &str = "\
#TITLE:Native gameplay composition;\n#BPMS:0=120;\n\
#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:8;\n#NOTES:\n\
1000\n0100\n0010\n0001\n,\n\
2000\n0100\n0010\n0001\n,\n\
3000\n0100\n0010\n0001\n,\n\
1000\n0100\n0010\n0001\n;\n";

fn gameplay_identity_fixture(reverse: bool, scripted: bool) -> MountedStageFixture {
    let source = if scripted {
        format!("#FGCHANGES:0=default.lua=1=0=0=1;\n{NATIVE_GAMEPLAY_SIMFILE}")
    } else {
        NATIVE_GAMEPLAY_SIMFILE.to_owned()
    };
    let path = write_fixture("native-gameplay-composition", &source);
    if scripted {
        fs::write(path.parent().unwrap().join("default.lua"),
            "return Def.ActorFrame{OnCommand=function(self) self:sleep(30) end,Def.Quad{Name='ForegroundMarker',InitCommand=function(self) self:xy(320,240):zoomto(24,24):diffuse(1,1,1,0.5) end}}").unwrap();
    }
    let profiles = std::array::from_fn(|side| {
        let mut profile = profile_data::Profile {
            display_name: format!("NAMED PLAYER {}", side + 1),
            noteskin: profile_data::NoteSkin::new("default"),
            scroll_speed: ScrollSpeedSetting::XMod(2.0),
            ..Default::default()
        };
        if reverse {
            profile.set_scroll_option(profile_data::ScrollOption::Reverse);
        }
        profile
    });
    let session = profile::get_session_snapshot();
    let mut state = build_mounted_test_state(
        &path,
        GameplayViewport::new(640.0, 480.0),
        GameplaySession {
            player_side: if session.player_side == profile_data::PlayerSide::P2 {
                deadsync_gameplay::GameplayInputPlayerSide::P2
            } else {
                deadsync_gameplay::GameplayInputPlayerSide::P1
            },
            play_style: if session.play_style.is_versus() {
                deadsync_gameplay::GameplayInputPlayStyle::Versus
            } else {
                deadsync_gameplay::GameplayInputPlayStyle::Single
            },
            joined_sides: [
                session.side_joined(profile_data::PlayerSide::P1),
                session.side_joined(profile_data::PlayerSide::P2),
            ],
            ..Default::default()
        },
        profiles,
        crate::theme_registry::mount(deadsync_config::theme::ThemeFlag::StepMania395).gameplay,
    );
    assert_eq!(state.song_frame().uses_song_lua_compositor(), scripted);
    if scripted {
        assert!(state.mods.song_lua_compile_succeeded);
        let visuals = state.song_lua_visuals();
        assert!(
            visuals
                .overlays
                .iter()
                .chain(
                    visuals
                        .foreground_visual_layers
                        .iter()
                        .flat_map(|layer| &layer.overlays)
                )
                .any(|actor| actor.name.as_deref() == Some("ForegroundMarker")),
            "the fixture must contain its compiled song-foreground marker"
        );
    }
    set_mounted_fixture_time(&mut state.stage, 2.5);
    for player in 0..state.num_players() {
        add_sprite_core_feedback(&mut state, player, 0, 42);
    }
    state
}

fn compose_native_fixture_frame<T: TextureContext + ?Sized>(
    state: &mut MountedStageFixture,
    assets: &deadlib_assets::AssetManager,
    metrics: &space::Metrics,
    actors: &mut Vec<Actor>,
    text: &mut compose::TextLayoutCache,
    scratch: &mut compose::ComposeScratch,
    hud_motion: deadsync_theme::views::GameplayHudMotion,
    textures: &T,
) -> deadlib_render_core::RenderFrame {
    actors.clear();
    let segments = crate::gameplay_runtime::push_actors(
        actors,
        state,
        assets,
        deadsync_assets::gameplay_presentation::GameplayRenderView {
            hud_motion,
            ..Default::default()
        },
        123.0,
        (),
    );
    compose::build_passes(
        segments.segments(state.song_frame(), actors),
        state.render_targets(),
        [0.0, 0.0, 0.0, 1.0],
        metrics,
        assets.fonts(),
        10.0,
        text,
        scratch,
        textures,
        Some(state.actor_resources()),
    )
}

#[test]
fn native_gameplay_hud_controls_footer_and_layering() {
    std::thread::Builder::new()
        .name("native-gameplay-footer".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let metrics = space::Metrics::centered(640.0, 480.0);
                    space::set_current_metrics(metrics);
                    space::set_current_window_px(640, 480);
                    for scripted in [false, true] {
                        let mut state = gameplay_identity_fixture(false, scripted);
                        let assets = fixture_assets();
                        let mut actors = Vec::new();
                        let mut text = compose::TextLayoutCache::default();
                        let mut scratch = compose::ComposeScratch::default();
                        let mut native_frame = compose_native_fixture_frame(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text,
                            &mut scratch,
                            Default::default(),
                            &FIXTURE_TEXTURES,
                        );
                        assert!(
                            has_native_profile_label(&actors, "NAMED PLAYER 1", 0),
                            "the mounted gameplay must use its native system-credit profile label"
                        );
                        let score_texture = FIXTURE_TEXTURES.texture_handle("stepmania-395/gameplay-score-frame");
                        let score_draw = native_frame.ops.iter().position(|op| matches!(op,
                            DrawOp::Sprite(run) if run.texture_handle == score_texture
                        )).expect("the mounted score frame draws");
                        let mut note_textures = std::collections::HashSet::new();
                        for skin in state.noteskin_assets.noteskin.iter().flatten() {
                            skin.for_each_slot(|slot| {
                                note_textures.insert(FIXTURE_TEXTURES.texture_handle(slot.texture_key()));
                            });
                        }
                        let last_note_draw = native_frame.ops.iter().rposition(|op|
                            matches!(op, DrawOp::Sprite(run) if note_textures.contains(&run.texture_handle))
                        ).expect("the prepared noteskin draws");
                        assert!(score_draw > last_note_draw, "HUD must follow player fields, scripted={scripted}");
                        if scripted {
                            let marker_draw = foreground_marker_draw(&native_frame);
                            assert!(marker_draw > score_draw, "song foreground must follow the HUD");
                            scratch.recycle_frame(&mut native_frame);
                            let entry_frame = compose_native_fixture_frame(
                                &mut state, &assets, &metrics, &mut actors, &mut text, &mut scratch,
                                deadsync_theme::views::GameplayHudMotion::Entering {
                                    elapsed_seconds: 0.1,
                                },
                                &FIXTURE_TEXTURES,
                            );
                            let marker_draw = foreground_marker_draw(&entry_frame);
                            let wipe_texture = FIXTURE_TEXTURES.texture_handle("stepmania-395/gameplay-in-right");
                            let wipe_draw = entry_frame.ops.iter().position(|op| matches!(op,
                                DrawOp::Sprite(run) if run.texture_handle == wipe_texture
                            )).expect("the gameplay entry wipe draws");
                            assert!(wipe_draw > marker_draw, "entry transition must cover song foreground");
                        }
                    }
                },
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

fn has_native_profile_label(actors: &[Actor], expected: &str, side: usize) -> bool {
    actors.iter().any(|actor| match actor {
        Actor::Text { content, font: "stepmania-395/credits", offset, .. } => {
            content.as_str() == expected
                && *offset == [if side == 0 { 120.0 } else { 520.0 }, 474.0]
        }
        Actor::RetainedFrame { frame, .. } => has_native_profile_label(frame.children(), expected, side),
        _ => false,
    })
}

fn foreground_marker_draw(frame: &deadlib_render_core::RenderFrame) -> usize {
    // Song artwork follows the original renderer's 8-bit vertex-color encoding.
    let marker_color =
        |rgba: [f32; 4]| rgba[..3] == [1.0; 3] && (rgba[3] - 0.5).abs() <= 1.0 / 255.0;
    frame
        .ops
        .iter()
        .position(|op| match op {
            DrawOp::Sprite(run) => frame.sprite_instances
                [run.instance_start as usize..(run.instance_start + run.instance_count) as usize]
                .iter()
                .any(|instance| marker_color(instance.tint)),
            DrawOp::Mesh(run) => frame.mesh_vertices
                [run.vertex_start as usize..(run.vertex_start + run.vertex_count) as usize]
                .iter()
                .any(|vertex| marker_color(vertex.color)),
            DrawOp::TexturedMesh(run) => {
                let vertices = &frame.tmesh_geometries[run.geometry as usize].vertices;
                frame.tmesh_instances[run.instance_start as usize
                    ..(run.instance_start + run.instance_count) as usize]
                    .iter()
                    .any(|instance| {
                        vertices.iter().any(|vertex| {
                            marker_color(std::array::from_fn::<_, 4, _>(|channel| {
                                vertex.color[channel] * instance.tint[channel]
                            }))
                        })
                    })
            }
        })
        .expect("the song foreground marker must actually render")
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a graphics device; captures the complete native gameplay composition"]
fn native_gameplay_composition_captures() {
    std::thread::Builder::new()
        .name("native-gameplay-capture".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            use deadsync_theme::ThemeResources;
            use winit::platform::windows::EventLoopBuilderExtWindows;
            crate::tests::init_paths();
            let events = winit::event_loop::EventLoop::builder()
                .with_any_thread(true)
                .build()
                .unwrap();
            #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
            let window = Arc::new(
                events
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(640, 480)),
                    )
                    .unwrap(),
            );
            let metrics = space::Metrics::centered(640.0, 480.0);
            let mut backend = deadlib_render::create_backend(
                deadlib_render_core::BackendType::VulkanWgpu,
                window,
                metrics.projection(),
                false,
                deadlib_render_core::PresentModePolicy::Immediate,
                false,
                true,
            )
            .unwrap();
            let mut assets = deadlib_assets::AssetManager::new();
            load_gpu_capture_assets(&mut assets, &mut backend).unwrap();
            deadsync_assets::load_initial_assets(
                &mut assets,
                &mut backend,
                deadsync_theme_stepmania_395::StepManiaTheme::asset_manifest(),
            )
            .unwrap();
            let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/native-395-gameplay-captures");
            fs::create_dir_all(&output).unwrap();
            for (name, style, side, joined, reverse, scripted) in [
                (
                    "single-p1",
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    [true, false],
                    false,
                    false,
                ),
                (
                    "single-p2",
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P2,
                    [false, true],
                    false,
                    false,
                ),
                (
                    "reverse-p1",
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    [true, false],
                    true,
                    false,
                ),
                (
                    "versus",
                    profile_data::PlayStyle::Versus,
                    profile_data::PlayerSide::P1,
                    [true, true],
                    false,
                    false,
                ),
                (
                    "scripted-versus",
                    profile_data::PlayStyle::Versus,
                    profile_data::PlayerSide::P1,
                    [true, true],
                    false,
                    true,
                ),
            ] {
                with_session(style, side, joined[0], joined[1], || {
                    space::set_current_metrics(metrics);
                    space::set_current_window_px(640, 480);
                    space::set_overscan(0, 0, 0, 0);
                    let mut state = gameplay_identity_fixture(reverse, scripted);
                    prewarm_gpu_noteskins(&state, &mut assets, &mut backend).unwrap();
                    let mut actors = Vec::new();
                    let mut text = compose::TextLayoutCache::default();
                    let mut scratch = compose::ComposeScratch::default();
                    let mut frame = compose_native_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text,
                        &mut scratch,
                        Default::default(),
                        assets.texture_context(),
                    );
                    for (side, joined) in joined.into_iter().enumerate() {
                        if joined {
                            assert!(has_native_profile_label(&actors, &format!("NAMED PLAYER {}", side + 1), side),
                                "{name} must use its physical player's native credit label");
                        }
                    }
                    backend.request_screenshot();
                    backend.draw(&frame, assets.textures(), false).unwrap();
                    let mut image = backend.capture_frame().unwrap();
                    deadsync_assets::screenshot::set_opaque_alpha(&mut image);
                    image.save(output.join(format!("{name}.png"))).unwrap();
                    assert!(!frame.ops.is_empty());
                    let mut note_textures = std::collections::HashSet::new();
                    for skin in state.noteskin_assets.noteskin.iter().flatten() {
                        skin.for_each_slot(|slot| {
                            note_textures.insert(
                                assets.texture_context().texture_handle(slot.texture_key()),
                            );
                        });
                    }
                    let note_sprites: usize = frame
                        .ops
                        .iter()
                        .filter_map(|op| match op {
                            DrawOp::Sprite(run) if note_textures.contains(&run.texture_handle) => {
                                Some(run.instance_count as usize)
                            }
                            _ => None,
                        })
                        .sum();
                    assert!(
                        note_sprites >= 8 * state.num_players(),
                        "{name} must contain resident note/receptor sprites, found {note_sprites}"
                    );
                    scratch.recycle_frame(&mut frame);
                });
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
