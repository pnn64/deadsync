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
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, LazyLock, Mutex};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::Instant,
    };

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[global_allocator]
    static ALLOC: CountingAlloc = CountingAlloc::new();

    struct CountingAlloc {
        enabled: AtomicBool,
        allocs: AtomicU64,
        reallocs: AtomicU64,
        deallocs: AtomicU64,
        alloc_bytes: AtomicU64,
        realloc_bytes: AtomicU64,
        dealloc_bytes: AtomicU64,
    }

    impl CountingAlloc {
        const fn new() -> Self {
            Self {
                enabled: AtomicBool::new(false),
                allocs: AtomicU64::new(0),
                reallocs: AtomicU64::new(0),
                deallocs: AtomicU64::new(0),
                alloc_bytes: AtomicU64::new(0),
                realloc_bytes: AtomicU64::new(0),
                dealloc_bytes: AtomicU64::new(0),
            }
        }

        fn begin(&self) {
            assert!(!self.enabled.load(Ordering::Relaxed));
            self.allocs.store(0, Ordering::Relaxed);
            self.reallocs.store(0, Ordering::Relaxed);
            self.deallocs.store(0, Ordering::Relaxed);
            self.alloc_bytes.store(0, Ordering::Relaxed);
            self.realloc_bytes.store(0, Ordering::Relaxed);
            self.dealloc_bytes.store(0, Ordering::Relaxed);
            self.enabled.store(true, Ordering::Relaxed);
        }

        fn end(&self) -> AllocCounts {
            self.enabled.store(false, Ordering::Relaxed);
            AllocCounts {
                allocs: self.allocs.load(Ordering::Relaxed),
                reallocs: self.reallocs.load(Ordering::Relaxed),
                deallocs: self.deallocs.load(Ordering::Relaxed),
                alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
                realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
                dealloc_bytes: self.dealloc_bytes.load(Ordering::Relaxed),
            }
        }

        #[inline(always)]
        fn counting(&self) -> bool {
            self.enabled.load(Ordering::Relaxed)
        }
    }

    // SAFETY: every operation delegates to `System` with the caller-provided
    // pointer and layout. The atomics only observe calls while the isolated
    // fixture explicitly enables counting and do not affect ownership.
    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: `layout` is forwarded unchanged to the system allocator.
            let ptr = unsafe { System.alloc(layout) };
            if !ptr.is_null() && self.counting() {
                self.allocs.fetch_add(1, Ordering::Relaxed);
                self.alloc_bytes
                    .fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: `layout` is forwarded unchanged to the system allocator.
            let ptr = unsafe { System.alloc_zeroed(layout) };
            if !ptr.is_null() && self.counting() {
                self.allocs.fetch_add(1, Ordering::Relaxed);
                self.alloc_bytes
                    .fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            ptr
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if self.counting() {
                self.deallocs.fetch_add(1, Ordering::Relaxed);
                self.dealloc_bytes
                    .fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            // SAFETY: the allocator caller supplies the original pointer/layout.
            unsafe { System.dealloc(ptr, layout) };
        }

        unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: all arguments are forwarded unchanged to `System`.
            let out = unsafe { System.realloc(ptr, old, new_size) };
            if !out.is_null() && self.counting() {
                self.reallocs.fetch_add(1, Ordering::Relaxed);
                self.realloc_bytes
                    .fetch_add(new_size as u64, Ordering::Relaxed);
            }
            out
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct AllocCounts {
        allocs: u64,
        reallocs: u64,
        deallocs: u64,
        alloc_bytes: u64,
        realloc_bytes: u64,
        dealloc_bytes: u64,
    }

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
        session: GameplaySession,
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
        let charts = [chart.clone(), chart];
        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
        let scroll_speed = [
            player_profiles[0].scroll_speed,
            player_profiles[1].scroll_speed,
        ];
        let init_view = crate::views::GameplayInitView {
            hud: profile::gameplay_hud_snapshot(),
            ..Default::default()
        };
        screen_gameplay::init(
            song,
            charts,
            gameplay_charts,
            viewport,
            session,
            GameplayConfig::default(),
            5,
            1.0,
            scroll_speed,
            player_profiles,
            None,
            None,
            None,
            Arc::from("EVENT"),
            None,
            None,
            None,
            None,
            None,
            None,
            [0; MAX_PLAYERS],
            init_view,
        )
    }

    fn set_fixture_time(state: &mut screen_gameplay::State, music_time: f32) {
        state.boundary.total_elapsed_in_screen = 10.0;
        state.set_current_music_time_ns(deadsync_core::song_time::song_time_ns_from_seconds(
            music_time,
        ));
        refresh_active_attack_masks(&mut state.gameplay, 0.0);
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
        state.set_last_judgment(player_idx, judgment);
        state.error_bar_register_tap(player_idx, &judgment, 2.5);
        state.trigger_tap_judgment_explosion(player_idx, column, &judgment);
    }

    fn compose_fixture_frame_with_textures<T: TextureContext + ?Sized>(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
        texture_ctx: &T,
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
            texture_ctx,
            state.actor_resources(),
        )
    }

    fn compose_fixture_frame(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render::RenderList {
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
        state: &mut crate::screens::practice::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render::RenderList {
        actors.clear();
        crate::screens::practice::push_actors(
            actors,
            state,
            assets,
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
            state.gameplay.actor_resources(),
        )
    }

    fn prepare_fixture_frame(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
        draw_scratch: &mut DrawScratch,
    ) -> usize {
        let mut render =
            compose_fixture_frame(state, assets, metrics, actors, text_cache, compose_scratch);
        draw_prep::prepare(&render, draw_scratch, |_, _| false);
        let checksum = render
            .objects
            .len()
            .wrapping_add(render.sprite_instances.len())
            .wrapping_add(render.batches.len())
            .wrapping_add(draw_scratch.ops.len());
        compose_scratch.recycle_render_list(&mut render);
        actors.clear();
        checksum
    }

    fn assert_repeatable_frame(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
    ) -> deadlib_render::RenderList {
        assert_repeatable_composition(compose_scratch, |scratch| {
            compose_fixture_frame(state, assets, metrics, actors, text_cache, scratch)
        })
    }

    fn assert_repeatable_composition(
        compose_scratch: &mut compose::ComposeScratch,
        mut compose_frame: impl FnMut(&mut compose::ComposeScratch) -> deadlib_render::RenderList,
    ) -> deadlib_render::RenderList {
        let mut warm = compose_frame(compose_scratch);
        compose_scratch.recycle_render_list(&mut warm);

        let mut expected_frame = compose_frame(compose_scratch);
        let expected = expected_frame.clone();
        compose_scratch.recycle_render_list(&mut expected_frame);
        let mut actual = compose_frame(compose_scratch);

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
        crate::assets::AssetManager,
        space::Metrics,
    ) {
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
                let actor_stats = deadlib_present::actors::actor_tree_stats(&actors);
                assert!(
                    actor_stats.resolved_sprites >= 100,
                    "dense sprite fixture did not select the resolved gameplay path: {actor_stats:?}"
                );
                assert!(expected.objects.len() >= 250);
                assert!(expected.sprite_instances.len() >= 240);
                assert!(expected.batches.len() >= 120);
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
                add_sprite_core_feedback(&mut state, 0, 0, 42);
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

    #[cfg(target_os = "windows")]
    #[derive(Debug)]
    struct GpuCaptureArtifacts {
        normal_path: PathBuf,
        reverse_path: PathBuf,
        manifest_path: PathBuf,
        timing_path: Option<PathBuf>,
    }

    #[cfg(target_os = "windows")]
    struct GpuCaptureApp {
        backend_type: deadlib_render::BackendType,
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
        objects: usize,
        instances: usize,
        batches: usize,
        noteskin_sprites: usize,
    }

    #[cfg(target_os = "windows")]
    const GPU_TIMING_PHASES: [&str; 10] = [
        "frame_pipeline",
        "actor_build",
        "build_screen",
        "compose",
        "sort",
        "draw",
        "backend_setup",
        "draw_prepare",
        "backend_upload",
        "backend_record",
    ];

    #[cfg(target_os = "windows")]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct GpuTimingTail {
        p50: u32,
        p95: u32,
        p99: u32,
        worst: u32,
    }

    #[cfg(target_os = "windows")]
    impl std::fmt::Display for GpuTimingTail {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}/{}/{}/{}", self.p50, self.p95, self.p99, self.worst)
        }
    }

    #[cfg(target_os = "windows")]
    struct GpuTimingSamples {
        phases: [Vec<u32>; GPU_TIMING_PHASES.len()],
    }

    #[cfg(target_os = "windows")]
    impl GpuTimingSamples {
        fn new(frames: usize) -> Self {
            Self {
                phases: std::array::from_fn(|_| Vec::with_capacity(frames)),
            }
        }

        fn push(&mut self, phases: [u32; GPU_TIMING_PHASES.len()]) {
            for (samples, value) in self.phases.iter_mut().zip(phases) {
                samples.push(value);
            }
        }

        fn tails(&mut self) -> [GpuTimingTail; GPU_TIMING_PHASES.len()] {
            std::array::from_fn(|index| gpu_timing_tail(&mut self.phases[index]))
        }
    }

    #[cfg(target_os = "windows")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct GpuTimingStorage {
        actor_capacity: usize,
        compose: compose::ComposeStorageStats,
        draw: deadlib_render::draw_prep::DrawStorageStats,
    }

    #[cfg(target_os = "windows")]
    struct GpuFrameTiming {
        phases: [u32; GPU_TIMING_PHASES.len()],
        storage: GpuTimingStorage,
        sort_fallback: bool,
    }

    #[cfg(target_os = "windows")]
    #[derive(Clone, Copy)]
    struct GpuTimingConfig {
        warmup_frames: usize,
        measured_frames: usize,
    }

    #[cfg(target_os = "windows")]
    #[derive(Default)]
    struct CaptureTextureContext {
        keys: std::cell::RefCell<std::collections::HashSet<String>>,
    }

    #[cfg(target_os = "windows")]
    impl TextureContext for CaptureTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            deadsync_assets::texture_registry_generation()
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            deadsync_assets::texture_dims(key).map(|meta| TextureMeta {
                w: meta.w,
                h: meta.h,
            })
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            deadsync_assets::sprite_sheet_dims(key)
        }

        fn texture_handle(&self, key: &str) -> deadlib_render::TextureHandle {
            self.keys.borrow_mut().insert(key.to_owned());
            deadsync_assets::texture_handle(key)
        }
    }

    #[cfg(target_os = "windows")]
    fn load_gpu_capture_assets(
        assets: &mut deadsync_assets::AssetManager,
        backend: &mut deadlib_renderer::Backend,
    ) -> Result<(), String> {
        let texture_assets = crate::resources::initial_texture_assets().collect::<Vec<_>>();
        assets
            .load_initial_assets(
                backend,
                deadsync_theme::ThemeAssetManifest {
                    fonts: &[],
                    textures: texture_assets.iter().copied(),
                    texture_needs_repeat_sampler: crate::resources::texture_needs_repeat_sampler,
                },
            )
            .map_err(|error| format!("failed to load capture textures: {error}"))?;

        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let asset_root = project_root.join("assets");
        let asset_roots = [asset_root.clone()];
        for spec in crate::resources::FONT_ASSETS {
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
        assets: &mut deadsync_assets::AssetManager,
        backend: &mut deadlib_renderer::Backend,
        key: &str,
        sampler: Option<deadlib_render::SamplerDesc>,
    ) -> Result<(), String> {
        if let Some(sampler) = sampler {
            assets.ensure_texture_for_key_with_sampler(backend, key, sampler);
        } else {
            assets.ensure_texture_for_key(backend, key);
        }
        if assets.has_uploaded_texture_key(key) {
            return Ok(());
        }

        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path =
            deadlib_assets::texture_key_source_path(key, key, |path| project_root.join(path));
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
                crate::resources::texture_needs_repeat_sampler(key),
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
        assets: &mut deadsync_assets::AssetManager,
        backend: &mut deadlib_renderer::Backend,
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
    fn gpu_elapsed_us(started: Instant) -> u32 {
        started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32
    }

    #[cfg(target_os = "windows")]
    fn gpu_timing_tail(samples: &mut [u32]) -> GpuTimingTail {
        if samples.is_empty() {
            return GpuTimingTail::default();
        }
        samples.sort_unstable();
        let percentile = |pct: usize| {
            let rank = samples.len().saturating_mul(pct).saturating_add(99) / 100;
            samples[rank.saturating_sub(1).min(samples.len() - 1)]
        };
        GpuTimingTail {
            p50: percentile(50),
            p95: percentile(95),
            p99: percentile(99),
            worst: samples[samples.len() - 1],
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn gpu_timing_tail_uses_ceiling_percentile_rank() {
        let mut samples = (1..=100).collect::<Vec<_>>();

        assert_eq!(
            gpu_timing_tail(&mut samples),
            GpuTimingTail {
                p50: 50,
                p95: 95,
                p99: 99,
                worst: 100,
            }
        );
    }

    #[cfg(target_os = "windows")]
    fn gpu_timing_config() -> Result<Option<GpuTimingConfig>, String> {
        let Some(measured) = std::env::var_os("DEADSYNC_F0_TIMING_FRAMES") else {
            return Ok(None);
        };
        let measured = measured
            .to_str()
            .ok_or("DEADSYNC_F0_TIMING_FRAMES must be valid Unicode")?
            .parse::<usize>()
            .map_err(|error| format!("invalid DEADSYNC_F0_TIMING_FRAMES: {error}"))?;
        if !(100..=100_000).contains(&measured) {
            return Err("DEADSYNC_F0_TIMING_FRAMES must be between 100 and 100000".to_owned());
        }
        let warmup = match std::env::var("DEADSYNC_F0_WARMUP_FRAMES") {
            Ok(value) => value
                .parse::<usize>()
                .map_err(|error| format!("invalid DEADSYNC_F0_WARMUP_FRAMES: {error}"))?,
            Err(std::env::VarError::NotPresent) => 512,
            Err(error) => {
                return Err(format!("invalid DEADSYNC_F0_WARMUP_FRAMES: {error}"));
            }
        };
        if !(64..=100_000).contains(&warmup) {
            return Err("DEADSYNC_F0_WARMUP_FRAMES must be between 64 and 100000".to_owned());
        }
        Ok(Some(GpuTimingConfig {
            warmup_frames: warmup,
            measured_frames: measured,
        }))
    }

    #[cfg(target_os = "windows")]
    #[expect(
        clippy::too_many_arguments,
        reason = "the timing frame makes every warmed presentation resource explicit"
    )]
    fn run_gpu_timing_frame(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        backend: &mut deadlib_renderer::Backend,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
        collect_sort_timing: bool,
    ) -> Result<GpuFrameTiming, String> {
        let frame_started = Instant::now();
        actors.clear();
        let actor_started = Instant::now();
        screen_gameplay::push_actors(
            actors,
            state,
            assets,
            screen_gameplay::ActorViewOverride::default(),
            123.0,
            crate::views::SimplyLoveVisualPolicyView::default(),
        );
        let actor_us = gpu_elapsed_us(actor_started);

        compose_scratch.begin_frame_stats(collect_sort_timing);
        let build_started = Instant::now();
        let mut render =
            compose::build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
                actors,
                [0.0, 0.0, 0.0, 1.0],
                metrics,
                assets.fonts(),
                10.0,
                text_cache,
                compose_scratch,
                &deadsync_assets::PRESENT_TEXTURE_CONTEXT,
                state.actor_resources(),
            );
        let build_us = gpu_elapsed_us(build_started);
        let compose_frame = compose_scratch.frame_stats();

        let draw_started = Instant::now();
        let draw = backend
            .draw(&render, assets.textures(), false)
            .map_err(|error| format!("timing draw failed: {error}"))?;
        let draw_us = gpu_elapsed_us(draw_started);
        compose_scratch.recycle_render_list(&mut render);
        let frame_us = gpu_elapsed_us(frame_started);

        Ok(GpuFrameTiming {
            phases: [
                frame_us,
                actor_us,
                build_us,
                actor_us.saturating_add(build_us),
                compose_frame.sort_us,
                draw_us,
                draw.backend_setup_us,
                draw.backend_prepare_us,
                draw.backend_upload_us,
                draw.backend_record_us,
            ],
            storage: GpuTimingStorage {
                actor_capacity: actors.capacity(),
                compose: compose_scratch.storage_stats(),
                draw: draw.storage,
            },
            sort_fallback: compose_frame.sort_fallback,
        })
    }

    #[cfg(target_os = "windows")]
    #[expect(
        clippy::too_many_arguments,
        reason = "the timing capture owns one explicit value for each warmed presentation resource"
    )]
    fn capture_gpu_timing(
        state: &mut screen_gameplay::State,
        assets: &crate::assets::AssetManager,
        backend: &mut deadlib_renderer::Backend,
        backend_type: deadlib_render::BackendType,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
        output_dir: &Path,
        config: GpuTimingConfig,
    ) -> Result<PathBuf, String> {
        let mut last_storage = None;
        for _ in 0..config.warmup_frames {
            last_storage = Some(
                run_gpu_timing_frame(
                    state,
                    assets,
                    backend,
                    metrics,
                    actors,
                    text_cache,
                    compose_scratch,
                    false,
                )?
                .storage,
            );
        }
        let mut last_storage = last_storage.expect("warmup count is validated as nonzero");
        let mut storage_change_frames = 0usize;
        let mut sort_fallbacks = 0usize;
        let mut samples = GpuTimingSamples::new(config.measured_frames);
        let measured_started = Instant::now();
        for _ in 0..config.measured_frames {
            let sample = run_gpu_timing_frame(
                state,
                assets,
                backend,
                metrics,
                actors,
                text_cache,
                compose_scratch,
                true,
            )?;
            samples.push(sample.phases);
            sort_fallbacks += usize::from(sample.sort_fallback);
            if sample.storage != last_storage {
                storage_change_frames += 1;
                last_storage = sample.storage;
            }
        }
        let measured_ms = measured_started.elapsed().as_secs_f64() * 1000.0;
        if storage_change_frames != 0 {
            return Err(format!(
                "timing capture changed retained capacity on {storage_change_frames} measured frames"
            ));
        }

        let tails = samples.tails();
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "optimized"
        };
        let mut manifest = format!(
            "fixture=F0-sprite-core\nbackend={backend_type}\nprofile={profile}\n\
             scope=actor-compose-draw\nresolution=640x480\nmusic_time_seconds=2.5\n\
             present_mode=immediate\nvsync=false\npresent_back_pressure=false\n\
             warmup_frames={}\nmeasured_frames={}\nmeasured_duration_ms={measured_ms:.3}\n\
             tail_order=p50/p95/p99/worst_us\nsort_fallbacks={sort_fallbacks}\n\
             storage_change_frames={storage_change_frames}\n",
            config.warmup_frames, config.measured_frames,
        );
        use std::fmt::Write as _;
        for ((name, tail), samples) in GPU_TIMING_PHASES
            .iter()
            .zip(tails)
            .zip(samples.phases.iter())
        {
            writeln!(manifest, "{name}={tail} samples={}", samples.len())
                .expect("writing to a String cannot fail");
        }

        let path = output_dir.join("timing.txt");
        std::fs::write(&path, manifest)
            .map_err(|error| format!("failed to write '{}': {error}", path.display()))?;
        Ok(path)
    }

    #[cfg(target_os = "windows")]
    fn missing_capture_texture_keys(
        texture_ctx: &CaptureTextureContext,
        assets: &crate::assets::AssetManager,
    ) -> Vec<String> {
        texture_ctx
            .keys
            .borrow()
            .iter()
            .filter(|key| {
                let handle = deadsync_assets::texture_handle(key);
                !assets.textures().contains_key(&handle)
            })
            .cloned()
            .collect()
    }

    #[cfg(target_os = "windows")]
    fn missing_capture_texture_report(
        missing_keys: &[String],
        assets: &crate::assets::AssetManager,
    ) -> String {
        missing_keys
            .iter()
            .map(|key| {
                let canonical = deadsync_assets::canonical_texture_key(key);
                let handle = deadsync_assets::texture_handle(key);
                let canonical_handle = deadsync_assets::texture_handle(&canonical);
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
        assets: &mut crate::assets::AssetManager,
        backend: &mut deadlib_renderer::Backend,
        metrics: &space::Metrics,
        actors: &mut Vec<deadlib_present::actors::Actor>,
        text_cache: &mut compose::TextLayoutCache,
        compose_scratch: &mut compose::ComposeScratch,
        output_dir: &Path,
    ) -> Result<(PathBuf, GpuFrameStats), String> {
        let texture_ctx = CaptureTextureContext::default();
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
            compose_scratch.recycle_render_list(&mut render);
            for key in &missing_keys {
                ensure_gpu_capture_texture(assets, backend, key, None)?;
            }
            texture_ctx.keys.borrow_mut().clear();
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
                let handle = deadsync_assets::texture_handle(slot.texture_key());
                if handle != deadlib_render::INVALID_TEXTURE_HANDLE
                    && assets.textures().contains_key(&handle)
                {
                    noteskin_handles.insert(handle);
                }
            });
        }
        let noteskin_sprites = render
            .objects
            .iter()
            .filter(|object| {
                matches!(object.object_type, ObjectType::Sprite(_))
                    && noteskin_handles.contains(&object.texture_handle)
            })
            .count();
        if noteskin_sprites < 200 {
            compose_scratch.recycle_render_list(&mut render);
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
            objects: render.objects.len(),
            instances: render.sprite_instances.len(),
            batches: render.batches.len(),
            noteskin_sprites,
        };
        compose_scratch.recycle_render_list(&mut render);
        Ok((path, stats))
    }

    #[cfg(target_os = "windows")]
    fn capture_sprite_core_gpu(
        event_loop: &winit::event_loop::ActiveEventLoop,
        backend_type: deadlib_render::BackendType,
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
        let mut backend = deadlib_renderer::create_backend(
            backend_type,
            Arc::clone(&window),
            false,
            deadlib_render::PresentModePolicy::Immediate,
            false,
            true,
        )
        .map_err(|error| format!("failed to create {backend_type} backend: {error}"))?;
        let mut assets = deadsync_assets::AssetManager::new();
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
            let timing_path = gpu_timing_config()?
                .map(|config| {
                    capture_gpu_timing(
                        &mut state,
                        &assets,
                        &mut backend,
                        backend_type,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut compose_scratch,
                        output_dir,
                        config,
                    )
                })
                .transpose()?;

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
                 normal_hash={:016x}\nnormal_objects={}\nnormal_instances={}\nnormal_batches={}\n\
                 normal_noteskin_sprites={}\n\
                 reverse_hash={:016x}\nreverse_objects={}\nreverse_instances={}\nreverse_batches={}\n\
                 reverse_noteskin_sprites={}\n",
                normal.hash,
                normal.objects,
                normal.instances,
                normal.batches,
                normal.noteskin_sprites,
                reverse.hash,
                reverse.objects,
                reverse.instances,
                reverse.batches,
                reverse.noteskin_sprites,
            );
            std::fs::write(&manifest_path, manifest).map_err(|error| {
                format!("failed to write '{}': {error}", manifest_path.display())
            })?;
            Ok(GpuCaptureArtifacts {
                normal_path,
                reverse_path,
                manifest_path,
                timing_path,
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
            .parse::<deadlib_render::BackendType>()
            .unwrap_or_else(|error| panic!("invalid DEADSYNC_F0_BACKEND: {error}"));
        assert_ne!(backend_type, deadlib_render::BackendType::Software);
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
                if let Some(timing_path) = artifacts.timing_path {
                    println!("F0 GPU timing: '{}'", timing_path.display());
                }
            },
        );
    }

    #[test]
    fn sprite_core_warmed_pipeline_reports_exact_allocations() {
        const WARMUP_FRAMES: usize = 64;
        const MEASURE_FRAMES: usize = 256;

        let simfile = write_fixture("f0-sprite-core-alloc", generated_sprite_core_simfile());
        with_session(
            profile_data::PlayStyle::Single,
            profile_data::PlayerSide::P1,
            true,
            false,
            || {
                ALLOC.begin();
                let probe = std::hint::black_box(vec![0u8; std::hint::black_box(1_024)]);
                std::hint::black_box(probe.as_slice());
                drop(probe);
                let probe_counts = ALLOC.end();
                assert!(probe_counts.allocs >= 1);
                assert!(probe_counts.deallocs >= 1);
                assert!(probe_counts.alloc_bytes >= 1_024);

                let (mut state, assets, metrics) = sprite_core_fixture(&simfile);
                let mut actors = Vec::with_capacity(512);
                let mut text_cache = compose::TextLayoutCache::default();
                let mut compose_scratch = compose::ComposeScratch::default();
                let mut draw_scratch = DrawScratch::default();
                let mut checksum = 0usize;

                for _ in 0..WARMUP_FRAMES {
                    checksum = checksum.wrapping_add(prepare_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut compose_scratch,
                        &mut draw_scratch,
                    ));
                }
                let capacity_before = (
                    actors.capacity(),
                    compose_scratch.storage_stats(),
                    draw_scratch.storage_stats(),
                );

                ALLOC.begin();
                for _ in 0..MEASURE_FRAMES {
                    checksum = checksum.wrapping_add(prepare_fixture_frame(
                        &mut state,
                        &assets,
                        &metrics,
                        &mut actors,
                        &mut text_cache,
                        &mut compose_scratch,
                        &mut draw_scratch,
                    ));
                }
                let counts = ALLOC.end();
                let capacity_after = (
                    actors.capacity(),
                    compose_scratch.storage_stats(),
                    draw_scratch.storage_stats(),
                );

                assert_ne!(std::hint::black_box(checksum), 0);
                assert_eq!(capacity_after, capacity_before);
                println!(
                    "F0-sprite-core warmed pipeline: frames={MEASURE_FRAMES} \
                     allocs={} ({:.3}/frame) reallocs={} ({:.3}/frame) \
                     deallocs={} ({:.3}/frame) alloc_bytes={} ({:.1}/frame) \
                     realloc_bytes={} ({:.1}/frame) dealloc_bytes={} ({:.1}/frame)",
                    counts.allocs,
                    counts.allocs as f64 / MEASURE_FRAMES as f64,
                    counts.reallocs,
                    counts.reallocs as f64 / MEASURE_FRAMES as f64,
                    counts.deallocs,
                    counts.deallocs as f64 / MEASURE_FRAMES as f64,
                    counts.alloc_bytes,
                    counts.alloc_bytes as f64 / MEASURE_FRAMES as f64,
                    counts.realloc_bytes,
                    counts.realloc_bytes as f64 / MEASURE_FRAMES as f64,
                    counts.dealloc_bytes,
                    counts.dealloc_bytes as f64 / MEASURE_FRAMES as f64,
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
                let metrics = space::metrics_for_window(1280, 720);
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
                        assert!(frame.objects.iter().any(|object| {
                            matches!(
                                object.object_type,
                                ObjectType::TexturedMesh { geom_cache_key, .. }
                                    if geom_cache_key != deadlib_render::INVALID_TMESH_CACHE_KEY
                            )
                        }));
                        frame
                    };

                let split = render_variant(profile_data::ScrollOption::Split, [0.0, 0.0, 1.0, 1.0]);
                let alternate =
                    render_variant(profile_data::ScrollOption::Alternate, [0.0, 1.0, 0.0, 1.0]);
                let cross = render_variant(profile_data::ScrollOption::Cross, [0.0, 1.0, 1.0, 0.0]);
                assert_ne!(compare_render_lists(&split, &alternate), Ok(()));
                assert_ne!(compare_render_lists(&split, &cross), Ok(()));
                assert_ne!(compare_render_lists(&alternate, &cross), Ok(()));
            },
        );
    }

    fn generated_pipeline_song_lua() -> &'static str {
        r#"
local player = nil
local capture = nil
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
    Def.ActorFrameTexture{
        Name="FixtureCapture",
        InitCommand=function(self)
            capture = self
            self:SetTextureName("FixtureCaptureTexture")
            self:SetWidth(320)
            self:SetHeight(180)
            self:Create()
        end,
        Def.ActorProxy{
            Name="FixtureNoteFieldProxy",
            OnCommand=function(self)
                local nf = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
                if nf:GetNumWrapperStates() == 0 then
                    nf:AddWrapperState()
                end
                self:SetTarget(nf:GetWrapperState(1))
                self:visible(true)
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

    fn write_pipeline_song_lua_fixture() -> PathBuf {
        let simfile = write_fixture("f0-song-lua", generated_pipeline_song_lua_simfile());
        let lua_dir = simfile
            .parent()
            .expect("fixture simfile should have a song directory")
            .join("lua");
        fs::create_dir_all(&lua_dir).unwrap();
        let lua_entry = lua_dir.join("default.lua");
        fs::write(&lua_entry, generated_pipeline_song_lua()).unwrap();
        simfile
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
                        let metrics = space::metrics_for_window(1280, 720);
                        space::set_current_metrics(metrics);
                        space::set_current_window_px(1280, 720);
                        space::set_overscan(0, 0, 0, 0);

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
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorFrameTexture
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::ActorProxy {
                                    target:
                                        deadsync_assets::song_lua::SongLuaProxyTarget::NoteField {
                                            player_index: 0
                                        }
                                }
                            )
                        }));
                        assert!(visuals.overlays.iter().any(|overlay| {
                            matches!(
                                &overlay.kind,
                                deadsync_assets::song_lua::SongLuaOverlayKind::AftSprite {
                                    capture_name
                                } if capture_name == "FixtureCaptureTexture"
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
                        assert_ne!(compare_render_lists(&first, &second), Ok(()));
                    },
                );
            })
            .expect("SongLua frame regression thread should spawn")
            .join()
            .expect("SongLua frame regression thread should finish");
    }

    #[test]
    fn versus_modes_frame_is_structurally_repeatable() {
        let simfile = write_fixture("f0-versus-modes", generated_sprite_core_simfile());
        let metrics = space::metrics_for_window(1280, 720);
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
                assert!(state.display.visual_feedback.tap_explosions[0].is_some());
                assert!(state.display.visual_feedback.tap_explosions[4].is_some());

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
        assert_ne!(compare_render_lists(&autoplay, &autoplay_disabled), Ok(()));

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
                let mut practice = crate::screens::practice::init(
                    gameplay,
                    crate::views::PracticeRuntimeView::default(),
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
        assert_ne!(compare_render_lists(&practice_base, &practice), Ok(()));
        assert_ne!(compare_render_lists(&versus, &autoplay), Ok(()));
        assert_ne!(compare_render_lists(&versus, &practice), Ok(()));
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
