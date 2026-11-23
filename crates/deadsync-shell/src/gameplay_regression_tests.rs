use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    ActiveComboMilestone, ComboMilestoneKind, CourseDisplayCarry, CourseDisplayTiming,
    CourseDisplayTotals, GameplayConfig, GameplayMiniIndicatorData, GameplayNoteskinData,
    GameplaySession, GameplayViewport, LeadInTiming, ReplayInputEdge, ReplayOffsetSnapshot,
    cached_hold_end_time_ns, refresh_active_attack_masks,
};
use deadsync_rules::scroll::ScrollSpeedSetting;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/support/song_fixture.rs"]
mod song_fixture;

use deadsync_profile_gameplay::GameplayProfile;

type State = deadsync_assets::gameplay_stage::GameplayCoreState;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use super::{MAX_PLAYERS, ScrollSpeedSetting, refresh_active_attack_masks};
    use deadlib_present::{
        actors::{Actor, SpriteSource},
        compose::{self, TextureContext, TextureMeta},
        space,
    };
    use deadlib_render_core::{
        DrawOp,
        frame_compare::{compare_render_frames, compare_render_frames_semantic},
    };
    use deadsync_assets::gameplay_noteskin::{GameplayNoteskinAssets, gameplay_noteskin_assets};
    use deadsync_assets::gameplay_stage::{
        ActiveGameplayStage, GameplayHost, GameplayStageInput, GameplayStagePresenter,
        PracticeEditorPresentation, PracticeLivePresentation, PracticeStagePresenter,
        PreparedGameplayStage, prepare_gameplay_stage,
    };
    use deadsync_assets::noteskin;
    use deadsync_assets::song_lua::compile_song_lua;
    use deadsync_chart::SongData;
    use deadsync_chart::{ChartData, GameplayChartData};
    use deadsync_core::note::NoteType;
    use deadsync_noteskin::{NoteskinSlot, Style};
    use deadsync_profile as profile_data;
    use deadsync_profile::compat as profile;
    use deadsync_rules::judgment::{JudgeGrade, Judgment, TimingWindow};
    use deadsync_song_lua::gameplay_presentation::{
        FieldHudBuffers, FieldPresentation, FramePresentationContext, GameplayFrameBuffers,
        GameplayFramePresenter, GameplayFrameView, SongFrameContent, compose_stage_frame,
    };
    use deadsync_song_lua::playback::{
        FieldFrame, FieldLayout, FrameScratch, GameplayActorSegments, ScreenLayer,
    };
    use deadsync_theme_simply_love::screens::gameplay as screen_gameplay;
    use deadsync_theme_simply_love::views::GameplayInitView;


    use std::sync::{Arc, LazyLock, Mutex};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    include!("looping_rooms_regression_tests.rs");
    include!("paperclip_regression_tests.rs");


    #[test]
    fn model_cache_prewarms_more_than_legacy_slot_limit() {
        crate::tests::init_paths();
        let style = Style {
            num_cols: 8,
            num_players: 1,
        };
        let load = |name| {
            Arc::new(
                noteskin::load_itg_skin(&style, name)
                    .unwrap_or_else(|error| panic!("dance/{name} should load: {error}")),
            )
        };
        let assets = GameplayNoteskinAssets {
            noteskin: [Some(load("lambda")), None],
            mine_noteskin: [Some(load("cel")), None],
            receptor_noteskin: [Some(load("ddr-note")), None],
            tap_explosion_noteskin: [Some(load("metal")), None],
        };

        let mut stable_ids = std::collections::HashSet::new();
        for skin in [
            assets.noteskin[0].as_ref(),
            assets.mine_noteskin[0].as_ref(),
            assets.receptor_noteskin[0].as_ref(),
            assets.tap_explosion_noteskin[0].as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            skin.for_each_slot(|slot| {
                stable_ids.insert(slot.stable_id());
            });
        }
        assert!(stable_ids.len() > 512);

        let caches = screen_gameplay::notefield_model_cache_from_assets(&assets, 1);
        let mut cache = caches[0].borrow_mut();
        for skin in [
            assets.noteskin[0].as_ref(),
            assets.mine_noteskin[0].as_ref(),
            assets.receptor_noteskin[0].as_ref(),
            assets.tap_explosion_noteskin[0].as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            skin.for_each_slot(|slot| {
                assert!(cache.prewarm_slot(slot), "slot should already be retained");
            });
        }
        assert_eq!(cache.stats().saturated_misses, 0);
        assert_eq!(cache.frame_stats().saturated_misses, 0);
    }

    #[inline(always)]
    fn init(
        song: Arc<SongData>,
        charts: [Arc<ChartData>; MAX_PLAYERS],
        gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
        viewport: super::GameplayViewport,
        session: super::GameplaySession,
        config: super::GameplayConfig,
        pack_sync_pref: deadsync_chart::SyncPref,
        mini_indicator_data: super::GameplayMiniIndicatorData,
        noteskin_data: super::GameplayNoteskinData,
        song_lua_data: crate::gameplay_entry::PreparedGameplaySongLua,
        active_color_index: i32,
        music_rate: f32,
        scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        replay_edges: Option<Vec<super::ReplayInputEdge>>,
        replay_offsets: Option<super::ReplayOffsetSnapshot>,
        lead_in_timing: Option<super::LeadInTiming>,
        course_display_carry: Option<[super::CourseDisplayCarry; MAX_PLAYERS]>,
        course_display_totals: Option<[super::CourseDisplayTotals; MAX_PLAYERS]>,
        course_display_timing: Option<super::CourseDisplayTiming>,
        combo_carry: [u32; MAX_PLAYERS],
    ) -> super::State {
        deadsync_gameplay::init_gameplay_runtime(
            song,
            charts,
            gameplay_charts,
            viewport,
            session,
            config,
            pack_sync_pref,
            mini_indicator_data,
            noteskin_data,
            song_lua_data,
            deadsync_gameplay::empty_crossover_annotations,
            active_color_index,
            music_rate,
            scroll_speed,
            player_profiles.map(GameplayProfile::from),
            replay_edges,
            replay_offsets,
            lead_in_timing,
            course_display_carry,
            course_display_totals,
            course_display_timing,
            None,
            [deadsync_gameplay::CourseLifeConfig::Bar; MAX_PLAYERS],
            false,
            combo_carry,
        )
    }

    struct SessionRestore {
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
    }

    impl Drop for SessionRestore {
        fn drop(&mut self) {
            profile::set_session_play_style(self.play_style);
            profile::set_session_player_side(self.player_side);
            profile::set_session_joined(self.p1_joined, self.p2_joined);
        }
    }

    pub(crate) fn with_session<R>(
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
        f: impl FnOnce() -> R,
    ) -> R {
        crate::tests::init_paths();
        let _lock = SESSION_TEST_LOCK.lock().expect("session test lock");
        let _restore = SessionRestore {
            play_style: profile::get_session_play_style(),
            player_side: profile::get_session_player_side(),
            p1_joined: profile::is_session_side_joined(profile_data::PlayerSide::P1),
            p2_joined: profile::is_session_side_joined(profile_data::PlayerSide::P2),
        };
        profile::set_session_play_style(play_style);
        profile::set_session_player_side(player_side);
        profile::set_session_joined(p1_joined, p2_joined);
        f()
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-gameplay-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct FixtureTextureContext;

    impl TextureContext for FixtureTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            0xf0f0_f0f0_f0f0_f0f0
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            Some(
                deadlib_assets::texture_dims(key)
                    .map(|meta| TextureMeta {
                        w: meta.w,
                        h: meta.h,
                    })
                    .unwrap_or(TextureMeta { w: 64, h: 64 }),
            )
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            deadlib_assets::sprite_sheet_dims(key)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render_core::TextureHandle {
            let hash = key.as_bytes().iter().fold(0x811c_9dc5u32, |hash, byte| {
                (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
            });
            u64::from(hash.max(1))
        }
    }

    const FIXTURE_TEXTURES: FixtureTextureContext = FixtureTextureContext;

    pub(crate) fn actor_tree_has_text(actors: &[Actor], expected: &str) -> bool {
        actors.iter().any(|actor| match actor {
            Actor::Text { content, .. } => content.as_str() == expected,
            Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
                actor_tree_has_text(children, expected)
            }
            Actor::SharedFrame { children, .. } | Actor::SharedTransform { children, .. }
            | Actor::SharedCamera { children, .. } => {
                actor_tree_has_text(children, expected)
            }
            Actor::RetainedFrame { frame, .. } => actor_tree_has_text(frame.children(), expected),
            Actor::Shadow { child, .. } => {
                actor_tree_has_text(std::slice::from_ref(child.as_ref()), expected)
            }
            Actor::Sprite { .. }
            | Actor::Mesh { .. }
            | Actor::ReusableMesh { .. }
            | Actor::TexturedMesh { .. }
            | Actor::ReusableTexturedMesh { .. }
            | Actor::CameraPush { .. }
            | Actor::CameraPop => false,
        })
    }

    fn top_screen_text_draw(
        actors: &[Actor],
        expected: &str,
        base_z: i16,
        alpha: f32,
    ) -> Option<(i16, f32)> {
        actors.iter().find_map(|actor| match actor {
            Actor::Text {
                content, color, z, ..
            } if content.as_str() == expected => {
                Some((base_z.saturating_add(*z), alpha * color[3]))
            }
            Actor::Frame { children, z, .. } => {
                top_screen_text_draw(children, expected, base_z.saturating_add(*z), alpha)
            }
            Actor::SharedFrame {
                children, z, tint, ..
            }
            | Actor::SharedTransform {
                children, z, tint, ..
            } => top_screen_text_draw(
                children,
                expected,
                base_z.saturating_add(*z),
                alpha * tint[3],
            ),
            Actor::RetainedFrame {
                frame,
                z,
                tint,
                visible,
                ..
            } if *visible => top_screen_text_draw(
                frame.children(),
                expected,
                base_z.saturating_add(*z),
                alpha * tint[3],
            ),
            Actor::Camera { children, .. } => {
                top_screen_text_draw(children, expected, base_z, alpha)
            }
            Actor::SharedCamera { children, .. } => {
                top_screen_text_draw(children, expected, base_z, alpha)
            }
            Actor::Shadow { child, .. } => top_screen_text_draw(
                std::slice::from_ref(child.as_ref()),
                expected,
                base_z,
                alpha,
            ),
            Actor::Sprite { .. }
            | Actor::Text { .. }
            | Actor::Mesh { .. }
            | Actor::ReusableMesh { .. }
            | Actor::TexturedMesh { .. }
            | Actor::ReusableTexturedMesh { .. }
            | Actor::RetainedFrame { .. }
            | Actor::CameraPush { .. }
            | Actor::CameraPop => None,
        })
    }

    fn max_render_target_sprite_z(actors: &[Actor], base_z: i16) -> Option<i16> {
        actors
            .iter()
            .filter_map(|actor| match actor {
                Actor::Sprite {
                    source: SpriteSource::RenderTarget { .. },
                    z,
                    visible: true,
                    ..
                } => Some(base_z.saturating_add(*z)),
                Actor::Frame { children, z, .. } => {
                    max_render_target_sprite_z(children, base_z.saturating_add(*z))
                }
                Actor::SharedFrame { children, z, .. }
                | Actor::SharedTransform { children, z, .. } => {
                    max_render_target_sprite_z(children, base_z.saturating_add(*z))
                }
                Actor::RetainedFrame {
                    frame,
                    z,
                    visible: true,
                    ..
                } => max_render_target_sprite_z(frame.children(), base_z.saturating_add(*z)),
                Actor::Camera { children, .. } => max_render_target_sprite_z(children, base_z),
                Actor::SharedCamera { children, .. } => max_render_target_sprite_z(children, base_z),
                Actor::Shadow { child, .. } => {
                    max_render_target_sprite_z(std::slice::from_ref(child.as_ref()), base_z)
                }
                Actor::Sprite { .. }
                | Actor::Text { .. }
                | Actor::Mesh { .. }
                | Actor::ReusableMesh { .. }
                | Actor::TexturedMesh { .. }
                | Actor::ReusableTexturedMesh { .. }
                | Actor::RetainedFrame { .. }
                | Actor::CameraPush { .. }
                | Actor::CameraPop => None,
            })
            .max()
    }

    pub(crate) fn fixture_assets() -> deadlib_assets::AssetManager {
        let mut assets = deadlib_assets::AssetManager::new();
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let asset_root = project_root.join("assets");
        for spec in deadsync_theme_simply_love::asset_manifest().fonts {
            let ini_path = project_root.join(spec.ini_path);
            let mut font =
                deadlib_assets::parse_font_with_asset_context(&ini_path, vec![asset_root.clone()])
                    .unwrap_or_else(|error| {
                        panic!(
                            "fixture font '{}' at '{}' should parse: {error}",
                            spec.name,
                            ini_path.display()
                        )
                    })
                    .font;
            deadlib_assets::set_font_fallback(&mut font, spec.fallback_font_name);
            assets.register_font(spec.name, font);
        }
        assets
    }

    fn build_test_state(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
    ) -> screen_gameplay::State {
        build_test_state_with_renderer(
            simfile,
            viewport,
            session,
            player_profiles,
            &deadlib_render_core::BackendType::Software.to_string(),
        )
    }

    /// Bound compilation for compositor fixtures that only exercise the opening.
    pub(crate) fn build_compiled_test_state(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
    ) -> screen_gameplay::State {
        build_test_state_with_renderer_horizon(simfile, viewport, session, player_profiles,
            &deadlib_render_core::BackendType::Software.to_string(), Some(32.0))
    }

    fn build_test_state_with_renderer(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        renderer: &str,
    ) -> screen_gameplay::State {
        build_test_state_with_renderer_horizon(
            simfile,
            viewport,
            session,
            player_profiles,
            renderer,
            None,
        )
    }

    fn build_test_state_with_renderer_horizon(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        renderer: &str,
        until_beat: Option<f32>,
    ) -> screen_gameplay::State {
        build_test_state_with_lua_mode(simfile, viewport, session, player_profiles,
            renderer, until_beat, false)
    }

    fn build_test_state_with_lua_mode(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        renderer: &str,
        until_beat: Option<f32>,
        live_diagnostic: bool,
    ) -> screen_gameplay::State {
        build_test_state_with_lua_mode_and_clocks(
            simfile,
            viewport,
            session,
            player_profiles,
            renderer,
            until_beat,
            live_diagnostic,
            None,
            None,
        )
    }

    fn build_test_state_with_lua_mode_and_clocks(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        renderer: &str,
        until_beat: Option<f32>,
        live_diagnostic: bool,
        configure_compile_clocks: Option<&dyn Fn(&mut deadsync_song_lua::SongLuaCompileContext)>,
        config_override: Option<GameplayConfig>,
    ) -> screen_gameplay::State {
        build_test_state_with_lua_preparation(
            simfile, viewport, session, player_profiles, renderer, until_beat,
            live_diagnostic, configure_compile_clocks, config_override, false,
        )
    }

    // Bounded gameplay fixtures can exercise the same dependency fallback as
    // production without changing tests that explicitly verify finite capture.
    fn build_test_state_with_lua_preparation(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        renderer: &str,
        until_beat: Option<f32>,
        live_diagnostic: bool,
        configure_compile_clocks: Option<&dyn Fn(&mut deadsync_song_lua::SongLuaCompileContext)>,
        config_override: Option<GameplayConfig>,
        runtime_fallback: bool,
    ) -> screen_gameplay::State {
        let (prepared, scene, mut init) = prepare_stage_fixture(
            simfile, viewport, session, player_profiles, renderer, until_beat,
            live_diagnostic, configure_compile_clocks, config_override, runtime_fallback,
        );
        let scores = std::mem::take(&mut init.scores);
        crate::gameplay_runtime::present_stage(
            prepared,
            scene,
            None,
            crate::gameplay_runtime::GameplayStartup {
                mini_indicator: Default::default(),
                lobby_session: deadsync_online::gameplay_lobby::GameplayLobbySession::new(
                    Default::default(),
                    init.runtime.joined,
                    init.runtime.player_side,
                ),
                score_refresh: Default::default(),
                scores,
            },
            move |stage, assets, scores| {
                init.scores = scores;
                screen_gameplay::GameplayPresentation::from_stage(
                    stage, assets, Arc::from("EVENT"), None, None, None, init,
                )
            },
        )
    }

    fn prepare_stage_fixture(
        simfile: &Path,
        viewport: GameplayViewport,
        session: GameplaySession,
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
        renderer: &str,
        until_beat: Option<f32>,
        live_diagnostic: bool,
        configure_compile_clocks: Option<&dyn Fn(&mut deadsync_song_lua::SongLuaCompileContext)>,
        config_override: Option<GameplayConfig>,
        runtime_fallback: bool,
    ) -> (PreparedGameplayStage, Option<deadsync_song_lua::SongLuaLiveScene>, GameplayInitView) {
        // Ordinary fixtures use zero global offset. Native-oracle fixtures can
        // supply recorded preferences consistently to parsing and playback.
        let config = config_override.unwrap_or(GameplayConfig {
            global_offset_seconds: 0.0,
            ..Default::default()
        });
        let song = Arc::new(
            deadsync_simfile::app_runtime::parse_song_for_test(simfile, config.global_offset_seconds)
                .expect("generated gameplay fixture should parse"),
        );
        let chart_ix = song
            .charts
            .iter()
            .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
            .unwrap_or(0);
        let gameplay_chart = Arc::new(
            deadsync_simfile::app_runtime::load_gameplay_charts(
                &song, &[chart_ix], config.global_offset_seconds,
            )
                .expect("generated gameplay fixture chart should load")
                .remove(0),
        );
        let chart = Arc::new(song.charts[chart_ix].clone());
        let charts = [chart.clone(), chart];
        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
        let scroll_speed = [
            player_profiles[0].scroll_speed,
            player_profiles[1].scroll_speed,
        ];
        let mut hud = profile::gameplay_hud_snapshot();
        hud.p1
            .display_name
            .clone_from(&player_profiles[0].display_name);
        hud.p1.guest = false;
        hud.p1.hide_username = false;
        hud.p2
            .display_name
            .clone_from(&player_profiles[1].display_name);
        hud.p2.guest = false;
        hud.p2.hide_username = false;
        let init_view = GameplayInitView {
            hud,
            ..Default::default()
        };
        let prepared = if until_beat.is_none() && configure_compile_clocks.is_none() {
            crate::gameplay_entry::prepare_song_lua(
                &song,
                &charts,
                std::array::from_fn(|p| &gameplay_charts[p].timing),
                &player_profiles,
                &scroll_speed,
                1.0,
                viewport,
                space::current_window_px(),
                16.0 / 9.0,
                0.0,
                &session,
                &config,
                renderer,
                1,
                if live_diagnostic {
                    deadsync_config::options::SongLuaMode::Runtime
                } else {
                    deadsync_config::options::SongLuaMode::Baked
                },
                None,
            )
        } else {
            let mut context = crate::gameplay_entry::song_lua_context(
                &song,
                &charts,
                std::array::from_fn(|p| &gameplay_charts[p].timing),
                &player_profiles,
                &scroll_speed,
                1.0,
                viewport,
                space::current_window_px(),
                16.0 / 9.0,
                0.0,
                &session,
                &config,
                renderer,
                1,
                live_diagnostic,
            )
            .expect("production Lua context");
            if let Some(beat) = until_beat {
                context.music_length_seconds = gameplay_charts[0].timing.get_time_for_beat_exact(beat);
            }
            if let Some(configure_clocks) = configure_compile_clocks {
                configure_clocks(&mut context);
            }
            if runtime_fallback {
                deadsync_song_lua::playback::prepare_song_lua_with_runtime_fallback(
                    &song, &context, deadsync_assets::song_lua::compile_song_lua,
                    deadsync_assets::song_lua::compile_song_lua_layers,
                )
            } else {
                deadsync_song_lua::playback::prepare_song_lua(
                    &song,
                    &context,
                    deadsync_assets::song_lua::compile_song_lua,
                    deadsync_assets::song_lua::compile_song_lua_layers,
                )
            }
        };
        let (prepared, scene) = prepare_gameplay_stage(GameplayStageInput {
            song,
            charts,
            gameplay_charts,
            viewport,
            session,
            config,
            active_color_index: 5,
            music_rate: 1.0,
            scroll_speed,
            player_profiles,
            replay_edges: None,
            replay_offsets: None,
            lead_in_timing: None,
            course_display_carry: None,
            course_display_totals: None,
            course_display_timing: None,
            course_modifiers: None,
            course_life_config: [deadsync_gameplay::CourseLifeConfig::Bar; MAX_PLAYERS],
            include_post_fail_passes: false,
            combo_carry: [0; MAX_PLAYERS],
            song_lua_data: prepared,
            mini_indicator: Default::default(),
        });
        (prepared, scene, init_view)
    }

    #[test]
    fn lua_songs_render_combo_numbers_in_baked_and_runtime_modes() {
        std::thread::Builder::new()
            .name("lua-combo-visibility".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
                    true, false, || {
                    let metrics = space::Metrics::centered(640.0, 480.0);
                    space::set_current_metrics(metrics);
                    space::set_current_window_px(640, 480);
                    let dir = tempfile::tempdir().unwrap();
                    let simfile = dir.path().join("song.ssc");
                    fs::write(&simfile, "#TITLE:Lua combo regression;\n#BPMS:0=120;\n#FGCHANGES:0=default.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n").unwrap();
                    let assets = fixture_assets();
                    for live in [false, true] {
                        for (hide_profile, hide_script) in [(false, false), (true, false), (false, true)] {
                            fs::write(dir.path().join("default.lua"), if hide_script {
                                "return Def.ActorFrame{OnCommand=function() SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('Combo'):visible(false) end}"
                            } else {
                                "return Def.ActorFrame{}"
                            }).unwrap();
                            let profiles = std::array::from_fn(|_| profile_data::Profile {
                                hide_combo: hide_profile,
                                ..Default::default()
                            });
                            let mut state = build_test_state_with_lua_mode(&simfile,
                                GameplayViewport::new(640.0, 480.0), GameplaySession::default(),
                                profiles, "software", None, live);
                            assert!(state.mods.song_lua_compile_succeeded);
                            set_fixture_time(&mut state, 1.0);
                            let mut actors = Vec::new();
                            let mut text_cache = compose::TextLayoutCache::default();
                            let mut scratch = compose::ComposeScratch::default();
                            let without_combo = compose_fixture_frame(&mut state, &assets, &metrics,
                                &mut actors, &mut text_cache, &mut scratch).ops.len();
                            state.players_runtime.players[0].combo = 25;
                            let with_combo = compose_fixture_frame(&mut state, &assets, &metrics,
                                &mut actors, &mut text_cache, &mut scratch).ops.len();
                            if hide_profile || hide_script {
                                assert_eq!(with_combo, without_combo, "hidden combo: live={live}, profile={hide_profile}, script={hide_script}");
                            } else {
                                assert!(with_combo > without_combo, "Lua combo must emit digits: live={live}");
                            }
                        }
                    }
                });
            }).unwrap().join().unwrap();
    }

    #[test]
    fn lua_submission_uses_the_current_compile_for_both_services() {
        std::thread::Builder::new()
            .name("lua-submission-eligibility".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
                    true, false, || {
                    space::set_current_metrics(space::Metrics::centered(640.0, 480.0));
                    space::set_current_window_px(640, 480);
                    let dir = tempfile::tempdir().unwrap();
                    let simfile = dir.path().join("song.ssc");
                    fs::write(&simfile, "#TITLE:Lua submission regression;\n#BPMS:0=120;\n#FGCHANGES:0=default.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n").unwrap();
                    for succeeds in [true, false, true] {
                        fs::write(dir.path().join("default.lua"), if succeeds {
                            "return Def.ActorFrame{}"
                        } else {
                            "return ???"
                        }).unwrap();
                        let profiles = std::array::from_fn(|_| profile_data::Profile {
                            groovestats_is_pad_player: true,
                            ..Default::default()
                        });
                        let mut state = build_test_state(&simfile,
                            GameplayViewport::new(640.0, 480.0), GameplaySession::default(), profiles);
                        state.progress.stage.song_completed_naturally = true;
                        assert!(state.song().has_lua);
                        assert!(!deadsync_score::lua_chart_submit_allowed(&state.charts()[0].short_hash));
                        assert_eq!(state.mods.song_lua_compile_succeeded, succeeds);
                        let arrowcloud = deadsync_online::arrowcloud::gameplay_submit_player_from_runtime(
                            &state.gameplay, 0, profile_data::PlayerSide::P1, None, |_| false, "Test",
                        );
                        assert_eq!(arrowcloud.lua_submit_allowed, succeeds);
                        let groovestats = deadsync_online::groovestats::gameplay_submit_player_from_runtime(
                            &state.gameplay, 0, profile_data::PlayerSide::P1, None, None, |_| false, true,
                        );
                        assert_eq!(groovestats.invalid_reason.is_none(), succeeds,
                            "{:?}", groovestats.invalid_reason);
                        let evaluation = deadsync_online::groovestats::eval_state_from_runtime(
                            &state.gameplay, 0, |_| false, false, true,
                        );
                        assert_eq!(evaluation.valid, succeeds, "{:?}", evaluation.reason_lines);
                        assert_eq!(deadsync_profile_gameplay::groovestats_eval_state_from_app_runtime(
                            &state.gameplay, 0).valid, succeeds);
                    }
                });
            }).unwrap().join().unwrap();
    }

    fn set_fixture_time(state: &mut screen_gameplay::State, music_time: f32) {
        state.boundary.total_elapsed_in_screen = 10.0;
        state.set_current_music_time_ns(deadsync_core::song_time::song_time_ns_from_seconds(
            music_time,
        ));
        refresh_active_attack_masks(&mut state.gameplay, 0.0);
        if state.song_lua.is_some() {
            // Foreground::Update dispatches On at the authored starting beat;
            // changing the simulation clock alone does not activate actors.
            screen_gameplay::update(state, 0.0, deadsync_gameplay::GameplayAudioSnapshot {
                stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                    music_nanos: deadsync_core::song_time::song_time_ns_from_seconds(music_time),
                    has_music_mapping: true, valid_at_host_nanos: 1_000_000,
                    ..Default::default()
                }, ..Default::default()
            }, || 1_000_000, &mut Vec::new());
            let output = state.stage.take_output();
            assert!(output.errors.is_empty(), "fixture Lua frame failed: {:?}", output.errors);
        }
    }

    fn add_sprite_core_feedback(
        state: &mut screen_gameplay::State,
        player_idx: usize,
        column: usize,
        combo: u32,
    ) {
        let judgment = Judgment {
            time_error_ms: -7.0,
            time_error_music_ns: -7_000_000,
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        state.players_runtime.players[player_idx].combo = combo;
        state.players_runtime.players[player_idx].full_combo_grade = Some(JudgeGrade::Fantastic);
        state.players_runtime.players[player_idx].current_combo_grade = Some(JudgeGrade::Fantastic);
        state.players_runtime.players[player_idx].judgment_counts[0] = combo;
        state.players_runtime.players[player_idx]
            .combo_milestones
            .push(ActiveComboMilestone {
                kind: ComboMilestoneKind::Hundred,
                elapsed: 0.2,
            });
        state.set_last_judgment(player_idx, judgment);
        state.error_bar_register_tap(player_idx, &judgment, 2.5);
        state.trigger_tap_judgment_explosion(player_idx, column, &judgment);
    }

    fn compose_fixture_frame_with_textures<T: TextureContext + ?Sized>(
        state: &mut screen_gameplay::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
        texture_ctx: &T,
    ) -> deadlib_render_core::RenderFrame {
        compose_fixture_frame_with_view(
            state,
            assets,
            metrics,
            actors,
            text_cache,
            scratch,
            screen_gameplay::ActorViewOverride::default(),
            texture_ctx,
        )
    }

    #[derive(Clone, Copy)]
    struct FixtureFrameView {
        presentation: screen_gameplay::ActorViewOverride,
        preview: Option<SongFrameContent>,
    }

    impl From<screen_gameplay::ActorViewOverride> for FixtureFrameView {
        fn from(presentation: screen_gameplay::ActorViewOverride) -> Self {
            Self {
                presentation,
                preview: None,
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the render-frame fixture keeps actor composition inputs explicit"
    )]
    fn compose_fixture_frame_with_view<T: TextureContext + ?Sized>(
        state: &mut screen_gameplay::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
        view: impl Into<FixtureFrameView>,
        texture_ctx: &T,
    ) -> deadlib_render_core::RenderFrame {
        compose_fixture_frame_with_view_and_clock(
            state, assets, metrics, actors, text_cache, scratch, view, texture_ctx, None,
        )
    }

    #[expect(clippy::too_many_arguments, reason = "native frame inputs remain explicit")]
    fn compose_fixture_frame_with_view_and_clock<T: TextureContext + ?Sized>(
        state: &mut screen_gameplay::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
        view: impl Into<FixtureFrameView>,
        texture_ctx: &T,
        game_time: Option<f64>,
    ) -> deadlib_render_core::RenderFrame {
        actors.clear();
        let FixtureFrameView {
            presentation: view,
            preview,
        } = view.into();
        let uptime_seconds = game_time.unwrap_or(123.0);
        let policy = deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default();
        let segments = match preview {
            Some(song_content) => screen_gameplay::compose_frame(
                state,
                actors,
                FramePresentationContext {
                    assets,
                    view,
                    uptime_seconds,
                    policy,
                },
                song_content,
            ),
            None => {
                crate::gameplay_runtime::push_actors(actors, state, assets, view, uptime_seconds, policy)
            }
        };
        let actor_segments = segments.segments(state.song_frame(), actors);
        compose::build_passes(
            actor_segments,
            state.render_targets(),
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            assets.fonts(),
            game_time.map_or(10.0, |time| time as f32),
            text_cache,
            scratch,
            texture_ctx,
            Some(state.actor_resources()),
        )
    }

    fn compose_fixture_frame(
        state: &mut screen_gameplay::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render_core::RenderFrame {
        compose_fixture_frame_with_textures(
            state,
            assets,
            metrics,
            actors,
            text_cache,
            scratch,
            &FIXTURE_TEXTURES,
        )
    }

    fn compose_practice_fixture_frame(
        state: &mut deadsync_theme_simply_love::screens::practice::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render_core::RenderFrame {
        actors.clear();
        let segments = crate::gameplay_runtime::push_practice_actors(
            actors,
            state,
            assets,
            123.0,
            deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
        );
        let actor_segments = segments.segments(state.gameplay.song_frame(), actors);
        compose::build_passes(
            actor_segments,
            state.gameplay.render_targets(),
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            assets.fonts(),
            10.0,
            text_cache,
            scratch,
            &FIXTURE_TEXTURES,
            Some(state.gameplay.actor_resources()),
        )
    }

    fn prepare_fixture_frame(
        state: &mut screen_gameplay::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
    ) -> usize {
        let mut render =
            compose_fixture_frame(state, assets, metrics, actors, text_cache, compose_scratch);
        let checksum = render
            .ops
            .len()
            .wrapping_add(render.sprite_instances.len())
            .wrapping_add(render.mesh_vertices.len())
            .wrapping_add(render.tmesh_instances.len())
            .wrapping_add(
                render
                    .render_targets
                    .iter()
                    .map(|target| {
                        target
                            .ops
                            .len()
                            .wrapping_add(target.sprite_instances.len())
                            .wrapping_add(target.mesh_vertices.len())
                            .wrapping_add(target.tmesh_instances.len())
                    })
                    .sum::<usize>(),
            );
        compose_scratch.recycle_frame(&mut render);
        actors.clear();
        checksum
    }

    fn assert_repeatable_frame(
        state: &mut screen_gameplay::State,
        assets: &deadlib_assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render_core::RenderFrame {
        let expected = assert_repeatable_composition(compose_scratch, |scratch| {
            compose_fixture_frame(state, assets, metrics, actors, text_cache, scratch)
        });
        if !state.song_frame().uses_song_lua_compositor() {
            assert!(state.song_lua.is_none(), "ordinary charts must not own a Lua VM");
            assert!(state.song_frame().render_targets().is_empty());
            // The existing scripted compositor is the behavior oracle for the
            // same immutable simulation, native chart attacks and theme HUD.
            let native = state.song_scratch.take().unwrap();
            let mut scripted = deadsync_song_lua::playback::FrameScratch::new(&state.gameplay);
            scripted.enable_dynamic_scene();
            state.song_scratch = Some(Box::new(scripted));
            let mut actual = compose_fixture_frame(
                state, assets, metrics, actors, text_cache, compose_scratch,
            );
            assert_eq!(compare_render_frames(&expected, &actual), Ok(()),
                "native no-Lua composition must preserve the existing frame");
            compose_scratch.recycle_frame(&mut actual);
            state.song_scratch = Some(native);
        }
        expected
    }

    fn assert_repeatable_composition(
        compose_scratch: &mut compose::ComposeScratch,
        mut compose_frame: impl FnMut(&mut compose::ComposeScratch) -> deadlib_render_core::RenderFrame,
    ) -> deadlib_render_core::RenderFrame {
        let mut warm = compose_frame(compose_scratch);
        compose_scratch.recycle_frame(&mut warm);

        let mut expected_frame = compose_frame(compose_scratch);
        let expected = expected_frame.clone();
        compose_scratch.recycle_frame(&mut expected_frame);
        let mut actual = compose_frame(compose_scratch);

        assert!(
            expected
                .ops
                .iter()
                .any(|op| matches!(op, DrawOp::Sprite(_)))
        );
        assert!(
            expected
                .ops
                .iter()
                .any(|op| matches!(op, DrawOp::TexturedMesh(_)))
                || expected.render_targets.iter().any(|target| {
                    target
                        .ops
                        .iter()
                        .any(|op| matches!(op, DrawOp::TexturedMesh(_)))
                })
        );
        assert_eq!(compare_render_frames(&expected, &actual), Ok(()));
        compose_scratch.recycle_frame(&mut actual);
        expected
    }

    fn generated_sprite_core_simfile() -> &'static str {
        r#"#VERSION:0.83;
#TITLE:F0 Sprite Core;
#MUSIC:;
#OFFSET:0.000;
#BPMS:0.000=120.000;

#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:F0-sprite-core;
#DIFFICULTY:Challenge;
#METER:9;
#RADARVALUES:0,0,0,0,0;
#NOTES:
1000
0100
0010
0001
,
L000
0100
00F0
0001
1100
0011
1001
0110
1111
1000
0100
0010
0001
1010
0101
1111
,
1100
0011
1001
0110
1111
1000
0100
0010
0001
1010
0101
1111
1100
0011
1001
0110
,
1000
0100
0010
0001
;
"#
    }

    fn write_fixture(name: &str, contents: &str) -> PathBuf {
        let song_dir = test_dir(name);
        let simfile = song_dir.join(format!("{name}.ssc"));
        fs::write(&simfile, contents).unwrap();
        simfile
    }

    fn sprite_core_fixture(
        simfile: &Path,
    ) -> (
        screen_gameplay::State,
        deadlib_assets::AssetManager,
        space::Metrics,
    ) {
        let metrics = space::Metrics::centered(640.0, 480.0);
        space::set_current_metrics(metrics);
        space::set_current_window_px(640, 480);
        space::set_overscan(0, 0, 0, 0);

        let mut profiles = [
            profile_data::Profile::default(),
            profile_data::Profile::default(),
        ];
        profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
        profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
        let mut state = build_test_state(
            simfile,
            GameplayViewport::new(640.0, 480.0),
            GameplaySession::default(),
            profiles,
        );
        set_fixture_time(&mut state, 2.5);
        add_sprite_core_feedback(&mut state, 0, 0, 42);
        (state, fixture_assets(), metrics)
    }

    #[test]
    fn compiled_gameplay_keeps_reversal_splines_without_a_lua_vm() {
        with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
            true, false, || {
            let simfile = write_fixture("compiled-reversal", r#"
#TITLE:Compiled reversal;
#MUSIC:;
#BPMS:0=120;
#LASTSECONDHINT:2;
#FGCHANGES:0=default.lua=1=0=0=0=StretchNoLoop====;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Challenge;
#METER:1;
#NOTES:
1000
0100
0010
0001
;
"#);
            fs::write(simfile.parent().unwrap().join("default.lua"), r#"
return Def.ActorFrame { OnCommand=function(self)
    local column = SCREENMAN:GetTopScreen():GetChild('PlayerP1'):GetChild('NoteField'):GetColumnActors()[1]
    self:SetUpdateFunction(function()
        local beat = GAMESTATE:GetSongBeat()
        local position, zoom = column:GetPosHandler(), column:GetZoomHandler()
        if beat >= 3 then
            position:SetSplineMode('NoteColumnSplineMode_Disabled')
            zoom:SetSplineMode('NoteColumnSplineMode_Disabled')
            return
        end
        local reversed = beat >= 1 and beat < 2
        local direction, scale = reversed and -1 or 1, reversed and 0.5 or 1
        for _, handler in ipairs({position, zoom}) do
            handler:SetSplineMode('NoteColumnSplineMode_Position')
            handler:SetSubtractSongBeat(true):SetReceptorT(0):SetBeatsPerT(1)
        end
        local p, z = position:GetSpline(), zoom:GetSpline()
        p:SetSize(2):SetPoint(1,{0,-135*direction,0}):SetPoint(2,{0,(-135+128)*direction,0}):Solve()
        z:SetSize(2):SetPoint(1,{scale,scale,scale}):SetPoint(2,{scale,scale,scale}):Solve()
    end)
end }
"#).unwrap();
            space::set_current_metrics(space::Metrics::centered(854.0, 480.0));
            space::set_current_window_px(1280, 720);
            let state = build_test_state(&simfile, GameplayViewport::design(),
                GameplaySession::default(), std::array::from_fn(|_| profile_data::Profile::default()));
            assert!(state.song_lua.is_none(), "production compilation releases its VM");
            let track = state.song_lua_visuals().column_splines[0].iter()
                .find(|track| track.column == 0).expect("compiled position and zoom track");
            for (second, direction, scale) in [(0.375,1.0,1.0),(0.875,-1.0,0.5),(1.375,1.0,1.0)] {
                let frame = track.at_second(second).expect("baked spline frame");
                let (position, derivative) = frame.position.as_ref().unwrap().view().sample(second*2.0,second*2.0);
                assert_eq!(position[1],-135.0*direction);
                assert_eq!(derivative[1],128.0*direction);
                assert_eq!(frame.zoom.as_ref().unwrap().view().sample(second*2.0,second*2.0).0,[scale;3]);
            }
            let cleared = track.at_second(1.875).unwrap();
            assert!(cleared.position.is_none() && cleared.zoom.is_none());
        });
    }

    #[test]
    fn sharkmode_compiled_draws_keep_notes_in_direct_and_texture_passes() {
        let fixture = song_fixture::from_source("280-MODS-[MASTER] Sharkmode/Sharkmode.ssc");
        with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
            true, false, || {
            let metrics = space::Metrics::centered(854.0, 480.0);
            space::set_current_metrics(metrics);
            space::set_current_window_px(1280, 720);
            let mut state = build_test_state(&fixture.simfile, GameplayViewport::design(),
                GameplaySession::default(), std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("lambda"), ..Default::default()
                }));
            assert!(state.song_lua.is_none(), "production must release the compiler VM");
            let visuals = state.song_lua_visuals();
            assert!(visuals.overlays.iter().chain(visuals.background_visual_layers.iter()
                .chain(&visuals.foreground_visual_layers).flat_map(|layer| &layer.overlays)).any(|actor| matches!(
                &actor.kind, deadsync_song_lua::SongLuaOverlayKind::UpdateTracks { draw_frames, .. }
                if !draw_frames.is_empty())), "custom draws must be baked before gameplay");
            let assets = fixture_assets();
            let mut note_handles = Vec::new();
            state.noteskin_assets.noteskin[0].as_ref().unwrap().for_each_slot(|slot| {
                note_handles.push(FIXTURE_TEXTURES.texture_handle(slot.texture_key()));
            });
            let mut actors = Vec::new();
            let mut text = compose::TextLayoutCache::default();
            let mut scratch = compose::ComposeScratch::default();
            for beat in [16.0, 74.0, 122.0, 260.0, 290.0, 340.0] {
                let second = state.gameplay.timing_runtime.timing.get_time_for_beat(beat);
                set_fixture_time(&mut state, second);
                let frame = compose_fixture_frame_with_textures(&mut state, &assets, &metrics,
                    &mut actors, &mut text, &mut scratch, &FIXTURE_TEXTURES);
                assert!(frame.ops.iter().chain(frame.render_targets.iter().flat_map(|target| &target.ops))
                    .any(|op| matches!(op, DrawOp::Sprite(run)
                    if note_handles.contains(&run.texture_handle))), "Sharkmode notes/receptors at beat {beat}");
            }
        });
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires a local Vulkan device and screenshot readback"]
    fn sharkmode_compiled_notes_visible_vulkan_pixels() {
        let fixture = song_fixture::from_source("280-MODS-[MASTER] Sharkmode/Sharkmode.ssc");
        with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
            true, false, || {
            let metrics = space::Metrics::centered(854.0, 480.0);
            space::set_current_metrics(metrics);
            space::set_current_window_px(1600, 900);
            let mut state = build_test_state_with_renderer(&fixture.simfile, GameplayViewport::design(),
                GameplaySession::default(), std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"), ..Default::default()
                }), "Vulkan (wgpu)");
            assert!(state.song_lua.is_none(), "Vulkan must render compiled Lua");
            state.presentation.background_texture_key = deadlib_assets::media_path_key(
                state.song().background_path.as_deref().expect("Sharkmode background"));
            set_fixture_time(&mut state, 0.0);
            let mut gpu = SongLuaNotePixelCapture::new(&state, &metrics,
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/sharkmode-pixels"));
            for beat in [16.0, 74.0, 122.0, 290.0] {
                let second = state.gameplay.timing_runtime.timing.get_time_for_beat(beat);
                set_fixture_time(&mut state, second);
                assert!(gpu.capture_named(&mut state, &metrics, beat, "sharkmode") > 100,
                    "Sharkmode tap notes must reach the framebuffer at beat {beat}");
            }
        });
    }

    #[test]
    fn gameplay_stage_survives_replacing_presentation() {
        with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
            true, false, || {
            let simfile = write_fixture("presentation-swap", "+#TITLE:Presentation swap;
#MUSIC:music.wav;
#BPMS:0=120;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Easy;
#METER:1;
#NOTES:
1000
0000
0010
0000
;
");
            space::set_current_metrics(space::Metrics::centered(854.0, 480.0));
            space::set_current_window_px(1280, 720);
            let mut state = build_test_state(
                &simfile,
                GameplayViewport::design(),
                GameplaySession::default(),
                std::array::from_fn(|_| profile_data::Profile::default()),
            );
            set_fixture_time(&mut state, 0.25);
            let time = state.stage.current_music_time_ns();
            let chart = std::ptr::from_ref(state.stage.charts()[0].as_ref());
            let scratch = std::ptr::from_ref(state.stage.song_frame());
            let life = state.stage.players()[0].life;
            let pack_group = Arc::clone(&state.pack_group);
            let assets = fixture_assets();
            let mut actors = Vec::new();
            let mut render = |state: &mut screen_gameplay::State| {
                actors.clear();
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors, state, &assets, Default::default(), 1.0, Default::default(),
                );
                assert!(segments.segments(state.song_frame(), &actors).count() > 0);
            };
            render(&mut state);
            let mut score_profiles =
                std::array::from_fn(|_| deadsync_score::GameplayScoreboxProfileSnapshot::default());
            score_profiles[0].gs_active = true;
            state.score_refresh = deadsync_profile_gameplay::stage_scores::GameplayScoreRefresh::new(
                score_profiles,
                &[Some(deadsync_score::CachedPlayerLeaderboardData::loading()), None],
                [Some(profile_data::MiniIndicatorScoreType::Itg), None],
            );
            let replacement = screen_gameplay::GameplayPresentation::from_stage(
                &state.stage,
                deadsync_assets::gameplay_stage::GameplayStageAssets {
                    hide_song_intro: false,
                    pack_group: Arc::from("Presentation-only pack"),
                    pack_banner_path: None,
                },
                Arc::from("REPLACEMENT"),
                None,
                None,
                None,
                Default::default(),
            );
            state.presentation = replacement;
            let loading = state.score_refresh.poll(&mut state.stage.gameplay, |_, _| {
                Some(deadsync_score::CachedPlayerLeaderboardData::loading())
            });
            assert!(loading.iter().all(Option::is_none));
            assert!(state.score_refresh.has_pending());
            let updates = state.score_refresh.poll(&mut state.stage.gameplay, |hash, _| {
                assert!(!hash.is_empty());
                let pane = deadsync_score::leaderboard_pane(
                    "GrooveStats",
                    vec![deadsync_score::LeaderboardEntry {
                        rank: 1,
                        name: "Rival".into(),
                        machine_tag: None,
                        score: 9_750.0,
                        date: String::new(),
                        is_rival: true,
                        is_self: false,
                        is_fail: false,
                    }],
                    false,
                ).unwrap();
                Some(deadsync_score::CachedPlayerLeaderboardData::ready(
                    deadsync_score::PlayerLeaderboardData {
                        panes: vec![pane],
                        srpg_self_score: None,
                        itl_self_score: None,
                        itl_self_rank: None,
                    },
                ))
            });
            assert!(updates[0].is_some() && updates[1].is_none());
            assert!(!state.score_refresh.has_pending());
            assert_eq!(state.stage.mini_indicator_rival_score_percent(0), 97.5);
            render(&mut state);
            assert_eq!(&*state.presentation.stage_intro_text, "REPLACEMENT");
            assert_eq!(state.stage.current_music_time_ns(), time);
            assert_eq!(state.stage.players()[0].life, life);
            assert_eq!(state.pack_group, pack_group);
            assert_eq!(std::ptr::from_ref(state.stage.charts()[0].as_ref()), chart);
            assert_eq!(std::ptr::from_ref(state.stage.song_frame()), scratch);
        });
    }

    #[test]
    fn live_lua_screen_frames() {
        with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1,
            true, false, || {
            let simfile = write_fixture("live-screen", r#"
#TITLE:Live screen bridge;
#MUSIC:music.wav;
#BPMS:0=120;
#BGCHANGES:0=background.lua=1=0=0=0=StretchNoLoop====;
#FGCHANGES:0=primary.lua=1=0=0=0=StretchNoLoop====,
0.01=foreground.lua=1=0=0=0=StretchNoLoop====;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Easy;
#METER:1;
#NOTES:
1000
0000
0010
0000
;
"#);
            let metrics = space::Metrics::centered(640.0, 480.0);
            space::set_current_metrics(metrics);
            space::set_current_window_px(640, 480);
            let dir = simfile.parent().expect("fixture directory");
            fs::write(dir.join("background.lua"), r#"
LIVE_TICKS = 0
return Def.ActorFrame { Name = 'Background',
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then LIVE_TICKS = LIVE_TICKS + 1; self:x(80 * LIVE_TICKS) end
        end)
    end,
    -- Background::Update calls InitState before On, resetting Init properties.
    Def.Quad { OnCommand = function(self)
        self:xy(10, 100):zoomto(8, 8):diffuse({0.37, 0.19, 0.63, 1})
    end },
}
"#).expect("background script");
            fs::write(dir.join("primary.lua"), r#"
local stats = STATSMAN:GetCurStageStats():GetPlayerStageStats(PLAYER_1)
local requested = false
return Def.ActorFrame { Name = 'Primary',
    OffCommand = function(self) MESSAGEMAN:Broadcast('LiveOff') end,
    OnCommand = function(self)
        MESSAGEMAN:Broadcast('LiveEntry')
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then
                self:x(10 * stats:GetCurrentPossibleDancePoints())
                self:aux(stats:GetActualDancePoints())
                MESSAGEMAN:Broadcast('LiveFrame')
                if not requested and stats:GetCurrentPossibleDancePoints() > 0 then
                    requested = true
                    SCREENMAN:GetTopScreen():StartTransitioningScreen('SM_DoNextScreen')
                end
            end
        end)
    end,
    Def.Actor { OnCommand = function(self) self:sleep(10) end },
    Def.Quad { InitCommand = function(self)
        MESSAGEMAN:Broadcast('LiveInit')
        self:xy(10, 120):zoomto(8, 8):diffuse({0.81, 0.17, 0.29, 1})
    end },
}
"#).expect("primary script");
            fs::write(dir.join("foreground.lua"), r#"
return Def.ActorFrame { Name = 'Foreground',
    OnCommand = function(self)
        self:SetUpdateFunction(function(self, delta)
            if delta > 0 then self:x(20 * LIVE_TICKS) end
        end)
    end,
    Def.Actor { OnCommand = function(self) self:sleep(10) end },
    Def.Quad { InitCommand = function(self)
        self:xy(10, 140):zoomto(8, 8):diffuse({0.23, 0.71, 0.41, 1})
    end },
}
"#).expect("foreground script");
            let mut state = build_test_state_with_lua_mode(&simfile, GameplayViewport::new(640.0, 480.0),
                GameplaySession::default(), std::array::from_fn(|_| profile_data::Profile::default()),
                "Software", None, true);
            assert!(state.song_lua.is_some(), "runtime mode retains its VM");
            let startup = state.stage.take_output();
            assert!(startup.errors.is_empty(), "{:?}", startup.errors);
            // Foreground::HandleMessage suppresses loading-time On. Its first
            // Update at the authored start beat dispatches On instead.
            assert_eq!(startup.broadcasts.iter().filter(|event| event.1 == "LiveEntry").count(), 0);
            assert_eq!(startup.broadcasts.iter().filter(|event| event.1 == "LiveInit").count(), 1);
            assert!(state.stage.take_output().broadcasts.is_empty());
            state.boundary.total_elapsed_in_screen = 10.0;
            screen_gameplay::update(&mut state, 0.0, deadsync_gameplay::GameplayAudioSnapshot {
                stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                    music_nanos: 0, has_music_mapping: true,
                    valid_at_host_nanos: 1_000_000, ..Default::default()
                }, ..Default::default()
            }, || 1_000_000, &mut Vec::new());
            let activation = state.stage.take_output();
            assert!(activation.errors.is_empty(), "{:?}", activation.errors);
            assert_eq!(activation.broadcasts.iter().filter(|event| event.1 == "LiveEntry").count(), 1,
                "foreground activation at music={}, beat={}, broadcasts={:?}",
                state.current_music_time_seconds(), state.clock.song_position.current_beat, activation.broadcasts);
            assert_eq!(activation.broadcasts.iter().filter(|event| event.1 == "LiveFrame").count(), 0);

            let assets = fixture_assets();
            let mut actors = Vec::new();
            let mut text = compose::TextLayoutCache::default();
            let mut compose_scratch = compose::ComposeScratch::default();
            // Miss promotion follows judged-row processing. A subsequent real
            // frame scores that row; Lua sees the resulting domain progress.
            state.boundary.total_elapsed_in_screen = 10.0;
            let late = deadsync_core::song_time::song_time_ns_to_seconds(state.timing_runtime.step_resolution_distance_ns) + 0.05;
            assert!(late < 0.9, "the first overdue row must precede the second note");
            for (tick, now, progress) in [(1, late, 0), (2, late + 0.05, 5), (3, 1.05 + late, 10)] {
                let nanos = deadsync_core::song_time::song_time_ns_from_seconds(now);
                let snapshot = deadsync_gameplay::GameplayAudioSnapshot {
                    stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                        music_nanos: nanos, has_music_mapping: true,
                        valid_at_host_nanos: 1_000_000, ..Default::default()
                    }, ..Default::default()
                };
                let mut effects = Vec::new();
                screen_gameplay::update(&mut state, 0.1, snapshot, || 1_000_000, &mut effects);
                let output = state.stage.take_output();
                assert!(output.errors.is_empty(), "{:?}", output.errors);
                assert_eq!(output.broadcasts.iter().filter(|event| event.1 == "LiveFrame").count(), 1);
                let transition = (tick == 2).then(|| deadsync_song_lua::SongLuaLiveRequest::Transition("SM_DoNextScreen".into()));
                assert_eq!(output.request, transition, "tick={tick}, progress={}", state.players_runtime.players[0].current_possible_points);
                assert_eq!(output.broadcasts.iter().filter(|event| event.1 == "LiveOff").count(), usize::from(tick == 2));
                assert!(state.stage.take_output().broadcasts.is_empty());
                assert_eq!(state.players_runtime.players[0].current_possible_points, progress,
                    "clock={}, autoplay={}, notes={:?}, counts={:?}", state.current_music_time_seconds(),
                    state.live_autoplay_enabled(), state.chart_runtime.notes.iter().map(|note| note.beat).collect::<Vec<_>>(),
                    state.players_runtime.players[0].judgment_counts);
                assert_eq!(state.players_runtime.players[0].judgment_counts[5], (progress / 5) as u32);
                let root = state.mods.song_lua_visuals.overlays.iter().find(|actor|
                    actor.name.as_deref() == Some("primary.lua")).expect("primary binding");
                assert_eq!(root.initial_state.aux, state.players_runtime.players[0].earned_grade_points as f32);
                let frame = compose_fixture_frame_with_textures(&mut state, &assets, &metrics,
                    &mut actors, &mut text, &mut compose_scratch, &compose::NullTextureContext);
                for (color, x) in [([0.37, 0.19, 0.63, 1.0], 80.0 * tick as f32 + 10.0),
                    ([0.81, 0.17, 0.29, 1.0], 10.0 * progress as f32 + 10.0),
                    ([0.23, 0.71, 0.41, 1.0], 20.0 * tick as f32 + 10.0)] {
                    // Song Lua sprites submit RageColor bytes, while the
                    // fixture declares its identifying tint in Lua floats.
                    let tint = color.map(|channel| f32::from((channel * 256.0) as u8) / 255.0);
                    let markers: Vec<_> = frame.sprite_instances.iter().filter(|instance|
                        (0..4).all(|axis| (instance.tint[axis] - tint[axis]).abs() < 1e-5)).collect();
                    assert_eq!(markers.len(), 1, "live marker {color:?} at {now}");
                    // Renderer instances use the centered world coordinate system.
                    assert!((markers[0].center[0] + 320.0 - x).abs() < 1e-4, "rendered parent pose at {now}: {:?}", markers[0].center);
                }
            }
        });
    }

    #[test]
    fn runtime_song_options_reach_note_effects_and_reset_for_both_players() {
        std::thread::Builder::new()
            .name("runtime-song-options".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                with_session(
                    profile_data::PlayStyle::Versus,
                    profile_data::PlayerSide::P1,
                    true,
                    true,
                    || {
                        let dir = tempfile::tempdir().unwrap();
                        let simfile = dir.path().join("runtime-song-options.ssc");
                        let measures = ["1000\n0000\n0010\n0000"; 12].join("\n,\n");
                        fs::write(&simfile, format!(
                            "#TITLE:Runtime song option bridge;\n#BPMS:0=120;\n\
                            #FGCHANGES:0=default.lua=1=0=0=0=StretchNoLoop====;\n\
                            #NOTEDATA:;\n#STEPSTYPE:dance-single;\n\
                            #DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n{measures}\n;\n"
                        )).unwrap();
                        // Gemini keeps the foreground alive with a separate actor
                        // and writes Song options from a recurring queued command.
                        fs::write(dir.path().join("default.lua"), r#"
RuntimeModTicks = 0
local values = {
    {-0.1, 0.2, 0.4, 1.5, 0.25, 0.5},
    {0.35, 0.6, 0.8, 2.5, -0.1, 1},
}
return Def.ActorFrame {
    Def.Actor { OnCommand = function(self) self:sleep(9e9) end },
    Def.Actor {
        OnCommand = function(self) self:queuecommand('Update') end,
        UpdateCommand = function(self)
            RuntimeModTicks = RuntimeModTicks + 1
            local beat = GAMESTATE:GetSongBeat()
            local active = beat >= 8 and beat < 16
            for player = 1, 2 do
                local pn = 'PlayerNumber_P' .. player
                if GAMESTATE:IsPlayerEnabled(pn)
                    and GAMESTATE:GetCurrentSteps(pn):GetDifficulty() == 'Difficulty_Challenge' then
                    local po = GAMESTATE:GetPlayerState(pn):GetPlayerOptions('ModsLevel_Song')
                    local v = active and values[player] or {0, 0, 0, 1, 0, 0}
                    po:FailSetting('FailType_Off')
                    po:Flip(v[1], 10000)
                    po:Brake(v[2], 10000)
                    po:Stealth(v[3], 10000)
                    po:XMod(v[4], 10000)
                    po:Mini(v[5], 10000)
                    po:Reverse(v[6], 10000)
                end
            end
            self:sleep(1/75):queuecommand('Update')
        end,
    },
}
"#).unwrap();
                        let probe = dir.path().join("assert-current-options.lua");
                        fs::write(&probe, r#"
assert(RuntimeModTicks > 0, 'the authored queued command must execute')
local beat = GAMESTATE:GetSongBeat()
local active = beat >= 8 and beat < 16
local values = {{-0.1, 0.2, 0.4, 1.5, 0.25, 0.5}, {0.35, 0.6, 0.8, 2.5, -0.1, 1}}
local methods = {'Flip', 'Brake', 'Stealth', 'XMod', 'Mini', 'Reverse'}
for player = 1, 2 do
    local po = GAMESTATE:GetPlayerState('PlayerNumber_P' .. player):GetPlayerOptions('ModsLevel_Current')
    local expected = active and values[player] or {0, 0, 0, 1, 0, 0}
    for i, name in ipairs(methods) do
        local actual = po[name](po)
        assert(math.abs(actual - expected[i]) < 0.00001,
            'P' .. player .. ' Current ' .. name .. ': ' .. tostring(actual))
    end
end
"#).unwrap();
                        space::set_current_metrics(space::Metrics::centered(640.0, 480.0));
                        space::set_current_window_px(640, 480);
                        let profiles = std::array::from_fn(|_| profile_data::Profile {
                            scroll_speed: ScrollSpeedSetting::XMod(1.0),
                            ..Default::default()
                        });
                        let session = GameplaySession {
                            play_style: deadsync_gameplay::GameplayInputPlayStyle::Versus,
                            joined_sides: [true, true],
                            ..Default::default()
                        };
                        let mut state = build_test_state_with_lua_mode(
                            &simfile, GameplayViewport::new(640.0, 480.0), session,
                            profiles, "Software", None, true,
                        );
                        assert!(state.song_lua.is_some(), "Runtime mode retains the Lua scene");
                        assert!(state.stage.take_output().errors.is_empty());
                        state.boundary.total_elapsed_in_screen = 10.0;
                        state.gameplay.set_live_autoplay_enabled(true);
                        for tick in 0..=540 {
                            let now = tick as f32 / 60.0;
                            let host_nanos = 1_000_000_000 + tick as u64 * 1_000_000_000 / 60;
                            screen_gameplay::update(&mut state, if tick == 0 { 0.0 } else { 1.0 / 60.0 },
                                deadsync_gameplay::GameplayAudioSnapshot {
                                    stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                                        music_nanos: deadsync_core::song_time::song_time_ns_from_seconds(now),
                                        has_music_mapping: true, valid_at_host_nanos: host_nanos,
                                        ..Default::default()
                                    }, ..Default::default()
                                }, || host_nanos, &mut Vec::new());
                            let output = state.stage.take_output();
                            assert!(output.errors.is_empty(), "tick {tick}: {:?}", output.errors);
                            if ![30, 300, 540].contains(&tick) { continue; }
                            // Verify the VM independently before checking the state
                            // actually consumed by normal notefield composition.
                            state.song_lua.as_mut().unwrap().runtime_mut()
                                .execute_file(&probe, &[]).unwrap();
                            let active = tick == 300;
                            for (player, targets) in [
                                [-0.1, 0.2, 0.4, 1.5, 0.25, 0.5],
                                [0.35, 0.6, 0.8, 2.5, -0.1, 1.0],
                            ].into_iter().enumerate() {
                                let expected = if active { targets } else { [0.0, 0.0, 0.0, 1.0, 0.0, 0.0] };
                                let visual = deadsync_gameplay::effective_visual_effects_for_player(&state.gameplay, player);
                                let accel = state.gameplay.effective_accel_effects_for_player_with_mask(player, 0);
                                let appearance = state.gameplay.effective_appearance_effects_for_player(player);
                                let mini = deadsync_gameplay::effective_mini_percent_for_player(&state.gameplay, player) / 100.0;
                                let reverse = deadsync_gameplay::effective_scroll_effects_for_player(&state.gameplay, player).reverse;
                                for (name, actual, expected) in [
                                    ("Flip", visual.flip, expected[0]),
                                    ("Brake", accel.brake, expected[1]),
                                    ("Stealth", appearance.stealth, expected[2]),
                                    ("Mini", mini, expected[4]),
                                    ("Reverse", reverse, expected[5]),
                                ] {
                                    assert!((actual - expected).abs() < 0.00001,
                                        "tick {tick}, P{}, {name}: gameplay {actual}, Current {expected}", player + 1);
                                }
                                assert_eq!(state.gameplay.effective_scroll_speed_for_player_with_base(
                                    player, ScrollSpeedSetting::XMod(1.0)), ScrollSpeedSetting::XMod(expected[3]),
                                    "tick {tick}, P{} XMod", player + 1);
                            }
                        }
                    },
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn hidden_gameplay_intro_keeps_transition_timing() {
        let simfile = write_fixture("hidden-intro", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let (mut state, assets, _) = sprite_core_fixture(&simfile);
                let policy =
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default();
                for restart in [false, true] {
                    state.presentation.hide_song_intro = false;
                    let (visible, duration) =
                        screen_gameplay::in_transition(Some(&state), &assets, restart, policy);
                    assert!(!visible.is_empty());
                    state.presentation.hide_song_intro = true;
                    let (hidden, hidden_duration) =
                        screen_gameplay::in_transition(Some(&state), &assets, restart, policy);
                    assert!(hidden.is_empty());
                    assert!(duration > 0.0);
                    assert_eq!(hidden_duration, duration);
                }
            },
        );
    }

    #[test]
    fn ordinary_song_modes_and_views_match_existing_composition() {
        use deadsync_gameplay::{GameplayInputPlayStyle as G, GameplayInputPlayerSide as S};
        use profile_data::PlayStyle as P;
        for (style, runtime_style) in [
            (P::Single, G::Single),
            (P::Double, G::Double),
            (P::Versus, G::Versus),
            (P::PumpSingle, G::PumpSingle),
            (P::PumpDouble, G::PumpDouble),
            (P::PumpVersus, G::PumpVersus),
        ] {
            let cols = style.cols_per_player();
            let row = format!("1{}", "0".repeat(cols - 1));
            let notes = vec![row; 16].join("\n");
            let text = format!(
                "#TITLE:Native modes;\n#BPMS:0=120;\n#FGCHANGES:0=foreground.png;\n\
                #NOTEDATA:;\n#STEPSTYPE:{};\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:{notes};",
                style.chart_type()
            );
            let simfile = write_fixture(&format!("native-mode-{runtime_style:?}"), &text);
            image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255]))
                .save(simfile.parent().unwrap().join("foreground.png"))
                .unwrap();
            for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                let versus = style.player_count() == 2;
                with_session(
                    style,
                    side,
                    versus || side == profile_data::PlayerSide::P1,
                    versus || side == profile_data::PlayerSide::P2,
                    || {
                        let metrics = space::Metrics::centered(854.0, 480.0);
                        space::set_current_metrics(metrics);
                        space::set_current_window_px(1280, 720);
                        let profiles = std::array::from_fn(|p| profile_data::Profile {
                            noteskin: profile_data::NoteSkin::new("lambda"),
                            hide_song_bg: p == 0,
                            ..Default::default()
                        });
                        let session = GameplaySession {
                            play_style: runtime_style,
                            player_side: if side == profile_data::PlayerSide::P2 {
                                S::P2
                            } else {
                                S::P1
                            },
                            ..Default::default()
                        };
                        let mut state = build_test_state(
                            &simfile,
                            GameplayViewport::new(1280.0, 720.0),
                            session,
                            profiles,
                        );
                        set_fixture_time(&mut state, 1.0);
                        assert!(
                            state.stage.refresh_song_foreground(),
                            "native foreground media must actually activate"
                        );
                        assert!(!state.song_frame().uses_song_lua_compositor());
                        let assets = fixture_assets();
                        let mut actors = Vec::new();
                        let mut text_cache = compose::TextLayoutCache::default();
                        let mut scratch = compose::ComposeScratch::default();
                        for (hide_gameplay_hud, show_song_visuals, apply_attacks) in [
                            (false, true, true),
                            (true, true, true),
                            (false, false, false),
                            (true, false, false),
                        ] {
                            let view = FixtureFrameView {
                                presentation: screen_gameplay::ActorViewOverride {
                                    hide_gameplay_hud,
                                    ..Default::default()
                                },
                                preview: Some(SongFrameContent::preview(
                                    apply_attacks,
                                    show_song_visuals,
                                )),
                            };
                            let mut expected = compose_fixture_frame_with_view(
                                &mut state,
                                &assets,
                                &metrics,
                                &mut actors,
                                &mut text_cache,
                                &mut scratch,
                                view,
                                &FIXTURE_TEXTURES,
                            );
                            let native = state.song_scratch.take().unwrap();
                            let mut full =
                                deadsync_song_lua::playback::FrameScratch::new(&state.gameplay);
                            full.enable_dynamic_scene();
                            state.song_scratch = Some(Box::new(full));
                            let mut actual = compose_fixture_frame_with_view(
                                &mut state,
                                &assets,
                                &metrics,
                                &mut actors,
                                &mut text_cache,
                                &mut scratch,
                                view,
                                &FIXTURE_TEXTURES,
                            );
                            assert_eq!(
                                compare_render_frames(&expected, &actual),
                                Ok(()),
                                "{style:?}/{side:?}, hidden HUD={hide_gameplay_hud}, visuals={show_song_visuals}"
                            );
                            scratch.recycle_frame(&mut expected);
                            scratch.recycle_frame(&mut actual);
                            state.song_scratch = Some(native);
                        }
                    },
                );
            }
        }
    }

    #[test]
    fn ordinary_song_keeps_lua_runtime_and_input_idle() {
        let simfile = write_fixture("no-song-lua", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let (mut state, _, _) = sprite_core_fixture(&simfile);
                assert!(state.song_lua.is_none());
                assert!(!state.song_frame().uses_song_lua_compositor());
                assert!(state.note_field_tap_callback.is_none());
                assert!(state.step_callback.is_none());
                assert!(state.tap_row_judgment_callback.is_none());
                assert!(state.hold_judgment_callback.is_none());
                assert!(state.avoided_mine_judgment_callback.is_none());
                state.stage.song_lua.queue_inputs(std::iter::from_fn(|| {
                    panic!("ordinary gameplay must not consume Lua input events")
                }));
                let mut input_phases = 0;
                let mut effects = Vec::new();
                for tick in 0..60 {
                    screen_gameplay::update_with_input(
                        &mut state,
                        1.0 / 60.0,
                        deadsync_gameplay::GameplayAudioSnapshot {
                            stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                                music_nanos: 1_000_000_000 + tick * 16_666_667,
                                has_music_mapping: true,
                                valid_at_host_nanos: 1_000_000,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        || 1_000_000,
                        &mut effects,
                        |_| input_phases += 1,
                    );
                    let output = state.stage.take_output();
                    assert!(
                        output.errors.is_empty()
                            && output.sounds.is_empty()
                            && output.actor_sounds.is_empty()
                            && output.broadcasts.is_empty()
                            && output.draw_frames.is_empty()
                            && output.request.is_none()
                    );
                }
                assert_eq!(
                    input_phases, 60,
                    "native input still runs once after every update"
                );
                assert!(state.song_lua.is_none());
            },
        );
    }

    #[test]
    fn sprite_core_frame_is_structurally_repeatable() {
        let simfile = write_fixture("f0-sprite-core", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let (mut state, assets, metrics) = sprite_core_fixture(&simfile);

                assert!(
                    state
                        .chart_runtime
                        .notes
                        .iter()
                        .any(|note| note.note_type == NoteType::Tap)
                );
                assert!(
                    state
                        .chart_runtime
                        .notes
                        .iter()
                        .any(|note| note.note_type == NoteType::Lift)
                );
                assert!(
                    state
                        .chart_runtime
                        .notes
                        .iter()
                        .any(|note| note.note_type == NoteType::Fake || note.is_fake)
                );

                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let expected = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                // Batch merging can reduce draw operations without removing
                // visible sprites; coverage belongs to the instance payload.
                assert!(expected.sprite_instances.len() >= 240);
                let player = &mut state.players_runtime.players[0];
                player.combo = 0;
                player.last_judgment = None;
                state.display.visual_feedback.clear();
                state.display.visual_feedback.last_tap_judgments.fill(None);
                let mut without_feedback = compose_fixture_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert!(
                    expected.sprite_instances.len() > without_feedback.sprite_instances.len()
                        || expected.ops.len() > without_feedback.ops.len()
                );
                compose_scratch.recycle_frame(&mut without_feedback);

                state.profiles_runtime.profiles[0]
                    .set_scroll_option(profile_data::ScrollOption::Reverse);
                state.refresh_live_notefield_options(120.0);
                add_sprite_core_feedback(&mut state, 0, 0, 42);
                let reverse = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert_ne!(compare_render_frames(&expected, &reverse), Ok(()));
            },
        );
    }

    #[cfg(target_os = "windows")]
    #[derive(Debug)]
    struct GpuCaptureArtifacts {
        normal_path: PathBuf,
        reverse_path: PathBuf,
        manifest_path: PathBuf,
    }

    #[cfg(target_os = "windows")]
    struct GpuCaptureApp {
        backend_type: deadlib_render_core::BackendType,
        simfile: PathBuf,
        output_dir: PathBuf,
        result: Option<Result<GpuCaptureArtifacts, String>>,
    }

    #[cfg(target_os = "windows")]
    impl winit::application::ApplicationHandler for GpuCaptureApp {
        fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
            if self.result.is_some() {
                return;
            }
            self.result = Some(capture_sprite_core_gpu(
                event_loop,
                self.backend_type,
                &self.simfile,
                &self.output_dir,
            ));
            event_loop.exit();
        }

        fn window_event(
            &mut self,
            event_loop: &winit::event_loop::ActiveEventLoop,
            _window_id: winit::window::WindowId,
            event: winit::event::WindowEvent,
        ) {
            if matches!(event, winit::event::WindowEvent::CloseRequested) {
                event_loop.exit();
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[derive(Clone, Copy)]
    struct GpuFrameStats {
        hash: u64,
        ops: usize,
        instances: usize,
        geometries: usize,
        noteskin_sprites: usize,
    }


    #[cfg(target_os = "windows")]
    struct CaptureTextureContext<'a> {
        assets: &'a deadlib_assets::AssetManager,
        keys: std::cell::RefCell<std::collections::HashSet<String>>,
    }

    #[cfg(target_os = "windows")]
    impl TextureContext for CaptureTextureContext<'_> {
        fn texture_registry_generation(&self) -> u64 {
            self.assets.texture_context().revision()
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            self.assets.texture_context().texture_dims(key)
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            self.assets.texture_context().sprite_sheet_dims(key)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render_core::TextureHandle {
            self.keys.borrow_mut().insert(key.to_owned());
            self.assets.texture_context().texture_handle(key)
        }
    }

    #[cfg(target_os = "windows")]
    fn load_gpu_capture_assets(
        assets: &mut deadlib_assets::AssetManager,
        backend: &mut deadlib_render::Backend,
    ) -> Result<(), String> {
        let texture_assets = deadsync_theme_simply_love::asset_manifest()
            .textures
            .collect::<Vec<_>>();
        deadsync_assets::load_initial_assets(
            assets,
            backend,
            deadsync_theme::ThemeAssetManifest {
                fonts: &[],
                textures: texture_assets.iter().copied(),
                embedded_pngs: &[],
                texture_needs_repeat_sampler: (deadsync_theme_simply_love::asset_manifest()
                    .texture_needs_repeat_sampler),
            },
        )
        .map_err(|error| format!("failed to load capture textures: {error}"))?;

        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let asset_root = project_root.join("assets");
        let asset_roots = [asset_root.clone()];
        for spec in deadsync_theme_simply_love::asset_manifest().fonts {
            let ini_path = project_root.join(spec.ini_path);
            let deadlib_present::font::FontLoadData {
                mut font,
                required_textures,
            } = deadlib_assets::parse_font_with_asset_context(&ini_path, vec![asset_root.clone()])
                .map_err(|error| {
                    format!(
                        "failed to parse capture font '{}' at '{}': {error}",
                        spec.name,
                        ini_path.display()
                    )
                })?;
            deadlib_assets::set_font_fallback(&mut font, spec.fallback_font_name);
            let textures = deadlib_assets::prepare_required_font_textures(
                &font,
                &required_textures,
                &asset_roots,
                |key| assets.has_texture_key(key),
            )
            .map_err(|error| format!("failed to decode capture font '{}': {error}", spec.name))?;
            for deadlib_assets::PreparedFontTexture { key, image, hints } in textures {
                let texture = backend
                    .create_texture(&image, hints.sampler_desc())
                    .map_err(|error| {
                        format!("failed to upload capture font texture '{key}': {error}")
                    })?;
                deadlib_assets::register_texture_dims(&key, image.width(), image.height());
                if assets
                    .insert_texture(key.clone(), texture, image.width(), image.height())
                    .is_some()
                {
                    return Err(format!(
                        "capture font texture '{key}' replaced live storage"
                    ));
                }
            }
            assets.register_font(spec.name, font);
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn ensure_gpu_capture_texture(
        assets: &mut deadlib_assets::AssetManager,
        backend: &mut deadlib_render::Backend,
        key: &str,
        sampler: Option<deadlib_render_core::SamplerDesc>,
    ) -> Result<(), String> {
        if let Some(sampler) = sampler {
            deadsync_assets::textures::ensure_texture_for_key_with_sampler(
                assets, backend, key, sampler,
            );
        } else {
            deadsync_assets::textures::ensure_texture_for_key(
                assets,
                backend,
                key,
                deadsync_theme_simply_love::asset_manifest().texture_needs_repeat_sampler,
            );
        }
        if assets.has_uploaded_texture_key(key) {
            return Ok(());
        }

        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = deadsync_assets::textures::texture_key_source_path(key, key, |path| {
            project_root.join(path)
        });
        if !path.is_file() {
            return Err(format!(
                "capture texture '{key}' was not found at '{}'",
                path.display()
            ));
        }
        let hints = deadlib_assets::parse_texture_hints(key);
        let image = deadlib_assets::decode_texture_image(&path, &hints).map_err(|error| {
            format!(
                "failed to decode capture texture '{key}' at '{}': {error}",
                path.display()
            )
        })?;
        let sampler = sampler.unwrap_or_else(|| {
            deadlib_assets::texture_key_sampler(
                &hints,
                (deadsync_theme_simply_love::asset_manifest().texture_needs_repeat_sampler)(key),
            )
        });
        let texture = backend
            .create_texture(&image, sampler)
            .map_err(|error| format!("failed to upload capture texture '{key}': {error}"))?;
        assets.set_texture_for_key(
            backend,
            key.to_owned(),
            texture,
            image.width(),
            image.height(),
        );
        deadlib_assets::register_texture_dims(key, image.width(), image.height());
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn prewarm_gpu_noteskins(
        state: &screen_gameplay::State,
        assets: &mut deadlib_assets::AssetManager,
        backend: &mut deadlib_render::Backend,
    ) -> Result<(), String> {
        let mut seen = std::collections::HashSet::<String>::with_capacity(128);
        let mut seen_models = std::collections::HashSet::<String>::with_capacity(32);
        let sets = [
            &state.noteskin_assets.noteskin,
            &state.noteskin_assets.mine_noteskin,
            &state.noteskin_assets.receptor_noteskin,
            &state.noteskin_assets.tap_explosion_noteskin,
        ];
        for noteskin in sets.into_iter().flat_map(|set| set.iter().flatten()) {
            let mut error = None;
            noteskin.for_each_slot(|slot| {
                if error.is_none()
                    && seen.insert(slot.texture_key().to_owned())
                    && let Err(cause) =
                        ensure_gpu_capture_texture(assets, backend, slot.texture_key(), None)
                {
                    error = Some(cause);
                }
            });
            if let Some(error) = error {
                return Err(error);
            }

            let mut error = None;
            noteskin.for_each_slot(|slot| {
                if error.is_none()
                    && slot.model.is_some()
                    && seen_models.insert(slot.texture_key().to_owned())
                    && let Err(cause) = ensure_gpu_capture_texture(
                        assets,
                        backend,
                        slot.texture_key(),
                        Some(deadsync_assets::textures::model_texture_sampler(
                            slot.texture_key(),
                        )),
                    )
                {
                    error = Some(cause);
                }
            });
            if let Some(error) = error {
                return Err(error);
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn gpu_frame_hash(image: &image::RgbaImage) -> u64 {
        image
            .as_raw()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }


    #[cfg(target_os = "windows")]
    fn missing_capture_texture_keys(
        texture_ctx: &CaptureTextureContext,
        assets: &deadlib_assets::AssetManager,
    ) -> Vec<String> {
        texture_ctx
            .keys
            .borrow()
            .iter()
            .filter(|key| {
                let handle = assets.texture_context().texture_handle(key);
                !assets.textures().contains_key(&handle)
            })
            .cloned()
            .collect()
    }

    #[cfg(target_os = "windows")]
    fn missing_capture_texture_report(
        missing_keys: &[String],
        assets: &deadlib_assets::AssetManager,
    ) -> String {
        missing_keys
            .iter()
            .map(|key| {
                let canonical = deadsync_assets::textures::canonical_texture_key(key);
                let handle = assets.texture_context().texture_handle(key);
                let canonical_handle = assets.texture_context().texture_handle(&canonical);
                format!(
                    "{key} [canonical={canonical}, local={}, uploaded={}, handle={handle}, canonical_handle={canonical_handle}]",
                    assets.has_texture_key(key),
                    assets.has_uploaded_texture_key(key),
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    #[cfg(target_os = "windows")]
    #[expect(
        clippy::too_many_arguments,
        reason = "the capture owns one explicit value for each warmed presentation resource"
    )]
    fn capture_gpu_frame(
        name: &str,
        state: &mut screen_gameplay::State,
        assets: &mut deadlib_assets::AssetManager,
        backend: &mut deadlib_render::Backend,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
        output_dir: &Path,
    ) -> Result<(PathBuf, GpuFrameStats), String> {
        let texture_ctx = CaptureTextureContext {
            assets,
            keys: Default::default(),
        };
        let mut render = compose_fixture_frame_with_textures(
            state,
            assets,
            metrics,
            actors,
            text_cache,
            compose_scratch,
            &texture_ctx,
        );
        let missing_keys = missing_capture_texture_keys(&texture_ctx, assets);
        if !missing_keys.is_empty() {
            compose_scratch.recycle_frame(&mut render);
            drop(texture_ctx);
            for key in &missing_keys {
                ensure_gpu_capture_texture(assets, backend, key, None)?;
            }
            let texture_ctx = CaptureTextureContext {
                assets,
                keys: Default::default(),
            };
            render = compose_fixture_frame_with_textures(
                state,
                assets,
                metrics,
                actors,
                text_cache,
                compose_scratch,
                &texture_ctx,
            );
            let mut missing_keys = missing_capture_texture_keys(&texture_ctx, assets);
            if !missing_keys.is_empty() {
                missing_keys.sort_unstable();
                return Err(format!(
                    "{name} frame has nonresident textures: {}",
                    missing_capture_texture_report(&missing_keys, assets)
                ));
            }
        }
        let mut noteskin_handles = std::collections::HashSet::with_capacity(128);
        for noteskin in [
            &state.noteskin_assets.noteskin,
            &state.noteskin_assets.mine_noteskin,
            &state.noteskin_assets.receptor_noteskin,
            &state.noteskin_assets.tap_explosion_noteskin,
        ]
        .into_iter()
        .flat_map(|set| set.iter().flatten())
        {
            noteskin.for_each_slot(|slot| {
                let handle = assets.texture_context().texture_handle(slot.texture_key());
                if handle != deadlib_render_core::INVALID_TEXTURE_HANDLE
                    && assets.textures().contains_key(&handle)
                {
                    noteskin_handles.insert(handle);
                }
            });
        }
        let noteskin_sprites = render
            .ops
            .iter()
            .filter_map(|op| {
                let DrawOp::Sprite(run) = op else {
                    return None;
                };
                noteskin_handles
                    .contains(&run.texture_handle)
                    .then_some(run.instance_count as usize)
            })
            .sum();
        if noteskin_sprites < 200 {
            compose_scratch.recycle_frame(&mut render);
            return Err(format!(
                "{name} frame has only {noteskin_sprites} resident noteskin sprites"
            ));
        }

        backend.request_screenshot();
        backend
            .draw(&render, assets.textures(), false)
            .map_err(|error| format!("{name} draw failed: {error}"))?;
        let mut image = backend
            .capture_frame()
            .map_err(|error| format!("{name} readback failed: {error}"))?;
        deadsync_assets::screenshot::set_opaque_alpha(&mut image);
        if image.dimensions() != (640, 480) {
            return Err(format!(
                "{name} readback was {}x{}, expected 640x480",
                image.width(),
                image.height()
            ));
        }
        if !image
            .pixels()
            .any(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
        {
            return Err(format!("{name} readback is entirely black"));
        }

        let path = output_dir.join(format!("sprite-core-{name}.png"));
        image
            .save(&path)
            .map_err(|error| format!("failed to save '{}': {error}", path.display()))?;
        let stats = GpuFrameStats {
            hash: gpu_frame_hash(&image),
            ops: render.ops.len(),
            instances: render.sprite_instances.len(),
            geometries: render.tmesh_geometries.len(),
            noteskin_sprites,
        };
        compose_scratch.recycle_frame(&mut render);
        Ok((path, stats))
    }

    #[cfg(target_os = "windows")]
    fn capture_sprite_core_gpu(
        event_loop: &winit::event_loop::ActiveEventLoop,
        backend_type: deadlib_render_core::BackendType,
        simfile: &Path,
        output_dir: &Path,
    ) -> Result<GpuCaptureArtifacts, String> {
        let attributes = winit::window::Window::default_attributes()
            .with_title("DeadSync F0 GPU capture")
            .with_visible(false)
            .with_resizable(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(640, 480));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("failed to create capture window: {error}"))?,
        );
        let size = window.inner_size();
        if size.width != 640 || size.height != 480 {
            return Err(format!(
                "capture window was {}x{}, expected 640x480",
                size.width, size.height
            ));
        }
        let mut backend = deadlib_render::create_backend(
            backend_type,
            Arc::clone(&window),
            space::Metrics::centered(640.0, 480.0).projection(),
            false,
            deadlib_render_core::PresentModePolicy::Immediate,
            false,
            true,
        )
        .map_err(|error| format!("failed to create {backend_type} backend: {error}"))?;
        let mut assets = deadlib_assets::AssetManager::new();
        let result = (|| {
            load_gpu_capture_assets(&mut assets, &mut backend)?;
            let (mut state, _, metrics) = sprite_core_fixture(simfile);
            prewarm_gpu_noteskins(&state, &mut assets, &mut backend)?;
            std::fs::create_dir_all(output_dir).map_err(|error| {
                format!(
                    "failed to create capture directory '{}': {error}",
                    output_dir.display()
                )
            })?;

            let mut actors = Vec::with_capacity(512);
            let mut text_cache = compose::TextLayoutCache::default();
            let mut compose_scratch = compose::ComposeScratch::default();
            let (normal_path, normal) = capture_gpu_frame(
                "normal",
                &mut state,
                &mut assets,
                &mut backend,
                &metrics,
                &mut actors,
                &mut text_cache,
                &mut compose_scratch,
                output_dir,
            )?;
            state.profiles_runtime.profiles[0]
                .set_scroll_option(profile_data::ScrollOption::Reverse);
            state.refresh_live_notefield_options(120.0);
            add_sprite_core_feedback(&mut state, 0, 0, 42);
            let (reverse_path, reverse) = capture_gpu_frame(
                "reverse",
                &mut state,
                &mut assets,
                &mut backend,
                &metrics,
                &mut actors,
                &mut text_cache,
                &mut compose_scratch,
                output_dir,
            )?;
            if normal.hash == reverse.hash {
                return Err("normal and reverse GPU captures have the same pixel hash".to_owned());
            }

            let manifest_path = output_dir.join("manifest.txt");
            let profile = if cfg!(debug_assertions) {
                "debug"
            } else {
                "optimized"
            };
            let manifest = format!(
                "fixture=F0-sprite-core\nbackend={backend_type}\nprofile={profile}\n\
                 resolution=640x480\nmusic_time_seconds=2.5\n\
                 normal_hash={:016x}\nnormal_ops={}\nnormal_instances={}\nnormal_geometries={}\n\
                 normal_noteskin_sprites={}\n\
                 reverse_hash={:016x}\nreverse_ops={}\nreverse_instances={}\nreverse_geometries={}\n\
                 reverse_noteskin_sprites={}\n",
                normal.hash,
                normal.ops,
                normal.instances,
                normal.geometries,
                normal.noteskin_sprites,
                reverse.hash,
                reverse.ops,
                reverse.instances,
                reverse.geometries,
                reverse.noteskin_sprites,
            );
            std::fs::write(&manifest_path, manifest).map_err(|error| {
                format!("failed to write '{}': {error}", manifest_path.display())
            })?;
            Ok(GpuCaptureArtifacts {
                normal_path,
                reverse_path,
                manifest_path,
            })
        })();

        let mut textures = assets.take_textures();
        backend.dispose_textures(&mut textures);
        backend.cleanup();
        result
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires an explicitly selected real GPU backend and writes PNG artifacts"]
    fn sprite_core_gpu_capture_writes_baseline() {
        use winit::platform::windows::EventLoopBuilderExtWindows;

        let backend_name = std::env::var("DEADSYNC_F0_BACKEND")
            .expect("set DEADSYNC_F0_BACKEND to opengl, vulkan, or vulkan-wgpu");
        let backend_type = backend_name
            .parse::<deadlib_render_core::BackendType>()
            .unwrap_or_else(|error| panic!("invalid DEADSYNC_F0_BACKEND: {error}"));
        assert_ne!(backend_type, deadlib_render_core::BackendType::Software);
        let slug = backend_name
            .trim()
            .to_ascii_lowercase()
            .replace(|ch: char| !ch.is_ascii_alphanumeric(), "-");
        let output_root = std::env::var_os("DEADSYNC_F0_CAPTURE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/f0-gpu-captures")
            });
        let simfile = write_fixture("f0-sprite-core-gpu", generated_sprite_core_simfile());

        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let mut builder = winit::event_loop::EventLoop::builder();
                builder.with_any_thread(true);
                let event_loop = builder
                    .build()
                    .expect("capture event loop should initialize");
                let mut app = GpuCaptureApp {
                    backend_type,
                    simfile,
                    output_dir: output_root.join(slug),
                    result: None,
                };
                event_loop
                    .run_app(&mut app)
                    .expect("capture event loop should run");
                let artifacts = app
                    .result
                    .expect("capture app should run once")
                    .unwrap_or_else(|error| panic!("F0 GPU capture failed: {error}"));
                println!(
                    "F0 GPU captures: normal='{}' reverse='{}' manifest='{}'",
                    artifacts.normal_path.display(),
                    artifacts.reverse_path.display(),
                    artifacts.manifest_path.display()
                );
            },
        );
    }

    #[test]
    fn sprite_core_warmed_pipeline_reuses_storage() {
        const WARMUP_FRAMES: usize = 64;
        const VERIFY_FRAMES: usize = 2;

        let simfile = write_fixture("f0-sprite-core-storage", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let (mut state, assets, metrics) = sprite_core_fixture(&simfile);
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut checksum = 0usize;

                for _ in 0..WARMUP_FRAMES {
                    checksum = checksum.wrapping_add(prepare_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut compose_scratch,
                    ));
                }
                let capacity_before = (actors.capacity(), compose_scratch.storage_stats());

                for _ in 0..VERIFY_FRAMES {
                    checksum = checksum.wrapping_add(prepare_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut compose_scratch,
                    ));
                }
                let capacity_after = (actors.capacity(), compose_scratch.storage_stats());

                assert_ne!(std::hint::black_box(checksum), 0);
                assert_eq!(capacity_after, capacity_before);

            },
        );
    }

    #[test]
    fn sprite_core_skips_unprofitable_sprite_gather() {
        let simfile = write_fixture("f0-sprite-core-gather", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let (mut state, assets, metrics) = sprite_core_fixture(&simfile);
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();

                compose_scratch.begin_frame_stats(true);
                let checksum = prepare_fixture_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                let stats = compose_scratch.frame_stats();
                assert_ne!(checksum, 0);
                assert!(stats.sort_fallback);
                assert_eq!(stats.sprite_gathered, 0);
                assert_eq!(stats.sprite_runs_after, stats.sprite_runs_before);
                println!(
                    "F0 sprite gather: fallback={} sprites={} runs={} -> {}",
                    stats.sort_fallback,
                    stats.sprite_gathered,
                    stats.sprite_runs_before,
                    stats.sprite_runs_after,
                );
            },
        );
    }

    fn generated_hold_mine_simfile() -> &'static str {
        r#"#VERSION:0.83;
#TITLE:F0 Hold Mine;
#MUSIC:;
#OFFSET:0.000;
#BPMS:0.000=120.000;

#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:F0-hold-mine;
#DIFFICULTY:Challenge;
#METER:10;
#RADARVALUES:0,0,0,0,0;
#NOTES:
2000
0400
0020
000M
,
0000
000M
0000
0030
,
3000
0300
M000
0000
;
"#
    }

    fn add_hold_mine_state(state: &mut screen_gameplay::State) -> [usize; 3] {
        let note_index = |column, note_type| {
            state
                .chart_runtime
                .notes
                .iter()
                .position(|note| note.column == column && note.note_type == note_type)
                .expect("hold/mine fixture note should exist")
        };
        let hold_index = note_index(0, NoteType::Hold);
        let roll_index = note_index(1, NoteType::Roll);
        let dropped_index = note_index(2, NoteType::Hold);
        let current_time_ns = deadsync_core::song_time::song_time_ns_from_seconds(2.5);
        let active_hold = |note_index, life, is_pressed| {
            let note = &state.chart_runtime.notes[note_index];
            deadsync_gameplay::ActiveHold {
                note_index,
                start_time_ns: state.chart_runtime.note_time_cache_ns[note_index],
                end_time_ns: cached_hold_end_time_ns(
                    state.chart_runtime.hold_end_time_cache_ns[note_index],
                )
                .expect("fixture hold should have a cached tail time"),
                note_type: note.note_type,
                let_go: false,
                is_pressed,
                life,
                last_update_time_ns: current_time_ns,
            }
        };
        let live_hold = active_hold(hold_index, 1.0, true);
        let live_roll = active_hold(roll_index, 0.65, false);
        state.set_active_hold(0, Some(live_hold));
        state.set_active_hold(1, Some(live_roll));

        let let_go_time_ns = deadsync_core::song_time::song_time_ns_from_seconds(2.35);
        state.handle_hold_let_go(2, dropped_index, let_go_time_ns);
        let last_held_beat = 4.5;
        let last_held_row = state
            .timing_runtime
            .timing
            .get_row_for_beat(last_held_beat)
            .expect("fixture beat should map to a chart row");
        let dropped = state.chart_runtime.notes[dropped_index]
            .hold
            .as_mut()
            .expect("fixture dropped hold should have hold data");
        dropped.life = 0.35;
        dropped.last_held_beat = last_held_beat;
        dropped.last_held_row_index = last_held_row;

        state.trigger_receptor_step_pulse(0);
        state.trigger_mine_explosion(3);
        let judgment = Judgment {
            time_error_ms: 23.0,
            time_error_music_ns: 23_000_000,
            grade: JudgeGrade::Excellent,
            window: Some(TimingWindow::W2),
            miss_because_held: false,
        };
        state.error_bar_register_tap(0, &judgment, 2.5);
        [hold_index, roll_index, dropped_index]
    }

    #[test]
    fn completed_results_preserve_sides_stats_and_evaluation_view() {
        use deadsync_gameplay::{GameplayInputPlayStyle, GameplayInputPlayerSide};
        use deadsync_profile_gameplay::results::stage_result;
        use deadsync_score::{Grade, stage_stats::all_players_failed};
        use deadsync_theme_simply_love::{screens::evaluation, views::EvaluationContextView};
        let simfile = write_fixture("completed-results", generated_hold_mine_simfile());
        for (style, side, joined) in [
            (
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                [true, false],
            ),
            (
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P2,
                [false, true],
            ),
            (
                profile_data::PlayStyle::Versus,
                profile_data::PlayerSide::P1,
                [true, true],
            ),
        ] {
            with_session(style, side, joined[0], joined[1], || {
                let profiles = std::array::from_fn(|idx| profile_data::Profile {
                    display_name: format!("Player {idx}"),
                    show_fa_plus_window: true,
                    noteskin: profile_data::NoteSkin::new("lambda"),
                    ..Default::default()
                });
                let session = GameplaySession {
                    play_style: if style == profile_data::PlayStyle::Versus {
                        GameplayInputPlayStyle::Versus
                    } else {
                        GameplayInputPlayStyle::Single
                    },
                    player_side: if side == profile_data::PlayerSide::P2 {
                        GameplayInputPlayerSide::P2
                    } else {
                        GameplayInputPlayerSide::P1
                    },
                    joined_sides: joined,
                    ..Default::default()
                };
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(640.0, 480.0),
                    session,
                    profiles,
                );
                state.boundary.total_elapsed_in_screen = 12.5;
                state.progress.stage.song_completed_naturally = true;
                for runtime in 0..state.num_players() {
                    let (start, end) = state.note_range_for_player(runtime);
                    for note in &mut state.chart_runtime.notes[start..end] {
                        if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                            let column = note.column % 4;
                            note.result = Some(Judgment {
                                time_error_ms: [-7.0, 23.0, 0.0][column],
                                time_error_music_ns: [-7_000_000, 23_000_000, 0][column],
                                grade: [
                                    JudgeGrade::Fantastic,
                                    JudgeGrade::Excellent,
                                    JudgeGrade::Miss,
                                ][column],
                                window: [Some(TimingWindow::W0), Some(TimingWindow::W2), None]
                                    [column],
                                miss_because_held: column == 2,
                            });
                        }
                    }
                    state.chart_runtime.column_judgment_eligible[start..end].fill(true);
                    let player = &mut state.players_runtime.players[runtime];
                    player.judgment_counts = [1, 1, 0, 0, 0, 1];
                    player.scoring_counts = player.judgment_counts;
                    player.earned_grade_points = 12;
                    player.recorded_max_combo = 27 + runtime as u32;
                    player.holds_held = 1;
                    player.holds_held_for_score = 1;
                    player.rolls_held = 1;
                    player.rolls_held_for_score = 1;
                    player.mines_avoided = 2;
                    player.calories_burned = 3.25 + runtime as f32;
                    player.life_history = vec![(0.0, 1.0), (5.0, 0.75)];
                    state.progress.window_counts.canonical[runtime] =
                        deadsync_rules::timing::WindowCounts {
                            w0: 1,
                            w2: 1,
                            miss: 1,
                            ..Default::default()
                        };
                    state.progress.window_counts.ten_ms_blue[runtime] =
                        state.progress.window_counts.canonical[runtime];
                }
                let stage = stage_result(&state.gameplay, Default::default());
                assert_eq!(stage.duration_seconds, 12.5);
                assert_eq!(stage.players.iter().flatten().count(), state.num_players());
                assert!(!all_players_failed(&stage, joined));
                for runtime in 0..state.num_players() {
                    let physical = deadsync_gameplay::gameplay_player_side_index(
                        state.setup.session.runtime_player_side(runtime),
                    );
                    let result = stage.players[physical]
                        .as_ref()
                        .expect("result on physical side");
                    assert_eq!(result.judgment_counts, [1, 1, 0, 0, 0, 1]);
                    assert_eq!(result.notes_hit, 2);
                    assert_eq!(result.max_combo, 27 + runtime as u32);
                    assert_eq!(result.calories_burned, 3.25 + runtime as f32);
                    assert_eq!(result.column_judgments[0].w0, 1);
                    assert_eq!(result.column_judgments[1].w2, 1);
                    assert_eq!(result.column_judgments[2].miss, 1);
                    assert_eq!(result.column_judgments[2].held_miss, 1);
                    assert_eq!(result.histogram.bins, [(-7, 1), (23, 1)]);
                    assert_eq!(result.timing.mean_ms, 8.0);
                    assert_eq!(result.timing.mean_abs_ms, 15.0);
                    assert_eq!(result.window_counts.w0, 1);
                    assert_eq!(result.window_counts_10ms.w2, 1);
                    assert_eq!(result.holds_held, 1);
                    assert_eq!(result.rolls_held, 1);
                    assert_eq!(result.mines_avoided, 2);
                    assert_eq!(result.life_history, [(0.0, 1.0), (5.0, 0.75)]);
                    assert!(!result.disqualified);
                }
                let mut context = EvaluationContextView {
                    play_style: style,
                    player_side: side,
                    ..Default::default()
                };
                for (player, joined) in context.players.iter_mut().zip(joined) {
                    player.joined = joined;
                }
                let view = crate::session_results::evaluation_view(&state, &stage, context);
                for runtime in 0..state.num_players() {
                    let score = view.score_info[runtime].as_ref().expect("evaluation view");
                    let result = stage.players[profile_data::player_side_index(score.side)]
                        .as_ref()
                        .expect("canonical side");
                    assert_eq!(score.grade, result.grade);
                    assert_eq!(score.score_percent, result.score_percent);
                    assert_eq!(score.ex_score_percent, result.ex_score_percent);
                    assert_eq!(score.max_combo, result.max_combo);
                    assert_eq!(score.column_judgments, result.column_judgments);
                    assert_eq!(score.scatter.len(), result.scatter.len());
                    assert_eq!(score.histogram.bins, result.histogram.bins);
                }
                let mut screen = evaluation::init(view);
                assert_eq!(screen.stage_duration_seconds, 12.5);
                assert!(!evaluation::all_joined_players_failed(&screen));
                assert!(evaluation::overview_pane_selected(&screen));
                screen.screen_elapsed = deadsync_theme_openitg::EVALUATION_OVERVIEW_SETTLED_SECONDS;
                space::with_current_metrics(space::Metrics::centered(854.0, 480.0), || {
                    let assets = deadlib_assets::AssetManager::new();
                    let mut actors = Vec::new();
                    evaluation::push_actors_with_overview(
                        &mut actors,
                        &screen,
                        &assets,
                        Default::default(),
                        Some(deadsync_theme_openitg::push_evaluation_overview),
                    );
                    assert!(actor_tree_has_text(&actors, "CHECK YOUR STATS"));
                    assert!(actor_tree_has_text(&actors, "00027"));
                    assert_eq!(actor_tree_has_text(&actors, "00028"), style.is_versus());
                    let graph = actors.iter().find_map(|actor| match actor {
                        Actor::Mesh { vertices, .. } => Some(Arc::clone(vertices)),
                        _ => None,
                    }).expect("overview life graph");
                    actors.clear();
                    evaluation::push_actors_with_overview(
                        &mut actors,
                        &screen,
                        &assets,
                        Default::default(),
                        Some(deadsync_theme_openitg::push_evaluation_overview),
                    );
                    assert!(actors.iter().any(|actor| matches!(actor,
                        Actor::Mesh { vertices, .. } if Arc::ptr_eq(&graph, vertices)
                    )));
                });
                // Session storage does not need the screen and remains unchanged by it.
                drop(screen);
                let mut session = crate::session::SessionState::new(0, [0; MAX_PLAYERS]);
                session.record_stage_result(stage);
                assert_eq!(session.played_stages.len(), 1);
                state.players_runtime.players[0].is_failing = true;
                state.players_runtime.players[0].fail_time = Some(3.0);
                let failed = stage_result(&state.gameplay, Default::default());
                assert_eq!(
                    all_players_failed(&failed, joined),
                    state.num_players() == 1
                );
                state.progress.stage.autoplay_used = true;
                let autoplay = stage_result(&state.gameplay, Default::default());
                assert!(all_players_failed(&autoplay, joined));
                assert!(
                    autoplay
                        .players
                        .iter()
                        .flatten()
                        .all(|p| p.disqualified && !p.score_valid && p.grade == Grade::Failed)
                );
                state.progress.stage.autoplay_used = false;
                state.progress.stage.song_completed_naturally = false;
                let aborted = stage_result(&state.gameplay, Default::default());
                assert!(all_players_failed(&aborted, joined));
                assert!(aborted.players.iter().flatten().all(|p| !p.disqualified));
            });
        }
    }

    #[test]
    fn disabling_holding_explosions_keeps_hold_and_roll_heads_engaged() {
        let simfile = write_fixture("holding-explosion-off", generated_hold_mine_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(640.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(640, 480);
                space::set_overscan(0, 0, 0, 0);

                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0]
                    .tap_explosion_active_mask
                    .remove(profile_data::TapExplosionMask::HOLDING);
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(640.0, 480.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                add_hold_mine_state(&mut state);
                let hold = state
                    .active_hold(0)
                    .cloned()
                    .expect("fixture hold should be active");
                let roll = state
                    .active_hold(1)
                    .cloned()
                    .expect("fixture roll should be active");
                state.clear_active_holds();

                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let inactive = compose_fixture_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );

                state.set_active_hold(0, Some(hold));
                let engaged_hold = compose_fixture_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert_ne!(
                    compare_render_frames(&inactive, &engaged_hold),
                    Ok(()),
                    "an active hold must stay pinned when Holding explosions are disabled"
                );

                state.set_active_hold(0, None);
                state.set_active_hold(1, Some(roll));
                let engaged_roll = compose_fixture_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert_ne!(
                    compare_render_frames(&inactive, &engaged_roll),
                    Ok(()),
                    "an active roll must stay pinned when Holding explosions are disabled"
                );
            },
        );
    }

    #[test]
    fn hold_mine_frame_is_structurally_repeatable() {
        let simfile = write_fixture("f0-hold-mine", generated_hold_mine_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(640.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(640, 480);
                space::set_overscan(0, 0, 0, 0);

                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                profiles[0].error_bar_active_mask = profile_data::ErrorBarMask::COLORFUL
                    | profile_data::ErrorBarMask::MONOCHROME
                    | profile_data::ErrorBarMask::TEXT;
                profiles[0].text_error_bar_scalable = true;
                profiles[0].text_error_bar_threshold_ms = 10;
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(640.0, 480.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                let [hold_index, roll_index, dropped_index] = add_hold_mine_state(&mut state);

                assert_eq!(
                    state.chart_runtime.notes[hold_index].note_type,
                    NoteType::Hold
                );
                assert_eq!(
                    state.chart_runtime.notes[roll_index].note_type,
                    NoteType::Roll
                );
                assert!(
                    state
                        .chart_runtime
                        .notes
                        .iter()
                        .any(|note| note.note_type == NoteType::Mine)
                );
                assert_eq!(
                    state.active_hold(0).map(|hold| hold.note_index),
                    Some(hold_index)
                );
                assert_eq!(
                    state.active_hold(1).map(|hold| hold.note_index),
                    Some(roll_index)
                );
                let dropped = state.chart_runtime.notes[dropped_index]
                    .hold
                    .as_ref()
                    .expect("fixture dropped hold should have hold data");
                assert_eq!(
                    dropped.result,
                    Some(deadsync_rules::note::HoldResult::LetGo)
                );
                assert_eq!(dropped.life, 0.35);
                assert!(state.receptor_glow_visual_for_col(0).is_some());
                assert!(state.mine_explosions_for_columns(3, 1)[0].is_some());
                assert!(state.hold_judgment(2).is_some());
                let player = &state.players_runtime.players[0];
                assert!(player.error_bar_color_ticks.iter().any(Option::is_some));
                assert!(player.error_bar_mono_ticks.iter().any(Option::is_some));
                assert!(player.error_bar_text.is_some());

                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let normal = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );

                state.profiles_runtime.profiles[0]
                    .set_scroll_option(profile_data::ScrollOption::Reverse);
                state.refresh_live_notefield_options(120.0);
                let reverse = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert_ne!(compare_render_frames(&normal, &reverse), Ok(()));

                state.clear_active_holds();
                state.display.hold_feedback.clear();
                state.clear_visual_feedback();
                state.display.receptor_feedback.reset_for_practice();
                let mut without_live_feedback = compose_fixture_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert_ne!(
                    compare_render_frames(&reverse, &without_live_feedback),
                    Ok(())
                );
                compose_scratch.recycle_frame(&mut without_live_feedback);
            },
        );
    }

    fn generated_model_effects_simfile() -> &'static str {
        r#"#VERSION:0.83;
#TITLE:F0 Model Effects;
#MUSIC:;
#OFFSET:0.000;
#BPMS:0.000=120.000;

#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:F0-model-effects;
#DIFFICULTY:Challenge;
#METER:12;
#RADARVALUES:0,0,0,0,0;
#NOTES:
1000
0100
0010
0001
1100
0011
1001
0110
,
1000
0100
0010
0001
1000
0100
0010
0001
,
M000
0100
00L0
000F
1001
0110
0010
0001
,
1000
0100
0010
0001
1100
0011
1001
0110
;
"#
    }

    #[test]
    fn model_effects_frame_is_structurally_repeatable() {
        let simfile = write_fixture("f0-model-effects", generated_model_effects_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);

                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("vivid");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                profiles[0].scroll_option = profile_data::ScrollOption::Split;
                profiles[0].mini_percent = 35;
                profiles[0].perspective = profile_data::Perspective::Incoming;
                profiles[0].visual_effects_active_mask = profile_data::VisualEffectsMask::DRUNK
                    | profile_data::VisualEffectsMask::TORNADO
                    | profile_data::VisualEffectsMask::BUMPY;
                profiles[0].appearance_effects_active_mask =
                    profile_data::AppearanceEffectsMask::HIDDEN
                        | profile_data::AppearanceEffectsMask::SUDDEN;
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.625);

                let visual =
                    deadsync_gameplay::effective_visual_effects_for_player(&state.gameplay, 0);
                assert_eq!(visual.drunk, 1.0);
                assert_eq!(visual.tornado, 1.0);
                assert_eq!(visual.bumpy, 1.0);
                let appearance = state.gameplay.effective_appearance_effects_for_player(0);
                assert_eq!(appearance.hidden, 1.0);
                assert_eq!(appearance.sudden, 1.0);
                let perspective =
                    deadsync_gameplay::effective_perspective_effects_for_player(&state.gameplay, 0);
                assert_eq!(perspective.tilt, -1.0);
                assert_eq!(perspective.skew, 1.0);
                assert_eq!(
                    deadsync_gameplay::effective_mini_percent_for_player(&state.gameplay, 0),
                    35.0
                );

                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut render_variant =
                    |scroll_option: profile_data::ScrollOption, reverse: [f32; 4]| {
                        state.profiles_runtime.profiles[0].set_scroll_option(scroll_option);
                        state.refresh_live_notefield_options(120.0);
                        let scroll = deadsync_gameplay::effective_scroll_effects_for_player(
                            &state.gameplay,
                            0,
                        );
                        assert_eq!(
                            std::array::from_fn::<_, 4, _>(|column| {
                                scroll.reverse_percent_for_column(column, 4)
                            }),
                            reverse
                        );
                        let frame = assert_repeatable_frame(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text_cache,
                            &mut compose_scratch,
                        );
                        assert!(frame.tmesh_geometries.iter().any(|geometry| {
                            geometry.cache_key != deadlib_render_core::INVALID_TMESH_CACHE_KEY
                        }));
                        frame
                    };

                let split = render_variant(profile_data::ScrollOption::Split, [0.0, 0.0, 1.0, 1.0]);
                let alternate =
                    render_variant(profile_data::ScrollOption::Alternate, [0.0, 1.0, 0.0, 1.0]);
                let cross = render_variant(profile_data::ScrollOption::Cross, [0.0, 1.0, 1.0, 0.0]);
                assert_ne!(compare_render_frames(&split, &alternate), Ok(()));
                assert_ne!(compare_render_frames(&split, &cross), Ok(()));
                assert_ne!(compare_render_frames(&alternate, &cross), Ok(()));
            },
        );
    }

    #[test]
    fn diagnostic_frame_can_suppress_active_attack_visuals() {
        let simfile = write_fixture("f0-practice-attack", generated_model_effects_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);

                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("vivid");
                profiles[0].spacing_percent = 25;
                profiles[0].appearance_effects_active_mask =
                    profile_data::AppearanceEffectsMask::DYNAMIC_SUDDEN;
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.625);
                state.refresh_live_notefield_options(120.0);

                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let view = FixtureFrameView {
                    presentation: screen_gameplay::ActorViewOverride {
                        notefield: screen_gameplay::NotefieldViewOverride {
                            hide_display_mods: true,
                            ..Default::default()
                        },
                        hide_gameplay_hud: true,
                        ..Default::default()
                    },
                    preview: Some(SongFrameContent::preview(true, false)),
                };
                let baseline_frame =
                    assert_repeatable_composition(&mut compose_scratch, |scratch| {
                        compose_fixture_frame_with_view(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text_cache,
                            scratch,
                            view,
                            &FIXTURE_TEXTURES,
                        )
                    });
                let attack = deadsync_gameplay::ChartAttackWindow {
                    start_second: 0.0,
                    len_seconds: 10.0,
                    mods: "100% drunk,100% cover,80% mini,reverse".to_string(),
                };
                state.gameplay.mods.attacks.mask_windows[0] =
                    deadsync_gameplay::build_attack_mask_windows(std::slice::from_ref(&attack));
                // Native attacks approach their targets over elapsed time.
                refresh_active_attack_masks(&mut state.gameplay, 1.0);
                state.refresh_live_notefield_options(120.0);
                let (live_geometry, live_visual) = deadsync_notefield::resolve_notefield_motion(
                    &state.gameplay,
                    0,
                    false,
                    [854.0, 480.0],
                    None,
                    true,
                );
                let (edit_geometry, edit_visual) = deadsync_notefield::resolve_notefield_motion(
                    &state.gameplay,
                    0,
                    true,
                    [854.0, 480.0],
                    Some(Default::default()),
                    false,
                );
                assert_eq!(live_visual.visual.drunk, 1.0);
                assert_eq!(live_visual.visibility.cover, 1.0);
                assert!(live_geometry.reverse_scroll);
                assert_eq!(live_geometry.column_dirs[0], -1.0);
                assert_eq!(live_geometry.field_zoom, state.field_zoom_for_player(0));
                assert_eq!(edit_visual.visual.drunk, 0.0);
                assert_eq!(edit_visual.visibility.cover, 0.0);
                assert!(!edit_geometry.reverse_scroll);
                assert_eq!(edit_geometry.column_dirs, [1.0; deadsync_core::input::MAX_COLS]);
                assert_eq!(edit_geometry.field_zoom, 1.0);
                for visual in [live_visual, edit_visual] {
                    assert_eq!(visual.spacing_multiplier, 1.25);
                    assert!(visual.dynamic_sudden);
                }
                let suppressed_frame =
                    assert_repeatable_composition(&mut compose_scratch, |scratch| {
                        compose_fixture_frame_with_view(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text_cache,
                            scratch,
                            FixtureFrameView {
                                preview: Some(SongFrameContent::preview(false, false)),
                                ..view
                            },
                            &FIXTURE_TEXTURES,
                        )
                    });
                let active_frame = assert_repeatable_composition(&mut compose_scratch, |scratch| {
                    compose_fixture_frame_with_view(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        scratch,
                        view,
                        &FIXTURE_TEXTURES,
                    )
                });

                assert_ne!(
                    compare_render_frames(&baseline_frame, &active_frame),
                    Ok(())
                );
                assert_eq!(
                    compare_render_frames(&baseline_frame, &suppressed_frame),
                    Ok(())
                );
            },
        );
    }

    fn generated_pipeline_song_lua() -> &'static str {
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
local player = nil
local capture = nil
local local_target = nil
prefix_globals = {}

local function offset_col(value)
    local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
    local handler = nf:GetColumnActors()[2]:GetPosHandler()
    handler:SetSplineMode("NoteColumnSplineMode_Offset")
    handler:SetBeatsPerT(10)
    local spline = handler:GetSpline()
    spline:SetSize(2)
    spline:SetPoint(1, {0, value, 0})
    spline:SetPoint(2, {0, value, 0.001})
    spline:Solve()
end

mods_ease = {
    {4, 4, 36, -12, offset_col, "len", ease.outSine},
}

return Def.ActorFrame{
    OnCommand=function(self) self:sleep(20) end,
    InitCommand=function(self)
        prefix_globals.ease = {
            {4, 4, 0, 18, function(value) if player then player:rotationz(value) end end, "len", ease.inOutQuad},
            {4, 4, 1, 0.85, function(value) if player then player:zoom(value) end end, "len", ease.outQuad},
        }
        self:SetUpdateFunction(function(actor)
            local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
            local handler = nf:GetColumnActors()[1]:GetZoomHandler()
            handler:SetSplineMode("NoteColumnSplineMode_Offset")
                :SetSubtractSongBeat(false)
                :SetReceptorT(0)
                :SetBeatsPerT(4)
            local spline = handler:GetSpline()
            spline:SetSize(3)
            spline:SetPoint(1, {0, 0, 0})
            spline:SetPoint(2, {-1, -1, -1})
            spline:SetPoint(3, {-1, -1, -1})
            spline:Solve()
        end)
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            self:queuecommand("BindPlayer")
        end,
        BindPlayerCommand=function(self)
            player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        end,
    },
    Def.Quad{
        Name="ForegroundMarker",
        InitCommand=function(self)
            self:x(SCREEN_CENTER_X)
            self:y(80)
            self:zoomto(180, 36)
            self:diffuse(0.15, 0.55, 0.95, 0.8)
        end,
    },
    Def.ActorFrame{
        Name="LocalProxyTarget",
        InitCommand=function(self) local_target = self end,
        Def.Quad{
            Name="LocalProxyMarker",
            OnCommand=function(self)
                self:x(SCREEN_CENTER_X)
                self:y(SCREEN_CENTER_Y)
                self:zoomto(320, 180)
                self:diffuse(0.8, 0.2, 0.1, 0.7)
            end,
        },
    },
    Def.ActorFrameTexture{
        InitCommand=function(self)
            capture = self
            self:SetWidth(320)
            self:SetHeight(180)
            self:Create()
        end,
        Def.ActorProxy{
            Name="FixturePlayerProxy",
            OnCommand=function(self)
                local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                self:SetTarget(player)
                player:visible(false)
                self:visible(true)
            end,
        },
        Def.ActorProxy{
            Name="FixtureJudgmentProxy",
            OnCommand=function(self)
                local judgment = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("Judgment")
                self:SetTarget(judgment)
                self:visible(true)
            end,
        },
        Def.ActorProxy{
            Name="FixtureComboProxy",
            OnCommand=function(self)
                local combo = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("Combo")
                self:SetTarget(combo)
                self:visible(true)
            end,
        },
        Def.ActorProxy{
            Name="FixtureLocalProxy",
            OnCommand=function(self)
                self:SetTarget(local_target)
                local_target:visible(false)
            end,
        },
        Def.ActorProxy{
            Name="FixtureUnderlayProxy",
            OnCommand=function(self)
                local target = SCREENMAN:GetTopScreen():GetChild("Underlay")
                self:SetTarget(target)
                target:visible(false)
            end,
        },
        Def.ActorProxy{
            Name="FixtureOverlayProxy",
            OnCommand=function(self)
                local target = SCREENMAN:GetTopScreen():GetChild("Overlay")
                self:SetTarget(target)
                target:visible(false)
            end,
        },
    },
    Def.Sprite{
        Name="FixtureCaptureSprite",
        OnCommand=function(self)
            if capture then
                self:SetTexture(capture:GetTexture())
            end
            self:x(SCREEN_CENTER_X + 180)
            self:y(SCREEN_CENTER_Y)
            self:zoom(0.35)
            self:diffuse(1, 0.2, 0.2, 0.75)
            self:blend("add")
        end,
    },
}
"#
    }

    fn generated_pipeline_song_lua_simfile() -> &'static str {
        r#"#VERSION:0.83;
#TITLE:F0 SongLua;
#MUSIC:;
#OFFSET:0.000;
#BPMS:0.000=120.000;
#FGCHANGES:0.000=lua/default.lua=1.000=0=0=0=StretchNoLoop====;

#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:F0-song-lua;
#DIFFICULTY:Challenge;
#METER:12;
#RADARVALUES:0,0,0,0,0;
#NOTES:
1000
0100
0010
0001
,
1100
0011
1001
0110
,
1000
0100
0010
0001
,
0100
0010
0001
1000
;
"#
    }

    #[test]
    fn song_lua_conditional_spawn_skips_probed_receiver() {
        let simfile = write_fixture("conditional-spawn", generated_pipeline_song_lua_simfile());
        let lua_dir = simfile.parent().expect("fixture folder").join("lua");
        fs::create_dir_all(&lua_dir).expect("Lua fixture folder");
        fs::write(
            lua_dir.join("default.lua"),
            r#"
local account, fired = 1, false
local root = Def.ActorFrame {
    OnCommand=function(self)
        self:sleep(1000)
        self:SetUpdateFunction(function()
            if not fired and GAMESTATE:GetSongBeat() >= 1 then
                fired = true
                MESSAGEMAN:Broadcast("Spawn")
            end
        end)
    end,
    SpawnMessageCommand=function(self) account = account + 1 end,
}
for index=1,2 do
    local number = index
    root[#root+1] = Def.ActorProxy {
        OnCommand=function(self)
            self:SetTarget(SCREENMAN:GetTopScreen():GetChild("PlayerP1"))
            self:visible(false)
        end,
        SpawnMessageCommand=function(self)
            if number == account then self:visible(true) end
        end,
    }
end
return root
"#,
        )
        .expect("conditional Spawn fixture");
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                space::set_current_metrics(space::Metrics::centered(640.0, 480.0));
                space::set_current_window_px(640, 480);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(640.0, 480.0),
                    GameplaySession::default(),
                    std::array::from_fn(|_| profile_data::Profile::default()),
                );
                let assets = fixture_assets();
                let mut actors = Vec::new();
                for (seconds, proxies) in [(0.25, 0), (0.6, 1), (2.0, 1), (0.25, 0)] {
                    set_fixture_time(&mut state, seconds);
                    actors.clear();
                    let segments = crate::gameplay_runtime::push_actors(
                        &mut actors,
                        &mut state,
                        &assets,
                        screen_gameplay::ActorViewOverride::default(),
                        123.0,
                        deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                    );
                    assert_eq!(
                        segments.direct_field_proxy_count(state.song_frame()),
                        proxies,
                        "conditional receiver at {seconds}s"
                    );
                }
            },
        );
    }

    fn write_pipeline_song_lua_fixture() -> PathBuf {
        let simfile = write_fixture("f0-song-lua", generated_pipeline_song_lua_simfile());
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        let lua_entry = lua_dir.join("default.lua");
        fs::write(&lua_entry, generated_pipeline_song_lua()).unwrap();
        fs::write(simfile.parent().unwrap().join("easing.lua"),
            include_str!("../../../tests/fixtures/song_lua/easing.lua")).unwrap();
        simfile
    }

    #[test]
    fn song_lua_texture_clock_excludes_screen_lead_in() {
        let text = generated_pipeline_song_lua_simfile()
            .replace("#OFFSET:0.000;", "#OFFSET:-2.103;")
            .replace(
                "StretchNoLoop====;",
                "StretchNoLoop====,6.500=late/default.lua=1.000=0=0=0=StretchNoLoop====;",
            );
        let simfile = write_fixture("f0-foreground-clock", &text);
        let folder = simfile.parent().expect("fixture folder");
        for name in ["lua", "late"] {
            let folder = folder.join(name);
            fs::create_dir_all(&folder).expect("foreground folder");
            fs::write(
                folder.join("default.lua"),
                r#"
    return Def.ActorFrame {
    OnCommand=function(self) self:sleep(1000) end,
    NOTESKIN:LoadActorForNoteSkin("Down", "Tap Note", "cyber")..{
        Name="ClockArrow",
        InitCommand=function(self) self:xy(320,240) end,
    },
    }
    "#,
            )
            .expect("foreground model fixture");
        }
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                space::set_current_metrics(space::Metrics::centered(854.0, 480.0));
                space::set_current_window_px(1280, 720);
                let profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                let mut assets = fixture_assets();
                let visuals = state.gameplay.song_lua_visuals();
                for overlay in visuals.overlays.iter().chain(
                    visuals
                        .foreground_visual_layers
                        .iter()
                        .flat_map(|layer| &layer.overlays),
                ) {
                    match &overlay.kind {
                        deadsync_assets::song_lua::SongLuaOverlayKind::NoteskinActor { slots, .. } => {
                            for slot in slots.iter() {
                                assets.queue_texture_upload(
                                    slot.texture_key().to_owned(),
                                    image::RgbaImage::new(16, 16),
                                );
                            }
                        }
                        deadsync_assets::song_lua::SongLuaOverlayKind::Model { layers } => {
                            for layer in layers.iter() {
                                assets.queue_texture_upload(
                                    layer.texture_key.to_string(),
                                    image::RgbaImage::new(16, 16),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                // The fixture uses GameplayConfig's global offset as well
                // as the simfile offset, just as foreground activation does.
                let primary_start = state.music_time_for_beat(0.0);
                let late_start = state.music_time_for_beat(6.5);
                for music in [primary_start + 0.5, late_start + 0.25] {
                    set_fixture_time(&mut state, music);
                    let mut phases = Vec::new();
                    for screen_age in [10.0, 10.371] {
                        state.boundary.total_elapsed_in_screen = screen_age;
                        let mut actors = Vec::new();
                        crate::gameplay_runtime::push_actors(
                            &mut actors,
                            &mut state,
                            &assets,
                            screen_gameplay::ActorViewOverride::default(),
                            123.0,
                            deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(
                            ),
                        );
                        let actual = actors
                            .iter()
                            .filter_map(|actor| match actor {
                                Actor::TexturedMesh {
                                    texture,
                                    uv_tex_shift,
                                    ..
                                } if texture
                                    .texture_key()
                                    .is_some_and(|key| key.contains("cyber")) =>
                                {
                                    Some(uv_tex_shift[1].rem_euclid(1.0))
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        assert!(!actual.is_empty(), "foreground models must actually render");
                        assert!(
                            actual.iter().any(|phase| (*phase - 0.5).abs() < 0.00001),
                            "primary foreground must animate from beat zero: {actual:?}"
                        );
                        if music > late_start {
                            assert!(
                                actual.iter().any(|phase| (*phase - 0.75).abs() < 0.00001),
                                "later foreground must use its own start: {actual:?}"
                            );
                        }
                        phases.push(actual);
                    }
                    assert_eq!(
                        phases[0], phases[1],
                        "screen lead-in must not shift foreground texture animation"
                    );
                }
            },
        );
    }

    #[test]
    fn top_screen_text_stays_outside_song_lua_aft_captures() {
        let simfile = write_pipeline_song_lua_fixture();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].display_name = "PixelSafePlayer".to_string();
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                state.set_live_autoplay_enabled(true);

                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );

                assert!(actor_tree_has_text(&actors, "EVENT"));
                assert!(actor_tree_has_text(&actors, "PixelSafePlayer"));
                assert!(actor_tree_has_text(&actors, "AutoPlay"));
                assert!(
                    !state
                        .render_targets()
                        .iter()
                        .any(|target| actor_tree_has_text(&target.children, "EVENT"))
                );
                assert!(
                    !state
                        .render_targets()
                        .iter()
                        .any(|target| actor_tree_has_text(&target.children, "PixelSafePlayer"))
                );
                assert!(
                    !state
                        .render_targets()
                        .iter()
                        .any(|target| actor_tree_has_text(&target.children, "AutoPlay"))
                );

                let aft_z = max_render_target_sprite_z(&actors, 0)
                    .expect("fixture should draw an AFT output sprite");
                for text in ["EVENT", "PixelSafePlayer", "AutoPlay"] {
                    let (text_z, alpha) = top_screen_text_draw(&actors, text, 0, 1.0)
                        .unwrap_or_else(|| panic!("missing top-screen text {text}"));
                    assert_eq!(alpha, 1.0, "{text} should stay fully opaque");
                    assert!(
                        text_z > aft_z,
                        "{text} at z={text_z} should draw above the AFT sprite at z={aft_z}"
                    );
                }
            },
        );
    }

    #[test]
    fn system_overlay_ignores_song_gameplay_overlay_visibility_and_tint() {
        // ScreenManager::Draw renders ScreenSyncOverlay and ScreenSystemLayer
        // after ScreenGameplay. Dyscontrolled Galaxy hides the latter's
        // Overlay child; that must not hide or tint the separate system screens.
        for (name, overlay_state) in [
            ("tinted", "diffuse(0.1, 0.2, 0.3, 0.25)"),
            ("hidden", "visible(false)"),
        ] {
            let simfile = write_fixture(
                &format!("system-overlay-{name}"),
                generated_pipeline_song_lua_simfile(),
            );
            let lua_dir = simfile.parent().unwrap().join("lua");
            fs::create_dir_all(&lua_dir).unwrap();
            fs::write(
                lua_dir.join("default.lua"),
                r#"
return Def.ActorFrame {
    OnCommand=function(self)
        local screen = SCREENMAN:GetTopScreen()
        screen:xy(100, 50):rotationz(35):zoom(0.7):diffuse(0.2, 0.3, 0.4, 0.15)
        screen:GetChild("Underlay"):visible(false)
        screen:GetChild("Overlay"):OVERLAY_STATE
        self:sleep(1000)
    end,
}
"#
                .replace("OVERLAY_STATE", overlay_state),
            )
            .unwrap();
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    space::set_current_metrics(space::Metrics::centered(854.0, 480.0));
                    space::set_current_window_px(1280, 720);
                    space::set_overscan(0, 0, 0, 0);
                    let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
                    profiles[0].display_name = "SystemPlayer".to_string();
                    let mut state = build_compiled_test_state(
                        &simfile,
                        GameplayViewport::new(1280.0, 720.0),
                        GameplaySession::default(),
                        profiles,
                    );
                    set_fixture_time(&mut state, 2.5);
                    let assets = fixture_assets();
                    let mut actors = Vec::new();
                    for (autoplay, hide_gameplay_hud) in
                        [(true, false), (false, false), (true, true)]
                    {
                        state.set_live_autoplay_enabled(autoplay);
                        actors.clear();
                        crate::gameplay_runtime::push_actors(
                            &mut actors,
                            &mut state,
                            &assets,
                            screen_gameplay::ActorViewOverride {
                                hide_gameplay_hud,
                                ..Default::default()
                            },
                            123.0,
                            deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(
                            ),
                        );
                        assert_eq!(
                            actor_tree_has_text(&actors, "AutoPlay"),
                            autoplay && !hide_gameplay_hud,
                            "{name} gameplay overlay, autoplay={autoplay}, hidden HUD={hide_gameplay_hud}"
                        );
                        if hide_gameplay_hud {
                            assert!(!actor_tree_has_text(&actors, "SystemPlayer"));
                            continue;
                        }
                        assert_eq!(
                            top_screen_text_draw(&actors, "SystemPlayer", 0, 1.0)
                                .unwrap()
                                .1,
                            1.0,
                            "gameplay tint must not reach the system footer"
                        );
                        if autoplay {
                            let (position, color) = actors
                                .iter()
                                .find_map(|actor| match actor {
                                    Actor::Text {
                                        content,
                                        offset,
                                        color,
                                        ..
                                    } if content.as_str() == "AutoPlay" => Some((*offset, *color)),
                                    _ => None,
                                })
                                .expect("Autoplay should be a separate system actor");
                            assert_eq!(position, [427.0, 390.0]);
                            assert_eq!(color, [1.0; 4]);
                        }
                    }
                },
            );
        }
    }

    fn write_direct_note_field_proxy_fixture() -> PathBuf {
        let simfile = write_fixture(
            "f6-direct-note-field",
            generated_pipeline_song_lua_simfile(),
        );
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("default.lua"),
            r#"
return Def.ActorFrame{
    OnCommand=function(self) self:sleep(4) end,
    Def.ActorProxy{
        OnCommand=function(self)
            local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
            self:SetTarget(nf)
            self:x(SCREEN_CENTER_X)
            self:y(SCREEN_CENTER_Y)
            nf:visible(false)
        end,
    },
}
"#,
        )
        .unwrap();
        simfile
    }

    fn write_direct_player_proxy_fixture() -> PathBuf {
        let simfile = write_fixture("f6-direct-player", generated_pipeline_song_lua_simfile());
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("default.lua"),
            r#"
return Def.ActorFrame{
    OnCommand=function(self) self:sleep(4) end,
    Def.ActorProxy{
        OnCommand=function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            self:SetTarget(player)
                self:x(32)
                self:y(-18)
                self:diffuse(0.8, 0.6, 1.0, 0.75)
                self:blend("add")
                player:rotationx(7)
                player:rotationy(27)
                player:rotationz(11)
                player:skewx(0.125)
                player:skewy(-0.0625)
                player:zoomx(0.875)
                player:zoomy(1.125)
                player:diffuse(0.7, 0.6, 0.9, 0.8)
                player:blend("multiply")
                player:visible(false)
        end,
    },
}
"#,
        )
        .unwrap();
        simfile
    }

    fn write_direct_aft_note_field_proxy_fixture() -> PathBuf {
        let simfile = write_fixture(
            "f6-direct-aft-note-field",
            generated_pipeline_song_lua_simfile(),
        );
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("default.lua"),
            r#"
local capture = nil

return Def.ActorFrame{
    OnCommand=function(self) self:sleep(4) end,
    Def.ActorFrameTexture{
        Name="DirectCapture",
        InitCommand=function(self)
            capture = self
            self:SetWidth(320)
            self:SetHeight(180)
            self:Create()
        end,
        Def.ActorProxy{
            OnCommand=function(self)
                local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
                self:SetTarget(nf)
                self:x(SCREEN_CENTER_X)
                self:y(SCREEN_CENTER_Y)
                nf:visible(false)
            end,
        },
    },
    Def.Sprite{
        OnCommand=function(self)
            self:SetTexture(capture:GetTexture())
            self:x(SCREEN_CENTER_X + 48)
            self:y(SCREEN_CENTER_Y - 24)
            self:zoom(0.9)
            self:diffuse(0.8, 0.6, 1.0, 0.75)
            self:blend("add")
        end,
    },
}
"#,
        )
        .unwrap();
        simfile
    }

    fn write_direct_aft_player_proxy_fixture() -> PathBuf {
        let simfile = write_fixture(
            "f6-direct-aft-player",
            generated_pipeline_song_lua_simfile(),
        );
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("default.lua"),
            r#"
local capture = nil

return Def.ActorFrame{
    OnCommand=function(self) self:sleep(4) end,
    Def.ActorFrameTexture{
        Name="DirectPlayerCapture",
        InitCommand=function(self)
            capture = self
            self:SetWidth(320)
            self:SetHeight(180)
            self:Create()
        end,
        Def.ActorProxy{
            OnCommand=function(self)
                local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                self:SetTarget(player)
                self:x(32)
                self:y(-18)
                player:rotationx(7)
                player:rotationy(27)
                player:rotationz(11)
                player:skewx(0.125)
                player:skewy(-0.0625)
                player:zoomx(0.875)
                player:zoomy(1.125)
                player:diffuse(0.7, 0.6, 0.9, 0.8)
                player:blend("multiply")
                player:visible(false)
            end,
        },
    },
    Def.Sprite{
        OnCommand=function(self)
            self:SetTexture(capture:GetTexture())
            self:x(SCREEN_CENTER_X + 48)
            self:y(SCREEN_CENTER_Y - 24)
            self:zoom(0.9)
            self:rotationz(8)
            self:diffuse(0.8, 0.6, 1.0, 0.75)
            self:blend("add")
        end,
    },
}
"#,
        )
        .unwrap();
        simfile
    }

    fn write_direct_hud_proxy_fixture(name: &str, child: &str) -> PathBuf {
        let simfile = write_fixture(name, generated_pipeline_song_lua_simfile());
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(
            lua_dir.join("default.lua"),
            r#"
return Def.ActorFrame{
    OnCommand=function(self) self:sleep(3) end,
    Def.ActorProxy{
        OnCommand=function(self)
            local hud = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("HUD_CHILD")
            self:SetTarget(hud)
            self:x(SCREEN_CENTER_X)
            self:y(SCREEN_CENTER_Y)
        end,
    },
    Def.ActorProxy{
        OnCommand=function(self)
            local hud = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("HUD_CHILD")
            self:SetTarget(hud)
            self:x(SCREEN_CENTER_X + 96)
            self:y(SCREEN_CENTER_Y + 48)
        end,
    },
}
"#
            .replace("HUD_CHILD", child),
        )
        .unwrap();
        simfile
    }

    #[test]
    fn root_judgment_proxies_match_in_baked_and_runtime_playback() {
        let simfile = write_direct_hud_proxy_fixture("direct-judgment", "Judgment");
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(854, 480);
                let assets = fixture_assets();
                let mut actors = Vec::new();
                for live in [false, true] {
                    let mut state = build_test_state_with_lua_mode(
                        &simfile,
                        GameplayViewport::new(854.0, 480.0),
                        GameplaySession::default(),
                        std::array::from_fn(|_| profile_data::Profile::default()),
                        "Software",
                        None,
                        live,
                    );
                    assert_eq!(state.song_lua.is_some(), live);
                    set_fixture_time(&mut state, 2.5);
                    add_sprite_core_feedback(&mut state, 0, 0, 100);
                    for hud in [None, Some(deadsync_theme_openitg::GAMEPLAY_HUD)] {
                        if let Some(hud) = hud {
                            state.presentation.set_hud(hud);
                        }
                        actors.clear();
                        let segments = crate::gameplay_runtime::push_actors(
                            &mut actors,
                            &mut state,
                            &assets,
                            screen_gameplay::ActorViewOverride::default(),
                            123.0,
                            deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                        );
                        assert_eq!(segments.direct_proxy_count(), 2, "live={live}");
                    }
                    for (seconds, expected) in [(3.0, 2), (3.5, 0)] {
                        set_fixture_time(&mut state, seconds);
                        add_sprite_core_feedback(&mut state, 0, 0, 100);
                        actors.clear();
                        let segments = crate::gameplay_runtime::push_actors(
                            &mut actors,
                            &mut state,
                            &assets,
                            screen_gameplay::ActorViewOverride::default(),
                            123.0,
                            deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                        );
                        assert_eq!(segments.direct_proxy_count(), expected, "live={live}, seconds={seconds}");
                    }
                }
            },
        );
    }

    #[test]
    fn root_note_field_proxy_uses_repeatable_direct_segment() {
        let simfile = write_direct_note_field_proxy_fixture();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                let visuals = state.gameplay.song_lua_visuals();
                assert!(visuals.overlays.iter().any(|overlay| {
                    // An ordinary ActorFrame parent does not make this an AFT
                    // proxy. The direct-segment assertions below verify that.
                    matches!(
                            overlay.kind,
                            deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                target: deadsync_assets::song_lua::SongLuaProxyTarget::NoteField {
                                    player_index: 0
                                }
                            }
                        )
                }));
                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert!(segments.has_direct_field_proxy(state.song_frame()));
                let actor_segments = segments.segments(state.song_frame(), &actors);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut warm = compose::build_passes(
                    actor_segments,
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    10.0,
                    &mut text_cache,
                    &mut compose_scratch,
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                assert!(warm.ops.iter().any(|op| matches!(op, DrawOp::Sprite(_))));
                compose_scratch.recycle_frame(&mut warm);
                let _ = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );

                let mut edit_actors = Vec::with_capacity(512);
                let edit_segments = crate::gameplay_runtime::push_actors(
                    &mut edit_actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride {
                        notefield: screen_gameplay::NotefieldViewOverride {
                            edit_beat_bars: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert!(edit_segments.has_direct_field_proxy(state.song_frame()));

                let mut plain_edit_actors = Vec::with_capacity(512);
                let practice = deadsync_gameplay::practice::PracticeSession::new(
                    &mut state.gameplay,
                    Default::default(),
                );
                let plain_edit_segments = screen_gameplay::compose_frame(
                    &mut state,
                    &mut plain_edit_actors,
                    FramePresentationContext {
                        assets: &assets,
                        view: screen_gameplay::ActorViewOverride {
                            notefield: screen_gameplay::NotefieldViewOverride {
                                edit_beat_bars: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        uptime_seconds: 123.0,
                        policy: Default::default(),
                    },
                    SongFrameContent::for_practice(&practice),
                );
                assert!(!plain_edit_segments.has_direct_field_proxy(state.song_frame()));
                assert!(state.render_targets().is_empty());

                assert_alternate_stage_presentation(state, practice, &assets);
            },
        );
    }

    const ALTERNATE_GAMEPLAY_MARKER: i16 = 22_800;
    const ALTERNATE_PRACTICE_MARKER: i16 = 22_801;

    fn alternate_marker(z: i16) -> Actor {
        deadlib_present::__act_from_builder!(
            (xy(8.0, 8.0): setsize(12.0, 12.0): diffuse(0.2, 0.7, 0.4, 1.0): z(z))
            deadlib_assets::present_dsl::SpriteBuilder::solid()
        )
    }

    struct AlternateGameplay;

    impl deadsync_theme::views::GameplayLivePresentation<State> for AlternateGameplay {}

    struct AlternateGameplayTheme;

    impl deadsync_theme::Theme for AlternateGameplayTheme {
        type Screen = &'static str;
        type RuntimeRequest = ();

        fn screen_id(screen: Self::Screen) -> deadsync_theme::ThemeScreenId {
            deadsync_theme::ThemeScreenId::new(screen)
        }
    }

    impl deadsync_theme::ThemeScreenFlow for AlternateGameplayTheme {
        const SCREENS: deadsync_theme::ThemeScreens<Self::Screen> = deadsync_theme::ThemeScreens {
            menu: "home",
            gameplay: "dance",
            practice: "rehearsal",
            results: "record",
            song_selection: "library",
            course_selection: "marathon",
            player_options: "settings",
        };
    }

    impl GameplayFramePresenter for AlternateGameplay {
        type Profile = GameplayProfile;
        type Slot = noteskin::SpriteSlot;
        type View = GameplayFrameView;
        type Policy = ();
        type Scratch = ();

        fn draw_layers<'a>(
            &'a self,
            _scratch: &'a mut (),
            _context: FramePresentationContext<'a, GameplayFrameView, ()>,
        ) -> impl FnMut(ScreenLayer, &mut Vec<Actor>, FieldLayout) + 'a {
            |layer, actors, _| {
                if matches!(layer, ScreenLayer::System) {
                    actors.push(alternate_marker(ALTERNATE_GAMEPLAY_MARKER));
                }
            }
        }

        fn field_presentation(
            &self,
            _context: FramePresentationContext<'_, GameplayFrameView, ()>,
            _player: usize,
        ) -> FieldPresentation<'_> {
            FieldPresentation::default()
        }

        fn draw_field_hud(
            &self,
            _context: FramePresentationContext<'_, GameplayFrameView, ()>,
            _gameplay: &State,
            _frame: &FieldFrame<'_>,
            _request: &deadsync_notefield::NotefieldComposeRequest<'_, Self::Slot>,
            _prepared: &deadsync_notefield::PreparedNotefield<'_, Self::Slot>,
            _buffers: FieldHudBuffers<'_>,
        ) -> deadsync_notefield::NotefieldHudComposeResult {
            Default::default()
        }
    }

    impl<L> GameplayStagePresenter<L> for AlternateGameplay {
        type View = GameplayFrameView;
        type Policy = ();

        fn compose_stage_frame(
            &mut self,
            stage: &ActiveGameplayStage,
            _lobby: &L,
            song: &mut FrameScratch,
            actors: &mut Vec<Actor>,
            context: FramePresentationContext<'_, GameplayFrameView, ()>,
            content: SongFrameContent,
        ) -> GameplayActorSegments {
            compose_stage_frame(
                actors,
                stage,
                self,
                GameplayFrameBuffers {
                    song,
                    presentation: &mut (),
                },
                context,
                content,
            )
        }
    }

    struct AlternatePractice {
        overlay_visible: bool,
    }

    struct AlternatePracticeFeedback;

    impl PracticeLivePresentation<AlternateGameplay> for AlternatePractice {
        type Feedback<'a> = AlternatePracticeFeedback;
        type Effect = deadsync_theme::ThemeEffect<&'static str, ()>;
        type EditSnapshot = bool;

        fn editor_feedback(&mut self) -> Self::Feedback<'_> {
            AlternatePracticeFeedback
        }

        fn edit_snapshot(&self) -> Self::EditSnapshot {
            self.overlay_visible
        }

        fn restore_edit_snapshot(&mut self, visible: Self::EditSnapshot) {
            self.overlay_visible = visible;
        }
    }

    impl PracticeEditorPresentation<AlternateGameplay> for AlternatePracticeFeedback {
        fn emit(&mut self, _game: &State, _feedback: deadsync_gameplay::practice::editor::PracticeEditorFeedback) {}
    }

    impl<L> PracticeStagePresenter<L, AlternateGameplay> for AlternatePractice {
        fn frame_view(
            &self,
            _session: &deadsync_gameplay::practice::PracticeSession,
        ) -> GameplayFrameView {
            GameplayFrameView::default()
        }

        fn append_practice_overlay(
            &mut self,
            actors: &mut Vec<Actor>,
            _gameplay: &GameplayHost<L, AlternateGameplay>,
            _session: &deadsync_gameplay::practice::PracticeSession,
        ) {
            if self.overlay_visible {
                actors.push(alternate_marker(ALTERNATE_PRACTICE_MARKER));
            }
        }
    }

    #[test]
    fn alternate_presenter_receives_stage_facts_before_live_lua_attachment() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                with_session(
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    true,
                    false,
                    || {
                        let root = tempfile::tempdir().unwrap();
                        let song = root.path().join("startup.ssc");
                        fs::write(&song, "#TITLE:Alternate startup;\n#BPMS:0=120;\n#FGCHANGES:0=default.lua=1=0=0=1;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n").unwrap();
                        fs::write(root.path().join("default.lua"), "return Def.ActorFrame {}").unwrap();
                        let (prepared, scene, init) = prepare_stage_fixture(
                            &song,
                            GameplayViewport::new(640.0, 480.0),
                            GameplaySession::default(),
                            std::array::from_fn(|_| profile_data::Profile::default()),
                            "Software",
                            None,
                            true,
                            None,
                            None,
                            false,
                        );
                        assert!(scene.is_some());
                        let mut host = crate::gameplay_runtime::present_stage(
                            prepared,
                            scene,
                            Some(""),
                            crate::gameplay_runtime::GameplayStartup {
                                mini_indicator: Default::default(),
                                lobby_session: deadsync_online::gameplay_lobby::GameplayLobbySession::new(
                                    Default::default(), init.runtime.joined, init.runtime.player_side,
                                ),
                                score_refresh: Default::default(),
                                scores: deadsync_profile_gameplay::stage_scores::GameplayScoreSnapshots {
                                    scorebox_visible: [true, false],
                                    ..Default::default()
                                },
                            },
                            |stage, _, scores| {
                                assert_eq!(stage.song().title, "Alternate startup");
                                assert!(stage.song_lua.is_none());
                                assert_eq!(scores.scorebox_visible, [true, false]);
                                AlternateGameplay
                            },
                        );
                        assert_eq!(host.pack_group.as_ref(), "Untitled Course");
                        assert!(host.song_lua.is_some());
                        use crate::gameplay_runtime::{self, GameplayFrameEffect};
                        use deadsync_song_lua::gameplay_session::SongLuaGameplayEffect;

                        let mut audio = deadsync_audio_stream::AudioControl::without_audio();
                        let mut clock = deadsync_audio_stream::MusicClock::without_audio();
                        let mut score_cursor = Default::default();
                        let mut telemetry = Default::default();
                        let sounds = crate::gameplay_prewarm::GameplaySfx::default();
                        gameplay_runtime::enter(&mut host, &mut telemetry, false, &mut audio, &sounds, &mut clock);
                        // The inert audio clock never reaches a beat-zero foreground.
                        // Seed the same request and destination the Lua screen stores.
                        {
                            let mut runtime = host.song_lua.scene_mut().unwrap().runtime_mut();
                            runtime.vm_mut().with_native_stack(|stack| {
                                let top = stack.top();
                                stack.push_bytes(b"__songlua_top_screen")?;
                                stack.get_table(-10002, true)?;
                                let screen = stack.top_offset();
                                stack.push_bytes(b"__songlua_next_screen_name")?;
                                stack.push_bytes(b"ScreenPractice")?;
                                stack.set_table(screen, true)?;
                                stack.set_top(top)
                            }).unwrap();
                            deadsync_song_lua::test_support::seed_rust_transition(
                                &runtime, "SM_DoNextScreen",
                            );
                        }
                        assert_eq!(
                            host.song_lua.scene().unwrap().next_screen_name().unwrap().as_deref(),
                            Some("ScreenPractice"),
                        );
                        let destination = (0..20).find_map(|_| {
                            gameplay_runtime::update::<AlternateGameplayTheme, _>(
                                &mut host, 0.125, false, &mut audio, &sounds, &mut clock,
                                &mut score_cursor, &mut telemetry,
                            ).into_iter().find_map(|effect| match effect {
                                GameplayFrameEffect::Session(SongLuaGameplayEffect::Navigate(screen)) => Some(screen),
                                _ => None,
                            })
                        });
                        assert_eq!(destination, Some("rehearsal"));
                        assert!(host.song_lua.is_ended());
                        gameplay_runtime::sync_initial_scores(&mut host);
                        let practice_init = gameplay_runtime::practice_init(
                            &Default::default(),
                            &profile::gameplay_hud_snapshot(),
                        );
                        let mut practice = gameplay_runtime::prepare_practice(host, practice_init, |stage, _, _| {
                            assert!(stage.song_lua.is_ended());
                            AlternatePractice { overlay_visible: true }
                        });
                        assert!(!practice.session.is_playing());
                        gameplay_runtime::enter_practice(&mut practice, &mut audio, &sounds, &mut clock);

                        use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
                        use deadsync_core::input::InputSource;
                        use deadsync_input::{InputEvent, VirtualAction};
                        use deadsync_theme::ThemeEffect;

                        let mut effects = Vec::new();
                        let timestamp = std::time::Instant::now();
                        let mut raw_key = |code, pressed| {
                            gameplay_runtime::handle_practice_raw_key::<AlternateGameplayTheme, _, _>(
                                &mut practice,
                                &RawKeyboardEvent { code, pressed, repeat: false, timestamp, host_nanos: 0 },
                                &mut audio, &sounds, &mut clock, &mut effects,
                            )
                        };
                        let previous_session_rate = profile::get_session_music_rate();
                        let shifted = raw_key(KeyCode::ShiftLeft, true);
                        let rate_changed = raw_key(KeyCode::BracketRight, true);
                        let shift_released = raw_key(KeyCode::ShiftLeft, false);
                        let persisted_rate = profile::get_session_music_rate();
                        profile::set_session_music_rate(previous_session_rate);
                        assert!(shifted && rate_changed && shift_released);
                        assert!((persisted_rate - 1.01).abs() < 1e-6);
                        assert_eq!(practice.gameplay.music_rate(), persisted_rate);
                        assert!(effects.is_empty());

                        let input = |action| InputEvent::new(
                            action, 0, true, InputSource::Keyboard, timestamp, 0, timestamp, timestamp,
                        );
                        let editor = practice.session.editor().snapshot();
                        let saved = gameplay_runtime::practice_edit_snapshot(&practice);
                        practice.presentation.overlay_visible = false;
                        gameplay_runtime::handle_practice_input::<AlternateGameplayTheme, _, _>(
                            &mut practice, &input(VirtualAction::p1_menu_down), &mut audio, &sounds,
                            &mut clock, &mut effects,
                        );
                        assert_ne!(practice.session.editor().snapshot().cursor_beat, editor.cursor_beat);
                        gameplay_runtime::restore_practice_edit_snapshot(&mut practice, saved);
                        assert_eq!(practice.session.editor().snapshot(), editor);
                        assert!(practice.presentation.overlay_visible);
                        for action in [VirtualAction::p1_start, VirtualAction::p1_start] {
                            gameplay_runtime::handle_practice_input::<AlternateGameplayTheme, _, _>(
                                &mut practice, &input(action), &mut audio, &sounds, &mut clock, &mut effects,
                            );
                        }
                        assert!(practice.session.is_playing());
                        gameplay_runtime::update_practice(
                            &mut practice, 0.016, &mut audio, &sounds, &mut clock, &mut score_cursor, &mut effects,
                        );
                        assert!(practice.session.is_playing());
                        for action in [
                            VirtualAction::p1_back,
                            VirtualAction::p1_start,
                            VirtualAction::p1_menu_up,
                            VirtualAction::p1_start,
                        ] {
                            gameplay_runtime::handle_practice_input::<AlternateGameplayTheme, _, _>(
                                &mut practice, &input(action), &mut audio, &sounds, &mut clock, &mut effects,
                            );
                        }
                        assert!(!practice.session.is_playing());
                        assert_eq!(effects, vec![ThemeEffect::Navigate("library")]);
                    },
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    fn assert_alternate_stage_presentation(
        state: screen_gameplay::State,
        session: deadsync_gameplay::practice::PracticeSession,
        assets: &deadlib_assets::AssetManager,
    ) {
        let GameplayHost {
            stage,
            lobby_session,
            pack_group,
            score_refresh,
            ..
        } = state;
        let mut alternate = deadsync_gameplay::practice::PracticeHost {
            gameplay: GameplayHost {
                stage,
                lobby_session,
                pack_group,
                score_refresh,
                presentation: AlternateGameplay,
            },
            session,
            presentation: AlternatePractice { overlay_visible: true },
        };
        let gameplay_snapshot = |stage: &ActiveGameplayStage| {
            let player = &stage.players()[0];
            (
                stage.current_music_time_display(),
                player.life,
                player.combo,
                player.earned_grade_points,
            )
        };
        let initial = gameplay_snapshot(&alternate.gameplay.stage);
        let mut actors = Vec::new();
        let segments = crate::gameplay_runtime::push_actors(
            &mut actors,
            &mut alternate.gameplay,
            assets,
            GameplayFrameView::default(),
            123.0,
            (),
        );
        assert!(segments.has_direct_field_proxy(alternate.gameplay.song_frame()));
        assert!(actors.iter().any(|actor| matches!(
            actor,
            Actor::Sprite { z: ALTERNATE_GAMEPLAY_MARKER, .. }
        )));

        actors.clear();
        let segments = crate::gameplay_runtime::push_practice_actors(
            &mut actors,
            &mut alternate,
            assets,
            123.0,
            (),
        );
        assert!(!segments.has_direct_field_proxy(alternate.gameplay.song_frame()));
        assert!(matches!(
            actors.last(),
            Some(Actor::Sprite { z: ALTERNATE_PRACTICE_MARKER, .. })
        ));
        assert_eq!(gameplay_snapshot(&alternate.gameplay.stage), initial);
    }

    fn assert_proxy_note_draws(simfile: &Path, skin: &str, music_time: f32) {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(640.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(640, 480);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new(skin);
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    simfile,
                    GameplayViewport::new(640.0, 480.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, music_time);
                let player = &state.gameplay.song_lua_visuals().player_actors[0];
                assert!(
                    !player.initial_state.visible,
                    "original Player should be hidden"
                );
                assert_eq!(
                    player.initial_state.diffuse[3], 1.0,
                    "UI fades must preserve Player alpha"
                );
                let assets = fixture_assets();
                let mut note_handles = Vec::new();
                state.noteskin_assets.noteskin[0]
                    .as_ref()
                    .expect("fixture noteskin")
                    .for_each_slot(|slot| {
                        note_handles.push(FIXTURE_TEXTURES.texture_handle(slot.texture_key()));
                    });
                let mut actors = Vec::with_capacity(512);
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                let frame = compose::build_passes(
                    segments.segments(state.song_frame(), &actors),
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    10.0,
                    &mut compose::TextLayoutCache::default(),
                    &mut compose::ComposeScratch::default(),
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                let note_instances: u32 = frame
                    .ops
                    .iter()
                    .map(|op| match op {
                        DrawOp::Sprite(run) if note_handles.contains(&run.texture_handle) => {
                            run.instance_count
                        }
                        DrawOp::TexturedMesh(run) if note_handles.contains(&run.texture_handle) => {
                            run.instance_count
                        }
                        _ => 0,
                    })
                    .sum();
                assert!(
                    note_instances > 4,
                    "{skin} proxy emitted {note_instances} noteskin instances; expected four receptors and scrolling notes"
                );
            },
        );
    }

    #[test]
    fn faded_screen_layers_preserve_player_proxy_draws() {
        let simfile = write_fixture(
            "queued-hidden-player",
            generated_pipeline_song_lua_simfile(),
        );
        let lua_dir = simfile
            .parent()
            .expect("fixture song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).expect("create Lua fixture directory");
        fs::write(
            lua_dir.join("default.lua"),
            r#"
local player
return Def.ActorFrame{
    OnCommand=function(self)
        self:sleep(3)
        player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        player:visible(false)
        SCREENMAN:GetTopScreen():GetChild("Underlay"):visible(false)
        for name, layer in pairs(SCREENMAN:GetTopScreen():GetChildren()) do
            if name ~= "PlayerP1" and name ~= "PlayerP2" and name ~= "Underlay"
                and name ~= "SongBackground" and name ~= "SongForeground" then
                layer:smooth(1.5):diffusealpha(0)
            end
        end
    end,
    Def.ActorProxy{
        OnCommand=function(self) self:queuecommand("Set") end,
        SetCommand=function(self) self:SetTarget(player) end,
    },
}
"#,
        )
        .expect("write queued proxy fixture");
        for skin in ["lambda", "cel"] {
            assert_proxy_note_draws(&simfile, skin, 2.5);
        }
    }

    fn write_hidden_score_proxy_fixture(name: &str) -> PathBuf {
        let simfile = write_fixture(name, generated_pipeline_song_lua_simfile());
        let dir = simfile.parent().expect("song directory").join("lua");
        fs::create_dir_all(&dir).expect("Lua directory");
        fs::write(
            dir.join("default.lua"),
            r#"
local player, score
return Def.ActorFrame{
    OnCommand=function(self)
        self:sleep(4)
        local top = SCREENMAN:GetTopScreen()
        player = top:GetChild("PlayerP1")
        player:visible(false)
        local underlay = top:GetChild("Underlay")
        underlay:visible(false)
        score = underlay:GetChild("P1Score")
        score:visible(true):diffusealpha(1)
        top:GetChild("Overlay"):smooth(1.5):diffusealpha(0)
    end,
    Def.ActorProxy{
        OnCommand=function(self) self:queuecommand("Set") end,
        SetCommand=function(self) self:SetTarget(score) end,
    },
    Def.ActorProxy{
        OnCommand=function(self) self:queuecommand("Set") end,
        SetCommand=function(self) self:SetTarget(player) end,
    },
}
"#,
        )
        .expect("score/fade Lua");
        simfile
    }

    #[test]
    fn hidden_score_proxy_keeps_overlay_fade() {
        let simfile = write_hidden_score_proxy_fixture("hidden-score-fade");
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                space::set_current_metrics(space::Metrics::centered(854.0, 480.0));
                space::set_current_window_px(1280, 720);
                let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].display_name = "SystemFooter".into();
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                let assets = fixture_assets();
                let prompt = deadsync_theme_simply_love::i18n::tr(
                    "Gameplay",
                    "ContinueHoldingStartGiveUp",
                );
                let prompt = deadlib_present::font::replace_markers(&prompt);
                for (time, alpha) in [(0.0, 1.0), (0.75, 0.5), (2.5, 0.0), (0.75, 0.5)] {
                    set_fixture_time(&mut state, time);
                    // The exit prompt belongs to ScreenGameplay's Overlay. Keep
                    // its own fade settled while seeking through the Lua tween.
                    let input = &mut state.gameplay.control.exit_input;
                    input.hold_to_exit_key = Some(deadsync_gameplay::HoldToExitKey::Start);
                    input.hold_to_exit_start = Some(
                        std::time::Instant::now() - std::time::Duration::from_secs(1),
                    );
                    let mut actors = Vec::new();
                    let segments = crate::gameplay_runtime::push_actors(
                        &mut actors,
                        &mut state,
                        &assets,
                        screen_gameplay::ActorViewOverride::default(),
                        123.0,
                        deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                    );
                    assert_eq!(segments.direct_field_proxy_count(state.song_frame()), 1);
                    assert!(!actor_tree_has_text(&actors, &state.song().title));
                    if time > 0.0 {
                        assert!(actor_tree_has_text(&actors, "."));
                    }
                    let actual = top_screen_text_draw(&actors, &prompt, 0, 1.0)
                        .map_or(0.0, |(_, alpha)| alpha);
                    assert!(
                        (actual - alpha).abs() < 0.02,
                        "exit prompt fade at {time}: {actual} != {alpha}"
                    );
                    // The system footer and the In child's Event label draw
                    // independently of the song's Overlay.
                    assert_eq!(
                        top_screen_text_draw(&actors, "SystemFooter", 0, 1.0).unwrap().1,
                        1.0,
                    );
                    assert_eq!(top_screen_text_draw(&actors, "EVENT", 0, 1.0).unwrap().1, 1.0);
                }
            },
        );
    }

    #[test]
    fn openitg_hud_uses_live_score_in_lua_proxy() {
        use deadsync_theme::views::GameplayHudMotion;

        let simfile = write_hidden_score_proxy_fixture("openitg-score-proxy");
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                space::set_current_metrics(space::Metrics::centered(854.0, 480.0));
                space::set_current_window_px(1280, 720);
                let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                let mode = deadsync_profile_gameplay::score_display_mode_from_profile(
                    profiles[0].score_display_mode,
                );
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                state.presentation.set_hud(deadsync_theme_openitg::GAMEPLAY_HUD);
                set_fixture_time(&mut state, 0.75);
                let score = state.gameplay.display_gameplay_itg_score_percent(0, mode);
                let expected = format!("{:.2}%", (score * 100.0).trunc() / 100.0);
                let mut actors = Vec::new();
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &fixture_assets(),
                    screen_gameplay::ActorViewOverride {
                        hud_motion: GameplayHudMotion::Leaving { elapsed_seconds: 0.4 },
                        ..Default::default()
                    },
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert_eq!(segments.direct_field_proxy_count(state.song_frame()), 1);
                assert!(actor_tree_has_text(&actors, &expected));
                assert!(!actor_tree_has_text(&actors, &state.song().title));
                assert_eq!(state.gameplay.display_gameplay_itg_score_percent(0, mode), score);
            },
        );
    }

    #[test]
    fn simply_love_and_openitg_produce_identical_gameplay_results() {
        use deadlib_present::actors::SizeSpec;
        use deadsync_core::input::InputSource;
        use deadsync_gameplay::{
            GameplayAction, GameplayAudioSnapshot, GameplayExit, GameplayStreamClockSnapshot,
        };
        use deadsync_input::{InputEvent, VirtualAction};
        use deadsync_profile_gameplay::results::stage_result;
        use deadsync_score::{
            Grade,
            stage_stats::{PlayerStageSummary, all_players_failed},
        };
        use deadsync_theme::ThemeEffect;
        use deadsync_theme_simply_love::SimplyLoveRuntimeRequest;
        use std::time::{Duration, Instant};

        let simfile = write_fixture(
            "theme-gameplay-parity",
            "\
#TITLE:Theme gameplay parity;
#MUSIC:music.wav;
#OFFSET:0;
#BPMS:0=120;
#NOTEDATA:;
#STEPSTYPE:dance-single;
#DIFFICULTY:Challenge;
#METER:5;
#NOTES:
0000
1000
0100
0010
,
1000
0100
0010
0001
,
1000
0100
0010
0001
,
1000
0100
0010
0001
;",
        );
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                space::set_current_metrics(space::Metrics::centered(640.0, 480.0));
                space::set_current_window_px(640, 480);
                let assets = fixture_assets();
                let actions = [
                    VirtualAction::p1_left,
                    VirtualAction::p1_down,
                    VirtualAction::p1_up,
                    VirtualAction::p1_right,
                ];
                let now = Instant::now();
                let epoch = now.checked_sub(Duration::from_secs(30)).unwrap_or(now);
                let host_epoch = 1_000_000_000_000;

                for should_fail in [false, true] {
                    let build = || {
                        build_test_state(
                            &simfile,
                            GameplayViewport::new(640.0, 480.0),
                            GameplaySession::default(),
                            std::array::from_fn(|_| profile_data::Profile {
                                noteskin: profile_data::NoteSkin::new("lambda"),
                                lifemeter_type: profile_data::LifeMeterType::Standard,
                                ..Default::default()
                            }),
                        )
                    };
                    let mut states = [Box::new(build()), Box::new(build())];
                    states[1]
                        .presentation
                        .set_hud(deadsync_theme_openitg::GAMEPLAY_HUD);
                    let chart: Vec<_> = states[0]
                        .notes()
                        .iter()
                        .zip(states[0].note_time_cache_ns())
                        .map(|(note, &time)| (time, note.column))
                        .collect();
                    assert_eq!(chart.len(), 15);

                    let mut previous = chart[0].0 - 60_000_000;
                    for state in &mut states {
                        state.set_current_music_time_ns(previous);
                    }
                    let mut actors = Vec::new();
                    let mut effects = Vec::new();
                    let mut completed = false;
                    let mut tick = |states: &mut [Box<screen_gameplay::State>; 2],
                                    music_ns: i64,
                                    edge: Option<(usize, bool, i64)>,
                                    check_presenters: bool| {
                        let dt = (music_ns - previous) as f32 / 1_000_000_000.0;
                        assert!(dt > 0.0);
                        previous = music_ns;
                        let host_ns = host_epoch + music_ns as u64;
                        let audio = GameplayAudioSnapshot {
                            stream_clock: GameplayStreamClockSnapshot {
                                music_nanos: music_ns,
                                music_seconds_per_second: 1.0,
                                has_music_mapping: true,
                                valid_at: epoch + Duration::from_nanos(music_ns as u64),
                                valid_at_host_nanos: host_ns,
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        let mut completion = [false; 2];
                        for (theme, state) in states.iter_mut().enumerate() {
                            effects.clear();
                            if let Some((column, pressed, captured_ns)) = edge {
                                let timestamp = epoch + Duration::from_nanos(captured_ns as u64);
                                let event = InputEvent {
                                    action: actions[column],
                                    input_slot: column as u32,
                                    pressed,
                                    source: InputSource::Keyboard,
                                    timestamp,
                                    timestamp_host_nanos: host_epoch + captured_ns as u64,
                                    stored_at: timestamp,
                                    emitted_at: timestamp,
                                };
                                screen_gameplay::handle_input(state, &event, &mut effects);
                            }
                            screen_gameplay::update(state, dt, audio, || host_ns, &mut effects);
                            completion[theme] = effects.iter().any(|effect| {
                                matches!(
                                    effect,
                                    ThemeEffect::Runtime(SimplyLoveRuntimeRequest::GameplayAction(
                                        GameplayAction::Navigate(GameplayExit::Complete)
                                            | GameplayAction::NavigateNoFade(
                                                GameplayExit::Complete
                                            )
                                    ))
                                )
                            });
                            assert_eq!(state.current_music_time_ns(), music_ns);
                            assert!(state.stage.take_output().errors.is_empty());

                            actors.clear();
                            let segments = crate::gameplay_runtime::push_actors(
                                &mut actors,
                                state,
                                &assets,
                                Default::default(),
                                host_ns as f64 / 1_000_000_000.0,
                                Default::default(),
                            );
                            assert!(segments.segments(state.song_frame(), &actors).count() > 0);
                            if check_presenters {
                                let openitg_life = actors.iter().any(|actor| matches!(actor,
                                Actor::Sprite { offset, size: [SizeSpec::Px(w), SizeSpec::Px(h)], z: 90, .. }
                                    if *offset == [4.0, 129.0] && *w == 40.0 && *h == 298.0
                            ));
                                assert_eq!(
                                    openitg_life,
                                    theme == 1,
                                    "both concrete HUDs must render"
                                );
                            }
                        }
                        assert_eq!(completion[0], completion[1], "completion at {music_ns}");
                        completed |= completion[0];

                        let [left, right] = states;
                        let judgment = |result: Option<Judgment>| {
                            result.map(|hit| {
                                (
                                    hit.grade,
                                    hit.window,
                                    hit.time_error_music_ns,
                                    hit.miss_because_held,
                                )
                            })
                        };
                        assert_eq!(left.notes().len(), right.notes().len());
                        for (index, (a, b)) in left.notes().iter().zip(right.notes()).enumerate() {
                            assert_eq!(
                                judgment(a.result),
                                judgment(b.result),
                                "note {index} at {music_ns}"
                            );
                        }
                        let l = &left.players()[0];
                        let r = &right.players()[0];
                        assert_eq!(l.life.to_bits(), r.life.to_bits(), "life at {music_ns}");
                        assert_eq!(l.life_history, r.life_history);
                        assert_eq!(l.combo_runs, r.combo_runs);
                        assert_eq!(
                            (l.combo, l.recorded_max_combo, l.miss_combo),
                            (r.combo, r.recorded_max_combo, r.miss_combo)
                        );
                        assert_eq!(
                            (l.is_failing, l.stage_failed, l.fail_time),
                            (r.is_failing, r.stage_failed, r.fail_time)
                        );
                        assert_eq!(l.judgment_counts, r.judgment_counts);
                        assert_eq!(l.earned_grade_points, r.earned_grade_points);
                        assert_eq!(
                            left.song_completed_naturally(),
                            right.song_completed_naturally()
                        );
                    };

                    tick(&mut states, chart[0].0 - 50_000_000, None, true);
                    for (index, &(note_ns, column)) in chart.iter().enumerate() {
                        let offset_ns = match index {
                            0 => -7_000_000,
                            1 => 30_000_000,
                            _ => 0,
                        };
                        // The third note receives an input on the wrong lane in both traces.
                        let hit_column = if index == 2 {
                            (column + 1) % actions.len()
                        } else {
                            column
                        };
                        if !should_fail || index < 3 {
                            let pressed_ns = note_ns + offset_ns;
                            tick(
                                &mut states,
                                pressed_ns + 8_000_000,
                                Some((hit_column, true, pressed_ns)),
                                false,
                            );
                            if index == 2 {
                                assert!(states[0].notes()[index].result.is_none());
                            } else {
                                let hit = states[0].notes()[index]
                                    .result
                                    .expect("input should score this note");
                                let expected = if index == 1 {
                                    JudgeGrade::Excellent
                                } else {
                                    JudgeGrade::Fantastic
                                };
                                assert_eq!(
                                    (hit.grade, hit.time_error_music_ns),
                                    (expected, offset_ns)
                                );
                            }
                            let released_ns = pressed_ns + 25_000_000;
                            tick(
                                &mut states,
                                released_ns + 8_000_000,
                                Some((hit_column, false, released_ns)),
                                false,
                            );
                            assert!(!states[0].lane_pressed(hit_column));
                        }
                        let life_before_miss_scan = states[0].players()[0].life;
                        // The miss scan waits for the widest judgment window, including rolls.
                        tick(&mut states, note_ns + 400_000_000, None, false);
                        if index == 2 {
                            assert_eq!(
                                states[0].notes()[index].result.unwrap().grade,
                                JudgeGrade::Miss
                            );
                            // The next update finalizes the missed row and applies its life change.
                            tick(&mut states, note_ns + 450_000_000, None, false);
                            assert!(states[0].players()[0].life < life_before_miss_scan);
                        }
                    }
                    let finish_ns =
                        states[0].music_end_time_ns().max(chart.last().unwrap().0) + 1_100_000_000;
                    tick(&mut states, finish_ns, None, false);
                    assert!(completed);
                    assert!(states.iter().all(|state| state.song_completed_naturally()));

                    let results = states
                        .each_ref()
                        .map(|state| stage_result(&state.gameplay, Default::default()));
                    let fingerprint = |record: &PlayerStageSummary| {
                        (
                            record.grade,
                            record.score_percent.to_bits(),
                            record.ex_score_percent.to_bits(),
                            record.hard_ex_score_percent.to_bits(),
                            record.earned_grade_points,
                            record.possible_grade_points,
                            record.max_combo,
                            record.judgment_counts,
                            record.notes_hit,
                            record.fail_time,
                        )
                    };
                    let [left, right] = results
                        .each_ref()
                        .map(|result| result.players[0].as_ref().unwrap());
                    assert_eq!(fingerprint(left), fingerprint(right));
                    assert_eq!(left.life_history, right.life_history);
                    assert_eq!(
                        results[0].duration_seconds.to_bits(),
                        results[1].duration_seconds.to_bits()
                    );
                    for result in &results {
                        assert_eq!(all_players_failed(result, [true, false]), should_fail);
                        let record = result.players[0].as_ref().unwrap();
                        assert_eq!(record.grade == Grade::Failed, should_fail);
                        assert_eq!(record.max_combo, if should_fail { 2 } else { 12 });
                        assert_eq!(record.fail_time.is_some(), should_fail);
                        assert!(result.players[1].is_none());
                    }
                }
            },
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "long song-fixture playback and GPU readback regression"]
    fn lake_hold_bodies_draw_under_lua_player_callback() {
        let fixture = song_fixture::from_source("Lake of Lost Nostalgia/Lake of Lost Nostalgia.ssc");
        let simfile = fixture.simfile.clone();
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .expect("capture event loop");
        #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720)),
                )
                .expect("hidden capture window"),
        );
        let backend_type = std::env::var("DEADSYNC_LAKE_BACKEND")
            .unwrap_or_else(|_| "vulkan-wgpu".to_owned())
            .parse::<deadlib_render_core::BackendType>()
            .expect("capture backend");
        let mut backend = deadlib_render::create_backend(
            backend_type,
            window,
            space::Metrics::centered(854.0, 480.0).projection(),
            false,
            deadlib_render_core::PresentModePolicy::Immediate,
            false,
            true,
        )
        .expect("capture backend");
        for skin in ["cel", "lambda"] {
            with_session(
                profile_data::PlayStyle::Single,
                profile_data::PlayerSide::P1,
                true,
                false,
                || {
                    let metrics = space::Metrics::centered(854.0, 480.0);
                    space::set_current_metrics(metrics);
                    space::set_current_window_px(1280, 720);
                    space::set_overscan(0, 0, 0, 0);
                    let mut profiles = [
                        profile_data::Profile::default(),
                        profile_data::Profile::default(),
                    ];
                    profiles[0].noteskin = profile_data::NoteSkin::new(skin);
                    profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                    let mut state = build_test_state(
                        &simfile,
                        GameplayViewport::new(1280.0, 720.0),
                        GameplaySession::default(),
                        profiles,
                    );
                    let index = state
                        .chart_runtime
                        .notes
                        .iter()
                        .position(|note| note.note_type == NoteType::Hold)
                        .expect("Lake hold");
                    let head_ns = state.chart_runtime.note_time_cache_ns[index];
                    let tail_ns =
                        cached_hold_end_time_ns(state.chart_runtime.hold_end_time_cache_ns[index])
                            .expect("hold tail");
                    let column = state.chart_runtime.notes[index].column;
                    let mut assets = fixture_assets();
                    prewarm_gpu_noteskins(&state, &mut assets, &mut backend)
                        .expect("noteskin textures");
                    let mut body_handles = Vec::new();
                    for visual in &state.noteskin_assets.noteskin[0]
                        .as_ref()
                        .expect("noteskin")
                        .hold_columns
                    {
                        for slot in [visual.body_inactive.as_ref(), visual.body_active.as_ref()]
                            .into_iter()
                            .flatten()
                        {
                            body_handles
                                .push(assets.texture_context().texture_handle(slot.texture_key()));
                        }
                    }
                    for held in [false, true] {
                        let now_ns = head_ns + if held { 100_000_000 } else { -100_000_000 };
                        set_fixture_time(&mut state, now_ns as f32 / 1_000_000_000.0);
                        state.set_active_hold(
                            column,
                            held.then_some(deadsync_gameplay::ActiveHold {
                                note_index: index,
                                start_time_ns: head_ns,
                                end_time_ns: tail_ns,
                                note_type: NoteType::Hold,
                                let_go: false,
                                is_pressed: true,
                                life: 1.0,
                                last_update_time_ns: now_ns,
                            }),
                        );
                        let mut frame = compose_fixture_frame_with_textures(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut Vec::new(),
                            &mut compose::TextLayoutCache::default(),
                            &mut compose::ComposeScratch::default(),
                            assets.texture_context(),
                        );
                        let bodies = frame
                            .ops
                            .iter()
                            .filter(|op| match op {
                                DrawOp::Sprite(run) => body_handles.contains(&run.texture_handle),
                                DrawOp::TexturedMesh(run) => {
                                    body_handles.contains(&run.texture_handle)
                                }
                                _ => false,
                            })
                            .count();
                        assert!(
                            bodies > 0,
                            "Lake {skin} held={held}: no hold body reaches the renderer"
                        );
                        backend.request_screenshot();
                        backend
                            .draw(&frame, assets.textures(), false)
                            .expect("draw Lake");
                        let image = backend.capture_frame().expect("Lake pixels");
                        if let Some(output) = std::env::var_os("DEADSYNC_LAKE_CAPTURE_DIR") {
                            let output = PathBuf::from(output);
                            fs::create_dir_all(&output).expect("capture directory");
                            image
                                .save(
                                    output.join(format!(
                                        "lake-hold-{skin}-{backend_type}-{held}.png"
                                    )),
                                )
                                .expect("save capture");
                        }
                        frame.ops.retain(|op| match op {
                            DrawOp::Sprite(run) => !body_handles.contains(&run.texture_handle),
                            DrawOp::TexturedMesh(run) => {
                                !body_handles.contains(&run.texture_handle)
                            }
                            _ => true,
                        });
                        backend.request_screenshot();
                        backend
                            .draw(&frame, assets.textures(), false)
                            .expect("draw without bodies");
                        let without = backend.capture_frame().expect("without body pixels");
                        let changed = image
                            .pixels()
                            .zip(without.pixels())
                            .filter(|(a, b)| a != b)
                            .count();
                        eprintln!("Lake {skin} {backend_type} held={held}: {changed} body pixels");
                        assert!(
                            changed > 0,
                            "Lake {skin} {backend_type} held={held}: composed hold bodies draw no pixels"
                        );
                    }
                    let mut textures = assets.take_textures();
                    backend.dispose_textures(&mut textures);
                },
            );
        }
        backend.cleanup();
    }

    #[test]
    fn f3_stats_stay_fixed_during_song_lua_screen_transforms() {
        std::thread::Builder::new()
            .name("stats-screen-boundary".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                with_session(
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    true,
                    false,
                    || {
                        use deadsync_theme_simply_love::screens::components::shared::stats_overlay;

                        let metrics = space::Metrics::centered(640.0, 480.0);
                        space::set_current_metrics(metrics);
                        space::set_current_window_px(640, 480);
                        space::set_overscan(0, 0, 0, 0);
                        let simfile = write_fixture(
                            "stats-screen-boundary",
                            generated_pipeline_song_lua_simfile(),
                        );
                        let lua_dir = simfile.parent().unwrap().join("lua");
                        fs::create_dir_all(&lua_dir).unwrap();
                        fs::write(lua_dir.join("default.lua"), r#"
local function update_screen()
    local screen = SCREENMAN:GetTopScreen()
    local beat = GAMESTATE:GetSongBeat()
    if beat < 1 then
        screen:xy(0, 0):rotationx(0):rotationy(0):rotationz(0):zoom(1):fov(0)
    else
        screen:xy(210, -95):rotationx(28):rotationy(-35):rotationz(67):fov(70)
        screen:zoom(beat < 3 and 0.45 or 0)
    end
end
return Def.ActorFrame {
    OnCommand=function(self)
        self:sleep(10)
        update_screen()
        self:SetUpdateFunction(update_screen)
    end,
    Def.Quad {
        OnCommand=function(self)
            self:xy(80, 100):zoomto(20, 12):diffuse({0.31, 0.57, 0.83, 1})
        end,
    },
}
"#).unwrap();
                        let assets = fixture_assets();
                        let backend = deadlib_render_core::BackendType::Software;
                        let mut stats = Vec::new();
                        stats_overlay::push(&mut stats, backend, 240.0, 12345, None);
                        assert!(actor_tree_has_text(&stats, &format!("240 FPS\n12345 VPF\n{backend}")));
                        let reference = compose::build_passes(
                            [compose::ActorSegment::new(&stats)].into_iter(),
                            &[],
                            [0.0, 0.0, 0.0, 1.0],
                            &metrics,
                            assets.fonts(),
                            10.0,
                            &mut compose::TextLayoutCache::default(),
                            &mut compose::ComposeScratch::default(),
                            &FIXTURE_TEXTURES,
                            None,
                        );
                        // Text uses textured-mesh batches. Flatten the actual
                        // rendered vertices so batching and the quad's change
                        // from a sprite to a perspective mesh do not matter.
                        let clip_vertices = |frame: &deadlib_render_core::RenderFrame| {
                            let mut vertices = Vec::new();
                            for op in &frame.ops {
                                let (camera_id, texture, blend) = match op {
                                    DrawOp::Sprite(run) => (run.camera, run.texture_handle, run.blend),
                                    DrawOp::Mesh(run) => (run.camera, 0, run.blend),
                                    DrawOp::TexturedMesh(run) => (run.camera, run.texture_handle, run.blend),
                                };
                                let camera = frame.cameras.get(camera_id as usize).copied()
                                    .unwrap_or_else(|| metrics.projection());
                                let mut push = |position: [f32; 4], color: [f32; 4]| {
                                    vertices.push((camera.mul_vec4(position.into()).to_array(),
                                        camera.to_cols_array(), color, texture, blend));
                                };
                                match op {
                                    DrawOp::Sprite(run) => {
                                        for sprite in &frame.sprite_instances[run.instance_start as usize
                                            ..(run.instance_start + run.instance_count) as usize]
                                        {
                                            let [sin, cos] = sprite.rot_sin_cos;
                                            let [offset_sin, offset_cos] = sprite.local_offset_rot_sin_cos;
                                            let [offset_x, offset_y] = sprite.local_offset;
                                            let center_x = sprite.center[0] + offset_cos * offset_x - offset_sin * offset_y;
                                            let center_y = sprite.center[1] + offset_sin * offset_x + offset_cos * offset_y;
                                            for [x, y] in [
                                                [-0.5, -0.5], [0.5, -0.5], [0.5, 0.5],
                                                [-0.5, -0.5], [0.5, 0.5], [-0.5, 0.5],
                                            ] {
                                                let x = x * sprite.size[0];
                                                let y = y * sprite.size[1];
                                                push([center_x + cos * x - sin * y,
                                                    center_y + sin * x + cos * y,
                                                    sprite.center[2], 1.0], sprite.tint);
                                            }
                                        }
                                    }
                                    DrawOp::Mesh(run) => {
                                        for vertex in &frame.mesh_vertices[run.vertex_start as usize
                                            ..(run.vertex_start + run.vertex_count) as usize]
                                        {
                                            push([vertex.pos[0], vertex.pos[1], 0.0, 1.0], vertex.color);
                                        }
                                    }
                                    DrawOp::TexturedMesh(run) => {
                                        let geometry = &frame.tmesh_geometries[run.geometry as usize];
                                        for instance in &frame.tmesh_instances[run.instance_start as usize
                                            ..(run.instance_start + run.instance_count) as usize]
                                        {
                                            for vertex in geometry.vertices.iter() {
                                                let position = instance.transform().mul_vec4(
                                                    [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0].into());
                                                push(position.to_array(), std::array::from_fn(|axis|
                                                    vertex.color[axis] * instance.tint[axis]));
                                            }
                                        }
                                    }
                                }
                            }
                            vertices
                        };
                        let expected = clip_vertices(&reference);
                        assert!(expected.len() > 10 * 6, "real stats must compose more than ten visible glyph quads");
                        for (clip, _, _, _, _) in &expected {
                            assert!(clip.iter().all(|value| value.is_finite()) && clip[3] > 0.0);
                            assert!(clip[0].abs() < clip[3] && clip[1].abs() < clip[3],
                                "stats glyph remains inside the viewport: {clip:?}");
                        }

                        for live in [false, true] {
                            let mut state = build_test_state_with_lua_mode(
                                &simfile,
                                GameplayViewport::new(640.0, 480.0),
                                GameplaySession::default(),
                                std::array::from_fn(|_| profile_data::Profile::default()),
                                "Software",
                                Some(6.0),
                                live,
                            );
                            assert_eq!(state.song_lua.is_some(), live);
                            let mut marker_control = None;
                            for seconds in [0.25, 0.75, 1.75] {
                                set_fixture_time(&mut state, seconds);
                                let mut actors = Vec::new();
                                let segments = crate::gameplay_runtime::push_actors(
                                    &mut actors,
                                    &mut state,
                                    &assets,
                                    screen_gameplay::ActorViewOverride::default(),
                                    123.0,
                                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                                );
                                // Match the application: F3 is appended after
                                // gameplay returns its chart-owned segments.
                                stats_overlay::push(&mut actors, backend, 240.0, 12345, None);
                                let frame = compose::build_passes(
                                    segments.segments(state.song_frame(), &actors),
                                    state.render_targets(),
                                    [0.0, 0.0, 0.0, 1.0],
                                    &metrics,
                                    assets.fonts(),
                                    10.0,
                                    &mut compose::TextLayoutCache::default(),
                                    &mut compose::ComposeScratch::default(),
                                    &FIXTURE_TEXTURES,
                                    Some(state.actor_resources()),
                                );
                                let actual = clip_vertices(&frame);
                                assert!(actual.len() >= expected.len());
                                // F3 is the final, highest-layer text in this
                                // fixture. Compare glyph geometry and resolved
                                // cameras, not the unchanged source Actor XY.
                                assert_eq!(&actual[actual.len() - expected.len()..], expected.as_slice(),
                                    "F3 glyphs moved with TopScreen: live={live}, seconds={seconds}");
                                let marker: Vec<_> = actual.iter().filter(|(_, _, color, _, _)| {
                                    // Native actor colors use eight-bit channels
                                    // in the composed frame.
                                    color.iter().zip([0.31, 0.57, 0.83, 1.0])
                                        .all(|(actual, expected)| (*actual - expected).abs() <= 1.0 / 255.0)
                                }).map(|(clip, _, _, _, _)| *clip).collect();
                                let marker_bounds = (!marker.is_empty()).then(|| {
                                    let mut bounds = [[f32::INFINITY; 4], [f32::NEG_INFINITY; 4]];
                                    for point in &marker {
                                        for axis in 0..4 {
                                            assert!(point[axis].is_finite());
                                            bounds[0][axis] = bounds[0][axis].min(point[axis]);
                                            bounds[1][axis] = bounds[1][axis].max(point[axis]);
                                        }
                                    }
                                    bounds
                                });
                                if seconds == 0.25 {
                                    assert_eq!(marker.len(), 6, "the chart marker must actually render");
                                    marker_control = marker_bounds;
                                } else {
                                    if seconds == 0.75 {
                                        assert_eq!(marker.len(), 6, "the transformed chart marker must still render");
                                    }
                                    assert_ne!(marker_bounds, marker_control,
                                        "the chart must still move/collapse: live={live}, seconds={seconds}");
                                }
                            }
                        }
                    },
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn late_foreground_keeps_shared_gameplay_tracks() {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                for background in [false, true] {
                    let mut text = generated_pipeline_song_lua_simfile()
                        .replace("0.000=lua/default.lua", "1.000=lua/default.lua");
                    if background {
                        text = text.replace(
                            "#NOTEDATA:",
                            "#BGCHANGES:8.000=bg/default.lua=1.000=0=0=0=====;\n#NOTEDATA:",
                        );
                    }
                    let path = write_fixture(
                        if background {
                            "late-shared-background"
                        } else {
                            "late-shared-foreground"
                        },
                        &text,
                    );
                    let dir = path.parent().unwrap();
                    fs::create_dir_all(dir.join("lua")).unwrap();
                    fs::create_dir_all(dir.join("bg")).unwrap();
                    fs::write(
                        dir.join("lua/default.lua"),
                        include_str!("../../../tests/fixtures/song_lua/late-shared-runtime.lua")
                            .replace("self:ztest(1)", "self:ztest(1):sleep(3)"),
                    )
                    .unwrap();
                    fs::write(dir.join("bg/default.lua"), "return Def.ActorFrame{}").unwrap();
                    let profiles = std::array::from_fn(|_| profile_data::Profile::default());
                    let mut state = build_compiled_test_state(
                        &path,
                        GameplayViewport::design(),
                        GameplaySession::default(),
                        profiles,
                    );
                    assert!(
                        !state
                            .gameplay
                            .song_lua_visuals()
                            .overlays
                            .iter()
                            .any(|actor| actor.name.as_deref() == Some("LateForegroundMarker")),
                        "shared gameplay data must not promote timed graphics to the root"
                    );
                    let layers = &state.gameplay.song_lua_visuals().foreground_visual_layers;
                    assert_eq!(layers.len(), 1);
                    let start = state
                        .timing_for_player(0)
                        .unwrap()
                        .get_time_for_beat_exact(1.0);
                    assert!(
                        (layers[0].start_second - start).abs() < 0.001,
                        "late foreground starts at {}, expected {start}",
                        layers[0].start_second
                    );
                    assert!(
                        layers[0]
                            .overlays
                            .iter()
                            .any(|actor| actor.name.as_deref() == Some("LateForegroundMarker"))
                    );
                    for (name, expected) in
                        [("LateForegroundMarker", true), ("LateDepthDisabled", false)]
                    {
                        let actor = layers[0]
                            .overlays
                            .iter()
                            .find(|actor| actor.name.as_deref() == Some(name))
                            .unwrap();
                        assert_eq!(
                            actor.initial_state.depth_test, expected,
                            "recursive depth setter: {name}"
                        );
                    }
                    assert_eq!(
                        layers[0]
                            .overlays
                            .iter()
                            .find(|actor| actor.name.as_deref() == Some("LateDepthDisabled"))
                            .unwrap()
                            .initial_state
                            .blend,
                        deadsync_song_lua::SongLuaOverlayBlendMode::NoEffect
                    );
                    for (second, x, stealth, screen_y, expected_markers) in [
                        (2.5, -9999.0, 1.0, 12.0, 1),
                        (3.5, 213.5, 0.0, 0.0, 1),
                        (3.51, 213.5, 0.0, 0.0, 0),
                        (2.5, -9999.0, 1.0, 12.0, 1),
                    ] {
                        set_fixture_time(&mut state, second);
                        refresh_active_attack_masks(&mut state.gameplay, f32::MAX);
                        assert_eq!(state.gameplay.song_lua_player_transform(0).x, Some(x));
                        assert_eq!(
                            state
                                .gameplay
                                .effective_appearance_effects_for_player(0)
                                .stealth,
                            stealth
                        );
                        let visuals = state.gameplay.song_lua_visuals();
                        assert!(
                            visuals.screen_overlay_index.is_some(),
                            "shared ScreenGameplay pose"
                        );
                        let frame = compose_fixture_frame(
                            &mut state,
                            &fixture_assets(),
                            &metrics,
                            &mut Vec::new(),
                            &mut compose::TextLayoutCache::default(),
                            &mut compose::ComposeScratch::default(),
                        );
                        let mut markers = 0;
                        let mut projected_quads = Vec::new();
                        for op in &frame.ops {
                            let DrawOp::TexturedMesh(run) = op else { continue };
                            if !run.depth_test {
                                continue;
                            }
                            // Zoom can live in either geometry or the instance
                            // transform. Identify the authored 16px height in
                            // projected coordinates.
                            let vertices = &frame.tmesh_geometries[run.geometry as usize].vertices;
                            if vertices.len() != 6 {
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
                                let [top, bottom] = vertices.iter().fold(
                                    [f32::INFINITY, f32::NEG_INFINITY],
                                    |[top, bottom], vertex| {
                                        let [x, y, z] = vertex.pos;
                                        let p = matrix.mul_vec4([x, y, z, 1.0].into());
                                        let y = (1.0 - p.y / p.w) * 240.0;
                                        [top.min(y), bottom.max(y)]
                                    },
                                );
                                projected_quads.push([top, bottom]);
                                if (bottom - top - 16.0).abs() >= 0.02 {
                                    continue;
                                }
                                let y = f32::midpoint(top, bottom);
                                assert!(
                                    (y - 80.0 - screen_y).abs() < 0.01,
                                    "shared screen translation: {y}"
                                );
                                markers += 1;
                            }
                        }
                        assert_eq!(markers, expected_markers, "depth-tested quad bounds at {second}: {projected_quads:?}");
                    }
                }
            },
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "requires a graphics device"]
    fn song_lua_depth_mask_hides_art_then_opens() {
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1600, 900);
                let path = write_fixture("depth-mask-gameplay", generated_pipeline_song_lua_simfile());
                let dir = path.parent().unwrap().join("lua");
                fs::create_dir_all(&dir).unwrap();
                fs::write(
                    dir.join("default.lua"),
                    include_str!("../../../tests/fixtures/song_lua/no-effect-depth-mask.lua"),
                )
                .unwrap();
                let profiles = std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"),
                    ..Default::default()
                });
                let mut state = build_test_state(
                    &path,
                    GameplayViewport::design(),
                    GameplaySession::default(),
                    profiles,
                );
                let mut capture = SongLuaNotePixelCapture::new(
                    &state,
                    &metrics,
                    std::env::temp_dir().join("deadsync-depth-mask-capture"),
                );
                set_fixture_time(&mut state, 2.5);
                let closed = capture.capture(&mut state, &metrics, 5.0);
                assert!(
                    closed > 100,
                    "closed depth mask must preserve visible notes; got {closed} pixels"
                );
                set_fixture_time(&mut state, 3.5);
                let open = capture.capture(&mut state, &metrics, 7.0);
                assert_eq!(open, 0, "opening the mask must reveal the covering artwork");
            },
        );
    }

    #[test]
    #[ignore = "long full-song Mawaru transition and GPU readback regression"]
    fn mawaru7_compiled_notefield_transitions() {
        let fixture = song_fixture::from_source("mawaru7/mawaru7.sm");
        let simfile = fixture.simfile.clone();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1600, 900);
                let profiles = std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"),
                    scroll_speed: ScrollSpeedSetting::MMod(730.0),
                    mini_percent: 60,
                    background_filter: profile_data::BackgroundFilter::from_percent(95),
                    ..Default::default()
                });
                let until_beat = std::env::var("DEADSYNC_MAWARU7_UNTIL_BEAT")
                    .ok()
                    .map(|value| value.parse::<f32>().unwrap());
                let mut state = build_test_state_with_renderer_horizon(
                    &simfile,
                    GameplayViewport::design(),
                    GameplaySession::default(),
                    profiles,
                    "Vulkan (wgpu)",
                    until_beat,
                );
                let layers = &state.gameplay.song_lua_visuals().foreground_visual_layers;
                assert_eq!(layers.len(), 1, "Mawaru's Lua foreground must compile");
                let mask = layers[0]
                    .overlays
                    .iter()
                    .find(|actor| actor.name.as_deref() == Some("actor_yv"))
                    .expect("authored door mask");
                assert_eq!(
                    mask.initial_state.blend,
                    deadsync_song_lua::SongLuaOverlayBlendMode::NoEffect
                );
                let sky = layers[0]
                    .overlays
                    .iter()
                    .find(|actor| actor.name.as_deref() == Some("actor_zv"))
                    .expect("authored door sky");
                assert!(
                    sky.initial_state.depth_test,
                    "door sky must receive its ActorFrame's depth test"
                );
                assert!(
                    !sky.initial_state.depth_write,
                    "testing must not enable writing"
                );
                assert!(
                    mask.initial_state.depth_write,
                    "the mask explicitly writes depth"
                );
                let assets = fixture_assets();
                let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
                let mut handles = Vec::new();
                ns.for_each_slot(|slot| {
                    handles.push(FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                });
                #[cfg(target_os = "windows")]
                let mut gpu = Some(SongLuaNotePixelCapture::new(
                    &state,
                    &metrics,
                    std::env::var_os("DEADSYNC_MAWARU7_CAPTURE")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| std::env::temp_dir().join("deadsync-mawaru7-capture")),
                ));
                let mut tick = 0;
                for beat in [8.0, 70.0, 94.0, 116.0, 196.0, 308.0, 320.0, 338.0] {
                    if until_beat.is_some_and(|end| beat > end) {
                        break;
                    }
                    let seconds = state
                        .gameplay
                        .gameplay_chart(0)
                        .unwrap()
                        .timing
                        .get_time_for_beat_exact(beat);
                    while tick < (seconds * 120.0) as u32 {
                        tick += 1;
                        set_fixture_time(&mut state, tick as f32 / 120.0);
                        refresh_active_attack_masks(&mut state.gameplay, 1.0 / 120.0);
                    }
                    set_fixture_time(&mut state, seconds);
                    state
                        .stage
                        .gameplay
                        .refresh_live_notefield_options(state.current_bpm_display());
                    let frame = compose_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut Vec::new(),
                        &mut compose::TextLayoutCache::default(),
                        &mut compose::ComposeScratch::default(),
                    );
                    let count: u32 = frame
                        .ops
                        .iter()
                        .filter_map(|op| match op {
                            DrawOp::Sprite(run) if handles.contains(&run.texture_handle) => {
                                Some(run.instance_count)
                            }
                            DrawOp::TexturedMesh(run) if handles.contains(&run.texture_handle) => {
                                Some(run.instance_count)
                            }
                            _ => None,
                        })
                        .sum();
                    let pose = state.gameplay.song_lua_player_transform(0);
                    let appearance = state.gameplay.effective_appearance_effects_for_player(0);
                    if beat == 94.0 {
                        assert_eq!(appearance.stealth, 0.0);
                        assert!(count > 4, "rhythm section has tap geometry");
                    }
                    eprintln!(
                        "Mawaru7 beat {beat}: noteskin draws={count}, player_x={:?}, player_zoom={}, stealth={}",
                        pose.x, pose.zoom_x, appearance.stealth
                    );
                    #[cfg(target_os = "windows")]
                    if let Some(gpu) = &mut gpu {
                        if [70.0, 94.0, 116.0, 196.0, 308.0, 320.0, 338.0].contains(&beat) {
                            let visible_pixels = gpu.capture(&mut state, &metrics, beat);
                            if beat == 94.0 {
                                assert!(visible_pixels > 100, "rhythm section has no visible notes");
                            }
                        }
                    }
                }
            },
        );
    }

    #[cfg(target_os = "windows")]
    struct SongLuaNotePixelCapture {
        backend: deadlib_render::Backend,
        assets: deadlib_assets::AssetManager,
        _event_loop: winit::event_loop::EventLoop<()>,
        output: PathBuf,
    }

    #[cfg(target_os = "windows")]
    impl SongLuaNotePixelCapture {
        fn new(state: &screen_gameplay::State, metrics: &space::Metrics, output: PathBuf) -> Self {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            let event_loop = winit::event_loop::EventLoop::builder()
                .with_any_thread(true)
                .build()
                .unwrap();
            #[expect(deprecated, reason = "hidden GPU regression needs no event dispatch")]
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(1600, 900)),
                    )
                    .unwrap(),
            );
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
            prewarm_gpu_noteskins(state, &mut assets, &mut backend).unwrap();
            // Lua sprites bind and Models check texture availability during actor
            // construction, before the compositor can discover missing keys.
            let visuals = state.gameplay.song_lua_visuals();
            for overlay in visuals.overlays.iter().chain(
                visuals
                    .background_visual_layers
                    .iter()
                    .chain(&visuals.foreground_visual_layers)
                    .flat_map(|layer| &layer.overlays),
            ) {
                match &overlay.kind {
                    deadsync_assets::song_lua::SongLuaOverlayKind::Sprite {
                        texture_key,
                        textures,
                        ..
                    } => {
                        for key in std::iter::once(texture_key)
                            .chain(textures.iter().map(|texture| &texture.key))
                        {
                            if !deadlib_assets::dynamic::is_dynamic_video_path(Path::new(key.as_ref())) {
                                ensure_gpu_capture_texture(&mut assets, &mut backend, key, None).unwrap();
                            }
                        }
                    }
                    deadsync_assets::song_lua::SongLuaOverlayKind::Model { layers } => {
                        for layer in layers.iter() {
                            for key in std::iter::once(&layer.texture_key)
                                .chain(layer.additive.as_ref())
                                .chain(
                                    layer.texture_frames.iter()
                                        .chain(layer.additive_frames.iter())
                                        .filter_map(|frame| frame.texture_key.as_ref()),
                                )
                            {
                                ensure_gpu_capture_texture(
                                    &mut assets,
                                    &mut backend,
                                    key,
                                    Some(deadsync_assets::textures::model_texture_sampler(key)),
                                ).unwrap();
                            }
                        }
                    }
                    _ => {}
                }
            }
            fs::create_dir_all(&output).unwrap();
            Self {
                backend,
                assets,
                _event_loop: event_loop,
                output,
            }
        }

        fn capture(
            &mut self,
            state: &mut screen_gameplay::State,
            metrics: &space::Metrics,
            beat: f32,
        ) -> usize {
            self.capture_named(state, metrics, beat, "mawaru7")
        }

        fn capture_named(
            &mut self,
            state: &mut screen_gameplay::State,
            metrics: &space::Metrics,
            beat: f32,
            name: &str,
        ) -> usize {
            self.capture_named_at_game_time(state, metrics, beat, name, None)
        }

        fn capture_named_at_game_time(
            &mut self,
            state: &mut screen_gameplay::State,
            metrics: &space::Metrics,
            beat: f32,
            name: &str,
            game_time: Option<f64>,
        ) -> usize {
            let textures = CaptureTextureContext {
                assets: &self.assets,
                keys: Default::default(),
            };
            let frame = compose_fixture_frame_with_view_and_clock(
                state,
                &self.assets,
                metrics,
                &mut Vec::new(),
                &mut compose::TextLayoutCache::default(),
                &mut compose::ComposeScratch::default(),
                screen_gameplay::ActorViewOverride::default(),
                &textures,
                game_time,
            );
            let missing = missing_capture_texture_keys(&textures, &self.assets);
            drop(frame);
            drop(textures);
            for key in missing {
                ensure_gpu_capture_texture(&mut self.assets, &mut self.backend, &key, None).unwrap();
            }
            let mut frame = compose_fixture_frame_with_view_and_clock(
                state,
                &self.assets,
                metrics,
                &mut Vec::new(),
                &mut compose::TextLayoutCache::default(),
                &mut compose::ComposeScratch::default(),
                screen_gameplay::ActorViewOverride::default(),
                self.assets.texture_context(),
                game_time,
            );
            let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
            let taps: std::collections::HashSet<_> = ns
                .notes
                .iter()
                .chain(ns.note_layers.iter().flat_map(|layers| layers.iter()))
                .chain(ns.lift_note_layers.iter().flat_map(|layers| layers.iter()))
                .map(|slot| {
                    self.assets
                        .texture_context()
                        .texture_handle(slot.texture_key())
                })
                .collect();
            self.backend.request_screenshot();
            self.backend
                .draw(&frame, self.assets.textures(), false)
                .unwrap();
            let image = self.backend.capture_frame().unwrap();
            image
                .save(self.output.join(format!("{name}-beat-{beat}.png")))
                .unwrap();
            if name == "mawaru7" && [308.0, 320.0].contains(&beat) {
                let visuals = state.gameplay.song_lua_visuals();
                let titan: std::collections::HashSet<_> = visuals
                    .foreground_visual_layers
                    .iter()
                    .flat_map(|layer| &layer.overlays)
                    .filter_map(|overlay| match &overlay.kind {
                        deadsync_assets::song_lua::SongLuaOverlayKind::Sprite {
                            texture_key,
                            ..
                        } if texture_key.contains("games/titan/")
                            && !texture_key.contains("/skyp")
                            && !texture_key.ends_with("/note.png") =>
                        {
                            Some(self.assets.texture_context().texture_handle(texture_key))
                        }
                        _ => None,
                    })
                    .collect();
                assert!(!titan.is_empty(), "Titan scene image resources");
                let mut without_art = frame.clone();
                without_art.ops.retain(|op| match op {
                    DrawOp::Sprite(run) => !titan.contains(&run.texture_handle),
                    DrawOp::TexturedMesh(run) => !titan.contains(&run.texture_handle),
                    _ => true,
                });
                self.backend.request_screenshot();
                self.backend
                    .draw(&without_art, self.assets.textures(), false)
                    .unwrap();
                let no_art = self.backend.capture_frame().unwrap();
                let changed = image
                    .pixels()
                    .zip(no_art.pixels())
                    .filter(|(a, b)| a != b)
                    .count();
                eprintln!(
                    "Mawaru7 GPU beat {beat}: {changed} visible Titan scenery/character pixels"
                );
                assert!(
                    changed > 1_000,
                    "Titan scenery and characters are hidden behind the sky"
                );
            }
            let without_taps = |op: &DrawOp| match op {
                DrawOp::Sprite(run) => !taps.contains(&run.texture_handle),
                DrawOp::TexturedMesh(run) => !taps.contains(&run.texture_handle),
                _ => true,
            };
            frame.ops.retain(without_taps);
            for target in &mut frame.render_targets {
                target.ops.retain(without_taps);
            }
            self.backend.request_screenshot();
            self.backend
                .draw(&frame, self.assets.textures(), false)
                .unwrap();
            let no_taps = self.backend.capture_frame().unwrap();
            let changed = image
                .pixels()
                .zip(no_taps.pixels())
                .filter(|(a, b)| a != b)
                .count();
            if name == "dyscontrolled" {
                // Native pad captures place the landing taps in this design-space
                // region. Exclude flying arrows that could survive while the pad
                // hides the landing taps.
                let [left, right] = [360.0, 470.0]
                    .map(|x| (x * image.width() as f32 / 854.0) as u32);
                let [top, bottom] = [150.0, 260.0]
                    .map(|y| (y * image.height() as f32 / 480.0) as u32);
                let landing_pixels = image
                    .enumerate_pixels()
                    .zip(no_taps.pixels())
                    .filter(|((x, y, a), b)| {
                        (left..right).contains(x) && (top..bottom).contains(y) && a != b
                    })
                    .count();
                eprintln!("Dyscontrolled GPU beat {beat}: {landing_pixels} tap pixels over the pad");
                assert!(
                    landing_pixels > 100,
                    "landing arrows must remain visible above the pad at beat {beat}"
                );
            }
            eprintln!("{name} GPU beat {beat}: {changed} visible tap pixels");
            changed
        }
    }

    #[cfg(target_os = "windows")]
    impl Drop for SongLuaNotePixelCapture {
        fn drop(&mut self) {
            let mut textures = self.assets.take_textures();
            self.backend.dispose_textures(&mut textures);
            self.backend.cleanup();
        }
    }

    #[test]
    #[ignore = "long full-song 7th Gear native geometry regression"]
    fn seventh_saved_mini_reaches_aft_receptors() {
        let fixture = song_fixture::from_source("7th Gear/7th Gear.ssc");
        let simfile = fixture.simfile.clone();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                let profiles = std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"),
                    scroll_speed: ScrollSpeedSetting::MMod(730.0),
                    mini_percent: 60,
                    ..Default::default()
                });
                // OpenGL selects the song's real three-AFT branch during loading.
                let mut state = build_test_state_with_renderer(
                    &simfile,
                    GameplayViewport::design(),
                    GameplaySession::default(),
                    profiles,
                    "OpenGL",
                );
                let assets = fixture_assets();
                let receptor_handles: Vec<_> = state.noteskin_assets.receptor_noteskin[0]
                    .as_ref()
                    .or(state.noteskin_assets.noteskin[0].as_ref())
                    .unwrap()
                    .receptor_off
                    .iter()
                    .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                    .collect();
                let mut probes = vec![
                    (112.0, Some(115.0), None),
                    (115.0, Some(385.0), None),
                    (123.0, Some(115.0), None),
                ];
                let native: serde_json::Value = serde_json::from_slice(&zstd::stream::decode_all(
                    include_bytes!("../../../tests/fixtures/itgmania-song-lua-micro/seventh-current-704cdd-native.json.zst").as_slice(),
                ).unwrap()).unwrap();
                assert_eq!(native["oracle"], "itgmania_notefield_render_trace");
                assert_eq!(native["input"]["window_visible"], false);
                assert_eq!(native["input"]["native_source"]["revision"], "704cdd5624155f743b59b8b64fedeb0588dae067");
                assert_eq!(native["input"]["native_source"]["modified_src"], false);
                let native_project = |clip: &[f64]| {
                    [
                        ((1.0 + clip[0] / clip[3]) * 427.0) as f32,
                        ((1.0 - clip[1] / clip[3]) * 240.0) as f32,
                    ]
                };
                let mut seen_native_beats = std::collections::HashSet::new();
                let mut native_arrivals = Vec::new();
                for sample in native["model_geometry_samples"].as_array().unwrap() {
                    let context = &sample["state"]["note_context"];
                    if context["kind"] != "actor" || context["note_beat"] != 130.75
                        || context["column"] != 3 || sample["state"]["mesh_index"] != 0
                        // Native also draws the original Player to target zero.
                        // Match the submitted AFT geometry instead of that pass.
                        || sample["state"]["render_target"].as_u64().unwrap() == 0
                    {
                        continue;
                    }
                    let beat = context["draw_beat"].as_f64().unwrap() as f32;
                    if beat < 130.2 { continue; }
                    if !seen_native_beats.insert(beat.to_bits()) {
                        continue;
                    }
                    let anchors: Vec<Vec<f64>> =
                        serde_json::from_value(sample["state"]["anchors_clip"].clone()).unwrap();
                    let origin = native_project(&anchors[0]);
                    let axis = native_project(&anchors[1]);
                    let scale = ((axis[0] - origin[0]).powi(2) + (axis[1] - origin[1]).powi(2)).sqrt();
                    let geometry = native["model_geometry_meshes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|mesh| mesh["id"] == sample["geometry"])
                        .unwrap();
                    let vertices: Vec<[f64; 3]> =
                        serde_json::from_value(geometry["vertices"].clone()).unwrap();
                    let mut bounds = [
                        f32::INFINITY,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    ];
                    let mut native_points = Vec::new();
                    for vertex in vertices {
                        let clip: Vec<f64> = (0..4)
                            .map(|axis| {
                                anchors[0][axis]
                                    + (0..3)
                                        .map(|component| {
                                            vertex[component]
                                                * (anchors[component + 1][axis] - anchors[0][axis])
                                        })
                                        .sum::<f64>()
                            })
                            .collect();
                        let point = native_project(&clip);
                        native_points.push(point);
                        for axis in 0..2 {
                            bounds[axis] = bounds[axis].min(point[axis]);
                            bounds[axis + 2] = bounds[axis + 2].max(point[axis]);
                        }
                    }
                    let center: [f32; 2] = std::array::from_fn(|axis| {
                        native_points.iter().map(|point| point[axis]).sum::<f32>()
                            / native_points.len() as f32
                    });
                    for point in &mut native_points {
                        for axis in 0..2 {
                            point[axis] = (point[axis] - center[axis]) / scale;
                        }
                    }
                    native_arrivals.push(
                        serde_json::json!({"beat":beat,"note_beat":130.75,"column":4,
                        "origin":origin,"scale":scale,"normalized_points":native_points,
                        "normalized_extent":[(bounds[2]-bounds[0])/scale,(bounds[3]-bounds[1])/scale]}),
                    );
                }
                assert!(
                    native_arrivals.len() >= 20,
                    "consecutive native rendered first-arrow frames"
                );
                for frame in &native_arrivals {
                    probes.push((frame["beat"].as_f64().unwrap() as f32, None, None));
                }
                let mut native_caps = Vec::new();
                let mut seen_cap_beats = std::collections::HashSet::new();
                for sample in native["drawable_samples"].as_array().unwrap() {
                    let context = &sample["state"]["note_context"];
                    let beat = context["draw_beat"].as_f64().unwrap() as f32;
                    if context["part"] != 2 || sample["state"]["render_target"] == 0
                        || !((121.91..121.99).contains(&beat) || (129.91..129.99).contains(&beat))
                        || !seen_cap_beats.insert(beat.to_bits()) {
                        continue;
                    }
                    let tail_beat = context["note_beat"].as_f64().unwrap() as f32;
                    let column = context["column"].as_u64().unwrap() as usize;
                    let index = state.chart_runtime.notes.iter().position(|note|
                        note.column == column && note.hold.as_ref().is_some_and(|hold| hold.end_beat == tail_beat)
                    ).unwrap();
                    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
                    for vertex in sample["vertices"].as_array().unwrap() {
                        let clip: Vec<f64> = vertex.as_array().unwrap()[3..7].iter().map(|v| v.as_f64().unwrap()).collect();
                        let point = native_project(&clip);
                        for axis in 0..2 {
                            bounds[axis] = bounds[axis].min(point[axis]);
                            bounds[axis + 2] = bounds[axis + 2].max(point[axis]);
                        }
                    }
                    native_caps.push((beat, bounds));
                    probes.push((beat, None, Some(index)));
                }
                assert!(native_caps.iter().any(|(beat, _)| *beat < 122.0));
                assert!(native_caps.iter().any(|(beat, _)| *beat > 129.0));
                let timing = &state.gameplay.gameplay_chart(0).unwrap().timing;
                let tracks = &state.gameplay.song_lua_visuals().column_splines[0];
                let mut smallest_scale = (f32::INFINITY, 0.0f32, 0usize);
                // Inspect real incoming notes at every compiled swap frame,
                // rather than only the receptor at three steady-state beats.
                for track in tracks {
                    for frame in track.frames.iter() {
                        let beat = timing.get_beat_for_time(frame.second + track.time_offset);
                        if !(111.5..174.5).contains(&beat) {
                            continue;
                        }
                        let Some(zoom) = &frame.zoom else {
                            continue;
                        };
                        for (index, note) in state.chart_runtime.notes.iter().enumerate() {
                            if note.column != track.column as usize
                                || note.beat < beat
                                || note.beat > beat + 2.0
                                || !matches!(
                                    note.note_type,
                                    NoteType::Tap | NoteType::Hold | NoteType::Roll
                                )
                            {
                                continue;
                            }
                            let scale = zoom.view().sample(beat, note.beat).0[0];
                            if scale < smallest_scale.0 {
                                smallest_scale = (scale, beat, index);
                            }
                        }
                    }
                }
                assert!(smallest_scale.0.is_finite(), "real incoming swap notes");
                eprintln!(
                    "7th Gear incoming note {} at beat {}: spline scale {} at song beat {}",
                    smallest_scale.2,
                    state.chart_runtime.notes[smallest_scale.2].beat,
                    smallest_scale.0,
                    smallest_scale.1
                );
                probes.push((smallest_scale.1, None, None));
                for beat in [114.45, 114.52, 122.45, 122.52, 122.7, 130.5] {
                    probes.push((beat, None, None));
                }
                let mut hold_directions = [false; 2];
                for (index, note) in state.chart_runtime.notes.iter().enumerate() {
                    let Some(hold) = &note.hold else {
                        continue;
                    };
                    let beat = (note.beat + hold.end_beat) * 0.5;
                    if !(112.0..174.0).contains(&beat) {
                        continue;
                    }
                    let track = tracks
                        .iter()
                        .find(|track| track.column as usize == note.column)
                        .unwrap();
                    let frame = track
                        .at_second(timing.get_time_for_beat_exact(beat))
                        .unwrap();
                    let Some(position) = &frame.position else {
                        continue;
                    };
                    let derivative = position.view().sample(beat, beat).1;
                    let reversed = usize::from(derivative[1] < 0.0);
                    if derivative[1] == 0.0 || hold_directions[reversed] {
                        continue;
                    }
                    hold_directions[reversed] = true;
                    probes.push((beat, None, Some(index)));
                }
                assert_eq!(
                    hold_directions, [true; 2],
                    "actual holds in both spline directions"
                );
                probes.sort_by(|a, b| a.0.total_cmp(&b.0));
                let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
                let body_handles: Vec<_> = ns
                    .hold_columns
                    .iter()
                    .chain(&ns.roll_columns)
                    .flat_map(|column| [column.body_active.as_ref(), column.body_inactive.as_ref()])
                    .flatten()
                    .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                    .collect();
                let tap_handles: std::collections::HashSet<_> = ns
                    .notes
                    .iter()
                    .chain(ns.note_layers.iter().flat_map(|layers| layers.iter()))
                    .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                    .collect();
                let cap_handles: std::collections::HashSet<_> = ns.hold_columns.iter()
                    .flat_map(|column| [column.bottomcap_active.as_ref(), column.bottomcap_inactive.as_ref()])
                    .flatten().map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key())).collect();
                #[cfg(target_os = "windows")]
                let mut capture = std::env::var_os("DEADSYNC_SEVENTH_CAPTURE").map(|_| {
                    SongLuaNotePixelCapture::new(
                        &state,
                        &metrics,
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../../target/seventh-swap-geometry"),
                    )
                });
                let mut tick = 0;
                let mut native_pose_failures = Vec::new();
                let mut native_pose_errors = Vec::new();
                for (beat, expected_y, active_hold) in probes {
                    let seconds = state
                        .gameplay
                        .gameplay_chart(0)
                        .unwrap()
                        .timing
                        .get_time_for_beat_exact(beat);
                    while tick < (seconds * 120.0) as u32 {
                        tick += 1;
                        set_fixture_time(&mut state, tick as f32 / 120.0);
                        refresh_active_attack_masks(&mut state.gameplay, 1.0 / 120.0);
                    }
                    set_fixture_time(&mut state, seconds);
                    state.clear_active_holds();
                    let native_arrival = native_arrivals
                        .iter()
                        .find(|frame| frame["beat"].as_f64().unwrap() as f32 == beat);
                    let native_cap = native_caps.iter().find(|(at, _)| *at == beat).map(|(_, bounds)| bounds);
                    let arrival_note = native_arrival.map(|frame| {
                        let note_beat = frame["note_beat"].as_f64().unwrap() as f32;
                        let column = frame["column"].as_u64().unwrap() as usize - 1;
                        state
                            .chart_runtime
                            .notes
                            .iter()
                            .position(|note| {
                                note.beat == note_beat
                                    && note.column == column
                                    && note.note_type == NoteType::Tap
                            })
                            .expect("native first incoming tap")
                    });
                    // Recreate autoplay draw visibility after seeking: past
                    // taps and completed holds must not leave clamped fragments.
                    // Isolate the selected hold for its width assertion.
                    for (index, note) in state.chart_runtime.notes.iter_mut().enumerate() {
                        note.result = (arrival_note
                            .map_or(note.beat < beat, |selected| selected != index))
                        .then_some(Judgment {
                            time_error_ms: 0.0,
                            time_error_music_ns: 0,
                            grade: JudgeGrade::Fantastic,
                            window: Some(TimingWindow::W1),
                            miss_because_held: false,
                        });
                        if let Some(hold) = &mut note.hold {
                            hold.result = (arrival_note.is_some()
                                || hold.end_beat < beat
                                || active_hold.is_some_and(|selected| selected != index))
                            .then_some(deadsync_rules::note::HoldResult::Held);
                        }
                    }
                    {
                        let runtime = &mut state.chart_runtime;
                        for row in &mut runtime.row_entries {
                            row.final_outcome = row
                                .note_indices()
                                .iter()
                                .all(|index| runtime.notes[index.get()].result.is_some())
                                .then_some(deadsync_gameplay::FinalizedRowOutcome {
                                    final_grade: JudgeGrade::Fantastic,
                                });
                        }
                    }
                    if let Some(index) = active_hold {
                        let note = &state.chart_runtime.notes[index];
                        let column = note.column;
                        let hold = deadsync_gameplay::ActiveHold {
                            note_index: index,
                            start_time_ns: state.chart_runtime.note_time_cache_ns[index],
                            end_time_ns: cached_hold_end_time_ns(
                                state.chart_runtime.hold_end_time_cache_ns[index],
                            )
                            .unwrap(),
                            note_type: note.note_type,
                            let_go: false,
                            is_pressed: true,
                            life: 1.0,
                            last_update_time_ns: deadsync_core::song_time::song_time_ns_from_seconds(
                                seconds,
                            ),
                        };
                        state.set_active_hold(column, Some(hold));
                    }
                    state.gameplay.refresh_live_notefield_options(124.0);
                    assert!(
                        (state.field_zoom_for_player(0) - 0.7).abs() < 0.001,
                        "saved Mini was applied twice at beat {beat}"
                    );
                    let frame = compose_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut Vec::new(),
                        &mut compose::TextLayoutCache::default(),
                        &mut compose::ComposeScratch::default(),
                    );
                    // Update explicitly draws these AFTs, and their visible
                    // actors draw them again: six passes, three textures.
                    let targets: std::collections::HashSet<_> = frame
                        .render_targets
                        .iter()
                        .map(|pass| pass.texture_handle)
                        .collect();
                    assert_eq!(targets.len(), 3, "actual AFT chain");
                    let mut receptors = 0;
                    let mut wide_bodies = 0;
                    let mut arrival_meshes = 0;
                    let mut cap_meshes = 0;
                    for pass in &frame.render_targets {
                        for op in &pass.ops {
                            if let Some(expected) = native_cap
                                && let DrawOp::TexturedMesh(run) = op
                                && cap_handles.contains(&run.texture_handle) {
                                let camera = pass.cameras.get(run.camera as usize).copied().unwrap_or_else(|| metrics.projection());
                                for instance in &pass.tmesh_instances[run.instance_start as usize..(run.instance_start + run.instance_count) as usize] {
                                    let transform = camera * instance.transform();
                                    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
                                    for vertex in pass.tmesh_geometries[run.geometry as usize].vertices.iter() {
                                        let clip = transform.mul_vec4([vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0].into());
                                        let point = [427.0 + clip.x / clip.w * 427.0,
                                            240.0 - clip.y / clip.w * pass.viewport[1] as f32 * 427.0 / pass.viewport[0] as f32];
                                        for axis in 0..2 {
                                            bounds[axis] = bounds[axis].min(point[axis]);
                                            bounds[axis + 2] = bounds[axis + 2].max(point[axis]);
                                        }
                                    }
                                    for axis in 0..4 {
                                        assert!((bounds[axis] - expected[axis]).abs() < 0.05,
                                            "native cap at beat {beat}, bound {axis}: {bounds:?} vs {expected:?}");
                                    }
                                    cap_meshes += 1;
                                }
                            }
                            if let Some(native) = native_arrival
                                && let DrawOp::TexturedMesh(run) = op
                                && tap_handles.contains(&run.texture_handle)
                            {
                                let camera = pass
                                    .cameras
                                    .get(run.camera as usize)
                                    .copied()
                                    .unwrap_or_else(|| metrics.projection());
                                for instance in &pass.tmesh_instances[run.instance_start as usize
                                    ..(run.instance_start + run.instance_count) as usize]
                                {
                                    let transform = camera * instance.transform();
                                    let project = |p: [f32; 4]| {
                                        let clip = transform.mul_vec4(p.into());
                                        [
                                            427.0 + clip.x / clip.w * 427.0,
                                            240.0
                                                - clip.y / clip.w * pass.viewport[1] as f32 * 427.0
                                                    / pass.viewport[0] as f32,
                                        ]
                                    };
                                    let origin = project([0.0, 0.0, 0.0, 1.0]);
                                    let along_x = project([1.0, 0.0, 0.0, 1.0]);
                                    let scale = ((along_x[0] - origin[0]).powi(2)
                                        + (along_x[1] - origin[1]).powi(2))
                                    .sqrt();
                                    let expected_scale = native["scale"].as_f64().unwrap() as f32;
                                    if arrival_meshes == 0 {
                                        native_pose_errors.push((beat, [
                                            origin[0] - native["origin"][0].as_f64().unwrap() as f32,
                                            origin[1] - native["origin"][1].as_f64().unwrap() as f32,
                                            scale - expected_scale,
                                        ]));
                                    }
                                    if (scale - expected_scale).abs() >= 0.021 {
                                        native_pose_failures.push(format!("native rendered first arrow size at {beat}: {scale} vs {expected_scale}"));
                                    }
                                    for axis in 0..2 {
                                        let expected = native["origin"][axis].as_f64().unwrap() as f32;
                                        if (origin[axis] - expected).abs() >= 4.0 {
                                            native_pose_failures.push(format!("native rendered first arrow position at {beat}: {origin:?}, axis {axis}={expected}"));
                                        }
                                    }
                                    let vertices = pass.tmesh_geometries[run.geometry as usize]
                                        .vertices
                                        .as_ref();
                                    let local_width = vertices
                                        .iter()
                                        .map(|v| v.pos[0])
                                        .fold(f32::NEG_INFINITY, f32::max)
                                        - vertices
                                            .iter()
                                            .map(|v| v.pos[0])
                                            .fold(f32::INFINITY, f32::min);
                                    if local_width > 60.0 {
                                        let mut bounds = [
                                            f32::INFINITY,
                                            f32::INFINITY,
                                            f32::NEG_INFINITY,
                                            f32::NEG_INFINITY,
                                        ];
                                        let mut actual_points = Vec::new();
                                        for vertex in vertices {
                                            let point = project([
                                                vertex.pos[0],
                                                vertex.pos[1],
                                                vertex.pos[2],
                                                1.0,
                                            ]);
                                            actual_points.push(point);
                                            for axis in 0..2 {
                                                bounds[axis] = bounds[axis].min(point[axis]);
                                                bounds[axis + 2] = bounds[axis + 2].max(point[axis]);
                                            }
                                        }
                                        let center: [f32; 2] = std::array::from_fn(|axis| {
                                            actual_points.iter().map(|point| point[axis]).sum::<f32>()
                                                / actual_points.len() as f32
                                        });
                                        for point in &mut actual_points {
                                            for axis in 0..2 {
                                                point[axis] = (point[axis] - center[axis]) / scale;
                                            }
                                        }
                                        let expected_points: Vec<[f32; 2]> =
                                            serde_json::from_value(native["normalized_points"].clone())
                                                .unwrap();
                                        let distance = |from: &[[f32; 2]], to: &[[f32; 2]]| {
                                            from.iter()
                                                .map(|a| {
                                                    to.iter()
                                                        .map(|b| {
                                                            ((a[0] - b[0]).powi(2)
                                                                + (a[1] - b[1]).powi(2))
                                                            .sqrt()
                                                        })
                                                        .fold(f32::INFINITY, f32::min)
                                                })
                                                .fold(0.0, f32::max)
                                        };
                                        let error = distance(&actual_points, &expected_points)
                                            .max(distance(&expected_points, &actual_points));
                                        assert!(
                                            error < 0.2,
                                            "native rendered 3D vertex cloud at {beat}: {error} model units"
                                        );
                                        for axis in 0..2 {
                                            let extent = (bounds[axis + 2] - bounds[axis]) / scale;
                                            let expected =
                                                native["normalized_extent"][axis].as_f64().unwrap()
                                                    as f32;
                                            assert!(
                                                (extent - expected).abs() < 0.35,
                                                "native rendered 3D arrow extent at {beat}, axis {axis}: {extent} vs {expected}"
                                            );
                                        }
                                    }
                                    arrival_meshes += 1;
                                }
                            }
                            if let DrawOp::TexturedMesh(run) = op
                                && body_handles.contains(&run.texture_handle)
                            {
                                let vertices = pass.tmesh_geometries[run.geometry as usize]
                                    .vertices
                                    .as_ref();
                                let min_x = vertices
                                    .iter()
                                    .map(|v| v.pos[0])
                                    .fold(f32::INFINITY, f32::min);
                                let max_x = vertices
                                    .iter()
                                    .map(|v| v.pos[0])
                                    .fold(f32::NEG_INFINITY, f32::max);
                                wide_bodies += usize::from(max_x - min_x > 10.0);
                            }
                            let DrawOp::Sprite(run) = op else { continue };
                            if !receptor_handles.contains(&run.texture_handle) {
                                continue;
                            }
                            for instance in &pass.sprite_instances[run.instance_start as usize
                                ..(run.instance_start + run.instance_count) as usize]
                            {
                                // Direct proxy draws retain their transform in
                                // a camera; instances are centered world-space
                                // coordinates, not already projected pixels.
                                let camera = pass
                                    .cameras
                                    .get(run.camera as usize)
                                    .copied()
                                    .unwrap_or_else(|| metrics.projection());
                                let clip = camera.mul_vec4(
                                    [
                                        instance.center[0],
                                        instance.center[1],
                                        instance.center[2],
                                        1.0,
                                    ]
                                    .into(),
                                );
                                let pixel_y = (1.0 - clip.y / clip.w) * pass.viewport[1] as f32 / 2.0;
                                // FrameProxyP1 uses dh/2; ProxyP1N uses dw/sw.
                                let y = 240.0
                                    + (pixel_y - pass.viewport[1] as f32 / 2.0) * 854.0
                                        / pass.viewport[0] as f32;
                                assert!(
                                    (y - 115.0).abs() < 0.05 || (y - 385.0).abs() < 0.05,
                                    "beat {beat}: receptor swept between native anchors: Y={y}"
                                );
                                assert!(
                                    expected_y.is_none_or(|expected_y| (y - expected_y).abs() < 0.05),
                                    "beat {beat}: rendered receptor Y={y}, native={expected_y:?}; raw={:?}, camera={camera:?}",
                                    instance.center,
                                );
                                receptors += 1;
                            }
                        }
                    }
                    assert!(receptors >= 4, "all four receptors must reach the AFT pass");
                    if native_arrival.is_some() {
                        assert!(
                            arrival_meshes >= 2,
                            "first arrow must reach both source AFT draws"
                        );
                    }
                    if active_hold.is_some() {
                        assert!(
                            wide_bodies > 0,
                            "beat {beat}: actual active hold collapsed in AFT geometry"
                        );
                    }
                    if native_cap.is_some() {
                        assert!(cap_meshes >= 2, "native tail cap must reach both source AFT draws");
                    }
                    eprintln!(
                        "7th Gear beat {beat}: {receptors} AFT receptor sprites, {wide_bodies} full-width hold bodies, Mini=60"
                    );
                    #[cfg(target_os = "windows")]
                    if let Some(capture) = &mut capture {
                        assert!(
                            capture.capture(&mut state, &metrics, beat) > 0,
                            "visible Cel note pixels"
                        );
                    }
                }
                assert!(
                    native_pose_failures.is_empty(),
                    "{}",
                    native_pose_failures.join("\n")
                );
                let max_error: [f32; 3] = std::array::from_fn(|axis| native_pose_errors.iter()
                    .map(|(_, error)| error[axis].abs()).fold(0.0, f32::max));
                let max_error_step: [f32; 3] = std::array::from_fn(|axis| native_pose_errors.windows(2)
                    .map(|pair| (pair[1].1[axis] - pair[0].1[axis]).abs()).fold(0.0, f32::max));
                eprintln!("native pose errors [x,y,scale]: max={max_error:?}, consecutive delta={max_error_step:?}");
                for (axis, error) in max_error_step[..2].iter().enumerate() {
                    assert!(*error < 4.0,
                        "first-arrow motion differs from native between consecutive frames, axis {axis}: {error}");
                }
            },
        );
    }

    #[test]
    #[ignore = "full-song fixture with GPU readback on Windows"]
    fn bank_account_draws_complete_proxy_stack() {
        let fixture = song_fixture::from_source("Bank Account/Bank Account.ssc");
        let simfile = fixture.simfile.clone();
        with_session(
            profile_data::PlayStyle::Versus,
            profile_data::PlayerSide::P1,
            true,
            true,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                for skin in ["cel", "lambda"] {
                    let profiles = std::array::from_fn(|_| profile_data::Profile {
                        noteskin: profile_data::NoteSkin::new(skin),
                        scroll_speed: ScrollSpeedSetting::XMod(1.0),
                        ..Default::default()
                    });
                    let mut state = build_test_state(
                        &simfile,
                        GameplayViewport::new(854.0, 480.0),
                        GameplaySession {
                            play_style: deadsync_gameplay::GameplayInputPlayStyle::Versus,
                            joined_sides: [true, true],
                            ..Default::default()
                        },
                        profiles,
                    );
                    let assets = fixture_assets();
                    let mut actors = Vec::new();
                    let mut tick = 0;
                    // Native capture: first spawn selects #2, Despawn clears all,
                    // and the final spawn leaves #1..#62 visible for both players.
                    for (seconds, proxies) in [(18.2, 2), (46.2, 0), (83.0, 124)] {
                        // Advance the approach rates at the reference's 60 Hz;
                        // a direct seek leaves slow Mini transitions frozen.
                        while tick < (seconds * 60.0) as u32 {
                            tick += 1;
                            set_fixture_time(&mut state, tick as f32 / 60.0);
                            refresh_active_attack_masks(&mut state.gameplay, 1.0 / 60.0);
                        }
                        set_fixture_time(&mut state, seconds);
                        state.gameplay.refresh_live_notefield_options(106.0);
                        if seconds == 83.0 {
                            for player in 0..2 {
                                let mini = deadsync_gameplay::effective_mini_percent_for_player(
                                    &state.gameplay,
                                    player,
                                );
                                assert!(
                                    (mini - 155.0).abs() < 0.1,
                                    "{skin} P{player} Mini: {mini}"
                                );
                                assert!(
                                    (state.field_zoom_for_player(player) - 0.225).abs() < 0.001
                                );
                            }
                        }
                        actors.clear();
                        let segments = crate::gameplay_runtime::push_actors(
                            &mut actors,
                            &mut state,
                            &assets,
                            screen_gameplay::ActorViewOverride::default(),
                            123.0,
                            deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(
                            ),
                        );
                        assert_eq!(
                            segments.direct_field_proxy_count(state.song_frame()),
                            proxies,
                            "{skin} at {seconds}s"
                        );
                        let frame = compose::build_passes(
                            segments.segments(state.song_frame(), &actors),
                            state.render_targets(),
                            [0.0, 0.0, 0.0, 1.0],
                            &metrics,
                            assets.fonts(),
                            seconds,
                            &mut compose::TextLayoutCache::default(),
                            &mut compose::ComposeScratch::default(),
                            &FIXTURE_TEXTURES,
                            Some(state.actor_resources()),
                        );
                        let mut handles = Vec::new();
                        for noteskin in state.noteskin_assets.noteskin.iter().flatten() {
                            noteskin.for_each_slot(|slot| {
                                handles.push(FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                            });
                        }
                        let count: u32 = frame
                            .ops
                            .iter()
                            .filter_map(|op| match op {
                                DrawOp::Sprite(run) if handles.contains(&run.texture_handle) => {
                                    Some(run.instance_count)
                                }
                                _ => None,
                            })
                            .sum();
                        assert!(
                            count >= (proxies as u32).max(1) * 4,
                            "{skin} at {seconds}s: {count} noteskin sprites for {proxies} proxies"
                        );
                        eprintln!(
                            "Bank Account {skin} at {seconds}s: {proxies} proxies, {count} note sprites"
                        );
                    }
                    #[cfg(target_os = "windows")]
                    if skin == "cel" {
                        assert_bank_stack_pixels(&mut state, &metrics);
                    }
                }
            },
        );
    }

    #[cfg(target_os = "windows")]
    fn assert_bank_stack_pixels(state: &mut screen_gameplay::State, metrics: &space::Metrics) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .expect("capture loop");
        #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720)),
                )
                .expect("capture window"),
        );
        let mut backend = deadlib_render::create_backend(
            deadlib_render_core::BackendType::VulkanWgpu,
            window,
            metrics.projection(),
            false,
            deadlib_render_core::PresentModePolicy::Immediate,
            false,
            true,
        )
        .expect("capture renderer");
        let mut assets = deadlib_assets::AssetManager::new();
        load_gpu_capture_assets(&mut assets, &mut backend).expect("theme textures and fonts");
        prewarm_gpu_noteskins(state, &mut assets, &mut backend).expect("note textures");
        let mut frame = compose_fixture_frame_with_textures(
            state,
            &assets,
            metrics,
            &mut Vec::new(),
            &mut compose::TextLayoutCache::default(),
            &mut compose::ComposeScratch::default(),
            assets.texture_context(),
        );
        let mut handles = Vec::new();
        for noteskin in state.noteskin_assets.noteskin.iter().flatten() {
            noteskin.for_each_slot(|slot| {
                handles.push(assets.texture_context().texture_handle(slot.texture_key()))
            });
        }
        backend.request_screenshot();
        backend
            .draw(&frame, assets.textures(), false)
            .expect("draw Bank Account");
        let image = backend.capture_frame().expect("Bank Account pixels");
        if let Some(output) = std::env::var_os("DEADSYNC_BANK_CAPTURE") {
            image.save(output).expect("save Bank Account capture");
        }
        frame.ops.retain(|op| match op {
            DrawOp::Sprite(run) => !handles.contains(&run.texture_handle),
            DrawOp::TexturedMesh(run) => !handles.contains(&run.texture_handle),
            _ => true,
        });
        backend.request_screenshot();
        backend
            .draw(&frame, assets.textures(), false)
            .expect("draw without notes");
        let without = backend.capture_frame().expect("pixels without notes");
        let changed = image
            .pixels()
            .zip(without.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(
            changed > 500,
            "Bank Account stack draws only {changed} note pixels"
        );
        eprintln!("Bank Account stack changes {changed} GPU pixels");
        let mut textures = assets.take_textures();
        backend.dispose_textures(&mut textures);
        backend.cleanup();
    }

    #[test]
    #[ignore = "long full-song compiled render comparison"]
    fn delightful_day_player_proxy_draws_noteskin_instances() {
        let fixture = song_fixture::from_source("Delightful Day/Delightful Day.ssc");
        let simfile = fixture.simfile.clone();
        for skin in ["lambda", "cel"] {
            assert_proxy_note_draws(&simfile, skin, 15.0);
        }
    }

    #[test]
    #[ignore = "long song-fixture video export and GPU readback regression"]
    fn delightful_day_hides_theme_and_keeps_video() {
        let fixture = song_fixture::from_source("Delightful Day/Delightful Day.ssc");
        let simfile = fixture.simfile.clone();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                let profiles = std::array::from_fn(|_| profile_data::Profile::default());
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 30.0);
                let visuals = state.song_lua_visuals();
                assert!(!visuals.screen_layers[0].initial_state.visible);
                let score_targets = visuals
                    .overlays
                    .iter()
                    .filter(|overlay| {
                        matches!(
                            overlay.kind,
                            deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                target: deadsync_assets::song_lua::SongLuaProxyTarget::Score { .. }
                            }
                        )
                    })
                    .count();
                assert_eq!(
                    score_targets, 2,
                    "score children must not proxy the entire Underlay"
                );
                assert_eq!(
                    state.song_media.video_paths().len(),
                    1,
                    "both sprites share the movie texture"
                );
                let assets = fixture_assets();
                let mut actors = Vec::new();
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert_eq!(segments.direct_proxy_count(), 1);
                assert!(
                    !actor_tree_has_text(&actors, &state.song().title),
                    "hidden song HUD is still drawn"
                );
                assert!(
                    !actor_tree_has_text(&actors, "EVENT"),
                    "faded stage/footer is still drawn"
                );
                let frame = compose::build_passes(
                    segments.segments(state.song_frame(), &actors),
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    30.0,
                    &mut compose::TextLayoutCache::default(),
                    &mut compose::ComposeScratch::default(),
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                assert!(
                    !frame
                        .sprite_instances
                        .iter()
                        .any(|instance| instance.center[2] == -99.0),
                    "hidden note field filter reaches the renderer"
                );
                assert!(
                    actor_tree_has_text(&actors, "."),
                    "the independently proxied score must remain visible"
                );
                #[cfg(target_os = "windows")]
                assert_delightful_video_pixels(&mut state, &metrics);
            },
        );
    }

    #[cfg(target_os = "windows")]
    fn assert_delightful_video_pixels(
        state: &mut screen_gameplay::State,
        metrics: &space::Metrics,
    ) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let event_loop = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .expect("capture loop");
        #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720)),
                )
                .expect("capture window"),
        );
        let mut backend = deadlib_render::create_backend(
            deadlib_render_core::BackendType::VulkanWgpu,
            window,
            metrics.projection(),
            false,
            deadlib_render_core::PresentModePolicy::Immediate,
            false,
            true,
        )
        .expect("capture renderer");
        let movie = state.song_media.video_paths()[0].clone();
        // Decode a known lyric-bearing frame at the movie's actual source size.
        let output = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-ss", "30", "-i"])
            .arg(&movie)
            .args([
                "-an",
                "-frames:v",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                "png",
                "-",
            ])
            .output()
            .expect("decode lyric frame");
        assert!(output.status.success(), "lyric frame decode failed");
        let image = image::load_from_memory(&output.stdout)
            .expect("lyric pixels")
            .into_rgba8();
        assert_eq!(image.dimensions(), (1920, 1080));
        let key = movie.to_string_lossy().into_owned();
        let mut assets = deadlib_assets::AssetManager::new();
        load_gpu_capture_assets(&mut assets, &mut backend).expect("theme textures and fonts");
        prewarm_gpu_noteskins(state, &mut assets, &mut backend).expect("note textures");
        let texture = backend
            .create_texture(&image, deadlib_render_core::SamplerDesc::default())
            .expect("movie texture");
        assets.set_texture_for_key(
            &mut backend,
            key.clone(),
            texture,
            image.width(),
            image.height(),
        );
        let handle = assets.texture_context().texture_handle(&key);
        let mut frame = compose_fixture_frame_with_textures(
            state,
            &assets,
            metrics,
            &mut Vec::new(),
            &mut compose::TextLayoutCache::default(),
            &mut compose::ComposeScratch::default(),
            assets.texture_context(),
        );
        let movie_runs: Vec<_> = frame
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Sprite(run) if run.texture_handle == handle => Some(run),
                _ => None,
            })
            .collect();
        assert_eq!(
            movie_runs.iter().map(|run| run.instance_count).sum::<u32>(),
            2
        );
        for run in movie_runs {
            for sprite in &frame.sprite_instances
                [run.instance_start as usize..(run.instance_start + run.instance_count) as usize]
            {
                assert!(
                    (sprite.size[0] - 854.0).abs() < 2.0,
                    "movie is enlarged: {:?}",
                    sprite.size
                );
                assert!(
                    (sprite.size[1] - 480.0).abs() < 2.0,
                    "lyrics are cropped: {:?}",
                    sprite.size
                );
            }
        }
        backend.request_screenshot();
        backend
            .draw(&frame, assets.textures(), false)
            .expect("draw lyric frame");
        let image = backend.capture_frame().expect("lyric frame pixels");
        if let Some(path) = std::env::var_os("DEADSYNC_DELIGHTFUL_CAPTURE") {
            image.save(path).expect("save lyric frame");
        }
        frame
            .ops
            .retain(|op| !matches!(op, DrawOp::Sprite(run) if run.texture_handle == handle));
        backend.request_screenshot();
        backend
            .draw(&frame, assets.textures(), false)
            .expect("draw without movie");
        let without = backend.capture_frame().expect("without movie pixels");
        let y0 = image.height() * 82 / 100;
        let y1 = image.height() * 96 / 100;
        let changed = (y0..y1)
            .flat_map(|y| (0..image.width()).map(move |x| (x, y)))
            .filter(|&(x, y)| image.get_pixel(x, y) != without.get_pixel(x, y))
            .count();
        assert!(
            changed > 10_000,
            "movie/lyric region is absent: {changed} pixels"
        );
        eprintln!("Delightful Day: {changed} movie pixels in the lyric band");
        let mut textures = assets.take_textures();
        backend.dispose_textures(&mut textures);
        backend.cleanup();
    }

    #[test]
    fn root_player_proxy_uses_repeatable_direct_field_and_hud_segments() {
        let simfile = write_direct_player_proxy_fixture();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                add_sprite_core_feedback(&mut state, 0, 0, 100);
                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                // A whole-Player proxy carries separate Combo, Field, and
                // Judgment segments so their draw order is preserved.
                assert_eq!(segments.direct_proxy_count(), 3);
                assert_eq!(segments.direct_field_proxy_count(state.song_frame()), 1);
                assert_eq!(segments.direct_combo_proxy_count(state.song_frame()), 1);
                let actor_segments = segments.segments(state.song_frame(), &actors);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut warm = compose::build_passes(
                    actor_segments,
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    10.0,
                    &mut text_cache,
                    &mut compose_scratch,
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                assert!(warm.ops.iter().any(|op| matches!(op, DrawOp::Sprite(_))));
                compose_scratch.recycle_frame(&mut warm);
                let _ = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
            },
        );
    }

    #[test]
    fn single_source_aft_note_field_uses_repeatable_target_pass() {
        let simfile = write_direct_aft_note_field_proxy_fixture();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert_eq!(segments.direct_field_proxy_count(state.song_frame()), 0);
                let actor_segments = segments.segments(state.song_frame(), &actors);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut warm = compose::build_passes(
                    actor_segments,
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    10.0,
                    &mut text_cache,
                    &mut compose_scratch,
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                assert!(warm.ops.iter().any(|op| matches!(op, DrawOp::Sprite(_))));
                assert_eq!(warm.render_targets.len(), 1);
                assert!(
                    warm.render_targets[0]
                        .ops
                        .iter()
                        .any(|op| matches!(op, DrawOp::Sprite(_)))
                );
                compose_scratch.recycle_frame(&mut warm);
                let _ = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
            },
        );
    }

    #[test]
    fn single_source_aft_player_uses_repeatable_target_pass() {
        let simfile = write_direct_aft_player_proxy_fixture();
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                add_sprite_core_feedback(&mut state, 0, 0, 100);
                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert_eq!(segments.direct_proxy_count(), 0);
                assert_eq!(segments.direct_field_proxy_count(state.song_frame()), 0);
                let actor_segments = segments.segments(state.song_frame(), &actors);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut warm = compose::build_passes(
                    actor_segments,
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    10.0,
                    &mut text_cache,
                    &mut compose_scratch,
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                assert!(warm.ops.iter().any(|op| matches!(op, DrawOp::Sprite(_))));
                assert_eq!(warm.render_targets.len(), 1);
                assert!(!warm.render_targets[0].ops.is_empty());
                compose_scratch.recycle_frame(&mut warm);
                let _ = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
            },
        );
    }

    #[test]
    fn duplicate_root_combo_proxies_use_repeatable_direct_segments() {
        let simfile = write_direct_hud_proxy_fixture("f6-direct-combo", "Combo");
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_compiled_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                add_sprite_core_feedback(&mut state, 0, 0, 100);
                let visuals = state.gameplay.song_lua_visuals();
                assert!(visuals.overlays.iter().any(|overlay| {
                    // An ordinary ActorFrame parent does not make this an AFT
                    // proxy. The direct-segment assertions below verify that.
                    matches!(
                            overlay.kind,
                            deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                target: deadsync_assets::song_lua::SongLuaProxyTarget::Combo {
                                    player_index: 0
                                }
                            }
                        )
                }));
                let assets = fixture_assets();
                let mut actors = Vec::with_capacity(512);
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert_eq!(segments.direct_combo_proxy_count(state.song_frame()), 2);
                let actor_segments = segments.segments(state.song_frame(), &actors);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut warm = compose::build_passes(
                    actor_segments,
                    state.render_targets(),
                    [0.0, 0.0, 0.0, 1.0],
                    &metrics,
                    assets.fonts(),
                    10.0,
                    &mut text_cache,
                    &mut compose_scratch,
                    &FIXTURE_TEXTURES,
                    Some(state.actor_resources()),
                );
                assert!(warm.ops.iter().any(|op| matches!(op, DrawOp::Sprite(_))));
                compose_scratch.recycle_frame(&mut warm);
                let _ = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );

                state.presentation.set_hud(deadsync_theme_openitg::GAMEPLAY_HUD);
                actors.clear();
                let segments = crate::gameplay_runtime::push_actors(
                    &mut actors,
                    &mut state,
                    &assets,
                    screen_gameplay::ActorViewOverride::default(),
                    123.0,
                    deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                );
                assert_eq!(segments.direct_combo_proxy_count(state.song_frame()), 2);
                let _ = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
            },
        );
    }

    #[test]
    fn song_lua_frame_is_structurally_repeatable() {
        let simfile = write_pipeline_song_lua_fixture();
        const SONG_LUA_TEST_STACK: usize = 16 * 1024 * 1024;
        std::thread::Builder::new()
            .name("song-lua-frame-regression".to_string())
            .stack_size(SONG_LUA_TEST_STACK)
            .spawn(move || {
                with_session(
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    true,
                    false,
                    || {
                        let metrics = space::Metrics::centered(854.0, 480.0);
                        space::set_current_metrics(metrics);
                        space::set_current_window_px(1280, 720);
                        space::set_overscan(0, 0, 0, 0);

                        let mut profiles = [
                            profile_data::Profile::default(),
                            profile_data::Profile::default(),
                        ];
                        profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                        profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                        let mut state = build_compiled_test_state(
                            &simfile,
                            GameplayViewport::new(1280.0, 720.0),
                            GameplaySession::default(),
                            profiles,
                        );
                        add_sprite_core_feedback(&mut state, 0, 0, 42);

                        let visuals = state.gameplay.song_lua_visuals();
                        assert!(visuals.note_hides[0].iter().any(|window| {
                            window.column == 0 && window.start_beat <= 5.0 && window.end_beat >= 5.0
                        }));
                        assert!(
                            visuals.column_offsets[0]
                                .iter()
                                .any(|window| window.column == 1)
                        );
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::Quad
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target: deadsync_assets::song_lua::SongLuaProxyTarget::Combo {
                                        player_index: 0
                                    }
                                }
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target:
                                        deadsync_assets::song_lua::SongLuaProxyTarget::Judgment {
                                            player_index: 0
                                        }
                                }
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target: deadsync_assets::song_lua::SongLuaProxyTarget::Actor {
                                        overlay_index
                                    }
                                } if visuals.overlays[*overlay_index].name.as_deref()
                                    == Some("LocalProxyTarget")
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target:
                                        deadsync_assets::song_lua::SongLuaProxyTarget::Underlay {
                                            hidden: true
                                        }
                                }
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target:
                                        deadsync_assets::song_lua::SongLuaProxyTarget::Overlay {
                                            hidden: true
                                        }
                                }
                            )
                        }));
                        assert!(
                            visuals.overlays.iter().any(|overlay| {
                                matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorFrameTexture {
                                    ..
                                }
                            )
                            })
                        );
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target: deadsync_assets::song_lua::SongLuaProxyTarget::Player {
                                        player_index: 0
                                    }
                                }
                            )
                        }));
                        let capture_name = visuals
                            .overlays
                            .iter()
                            .find_map(|overlay| match &overlay.kind {
                                deadsync_assets::song_lua::SongLuaOverlayKind::AftSprite {
                                    capture_name,
                                } => Some(capture_name),
                                _ => None,
                            })
                            .expect("anonymous AFT sprite should compile");
                        assert!(capture_name.starts_with("ActorFrameTexture "));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorFrameTexture {
                                    capture_name: name, ..
                                } if name == capture_name
                            )
                        }));

                        let assets = fixture_assets();
                        let mut actors = Vec::with_capacity(512);
                        let mut text_cache = compose::TextLayoutCache::default();
                        let mut compose_scratch = compose::ComposeScratch::default();
                        set_fixture_time(&mut state, 2.5);
                        let first_transform = state.gameplay.song_lua_player_transform(0);
                        assert!(first_transform.rotation_z > 0.0);
                        assert!(first_transform.zoom_x < 1.0);
                        let first = assert_repeatable_frame(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text_cache,
                            &mut compose_scratch,
                        );

                        set_fixture_time(&mut state, 3.5);
                        let second_transform = state.gameplay.song_lua_player_transform(0);
                        assert!(second_transform.rotation_z > first_transform.rotation_z);
                        assert!(second_transform.zoom_x < first_transform.zoom_x);
                        let second = assert_repeatable_frame(
                            &mut state,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text_cache,
                            &mut compose_scratch,
                        );
                        assert_ne!(compare_render_frames(&first, &second), Ok(()));
                    },
                );
            })
            .expect("SongLua frame regression thread should spawn")
            .join()
            .expect("SongLua frame regression thread should finish");
    }

    #[test]
    fn versus_score_leader_alpha_tracks_live_itg_scores() {
        let simfile = write_fixture("versus-score-leader", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Versus,
            profile_data::PlayerSide::P1,
            true,
            true,
            || {
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[1].noteskin = profile_data::NoteSkin::new("lambda");
                let session = GameplaySession {
                    play_style: deadsync_gameplay::GameplayInputPlayStyle::Versus,
                    player_side: deadsync_gameplay::GameplayInputPlayerSide::P1,
                    joined_sides: [true, true],
                    ..GameplaySession::default()
                };
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    session,
                    profiles,
                );

                assert_eq!(
                    screen_gameplay::gameplay_score_leader_alphas(&state.gameplay),
                    [1.0, 1.0]
                );

                let fantastic = deadsync_rules::judgment::judge_grade_ix(JudgeGrade::Fantastic);
                let excellent = deadsync_rules::judgment::judge_grade_ix(JudgeGrade::Excellent);
                state.players_runtime.players[0].scoring_counts[fantastic] = 1;
                state.players_runtime.players[1].scoring_counts[excellent] = 1;
                assert!(state.display_itg_score_percent(0) > state.display_itg_score_percent(1));
                assert_eq!(
                    screen_gameplay::gameplay_score_leader_alphas(&state.gameplay),
                    [1.0, 0.65]
                );

                state.profiles_runtime.profiles[1].show_ex_score = true;
                assert_eq!(
                    screen_gameplay::gameplay_score_leader_alphas(&state.gameplay),
                    [1.0, 1.0]
                );
            },
        );
    }

    #[test]
    fn versus_modes_frame_is_structurally_repeatable() {
        let simfile = write_fixture("f0-versus-modes", generated_sprite_core_simfile());
        let metrics = space::Metrics::centered(854.0, 480.0);
        space::set_current_metrics(metrics);
        space::set_current_window_px(1280, 720);
        space::set_overscan(0, 0, 0, 0);
        let assets = fixture_assets();

        let versus = with_session(
            profile_data::PlayStyle::Versus,
            profile_data::PlayerSide::P1,
            true,
            true,
            || {
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                profiles[1].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[1].scroll_speed = ScrollSpeedSetting::XMod(2.25);
                profiles[1].scroll_option = profile_data::ScrollOption::Reverse;
                let session = GameplaySession {
                    play_style: deadsync_gameplay::GameplayInputPlayStyle::Versus,
                    player_side: deadsync_gameplay::GameplayInputPlayerSide::P1,
                    joined_sides: [true, true],
                    ..GameplaySession::default()
                };
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    session,
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                add_sprite_core_feedback(&mut state, 0, 0, 42);
                add_sprite_core_feedback(&mut state, 1, 4, 37);

                assert_eq!(state.num_players(), 2);
                assert_eq!(state.cols_per_player(), 4);
                assert_eq!(state.num_cols(), 8);
                let p1_range = state.note_range_for_player(0);
                let p2_range = state.note_range_for_player(1);
                assert!(p1_range.0 < p1_range.1);
                assert!(p1_range.1 <= p2_range.0);
                assert!(p2_range.0 < p2_range.1);
                assert!(
                    state.chart_runtime.notes[p2_range.0..p2_range.1]
                        .iter()
                        .all(|note| note.column >= 4)
                );
                assert!(state.tap_explosions_for_columns(0, 1)[0].is_some());
                assert!(state.tap_explosions_for_columns(4, 1)[0].is_some());

                let mut actors = Vec::with_capacity(1024);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                )
            },
        );

        let (autoplay, autoplay_disabled) = with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut state, 2.5);
                state.set_live_autoplay_enabled(true);
                assert!(state.autoplay_enabled());
                assert!(state.live_autoplay_enabled());
                assert!(state.autoplay_blocks_scoring());

                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let autoplay = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                state.set_live_autoplay_enabled(false);
                assert!(!state.autoplay_enabled());
                let disabled = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                (autoplay, disabled)
            },
        );
        assert_ne!(compare_render_frames(&autoplay, &autoplay_disabled), Ok(()));

        let (practice_base, practice) = with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut gameplay = build_test_state(
                    &simfile,
                    GameplayViewport::new(1280.0, 720.0),
                    GameplaySession::default(),
                    profiles,
                );
                set_fixture_time(&mut gameplay, 2.5);

                let mut actors = Vec::with_capacity(1024);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let base = assert_repeatable_frame(
                    &mut gameplay,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                let mut practice = deadsync_theme_simply_love::screens::practice::init(
                    gameplay,
                    deadsync_theme_simply_love::views::PracticeRuntimeView::default(),
                );
                set_fixture_time(&mut practice.gameplay, 2.5);
                assert!(!practice.gameplay.score_valid_for_player(0));
                let practice_frame =
                    assert_repeatable_composition(&mut compose_scratch, |scratch| {
                        compose_practice_fixture_frame(
                            &mut practice,
                            &assets,
                            &metrics,
                            &mut actors,
                            &mut text_cache,
                            scratch,
                        )
                    });
                (base, practice_frame)
            },
        );
        assert_ne!(compare_render_frames(&practice_base, &practice), Ok(()));
        assert_ne!(compare_render_frames(&versus, &autoplay), Ok(()));
        assert_ne!(compare_render_frames(&versus, &practice), Ok(()));
    }

    #[test]
    fn practice_editor_ignores_player_scroll_mods() {
        let simfile = write_fixture("practice-scroll-mods", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                space::set_overscan(0, 0, 0, 0);
                let assets = fixture_assets();
                let mut actors = Vec::new();
                let mut text_cache = compose::TextLayoutCache::default();
                let mut scratch = compose::ComposeScratch::default();
                let mut normal_frames = None;
                for scroll in [
                    profile_data::ScrollOption::Normal,
                    profile_data::ScrollOption::Reverse,
                    profile_data::ScrollOption::Split,
                    profile_data::ScrollOption::Alternate,
                    profile_data::ScrollOption::Cross,
                    profile_data::ScrollOption::Centered,
                    profile_data::ScrollOption::Reverse.union(profile_data::ScrollOption::Split),
                ] {
                    let mut profiles = std::array::from_fn(|_| profile_data::Profile::default());
                    profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                    profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                    profiles[0].scroll_option = scroll;
                    let gameplay = build_test_state(
                        &simfile,
                        GameplayViewport::new(1280.0, 720.0),
                        GameplaySession::default(),
                        profiles,
                    );
                    let mut practice = deadsync_theme_simply_love::screens::practice::init(
                        gameplay,
                        deadsync_theme_simply_love::views::PracticeRuntimeView::default(),
                    );
                    set_fixture_time(&mut practice.gameplay, 2.5);
                    let editor = compose_practice_fixture_frame(
                        &mut practice,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut scratch,
                    );
                    assert_eq!(practice.gameplay.profiles()[0].scroll_option, scroll);
                    // Playback uses the unmodified gameplay view and must still
                    // honor the profile after the editor has drawn its field.
                    let gameplay = compose_fixture_frame(
                        &mut practice.gameplay,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut scratch,
                    );
                    if let Some((normal_editor, normal_gameplay)) = &normal_frames {
                        assert_eq!(
                            compare_render_frames_semantic(normal_editor, &editor),
                            Ok(()),
                            "{scroll}"
                        );
                        assert_ne!(
                            compare_render_frames_semantic(normal_gameplay, &gameplay),
                            Ok(()),
                            "{scroll}"
                        );
                    } else {
                        normal_frames = Some((editor, gameplay));
                    }
                }
            },
        );
    }

    fn generated_runtime_mod_lua() -> &'static str {
        r#"
dofile(GAMESTATE:GetCurrentSong():GetSongDir() .. "easing.lua")
mods = {
    {0, 9999, "*1000 no beat, *1000 no drunk, *1000 no tipsy, *1000 no invert, *1000 no flip, *1000 no dizzy", "end"},
}
mod_time = {
    {0.00, 999, "*1 0 Dark1, *1 0 Dark2, *1 0 Dark3, *1 0 Dark4, *1 0 PulseOuter, *1 0 PulseOffset, *1 0 Wave, *1 0 Bumpy3, *1 0 BumpyPeriod, *1 0 Stealth, *1 0 Blind, *1 0 Sudden, *1 0 Tipsy, *1 0 Drunk, *1 0 Dark", "len"},
}
mods_ease = {}

local l = "len"
local function me(...)
    table.insert(mods_ease, {...})
end

me(4, 0.75, 250, 0, "Bumpy1", l, ease.outQuad)
me(4, 0.75, -125, 0, "BumpyPeriod", l, ease.outQuad)
me(4, 0.75, 75, 0, "Wave", l, ease.outElastic)
me(8, 0.75, 250, 0, "Bumpy2", l, ease.outQuad)
me(12, 0.75, 250, 0, "Bumpy3", l, ease.outQuad)
me(16, 0.75, 250, 0, "Bumpy4", l, ease.outQuad)
me(20, 1.5, 50, 1, "hidden", l, ease.outInQuad)
me(24, 0.5, 25, 0, "beat", l, ease.outBounce)

return Def.ActorFrame{}
"#
    }

    fn generated_lua_song_simfile() -> &'static str {
        r#"#VERSION:0.83;
#TITLE:Generated Lua Regression;
#MUSIC:;
#OFFSET:0.000;
#BPMS:0.000=120.000;
#FGCHANGES:0.000=lua/default.lua=1.000=0=0=0=StretchNoLoop====;

#NOTEDATA:;
#STEPSTYPE:dance-single;
#DESCRIPTION:Generated;
#DIFFICULTY:Challenge;
#METER:12;
#RADARVALUES:0,0,0,0,0;
#NOTES:
0000
0000
0000
1000
,
0100
0000
0010
0001
,
1000
0100
0010
0001
,
0010
0001
1000
0100
,
0001
0010
0100
1000
,
1000
0000
0100
0000
,
0010
0000
0001
0000
;
"#
    }

    fn write_generated_lua_song_fixture() -> PathBuf {
        let song_dir = test_dir("generated-lua-song");
        let lua_dir = song_dir.join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        fs::write(lua_dir.join("default.lua"), generated_runtime_mod_lua()).unwrap();
        fs::write(song_dir.join("easing.lua"),
            include_str!("../../../tests/fixtures/song_lua/easing.lua")).unwrap();
        let simfile = song_dir.join("generated_lua_regression.ssc");
        fs::write(&simfile, generated_lua_song_simfile()).unwrap();
        simfile
    }

    #[test]
    fn gameplay_handles_generated_song_lua_actor_build() {
        crate::tests::init_paths();
        let simfile = write_generated_lua_song_fixture();
        const SONG_LUA_TEST_STACK: usize = 16 * 1024 * 1024;
        std::thread::Builder::new()
            .name("song-lua-actor-build-regression".to_string())
            .stack_size(SONG_LUA_TEST_STACK)
            .spawn(move || {
                let song = Arc::new(
                    deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0)
                        .expect("generated lua simfile should parse"),
                );
                let chart_ix = song
                    .charts
                    .iter()
                    .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
                    .unwrap_or(0);
                let gameplay_chart = Arc::new(
                    deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[chart_ix], 0.0)
                        .expect("generated lua gameplay chart should load")
                        .remove(0),
                );
                let chart = Arc::new(song.charts[chart_ix].clone());
                let mut player_profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                player_profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                player_profiles[1].scroll_speed = ScrollSpeedSetting::CMod(516.0);

                with_session(
                    profile_data::PlayStyle::Single,
                    profile_data::PlayerSide::P1,
                    true,
                    false,
                    || {
                        let session = super::GameplaySession::default();
                        let charts = [chart.clone(), chart];
                        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
                        let scroll_speed = [
                            player_profiles[0].scroll_speed,
                            player_profiles[1].scroll_speed,
                        ];
                        let runtime_profiles =
                            deadsync_profile_gameplay::gameplay_runtime_profile_data(
                                &player_profiles,
                                &session,
                            );
                        let noteskin_assets = gameplay_noteskin_assets(
                            session.play_style.cols_per_player(),
                            session.play_style.player_count(),
                            &runtime_profiles,
                            song.simfile_path.parent().expect("song directory"),
                            &[None, None],
                            0.0,
                        );
                        let noteskin_data = noteskin_assets.gameplay_data(
                            session.play_style.cols_per_player(),
                            session.play_style.player_count(),
                            &runtime_profiles,
                        );
                        let context = deadsync_profile_gameplay::song_lua_compile_context(
                            song.as_ref(),
                            &charts,
                            session.play_style.player_count(),
                            &player_profiles,
                            &scroll_speed,
                            1.0,
                            0.0,
                            super::GameplayViewport::default(),
                            &session,
                            false,
                        );
                        let primary = song
                            .foreground_lua_changes
                            .iter()
                            .find(|change| change.start_beat <= 0.0 && change.path.is_file())
                            .map(|change| {
                                compile_song_lua(&change.path, &context)
                                    .expect("generated song lua should compile")
                            });
                        let song_lua_data =
                            deadsync_song_lua::playback::actor_conformance::prepared_primary(primary);
                        let mut state = screen_gameplay::from_gameplay(
                            init(
                                song,
                                charts,
                                gameplay_charts,
                                super::GameplayViewport::default(),
                                session,
                                super::GameplayConfig::default(),
                                deadsync_chart::SyncPref::Default,
                                super::GameplayMiniIndicatorData::default(),
                                noteskin_data,
                                song_lua_data,
                                5,
                                1.0,
                                scroll_speed,
                                player_profiles,
                                None,
                                None,
                                None,
                                None,
                                None,
                                None,
                                [0; MAX_PLAYERS],
                            ),
                            noteskin_assets,
                        );
                        assert!(!state.mods.attacks.song_lua_ease_windows[0].is_empty());

                        let mut times =
                            vec![0.0, state.clock.song_position.current_music_time_display];
                        for window in &state.mods.attacks.song_lua_ease_windows[0] {
                            times.push(window.start_second);
                            times.push(f32::midpoint(window.start_second, window.end_second));
                            times.push(window.end_second);
                            times.push(window.sustain_end_second);
                        }
                        times.sort_by(f32::total_cmp);
                        times.dedup_by(|a, b| (*a - *b).abs() <= 0.001);

                        let assets = deadlib_assets::AssetManager::new();
                        for time in times {
                            state.clock.song_position.current_music_time_display = time;
                            state.clock.visible_timing.current_music_time = [time; MAX_PLAYERS];
                            state.clock.song_position.current_beat =
                                state.timing_runtime.timing.get_beat_for_time(time);
                            refresh_active_attack_masks(&mut state.gameplay, 0.0);
                            let mut actors = Vec::new();
                            crate::gameplay_runtime::push_actors(
                                &mut actors,
                                &mut state,
                                &assets,
                                screen_gameplay::ActorViewOverride::default(),
                                123.0,
                                deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView::default(),
                            );
                        }
                    },
                );
            })
            .expect("song-lua actor build regression thread should spawn")
            .join()
            .expect("song-lua actor build regression thread should finish");
    }

    /// Dyscontrolled Galaxy's pad section (beats 34.5-43) moves and tilts
    /// PlayerP1, translates it in Z, and zooms its wrapper state in Z while
    /// Bumpy gives every note depth. Native draws the notefield through that
    /// whole chain under a perspective centered on the Player's own X/Y.
    /// The song-Lua replay compares only authored actors, so these engine
    /// note draws need the native notefield trace. The foreground pad draws
    /// over flat receptor sprites; moving Player in Z must not reorder it
    /// above SongForeground (Actor::SetZ and SetDrawOrder are independent).
    #[test]
    fn dyscontrolled_pad_matches_native_geometry_and_draw_order() {
        let native: serde_json::Value = serde_json::from_slice(&zstd::stream::decode_all(
            include_bytes!("../../../tests/fixtures/itgmania-song-lua-micro/dyscontrolled-pad-native.json.zst").as_slice(),
        ).unwrap()).unwrap();
        assert_eq!(native["oracle"], "itgmania_notefield_render_trace");
        assert_eq!(native["input"]["window_visible"], false);
        assert_eq!(native["input"]["player_options"], "m690, -10% mini, cel");
        assert_eq!(
            native["input"]["native_source"]["revision"],
            "663525dcb86eb570d0ef6a2e209678436660ae26"
        );
        assert_eq!(native["input"]["native_source"]["modified_src"], false);
        assert_eq!(native["input"]["scene_draws"], true);
        assert_eq!(native["capabilities"]["native_fast_note_rendering"], true);
        for field in [
            "fast_note_rendering_requested",
            "fast_note_rendering",
            "fast_note_rendering_at_end",
        ] {
            assert_eq!(native["input"][field], true, "native depth policy: {field}");
        }
        assert_eq!(
            native["capabilities"]["native_receptor_geometry_protocol"],
            1
        );
        for field in [
            "dropped_model_geometry_samples",
            "dropped_drawable_samples",
            "dropped_receptor_visits",
            "dropped_script_errors",
        ] {
            assert_eq!(native[field], 0, "incomplete native capture: {field}");
        }
        assert!(native["script_errors"].as_array().unwrap().is_empty());
        // The foreground's sole Model is the platform. Its first mesh is
        // Plane04 in lua/pad/platform meshes.txt. Graphics submission IDs span
        // the sprite and model channels, so they retain native painter order.
        let mut native_pad_order = std::collections::BTreeMap::<u64, u64>::new();
        for sample in native["model_geometry_samples"].as_array().unwrap() {
            let state = &sample["state"];
            if state["render_target"] == 0
                && state["mesh_index"] == 0
                && state["mesh_name"] == "Plane04"
                && state["note_context"].is_null()
            {
                let update = sample["update_sequence"].as_u64().unwrap();
                let order = state["native_draw_sequence"].as_u64().unwrap();
                native_pad_order
                    .entry(update)
                    .and_modify(|first| *first = (*first).min(order))
                    .or_insert(order);
            }
        }
        let mut native_receptors = std::collections::BTreeMap::<u32, Vec<[f32; 3]>>::new();
        for sample in native["drawable_samples"].as_array().unwrap() {
            let state = &sample["state"];
            let context = &state["note_context"];
            if context["kind"] != "receptor"
                || context["player"] != 0
                || state["render_target"] != 0
            {
                continue;
            }
            let beat = context["draw_beat"].as_f64().unwrap() as f32;
            if !(34.6..42.9).contains(&beat) {
                continue;
            }
            let update = sample["update_sequence"].as_u64().unwrap();
            let pad_order = native_pad_order
                .get(&update)
                .expect("native pad draw in receptor frame");
            assert!(
                state["native_draw_sequence"].as_u64().unwrap() < *pad_order,
                "native receptors draw before the foreground pad at beat {beat}"
            );
            let vertices = sample["vertices"].as_array().unwrap();
            assert_eq!(vertices.len(), 4, "Cel receptor Sprite submits one quad");
            let clip: [f64; 4] = std::array::from_fn(|axis| {
                vertices
                    .iter()
                    .map(|vertex| vertex[3 + axis].as_f64().unwrap())
                    .sum::<f64>()
                    / 4.0
            });
            let point = [
                ((1.0 + clip[0] / clip[3]) * 427.0) as f32,
                ((1.0 - clip[1] / clip[3]) * 240.0) as f32,
                clip[3] as f32,
            ];
            let receptors = native_receptors.entry(beat.to_bits()).or_default();
            if !receptors
                .iter()
                .any(|seen| (0..3).all(|axis| (seen[axis] - point[axis]).abs() < 0.01))
            {
                receptors.push(point);
            }
        }
        let mut visible_native_visits = 0;
        for visit in native["receptor_visits"].as_array().unwrap() {
            let state = &visit["state"];
            let beat = state["note_context"]["draw_beat"].as_f64().unwrap();
            if (35.5..42.9).contains(&beat) && state["render_target"] == 0 {
                assert_eq!(
                    state["base_alpha"], 1.0,
                    "native Dark has ended at beat {beat}"
                );
                assert_eq!(state["visible"], true);
                visible_native_visits += 1;
            }
        }
        assert!(
            visible_native_visits > 500,
            "native receptor visits: {visible_native_visits}"
        );
        // Screen-target model origins of every drawn tap, per native frame.
        let mut native_frames = std::collections::BTreeMap::<u32, Vec<[f32; 3]>>::new();
        for sample in native["model_geometry_samples"].as_array().unwrap() {
            let state = &sample["state"];
            if state["render_target"] != 0
                || state["mesh_index"] != 0
                || state["note_context"]["kind"] != "actor"
            {
                continue;
            }
            let beat = state["note_context"]["draw_beat"].as_f64().unwrap() as f32;
            if !(34.6..42.9).contains(&beat) {
                continue;
            }
            let clip: Vec<f64> = serde_json::from_value(state["anchors_clip"][0].clone()).unwrap();
            let point = [
                ((1.0 + clip[0] / clip[3]) * 427.0) as f32,
                ((1.0 - clip[1] / clip[3]) * 240.0) as f32,
                clip[3] as f32,
            ];
            // Each tap draws its model layers at one origin.
            let notes = native_frames.entry(beat.to_bits()).or_default();
            if !notes
                .iter()
                .any(|seen| (0..3).all(|axis| (seen[axis] - point[axis]).abs() < 0.01))
            {
                notes.push(point);
            }
        }
        let mut native_frames: Vec<(f32, Vec<[f32; 3]>)> = native_frames
            .into_iter()
            .map(|(beat, notes)| (f32::from_bits(beat), notes))
            .collect();
        native_frames.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert!(
            native_frames.len() > 150,
            "native pad frames: {}",
            native_frames.len()
        );
        let fixture = song_fixture::from_source("Dyscontrolled Galaxy/steps.sm");
        let simfile = fixture.simfile.clone();
        std::thread::Builder::new().stack_size(256 << 20).spawn(move || {
            with_session(profile_data::PlayStyle::Single, profile_data::PlayerSide::P1, true, false, || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1280, 720);
                let profiles = std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"),
                    scroll_speed: ScrollSpeedSetting::MMod(690.0),
                    mini_percent: -10,
                    ..Default::default()
                });
                // Compare the native OpenGL geometry with the user's renderer.
                let mut state = build_test_state_with_renderer(&simfile, GameplayViewport::design(),
                    GameplaySession::default(), profiles, "Vulkan (wgpu)");
                state.set_live_autoplay_enabled(true);
                let mut assets = fixture_assets();
                let ns = state.noteskin_assets.noteskin[0].as_ref().unwrap();
                let tap_handles: std::collections::HashSet<_> = ns.notes.iter()
                    .chain(ns.note_layers.iter().flat_map(|layers| layers.iter()))
                    .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                    .collect();
                let receptor_handles: std::collections::HashSet<_> = state.noteskin_assets
                    .receptor_noteskin[0].as_ref().unwrap_or(ns)
                    .receptor_off.iter()
                    .map(|slot| FIXTURE_TEXTURES.texture_handle(slot.texture_key()))
                    .collect();
                let visuals = state.gameplay.song_lua_visuals();
                let mut pad_handles = std::collections::HashSet::new();
                // Models check AssetManager before emitting actors. Register their
                // keys even though this geometry-only compositor uses hashed handles.
                for overlay in visuals.overlays.iter().chain(
                    visuals.background_visual_layers.iter()
                        .chain(&visuals.foreground_visual_layers)
                        .flat_map(|layer| &layer.overlays),
                ) {
                    let deadsync_assets::song_lua::SongLuaOverlayKind::Model { layers } = &overlay.kind
                        else { continue; };
                    for layer in layers.iter() {
                        let is_pad = layer.texture_key.replace('\\', "/").contains("/lua/pad/");
                        for key in std::iter::once(&layer.texture_key)
                            .chain(layer.additive.as_ref())
                            .chain(
                                layer.texture_frames.iter()
                                    .chain(layer.additive_frames.iter())
                                    .filter_map(|frame| frame.texture_key.as_ref()),
                            )
                        {
                            assets.reserve_texture_handle(key.to_string());
                            if is_pad {
                                pad_handles.insert(FIXTURE_TEXTURES.texture_handle(key));
                            }
                        }
                    }
                }
                assert!(!pad_handles.is_empty(), "foreground pad model textures");
                #[cfg(target_os = "windows")]
                let mut capture = std::env::var_os("DEADSYNC_DYSCONTROLLED_CAPTURE_DIR")
                    .map(|output| SongLuaNotePixelCapture::new(&state, &metrics, PathBuf::from(output)));
                #[cfg(target_os = "windows")]
                let mut next_capture = 0;
                let timing = state.gameplay.gameplay_chart(0).unwrap().timing.clone();
                let step = |state: &mut screen_gameplay::State, at: f32, delta: f32| {
                    let music = deadsync_core::song_time::song_time_ns_from_seconds(at);
                    let host = (f64::from(at) * 1e9) as u64 + 1_000_000;
                    state.set_current_music_time_ns(music);
                    screen_gameplay::update(state, delta, deadsync_gameplay::GameplayAudioSnapshot {
                        stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                            music_nanos: music,
                            has_music_mapping: true,
                            valid_at_host_nanos: host,
                            ..Default::default()
                        },
                        ..Default::default()
                    }, || host, &mut Vec::new());
                    let output = state.stage.take_output();
                    assert!(output.errors.is_empty(), "Lua frame failed: {:?}", output.errors);
                };
                let mut now = 0.0f32;
                let mut compared = 0;
                for (beat, expected) in native_frames.iter().step_by(4) {
                    let target = timing.get_time_for_beat_exact(*beat);
                    while now + 1.0 / 60.0 < target {
                        now += 1.0 / 60.0;
                        step(&mut state, now, 1.0 / 60.0);
                    }
                    step(&mut state, target, target - now);
                    now = target;
                    let frame = compose_fixture_frame(&mut state, &assets, &metrics, &mut Vec::new(),
                        &mut compose::TextLayoutCache::default(), &mut compose::ComposeScratch::default());
                    assert!(frame.render_targets.is_empty(), "pad section draws the Player to the screen at beat {beat}");
                    let last_receptor = frame.ops.iter().rposition(|op| {
                        matches!(op, DrawOp::Sprite(run) if receptor_handles.contains(&run.texture_handle))
                    }).unwrap_or_else(|| panic!("native Dark is restored: receptor sprites are still submitted at beat {beat}"));
                    let first_pad = frame.ops.iter().position(|op| {
                        matches!(op, DrawOp::TexturedMesh(run) if pad_handles.contains(&run.texture_handle))
                    }).unwrap_or_else(|| panic!("foreground pad model is drawn at beat {beat}"));
                    assert!(last_receptor < first_pad,
                        "Player Z must not raise receptors above the pad at beat {beat}: receptor op {last_receptor}, pad op {first_pad}");
                    let mut receptor_points: Vec<[f32; 3]> = Vec::new();
                    for op in &frame.ops {
                        let DrawOp::Sprite(run) = op else { continue };
                        if !receptor_handles.contains(&run.texture_handle) { continue; }
                        let camera = frame.cameras.get(run.camera as usize).copied()
                            .unwrap_or_else(|| metrics.projection());
                        for instance in &frame.sprite_instances[run.instance_start as usize
                            ..(run.instance_start + run.instance_count) as usize]
                        {
                            let clip = camera.mul_vec4([instance.center[0], instance.center[1],
                                instance.center[2], 1.0].into());
                            let point = [427.0 + clip.x / clip.w * 427.0,
                                240.0 - clip.y / clip.w * 240.0, clip.w];
                            if !receptor_points.iter().any(|seen| (0..3).all(|axis| (seen[axis] - point[axis]).abs() < 0.01)) {
                                receptor_points.push(point);
                            }
                        }
                    }
                    let expected_receptors = native_receptors.get(&beat.to_bits())
                        .expect("native receptor geometry in tap frame");
                    assert_eq!(expected_receptors.len(), 4, "all native pad columns at beat {beat}");
                    assert_eq!(receptor_points.len(), expected_receptors.len(), "receptor count at beat {beat}");
                    for expected in expected_receptors {
                        let index = receptor_points.iter().position(|actual| {
                            (actual[0] - expected[0]).hypot(actual[1] - expected[1]) < 0.5
                                && (actual[2] - expected[2]).abs() < 0.5
                        }).unwrap_or_else(|| panic!("native receptor {expected:?} at beat {beat}, actual {receptor_points:?}"));
                        receptor_points.swap_remove(index);
                    }
                    let mut actual: Vec<[f32; 3]> = Vec::new();
                    for op in &frame.ops {
                        let DrawOp::TexturedMesh(run) = op else { continue };
                        if !tap_handles.contains(&run.texture_handle) {
                            continue;
                        }
                        let camera = frame.cameras.get(run.camera as usize).copied()
                            .unwrap_or_else(|| metrics.projection());
                        for instance in &frame.tmesh_instances[run.instance_start as usize
                            ..(run.instance_start + run.instance_count) as usize]
                        {
                            let clip = camera * instance.transform().w_axis;
                            let point = [427.0 + clip.x / clip.w * 427.0, 240.0 - clip.y / clip.w * 240.0, clip.w];
                            // Each tap draws its model parts at one origin.
                            if !actual.iter().any(|seen| (0..3).all(|axis| (seen[axis] - point[axis]).abs() < 0.01)) {
                                actual.push(point);
                            }
                        }
                    }
                    assert_eq!(actual.len(), expected.len(),
                        "taps at beat {beat}: deadsync {actual:?} vs native {expected:?}");
                    for note in expected {
                        let distance = |p: &[f32; 3]| (p[0] - note[0]).hypot(p[1] - note[1]);
                        let nearest_index = actual.iter().enumerate()
                            .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
                            .map(|(index, _)| index).unwrap();
                        // Match one submitted tap to each native tap; a correct
                        // origin must not conceal a second misplaced note.
                        let nearest = actual.swap_remove(nearest_index);
                        assert!(
                            distance(&nearest) < 0.5 && (nearest[2] - note[2]).abs() < 0.5,
                            "native tap at beat {beat}: {note:?}, nearest deadsync {nearest:?}"
                        );
                    }
                    #[cfg(target_os = "windows")]
                    if let Some(capture) = capture.as_mut()
                        && next_capture < 3 && *beat >= [36.0, 39.0, 42.0][next_capture]
                    {
                        let pixels = capture.capture_named(&mut state, &metrics, *beat, "dyscontrolled");
                        // With the captured FastNoteRendering setting, Model depth
                        // survives until the pad draw and keeps nearer taps visible.
                        assert!(pixels > 100, "thrown notes must remain visible at beat {beat}");
                        next_capture += 1;
                    }
                    compared += 1;
                }
                assert!(compared > 40, "compared pad frames: {compared}");
                #[cfg(target_os = "windows")]
                if capture.is_some() {
                    assert_eq!(next_capture, 3, "all requested pad captures must run");
                }
                eprintln!("Dyscontrolled Galaxy matched native geometry and draw order in {compared} frames");
            });
        }).unwrap().join().unwrap();
    }

    type NativeClipProbe = (usize, bool, [[f32; 4]; 4]);

    /// A native Vibrate draw consumes unrecorded random inputs. Admit only one
    /// bounded XY translation shared by every submitted mesh in that frame;
    /// scale, rotation, depth, W, relative note positions, and counts stay exact.
    /// This aligns observations for comparison and never changes renderer inputs.
    fn native_clip_translation_within_bounds(
        expected: &[NativeClipProbe],
        actual: &[NativeClipProbe],
        bounds: [f32; 2],
    ) -> Option<[f32; 2]> {
        if expected.len() != actual.len()
            || !bounds
                .into_iter()
                .all(|bound| bound.is_finite() && bound >= 0.0)
            || !expected
                .iter()
                .all(|probe| probe.2.iter().flatten().all(|value| value.is_finite()))
        {
            return None;
        }
        let Some(first) = expected.first() else {
            return Some([0.0; 2]);
        };
        for candidate in actual
            .iter()
            .filter(|candidate| candidate.0 == first.0 && candidate.1 == first.1)
        {
            let shift = [
                candidate.2[0][0] - first.2[0][0],
                candidate.2[0][1] - first.2[0][1],
            ];
            if !(0..2).all(|axis| shift[axis].is_finite() && shift[axis].abs() <= bounds[axis]) {
                continue;
            }
            let delta = [shift[0], shift[1], 0.0, 0.0];
            let mut used = vec![false; actual.len()];
            let matches = expected.iter().all(|native| {
                let closest = actual
                    .iter()
                    .enumerate()
                    .filter(|(index, value)| {
                        !used[*index] && value.0 == native.0 && value.1 == native.1
                    })
                    .map(|(index, value)| {
                        let error = if value.2.iter().flatten().all(|value| value.is_finite()) {
                            (0..4)
                                .flat_map(|anchor| {
                                    (0..4).map(move |axis| {
                                        (value.2[anchor][axis]
                                            - native.2[anchor][axis]
                                            - delta[axis])
                                            .abs()
                                    })
                                })
                                .fold(0.0f32, f32::max)
                        } else {
                            f32::INFINITY
                        };
                        (index, error)
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((index, error)) = closest
                    && error < 0.5
                {
                    used[index] = true;
                    true
                } else {
                    false
                }
            });
            if matches {
                return Some(shift);
            }
        }
        None
    }

    #[test]
    fn native_shake_comparison_rejects_geometry_and_visibility_errors() {
        let anchors = [
            [0.0, 0.0, 10.0, 11.0],
            [32.0, 0.0, 10.0, 11.0],
            [0.0, 32.0, 10.0, 11.0],
            [0.0, 0.0, 42.0, 43.0],
        ];
        let expected = [(0, false, anchors), (1, true, anchors)];
        let mut actual = expected;
        for (_, _, probes) in &mut actual {
            for probe in probes {
                probe[0] += 12.0;
                probe[1] -= 8.0;
            }
        }
        assert_eq!(
            native_clip_translation_within_bounds(&expected, &actual, [20.0; 2]),
            Some([12.0, -8.0])
        );
        assert!(native_clip_translation_within_bounds(&expected, &actual, [5.0; 2]).is_none());
        assert!(
            native_clip_translation_within_bounds(&expected, &actual[..1], [20.0; 2]).is_none()
        );
        let mut distorted = actual;
        distorted[1].2[1][0] += 2.0;
        assert!(native_clip_translation_within_bounds(&expected, &distorted, [20.0; 2]).is_none());
        let mut misplaced = actual;
        for probe in &mut misplaced[1].2 {
            probe[1] += 2.0;
        }
        assert!(native_clip_translation_within_bounds(&expected, &misplaced, [20.0; 2]).is_none());
        let mut wrong_depth = actual;
        for (_, _, probes) in &mut wrong_depth {
            for probe in probes {
                probe[2] += 2.0;
            }
        }
        assert!(
            native_clip_translation_within_bounds(&expected, &wrong_depth, [20.0; 2]).is_none()
        );
        let mut invalid_native = expected;
        invalid_native[0].2[1][0] = f32::NAN;
        assert!(
            native_clip_translation_within_bounds(&invalid_native, &actual, [20.0; 2]).is_none()
        );
    }

    #[test]
    fn dyscontrolled_ending_baked_keeps_player_fields_separated() {
        let fixture = song_fixture::from_source("Dyscontrolled Galaxy/steps.sm");
        let simfile = fixture.simfile.clone();
        std::thread::Builder::new()
            .name("dyscontrolled-ending-baked".into())
            .stack_size(256 << 20)
            .spawn(move || {
                with_session(
                    profile_data::PlayStyle::Versus,
                    profile_data::PlayerSide::P1,
                    true,
                    true,
                    || {
                        let viewport = GameplayViewport::design();
                        space::set_current_metrics(space::Metrics::centered(
                            viewport.width(), viewport.height()));
                        space::set_current_window_px(1600, 900);
                        let profiles = std::array::from_fn(|_| profile_data::Profile {
                            noteskin: profile_data::NoteSkin::new("cel"),
                            scroll_speed: ScrollSpeedSetting::MMod(690.0),
                            mini_percent: -10,
                            ..Default::default()
                        });
                        // Exercise production preparation for the entire chart:
                        // no shortened horizon, imported clocks, or live Lua scene.
                        let mut state = build_test_state_with_renderer(
                            &simfile,
                            viewport,
                            GameplaySession {
                                play_style: deadsync_gameplay::GameplayInputPlayStyle::Versus,
                                joined_sides: [true, true],
                                ..Default::default()
                            },
                            profiles,
                            "Vulkan (wgpu)",
                        );
                        assert!(state.song_lua.is_none(), "ending regression must use Baked Lua");
                        state.set_live_autoplay_enabled(true);
                        state.set_current_music_time_ns(0);
                        let timing = state.gameplay.timing().clone();
                        let step = |state: &mut screen_gameplay::State, at: f32, delta: f32| {
                            let music = deadsync_core::song_time::song_time_ns_from_seconds(at);
                            let host = (f64::from(at) * 1e9) as u64 + 1_000_000;
                            screen_gameplay::update(
                                state,
                                delta,
                                deadsync_gameplay::GameplayAudioSnapshot {
                                    stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                                        music_nanos: music,
                                        has_music_mapping: true,
                                        valid_at_host_nanos: host,
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                },
                                || host,
                                &mut Vec::new(),
                            );
                            state.set_current_music_time_ns(music);
                            let output = state.stage.take_output();
                            assert!(output.errors.is_empty(),
                                "ending Baked Lua at {at}s: {:?}", output.errors);
                        };
                        let mut now = 0.0f32;
                        for beat in [294.0f32, 302.0, 315.0, 320.25, 322.25] {
                            let target = timing.get_time_for_beat_exact(beat);
                            while now + 1.0 / 60.0 < target {
                                now += 1.0 / 60.0;
                                step(&mut state, now, 1.0 / 60.0);
                            }
                            step(&mut state, target, target - now);
                            now = target;
                            // Read the consumed gameplay state, rather than the
                            // last captured ease (which can hide application errors).
                            let xs = [
                                state.gameplay.song_lua_player_transform(0).x,
                                state.gameplay.song_lua_player_transform(1).x,
                            ];
                            let [Some(p1), Some(p2)] = xs else {
                                panic!("ending Player X missing at beat {beat}: P1/P2={xs:?}");
                            };
                            let xs = [p1, p2];
                            assert!(xs.iter().all(|x| x.is_finite()),
                                "ending Player X must be finite at beat {beat}: P1/P2={xs:?}");
                            let center = viewport.center_x();
                            let height = viewport.height() / 480.0;
                            if beat < 320.0 {
                                // default.lua's 286..320 update reads the current
                                // dys_rotamp:GetX(), which reaches 180 by beat 288
                                // (ITGmania Actor.h: GetX reads m_current.pos.x).
                                // The two roots are in quadrature and both reverse
                                // on each sleep(.02) callback. Accept either common
                                // flicker phase and 12px of 60Hz callback quantization.
                                let phase_beat = if beat > 305.0 { 305.0 - beat } else { beat };
                                let theta = phase_beat * std::f32::consts::FRAC_PI_8;
                                let offsets = [theta.sin(), theta.cos()]
                                    .map(|value| 180.0 * height * value);
                                assert!([-1.0f32, 1.0].into_iter().any(|flicker| {
                                    (0..2).all(|player|
                                        (xs[player] - (center - flicker * offsets[player])).abs()
                                            < 12.0 * height)
                                }), "ending orbit at beat {beat}: P1/P2={xs:?}, native offsets={offsets:?}");
                                // These probes avoid the authored crossings at
                                // beats 290/298/311: separation is 180*sqrt(2).
                                assert!((p2 - p1).abs() > 230.0 * height,
                                    "ending fields overlap at beat {beat}: P1/P2={xs:?}");
                            } else {
                                // The beat-320 action explicitly separates the
                                // roots; subsequent gunfire commands tween zoom.
                                let expected = [center - 320.0 * height, center + 320.0 * height];
                                assert!((0..2).all(|player| (xs[player] - expected[player]).abs() < 1.0),
                                    "ending split at beat {beat}: P1/P2={xs:?}, expected={expected:?}");
                            }
                        }
                        assert!(state.song_lua.is_none(), "ending playback must remain Baked");
                    },
                );
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn dyscontrolled_post_pad_matches_native_two_player_taps() {
        dyscontrolled_matches_native_taps(
            include_bytes!(
                "../../../tests/fixtures/itgmania-song-lua-micro/dyscontrolled-post-pad-native.json.zst"
            ),
            true,
            DyscontrolledNativeSection::PostPad,
        );
    }

    #[test]
    fn dyscontrolled_post_pad_matches_native_single_player_taps() {
        dyscontrolled_matches_native_taps(
            include_bytes!(
                "../../../tests/fixtures/itgmania-song-lua-micro/dyscontrolled-post-pad-single-native.json.zst"
            ),
            false,
            DyscontrolledNativeSection::PostPad,
        );
    }

    #[test]
    fn dyscontrolled_ending_matches_native_two_player_taps() {
        dyscontrolled_matches_native_taps(
            include_bytes!(
                "../../../tests/fixtures/itgmania-song-lua-micro/dyscontrolled-ending-native.json.zst"
            ),
            true,
            DyscontrolledNativeSection::Ending,
        );
    }

    #[derive(Clone, Copy)]
    enum DyscontrolledNativeSection {
        PostPad,
        Ending,
    }

    /// The post-pad Mini/Bumpy transition and signed Hidden/Sudden modifiers
    /// exercise Single P1 and Versus. In Versus's stars section P2 remains
    /// visible during frames where P1 has no submitted taps. The geometric oracle
    /// compares both meshes, material visibility and pass order with native
    /// input clocks during compilation and playback; no native actor state is
    /// imported. Optional GPU probes also exercise ordinary production Baked
    /// compilation with its default clock. The ending also covers separated and
    /// crossing Player orbits, the final split, and their return to the center.
    fn dyscontrolled_matches_native_taps(
        native_bytes: &[u8],
        versus: bool,
        section: DyscontrolledNativeSection,
    ) {
        let player_count = if versus { 2 } else { 1 };
        let mode_name = if versus { "Versus" } else { "Single P1" };
        let ending = matches!(section, DyscontrolledNativeSection::Ending);
        let section_name = if ending { "ending" } else { "post-pad" };
        let beat_range = if ending { 284.0..345.0 } else { 43.0..104.0 };
        assert!(!ending || versus, "ending oracle captures both players");
        #[derive(serde::Deserialize)]
        struct NativeSource {
            revision: String,
            modified_src: bool,
        }
        #[derive(serde::Deserialize)]
        struct NativePlayer {
            player: String,
            noteskin: String,
            mini: f32,
            preferred_visual_delay_seconds: f32,
        }
        #[derive(serde::Deserialize)]
        struct NativePreferences {
            global_offset_seconds: f32,
            default_sync_offset: String,
            visual_delay_seconds: f32,
            music_rate: f32,
        }
        #[derive(serde::Deserialize)]
        struct NativeCatalogPreferences {
            initialization: String,
            global_offset_seconds: f32,
            music_rate: f32,
        }
        #[derive(serde::Deserialize)]
        struct NativeInput {
            window_visible: bool,
            player_options: String,
            players: Vec<NativePlayer>,
            preferences: NativePreferences,
            catalog_preferences: NativeCatalogPreferences,
            native_source: NativeSource,
            fast_note_rendering_requested: bool,
            fast_note_rendering: bool,
            fast_note_rendering_at_end: bool,
        }
        #[derive(serde::Deserialize)]
        struct NativeCapabilities {
            native_fast_note_rendering: bool,
            native_song_position_clocks: bool,
            native_option_clock_deltas: bool,
            #[serde(default)]
            native_arrow_update_clocks: bool,
            #[serde(default)]
            native_player_actor_positions: bool,
            #[serde(default)]
            native_player_current_options: bool,
            native_pre_theme_preferences: bool,
            native_model_render_state_protocol: u32,
            native_model_render_state_backend: String,
        }
        #[derive(serde::Deserialize)]
        struct NativeOptionDeltas {
            song: f32,
            players: [f32; 2],
        }
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum NativeArrowPlayerClocks {
            Values {
                tipsy_seconds: Option<f64>,
                tan_tipsy_seconds: Option<f64>,
            },
            Empty([(); 0]),
        }
        impl NativeArrowPlayerClocks {
            fn times(&self) -> [Option<f64>; 2] {
                match self {
                    Self::Values {
                        tipsy_seconds,
                        tan_tipsy_seconds,
                    } => [*tipsy_seconds, *tan_tipsy_seconds],
                    Self::Empty(_) => [None; 2],
                }
            }
        }
        #[derive(serde::Deserialize)]
        struct NativeArrowClocks {
            update_seconds: f64,
            players: [NativeArrowPlayerClocks; 2],
        }
        #[derive(serde::Deserialize)]
        struct NativeUpdate {
            sequence: u64,
            native_update_frame: u64,
            delta: f32,
            beat: f32,
            seconds: f32,
            song_beat: f32,
            visible_beat: f32,
            visible_seconds: f32,
            wall_seconds: f64,
            option_deltas: NativeOptionDeltas,
            arrow_clock_inputs: Option<NativeArrowClocks>,
        }
        #[derive(Debug, serde::Deserialize)]
        struct NativeCurrentOptions {
            perspective_tilt: f32,
            skew: f32,
            mini: f32,
            brake: f32,
            drunk: f32,
            tipsy: f32,
        }
        #[derive(Debug, serde::Deserialize)]
        struct NativePlayerPosition {
            player: usize,
            x: f32,
            y: f32,
            z: f32,
            current_options: Option<NativeCurrentOptions>,
        }
        #[derive(serde::Deserialize)]
        struct NativePlayerPositionFrame {
            update_sequence: u64,
            native_update_frame: u64,
            players: Vec<NativePlayerPosition>,
        }
        #[derive(serde::Deserialize)]
        struct NativeContext {
            kind: String,
            player: usize,
            column: u32,
            draw_beat: f32,
            note_beat: f32,
            part: u32,
        }
        #[derive(serde::Deserialize)]
        struct NativeMaterial {
            diffuse: [f32; 4],
        }
        #[derive(serde::Deserialize)]
        struct NativeBlend {
            enabled: bool,
            src_rgb: u32,
            dst_rgb: u32,
            src_alpha: u32,
            dst_alpha: u32,
            equation_rgb: u32,
            equation_alpha: u32,
        }
        #[derive(serde::Deserialize)]
        struct NativeDepth {
            test_enabled: bool,
            write: bool,
            function: u32,
            range: [f32; 2],
        }
        #[derive(serde::Deserialize)]
        struct NativeCull {
            enabled: bool,
            face: u32,
        }
        #[derive(serde::Deserialize)]
        struct NativeTexture {
            unit: u32,
            enabled: bool,
            mode: i32,
            texture: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct NativeRenderState {
            backend: String,
            lighting: bool,
            current_color: [f32; 4],
            material: NativeMaterial,
            blend: NativeBlend,
            depth: NativeDepth,
            cull: NativeCull,
            textures: Vec<NativeTexture>,
        }
        #[derive(serde::Deserialize)]
        struct NativeModelState {
            render_target: u64,
            native_draw_sequence: u64,
            mesh_index: u32,
            mesh_name: String,
            note_context: Option<NativeContext>,
            anchors_clip: [[f32; 4]; 4],
            render_state: NativeRenderState,
        }
        #[derive(serde::Deserialize)]
        struct NativeModelSample {
            update_sequence: u64,
            geometry: u64,
            state: NativeModelState,
        }
        #[derive(serde::Deserialize)]
        struct NativeGeometry {
            id: u64,
            vertices: Vec<[f32; 3]>,
        }
        #[derive(serde::Deserialize)]
        struct NativeReceptorVisit {
            update_sequence: u64,
        }
        #[derive(serde::Deserialize)]
        struct NativeCapture {
            oracle: String,
            input: NativeInput,
            capabilities: NativeCapabilities,
            dropped_model_geometry_samples: u64,
            dropped_drawable_samples: u64,
            dropped_receptor_visits: u64,
            dropped_script_errors: u64,
            script_errors: Vec<serde_json::Value>,
            update_frames: Vec<NativeUpdate>,
            #[serde(default)]
            player_actor_position_samples: Vec<NativePlayerPositionFrame>,
            model_geometry_meshes: Vec<NativeGeometry>,
            model_geometry_samples: Vec<NativeModelSample>,
            receptor_visits: Vec<NativeReceptorVisit>,
        }
        #[derive(Clone, Debug)]
        struct NativeTap {
            player: usize,
            column: u32,
            note_beat: f32,
            part: u32,
            mesh: usize,
            draw_sequence: u64,
            tint: [f32; 4],
            texture_mask: bool,
            texture: String,
            blend: deadlib_render_core::BlendMode,
            depth_test: bool,
            depth_write: bool,
            cull_mode: f32,
            // Origin and 32-unit X/Y/Z probes in native OpenGL clip space.
            anchors: [[f32; 4]; 4],
        }
        #[derive(Debug)]
        struct ActualTap {
            ordinal: usize,
            mesh: usize,
            anchors: [[f32; 4]; 4],
            tint: [f32; 4],
            texture_mask: bool,
            texture: String,
            blend: deadlib_render_core::BlendMode,
            depth_test: bool,
            depth_write: bool,
            cull_mode: f32,
        }
        let texture_name = |name: &str| {
            // Model residency uses a distinct key for the same source image.
            deadsync_assets::textures::texture_source_key(name)
                .replace('\\', "/")
                .rsplit('/')
                .next()
                .unwrap()
                .to_ascii_lowercase()
        };
        #[derive(Clone)]
        struct NativeFrame {
            beat: f32,
            taps: Vec<NativeTap>,
        }
        // Read only the needed channels from the large native trace. In
        // particular, do not retain every Lua actor snapshot in a JSON Value.
        let decoder = zstd::stream::read::Decoder::new(native_bytes).unwrap();
        let native: NativeCapture =
            serde_json::from_reader(std::io::BufReader::new(decoder)).unwrap();
        assert_eq!(native.oracle, "itgmania_notefield_render_trace");
        assert!(!native.input.window_visible);
        assert_eq!(native.input.player_options, "m690, -10% mini, cel");
        assert_eq!(
            native.input.native_source.revision,
            "663525dcb86eb570d0ef6a2e209678436660ae26"
        );
        assert!(!native.input.native_source.modified_src);
        assert!(native.capabilities.native_fast_note_rendering);
        assert!(native.capabilities.native_song_position_clocks);
        assert!(native.capabilities.native_option_clock_deltas);
        if ending {
            assert!(native.capabilities.native_arrow_update_clocks);
            assert!(native.capabilities.native_player_actor_positions);
            assert!(native.capabilities.native_player_current_options);
        }
        assert!(native.capabilities.native_pre_theme_preferences);
        assert_eq!(native.capabilities.native_model_render_state_protocol, 1);
        assert_eq!(
            native.capabilities.native_model_render_state_backend,
            "opengl"
        );
        assert_eq!(
            native.input.preferences.default_sync_offset,
            "SyncOffset_ITG"
        );
        assert_eq!(native.input.preferences.visual_delay_seconds, 0.0);
        assert_eq!(native.input.preferences.music_rate, 1.0);
        assert!((native.input.preferences.global_offset_seconds + 0.008).abs() < 0.000001);
        assert_eq!(
            native.input.catalog_preferences.initialization,
            "native-catalog-note-time-calculation"
        );
        assert_eq!(
            native.input.catalog_preferences.global_offset_seconds,
            native.input.preferences.global_offset_seconds
        );
        assert_eq!(
            native.input.catalog_preferences.music_rate,
            native.input.preferences.music_rate
        );
        for update in &native.update_frames {
            if ending && update.song_beat >= 0.0 {
                let clocks = update
                    .arrow_clock_inputs
                    .as_ref()
                    .expect("native ArrowEffects cache clocks on every active update");
                assert!(clocks.update_seconds.is_finite() && clocks.update_seconds >= 0.0);
                assert!(
                    clocks
                        .players
                        .iter()
                        .flat_map(NativeArrowPlayerClocks::times)
                        .flatten()
                        .all(|time| time.is_finite() && time >= 0.0)
                );
            }
            assert_eq!(
                update.beat, update.visible_beat,
                "legacy geometry beat aliases the visible clock"
            );
            assert_eq!(
                update.seconds, update.visible_seconds,
                "zero visual delay in this capture"
            );
            assert_eq!(
                update.song_beat, update.visible_beat,
                "zero visual delay in this capture"
            );
            assert!(
                [
                    update.seconds,
                    update.song_beat,
                    update.visible_seconds,
                    update.visible_beat
                ]
                .into_iter()
                .all(f32::is_finite)
            );
            assert!(
                std::iter::once(update.option_deltas.song)
                    .chain(update.option_deltas.players)
                    .all(|delta| delta.is_finite() && delta >= 0.0),
                "native option approach clocks at update {}",
                update.sequence
            );
        }
        assert!(
            native.input.fast_note_rendering_requested
                && native.input.fast_note_rendering
                && native.input.fast_note_rendering_at_end,
            "native Model depth must persist between taps"
        );
        assert_eq!(
            native.input.players.len(),
            player_count,
            "native {mode_name} joined players"
        );
        for (index, player) in native.input.players.iter().enumerate() {
            assert_eq!(player.player, format!("PlayerNumber_P{}", index + 1));
            assert_eq!(player.noteskin, "cel");
            assert!((player.mini + 0.1).abs() < 1e-6);
            assert_eq!(player.preferred_visual_delay_seconds, 0.0);
        }
        assert_eq!(
            [
                native.dropped_model_geometry_samples,
                native.dropped_drawable_samples,
                native.dropped_receptor_visits,
                native.dropped_script_errors
            ],
            [0; 4]
        );
        assert!(
            native.script_errors.is_empty(),
            "native script errors: {:?}",
            native.script_errors
        );

        // Receptor visits prove that native actually drew these frames even
        // when the chart intentionally submitted no taps for either player.
        let drawn_updates: std::collections::HashSet<_> = native
            .receptor_visits
            .iter()
            .map(|visit| visit.update_sequence)
            .collect();
        let mut frames: std::collections::BTreeMap<u64, NativeFrame> = native
            .update_frames
            .iter()
            .filter(|update| {
                beat_range.contains(&update.visible_beat)
                    && drawn_updates.contains(&update.sequence)
            })
            .map(|update| {
                (
                    update.sequence,
                    NativeFrame {
                        beat: update.visible_beat,
                        taps: Vec::new(),
                    },
                )
            })
            .collect();
        assert!(
            frames.len() > if ending { 1_400 } else if versus { 2_300 } else { 1_400 },
            "native {mode_name} {section_name} draw coverage: {}",
            frames.len()
        );
        let geometries: std::collections::HashMap<_, _> = native
            .model_geometry_meshes
            .into_iter()
            .map(|geometry| (geometry.id, geometry.vertices))
            .collect();
        let mut checked_geometries = std::collections::HashSet::new();
        let mut native_vertices: [Option<Vec<[f32; 3]>>; 2] = [None, None];
        for sample in native.model_geometry_samples {
            let state = sample.state;
            let Some(context) = state.note_context else {
                continue;
            };
            // Cel redirects hold/roll heads to the same tap Model. Native
            // NotePart_Tap is 0 and NotePart_HoldHead is 4 (NoteDisplay.h).
            // They share geometry and texture in the composed frame, so match
            // both without discarding either player's overlapping submissions.
            if context.kind != "actor"
                || !matches!(context.part, 0 | 4)
                || state.render_target != 0
                || !beat_range.contains(&context.draw_beat)
            {
                continue;
            }
            let mesh = state.mesh_index as usize;
            assert!(mesh < 2, "Cel Model mesh {mesh}: {}", state.mesh_name);
            assert_eq!(state.mesh_name, ["cells", "scroller"][mesh]);
            if checked_geometries.insert(sample.geometry) {
                let vertices = geometries
                    .get(&sample.geometry)
                    .expect("native Cel geometry");
                if let Some(expected) = &native_vertices[mesh] {
                    assert_eq!(
                        vertices, expected,
                        "Cel's unskinned mesh {mesh} is shared by every tap"
                    );
                } else {
                    native_vertices[mesh] = Some(vertices.clone());
                }
            }
            assert!(context.player < player_count && context.column < 4);
            let update = &native.update_frames[(sample.update_sequence - 1) as usize];
            assert_eq!(update.sequence, sample.update_sequence);
            assert!((update.visible_beat - context.draw_beat).abs() < 0.0001);
            let frame = frames
                .get_mut(&sample.update_sequence)
                .expect("native tap update has independent draw coverage");
            assert!((frame.beat - context.draw_beat).abs() < 0.0001);
            assert!(
                frame
                    .taps
                    .last()
                    .is_none_or(|previous| previous.draw_sequence < state.native_draw_sequence),
                "native Model pass order must preserve actual draw sequence"
            );
            // Keep native submission multiplicity: diffuse/glow can draw the
            // same mesh twice, and distinct players can overlap exactly.
            // A unit basis alone allows a visibly wrong scale within the origin
            // tolerance. Extend the captured native basis to an arrow-sized span.
            let anchors = std::array::from_fn(|anchor| {
                std::array::from_fn(|axis| {
                    state.anchors_clip[0][axis]
                        + 32.0 * (state.anchors_clip[anchor][axis] - state.anchors_clip[0][axis])
                })
            });
            let render = state.render_state;
            assert_eq!(render.backend, "opengl");
            assert!(!render.lighting, "Cel uses native unlit material color");
            // SetMaterial's unlit color includes ambient/emissive RGB. Its
            // alpha is diffuse alpha; native glow can exceed one before the
            // fixed-function pipeline clamps it. Model colors are floats.
            let tint = render.current_color.map(|value| value.clamp(0.0, 1.0));
            assert!((tint[3] - render.material.diffuse[3].clamp(0.0, 1.0)).abs() < 0.00001);
            assert!(render.blend.enabled);
            assert_eq!(
                [render.blend.equation_rgb, render.blend.equation_alpha],
                [32774; 2]
            );
            assert_eq!([render.blend.src_alpha, render.blend.dst_alpha], [1, 771]);
            let blend = match (render.blend.src_rgb, render.blend.dst_rgb) {
                (770, 771) => deadlib_render_core::BlendMode::Alpha,
                (770, 1) => deadlib_render_core::BlendMode::Add,
                other => panic!("unexpected native Cel blend: {other:?}"),
            };
            assert_eq!(render.depth.function, 515, "native GL_LEQUAL Model depth");
            // Native SetZBias reserves the first 5% at the default bias. This
            // shared monotone depth mapping does not alter model clip geometry.
            assert_eq!(render.depth.range, [0.05, 1.0]);
            let cull_mode = if !render.cull.enabled {
                0.0
            } else {
                match render.cull.face {
                    1029 => 1.0,
                    1028 => 2.0,
                    face => panic!("unexpected native cull face {face}"),
                }
            };
            let texture = render
                .textures
                .iter()
                .find(|texture| texture.unit == 0)
                .unwrap();
            assert!(texture.enabled && matches!(texture.mode, 0 | 1));
            assert!(
                render
                    .textures
                    .iter()
                    .filter(|texture| texture.unit > 0)
                    .all(|texture| !texture.enabled),
                "Cel has no additive texture stage"
            );
            frame.taps.push(NativeTap {
                player: context.player,
                column: context.column,
                note_beat: context.note_beat,
                part: context.part,
                mesh,
                draw_sequence: state.native_draw_sequence,
                tint,
                texture_mask: texture.mode == 1,
                texture: texture_name(texture.texture.as_deref().expect("native bound texture")),
                blend,
                depth_test: render.depth.test_enabled,
                depth_write: render.depth.write,
                cull_mode,
                anchors,
            });
        }
        let unique_counts = |frame: &NativeFrame| -> [usize; 2] {
            std::array::from_fn(|player| {
                frame
                    .taps
                    .iter()
                    .filter(|tap| tap.player == player)
                    .map(|tap| (tap.column, tap.note_beat.to_bits(), tap.part))
                    .collect::<std::collections::HashSet<_>>()
                    .len()
            })
        };
        let p2_star_frames = frames
            .values()
            .filter(|frame| (72.0..96.0).contains(&frame.beat))
            .filter(|frame| unique_counts(frame)[1] != 0)
            .count();
        let p2_star_taps: usize = frames
            .values()
            .filter(|frame| (72.0..96.0).contains(&frame.beat))
            .map(|frame| unique_counts(frame)[1])
            .sum();
        if ending {
            assert!(frames.first_key_value().unwrap().1.beat < 284.1
                && frames.last_key_value().unwrap().1.beat > 344.9,
                "native ending covers the centered entrance through the outro");
            for stage in [284.0..288.0, 288.0..304.0, 304.0..318.0,
                318.0..320.0, 320.0..334.0, 334.0..344.0]
            {
                let counts = frames.values().filter(|frame| stage.contains(&frame.beat))
                    .fold([0usize; 2], |mut total, frame| {
                        for (total, count) in total.iter_mut().zip(unique_counts(frame)) {
                            *total += count;
                        }
                        total
                    });
                assert!(counts.iter().all(|count| *count > 0),
                    "native ending stage {stage:?} requires both players' taps: {counts:?}");
            }
        } else if versus {
            assert!(
                p2_star_frames > 500 && p2_star_taps > 1_000,
                "native P2 stars coverage: {p2_star_frames} frames, {p2_star_taps} taps"
            );
        } else {
            assert_eq!((p2_star_frames, p2_star_taps), (0, 0));
        }
        let probes: Vec<f32> = if ending {
            // Include genuine crossings as well as the separated orbit, the
            // 304..305 phase pause, splitter, and final return to the center.
            // Post-320 probes are outside the gunfire's half-beat shake windows.
            vec![284.0, 286.0, 288.0, 290.0, 294.0, 298.0, 302.0, 304.5,
                305.0, 311.0, 315.0, 318.25, 319.5, 321.5, 323.5, 327.5,
                329.5, 331.5, 334.25, 338.0, 344.0]
        } else {
            [43.25, 43.5, 43.75]
                .into_iter()
                .chain((88..=104).map(|half| half as f32 * 0.5))
                .chain((53..=71).map(|beat| beat as f32))
                .chain((72..=96).map(|beat| beat as f32))
                // Fractional zoom/flip, Tornado/Dizzy reversals, and the authored
                // collapse to zero Player zoom must survive between integer beats.
                .chain([
                    78.25, 78.5, 78.75, 86.5, 86.75, 87.25, 87.5, 87.75, 94.25, 94.5, 94.75,
                ])
                .chain((!versus).then_some(79.25))
                .chain((!versus).then_some(91.25))
                .chain([98.0, 100.0, 102.0, 103.5])
                .collect()
        };
        let mut selected = std::collections::BTreeMap::new();
        for probe in probes {
            let (&sequence, frame) = frames
                .iter()
                .find(|(_, frame)| frame.beat >= probe)
                .unwrap_or_else(|| panic!("native tap frame at beat {probe}"));
            assert!(
                frame.beat - probe < 0.1,
                "native frame gap near beat {probe}: {}",
                frame.beat
            );
            selected.insert(sequence, frame.clone());
        }
        assert_eq!(selected.len(), if ending { 21 } else if versus { 79 } else { 81 });
        if ending {
            const GUNFIRE_BEATS: [f32; 15] = [320.01, 320.75, 322.0, 322.75,
                324.0, 324.75, 325.5, 326.25, 328.0, 328.75, 330.0, 330.75,
                332.0, 332.75, 333.5];
            assert!(selected.values().all(|frame| GUNFIRE_BEATS.iter()
                .all(|&start| !(start..start + 0.5).contains(&frame.beat))),
                "ending comparison must not absorb native random screen shake");
            assert!(selected.last_key_value().unwrap().1.taps.is_empty(),
                "native outro no longer submits tap models at beat 344");
        } else {
            // Mini reaches 200% at beat64. Integer-only probes can miss the brief
            // interval where native field scale is below the reverse divisor's
            // safety threshold. Retain every observed frame around that collapse.
            let collapse_frames: Vec<_> = frames
                .iter()
                .filter(|(_, frame)| (63.95..=64.05).contains(&frame.beat))
                .collect();
            assert!(
                collapse_frames.len() >= 2,
                "native coverage around zero Mini scale"
            );
            for (&sequence, frame) in collapse_frames {
                selected.insert(sequence, frame.clone());
            }
        }
        drop(frames);
        let native_vertices =
            native_vertices.map(|vertices| vertices.expect("both native Cel meshes"));
        assert!(native_vertices.iter().all(|vertices| !vertices.is_empty()));
        let native_player_positions: std::collections::BTreeMap<_, _> = native
            .player_actor_position_samples.into_iter()
            .map(|sample| (sample.update_sequence, sample)).collect();
        if ending {
            assert_eq!(native_player_positions.len(), drawn_updates.len());
        }
        let updates = native.update_frames;
        let native_preferences = native.input.preferences;
        // Default/Game mod timers use process elapsed time, independently of
        // the song and actor delta clocks. Ordinary playback below advances
        // that clock naturally from the same song-start epoch.
        let first_active = updates
            .iter()
            .find(|update| update.song_beat >= 0.0)
            .expect("native song-start clock");
        let game_time_origin = first_active.wall_seconds
            - f64::from(first_active.seconds / native_preferences.music_rate);
        let fixture = song_fixture::from_source("Dyscontrolled Galaxy/steps.sm");
        let simfile = fixture.simfile.clone();
        std::thread::Builder::new().stack_size(256 << 20).spawn(move || {
            let play_style = if versus {
                profile_data::PlayStyle::Versus
            } else {
                profile_data::PlayStyle::Single
            };
            with_session(play_style, profile_data::PlayerSide::P1, true, versus, || {
                let metrics = space::Metrics::centered(854.0, 480.0);
                space::set_current_metrics(metrics);
                space::set_current_window_px(1600, 900);
                let profiles = || std::array::from_fn(|_| profile_data::Profile {
                    noteskin: profile_data::NoteSkin::new("cel"),
                    scroll_speed: ScrollSpeedSetting::MMod(690.0),
                    mini_percent: -10,
                    ..Default::default()
                });
                let session = || GameplaySession {
                    play_style: if versus {
                        deadsync_gameplay::GameplayInputPlayStyle::Versus
                    } else {
                        deadsync_gameplay::GameplayInputPlayStyle::Single
                    },
                    joined_sides: [true, versus],
                    ..Default::default()
                };
                // Use the preferences observed by the native renderer.
                // Production context/runtime apply ITG's group sync offset once;
                // do not shift the chart or captured music timestamps manually.
                let native_config = || GameplayConfig {
                    global_offset_seconds: native_preferences.global_offset_seconds,
                    visual_delay_seconds: native_preferences.visual_delay_seconds,
                    machine_pack_ini_offsets: true,
                    machine_default_sync_pref: deadsync_chart::SyncPref::Itg,
                    ..Default::default()
                };
                let native_clocks = |context: &mut deadsync_song_lua::SongLuaCompileContext| {
                    let rate = deadsync_song_lua::song_music_rate(context);
                    let timing = context.song_timing.as_ref().expect("chart timing");
                    let origin = timing.get_time_for_beat_exact(0.0);
                    assert!((origin - 0.059).abs() < 0.000001,
                        "native timing origin includes global and ITG sync offsets: {origin}");
                    let mut previous_actor_time = timing.get_song_time_for_beat(0.0, rate);
                    let mut inputs = vec![[f64::from(origin), 0.0, 0.0]];
                    let mut screen_deltas = vec![0.0];
                    let mut option_deltas = vec![[0.0; 3]];
                    // Match generated_foreground_clock_inputs in the semantic
                    // harness. Foreground.cpp advances actors by music-time
                    // differences, not the screen's update delta. Its initial
                    // beat-zero sample precedes every active captured frame.
                    // These are inputs only: Lua still computes all actor and
                    // modifier state, including authored sleep(0.02) queues.
                    for update in updates.iter().filter(|update| update.song_beat >= 0.0) {
                        let music = f64::from(update.seconds);
                        assert!(music.is_finite() && update.song_beat.is_finite()
                            && music > inputs.last().unwrap()[0],
                            "increasing native foreground clock at update {}", update.sequence);
                        let delta = (update.seconds - previous_actor_time).max(0.0) / rate;
                        inputs.push([music, f64::from(update.song_beat), f64::from(delta)]);
                        // Player/wrapper tweens advance on Screen::Update's
                        // delta; Foreground replaces that with music elapsed.
                        screen_deltas.push(update.delta);
                        option_deltas.push([
                            update.option_deltas.song,
                            update.option_deltas.players[0],
                            update.option_deltas.players[1],
                        ]);
                        previous_actor_time = update.seconds;
                    }
                    assert!(inputs.len() > 2);
                    context.compile_clock_inputs = inputs;
                    context.compile_screen_actor_deltas = screen_deltas;
                    context.compile_option_clock_deltas = option_deltas;
                };
                let mut state = build_test_state_with_lua_mode_and_clocks(
                    &simfile, GameplayViewport::design(), session(), profiles(),
                    "Vulkan (wgpu)", None, false, Some(&native_clocks), Some(native_config()));
                assert!((state.gameplay.timing().get_time_for_beat_exact(0.0) - 0.059).abs() < 0.000001,
                    "runtime timing matches the native compiler clock");
                state.set_live_autoplay_enabled(true);
                let mut assets = fixture_assets();
                let mut tap_textures = std::collections::HashMap::new();
                for player in 0..player_count {
                    let ns = state.noteskin_assets.noteskin[player].as_ref().unwrap();
                    for (slot, layers) in ns.notes.iter().zip(&ns.note_layers) {
                        assert_eq!(layers.len(), native_vertices.len());
                        assert_eq!(layers[0].texture_key(), slot.texture_key());
                        // Cells and scroller share a PNG: the latter's animated
                        // texture INI refers to Tap Note parts too. Geometry,
                        // rather than texture identity, distinguishes the meshes.
                        for (mesh, (layer, vertices)) in layers.iter().zip(&native_vertices).enumerate() {
                            let model = layer.model.as_ref().expect("Cel tap Model mesh");
                            // MilkShape XYZ is retained verbatim. Rotation and
                            // Y conversion belong to the instance matrix.
                            assert!(!layer.def.mirror_h && !layer.def.mirror_v);
                            assert_eq!(model.vertices.len(), vertices.len());
                            assert!(model.vertices.iter().zip(vertices).all(|(actual, native)|
                                (0..3).all(|axis| (actual.pos[axis] - native[axis]).abs() < 0.00001)),
                                "Cel mesh {mesh} uses native model coordinates");
                            let key = layer.texture_key();
                            tap_textures.insert(FIXTURE_TEXTURES.texture_handle(key), texture_name(key));
                        }
                    }
                }
                assert!(!tap_textures.is_empty());
                let visuals = state.gameplay.song_lua_visuals();
                for overlay in visuals.overlays.iter().chain(
                    visuals.background_visual_layers.iter().chain(&visuals.foreground_visual_layers)
                        .flat_map(|layer| &layer.overlays),
                ) {
                    let deadsync_assets::song_lua::SongLuaOverlayKind::Model { layers } = &overlay.kind
                        else { continue; };
                    for layer in layers.iter() {
                        for key in std::iter::once(&layer.texture_key).chain(layer.additive.as_ref()).chain(
                            layer.texture_frames.iter().chain(layer.additive_frames.iter())
                                .filter_map(|frame| frame.texture_key.as_ref()),
                        ) { assets.reserve_texture_handle(key.to_string()); }
                    }
                }
                #[cfg(target_os = "windows")]
                let mut capture = std::env::var_os("DEADSYNC_DYSCONTROLLED_CAPTURE_DIR")
                    .map(|output| SongLuaNotePixelCapture::new(&state, &metrics, PathBuf::from(output)));
                #[cfg(target_os = "windows")]
                let mut next_capture = 0;
                #[cfg(target_os = "windows")]
                let capture_beats = if ending {
                    [288.0, 294.0, 315.0, 319.5, 321.5, 338.0]
                } else {
                    [46.0, 54.0, 74.0, 80.0, 88.0, 96.0]
                };
                let mut compared = 0;
                let mut compared_p2_taps = 0;
                let mut compared_player_taps = [0usize; 2];
                let mut max_clip_error = 0.0f32;
                let mut max_pixel_error = 0.0f32;
                let mut bounded_shake_frames = 0;
                let mut parity_errors = Vec::<(f32, String)>::new();
                let last_sequence = *selected.last_key_value().unwrap().0;
                state.set_current_music_time_ns(deadsync_core::song_time::song_time_ns_from_seconds(
                    updates.first().expect("native preroll update").seconds));
                for update in updates {
                    if update.sequence > last_sequence { break; }
                    // Native deltas and music positions are separate clocks;
                    // replay both, including preroll and modifier approaches.
                    let music = deadsync_core::song_time::song_time_ns_from_seconds(update.seconds);
                    let host = (update.wall_seconds * 1e9) as u64;
                    // ModsGroup::Update samples its own timer before the screen
                    // update. Reuse that observed input only for option approach.
                    state.gameplay.set_player_option_clock_deltas(update.option_deltas.players);
                    if let Some(clocks) = &update.arrow_clock_inputs {
                        state.presentation.set_notefield_tipsy_clock_inputs(
                            clocks.players.each_ref().map(|player| player.times()[0]),
                        );
                    } else {
                        state.presentation.set_notefield_tipsy_clock_inputs([None; 2]);
                    }
                    screen_gameplay::update(&mut state, update.delta, deadsync_gameplay::GameplayAudioSnapshot {
                        stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                            music_nanos: music, has_music_mapping: true, valid_at_host_nanos: host,
                            ..Default::default()
                        },
                        ..Default::default()
                    }, || host, &mut Vec::new());
                    // Pin presentation after the normal update. Resetting it
                    // before update makes the display smoother add this frame's
                    // delta twice. This setter leaves attack approach history
                    // intact; each captured delta advances modifiers only once.
                    state.set_current_music_time_ns(music);
                    let output = state.stage.take_output();
                    assert!(output.errors.is_empty(), "Lua update {} at beat {}: {:?}",
                        update.sequence, update.beat, output.errors);
                    let Some(expected) = selected.get(&update.sequence) else { continue; };
                    let beat = expected.beat;
                    if ending {
                        let positions = native_player_positions
                            .get(&update.sequence)
                            .expect("native Player current positions for selected drawn frame");
                        assert_eq!(positions.native_update_frame, update.native_update_frame);
                        assert_eq!(positions.players.len(), player_count);
                        for (player, native_position) in positions.players.iter().enumerate() {
                            assert_eq!(native_position.player, player);
                            assert!(
                                [native_position.x, native_position.y, native_position.z]
                                    .into_iter()
                                    .all(f32::is_finite)
                            );
                            let pose = state.gameplay.song_lua_player_transform(player);
                            for (axis, actual, expected) in [
                                ("X", pose.x.expect("ending Player X is authored"), native_position.x),
                                ("Y", pose.y.unwrap_or(240.0), native_position.y),
                                ("Z", pose.z, native_position.z),
                            ] {
                                if !actual.is_finite() || (actual - expected).abs() >= 0.01 {
                                    parity_errors.push((beat, format!(
                                        "update {}: Player{} current {axis}: native {expected}, actual {actual}",
                                        update.sequence, player + 1,
                                    )));
                                }
                            }
                            // Current values are independently observed outputs.
                            // Only the fixture's clocks enter the implementation.
                            let expected_options = native_position.current_options.as_ref()
                                .expect("native Player Current options for selected drawn frame");
                            let perspective = state.gameplay
                                .effective_perspective_effects_for_player_with_base(
                                    player, deadsync_gameplay::PerspectiveEffects::default());
                            let visual = state.gameplay
                                .effective_visual_effects_for_player_with_mask(player, 0);
                            let accel = state.gameplay
                                .effective_accel_effects_for_player_with_mask(player, 0);
                            let mini = state.gameplay
                                .effective_mini_percent_for_player_with_base(player, -10.0) * 0.01;
                            for (name, actual, expected) in [
                                ("Tilt", perspective.tilt, expected_options.perspective_tilt),
                                ("Skew", perspective.skew, expected_options.skew),
                                ("Mini", mini, expected_options.mini),
                                ("Brake", accel.brake, expected_options.brake),
                                ("Drunk", visual.drunk, expected_options.drunk),
                                ("Tipsy", visual.tipsy, expected_options.tipsy),
                            ] {
                                if !actual.is_finite() || !expected.is_finite()
                                    || (actual - expected).abs() >= 0.0001
                                {
                                    parity_errors.push((beat, format!(
                                        "update {}: Player{} Current {name}: native {expected}, actual {actual}",
                                        update.sequence, player + 1,
                                    )));
                                }
                            }
                        }
                    }
                    let frame = compose_fixture_frame_with_view_and_clock(
                        &mut state, &assets, &metrics, &mut Vec::new(),
                        &mut compose::TextLayoutCache::default(), &mut compose::ComposeScratch::default(),
                        screen_gameplay::ActorViewOverride::default(), &FIXTURE_TEXTURES,
                        Some(update.wall_seconds));
                    assert!(frame.render_targets.is_empty(), "{section_name} Player screen draws at beat {beat}");
                    let cel_geometries: Vec<_> = frame.tmesh_geometries.iter().map(|geometry| {
                        let vertices = geometry.vertices.as_ref();
                        let mesh = native_vertices.iter().position(|native| vertices.len() == native.len()
                            && vertices.iter().zip(native).all(|(actual, native)|
                                (0..3).all(|axis| (actual.pos[axis] - native[axis]).abs() < 0.00001)));
                        if mesh.is_some() {
                            // Model material color is carried by instance.tint;
                            // hidden vertex alpha must not evade that comparison.
                            assert!(vertices.iter().all(|vertex| vertex.color == [1.0; 4]));
                        }
                        mesh
                    }).collect();
                    let mut actual = Vec::<ActualTap>::new();
                    for op in &frame.ops {
                        let DrawOp::TexturedMesh(run) = op else { continue; };
                        let Some(texture) = tap_textures.get(&run.texture_handle) else { continue; };
                        let Some(mesh) = cel_geometries[run.geometry as usize] else { continue; };
                        let camera = frame.cameras.get(run.camera as usize).copied()
                            .unwrap_or_else(|| metrics.projection());
                        for instance in &frame.tmesh_instances[run.instance_start as usize
                            ..(run.instance_start + run.instance_count) as usize]
                        {
                            let matrix = camera * instance.transform();
                            actual.push(ActualTap {
                                ordinal: actual.len(),
                                mesh,
                                anchors: [matrix.w_axis.to_array(),
                                    (matrix.w_axis + 32.0 * matrix.x_axis).to_array(),
                                    (matrix.w_axis + 32.0 * matrix.y_axis).to_array(),
                                    (matrix.w_axis + 32.0 * matrix.z_axis).to_array()],
                                tint: instance.tint,
                                texture_mask: instance.texture_mask != 0.0,
                                texture: texture.clone(),
                                blend: run.blend,
                                depth_test: run.depth_test,
                                depth_write: run.depth_write,
                                cull_mode: instance.cull_mode,
                            });
                        }
                    }
                    let counts = unique_counts(expected);
                    if actual.len() != expected.taps.len() {
                        parity_errors.push((beat, format!(
                            "update {}: native unique P1/P2 {counts:?}; tap/hold-head submissions {} vs native {}",
                            update.sequence, actual.len(), expected.taps.len())));
                        compared += 1;
                        continue;
                    }
                    let mut frame_error = None;
                    let clip_translation = if !ending && (102.01..103.8).contains(&beat) {
                        // The chart's TopScreen Vibrate magnitude is bounded
                        // by 30 native pixels in X/Y; GetZoom stays 1 here.
                        // Actor::PreDraw consumes independent randomf(-1,1)
                        // values, which this fixture does not record. Allow
                        // only their bounded common world translation, through
                        // Player's 45-degree menu camera. Every relative mesh
                        // basis/position and all Z/W remain strict below.
                        let focal = 1.0 / (45.0_f32.to_radians() * 0.5).tan();
                        let bounds = [60.0 * focal, 60.0 * focal * 854.0 / 480.0];
                        let expected_probes: Vec<_> = expected.taps.iter()
                            .map(|tap| (tap.mesh, tap.texture_mask, tap.anchors)).collect();
                        let actual_probes: Vec<_> = actual.iter()
                            .map(|tap| (tap.mesh, tap.texture_mask, tap.anchors)).collect();
                        bounded_shake_frames += 1;
                        native_clip_translation_within_bounds(&expected_probes, &actual_probes, bounds)
                            .unwrap_or_else(|| {
                                frame_error = Some(format!(
                                    "update {}: geometry cannot be matched by one bounded native screen-shake translation",
                                    update.sequence));
                                [0.0; 2]
                            })
                    } else {
                        [0.0; 2]
                    };
                    let clip_translation = [clip_translation[0], clip_translation[1], 0.0, 0.0];
                    let mut last_draw_by_note = std::collections::HashMap::new();
                    for tap in &expected.taps {
                        let distance = |anchors: &[[f32; 4]; 4]| (0..4).flat_map(|anchor|
                            (0..4).map(move |axis| (anchors[anchor][axis] - tap.anchors[anchor][axis]
                                - clip_translation[axis]).abs()))
                            .fold(0.0f32, f32::max);
                        let ordering = |a: &ActualTap, b: &ActualTap| {
                            let identity = |candidate: &ActualTap|
                                candidate.mesh == tap.mesh && candidate.texture_mask == tap.texture_mask;
                            identity(b).cmp(&identity(a))
                                .then_with(|| distance(&a.anchors).total_cmp(&distance(&b.anchors)))
                                .then_with(|| a.ordinal.cmp(&b.ordinal))
                        };
                        let index = actual.iter().enumerate().min_by(|(_, a), (_, b)| ordering(a, b))
                            .map(|(index, _)| index).unwrap();
                        let nearest = actual.swap_remove(index);
                        let error = distance(&nearest.anchors);
                        if !(nearest.anchors.iter().flatten().all(|value| value.is_finite()) && error < 0.5
                            && (0..4).all(|anchor| nearest.anchors[anchor][3].signum() == tap.anchors[anchor][3].signum()))
                        {
                            frame_error.get_or_insert_with(|| format!(
                                "update {}, native {tap:?}, actual {nearest:?}, max clip error {error}", update.sequence));
                        }
                        if nearest.mesh != tap.mesh || nearest.texture_mask != tap.texture_mask
                            || nearest.texture != tap.texture || nearest.blend != tap.blend
                            || nearest.depth_test != tap.depth_test || nearest.depth_write != tap.depth_write
                            || nearest.cull_mode != tap.cull_mode
                            || !(0..4).all(|axis| (nearest.tint[axis] - tap.tint[axis]).abs() < 0.0001)
                        {
                            frame_error.get_or_insert_with(|| format!(
                                "native material/pass state {tap:?}, actual {nearest:?}"));
                        }
                        let key = (tap.player, tap.column, tap.note_beat.to_bits(), tap.part);
                        if let Some(previous) = last_draw_by_note.insert(key, nearest.ordinal)
                            && previous >= nearest.ordinal
                        {
                            frame_error.get_or_insert_with(|| format!(
                                "native Model mesh/pass order for {key:?}: actual draw {previous} precedes {}", nearest.ordinal));
                        }
                        max_clip_error = max_clip_error.max(error);
                        // Both use OpenGL clip coordinates. Check origin and
                        // axis probes so scale/rotation errors cannot hide behind
                        // a correct center, while Z/W retain camera-side evidence.
                        for (anchor, (actual, native)) in nearest.anchors.iter().zip(&tap.anchors).enumerate() {
                            if native[3] > 0.0 && (0..3).all(|axis| native[axis].abs() < native[3]) {
                                let x_error = 427.0 * ((actual[0] - clip_translation[0]) / actual[3] - native[0] / native[3]);
                                let y_error = 240.0 * ((actual[1] - clip_translation[1]) / actual[3] - native[1] / native[3]);
                                let pixels = x_error.hypot(y_error);
                                if !pixels.is_finite() || pixels >= 0.5 {
                                    frame_error.get_or_insert_with(|| format!(
                                        "visible basis {anchor}, native {tap:?}, actual {nearest:?}, pixel error {pixels}"));
                                }
                                max_pixel_error = max_pixel_error.max(pixels);
                            }
                        }
                    }
                    if let Some(error) = frame_error {
                        parity_errors.push((beat, error));
                    }
                    if (72.0..=96.1).contains(&beat) { compared_p2_taps += counts[1]; }
                    for (total, count) in compared_player_taps.iter_mut().zip(counts) {
                        *total += count;
                    }
                    #[cfg(target_os = "windows")]
                    if let Some(capture) = capture.as_mut()
                        && parity_errors.is_empty()
                        && next_capture < capture_beats.len() && beat >= capture_beats[next_capture]
                    {
                        let prefix = if ending {
                            "dyscontrolled-ending-native-clock"
                        } else if versus {
                            "dyscontrolled-native-clock"
                        } else {
                            "dyscontrolled-single-native-clock"
                        };
                        let pixels = capture.capture_named_at_game_time(
                            &mut state, &metrics, beat, prefix, Some(update.wall_seconds));
                        if expected.taps.is_empty() {
                            assert_eq!(pixels, 0, "native submits no tap/hold-head models at beat {beat}");
                        } else if expected.taps.iter().any(|tap| tap.tint[3] > 1.0 / 256.0 && tap.anchors[0][3] > 0.0
                            && (0..3).all(|axis| tap.anchors[0][axis].abs() < tap.anchors[0][3]))
                        {
                            assert!(pixels > 100, "native-visible {section_name} taps at beat {beat}: {pixels} pixels");
                        }
                        next_capture += 1;
                    }
                    compared += 1;
                }
                assert_eq!(compared, selected.len());
                let error_sections = if ending {
                    [284.0..288.0, 288.0..304.0, 304.0..306.0, 306.0..318.0,
                        318.0..320.0, 320.0..334.0, 334.0..345.0]
                } else {
                    [43.0..68.0, 68.0..72.0, 72.0..80.0, 80.0..86.0,
                        86.0..90.0, 90.0..98.0, 98.0..104.0]
                };
                assert!(parity_errors.is_empty(),
                    "native {section_name} parity failures at beats {:?}; max clip error {max_clip_error}, max pixel error {max_pixel_error}; examples across sections: {:?}",
                    parity_errors.iter().map(|(beat, _)| *beat).collect::<Vec<_>>(),
                    error_sections.into_iter()
                        .filter_map(|section| parity_errors.iter().find(|(beat, _)| section.contains(beat)))
                        .collect::<Vec<_>>());
                if ending {
                    assert!(compared_player_taps.iter().all(|count| *count > 50),
                        "compared ending P1/P2 taps: {compared_player_taps:?}");
                    assert_eq!(bounded_shake_frames, 0, "ending geometry has no shake allowance");
                } else if versus {
                    assert!(compared_p2_taps > 100, "compared P2 stars taps: {compared_p2_taps}");
                } else {
                    assert_eq!(compared_p2_taps, 0);
                }
                #[cfg(target_os = "windows")]
                if capture.is_some() { assert_eq!(next_capture, capture_beats.len(), "all {section_name} captures must run"); }
                eprintln!("Dyscontrolled Galaxy {mode_name} {section_name} matched {compared} frames and {compared_player_taps:?} P1/P2 taps ({compared_p2_taps} P2 stars taps); {bounded_shake_frames} frames allow only bounded common screen shake; max clip residual {max_clip_error}, max pixel residual {max_pixel_error}");
                #[cfg(target_os = "windows")]
                if let Some(capture) = capture.as_mut() {
                    // Reuse the GPU backend/EventLoop and identical texture
                    // keys. This state takes the ordinary production Baked path,
                    // with no imported clocks, and advances at normal 60 Hz.
                    drop(state);
                    let mut production = build_test_state_with_lua_mode_and_clocks(&simfile,
                        GameplayViewport::design(), session(), profiles(), "Vulkan (wgpu)",
                        None, false, None, Some(native_config()));
                    production.set_live_autoplay_enabled(true);
                    production.set_current_music_time_ns(0);
                    let timing = production.gameplay.timing().clone();
                    assert!((timing.get_time_for_beat_exact(0.0) - 0.059).abs() < 0.000001,
                        "production Baked timing uses the same native preferences");
                    let step = |state: &mut screen_gameplay::State, at: f32, delta: f32| {
                        let music = deadsync_core::song_time::song_time_ns_from_seconds(at);
                        let host = (f64::from(at) * 1e9) as u64 + 1_000_000;
                        screen_gameplay::update(state, delta, deadsync_gameplay::GameplayAudioSnapshot {
                            stream_clock: deadsync_gameplay::GameplayStreamClockSnapshot {
                                music_nanos: music, has_music_mapping: true, valid_at_host_nanos: host,
                                ..Default::default()
                            },
                            ..Default::default()
                        }, || host, &mut Vec::new());
                        state.set_current_music_time_ns(music);
                        let output = state.stage.take_output();
                        assert!(output.errors.is_empty(), "production Baked Lua at {at}s: {:?}", output.errors);
                    };
                    let mut now = 0.0f32;
                    let mut positive_captures = 0;
                    let mut empty_captures = 0;
                    let mut observational_captures = 0;
                    let probes: &[(f32, Option<bool>)] = if ending {
                        // Callback cadence changes the flicker phase in ordinary
                        // playback. Keep images for review; the native-clock path
                        // above checks exact geometry and the final empty frame.
                        &[(294.0, None), (302.0, None), (315.0, None),
                            (321.5, None), (338.0, None), (344.0, Some(false))]
                    } else if versus {
                        &[
                            (46.0, Some(true)),
                            (54.0, Some(true)),
                            (66.0, Some(false)),
                            (74.0, Some(true)),
                            (80.0, Some(true)),
                            (88.0, Some(true)),
                            (96.0, Some(true)),
                        ]
                    } else {
                        &[
                            (46.0, Some(true)),
                            (54.0, Some(true)),
                            (66.0, Some(false)),
                            (74.0, Some(true)),
                            // Native P1 is empty throughout 78.54..80.06 and
                            // 90.77..92.07. Probe inside those intervals.
                            (79.25, Some(false)),
                            (80.0, None),
                            (88.0, Some(true)),
                            (91.25, Some(false)),
                            // Reset boundaries depend on callback cadence;
                            // retain these images without a pixel assertion.
                            (96.0, None),
                        ]
                    };
                    for &(beat, should_show_taps) in probes {
                        let evidence = selected.values().find(|frame| frame.beat >= beat).unwrap();
                        assert!(evidence.beat - beat < 0.1);
                        if should_show_taps == Some(true) {
                            assert!(evidence.taps.iter().any(|tap| tap.tint[3] > 1.0 / 256.0 && tap.anchors[0][3] > 0.0
                                && (0..3).all(|axis| tap.anchors[0][axis].abs() < tap.anchors[0][3])),
                                "native evidence for visible production taps at beat {beat}");
                        } else if should_show_taps == Some(false) {
                            assert!(evidence.taps.is_empty(), "native empty interval at beat {beat}");
                        }
                        let target = timing.get_time_for_beat_exact(beat);
                        while now + 1.0 / 60.0 < target {
                            now += 1.0 / 60.0;
                            step(&mut production, now, 1.0 / 60.0);
                        }
                        step(&mut production, target, target - now);
                        now = target;
                        let prefix = if ending {
                            "dyscontrolled-ending-production"
                        } else if versus {
                            "dyscontrolled-production"
                        } else {
                            "dyscontrolled-single-production"
                        };
                        let pixels = capture.capture_named_at_game_time(
                            &mut production, &metrics, beat, prefix,
                            Some(game_time_origin + f64::from(target / native_preferences.music_rate)));
                        if should_show_taps == Some(true) {
                            assert!(pixels > 100, "production Baked taps at beat {beat}: {pixels} pixels");
                            positive_captures += 1;
                        } else if should_show_taps == Some(false) {
                            assert_eq!(pixels, 0, "production Baked empty interval at beat {beat}");
                            empty_captures += 1;
                        } else {
                            observational_captures += 1;
                        }
                    }
                    assert_eq!(positive_captures, if ending { 0 } else if versus { 6 } else { 4 });
                    assert_eq!(empty_captures, if ending || versus { 1 } else { 3 });
                    assert_eq!(observational_captures, if ending { 5 } else if versus { 0 } else { 2 });
                }
            });
        }).unwrap().join().unwrap();
    }
}
