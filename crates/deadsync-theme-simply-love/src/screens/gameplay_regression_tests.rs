use deadsync_core::input::{MAX_COLS, MAX_PLAYERS};
use deadsync_gameplay::{
    CourseDisplayCarry, CourseDisplayTiming, CourseDisplayTotals, GameplayConfig,
    GameplayMiniIndicatorData, GameplayNoteskinData, GameplayNoteskinEffects,
    GameplayReceptorGlowBehavior, GameplayReceptorStepBehavior, GameplayRuntimeState,
    GameplaySession, GameplayTween, GameplayViewport, LeadInTiming, MINE_EXPLOSION_DURATION,
    RECEPTOR_STEP_WINDOWS, ReplayInputEdge, ReplayOffsetSnapshot, TAP_EXPLOSION_WINDOWS,
    refresh_active_attack_masks,
};
use deadsync_rules::scroll::ScrollSpeedSetting;

use deadsync_profile_gameplay::GameplayProfile;

type State = GameplayRuntimeState<
    GameplayProfile,
    deadsync_song_lua::SongLuaOverlayActor<deadsync_assets::song_lua::SongLuaOverlayKind>,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>,
>;

#[cfg(test)]
mod tests {
    use super::*;

    use super::{MAX_COLS, MAX_PLAYERS, ScrollSpeedSetting, refresh_active_attack_masks};
    use crate::screens::gameplay as screen_gameplay;
    use deadlib_present::{
        compose::{self, TextureContext, TextureMeta},
        space,
    };
    use deadlib_render::{
        ObjectType,
        draw_prep::{self, DrawScratch},
        frame_compare::{compare_draw_scratch, compare_render_lists},
    };
    use deadsync_assets::noteskin::{self, Noteskin};
    use deadsync_assets::song_lua::compile_song_lua;
    use deadsync_chart::SongData;
    use deadsync_chart::{ChartData, GameplayChartData};
    use deadsync_core::note::NoteType;
    use deadsync_noteskin::{
        NoteskinSlot, ReceptorGlowBehavior, ReceptorStepBehavior, Style, TweenType,
    };
    use deadsync_profile as profile_data;
    use deadsync_profile::compat as profile;
    use deadsync_rules::judgment::{JudgeGrade, Judgment, TimingWindow};
    use std::sync::{Arc, LazyLock, Mutex};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn model_cache_prewarms_more_than_legacy_slot_limit() {
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
        let assets = screen_gameplay::GameplayNoteskinAssets {
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
        song_lua_data: screen_gameplay::GameplaySongLuaData,
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

    fn with_session<R>(
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
        f: impl FnOnce() -> R,
    ) -> R {
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

    #[inline(always)]
    fn test_gameplay_tween(tween: TweenType) -> super::GameplayTween {
        match tween {
            TweenType::Linear => super::GameplayTween::Linear,
            TweenType::Accelerate => super::GameplayTween::Accelerate,
            TweenType::Decelerate => super::GameplayTween::Decelerate,
        }
    }

    #[inline(always)]
    fn test_gameplay_receptor_glow_behavior(
        behavior: ReceptorGlowBehavior,
    ) -> super::GameplayReceptorGlowBehavior {
        super::GameplayReceptorGlowBehavior {
            press_duration: behavior.press_duration,
            press_alpha_start: behavior.press_alpha_start,
            press_alpha_end: behavior.press_alpha_end,
            press_zoom_start: behavior.press_zoom_start,
            press_zoom_end: behavior.press_zoom_end,
            press_tween: test_gameplay_tween(behavior.press_tween),
            duration: behavior.duration,
            alpha_start: behavior.alpha_start,
            alpha_end: behavior.alpha_end,
            zoom_start: behavior.zoom_start,
            zoom_end: behavior.zoom_end,
            tween: test_gameplay_tween(behavior.tween),
            blend_add: behavior.blend_add,
        }
    }

    #[inline(always)]
    fn test_gameplay_receptor_step_behavior(
        behavior: ReceptorStepBehavior,
    ) -> super::GameplayReceptorStepBehavior {
        super::GameplayReceptorStepBehavior {
            duration: behavior.duration,
            zoom_start: behavior.zoom_start,
            zoom_end: behavior.zoom_end,
            tween: test_gameplay_tween(behavior.tween),
            interrupts: behavior.interrupts,
        }
    }

    fn test_noteskin_data(
        cols_per_player: usize,
        num_players: usize,
        player_profiles: &[profile_data::Profile; MAX_PLAYERS],
        session: &super::GameplaySession,
    ) -> super::GameplayNoteskinData {
        let style = Style {
            num_cols: cols_per_player,
            num_players: 1,
        };
        let mut runtime_profiles = (*player_profiles).clone();
        if session.p2_runtime_player() {
            runtime_profiles[0] = runtime_profiles[1].clone();
        }
        let noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= num_players {
                return None;
            }
            let skin = runtime_profiles[player].noteskin.to_string();
            noteskin::load_itg_skin_cached(&style, &skin).ok()
        });
        let mine_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= num_players {
                return None;
            }
            let skin = runtime_profiles[player]
                .resolved_mine_noteskin()
                .to_string();
            noteskin::load_itg_skin_cached(&style, &skin)
                .ok()
                .or_else(|| noteskin[player].clone())
        });
        let receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] =
            std::array::from_fn(|player| {
                if player >= num_players {
                    return None;
                }
                let skin = runtime_profiles[player]
                    .resolved_receptor_noteskin()
                    .to_string();
                noteskin::load_itg_skin_cached(&style, &skin)
                    .ok()
                    .or_else(|| noteskin[player].clone())
            });
        let tap_explosion_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] =
            std::array::from_fn(|player| {
                if player >= num_players {
                    return None;
                }
                let Some(skin) = runtime_profiles[player].resolved_tap_explosion_noteskin() else {
                    return None;
                };
                noteskin::load_itg_skin_cached(&style, skin.as_str())
                    .ok()
                    .or_else(|| noteskin[player].clone())
            });
        let mut effects = super::GameplayNoteskinEffects::default();
        let cols = cols_per_player.min(MAX_COLS);
        for player in 0..num_players.min(MAX_PLAYERS) {
            let receptor_ns = receptor_noteskin[player]
                .as_deref()
                .or_else(|| noteskin[player].as_deref());
            if let Some(ns) = receptor_ns {
                effects.set_receptor_glow_behavior(
                    player,
                    test_gameplay_receptor_glow_behavior(ns.receptor_glow_behavior),
                );
                for col in 0..cols {
                    for window in super::RECEPTOR_STEP_WINDOWS {
                        effects.set_receptor_step_behavior(
                            player,
                            col,
                            window,
                            test_gameplay_receptor_step_behavior(
                                ns.receptor_step_behavior_for_col(col, window),
                            ),
                        );
                    }
                }
            }

            let tap_ns = if runtime_profiles[player].tap_explosion_noteskin_hidden() {
                None
            } else {
                tap_explosion_noteskin[player]
                    .as_deref()
                    .or_else(|| noteskin[player].as_deref())
            };
            if let Some(ns) = tap_ns {
                for col in 0..cols {
                    for window in super::TAP_EXPLOSION_WINDOWS {
                        for bright in [false, true] {
                            effects.set_tap_explosion_duration(
                                player,
                                col,
                                window,
                                bright,
                                ns.tap_explosion_for_col_with_bright(col, window, bright)
                                    .map(|explosion| explosion.duration()),
                            );
                        }
                    }
                }
            }

            let duration = mine_noteskin[player]
                .as_deref()
                .or_else(|| noteskin[player].as_deref())
                .and_then(|ns| ns.mine_hit_explosion.as_ref())
                .map_or(super::MINE_EXPLOSION_DURATION, |explosion| {
                    explosion.duration()
                });
            effects.set_mine_explosion_duration(player, duration);
        }
        super::GameplayNoteskinData { effects }
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
                crate::assets::texture_dims(key)
                    .map(|meta| TextureMeta {
                        w: meta.w,
                        h: meta.h,
                    })
                    .unwrap_or(TextureMeta { w: 64, h: 64 }),
            )
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            crate::assets::sprite_sheet_dims(key)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
            let hash = key.as_bytes().iter().fold(0x811c_9dc5u32, |hash, byte| {
                (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
            });
            u64::from(hash.max(1))
        }
    }

    const FIXTURE_TEXTURES: FixtureTextureContext = FixtureTextureContext;

    fn fixture_assets() -> crate::assets::AssetManager {
        let mut assets = crate::assets::AssetManager::new();
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let asset_root = project_root.join("assets");
        for spec in crate::resources::FONT_ASSETS {
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
        player_profiles: [profile_data::Profile; MAX_PLAYERS],
    ) -> screen_gameplay::State {
        let song = Arc::new(
            deadsync_simfile::app_runtime::parse_song_for_test(simfile, 0.0)
                .expect("generated gameplay fixture should parse"),
        );
        let chart_ix = song
            .charts
            .iter()
            .position(|chart| chart.difficulty.eq_ignore_ascii_case("challenge"))
            .unwrap_or(0);
        let gameplay_chart = Arc::new(
            deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[chart_ix], 0.0)
                .expect("generated gameplay fixture chart should load")
                .remove(0),
        );
        let chart = Arc::new(song.charts[chart_ix].clone());
        let session = GameplaySession::default();
        let charts = [chart.clone(), chart];
        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
        let scroll_speed = [
            player_profiles[0].scroll_speed,
            player_profiles[1].scroll_speed,
        ];
        let noteskin_data = test_noteskin_data(
            session.play_style.cols_per_player(),
            session.play_style.player_count(),
            &player_profiles,
            &session,
        );
        let runtime_profiles =
            deadsync_profile_gameplay::gameplay_runtime_profile_data(&player_profiles, &session);
        let noteskin_assets = screen_gameplay::gameplay_noteskin_assets(
            session.play_style.cols_per_player(),
            session.play_style.player_count(),
            &runtime_profiles,
        );
        screen_gameplay::State::from_gameplay(
            init(
                song,
                charts,
                gameplay_charts,
                viewport,
                session,
                GameplayConfig::default(),
                deadsync_chart::SyncPref::Default,
                GameplayMiniIndicatorData::default(),
                noteskin_data,
                screen_gameplay::GameplaySongLuaData::default(),
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
        )
    }

    fn set_fixture_time(state: &mut screen_gameplay::State, music_time: f32) {
        state.boundary.total_elapsed_in_screen = 10.0;
        state.clock.song_position.current_music_time_display = music_time;
        state.clock.visible_timing.current_music_time = [music_time; MAX_PLAYERS];
        state.clock.song_position.current_beat =
            state.timing_runtime.timing.get_beat_for_time(music_time);
        refresh_active_attack_masks(&mut state.gameplay, 0.0);
    }

    fn add_sprite_core_feedback(state: &mut screen_gameplay::State) {
        let judgment = Judgment {
            time_error_ms: -7.0,
            time_error_music_ns: -7_000_000,
            grade: JudgeGrade::Fantastic,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        state.players_runtime.players[0].combo = 42;
        state.players_runtime.players[0].full_combo_grade = Some(JudgeGrade::Fantastic);
        state.players_runtime.players[0].current_combo_grade = Some(JudgeGrade::Fantastic);
        state.players_runtime.players[0].judgment_counts[0] = 42;
        state.set_last_judgment(0, judgment);
        state.error_bar_register_tap(0, &judgment, 2.5);
        state.trigger_tap_judgment_explosion(0, 0, &judgment);
    }

    fn compose_fixture_frame(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render::RenderList {
        actors.clear();
        screen_gameplay::push_actors(
            actors,
            state,
            assets,
            screen_gameplay::ActorViewOverride::default(),
            123.0,
            crate::views::SimplyLoveVisualPolicyView::default(),
        );
        compose::build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
            actors,
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            assets.fonts(),
            10.0,
            text_cache,
            scratch,
            &FIXTURE_TEXTURES,
            state.actor_resources(),
        )
    }

    fn assert_repeatable_frame(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render::RenderList {
        let mut warm =
            compose_fixture_frame(state, assets, metrics, actors, text_cache, compose_scratch);
        compose_scratch.recycle_render_list(&mut warm);

        let mut expected_frame =
            compose_fixture_frame(state, assets, metrics, actors, text_cache, compose_scratch);
        let expected = expected_frame.clone();
        compose_scratch.recycle_render_list(&mut expected_frame);
        let mut actual =
            compose_fixture_frame(state, assets, metrics, actors, text_cache, compose_scratch);

        assert!(
            expected
                .objects
                .iter()
                .any(|object| matches!(object.object_type, ObjectType::Sprite(_)))
        );
        assert!(
            expected
                .objects
                .iter()
                .any(|object| matches!(object.object_type, ObjectType::TexturedMesh { .. }))
        );
        assert!(!expected.batches.is_empty());
        assert_eq!(compare_render_lists(&expected, &actual), Ok(()));

        let mut expected_draw = DrawScratch::default();
        let mut actual_draw = DrawScratch::default();
        draw_prep::prepare(&expected, &mut expected_draw, |_, _| false);
        draw_prep::prepare(&actual, &mut actual_draw, |_, _| false);
        assert!(!expected_draw.ops.is_empty());
        assert_eq!(compare_draw_scratch(&expected_draw, &actual_draw), Ok(()));
        compose_scratch.recycle_render_list(&mut actual);
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
;
"#
    }

    fn write_fixture(name: &str, contents: &str) -> PathBuf {
        let song_dir = test_dir(name);
        let simfile = song_dir.join(format!("{name}.ssc"));
        fs::write(&simfile, contents).unwrap();
        simfile
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
                let metrics = space::metrics_for_window(640, 480);
                space::set_current_metrics(metrics);
                space::set_current_window_px(640, 480);
                space::set_overscan(0, 0, 0, 0);

                let mut profiles = [
                    profile_data::Profile::default(),
                    profile_data::Profile::default(),
                ];
                profiles[0].noteskin = profile_data::NoteSkin::new("lambda");
                profiles[0].scroll_speed = ScrollSpeedSetting::XMod(2.0);
                let mut state =
                    build_test_state(&simfile, GameplayViewport::new(640.0, 480.0), profiles);
                set_fixture_time(&mut state, 2.5);
                add_sprite_core_feedback(&mut state);

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

                let assets = fixture_assets();
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
                assert!(expected.objects.len() > without_feedback.objects.len());
                compose_scratch.recycle_render_list(&mut without_feedback);

                state.profiles_runtime.profiles[0]
                    .set_scroll_option(profile_data::ScrollOption::Reverse);
                state.refresh_live_notefield_options(120.0);
                add_sprite_core_feedback(&mut state);
                let reverse = assert_repeatable_frame(
                    &mut state,
                    &assets,
                    &metrics,
                    &mut actors,
                    &mut text_cache,
                    &mut compose_scratch,
                );
                assert_ne!(compare_render_lists(&expected, &reverse), Ok(()));
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
                end_time_ns: state.chart_runtime.hold_end_time_cache_ns[note_index]
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
        state.hold_runtime.active_holds[0] = Some(live_hold);
        state.hold_runtime.active_holds[1] = Some(live_roll);

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
    fn hold_mine_frame_is_structurally_repeatable() {
        let simfile = write_fixture("f0-hold-mine", generated_hold_mine_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                let metrics = space::metrics_for_window(640, 480);
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
                let mut state =
                    build_test_state(&simfile, GameplayViewport::new(640.0, 480.0), profiles);
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
                assert!(state.display.visual_feedback.mine_explosions[3].is_some());
                assert!(state.display.hold_feedback.hold_judgments[2].is_some());
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
                assert_ne!(compare_render_lists(&normal, &reverse), Ok(()));

                state.hold_runtime.active_holds.fill(None);
                state.display.hold_feedback.clear();
                state.display.visual_feedback.mine_explosions.fill(None);
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
                    compare_render_lists(&reverse, &without_live_feedback),
                    Ok(())
                );
                compose_scratch.recycle_render_list(&mut without_live_feedback);
            },
        );
    }

    fn generated_runtime_mod_lua() -> &'static str {
        r#"
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
        let simfile = song_dir.join("generated_lua_regression.ssc");
        fs::write(&simfile, generated_lua_song_simfile()).unwrap();
        simfile
    }

    #[test]
    fn gameplay_handles_generated_song_lua_actor_build() {
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
                        let noteskin_data = test_noteskin_data(
                            session.play_style.cols_per_player(),
                            session.play_style.player_count(),
                            &player_profiles,
                            &session,
                        );
                        let runtime_profiles =
                            deadsync_profile_gameplay::gameplay_runtime_profile_data(
                                &player_profiles,
                                &session,
                            );
                        let noteskin_assets = screen_gameplay::gameplay_noteskin_assets(
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
                            })
                            .map(|compiled| screen_gameplay::GameplayCompiledSongLua {
                                compiled,
                                compile_ms: 0.0,
                            });
                        let song_lua_data = screen_gameplay::GameplaySongLuaData {
                            primary,
                            ..Default::default()
                        };
                        let mut state = screen_gameplay::State::from_gameplay(
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
                            times.push((window.start_second + window.end_second) * 0.5);
                            times.push(window.end_second);
                            times.push(window.sustain_end_second);
                        }
                        times.sort_by(f32::total_cmp);
                        times.dedup_by(|a, b| (*a - *b).abs() <= 0.001);

                        let assets = crate::assets::AssetManager::new();
                        for time in times {
                            state.clock.song_position.current_music_time_display = time;
                            state.clock.visible_timing.current_music_time = [time; MAX_PLAYERS];
                            state.clock.song_position.current_beat =
                                state.timing_runtime.timing.get_beat_for_time(time);
                            refresh_active_attack_masks(&mut state.gameplay, 0.0);
                            let mut actors = Vec::new();
                            screen_gameplay::push_actors(
                                &mut actors,
                                &mut state,
                                &assets,
                                screen_gameplay::ActorViewOverride::default(),
                                123.0,
                                Default::default(),
                            );
                        }
                    },
                );
            })
            .expect("song-lua actor build regression thread should spawn")
            .join()
            .expect("song-lua actor build regression thread should finish");
    }
}
