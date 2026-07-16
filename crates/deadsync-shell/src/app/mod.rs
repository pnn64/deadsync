use deadsync_profile as profile_data;
#[cfg(test)]
use deadsync_score as score_data;
use deadsync_score::stage_stats;
mod audio_requests;
mod commands;
mod config_requests;
mod graphics;
mod input_routing;
mod screen_nav;
mod screenshot;
mod select_music_views;
mod smx_runtime;
mod updater;

use self::screenshot::auto_screenshot_eval_results;
use crate::command::Command;
use crate::course::{
    build_course_graph_stages, build_course_run_from_selection, build_course_summary_score_info,
    build_course_summary_stage, course_display_timing_for_run, merge_course_score_columns,
};
use crate::diagnostics::timing_health;
use crate::dynamic_media::DynamicMedia;
use crate::frame_loop::{FrameScreenStepContext, FrameWaitControl, frame_screen_step_plan};
use crate::frame_stats::{
    FrameStatsSummaryContext, frame_stats_summary, frame_stats_target_us, frame_stats_two_player,
};
use crate::frame_stutter::{ComposeBreakdown, trace_frame_stutter};
use crate::gameplay_entry::{gameplay_chart_entry_plan, gameplay_last_played_commands};
use crate::gameplay_prewarm::{
    gameplay_overlay_video_paths, prewarm_gameplay_assets, prewarm_gameplay_sfx,
};
pub use crate::input::UserEvent;
use crate::input::{
    AppRawKeyShortcut, EvaluationRawKeyShortcut, GameplayQueuedEvent, QueuedInputBatchState,
    QueuedInputEventRoute, RawKeyScreenRoute, RawKeyTextRoute, RawPadScreenRoute,
    app_raw_key_shortcut, evaluation_raw_key_shortcut, gamepad_system_event_plan,
    gamepad_system_view, gameplay_dispatch_continues, gameplay_raw_key_event,
    practice_reload_shortcut, queued_input_flush_plan, raw_key_alt_f4_quit, raw_key_screen_route,
    raw_key_text_route, raw_pad_screen_route, smx_panel_press_feedback_plan,
};
use crate::input_backend::{InputBackendConfig, launch_input_backends};
use crate::lighting::{
    GameplayLightSyncTarget, LightInputRoute, OperatorMenuButtonRoute, SmxAnimationSyncKey,
    SmxPanelDriver, hide_flags_for_profiles, light_input_route, lighting_frame_plan,
    lights_test_view, load_cabinet_light_chart, operator_menu_button_route, smx_pad_blackout,
    smx_pad_gif_frame_plan,
};
use crate::navigation::{
    TransitionCompletion, TransitionMusicPaths, TransitionState, is_actor_fade_screen,
};
use crate::offset_prompt::{
    GameplayOffsetSavePrompt, GameplayOffsetSnapshot, OffsetPromptInput,
    gameplay_offset_prompt_needed, gameplay_offset_prompt_text, gameplay_offset_save_targets,
    gameplay_offset_saveable_changed, route_offset_prompt_input,
};
use crate::profile_session::{
    persist_gameplay_combo_carry, profile_selection_session_plan, record_last_played_course,
    reset_operator_profile_session,
};
use crate::restart::{
    GameplayReloadSource, GameplayRestartRoute, RestartPrepareSource, fast_gameplay_restart_plan,
    gameplay_reload_source, gameplay_restart_prepare_source, gameplay_restart_route,
    practice_from_eval_allowed, practice_reload_allowed, practice_restart_prepare_source,
    restart_chart_steps, restart_payload_from_eval,
};
use crate::runtime::ShellState;
use crate::screen_flow::{
    LateJoinContext, ProfileSelectionContext, SelectMusicJoinContext, ThemeEffectExecution,
    ThemeEffectRouteContext, evaluation_summary_return_to, execute_effect_batch, late_join_side,
    profile_selection_plan, select_music_join_plan, theme_effect_execution_plan,
};
use crate::screenshot::{AutoScreenshotFrameContext, auto_screenshot_frame_plan};
use crate::session::SessionState as ShellSessionState;
use crate::session_results::{
    post_select_display_stage_count, post_select_display_stages, stage_summary_from_score_info,
};
use crate::stutter_diag::{
    STUTTER_DIAG_FRAME_CAPACITY, STUTTER_DIAG_WINDOW_NS, StutterDiagDumpContext,
    stutter_diag_dump_lines,
};
use crate::transition_effects::{
    PlayerOptionsTransition, TransitionEffectContext, transition_effect_plan,
};
#[cfg(target_os = "windows")]
use crate::window_state::{WindowMinimizePlan, exclusive_fullscreen_focus_plan};
use crate::window_state::{
    apply_shell_surface_active, apply_shell_window_focus, apply_shell_window_occlusion,
};
#[cfg(test)]
use crate::{
    course::{CourseRunState, CourseStageRuntime, score_info_from_stage},
    input::raw_keyboard_restart_screen,
};
use deadlib_platform::dirs;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use deadlib_platform::host_time;
use deadlib_present::color;
use deadlib_present::compose;
use deadlib_present::space::{self as space, Metrics};
use deadlib_render as renderer;
use deadlib_render::{BackendType, PresentModePolicy};
use deadlib_renderer as renderer_backend;
use deadsync_assets::{AssetManager, PRESENT_TEXTURE_CONTEXT, TextureUploadBudget, media_cache};
use deadsync_config::prelude::{
    self as config, FrameIntervalState, FrameLoopMode, elapsed_us_between, elapsed_us_since,
    stutter_severity,
};
use deadsync_online::score_compat as scores;
use deadsync_profile::compat as profile;
use deadsync_profile::pad_config_sync;
use deadsync_profile_gameplay::{
    gameplay_config_from_config, gameplay_play_style_from_profile,
    gameplay_player_side_from_profile, gameplay_tick_mode_from_profile,
};
use deadsync_simfile::{app_runtime as song_loading, sync_offset};
use deadsync_theme_simply_love::views::{OptionsSongPackView, TimingHealth};
use deadsync_theme_simply_love::{
    screens::{
        self, credits, evaluation, evaluation_summary, gameover, gameplay, init, initials,
        input as input_screen, manage_local_profiles, mappings, menu, options, overscan_adjustment,
        player_options, practice, profile_load, sandbox, select_color, select_course, select_mode,
        select_music, select_profile, select_style, test_lights,
    },
    visual_styles,
};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::Window,
};

use log::{debug, error, info, trace, warn};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[cfg(all(
    not(windows),
    not(target_os = "linux"),
    not(target_os = "freebsd"),
    not(target_os = "macos")
))]
compile_error!(
    "deadsync control input requires a raw keyboard backend; only Windows, Linux, FreeBSD, and macOS are wired for full app input"
);

use deadlib_present::actors::Actor;
use deadsync_chart::STANDARD_DIFFICULTY_COUNT;
use deadsync_core::input::MAX_PLAYERS;
#[cfg(test)]
use deadsync_gameplay::CourseDisplayTotals;
use deadsync_gameplay::{
    GameplaySession, GameplayViewport, LeadInTiming, ReplayInputEdge, ReplayOffsetSnapshot,
};
use deadsync_input as logical_input;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, PadEvent, VirtualAction};
use deadsync_input_fsr as fsr_input;
use deadsync_lights::cabinet_chart::{
    cabinet_light_chart_from_loaded, cabinet_light_key, cabinet_light_plan,
};
use deadsync_lights::{self as lights, HideFlags};
#[cfg(test)]
use deadsync_rules::judgment as judgment_rules;
use deadsync_rules::scroll::ScrollSpeedSetting;
#[cfg(test)]
use deadsync_rules::timing as timing_rules;
use deadsync_theme::views::{
    AppPathView, AppPathsView, DensityGraphView as DensityGraphSource, NoteskinCatalogView,
    SmxAssignmentView,
};
use deadsync_theme::{AudioRequest, PlatformRequest, RevealPathKind};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as CurrentScreen;
use deadsync_theme_simply_love::views::{
    EvaluationInitPlayerView, EvaluationInitView, EvaluationRuntimeView, EvaluationSubmissionView,
    MusicWheelRankSource, MusicWheelRuntimeRequest, MusicWheelRuntimeView,
    MusicWheelSlotRuntimeRequest, MusicWheelSlotRuntimeView, ScoreboxLocalView,
    ScoreboxMachineView, ScoreboxSideView, SelectCourseRuntimeView, SelectCourseScoreRequest,
    SelectCourseScoreView, SimplyLoveDensityGraphSlot as DensityGraphSlot,
    SimplyLoveGrooveStatsService, SimplyLoveLobbyRuntimeView,
};
use deadsync_theme_simply_love::{
    SimplyLoveConfigRequest, SimplyLoveEffect as ThemeEffect, SimplyLoveHardwareRequest,
    SimplyLoveLobbyRequest, SimplyLoveOnlineRequest, SimplyLoveProfileRequest,
    SimplyLoveQrLoginService, SimplyLoveRuntimeRequest, SimplyLoveSyncOwner, SimplyLoveSyncRequest,
};

/// Imperative effects to be executed by the shell.
/* -------------------- transition timing constants -------------------- */
const MENU_ACTORS_FADE_DURATION: f32 = 0.65;
const COURSE_MIN_SECONDS_TO_STEP_NEXT_SONG: f32 = 4.0;
const COURSE_MIN_SECONDS_TO_MUSIC_NEXT_SONG: f32 = 0.0;
const GAMEPLAY_OFFSET_PROMPT_Z_BACKDROP: i16 = 31990;
const GAMEPLAY_OFFSET_PROMPT_Z_CURSOR: i16 = 31991;
const GAMEPLAY_OFFSET_PROMPT_Z_TEXT: i16 = 31993;
const UI_TEXT_LAYOUT_CACHE_LIMIT: usize = 4_096;
const GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT: usize = 32_768;
/// Game-thread, song-lifetime reserve for values not known at transition prewarm.
/// Owned and shared text each receive this allowance, bounded by the 32K hard cap.
/// Misses build once and remain until the next song; overflow saturates without
/// eviction, scanning, or live-frame destruction.
const GAMEPLAY_TEXT_LAYOUT_LIVE_RESERVE: usize = 4_096;
const LIVE_TEXTURE_UPLOAD_MAX_OPS: usize = 2;
const LIVE_TEXTURE_UPLOAD_MAX_BYTES: usize = 8 * 1024 * 1024;
const EVALUATION_LEADERBOARD_ROWS: usize = 10;
const SERVICE_SWITCH_PRESSED: &str = "Service switch pressed";

fn sequence_effects(first: ThemeEffect, second: ThemeEffect) -> ThemeEffect {
    match (first, second) {
        (ThemeEffect::None, second) => second,
        (first, ThemeEffect::None) => first,
        (ThemeEffect::Batch(mut effects), second) => {
            effects.push(second);
            ThemeEffect::Batch(effects)
        }
        (first, second) => ThemeEffect::Batch(vec![first, second]),
    }
}

fn lobby_effect_only(effect: ThemeEffect) -> Option<ThemeEffect> {
    match effect {
        effect @ ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Online(
            SimplyLoveOnlineRequest::Lobby(_),
        )) => Some(effect),
        ThemeEffect::Batch(effects) => {
            let mut lobby_effects = effects
                .into_iter()
                .filter_map(lobby_effect_only)
                .collect::<Vec<_>>();
            match lobby_effects.len() {
                0 => None,
                1 => lobby_effects.pop(),
                _ => Some(ThemeEffect::Batch(lobby_effects)),
            }
        }
        _ => None,
    }
}

fn gameplay_viewport(metrics: Metrics) -> GameplayViewport {
    GameplayViewport::new(metrics.right - metrics.left, metrics.top - metrics.bottom)
}

fn gameplay_session() -> GameplaySession {
    let (session, active_profile_ids) = profile::get_session_snapshot_with_active_ids();
    GameplaySession {
        play_style: gameplay_play_style_from_profile(session.play_style),
        player_side: gameplay_player_side_from_profile(session.player_side),
        joined_sides: std::array::from_fn(|idx| {
            session.side_joined(profile_data::player_side_for_index(idx))
        }),
        active_profile_ids,
        tick_mode: gameplay_tick_mode_from_profile(session.timing_tick_mode),
    }
}

fn gameplay_offset_snapshot(gs: &gameplay::State) -> GameplayOffsetSnapshot {
    GameplayOffsetSnapshot {
        initial_global_seconds: gs.initial_global_offset_seconds(),
        global_seconds: gs.global_offset_seconds(),
        initial_song_seconds: gs.initial_song_offset_seconds(),
        song_seconds: gs.song_offset_seconds(),
        song_writable: config::song_path_is_writable(gs.song().simfile_path.as_path()),
    }
}

#[inline(always)]
fn stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

#[inline(always)]
const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

/// Active screen data bundle.
pub struct ScreensState {
    current_screen: CurrentScreen,
    menu_state: menu::State,
    gameplay_state: Option<gameplay::State>,
    practice_state: Option<practice::State>,
    options_state: options::State,
    credits_state: credits::State,
    manage_local_profiles_state: manage_local_profiles::State,
    mappings_state: mappings::State,
    input_state: input_screen::State,
    pad_config_state: screens::pad_config::State,
    test_lights_state: test_lights::State,
    overscan_adjustment_state: overscan_adjustment::State,
    smx_assign_state: screens::smx_assign::State,
    /// Latched while a same-jumper SMX conflict is being auto-prompted, so the
    /// assignment screen is only opened once per conflict episode (cleared when
    /// the conflict resolves). See `App::maybe_autoprompt_smx_assign`.
    smx_autoprompt_latched: bool,
    smx_options_light_preview: deadsync_smx::OptionsLightPreview,
    smx_po_light_preview: deadsync_smx::PlayerOptionsLightPreview,
    player_options_state: Option<player_options::State>,
    init_state: init::State,
    select_profile_state: select_profile::State,
    select_color_state: select_color::State,
    arrowcloud_login_state: screens::arrowcloud_login::State,
    groovestats_login_state: screens::groovestats_login::State,
    select_style_state: select_style::State,
    select_play_mode_state: select_mode::State,
    profile_load_state: profile_load::State,
    select_music_state: select_music::State,
    select_course_state: select_course::State,
    sandbox_state: sandbox::State,
    evaluation_state: evaluation::State,
    evaluation_summary_state: evaluation_summary::State,
    initials_state: initials::State,
    gameover_state: gameover::State,
}

pub type SessionState = ShellSessionState<evaluation::State>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InputRoutePolicy {
    only_dedicated_menu_buttons: bool,
    keyboard_features: bool,
    smx_input: bool,
    smx_panel_lights: bool,
}

impl InputRoutePolicy {
    const fn from_config(cfg: &config::Config) -> Self {
        Self {
            only_dedicated_menu_buttons: cfg.only_dedicated_menu_buttons,
            keyboard_features: cfg.keyboard_features,
            smx_input: cfg.smx_input,
            smx_panel_lights: cfg.smx_panel_lights,
        }
    }

    fn from_runtime() -> Self {
        let cfg = config::input_routing_config();
        Self {
            only_dedicated_menu_buttons: cfg.only_dedicated_menu_buttons,
            keyboard_features: cfg.keyboard_features,
            smx_input: cfg.smx_input,
            smx_panel_lights: cfg.smx_panel_lights,
        }
    }
}

/// Pure-ish container for the high-level game state.
/// This keeps screen flow, timing and UI state separate from the window/renderer shell.
pub struct AppState {
    shell: ShellState,
    screens: ScreensState,
    session: SessionState,
    gameplay_offset_save_prompt: Option<GameplayOffsetSavePrompt>,
    play_input_policy: InputRoutePolicy,
}

fn apply_course_summary_column_judgments(
    course_page: &mut evaluation::State,
    song_pages: &[evaluation::State],
) {
    for summary in course_page.score_info.iter_mut().flatten() {
        merge_course_score_columns(
            summary,
            song_pages
                .iter()
                .flat_map(|page| page.score_info.iter().flatten()),
        );
    }
}

fn build_course_summary_eval_state(
    stage: &stage_stats::StageSummary,
    course_graph_stages: &[Vec<evaluation::CourseGraphStage>; MAX_PLAYERS],
    active_color_index: i32,
    session_elapsed: f32,
    gameplay_elapsed: f32,
) -> evaluation::State {
    let profile_session = profile::get_session_snapshot();
    let score_info = build_course_summary_score_info(
        stage,
        course_graph_stages,
        profile_session.play_style,
        profile_session.player_side,
    );
    let mut state = evaluation::init_from_score_info(score_info, stage.duration_seconds);
    state.active_color_index = active_color_index;
    state.session_elapsed = session_elapsed;
    state.gameplay_elapsed = gameplay_elapsed;
    state.return_to_course = true;
    state.allow_online_panes = false;
    state
}

fn sync_gameplay_banners(
    media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut renderer_backend::Backend,
    state: &gameplay::State,
    mode: config::GameplayBannerMode,
) {
    let visible_paths = gameplay::visible_banner_paths(state);
    let desired_paths: SmallVec<[&Path; 2]> = mode
        .looped()
        .into_iter()
        .flat_map(|_| visible_paths.into_iter().flatten())
        .collect();
    media.sync_active_banner_videos(
        assets,
        backend,
        &desired_paths,
        mode.looped().unwrap_or(false),
    );
}

fn prewarm_gameplay_banners(
    media: &mut DynamicMedia,
    assets: &mut AssetManager,
    backend: &mut renderer_backend::Backend,
    state: &gameplay::State,
    mode: config::GameplayBannerMode,
) {
    let visible_paths: SmallVec<[&Path; 2]> = gameplay::visible_banner_paths(state)
        .into_iter()
        .flatten()
        .collect();

    // A gameplay entry or restart owns a fresh playback interval. Retire menu or
    // previous-attempt decoders before restoring the first-frame posters.
    media.sync_active_banner_videos(assets, backend, &[], false);
    for path in &visible_paths {
        if deadlib_assets::dynamic::is_dynamic_video_path(path) {
            if let Err(e) =
                deadsync_assets::dynamic_media::set_banner_texture_for_path(assets, backend, path)
            {
                warn!(
                    "Failed to reset gameplay banner poster '{}': {e:?}",
                    path.display()
                );
            }
        } else {
            media_cache::ensure_banner_texture(assets, backend, path);
        }
    }
    if let Some(looped) = mode.looped() {
        media.sync_active_banner_videos(assets, backend, &visible_paths, looped);
    }
}

fn prewarm_gameplay_text_layout_cache(
    assets: &AssetManager,
    metrics: &Metrics,
    cache: &mut compose::TextLayoutCache,
    compose_scratch: &mut compose::ComposeScratch,
    state: &mut gameplay::State,
    config: &config::Config,
) {
    let started = Instant::now();
    // Gameplay prewarm owns the whole cache for the next song, so start from an
    // empty working set instead of scan-pruning stale entries from older screens.
    cache.clear();
    cache.configure(GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT);
    cache.begin_frame_stats(true);
    compose_scratch.clear_retained_frames();

    let fonts = assets.fonts();
    screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(state);
    let mut actors = Vec::with_capacity(256);
    gameplay::push_actors(
        &mut actors,
        state,
        assets,
        gameplay::ActorViewOverride::default(),
        arrow_effect_time_seconds(started),
        config,
    );
    let mut render =
        compose::build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
            &actors,
            [0.0, 0.0, 0.0, 1.0],
            metrics,
            fonts,
            0.0,
            cache,
            compose_scratch,
            &PRESENT_TEXTURE_CONTEXT,
            state.actor_resources(),
        );
    compose_scratch.recycle_render_list(&mut render);
    gameplay::prewarm_text_layout(cache, fonts, state, config);
    screens::components::gameplay::gameplay_stats::prewarm_text_layout(cache, fonts, assets, state);
    screens::components::gameplay::notefield::prewarm_text_layout(cache, fonts, state);
    // Keep a bounded song-local allowance for genuinely dynamic values. Reserving
    // it here avoids layout-arena growth and all pruning during live gameplay.
    cache.lock_growth_with_reserve(GAMEPLAY_TEXT_LAYOUT_LIVE_RESERVE);

    let stats = cache.frame_stats();
    let actor_resources = state.actor_resources().stats();
    let retained_frames = compose_scratch.retained_frame_stats();
    debug!(
        "Gameplay cache prewarm: text_entries={} shared={} actor_textures={} actor_misses={} actor_saturated={} retained_entries={} retained_misses={} retained_saturated={} elapsed_ms={:.3}",
        stats.owned_entries,
        stats.shared_aliases,
        actor_resources.textures,
        actor_resources.texture_misses,
        actor_resources.texture_saturated,
        retained_frames.entries,
        retained_frames.misses,
        retained_frames.saturated,
        started.elapsed().as_secs_f64() * 1000.0,
    );
    compose_scratch.reset_retained_frame_stats();
}

#[inline(always)]
fn arrow_effect_time_seconds(at: Instant) -> f32 {
    deadlib_platform::host_time::instant_nanos(at) as f32 / 1_000_000_000.0
}

fn app_path_view(path: PathBuf) -> AppPathView {
    AppPathView {
        display: deadlib_platform::dirs::path_shorthand(&path),
        path,
    }
}

fn app_paths_view() -> AppPathsView {
    let dirs = deadlib_platform::dirs::app_dirs();
    AppPathsView {
        data: app_path_view(dirs.data_dir.clone()),
        cache: app_path_view(dirs.cache_dir.clone()),
        songs: app_path_view(dirs.songs_dir()),
        courses: app_path_view(dirs.courses_dir()),
        profiles: app_path_view(dirs.profiles_root()),
        screenshots: app_path_view(dirs.screenshots_dir()),
        log_file: app_path_view(dirs.log_path()),
        config_file: app_path_view(dirs.config_path()),
    }
}

fn options_song_pack_view() -> Vec<OptionsSongPackView> {
    deadsync_simfile::runtime_cache::get_song_cache()
        .iter()
        .map(|pack| OptionsSongPackView {
            group_name: pack.group_name.clone(),
            display_name: pack.name.clone(),
            songs: pack.songs.clone(),
        })
        .collect()
}

fn noteskin_catalog_view() -> NoteskinCatalogView {
    let roots = deadlib_platform::dirs::app_dirs().noteskin_roots();
    NoteskinCatalogView {
        names: deadsync_noteskin::itg::discover_skins(&roots, "dance"),
    }
}

impl ScreensState {
    fn new(color_index: i32, preferred_difficulty_index: usize) -> Self {
        let mut menu_state = menu::init();
        menu_state.active_color_index = color_index;

        let mut select_profile_state = select_profile::init();
        select_profile_state.active_color_index = color_index;

        let mut select_color_state = select_color::init();
        select_color_state.active_color_index = color_index;
        select_color::snap_scroll_to_active(&mut select_color_state);
        select_color_state.bg_from_index = color_index;
        select_color_state.bg_to_index = color_index;

        let mut arrowcloud_login_state = screens::arrowcloud_login::init();
        arrowcloud_login_state.active_color_index = color_index;

        let mut groovestats_login_state = screens::groovestats_login::init();
        groovestats_login_state.active_color_index = color_index;

        let mut select_music_state = select_music::init_placeholder();
        select_music_state.active_color_index = color_index;
        select_music_state.preferred_difficulty_index = preferred_difficulty_index;
        select_music_state.selected_steps_index = preferred_difficulty_index;

        let mut select_course_state =
            select_course::init(crate::profile_load::select_course_init_view());
        select_course_state.active_color_index = color_index;

        let mut select_style_state = select_style::init();
        select_style_state.active_color_index = color_index;

        let mut select_play_mode_state = select_mode::init();
        select_play_mode_state.active_color_index = color_index;

        let mut profile_load_state = profile_load::init();
        profile_load_state.active_color_index = color_index;

        let app_paths = app_paths_view();
        let init_songs_root = app_paths.songs.path.clone();
        let init_courses_root = app_paths.courses.path.clone();
        let mut options_state = options::init(
            updater::capabilities(),
            app_paths,
            audio_requests::options_view(),
            graphics::options_graphics_view(),
            options_song_pack_view(),
            noteskin_catalog_view(),
            crate::smx_config::smx_assignment_view(),
            crate::smx_config::smx_gif_catalog_view(),
        );
        options_state.active_color_index = color_index;

        let mut credits_state = credits::init();
        credits_state.active_color_index = color_index;

        let mut manage_local_profiles_state = manage_local_profiles::init();
        manage_local_profiles_state.active_color_index = color_index;

        let mut mappings_state = mappings::init(crate::mappings::runtime_view());
        mappings_state.active_color_index = color_index;

        let mut input_state = input_screen::init();
        input_state.active_color_index = color_index;

        let mut test_lights_state = test_lights::init();
        test_lights_state.active_color_index = color_index;

        let mut overscan_adjustment_state = overscan_adjustment::init();
        overscan_adjustment_state.active_color_index = color_index;

        let mut smx_assign_state = screens::smx_assign::init();
        smx_assign_state.active_color_index = color_index;

        let mut init_state = init::init(init_songs_root, init_courses_root);
        init_state.active_color_index = color_index;

        let mut evaluation_state = evaluation::init(None, EvaluationInitView::default());
        evaluation_state.active_color_index = color_index;

        let mut evaluation_summary_state = evaluation_summary::init();
        evaluation_summary_state.active_color_index = color_index;

        let mut initials_state = initials::init();
        initials_state.active_color_index = color_index;

        let mut gameover_state = gameover::init_blank();
        gameover_state.active_color_index = color_index;

        Self {
            current_screen: CurrentScreen::Init,
            menu_state,
            gameplay_state: None,
            practice_state: None,
            options_state,
            credits_state,
            manage_local_profiles_state,
            mappings_state,
            input_state,
            pad_config_state: {
                let mut s = screens::pad_config::init();
                s.active_color_index = color_index;
                s
            },
            test_lights_state,
            overscan_adjustment_state,
            smx_assign_state,
            smx_autoprompt_latched: false,
            smx_options_light_preview: deadsync_smx::OptionsLightPreview::default(),
            smx_po_light_preview: deadsync_smx::PlayerOptionsLightPreview::default(),
            player_options_state: None,
            init_state,
            select_profile_state,
            select_color_state,
            arrowcloud_login_state,
            groovestats_login_state,
            select_style_state,
            select_play_mode_state,
            profile_load_state,
            select_music_state,
            select_course_state,
            sandbox_state: sandbox::init(),
            evaluation_state,
            evaluation_summary_state,
            initials_state,
            gameover_state,
        }
    }

    fn step_idle(
        &mut self,
        delta_time: f32,
        now: Instant,
        session: &SessionState,
        asset_manager: &AssetManager,
        smx_assignment: &SmxAssignmentView,
        gameplay_smx_input: bool,
    ) -> (Option<ThemeEffect>, bool) {
        match self.current_screen {
            CurrentScreen::Gameplay => self
                .gameplay_state
                .as_mut()
                .map(|gs| crate::gameplay_runtime::update(gs, delta_time, gameplay_smx_input))
                .map_or((None, false), |action| (Some(action), false)),
            CurrentScreen::Practice => self
                .practice_state
                .as_mut()
                .map(|ps| crate::gameplay_runtime::update_practice(ps, delta_time))
                .map_or((None, false), |action| (Some(action), false)),
            CurrentScreen::Init => (Some(init::update(&mut self.init_state, delta_time)), false),
            CurrentScreen::Options => (
                options::update(
                    &mut self.options_state,
                    delta_time,
                    asset_manager,
                    smx_assignment,
                ),
                false,
            ),
            CurrentScreen::Credits => {
                credits::update(&mut self.credits_state, delta_time);
                (None, false)
            }
            CurrentScreen::ManageLocalProfiles => (
                manage_local_profiles::update(&mut self.manage_local_profiles_state, delta_time),
                false,
            ),
            CurrentScreen::Mappings => (
                Some(mappings::update(&mut self.mappings_state, delta_time)),
                false,
            ),
            CurrentScreen::Input => (
                input_screen::update(&mut self.input_state, delta_time),
                false,
            ),
            CurrentScreen::ConfigurePads => (
                screens::pad_config::update(&mut self.pad_config_state, delta_time),
                false,
            ),
            CurrentScreen::TestLights => (
                test_lights::update(&mut self.test_lights_state, delta_time),
                false,
            ),
            CurrentScreen::OverscanAdjustment => (
                overscan_adjustment::update(&mut self.overscan_adjustment_state, delta_time),
                false,
            ),
            CurrentScreen::SmxAssignPads => (
                screens::smx_assign::update(&mut self.smx_assign_state, delta_time, smx_assignment),
                false,
            ),
            CurrentScreen::PlayerOptions => {
                if let Some(state) = self.player_options_state.as_mut() {
                    crate::heart_rate::refresh_player_options(state);
                }
                (
                    self.player_options_state
                        .as_mut()
                        .and_then(|pos| player_options::update(pos, delta_time, asset_manager)),
                    false,
                )
            }
            CurrentScreen::Sandbox => {
                sandbox::update(&mut self.sandbox_state, delta_time);
                (None, false)
            }
            CurrentScreen::SelectProfile => {
                select_profile::update(&mut self.select_profile_state, delta_time);
                (None, false)
            }
            CurrentScreen::SelectColor => {
                select_color::update(&mut self.select_color_state, delta_time);
                (None, false)
            }
            CurrentScreen::ArrowCloudLogin => (
                screens::arrowcloud_login::update(&mut self.arrowcloud_login_state, delta_time),
                false,
            ),
            CurrentScreen::GrooveStatsLogin => (
                screens::groovestats_login::update(&mut self.groovestats_login_state, delta_time),
                false,
            ),
            CurrentScreen::SelectStyle => (
                select_style::update(&mut self.select_style_state, delta_time),
                false,
            ),
            CurrentScreen::SelectPlayMode => (
                select_mode::update(&mut self.select_play_mode_state, delta_time),
                false,
            ),
            CurrentScreen::ProfileLoad => (
                profile_load::update(&mut self.profile_load_state, delta_time),
                false,
            ),
            CurrentScreen::Evaluation => {
                if let Some(start) = session.session_start_time {
                    self.evaluation_state.session_elapsed = now.duration_since(start).as_secs_f32();
                }
                self.evaluation_state.gameplay_elapsed =
                    stage_stats::total_stage_duration_seconds(&session.played_stages);
                let update_effect = evaluation::update(&mut self.evaluation_state, delta_time);
                let navigation = if let Some(delay) = self.evaluation_state.auto_advance_seconds
                    && self.evaluation_state.screen_elapsed >= delay
                    && self.player_options_state.is_some()
                {
                    ThemeEffect::Navigate(CurrentScreen::Gameplay)
                } else {
                    ThemeEffect::None
                };
                (Some(sequence_effects(update_effect, navigation)), false)
            }
            CurrentScreen::EvaluationSummary => {
                evaluation_summary::update(&mut self.evaluation_summary_state, delta_time);
                (None, false)
            }
            CurrentScreen::Initials => (
                initials::update(&mut self.initials_state, delta_time),
                false,
            ),
            CurrentScreen::GameOver => (
                gameover::update(&mut self.gameover_state, delta_time),
                false,
            ),
            CurrentScreen::SelectMusic => {
                if let Some(start) = session.session_start_time {
                    self.select_music_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                self.select_music_state.gameplay_elapsed =
                    stage_stats::total_stage_duration_seconds(&session.played_stages);
                (
                    Some(select_music::update(
                        &mut self.select_music_state,
                        delta_time,
                        smx_assignment,
                    )),
                    false,
                )
            }
            CurrentScreen::SelectCourse => {
                if let Some(start) = session.session_start_time {
                    self.select_course_state.session_elapsed =
                        now.duration_since(start).as_secs_f32();
                }
                (
                    Some(select_course::update(
                        &mut self.select_course_state,
                        delta_time,
                    )),
                    false,
                )
            }
            CurrentScreen::Menu => (None, false),
        }
    }
}

impl AppState {
    fn new(
        cfg: config::Config,
        profile_data: profile_data::Profile,
        overlay_mode: u8,
        color_index: i32,
    ) -> Self {
        let play_style = profile::get_session_play_style();
        let preferred = deadsync_profile::preferred_difficulty_index(&profile_data, play_style);

        let shell = ShellState::new(&cfg, overlay_mode);
        let session = SessionState::new(preferred, profile::combo_carry());
        let screens = ScreensState::new(color_index, preferred);

        Self {
            shell,
            screens,
            session,
            gameplay_offset_save_prompt: None,
            play_input_policy: InputRoutePolicy::from_config(&cfg),
        }
    }
}

pub struct App {
    window: Option<Arc<Window>>,
    backend: Option<renderer_backend::Backend>,
    backend_type: BackendType,
    _idle_inhibitor: deadlib_platform::idle_inhibit::IdleInhibitor,
    fsr_monitor: fsr_input::Monitor,
    /// Whether the Configure Pads screen currently has FSR live-reads enabled,
    /// so `set_active` only toggles on screen enter/leave (not every frame).
    fsr_pads_active: bool,
    /// App-owned source of truth for SMX managed-config resolution + the active
    /// marker (mirrored to the Song Select screen each frame). See
    /// [`pad_config_sync`].
    pad_config_sync: pad_config_sync::PadConfigSync,
    lights: lights::Manager,
    gameplay_lights: lights::gameplay::GameplayLightTracker,
    smx_panels: SmxPanelDriver,
    /// Last per-slot pad-light brightness pushed to the SMX crate (`[P1, P2]`),
    /// cached so the resolve-and-push only fires when the value actually changes.
    smx_light_brightness: [u8; 2],
    /// Preloaded SMX pad GIF animations, decoded once on first use (the pad-gifs
    /// option toggling on). `None` until then; never loaded on the gameplay path.
    smx_gifs: Option<std::sync::Arc<deadsync_smx::gifs::GifRegistry>>,
    /// Background state last pushed to `smx_panels`, so the
    /// per-frame sync only does lookups when the toggle, screen role, pack, or
    /// current song change. Reset on a song rescan, so a recycled `Arc`
    /// pointer can't be mistaken for the same song.
    smx_bg_synced: Option<SmxAnimationSyncKey>,
    /// Per-slot blackout state last sent to the SMX lights worker; `[P1_slot, P2_slot]`.
    smx_blackout_synced: [bool; 2],
    /// Decoded per-song / per-pack pad background variants, keyed by their
    /// `smx-pad-lights/` folder and role. An empty vec is the negative entry
    /// (folder scanned, no matching gif) so a folder is touched only once; the
    /// per-song BPM variant is selected from the list per resolution. Cleared
    /// when the song cache is rescanned. Only grows with folders visited.
    smx_scoped_bg_cache: std::collections::HashMap<
        (PathBuf, &'static str),
        Vec<deadsync_smx::gifs::BackgroundVariant>,
    >,
    /// Song-cache generation the scoped cache was built against; a change means
    /// a rescan, so the scoped cache is dropped (files may have changed).
    smx_scoped_bg_generation: u64,
    /// Cache of `MatchColorToDifficulty`-tinted backgrounds, keyed by (pack,
    /// role, difficulty). Holds the theme color index the tint was computed
    /// against alongside the `Arc`, so a stale entry (the player changed their
    /// theme color) is detected and recomputed on next use rather than pruned
    /// eagerly; at most one entry lives per (pack, role, difficulty) at a time.
    smx_difficulty_tint_cache: std::collections::HashMap<
        (String, &'static str, &'static str),
        (i32, std::sync::Arc<deadsync_smx::gifs::FullPadAnim>),
    >,
    asset_manager: AssetManager,
    dynamic_media: DynamicMedia,
    options_song_pack_generation: u64,
    profile_import: crate::profile_import::Service,
    profile_load: crate::profile_load::Service,
    heart_rate: crate::heart_rate::Runtime,
    qr_login: crate::qr_login::Service,
    score_import: crate::score_import::Service,
    sync_analysis: crate::sync_analysis::Service,
    ui_text_layout_cache: compose::TextLayoutCache,
    gameplay_text_layout_cache: compose::TextLayoutCache,
    ui_compose_scratch: compose::ComposeScratch,
    gameplay_compose_scratch: compose::ComposeScratch,
    actor_scratch: Vec<Actor>,
    state: AppState,
    software_renderer_threads: u8,
    gfx_debug_enabled: bool,
}

fn execute_platform_request(request: PlatformRequest) {
    match request {
        PlatformRequest::RevealPath { path, kind } => {
            dirs::ensure_dirs_exist();
            if matches!(kind, RevealPathKind::Directory) && !path.exists() {
                if let Err(e) = std::fs::create_dir_all(&path) {
                    warn!(
                        "Failed to create folder before opening '{}': {e}",
                        path.display()
                    );
                }
            }
            if let Err(e) = deadlib_platform::open_path::reveal(&path) {
                warn!("Failed to open '{}' in file explorer: {e}", path.display());
            }
        }
    }
}

fn apply_music_preferences(state: &mut select_music::State, p1: usize, p2: usize) {
    state.selected_steps_index = p1;
    state.preferred_difficulty_index = p1;
    state.p2_selected_steps_index = p2;
    state.p2_preferred_difficulty_index = p2;
    select_music::select_preferred_steps(state);
    select_music::trigger_immediate_refresh(state);
}

impl App {
    #[inline(always)]
    fn input_route_policy(&self, screen: CurrentScreen) -> InputRoutePolicy {
        if matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Practice) {
            self.state.play_input_policy
        } else {
            InputRoutePolicy::from_runtime()
        }
    }

    fn sync_options_song_packs(&mut self) {
        if self.state.screens.current_screen != CurrentScreen::Options {
            return;
        }
        let generation = deadsync_simfile::runtime_cache::song_cache_generation();
        if generation == self.options_song_pack_generation {
            return;
        }
        options::sync_song_packs(
            &mut self.state.screens.options_state,
            options_song_pack_view(),
        );
        self.options_song_pack_generation = generation;
    }

    fn sync_options_stepmaniaonline(&mut self) {
        if self.state.screens.current_screen != CurrentScreen::Options {
            return;
        }
        options::sync_stepmaniaonline(
            &mut self.state.screens.options_state,
            deadsync_online::stepmaniaonline::runtime_snapshot(),
            deadsync_online::stepmaniaonline::runtime_take_ready_song_dirs(),
        );
    }

    fn poll_profile_load(&mut self) {
        if self.state.screens.current_screen != CurrentScreen::ProfileLoad {
            return;
        }
        let Some(prepared) = self.profile_load.poll() else {
            return;
        };
        match prepared {
            crate::profile_load::PreparedState::Music(mut state) => {
                state.active_color_index = self.state.screens.profile_load_state.active_color_index;
                let preferred = self.state.session.preferred_difficulty_index;
                let p2_preferred = profile::preferred_difficulty_for_side(
                    profile_data::PlayerSide::P2,
                    profile::get_session_play_style(),
                );
                apply_music_preferences(&mut state, preferred, p2_preferred);
                self.state.screens.select_music_state = state;
            }
            crate::profile_load::PreparedState::Course(mut state) => {
                state.active_color_index = self.state.screens.profile_load_state.active_color_index;
                select_course::trigger_immediate_refresh(&mut state);
                self.state.screens.select_course_state = state;
            }
        }
        profile_load::sync_ready(&mut self.state.screens.profile_load_state, true);
    }

    fn poll_profile_import(&mut self) {
        let events = self.profile_import.poll();
        if !events.is_empty() {
            manage_local_profiles::apply_import_events(
                &mut self.state.screens.manage_local_profiles_state,
                events,
            );
        }
    }

    fn poll_score_import(&mut self) {
        let events = self.score_import.poll();
        if !events.is_empty() {
            options::apply_score_import_events(&mut self.state.screens.options_state, events);
        }
    }

    fn poll_qr_login(&mut self) {
        let mut arrowcloud = Vec::new();
        let mut groovestats = Vec::new();
        for event in self.qr_login.poll() {
            match event.service() {
                SimplyLoveQrLoginService::ArrowCloud => arrowcloud.push(event),
                SimplyLoveQrLoginService::GrooveStats => groovestats.push(event),
            }
        }
        if !arrowcloud.is_empty() {
            screens::arrowcloud_login::apply_events(
                &mut self.state.screens.arrowcloud_login_state,
                arrowcloud,
            );
        }
        if !groovestats.is_empty() {
            screens::groovestats_login::apply_events(
                &mut self.state.screens.groovestats_login_state,
                groovestats,
            );
        }
    }

    fn poll_sync_analysis(&mut self) {
        let mut song_events = Vec::new();
        let mut select_pack_events = Vec::new();
        let mut options_pack_events = Vec::new();
        for (owner, event) in self.sync_analysis.poll() {
            match owner {
                SimplyLoveSyncOwner::SelectMusicSong => song_events.push(event),
                SimplyLoveSyncOwner::SelectMusicPack => select_pack_events.push(event),
                SimplyLoveSyncOwner::OptionsPack => options_pack_events.push(event),
            }
        }
        if !song_events.is_empty() {
            select_music::apply_sync_analysis_events(
                &mut self.state.screens.select_music_state,
                SimplyLoveSyncOwner::SelectMusicSong,
                song_events,
            );
        }
        if !select_pack_events.is_empty() {
            select_music::apply_sync_analysis_events(
                &mut self.state.screens.select_music_state,
                SimplyLoveSyncOwner::SelectMusicPack,
                select_pack_events,
            );
        }
        if !options_pack_events.is_empty() {
            options::apply_sync_analysis_events(
                &mut self.state.screens.options_state,
                options_pack_events,
            );
        }
    }

    #[inline(always)]
    fn redraw_interval_state(&self, _window: &Window) -> FrameIntervalState {
        self.state
            .shell
            .frame_interval_state(self.state.screens.current_screen)
    }

    #[inline(always)]
    fn apply_present_back_pressure(&self) -> bool {
        if self.state.shell.vsync_enabled {
            return false;
        }
        #[cfg(target_os = "macos")]
        if self.backend_type == BackendType::OpenGL {
            return true;
        }
        self.state.shell.present_mode_policy == PresentModePolicy::Mailbox
    }

    #[inline(always)]
    fn accepts_live_input(&self) -> bool {
        config::foreground_input_active(
            self.state.shell.frame_loop.window_focused(),
            self.state.shell.frame_loop.surface_active(),
        )
    }

    /// Apply a window focus change to all subsystems that care about it.
    ///
    /// Used by both the `WindowEvent::Focused` handler and the initial focus
    /// seed performed in `init_graphics` (and on renderer-switch window
    /// recreation). Always pushes the new focus state to the raw input
    /// backends so their gating flag stays in sync with the shell, and only
    /// runs the change-only side effects (capture sync, modifier reset,
    /// debounce/queue clear, redraw) when the shell focus actually toggled.
    pub(super) fn apply_window_focus_change(
        &mut self,
        focused: bool,
        now: Instant,
        window: Option<&Arc<Window>>,
    ) {
        deadsync_input_native::set_raw_keyboard_window_focused(focused);
        let plan = apply_shell_window_focus(&mut self.state.shell, focused, now);
        if !plan.changed {
            return;
        }
        if plan.sync_gameplay_capture {
            self.sync_gameplay_input_capture();
        }
        debug!(
            "Window focus changed: focused={} screen={:?}",
            focused, self.state.screens.current_screen
        );
        if plan.clear_live_input {
            self.state.shell.interaction.controls_mut().clear();
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
        } else if let (Some(reason), Some(w)) = (plan.redraw_reason, window) {
            self.request_redraw(w, reason);
        }
    }

    fn sync_lights(&mut self, delta_time: f32, elapsed_seconds: f32, config: &config::Config) {
        self.lights
            .set_driver(config.lights_driver, config.lights_com_port.as_str());
        self.lights
            .set_gameplay_pad_lights(config.lights_gameplay_pad_lights);
        let plan = lighting_frame_plan(
            self.state.screens.current_screen,
            config.smx_input,
            config.smx_panel_lights,
        );
        if let Some(context) = plan.screen_mode {
            self.lights.set_mode(lights::screen_light_mode(context));
        }
        let session = profile::get_session_snapshot();
        self.lights.set_joined([
            session.side_joined(profile_data::PlayerSide::P1),
            session.side_joined(profile_data::PlayerSide::P2),
        ]);
        self.lights.set_hide_flags(self.current_light_hide_flags());
        self.sync_gameplay_light_blinks(
            plan.gameplay_target,
            config.lights_simplify_bass,
            plan.smx_panels_enabled,
            session.play_style,
            session.player_side,
        );
        // Per-player pack overrides, resolved per pad slot so that in versus
        // mode P1's pad uses P1's pack and P2's pad uses P2's pack. One profile
        // lock, no clones; skipped entirely while the feature is off.
        let (bg_packs, judge_packs) = if plan.smx_panels_enabled {
            profile::smx_gif_packs(config.smx_pad_gifs_pack, config.smx_judge_gifs_pack)
        } else {
            let none = [config::SmxPackName::default(); 2];
            (none, none)
        };
        self.sync_smx_pad_gifs(
            plan.smx_panels_enabled,
            config.smx_input,
            config.smx_idle_lights_black,
            config.simply_love_color,
            bg_packs,
            judge_packs,
        );
        self.sync_smx_pad_blackout(
            plan.smx_panels_enabled,
            session.play_style,
            session.player_side,
        );
        if plan.smx_select_music_beat {
            // One f32 per frame; the driver drops it unless the background is
            // actually beat-locked.
            self.smx_panels
                .set_beat(screens::select_music::selection_anim_beat(
                    &self.state.screens.select_music_state,
                ));
        }
        self.lights.tick(delta_time, elapsed_seconds);
    }

    fn lobby_runtime_view() -> SimplyLoveLobbyRuntimeView {
        let (snapshot, reconnect_status_text) = deadsync_online::lobbies::runtime_view();
        SimplyLoveLobbyRuntimeView {
            snapshot,
            reconnect_status_text,
            disconnect_hold_seconds: deadsync_online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS,
        }
    }

    fn refresh_lobby_runtime_view() -> SimplyLoveLobbyRuntimeView {
        deadsync_online::lobbies::runtime_poll_reconnect_default();
        Self::lobby_runtime_view()
    }

    fn groovestats_service_view() -> SimplyLoveGrooveStatsService {
        match deadsync_online::runtime::active_groovestats_service() {
            deadsync_online::groovestats::Service::GrooveStats => {
                SimplyLoveGrooveStatsService::GrooveStats
            }
            deadsync_online::groovestats::Service::BoogieStats => {
                SimplyLoveGrooveStatsService::BoogieStats
            }
        }
    }

    fn evaluation_submission_view(
        score_info: Option<&evaluation::ScoreInfo>,
    ) -> EvaluationSubmissionView {
        let Some(score_info) = score_info else {
            return EvaluationSubmissionView::default();
        };
        let chart_hash = score_info.chart.short_hash.as_str();
        let side = score_info.side;
        EvaluationSubmissionView {
            groovestats_status: scores::get_groovestats_submit_ui_status_for_side(chart_hash, side),
            arrowcloud_status: scores::get_arrowcloud_submit_ui_status_for_side(chart_hash, side),
            event_progress: scores::get_groovestats_submit_event_progress_for_side(
                chart_hash, side,
            ),
            record_banner: scores::get_groovestats_submit_record_banner_for_side(chart_hash, side),
            groovestats_next_retry_secs: scores::groovestats_next_retry_remaining_secs(
                chart_hash, side,
            ),
            arrowcloud_next_retry_secs: scores::arrowcloud_next_retry_remaining_secs(
                chart_hash, side,
            ),
            groovestats_next_retry_is_auto: scores::groovestats_next_retry_is_auto(
                chart_hash, side,
            ),
            arrowcloud_next_retry_is_auto: scores::arrowcloud_next_retry_is_auto(chart_hash, side),
        }
    }

    fn evaluation_init_view(gameplay: &gameplay::State) -> EvaluationInitView {
        EvaluationInitView {
            players: std::array::from_fn(|player_idx| {
                if player_idx >= gameplay.num_players().min(MAX_PLAYERS) {
                    return EvaluationInitPlayerView::default();
                }
                let side = if gameplay.num_players() >= 2 {
                    profile_data::player_side_for_index(player_idx)
                } else {
                    profile::get_session_player_side()
                };
                let chart_hash = gameplay.charts()[player_idx].short_hash.as_str();
                EvaluationInitPlayerView {
                    machine_records: scores::get_machine_leaderboard_local(chart_hash, usize::MAX),
                    personal_records: scores::get_personal_leaderboard_local_for_side(
                        chart_hash,
                        side,
                        usize::MAX,
                    ),
                    groovestats: scores::groovestats_eval_state_from_gameplay(gameplay, player_idx),
                    itl: scores::itl_eval_state_from_gameplay(gameplay, player_idx),
                }
            }),
        }
    }

    fn evaluation_runtime_view(state: &evaluation::State) -> EvaluationRuntimeView {
        let config = config::get();
        let profile_view = profile_data::runtime_scorebox_view(
            config.enable_groovestats,
            config.enable_arrowcloud,
            config.auto_populate_gs_scores,
        );
        let leaderboard_requests = evaluation::leaderboard_requests(state);
        let leaderboards: [Option<deadsync_score::CachedPlayerLeaderboardData>; MAX_PLAYERS] =
            std::array::from_fn(|player_idx| {
                if !state.allow_online_panes || !leaderboard_requests[player_idx] {
                    return None;
                }
                let score_info = state.score_info.get(player_idx)?.as_ref()?;
                scores::get_or_fetch_player_leaderboards_for_profile(
                    score_info.chart.short_hash.as_str(),
                    &profile_view.sides[profile_data::player_side_index(score_info.side)]
                        .leaderboard,
                    EVALUATION_LEADERBOARD_ROWS,
                )
            });
        EvaluationRuntimeView {
            lobby: Self::refresh_lobby_runtime_view(),
            groovestats_service: Self::groovestats_service_view(),
            submissions: std::array::from_fn(|player_idx| {
                Self::evaluation_submission_view(
                    state.score_info.get(player_idx).and_then(Option::as_ref),
                )
            }),
            scoreboxes: std::array::from_fn(|player_idx| {
                let Some(score_info) = state.score_info.get(player_idx).and_then(Option::as_ref)
                else {
                    return ScoreboxSideView::default();
                };
                Self::scorebox_side_view(
                    profile_view.sides[profile_data::player_side_index(score_info.side)].clone(),
                    Some(score_info.chart.short_hash.clone()),
                    leaderboards[player_idx].clone(),
                )
            }),
        }
    }

    fn execute_evaluation_score_runtime(gameplay: &gameplay::State) {
        // Persist one score file per play, including fails and replay lane input,
        // unless the gameplay runtime marked the run as disqualified.
        scores::save_local_scores_from_gameplay(gameplay);
        let _ = scores::save_itl_data_from_gameplay(gameplay);
        scores::submit_groovestats_payloads_from_gameplay(gameplay);
        scores::submit_arrowcloud_payloads_from_gameplay(gameplay, gameplay.pack_group.as_ref());
    }

    fn retry_evaluation_submissions(state: &evaluation::State) -> bool {
        let mut retried = false;
        for score_info in state.score_info.iter().flatten() {
            let chart_hash = score_info.chart.short_hash.as_str();
            retried |= scores::retry_groovestats_submit(chart_hash, score_info.side);
            retried |= scores::retry_arrowcloud_submit(chart_hash, score_info.side);
        }
        retried
    }

    fn sync_active_online_runtime_view(&mut self) {
        match self.state.screens.current_screen {
            CurrentScreen::Gameplay => {
                if let Some(state) = self.state.screens.gameplay_state.as_mut() {
                    gameplay::sync_lobby_runtime_view(state, Self::refresh_lobby_runtime_view());
                }
            }
            CurrentScreen::Evaluation => {
                scores::tick_groovestats_auto_retries();
                scores::tick_arrowcloud_auto_retries();
                let view = Self::evaluation_runtime_view(&self.state.screens.evaluation_state);
                evaluation::sync_runtime_view(&mut self.state.screens.evaluation_state, view);
            }
            _ => {}
        }
    }

    fn scorebox_side_view(
        player: profile_data::ScoreboxProfileView,
        chart_hash: Option<String>,
        leaderboards: Option<deadsync_score::CachedPlayerLeaderboardData>,
    ) -> ScoreboxSideView {
        let profile_id = player.leaderboard.persistent_profile_id();
        let local_itg = chart_hash.as_deref().and_then(|hash| {
            profile_id
                .and_then(|profile_id| scores::get_cached_local_score_for_profile(profile_id, hash))
                .map(|score| ScoreboxLocalView {
                    score_10000: score.score_percent * 10000.0,
                    failed: score.grade == deadsync_score::Grade::Failed,
                })
        });
        let local_ex = chart_hash.as_deref().and_then(|hash| {
            profile_id
                .and_then(|profile_id| {
                    scores::get_cached_local_ex_score_for_profile(profile_id, hash)
                })
                .map(|score| ScoreboxLocalView {
                    score_10000: score.percent * 100.0,
                    failed: score.is_fail,
                })
        });
        let local_hard_ex = chart_hash.as_deref().and_then(|hash| {
            profile_id
                .and_then(|profile_id| {
                    scores::get_cached_local_hard_ex_score_for_profile(profile_id, hash)
                })
                .map(|score| ScoreboxLocalView {
                    score_10000: score.percent * 100.0,
                    failed: score.is_fail,
                })
        });
        let local_itl = chart_hash.as_deref().and_then(|hash| {
            scores::get_cached_itl_score_for_profile(hash, profile_id).map(|score| {
                ScoreboxLocalView {
                    score_10000: f64::from(score.ex_hundredths),
                    failed: false,
                }
            })
        });
        let machine_itg = chart_hash.as_deref().and_then(|hash| {
            scores::get_machine_record_local(hash).map(|(name, score)| ScoreboxMachineView {
                name,
                score_10000: score.score_percent * 10000.0,
                failed: score.grade == deadsync_score::Grade::Failed,
            })
        });
        ScoreboxSideView {
            joined: player.joined,
            chart_hash,
            groovestats_active: player.leaderboard.gs_active,
            show_ex_score: player.leaderboard.show_ex_score,
            display_name: player.display_name,
            groovestats_username: player.groovestats_username,
            player_initials: player.player_initials,
            local_itg,
            local_ex,
            local_hard_ex,
            local_itl,
            machine_itg,
            leaderboards,
        }
    }

    /// Translate the complete fixed wheel request in one pass so profiles,
    /// caches, and online snapshots are captured once per frame. The large
    /// entry match is intentionally kept here because splitting it would pass
    /// the same runtime context through several single-use wrappers.
    fn prepare_music_wheel_runtime(
        request: MusicWheelRuntimeRequest<'_>,
        profiles: &profile_data::ScoreboxRuntimeView,
    ) -> MusicWheelRuntimeView {
        let joined = [profiles.sides[0].joined, profiles.sides[1].joined];
        let profile_ids: [Option<&str>; 2] = std::array::from_fn(|side_idx| {
            profiles.sides[side_idx].leaderboard.persistent_profile_id()
        });
        for side_idx in 0..2 {
            let Some(profile_id) = profile_ids[side_idx] else {
                continue;
            };
            if request.read_scores {
                scores::ensure_score_caches_loaded(profile_id);
            }
            if request.rank_source != MusicWheelRankSource::None || request.read_itl_scores {
                scores::ensure_itl_wheel_caches_loaded(profile_id);
            }
        }
        let itl_contexts: [Option<scores::ItlWheelSideContext<'_>>; 2] =
            std::array::from_fn(|side_idx| {
                let fetch = request.sides[side_idx];
                (joined[side_idx]
                    && (request.rank_source != MusicWheelRankSource::None
                        || request.read_itl_scores
                        || fetch.fetch_itl_rank
                        || fetch.fetch_itl_score
                        || fetch.fetch_srpg_score))
                    .then(|| {
                        scores::ItlWheelSideContext::for_profile(
                            &profiles.sides[side_idx].leaderboard,
                        )
                    })
            });
        for (side_idx, side_request) in request.sides.into_iter().enumerate() {
            if !joined[side_idx] {
                continue;
            }
            let Some(chart_hash) = side_request.chart_hash else {
                continue;
            };
            if let Some(context) = itl_contexts[side_idx].as_ref() {
                if side_request.fetch_itl_rank {
                    let _ = context.get_or_fetch_tournament_rank(chart_hash);
                }
                if side_request.fetch_itl_score {
                    let _ = context.get_or_fetch_self_ex_score(chart_hash);
                }
                if side_request.fetch_srpg_score {
                    let _ = context.get_or_fetch_srpg_self_score(chart_hash);
                }
            }
        }
        let overall_ranks: [Option<Arc<std::collections::HashMap<String, u32>>>; 2] =
            std::array::from_fn(|side_idx| {
                (joined[side_idx] && request.rank_source == MusicWheelRankSource::Overall).then(
                    || {
                        scores::get_cached_itl_tournament_overall_ranks_for_profile(
                            side_idx,
                            joined[side_idx],
                            &profiles.sides[side_idx].leaderboard,
                        )
                    },
                )
            });
        let favorite_queries = request.slots.map(|slot| match slot {
            MusicWheelSlotRuntimeRequest::Empty => profile_data::FavoriteMembershipQuery::None,
            MusicWheelSlotRuntimeRequest::Pack { key } => {
                profile_data::FavoriteMembershipQuery::Pack(key)
            }
            MusicWheelSlotRuntimeRequest::Song { song, .. } => {
                profile_data::FavoriteMembershipQuery::Song(song)
            }
        });
        let favorite_membership = profile_data::runtime_favorite_membership(&favorite_queries);
        let slots = std::array::from_fn(|slot_idx| {
            let mut view = MusicWheelSlotRuntimeView::default();
            match request.slots[slot_idx] {
                MusicWheelSlotRuntimeRequest::Empty => {}
                MusicWheelSlotRuntimeRequest::Pack { .. } => {
                    for side_idx in 0..2 {
                        view.sides[side_idx].favorite =
                            joined[side_idx] && favorite_membership[slot_idx][side_idx];
                    }
                }
                MusicWheelSlotRuntimeRequest::Song {
                    song,
                    chart_hashes,
                    is_srpg_event,
                } => {
                    let unlock_song_dir = deadsync_simfile::playlist::song_pack_and_dir_name(song)
                        .and_then(|(pack_dir, song_dir)| {
                            scores::is_itl_unlocks_pack(pack_dir).then_some(song_dir)
                        });
                    for side_idx in 0..2 {
                        if !joined[side_idx] {
                            continue;
                        }
                        let chart_hash = chart_hashes[side_idx];
                        let profile_id = profile_ids[side_idx];
                        let context = itl_contexts[side_idx].as_ref();
                        let side_view = &mut view.sides[side_idx];
                        side_view.favorite = favorite_membership[slot_idx][side_idx];
                        side_view.locked = unlock_song_dir.is_some_and(|song_dir| {
                            !scores::is_itl_song_folder_unlocked_with_profile(song_dir, profile_id)
                        });
                        if let Some(chart_hash) = chart_hash {
                            side_view.score = request
                                .read_scores
                                .then(|| {
                                    profile_id.and_then(|id| {
                                        scores::get_cached_score_with_profile(chart_hash, id)
                                    })
                                })
                                .flatten();
                            side_view.itl_rank = match request.rank_source {
                                MusicWheelRankSource::None => None,
                                MusicWheelRankSource::Chart => context
                                    .and_then(|context| context.cached_tournament_rank(chart_hash)),
                                MusicWheelRankSource::Overall => overall_ranks[side_idx]
                                    .as_ref()
                                    .and_then(|ranks| ranks.get(chart_hash))
                                    .copied(),
                            };
                            if is_srpg_event {
                                side_view.srpg_pass_rate_hundredths = profile_id.and_then(|id| {
                                    scores::get_cached_local_pass_rate_with_profile(chart_hash, id)
                                });
                                side_view.srpg_itl_ex_hundredths = request
                                    .read_itl_scores
                                    .then(|| {
                                        context.and_then(|context| {
                                            context.cached_srpg_self_score(chart_hash)
                                        })
                                    })
                                    .flatten();
                            } else if request.read_itl_scores {
                                side_view.local_itl = context
                                    .and_then(|context| context.cached_local_itl_score(song));
                                side_view.online_itl_ex_hundredths = context
                                    .and_then(|context| context.cached_self_ex_score(chart_hash));
                                side_view.online_itl_points =
                                    side_view.online_itl_ex_hundredths.and_then(|online_ex| {
                                        song.charts
                                            .iter()
                                            .find(|chart| chart.short_hash == chart_hash)
                                            .and_then(|chart| {
                                                scores::itl_points_for_chart(chart, online_ex)
                                            })
                                    });
                            }
                        }
                    }
                }
            }
            view
        });
        MusicWheelRuntimeView {
            joined,
            play_style: profiles.play_style,
            slots,
        }
    }

    fn prepare_select_course_score(
        request: SelectCourseScoreRequest<'_>,
        profiles: &profile_data::ScoreboxRuntimeView,
    ) -> SelectCourseScoreView {
        let pane_side =
            if profile_data::is_single_p2_side(profiles.play_style, profiles.player_side) {
                profile_data::PlayerSide::P2
            } else {
                profile_data::PlayerSide::P1
            };
        let pane_profile = &profiles.sides[profile_data::player_side_index(pane_side)];
        let player_score_percent = request
            .course_hash
            .and_then(|hash| {
                pane_profile
                    .leaderboard
                    .persistent_profile_id()
                    .and_then(|profile_id| {
                        scores::get_cached_local_score_for_profile(profile_id, hash)
                    })
            })
            .filter(|score| {
                score.grade != deadsync_score::Grade::Failed || score.score_percent > 0.0
            })
            .map(|score| score.score_percent);
        let (machine_initials, machine_score_percent) = request
            .course_hash
            .and_then(scores::get_machine_record_local)
            .filter(|(_, score)| {
                score.grade != deadsync_score::Grade::Failed || score.score_percent > 0.0
            })
            .map_or((None, None), |(initials, score)| {
                (Some(initials), Some(score.score_percent))
            });
        SelectCourseScoreView {
            mode_show_ex_score: profiles.sides[0].leaderboard.show_ex_score,
            pane_show_ex_score: pane_profile.leaderboard.show_ex_score,
            player_initials: pane_profile.player_initials.clone(),
            player_score_percent,
            machine_initials,
            machine_score_percent,
        }
    }

    fn sync_select_course_runtime_view(&mut self, config: &config::Config) {
        if self.state.screens.current_screen != CurrentScreen::SelectCourse {
            return;
        }
        let profile_view = profile_data::runtime_scorebox_view(
            config.enable_groovestats,
            config.enable_arrowcloud,
            config.auto_populate_gs_scores,
        );
        let music_wheel = Self::prepare_music_wheel_runtime(
            select_course::music_wheel_runtime_request(&self.state.screens.select_course_state),
            &profile_view,
        );
        let score = Self::prepare_select_course_score(
            select_course::score_runtime_request(&self.state.screens.select_course_state),
            &profile_view,
        );
        select_course::sync_runtime_view(
            &mut self.state.screens.select_course_state,
            SelectCourseRuntimeView { music_wheel, score },
        );
    }

    fn sync_main_menu_runtime_view(&mut self) {
        let view = crate::main_menu::runtime_view();
        menu::sync_runtime_view(&mut self.state.screens.menu_state, view);
    }

    fn sync_light_input(&mut self, ev: &InputEvent) {
        match light_input_route(ev.action, ev.pressed) {
            LightInputRoute::Pad {
                player,
                button,
                pressed,
            } => {
                self.lights.set_button_pressed(player, button, pressed);
            }
            LightInputRoute::Menu {
                player,
                button,
                pressed,
            } => {
                self.lights.set_menu_button_pressed(player, button, pressed);
            }
            LightInputRoute::Ignore => {}
        }
    }

    fn current_light_hide_flags(&self) -> [HideFlags; 2] {
        let screen = self.state.screens.current_screen;
        let gameplay_state = match screen {
            CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_ref(),
            CurrentScreen::Practice => self
                .state
                .screens
                .practice_state
                .as_ref()
                .map(|state| &state.gameplay),
            _ => None,
        };
        gameplay_state.map_or([HideFlags::default(); 2], |state| {
            hide_flags_for_profiles(std::array::from_fn(|player| {
                state.profiles()[player].hide_light_type
            }))
        })
    }

    fn sync_gameplay_light_blinks(
        &mut self,
        target: GameplayLightSyncTarget,
        simplify_bass: bool,
        smx_enabled: bool,
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
    ) {
        match target {
            GameplayLightSyncTarget::Gameplay => {
                if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
                    self.gameplay_lights
                        .queue_blinks(&mut self.lights, gs, simplify_bass);
                    if smx_enabled {
                        self.smx_panels.update(gs, play_style, player_side);
                    } else {
                        self.smx_panels.deactivate();
                    }
                    return;
                }
            }
            GameplayLightSyncTarget::Practice => {
                if let Some(ps) = self.state.screens.practice_state.as_ref() {
                    self.gameplay_lights.queue_blinks(
                        &mut self.lights,
                        &ps.gameplay,
                        simplify_bass,
                    );
                    if smx_enabled {
                        self.smx_panels
                            .update(&ps.gameplay, play_style, player_side);
                    } else {
                        self.smx_panels.deactivate();
                    }
                    return;
                }
            }
            GameplayLightSyncTarget::Clear => {}
        }
        self.gameplay_lights.clear();
        self.smx_panels.deactivate();
    }

    /// Keep the SMX pad GIF state in step with the options and the current screen:
    /// the full-pad background follows the screen role (and the current song's
    /// per-song/per-pack override, if any), and the judgement animation set
    /// follows the (enabled, pack) pair. Cheap per frame: lookups only happen
    /// when the toggle, screen role, pack, or current song folder changes, and
    /// the driver deduplicates the rest.
    fn sync_smx_pad_gifs(
        &mut self,
        enabled: bool,
        smx_input: bool,
        idle_black: bool,
        theme_index: i32,
        bg_packs: [config::SmxPackName; 2],
        judge_packs: [config::SmxPackName; 2],
    ) {
        let frame_plan = smx_pad_gif_frame_plan(
            self.state.screens.current_screen,
            enabled,
            smx_input,
            options::is_smx_config_view(&self.state.screens.options_state),
            &self.state.screens.evaluation_state.score_info,
        );
        let role = frame_plan.role;

        // Idle-black mode: when the feature is on and the screen would normally
        // show a background (role is Some) but nothing resolves for it, keep the
        // worker active so the pads hold solid black instead of reverting to the
        // pad firmware's built-in lighting. Screens with no role (Init,
        // TestLights, pad assignment, the SMX options assignment preview) still
        // release the pads: they drive the LEDs themselves. Runs before the
        // dedup key check below because the option is not part of the key; the
        // driver dedups the value itself.
        self.smx_panels.set_idle_black(role.is_some() && idle_black);

        // A song rescan may have changed per-song/per-pack files; drop the
        // scoped cache and force a re-resolve (a recycled `Arc` pointer must not
        // read as the same song).
        let generation = deadsync_simfile::runtime_cache::song_cache_generation();
        if generation != self.smx_scoped_bg_generation {
            self.smx_scoped_bg_generation = generation;
            self.smx_scoped_bg_cache.clear();
            self.smx_bg_synced = None;
        }

        // The song whose per-song/per-pack background applies here: the playing
        // song in gameplay, the highlighted song on song select. Identified by
        // `Arc` pointer so the per-frame dedup never allocates.
        let song = frame_plan
            .current_song_needed
            .then(|| self.current_smx_song())
            .flatten();
        let song_id = song.as_ref().map(|s| std::sync::Arc::as_ptr(s) as usize);

        // On results screens, include the grade (and the difficulty of the chart
        // that earned it) in the key so a new result re-resolves to a
        // grade/difficulty-specific gif even when the role and song haven't changed.
        let result_context = frame_plan.result_context;
        let eval_grade = result_context.grade;
        let eval_difficulty = result_context.difficulty;

        let synced = SmxAnimationSyncKey::new(
            enabled,
            role,
            bg_packs,
            judge_packs,
            song_id,
            result_context.grade_sprite_state(),
            eval_difficulty,
        );
        if self.smx_bg_synced == Some(synced) {
            return;
        }
        let pack_changed = self
            .smx_bg_synced
            .is_none_or(|previous| previous.packs_changed(synced));
        self.smx_bg_synced = Some(synced);

        let song_dir = song
            .as_ref()
            .and_then(|s| s.simfile_path.parent())
            .map(Path::to_path_buf);
        // The song's tempo selects among BPM variants of a role (denser gif at
        // low tempo, sparser at high). `max_bpm` is the conservative pick: the
        // chosen variant stays under the pad's 30fps even at the song's fastest.
        let song_bpm = song
            .as_ref()
            .map(|s| s.max_bpm as f32)
            .filter(|b| b.is_finite() && *b > 0.0);

        // Resolve and push the background for each pad slot independently so P1 and P2
        // can show different packs in versus mode. Scoped (per-song/per-pack folder) gifs
        // are pack-independent and may resolve to the same Arc for both slots; the driver
        // deduplicates by pointer so no redundant work reaches the worker.
        for pad in 0..deadsync_smx::panels::PADS {
            let pack_str = (!bg_packs[pad].is_empty()).then_some(bg_packs[pad].as_str());
            let anim = role.and_then(|role| {
                // Resolution order: the song's own background, then its pack's, then
                // the global pack (selected -> basic), then the global `default`
                // role. `_25` is the baseline both pad layouts render; 16-LED pads
                // show its outer ring. Each tier picks the BPM-best variant.
                // Scoped (song/pack folder) gifs are authored by the song and sit
                // above pack policy, so the selected pack's `CanBeEmpty` does not
                // affect them.
                let scoped = song_dir
                    .as_deref()
                    .and_then(|dir| self.resolve_scoped_smx_background(dir, role, song_bpm));
                scoped.or_else(|| {
                    let registry = self.smx_gif_registry().clone();
                    let size = deadsync_smx::gifs::PadSize::Leds25;
                    // Global-registry role candidates, most specific first: on
                    // results screens the grade- and difficulty-specific roles
                    // (`results_role_candidates` documents and tests the exact
                    // order), then the screen role, then the global `default`
                    // role. A candidate the selected pack declares under
                    // `CanBeEmpty` (and doesn't supply) ends the chain with no
                    // animation at all: the pack opted that name out, so a later
                    // candidate must not resurrect one.
                    let mut candidates: Vec<String> = if role == "results" {
                        eval_grade
                            .map(|grade| {
                                deadsync_smx::panel_fx::results_role_candidates(
                                    grade,
                                    eval_difficulty,
                                )
                            })
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    candidates.push(role.to_owned());
                    candidates.push("default".to_owned());
                    for name in &candidates {
                        if registry.background_declared_empty(pack_str, name, size) {
                            return None;
                        }
                        if let Some(anim) = registry.background(pack_str, name, size, song_bpm) {
                            // Only pack-resolved gifs get tinted; a per-song/pack
                            // scoped gif (the `scoped` branch above) is fully
                            // authored by the song and left as-is.
                            return Some(self.maybe_tint_smx_background(
                                pack_str,
                                role,
                                eval_difficulty,
                                anim,
                                theme_index,
                            ));
                        }
                    }
                    None
                })
            });
            let background = anim.map(|anim| {
                // A beat-suffixed gif beat-locks on song select, the one screen with
                // a live beat source (the music preview); elsewhere it plays realtime
                // rather than freezing on a stale beat.
                let clock = match anim.beats_per_loop {
                    Some(beats_per_loop) if frame_plan.beat_locked => {
                        deadsync_smx::panels::Clock::BeatLocked { beats_per_loop }
                    }
                    _ => deadsync_smx::panels::Clock::Realtime,
                };
                (anim, clock)
            });
            self.smx_panels.set_background_for_pad(pad, background);
        }

        if pack_changed {
            for pad in 0..deadsync_smx::panels::PADS {
                let gifs = if enabled {
                    let registry = self.smx_gif_registry().clone();
                    let judge_pack_str =
                        (!judge_packs[pad].is_empty()).then_some(judge_packs[pad].as_str());
                    deadsync_smx::panel_fx::JudgementGifs::resolve(&registry, judge_pack_str)
                } else {
                    deadsync_smx::panel_fx::JudgementGifs::default()
                };
                self.smx_panels.set_judgement_gifs_for_pad(pad, gifs);
            }
        }
    }

    /// Black out the unused pad slot when a single player is in game mode.
    /// In Versus / Doubles both pads are in use; in Single play the non-session slot
    /// gets solid black so only the pad the player stands on is lit.
    fn sync_smx_pad_blackout(
        &mut self,
        enabled: bool,
        play_style: profile_data::PlayStyle,
        player_side: profile_data::PlayerSide,
    ) {
        let blackout = smx_pad_blackout(
            self.state.screens.current_screen,
            enabled,
            play_style,
            player_side,
        );
        if blackout != self.smx_blackout_synced {
            self.smx_blackout_synced = blackout;
            for (pad, &on) in blackout.iter().enumerate() {
                self.smx_panels.set_pad_blackout(pad, on);
            }
        }
    }

    /// The song whose per-song/per-pack SMX background applies on the current
    /// screen: the playing song in gameplay/practice, the highlighted song on
    /// song select. `None` elsewhere or with no song. A cheap `Arc` clone.
    fn current_smx_song(&self) -> Option<std::sync::Arc<deadsync_chart::SongData>> {
        let screens = &self.state.screens;
        match screens.current_screen {
            CurrentScreen::Gameplay => screens.gameplay_state.as_ref().map(|gs| gs.song_arc()),
            CurrentScreen::Practice => screens
                .practice_state
                .as_ref()
                .map(|ps| ps.gameplay.song_arc()),
            CurrentScreen::SelectMusic => {
                select_music::highlighted_song(&screens.select_music_state)
            }
            _ => None,
        }
    }

    /// Resolve a per-song then per-pack background for `role` from the song
    /// folder's and pack folder's `smx-pad-lights/` subfolders, decoding once
    /// and caching the variant list (an empty list is the negative "no gif
    /// here" entry) so a folder is touched at most once per song-cache
    /// generation. Picks the BPM-best variant for `song_bpm`.
    fn resolve_scoped_smx_background(
        &mut self,
        song_dir: &Path,
        role: &'static str,
        song_bpm: Option<f32>,
    ) -> Option<std::sync::Arc<deadsync_smx::gifs::FullPadAnim>> {
        let song_scope = song_dir.join("smx-pad-lights");
        let pack_scope = song_dir.parent().map(|p| p.join("smx-pad-lights"));
        for dir in std::iter::once(song_scope).chain(pack_scope) {
            let variants = self
                .smx_scoped_bg_cache
                .entry((dir.clone(), role))
                .or_insert_with(|| {
                    deadsync_smx::gifs::load_scoped_background(
                        &dir,
                        role,
                        deadsync_smx::gifs::PadSize::Leds25,
                    )
                });
            if let Some(anim) = deadsync_smx::gifs::select_variant(variants, song_bpm) {
                return Some(anim);
            }
        }
        None
    }

    /// Decode the SMX GIF assets on first use. A cold path: runs when the pad-gifs
    /// option first resolves a background, never per frame.
    fn smx_gif_registry(&mut self) -> &std::sync::Arc<deadsync_smx::gifs::GifRegistry> {
        self.smx_gifs.get_or_insert_with(|| {
            let root = dirs::app_dirs().resolve_asset_path("assets");
            std::sync::Arc::new(deadsync_smx::gifs::GifRegistry::load(&root))
        })
    }

    /// Recolor a resolved background to the played chart's difficulty color if
    /// `pack` declares `role` under `MatchColorToDifficulty` in its
    /// `gifpack.ini`; otherwise returns `anim` unchanged. Reuses the same
    /// theme-relative color the rest of the UI uses for difficulty (see
    /// `deadlib_present::color::difficulty_rgba`), so `Challenge` always tints
    /// to the player's current theme color and easier difficulties step back
    /// around the same hue wheel. Cheap: a cache keyed by (pack, role,
    /// difficulty) holds one tinted `Arc` per combination, recomputed only
    /// when the player's theme color changes (never per frame).
    fn maybe_tint_smx_background(
        &mut self,
        pack_str: Option<&str>,
        role: &'static str,
        difficulty: Option<&'static str>,
        anim: std::sync::Arc<deadsync_smx::gifs::FullPadAnim>,
        theme_index: i32,
    ) -> std::sync::Arc<deadsync_smx::gifs::FullPadAnim> {
        let Some(difficulty) = difficulty else {
            return anim;
        };
        if !self
            .smx_gif_registry()
            .background_wants_difficulty_tint(pack_str, role)
        {
            return anim;
        }
        let cache_key = (
            pack_str
                .unwrap_or(deadsync_smx::gifs::DEFAULT_PACK)
                .to_owned(),
            role,
            difficulty,
        );
        if let Some((cached_theme, cached_anim)) = self.smx_difficulty_tint_cache.get(&cache_key)
            && *cached_theme == theme_index
        {
            return cached_anim.clone();
        }
        let target_rgba = deadlib_present::color::difficulty_rgba(difficulty, theme_index)
            .map(|c| (c * 255.0).round() as u8);
        // The palette is sRGB for screen use; the pad LEDs are linear, so the
        // pastel difficulty colors read as off-white without this correction.
        let target_rgb =
            deadsync_smx::gifs::saturate_for_leds([target_rgba[0], target_rgba[1], target_rgba[2]]);
        let tinted = std::sync::Arc::new(deadsync_smx::gifs::tint_full_pad(&anim, target_rgb));
        self.smx_difficulty_tint_cache
            .insert(cache_key, (theme_index, tinted.clone()));
        tinted
    }

    #[inline(always)]
    fn stats_overlay_timing(&self) -> Option<TimingHealth> {
        if !self.state.shell.overlay_mode.shows_timing() {
            return None;
        }
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        Some(timing_health(
            self.state.shell.last_present_stats,
            display_clock.error_seconds,
            display_clock.catching_up,
            deadsync_audio_stream::get_output_timing_snapshot(),
        ))
    }

    #[inline(always)]
    fn request_redraw(&mut self, window: &Window, reason: &'static str) {
        self.state
            .shell
            .note_redraw_requested(Instant::now(), reason);
        window.request_redraw();
    }

    #[inline(always)]
    fn request_redraw_if_needed(&mut self, window: &Window, reason: &'static str) {
        if !self.state.shell.redraw_pending() {
            self.request_redraw(window, reason);
        }
    }

    #[inline(always)]
    fn chain_continuous_redraw(&mut self, window: &Window) {
        if self.redraw_interval_state(window).interval.is_none()
            && !self.state.shell.should_skip_compose_and_draw()
        {
            self.request_redraw_if_needed(window, "chain");
        }
    }

    fn log_frame_loop_mode(&mut self, mode: FrameLoopMode) {
        if !self.state.shell.note_frame_loop_mode(mode) {
            return;
        }
        let screen = self.state.screens.current_screen;
        let focused = self.state.shell.frame_loop.window_focused();
        let occluded = self.state.shell.frame_loop.window_occluded();
        let surface_active = self.state.shell.frame_loop.surface_active();
        let max_fps = self
            .state
            .shell
            .frame_loop
            .frame_interval()
            .map(|interval| (1.0 / interval.as_secs_f64()).round() as u32)
            .unwrap_or(0);
        match mode {
            FrameLoopMode::Poll => debug!(
                "Frame pacing: poll screen={screen:?} focused={focused} occluded={occluded} surface_active={surface_active} vsync={} present={} max_fps={max_fps}",
                self.state.shell.vsync_enabled, self.state.shell.present_mode_policy,
            ),
            FrameLoopMode::WaitPending => debug!(
                "Frame pacing: wait_pending screen={screen:?} focused={focused} occluded={occluded} surface_active={surface_active} vsync={} present={} max_fps={max_fps}",
                self.state.shell.vsync_enabled, self.state.shell.present_mode_policy,
            ),
            FrameLoopMode::Scheduled(reason, interval) => debug!(
                "Frame pacing: scheduled reason={} interval_ms={:.3} screen={screen:?} focused={focused} occluded={occluded} surface_active={surface_active} vsync={} present={} max_fps={max_fps}",
                reason.as_str(),
                interval.as_secs_f64() * 1000.0,
                self.state.shell.vsync_enabled,
                self.state.shell.present_mode_policy,
            ),
        }
    }

    fn run_frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        redraw_started: Instant,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
    ) {
        let prev_frame_end = self.state.shell.last_frame_end_time;
        let pre_redraw_gap_us = elapsed_us_between(redraw_started, prev_frame_end);
        let delta_time = redraw_started
            .duration_since(self.state.shell.last_frame_time)
            .as_secs_f32();
        self.state.shell.last_frame_time = redraw_started;
        let total_elapsed = redraw_started
            .duration_since(self.state.shell.start_time)
            .as_secs_f32();

        // Tab acceleration scales non-gameplay screen dt. Gameplay, Practice,
        // and gameplay steps under evaluation transitions stay on wall-clock
        // `delta_time`. See issue #174.
        let tab_acceleration_allowed = !matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        );
        let logic_dt = self
            .state
            .shell
            .interaction
            .controls()
            .logic_delta(delta_time, tab_acceleration_allowed);
        deadlib_present::runtime::tick(logic_dt);
        screens::components::shared::visual_style_bg::tick_global(logic_dt);

        // One immutable configuration snapshot owns this frame. Downstream
        // gameplay and device helpers must not reacquire the global config lock.
        let frame_config = config::get();
        self.sync_gameplay_input_capture();
        self.sync_pad_config_fsr(&frame_config);
        self.reconcile_smx_assignment(&frame_config);
        self.maybe_autoprompt_smx_assign(&frame_config);
        self.drive_smx_options_lights(delta_time, &frame_config);
        self.drive_smx_player_options_lights(delta_time, &frame_config);
        self.apply_smx_managed_preset(&frame_config);
        self.drive_smx_light_brightness(&frame_config);
        self.state.shell.interaction.update_message(redraw_started);
        self.sync_options_song_packs();
        self.sync_options_stepmaniaonline();
        self.poll_profile_load();
        self.poll_profile_import();
        self.poll_qr_login();
        self.poll_score_import();
        self.poll_sync_analysis();
        self.sync_active_online_runtime_view();
        self.heart_rate.sync(
            frame_config.machine_enable_heart_rate_monitors,
            self.state.screens.current_screen == CurrentScreen::PlayerOptions,
        );

        let mut upload_us: u32 = 0;
        let mut draw_us: u32 = 0;
        let mut draw_stats = renderer::DrawStats::default();
        let input_started = Instant::now();
        if let Err(e) = self.flush_due_input_events(event_loop) {
            error!("Failed to handle debounced input: {e}");
            event_loop.exit();
            return;
        }
        let input_us: u32 = elapsed_us_since(input_started);

        let update_started = Instant::now();
        let transition_plan = self.state.shell.transition.advance_frame(
            logic_dt,
            self.state.screens.current_screen,
            MENU_ACTORS_FADE_DURATION,
        );
        let transition_lobby_effect = if transition_plan.tick_gameplay
            && let Some(gs) = self.state.screens.gameplay_state.as_mut()
        {
            // Keep gameplay stepping under evaluation fades so late judgments
            // and HUD animations settle while transition input remains blocked.
            lobby_effect_only(crate::gameplay_runtime::update(
                gs,
                delta_time,
                self.state.play_input_policy.smx_input,
            ))
        } else {
            None
        };
        if let Some(effect) = transition_lobby_effect {
            let _ = self.handle_action(effect, event_loop);
        }
        let step_plan = frame_screen_step_plan(FrameScreenStepContext {
            current_screen: self.state.screens.current_screen,
            transition_step_screen: transition_plan.step_screen,
            gameplay_offset_prompt_active: self.state.gameplay_offset_save_prompt.is_some(),
        });
        if transition_plan.step_screen {
            if step_plan.step_screen {
                let smx_assignment = crate::smx_config::smx_assignment_view();
                let (action, _) = self.state.screens.step_idle(
                    logic_dt,
                    redraw_started,
                    &self.state.session,
                    &self.asset_manager,
                    &smx_assignment,
                    self.state.play_input_policy.smx_input,
                );
                if let Some(action) = action
                    && !matches!(action, ThemeEffect::None)
                {
                    let _ = self.handle_action(action, event_loop);
                }
            }
            let current_screen = self.state.screens.current_screen;
            let auto_screenshot_ready = current_screen == CurrentScreen::Evaluation
                && evaluation::auto_screenshot_ready(&self.state.screens.evaluation_state);
            let auto_screenshot_plan = auto_screenshot_frame_plan(
                AutoScreenshotFrameContext {
                    screen: current_screen,
                    already_taken: self.state.screens.evaluation_state.auto_screenshot_taken,
                    ready: auto_screenshot_ready,
                    mask: if current_screen == CurrentScreen::Evaluation {
                        config::get().auto_screenshot_eval
                    } else {
                        0
                    },
                },
                auto_screenshot_eval_results(&self.state.screens.evaluation_state),
            );
            if auto_screenshot_plan.mark_taken {
                self.state.screens.evaluation_state.auto_screenshot_taken = true;
            }
            if auto_screenshot_plan.request_capture {
                self.state.shell.screenshot.request(None);
            }
        }
        match transition_plan.completion {
            Some(TransitionCompletion::ActorFadeOut(target)) => {
                self.finish_actor_fade_out(target, event_loop);
            }
            Some(TransitionCompletion::GlobalFadeOut(target)) => {
                self.on_fade_complete(target, event_loop);
            }
            None => {}
        }
        self.sync_select_music_runtime_view(&frame_config);
        self.sync_select_course_runtime_view(&frame_config);
        let update_us: u32 = elapsed_us_since(update_started);
        self.sync_lights(delta_time, total_elapsed, &frame_config);

        if self.window.as_ref().map(|w| w.id()) != Some(window.id()) {
            self.state.shell.last_frame_end_time = Instant::now();
            return;
        }
        if self.state.shell.should_skip_compose_and_draw() {
            self.state.shell.current_frame_vpf = 0;
            self.state.shell.last_frame_end_time = Instant::now();
            return;
        }

        self.sync_gameplay_background(frame_config.show_video_backgrounds);
        self.sync_theme_background_video(total_elapsed, &frame_config);
        let actor_build_started = Instant::now();
        let arrow_effect_time_s = arrow_effect_time_seconds(actor_build_started);
        let (mut actors, clear_color) = self.get_current_actors(arrow_effect_time_s, &frame_config);
        let actor_build_us = elapsed_us_since(actor_build_started);
        self.state.shell.update_fps_stats(redraw_started);
        let screens = &self.state.screens;
        let current_screen = screens.current_screen;
        let show_select_music_video_banners = frame_config.show_select_music_video_banners;
        let show_select_music_banners = frame_config.show_select_music_banners;
        let show_course_individual_scores = frame_config.show_course_individual_scores;
        let gameplay_banner_mode = frame_config.gameplay_banner_mode;
        if let Some(backend) = &mut self.backend {
            let upload_started = Instant::now();
            let gameplay_time = match current_screen {
                CurrentScreen::Gameplay => screens.gameplay_state.as_ref().map(|state| {
                    deadsync_core::song_time::song_time_ns_to_seconds(state.current_music_time_ns())
                }),
                CurrentScreen::Practice => screens.practice_state.as_ref().map(|state| {
                    deadsync_core::song_time::song_time_ns_to_seconds(
                        state.gameplay.current_music_time_ns(),
                    )
                }),
                _ => None,
            };
            {
                let post_select_stages = if show_select_music_video_banners
                    && matches!(
                        current_screen,
                        CurrentScreen::EvaluationSummary | CurrentScreen::Initials
                    ) {
                    Some(post_select_display_stages(
                        &self.state.session.played_stages,
                        &self.state.session.course_individual_stage_indices,
                        show_course_individual_scores,
                    ))
                } else {
                    None
                };
                let post_select_banner_paths: SmallVec<[&Path; 8]> = post_select_stages
                    .iter()
                    .flat_map(|stages| stages.iter())
                    .filter_map(|stage| stage.song.banner_path.as_deref())
                    .collect();
                match current_screen {
                    CurrentScreen::SelectMusic => {
                        let state = &screens.select_music_state;
                        let desired_path = if show_select_music_video_banners
                            && show_select_music_banners
                            && state.banner_high_quality_requested
                        {
                            match state.entries.get(state.selected_index) {
                                Some(select_music::MusicWheelEntry::Song(song)) => {
                                    song.banner_path.as_deref()
                                }
                                Some(select_music::MusicWheelEntry::PackHeader {
                                    banner_path,
                                    ..
                                }) => banner_path.as_deref(),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        self.dynamic_media.sync_active_banner_video(
                            &mut self.asset_manager,
                            backend,
                            desired_path,
                            true,
                        );
                    }
                    CurrentScreen::SelectCourse => {
                        let state = &screens.select_course_state;
                        let desired_path = if show_select_music_video_banners
                            && show_select_music_banners
                            && state.banner_high_quality_requested
                        {
                            match state.entries.get(state.selected_index) {
                                Some(select_music::MusicWheelEntry::Song(song)) => {
                                    song.banner_path.as_deref()
                                }
                                Some(select_music::MusicWheelEntry::PackHeader {
                                    banner_path,
                                    ..
                                }) => banner_path.as_deref(),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        self.dynamic_media.sync_active_banner_video(
                            &mut self.asset_manager,
                            backend,
                            desired_path,
                            true,
                        );
                    }
                    CurrentScreen::Evaluation => {
                        let desired_path = if show_select_music_video_banners {
                            screens
                                .evaluation_state
                                .score_info
                                .iter()
                                .flatten()
                                .find_map(|score| score.song.banner_path.as_deref())
                        } else {
                            None
                        };
                        self.dynamic_media.sync_active_banner_video(
                            &mut self.asset_manager,
                            backend,
                            desired_path,
                            true,
                        );
                    }
                    CurrentScreen::EvaluationSummary | CurrentScreen::Initials => {
                        self.dynamic_media.sync_active_banner_videos(
                            &mut self.asset_manager,
                            backend,
                            &post_select_banner_paths,
                            true,
                        );
                    }
                    CurrentScreen::Gameplay | CurrentScreen::Practice => {
                        let state = match current_screen {
                            CurrentScreen::Gameplay => screens.gameplay_state.as_ref(),
                            CurrentScreen::Practice => {
                                screens.practice_state.as_ref().map(|state| &state.gameplay)
                            }
                            _ => None,
                        };
                        if let Some(state) = state {
                            sync_gameplay_banners(
                                &mut self.dynamic_media,
                                &mut self.asset_manager,
                                backend,
                                state,
                                gameplay_banner_mode,
                            );
                        }
                    }
                    _ => {
                        self.dynamic_media.sync_active_banner_video(
                            &mut self.asset_manager,
                            backend,
                            None,
                            true,
                        );
                    }
                }
            }
            self.dynamic_media.queue_video_frames(
                &mut self.asset_manager,
                gameplay_time,
                total_elapsed,
            );
            self.asset_manager.queue_pending_generated_textures();
            self.asset_manager.drain_texture_uploads(
                backend,
                TextureUploadBudget {
                    max_uploads: LIVE_TEXTURE_UPLOAD_MAX_OPS,
                    max_bytes: LIVE_TEXTURE_UPLOAD_MAX_BYTES,
                },
            );
            upload_us = elapsed_us_since(upload_started);
        }
        let fonts = self.asset_manager.fonts();
        let build_screen_started = Instant::now();
        let collect_text_layout_stats = stutter_diag_enabled();
        let uses_gameplay_present = matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        );
        let actor_resources = match self.state.screens.current_screen {
            CurrentScreen::Gameplay => self
                .state
                .screens
                .gameplay_state
                .as_ref()
                .map(gameplay::State::actor_resources),
            CurrentScreen::Practice => self
                .state
                .screens
                .practice_state
                .as_ref()
                .map(|state| state.gameplay.actor_resources()),
            _ => None,
        };
        let (mut screen, text_layout) = if uses_gameplay_present {
            let text_layout_cache = &mut self.gameplay_text_layout_cache;
            let compose_scratch = &mut self.gameplay_compose_scratch;
            text_layout_cache.begin_frame_stats(collect_text_layout_stats);
            let screen = if let Some(actor_resources) = actor_resources {
                compose::build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                    &PRESENT_TEXTURE_CONTEXT,
                    actor_resources,
                )
            } else {
                compose::build_screen_cached_with_scratch_and_texture_context(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                    &PRESENT_TEXTURE_CONTEXT,
                )
            };
            (screen, text_layout_cache.frame_stats())
        } else {
            let text_layout_cache = &mut self.ui_text_layout_cache;
            let compose_scratch = &mut self.ui_compose_scratch;
            text_layout_cache.begin_frame_stats(collect_text_layout_stats);
            let screen = if let Some(actor_resources) = actor_resources {
                compose::build_screen_cached_with_scratch_and_texture_context_and_actor_resources(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                    &PRESENT_TEXTURE_CONTEXT,
                    actor_resources,
                )
            } else {
                compose::build_screen_cached_with_scratch_and_texture_context(
                    &actors,
                    clear_color,
                    &self.state.shell.metrics,
                    fonts,
                    total_elapsed,
                    text_layout_cache,
                    compose_scratch,
                    &PRESENT_TEXTURE_CONTEXT,
                )
            };
            (screen, text_layout_cache.frame_stats())
        };
        let build_screen_us = elapsed_us_since(build_screen_started);
        let resolve_textures_us = 0;
        let compose_us: u32 = actor_build_us
            .saturating_add(build_screen_us)
            .saturating_add(resolve_textures_us);
        let compose_breakdown: ComposeBreakdown = ComposeBreakdown {
            actor_build_us,
            build_screen_us,
            resolve_textures_us,
            render_objects: saturating_u32(screen.objects.len()),
            render_cameras: saturating_u32(screen.cameras.len()),
            text_layout,
        };

        let apply_present_back_pressure = self.apply_present_back_pressure();
        let mut capture_screenshot = false;
        if let Some(backend) = &mut self.backend {
            if self.state.shell.screenshot.pending() {
                backend.request_screenshot();
            }
            let draw_started = Instant::now();
            match backend.draw(
                &screen,
                self.asset_manager.textures(),
                apply_present_back_pressure,
            ) {
                Ok(stats) => {
                    draw_stats = stats;
                    self.state.shell.current_frame_vpf = stats.vertices;
                    self.state.shell.last_present_stats = stats.present_stats;
                    draw_us = elapsed_us_since(draw_started);
                    capture_screenshot = true;
                }
                Err(e) => {
                    error!("Failed to draw frame: {e}");
                    event_loop.exit();
                    return;
                }
            }
        }
        if uses_gameplay_present {
            self.gameplay_compose_scratch
                .recycle_render_list(&mut screen);
        } else {
            self.ui_compose_scratch.recycle_render_list(&mut screen);
        }
        if capture_screenshot {
            self.capture_pending_screenshot(redraw_started);
        }
        let frame_finished = Instant::now();
        let frame_seconds = frame_finished.duration_since(prev_frame_end).as_secs_f32();
        self.state.shell.last_frame_end_time = frame_finished;
        self.chain_continuous_redraw(&window);
        let total_elapsed_end = frame_finished
            .duration_since(self.state.shell.start_time)
            .as_secs_f32();
        let frame_host_nanos = deadlib_platform::host_time::now_nanos();
        let current_screen = self.state.screens.current_screen;
        self.state
            .shell
            .update_stutter_samples(current_screen, frame_seconds, total_elapsed_end);
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|state| state.display_clock_health())
            .unwrap_or_default();
        self.state.shell.record_frame_stats_sample(
            frame_host_nanos,
            frame_seconds,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            draw_stats,
            display_clock.error_seconds,
            display_clock.catching_up,
        );
        self.record_stutter_diag_frame(
            frame_host_nanos,
            self.state.screens.current_screen,
            frame_seconds,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            draw_stats,
        );
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|state| state.display_clock_health())
            .unwrap_or_default();
        trace_frame_stutter(
            frame_seconds,
            self.state
                .shell
                .expected_frame_seconds(self.state.screens.current_screen),
            total_elapsed_end,
            self.state.screens.current_screen,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            &actors,
            draw_stats,
            compose_breakdown,
            display_clock.error_seconds,
            display_clock.catching_up,
        );
        self.trace_stutter_diag_dump_if_needed(
            frame_host_nanos,
            total_elapsed_end,
            self.state.screens.current_screen,
            frame_seconds,
        );
        self.state.shell.gameplay_pacing_trace.record_frame(
            frame_finished,
            self.state.screens.current_screen == CurrentScreen::Gameplay,
            frame_seconds,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            draw_us,
            draw_stats,
            display_clock.error_seconds,
            display_clock.catching_up,
        );
        actors.clear();
        self.actor_scratch = actors;
    }

    fn reset_options_state_for_entry(&mut self, from: CurrentScreen) {
        let current_color_index = self.state.screens.options_state.active_color_index;
        self.state.screens.options_state = options::init(
            updater::capabilities(),
            app_paths_view(),
            audio_requests::options_view(),
            graphics::options_graphics_view(),
            options_song_pack_view(),
            noteskin_catalog_view(),
            crate::smx_config::smx_assignment_view(),
            crate::smx_config::smx_gif_catalog_view(),
        );
        self.state.screens.options_state.active_color_index = current_color_index;
        self.options_song_pack_generation =
            deadsync_simfile::runtime_cache::song_cache_generation();
        if matches!(
            from,
            CurrentScreen::Mappings | CurrentScreen::Input | CurrentScreen::ConfigurePads
        ) {
            options::open_input_submenu(&mut self.state.screens.options_state);
        } else if from == CurrentScreen::TestLights {
            options::open_lights_submenu(&mut self.state.screens.options_state);
        } else if from == CurrentScreen::SmxAssignPads {
            options::open_smx_config_submenu(&mut self.state.screens.options_state);
        }
    }

    fn new(
        backend_type: BackendType,
        overlay_mode: u8,
        color_index: i32,
        config: config::Config,
        profile_data: profile_data::Profile,
    ) -> Self {
        let software_renderer_threads = config.software_renderer_threads;
        let gfx_debug_enabled = config.gfx_debug;
        let state = AppState::new(config, profile_data, overlay_mode, color_index);
        Self {
            window: None,
            backend: None,
            backend_type,
            _idle_inhibitor: deadlib_platform::idle_inhibit::IdleInhibitor::acquire(),
            fsr_monitor: fsr_input::Monitor::new(),
            fsr_pads_active: false,
            pad_config_sync: pad_config_sync::PadConfigSync::default(),
            lights: lights::Manager::new(config.lights_driver, config.lights_com_port.as_str()),
            gameplay_lights: lights::gameplay::GameplayLightTracker::default(),
            smx_panels: SmxPanelDriver::default(),
            smx_light_brightness: [100, 100],
            smx_gifs: None,
            smx_bg_synced: None,
            smx_blackout_synced: [false; 2],
            smx_scoped_bg_cache: std::collections::HashMap::new(),
            smx_scoped_bg_generation: 0,
            smx_difficulty_tint_cache: std::collections::HashMap::new(),
            asset_manager: AssetManager::new(),
            dynamic_media: DynamicMedia::new(),
            options_song_pack_generation: deadsync_simfile::runtime_cache::song_cache_generation(),
            profile_import: crate::profile_import::Service::default(),
            profile_load: crate::profile_load::Service::default(),
            heart_rate: crate::heart_rate::Runtime::default(),
            qr_login: crate::qr_login::Service::default(),
            score_import: crate::score_import::Service::default(),
            sync_analysis: crate::sync_analysis::Service::default(),
            // Screen transitions clear the UI cache, so misses stop inserting
            // once the cache reaches its fixed footprint.
            ui_text_layout_cache: compose::TextLayoutCache::new(UI_TEXT_LAYOUT_CACHE_LIMIT),
            gameplay_text_layout_cache: compose::TextLayoutCache::new(
                GAMEPLAY_TEXT_LAYOUT_CACHE_LIMIT,
            ),
            ui_compose_scratch: compose::ComposeScratch::default(),
            gameplay_compose_scratch: compose::ComposeScratch::default(),
            actor_scratch: Vec::with_capacity(256),
            state,
            software_renderer_threads,
            gfx_debug_enabled,
        }
    }

    fn handle_action(
        &mut self,
        action: ThemeEffect,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), Box<dyn Error>> {
        let current_screen = self.state.screens.current_screen;
        let course_active = self.state.session.course_run.is_some();
        let course_has_next_stage = self
            .state
            .session
            .course_run
            .as_ref()
            .is_some_and(|course| course.next_stage_index < course.stages.len());
        let gameplay_failed = matches!(action, ThemeEffect::Navigate(CurrentScreen::Evaluation))
            && current_screen == CurrentScreen::Gameplay
            && self.current_gameplay_stage_failed();
        let plan = theme_effect_execution_plan(
            action,
            ThemeEffectRouteContext {
                current_screen,
                restart_pending: self.state.session.restart_pending,
                course_active,
                course_has_next_stage,
                gameplay_failed,
            },
        );
        if plan.clear_restart_pending {
            self.state.session.restart_pending = false;
        }

        let commands = match plan.effect {
            ThemeEffectExecution::None => Vec::new(),
            ThemeEffectExecution::Batch(effects) => {
                execute_effect_batch(effects, |effect| self.handle_action(effect, event_loop))?;
                return Ok(());
            }
            ThemeEffectExecution::Navigate(screen) => {
                self.handle_navigation_action(screen);
                Vec::new()
            }
            ThemeEffectExecution::NavigateNoFade(screen) => {
                if self.maybe_begin_gameplay_offset_prompt(
                    self.state.screens.current_screen,
                    screen,
                    true,
                ) {
                    return Ok(());
                }
                // Skip the current screen's out-transition and immediately enter `screen`,
                // letting the target screen's in-transition handle the visual change.
                if matches!(self.state.shell.transition, TransitionState::Idle) {
                    self.on_fade_complete(screen, event_loop);
                }
                return Ok(());
            }
            ThemeEffectExecution::ProcessExit(request) => self.handle_process_exit(request),
            ThemeEffectExecution::RequestScreenshot(side) => {
                self.state.shell.screenshot.request(side);
                Vec::new()
            }
            ThemeEffectExecution::RunCommands(commands) => commands,
            ThemeEffectExecution::LinkOnlineProfile(link) => {
                match link.target {
                    CurrentScreen::ArrowCloudLogin => {
                        self.state.screens.arrowcloud_login_state.active_color_index =
                            self.state.screens.menu_state.active_color_index;
                        self.state.screens.arrowcloud_login_state.target_profile =
                            Some(screens::arrowcloud_login::ProfileTarget {
                                id: link.profile_id,
                                display_name: link.display_name,
                            });
                    }
                    CurrentScreen::GrooveStatsLogin => {
                        self.state
                            .screens
                            .groovestats_login_state
                            .active_color_index = self.state.screens.menu_state.active_color_index;
                        self.state.screens.groovestats_login_state.target_profile =
                            Some(screens::groovestats_login::ProfileTarget {
                                id: link.profile_id,
                                display_name: link.display_name,
                            });
                    }
                    _ => {}
                }
                self.handle_navigation_action(link.target);
                Vec::new()
            }
            ThemeEffectExecution::WriteFsrDump { path } => {
                match self.fsr_monitor.write_debug_dump(&path) {
                    Ok(()) => {
                        info!("Wrote FSR debug dump to '{}'", path.display());
                        self.state
                            .shell
                            .interaction
                            .show_message(format!("Wrote {}", path.display()), Instant::now());
                    }
                    Err(e) => {
                        warn!("Failed to write FSR debug dump: {e}");
                        self.state
                            .shell
                            .interaction
                            .show_message(format!("FSR dump failed: {e}"), Instant::now());
                    }
                }
                Vec::new()
            }
            ThemeEffectExecution::Runtime(request) => match request {
                SimplyLoveRuntimeRequest::Profile(SimplyLoveProfileRequest::Select {
                    p1,
                    p2,
                    p1_joined,
                    p2_joined,
                    fast_switch,
                }) => {
                    let session = profile_selection_session_plan(
                        profile::get_session_play_style(),
                        p1_joined,
                        p2_joined,
                    );
                    profile::set_session_player_side(session.active_side);
                    profile::set_session_joined(session.p1_joined, session.p2_joined);
                    profile::set_session_play_style(session.play_style);
                    let profile_data = profile::set_active_profiles(p1, p2);
                    let (show_groovestats_login, show_arrowcloud_login) = if fast_switch {
                        (false, false)
                    } else {
                        let cfg = config::get();
                        (
                            crate::qr_login::should_auto_show_groovestats(
                                cfg.groovestats_qr_login_when,
                            ),
                            crate::qr_login::should_auto_show_arrowcloud(
                                cfg.arrowcloud_qr_login_when,
                            ),
                        )
                    };
                    let plan = profile_selection_plan(
                        &profile_data,
                        ProfileSelectionContext {
                            play_style: session.play_style,
                            active_side: session.active_side,
                            fast_switch,
                            current_screen: self.state.screens.current_screen,
                            show_groovestats_login,
                            show_arrowcloud_login,
                        },
                    );
                    self.state.session.combo_carry = plan.combo_carry;
                    if let Some(backend) = self.backend.as_mut() {
                        self.dynamic_media.set_profile_avatar_for_side(
                            &mut self.asset_manager,
                            backend,
                            profile_data::PlayerSide::P1,
                            profile_data[0].avatar_path.clone(),
                        );
                        self.dynamic_media.set_profile_avatar_for_side(
                            &mut self.asset_manager,
                            backend,
                            profile_data::PlayerSide::P2,
                            profile_data[1].avatar_path.clone(),
                        );
                    }

                    self.state.session.preferred_difficulty_index = plan.preferred_active;

                    if plan.refresh_select_music {
                        let current_color_index =
                            self.state.screens.select_profile_state.active_color_index;
                        self.state.screens.select_music_state.active_color_index =
                            current_color_index;
                        self.state
                            .screens
                            .select_music_state
                            .preferred_difficulty_index = plan.preferred_active;
                        self.state.screens.select_music_state.selected_steps_index =
                            plan.preferred_active;
                        self.state
                            .screens
                            .select_music_state
                            .p2_preferred_difficulty_index = plan.preferred_p2;
                        self.state
                            .screens
                            .select_music_state
                            .p2_selected_steps_index = plan.preferred_p2;
                        select_music::trigger_immediate_refresh(
                            &mut self.state.screens.select_music_state,
                        );
                    }
                    if let Some(target) = plan.navigation_target {
                        // ProfileLoad asynchronously prepares SelectMusic/SelectCourse state;
                        // avoid redundant eager init here.
                        self.handle_navigation_action(target);
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Profile(
                    SimplyLoveProfileRequest::DiscoverItgProfiles,
                ) => {
                    self.profile_import.discover();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Profile(
                    SimplyLoveProfileRequest::BrowseItgProfiles { title },
                ) => {
                    self.profile_import.browse(title);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Profile(
                    SimplyLoveProfileRequest::StartItgProfileImport { dir },
                ) => {
                    self.profile_import.start(dir);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Profile(
                    SimplyLoveProfileRequest::CancelItgProfileImport,
                ) => {
                    self.profile_import.cancel();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Sync(SimplyLoveSyncRequest::ApplySongOffset {
                    simfile_path,
                    delta_seconds,
                }) => {
                    if let Err(e) =
                        self.save_gameplay_song_offset(simfile_path.as_path(), delta_seconds)
                    {
                        warn!("Failed to save song offset sync changes: {e}");
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Sync(SimplyLoveSyncRequest::StartAnalysis {
                    owner,
                    targets,
                    emit_freq_delta,
                }) => {
                    self.sync_analysis.start(owner, targets, emit_freq_delta);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Sync(SimplyLoveSyncRequest::CancelAnalysis(owner)) => {
                    self.sync_analysis.cancel(owner);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Sync(SimplyLoveSyncRequest::ApplySongOffsetBatch {
                    changes,
                }) => {
                    if let Err(e) = self.save_song_offset_changes(&changes) {
                        warn!("Failed to save pack sync changes: {e}");
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Audio(request) => {
                    audio_requests::execute(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Graphics(request) => {
                    self.handle_graphics_change(request, event_loop)?;
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Updater(request) => {
                    updater::execute(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::ShowOverlay(mode)) => {
                    self.state.shell.set_overlay_mode(mode);
                    config::update_show_stats_mode(mode);
                    options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::MouseCursorHidden(
                    hidden,
                )) => {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(!hidden);
                    }
                    config::update_hide_mouse_cursor(hidden);
                    options::sync_hide_mouse_cursor(&mut self.state.screens.options_state, hidden);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::PersistColor(index)) => {
                    crate::smx_config::set_theme_color(index);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Advanced(request)) => {
                    config_requests::execute_advanced(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Course(request)) => {
                    config_requests::execute_course(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Gameplay(request)) => {
                    config_requests::execute_gameplay(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Lights(request)) => {
                    config_requests::execute_lights(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Machine(request)) => {
                    config_requests::execute_machine(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Mappings(request)) => {
                    crate::mappings::execute(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::NullOrDie(request)) => {
                    config_requests::execute_null_or_die(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::Online(request)) => {
                    config_requests::execute_online(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Config(SimplyLoveConfigRequest::SelectMusic(request)) => {
                    config_requests::execute_select_music(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(SimplyLoveHardwareRequest::TestLightsAuto) => {
                    test_lights::on_enter(&mut self.state.screens.test_lights_state);
                    self.lights.set_test_auto_cycle();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(SimplyLoveHardwareRequest::StepTestCabinet(
                    delta,
                )) => {
                    self.lights.step_test_cabinet(delta);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(SimplyLoveHardwareRequest::StepTestButton(
                    delta,
                )) => {
                    self.lights.step_test_button(delta);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(SimplyLoveHardwareRequest::AssignSmxPads {
                    p1_serial,
                    p2_serial,
                }) => {
                    crate::smx_config::set_smx_assignment(p1_serial, p2_serial);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(SimplyLoveHardwareRequest::SwapSmxPads) => {
                    let _ = crate::smx_config::swap_smx_assignment();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::SetSmxUnderglowTheme(enabled),
                ) => {
                    crate::smx_config::set_smx_underglow_theme(enabled);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::SetSmxUnderglowGrb(grb),
                ) => {
                    crate::smx_config::set_smx_underglow_grb(grb);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::ApplySmxPadPreset { pad, name },
                ) => {
                    if crate::smx_config::apply_smx_pad_preset(pad, &name) {
                        self.state
                            .screens
                            .select_music_state
                            .smx_pad_profile_events
                            .push(select_music::SmxPadProfileEvent::Applied {
                                pad,
                                preset: true,
                                name,
                            });
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::ApplySmxPadConfig {
                        pad,
                        profile_id,
                        name,
                    },
                ) => {
                    if crate::smx_config::apply_smx_saved_pad_config(pad, &profile_id, &name) {
                        self.state
                            .screens
                            .select_music_state
                            .smx_pad_profile_events
                            .push(select_music::SmxPadProfileEvent::Applied {
                                pad,
                                preset: false,
                                name,
                            });
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::CaptureSmxPadConfig {
                        pad,
                        profile_id,
                        name,
                        set_default,
                        overwrite,
                    },
                ) => {
                    if crate::smx_config::capture_smx_pad_config(
                        pad,
                        &profile_id,
                        &name,
                        set_default,
                    ) {
                        self.state
                            .screens
                            .select_music_state
                            .smx_pad_profile_events
                            .push(select_music::SmxPadProfileEvent::Captured {
                                pad,
                                profile_id,
                                name,
                                overwrite,
                            });
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::SetSmxPlayerLights(colors),
                ) => {
                    deadsync_smx::set_player_lights(colors);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Hardware(
                    SimplyLoveHardwareRequest::ReenableSmxAutoLights,
                ) => {
                    deadsync_smx::reenable_auto_lights();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Platform(request) => {
                    execute_platform_request(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::Reinitialize) => {
                    deadsync_online::runtime::init();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::Lobby(request)) => {
                    match request {
                        SimplyLoveLobbyRequest::Search => {
                            deadsync_online::lobbies::runtime_search_lobbies_default();
                        }
                        SimplyLoveLobbyRequest::Create { password } => {
                            deadsync_online::lobbies::runtime_create_lobby_with_password_default(
                                &password,
                            );
                        }
                        SimplyLoveLobbyRequest::Join { code, password } => {
                            deadsync_online::lobbies::runtime_join_lobby_with_password_default(
                                &code, &password,
                            );
                        }
                        SimplyLoveLobbyRequest::Leave => {
                            deadsync_online::lobbies::runtime_leave_lobby_default();
                        }
                        SimplyLoveLobbyRequest::SelectSong(song) => {
                            deadsync_online::lobbies::runtime_select_song_default(song);
                        }
                        SimplyLoveLobbyRequest::UpdateMachineState { screen_name, ready } => {
                            deadsync_online::lobbies::runtime_update_machine_state_default(
                                screen_name,
                                ready,
                            );
                        }
                        SimplyLoveLobbyRequest::UpdateMachineStats {
                            screen_name,
                            p1_ready,
                            p2_ready,
                            p1_stats,
                            p2_stats,
                        } => {
                            deadsync_online::lobbies::runtime_update_machine_state_sides_with_stats_default(
                                screen_name,
                                p1_ready,
                                p2_ready,
                                p1_stats,
                                p2_stats,
                            );
                        }
                        SimplyLoveLobbyRequest::Disconnect => {
                            deadsync_online::lobbies::runtime_disconnect();
                        }
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::StartQrLogin(
                    request,
                )) => {
                    self.qr_login.start(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::CancelQrLogin(
                    service,
                )) => {
                    self.qr_login.cancel(service);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::StartScoreImport(
                    request,
                )) => {
                    self.score_import.start(request);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::CancelScoreImport) => {
                    self.score_import.cancel();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(
                    SimplyLoveOnlineRequest::RefreshPlayerLeaderboard {
                        chart_hash,
                        side,
                        max_entries,
                    },
                ) => {
                    let _ = scores::refresh_player_leaderboards_for_side(
                        &chart_hash,
                        side,
                        max_entries,
                    );
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(SimplyLoveOnlineRequest::RefreshSrpgShop {
                    side,
                }) => {
                    let profile = deadsync_profile::runtime_profile_for_side(side);
                    let password = profile.groovestats_password.expose().to_owned();
                    deadsync_online::srpg_shop::runtime_refresh(
                        profile.groovestats_username,
                        password,
                    );
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(
                    SimplyLoveOnlineRequest::DownloadSrpgShopUnlock { shop_id, name, url },
                ) => {
                    let folder = deadsync_config::runtime::get().srpg_shop_folder;
                    let pack_name = deadsync_online::srpg_shop::download_folder(shop_id, folder);
                    deadsync_online::runtime::forget_cached_unlock(&url, pack_name);
                    deadsync_online::runtime::queue_event_unlock_download(&url, &name, pack_name);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(
                    SimplyLoveOnlineRequest::EnsureStepManiaOnlineCatalog,
                ) => {
                    deadsync_online::stepmaniaonline::runtime_ensure_catalog();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(
                    SimplyLoveOnlineRequest::RefreshStepManiaOnlineCatalog,
                ) => {
                    deadsync_online::stepmaniaonline::runtime_refresh_catalog();
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(
                    SimplyLoveOnlineRequest::DownloadStepManiaOnlinePack { pack_id },
                ) => {
                    if let Err(error) = deadsync_online::stepmaniaonline::runtime_queue_download(
                        pack_id,
                        dirs::app_dirs().songs_dir(),
                    ) {
                        warn!("Could not queue StepManiaOnline pack {pack_id}: {error}");
                    }
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Online(
                    SimplyLoveOnlineRequest::PurchaseSrpgShopItem {
                        shop_id,
                        item_id,
                        type_id,
                    },
                ) => {
                    deadsync_online::srpg_shop::runtime_purchase(shop_id, item_id, type_id);
                    Vec::new()
                }
                SimplyLoveRuntimeRequest::Media(_)
                | SimplyLoveRuntimeRequest::Online(_)
                | SimplyLoveRuntimeRequest::Debug(_) => Vec::new(),
            },
        };
        self.run_commands(commands, event_loop)
    }

    fn save_song_offset_changes(
        &mut self,
        changes: &[sync_offset::SongOffsetSyncChange],
    ) -> Result<(), String> {
        let summary = sync_offset::save_song_offset_changes(
            changes,
            config::song_path_is_writable,
            |simfile_path| {
                let updated_song = song_loading::reload_song_in_cache(simfile_path)?;
                if let Some(po_state) = self.state.screens.player_options_state.as_mut() {
                    let _ = deadsync_simfile::runtime_cache::replace_song_arc_if_same_simfile(
                        &mut po_state.song,
                        &updated_song,
                    );
                }
                Ok(())
            },
        )?;

        if summary.saved_files == 0 {
            if let Some(path) = summary.first_skipped_path {
                return Err(format!(
                    "Song offset sync changes target read-only AdditionalSongFoldersReadOnly path '{}'",
                    path.display()
                ));
            }
            return Ok(());
        }

        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);
        if summary.skipped_read_only > 0
            && let Some(path) = summary.first_skipped_path.as_deref()
        {
            warn!(
                "Skipped {} song offset sync change(s) under read-only AdditionalSongFoldersReadOnly roots; first skipped '{}'.",
                summary.skipped_read_only,
                path.display()
            );
        }
        if summary.saved_files == 1 {
            if let Some(path) = summary.first_saved_path.as_deref() {
                info!(
                    "Saved song offset sync changes to '{}' (updated {} #OFFSET tags; refreshed song cache).",
                    path.display(),
                    summary.changed_tags_total
                );
            }
        } else {
            info!(
                "Saved pack sync changes to {} simfiles (updated {} #OFFSET tags; refreshed song cache).",
                summary.saved_files, summary.changed_tags_total
            );
        }
        Ok(())
    }

    fn save_gameplay_song_offset(&mut self, simfile_path: &Path, delta: f32) -> Result<(), String> {
        self.save_song_offset_changes(&[sync_offset::SongOffsetSyncChange {
            simfile_path: simfile_path.to_path_buf(),
            delta_seconds: delta,
        }])
    }

    fn maybe_begin_gameplay_offset_prompt(
        &mut self,
        from: CurrentScreen,
        target: CurrentScreen,
        navigate_no_fade: bool,
    ) -> bool {
        if self.state.gameplay_offset_save_prompt.is_some() {
            return true;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return false;
        };
        if !gameplay_offset_prompt_needed(
            from,
            self.state.session.course_run.is_some(),
            gameplay_offset_snapshot(gs),
        ) {
            return false;
        }
        self.state.gameplay_offset_save_prompt =
            Some(GameplayOffsetSavePrompt::new(target, navigate_no_fade));
        true
    }

    fn finalize_gameplay_offset_prompt(
        &mut self,
        save_changes: bool,
        event_loop: &ActiveEventLoop,
    ) {
        let Some(prompt) = self.state.gameplay_offset_save_prompt.take() else {
            return;
        };
        if save_changes {
            let mut song_offset_change: Option<(PathBuf, f32)> = None;
            if let Some(gs) = self.state.screens.gameplay_state.as_ref() {
                let targets = gameplay_offset_save_targets(gameplay_offset_snapshot(gs));
                if let Some(global_offset) = targets.global_seconds {
                    config::update_global_offset(global_offset);
                }
                if let Some(delta) = targets.song_delta_seconds {
                    song_offset_change = Some((gs.song().simfile_path.clone(), delta));
                }
            }
            if let Some((simfile_path, delta)) = song_offset_change
                && let Err(e) = self.save_gameplay_song_offset(simfile_path.as_path(), delta)
            {
                warn!("Failed to save song offset sync changes: {e}");
            }
        }
        if prompt.navigate_no_fade {
            if matches!(self.state.shell.transition, TransitionState::Idle) {
                self.on_fade_complete(prompt.target, event_loop);
            }
            return;
        }
        self.handle_navigation_action_after_prompt(prompt.target);
    }

    fn route_gameplay_offset_prompt_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        ev: &InputEvent,
    ) -> bool {
        if self.state.screens.current_screen != CurrentScreen::Gameplay
            || self.state.gameplay_offset_save_prompt.is_none()
        {
            return false;
        }
        let input = route_offset_prompt_input(
            self.state
                .gameplay_offset_save_prompt
                .as_mut()
                .expect("prompt presence checked above"),
            ev.pressed,
            ev.action,
            self.state.play_input_policy.only_dedicated_menu_buttons,
        );
        match input {
            OffsetPromptInput::Consumed => {}
            OffsetPromptInput::ChoiceChanged => {
                deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
            }
            OffsetPromptInput::Decide(save_changes) => {
                deadsync_audio_stream::play_sfx("assets/sounds/start.ogg");
                self.finalize_gameplay_offset_prompt(save_changes, event_loop);
            }
        }
        true
    }

    fn update_combo_carry_from_gameplay(&mut self, gs: &gameplay::State) {
        let num_players = gs.num_players().min(MAX_PLAYERS);
        let player_combos =
            std::array::from_fn(|idx| (idx < num_players).then(|| gs.players()[idx].combo));
        persist_gameplay_combo_carry(&mut self.state.session, gs.autoplay_used(), player_combos);
    }

    fn start_course_run_from_selected(&mut self) -> bool {
        let Some(selection) =
            select_course::selected_course_plan(&self.state.screens.select_course_state)
        else {
            warn!("Unable to start course run: selected course has no playable stages.");
            return false;
        };
        let course_path = selection.path.clone();
        let course_difficulty_name = selection.course_difficulty_name.clone();
        let Some(course_run) = build_course_run_from_selection(
            selection,
            profile::get_session_play_style().chart_type(),
        ) else {
            warn!("Unable to start course run: failed to resolve course stages.");
            return false;
        };
        self.state.session.last_course_wheel_path = Some(course_path.clone());
        self.state.session.last_course_wheel_difficulty_name = Some(course_difficulty_name.clone());
        record_last_played_course(course_path.as_path(), course_difficulty_name.as_str());
        self.state.session.course_run = Some(course_run);
        self.state.session.course_stage_eval_pages.clear();
        self.state.session.course_eval_pages.clear();
        self.state.session.course_eval_page_index = 0;
        true
    }

    fn prepare_player_options_for_course_stage(
        &mut self,
        color_index: i32,
        prewarm_noteskin_catalog: bool,
    ) -> bool {
        let Some(course_run) = self.state.session.course_run.as_ref() else {
            return false;
        };
        let Some(stage) = course_run.stages.get(course_run.next_stage_index) else {
            return false;
        };
        let init = if prewarm_noteskin_catalog {
            player_options::init
        } else {
            player_options::init_for_gameplay
        };
        self.state.screens.player_options_state = Some(init(
            stage.song.clone(),
            stage.steps_index,
            stage.preferred_difficulty_index,
            color_index,
            CurrentScreen::SelectCourse,
            Some(player_options::FixedStepchart {
                label: course_run.course_stepchart_label.clone(),
            }),
            noteskin_catalog_view(),
            crate::smx_config::smx_gif_catalog_view(),
            crate::heart_rate::devices_view(),
        ));
        true
    }

    fn prepare_restart_player_options(
        &mut self,
        song: Arc<deadsync_chart::SongData>,
        chart_hashes: [&str; MAX_PLAYERS],
        music_rate: f32,
        scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
        active_color_index: i32,
        return_screen: CurrentScreen,
    ) -> bool {
        let session = profile::get_session_snapshot();
        let play_style = session.play_style;
        let player_side = session.player_side;
        let target_chart_type = play_style.chart_type();
        let fallback_steps = self.state.session.preferred_difficulty_index;
        let chart_steps_index = restart_chart_steps(
            song.as_ref(),
            chart_hashes,
            target_chart_type,
            fallback_steps,
            play_style,
            player_side,
        );

        let mut po_state = player_options::init_for_gameplay(
            song,
            chart_steps_index,
            chart_steps_index,
            active_color_index,
            return_screen,
            None,
            noteskin_catalog_view(),
            crate::smx_config::smx_gif_catalog_view(),
            crate::heart_rate::devices_view(),
        );
        po_state.music_rate = music_rate;
        po_state.speed_mod =
            std::array::from_fn(|i| player_options::SpeedMod::from(scroll_speed[i]));
        player_options::sync_speed_mod_type_rows(&mut po_state);
        self.state.screens.player_options_state = Some(po_state);
        true
    }

    fn prepare_player_options_for_gameplay_restart(&mut self) -> bool {
        let eval_payload =
            restart_payload_from_eval(&self.state.screens.evaluation_state.score_info);
        match gameplay_restart_prepare_source(
            self.state.screens.current_screen,
            self.state.screens.gameplay_state.is_some(),
            eval_payload.is_some(),
        ) {
            RestartPrepareSource::Gameplay => {
                let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
                    return false;
                };
                let song = gs.song_arc();
                let chart_hashes = [
                    gs.charts()[0].short_hash.clone(),
                    gs.charts()[1].short_hash.clone(),
                ];
                let music_rate = gs.music_rate();
                let scroll_speed = [gs.scroll_speed_for_player(0), gs.scroll_speed_for_player(1)];
                let active_color_index = gs.active_color_index();
                self.prepare_restart_player_options(
                    song,
                    [chart_hashes[0].as_str(), chart_hashes[1].as_str()],
                    music_rate,
                    scroll_speed,
                    active_color_index,
                    CurrentScreen::Gameplay,
                )
            }
            RestartPrepareSource::Evaluation => {
                let Some(payload) = eval_payload else {
                    return false;
                };
                let active_color_index = self.state.screens.evaluation_state.active_color_index;
                self.prepare_restart_player_options(
                    payload.song,
                    [
                        payload.chart_hashes[0].as_str(),
                        payload.chart_hashes[1].as_str(),
                    ],
                    payload.music_rate,
                    payload.scroll_speed,
                    active_color_index,
                    CurrentScreen::Gameplay,
                )
            }
            RestartPrepareSource::Unavailable => false,
        }
    }

    fn prepare_player_options_for_practice_from_eval(&mut self) -> bool {
        let eval_payload =
            restart_payload_from_eval(&self.state.screens.evaluation_state.score_info);
        match practice_restart_prepare_source(
            self.state.screens.current_screen,
            eval_payload.is_some(),
        ) {
            RestartPrepareSource::Evaluation => {
                let Some(payload) = eval_payload else {
                    return false;
                };
                let active_color_index = self.state.screens.evaluation_state.active_color_index;
                self.prepare_restart_player_options(
                    payload.song,
                    [
                        payload.chart_hashes[0].as_str(),
                        payload.chart_hashes[1].as_str(),
                    ],
                    payload.music_rate,
                    payload.scroll_speed,
                    active_color_index,
                    CurrentScreen::Practice,
                )
            }
            RestartPrepareSource::Gameplay | RestartPrepareSource::Unavailable => false,
        }
    }

    fn try_gameplay_restart(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        let restartable = self.prepare_player_options_for_gameplay_restart();
        match gameplay_restart_route(
            self.state.screens.current_screen,
            restartable,
            self.state.screens.gameplay_state.is_some(),
        ) {
            GameplayRestartRoute::MissingState => {
                log::warn!("Ignored {label} restart: no restartable stage state.");
                return false;
            }
            GameplayRestartRoute::FastGameplayExit => {
                // SL/zmod parity: if we're already in Gameplay, run the fast Cancel
                // exit (~0.5s) instead of the full ~1.5s gameplay out-transition.
                // The Cancel navigation is intercepted in `handle_action` and
                // redirected back to Gameplay, which uses a shortened in-transition.
                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                    let already_exiting = gs.exit_transition_active();
                    gs.begin_restart_exit();
                    crate::gameplay_runtime::drain(gs);
                    if let Some(plan) = fast_gameplay_restart_plan(
                        self.state.session.gameplay_restart_count,
                        already_exiting,
                        gs.exit_transition_active(),
                    ) {
                        self.state.session.gameplay_restart_count = plan.restart_count;
                        self.state.session.restart_pending = plan.restart_pending;
                    }
                }
                return true;
            }
            GameplayRestartRoute::Navigate(target) => {
                if let Err(e) = self.handle_action(ThemeEffect::Navigate(target), event_loop) {
                    log::error!("Failed to restart Gameplay with {label}: {e}");
                } else {
                    self.state.session.gameplay_restart_count =
                        self.state.session.gameplay_restart_count.saturating_add(1);
                }
            }
        }
        true
    }

    fn try_gameplay_reload(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        let eval_payload =
            restart_payload_from_eval(&self.state.screens.evaluation_state.score_info);
        let simfile_path = match gameplay_reload_source(
            self.state.screens.current_screen,
            self.state.screens.gameplay_state.is_some(),
            eval_payload.is_some(),
        ) {
            GameplayReloadSource::Gameplay => self
                .state
                .screens
                .gameplay_state
                .as_ref()
                .map(|gs| gs.song().simfile_path.clone()),
            GameplayReloadSource::Evaluation => {
                eval_payload.map(|payload| payload.song.simfile_path.clone())
            }
            GameplayReloadSource::Unavailable => None,
        };
        let Some(simfile_path) = simfile_path else {
            log::warn!("Ignored {label} reload: no restartable stage state.");
            return false;
        };

        let updated_song = match song_loading::reload_song_in_cache(simfile_path.as_path()) {
            Ok(song) => song,
            Err(e) => {
                log::warn!(
                    "Ignored {label} reload for '{}': {e}",
                    simfile_path.display()
                );
                return false;
            }
        };
        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);

        if !self.try_gameplay_restart(event_loop, label) {
            return false;
        }

        if let Some(po_state) = self.state.screens.player_options_state.as_mut() {
            let _ = deadsync_simfile::runtime_cache::replace_song_arc_if_same_simfile(
                &mut po_state.song,
                &updated_song,
            );
        }
        true
    }

    /// SL-zmod parity (`BGAnimations/ScreenEvaluation common/Shared/RestartHandler.lua`):
    /// Ctrl+P on the Evaluation screen re-enters the just-played chart in
    /// Practice mode. Mirrors `try_gameplay_restart`, but routes to
    /// `CurrentScreen::Practice` and does not touch
    /// `gameplay_restart_count` / `restart_pending` (those are gameplay-only).
    fn try_practice_from_eval(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        let eval_payload_available =
            restart_payload_from_eval(&self.state.screens.evaluation_state.score_info).is_some();
        if !practice_from_eval_allowed(self.state.screens.current_screen, eval_payload_available) {
            return false;
        }
        if !self.prepare_player_options_for_practice_from_eval() {
            log::warn!("Ignored {label} practice: no replayable evaluation payload.");
            return false;
        }
        if let Err(e) =
            self.handle_action(ThemeEffect::Navigate(CurrentScreen::Practice), event_loop)
        {
            log::error!("Failed to enter Practice with {label}: {e}");
            return false;
        }
        true
    }

    fn try_practice_reload(&mut self, event_loop: &ActiveEventLoop, label: &str) -> bool {
        if !practice_reload_allowed(
            self.state.screens.current_screen,
            self.state.screens.practice_state.is_some(),
        ) {
            return false;
        }
        let Some((
            simfile_path,
            old_song,
            music_rate,
            scroll_speed,
            active_color_index,
            old_hashes,
            old_difficulties,
        )) = self.state.screens.practice_state.as_ref().map(|ps| {
            let gs = &ps.gameplay;
            (
                gs.song().simfile_path.clone(),
                gs.song_arc(),
                gs.music_rate(),
                [gs.scroll_speed_for_player(0), gs.scroll_speed_for_player(1)],
                gs.active_color_index(),
                [
                    gs.charts()[0].short_hash.clone(),
                    gs.charts()[1].short_hash.clone(),
                ],
                [
                    gs.charts()[0].difficulty.clone(),
                    gs.charts()[1].difficulty.clone(),
                ],
            )
        })
        else {
            log::warn!("Ignored {label} reload: no practice stage state.");
            return false;
        };

        let updated_song = match song_loading::reload_song_in_cache(simfile_path.as_path()) {
            Ok(song) => song,
            Err(e) => {
                log::warn!(
                    "Ignored {label} reload for '{}': {e}",
                    simfile_path.display()
                );
                return false;
            }
        };
        select_music::refresh_from_song_cache(&mut self.state.screens.select_music_state);

        let target_chart_type = profile::get_session_play_style().chart_type();
        let new_hashes = deadsync_simfile::runtime_cache::reloaded_chart_hashes_for_restart(
            old_song.as_ref(),
            updated_song.as_ref(),
            target_chart_type,
            [old_hashes[0].as_str(), old_hashes[1].as_str()],
            [old_difficulties[0].as_str(), old_difficulties[1].as_str()],
        );

        if !self.prepare_restart_player_options(
            updated_song,
            [new_hashes[0].as_str(), new_hashes[1].as_str()],
            music_rate,
            scroll_speed,
            active_color_index,
            CurrentScreen::Practice,
        ) {
            log::warn!("Ignored {label} reload: could not rebuild practice options.");
            return false;
        }
        if let Err(e) =
            self.handle_action(ThemeEffect::Navigate(CurrentScreen::Practice), event_loop)
        {
            log::error!("Failed to reload Practice with {label}: {e}");
            return false;
        }
        true
    }

    fn current_gameplay_stage_failed(&self) -> bool {
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return false;
        };
        (0..gs.num_players().min(MAX_PLAYERS)).any(|player_idx| {
            let p = &gs.players()[player_idx];
            p.is_failing || p.life <= 0.0 || p.fail_time.is_some()
        })
    }

    fn append_stage_results_from_eval(
        &mut self,
        eval_state: &evaluation::State,
    ) -> Option<stage_stats::StageSummary> {
        let in_course_run = self.state.session.course_run.is_some();
        let session = profile::get_session_snapshot();
        let stage_summary = stage_summary_from_score_info(
            &eval_state.score_info,
            eval_state.stage_duration_seconds,
            session.play_style,
            session.player_side,
        );
        if let Some(stage) = stage_summary.as_ref() {
            for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                if let Some(p) = stage
                    .players
                    .get(profile_data::player_side_index(side))
                    .and_then(|p| p.as_ref())
                {
                    profile::add_stage_calories_for_side(side, p.calories_burned);
                }
            }
        }
        let course_page = if in_course_run {
            let mut stage_page = eval_state.clone();
            stage_page.return_to_course = true;
            stage_page.auto_advance_seconds = None;
            Some(stage_page)
        } else {
            None
        };
        self.state
            .session
            .record_stage_result(stage_summary.clone(), course_page);
        stage_summary
    }

    fn finalize_entered_evaluation(&mut self) {
        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media
                .clear_gameplay_backgrounds(&mut self.asset_manager, backend);
        }

        let color_idx = self.state.screens.evaluation_state.active_color_index;
        let eval_snapshot = self.state.screens.evaluation_state.clone();
        let _ = self.append_stage_results_from_eval(&eval_snapshot);
        self.state.screens.evaluation_state.return_to_course =
            self.state.session.course_run.is_some();
        self.state.screens.evaluation_state.auto_advance_seconds = None;

        // Pass / Fail SFX (zmod parity, issue #375). Based on the per-stage
        // result that was just captured into `eval_snapshot`; even when that
        // is immediately replaced by a course summary, this is the cue tied to
        // the player's actual exit from gameplay.
        let failed = screens::evaluation::all_joined_players_failed(&eval_snapshot);
        if visual_styles::srpg10_active() {
            let sfx = if failed {
                visual_styles::SRPG10_EVAL_FAILED_SFX
            } else {
                visual_styles::SRPG10_EVAL_PASSED_SFX
            };
            deadsync_audio_stream::play_screen_sfx(sfx);
        } else {
            let folder = if failed {
                "assets/sounds/evaluation_fail"
            } else {
                "assets/sounds/evaluation_pass"
            };
            deadsync_assets::audio_folder::play_random_screen_sfx(folder);
        }

        if let Some((course_run, per_song_pages)) = self.state.session.take_final_course(failed) {
            let score_hash = course_run.score_hash.clone();
            let course_graph_stages = build_course_graph_stages(
                &course_run,
                profile::get_session_play_style().chart_type(),
            );
            let course_summary = build_course_summary_stage(&course_run);

            if let Some(course_stage) = course_summary {
                for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
                    if let Some(player) =
                        course_stage.players[profile_data::player_side_index(side)].as_ref()
                    {
                        scores::save_local_summary_score_for_side(
                            score_hash.as_str(),
                            side,
                            course_stage.music_rate,
                            player,
                        );
                    }
                }
                self.state.session.played_stages.push(course_stage.clone());

                let gameplay_elapsed =
                    stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);
                let session_elapsed = self.state.screens.evaluation_state.session_elapsed;
                let screen_elapsed = self.state.screens.evaluation_state.screen_elapsed;
                let mut course_page = build_course_summary_eval_state(
                    &course_stage,
                    &course_graph_stages,
                    color_idx,
                    session_elapsed,
                    gameplay_elapsed,
                );
                apply_course_summary_column_judgments(&mut course_page, &per_song_pages);
                course_page.screen_elapsed = screen_elapsed;
                self.state.screens.evaluation_state = course_page.clone();

                let mut pages = Vec::with_capacity(per_song_pages.len().saturating_add(1));
                pages.push(course_page);
                for mut page in per_song_pages {
                    page.return_to_course = true;
                    page.auto_advance_seconds = None;
                    page.screen_elapsed = screen_elapsed;
                    page.session_elapsed = session_elapsed;
                    page.gameplay_elapsed = gameplay_elapsed;
                    pages.push(page);
                }
                self.state.session.replace_course_eval_pages(pages);
            }
        } else {
            self.state.session.clear_course_eval_pages();
        }

        self.state.screens.evaluation_state.gameplay_elapsed =
            stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);
    }

    fn post_select_display_stages(
        &self,
        show_course_individual_scores: bool,
    ) -> Cow<'_, [stage_stats::StageSummary]> {
        post_select_display_stages(
            &self.state.session.played_stages,
            &self.state.session.course_individual_stage_indices,
            show_course_individual_scores,
        )
    }

    fn post_select_display_stage_count(&self, show_course_individual_scores: bool) -> usize {
        post_select_display_stage_count(
            self.state.session.played_stages.len(),
            &self.state.session.course_individual_stage_indices,
            show_course_individual_scores,
        )
    }

    fn step_course_eval_page(&mut self, delta: i32) {
        let Some(mut page) = self.state.session.step_course_eval_page(delta) else {
            return;
        };
        page.screen_elapsed = self.state.screens.evaluation_state.screen_elapsed;
        page.session_elapsed = self.state.screens.evaluation_state.session_elapsed;
        page.gameplay_elapsed = self.state.screens.evaluation_state.gameplay_elapsed;
        page.return_to_course = true;
        page.auto_advance_seconds = None;
        self.state.screens.evaluation_state = page;
        let view = Self::evaluation_runtime_view(&self.state.screens.evaluation_state);
        evaluation::sync_runtime_view(&mut self.state.screens.evaluation_state, view);
        deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
    }

    fn apply_select_music_join(&mut self, join_side: profile_data::PlayerSide) {
        let session = profile::get_session_snapshot();
        let play_style = session.play_style;
        let p1_pref =
            profile::preferred_difficulty_for_side(profile_data::PlayerSide::P1, play_style);
        let p2_pref =
            profile::preferred_difficulty_for_side(profile_data::PlayerSide::P2, play_style);

        let side = session.player_side;
        let sm = &mut self.state.screens.select_music_state;
        let plan = select_music_join_plan(SelectMusicJoinContext {
            active_side: side,
            join_side,
            selected_steps: sm.selected_steps_index,
            preferred_difficulty: sm.preferred_difficulty_index,
            p1_profile_preferred: p1_pref,
            p2_profile_preferred: p2_pref,
        });
        sm.selected_steps_index = plan.selected_steps;
        sm.preferred_difficulty_index = plan.preferred_difficulty;
        sm.p2_selected_steps_index = plan.p2_selected_steps;
        sm.p2_preferred_difficulty_index = plan.p2_preferred_difficulty;
        select_music::select_preferred_steps(sm);

        self.state.session.preferred_difficulty_index = sm.preferred_difficulty_index;
        select_music::trigger_immediate_refresh(sm);
        select_music::prime_displayed_chart_data(sm);
    }

    fn try_handle_late_join(&mut self, ev: &InputEvent) -> Option<ThemeEffect> {
        let screen = self.state.screens.current_screen;
        let screen_allows_join = match screen {
            CurrentScreen::SelectMusic => {
                screens::select_music::allows_late_join(&self.state.screens.select_music_state)
            }
            CurrentScreen::SelectCourse => {
                screens::select_course::allows_late_join(&self.state.screens.select_course_state)
            }
            CurrentScreen::SelectColor
            | CurrentScreen::SelectStyle
            | CurrentScreen::SelectPlayMode => true,
            _ => false,
        };
        if !screen_allows_join
            || !ev.pressed
            || !matches!(ev.action, VirtualAction::p1_start | VirtualAction::p2_start)
        {
            return None;
        }
        let session = profile::get_session_snapshot();
        let joined = [
            session.side_joined(profile_data::PlayerSide::P1),
            session.side_joined(profile_data::PlayerSide::P2),
        ];
        let Some(join_side) = late_join_side(
            ev.pressed,
            ev.action,
            LateJoinContext {
                screen,
                screen_allows_join,
                play_style: session.play_style,
                joined,
            },
        ) else {
            return None;
        };

        profile::set_session_joined(true, true);
        profile::set_session_play_style(profile_data::PlayStyle::Versus);
        let show_select_profile = config::get().machine_show_select_profile;
        let join_profile = if show_select_profile {
            profile_data::ActiveProfile::Guest
        } else {
            profile::get_default_profile_for_side(join_side)
        };
        let joined_profile = profile::set_active_profile_for_side(join_side, join_profile);
        self.state.session.combo_carry[profile_data::player_side_index(join_side)] =
            joined_profile.current_combo;
        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media.set_profile_avatar_for_side(
                &mut self.asset_manager,
                backend,
                join_side,
                joined_profile.avatar_path.clone(),
            );
        }

        if screen == CurrentScreen::SelectStyle {
            select_style::set_selected_index(&mut self.state.screens.select_style_state, 1);
        }
        let mut pending = ThemeEffect::None;
        if screen == CurrentScreen::SelectMusic {
            self.apply_select_music_join(join_side);
            // Per Simply-Love-SM5#741: when the Select Profile screen is on,
            // prompt the late-joining player with the profile-select widget
            // instead of silently leaving them as Guest.
            if show_select_profile {
                pending = screens::select_music::open_late_join_profile_overlay(
                    &mut self.state.screens.select_music_state,
                    join_side,
                );
            }
        }

        let start = ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(
            "assets/sounds/start.ogg".to_owned(),
        )));
        Some(match pending {
            ThemeEffect::None => start,
            pending => ThemeEffect::Batch(vec![pending, start]),
        })
    }

    fn route_operator_menu_button(&mut self, ev: &InputEvent) -> bool {
        match operator_menu_button_route(self.state.screens.current_screen, ev.pressed, ev.action) {
            OperatorMenuButtonRoute::Ignore => return false,
            OperatorMenuButtonRoute::ConsumeLocked => return true,
            OperatorMenuButtonRoute::NavigateOptions => {}
        }

        info!("{SERVICE_SWITCH_PRESSED}");
        self.state
            .shell
            .interaction
            .show_message(SERVICE_SWITCH_PRESSED.to_string(), Instant::now());
        self.state.session = reset_operator_profile_session();
        self.state.gameplay_offset_save_prompt = None;
        self.handle_navigation_action_after_prompt(CurrentScreen::Options);
        true
    }

    fn refresh_gameplay_background_path(
        state: &mut gameplay::State,
        show_video_backgrounds: bool,
    ) -> Option<PathBuf> {
        let path = state
            .song()
            .gameplay_background_path_for_changes(
                &state.background_changes,
                state.next_background_change_ix,
                show_video_backgrounds,
            )
            .cloned();
        state.current_background_key = path.as_deref().map(deadsync_assets::media_path_key);
        state.current_background_path = path.clone();
        state.background_allow_video = show_video_backgrounds;
        state.background_path_dirty = false;
        path
    }

    fn active_gameplay_background_change(
        state: &gameplay::State,
    ) -> Option<&deadsync_chart::SongBackgroundChange> {
        state
            .next_background_change_ix
            .checked_sub(1)
            .and_then(|ix| state.background_changes.get(ix))
    }

    fn sync_gameplay_background(&mut self, show_video_backgrounds: bool) {
        if !matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }
        let (desired_path, desired_key, gameplay_time_sec, background_rate) = {
            let gs = match self.state.screens.current_screen {
                CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_mut(),
                CurrentScreen::Practice => self
                    .state
                    .screens
                    .practice_state
                    .as_mut()
                    .map(|state| &mut state.gameplay),
                _ => None,
            };
            let Some(gs) = gs else {
                return;
            };
            let old_path_key = gs.current_background_key.clone();
            let old_texture_key = gs.background_texture_key.clone();
            let had_pending_background_change = gs.background_path_dirty;
            let mut background_changed = false;
            while let Some(change) = gs.background_changes.get(gs.next_background_change_ix) {
                if gs.current_beat() < change.start_beat {
                    break;
                }
                gs.next_background_change_ix += 1;
                background_changed = true;
            }
            if background_changed {
                gs.background_path_dirty = true;
            }
            if gs.background_path_dirty || gs.background_allow_video != show_video_backgrounds {
                Self::refresh_gameplay_background_path(gs, show_video_backgrounds);
            }
            if (background_changed || had_pending_background_change)
                && old_path_key != gs.current_background_key
            {
                let transition = Self::active_gameplay_background_change(gs)
                    .map(|change| change.transition.clone())
                    .unwrap_or_default();
                if transition.is_empty() || &*old_texture_key == "__black" {
                    gs.previous_background_texture_key = None;
                    gs.background_transition.clear();
                } else {
                    gs.previous_background_texture_key = Some(old_texture_key);
                    gs.background_transition = transition;
                    gs.background_transition_start_time =
                        deadsync_core::song_time::song_time_ns_to_seconds(
                            gs.current_music_time_ns(),
                        );
                }
            }
            (
                gs.current_background_path.clone(),
                gs.current_background_key.clone(),
                deadsync_core::song_time::song_time_ns_to_seconds(gs.current_music_time_ns()),
                Self::active_gameplay_background_change(gs)
                    .map(|change| change.rate)
                    .unwrap_or(1.0),
            )
        };

        let next_key = self.backend.as_mut().and_then(|backend| {
            self.dynamic_media.sync_gameplay_background(
                &mut self.asset_manager,
                backend,
                desired_path.as_deref(),
                desired_key.as_deref(),
                show_video_backgrounds,
                gameplay_time_sec,
                background_rate,
            )
        });
        if let Some(key) = next_key {
            let key = Arc::<str>::from(key);
            match self.state.screens.current_screen {
                CurrentScreen::Gameplay => {
                    if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                        gs.background_texture_key = key;
                    }
                }
                CurrentScreen::Practice => {
                    if let Some(ps) = self.state.screens.practice_state.as_mut() {
                        ps.gameplay.background_texture_key = key;
                    }
                }
                _ => {}
            }
        }
        let gs = match self.state.screens.current_screen {
            CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_ref(),
            CurrentScreen::Practice => self
                .state
                .screens
                .practice_state
                .as_ref()
                .map(|state| &state.gameplay),
            _ => None,
        };
        if let (Some(backend), Some(gs)) = (self.backend.as_mut(), gs) {
            let overlay_video_paths = gameplay_overlay_video_paths(
                gs.song_lua_visuals(),
                gs.song()
                    .active_foreground_path(gs.current_beat())
                    .map(|path| path.as_path()),
            );
            self.dynamic_media.sync_active_song_lua_videos(
                &mut self.asset_manager,
                backend,
                &overlay_video_paths,
            );
        }
    }

    fn sync_theme_background_video(&mut self, ui_time_sec: f32, config: &config::Config) {
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            screens::components::shared::visual_style_bg::set_srpg_background_key(None);
            return;
        }

        let path = (config.visual_style.is_srpg() && config.show_video_backgrounds)
            .then(visual_styles::shared_background_video_asset_path)
            .flatten()
            .map(|path| dirs::app_dirs().resolve_asset_path(path));

        let Some(backend) = self.backend.as_mut() else {
            screens::components::shared::visual_style_bg::set_srpg_background_key(None);
            return;
        };

        let key = self.dynamic_media.set_background(
            &mut self.asset_manager,
            backend,
            path,
            ui_time_sec,
            config.show_video_backgrounds,
        );
        let srpg_key = if key == "__black" { None } else { Some(key) };
        screens::components::shared::visual_style_bg::set_srpg_background_key(srpg_key);
    }

    fn append_gameplay_offset_prompt_actors(&self, actors: &mut Vec<Actor>) {
        if self.state.screens.current_screen != CurrentScreen::Gameplay
            || self.state.gameplay_offset_save_prompt.is_none()
        {
            return;
        }
        let Some(gs) = self.state.screens.gameplay_state.as_ref() else {
            return;
        };
        let snapshot = gameplay_offset_snapshot(gs);
        if !gameplay_offset_saveable_changed(snapshot) {
            return;
        }
        let active_choice = self
            .state
            .gameplay_offset_save_prompt
            .as_ref()
            .map_or(0, |prompt| prompt.active_choice)
            .min(1);
        let title = gs.song().display_full_title(false);
        let prompt_text = gameplay_offset_prompt_text(title.as_str(), snapshot);
        if prompt_text.is_empty() {
            return;
        }

        let w = space::screen_width();
        let h = space::screen_height();
        let cx = w * 0.5;
        let cy = h * 0.5;
        let answer_y = cy + 120.0;
        let choice_yes_x = cx - 100.0;
        let choice_no_x = cx + 100.0;
        let cursor_x = [choice_yes_x, choice_no_x][active_choice as usize];
        let cursor_color = color::simply_love_rgba(gs.active_color_index());

        actors.push(deadlib_present::__act_from_builder!(
            (align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(w, h):
            diffuse(0.0, 0.0, 0.0, 0.9):
            z(GAMEPLAY_OFFSET_PROMPT_Z_BACKDROP))
            deadsync_assets::present_dsl::SpriteBuilder::solid()
        ));
        actors.push(deadlib_present::__act_from_builder!(
            (align(0.5, 0.5):
            xy(cursor_x, answer_y):
            setsize(145.0, 40.0):
            diffuse(cursor_color[0], cursor_color[1], cursor_color[2], 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_CURSOR))
            deadsync_assets::present_dsl::SpriteBuilder::solid()
        ));
        actors.push(deadlib_present::__act_from_builder!(
            (align(0.5, 0.5):
            xy(cx, cy - 60.0):
            font("miso"):
            zoom(0.95):
            maxwidth(w - 100.0):
            settext(prompt_text):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_TEXT):
            horizalign(center))
            deadsync_assets::present_dsl::TextBuilder::new()
        ));
        actors.push(deadlib_present::__act_from_builder!(
            (align(0.5, 0.5):
            xy(choice_yes_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext("YES"):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_TEXT):
            horizalign(center))
            deadsync_assets::present_dsl::TextBuilder::new()
        ));
        actors.push(deadlib_present::__act_from_builder!(
            (align(0.5, 0.5):
            xy(choice_no_x, answer_y):
            font("wendy"):
            zoom(0.72):
            settext("NO"):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GAMEPLAY_OFFSET_PROMPT_Z_TEXT):
            horizalign(center))
            deadsync_assets::present_dsl::TextBuilder::new()
        ));
    }

    fn get_current_actors(
        &mut self,
        arrow_effect_time_s: f32,
        config: &config::Config,
    ) -> (Vec<Actor>, [f32; 4]) {
        const CLEAR: [f32; 4] = [0.03, 0.03, 0.03, 1.0];
        let mut screen_alpha_multiplier = 1.0;

        let is_actor_fade_screen = is_actor_fade_screen(self.state.screens.current_screen);

        if is_actor_fade_screen {
            match self.state.shell.transition {
                TransitionState::ActorsFadeIn { elapsed } => {
                    screen_alpha_multiplier = (elapsed / MENU_ACTORS_FADE_DURATION).clamp(0.0, 1.0);
                }
                TransitionState::ActorsFadeOut {
                    elapsed, duration, ..
                } => {
                    screen_alpha_multiplier = 1.0 - (elapsed / duration).clamp(0.0, 1.0);
                }
                _ => {}
            }
        }

        let mut actors = std::mem::take(&mut self.actor_scratch);
        actors.clear();

        match self.state.screens.current_screen {
            CurrentScreen::Menu => {
                self.sync_main_menu_runtime_view();
                let update_tag = updater::available_update_tag();
                menu::push_actors(
                    &mut actors,
                    &self.state.screens.menu_state,
                    update_tag.as_deref(),
                    screen_alpha_multiplier,
                );
            }
            CurrentScreen::Gameplay => {
                if let Some(gs) = &mut self.state.screens.gameplay_state {
                    screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(gs);
                    let smx_overlay_alpha = match self.state.shell.transition {
                        TransitionState::FadingIn { elapsed, duration } => {
                            if duration <= gameplay::TRANSITION_IN_RESTART_DURATION + 0.01 {
                                // Restart: the in-transition black fades over the whole short
                                // duration; mirror it so the overlays fade in with the black.
                                (elapsed / duration).clamp(0.0, 1.0)
                            } else {
                                // Normal entry: black holds solid until the last
                                // TRANSITION_IN_BLACK_FADE_DURATION seconds, then lifts.
                                let fade_start =
                                    duration - gameplay::TRANSITION_IN_BLACK_FADE_DURATION;
                                ((elapsed - fade_start)
                                    / gameplay::TRANSITION_IN_BLACK_FADE_DURATION)
                                    .clamp(0.0, 1.0)
                            }
                        }
                        TransitionState::FadingOut { elapsed, .. } => {
                            // Mirror the out-transition black quad: hold full opacity
                            // during TRANSITION_OUT_DELAY then fade down as the black fades up.
                            1.0 - ((elapsed - gameplay::TRANSITION_OUT_DELAY)
                                / gameplay::TRANSITION_OUT_FADE_DURATION)
                                .clamp(0.0, 1.0)
                        }
                        _ => 1.0,
                    };
                    gameplay::push_actors(
                        &mut actors,
                        gs,
                        &self.asset_manager,
                        gameplay::ActorViewOverride {
                            smx_overlay_alpha,
                            ..Default::default()
                        },
                        arrow_effect_time_s,
                        config,
                    );
                }
            }
            CurrentScreen::Practice => {
                if let Some(ps) = &mut self.state.screens.practice_state {
                    screens::components::gameplay::gameplay_stats::refresh_density_graph_meshes(
                        &mut ps.gameplay,
                    );
                    practice::push_actors(
                        &mut actors,
                        ps,
                        &self.asset_manager,
                        arrow_effect_time_s,
                        config,
                    );
                }
            }
            CurrentScreen::Options => {
                let updater = updater::view();
                options::push_actors(
                    &mut actors,
                    &self.state.screens.options_state,
                    &self.asset_manager,
                    &updater,
                    screen_alpha_multiplier,
                );
            }
            CurrentScreen::Credits => {
                credits::push_actors(&mut actors, &self.state.screens.credits_state)
            }
            CurrentScreen::ManageLocalProfiles => manage_local_profiles::push_actors(
                &mut actors,
                &self.state.screens.manage_local_profiles_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Mappings => mappings::push_actors(
                &mut actors,
                &self.state.screens.mappings_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::Input => {
                input_screen::push_actors(&mut actors, &self.state.screens.input_state)
            }
            CurrentScreen::ConfigurePads => {
                screens::pad_config::push_actors(&mut actors, &self.state.screens.pad_config_state);
            }
            CurrentScreen::TestLights => test_lights::push_actors(
                &mut actors,
                &self.state.screens.test_lights_state,
                lights_test_view(self.lights.state_snapshot(), self.lights.mode()),
                screen_alpha_multiplier,
            ),
            CurrentScreen::OverscanAdjustment => overscan_adjustment::push_actors(
                &mut actors,
                &self.state.screens.overscan_adjustment_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SmxAssignPads => screens::smx_assign::push_actors(
                &mut actors,
                &self.state.screens.smx_assign_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::PlayerOptions => {
                if let Some(pos) = &self.state.screens.player_options_state {
                    player_options::push_actors(&mut actors, pos, &self.asset_manager);
                }
            }
            CurrentScreen::SelectProfile => select_profile::push_actors(
                &mut actors,
                &self.state.screens.select_profile_state,
                &self.asset_manager,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectColor => select_color::push_actors(
                &mut actors,
                &self.state.screens.select_color_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::ArrowCloudLogin => screens::arrowcloud_login::push_actors(
                &mut actors,
                &self.state.screens.arrowcloud_login_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::GrooveStatsLogin => screens::groovestats_login::push_actors(
                &mut actors,
                &self.state.screens.groovestats_login_state,
                screen_alpha_multiplier,
            ),
            CurrentScreen::SelectStyle => {
                select_style::push_actors(&mut actors, &self.state.screens.select_style_state);
            }
            CurrentScreen::SelectPlayMode => select_mode::push_actors(
                &mut actors,
                &self.state.screens.select_play_mode_state,
                &self.asset_manager,
            ),
            CurrentScreen::ProfileLoad => {
                profile_load::push_actors(&mut actors, &self.state.screens.profile_load_state);
            }
            CurrentScreen::SelectMusic => {
                select_music::push_actors(
                    &mut actors,
                    &self.state.screens.select_music_state,
                    &self.asset_manager,
                    self.state.session.played_stages.len() + 1,
                );
            }
            CurrentScreen::SelectCourse => select_course::push_actors(
                &mut actors,
                &self.state.screens.select_course_state,
                &self.asset_manager,
            ),
            CurrentScreen::Sandbox => {
                sandbox::push_actors(&mut actors, &self.state.screens.sandbox_state)
            }
            CurrentScreen::Init => init::push_actors(&mut actors, &self.state.screens.init_state),
            CurrentScreen::Evaluation => {
                evaluation::push_actors(
                    &mut actors,
                    &self.state.screens.evaluation_state,
                    &self.asset_manager,
                );
            }
            CurrentScreen::EvaluationSummary => {
                let stages = self.post_select_display_stages(config.show_course_individual_scores);
                evaluation_summary::push_actors(
                    &mut actors,
                    &self.state.screens.evaluation_summary_state,
                    &stages,
                    &self.asset_manager,
                );
            }
            CurrentScreen::Initials => {
                let stages = self.post_select_display_stages(config.show_course_individual_scores);
                initials::push_actors(
                    &mut actors,
                    &self.state.screens.initials_state,
                    &stages,
                    &self.asset_manager,
                );
            }
            CurrentScreen::GameOver => gameover::push_actors(
                &mut actors,
                &self.state.screens.gameover_state,
                &self.state.session.played_stages,
                &self.asset_manager,
            ),
        };

        if self.state.shell.overlay_mode.shows_fps() {
            let overlay = screens::components::shared::stats_overlay::build(
                self.backend_type,
                self.state.shell.last_fps,
                self.state.shell.last_vpf,
                self.stats_overlay_timing(),
            );
            actors.extend(overlay);
            if self.state.shell.overlay_mode.shows_stutter() {
                let now_seconds = Instant::now()
                    .duration_since(self.state.shell.start_time)
                    .as_secs_f32();
                let stutters = self.state.shell.stutter_samples.visible(now_seconds);
                actors.extend(screens::components::shared::stats_overlay::build_stutter(
                    &stutters,
                ));
            }
        }

        self.push_frame_stats_overlay(&mut actors);

        // Bottom-corner build watermark so videos / screenshots always
        // carry the running version. Default on; user-toggleable via
        // Options, with a separate Left/Right side preference.
        if config.show_version_overlay {
            actors.extend(screens::components::shared::version_overlay::build(
                config.version_overlay_side,
                config.log_level,
                option_env!("DEADSYNC_BUILD_HASH"),
            ));
        }

        // Gamepad connection overlay (always on top of screen, but below transitions)
        if let Some(msg) = self.state.shell.interaction.message() {
            let params = screens::components::shared::gamepad_overlay::Params { message: msg };
            actors.extend(screens::components::shared::gamepad_overlay::build(params));
        }
        self.append_gameplay_offset_prompt_actors(&mut actors);

        match &self.state.shell.transition {
            TransitionState::FadingOut { .. } => {
                let (out_actors, _) =
                    self.get_out_transition_for_screen(self.state.screens.current_screen);
                actors.extend(out_actors);
            }
            TransitionState::ActorsFadeOut { target, .. } => {
                // Special case: Menu -> SelectColor / Menu -> Options should keep the
                // visual-style background bright and only fade UI, but still play the splash.
                if self.state.screens.current_screen == CurrentScreen::Menu
                    && (*target == CurrentScreen::SelectProfile
                        || *target == CurrentScreen::SelectColor
                        || *target == CurrentScreen::Options)
                {
                    let splash = screens::components::menu::menu_splash::build(
                        self.state.screens.menu_state.active_color_index,
                    );
                    actors.extend(splash);
                }
            }
            TransitionState::FadingIn { .. } => {
                let (in_actors, _) =
                    self.get_in_transition_for_screen(self.state.screens.current_screen);
                actors.extend(in_actors);
            }
            _ => {}
        }

        self.append_screenshot_overlay_actors(&mut actors, Instant::now());

        (actors, CLEAR)
    }

    fn push_frame_stats_overlay(&mut self, actors: &mut Vec<Actor>) {
        use screens::components::shared::frame_stats_overlay;

        if !self.state.shell.frame_stats.enabled() {
            return;
        }

        // Target frame time for the graph reference lines: the monitor refresh period if
        // known, else the configured max-FPS cap. The controller falls back to its average.
        let refresh_ns = self.state.shell.last_present_stats.refresh_ns;
        let target_frame_us = frame_stats_target_us(
            refresh_ns,
            self.state
                .shell
                .background_frame_interval(self.state.screens.current_screen),
        );

        let in_gameplay = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();

        let audio = deadsync_audio_stream::get_output_timing_snapshot();
        let raw_callback_gap_ms =
            deadsync_audio_stream::timing_diag_last_callback_gap_ns() as f32 / 1_000_000.0;
        let fps = self.state.shell.last_fps;
        let Some(view) = self
            .state
            .shell
            .frame_stats
            .view(target_frame_us, raw_callback_gap_ms)
        else {
            return;
        };
        let summary = frame_stats_summary(
            view.metrics,
            FrameStatsSummaryContext {
                fps,
                display_error_seconds: display_clock.error_seconds,
                display_catching_up: display_clock.catching_up,
                in_gameplay,
                audio,
            },
        );
        let screen_w = deadlib_present::space::screen_width();
        let screen_h = deadlib_present::space::screen_height();
        // Always render the full overlay, including 2 players — the panel is narrow enough
        // (~half-screen) to sit in a corner or the bottom-center seam without covering either
        // notefield, so there's no need to drop to the stripped compact layout.
        actors.extend(frame_stats_overlay::build(
            view.samples,
            summary,
            view.anchor,
            false,
            view.style,
            screen_w,
            screen_h,
        ));
    }

    #[inline(always)]
    fn record_stutter_diag_frame(
        &mut self,
        frame_host_nanos: u64,
        screen: CurrentScreen,
        frame_seconds: f32,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        input_us: u32,
        update_us: u32,
        compose_us: u32,
        upload_us: u32,
        draw_us: u32,
        draw_stats: renderer::DrawStats,
    ) {
        if !stutter_diag_enabled() {
            return;
        }
        let display_clock = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_health())
            .unwrap_or_default();
        let expected_seconds = self.state.shell.expected_frame_seconds(screen);
        self.state.shell.stutter_diag.record_frame(
            frame_host_nanos,
            screen,
            frame_seconds,
            expected_seconds,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            draw_stats,
            display_clock.error_seconds,
            display_clock.catching_up,
        );
    }

    fn dump_stutter_diag_window(
        &self,
        now_host_nanos: u64,
        total_elapsed: f32,
        screen: CurrentScreen,
        stutter_severity: u8,
        audio_triggered: bool,
        display_triggered: bool,
    ) {
        let mut frames = Vec::with_capacity(STUTTER_DIAG_FRAME_CAPACITY);
        self.state
            .shell
            .stutter_diag
            .collect_recent(now_host_nanos, &mut frames);
        let mut audio_events = Vec::with_capacity(32);
        deadsync_audio_stream::collect_stutter_diag_events(
            now_host_nanos,
            STUTTER_DIAG_WINDOW_NS,
            &mut audio_events,
        );
        let mut display_events = Vec::with_capacity(32);
        if let Some(gameplay_state) = self.state.screens.gameplay_state.as_ref() {
            gameplay_state.collect_display_clock_stutter_diag_events(
                now_host_nanos,
                STUTTER_DIAG_WINDOW_NS,
                &mut display_events,
            );
        }
        for line in stutter_diag_dump_lines(
            StutterDiagDumpContext {
                now_host_nanos,
                total_elapsed,
                screen,
                stutter_severity,
                audio_triggered,
                display_triggered,
            },
            &frames,
            &display_events,
            &audio_events,
        ) {
            trace!("{line}");
        }
    }

    fn trace_stutter_diag_dump_if_needed(
        &mut self,
        now_host_nanos: u64,
        total_elapsed: f32,
        screen: CurrentScreen,
        frame_seconds: f32,
    ) {
        if !stutter_diag_enabled() {
            return;
        }
        if now_host_nanos == 0 {
            return;
        }
        let expected = self.state.shell.expected_frame_seconds(screen);
        let stutter_severity = stutter_severity(frame_seconds, expected);
        let audio_trigger_seq = deadsync_audio_stream::stutter_diag_trigger_seq();
        let display_trigger_seq = self
            .state
            .screens
            .gameplay_state
            .as_ref()
            .map(|gs| gs.display_clock_stutter_diag_trigger_seq())
            .unwrap_or(0);
        let Some((audio_triggered, display_triggered)) =
            self.state.shell.stutter_diag.take_dump_trigger(
                now_host_nanos,
                stutter_severity,
                audio_trigger_seq,
                display_trigger_seq,
            )
        else {
            return;
        };
        self.dump_stutter_diag_window(
            now_host_nanos,
            total_elapsed,
            screen,
            stutter_severity,
            audio_triggered,
            display_triggered,
        );
    }

    /* -------------------- keyboard: map -> route -------------------- */

    #[inline(always)]
    fn handle_key_text(&mut self, event_loop: &ActiveEventLoop, text: &str) {
        let action = match raw_key_text_route(self.state.screens.current_screen) {
            RawKeyTextRoute::ManageLocalProfiles => {
                screens::manage_local_profiles::handle_raw_key_event(
                    &mut self.state.screens.manage_local_profiles_state,
                    None,
                    Some(text),
                )
            }
            RawKeyTextRoute::Options => screens::options::handle_raw_key_event(
                &mut self.state.screens.options_state,
                None,
                Some(text),
            ),
            RawKeyTextRoute::SelectMusic => screens::select_music::handle_raw_key_event(
                &mut self.state.screens.select_music_state,
                None,
                Some(text),
            ),
            RawKeyTextRoute::Ignore => ThemeEffect::None,
        };
        if matches!(action, ThemeEffect::None) {
            return;
        }
        if let Err(e) = self.handle_action(action, event_loop) {
            log::error!("Failed to handle text input action: {e}");
        }
    }

    #[inline(always)]
    fn handle_raw_key_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        raw_key: RawKeyboardEvent,
    ) -> bool {
        use winit::keyboard::KeyCode;

        self.state
            .shell
            .interaction
            .controls_mut()
            .update_modifier(raw_key.code, raw_key.pressed);

        if logical_input::with_keymap(|km| {
            km.raw_key_event_has_action(&raw_key, |action| {
                action == VirtualAction::system_fast_forward
            })
        }) {
            self.state
                .shell
                .interaction
                .controls_mut()
                .set_fast_forward(raw_key.pressed);
        }
        if logical_input::with_keymap(|km| {
            km.raw_key_event_has_action(&raw_key, |action| {
                action == VirtualAction::system_slow_down
            })
        }) {
            self.state
                .shell
                .interaction
                .controls_mut()
                .set_slow_down(raw_key.pressed);
        }

        let ctrl_held = self.state.shell.interaction.controls().ctrl();
        let shift_held = self.state.shell.interaction.controls().shift();
        let alt_held = self.state.shell.interaction.controls().alt();

        if raw_key_alt_f4_quit(raw_key.pressed, raw_key.code, alt_held) {
            info!("Alt+F4 quit shortcut pressed. Shutting down.");
            event_loop.exit();
            return true;
        }

        match raw_key_screen_route(self.state.screens.current_screen) {
            RawKeyScreenRoute::Sandbox => {
                let action = screens::sandbox::handle_raw_key_event(
                    &mut self.state.screens.sandbox_state,
                    &raw_key,
                );
                if !matches!(action, ThemeEffect::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle Sandbox raw key action: {e}");
                    }
                    return true;
                }
            }
            RawKeyScreenRoute::Menu => {
                self.sync_main_menu_runtime_view();
                let action = screens::menu::handle_raw_key_event(
                    &mut self.state.screens.menu_state,
                    &raw_key,
                );
                if !matches!(action, ThemeEffect::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle Menu raw key action: {e}");
                    }
                    return true;
                }
            }
            RawKeyScreenRoute::Mappings => {
                let action = screens::mappings::handle_raw_key_event(
                    &mut self.state.screens.mappings_state,
                    &raw_key,
                );
                if !matches!(action, ThemeEffect::None)
                    && let Err(e) = self.handle_action(action, event_loop)
                {
                    log::error!("Failed to handle Mappings raw key action: {e}");
                }
                // On the Mappings screen, arrows/Enter/Escape are handled entirely
                // via raw keycodes; do not route through the virtual keymap.
                return true;
            }
            RawKeyScreenRoute::ManageLocalProfiles => {
                let action = screens::manage_local_profiles::handle_raw_key_event(
                    &mut self.state.screens.manage_local_profiles_state,
                    Some(&raw_key),
                    None,
                );
                if !matches!(action, ThemeEffect::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle ManageLocalProfiles raw key action: {e}");
                    }
                    return true;
                }
            }
            RawKeyScreenRoute::OverscanAdjustment => {
                // The overscan screen owns the W/A/S/D/I/J/K/L adjustment keys so they
                // do not also fire as virtual P1 pad directions. Other keys (arrows,
                // Enter, Escape) fall through to the virtual keymap for menu/pad nav.
                if screens::overscan_adjustment::handle_raw_key_event(
                    &mut self.state.screens.overscan_adjustment_state,
                    &raw_key,
                ) {
                    return true;
                }
            }
            RawKeyScreenRoute::Input => {
                let action = screens::input::handle_raw_key_event(
                    &mut self.state.screens.input_state,
                    &raw_key,
                );
                if !matches!(action, ThemeEffect::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle Input raw key action: {e}");
                    }
                    return true;
                }
            }
            RawKeyScreenRoute::Options => {
                let action = screens::options::handle_raw_key_event(
                    &mut self.state.screens.options_state,
                    Some(&raw_key),
                    None,
                );
                if !matches!(action, ThemeEffect::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle Options raw key action: {e}");
                    }
                    return true;
                }
            }
            RawKeyScreenRoute::SelectMusic => {
                // Route screen-specific raw key handling (e.g., F7 fetch) to the screen
                let action = screens::select_music::handle_raw_key_event_with_modifiers(
                    &mut self.state.screens.select_music_state,
                    Some(&raw_key),
                    None,
                    ctrl_held,
                    shift_held,
                );
                if !matches!(action, ThemeEffect::None) {
                    if let Err(e) = self.handle_action(action, event_loop) {
                        log::error!("Failed to handle SelectMusic raw key action: {e}");
                    }
                    return true;
                }
            }
            RawKeyScreenRoute::PlayerOptions => {
                let returns_to_select_music = self
                    .state
                    .screens
                    .player_options_state
                    .as_ref()
                    .is_some_and(|state| state.return_screen == CurrentScreen::SelectMusic);
                if returns_to_select_music {
                    let action = screens::select_music::handle_player_options_mute_hotkey(
                        &mut self.state.screens.select_music_state,
                        &raw_key,
                    );
                    if !matches!(action, ThemeEffect::None) {
                        if let Err(e) = self.handle_action(action, event_loop) {
                            log::error!("Failed to handle PlayerOptions mute shortcut: {e}");
                        }
                        return true;
                    }
                }
            }
            RawKeyScreenRoute::Practice => {
                if practice_reload_shortcut(
                    raw_key.pressed,
                    raw_key.repeat,
                    raw_key.code,
                    ctrl_held,
                    shift_held,
                    self.input_route_policy(CurrentScreen::Practice)
                        .keyboard_features,
                ) {
                    self.try_practice_reload(event_loop, "Ctrl+Shift+R");
                    return true;
                }
                if let Some(ps) = self.state.screens.practice_state.as_mut() {
                    let (consumed, action) =
                        crate::gameplay_runtime::handle_practice_raw_key(ps, &raw_key);
                    if !matches!(action, ThemeEffect::None) {
                        if let Err(e) = self.handle_action(action, event_loop) {
                            log::error!("Failed to handle Practice raw key action: {e}");
                        }
                        return true;
                    }
                    if consumed {
                        return true;
                    }
                }
            }
            RawKeyScreenRoute::Evaluation => {
                screens::evaluation::handle_raw_key_event(
                    &mut self.state.screens.evaluation_state,
                    &raw_key,
                );
                let shortcut = evaluation_raw_key_shortcut(
                    raw_key.pressed,
                    raw_key.repeat,
                    raw_key.code,
                    ctrl_held,
                    shift_held,
                    self.input_route_policy(CurrentScreen::Evaluation)
                        .keyboard_features,
                    self.state.session.course_run.is_some(),
                    screens::evaluation::submission_retry_available(
                        &self.state.screens.evaluation_state,
                    ),
                    !self.state.session.course_eval_pages.is_empty(),
                );
                match shortcut {
                    Some(EvaluationRawKeyShortcut::GameplayRestart) => {
                        self.try_gameplay_restart(event_loop, "Ctrl+R");
                        return true;
                    }
                    Some(EvaluationRawKeyShortcut::GameplayReload) => {
                        self.try_gameplay_reload(event_loop, "Ctrl+Shift+R");
                        return true;
                    }
                    Some(EvaluationRawKeyShortcut::PracticeFromEvaluation) => {
                        if self.try_practice_from_eval(event_loop, "Ctrl+P") {
                            return true;
                        }
                    }
                    Some(EvaluationRawKeyShortcut::RetrySubmissions) => {
                        Self::retry_evaluation_submissions(&self.state.screens.evaluation_state);
                        return true;
                    }
                    Some(EvaluationRawKeyShortcut::StepCourseEvalPage(delta)) => {
                        self.step_course_eval_page(delta);
                        return true;
                    }
                    None => {}
                }
            }
            RawKeyScreenRoute::None => {}
        }
        let queued_input_plan = queued_input_flush_plan(
            self.state.screens.current_screen,
            &self.state.shell.transition,
        );

        let app_raw_shortcut = app_raw_key_shortcut(
            raw_key.pressed,
            raw_key.repeat,
            raw_key.code,
            ctrl_held,
            shift_held,
            alt_held,
            self.state.shell.frame_stats.enabled(),
        );
        if raw_key.pressed && raw_key.code == KeyCode::F3 {
            if matches!(
                app_raw_shortcut,
                Some(AppRawKeyShortcut::FrameStatsCycleAnchor)
            ) {
                // Ctrl+Shift+F3: move the frame-stats overlay to the next corner (runtime only).
                if !raw_key.repeat && self.state.shell.frame_stats.enabled() {
                    let num_players = self
                        .state
                        .screens
                        .gameplay_state
                        .as_ref()
                        .map(|state| state.num_players())
                        .unwrap_or(1);
                    let two_player = frame_stats_two_player(
                        deadsync_profile::compat::get_session_play_style(),
                        num_players,
                    );
                    let anchor = self.state.shell.frame_stats.cycle_anchor(two_player);
                    config::update_frame_stats_overlay_anchor(anchor.to_key());
                    debug!("Frame stats overlay corner {anchor:?}");
                }
            } else if matches!(
                app_raw_shortcut,
                Some(AppRawKeyShortcut::FrameStatsToggleStyle)
            ) {
                // Ctrl+Alt+F3: switch the overlay presentation (detailed ↔ minimal).
                if !raw_key.repeat && self.state.shell.frame_stats.enabled() {
                    let style = self.state.shell.frame_stats.toggle_style();
                    config::update_frame_stats_overlay_style(style.label());
                    debug!("Frame stats overlay style {}", style.label());
                }
            } else if matches!(app_raw_shortcut, Some(AppRawKeyShortcut::FrameStatsToggle)) {
                if !raw_key.repeat {
                    let on = self.state.shell.frame_stats.toggle();
                    // Only auto-place when the user hasn't positioned it themselves; otherwise
                    // restore the remembered corner (persisted across toggles and restarts).
                    if on {
                        self.state.shell.frame_stats.use_default_anchor();
                    }
                    debug!("Frame stats overlay {}", if on { "ON" } else { "OFF" });
                }
            } else if matches!(app_raw_shortcut, Some(AppRawKeyShortcut::CycleOverlayMode)) {
                let mode = self.state.shell.cycle_overlay_mode();
                debug!("Overlay {}", self.state.shell.overlay_mode.label());
                config::update_show_stats_mode(mode);
                options::sync_show_stats_mode(&mut self.state.screens.options_state, mode);
            }
        }
        if matches!(
            app_raw_shortcut,
            Some(AppRawKeyShortcut::ToggleTranslatedTitles)
        ) {
            let new_value = !config::get().translated_titles;
            config::update_translated_titles(new_value);
            options::sync_translated_titles(&mut self.state.screens.options_state, new_value);
            deadsync_audio_stream::play_sfx("assets/sounds/change.ogg");
        }
        // Screen-specific Escape handling resides in per-screen raw handlers now

        let Some(plan) = queued_input_plan else {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
            return true;
        };

        if plan.gameplay_screen {
            return false;
        }

        let mut input_err: Option<Box<dyn Error>> = None;
        let mut batch = QueuedInputBatchState::new();
        logical_input::map_raw_key_event_with(&raw_key, |ev| {
            match plan.route_mapped_event(&batch, input_err.is_some()) {
                QueuedInputEventRoute::Skip | QueuedInputEventRoute::Gameplay => {}
                QueuedInputEventRoute::Screen => {
                    if let Err(e) = self.route_input_event(event_loop, ev) {
                        input_err = Some(e);
                    }
                    plan.note_dispatched_event(
                        &mut batch,
                        self.state.screens.current_screen,
                        &self.state.shell.transition,
                    );
                }
            }
        });
        if let Some(e) = input_err {
            log::error!("Failed to handle input: {e}");
            event_loop.exit();
            return true;
        }
        false
    }

    #[inline(always)]
    fn handle_live_key_event(&mut self, event_loop: &ActiveEventLoop, raw_key: RawKeyboardEvent) {
        let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
        let handled_started = Instant::now();

        if !self.handle_raw_key_event(event_loop, raw_key) {
            if gameplay_screen {
                let start_screen = self.state.screens.current_screen;
                if let Some(gameplay_ev) = gameplay_raw_key_event(&raw_key)
                    && let Err(e) = self.route_gameplay_event(event_loop, gameplay_ev)
                {
                    log::error!("Failed to handle gameplay raw key: {e}");
                    event_loop.exit();
                    return;
                }
                if !gameplay_dispatch_continues(
                    start_screen,
                    self.state.screens.current_screen,
                    &self.state.shell.transition,
                ) {
                    return;
                }
            }

            let mut input_err: Option<Box<dyn Error>> = None;
            let start_screen = self.state.screens.current_screen;
            let mut batch = QueuedInputBatchState::new();
            let plan = queued_input_flush_plan(start_screen, &self.state.shell.transition);
            logical_input::map_raw_key_event_with(&raw_key, |ev| {
                let Some(plan) = plan else {
                    return;
                };
                match plan.route_mapped_event(&batch, input_err.is_some()) {
                    QueuedInputEventRoute::Skip => {}
                    QueuedInputEventRoute::Gameplay => {
                        if let Err(e) =
                            self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(ev))
                        {
                            input_err = Some(e);
                            return;
                        }
                        plan.note_dispatched_event(
                            &mut batch,
                            self.state.screens.current_screen,
                            &self.state.shell.transition,
                        );
                    }
                    QueuedInputEventRoute::Screen => {
                        if let Err(e) = self.route_input_event(event_loop, ev) {
                            input_err = Some(e);
                        }
                    }
                }
            });
            if let Some(e) = input_err {
                log::error!("Failed to handle input: {e}");
                event_loop.exit();
                return;
            }
        }

        self.state.shell.gameplay_input_trace.note_key_handler(
            gameplay_screen,
            raw_key.repeat,
            elapsed_us_since(handled_started),
        );
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[inline(always)]
    fn handle_unix_window_keyboard_fallback(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: &winit::event::KeyEvent,
    ) {
        use winit::event::ElementState;
        use winit::keyboard::PhysicalKey;

        if deadsync_input_native::unix_raw_keyboard_backend_active() || !self.accepts_live_input() {
            return;
        }
        let PhysicalKey::Code(code) = key_event.physical_key else {
            return;
        };
        self.handle_live_key_event(
            event_loop,
            RawKeyboardEvent {
                code,
                pressed: key_event.state == ElementState::Pressed,
                repeat: key_event.repeat,
                timestamp: Instant::now(),
                host_nanos: host_time::now_nanos(),
            },
        );
    }

    /* -------------------- pad event routing -------------------- */

    #[inline(always)]
    fn handle_pad_event(&mut self, event_loop: &ActiveEventLoop, ev: PadEvent) {
        // Press-feedback gif: on any SMX panel press/release outside gameplay, play
        // the pack's `press` animation on that panel's low-priority layer. Gated on
        // smx_panel_lights; gated on non-gameplay so the judgement/sustain layers
        // (which are higher priority) own gameplay fully.
        let current_screen = self.state.screens.current_screen;
        if current_screen != CurrentScreen::Gameplay {
            let input_policy = self.input_route_policy(current_screen);
            if let Some(plan) = smx_panel_press_feedback_plan(
                input_policy.smx_input,
                input_policy.smx_panel_lights,
                current_screen,
                &self.smx_blackout_synced,
                &ev,
            ) {
                self.smx_panels
                    .on_raw_panel(plan.pad_slot, plan.panel, plan.pressed);
            }
        }

        let Some(plan) = queued_input_flush_plan(current_screen, &self.state.shell.transition)
        else {
            logical_input::clear_debounce_state();
            self.lights.clear_button_pressed();
            self.clear_gameplay_input_events();
            return;
        };
        let mut input_err: Option<Box<dyn Error>> = None;
        let mut batch = QueuedInputBatchState::new();
        logical_input::map_pad_event_with(&ev, |iev| {
            match plan.route_mapped_event(&batch, input_err.is_some()) {
                QueuedInputEventRoute::Skip => {}
                QueuedInputEventRoute::Gameplay => {
                    if let Err(e) =
                        self.route_gameplay_event(event_loop, GameplayQueuedEvent::Input(iev))
                    {
                        input_err = Some(e);
                        return;
                    }
                    plan.note_dispatched_event(
                        &mut batch,
                        self.state.screens.current_screen,
                        &self.state.shell.transition,
                    );
                }
                QueuedInputEventRoute::Screen => {
                    if let Err(e) = self.route_input_event(event_loop, iev) {
                        input_err = Some(e);
                    }
                }
            }
        });
        if let Some(e) = input_err {
            error!("Failed to handle pad input: {e}");
            event_loop.exit();
        }
    }

    // legacy virtual-action dispatcher removed; screens own their input

    #[cfg(any())]
    #[inline(always)]
    fn poll_gamepad_and_dispatch(&mut self, _event_loop: &ActiveEventLoop) {}

    fn handle_audio_and_profile_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let player_options = if prev == CurrentScreen::PlayerOptions {
            let session = profile::get_session_snapshot();
            self.state
                .screens
                .player_options_state
                .as_ref()
                .map(|state| PlayerOptionsTransition {
                    speed_mod: &state.speed_mod,
                    chart_difficulty_index: state.chart_difficulty_index,
                    music_rate: state.music_rate,
                    play_style: session.play_style,
                    player_side: session.player_side,
                })
        } else {
            None
        };
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: prev,
            target,
            menu_music_enabled: config::get().menu_music,
            gameover_music_enabled: visual_styles::srpg10_active(),
            music_paths: TransitionMusicPaths {
                menu: visual_styles::menu_music_resolved_path(),
                course: dirs::app_dirs()
                    .resolve_asset_path("assets/music/select_course (loop).ogg"),
                credits: dirs::app_dirs().resolve_asset_path("assets/music/credits.ogg"),
                gameover: visual_styles::srpg10_gameover_music_path(),
            },
            player_options,
            select_music_preferred_difficulty: (prev == CurrentScreen::SelectMusic).then_some(
                self.state
                    .screens
                    .select_music_state
                    .preferred_difficulty_index,
            ),
        });
        if plan.stop_screen_sfx {
            deadsync_audio_stream::stop_screen_sfx();
        }
        if plan.clear_play_background {
            if let Some(backend) = self.backend.as_mut() {
                self.dynamic_media.set_background(
                    &mut self.asset_manager,
                    backend,
                    None,
                    0.0,
                    false,
                );
            }
        }
        if prev == CurrentScreen::PlayerOptions {
            for command in &plan.commands {
                if let Command::UpdateScrollSpeed { side, setting } = command {
                    debug!("Saved scroll speed ({side:?}): {setting}");
                }
            }
            if let Some(state) = &self.state.screens.player_options_state {
                debug!("Session music rate set to {:.2}x", state.music_rate);
            }
        }
        if let Some(preferred) = plan.preferred_difficulty_index {
            self.state.session.preferred_difficulty_index = preferred;
            if prev == CurrentScreen::PlayerOptions {
                debug!("Updated preferred difficulty index to {preferred} from PlayerOptions");
            }
        }
        plan.commands
    }

    /// Begin a new play session: start the session timer, clear per-session state,
    /// and drop the SMX managed-config resolve signatures so each connected pad's
    /// default is reasserted for the new session. (A manual Apply from a prior
    /// session writes the pad + marker but not the resolve signature, so without
    /// the reset the override would persist into the next session — unlike a full
    /// app restart, which always resolves fresh.) No-op if a session is active.
    fn begin_play_session(&mut self) {
        if !self.state.session.begin_play_session(Instant::now()) {
            return;
        }
        self.pad_config_sync.reset_signatures();
        debug!("Session timer started.");
    }

    fn sync_profile_load_state(
        &mut self,
        profiles: &[profile_data::Profile; profile_data::PLAYER_SLOTS],
    ) {
        self.state.session.combo_carry = profile::combo_carry_for_profiles(profiles);
        let session = profile::get_session_snapshot();
        let play_style = session.play_style;
        let active_side = session.player_side;
        let active_ix = profile_data::player_side_index(active_side);
        self.state.session.preferred_difficulty_index =
            profile::preferred_difficulty_for_profile(&profiles[active_ix], play_style);

        if let Some(backend) = self.backend.as_mut() {
            self.dynamic_media.set_profile_avatar_for_side(
                &mut self.asset_manager,
                backend,
                profile_data::PlayerSide::P1,
                profiles[0].avatar_path.clone(),
            );
            self.dynamic_media.set_profile_avatar_for_side(
                &mut self.asset_manager,
                backend,
                profile_data::PlayerSide::P2,
                profiles[1].avatar_path.clone(),
            );
        }
    }

    fn handle_screen_state_on_fade(&mut self, prev: CurrentScreen, target: CurrentScreen) {
        if prev == CurrentScreen::SelectColor {
            let idx = self.state.screens.select_color_state.active_color_index;
            self.sync_screen_color_index(idx);
        } else if prev == CurrentScreen::Options {
            let idx = self.state.screens.options_state.active_color_index;
            self.sync_screen_color_index(idx);
        }

        if target == CurrentScreen::Menu {
            self.state.session.reset_for_menu(profile::combo_carry());
            let current_color_index = self.state.screens.menu_state.active_color_index;
            self.state.screens.menu_state = menu::init();
            self.state.screens.menu_state.active_color_index = current_color_index;
        } else if target == CurrentScreen::Options {
            self.reset_options_state_for_entry(prev);
        } else if target == CurrentScreen::Credits {
            self.state.screens.credits_state = credits::init();
            self.state.screens.credits_state.active_color_index =
                self.state.screens.options_state.active_color_index;
        } else if target == CurrentScreen::ManageLocalProfiles {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.manage_local_profiles_state = manage_local_profiles::init();
            self.state
                .screens
                .manage_local_profiles_state
                .active_color_index = color_index;
        } else if target == CurrentScreen::Mappings {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.mappings_state = mappings::init(crate::mappings::runtime_view());
            self.state.screens.mappings_state.active_color_index = color_index;
        } else if target == CurrentScreen::TestLights {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.test_lights_state = test_lights::init();
            self.state.screens.test_lights_state.active_color_index = color_index;
            test_lights::on_enter(&mut self.state.screens.test_lights_state);
            self.lights.set_test_auto_cycle();
        } else if target == CurrentScreen::OverscanAdjustment {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.overscan_adjustment_state = overscan_adjustment::init();
            self.state
                .screens
                .overscan_adjustment_state
                .active_color_index = color_index;
            overscan_adjustment::on_enter(&mut self.state.screens.overscan_adjustment_state);
        } else if target == CurrentScreen::SmxAssignPads {
            let color_index = self.state.screens.options_state.active_color_index;
            self.state.screens.smx_assign_state = screens::smx_assign::init();
            self.state.screens.smx_assign_state.active_color_index = color_index;
            screens::smx_assign::on_enter(
                &mut self.state.screens.smx_assign_state,
                &crate::smx_config::smx_assignment_view(),
            );
        } else if target == CurrentScreen::SelectProfile {
            let current_color_index = self.state.screens.select_profile_state.active_color_index;
            self.state.screens.select_profile_state = select_profile::init();
            self.state.screens.select_profile_state.active_color_index = current_color_index;
            select_profile::set_fast_switch(
                &mut self.state.screens.select_profile_state,
                prev == CurrentScreen::SelectMusic,
            );
            if prev == CurrentScreen::Menu {
                let p2 = self.state.screens.menu_state.started_by_p2;
                select_profile::set_joined(&mut self.state.screens.select_profile_state, !p2, p2);
            } else if prev == CurrentScreen::SelectMusic {
                let session = profile::get_session_snapshot();
                select_profile::set_joined(
                    &mut self.state.screens.select_profile_state,
                    session.side_joined(profile_data::PlayerSide::P1),
                    session.side_joined(profile_data::PlayerSide::P2),
                );
            }
        } else if target == CurrentScreen::SelectStyle {
            let current_color_index = self.state.screens.select_style_state.active_color_index;
            self.state.screens.select_style_state = select_style::init();
            self.state.screens.select_style_state.active_color_index = current_color_index;
            let session = profile::get_session_snapshot();
            select_style::set_selected_index(
                &mut self.state.screens.select_style_state,
                if session.side_joined(profile_data::PlayerSide::P1)
                    && session.side_joined(profile_data::PlayerSide::P2)
                {
                    1 // "2 Players"
                } else {
                    0 // "1 Player"
                },
            );
        } else if target == CurrentScreen::SelectPlayMode {
            let current_color_index = match prev {
                CurrentScreen::SelectStyle => {
                    self.state.screens.select_style_state.active_color_index
                }
                CurrentScreen::SelectColor => {
                    self.state.screens.select_color_state.active_color_index
                }
                CurrentScreen::SelectProfile => {
                    self.state.screens.select_profile_state.active_color_index
                }
                CurrentScreen::Menu => self.state.screens.menu_state.active_color_index,
                _ => self.state.screens.select_play_mode_state.active_color_index,
            };
            self.state.screens.select_play_mode_state = select_mode::init();
            self.state.screens.select_play_mode_state.active_color_index = current_color_index;
            select_mode::on_enter(&mut self.state.screens.select_play_mode_state);
        } else if target == CurrentScreen::ProfileLoad {
            let current_color_index = match prev {
                CurrentScreen::SelectPlayMode => {
                    self.state.screens.select_play_mode_state.active_color_index
                }
                CurrentScreen::SelectStyle => {
                    self.state.screens.select_style_state.active_color_index
                }
                CurrentScreen::SelectColor => {
                    self.state.screens.select_color_state.active_color_index
                }
                CurrentScreen::SelectProfile => {
                    self.state.screens.select_profile_state.active_color_index
                }
                CurrentScreen::Menu => self.state.screens.menu_state.active_color_index,
                _ => self.state.screens.profile_load_state.active_color_index,
            };
            self.state.screens.profile_load_state = profile_load::init();
            self.state.screens.profile_load_state.active_color_index = current_color_index;
            let profiles = profile::load_default_profiles_for_joined_sides();
            self.sync_profile_load_state(&profiles);
            let play_mode = profile::get_session_play_mode();
            profile_load::on_enter(&mut self.state.screens.profile_load_state, play_mode);
            self.profile_load
                .start(play_mode, crate::select_music::init_view());
        } else if target == CurrentScreen::PlayerOptions {
            if prev == CurrentScreen::SelectCourse {
                if !self.start_course_run_from_selected() {
                    self.state.screens.player_options_state = None;
                    return;
                }
                let color_index = self.state.screens.select_course_state.active_color_index;
                if !self.prepare_player_options_for_course_stage(color_index, true) {
                    self.state.screens.player_options_state = None;
                    warn!("Unable to prepare PlayerOptions for the selected course.");
                }
            } else {
                let (song_arc, chart_steps_index, preferred_difficulty_index) = {
                    let sm_state = &self.state.screens.select_music_state;
                    let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                    let song = match entry {
                        select_music::MusicWheelEntry::Song(s) => s,
                        _ => panic!("Cannot open player options on a pack header"),
                    };
                    let play_style = profile::get_session_play_style();
                    let (steps, pref) = match play_style {
                        profile_data::PlayStyle::Versus => (
                            [
                                sm_state.selected_steps_index,
                                sm_state.p2_selected_steps_index,
                            ],
                            [
                                sm_state.preferred_difficulty_index,
                                sm_state.p2_preferred_difficulty_index,
                            ],
                        ),
                        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => (
                            [sm_state.selected_steps_index; 2],
                            [sm_state.preferred_difficulty_index; 2],
                        ),
                    };
                    (song.clone(), steps, pref)
                };

                let color_index = self.state.screens.select_music_state.active_color_index;
                let return_screen = if prev == CurrentScreen::Practice {
                    CurrentScreen::Practice
                } else {
                    CurrentScreen::SelectMusic
                };
                self.state.screens.player_options_state = Some(player_options::init(
                    song_arc,
                    chart_steps_index,
                    preferred_difficulty_index,
                    color_index,
                    return_screen,
                    None,
                    noteskin_catalog_view(),
                    crate::smx_config::smx_gif_catalog_view(),
                    crate::heart_rate::devices_view(),
                ));
            }
        } else if target == CurrentScreen::Gameplay && prev == CurrentScreen::Gameplay {
            if self.state.session.course_run.is_some() {
                let color_index = self.state.screens.gameplay_state.as_ref().map_or(
                    self.state.screens.select_course_state.active_color_index,
                    |gs| gs.gameplay.active_color_index(),
                );
                if !self.prepare_player_options_for_course_stage(color_index, false) {
                    self.state.screens.player_options_state = None;
                    warn!("Unable to prepare gameplay for the next course stage.");
                }
            }
        } else if matches!(target, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && (prev == CurrentScreen::SelectMusic
                || (target == CurrentScreen::Gameplay && prev == CurrentScreen::SelectCourse))
            && self.state.screens.player_options_state.is_none()
        {
            // Allow starting Gameplay directly from SelectMusic (Simply Love behavior) by
            // constructing a PlayerOptions state from persisted profile/session defaults.
            if prev == CurrentScreen::SelectCourse {
                if !self.start_course_run_from_selected() {
                    warn!("Unable to start gameplay: selected course has no playable stages.");
                    return;
                }
                let color_index = self.state.screens.select_course_state.active_color_index;
                if !self.prepare_player_options_for_course_stage(color_index, false) {
                    warn!("Unable to prepare gameplay for the selected course stage.");
                }
            } else {
                let (song_arc, chart_steps_index, preferred_difficulty_index) = {
                    let sm_state = &self.state.screens.select_music_state;
                    let entry = sm_state.entries.get(sm_state.selected_index).unwrap();
                    let song = match entry {
                        select_music::MusicWheelEntry::Song(s) => s,
                        _ => panic!("Cannot start gameplay or practice on a pack header"),
                    };
                    let play_style = profile::get_session_play_style();
                    let (steps, pref) = match play_style {
                        profile_data::PlayStyle::Versus => (
                            [
                                sm_state.selected_steps_index,
                                sm_state.p2_selected_steps_index,
                            ],
                            [
                                sm_state.preferred_difficulty_index,
                                sm_state.p2_preferred_difficulty_index,
                            ],
                        ),
                        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => (
                            [sm_state.selected_steps_index; 2],
                            [sm_state.preferred_difficulty_index; 2],
                        ),
                    };
                    (song.clone(), steps, pref)
                };
                let color_index = self.state.screens.select_music_state.active_color_index;
                self.state.screens.player_options_state = Some(player_options::init_for_gameplay(
                    song_arc,
                    chart_steps_index,
                    preferred_difficulty_index,
                    color_index,
                    CurrentScreen::SelectMusic,
                    None,
                    noteskin_catalog_view(),
                    crate::smx_config::smx_gif_catalog_view(),
                    crate::heart_rate::devices_view(),
                ));
            }
        }
    }

    fn sync_screen_color_index(&mut self, idx: i32) {
        self.state.screens.menu_state.active_color_index = idx;
        self.state.screens.select_profile_state.active_color_index = idx;
        self.state.screens.select_style_state.active_color_index = idx;
        self.state.screens.select_play_mode_state.active_color_index = idx;
        self.state.screens.profile_load_state.active_color_index = idx;
        self.state.screens.select_music_state.active_color_index = idx;
        self.state.screens.select_course_state.active_color_index = idx;
        self.state.screens.options_state.active_color_index = idx;
        self.state.screens.credits_state.active_color_index = idx;
        self.state
            .screens
            .manage_local_profiles_state
            .active_color_index = idx;
        self.state.screens.input_state.active_color_index = idx;
        self.state.screens.pad_config_state.active_color_index = idx;
        self.state.screens.test_lights_state.active_color_index = idx;
        self.state
            .screens
            .evaluation_summary_state
            .active_color_index = idx;
        self.state.screens.initials_state.active_color_index = idx;
        self.state.screens.gameover_state.active_color_index = idx;
        if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
            gs.set_color_indices(idx, idx);
        }
    }

    fn handle_screen_entry_on_fade(
        &mut self,
        prev: CurrentScreen,
        target: CurrentScreen,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        if matches!(prev, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && !matches!(target, CurrentScreen::Gameplay | CurrentScreen::Practice)
            && target != CurrentScreen::Evaluation
            && let Some(backend) = self.backend.as_mut()
        {
            self.dynamic_media
                .clear_gameplay_backgrounds(&mut self.asset_manager, backend);
        }
        if target == CurrentScreen::Practice {
            deadsync_audio_stream::stop_music();
            if let Some(mut po_state) = self.state.screens.player_options_state.take() {
                // Preserve the editor cursor/selection across a returning
                // PlayerOptions->Practice trip and across an in-place Practice
                // chart reload (Ctrl+Shift+R re-enters Practice from Practice).
                let edit_snapshot = ((prev == CurrentScreen::PlayerOptions
                    && po_state.return_screen == CurrentScreen::Practice)
                    || prev == CurrentScreen::Practice)
                    .then(|| {
                        self.state
                            .screens
                            .practice_state
                            .as_ref()
                            .map(practice::edit_snapshot)
                    })
                    .flatten();
                let song_arc = po_state.song.clone();
                let session = profile::get_session_snapshot();
                let play_style = session.play_style;
                let player_side = session.player_side;
                let chart_plan = gameplay_chart_entry_plan(
                    &song_arc,
                    po_state.chart_steps_index,
                    po_state.chart_difficulty_index,
                    play_style,
                    player_side,
                );
                let charts = chart_plan.charts;
                let chart_ixs = chart_plan.chart_indices;
                let resolved_steps_index = chart_plan.resolved_steps_index;
                let last_played_idx = chart_plan.last_played_index;

                let cfg = config::get();
                self.state.play_input_policy = InputRoutePolicy::from_config(&cfg);
                let global_offset_seconds = cfg.global_offset_seconds;
                let pack_sync_offset_seconds =
                    deadsync_simfile::runtime_cache::pack_sync_offset_for_song_config(
                        song_arc.as_ref(),
                        &cfg,
                    );
                let cabinet_light_plan = cabinet_light_plan(song_arc.as_ref(), chart_ixs[0]);
                let mut requested_chart_ixs = chart_ixs.to_vec();
                let light_payload_start = requested_chart_ixs.len();
                if let Some(plan) = cabinet_light_plan.as_ref() {
                    requested_chart_ixs.extend(plan.request_chart_ixs());
                }

                let payload_started = Instant::now();
                let gameplay_song = match song_loading::load_gameplay_charts(
                    song_arc.as_ref(),
                    &requested_chart_ixs,
                    global_offset_seconds,
                ) {
                    Ok(gameplay_song) => gameplay_song,
                    Err(e) => {
                        error!(
                            "Failed to load practice payload for '{}': {}",
                            song_arc.title, e
                        );
                        player_options::prewarm_noteskin_previews(&mut po_state);
                        self.commit_screen_change(CurrentScreen::PlayerOptions);
                        self.state.screens.player_options_state = Some(po_state);
                        return commands;
                    }
                };
                let gameplay_charts = [
                    Arc::new(gameplay_song[0].clone()),
                    Arc::new(gameplay_song[1].clone()),
                ];
                if let Some(plan) = cabinet_light_plan.as_ref() {
                    let (key, events) = cabinet_light_chart_from_loaded(
                        song_arc.as_ref(),
                        plan,
                        &gameplay_song[light_payload_start..],
                        global_offset_seconds,
                        pack_sync_offset_seconds,
                    );
                    self.gameplay_lights.set_cabinet_chart(key, events);
                }
                let payload_ms = payload_started.elapsed().as_secs_f64() * 1000.0;

                if play_style == profile_data::PlayStyle::Versus {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = po_state.chart_difficulty_index[0];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[0];
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = po_state.chart_difficulty_index[1];
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = resolved_steps_index[1];
                } else {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index =
                        po_state.chart_difficulty_index[last_played_idx];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[last_played_idx];
                }

                // Auto-switch CMod → the player's configured alternative for
                // no-cmod charts (this play only; the persisted profile is
                // untouched, so song select restores CMod).
                let scroll_speeds = player_options::apply_no_cmod_alternative(&mut po_state);

                let init_started = Instant::now();
                let mut gs = gameplay::init(
                    song_arc,
                    charts,
                    gameplay_charts,
                    gameplay_viewport(self.state.shell.metrics),
                    gameplay_session(),
                    gameplay_config_from_config(&cfg),
                    po_state.active_color_index,
                    po_state.music_rate,
                    scroll_speeds,
                    po_state.player_profiles,
                    None,
                    None,
                    Some(Arc::from("Practice Mode")),
                    Arc::from("PRACTICE MODE"),
                    Some(LeadInTiming {
                        min_seconds_to_step: 0.0,
                        min_seconds_to_music: 0.0,
                    }),
                    None,
                    None,
                    None,
                    None,
                    None,
                    [0; MAX_PLAYERS],
                );
                gs.disable_score_for_practice();
                let init_ms = init_started.elapsed().as_secs_f64() * 1000.0;
                let overlay_video_paths = gameplay_overlay_video_paths(
                    gs.song_lua_visuals(),
                    gs.song()
                        .active_foreground_path(gs.current_beat())
                        .map(|path| path.as_path()),
                );

                let sfx_prewarm_started = Instant::now();
                prewarm_gameplay_sfx(gs.song_lua_visuals(), &gs.song_lua_sound_paths);
                let sfx_prewarm_ms = sfx_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let asset_prewarm_started = Instant::now();
                if let Some(backend) = self.backend.as_mut() {
                    prewarm_gameplay_assets(
                        &mut self.asset_manager,
                        backend,
                        [
                            &gs.noteskin_assets.noteskin,
                            &gs.noteskin_assets.mine_noteskin,
                            &gs.noteskin_assets.receptor_noteskin,
                            &gs.noteskin_assets.tap_explosion_noteskin,
                        ],
                        gs.song(),
                        &gs.background_changes,
                        gs.song_lua_visuals(),
                    );
                    self.dynamic_media.set_gameplay_background_keys(
                        &mut self.asset_manager,
                        backend,
                        deadsync_assets::dynamic_media::gameplay_media_keys(
                            gs.song(),
                            &gs.background_changes,
                        ),
                    );
                    self.dynamic_media.sync_active_song_lua_videos(
                        &mut self.asset_manager,
                        backend,
                        &overlay_video_paths,
                    );
                    prewarm_gameplay_banners(
                        &mut self.dynamic_media,
                        &mut self.asset_manager,
                        backend,
                        &gs,
                        cfg.gameplay_banner_mode,
                    );
                }
                let asset_prewarm_ms = asset_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let text_prewarm_started = Instant::now();
                prewarm_gameplay_text_layout_cache(
                    &self.asset_manager,
                    &self.state.shell.metrics,
                    &mut self.gameplay_text_layout_cache,
                    &mut self.gameplay_compose_scratch,
                    &mut gs,
                    &cfg,
                );
                let text_prewarm_ms = text_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let song = gs.song();
                debug!(
                    "Practice transition timing: song='{}' payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3}",
                    song.title
                );
                commands.push(Command::SetPackBanner(gs.pack_banner_path.clone()));
                let show_video_backgrounds = cfg.show_video_backgrounds;
                let background_path =
                    Self::refresh_gameplay_background_path(&mut gs, show_video_backgrounds);
                commands.push(Command::SetDynamicBackground(background_path));
                let mut practice_state = practice::init(gs);
                if let Some(snapshot) = edit_snapshot {
                    practice::restore_edit_snapshot(&mut practice_state, snapshot);
                }
                self.state.screens.practice_state = Some(practice_state);
                if let Some(ps) = self.state.screens.practice_state.as_mut() {
                    crate::gameplay_runtime::enter_practice(ps);
                }
            } else {
                panic!("Navigating to Practice without PlayerOptions state!");
            }
        }
        if target == CurrentScreen::Gameplay {
            deadsync_audio_stream::stop_music();
            if prev != CurrentScreen::Gameplay {
                self.state.session.gameplay_restart_count = 0;
                self.state.session.restart_pending = false;
            }
            let mut course_display_carry = None;
            let course_display_totals = self
                .state
                .session
                .course_run
                .as_ref()
                .map(|course| course.course_display_totals);
            let course_display_timing = self
                .state
                .session
                .course_run
                .as_ref()
                .map(course_display_timing_for_run);
            if prev == CurrentScreen::Gameplay && self.state.session.course_run.is_some() {
                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                    crate::gameplay_runtime::exit(gs);
                }
            }
            if prev == CurrentScreen::Gameplay
                && self.state.session.course_run.is_some()
                && let Some(gameplay_results) = self.state.screens.gameplay_state.take()
            {
                self.update_combo_carry_from_gameplay(&gameplay_results);
                course_display_carry = Some(gameplay_results.course_display_carry());
                let color_idx = gameplay_results.active_color_index();
                Self::execute_evaluation_score_runtime(&gameplay_results);
                let init_view = Self::evaluation_init_view(&gameplay_results);
                let mut eval_state = evaluation::init(Some(gameplay_results), init_view);
                eval_state.active_color_index = color_idx;
                let _ = self.append_stage_results_from_eval(&eval_state);
            }

            let replay_pending =
                select_music::take_pending_replay(&mut self.state.screens.select_music_state);
            let replay_edges = replay_pending.as_ref().map(|payload| {
                payload
                    .replay
                    .iter()
                    .copied()
                    .map(|e| ReplayInputEdge {
                        lane_index: e.lane_index,
                        pressed: e.pressed,
                        source: e.source,
                        event_music_time_ns: e.event_music_time_ns,
                    })
                    .collect::<Vec<_>>()
            });
            let replay_offsets = replay_pending.as_ref().map(|payload| ReplayOffsetSnapshot {
                beat0_time_ns: payload.replay_beat0_time_ns,
            });
            let replay_status_text = replay_pending.as_ref().map(|payload| {
                Arc::<str>::from(format!(
                    "Autoplay - {} {:.2}%",
                    payload.name,
                    payload.score / 100.0
                ))
            });
            if let Some(mut po_state) = self.state.screens.player_options_state.take() {
                let song_arc = po_state.song.clone();
                let session = profile::get_session_snapshot();
                let play_style = session.play_style;
                let player_side = session.player_side;
                let chart_plan = gameplay_chart_entry_plan(
                    &song_arc,
                    po_state.chart_steps_index,
                    po_state.chart_difficulty_index,
                    play_style,
                    player_side,
                );
                let last_played_commands = gameplay_last_played_commands(
                    song_arc.as_ref(),
                    &chart_plan,
                    po_state.chart_difficulty_index,
                    play_style,
                    player_side,
                );
                let charts = chart_plan.charts;
                let chart_ixs = chart_plan.chart_indices;
                let resolved_steps_index = chart_plan.resolved_steps_index;
                let last_played_idx = chart_plan.last_played_index;

                let gameplay_entry_started = Instant::now();
                let cfg = config::get();
                self.state.play_input_policy = InputRoutePolicy::from_config(&cfg);
                let global_offset_seconds = cfg.global_offset_seconds;
                let pack_sync_offset_seconds =
                    deadsync_simfile::runtime_cache::pack_sync_offset_for_song_config(
                        song_arc.as_ref(),
                        &cfg,
                    );
                let cabinet_light_plan = cabinet_light_plan(song_arc.as_ref(), chart_ixs[0]);
                let cabinet_light_key = cabinet_light_plan.as_ref().map(|plan| {
                    cabinet_light_key(
                        song_arc.as_ref(),
                        plan,
                        global_offset_seconds,
                        pack_sync_offset_seconds,
                    )
                });
                let reused_gameplay_charts =
                    if prev == CurrentScreen::Gameplay && self.state.session.course_run.is_none() {
                        self.state
                            .screens
                            .gameplay_state
                            .as_ref()
                            .filter(|current| {
                                deadsync_simfile::runtime_cache::can_reuse_quick_restart_payload(
                                    current.song(),
                                    [
                                        current.charts()[0].short_hash.as_str(),
                                        current.charts()[1].short_hash.as_str(),
                                    ],
                                    song_arc.as_ref(),
                                    [charts[0].short_hash.as_str(), charts[1].short_hash.as_str()],
                                )
                            })
                            .map(|current| current.gameplay_charts().clone())
                    } else {
                        None
                    };
                let reusing_gameplay_payload = reused_gameplay_charts.is_some();
                let payload_started = Instant::now();
                let gameplay_charts = if let Some(gameplay_charts) = reused_gameplay_charts {
                    debug!(
                        "Reusing gameplay payload for quick restart '{}'",
                        song_arc.title
                    );
                    match (cabinet_light_plan.as_ref(), cabinet_light_key.as_ref()) {
                        (Some(_), Some(key)) if self.gameplay_lights.cabinet_key_matches(key) => {
                            self.gameplay_lights.restart_cabinet_chart();
                        }
                        (Some(plan), _) => match load_cabinet_light_chart(
                            song_arc.as_ref(),
                            plan,
                            global_offset_seconds,
                            pack_sync_offset_seconds,
                        ) {
                            Ok((key, events)) => {
                                self.gameplay_lights.set_cabinet_chart(key, events)
                            }
                            Err(error) => {
                                warn!(
                                    "Failed to load cabinet light chart for '{}': {}",
                                    song_arc.title, error
                                );
                                self.gameplay_lights.clear();
                            }
                        },
                        _ => self.gameplay_lights.clear(),
                    }
                    gameplay_charts
                } else {
                    let mut requested_chart_ixs = chart_ixs.to_vec();
                    let light_payload_start = requested_chart_ixs.len();
                    if let Some(plan) = cabinet_light_plan.as_ref() {
                        requested_chart_ixs.extend(plan.request_chart_ixs());
                    }
                    let gameplay_song = match song_loading::load_gameplay_charts(
                        song_arc.as_ref(),
                        &requested_chart_ixs,
                        global_offset_seconds,
                    ) {
                        Ok(gameplay_song) => gameplay_song,
                        Err(e) => {
                            error!(
                                "Failed to load gameplay payload for '{}': {}",
                                song_arc.title, e
                            );
                            player_options::prewarm_noteskin_previews(&mut po_state);
                            self.commit_screen_change(CurrentScreen::PlayerOptions);
                            self.state.screens.player_options_state = Some(po_state);
                            return commands;
                        }
                    };
                    let gameplay_charts = [
                        Arc::new(gameplay_song[0].clone()),
                        Arc::new(gameplay_song[1].clone()),
                    ];
                    if let Some(plan) = cabinet_light_plan.as_ref() {
                        let (key, events) = cabinet_light_chart_from_loaded(
                            song_arc.as_ref(),
                            plan,
                            &gameplay_song[light_payload_start..],
                            global_offset_seconds,
                            pack_sync_offset_seconds,
                        );
                        self.gameplay_lights.set_cabinet_chart(key, events);
                    } else {
                        self.gameplay_lights.clear();
                    }
                    gameplay_charts
                };
                let payload_ms = payload_started.elapsed().as_secs_f64() * 1000.0;

                // Keep SelectMusic's current stepchart in sync with what we're about to play.
                if play_style == profile_data::PlayStyle::Versus {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = po_state.chart_difficulty_index[0];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[0];
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = po_state.chart_difficulty_index[1];
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = resolved_steps_index[1];
                } else {
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index =
                        po_state.chart_difficulty_index[last_played_idx];
                    self.state.screens.select_music_state.selected_steps_index =
                        resolved_steps_index[last_played_idx];
                }

                // Auto-switch CMod → the player's configured alternative for
                // no-cmod charts (this play only; the persisted profile is
                // untouched, so song select restores CMod). Replays must
                // reproduce the recorded scroll speed, so skip the swap there.
                commands.extend(last_played_commands);
                let scroll_speeds = if replay_pending.is_none() {
                    player_options::apply_no_cmod_alternative(&mut po_state)
                } else {
                    let to_scroll_speed = |m: &player_options::SpeedMod| match m.mod_type {
                        player_options::SpeedModType::X => ScrollSpeedSetting::XMod(m.value),
                        player_options::SpeedModType::C => ScrollSpeedSetting::CMod(m.value),
                        player_options::SpeedModType::M => ScrollSpeedSetting::MMod(m.value),
                    };
                    [
                        to_scroll_speed(&po_state.speed_mod[0]),
                        to_scroll_speed(&po_state.speed_mod[1]),
                    ]
                };

                let color_index = po_state.active_color_index;
                let lead_in_timing = self.state.session.course_run.as_ref().and_then(|course| {
                    (course.next_stage_index > 0).then_some(LeadInTiming {
                        min_seconds_to_step: COURSE_MIN_SECONDS_TO_STEP_NEXT_SONG,
                        min_seconds_to_music: COURSE_MIN_SECONDS_TO_MUSIC_NEXT_SONG,
                    })
                });
                let (course_display_info, course_banner_path) = self
                    .state
                    .session
                    .course_run
                    .as_ref()
                    .map_or((None, None), |course| {
                        (
                            Some(gameplay::CourseDisplayInfo {
                                name: Arc::from(course.name.as_str()),
                            }),
                            course.banner_path.clone(),
                        )
                    });
                let stage_intro_text: Arc<str> = if let Some(course) =
                    self.state.session.course_run.as_ref()
                {
                    let stage_num = course.next_stage_index.saturating_add(1);
                    let total = course.stages.len().max(1);
                    Arc::from(format!("STAGE {stage_num} / {total}"))
                } else if cfg.keyboard_features && self.state.session.gameplay_restart_count > 0 {
                    Arc::from(format!(
                        "RESTART {}",
                        self.state.session.gameplay_restart_count
                    ))
                } else {
                    deadsync_simfile::event_intro::gameplay_event_intro_text(song_arc.as_ref())
                };
                let combo_carry = self.state.session.combo_carry;
                let init_started = Instant::now();
                let mut gs = gameplay::init(
                    song_arc,
                    charts,
                    gameplay_charts,
                    gameplay_viewport(self.state.shell.metrics),
                    gameplay_session(),
                    gameplay_config_from_config(&cfg),
                    color_index,
                    po_state.music_rate,
                    scroll_speeds,
                    po_state.player_profiles,
                    replay_edges,
                    replay_offsets,
                    replay_status_text,
                    stage_intro_text,
                    lead_in_timing,
                    course_display_carry,
                    course_display_totals,
                    course_display_timing,
                    course_display_info,
                    course_banner_path,
                    combo_carry,
                );
                let init_ms = init_started.elapsed().as_secs_f64() * 1000.0;
                let overlay_video_paths = gameplay_overlay_video_paths(
                    gs.song_lua_visuals(),
                    gs.song()
                        .active_foreground_path(gs.current_beat())
                        .map(|path| path.as_path()),
                );

                let sfx_prewarm_started = Instant::now();
                prewarm_gameplay_sfx(gs.song_lua_visuals(), &gs.song_lua_sound_paths);
                let sfx_prewarm_ms = sfx_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let asset_prewarm_started = Instant::now();
                if let Some(backend) = self.backend.as_mut() {
                    prewarm_gameplay_assets(
                        &mut self.asset_manager,
                        backend,
                        [
                            &gs.noteskin_assets.noteskin,
                            &gs.noteskin_assets.mine_noteskin,
                            &gs.noteskin_assets.receptor_noteskin,
                            &gs.noteskin_assets.tap_explosion_noteskin,
                        ],
                        gs.song(),
                        &gs.background_changes,
                        gs.song_lua_visuals(),
                    );
                    self.dynamic_media.set_gameplay_background_keys(
                        &mut self.asset_manager,
                        backend,
                        deadsync_assets::dynamic_media::gameplay_media_keys(
                            gs.song(),
                            &gs.background_changes,
                        ),
                    );
                    self.dynamic_media.sync_active_song_lua_videos(
                        &mut self.asset_manager,
                        backend,
                        &overlay_video_paths,
                    );
                    prewarm_gameplay_banners(
                        &mut self.dynamic_media,
                        &mut self.asset_manager,
                        backend,
                        &gs,
                        cfg.gameplay_banner_mode,
                    );
                }
                let asset_prewarm_ms = asset_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let text_prewarm_started = Instant::now();
                prewarm_gameplay_text_layout_cache(
                    &self.asset_manager,
                    &self.state.shell.metrics,
                    &mut self.gameplay_text_layout_cache,
                    &mut self.gameplay_compose_scratch,
                    &mut gs,
                    &cfg,
                );
                let text_prewarm_ms = text_prewarm_started.elapsed().as_secs_f64() * 1000.0;
                let total_ms = gameplay_entry_started.elapsed().as_secs_f64() * 1000.0;
                let song = gs.song();
                if total_ms >= 50.0 {
                    info!(
                        "Gameplay transition timing: song='{}' restart={} payload_source={} payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3} elapsed_ms={total_ms:.3}",
                        song.title,
                        prev == CurrentScreen::Gameplay,
                        if reusing_gameplay_payload {
                            "reuse"
                        } else {
                            "load"
                        },
                    );
                } else {
                    debug!(
                        "Gameplay transition timing: song='{}' restart={} payload_source={} payload_ms={payload_ms:.3} init_ms={init_ms:.3} sfx_prewarm_ms={sfx_prewarm_ms:.3} asset_prewarm_ms={asset_prewarm_ms:.3} text_prewarm_ms={text_prewarm_ms:.3} elapsed_ms={total_ms:.3}",
                        song.title,
                        prev == CurrentScreen::Gameplay,
                        if reusing_gameplay_payload {
                            "reuse"
                        } else {
                            "load"
                        },
                    );
                }
                commands.push(Command::SetPackBanner(gs.pack_banner_path.clone()));
                let show_video_backgrounds = cfg.show_video_backgrounds;
                let background_path =
                    Self::refresh_gameplay_background_path(&mut gs, show_video_backgrounds);
                commands.push(Command::SetDynamicBackground(background_path));
                self.state.screens.gameplay_state = Some(gs);
                if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                    gameplay::sync_lobby_runtime_view(gs, Self::refresh_lobby_runtime_view());
                    crate::gameplay_runtime::enter(gs, self.state.play_input_policy.smx_input);
                }
                // Song Start / Restart SFX (zmod parity, issue #375). At this
                // point `gameplay_restart_count` has already been zeroed for
                // fresh entries (line above) and preserved for in-screen
                // restarts (`try_gameplay_restart` incremented it before we
                // arrived).
                let restart_count = self.state.session.gameplay_restart_count;
                if restart_count == 0 {
                    deadsync_assets::audio_folder::play_random_sfx("assets/sounds/song_start");
                } else {
                    deadsync_assets::audio_folder::play_indexed_sfx(
                        "assets/sounds/song_start/restart",
                        restart_count,
                        "restart.ogg",
                    );
                }
                if let Some(course) = self.state.session.course_run.as_mut() {
                    course.next_stage_index = course.next_stage_index.saturating_add(1);
                }
            } else {
                panic!("Navigating to Gameplay without PlayerOptions state!");
            }
        }

        if target == CurrentScreen::Evaluation {
            if let Some(gs) = self.state.screens.gameplay_state.as_mut() {
                crate::gameplay_runtime::exit(gs);
            }
            let gameplay_results = self.state.screens.gameplay_state.take();
            if let Some(gs) = gameplay_results.as_ref() {
                self.update_combo_carry_from_gameplay(gs);
            }
            if let (Some(backend), Some(gs)) = (self.backend.as_mut(), gameplay_results.as_ref())
                && let Some(path) = gs.song().banner_path.as_ref()
            {
                media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
            }
            let color_idx = gameplay_results.as_ref().map_or(
                self.state.screens.evaluation_state.active_color_index,
                |gs| gs.gameplay.active_color_index(),
            );
            if let Some(gameplay) = gameplay_results.as_ref() {
                Self::execute_evaluation_score_runtime(gameplay);
            }
            let init_view = gameplay_results
                .as_ref()
                .map(Self::evaluation_init_view)
                .unwrap_or_default();
            self.state.screens.evaluation_state = evaluation::init(gameplay_results, init_view);
            self.state.screens.evaluation_state.active_color_index = color_idx;
            self.state.screens.evaluation_state.return_to_course =
                self.state.session.course_run.is_some();
            self.state.screens.evaluation_state.auto_advance_seconds = None;
            if let Some(start) = self.state.session.session_start_time {
                self.state.screens.evaluation_state.session_elapsed =
                    Instant::now().duration_since(start).as_secs_f32();
            }
            self.state.screens.evaluation_state.gameplay_elapsed =
                stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);
            self.finalize_entered_evaluation();
            let view = Self::evaluation_runtime_view(&self.state.screens.evaluation_state);
            evaluation::sync_runtime_view(&mut self.state.screens.evaluation_state, view);
        }

        if target == CurrentScreen::EvaluationSummary {
            let color_idx = match prev {
                CurrentScreen::SelectMusic => {
                    self.state.screens.select_music_state.active_color_index
                }
                CurrentScreen::SelectCourse => {
                    self.state.screens.select_course_state.active_color_index
                }
                CurrentScreen::Evaluation => self.state.screens.evaluation_state.active_color_index,
                _ => {
                    self.state
                        .screens
                        .evaluation_summary_state
                        .active_color_index
                }
            };
            let return_to = evaluation_summary_return_to(
                prev,
                std::mem::take(&mut self.state.session.pending_post_select_summary_exit),
            );
            self.state.screens.evaluation_summary_state =
                evaluation_summary::init_for_return(return_to);
            self.state
                .screens
                .evaluation_summary_state
                .active_color_index = color_idx;

            let display_stages = self
                .post_select_display_stages(config::get().show_course_individual_scores)
                .into_owned();
            if let Some(backend) = self.backend.as_mut() {
                for stage in display_stages.iter() {
                    if let Some(path) = stage.song.banner_path.as_ref() {
                        media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
                    }
                }
            }
        }

        if target == CurrentScreen::Initials {
            let color_idx = match prev {
                CurrentScreen::EvaluationSummary => {
                    self.state
                        .screens
                        .evaluation_summary_state
                        .active_color_index
                }
                CurrentScreen::SelectMusic => {
                    self.state.screens.select_music_state.active_color_index
                }
                CurrentScreen::SelectCourse => {
                    self.state.screens.select_course_state.active_color_index
                }
                CurrentScreen::Evaluation => self.state.screens.evaluation_state.active_color_index,
                _ => self.state.screens.initials_state.active_color_index,
            };
            self.state.screens.initials_state = initials::init();
            self.state.screens.initials_state.active_color_index = color_idx;
            let display_stages = self
                .post_select_display_stages(config::get().show_course_individual_scores)
                .into_owned();
            initials::set_highscore_lists(&mut self.state.screens.initials_state, &display_stages);

            if let Some(backend) = self.backend.as_mut() {
                for stage in display_stages.iter() {
                    if let Some(path) = stage.song.banner_path.as_ref() {
                        media_cache::ensure_banner_texture(&mut self.asset_manager, backend, path);
                    }
                }
            }
        }

        if target == CurrentScreen::GameOver {
            let color_idx = match prev {
                CurrentScreen::Initials => self.state.screens.initials_state.active_color_index,
                CurrentScreen::EvaluationSummary => {
                    self.state
                        .screens
                        .evaluation_summary_state
                        .active_color_index
                }
                CurrentScreen::SelectMusic => {
                    self.state.screens.select_music_state.active_color_index
                }
                CurrentScreen::SelectCourse => {
                    self.state.screens.select_course_state.active_color_index
                }
                CurrentScreen::Evaluation => self.state.screens.evaluation_state.active_color_index,
                _ => self.state.screens.gameover_state.active_color_index,
            };
            self.state.screens.gameover_state = gameover::init();
            self.state.screens.gameover_state.active_color_index = color_idx;
        }

        if target == CurrentScreen::SelectMusic {
            self.state.session.clear_course_runtime();
            self.begin_play_session();
            let profile_session = profile::get_session_snapshot();

            match prev {
                CurrentScreen::PlayerOptions => {
                    let preferred = self.state.session.preferred_difficulty_index;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = preferred;

                    if let Some(po) = self.state.screens.player_options_state.as_ref() {
                        match profile_session.play_style {
                            profile_data::PlayStyle::Versus => {
                                self.state.screens.select_music_state.selected_steps_index =
                                    po.chart_steps_index[0];
                                self.state
                                    .screens
                                    .select_music_state
                                    .p2_selected_steps_index = po.chart_steps_index[1];
                                self.state
                                    .screens
                                    .select_music_state
                                    .preferred_difficulty_index = po.chart_difficulty_index[0];
                                self.state
                                    .screens
                                    .select_music_state
                                    .p2_preferred_difficulty_index = po.chart_difficulty_index[1];
                            }
                            profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
                                let idx =
                                    profile_data::player_side_index(profile_session.player_side);
                                self.state.screens.select_music_state.selected_steps_index =
                                    po.chart_steps_index[idx];
                                self.state
                                    .screens
                                    .select_music_state
                                    .preferred_difficulty_index = po.chart_difficulty_index[idx];
                            }
                        }
                    }

                    let desired_steps_index =
                        self.state.screens.select_music_state.selected_steps_index;

                    if let Some(select_music::MusicWheelEntry::Song(song)) = self
                        .state
                        .screens
                        .select_music_state
                        .entries
                        .get(self.state.screens.select_music_state.selected_index)
                    {
                        let chart_type = profile_session.play_style.chart_type();
                        if song
                            .chart_for_steps_index(chart_type, desired_steps_index)
                            .is_none()
                        {
                            let mut best_match_index = None;
                            let mut min_diff = i32::MAX;
                            for i in 0..STANDARD_DIFFICULTY_COUNT {
                                if select_music::is_difficulty_playable(song, i) {
                                    let diff = (i as i32 - preferred as i32).abs();
                                    if diff < min_diff {
                                        min_diff = diff;
                                        best_match_index = Some(i);
                                    }
                                }
                            }
                            if let Some(idx) = best_match_index {
                                self.state.screens.select_music_state.selected_steps_index = idx;
                            }
                        }
                    }

                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                CurrentScreen::Gameplay | CurrentScreen::Practice | CurrentScreen::Evaluation => {
                    select_music::reset_preview_after_gameplay(
                        &mut self.state.screens.select_music_state,
                        crate::select_music::history_view(),
                    );
                }
                CurrentScreen::EvaluationSummary => {
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                CurrentScreen::ProfileLoad => {
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
                _ => {
                    let current_color_index =
                        self.state.screens.select_music_state.active_color_index;
                    self.state.screens.select_music_state =
                        select_music::init(crate::select_music::prepared_init_view());
                    self.state.screens.select_music_state.active_color_index = current_color_index;
                    let preferred = self.state.session.preferred_difficulty_index;
                    self.state.screens.select_music_state.selected_steps_index = preferred;
                    self.state
                        .screens
                        .select_music_state
                        .preferred_difficulty_index = preferred;

                    let p2_pref = profile::preferred_difficulty_for_side(
                        profile_data::PlayerSide::P2,
                        profile_session.play_style,
                    );
                    self.state
                        .screens
                        .select_music_state
                        .p2_selected_steps_index = p2_pref;
                    self.state
                        .screens
                        .select_music_state
                        .p2_preferred_difficulty_index = p2_pref;

                    // Treat the initial selection as already "settled" so preview/graphs can start
                    // immediately after the transition, matching ITG/Simply Love behavior.
                    select_music::trigger_immediate_refresh(
                        &mut self.state.screens.select_music_state,
                    );
                }
            }
            self.state.screens.select_music_state.gameplay_elapsed =
                stage_stats::total_stage_duration_seconds(&self.state.session.played_stages);

            // Prime the delayed panes (tech counts, breakdown, etc.) for the selected chart so they
            // render immediately on entry (no initial debounce delay).
            select_music::prime_displayed_chart_data(&mut self.state.screens.select_music_state);

            // Load the selected entry's banner during the fade-in so it appears immediately.
            let banner_path = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.banner_path.clone(),
                Some(select_music::MusicWheelEntry::PackHeader { banner_path, .. }) => {
                    banner_path.clone()
                }
                None => None,
            };
            commands.push(Command::SetBanner(banner_path));
            let cdtitle_path = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.cdtitle_path.clone(),
                _ => None,
            };
            commands.push(Command::SetCdTitle(cdtitle_path));

            // Pre-render the density graph during the fade-in so the panel isn't blank on entry.
            let chart_to_graph = match self
                .state
                .screens
                .select_music_state
                .entries
                .get(self.state.screens.select_music_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => {
                    let chart_type = profile_session.play_style.chart_type();
                    song.chart_for_steps_index(
                        chart_type,
                        self.state.screens.select_music_state.selected_steps_index,
                    )
                    .map(|c| DensityGraphSource {
                        max_nps: c.max_nps,
                        measure_nps_vec: c.measure_nps_vec.clone(),
                        measure_seconds_vec: c.measure_seconds_vec.clone(),
                        first_second: c.first_second,
                        last_second: song.precise_last_second(),
                    })
                }
                _ => None,
            };
            commands.push(Command::SetDensityGraph {
                slot: DensityGraphSlot::SelectMusicP1,
                chart_opt: chart_to_graph,
            });

            if profile_session.play_style == profile_data::PlayStyle::Versus {
                let chart_to_graph_p2 = match self
                    .state
                    .screens
                    .select_music_state
                    .entries
                    .get(self.state.screens.select_music_state.selected_index)
                {
                    Some(select_music::MusicWheelEntry::Song(song)) => {
                        let chart_type = profile_session.play_style.chart_type();
                        song.chart_for_steps_index(
                            chart_type,
                            self.state
                                .screens
                                .select_music_state
                                .p2_selected_steps_index,
                        )
                        .map(|c| DensityGraphSource {
                            max_nps: c.max_nps,
                            measure_nps_vec: c.measure_nps_vec.clone(),
                            measure_seconds_vec: c.measure_seconds_vec.clone(),
                            first_second: c.first_second,
                            last_second: song.precise_last_second(),
                        })
                    }
                    _ => None,
                };
                commands.push(Command::SetDensityGraph {
                    slot: DensityGraphSlot::SelectMusicP2,
                    chart_opt: chart_to_graph_p2,
                });
            }

            // PlayerOptions state is tied to a specific song selection; once we're back on
            // SelectMusic, drop it so direct Gameplay starts cannot reuse stale song data.
            self.state.screens.player_options_state = None;
        }

        if target == CurrentScreen::SelectCourse {
            let restore_course_selection = self
                .state
                .session
                .course_run
                .as_ref()
                .map(|course| {
                    (
                        course.path.clone(),
                        Some(course.course_difficulty_name.clone()),
                    )
                })
                .or_else(|| {
                    self.state
                        .session
                        .last_course_wheel_path
                        .as_ref()
                        .map(|path| {
                            (
                                path.clone(),
                                self.state.session.last_course_wheel_difficulty_name.clone(),
                            )
                        })
                });
            self.state.session.clear_course_runtime();
            self.begin_play_session();

            match prev {
                CurrentScreen::ProfileLoad | CurrentScreen::EvaluationSummary => {
                    select_course::trigger_immediate_refresh(
                        &mut self.state.screens.select_course_state,
                    );
                }
                _ => {
                    let current_color_index =
                        self.state.screens.select_course_state.active_color_index;
                    self.state.screens.select_course_state =
                        select_course::init(crate::profile_load::select_course_init_view());
                    self.state.screens.select_course_state.active_color_index = current_color_index;
                    select_course::trigger_immediate_refresh(
                        &mut self.state.screens.select_course_state,
                    );
                }
            }
            if let Some((course_path, course_diff_name)) = restore_course_selection.as_ref() {
                select_course::restore_selection_for_course(
                    &mut self.state.screens.select_course_state,
                    course_path.as_path(),
                    course_diff_name.as_deref(),
                );
            }

            let banner_path = match self
                .state
                .screens
                .select_course_state
                .entries
                .get(self.state.screens.select_course_state.selected_index)
            {
                Some(select_music::MusicWheelEntry::Song(song)) => song.banner_path.clone(),
                Some(select_music::MusicWheelEntry::PackHeader { banner_path, .. }) => {
                    banner_path.clone()
                }
                None => None,
            };
            commands.push(Command::SetBanner(banner_path));
            commands.push(Command::SetCdTitle(None));
        }
        if prev == CurrentScreen::PlayerOptions && target != CurrentScreen::PlayerOptions {
            self.state.screens.player_options_state = None;
            deadsync_assets::noteskin::clear_itg_runtime_caches();
        }
        commands
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, _cause: StartCause) {
        self.state
            .shell
            .gameplay_input_trace
            .note_new_events(Instant::now());
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GamepadSystem(ev) => {
                let plan = gamepad_system_event_plan(self.state.screens.current_screen, &ev);
                if plan.forward_to_sandbox {
                    let view = gamepad_system_view(&ev);
                    screens::sandbox::handle_gamepad_system_event(
                        &mut self.state.screens.sandbox_state,
                        &view,
                    );
                }
                if let Some(message) = plan.log_message {
                    debug!("{message}");
                }
                if plan.refresh_smx_underglow {
                    crate::smx_config::apply_smx_underglow();
                }
                if let Some(message) = plan.user_message {
                    self.state
                        .shell
                        .interaction
                        .show_message(message, Instant::now());
                }
            }
            UserEvent::Pad(ev) => {
                if !self.accepts_live_input() {
                    return;
                }
                let gameplay_screen = self.state.screens.current_screen == CurrentScreen::Gameplay;
                let handled_started = Instant::now();
                let mut raw_pad_consumed = false;
                match raw_pad_screen_route(self.state.screens.current_screen) {
                    RawPadScreenRoute::Sandbox => {
                        screens::sandbox::handle_raw_pad_event(
                            &mut self.state.screens.sandbox_state,
                            &ev,
                        );
                    }
                    RawPadScreenRoute::Mappings => {
                        let (consumed, action) = screens::mappings::handle_raw_pad_event(
                            &mut self.state.screens.mappings_state,
                            &ev,
                        );
                        raw_pad_consumed = consumed;
                        if !matches!(action, ThemeEffect::None)
                            && let Err(e) = self.handle_action(action, event_loop)
                        {
                            log::error!("Failed to handle Mappings raw pad action: {e}");
                        }
                    }
                    RawPadScreenRoute::Input => {
                        screens::input::handle_raw_pad_event(
                            &mut self.state.screens.input_state,
                            &ev,
                        );
                    }
                    RawPadScreenRoute::SelectMusic => {
                        screens::select_music::handle_raw_pad_event(
                            &mut self.state.screens.select_music_state,
                            &ev,
                        );
                    }
                    RawPadScreenRoute::Evaluation => {
                        screens::evaluation::handle_raw_pad_event(
                            &mut self.state.screens.evaluation_state,
                            &ev,
                        );
                    }
                    RawPadScreenRoute::None => {}
                }
                if !raw_pad_consumed {
                    self.handle_pad_event(event_loop, ev);
                }
                self.state
                    .shell
                    .gameplay_input_trace
                    .note_pad_handler(gameplay_screen, elapsed_us_since(handled_started));
            }
            UserEvent::Key(ev) => {
                if !self.accepts_live_input() {
                    return;
                }
                self.handle_live_key_event(event_loop, ev);
            }
        }
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(e) = self.init_graphics(event_loop) {
                error!("Failed to initialize graphics: {e}");
                event_loop.exit();
            }
            // After all initial loading is complete, start network checks.
            deadsync_online::runtime::init();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window_id != window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                info!("Close requested. Shutting down.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                let now = Instant::now();
                let plan = apply_shell_surface_active(
                    &mut self.state.shell,
                    new_size.width > 0 && new_size.height > 0,
                    now,
                );
                if plan.sync_gameplay_capture {
                    self.sync_gameplay_input_capture();
                }
                if plan.changed {
                    debug!(
                        "Window surface state changed: active={} size={}x{} screen={:?}",
                        self.state.shell.frame_loop.surface_active(),
                        new_size.width,
                        new_size.height,
                        self.state.screens.current_screen
                    );
                }
                self.sync_window_size(new_size);
                if let Some(reason) = plan.redraw_reason {
                    self.request_redraw(&window, reason);
                }
            }
            WindowEvent::Focused(focused) => {
                #[cfg(target_os = "windows")]
                match exclusive_fullscreen_focus_plan(
                    self.state.shell.display_mode,
                    focused,
                    window.is_minimized().unwrap_or(false),
                ) {
                    WindowMinimizePlan::Minimize => {
                        window.set_minimized(true);
                    }
                    WindowMinimizePlan::Restore => {
                        window.set_minimized(false);
                    }
                    WindowMinimizePlan::None => {}
                }
                self.apply_window_focus_change(focused, Instant::now(), Some(&window));
            }
            WindowEvent::Occluded(occluded) => {
                let plan =
                    apply_shell_window_occlusion(&mut self.state.shell, occluded, Instant::now());
                if plan.sync_gameplay_capture {
                    self.sync_gameplay_input_capture();
                }
                if plan.changed {
                    debug!(
                        "Window occlusion changed: occluded={} screen={:?}",
                        occluded, self.state.screens.current_screen
                    );
                }
                if let Some(reason) = plan.redraw_reason {
                    self.request_redraw(&window, reason);
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if !self.accepts_live_input() {
                    return;
                }
                if key_event.state == winit::event::ElementState::Pressed
                    && let Some(text) = key_event.text.as_deref()
                {
                    self.handle_key_text(event_loop, text);
                }
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                self.handle_unix_window_keyboard_fallback(event_loop, &key_event);
            }
            WindowEvent::RedrawRequested => {
                let redraw_started = Instant::now();
                let (request_to_redraw_us, redraw_request_reason) =
                    self.state.shell.take_redraw_request_timing(redraw_started);
                self.run_frame(
                    event_loop,
                    window,
                    redraw_started,
                    request_to_redraw_us,
                    redraw_request_reason,
                );
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.clone() else {
            return;
        };
        self.state
            .shell
            .gameplay_input_trace
            .finish_batch(Instant::now(), self.state.screens.current_screen);
        self.sync_gameplay_input_capture();
        match self.flush_due_input_events(event_loop) {
            Ok(true) => self.request_redraw(&window, "input_debounce"),
            Ok(false) => {}
            Err(e) => {
                error!("Failed to handle debounced input before wait: {e}");
                event_loop.exit();
                return;
            }
        }
        let interval_state = self.redraw_interval_state(&window);
        let plan = self
            .state
            .shell
            .frame_loop
            .plan_wait(Instant::now(), interval_state);
        self.log_frame_loop_mode(plan.mode);
        if let Some(reason) = plan.redraw_reason {
            self.request_redraw_if_needed(&window, reason);
        }
        match plan.control {
            FrameWaitControl::Poll => event_loop.set_control_flow(ControlFlow::Poll),
            FrameWaitControl::Wait => event_loop.set_control_flow(ControlFlow::Wait),
            FrameWaitControl::WaitUntil(deadline) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        config::flush_pending_saves();
        if let Some(backend) = &mut self.backend {
            self.dynamic_media
                .destroy_assets(&mut self.asset_manager, backend);
            let mut textures = self.asset_manager.take_textures();
            backend.dispose_textures(&mut textures);
            backend.cleanup();
        }
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::get();
    let backend_type = config.video_renderer;
    let show_stats_mode = config.show_stats_mode.min(2);
    let color_index = config.simply_love_color;
    let profile_data = profile::get();
    let event_loop: EventLoop<UserEvent> = EventLoop::<UserEvent>::with_user_event().build()?;
    let mut app = App::new(
        backend_type,
        show_stats_mode,
        color_index,
        config,
        profile_data,
    );

    // Spawn background input backend threads; all input stays decoupled from frame rate.
    let proxy = event_loop.create_proxy();
    // Raw input backends default to "unfocused" until init_graphics seeds the
    // real focus state from the created window. This prevents global keyboard
    // input (e.g. Win32 RawInput RIDEV_INPUTSINK, evdev, IOHID) from being
    // routed into the game while it is launched into the background.
    app.sync_gameplay_input_capture();
    let (smx_p1_serial, smx_p2_serial) = config::smx_pad_assignment();
    launch_input_backends(
        proxy,
        InputBackendConfig {
            windows_pad_backend: config.windows_gamepad_backend,
            smx_input: config.smx_input,
            smx_p1_serial,
            smx_p2_serial,
        },
    );
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};

    #[test]
    fn transition_gameplay_keeps_only_lobby_runtime_effects() {
        let effect = ThemeEffect::Batch(vec![
            ThemeEffect::Navigate(CurrentScreen::Evaluation),
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Online(
                SimplyLoveOnlineRequest::Lobby(SimplyLoveLobbyRequest::Disconnect),
            )),
        ]);

        assert!(matches!(
            lobby_effect_only(effect),
            Some(ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Online(
                SimplyLoveOnlineRequest::Lobby(SimplyLoveLobbyRequest::Disconnect)
            )))
        ));
    }

    fn test_chart(hash: &str) -> ChartData {
        test_chart_with("dance-single", "Hard", hash)
    }

    fn test_chart_with(chart_type: &str, difficulty: &str, hash: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 9,
            step_artist: String::new(),
            music_path: None,
            short_hash: hash.to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn test_song(path: &str, offset: f32, hashes: [&str; 2]) -> SongData {
        SongData {
            simfile_path: PathBuf::from(path),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
            normalized_bpms: "120.000".to_string(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: vec![test_chart(hashes[0]), test_chart(hashes[1])],
        }
    }

    #[test]
    fn music_preferences_snap_missing_medium() {
        let chart_type = profile::get_session_play_style().chart_type();
        let mut song = test_song("song.ssc", 0.0, ["hard", "unused"]);
        song.charts = vec![test_chart_with(chart_type, "Hard", "hard")];

        let mut state = select_music::init_placeholder();
        state.entries = vec![select_music::MusicWheelEntry::Song(Arc::new(song))];
        apply_music_preferences(&mut state, 2, 2);

        assert_eq!(state.selected_steps_index, 3);
        assert_eq!(state.p2_selected_steps_index, 3);
        assert_eq!(state.preferred_difficulty_index, 2);
    }

    fn test_score_info(
        song: Arc<SongData>,
        side: profile_data::PlayerSide,
        hash: &str,
        speed_mod: ScrollSpeedSetting,
        music_rate: f32,
    ) -> evaluation::ScoreInfo {
        evaluation::ScoreInfo {
            song: song.clone(),
            chart: Arc::new(test_chart(hash)),
            course_graph_stages: Vec::new(),
            side,
            profile_name: String::new(),
            score_valid: true,
            disqualified: false,
            expected_groovestats_submit: false,
            expected_arrowcloud_submit: false,
            groovestats: score_data::GrooveStatsEvalState::default(),
            itl: score_data::ItlEvalState::default(),
            judgment_counts: [0; judgment_rules::JUDGE_GRADE_COUNT],
            score_percent: 0.0,
            earned_grade_points: 0,
            possible_grade_points: 0,
            grade: score_data::Grade::Tier01,
            speed_mod,
            mods_text: {
                let profile = profile::get_for_side(side);
                profile_data::evaluation_mods_text(&profile, speed_mod)
            },
            hands_achieved: 0,
            hands_total: 0,
            holds_held: 0,
            holds_held_for_score: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_held_for_score: 0,
            rolls_total: 0,
            mines_hit_for_score: 0,
            mines_avoided: 0,
            mines_total: 0,
            timing: timing_rules::TimingStats::default(),
            arrow_timing: Default::default(),
            scatter: Vec::new(),
            scatter_worst_window_ms: 45.0,
            histogram: timing_rules::HistogramMs::default(),
            graph_first_second: 0.0,
            graph_last_second: song.precise_last_second(),
            music_rate,
            life_history: Vec::new(),
            fail_time: None,
            window_counts: timing_rules::WindowCounts::default(),
            window_counts_10ms: timing_rules::WindowCounts::default(),
            ex_score_percent: 0.0,
            hard_ex_score_percent: 0.0,
            calories_burned: 0.0,
            column_judgments: Vec::new(),
            noteskin: None,
            show_fa_plus_window: false,
            show_ex_score: false,
            show_hard_ex_score: false,
            show_fa_plus_pane: false,
            track_early_judgments: false,
            dim_post_fail_scatter: true,
            disabled_timing_windows: [false; 5],
            machine_records: Vec::new(),
            machine_record_highlight_rank: None,
            personal_records: Vec::new(),
            personal_record_highlight_rank: None,
            show_machine_personal_split: false,
        }
    }

    fn test_song_with_duration(path: &str, hash: &str, seconds: f32) -> Arc<SongData> {
        let mut song = test_song(path, 0.0, [hash, hash]);
        song.music_length_seconds = seconds;
        song.total_length_seconds = seconds.round() as i32;
        song.precise_last_second_seconds = seconds;
        Arc::new(song)
    }

    fn test_course_stage(song: Arc<SongData>) -> CourseStageRuntime {
        CourseStageRuntime {
            song,
            steps_index: [0; MAX_PLAYERS],
            preferred_difficulty_index: [0; MAX_PLAYERS],
        }
    }

    fn test_player_stage_summary(
        chart: Arc<ChartData>,
        grade: score_data::Grade,
        score_percent: f64,
        earned_grade_points: i32,
        possible_grade_points: i32,
    ) -> stage_stats::PlayerStageSummary {
        stage_stats::PlayerStageSummary {
            profile_name: "P1".to_string(),
            chart,
            score_valid: true,
            disqualified: false,
            groovestats: score_data::GrooveStatsEvalState::default(),
            itl: score_data::ItlEvalState::default(),
            grade,
            score_percent,
            earned_grade_points,
            possible_grade_points,
            ex_score_percent: 100.0,
            hard_ex_score_percent: 100.0,
            hands_achieved: 1,
            hands_total: 1,
            holds_held: 2,
            holds_held_for_score: 2,
            holds_total: 2,
            rolls_held: 1,
            rolls_held_for_score: 1,
            rolls_total: 1,
            mines_hit_for_score: 0,
            mines_avoided: 3,
            mines_total: 3,
            notes_hit: 20,
            calories_burned: 12.5,
            window_counts: timing_rules::WindowCounts {
                w0: 20,
                ..Default::default()
            },
            window_counts_10ms: timing_rules::WindowCounts {
                w0: 16,
                w1: 4,
                ..Default::default()
            },
            timing: timing_rules::TimingStats {
                mean_abs_ms: 10.0,
                mean_ms: 10.0,
                stddev_ms: 0.0,
                max_abs_ms: 10.0,
            },
            arrow_timing: Default::default(),
            scatter: vec![timing_rules::ScatterPoint {
                time_sec: 12.0,
                offset_ms: Some(10.0),
                direction_code: 1,
                is_stream: false,
                is_left_foot: true,
                miss_because_held: false,
            }],
            scatter_worst_window_ms: 45.0,
            histogram: timing_rules::HistogramMs {
                bins: vec![(10, 1)],
                smoothed: Vec::new(),
                max_count: 1,
                worst_observed_ms: 10.0,
                worst_window_ms: 45.0,
            },
            graph_first_second: 0.0,
            graph_last_second: 60.0,
            life_history: vec![(0.0, 1.0), (60.0, 0.0)],
            fail_time: Some(60.0),
            show_w0: true,
            show_ex_score: true,
            show_hard_ex_score: true,
            show_fa_plus_pane: true,
            track_early_judgments: true,
            dim_post_fail_scatter: true,
        }
    }

    #[test]
    fn raw_keyboard_restart_screen_matches_zmod_restart_flow() {
        assert!(raw_keyboard_restart_screen(CurrentScreen::Gameplay));
        assert!(raw_keyboard_restart_screen(CurrentScreen::Evaluation));
        assert!(!raw_keyboard_restart_screen(
            CurrentScreen::EvaluationSummary,
        ));
    }

    #[test]
    fn course_eval_final_on_completion_or_failure() {
        assert!(!stage_stats::course_eval_is_final(1, 3, false));
        assert!(stage_stats::course_eval_is_final(1, 3, true));
        assert!(stage_stats::course_eval_is_final(3, 3, false));
    }

    #[test]
    fn course_summary_uses_trail_totals_and_keeps_timing_graphs() {
        let song_a = test_song_with_duration("Songs/Test/a.ssc", "a", 60.0);
        let song_b = test_song_with_duration("Songs/Test/b.ssc", "b", 90.0);
        let mut chart = test_chart("stage-a");
        chart.step_artist = "Stage Artist".to_string();
        chart.description = "Stage Description".to_string();
        chart.chart_name = "Stage Chart Name".to_string();
        let chart = Arc::new(chart);
        let mut stage_players: [Option<stage_stats::PlayerStageSummary>; MAX_PLAYERS] =
            std::array::from_fn(|_| None);
        stage_players[0] = Some(test_player_stage_summary(
            chart,
            score_data::Grade::Failed,
            1.0,
            500,
            500,
        ));

        let mut course_display_totals = [CourseDisplayTotals::default(); MAX_PLAYERS];
        course_display_totals[0] = CourseDisplayTotals {
            possible_grade_points: 1000,
            total_steps: 40,
            holds_total: 4,
            rolls_total: 2,
            mines_total: 6,
        };
        let course = CourseRunState {
            path: PathBuf::from("Courses/Test.crs"),
            name: "Test Course".to_string(),
            banner_path: None,
            score_hash: "course-hash".to_string(),
            course_difficulty_name: "Hard".to_string(),
            course_meter: Some(12),
            course_stepchart_label: "Hard".to_string(),
            song_stub: song_a.clone(),
            stages: vec![
                test_course_stage(song_a.clone()),
                test_course_stage(song_b.clone()),
            ],
            course_display_totals,
            next_stage_index: 1,
            stage_summaries: vec![stage_stats::StageSummary {
                song: song_a.clone(),
                music_rate: 1.0,
                duration_seconds: 60.0,
                players: stage_players,
            }],
        };

        let summary = build_course_summary_stage(&course).expect("course summary");
        let player = summary.players[0].as_ref().expect("P1 summary");
        assert!((summary.duration_seconds - 150.0).abs() <= f32::EPSILON);
        assert!((player.score_percent - 0.5).abs() <= f64::EPSILON);
        assert_eq!(player.earned_grade_points, 500);
        assert_eq!(player.possible_grade_points, 1000);
        assert_eq!(player.holds_total, 4);
        assert_eq!(player.rolls_total, 2);
        assert_eq!(player.mines_total, 6);
        assert_eq!(player.grade, score_data::Grade::Failed);
        assert!(player.chart.step_artist.is_empty());
        assert!(player.chart.description.is_empty());
        assert!(player.chart.chart_name.is_empty());
        assert_eq!(player.scatter.len(), 1);
        assert!(!player.histogram.bins.is_empty());
        assert!((player.timing.mean_ms - 10.0).abs() <= f32::EPSILON);

        let course_page =
            score_info_from_stage(&summary, profile_data::PlayerSide::P1).expect("course page");
        let song_page =
            score_info_from_stage(&course.stage_summaries[0], profile_data::PlayerSide::P1)
                .expect("song page");
        assert!((course_page.score_percent - 0.5).abs() <= f64::EPSILON);
        assert!((song_page.score_percent - 1.0).abs() <= f64::EPSILON);
        assert!(!course_page.histogram.bins.is_empty());
        assert_eq!(course_page.scatter.len(), 1);
    }

    #[test]
    fn course_summary_merges_column_judgments_from_song_pages() {
        let song = test_song_with_duration("Songs/Test/course.ssc", "course", 120.0);
        let side = profile_data::PlayerSide::P2;
        let mut course_score = std::array::from_fn(|_| None);
        course_score[0] = Some(test_score_info(
            song.clone(),
            side,
            "course",
            ScrollSpeedSetting::default(),
            1.0,
        ));
        let mut course_page = evaluation::init_from_score_info(course_score, 120.0);

        let mut first = std::array::from_fn(|_| None);
        let mut first_p2 = test_score_info(
            song.clone(),
            side,
            "stage-a",
            ScrollSpeedSetting::default(),
            1.0,
        );
        first_p2.column_judgments = vec![
            evaluation::ColumnJudgments {
                w0: 1,
                w1: 2,
                early_w1: 1,
                early_total_w0: 1,
                held_miss: 1,
                ..Default::default()
            },
            evaluation::ColumnJudgments {
                w2: 3,
                miss: 1,
                early_w2: 2,
                early_total_w2: 2,
                ..Default::default()
            },
        ];
        first[0] = Some(first_p2);
        let mut ignored_p1 = test_score_info(
            song.clone(),
            profile_data::PlayerSide::P1,
            "ignored",
            ScrollSpeedSetting::default(),
            1.0,
        );
        ignored_p1.column_judgments = vec![evaluation::ColumnJudgments {
            w4: 1000,
            ..Default::default()
        }];
        first[1] = Some(ignored_p1);
        let first_page = evaluation::init_from_score_info(first, 60.0);

        let mut second = std::array::from_fn(|_| None);
        let mut second_p2 = test_score_info(
            song.clone(),
            side,
            "stage-b",
            ScrollSpeedSetting::default(),
            1.0,
        );
        second_p2.column_judgments = vec![
            evaluation::ColumnJudgments {
                w0: 4,
                w3: 5,
                early_w3: 1,
                early_total_w3: 1,
                held_miss: 2,
                ..Default::default()
            },
            evaluation::ColumnJudgments::default(),
            evaluation::ColumnJudgments {
                w5: 6,
                early_w5: 3,
                early_total_w5: 4,
                ..Default::default()
            },
        ];
        second[0] = Some(second_p2);
        let second_page = evaluation::init_from_score_info(second, 60.0);

        apply_course_summary_column_judgments(&mut course_page, &[first_page, second_page]);

        let columns = &course_page.score_info[0]
            .as_ref()
            .expect("course summary score")
            .column_judgments;
        assert_eq!(columns.len(), 3);
        assert_eq!(columns[0].w0, 5);
        assert_eq!(columns[0].w1, 2);
        assert_eq!(columns[0].w3, 5);
        assert_eq!(columns[0].w4, 0);
        assert_eq!(columns[0].early_w1, 1);
        assert_eq!(columns[0].early_w3, 1);
        assert_eq!(columns[0].early_total_w0, 1);
        assert_eq!(columns[0].early_total_w3, 1);
        assert_eq!(columns[0].held_miss, 3);
        assert_eq!(columns[1].w2, 3);
        assert_eq!(columns[1].miss, 1);
        assert_eq!(columns[1].early_w2, 2);
        assert_eq!(columns[1].early_total_w2, 2);
        assert_eq!(columns[2].w5, 6);
        assert_eq!(columns[2].early_w5, 3);
        assert_eq!(columns[2].early_total_w5, 4);
    }

    #[test]
    fn evaluation_restart_payload_uses_score_side_for_single_p2() {
        let song = Arc::new(test_song("Songs/Test/song.ssc", 0.0, ["p1", "p2"]));
        let mut score_info = std::array::from_fn(|_| None);
        score_info[0] = Some(test_score_info(
            song.clone(),
            profile_data::PlayerSide::P2,
            "p2hash",
            ScrollSpeedSetting::MMod(777.0),
            1.5,
        ));

        let payload = restart_payload_from_eval(&score_info).expect("score info should restart");

        assert!(Arc::ptr_eq(&payload.song, &song));
        assert!(payload.chart_hashes[0].is_empty());
        assert_eq!(payload.chart_hashes[1], "p2hash");
        assert!((payload.music_rate - 1.5).abs() < f32::EPSILON);
        assert_eq!(payload.scroll_speed[0], ScrollSpeedSetting::default());
        assert_eq!(payload.scroll_speed[1], ScrollSpeedSetting::MMod(777.0));
    }

    #[test]
    fn evaluation_summary_return_to_stays_in_select_music_for_set_summary() {
        assert_eq!(
            evaluation_summary_return_to(CurrentScreen::SelectMusic, false),
            CurrentScreen::SelectMusic,
        );
    }

    #[test]
    fn evaluation_summary_return_to_keeps_exit_flow_moving() {
        assert_eq!(
            evaluation_summary_return_to(CurrentScreen::SelectMusic, true),
            CurrentScreen::Initials,
        );
        assert_eq!(
            evaluation_summary_return_to(CurrentScreen::SelectCourse, true),
            CurrentScreen::Initials,
        );
    }
}
