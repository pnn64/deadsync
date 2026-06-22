use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::sprite_sheet_dims;
use crate::assets::{FontRole, current_machine_font_key, visual_styles};
use crate::game::parsing::noteskin::{
    self, ModelDrawState, ModelMeshCache, ModelMeshCacheStats, Noteskin, SpriteSlot, Style,
};
use crate::game::parsing::song_lua::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaCompileContext, SongLuaDifficulty,
    SongLuaOverlayActor, SongLuaOverlayBlendMode, SongLuaOverlayCommandBlock, SongLuaOverlayKind,
    SongLuaOverlayMeshVertex, SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw,
    SongLuaOverlayModelLayer, SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaPlayerContext,
    SongLuaProxyTarget, SongLuaSpeedMod, SongLuaTextGlowMode, compile_song_lua,
};
use crate::game::{
    GameplayProfile, profile, profile_side_from_gameplay, profile_tick_mode_from_gameplay,
    score_display_mode_from_profile, scores, scroll_effects_from_option,
};
use crate::screens::components::gameplay::{gameplay_stats, notefield, step_stats_gifs};
use crate::screens::components::shared::banner as shared_banner;
use crate::screens::components::shared::gs_scorebox;
use crate::screens::components::shared::lobby_hud;
use crate::screens::components::shared::noteskin_model::noteskin_model_actor_from_draw;
use crate::screens::components::shared::screen_bar::{self, AvatarParams, ScreenBarParams};
use crate::screens::{Screen, ScreenAction};
use deadlib_present::actors::{Actor, SizeSpec, SpriteSource, TextAttribute, TextContent};
use deadlib_present::anim::EffectState;
use deadlib_present::cache::{TextCache, cached_text};
use deadlib_present::color;
use deadlib_present::compose::TextLayoutCache;
use deadlib_present::density::{self, DensityHistCache};
use deadlib_present::font;
use deadlib_present::space::widescale;
use deadlib_present::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width,
};
use deadlib_render::{BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, TexturedMeshVertex};
use deadsync_chart::background::expand_random_background_changes;
use deadsync_chart::{
    ChartData, GameplayChartData, SongBackgroundChange, SongBackgroundChangeTarget, SongData,
    SyncPref,
};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::song_time::song_time_ns_to_seconds;
use deadsync_gameplay::{
    AUTOSYNC_OFFSET_SAMPLE_COUNT, AutosyncMode, CourseDisplayCarry, CourseDisplayTiming,
    CourseDisplayTotals, CrossoverRow, ExitTransitionKind, FantasticWindowOptions, GameplayAction,
    GameplayAudioCommand, GameplayAudioSnapshot, GameplayConfig, GameplayExit,
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplayMiniIndicatorData, GameplayMusicCut,
    GameplayNoteskinData, GameplayNoteskinEffects, GameplayReceptorGlowBehavior,
    GameplayReceptorStepBehavior, GameplaySession, GameplaySessionCommand,
    GameplayStreamClockSnapshot, GameplayTween, GameplayViewport, HoldToExitKey, LeadInTiming,
    MINE_EXPLOSION_DURATION, RECEPTOR_STEP_WINDOWS, RECEPTOR_Y_OFFSET_FROM_CENTER,
    RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, ReplayInputEdge, ReplayOffsetSnapshot,
    SongLuaCompilePlayStyle, SongLuaOverlayMessageRuntime, TAP_EXPLOSION_WINDOWS,
    autosync_mode_status_line, blue_fantastic_window_ms, build_crossover_rows,
    exit_transition_alpha, gameplay_is_single_p2_side, gameplay_runtime_charts, handle_core_input,
    scroll_receptor_y,
    song_lua_compile_player_screen_x as gameplay_song_lua_compile_player_screen_x,
    song_lua_ease_factor, spacing_multiplier_for_percent, update_core,
};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_online::lobbies as lobby_data;
use deadsync_profile as profile_data;
use deadsync_rules::note::Note;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingSegments;
use deadsync_score as score_data;
use deadsync_smx::{self, SensorTestMode};
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4 as Vector4};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

pub use crate::game::GameplayCoreState;

const TEXT_CACHE_LIMIT: usize = 8192;
type SongLuaOverlayEaseWindowRuntime =
    deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>;
type SongLuaRuntimeOverlayStateDelta =
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<SongLuaOverlayStateDelta>;

#[derive(Clone, Debug)]
pub(crate) struct GameplayCompiledSongLua {
    pub(crate) compiled: CompiledSongLua,
    pub(crate) compile_ms: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct GameplaySongLuaLayer {
    pub(crate) start_beat: f32,
    pub(crate) compiled: CompiledSongLua,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct GameplaySongLuaData {
    pub(crate) primary: Option<GameplayCompiledSongLua>,
    pub(crate) background_layers: Vec<GameplaySongLuaLayer>,
    pub(crate) foreground_layers: Vec<GameplaySongLuaLayer>,
}

impl
    deadsync_gameplay::SongLuaRuntimeBuilder<
        SongLuaOverlayActor,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    > for GameplaySongLuaData
{
    fn build_song_lua_runtime(
        self,
        params: deadsync_gameplay::SongLuaRuntimeWindowBuild<'_>,
    ) -> deadsync_gameplay::SongLuaRuntimeBuildOutput<
        SongLuaOverlayActor,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    > {
        build_song_lua_runtime_windows_for_data(params, self)
    }
}
const INTRO_TEXT_SETTLE_SECONDS: f32 = 1.49; // 0.5 + 0.66 + 0.33 (SL OnCommand chain)
const INTRO_TEXT_GETWIDTH_PAD: f32 = 0.25;
const DIFFICULTY_METER_Y: f32 = 56.0;
const DIFFICULTY_METER_SIZE: f32 = 30.0;
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;

pub use crate::screens::components::gameplay::notefield::ViewOverride as NotefieldViewOverride;

#[inline(always)]
fn player_blue_window_ms(state: &GameplayCoreState, player_idx: usize) -> f32 {
    let base = state.default_fa_plus_window_s();
    let Some(profile) = state.profiles().get(player_idx) else {
        return base * 1000.0;
    };
    blue_fantastic_window_ms(FantasticWindowOptions {
        base_fa_plus_s: base,
        custom_fantastic_window_s: profile.custom_fantastic_window.then(|| {
            f32::from(profile_data::clamp_custom_fantastic_window_ms(
                profile.custom_fantastic_window_ms,
            )) / 1000.0
        }),
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
    })
}

#[inline(always)]
fn song_lua_difficulty_from_chart(difficulty: &str) -> SongLuaDifficulty {
    if difficulty.eq_ignore_ascii_case("beginner") {
        SongLuaDifficulty::Beginner
    } else if difficulty.eq_ignore_ascii_case("easy") || difficulty.eq_ignore_ascii_case("basic") {
        SongLuaDifficulty::Easy
    } else if difficulty.eq_ignore_ascii_case("medium")
        || difficulty.eq_ignore_ascii_case("standard")
    {
        SongLuaDifficulty::Medium
    } else if difficulty.eq_ignore_ascii_case("hard")
        || difficulty.eq_ignore_ascii_case("difficult")
    {
        SongLuaDifficulty::Hard
    } else if difficulty.eq_ignore_ascii_case("edit") {
        SongLuaDifficulty::Edit
    } else {
        SongLuaDifficulty::Challenge
    }
}

#[inline(always)]
const fn song_lua_speedmod_from_setting(speed: ScrollSpeedSetting) -> SongLuaSpeedMod {
    match speed {
        ScrollSpeedSetting::XMod(value) => SongLuaSpeedMod::X(value),
        ScrollSpeedSetting::CMod(value) => SongLuaSpeedMod::C(value),
        ScrollSpeedSetting::MMod(value) => SongLuaSpeedMod::M(value),
    }
}

#[inline(always)]
const fn song_lua_compile_play_style(
    play_style: GameplayInputPlayStyle,
) -> SongLuaCompilePlayStyle {
    match play_style {
        GameplayInputPlayStyle::Single => SongLuaCompilePlayStyle::Single,
        GameplayInputPlayStyle::Versus => SongLuaCompilePlayStyle::Versus,
        GameplayInputPlayStyle::Double => SongLuaCompilePlayStyle::Double,
    }
}

const fn song_lua_runtime_time_unit(
    unit: deadsync_song_lua::SongLuaTimeUnit,
) -> deadsync_gameplay::SongLuaRuntimeTimeUnit {
    match unit {
        deadsync_song_lua::SongLuaTimeUnit::Beat => deadsync_gameplay::SongLuaRuntimeTimeUnit::Beat,
        deadsync_song_lua::SongLuaTimeUnit::Second => {
            deadsync_gameplay::SongLuaRuntimeTimeUnit::Second
        }
    }
}

const fn song_lua_runtime_span_mode(
    span_mode: deadsync_song_lua::SongLuaSpanMode,
) -> deadsync_gameplay::SongLuaRuntimeSpanMode {
    match span_mode {
        deadsync_song_lua::SongLuaSpanMode::Len => deadsync_gameplay::SongLuaRuntimeSpanMode::Len,
        deadsync_song_lua::SongLuaSpanMode::End => deadsync_gameplay::SongLuaRuntimeSpanMode::End,
    }
}

fn song_lua_runtime_ease_target(
    target: &deadsync_song_lua::SongLuaEaseTarget,
) -> deadsync_gameplay::SongLuaRuntimeEaseTargetOwned {
    match target {
        deadsync_song_lua::SongLuaEaseTarget::Mod(target_name) => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Mod(target_name.clone())
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZ,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerRotationX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerRotationY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerRotationZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerRotationZ,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerSkewX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerSkewY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerSkewY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoom => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoom,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoomX => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomX,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoomY => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomY,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::PlayerZoomZ => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Player(
                deadsync_gameplay::SongLuaEaseMaskTarget::PlayerZoomZ,
            )
        }
        deadsync_song_lua::SongLuaEaseTarget::Function => {
            deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Function
        }
    }
}

pub(crate) fn song_lua_runtime_mod_windows(
    windows: &[deadsync_song_lua::SongLuaModWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeModWindow> {
    windows
        .iter()
        .map(|window| deadsync_gameplay::SongLuaRuntimeModWindow {
            player: window.player,
            unit: song_lua_runtime_time_unit(window.unit),
            start: window.start,
            limit: window.limit,
            span_mode: song_lua_runtime_span_mode(window.span_mode),
            mods: window.mods.clone(),
        })
        .collect()
}

pub(crate) fn song_lua_runtime_ease_windows(
    windows: &[deadsync_song_lua::SongLuaEaseWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeEaseWindow> {
    windows
        .iter()
        .map(|window| deadsync_gameplay::SongLuaRuntimeEaseWindow {
            player: window.player,
            unit: song_lua_runtime_time_unit(window.unit),
            start: window.start,
            limit: window.limit,
            span_mode: song_lua_runtime_span_mode(window.span_mode),
            target: song_lua_runtime_ease_target(&window.target),
            from: window.from,
            to: window.to,
            easing: window.easing.clone(),
            sustain: window.sustain,
            opt1: window.opt1,
            opt2: window.opt2,
        })
        .collect()
}

pub(crate) fn song_lua_runtime_column_offset_windows(
    windows: &[deadsync_song_lua::SongLuaColumnOffsetWindow],
) -> Vec<deadsync_gameplay::SongLuaRuntimeColumnOffsetWindow> {
    windows
        .iter()
        .map(
            |window| deadsync_gameplay::SongLuaRuntimeColumnOffsetWindow {
                player: window.player,
                unit: song_lua_runtime_time_unit(window.unit),
                start: window.start,
                limit: window.limit,
                span_mode: song_lua_runtime_span_mode(window.span_mode),
                column: window.column,
                from_y: window.from_y,
                to_y: window.to_y,
                easing: window.easing.clone(),
                sustain: window.sustain,
                opt1: window.opt1,
                opt2: window.opt2,
            },
        )
        .collect()
}

fn song_lua_overlay_delta_mask(
    delta: &SongLuaOverlayStateDelta,
) -> deadsync_gameplay::SongLuaOverlayDeltaMask {
    let mut mask = 0u128;
    let mut bit = 0u32;
    macro_rules! field {
        ($field:ident) => {{
            if delta.$field.is_some() {
                mask |= 1u128 << bit;
            }
            bit += 1;
        }};
    }

    field!(x);
    field!(y);
    field!(z);
    field!(z_bias);
    field!(draw_order);
    field!(draw_by_z_position);
    field!(halign);
    field!(valign);
    field!(text_align);
    field!(uppercase);
    field!(shadow_len);
    field!(shadow_color);
    field!(glow);
    field!(fov);
    field!(vanishpoint);
    field!(diffuse);
    field!(vertex_colors);
    field!(visible);
    field!(cropleft);
    field!(cropright);
    field!(croptop);
    field!(cropbottom);
    field!(fadeleft);
    field!(faderight);
    field!(fadetop);
    field!(fadebottom);
    field!(mask_source);
    field!(mask_dest);
    field!(depth_test);
    field!(zoom);
    field!(zoom_x);
    field!(zoom_y);
    field!(zoom_z);
    field!(basezoom);
    field!(basezoom_x);
    field!(basezoom_y);
    field!(basezoom_z);
    field!(rot_x_deg);
    field!(rot_y_deg);
    field!(rot_z_deg);
    field!(skew_x);
    field!(skew_y);
    field!(blend);
    field!(vibrate);
    field!(effect_magnitude);
    field!(effect_clock);
    field!(effect_mode);
    field!(effect_color1);
    field!(effect_color2);
    field!(effect_period);
    field!(effect_offset);
    field!(effect_timing);
    field!(rainbow);
    field!(rainbow_scroll);
    field!(text_jitter);
    field!(text_distortion);
    field!(text_glow_mode);
    field!(mult_attrs_with_diffuse);
    field!(sprite_animate);
    field!(sprite_loop);
    field!(sprite_playback_rate);
    field!(sprite_state_delay);
    field!(sprite_state_index);
    field!(vert_spacing);
    field!(wrap_width_pixels);
    field!(max_width);
    field!(max_height);
    field!(max_w_pre_zoom);
    field!(max_h_pre_zoom);
    field!(max_dimension_uses_zoom);
    field!(texture_filtering);
    field!(texture_wrapping);
    field!(texcoord_offset);
    field!(custom_texture_rect);
    field!(texcoord_velocity);
    field!(size);
    field!(stretch_rect);
    field!(sound_play);

    let _ = bit;
    mask
}

fn song_lua_runtime_overlay_state_delta(
    delta: SongLuaOverlayStateDelta,
) -> SongLuaRuntimeOverlayStateDelta {
    SongLuaRuntimeOverlayStateDelta {
        overlap_mask: song_lua_overlay_delta_mask(&delta),
        delta,
    }
}

fn song_lua_runtime_overlay_ease_window(
    ease: &deadsync_song_lua::SongLuaOverlayEase,
) -> deadsync_gameplay::SongLuaRuntimeOverlayEaseWindow<SongLuaRuntimeOverlayStateDelta> {
    deadsync_gameplay::SongLuaRuntimeOverlayEaseWindow {
        overlay_index: ease.overlay_index,
        unit: song_lua_runtime_time_unit(ease.unit),
        start: ease.start,
        limit: ease.limit,
        span_mode: song_lua_runtime_span_mode(ease.span_mode),
        sustain: ease.sustain,
        from: song_lua_runtime_overlay_state_delta(ease.from),
        to: song_lua_runtime_overlay_state_delta(ease.to),
        easing: ease.easing.clone(),
        opt1: ease.opt1,
        opt2: ease.opt2,
    }
}

fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    profile: &profile_data::Profile,
    viewport: GameplayViewport,
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    gameplay_song_lua_compile_player_screen_x(
        num_players,
        player_index,
        viewport,
        song_lua_compile_play_style(play_style),
        gameplay_is_single_p2_side(play_style, player_side),
        profile.note_field_offset_x as f32,
        center_1player_notefield,
    )
}

pub(crate) fn song_lua_compile_context(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    scroll_speed: &[ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    machine_global_offset_seconds: f32,
    viewport: GameplayViewport,
    session: &GameplaySession,
    center_1player_notefield: bool,
) -> SongLuaCompileContext {
    let play_style = session.play_style;
    let player_side = session.player_side;
    let mut context = SongLuaCompileContext::new(
        song.simfile_path
            .parent()
            .map(|path| path.to_path_buf())
            .unwrap_or_default(),
        song.title.clone(),
    );
    context.song_display_bpms =
        song.display_bpm_pair_or(charts.first().map(|chart| chart.as_ref()), [60.0, 60.0]);
    context.song_music_rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    context.music_length_seconds = song.music_length_seconds.max(song.precise_last_second());
    context.style_name = match play_style {
        GameplayInputPlayStyle::Single => "single",
        GameplayInputPlayStyle::Versus => "versus",
        GameplayInputPlayStyle::Double => "double",
    }
    .to_string();
    context.global_offset_seconds = machine_global_offset_seconds;
    context.screen_width = viewport.width();
    context.screen_height = viewport.height();
    context.confusion_offset_available = true;
    context.confusion_available = true;
    context.amod_available = false;
    context.players = std::array::from_fn(|player| SongLuaPlayerContext {
        enabled: player < num_players,
        difficulty: if player < num_players {
            song_lua_difficulty_from_chart(&charts[player].difficulty)
        } else {
            SongLuaDifficulty::default_enabled()
        },
        display_bpms: if player < num_players {
            song.display_bpm_pair_or(Some(charts[player].as_ref()), [60.0, 60.0])
        } else {
            [60.0, 60.0]
        },
        speedmod: if player < num_players {
            song_lua_speedmod_from_setting(scroll_speed[player])
        } else {
            SongLuaSpeedMod::default()
        },
        noteskin_name: if player < num_players {
            player_profiles[player].noteskin.to_string()
        } else {
            profile_data::NoteSkin::default().to_string()
        },
        screen_x: song_lua_compile_player_screen_x(
            num_players,
            player,
            &player_profiles[player],
            viewport,
            play_style,
            player_side,
            center_1player_notefield,
        ),
        screen_y: viewport.center_y(),
    });
    context
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ActorViewOverride {
    pub notefield: NotefieldViewOverride,
    pub hide_gameplay_hud: bool,
}

#[derive(Clone, Debug)]
pub struct CourseDisplayInfo {
    pub name: Arc<str>,
}

#[derive(Clone, Debug)]
struct GameplayScoreboxData {
    profile_snapshot: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
    side_snapshot: [Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
}

impl Default for GameplayScoreboxData {
    fn default() -> Self {
        Self {
            profile_snapshot: std::array::from_fn(|_| {
                score_data::GameplayScoreboxProfileSnapshot::default()
            }),
            side_snapshot: std::array::from_fn(|_| None),
        }
    }
}

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
const TRANSITION_IN_DURATION: f32 = 2.0;
/// SL/zmod parity: when re-entering Gameplay as a restart, skip the splode +
/// stage-text in-transition (`ScreenGameplay in/default.lua` calls
/// `Hide` immediately when `SL.Global.GameplayReloadCheck` is true). Use a
/// short fade-from-black so the new gameplay frame doesn't pop in.
const TRANSITION_IN_RESTART_DURATION: f32 = 0.2;
// Simply Love ScreenGameplay out.lua: sleep(0.5), linear(1.0).
const TRANSITION_OUT_DELAY: f32 = 0.5;
const TRANSITION_OUT_FADE_DURATION: f32 = 1.0;
const TRANSITION_OUT_DURATION: f32 = TRANSITION_OUT_DELAY + TRANSITION_OUT_FADE_DURATION;

pub struct DensityGraphRenderState {
    pub cache: [Option<DensityHistCache>; MAX_PLAYERS],
    pub mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub mesh_offset_px: [i32; MAX_PLAYERS],
    pub life_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub life_mesh_offset_px: [i32; MAX_PLAYERS],
    pub top_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
}

impl DensityGraphRenderState {
    fn from_gameplay(state: &GameplayCoreState) -> Self {
        let graph = state.density_graph_view();
        let top_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            let graph_w = graph.top_w[player];
            let graph_h = graph.top_mesh_h(player);
            if player >= state.num_players() || graph_w <= 0.0 || graph_h <= 0.0 {
                return None;
            }

            let chart = state.charts()[player].as_ref();
            let verts = density::build_density_histogram_mesh(
                &chart.measure_nps_vec,
                chart.max_nps,
                &chart.measure_seconds_vec,
                graph.first_second,
                graph.last_second,
                graph_w,
                graph_h,
                0.0,
                graph_w,
                None,
                1.0,
            );
            if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            }
        });

        let cache: [Option<DensityHistCache>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= state.num_players() || graph.graph_w <= 0.0 || graph.graph_h <= 0.0 {
                return None;
            }

            let chart = state.charts()[player].as_ref();
            density::build_density_histogram_cache(
                &chart.measure_nps_vec,
                chart.max_nps,
                &chart.measure_seconds_vec,
                graph.first_second,
                graph.last_second,
                graph.scaled_width,
                graph.graph_h,
                None,
                1.0,
            )
        });

        let mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= state.num_players() || cache[player].is_none() {
                return None;
            }
            let mut mesh = None;
            density::update_density_hist_mesh(
                &mut mesh,
                cache[player].as_ref(),
                0.0,
                graph.graph_w,
            );
            mesh
        });

        Self {
            cache,
            mesh,
            mesh_offset_px: [0; MAX_PLAYERS],
            life_mesh: std::array::from_fn(|_| None),
            life_mesh_offset_px: [0; MAX_PLAYERS],
            top_mesh,
        }
    }
}

#[derive(Clone)]
pub struct GameplayNoteskinAssets {
    pub noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub mine_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub tap_explosion_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
}

impl GameplayNoteskinAssets {
    pub(crate) fn gameplay_data(
        &self,
        cols_per_player: usize,
        num_players: usize,
        runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
    ) -> GameplayNoteskinData {
        GameplayNoteskinData {
            effects: noteskin_effects_from_assets(
                self,
                runtime_profiles,
                num_players,
                cols_per_player,
            ),
        }
    }
}

impl Default for GameplayNoteskinAssets {
    fn default() -> Self {
        Self {
            noteskin: std::array::from_fn(|_| None),
            mine_noteskin: std::array::from_fn(|_| None),
            receptor_noteskin: std::array::from_fn(|_| None),
            tap_explosion_noteskin: std::array::from_fn(|_| None),
        }
    }
}

fn noteskin_effects_from_assets(
    assets: &GameplayNoteskinAssets,
    runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
    num_players: usize,
    cols_per_player: usize,
) -> GameplayNoteskinEffects {
    let mut effects = GameplayNoteskinEffects::default();
    let cols = cols_per_player.min(deadsync_core::input::MAX_COLS);
    for player in 0..num_players.min(MAX_PLAYERS) {
        let receptor_ns = assets.receptor_noteskin[player]
            .as_deref()
            .or_else(|| assets.noteskin[player].as_deref());
        if let Some(ns) = receptor_ns {
            effects.set_receptor_glow_behavior(
                player,
                gameplay_receptor_glow_behavior(ns.receptor_glow_behavior),
            );
            for col in 0..cols {
                for window in RECEPTOR_STEP_WINDOWS {
                    effects.set_receptor_step_behavior(
                        player,
                        col,
                        window,
                        gameplay_receptor_step_behavior(
                            ns.receptor_step_behavior_for_col(col, window),
                        ),
                    );
                }
            }
        }

        let tap_ns = if runtime_profiles[player].tap_explosion_noteskin_hidden() {
            None
        } else {
            assets.tap_explosion_noteskin[player]
                .as_deref()
                .or_else(|| assets.noteskin[player].as_deref())
        };
        if let Some(ns) = tap_ns {
            for col in 0..cols {
                for window in TAP_EXPLOSION_WINDOWS {
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

        let mine_duration = assets.mine_noteskin[player]
            .as_deref()
            .or_else(|| assets.noteskin[player].as_deref())
            .and_then(|ns| ns.mine_hit_explosion.as_ref())
            .map_or(MINE_EXPLOSION_DURATION, |explosion| explosion.duration());
        effects.set_mine_explosion_duration(player, mine_duration);
    }
    effects
}

#[inline(always)]
fn gameplay_tween(tween: noteskin::TweenType) -> GameplayTween {
    match tween {
        noteskin::TweenType::Linear => GameplayTween::Linear,
        noteskin::TweenType::Accelerate => GameplayTween::Accelerate,
        noteskin::TweenType::Decelerate => GameplayTween::Decelerate,
    }
}

#[inline(always)]
fn gameplay_receptor_glow_behavior(
    behavior: noteskin::ReceptorGlowBehavior,
) -> GameplayReceptorGlowBehavior {
    GameplayReceptorGlowBehavior {
        press_duration: behavior.press_duration,
        press_alpha_start: behavior.press_alpha_start,
        press_alpha_end: behavior.press_alpha_end,
        press_zoom_start: behavior.press_zoom_start,
        press_zoom_end: behavior.press_zoom_end,
        press_tween: gameplay_tween(behavior.press_tween),
        duration: behavior.duration,
        alpha_start: behavior.alpha_start,
        alpha_end: behavior.alpha_end,
        zoom_start: behavior.zoom_start,
        zoom_end: behavior.zoom_end,
        tween: gameplay_tween(behavior.tween),
        blend_add: behavior.blend_add,
    }
}

#[inline(always)]
fn gameplay_receptor_step_behavior(
    behavior: noteskin::ReceptorStepBehavior,
) -> GameplayReceptorStepBehavior {
    GameplayReceptorStepBehavior {
        duration: behavior.duration,
        zoom_start: behavior.zoom_start,
        zoom_end: behavior.zoom_end,
        tween: gameplay_tween(behavior.tween),
        interrupts: behavior.interrupts,
    }
}

const SONG_LUA_CHILD_ORDER_STATIC: u8 = 0;
const SONG_LUA_CHILD_ORDER_DRAW: u8 = 1;
const SONG_LUA_CHILD_ORDER_Z: u8 = 2;

#[derive(Default)]
struct SongLuaOverlayOrderCache {
    child_lists: Vec<Vec<usize>>,
    dynamic_draw_order: Vec<bool>,
    sort_modes: Vec<u8>,
}

fn song_lua_overlay_child_list_index(parent_index: Option<usize>) -> usize {
    parent_index.map_or(0, |idx| idx + 1)
}

fn song_lua_sort_static_children(overlays: &[SongLuaOverlayActor], children: &mut [usize]) {
    children.sort_by_key(|&idx| (overlays[idx].initial_state.draw_order, idx));
}

fn song_lua_overlay_order_cache_from(
    overlays: &[SongLuaOverlayActor],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
) -> SongLuaOverlayOrderCache {
    let mut child_lists = vec![Vec::new(); overlays.len() + 1];
    for (idx, overlay) in overlays.iter().enumerate() {
        let list_idx = match overlay.parent_index {
            Some(parent_index) if parent_index < overlays.len() => parent_index + 1,
            Some(_) => continue,
            None => 0,
        };
        child_lists[list_idx].push(idx);
    }
    for children in &mut child_lists {
        song_lua_sort_static_children(overlays, children);
    }

    let mut dynamic_actor_draw_order = vec![false; overlays.len()];
    for (idx, overlay) in overlays.iter().enumerate() {
        dynamic_actor_draw_order[idx] = overlay.message_commands.iter().any(|command| {
            command
                .blocks
                .iter()
                .any(|block| block.delta.draw_order.is_some())
        });
    }
    for ease in overlay_eases {
        if ease.overlay_index < dynamic_actor_draw_order.len()
            && (ease.from.delta.draw_order.is_some() || ease.to.delta.draw_order.is_some())
        {
            dynamic_actor_draw_order[ease.overlay_index] = true;
        }
    }

    let dynamic_draw_order = child_lists
        .iter()
        .map(|children| {
            children
                .iter()
                .any(|&idx| dynamic_actor_draw_order.get(idx).copied().unwrap_or(false))
        })
        .collect::<Vec<_>>();
    let sort_modes = vec![SONG_LUA_CHILD_ORDER_STATIC; child_lists.len()];
    SongLuaOverlayOrderCache {
        child_lists,
        dynamic_draw_order,
        sort_modes,
    }
}

pub struct State {
    pub(crate) gameplay: GameplayCoreState,
    pub(crate) noteskin_assets: GameplayNoteskinAssets,
    pub density_graph: DensityGraphRenderState,
    pub step_stats_extra_resolved: [profile_data::StepStatsExtra; MAX_PLAYERS],
    pub song_full_title: Arc<str>,
    pub stage_intro_text: Arc<str>,
    pub replay_status_text: Option<Arc<str>>,
    pub course_display_info: Option<CourseDisplayInfo>,
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
    pub scorebox_profile_snapshot: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
    pub scorebox_side_snapshot: [Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
    pub lobby_music_started: bool,
    pub lobby_ready_p1: bool,
    pub lobby_ready_p2: bool,
    pub lobby_disconnect_hold_p1: Option<Instant>,
    pub lobby_disconnect_hold_p2: Option<Instant>,
    pub(crate) song_banner_key: Option<Arc<str>>,
    pub(crate) pack_banner_key: Option<Arc<str>>,
    pub(crate) notefield_model_cache: [RefCell<ModelMeshCache>; MAX_PLAYERS],
    pub background_path_dirty: bool,
    pub background_changes: Vec<SongBackgroundChange>,
    pub next_background_change_ix: usize,
    pub current_background_path: Option<PathBuf>,
    pub current_background_key: Option<Arc<str>>,
    pub background_allow_video: bool,
    pub background_texture_key: Arc<str>,
    pub previous_background_texture_key: Option<Arc<str>>,
    pub background_transition: String,
    pub background_transition_start_time: f32,
    pub song_lua_sound_paths: Vec<PathBuf>,
    smx_sensor_data: [Option<deadsync_smx::SensorTestData>; 2],
    smx_sensor_config: [Option<deadsync_smx::SmxConfig>; 2],
    // Time banked toward the next throttled sensor refresh (see
    // `maybe_refresh_smx_sensor_data`). Seeded to fire on the first frame.
    smx_sensor_refresh_accum: f32,
    song_lua_overlay_order: SongLuaOverlayOrderCache,
    song_lua_background_visual_layer_orders: Vec<SongLuaOverlayOrderCache>,
    song_lua_foreground_visual_layer_orders: Vec<SongLuaOverlayOrderCache>,
    song_lua_local_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_overlay_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_layer_local_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_layer_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_capture_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_order_scratch: Vec<usize>,
    song_lua_capture_order_scratch: Vec<usize>,
    notefield_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    notefield_hud_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    player_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
}

impl State {
    pub fn from_gameplay(
        gameplay: GameplayCoreState,
        noteskin_assets: GameplayNoteskinAssets,
    ) -> Self {
        Self::from_gameplay_with_screen_data(
            gameplay,
            noteskin_assets,
            Vec::new(),
            Vec::new(),
            Arc::from("EVENT"),
            None,
            None,
            Arc::from(""),
            None,
            GameplayScoreboxData::default(),
        )
    }

    fn from_gameplay_with_screen_data(
        gameplay: GameplayCoreState,
        noteskin_assets: GameplayNoteskinAssets,
        song_lua_sound_paths: Vec<PathBuf>,
        background_changes: Vec<SongBackgroundChange>,
        stage_intro_text: Arc<str>,
        replay_status_text: Option<Arc<str>>,
        course_display_info: Option<CourseDisplayInfo>,
        pack_group: Arc<str>,
        pack_banner_path: Option<PathBuf>,
        scorebox_data: GameplayScoreboxData,
    ) -> Self {
        let density_graph = DensityGraphRenderState::from_gameplay(&gameplay);
        let step_stats_profiles =
            std::array::from_fn(|player| gameplay.profiles()[player].0.clone());
        let step_stats_extra_resolved =
            step_stats_gifs::resolve_random_extras(&step_stats_profiles);
        let song = gameplay.song();
        let song_full_title: Arc<str> =
            Arc::from(song.display_full_title(crate::config::get().translated_titles));
        let song_banner_key = song
            .banner_path
            .as_deref()
            .map(crate::assets::media_path_key);
        let pack_banner_key = pack_banner_path
            .as_deref()
            .map(crate::assets::media_path_key);
        let notefield_model_cache =
            notefield_model_cache_from_assets(&noteskin_assets, gameplay.num_players());
        let background_transition_start_time = gameplay.current_music_time_display();
        let next_background_change_ix = background_changes
            .iter()
            .take_while(|change| change.start_beat <= gameplay.current_beat())
            .count();
        let song_lua_visuals = gameplay.song_lua_visuals();
        let song_lua_overlay_order = song_lua_overlay_order_cache_from(
            &song_lua_visuals.overlays,
            &song_lua_visuals.overlay_eases,
        );
        let song_lua_background_visual_layer_orders = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| song_lua_overlay_order_cache_from(&layer.overlays, &layer.overlay_eases))
            .collect();
        let song_lua_foreground_visual_layer_orders = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| song_lua_overlay_order_cache_from(&layer.overlays, &layer.overlay_eases))
            .collect();
        Self {
            gameplay,
            noteskin_assets,
            density_graph,
            step_stats_extra_resolved,
            song_full_title,
            stage_intro_text,
            replay_status_text,
            course_display_info,
            pack_group,
            pack_banner_path,
            scorebox_profile_snapshot: scorebox_data.profile_snapshot,
            scorebox_side_snapshot: scorebox_data.side_snapshot,
            lobby_music_started: false,
            lobby_ready_p1: false,
            lobby_ready_p2: false,
            lobby_disconnect_hold_p1: None,
            lobby_disconnect_hold_p2: None,
            song_banner_key,
            pack_banner_key,
            notefield_model_cache,
            background_path_dirty: true,
            background_changes,
            next_background_change_ix,
            current_background_path: None,
            current_background_key: None,
            background_allow_video: false,
            background_texture_key: Arc::from("__black"),
            previous_background_texture_key: None,
            background_transition: String::new(),
            background_transition_start_time,
            song_lua_sound_paths,
            smx_sensor_data: [None, None],
            smx_sensor_config: [None, None],
            smx_sensor_refresh_accum: SMX_SENSOR_REFRESH_INTERVAL,
            song_lua_overlay_order,
            song_lua_background_visual_layer_orders,
            song_lua_foreground_visual_layer_orders,
            song_lua_local_state_scratch: Vec::new(),
            song_lua_overlay_state_scratch: Vec::new(),
            song_lua_layer_local_state_scratch: Vec::new(),
            song_lua_layer_state_scratch: Vec::new(),
            song_lua_capture_state_scratch: Vec::new(),
            song_lua_order_scratch: Vec::new(),
            song_lua_capture_order_scratch: Vec::new(),
            notefield_actor_scratch: std::array::from_fn(|_| Vec::new()),
            notefield_hud_actor_scratch: std::array::from_fn(|_| Vec::new()),
            player_actor_scratch: std::array::from_fn(|_| Vec::new()),
        }
    }

    pub fn reset_notefield_model_cache_stats(&self) {
        for cache in &self.notefield_model_cache {
            cache.borrow_mut().reset_stats();
        }
    }

    pub fn notefield_model_cache_stats(&self) -> [ModelMeshCacheStats; MAX_PLAYERS] {
        std::array::from_fn(|player| self.notefield_model_cache[player].borrow().stats())
    }

    pub fn summed_notefield_model_cache_stats(&self) -> ModelMeshCacheStats {
        self.notefield_model_cache_stats().into_iter().fold(
            ModelMeshCacheStats::default(),
            |mut acc, stats| {
                acc.hits = acc.hits.saturating_add(stats.hits);
                acc.misses = acc.misses.saturating_add(stats.misses);
                acc.saturated_misses = acc.saturated_misses.saturating_add(stats.saturated_misses);
                acc
            },
        )
    }

    pub(crate) fn set_pack_display(
        &mut self,
        pack_group: Arc<str>,
        pack_banner_path: Option<PathBuf>,
    ) {
        self.pack_banner_key = pack_banner_path
            .as_deref()
            .map(crate::assets::media_path_key);
        self.pack_group = pack_group;
        self.pack_banner_path = pack_banner_path;
    }
}

impl Deref for State {
    type Target = GameplayCoreState;

    fn deref(&self) -> &Self::Target {
        &self.gameplay
    }
}

impl DerefMut for State {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gameplay
    }
}

fn song_pack_group(song: &SongData) -> Arc<str> {
    Arc::from(
        song.simfile_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned(),
    )
}

fn gameplay_pack_data(
    song: &SongData,
    course_display_info: Option<&CourseDisplayInfo>,
    course_banner_path: Option<&PathBuf>,
) -> (Arc<str>, Option<PathBuf>, SyncPref) {
    let pack_group = song_pack_group(song);
    let mut pack_banner_path = None;
    let mut sync_pref = SyncPref::Default;
    if !pack_group.is_empty()
        && let Some(pack) = crate::game::song::get_song_cache()
            .iter()
            .find(|pack| pack.group_name == pack_group.as_ref())
    {
        pack_banner_path = pack.banner_path.clone();
        sync_pref = pack.sync_pref;
    }
    if let Some(course_info) = course_display_info {
        return (
            course_info.name.clone(),
            course_banner_path.cloned(),
            sync_pref,
        );
    }
    (pack_group, pack_banner_path, sync_pref)
}

fn mini_indicator_personal_best_percent(
    chart_hash: &str,
    side: profile_data::PlayerSide,
    score_type: profile_data::MiniIndicatorScoreType,
) -> Option<f64> {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => {
            scores::get_cached_score_for_side(chart_hash, side)
                .map(|s| (s.score_percent * 100.0).clamp(0.0, 100.0))
        }
        profile_data::MiniIndicatorScoreType::Ex => {
            scores::get_cached_local_ex_score_for_side(chart_hash, side)
                .map(|s| s.percent.clamp(0.0, 100.0))
        }
        profile_data::MiniIndicatorScoreType::HardEx => {
            scores::get_cached_local_hard_ex_score_for_side(chart_hash, side)
                .map(|s| s.percent.clamp(0.0, 100.0))
        }
    }
}

fn mini_indicator_machine_best_percent(
    chart_hash: &str,
    score_type: profile_data::MiniIndicatorScoreType,
) -> Option<f64> {
    match score_type {
        profile_data::MiniIndicatorScoreType::Itg => scores::get_machine_record_local(chart_hash)
            .map(|(_, s)| (s.score_percent * 100.0).clamp(0.0, 100.0)),
        profile_data::MiniIndicatorScoreType::Ex | profile_data::MiniIndicatorScoreType::HardEx => {
            None
        }
    }
}

fn gameplay_mini_indicator_data(
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    session: &GameplaySession,
) -> GameplayMiniIndicatorData {
    let mut data = GameplayMiniIndicatorData::default();
    let num_players = session.play_style.player_count();
    for p in 0..num_players {
        let side = profile_side_from_gameplay(session.runtime_player_side(p));
        let chart_hash = charts[p].short_hash.as_str();
        let score_type = player_profiles[p].mini_indicator_score_type;
        data.personal_best_percent[p] =
            mini_indicator_personal_best_percent(chart_hash, side, score_type);
        data.machine_best_percent[p] = mini_indicator_machine_best_percent(chart_hash, score_type);
    }
    data
}

fn gameplay_scorebox_data(
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    session: &GameplaySession,
) -> GameplayScoreboxData {
    let mut data = GameplayScoreboxData::default();
    let num_players = session.play_style.player_count();
    for p in 0..num_players {
        let gameplay_side = session.runtime_player_side(p);
        let side = profile_side_from_gameplay(gameplay_side);
        data.profile_snapshot[profile_data::player_side_index(side)] =
            scores::scorebox_profile_snapshot(
                &player_profiles[p],
                session.side_joined(gameplay_side),
                session.active_profile_id_for_side(gameplay_side),
            );
    }

    for p in 0..num_players {
        let side = profile_side_from_gameplay(session.runtime_player_side(p));
        let idx = profile_data::player_side_index(side);
        let profile_snapshot = &data.profile_snapshot[idx];
        if !profile_snapshot.display_scorebox || !profile_snapshot.gs_active {
            continue;
        }
        let chart_hash = charts[p].short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }
        data.side_snapshot[idx] = scores::get_or_fetch_player_leaderboards_for_profile(
            chart_hash,
            profile_snapshot,
            gs_scorebox::SCOREBOX_NUM_ENTRIES,
        );
    }
    data
}

pub(crate) fn gameplay_runtime_profile_data(
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    session: &GameplaySession,
) -> [profile_data::Profile; MAX_PLAYERS] {
    let mut runtime_profiles = (*player_profiles).clone();
    if session.p2_runtime_player() {
        runtime_profiles[0] = runtime_profiles[1].clone();
    }
    runtime_profiles
}

pub(crate) fn gameplay_crossover_annotations_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    timing_segments: &TimingSegments,
    cols_per_player: usize,
    col_start: usize,
) -> Vec<CrossoverRow> {
    let (start, end) = note_range;
    if start >= end {
        return Vec::new();
    }
    let rssp_segments =
        deadsync_simfile::timing::rssp_timing_segments_from_deadsync(timing_segments);
    let rssp_timing = rssp::timing::timing_data_from_segments(0.0, 0.0, &rssp_segments);
    let annotations = match cols_per_player {
        4 => {
            let (rows, row_to_beat) = build_crossover_rows::<4>(notes, note_range, col_start);
            let Some(mut scratch) = rssp::step_parity::timing_rows_scratch::<4>() else {
                return Vec::new();
            };
            rssp::step_parity::annotate_timing_rows::<4>(
                &rows,
                &row_to_beat,
                &rssp_timing,
                &mut scratch,
            )
        }
        8 => {
            let (rows, row_to_beat) = build_crossover_rows::<8>(notes, note_range, col_start);
            let Some(mut scratch) = rssp::step_parity::timing_rows_scratch::<8>() else {
                return Vec::new();
            };
            rssp::step_parity::annotate_timing_rows::<8>(
                &rows,
                &row_to_beat,
                &rssp_timing,
                &mut scratch,
            )
        }
        _ => return Vec::new(),
    };
    annotations
        .iter()
        .map(|annotation| CrossoverRow {
            beat: annotation.beat,
            column_mask: annotation.column_mask,
            crossover: annotation.row_tech.crossovers > 0,
            bracket: annotation.foot_count() > 1,
        })
        .collect()
}

fn prewarm_notefield_model_cache_slots(
    cache: &[RefCell<ModelMeshCache>; MAX_PLAYERS],
    assets: &GameplayNoteskinAssets,
    num_players: usize,
) {
    for player in 0..num_players.min(MAX_PLAYERS) {
        let mut cache = cache[player].borrow_mut();
        for skin in [
            assets.noteskin[player].as_ref(),
            assets.mine_noteskin[player].as_ref(),
            assets.receptor_noteskin[player].as_ref(),
            assets.tap_explosion_noteskin[player].as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            skin.for_each_model_slot(|slot| cache.prewarm_slot(slot));
        }
        cache.reset_stats();
    }
}

pub(crate) fn notefield_model_cache_from_assets(
    assets: &GameplayNoteskinAssets,
    num_players: usize,
) -> [RefCell<ModelMeshCache>; MAX_PLAYERS] {
    let cache: [RefCell<ModelMeshCache>; MAX_PLAYERS] = std::array::from_fn(|player| {
        RefCell::new(if player < num_players {
            ModelMeshCache::with_capacity(96)
        } else {
            ModelMeshCache::default()
        })
    });
    prewarm_notefield_model_cache_slots(&cache, assets, num_players);
    cache
}

pub(crate) fn gameplay_noteskin_assets(
    cols_per_player: usize,
    num_players: usize,
    runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
) -> GameplayNoteskinAssets {
    let style = Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
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
    let receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
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
    GameplayNoteskinAssets {
        noteskin,
        mine_noteskin,
        receptor_noteskin,
        tap_explosion_noteskin,
    }
}

fn compile_primary_song_lua(
    song_title: &str,
    path: &Path,
    context: &crate::game::parsing::song_lua::SongLuaCompileContext,
) -> Option<GameplayCompiledSongLua> {
    let compile_started = Instant::now();
    match compile_song_lua(path, context) {
        Ok(compiled) => Some(GameplayCompiledSongLua {
            compiled,
            compile_ms: compile_started.elapsed().as_secs_f64() * 1000.0,
        }),
        Err(err) => {
            log::warn!(
                "Failed to compile gameplay lua for '{}' from '{}': {}",
                song_title,
                path.display(),
                err,
            );
            None
        }
    }
}

fn compile_song_lua_layer(
    song_title: &str,
    path: &Path,
    start_beat: f32,
    label: &str,
    context: &crate::game::parsing::song_lua::SongLuaCompileContext,
) -> Option<GameplaySongLuaLayer> {
    match compile_song_lua(path, context) {
        Ok(compiled) => Some(GameplaySongLuaLayer {
            start_beat,
            compiled,
        }),
        Err(err) => {
            log::warn!(
                "Failed to compile {} for '{}' from '{}': {}",
                label,
                song_title,
                path.display(),
                err,
            );
            None
        }
    }
}

fn gameplay_song_lua_data(
    song: &SongData,
    charts: &[Arc<ChartData>; MAX_PLAYERS],
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    scroll_speed: &[ScrollSpeedSetting; MAX_PLAYERS],
    music_rate: f32,
    viewport: GameplayViewport,
    session: &GameplaySession,
    config: &GameplayConfig,
) -> GameplaySongLuaData {
    let primary_ix = song
        .foreground_lua_changes
        .iter()
        .position(|change| change.start_beat <= 0.0 && change.path.is_file());
    if primary_ix.is_none()
        && song.background_lua_changes.is_empty()
        && song.foreground_lua_changes.is_empty()
    {
        return GameplaySongLuaData::default();
    }

    let mut runtime_charts = [charts[0].clone(), charts[1].clone()];
    let mut runtime_profiles = (*player_profiles).clone();
    let mut runtime_scroll_speed = [scroll_speed[0], scroll_speed[1]];
    if session.p2_runtime_player() {
        runtime_charts[0] = runtime_charts[1].clone();
        runtime_profiles[0] = runtime_profiles[1].clone();
        runtime_scroll_speed[0] = runtime_scroll_speed[1];
    }

    let context = song_lua_compile_context(
        song,
        &runtime_charts,
        session.play_style.player_count(),
        &runtime_profiles,
        &runtime_scroll_speed,
        music_rate,
        config.global_offset_seconds,
        viewport,
        session,
        config.center_1player_notefield,
    );
    let primary = primary_ix.and_then(|ix| {
        compile_primary_song_lua(
            song.title.as_str(),
            &song.foreground_lua_changes[ix].path,
            &context,
        )
    });
    let primary_key = primary_ix.map(|ix| {
        let change = &song.foreground_lua_changes[ix];
        (change.start_beat.to_bits(), change.path.clone())
    });
    let background_layers = song
        .background_lua_changes
        .iter()
        .filter_map(|change| {
            compile_song_lua_layer(
                song.title.as_str(),
                &change.path,
                change.start_beat,
                "background lua layer",
                &context,
            )
        })
        .collect();
    let foreground_layers = song
        .foreground_lua_changes
        .iter()
        .filter(|change| {
            change.path.is_file()
                && !primary_key.as_ref().is_some_and(|(beat_bits, path)| {
                    change.start_beat.to_bits() == *beat_bits && change.path == *path
                })
        })
        .filter_map(|change| {
            compile_song_lua_layer(
                song.title.as_str(),
                &change.path,
                change.start_beat,
                "foreground lua layer",
                &context,
            )
        })
        .collect();

    GameplaySongLuaData {
        primary,
        background_layers,
        foreground_layers,
    }
}

fn extend_song_lua_sound_paths(out: &mut Vec<PathBuf>, paths: &[PathBuf]) {
    for path in paths {
        if !out.contains(path) {
            out.push(path.clone());
        }
    }
}

fn song_lua_sound_paths(data: &GameplaySongLuaData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(primary) = data.primary.as_ref() {
        extend_song_lua_sound_paths(&mut out, &primary.compiled.sound_paths);
    }
    for layer in data
        .background_layers
        .iter()
        .chain(data.foreground_layers.iter())
    {
        extend_song_lua_sound_paths(&mut out, &layer.compiled.sound_paths);
    }
    out
}

fn build_song_lua_actor_message_events_for_commands(
    messages: &[deadsync_song_lua::SongLuaMessageEvent],
    message_seconds: &[Option<f32>],
    commands: &[deadsync_song_lua::SongLuaOverlayMessageCommand],
) -> Vec<SongLuaOverlayMessageRuntime> {
    deadsync_gameplay::build_song_lua_actor_message_events_with_seconds(
        messages
            .iter()
            .enumerate()
            .map(|(idx, message)| (idx, message.message.as_str())),
        message_seconds,
        commands
            .iter()
            .enumerate()
            .map(|(idx, command)| (idx, command.message.as_str())),
    )
}

fn build_song_lua_overlay_message_events_with_seconds(
    compiled: &CompiledSongLua,
    message_seconds: &[Option<f32>],
) -> Vec<Vec<SongLuaOverlayMessageRuntime>> {
    compiled
        .overlays
        .iter()
        .map(|overlay| {
            build_song_lua_actor_message_events_for_commands(
                &compiled.messages,
                message_seconds,
                &overlay.message_commands,
            )
        })
        .collect()
}

fn song_lua_compiled_overlay_ease_cutoff_second(
    compiled: &CompiledSongLua,
    ease: &deadsync_song_lua::SongLuaOverlayEase,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    start_second: f32,
) -> Option<f32> {
    let overlay = compiled.overlays.get(ease.overlay_index)?;
    let events = overlay_events.get(ease.overlay_index)?;
    let from_mask = song_lua_overlay_delta_mask(&ease.from);
    let to_mask = song_lua_overlay_delta_mask(&ease.to);
    let blocks = events
        .iter()
        .filter_map(|event| {
            let command = overlay.message_commands.get(event.command_index)?;
            Some((event.event_second, command))
        })
        .flat_map(|(event_second, command)| {
            command.blocks.iter().map(move |block| {
                (
                    event_second,
                    block.start,
                    song_lua_overlay_delta_mask(&block.delta),
                )
            })
        });
    deadsync_gameplay::song_lua_overlay_ease_cutoff_second(
        start_second,
        &from_mask,
        &to_mask,
        blocks,
    )
}

pub(crate) fn build_song_lua_overlay_ease_windows_with_events(
    compiled: &CompiledSongLua,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let mut out = Vec::new();
    for ease in &compiled.overlay_eases {
        let runtime_ease = song_lua_runtime_overlay_ease_window(ease);
        if let Some(window) = deadsync_gameplay::build_song_lua_overlay_ease_window_for(
            &runtime_ease,
            timing_player,
            global_offset_seconds,
            |start_second| {
                song_lua_compiled_overlay_ease_cutoff_second(
                    compiled,
                    ease,
                    overlay_events,
                    start_second,
                )
            },
        ) {
            out.push(window);
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn build_song_lua_overlay_ease_windows(
    compiled: &CompiledSongLua,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        timing_player,
        global_offset_seconds,
    );
    let overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    )
}

#[cfg(test)]
pub(crate) fn build_compiled_song_lua_ease_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &deadsync_rules::timing::TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[deadsync_gameplay::AttackMaskWindow],
) -> (Vec<deadsync_gameplay::SongLuaEaseMaskWindow>, usize) {
    let eases = song_lua_runtime_ease_windows(&compiled.eases);
    deadsync_gameplay::build_song_lua_ease_windows_for_player(
        &eases,
        timing_player,
        player,
        global_offset_seconds,
        constant_windows,
        |window| log_unsupported_song_lua_ease_target(player, window),
    )
}

fn build_song_lua_compiled_visual_layer_runtime(
    song_title: &str,
    start_beat: f32,
    compiled: &CompiledSongLua,
    timing_player: &deadsync_rules::timing::TimingData,
    global_offset_seconds: f32,
) -> Option<
    deadsync_gameplay::SongLuaVisualLayerRuntime<
        SongLuaOverlayActor,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    >,
> {
    let start_second = deadsync_gameplay::song_lua_time_to_second_like(
        deadsync_gameplay::SongLuaRuntimeTimeUnit::Beat,
        start_beat,
        timing_player,
        global_offset_seconds,
    );
    if !start_second.is_finite() {
        log::warn!(
            "Skipping song lua visual layer for '{}' at beat {:.3}: invalid start time",
            song_title,
            start_beat
        );
        return None;
    }

    let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        timing_player,
        global_offset_seconds,
    );
    let overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    let overlay_eases = build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    );
    let song_foreground_events = build_song_lua_actor_message_events_for_commands(
        &compiled.messages,
        &message_seconds,
        &compiled.song_foreground.message_commands,
    );

    Some(deadsync_gameplay::build_song_lua_visual_layer_runtime(
        start_second,
        compiled.screen_width,
        compiled.screen_height,
        compiled.overlays.clone(),
        overlay_eases,
        overlay_events,
        compiled.song_foreground.clone(),
        song_foreground_events,
    ))
}

fn log_song_lua_runtime_debug(
    song_title: &str,
    compiled: &CompiledSongLua,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    messages: &[deadsync_song_lua::SongLuaMessageEvent],
    hidden_players: &[bool; MAX_PLAYERS],
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
) {
    log::debug!(
        "Song lua runtime detail for '{}': entry='{}' screen_space={:.1}x{:.1} hidden_players={:?} constants={} eases={} column_offsets={} overlay_eases={} overlays={} messages={} sound_assets={} unsupported_targets={} unsupported_function_eases={} unsupported_function_actions={} unsupported_perframes={} skipped_message_commands={}",
        song_title,
        compiled.entry_path.display(),
        compiled.screen_width,
        compiled.screen_height,
        hidden_players,
        total_constant,
        total_eases,
        total_column_offsets,
        overlay_eases.len(),
        compiled.overlays.len(),
        messages.len(),
        compiled.sound_paths.len(),
        unsupported_targets,
        compiled.info.unsupported_function_eases,
        compiled.info.unsupported_function_actions,
        compiled.info.unsupported_perframes,
        compiled.info.skipped_message_command_captures.len(),
    );

    let mut message_counts = std::collections::BTreeMap::<&str, usize>::new();
    for event in messages {
        *message_counts.entry(event.message.as_str()).or_default() += 1;
    }
    if !message_counts.is_empty() {
        log::debug!(
            "Song lua message kinds for '{}': {}",
            song_title,
            message_counts
                .iter()
                .map(|(message, count)| format!("{message}x{count}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !compiled.sound_paths.is_empty() {
        log::debug!(
            "Song lua sound assets for '{}': {}",
            song_title,
            compiled
                .sound_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    if !compiled.info.skipped_message_command_captures.is_empty() {
        log::debug!(
            "Song lua skipped message command captures for '{}': {}",
            song_title,
            compiled.info.skipped_message_command_captures.join(" | ")
        );
    }
    if !compiled
        .info
        .unsupported_function_action_captures
        .is_empty()
    {
        log::debug!(
            "Song lua unsupported function action captures for '{}': {}",
            song_title,
            compiled
                .info
                .unsupported_function_action_captures
                .join(" | ")
        );
    }
    if !compiled.info.unsupported_function_ease_captures.is_empty() {
        log::debug!(
            "Song lua unsupported function ease captures for '{}': {}",
            song_title,
            compiled.info.unsupported_function_ease_captures.join(" | ")
        );
    }
    if !compiled.info.unsupported_perframe_captures.is_empty() {
        log::debug!(
            "Song lua unsupported perframe captures for '{}': {}",
            song_title,
            compiled.info.unsupported_perframe_captures.join(" | ")
        );
    }

    for (index, overlay) in compiled.overlays.iter().enumerate() {
        let message_names = overlay
            .message_commands
            .iter()
            .map(|command| format!("{}({})", command.message, command.blocks.len()))
            .collect::<Vec<_>>();
        log::debug!(
            "Song lua overlay[{index}] for '{}': kind={:?} name={:?} parent={:?} visible={} xy=({:.1},{:.1}) zoom={:.3}/{:.3}/{:.3} rot=({:.1},{:.1},{:.1}) alpha={:.3} msgs=[{}]",
            song_title,
            overlay.kind,
            overlay.name,
            overlay.parent_index,
            overlay.initial_state.visible,
            overlay.initial_state.x,
            overlay.initial_state.y,
            overlay.initial_state.basezoom,
            overlay.initial_state.zoom_x,
            overlay.initial_state.zoom_y,
            overlay.initial_state.rot_x_deg,
            overlay.initial_state.rot_y_deg,
            overlay.initial_state.rot_z_deg,
            overlay.initial_state.diffuse[3],
            message_names.join(", ")
        );
    }

    for (index, ease) in overlay_eases.iter().enumerate() {
        log::trace!(
            "Song lua overlay_ease[{index}] for '{}': overlay={} start_s={:.3} end_s={:.3} sustain_end_s={:.3} cutoff_s={:?} easing={:?} from={:?} to={:?}",
            song_title,
            ease.overlay_index,
            ease.start_second,
            ease.end_second,
            ease.sustain_end_second,
            ease.cutoff_second,
            ease.easing,
            ease.from,
            ease.to
        );
    }
    for (index, event) in messages.iter().enumerate() {
        log::trace!(
            "Song lua message[{index}] for '{}': beat={:.3} message='{}' persists={}",
            song_title,
            event.beat,
            event.message,
            event.persists
        );
    }
}

fn song_lua_runtime_summary_is_notable(
    compiled: &CompiledSongLua,
    overlay_ease_count: usize,
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
) -> bool {
    total_constant > 0
        || total_eases > 0
        || total_column_offsets > 0
        || !compiled.overlays.is_empty()
        || overlay_ease_count > 0
        || !compiled.messages.is_empty()
        || !compiled.sound_paths.is_empty()
        || compiled.info.unsupported_perframes > 0
        || compiled.info.unsupported_function_eases > 0
        || compiled.info.unsupported_function_actions > 0
        || !compiled.info.skipped_message_command_captures.is_empty()
        || unsupported_targets > 0
}

fn log_song_lua_runtime_summary(
    song_title: &str,
    compiled: &CompiledSongLua,
    overlay_ease_count: usize,
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
    compile_ms: f64,
    runtime_ms: f64,
) {
    log::info!(
        "Compiled gameplay lua for '{}' (constants={}, eases={}, column_offsets={}, overlay_eases={}, overlays={}, messages={}, sound_assets={}, unsupported_targets={}, function_eases={}, function_actions={}, perframes={}, skipped_message_commands={}, compile_ms={compile_ms:.3}, runtime_ms={runtime_ms:.3}).",
        song_title,
        total_constant,
        total_eases,
        total_column_offsets,
        overlay_ease_count,
        compiled.overlays.len(),
        compiled.messages.len(),
        compiled.sound_paths.len(),
        unsupported_targets,
        compiled.info.unsupported_function_eases,
        compiled.info.unsupported_function_actions,
        compiled.info.unsupported_perframes,
        compiled.info.skipped_message_command_captures.len(),
    );
}

fn log_unsupported_song_lua_ease_target(
    player: usize,
    window: &deadsync_gameplay::SongLuaRuntimeEaseWindow,
) {
    if let deadsync_gameplay::SongLuaRuntimeEaseTargetOwned::Mod(target_name) = &window.target {
        log::debug!(
            "Unsupported gameplay lua ease target for player {}: target='{}' start={:.3} limit={:.3} span={:?} from={:.3} to={:.3} easing={:?}",
            player + 1,
            target_name,
            window.start,
            window.limit,
            window.span_mode,
            window.from,
            window.to,
            window.easing
        );
    }
}

fn build_song_lua_runtime_windows_for_data(
    params: deadsync_gameplay::SongLuaRuntimeWindowBuild<'_>,
    song_lua_data: GameplaySongLuaData,
) -> deadsync_gameplay::SongLuaRuntimeBuildOutput<
    SongLuaOverlayActor,
    SongLuaCapturedActor,
    SongLuaRuntimeOverlayStateDelta,
> {
    let mut constant_windows: [Vec<deadsync_gameplay::AttackMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut ease_windows: [Vec<deadsync_gameplay::SongLuaEaseMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut overlays = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut overlay_ease_ranges = Vec::new();
    let mut overlay_events = Vec::new();
    let mut background_visual_layers = Vec::new();
    let mut foreground_visual_layers = Vec::new();
    let mut player_actors: [SongLuaCapturedActor; MAX_PLAYERS] = std::array::from_fn(|player| {
        let default = params.player_actor_defaults[player];
        SongLuaCapturedActor {
            initial_state: SongLuaOverlayState {
                x: default.x,
                y: default.y,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        }
    });
    let mut player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut song_foreground = SongLuaCapturedActor::default();
    let mut song_foreground_events = Vec::new();
    let mut hidden_players = [false; MAX_PLAYERS];
    let mut note_hides: [Vec<deadsync_gameplay::SongLuaNoteHideWindowRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut column_offsets: [Vec<deadsync_gameplay::SongLuaColumnOffsetWindowRuntime>;
        MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());

    if song_lua_data.primary.is_none()
        && song_lua_data.background_layers.is_empty()
        && song_lua_data.foreground_layers.is_empty()
    {
        return (
            constant_windows,
            ease_windows,
            deadsync_gameplay::build_song_lua_runtime_visuals(
                overlays,
                overlay_eases,
                overlay_ease_ranges,
                overlay_events,
                background_visual_layers,
                foreground_visual_layers,
                player_actors,
                player_events,
                song_foreground,
                song_foreground_events,
                hidden_players,
                note_hides,
                column_offsets,
                params.screen_width,
                params.screen_height,
            ),
        );
    }

    let mut out_screen_width = params.screen_width;
    let mut out_screen_height = params.screen_height;

    if let Some(primary) = song_lua_data.primary.as_ref() {
        let compiled = &primary.compiled;
        let runtime_started = Instant::now();
        overlays = compiled.overlays.clone();
        let message_seconds = deadsync_gameplay::build_song_lua_message_seconds(
            compiled.messages.iter().map(|message| message.beat),
            params.timing_players[0],
            params.machine_global_offset_seconds,
        );
        overlay_events =
            build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
        let overlay_runtime_eases = build_song_lua_overlay_ease_windows_with_events(
            compiled,
            params.timing_players[0],
            params.machine_global_offset_seconds,
            &overlay_events,
        );
        (overlay_eases, overlay_ease_ranges) = deadsync_gameplay::group_song_lua_overlay_eases(
            compiled.overlays.len(),
            overlay_runtime_eases,
        );
        deadsync_gameplay::apply_song_lua_player_actor_overrides(
            &mut player_actors,
            &compiled.player_actors,
        );
        player_events = deadsync_gameplay::build_song_lua_player_message_events(
            &compiled.player_actors,
            |actor| {
                build_song_lua_actor_message_events_for_commands(
                    &compiled.messages,
                    &message_seconds,
                    &actor.message_commands,
                )
            },
        );
        song_foreground = compiled.song_foreground.clone();
        song_foreground_events = build_song_lua_actor_message_events_for_commands(
            &compiled.messages,
            &message_seconds,
            &compiled.song_foreground.message_commands,
        );
        hidden_players = deadsync_gameplay::build_song_lua_hidden_players(&compiled.hidden_players);
        note_hides = deadsync_gameplay::build_song_lua_note_hide_windows_for_players(
            compiled
                .note_hides
                .iter()
                .map(|hide| (hide.player, hide.column, hide.start_beat, hide.end_beat)),
        );

        let mut unsupported_targets = 0usize;
        let mut total_constant = 0usize;
        let mut total_eases = 0usize;
        let mut total_column_offsets = 0usize;
        let time_mods = song_lua_runtime_mod_windows(&compiled.time_mods);
        let beat_mods = song_lua_runtime_mod_windows(&compiled.beat_mods);
        let eases = song_lua_runtime_ease_windows(&compiled.eases);
        let column_offsets_src = song_lua_runtime_column_offset_windows(&compiled.column_offsets);
        for player in 0..params.num_players {
            let player_global_offset_seconds =
                deadsync_gameplay::effective_player_global_offset_seconds(
                    params.machine_global_offset_seconds,
                    params.player_global_offset_shift_seconds,
                    player,
                );
            let player_windows = deadsync_gameplay::build_song_lua_player_runtime_windows(
                &time_mods,
                &beat_mods,
                &eases,
                &column_offsets_src,
                params.timing_players[player],
                player,
                player_global_offset_seconds,
                |window| log_unsupported_song_lua_ease_target(player, window),
            );
            unsupported_targets += player_windows.unsupported_targets;
            total_constant += player_windows.constant_windows.len();
            total_eases += player_windows.ease_windows.len();
            total_column_offsets += player_windows.column_offsets.len();
            constant_windows[player] = player_windows.constant_windows;
            ease_windows[player] = player_windows.ease_windows;
            column_offsets[player] = player_windows.column_offsets;
        }

        let runtime_ms = runtime_started.elapsed().as_secs_f64() * 1000.0;
        if song_lua_runtime_summary_is_notable(
            compiled,
            overlay_eases.len(),
            total_constant,
            total_eases,
            total_column_offsets,
            unsupported_targets,
        ) {
            log_song_lua_runtime_summary(
                params.song_title,
                compiled,
                overlay_eases.len(),
                total_constant,
                total_eases,
                total_column_offsets,
                unsupported_targets,
                primary.compile_ms,
                runtime_ms,
            );
            log_song_lua_runtime_debug(
                params.song_title,
                compiled,
                &overlay_eases,
                &compiled.messages,
                &hidden_players,
                total_constant,
                total_eases,
                total_column_offsets,
                unsupported_targets,
            );
        }

        out_screen_width = compiled.screen_width;
        out_screen_height = compiled.screen_height;
    }

    for layer_data in &song_lua_data.background_layers {
        let compiled = &layer_data.compiled;
        if let Some(layer) = build_song_lua_compiled_visual_layer_runtime(
            params.song_title,
            layer_data.start_beat,
            compiled,
            params.timing_players[0],
            params.machine_global_offset_seconds,
        ) {
            background_visual_layers.push(layer);
        }
    }

    for layer_data in &song_lua_data.foreground_layers {
        let compiled = &layer_data.compiled;
        if let Some(layer) = build_song_lua_compiled_visual_layer_runtime(
            params.song_title,
            layer_data.start_beat,
            compiled,
            params.timing_players[0],
            params.machine_global_offset_seconds,
        ) {
            foreground_visual_layers.push(layer);
        }
    }

    (
        constant_windows,
        ease_windows,
        deadsync_gameplay::build_song_lua_runtime_visuals(
            overlays,
            overlay_eases,
            overlay_ease_ranges,
            overlay_events,
            background_visual_layers,
            foreground_visual_layers,
            player_actors,
            player_events,
            song_foreground,
            song_foreground_events,
            hidden_players,
            note_hides,
            column_offsets,
            out_screen_width,
            out_screen_height,
        ),
    )
}

fn build_background_changes(
    song: &SongData,
    gameplay_chart: &GameplayChartData,
    random_movie_paths: Vec<PathBuf>,
) -> Vec<SongBackgroundChange> {
    if random_movie_paths.is_empty() {
        return song.background_changes.clone();
    }
    let seed_text = song
        .simfile_path
        .parent()
        .map(|path| path.to_string_lossy())
        .unwrap_or_else(|| song.simfile_path.to_string_lossy());
    expand_random_background_changes(
        song,
        &gameplay_chart.timing,
        &gameplay_chart.timing_segments,
        random_movie_paths,
        seed_text.as_ref(),
    )
}

fn random_background_movies_enabled() -> bool {
    matches!(
        crate::config::get().random_background_mode,
        crate::config::RandomBackgroundMode::RandomMovies
    )
}

pub fn init(
    song: Arc<SongData>,
    charts: [Arc<ChartData>; MAX_PLAYERS],
    gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    viewport: GameplayViewport,
    session: GameplaySession,
    config: GameplayConfig,
    active_color_index: i32,
    music_rate: f32,
    scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    player_profiles: [profile_data::Profile; MAX_PLAYERS],
    replay_edges: Option<Vec<ReplayInputEdge>>,
    replay_offsets: Option<ReplayOffsetSnapshot>,
    replay_status_text: Option<Arc<str>>,
    stage_intro_text: Arc<str>,
    lead_in_timing: Option<LeadInTiming>,
    course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    course_display_timing: Option<CourseDisplayTiming>,
    course_display_info: Option<CourseDisplayInfo>,
    course_banner_path: Option<PathBuf>,
    combo_carry: [u32; MAX_PLAYERS],
) -> State {
    let random_movie_paths =
        crate::game::random_movies::random_movie_paths(&song, random_background_movies_enabled());
    let cols_per_player = session.play_style.cols_per_player();
    let num_players = session.play_style.player_count();
    let runtime_profile_data = gameplay_runtime_profile_data(&player_profiles, &session);
    let runtime_charts = gameplay_runtime_charts(&charts, &session);
    let noteskin_assets =
        gameplay_noteskin_assets(cols_per_player, num_players, &runtime_profile_data);
    let noteskin_data =
        noteskin_assets.gameplay_data(cols_per_player, num_players, &runtime_profile_data);
    let song_lua_data = gameplay_song_lua_data(
        &song,
        &charts,
        &player_profiles,
        &scroll_speed,
        music_rate,
        viewport,
        &session,
        &config,
    );
    let player_profiles = player_profiles.map(GameplayProfile::from);
    let song_lua_sound_paths = song_lua_sound_paths(&song_lua_data);
    let background_chart = if session.p2_runtime_player() {
        &gameplay_charts[1]
    } else {
        &gameplay_charts[0]
    };
    let background_changes = build_background_changes(&song, background_chart, random_movie_paths);
    let (pack_group, pack_banner_path, pack_sync_pref) = gameplay_pack_data(
        &song,
        course_display_info.as_ref(),
        course_banner_path.as_ref(),
    );
    let mini_indicator_data =
        gameplay_mini_indicator_data(&runtime_charts, &runtime_profile_data, &session);
    let scorebox_data = gameplay_scorebox_data(&runtime_charts, &runtime_profile_data, &session);
    State::from_gameplay_with_screen_data(
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
            gameplay_crossover_annotations_for_player,
            active_color_index,
            music_rate,
            scroll_speed,
            player_profiles,
            replay_edges,
            replay_offsets,
            lead_in_timing,
            course_display_carry,
            course_display_totals,
            course_display_timing,
            combo_carry,
        ),
        noteskin_assets,
        song_lua_sound_paths,
        background_changes,
        stage_intro_text,
        replay_status_text,
        course_display_info,
        pack_group,
        pack_banner_path,
        scorebox_data,
    )
}

#[inline(always)]
const fn screen_for_exit(exit: GameplayExit) -> Screen {
    match exit {
        GameplayExit::Complete => Screen::Evaluation,
        GameplayExit::Cancel => Screen::SelectMusic,
    }
}

#[inline(always)]
const fn map_gameplay_action(action: GameplayAction) -> ScreenAction {
    match action {
        GameplayAction::None => ScreenAction::None,
        GameplayAction::Navigate(exit) => ScreenAction::Navigate(screen_for_exit(exit)),
        GameplayAction::NavigateNoFade(exit) => ScreenAction::NavigateNoFade(screen_for_exit(exit)),
    }
}

fn local_lobby_side_is_active(side: profile_data::PlayerSide) -> bool {
    let p1_joined = profile::is_session_side_joined(profile_data::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile_data::PlayerSide::P2);
    if !(p1_joined || p2_joined) {
        return profile::get_session_player_side() == side;
    }
    match side {
        profile_data::PlayerSide::P1 => p1_joined,
        profile_data::PlayerSide::P2 => p2_joined,
    }
}

fn intro_text_width(asset_manager: &AssetManager, text: &str) -> f32 {
    let font_key = current_machine_font_key(FontRole::Header);
    asset_manager.with_fonts(|all_fonts| {
        asset_manager
            .with_font(font_key, |f| {
                font::measure_line_width_logical(f, text, all_fonts) as f32
            })
            .unwrap_or(0.0)
            .max(0.0)
    })
}

fn intro_text_target_x(
    state: &State,
    asset_manager: &AssetManager,
    text: &str,
    play_style: profile_data::PlayStyle,
    player_side: profile_data::PlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    let centered_notefield = state.num_players() == 1
        && (play_style == profile_data::PlayStyle::Double
            || (play_style == profile_data::PlayStyle::Single && center_1player_notefield));
    if !centered_notefield || state.cols_per_player() == 0 {
        return screen_center_x();
    }

    // Simply Love ScreenGameplay in/default.lua: when one human player's
    // notefield is centered, move the Stage/Event text outside GetNotefieldWidth().
    let side_sign = match player_side {
        profile_data::PlayerSide::P1 => -1.0,
        profile_data::PlayerSide::P2 => 1.0,
    };
    let notefield_width = state.cols_per_player() as f32 * 64.0;
    screen_center_x()
        + (notefield_width * 0.5 + intro_text_width(asset_manager, text) * INTRO_TEXT_GETWIDTH_PAD)
            * side_sign
}

fn gameplay_player_index_for_side(state: &State, side: profile_data::PlayerSide) -> Option<usize> {
    if state.num_players() >= 2 {
        return Some(profile_data::player_side_index(side));
    }
    if state.num_players() == 0 || profile::get_session_player_side() != side {
        return None;
    }
    Some(0)
}

#[derive(Clone, Copy)]
struct StepStatsScorePos {
    score_x: f32,
    score_y: f32,
    hard_ex_x: f32,
    hard_ex_y: f32,
}

fn step_stats_score_pos(
    player_side: profile_data::PlayerSide,
    score_x_other: f32,
    note_field_is_centered: bool,
) -> StepStatsScorePos {
    match (player_side, note_field_is_centered) {
        (profile_data::PlayerSide::P1, true) => StepStatsScorePos {
            score_x: score_x_other + widescale(-75.0, -124.0),
            score_y: widescale(150.0, 92.0),
            hard_ex_x: score_x_other + widescale(-74.0, -123.0),
            hard_ex_y: widescale(146.0, 90.0),
        },
        (profile_data::PlayerSide::P1, false) => StepStatsScorePos {
            score_x: score_x_other + widescale(-167.0, -244.0),
            score_y: 75.0,
            hard_ex_x: score_x_other + widescale(-166.0, -243.0),
            hard_ex_y: 73.0,
        },
        (profile_data::PlayerSide::P2, true) => StepStatsScorePos {
            score_x: score_x_other + widescale(32.0, 65.0),
            score_y: widescale(150.0, 92.0),
            hard_ex_x: score_x_other + widescale(-20.0, 12.0),
            hard_ex_y: widescale(146.0, 90.0),
        },
        (profile_data::PlayerSide::P2, false) => StepStatsScorePos {
            score_x: score_x_other + widescale(141.0, 189.0),
            score_y: 75.0,
            hard_ex_x: score_x_other + widescale(88.0, 135.0),
            hard_ex_y: 73.0,
        },
    }
}

#[inline(always)]
fn ranges_overlap(a_center: f32, a_size: f32, b_center: f32, b_size: f32) -> bool {
    let a_half = a_size * 0.5;
    let b_half = b_size * 0.5;
    a_center - a_half < b_center + b_half && b_center - b_half < a_center + a_half
}

fn saved_targets_hit_meter(profile: &profile_data::Profile, num_cols: usize, meter_y: f32) -> bool {
    if num_cols == 0 || !meter_y.is_finite() {
        return false;
    }

    let offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let receptor_y_normal = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + offset_y;
    let receptor_y_reverse = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + offset_y;
    let receptor_y_centered = screen_center_y() + offset_y;
    // This HUD dodge follows the player's chosen layout only. Live song
    // Lua/attack effects may move receptors, but should not move the rating box.
    let scroll = scroll_effects_from_option(profile.scroll_option);

    (0..num_cols).any(|col| {
        let receptor_y = scroll_receptor_y(
            scroll.reverse_percent_for_column(col, num_cols),
            scroll.centered,
            receptor_y_normal,
            receptor_y_reverse,
            receptor_y_centered,
        );
        ranges_overlap(
            receptor_y,
            TARGET_ARROW_PIXEL_SIZE,
            meter_y,
            DIFFICULTY_METER_SIZE,
        )
    })
}

fn difficulty_meter_hits_targets(
    state: &State,
    profile: &profile_data::Profile,
    player_idx: usize,
    field_x: f32,
    field_w: f32,
    meter_x: f32,
    meter_y: f32,
) -> bool {
    if player_idx >= state.num_players()
        || !field_x.is_finite()
        || !field_w.is_finite()
        || !meter_x.is_finite()
        || !meter_y.is_finite()
        || field_w <= 0.0
    {
        return false;
    }
    if !ranges_overlap(field_x, field_w, meter_x, DIFFICULTY_METER_SIZE) {
        return false;
    }

    let col_start = player_idx.saturating_mul(state.cols_per_player());
    let num_cols = (col_start + state.cols_per_player())
        .min(state.num_cols())
        .saturating_sub(col_start);
    if num_cols == 0 {
        return false;
    }

    saved_targets_hit_meter(profile, num_cols, meter_y)
}

#[inline(always)]
fn side_difficulty_meter_x(player_side: profile_data::PlayerSide) -> f32 {
    match player_side {
        profile_data::PlayerSide::P1 => DIFFICULTY_METER_SIZE * 0.5,
        profile_data::PlayerSide::P2 => screen_width() - DIFFICULTY_METER_SIZE * 0.5,
    }
}

fn difficulty_meter_x(
    state: &State,
    profile: &profile_data::Profile,
    player_idx: usize,
    player_side: profile_data::PlayerSide,
    field_x: f32,
    field_w: f32,
    normal_x: f32,
) -> f32 {
    if difficulty_meter_hits_targets(
        state,
        profile,
        player_idx,
        field_x,
        field_w,
        normal_x,
        DIFFICULTY_METER_Y,
    ) {
        side_difficulty_meter_x(player_side)
    } else {
        normal_x
    }
}

fn gameplay_lobby_player_stats(
    state: &State,
    side: profile_data::PlayerSide,
) -> Option<lobby_data::MachinePlayerStats> {
    let player_idx = gameplay_player_index_for_side(state, side)?;
    let blue_window_ms = player_blue_window_ms(state, player_idx);
    let ex_data = state.display_ex_score_data(player_idx, blue_window_ms);
    let judgments = lobby_data::LobbyJudgments {
        fantastic_plus: ex_data.counts.w0,
        fantastics: ex_data.counts.w1,
        excellents: ex_data.counts.w2,
        greats: ex_data.counts.w3,
        decents: ex_data.counts.w4,
        way_offs: ex_data.counts.w5,
        misses: ex_data.counts.miss,
        total_steps: ex_data.total_steps,
        mines_hit: ex_data.mines_hit,
        total_mines: ex_data.mines_total,
        holds_held: ex_data.holds_held,
        total_holds: ex_data.holds_total,
        rolls_held: ex_data.rolls_held,
        total_rolls: ex_data.rolls_total,
    };
    Some(lobby_data::MachinePlayerStats {
        judgments: Some(judgments),
        score: Some((state.display_itg_score_percent(player_idx) * 100.0) as f32),
        ex_score: Some(state.display_ex_score_percent(player_idx, blue_window_ms) as f32),
    })
}

fn update_lobby_machine_state(state: &State) {
    if !crate::game::online::lobbies::can_update_machine_state() {
        return;
    }

    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    crate::game::online::lobbies::update_machine_state_sides_with_stats(
        "ScreenGameplay",
        p1_ready,
        p2_ready,
        gameplay_lobby_player_stats(state, profile_data::PlayerSide::P1),
        gameplay_lobby_player_stats(state, profile_data::PlayerSide::P2),
    );
}

fn local_lobby_ready_tuple(state: &State) -> (bool, bool) {
    (
        local_lobby_side_is_active(profile_data::PlayerSide::P1) && state.lobby_ready_p1,
        local_lobby_side_is_active(profile_data::PlayerSide::P2) && state.lobby_ready_p2,
    )
}

fn local_lobby_players_ready(state: &State) -> bool {
    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    let mut any_active = false;
    let mut all_ready = true;
    if local_lobby_side_is_active(profile_data::PlayerSide::P1) {
        any_active = true;
        all_ready &= p1_ready;
    }
    if local_lobby_side_is_active(profile_data::PlayerSide::P2) {
        any_active = true;
        all_ready &= p2_ready;
    }
    any_active && all_ready
}

fn set_all_local_lobby_players_ready(state: &mut State, ready: bool) {
    state.lobby_ready_p1 = local_lobby_side_is_active(profile_data::PlayerSide::P1) && ready;
    state.lobby_ready_p2 = local_lobby_side_is_active(profile_data::PlayerSide::P2) && ready;
}

fn set_local_lobby_player_ready(state: &mut State, side: profile_data::PlayerSide) {
    match side {
        profile_data::PlayerSide::P1
            if local_lobby_side_is_active(profile_data::PlayerSide::P1) =>
        {
            state.lobby_ready_p1 = true;
        }
        profile_data::PlayerSide::P2
            if local_lobby_side_is_active(profile_data::PlayerSide::P2) =>
        {
            state.lobby_ready_p2 = true;
        }
        _ => {}
    }
}

fn clear_lobby_disconnect_holds(state: &mut State) {
    state.lobby_disconnect_hold_p1 = None;
    state.lobby_disconnect_hold_p2 = None;
}

fn set_lobby_disconnect_hold(
    state: &mut State,
    side: profile_data::PlayerSide,
    started_at: Option<Instant>,
) {
    match side {
        profile_data::PlayerSide::P1
            if local_lobby_side_is_active(profile_data::PlayerSide::P1) =>
        {
            state.lobby_disconnect_hold_p1 = started_at;
        }
        profile_data::PlayerSide::P2
            if local_lobby_side_is_active(profile_data::PlayerSide::P2) =>
        {
            state.lobby_disconnect_hold_p2 = started_at;
        }
        _ => {}
    }
}

fn lobby_disconnect_hold_elapsed(state: &State) -> Option<f32> {
    [
        state.lobby_disconnect_hold_p1,
        state.lobby_disconnect_hold_p2,
    ]
    .into_iter()
    .flatten()
    .map(|started_at| started_at.elapsed().as_secs_f32())
    .max_by(f32::total_cmp)
}

fn lobby_player_on_screen(player: &lobby_data::LobbyPlayer, screen_name: &str) -> bool {
    player.screen_name.eq_ignore_ascii_case(screen_name)
}

fn gameplay_requires_lobby_wait_for(joined: Option<&lobby_data::JoinedLobby>) -> bool {
    joined.is_some()
}

fn gameplay_requires_lobby_wait() -> bool {
    let snapshot = crate::game::online::lobbies::snapshot();
    gameplay_requires_lobby_wait_for(snapshot.joined_lobby.as_ref())
}

fn gameplay_lobby_wait_text_for(
    joined: &lobby_data::JoinedLobby,
    local_players_ready: bool,
    reconnect_status_text: Option<&str>,
) -> Option<String> {
    if let Some(text) = reconnect_status_text {
        return Some(text.to_string());
    }

    let all_in_gameplay = !joined.players.is_empty()
        && joined
            .players
            .iter()
            .all(|player| lobby_player_on_screen(player, "ScreenGameplay"));
    let all_ready = !joined.players.is_empty() && joined.players.iter().all(|player| player.ready);
    if all_in_gameplay && all_ready {
        return None;
    }

    let mut message = if all_in_gameplay {
        tr("Lobby", "WaitingForReadyUp").to_string()
    } else {
        tr("Lobby", "WaitingForSync").to_string()
    };
    if !local_players_ready {
        message.push('\n');
        message.push_str(&tr("Gameplay", "PressStartToReadyUp"));
    }
    Some(message)
}

fn gameplay_lobby_wait_text(state: &State) -> Option<String> {
    if state.lobby_music_started {
        return None;
    }

    let snapshot = crate::game::online::lobbies::snapshot();
    let joined = snapshot.joined_lobby.as_ref()?;
    let reconnect_status_text = crate::game::online::lobbies::reconnect_status_text();
    gameplay_lobby_wait_text_for(
        joined,
        local_lobby_players_ready(state),
        reconnect_status_text.as_deref(),
    )
}

fn gameplay_lobby_disconnect_prompt(state: &State) -> Option<String> {
    gameplay_lobby_wait_text(state)?;
    let Some(elapsed) = lobby_disconnect_hold_elapsed(state) else {
        return Some(tr("Lobby", "DisconnectBasicPrompt").to_string());
    };
    let remaining =
        (crate::game::online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS - elapsed).ceil() as i32;
    let remaining = remaining.max(0);
    Some(
        tr_fmt(
            "Lobby",
            "DisconnectHoldingFormat",
            &[
                ("remaining", &remaining.to_string()),
                ("s", if remaining == 1 { "" } else { "s" }),
            ],
        )
        .to_string(),
    )
}

fn gameplay_lobby_hud_status_text(state: &State) -> Option<String> {
    let mut text = gameplay_lobby_wait_text(state)?;
    if let Some(prompt) = gameplay_lobby_disconnect_prompt(state) {
        text.push('\n');
        text.push_str(prompt.as_str());
    }
    Some(text)
}

#[inline(always)]
fn audio_cut(cut: GameplayMusicCut) -> deadsync_audio_stream::Cut {
    deadsync_audio_stream::Cut {
        start_sec: cut.start_sec,
        length_sec: cut.length_sec,
        fade_in_sec: cut.fade_in_sec,
        fade_out_sec: cut.fade_out_sec,
    }
}

pub fn audio_snapshot() -> GameplayAudioSnapshot {
    let stream_clock = deadsync_audio_stream::get_music_stream_clock_snapshot();
    let output_timing = deadsync_audio_stream::get_output_timing_snapshot();
    GameplayAudioSnapshot {
        stream_clock: GameplayStreamClockSnapshot {
            stream_seconds: stream_clock.stream_seconds,
            music_nanos: stream_clock.music_nanos,
            music_seconds_per_second: stream_clock.music_seconds_per_second,
            has_music_mapping: stream_clock.has_music_mapping,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        },
        assist_sfx_generation: deadsync_audio_stream::assist_sfx_generation(),
        output_delay_seconds: output_timing.estimated_output_delay_ns as f32 * 1e-9,
        timing_diag_enabled: deadsync_audio_stream::timing_diag_enabled(),
        timing_diag_callback_gap_ns: deadsync_audio_stream::timing_diag_last_callback_gap_ns(),
    }
}

pub fn drain_core_audio_commands(state: &mut GameplayCoreState) {
    for command in state.drain_audio_commands() {
        match command {
            GameplayAudioCommand::StopMusic => {
                if deadsync_audio_stream::is_initialized() {
                    deadsync_audio_stream::stop_music();
                }
            }
            GameplayAudioCommand::PlayMusic {
                path,
                cut,
                looping,
                rate,
            } => deadsync_audio_stream::play_music(path, audio_cut(cut), looping, rate),
            GameplayAudioCommand::PlayPreloadedSfx(path) => {
                deadsync_audio_stream::play_preloaded_sfx(path);
            }
            GameplayAudioCommand::PlayPreloadedAssistTick(path) => {
                deadsync_audio_stream::play_preloaded_assist_tick(path);
            }
            GameplayAudioCommand::PlayAssistTickAtMusicTime {
                path,
                music_seconds,
            } => {
                if let Some(frame) =
                    deadsync_audio_stream::assist_tick_stream_frame_for_music_seconds(music_seconds)
                {
                    deadsync_audio_stream::play_scheduled_assist_tick(path, frame);
                } else {
                    deadsync_audio_stream::play_preloaded_assist_tick(path);
                }
            }
        }
    }
}

pub fn drain_core_session_commands(state: &mut GameplayCoreState) {
    for command in state.drain_session_commands() {
        match command {
            GameplaySessionCommand::SetTimingTickMode(mode) => {
                crate::game::profile::set_session_timing_tick_mode(
                    profile_tick_mode_from_gameplay(mode),
                );
            }
        }
    }
}

#[inline(always)]
pub fn scorebox_snapshot_for_side(
    state: &State,
    side: profile_data::PlayerSide,
) -> Option<&score_data::CachedPlayerLeaderboardData> {
    state.scorebox_side_snapshot[profile_data::player_side_index(side)].as_ref()
}

#[inline(always)]
pub fn scorebox_profile_for_side(
    state: &State,
    side: profile_data::PlayerSide,
) -> &score_data::GameplayScoreboxProfileSnapshot {
    &state.scorebox_profile_snapshot[profile_data::player_side_index(side)]
}

pub fn refresh_scorebox_snapshots(state: &mut State) {
    for p in 0..state.num_players() {
        let side = profile_side_from_gameplay(state.runtime_player_side(p));
        let idx = profile_data::player_side_index(side);
        let profile_snapshot = &state.scorebox_profile_snapshot[idx];
        if !profile_snapshot.display_scorebox || !profile_snapshot.gs_active {
            continue;
        }
        let needs_refresh = state.scorebox_side_snapshot[idx]
            .as_ref()
            .is_some_and(|snapshot| snapshot.loading);
        if !needs_refresh {
            continue;
        }
        let chart_hash = state.charts()[p].short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }
        if let Some(fresh) = scores::get_or_fetch_player_leaderboards_for_profile(
            chart_hash,
            profile_snapshot,
            gs_scorebox::SCOREBOX_NUM_ENTRIES,
        ) {
            if !fresh.loading {
                state.scorebox_side_snapshot[idx] = Some(fresh);
            }
        }
    }
}

pub fn drain_core_commands(state: &mut GameplayCoreState) {
    drain_core_audio_commands(state);
    drain_core_session_commands(state);
}

pub fn drain_audio_commands(state: &mut State) {
    drain_core_commands(&mut state.gameplay);
}

pub fn on_enter(state: &mut State) {
    state.lobby_music_started = false;
    set_all_local_lobby_players_ready(state, false);
    clear_lobby_disconnect_holds(state);

    for (store_idx, sdk_pad) in smx_fsr_display_pads(state).into_iter().flatten() {
        deadsync_smx::set_test_mode(sdk_pad, SensorTestMode::CalibratedValues);
        state.smx_sensor_config[store_idx] = deadsync_smx::get_config(sdk_pad);
    }

    if gameplay_requires_lobby_wait() {
        return;
    }

    set_all_local_lobby_players_ready(state, true);
    state.start_stage_music();
    drain_audio_commands(state);
    state.lobby_music_started = true;
}

pub fn on_exit(state: &mut State) {
    // Always clear test mode for both pads, even ones we never cached a config
    // for on enter (e.g. get_config returned None). A pad left in test mode keeps
    // streaming sensor data over the wire on later screens like the song wheel.
    // set_test_mode is a no-op when SMX is uninitialized or the pad is absent.
    for pad in 0..2usize {
        deadsync_smx::set_test_mode(pad, SensorTestMode::Off);
    }
    state.smx_sensor_data = [None, None];
    state.smx_sensor_config = [None, None];
}

pub fn update(state: &mut State, delta_time: f32) -> ScreenAction {
    crate::game::online::lobbies::poll_reconnect();

    if !state.lobby_music_started {
        if lobby_disconnect_hold_elapsed(state).is_some_and(|elapsed| {
            elapsed >= crate::game::online::lobbies::LOBBY_DISCONNECT_HOLD_SECONDS
        }) {
            clear_lobby_disconnect_holds(state);
            crate::game::online::lobbies::disconnect();
        }

        update_lobby_machine_state(state);

        if gameplay_lobby_wait_text(state).is_some() {
            return ScreenAction::None;
        }

        clear_lobby_disconnect_holds(state);
        set_all_local_lobby_players_ready(state, true);
        state.start_stage_music();
        drain_audio_commands(state);
        state.lobby_music_started = true;
    }
    update_lobby_machine_state(state);
    maybe_refresh_smx_sensor_data(state, delta_time);
    smx_profile::maybe_report();
    let previous_song_lua_time = state.current_music_time_display();
    let action = update_core(state, delta_time, audio_snapshot(), || {
        deadlib_platform::host_time::instant_nanos(Instant::now())
    });
    if matches!(action, GameplayAction::None) {
        refresh_scorebox_snapshots(state);
    }
    drain_audio_commands(state);
    play_song_lua_sound_events(
        state,
        previous_song_lua_time,
        state.current_music_time_display(),
    );
    map_gameplay_action(action)
}

fn song_lua_sound_time_crossed(previous: f32, now: f32, event_second: f32) -> bool {
    if !event_second.is_finite() {
        return false;
    }
    let starts_at_zero = previous <= 0.0 && event_second.abs() <= f32::EPSILON;
    (event_second > previous || starts_at_zero) && event_second <= now
}

fn play_song_lua_sound_events(state: &State, previous: f32, now: f32) {
    if !previous.is_finite() || !now.is_finite() || now < previous {
        return;
    }
    let song_lua_visuals = state.song_lua_visuals();
    play_song_lua_sound_events_for(
        &song_lua_visuals.overlays,
        &song_lua_visuals.overlay_events,
        previous,
        now,
    );
    for layer in &song_lua_visuals.background_visual_layers {
        play_song_lua_sound_events_for(&layer.overlays, &layer.overlay_events, previous, now);
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        play_song_lua_sound_events_for(&layer.overlays, &layer.overlay_events, previous, now);
    }
}

fn play_song_lua_sound_events_for(
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    previous: f32,
    now: f32,
) {
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let SongLuaOverlayKind::Sound { sound_path } = &overlay.kind else {
            continue;
        };
        let Some(events) = overlay_events.get(overlay_index) else {
            continue;
        };
        for event in events {
            let Some(command) = overlay.message_commands.get(event.command_index) else {
                continue;
            };
            for block in &command.blocks {
                if block.delta.sound_play != Some(true) {
                    continue;
                }
                let play_second = event.event_second + block.start;
                if song_lua_sound_time_crossed(previous, now, play_second) {
                    let key = sound_path.to_string_lossy();
                    deadsync_audio_stream::play_preloaded_sfx(key.as_ref());
                }
            }
        }
    }
}

/// The pads to drive for the FSR sensor display, as `(store_index, sdk_pad)`:
/// `store_index` is how the sensor arrays are keyed (profile index normally, SDK
/// pad in Doubles) and `sdk_pad` is the SDK pad to enable/read. `None` slots are
/// skipped. Returns all-`None` cheaply (before any config/session lookup) when no
/// player wants the display or SMX input is off, so the per-frame caller does no
/// further work.
fn smx_fsr_display_pads(state: &State) -> [Option<(usize, usize)>; 2] {
    let mut out = [None, None];
    if !state.profiles()[0].smx_fsr_display && !state.profiles()[1].smx_fsr_display {
        return out;
    }
    if !crate::config::get().smx_input {
        return out;
    }
    if profile::get_session_play_style() == profile_data::PlayStyle::Double {
        // One player drives both pads; key the sensor arrays by SDK pad.
        if state.profiles()[0].smx_fsr_display {
            out = [Some((0, 0)), Some((1, 1))];
        }
        return out;
    }
    // Each FSR-display player keys by profile index but reads its SIDE's SDK pad
    // (P1 -> 0, P2 -> 1); a single P2 player is profile 0 but plays pad 1.
    let mut n = 0;
    for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
        let Some(pidx) = gameplay_player_index_for_side(state, side) else {
            continue;
        };
        if !state.profiles()[pidx].smx_fsr_display {
            continue;
        }
        out[n] = Some((pidx, profile_data::player_side_index(side)));
        n += 1;
    }
    out
}

// The pad streams sensor data at ~30Hz on the wire (the SDK requests it on a
// fixed interval), so reading it once per render frame is wasted work that
// scales with the (vsync-off) frame rate and needlessly contends the SDK's
// shared state lock. Sample on a fixed timer instead. 60Hz comfortably
// oversamples the 30Hz source while decoupling the read cost from frame rate.
const SMX_SENSOR_REFRESH_HZ: f32 = 60.0;
const SMX_SENSOR_REFRESH_INTERVAL: f32 = 1.0 / SMX_SENSOR_REFRESH_HZ;

fn maybe_refresh_smx_sensor_data(state: &mut State, delta_time: f32) {
    state.smx_sensor_refresh_accum += delta_time;
    if state.smx_sensor_refresh_accum < SMX_SENSOR_REFRESH_INTERVAL {
        return;
    }
    // Keep the leftover so cadence stays steady, but cap it so a long stall
    // (load spike, alt-tab) can't bank up a burst of catch-up refreshes.
    state.smx_sensor_refresh_accum = (state.smx_sensor_refresh_accum - SMX_SENSOR_REFRESH_INTERVAL)
        .min(SMX_SENSOR_REFRESH_INTERVAL);
    smx_profile::time_read(|| refresh_smx_sensor_data(state));
}

fn refresh_smx_sensor_data(state: &mut State) {
    for (store_idx, sdk_pad) in smx_fsr_display_pads(state).into_iter().flatten() {
        state.smx_sensor_data[store_idx] = deadsync_smx::get_test_data(sdk_pad);
        if state.smx_sensor_config[store_idx].is_none() {
            state.smx_sensor_config[store_idx] = deadsync_smx::get_config(sdk_pad);
        }
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if gameplay_lobby_wait_text(state).is_some() {
        match ev.action {
            VirtualAction::p1_start => {
                if ev.pressed {
                    set_local_lobby_player_ready(state, profile_data::PlayerSide::P1);
                    set_lobby_disconnect_hold(
                        state,
                        profile_data::PlayerSide::P1,
                        Some(ev.timestamp),
                    );
                } else {
                    set_lobby_disconnect_hold(state, profile_data::PlayerSide::P1, None);
                }
            }
            VirtualAction::p2_start => {
                if ev.pressed {
                    set_local_lobby_player_ready(state, profile_data::PlayerSide::P2);
                    set_lobby_disconnect_hold(
                        state,
                        profile_data::PlayerSide::P2,
                        Some(ev.timestamp),
                    );
                } else {
                    set_lobby_disconnect_hold(state, profile_data::PlayerSide::P2, None);
                }
            }
            _ => {}
        }
        return ScreenAction::None;
    }
    let action = handle_core_input(state, ev);
    drain_audio_commands(state);
    map_gameplay_action(action)
}

thread_local! {
    static SCORE_2DP_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(1024));
    static RATE_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(128));
    static BPM_TEXT_CACHE: RefCell<TextCache<(u64, bool)>> = RefCell::new(HashMap::with_capacity(512));
    static LIFE_PERCENT_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(HashMap::with_capacity(1024));
    static METER_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(64));
    static AUTOSYNC_TEXT_CACHE: RefCell<TextCache<AutosyncTextKey>> =
        RefCell::new(HashMap::with_capacity(256));
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct AutosyncTextKey {
    mode: u8,
    old_offset_bits: u32,
    new_offset_bits: u32,
    stddev_bits: u32,
    sample_count: u16,
}

#[inline(always)]
fn empty_text() -> Arc<str> {
    static EMPTY: OnceLock<Arc<str>> = OnceLock::new();
    EMPTY.get_or_init(|| Arc::<str>::from("")).clone()
}

#[inline(always)]
fn quantize_centi_u32(value: f64) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 100.0).round()).clamp(0.0, u32::MAX as f64) as u32
}

#[inline(always)]
fn quantize_tenths_u32(value: f32) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 10.0).round()).clamp(0.0, u32::MAX as f32) as u32
}

#[inline(always)]
fn cached_score_2dp(value: f64) -> Arc<str> {
    let key = quantize_centi_u32(value);
    cached_text(&SCORE_2DP_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.2}", key as f64 / 100.0)
    })
}

#[inline(always)]
fn cached_rate_text(rate: f32) -> Arc<str> {
    if (rate - 1.0).abs() <= 0.001 {
        return empty_text();
    }
    cached_text(&RATE_TEXT_CACHE, rate.to_bits(), TEXT_CACHE_LIMIT, || {
        tr_fmt(
            "Gameplay",
            "RateDisplay",
            &[("rate", &format!("{rate:.2}"))],
        )
        .to_string()
    })
}

#[inline(always)]
fn cached_bpm_text(bpm: f64, show_decimal: bool) -> Arc<str> {
    if !bpm.is_finite() {
        return Arc::<str>::from("0");
    }
    if !show_decimal {
        let rounded = bpm.round().max(0.0);
        let key = (rounded.to_bits(), false);
        return cached_text(&BPM_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
            format!("{rounded:.0}")
        });
    }
    let rounded_tenth = (bpm * 10.0).round() / 10.0;
    let rounded_tenth = rounded_tenth.max(0.0);
    let key = (rounded_tenth.to_bits(), true);
    cached_text(&BPM_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        let nearest_int = rounded_tenth.round();
        if (rounded_tenth - nearest_int).abs() <= 0.001 {
            format!("{nearest_int:.0}")
        } else {
            format!("{rounded_tenth:.1}")
        }
    })
}

#[inline(always)]
fn cached_life_percent_text(life_percent: f32) -> Arc<str> {
    let key = quantize_tenths_u32(life_percent);
    cached_text(&LIFE_PERCENT_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.1}%", key as f32 / 10.0)
    })
}

#[inline(always)]
fn cached_meter_text(meter: u32) -> Arc<str> {
    cached_text(&METER_TEXT_CACHE, meter, TEXT_CACHE_LIMIT, || {
        meter.to_string()
    })
}

#[inline(always)]
fn quantize_offset_seconds(v: f32) -> f32 {
    let step = 0.001_f32;
    (v / step).round() * step
}

#[inline(always)]
fn quantized_offset_change_line(label: &str, start: f32, new: f32) -> Option<String> {
    let start_q = quantize_offset_seconds(start);
    let new_q = quantize_offset_seconds(new);
    let delta_q = new_q - start_q;
    if delta_q.abs() < 0.000_1_f32 {
        return None;
    }
    let direction = if delta_q > 0.0 { "earlier" } else { "later" };
    Some(format!(
        "{label} from {start_q:+.3} to {new_q:+.3} (notes {direction})"
    ))
}

fn sync_offset_overlay_message(state: &State) -> Option<String> {
    let mut message = String::new();
    if let Some(global_line) = quantized_offset_change_line(
        "Global Offset",
        state.initial_global_offset_seconds(),
        state.global_offset_seconds(),
    ) {
        message.push_str(&global_line);
    }
    if let Some(song_line) = quantized_offset_change_line(
        "Song offset",
        state.initial_song_offset_seconds(),
        state.song_offset_seconds(),
    ) {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(&song_line);
    }
    (!message.is_empty()).then_some(message)
}

fn sync_overlay_text(state: &State) -> Option<(Arc<str>, usize)> {
    let mut lines = [""; 4];
    let mut line_count = 0usize;
    let mut total_len = 0usize;
    let sync_message = sync_offset_overlay_message(state);
    if state.autoplay_enabled() {
        let line = state.replay_status_text.as_deref().unwrap_or("AutoPlay");
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = state.timing_tick_status_line() {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = autosync_mode_status_line(state.autosync_mode()) {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = sync_message.as_deref() {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if line_count == 0 {
        return None;
    }
    // Offset overlay text changes during live tweaks, so build this combined
    // string from current state instead of caching by pointer identity.
    let mut out = String::with_capacity(total_len + line_count.saturating_sub(1));
    out.push_str(lines[0]);
    for line in &lines[1..line_count] {
        out.push('\n');
        out.push_str(line);
    }
    Some((Arc::<str>::from(out), line_count))
}

#[inline(always)]
fn cached_autosync_text(state: &State, old_offset: f32, new_offset: f32) -> Arc<str> {
    let key = AutosyncTextKey {
        mode: state.autosync_mode() as u8,
        old_offset_bits: old_offset.to_bits(),
        new_offset_bits: new_offset.to_bits(),
        stddev_bits: state.autosync_standard_deviation().to_bits(),
        sample_count: state.autosync_sample_count().min(u16::MAX as usize) as u16,
    };
    cached_text(&AUTOSYNC_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        let collecting_sample = state
            .autosync_sample_count()
            .saturating_add(1)
            .min(AUTOSYNC_OFFSET_SAMPLE_COUNT);
        format!(
            "Old offset: {old_offset:0.3}\nNew offset: {new_offset:0.3}\nStandard deviation: {stddev:0.3}\nCollecting sample: {collecting_sample} / {max_samples}",
            stddev = state.autosync_standard_deviation(),
            max_samples = AUTOSYNC_OFFSET_SAMPLE_COUNT,
        )
    })
}

pub fn prewarm_text_layout(
    cache: &mut TextLayoutCache,
    fonts: &HashMap<&'static str, font::Font>,
    state: &State,
) {
    let cfg = crate::config::get();
    for centi in 0..=10_000 {
        let text = cached_score_2dp(centi as f64 / 100.0);
        cache.prewarm_text(
            fonts,
            current_machine_font_key(FontRole::Numbers),
            text.as_ref(),
            None,
        );
    }
    for tenths in 0..=1_000 {
        let text = cached_life_percent_text(tenths as f32 / 10.0);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    for player in 0..state.num_players() {
        let chart = &state.charts()[player];
        let meter_text = cached_meter_text(chart.meter);
        cache.prewarm_text(
            fonts,
            current_machine_font_key(FontRole::Header),
            meter_text.as_ref(),
            None,
        );
        let detail = color::difficulty_display_name_for_song(
            &chart.difficulty,
            &state.song().title,
            cfg.zmod_rating_box_text,
        );
        cache.prewarm_text(fonts, "miso", detail, None);
        let Some(gameplay_chart) = state.gameplay_chart(player) else {
            continue;
        };
        for &(_, bpm) in &gameplay_chart.timing_segments.bpms {
            let text = cached_bpm_text(
                f64::from(bpm.max(0.0)) * f64::from(state.music_rate()),
                cfg.show_bpm_decimal,
            );
            cache.prewarm_text(fonts, "miso", text.as_ref(), None);
        }
    }
    cache.prewarm_text(
        fonts,
        current_machine_font_key(FontRole::Header),
        state.stage_intro_text.as_ref(),
        None,
    );
    cache.prewarm_text(fonts, "miso", "Assist Tick", None);
    cache.prewarm_text(fonts, "miso", "Hit Tick", None);
    cache.prewarm_text(fonts, "miso", "AutoSync Song", None);
    cache.prewarm_text(fonts, "miso", "AutoSync Machine", None);
    cache.prewarm_text(
        fonts,
        "miso",
        &tr("Gameplay", "ContinueHoldingStartGiveUp"),
        None,
    );
    cache.prewarm_text(
        fonts,
        "miso",
        &tr("Gameplay", "ContinueHoldingBackGiveUp"),
        None,
    );
    cache.prewarm_text(fonts, "miso", &tr("Lobby", "DisconnectBasicPrompt"), None);
    cache.prewarm_text(fonts, "miso", &tr("Gameplay", "DontGoBack"), None);
    if let Some(text) = state.replay_status_text.as_ref() {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    if let Some(text) = sync_offset_overlay_message(state) {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
    if state.autosync_mode() != AutosyncMode::Off {
        let (old_offset, new_offset) = if state.autosync_mode() == AutosyncMode::Machine {
            (
                state.initial_global_offset_seconds(),
                state.global_offset_seconds(),
            )
        } else {
            (
                state.initial_song_offset_seconds(),
                state.song_offset_seconds(),
            )
        };
        let text = cached_autosync_text(state, old_offset, new_offset);
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
}

// --- TRANSITIONS ---
pub fn in_transition(
    state: Option<&State>,
    asset_manager: &AssetManager,
    is_restart: bool,
) -> (Vec<Actor>, f32) {
    if is_restart {
        // SL/zmod parity: on a song restart, skip the splode + stage-text
        // splash and run only a brief fade-from-black so the first gameplay
        // frame doesn't pop in. The "RESTART N" label still appears in the
        // gameplay footer overlay.
        let actor = act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1100):
            linear(TRANSITION_IN_RESTART_DURATION): alpha(0.0):
            linear(0.0): visible(false)
        );
        return (vec![actor], TRANSITION_IN_RESTART_DURATION);
    }
    let text = state
        .map(|gs| gs.stage_intro_text.clone())
        .unwrap_or_else(|| Arc::from("EVENT"));
    let intro_color = state.map_or(color::decorative_rgba(0), |gs| {
        color::decorative_rgba(gs.player_color_index())
    });
    let text_target_x = state.map_or(screen_center_x(), |gs| {
        intro_text_target_x(
            gs,
            asset_manager,
            text.as_ref(),
            profile::get_session_play_style(),
            profile::get_session_player_side(),
            crate::config::get().center_1player_notefield,
        )
    });
    let splode_tex = visual_styles::gameplayin_splode_texture_key();
    let minisplode_tex = visual_styles::gameplayin_minisplode_texture_key();
    let splode_zoom_scale = visual_styles::effect_zoom_scale(splode_tex);
    let minisplode_zoom_scale = visual_styles::effect_zoom_scale(minisplode_tex);
    let mut mirrored_splode = act!(sprite(splode_tex):
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        diffuse(intro_color[0], intro_color[1], intro_color[2], 0.8):
        rotationz(-10.0): zoom(0.0):
        z(1101):
        sleep(0.4):
        decelerate(0.6): rotationz(0.0): zoom(1.3 * splode_zoom_scale): alpha(0.0)
    );
    if let Actor::Sprite { flip_x, .. } = &mut mirrored_splode {
        // Simply Love uses rotationy(180) here; in deadsync 2D parity this is horizontal mirroring.
        *flip_x = true;
    }

    let actors = vec![
        act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1100):
            sleep(1.4):
            accelerate(0.6): alpha(0.0):
            linear(0.0): visible(false)
        ),
        act!(sprite(splode_tex):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 0.9):
            rotationz(10.0): zoom(0.0):
            z(1101):
            sleep(0.4):
            linear(0.6): rotationz(0.0): zoom(1.1 * splode_zoom_scale): alpha(0.0)
        ),
        mirrored_splode,
        act!(sprite(minisplode_tex):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 1.0):
            rotationz(10.0): zoom(0.0):
            z(1101):
            sleep(0.4):
            decelerate(0.8): rotationz(0.0): zoom(0.9 * minisplode_zoom_scale): alpha(0.0)
        ),
        act!(text:
            font(current_machine_font_key(FontRole::Header)): settext(text):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 0.0):
            z(1102):
            accelerate(0.5): alpha(1.0):
            sleep(0.66):
            accelerate(0.33): zoom(0.4): xy(text_target_x, screen_height() - 30.0):
            sleep((TRANSITION_IN_DURATION - INTRO_TEXT_SETTLE_SECONDS).max(0.0))
        ),
    ];
    (actors, TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        sleep(TRANSITION_OUT_DELAY):
        linear(TRANSITION_OUT_FADE_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

// --- DRAWING ---

fn push_background(
    actors: &mut Vec<Actor>,
    state: &State,
    bg_brightness: f32,
    base_color: crate::config::Color,
) {
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    let cy = screen_center_y();
    let bg_brightness = bg_brightness.clamp(0.0, 1.0);

    // Solid base fill behind everything. This is what shows when the song has no
    // background image, and what the song background is dimmed toward as
    // BGBrightness drops on the default path.
    let mut base =
        shared_banner::cover_sprite(Arc::<str>::from("__white"), cx, cy, sw, sh, 1.0, -101);
    if let Actor::Sprite { tint, .. } = &mut base {
        *tint = base_color.to_rgba();
    }
    actors.push(base);

    push_current_bgchange_media(actors, state, bg_brightness, cx, cy, sw, sh);
    push_bgchange_transition(actors, state, bg_brightness, cx, cy, sw, sh);
    // A non-default GameplayBgColor mirrors Chris's Simply Love underlay quad:
    // it covers song art but stays behind the notefield, filters, and HUD.
    push_custom_gameplay_backdrop(actors, base_color);
    push_layer2_bganimations(actors, state);
}

fn active_background_change(state: &State) -> Option<&SongBackgroundChange> {
    state
        .next_background_change_ix
        .checked_sub(1)
        .and_then(|ix| state.background_changes.get(ix))
}

fn bgchange_tint(change: Option<&SongBackgroundChange>, brightness: f32) -> [f32; 4] {
    let color = change.and_then(|change| change.color1).unwrap_or([1.0; 4]);
    [color[0], color[1], color[2], color[3] * brightness]
}

fn bgchange_movie_viz_tint(change: Option<&SongBackgroundChange>, brightness: f32) -> [f32; 4] {
    let color = change.and_then(|change| change.color2).unwrap_or([1.0; 4]);
    [color[0], color[1], color[2], color[3] * brightness]
}

fn background_media_sprite(
    key: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Actor {
    let mut actor = shared_banner::cover_sprite(key, x, y, w, h, 1.0, -100);
    if let Actor::Sprite {
        tint: actor_tint,
        blend: actor_blend,
        ..
    } = &mut actor
    {
        *actor_tint = tint;
        *actor_blend = blend;
    }
    actor
}

fn push_current_bgchange_media(
    actors: &mut Vec<Actor>,
    state: &State,
    bg_brightness: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    if &*state.background_texture_key == "__black" {
        return;
    }
    let change = active_background_change(state);
    if change.is_some_and(|change| change.effect_is("SongBgWithMovieViz")) {
        if let Some(path) = state.song().background_path.as_ref() {
            actors.push(background_media_sprite(
                crate::assets::media_path_key(path),
                bgchange_tint(change, bg_brightness),
                BlendMode::Alpha,
                x,
                y,
                w,
                h,
            ));
        }
        actors.push(background_media_sprite(
            state.background_texture_key.clone(),
            bgchange_movie_viz_tint(change, bg_brightness),
            BlendMode::Add,
            x,
            y,
            w,
            h,
        ));
    } else {
        actors.push(background_media_sprite(
            state.background_texture_key.clone(),
            bgchange_tint(change, bg_brightness),
            BlendMode::Alpha,
            x,
            y,
            w,
            h,
        ));
    }
}

fn push_bgchange_transition(
    actors: &mut Vec<Actor>,
    state: &State,
    bg_brightness: f32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let Some(key) = state.previous_background_texture_key.as_ref() else {
        return;
    };
    if &**key == "__black" {
        return;
    }
    let Some(duration) = bgchange_transition_duration(&state.background_transition) else {
        return;
    };
    let elapsed = state.current_music_time_display() - state.background_transition_start_time;
    let progress = (elapsed / duration).clamp(0.0, 1.0);
    if progress >= 1.0 {
        return;
    }
    let mut actor = background_media_sprite(
        key.clone(),
        [1.0, 1.0, 1.0, bg_brightness],
        BlendMode::Alpha,
        x,
        y,
        w,
        h,
    );
    apply_bgchange_transition(&mut actor, &state.background_transition, progress, w, h);
    actors.push(actor);
}

fn bgchange_transition_duration(transition: &str) -> Option<f32> {
    if transition.eq_ignore_ascii_case("CrossFade_Fastest") {
        Some(0.5)
    } else if transition.eq_ignore_ascii_case("CrossFade_Faster") {
        Some(0.75)
    } else if [
        "CrossFade",
        "FadeCenterHorizontal",
        "FadeCenterVertical",
        "FadeDown",
        "FadeLeft",
        "FadeRight",
        "FadeUp",
        "SlideDown",
        "SlideLeft",
        "SlideRight",
        "SlideUp",
    ]
    .iter()
    .any(|name| transition.eq_ignore_ascii_case(name))
    {
        Some(1.0)
    } else {
        None
    }
}

fn apply_bgchange_transition(
    actor: &mut Actor,
    transition: &str,
    progress: f32,
    screen_w: f32,
    screen_h: f32,
) {
    let Actor::Sprite {
        offset,
        tint,
        cropleft,
        cropright,
        croptop,
        cropbottom,
        fadeleft,
        faderight,
        fadetop,
        fadebottom,
        ..
    } = actor
    else {
        return;
    };
    if transition.eq_ignore_ascii_case("CrossFade")
        || transition.eq_ignore_ascii_case("CrossFade_Faster")
        || transition.eq_ignore_ascii_case("CrossFade_Fastest")
    {
        tint[3] *= 1.0 - progress;
    } else if transition.eq_ignore_ascii_case("SlideLeft") {
        offset[0] -= screen_w * progress;
        tint[3] *= 1.0 - progress;
    } else if transition.eq_ignore_ascii_case("SlideRight") {
        offset[0] += screen_w * progress;
        tint[3] *= 1.0 - progress;
    } else if transition.eq_ignore_ascii_case("SlideUp") {
        offset[1] -= screen_h * progress;
        tint[3] *= 1.0 - progress;
    } else if transition.eq_ignore_ascii_case("SlideDown") {
        offset[1] += screen_h * progress;
        tint[3] *= 1.0 - progress;
    } else if transition.eq_ignore_ascii_case("FadeUp") {
        *cropbottom = -0.3 + 1.6 * progress;
        *fadebottom = 0.3;
    } else if transition.eq_ignore_ascii_case("FadeDown") {
        *croptop = -0.3 + 1.6 * progress;
        *fadetop = 0.3;
    } else if transition.eq_ignore_ascii_case("FadeRight") {
        *cropleft = -0.3 + 1.6 * progress;
        *fadeleft = 0.3;
    } else if transition.eq_ignore_ascii_case("FadeLeft") {
        *cropright = -0.3 + 1.6 * progress;
        *faderight = 0.3;
    } else if transition.eq_ignore_ascii_case("FadeCenterHorizontal") {
        *croptop = -0.3 + 0.8 * progress;
        *cropbottom = -0.3 + 0.8 * progress;
        *fadetop = 0.3;
        *fadebottom = 0.3;
    } else if transition.eq_ignore_ascii_case("FadeCenterVertical") {
        *cropleft = -0.3 + 0.8 * progress;
        *cropright = -0.3 + 0.8 * progress;
        *fadeleft = 0.3;
        *faderight = 0.3;
    }
}

fn push_layer2_bganimations(actors: &mut Vec<Actor>, state: &State) {
    const FLASH_SECONDS: f32 = 0.6;
    let Some((change, elapsed)) = state
        .song()
        .background_layer2_changes
        .iter()
        .rev()
        .filter_map(|change| {
            let start = state.timing().get_time_for_beat(change.start_beat);
            let elapsed = state.current_music_time_display() - start;
            (elapsed >= 0.0 && elapsed <= FLASH_SECONDS).then_some((change, elapsed))
        })
        .next()
    else {
        return;
    };
    let SongBackgroundChangeTarget::Animation(name) = &change.target else {
        return;
    };
    let mut color = if name.eq_ignore_ascii_case("white flash") {
        [1.0, 1.0, 1.0, 1.0]
    } else if name.eq_ignore_ascii_case("yellow flash") {
        [1.0, 1.0, 160.0 / 255.0, 1.0]
    } else {
        return;
    };
    let progress = (elapsed / FLASH_SECONDS).clamp(0.0, 1.0);
    color[3] *= 1.0 - progress * progress;
    actors.push(act!(quad:
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        setsize(screen_width() * 2.0, screen_height() * 2.0):
        diffuse(color[0], color[1], color[2], color[3]):
        z(-98)
    ));
}

fn custom_gameplay_backdrop_enabled(color: crate::config::Color) -> bool {
    color != crate::config::Color::BLACK
}

fn push_custom_gameplay_backdrop(actors: &mut Vec<Actor>, color: crate::config::Color) {
    if !custom_gameplay_backdrop_enabled(color) {
        return;
    }
    let rgba = color.to_rgba();
    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        setsize(screen_width(), screen_height()):
        diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
        z(-99)
    ));
}

fn gameplay_header_rgba(color: crate::config::Color) -> [f32; 4] {
    if custom_gameplay_backdrop_enabled(color) {
        color.to_rgba()
    } else {
        [0.0, 0.0, 0.0, 0.85]
    }
}

fn song_lua_has_visible_tex(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    path: &Path,
) -> bool {
    overlays.iter().zip(overlay_states).any(|(overlay, state)| {
        matches!(
            &overlay.kind,
            SongLuaOverlayKind::Sprite { texture_path, .. } if texture_path.as_path() == path
        ) && state.visible
            && state.diffuse[3] > f32::EPSILON
    })
}

fn song_lua_owns_fg_media(
    state: &State,
    overlay_states: &[SongLuaOverlayState],
    path: &Path,
    layer_local_states: &mut Vec<SongLuaOverlayState>,
    layer_states: &mut Vec<SongLuaOverlayState>,
) -> bool {
    let song_lua_visuals = state.song_lua_visuals();
    if song_lua_has_visible_tex(&song_lua_visuals.overlays, overlay_states, path) {
        return true;
    }
    for layer in &song_lua_visuals.background_visual_layers {
        if state.current_music_time_display() < layer.start_second {
            continue;
        }
        song_lua_overlay_state_sets_from_into(
            state.current_music_time_display(),
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            layer_local_states,
            layer_states,
        );
        if song_lua_has_visible_tex(&layer.overlays, layer_states, path) {
            return true;
        }
    }
    for layer in &song_lua_visuals.foreground_visual_layers {
        if state.current_music_time_display() < layer.start_second {
            continue;
        }
        song_lua_overlay_state_sets_from_into(
            state.current_music_time_display(),
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            layer_local_states,
            layer_states,
        );
        if song_lua_has_visible_tex(&layer.overlays, layer_states, path) {
            return true;
        }
    }
    false
}

fn active_foreground_media(state: &State) -> Option<(&Path, Arc<str>)> {
    let path = state.song().active_foreground_path(state.current_beat())?;
    Some((path, crate::assets::media_path_key(path)))
}

fn build_foreground_media(
    state: &State,
    overlay_states: &[SongLuaOverlayState],
    layer_local_states: &mut Vec<SongLuaOverlayState>,
    layer_states: &mut Vec<SongLuaOverlayState>,
) -> Option<Actor> {
    let (path, texture_key) = active_foreground_media(state)?;
    if song_lua_owns_fg_media(
        state,
        overlay_states,
        path,
        layer_local_states,
        layer_states,
    ) {
        return None;
    }
    Some(shared_banner::cover_sprite(
        texture_key,
        screen_center_x(),
        screen_center_y(),
        screen_width(),
        screen_height(),
        1.0,
        1000,
    ))
}

#[inline(always)]
fn song_lua_overlay_space_width(state: &State) -> f32 {
    state.song_lua_visuals().screen_width.max(1.0)
}

#[inline(always)]
fn song_lua_overlay_space_height(state: &State) -> f32 {
    state.song_lua_visuals().screen_height.max(1.0)
}

fn apply_song_lua_overlay_delta(state: &mut SongLuaOverlayState, delta: &SongLuaOverlayStateDelta) {
    if let Some(value) = delta.x {
        state.x = value;
    }
    if let Some(value) = delta.y {
        state.y = value;
    }
    if let Some(value) = delta.z {
        state.z = value;
    }
    if let Some(value) = delta.z_bias {
        state.z_bias = value;
    }
    if let Some(value) = delta.draw_order {
        state.draw_order = value;
    }
    if let Some(value) = delta.draw_by_z_position {
        state.draw_by_z_position = value;
    }
    if let Some(value) = delta.halign {
        state.halign = value;
    }
    if let Some(value) = delta.valign {
        state.valign = value;
    }
    if let Some(value) = delta.text_align {
        state.text_align = value;
    }
    if let Some(value) = delta.uppercase {
        state.uppercase = value;
    }
    if let Some(value) = delta.shadow_len {
        state.shadow_len = value;
    }
    if let Some(value) = delta.shadow_color {
        state.shadow_color = value;
    }
    if let Some(value) = delta.glow {
        state.glow = value;
    }
    if let Some(value) = delta.fov {
        state.fov = Some(value);
    }
    if let Some(value) = delta.vanishpoint {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = delta.diffuse {
        state.diffuse = value;
    }
    if let Some(value) = delta.vertex_colors {
        state.vertex_colors = Some(value);
    }
    if let Some(value) = delta.visible {
        state.visible = value;
    }
    if let Some(value) = delta.cropleft {
        state.cropleft = value;
    }
    if let Some(value) = delta.cropright {
        state.cropright = value;
    }
    if let Some(value) = delta.croptop {
        state.croptop = value;
    }
    if let Some(value) = delta.cropbottom {
        state.cropbottom = value;
    }
    if let Some(value) = delta.fadeleft {
        state.fadeleft = value;
    }
    if let Some(value) = delta.faderight {
        state.faderight = value;
    }
    if let Some(value) = delta.fadetop {
        state.fadetop = value;
    }
    if let Some(value) = delta.fadebottom {
        state.fadebottom = value;
    }
    if let Some(value) = delta.mask_source {
        state.mask_source = value;
    }
    if let Some(value) = delta.mask_dest {
        state.mask_dest = value;
    }
    if let Some(value) = delta.depth_test {
        state.depth_test = value;
    }
    if let Some(value) = delta.zoom {
        state.zoom = value;
    }
    if let Some(value) = delta.zoom_x {
        state.zoom_x = value;
    }
    if let Some(value) = delta.zoom_y {
        state.zoom_y = value;
    }
    if let Some(value) = delta.zoom_z {
        state.zoom_z = value;
    }
    if let Some(value) = delta.basezoom {
        state.basezoom = value;
    }
    if let Some(value) = delta.basezoom_x {
        state.basezoom_x = value;
    }
    if let Some(value) = delta.basezoom_y {
        state.basezoom_y = value;
    }
    if let Some(value) = delta.basezoom_z {
        state.basezoom_z = value;
    }
    if let Some(value) = delta.rot_x_deg {
        state.rot_x_deg = value;
    }
    if let Some(value) = delta.rot_y_deg {
        state.rot_y_deg = value;
    }
    if let Some(value) = delta.rot_z_deg {
        state.rot_z_deg = value;
    }
    if let Some(value) = delta.skew_x {
        state.skew_x = value;
    }
    if let Some(value) = delta.skew_y {
        state.skew_y = value;
    }
    if let Some(value) = delta.blend {
        state.blend = value;
    }
    if let Some(value) = delta.vibrate {
        state.vibrate = value;
    }
    if let Some(value) = delta.effect_magnitude {
        state.effect_magnitude = value;
    }
    if let Some(value) = delta.effect_clock {
        state.effect_clock = value;
    }
    if let Some(value) = delta.effect_mode {
        state.effect_mode = value;
    }
    if let Some(value) = delta.effect_color1 {
        state.effect_color1 = value;
    }
    if let Some(value) = delta.effect_color2 {
        state.effect_color2 = value;
    }
    if let Some(value) = delta.effect_period {
        state.effect_period = value;
    }
    if let Some(value) = delta.effect_offset {
        state.effect_offset = value;
    }
    if let Some(value) = delta.effect_timing {
        state.effect_timing = Some(value);
    }
    if let Some(value) = delta.rainbow {
        state.rainbow = value;
    }
    if let Some(value) = delta.rainbow_scroll {
        state.rainbow_scroll = value;
    }
    if let Some(value) = delta.text_jitter {
        state.text_jitter = value;
    }
    if let Some(value) = delta.text_distortion {
        state.text_distortion = value;
    }
    if let Some(value) = delta.text_glow_mode {
        state.text_glow_mode = value;
    }
    if let Some(value) = delta.mult_attrs_with_diffuse {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = delta.sprite_animate {
        state.sprite_animate = value;
    }
    if let Some(value) = delta.sprite_loop {
        state.sprite_loop = value;
    }
    if let Some(value) = delta.sprite_playback_rate {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = delta.sprite_state_delay {
        state.sprite_state_delay = value;
    }
    if let Some(value) = delta.sprite_state_index {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = delta.vert_spacing {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = delta.wrap_width_pixels {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = delta.max_width {
        state.max_width = Some(value);
    }
    if let Some(value) = delta.max_height {
        state.max_height = Some(value);
    }
    if let Some(value) = delta.max_w_pre_zoom {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = delta.max_h_pre_zoom {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = delta.max_dimension_uses_zoom {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = delta.texture_filtering {
        state.texture_filtering = value;
    }
    if let Some(value) = delta.texture_wrapping {
        state.texture_wrapping = value;
    }
    if let Some(value) = delta.texcoord_offset {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = delta.custom_texture_rect {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = delta.texcoord_velocity {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = delta.size {
        state.size = Some(value);
    }
    if let Some(value) = delta.stretch_rect {
        state.stretch_rect = Some(value);
    }
}

fn song_lua_overlay_state_with_delta(
    mut state: SongLuaOverlayState,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    apply_song_lua_overlay_delta(&mut state, delta);
    state
}

fn song_lua_overlay_state_lerp(
    mut from: SongLuaOverlayState,
    to: SongLuaOverlayState,
    t: f32,
    delta: &SongLuaOverlayStateDelta,
) -> SongLuaOverlayState {
    if delta.x.is_some() {
        from.x = (to.x - from.x).mul_add(t, from.x);
    }
    if delta.y.is_some() {
        from.y = (to.y - from.y).mul_add(t, from.y);
    }
    if delta.z.is_some() {
        from.z = (to.z - from.z).mul_add(t, from.z);
    }
    if delta.z_bias.is_some() {
        from.z_bias = (to.z_bias - from.z_bias).mul_add(t, from.z_bias);
    }
    if delta.draw_order.is_some() && t >= 1.0 - f32::EPSILON {
        from.draw_order = to.draw_order;
    }
    if delta.draw_by_z_position.is_some() && t >= 1.0 - f32::EPSILON {
        from.draw_by_z_position = to.draw_by_z_position;
    }
    if delta.halign.is_some() {
        from.halign = (to.halign - from.halign).mul_add(t, from.halign);
    }
    if delta.valign.is_some() {
        from.valign = (to.valign - from.valign).mul_add(t, from.valign);
    }
    if delta.text_align.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_align = to.text_align;
    }
    if delta.uppercase.is_some() && t >= 1.0 - f32::EPSILON {
        from.uppercase = to.uppercase;
    }
    if delta.shadow_len.is_some() {
        from.shadow_len = [
            (to.shadow_len[0] - from.shadow_len[0]).mul_add(t, from.shadow_len[0]),
            (to.shadow_len[1] - from.shadow_len[1]).mul_add(t, from.shadow_len[1]),
        ];
    }
    if delta.shadow_color.is_some() {
        for i in 0..4 {
            from.shadow_color[i] =
                (to.shadow_color[i] - from.shadow_color[i]).mul_add(t, from.shadow_color[i]);
        }
    }
    if delta.glow.is_some() {
        for i in 0..4 {
            from.glow[i] = (to.glow[i] - from.glow[i]).mul_add(t, from.glow[i]);
        }
    }
    if delta.fov.is_some()
        && let (Some(from_fov), Some(to_fov)) = (from.fov, to.fov)
    {
        from.fov = Some((to_fov - from_fov).mul_add(t, from_fov));
    }
    if delta.vanishpoint.is_some()
        && let (Some(from_vanish), Some(to_vanish)) = (from.vanishpoint, to.vanishpoint)
    {
        from.vanishpoint = Some([
            (to_vanish[0] - from_vanish[0]).mul_add(t, from_vanish[0]),
            (to_vanish[1] - from_vanish[1]).mul_add(t, from_vanish[1]),
        ]);
    }
    if delta.diffuse.is_some() {
        for i in 0..4 {
            from.diffuse[i] = (to.diffuse[i] - from.diffuse[i]).mul_add(t, from.diffuse[i]);
        }
    }
    if delta.vertex_colors.is_some() {
        let mut from_colors = from.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        let to_colors = to.vertex_colors.unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
        for corner in 0..4 {
            for channel in 0..4 {
                from_colors[corner][channel] = (to_colors[corner][channel]
                    - from_colors[corner][channel])
                    .mul_add(t, from_colors[corner][channel]);
            }
        }
        from.vertex_colors = Some(from_colors);
    }
    if delta.cropleft.is_some() {
        from.cropleft = (to.cropleft - from.cropleft).mul_add(t, from.cropleft);
    }
    if delta.cropright.is_some() {
        from.cropright = (to.cropright - from.cropright).mul_add(t, from.cropright);
    }
    if delta.croptop.is_some() {
        from.croptop = (to.croptop - from.croptop).mul_add(t, from.croptop);
    }
    if delta.cropbottom.is_some() {
        from.cropbottom = (to.cropbottom - from.cropbottom).mul_add(t, from.cropbottom);
    }
    if delta.fadeleft.is_some() {
        from.fadeleft = (to.fadeleft - from.fadeleft).mul_add(t, from.fadeleft);
    }
    if delta.faderight.is_some() {
        from.faderight = (to.faderight - from.faderight).mul_add(t, from.faderight);
    }
    if delta.fadetop.is_some() {
        from.fadetop = (to.fadetop - from.fadetop).mul_add(t, from.fadetop);
    }
    if delta.fadebottom.is_some() {
        from.fadebottom = (to.fadebottom - from.fadebottom).mul_add(t, from.fadebottom);
    }
    if delta.mask_source.is_some() && t >= 1.0 - f32::EPSILON {
        from.mask_source = to.mask_source;
    }
    if delta.mask_dest.is_some() && t >= 1.0 - f32::EPSILON {
        from.mask_dest = to.mask_dest;
    }
    if delta.zoom.is_some() {
        from.zoom = (to.zoom - from.zoom).mul_add(t, from.zoom);
    }
    if delta.zoom_x.is_some() {
        from.zoom_x = (to.zoom_x - from.zoom_x).mul_add(t, from.zoom_x);
    }
    if delta.zoom_y.is_some() {
        from.zoom_y = (to.zoom_y - from.zoom_y).mul_add(t, from.zoom_y);
    }
    if delta.zoom_z.is_some() {
        from.zoom_z = (to.zoom_z - from.zoom_z).mul_add(t, from.zoom_z);
    }
    if delta.basezoom.is_some() {
        from.basezoom = (to.basezoom - from.basezoom).mul_add(t, from.basezoom);
    }
    if delta.basezoom_x.is_some() {
        from.basezoom_x = (to.basezoom_x - from.basezoom_x).mul_add(t, from.basezoom_x);
    }
    if delta.basezoom_y.is_some() {
        from.basezoom_y = (to.basezoom_y - from.basezoom_y).mul_add(t, from.basezoom_y);
    }
    if delta.basezoom_z.is_some() {
        from.basezoom_z = (to.basezoom_z - from.basezoom_z).mul_add(t, from.basezoom_z);
    }
    if delta.rot_x_deg.is_some() {
        from.rot_x_deg = (to.rot_x_deg - from.rot_x_deg).mul_add(t, from.rot_x_deg);
    }
    if delta.rot_y_deg.is_some() {
        from.rot_y_deg = (to.rot_y_deg - from.rot_y_deg).mul_add(t, from.rot_y_deg);
    }
    if delta.rot_z_deg.is_some() {
        from.rot_z_deg = (to.rot_z_deg - from.rot_z_deg).mul_add(t, from.rot_z_deg);
    }
    if delta.skew_x.is_some() {
        from.skew_x = (to.skew_x - from.skew_x).mul_add(t, from.skew_x);
    }
    if delta.skew_y.is_some() {
        from.skew_y = (to.skew_y - from.skew_y).mul_add(t, from.skew_y);
    }
    if delta.effect_magnitude.is_some() {
        for i in 0..3 {
            from.effect_magnitude[i] = (to.effect_magnitude[i] - from.effect_magnitude[i])
                .mul_add(t, from.effect_magnitude[i]);
        }
    }
    if delta.effect_color1.is_some() {
        for i in 0..4 {
            from.effect_color1[i] =
                (to.effect_color1[i] - from.effect_color1[i]).mul_add(t, from.effect_color1[i]);
        }
    }
    if delta.effect_color2.is_some() {
        for i in 0..4 {
            from.effect_color2[i] =
                (to.effect_color2[i] - from.effect_color2[i]).mul_add(t, from.effect_color2[i]);
        }
    }
    if delta.effect_period.is_some() {
        from.effect_period = (to.effect_period - from.effect_period).mul_add(t, from.effect_period);
    }
    if delta.effect_offset.is_some() {
        from.effect_offset = (to.effect_offset - from.effect_offset).mul_add(t, from.effect_offset);
    }
    if delta.effect_timing.is_some()
        && let (Some(from_timing), Some(to_timing)) = (from.effect_timing, to.effect_timing)
    {
        from.effect_timing = Some([
            (to_timing[0] - from_timing[0]).mul_add(t, from_timing[0]),
            (to_timing[1] - from_timing[1]).mul_add(t, from_timing[1]),
            (to_timing[2] - from_timing[2]).mul_add(t, from_timing[2]),
            (to_timing[3] - from_timing[3]).mul_add(t, from_timing[3]),
            (to_timing[4] - from_timing[4]).mul_add(t, from_timing[4]),
        ]);
    }
    if delta.sprite_playback_rate.is_some() {
        from.sprite_playback_rate = (to.sprite_playback_rate - from.sprite_playback_rate)
            .mul_add(t, from.sprite_playback_rate);
    }
    if delta.sprite_state_delay.is_some() {
        from.sprite_state_delay =
            (to.sprite_state_delay - from.sprite_state_delay).mul_add(t, from.sprite_state_delay);
    }
    if delta.sprite_state_index.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_state_index = to.sprite_state_index;
    }
    if delta.vert_spacing.is_some() && t >= 1.0 - f32::EPSILON {
        from.vert_spacing = to.vert_spacing;
    }
    if delta.wrap_width_pixels.is_some() && t >= 1.0 - f32::EPSILON {
        from.wrap_width_pixels = to.wrap_width_pixels;
    }
    if delta.max_width.is_some()
        && let (Some(from_width), Some(to_width)) = (from.max_width, to.max_width)
    {
        from.max_width = Some((to_width - from_width).mul_add(t, from_width));
    }
    if delta.max_height.is_some()
        && let (Some(from_height), Some(to_height)) = (from.max_height, to.max_height)
    {
        from.max_height = Some((to_height - from_height).mul_add(t, from_height));
    }
    if delta.max_w_pre_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_w_pre_zoom = to.max_w_pre_zoom;
    }
    if delta.max_h_pre_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_h_pre_zoom = to.max_h_pre_zoom;
    }
    if delta.max_dimension_uses_zoom.is_some() && t >= 1.0 - f32::EPSILON {
        from.max_dimension_uses_zoom = to.max_dimension_uses_zoom;
    }
    if delta.texcoord_offset.is_some()
        && let (Some(from_offset), Some(to_offset)) = (from.texcoord_offset, to.texcoord_offset)
    {
        from.texcoord_offset = Some([
            (to_offset[0] - from_offset[0]).mul_add(t, from_offset[0]),
            (to_offset[1] - from_offset[1]).mul_add(t, from_offset[1]),
        ]);
    }
    if delta.custom_texture_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.custom_texture_rect, to.custom_texture_rect)
    {
        from.custom_texture_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.texcoord_velocity.is_some()
        && let (Some(from_vel), Some(to_vel)) = (from.texcoord_velocity, to.texcoord_velocity)
    {
        from.texcoord_velocity = Some([
            (to_vel[0] - from_vel[0]).mul_add(t, from_vel[0]),
            (to_vel[1] - from_vel[1]).mul_add(t, from_vel[1]),
        ]);
    }
    if delta.size.is_some()
        && let (Some(from_size), Some(to_size)) = (from.size, to.size)
    {
        from.size = Some([
            (to_size[0] - from_size[0]).mul_add(t, from_size[0]),
            (to_size[1] - from_size[1]).mul_add(t, from_size[1]),
        ]);
    }
    if delta.stretch_rect.is_some()
        && let (Some(from_rect), Some(to_rect)) = (from.stretch_rect, to.stretch_rect)
    {
        from.stretch_rect = Some([
            (to_rect[0] - from_rect[0]).mul_add(t, from_rect[0]),
            (to_rect[1] - from_rect[1]).mul_add(t, from_rect[1]),
            (to_rect[2] - from_rect[2]).mul_add(t, from_rect[2]),
            (to_rect[3] - from_rect[3]).mul_add(t, from_rect[3]),
        ]);
    }
    if delta.visible.is_some() && t >= 1.0 - f32::EPSILON {
        from.visible = to.visible;
    }
    if delta.blend.is_some() && t >= 1.0 - f32::EPSILON {
        from.blend = to.blend;
    }
    if delta.vibrate.is_some() && t >= 1.0 - f32::EPSILON {
        from.vibrate = to.vibrate;
    }
    if delta.effect_clock.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_clock = to.effect_clock;
    }
    if delta.effect_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.effect_mode = to.effect_mode;
    }
    if delta.rainbow.is_some() && t >= 1.0 - f32::EPSILON {
        from.rainbow = to.rainbow;
    }
    if delta.rainbow_scroll.is_some() && t >= 1.0 - f32::EPSILON {
        from.rainbow_scroll = to.rainbow_scroll;
    }
    if delta.text_jitter.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_jitter = to.text_jitter;
    }
    if delta.text_distortion.is_some() {
        from.text_distortion =
            (to.text_distortion - from.text_distortion).mul_add(t, from.text_distortion);
    }
    if delta.text_glow_mode.is_some() && t >= 1.0 - f32::EPSILON {
        from.text_glow_mode = to.text_glow_mode;
    }
    if delta.mult_attrs_with_diffuse.is_some() && t >= 1.0 - f32::EPSILON {
        from.mult_attrs_with_diffuse = to.mult_attrs_with_diffuse;
    }
    if delta.sprite_animate.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_animate = to.sprite_animate;
    }
    if delta.sprite_loop.is_some() && t >= 1.0 - f32::EPSILON {
        from.sprite_loop = to.sprite_loop;
    }
    if delta.texture_wrapping.is_some() && t >= 1.0 - f32::EPSILON {
        from.texture_wrapping = to.texture_wrapping;
    }
    from
}

#[inline(always)]
fn song_lua_valid_sprite_state_index(state: SongLuaOverlayState) -> Option<u32> {
    state.sprite_state_index.filter(|&value| value != u32::MAX)
}

#[inline(always)]
fn song_lua_sprite_sheet_index(
    state: SongLuaOverlayState,
    texture_key: &str,
    total_elapsed: f32,
) -> Option<u32> {
    let start = song_lua_valid_sprite_state_index(state).unwrap_or(0);
    let (cols, rows) = sprite_sheet_dims(texture_key);
    let total = cols.saturating_mul(rows).max(1);
    if state.sprite_animate && state.sprite_state_delay > 0.0 && total > 1 {
        let steps =
            (total_elapsed * state.sprite_playback_rate / state.sprite_state_delay).floor() as i64;
        let frame = i64::from(start) + steps;
        let total = i64::from(total);
        return Some(if state.sprite_loop {
            frame.rem_euclid(total) as u32
        } else {
            frame.clamp(0, total - 1) as u32
        });
    }
    (state.sprite_animate || song_lua_valid_sprite_state_index(state).is_some()).then_some(start)
}

#[inline(always)]
fn song_lua_sprite_sheet_rect(index: u32, cols: u32, rows: u32) -> [f32; 4] {
    let cols = cols.max(1);
    let rows = rows.max(1);
    let col = index % cols;
    let row = (index / cols).min(rows.saturating_sub(1));
    let width = 1.0 / cols as f32;
    let height = 1.0 / rows as f32;
    let left = col as f32 * width;
    let top = row as f32 * height;
    [left, top, left + width, top + height]
}

fn song_lua_overlay_sprite_size(state: SongLuaOverlayState, texture_key: &str) -> Option<[f32; 2]> {
    if let Some(size) = state.size {
        return Some(size);
    }
    let tex = crate::assets::texture_dims(texture_key)?;
    let (mut width, mut height) = (tex.w as f32, tex.h as f32);
    if state.sprite_animate || song_lua_valid_sprite_state_index(state).is_some() {
        let (cols, rows) = sprite_sheet_dims(texture_key);
        width /= cols.max(1) as f32;
        height /= rows.max(1) as f32;
    }
    Some([width, height])
}

fn song_lua_overlay_uv_rect(
    state: SongLuaOverlayState,
    texture_key: Option<&str>,
    total_elapsed: f32,
) -> Option<[f32; 4]> {
    let mut rect = state.custom_texture_rect.or_else(|| {
        let texture_key = texture_key?;
        let state_index = song_lua_sprite_sheet_index(state, texture_key, total_elapsed)?;
        let (cols, rows) = sprite_sheet_dims(texture_key);
        Some(song_lua_sprite_sheet_rect(state_index, cols, rows))
    });
    if rect.is_none() && state.texcoord_offset.is_some() {
        rect = Some([0.0, 0.0, 1.0, 1.0]);
    }
    if let (Some([u0, v0, u1, v1]), Some([dx, dy])) = (rect, state.texcoord_offset) {
        rect = Some([u0 + dx, v0 + dy, u1 + dx, v1 + dy]);
    }
    rect
}

#[inline(always)]
fn song_lua_overlay_axis_scale(state: SongLuaOverlayState) -> [f32; 2] {
    let basezoom_x = if (state.basezoom_x - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_x
    };
    let basezoom_y = if (state.basezoom_y - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_y
    };
    let zoom_x = if (state.zoom_x - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_x
    };
    let zoom_y = if (state.zoom_y - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_y
    };
    [basezoom_x * zoom_x, basezoom_y * zoom_y]
}

#[inline(always)]
fn song_lua_overlay_z_scale(state: SongLuaOverlayState) -> f32 {
    let basezoom_z = if (state.basezoom_z - 1.0).abs() <= f32::EPSILON {
        state.basezoom
    } else {
        state.basezoom_z
    };
    let zoom_z = if (state.zoom_z - 1.0).abs() <= f32::EPSILON {
        state.zoom
    } else {
        state.zoom_z
    };
    basezoom_z * zoom_z
}

#[inline(always)]
fn song_lua_overlay_parent_uses_center_origin(
    parent_kind: &SongLuaOverlayKind,
    parent_axis: f32,
    overlay_space_axis: f32,
) -> bool {
    matches!(
        parent_kind,
        SongLuaOverlayKind::Actor
            | SongLuaOverlayKind::ActorFrame
            | SongLuaOverlayKind::ActorFrameTexture
    ) && (parent_axis - 0.5 * overlay_space_axis).abs() <= 0.01
}

fn song_lua_overlay_compose_state(
    parent_kind: &SongLuaOverlayKind,
    parent: SongLuaOverlayState,
    mut child: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> SongLuaOverlayState {
    let [parent_scale_x, parent_scale_y] = song_lua_overlay_axis_scale(parent);
    let (sin_z, cos_z) = parent.rot_z_deg.to_radians().sin_cos();
    let epsilon = 0.01;
    let local_x = if matches!(
        parent_kind,
        SongLuaOverlayKind::Actor
            | SongLuaOverlayKind::ActorFrame
            | SongLuaOverlayKind::ActorFrameTexture
    ) && song_lua_overlay_parent_uses_center_origin(
        parent_kind,
        parent.x,
        overlay_space_width,
    ) && (child.x - 0.5 * overlay_space_width).abs() <= epsilon
    {
        0.0
    } else {
        child.x
    } * parent_scale_x;
    let local_y = if matches!(
        parent_kind,
        SongLuaOverlayKind::Actor
            | SongLuaOverlayKind::ActorFrame
            | SongLuaOverlayKind::ActorFrameTexture
    ) && song_lua_overlay_parent_uses_center_origin(
        parent_kind,
        parent.y,
        overlay_space_height,
    ) && (child.y - 0.5 * overlay_space_height).abs() <= epsilon
    {
        0.0
    } else {
        child.y
    } * parent_scale_y;
    child.x = parent.x + local_x * cos_z - local_y * sin_z;
    child.y = parent.y + local_x * sin_z + local_y * cos_z;
    for i in 0..4 {
        child.diffuse[i] *= parent.diffuse[i];
    }
    child.texcoord_offset = match (parent.texcoord_offset, child.texcoord_offset) {
        (Some(parent), Some(child)) => Some([parent[0] + child[0], parent[1] + child[1]]),
        (Some(parent), None) => Some(parent),
        (None, child) => child,
    };
    child.visible = parent.visible && child.visible;
    child.mask_source |= parent.mask_source;
    child.mask_dest |= parent.mask_dest;
    child.basezoom *= parent.basezoom * parent.zoom;
    child.basezoom_x *= parent.basezoom_x * parent.zoom_x;
    child.basezoom_y *= parent.basezoom_y * parent.zoom_y;
    child.basezoom_z *= parent.basezoom_z * parent.zoom_z;
    child.rot_x_deg += parent.rot_x_deg;
    child.rot_y_deg += parent.rot_y_deg;
    child.rot_z_deg += parent.rot_z_deg;
    if let Some([left, top, right, bottom]) = child.stretch_rect
        && parent.rot_x_deg.abs() <= f32::EPSILON
        && parent.rot_y_deg.abs() <= f32::EPSILON
        && parent.rot_z_deg.abs() <= f32::EPSILON
    {
        child.stretch_rect = Some([
            parent.x + left * parent_scale_x,
            parent.y + top * parent_scale_y,
            parent.x + right * parent_scale_x,
            parent.y + bottom * parent_scale_y,
        ]);
    }
    child
}

fn song_lua_overlay_local_states_into(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    out: &mut Vec<SongLuaOverlayState>,
) {
    out.clear();
    out.reserve(overlays.len());
    for (idx, overlay) in overlays.iter().enumerate() {
        out.push(song_lua_overlay_render_state_from(
            now,
            idx,
            overlay,
            overlay_events,
            overlay_eases,
            overlay_ease_ranges,
        ));
    }
}

#[cfg(test)]
fn song_lua_overlay_states_from_local(
    overlays: &[SongLuaOverlayActor],
    local_states: &[SongLuaOverlayState],
    screen_width: f32,
    screen_height: f32,
) -> Vec<SongLuaOverlayState> {
    let mut out = Vec::with_capacity(overlays.len());
    song_lua_overlay_states_from_local_into(
        overlays,
        local_states,
        screen_width,
        screen_height,
        &mut out,
    );
    out
}

fn song_lua_overlay_states_from_local_into(
    overlays: &[SongLuaOverlayActor],
    local_states: &[SongLuaOverlayState],
    screen_width: f32,
    screen_height: f32,
    out: &mut Vec<SongLuaOverlayState>,
) {
    out.clear();
    out.reserve(overlays.len());
    for (idx, overlay) in overlays.iter().enumerate() {
        let local = local_states.get(idx).copied().unwrap_or_default();
        let composed = overlay
            .parent_index
            .and_then(|parent_index| {
                out.get(parent_index)
                    .copied()
                    .zip(overlays.get(parent_index))
            })
            .map(|(parent, parent_overlay)| {
                song_lua_overlay_compose_state(
                    &parent_overlay.kind,
                    parent,
                    local,
                    screen_width,
                    screen_height,
                )
            })
            .unwrap_or(local);
        out.push(composed);
    }
}

fn song_lua_overlay_state_sets_from_into(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    screen_width: f32,
    screen_height: f32,
    local_out: &mut Vec<SongLuaOverlayState>,
    overlay_out: &mut Vec<SongLuaOverlayState>,
) {
    song_lua_overlay_local_states_into(
        now,
        overlays,
        overlay_events,
        overlay_eases,
        overlay_ease_ranges,
        local_out,
    );
    song_lua_overlay_states_from_local_into(
        overlays,
        local_out,
        screen_width,
        screen_height,
        overlay_out,
    );
}

fn song_lua_overlay_state_sets_into(
    state: &State,
    local_out: &mut Vec<SongLuaOverlayState>,
    overlay_out: &mut Vec<SongLuaOverlayState>,
) {
    let song_lua_visuals = state.song_lua_visuals();
    song_lua_overlay_state_sets_from_into(
        state.current_music_time_display(),
        &song_lua_visuals.overlays,
        &song_lua_visuals.overlay_events,
        &song_lua_visuals.overlay_eases,
        &song_lua_visuals.overlay_ease_ranges,
        song_lua_visuals.screen_width,
        song_lua_visuals.screen_height,
        local_out,
        overlay_out,
    );
}

fn song_lua_proxy_active_players(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> [bool; 2] {
    let mut out = [false; 2];
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let SongLuaOverlayKind::ActorProxy { target } = &overlay.kind else {
            continue;
        };
        let player_index = match target {
            SongLuaProxyTarget::Player { player_index }
            | SongLuaProxyTarget::NoteField { player_index } => *player_index,
            _ => continue,
        };
        if player_index >= out.len() {
            continue;
        }
        let has_source = match target {
            SongLuaProxyTarget::Player { .. } => proxy_sources[player_index]
                .player
                .is_some_and(|source| !source.is_empty()),
            SongLuaProxyTarget::NoteField { .. } => proxy_sources[player_index]
                .note_field
                .is_some_and(|source| !source.is_empty()),
            _ => false,
        };
        if !has_source {
            continue;
        }
        if overlay_states
            .get(overlay_index)
            .copied()
            .is_some_and(song_lua_overlay_is_visible)
        {
            out[player_index] = true;
        }
    }
    out
}

fn song_lua_proxy_target_has_source(
    target: &SongLuaProxyTarget,
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> bool {
    match target {
        SongLuaProxyTarget::Player { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.player)
            .is_some_and(|source| !source.is_empty()),
        SongLuaProxyTarget::NoteField { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.note_field)
            .is_some_and(|source| !source.is_empty()),
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.judgment)
            .is_some_and(|source| !source.is_empty()),
        SongLuaProxyTarget::Combo { player_index } => proxy_sources
            .get(*player_index)
            .and_then(|sources| sources.combo)
            .is_some_and(|source| !source.is_empty()),
        SongLuaProxyTarget::Underlay | SongLuaProxyTarget::Overlay => false,
    }
}

fn song_lua_capture_replaces_player(
    overlays: &[SongLuaOverlayActor],
    capture_index: usize,
    player_index: usize,
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> bool {
    overlays.iter().enumerate().any(|(idx, overlay)| {
        if song_lua_overlay_aft_ancestor(overlays, idx) != Some(capture_index) {
            return false;
        }
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                matches!(
                    target,
                    SongLuaProxyTarget::Player { player_index: target_player }
                        | SongLuaProxyTarget::NoteField { player_index: target_player }
                        if *target_player == player_index
                ) && song_lua_proxy_target_has_source(target, proxy_sources)
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                song_lua_overlay_capture_index_by_name(overlays, capture_name).is_some_and(
                    |nested_capture| {
                        song_lua_capture_replaces_player(
                            overlays,
                            nested_capture,
                            player_index,
                            proxy_sources,
                        )
                    },
                )
            }
            _ => false,
        }
    })
}

fn song_lua_replacement_active_players(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
) -> [bool; 2] {
    let mut out = song_lua_proxy_active_players(overlays, overlay_states, proxy_sources);
    for (overlay_index, overlay) in overlays.iter().enumerate() {
        let Some(overlay_state) = overlay_states.get(overlay_index) else {
            continue;
        };
        if !overlay_state.visible || overlay_state.diffuse[3] <= f32::EPSILON {
            continue;
        }
        let SongLuaOverlayKind::AftSprite { capture_name } = &overlay.kind else {
            continue;
        };
        let Some(capture_index) = song_lua_overlay_capture_index_by_name(overlays, capture_name)
        else {
            continue;
        };
        for player_index in 0..out.len() {
            if song_lua_capture_replaces_player(
                overlays,
                capture_index,
                player_index,
                proxy_sources,
            ) {
                out[player_index] = true;
            }
        }
    }
    out
}

fn song_lua_overlay_aft_ancestor(
    overlays: &[SongLuaOverlayActor],
    mut index: usize,
) -> Option<usize> {
    while let Some(parent_index) = overlays.get(index).and_then(|overlay| overlay.parent_index) {
        match overlays.get(parent_index).map(|overlay| &overlay.kind) {
            Some(SongLuaOverlayKind::ActorFrameTexture) => return Some(parent_index),
            Some(_) => index = parent_index,
            None => return None,
        }
    }
    None
}

fn song_lua_overlay_capture_index_by_name(
    overlays: &[SongLuaOverlayActor],
    capture_name: &str,
) -> Option<usize> {
    overlays.iter().position(|overlay| {
        matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture)
            && overlay
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(capture_name))
    })
}

#[derive(Clone, Copy, Default)]
struct SongLuaPlayerProxySources<'a> {
    player: Option<&'a [Arc<[Actor]>]>,
    note_field: Option<&'a [Arc<[Actor]>]>,
    judgment: Option<&'a [Arc<[Actor]>]>,
    combo: Option<&'a [Arc<[Actor]>]>,
}

#[derive(Clone, Copy, Default)]
struct SongLuaPlayerProxyRequests {
    player: bool,
    note_field: bool,
    judgment: bool,
    combo: bool,
}

#[derive(Clone, Copy, Default)]
struct SongLuaScreenProxySources<'a> {
    players: [SongLuaPlayerProxySources<'a>; 2],
    underlay: Option<&'a [Arc<[Actor]>]>,
    overlay: Option<&'a [Arc<[Actor]>]>,
}

#[derive(Clone, Copy, Default)]
struct SongLuaScreenProxyRequests {
    players: [SongLuaPlayerProxyRequests; 2],
    underlay: bool,
    overlay: bool,
}

#[inline(always)]
fn song_lua_overlay_is_visible(state: SongLuaOverlayState) -> bool {
    state.visible && state.diffuse[3] > f32::EPSILON
}

#[inline(always)]
fn song_lua_capture_new_actors(
    dest: &mut Option<Vec<Arc<[Actor]>>>,
    actors: &mut Vec<Actor>,
    start: usize,
) {
    let Some(dest) = dest.as_mut() else {
        return;
    };
    if start >= actors.len() {
        return;
    }
    let children = Arc::<[Actor]>::from(actors.drain(start..).collect::<Vec<_>>());
    if children.is_empty() {
        return;
    }
    dest.push(Arc::clone(&children));
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

fn song_lua_player_child_proxy_source(
    actors: Vec<Actor>,
    origin_x: f32,
    origin_y: f32,
) -> Option<Vec<Arc<[Actor]>>> {
    if actors.is_empty() {
        return None;
    }
    Some(vec![Arc::from(vec![Actor::Frame {
        align: [0.0, 0.0],
        offset: [-origin_x, -origin_y],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: actors,
        background: None,
        z: 0,
    }])])
}

fn song_lua_share_actor_source_in_place(actors: &mut Vec<Actor>) -> Option<Vec<Arc<[Actor]>>> {
    if actors.is_empty() {
        return None;
    }
    let children = Arc::<[Actor]>::from(actors.drain(..).collect::<Vec<_>>());
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Arc::clone(&children),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
    Some(vec![children])
}

fn song_lua_shared_segment_actors(segments: Vec<Arc<[Actor]>>) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(segments.len());
    for segment in segments {
        actors.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: segment,
            background: None,
            z: 0,
            tint: [1.0; 4],
            blend: None,
        });
    }
    actors
}

fn song_lua_owned_segment_actors(segments: Vec<Arc<[Actor]>>) -> Vec<Actor> {
    let mut actors = Vec::new();
    for segment in segments {
        actors.reserve(segment.len());
        actors.extend(segment.iter().cloned());
    }
    actors
}

#[inline(always)]
fn song_lua_proxy_source<'a>(
    target: &SongLuaProxyTarget,
    proxy_sources: &SongLuaScreenProxySources<'a>,
) -> Option<&'a [Arc<[Actor]>]> {
    match target {
        SongLuaProxyTarget::Player { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.player.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::NoteField { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.note_field.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::Judgment { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.judgment.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::Combo { player_index } => proxy_sources
            .players
            .get(*player_index)
            .and_then(|sources| sources.combo.filter(|source| !source.is_empty())),
        SongLuaProxyTarget::Underlay => proxy_sources
            .underlay
            .filter(|segments| !segments.is_empty()),
        SongLuaProxyTarget::Overlay => proxy_sources
            .overlay
            .filter(|segments| !segments.is_empty()),
    }
}

fn song_lua_mark_proxy_target(
    requests: &mut SongLuaScreenProxyRequests,
    target: &SongLuaProxyTarget,
) {
    match target {
        SongLuaProxyTarget::Player { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.player = true;
            }
        }
        SongLuaProxyTarget::NoteField { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.note_field = true;
            }
        }
        SongLuaProxyTarget::Judgment { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.judgment = true;
            }
        }
        SongLuaProxyTarget::Combo { player_index } => {
            if let Some(player) = requests.players.get_mut(*player_index) {
                player.combo = true;
            }
        }
        SongLuaProxyTarget::Underlay => requests.underlay = true,
        SongLuaProxyTarget::Overlay => requests.overlay = true,
    }
}

fn song_lua_collect_capture_requests(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    capture_index: usize,
    requests: &mut SongLuaScreenProxyRequests,
    capture_stack: &mut Vec<usize>,
) {
    if capture_stack.contains(&capture_index) {
        return;
    }
    capture_stack.push(capture_index);
    for (idx, overlay) in overlays.iter().enumerate() {
        if song_lua_overlay_aft_ancestor(overlays, idx) != Some(capture_index) {
            continue;
        }
        let Some(overlay_state) = overlay_states.get(idx).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(requests, target);
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                if let Some(nested_capture) =
                    song_lua_overlay_capture_index_by_name(overlays, capture_name)
                {
                    song_lua_collect_capture_requests(
                        overlays,
                        overlay_states,
                        nested_capture,
                        requests,
                        capture_stack,
                    );
                }
            }
            _ => {}
        }
    }
    capture_stack.pop();
}

fn song_lua_proxy_requests(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
) -> SongLuaScreenProxyRequests {
    let mut requests = SongLuaScreenProxyRequests::default();
    let mut capture_stack = Vec::new();
    for (idx, overlay) in overlays.iter().enumerate() {
        if song_lua_overlay_aft_ancestor(overlays, idx).is_some() {
            continue;
        }
        let Some(overlay_state) = overlay_states.get(idx).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(&mut requests, target);
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                if let Some(capture_index) =
                    song_lua_overlay_capture_index_by_name(overlays, capture_name)
                {
                    song_lua_collect_capture_requests(
                        overlays,
                        overlay_states,
                        capture_index,
                        &mut requests,
                        &mut capture_stack,
                    );
                }
            }
            _ => {}
        }
    }
    requests
}

fn song_lua_merge_proxy_requests(
    into: &mut SongLuaScreenProxyRequests,
    from: SongLuaScreenProxyRequests,
) {
    for player_index in 0..into.players.len() {
        into.players[player_index].player |= from.players[player_index].player;
        into.players[player_index].note_field |= from.players[player_index].note_field;
        into.players[player_index].judgment |= from.players[player_index].judgment;
        into.players[player_index].combo |= from.players[player_index].combo;
    }
    into.underlay |= from.underlay;
    into.overlay |= from.overlay;
}

fn song_lua_build_proxy_actor(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Arc<[Actor]>],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = Some(song_lua_overlay_blend(state.blend));
    let mut children = Vec::with_capacity(source.len());
    for segment in source {
        children.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: song_lua_proxy_source_segment(segment),
            background: None,
            z: 0,
            tint: state.diffuse,
            blend,
        });
    }
    Some(Actor::Frame {
        align: [0.0, 0.0],
        offset: [
            state.x * screen_width() / overlay_space_width.max(1.0),
            state.y * screen_height() / overlay_space_height.max(1.0),
        ],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z,
    })
}

fn song_lua_proxy_source_segment(segment: &Arc<[Actor]>) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    Arc::from(song_lua_proxy_local_children(segment.iter().cloned()))
}

fn song_lua_proxy_actor_has_z(actor: &Actor) -> bool {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::TexturedMesh { z, .. } => *z != 0,
        Actor::Frame { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::SharedFrame { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::Camera { children, .. } => children.iter().any(song_lua_proxy_actor_has_z),
        Actor::Shadow { child, .. } => song_lua_proxy_actor_has_z(child),
        Actor::CameraPush { .. } | Actor::CameraPop => false,
    }
}

fn song_lua_proxy_actor_z(actor: &Actor) -> i16 {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::Frame { z, .. }
        | Actor::SharedFrame { z, .. } => *z,
        Actor::Shadow { child, .. } => song_lua_proxy_actor_z(child),
        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop => 0,
    }
}

fn song_lua_proxy_local_children(children: impl Iterator<Item = Actor>) -> Vec<Actor> {
    let mut children = children.collect::<Vec<_>>();
    if children
        .iter()
        .any(|actor| matches!(actor, Actor::CameraPush { .. } | Actor::CameraPop))
    {
        return song_lua_proxy_local_children_with_camera_scopes(children);
    }
    children.sort_by_key(song_lua_proxy_actor_z);
    children
        .into_iter()
        .map(song_lua_proxy_local_actor)
        .collect()
}

fn song_lua_proxy_local_children_with_camera_scopes(children: Vec<Actor>) -> Vec<Actor> {
    let mut out = Vec::with_capacity(children.len());
    let mut run = Vec::new();
    for actor in children {
        if matches!(actor, Actor::CameraPush { .. } | Actor::CameraPop) {
            out.extend(song_lua_proxy_local_children(run.drain(..)));
            out.push(song_lua_proxy_local_actor(actor));
        } else {
            run.push(actor);
        }
    }
    out.extend(song_lua_proxy_local_children(run.drain(..)));
    out
}

fn song_lua_proxy_local_actor(actor: Actor) -> Actor {
    match actor {
        Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint,
            glow,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
            ..
        } => Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint,
            glow,
            z: 0,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        },
        Actor::Text {
            align,
            offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            effect,
            ..
        } => Actor::Text {
            align,
            offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z: 0,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            effect,
        },
        Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            visible,
            blend,
            ..
        } => Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            visible,
            blend,
            z: 0,
        },
        Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            ..
        } => Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z: 0,
        },
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            ..
        } => Actor::Frame {
            align,
            offset,
            size,
            children: song_lua_proxy_local_children(children.into_iter()),
            background,
            z: 0,
        },
        Actor::SharedFrame {
            align,
            offset,
            size,
            children,
            background,
            tint,
            blend,
            ..
        } => Actor::SharedFrame {
            align,
            offset,
            size,
            children: song_lua_proxy_source_segment(&children),
            background,
            z: 0,
            tint,
            blend,
        },
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: song_lua_proxy_local_children(children.into_iter()),
        },
        Actor::CameraPush { view_proj } => Actor::CameraPush { view_proj },
        Actor::CameraPop => Actor::CameraPop,
        Actor::Shadow { len, color, child } => Actor::Shadow {
            len,
            color,
            child: Box::new(song_lua_proxy_local_actor(*child)),
        },
    }
}

#[cfg(test)]
fn song_lua_overlay_order(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    parent_index: Option<usize>,
) -> Vec<usize> {
    let mut cache = song_lua_overlay_order_cache_from(overlays, &[]);
    let mut out = Vec::with_capacity(overlays.len());
    song_lua_overlay_order_into(overlays, overlay_states, &mut cache, parent_index, &mut out);
    out
}

fn song_lua_overlay_order_into(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    parent_index: Option<usize>,
    out: &mut Vec<usize>,
) {
    out.clear();
    out.reserve(overlays.len());
    song_lua_push_order(overlays, overlay_states, order_cache, parent_index, out);
}

fn song_lua_push_order(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    parent_index: Option<usize>,
    out: &mut Vec<usize>,
) {
    let list_idx = song_lua_overlay_child_list_index(parent_index);
    if list_idx >= order_cache.child_lists.len() {
        return;
    }
    let draw_by_z_position = parent_index.is_some_and(|idx| {
        overlay_states
            .get(idx)
            .map_or(overlays[idx].initial_state.draw_by_z_position, |state| {
                state.draw_by_z_position
            })
    });
    if draw_by_z_position {
        order_cache.child_lists[list_idx].sort_by(|&left, &right| {
            let left_z = overlay_states
                .get(left)
                .map_or(overlays[left].initial_state.z, |state| state.z);
            let right_z = overlay_states
                .get(right)
                .map_or(overlays[right].initial_state.z, |state| state.z);
            left_z.total_cmp(&right_z).then_with(|| left.cmp(&right))
        });
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_Z;
    } else if order_cache
        .dynamic_draw_order
        .get(list_idx)
        .copied()
        .unwrap_or(false)
    {
        order_cache.child_lists[list_idx].sort_by_key(|&idx| {
            (
                overlay_states
                    .get(idx)
                    .map_or(overlays[idx].initial_state.draw_order, |state| {
                        state.draw_order
                    }),
                idx,
            )
        });
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_DRAW;
    } else if order_cache
        .sort_modes
        .get(list_idx)
        .copied()
        .unwrap_or(SONG_LUA_CHILD_ORDER_STATIC)
        != SONG_LUA_CHILD_ORDER_STATIC
    {
        song_lua_sort_static_children(overlays, &mut order_cache.child_lists[list_idx]);
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_STATIC;
    }
    let child_count = order_cache.child_lists[list_idx].len();
    for child_pos in 0..child_count {
        let idx = order_cache.child_lists[list_idx][child_pos];
        out.push(idx);
        song_lua_push_order(overlays, overlay_states, order_cache, Some(idx), out);
    }
}

fn song_lua_capture_root_state(state: SongLuaOverlayState) -> SongLuaOverlayState {
    SongLuaOverlayState {
        draw_order: state.draw_order,
        draw_by_z_position: state.draw_by_z_position,
        glow: state.glow,
        fov: state.fov,
        vanishpoint: state.vanishpoint,
        diffuse: state.diffuse,
        visible: state.visible,
        mask_source: state.mask_source,
        mask_dest: state.mask_dest,
        depth_test: state.depth_test,
        blend: state.blend,
        ..SongLuaOverlayState::default()
    }
}

fn song_lua_capture_overlay_states_into_scratch(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    capture_index: usize,
    overlay_space_width: f32,
    overlay_space_height: f32,
    out: &mut Vec<SongLuaOverlayState>,
) {
    out.clear();
    out.resize(overlays.len(), SongLuaOverlayState::default());
    let Some(capture_state) = overlay_states.get(capture_index).copied() else {
        return;
    };
    // AFTs capture in texture space; placement transforms apply to the sprite
    // that consumes the texture, not to the captured children.
    out[capture_index] = song_lua_capture_root_state(capture_state);
    song_lua_capture_overlay_child_states(
        overlays,
        local_overlay_states,
        order_cache,
        capture_index,
        overlay_space_width,
        overlay_space_height,
        out,
    );
}

fn song_lua_capture_overlay_child_states(
    overlays: &[SongLuaOverlayActor],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &SongLuaOverlayOrderCache,
    parent_index: usize,
    overlay_space_width: f32,
    overlay_space_height: f32,
    out: &mut [SongLuaOverlayState],
) {
    let list_idx = song_lua_overlay_child_list_index(Some(parent_index));
    let Some(children) = order_cache.child_lists.get(list_idx) else {
        return;
    };
    for &idx in children {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        let local = local_overlay_states.get(idx).copied().unwrap_or_default();
        let parent = out.get(parent_index).copied().unwrap_or_default();
        let parent_overlay = &overlays[parent_index];
        out[idx] = song_lua_overlay_compose_state(
            &parent_overlay.kind,
            parent,
            local,
            overlay_space_width,
            overlay_space_height,
        );
        if !matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture) {
            song_lua_capture_overlay_child_states(
                overlays,
                local_overlay_states,
                order_cache,
                idx,
                overlay_space_width,
                overlay_space_height,
                out,
            );
        }
    }
}

fn song_lua_capture_children(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    asset_manager: &AssetManager,
    capture_index: usize,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    capture_states: &mut Vec<SongLuaOverlayState>,
    order_scratch: &mut Vec<usize>,
) -> Vec<Actor> {
    song_lua_capture_overlay_states_into_scratch(
        overlays,
        overlay_states,
        local_overlay_states,
        order_cache,
        capture_index,
        overlay_space_width,
        overlay_space_height,
        capture_states,
    );
    let mut out = Vec::new();
    song_lua_overlay_order_into(
        overlays,
        capture_states,
        order_cache,
        Some(capture_index),
        order_scratch,
    );
    out.reserve(order_scratch.len());
    for (draw_idx, idx) in order_scratch.iter().copied().enumerate() {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        if song_lua_overlay_aft_ancestor(overlays, idx) != Some(capture_index) {
            continue;
        }
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::Actor
                | SongLuaOverlayKind::ActorFrame
                | SongLuaOverlayKind::ActorFrameTexture
        ) {
            continue;
        }
        let overlay_state = capture_states.get(idx).copied().unwrap_or_default();
        let actor = match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                    song_lua_build_proxy_actor(
                        overlay_state,
                        draw_idx.min(i16::MAX as usize) as i16,
                        source,
                        overlay_space_width,
                        overlay_space_height,
                    )
                    .map(one_song_lua_actor)
                })
            }
            _ => build_song_lua_overlay_actor(
                overlay,
                overlay_state,
                song_lua_overlay_camera_state(overlays, capture_states, overlay.parent_index),
                asset_manager,
                draw_idx.min(i16::MAX as usize) as i16,
                overlay_space_width,
                overlay_space_height,
                0.0,
                0.0,
                0.0,
            ),
        };
        if let Some(actors) = actor {
            out.extend(actors);
        }
    }
    out
}

fn song_lua_overlay_apply_blocks(
    state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    let mut current = state;
    for block in blocks {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_song_lua_overlay_delta(&mut current, &block.delta);
            continue;
        }
        let target = song_lua_overlay_state_with_delta(current, &block.delta);
        let t = song_lua_ease_factor(
            block.easing.as_deref(),
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            block.opt1,
            block.opt2,
        );
        return song_lua_overlay_state_lerp(current, target, t, &block.delta);
    }
    current
}

fn apply_song_lua_overlay_runtime_eases_for(
    now: f32,
    overlay_index: usize,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    mut current: SongLuaOverlayState,
) -> SongLuaOverlayState {
    let Some(ease_range) = overlay_ease_ranges.get(overlay_index) else {
        return current;
    };
    for ease in &overlay_eases[ease_range.clone()] {
        if ease.overlay_index != overlay_index || now < ease.start_second {
            continue;
        }
        if let Some(cutoff_second) = ease.cutoff_second
            && now >= cutoff_second
        {
            continue;
        }
        if now >= ease.sustain_end_second {
            apply_song_lua_overlay_delta(&mut current, &ease.to.delta);
            continue;
        }
        if ease.end_second <= ease.start_second || now >= ease.end_second {
            apply_song_lua_overlay_delta(&mut current, &ease.to.delta);
            continue;
        }
        let t = song_lua_ease_factor(
            ease.easing.as_deref(),
            ((now - ease.start_second) / (ease.end_second - ease.start_second)).clamp(0.0, 1.0),
            ease.opt1,
            ease.opt2,
        );
        let from_state = song_lua_overlay_state_with_delta(current, &ease.from.delta);
        let to_state = song_lua_overlay_state_with_delta(current, &ease.to.delta);
        current = song_lua_overlay_state_lerp(from_state, to_state, t, &ease.to.delta);
    }
    current
}

fn song_lua_overlay_render_state_from(
    now: f32,
    overlay_index: usize,
    overlay: &SongLuaOverlayActor,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
) -> SongLuaOverlayState {
    let current = song_lua_message_state(
        now,
        overlay.initial_state,
        &overlay.message_commands,
        overlay_events.get(overlay_index).map(Vec::as_slice),
    );
    apply_song_lua_overlay_runtime_eases_for(
        now,
        overlay_index,
        overlay_eases,
        overlay_ease_ranges,
        current,
    )
}

fn song_lua_message_state(
    now: f32,
    initial_state: SongLuaOverlayState,
    message_commands: &[SongLuaOverlayMessageCommand],
    events: Option<&[SongLuaOverlayMessageRuntime]>,
) -> SongLuaOverlayState {
    let Some(events) = events else {
        return initial_state;
    };
    let mut current = initial_state;
    let mut active: Option<(&[SongLuaOverlayCommandBlock], SongLuaOverlayState, f32)> = None;
    for event in events {
        let event_second = event.event_second;
        if event_second > now {
            break;
        }
        let Some(command) = message_commands.get(event.command_index) else {
            continue;
        };
        if let Some((blocks, base, start_second)) = active.take() {
            current = song_lua_overlay_apply_blocks(base, blocks, event_second - start_second);
        }
        let base = current;
        current = song_lua_overlay_apply_blocks(base, &command.blocks, 0.0);
        active = Some((&command.blocks, base, event_second));
    }
    if let Some((blocks, base, start_second)) = active {
        current = song_lua_overlay_apply_blocks(base, blocks, now - start_second);
    }
    current
}

fn song_lua_player_render_state(state: &State, player_index: usize) -> SongLuaOverlayState {
    let song_lua_visuals = state.song_lua_visuals();
    let Some(actor) = song_lua_visuals.player_actors.get(player_index) else {
        return SongLuaOverlayState::default();
    };
    song_lua_message_state(
        state.current_music_time_display(),
        actor.initial_state,
        &actor.message_commands,
        song_lua_visuals
            .player_events
            .get(player_index)
            .map(Vec::as_slice),
    )
}

fn song_lua_song_foreground_state_from(
    now: f32,
    song_foreground: &SongLuaCapturedActor,
    events: &[SongLuaOverlayMessageRuntime],
) -> SongLuaOverlayState {
    song_lua_message_state(
        now,
        song_foreground.initial_state,
        &song_foreground.message_commands,
        Some(events),
    )
}

fn song_lua_song_foreground_state(state: &State) -> SongLuaOverlayState {
    let song_lua_visuals = state.song_lua_visuals();
    song_lua_song_foreground_state_from(
        state.current_music_time_display(),
        &song_lua_visuals.song_foreground,
        song_lua_visuals.song_foreground_events.as_slice(),
    )
}

fn song_lua_capture_tint(color: [f32; 4], tint: [f32; 4]) -> [f32; 4] {
    [
        color[0] * tint[0],
        color[1] * tint[1],
        color[2] * tint[2],
        color[3] * tint[3],
    ]
}

fn song_lua_biased_world_z(state: SongLuaOverlayState, effect_z: f32) -> f32 {
    effect_z + state.z_bias
}

fn song_lua_add_z(z: i16, delta: i16) -> i16 {
    (i32::from(z) + i32::from(delta)).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

const SONG_LUA_PLAYER_LAYER_Z_BASE: i16 = 900;
const SONG_LUA_OVERLAY_LAYER_Z_BASE: i16 = 1100;

fn song_lua_rounded_z(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn song_lua_player_layer_z(
    song_lua_active: bool,
    actor: &SongLuaCapturedActor,
    current: SongLuaOverlayState,
    runtime_z: f32,
) -> i16 {
    if !song_lua_active {
        return 0;
    }
    let _ = actor;
    song_lua_add_z(
        SONG_LUA_PLAYER_LAYER_Z_BASE,
        song_lua_rounded_z(current.z + runtime_z),
    )
}

fn song_lua_style_capture_actor(
    actor: Actor,
    capture_tint: [f32; 4],
    blend: Option<BlendMode>,
    z_shift: i16,
) -> Actor {
    match actor {
        Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint: actor_tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend: actor_blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            glow: song_lua_capture_tint(glow, capture_tint),
            z: song_lua_add_z(z, z_shift),
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend: blend.unwrap_or(actor_blend),
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color: song_lua_capture_tint(shadow_color, capture_tint),
            effect,
        },
        Actor::Text {
            align,
            offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend: actor_blend,
            shadow_len,
            shadow_color,
            effect,
        } => Actor::Text {
            align,
            offset,
            local_transform,
            color: song_lua_capture_tint(color, capture_tint),
            stroke_color: stroke_color.map(|color| song_lua_capture_tint(color, capture_tint)),
            glow: song_lua_capture_tint(glow, capture_tint),
            font,
            content,
            attributes,
            align_text,
            z: song_lua_add_z(z, z_shift),
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend: blend.unwrap_or(actor_blend),
            shadow_len,
            shadow_color: song_lua_capture_tint(shadow_color, capture_tint),
            effect,
        },
        Actor::Mesh {
            align,
            offset,
            size,
            vertices,
            visible,
            blend: actor_blend,
            z,
        } => {
            let vertices = if capture_tint == [1.0; 4] {
                vertices
            } else {
                Arc::from(
                    vertices
                        .iter()
                        .copied()
                        .map(|mut vertex| {
                            vertex.color = song_lua_capture_tint(vertex.color, capture_tint);
                            vertex
                        })
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
            };
            Actor::Mesh {
                align,
                offset,
                size,
                vertices,
                visible,
                blend: blend.unwrap_or(actor_blend),
                z: song_lua_add_z(z, z_shift),
            }
        }
        Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint: actor_tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: actor_blend,
            z,
        } => Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            glow: song_lua_capture_tint(glow, capture_tint),
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        } => Actor::Frame {
            align,
            offset,
            size,
            children: children
                .into_iter()
                .map(|child| song_lua_style_capture_actor(child, capture_tint, blend, z_shift))
                .collect(),
            background,
            z: song_lua_add_z(z, z_shift),
        },
        Actor::SharedFrame {
            align,
            offset,
            size,
            children,
            background,
            z,
            tint: actor_tint,
            blend: actor_blend,
        } => Actor::SharedFrame {
            align,
            offset,
            size,
            children,
            background,
            z: song_lua_add_z(z, z_shift),
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            blend: blend.or(actor_blend),
        },
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_style_capture_actor(child, capture_tint, blend, z_shift))
                .collect(),
        },
        Actor::CameraPush { view_proj } => Actor::CameraPush { view_proj },
        Actor::CameraPop => Actor::CameraPop,
        Actor::Shadow { len, color, child } => Actor::Shadow {
            len,
            color: song_lua_capture_tint(color, capture_tint),
            child: Box::new(song_lua_style_capture_actor(
                *child,
                capture_tint,
                blend,
                z_shift,
            )),
        },
    }
}

fn song_lua_capture_transform_matrix(
    state: SongLuaOverlayState,
    extra_offset: [f32; 2],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Matrix4> {
    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let translate_x = (state.x - 0.5 * overlay_space_width) * x_scale + extra_offset[0];
    let translate_y = (state.y - 0.5 * overlay_space_height) * y_scale + extra_offset[1];
    let [scale_x, scale_y] = song_lua_overlay_axis_scale(state);
    let scale_z = song_lua_overlay_z_scale(state);
    if translate_x.abs() <= f32::EPSILON
        && translate_y.abs() <= f32::EPSILON
        && state.rot_z_deg.abs() <= f32::EPSILON
        && (scale_x - 1.0).abs() <= f32::EPSILON
        && (scale_y - 1.0).abs() <= f32::EPSILON
        && (scale_z - 1.0).abs() <= f32::EPSILON
    {
        return None;
    }
    Some(
        Matrix4::from_translation(Vector3::new(translate_x, -translate_y, 0.0))
            * Matrix4::from_rotation_z(state.rot_z_deg.to_radians())
            * Matrix4::from_scale(Vector3::new(scale_x, scale_y, scale_z)),
    )
}

fn song_lua_capture_channel_offset(
    name: Option<&str>,
    state: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> [f32; 2] {
    if !state.vibrate {
        return [0.0, 0.0];
    }
    let x = state.effect_magnitude[0].abs() * (screen_width() / overlay_space_width.max(1.0));
    let y =
        state.effect_magnitude[1].abs() * (screen_height() / overlay_space_height.max(1.0)) * 0.25;
    match name {
        Some(name) if name.ends_with('R') => [-x, -y],
        Some(name) if name.ends_with('B') => [x, y],
        _ => [0.0, 0.0],
    }
}

fn song_lua_rgb_aft_channel(state: SongLuaOverlayState) -> Option<usize> {
    const EPS: f32 = 0.0001;
    if !state.visible
        || state.diffuse[3] <= f32::EPSILON
        || state.blend != SongLuaOverlayBlendMode::Add
        || (state.vibrate && state.effect_magnitude.iter().any(|value| value.abs() > EPS))
    {
        return None;
    }
    let [r, g, b, _] = state.diffuse;
    if r >= 1.0 - EPS && g.abs() <= EPS && b.abs() <= EPS {
        Some(0)
    } else if g >= 1.0 - EPS && r.abs() <= EPS && b.abs() <= EPS {
        Some(1)
    } else if b >= 1.0 - EPS && r.abs() <= EPS && g.abs() <= EPS {
        Some(2)
    } else {
        None
    }
}

fn song_lua_rgb_aft_norm_state(mut state: SongLuaOverlayState) -> SongLuaOverlayState {
    state.diffuse = [1.0, 1.0, 1.0, state.diffuse[3]];
    state
}

fn song_lua_rgb_aft_group_for(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    draw_order: &[usize],
    index: usize,
) -> Option<(usize, [usize; 3])> {
    let overlay = overlays.get(index)?;
    let SongLuaOverlayKind::AftSprite { capture_name } = &overlay.kind else {
        return None;
    };
    let state = overlay_states.get(index).copied().unwrap_or_default();
    let channel = song_lua_rgb_aft_channel(state)?;
    let norm = song_lua_rgb_aft_norm_state(state);
    let mut group = [usize::MAX; 3];
    group[channel] = index;
    for (idx, candidate) in overlays.iter().enumerate() {
        if idx == index {
            continue;
        }
        let SongLuaOverlayKind::AftSprite {
            capture_name: candidate_capture,
        } = &candidate.kind
        else {
            continue;
        };
        if !candidate_capture.eq_ignore_ascii_case(capture_name) {
            continue;
        }
        let candidate_state = overlay_states.get(idx).copied().unwrap_or_default();
        let Some(candidate_channel) = song_lua_rgb_aft_channel(candidate_state) else {
            continue;
        };
        if song_lua_rgb_aft_norm_state(candidate_state) != norm {
            continue;
        }
        if group[candidate_channel] != usize::MAX {
            return None;
        }
        group[candidate_channel] = idx;
    }
    if group.contains(&usize::MAX) {
        return None;
    }
    let leader = draw_order
        .iter()
        .copied()
        .find(|idx| group.contains(idx))
        .unwrap_or(index);
    Some((leader, group))
}

fn song_lua_combined_rgb_aft_state(mut state: SongLuaOverlayState) -> SongLuaOverlayState {
    // ITGmania blends the finished AFT texture, not each captured actor.
    // Three aligned R/G/B additive sprites reconstruct that texture exactly,
    // so the render-target approximation should keep child blend modes intact.
    state.diffuse = [1.0, 1.0, 1.0, state.diffuse[3]];
    state.blend = SongLuaOverlayBlendMode::Alpha;
    state
}

fn song_lua_build_capture_actor(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    z: i16,
    source: Vec<Actor>,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = match state.blend {
        SongLuaOverlayBlendMode::Alpha => None,
        SongLuaOverlayBlendMode::Add => Some(BlendMode::Add),
        SongLuaOverlayBlendMode::Multiply => Some(BlendMode::Multiply),
        SongLuaOverlayBlendMode::Subtract => Some(BlendMode::Subtract),
    };
    let children = source
        .into_iter()
        .map(|actor| song_lua_style_capture_actor(actor, state.diffuse, blend, z))
        .collect::<Vec<_>>();
    let extra_offset = song_lua_capture_channel_offset(
        overlay.name.as_deref(),
        state,
        overlay_space_width,
        overlay_space_height,
    );
    if let Some(transform) = song_lua_capture_transform_matrix(
        state,
        extra_offset,
        overlay_space_width,
        overlay_space_height,
    ) {
        return Some(Actor::Camera {
            view_proj: Matrix4::orthographic_rh_gl(
                -0.5 * screen_width(),
                0.5 * screen_width(),
                -0.5 * screen_height(),
                0.5 * screen_height(),
                -1.0,
                1.0,
            ) * transform,
            children,
        });
    }
    Some(Actor::Frame {
        align: [0.0, 0.0],
        offset: extra_offset,
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z: 0,
    })
}

#[inline(always)]
fn song_lua_overlay_blend(blend: SongLuaOverlayBlendMode) -> BlendMode {
    match blend {
        SongLuaOverlayBlendMode::Alpha => BlendMode::Alpha,
        SongLuaOverlayBlendMode::Add => BlendMode::Add,
        SongLuaOverlayBlendMode::Multiply => BlendMode::Multiply,
        SongLuaOverlayBlendMode::Subtract => BlendMode::Subtract,
    }
}

#[inline(always)]
fn song_lua_overlay_effect_state(state: SongLuaOverlayState) -> EffectState {
    let period = state.effect_period.max(f32::EPSILON);
    EffectState {
        clock: state.effect_clock,
        mode: state.effect_mode,
        color1: state.effect_color1,
        color2: state.effect_color2,
        period,
        offset: state.effect_offset,
        timing: state
            .effect_timing
            .unwrap_or([period * 0.5, 0.0, period * 0.5, 0.0, 0.0]),
        magnitude: state.effect_magnitude,
        ..EffectState::default()
    }
}

#[inline(always)]
fn song_lua_effect_lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn song_lua_overlay_has_visible_output(state: SongLuaOverlayState) -> bool {
    if state.diffuse[3] > f32::EPSILON || state.glow[3] > f32::EPSILON {
        return true;
    }
    matches!(
        state.effect_mode,
        deadlib_present::anim::EffectMode::GlowShift
    ) && (state.effect_color1[3] > f32::EPSILON || state.effect_color2[3] > f32::EPSILON)
}

fn song_lua_apply_overlay_effect(
    effect: EffectState,
    rainbow: bool,
    effect_time: f32,
    effect_beat: f32,
    tint: &mut [f32; 4],
    glow: &mut [f32; 4],
    offset: &mut [f32; 3],
    scale: &mut [f32; 3],
    rot_deg: &mut [f32; 3],
) {
    if matches!(effect.mode, deadlib_present::anim::EffectMode::Spin) {
        let units = deadlib_present::anim::effect_clock_units(effect, effect_time, effect_beat);
        rot_deg[0] = (rot_deg[0] + effect.magnitude[0] * units).rem_euclid(360.0);
        rot_deg[1] = (rot_deg[1] + effect.magnitude[1] * units).rem_euclid(360.0);
        rot_deg[2] = (rot_deg[2] + effect.magnitude[2] * units).rem_euclid(360.0);
    }
    if let Some(percent) = deadlib_present::anim::effect_mix(effect, effect_time, effect_beat) {
        match effect.mode {
            deadlib_present::anim::EffectMode::DiffuseRamp => {
                for (idx, out) in tint.iter_mut().enumerate() {
                    let color =
                        song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], percent)
                            .clamp(0.0, 1.0);
                    *out = (*out * color).clamp(0.0, 1.0);
                }
            }
            deadlib_present::anim::EffectMode::DiffuseShift => {
                let between = deadlib_present::anim::glowshift_mix(percent);
                for (idx, out) in tint.iter_mut().enumerate() {
                    let color =
                        song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], between)
                            .clamp(0.0, 1.0);
                    *out = (*out * color).clamp(0.0, 1.0);
                }
            }
            deadlib_present::anim::EffectMode::GlowShift => {
                let between = deadlib_present::anim::glowshift_mix(percent);
                for (idx, out) in glow.iter_mut().enumerate() {
                    *out = song_lua_effect_lerp(effect.color2[idx], effect.color1[idx], between)
                        .clamp(0.0, 1.0);
                }
            }
            deadlib_present::anim::EffectMode::Pulse => {
                let pulse = (percent * std::f32::consts::PI).sin().clamp(0.0, 1.0);
                let zoom =
                    song_lua_effect_lerp(effect.magnitude[0], effect.magnitude[1], pulse).max(0.0);
                scale[0] *= zoom * song_lua_effect_lerp(effect.color1[0], effect.color2[0], pulse);
                scale[1] *= zoom * song_lua_effect_lerp(effect.color1[1], effect.color2[1], pulse);
                scale[2] *= zoom * song_lua_effect_lerp(effect.color1[2], effect.color2[2], pulse);
            }
            deadlib_present::anim::EffectMode::Bob => {
                let bob = (percent * 2.0 * std::f32::consts::PI).sin();
                for i in 0..3 {
                    offset[i] += effect.magnitude[i] * bob;
                }
            }
            deadlib_present::anim::EffectMode::Bounce => {
                let bounce = (percent * std::f32::consts::PI).sin();
                for i in 0..3 {
                    offset[i] += effect.magnitude[i] * bounce;
                }
            }
            deadlib_present::anim::EffectMode::Wag => {
                let wag = (percent * 2.0 * std::f32::consts::PI).sin();
                for i in 0..3 {
                    rot_deg[i] += effect.magnitude[i] * wag;
                }
            }
            deadlib_present::anim::EffectMode::Spin | deadlib_present::anim::EffectMode::None => {}
        }
    }
    if rainbow {
        let color = song_lua_rainbow_color(effect_time, effect.period, effect.offset);
        tint[0] *= color[0];
        tint[1] *= color[1];
        tint[2] *= color[2];
    }
    offset[0] = offset[0].max(-1_000_000.0).min(1_000_000.0);
    offset[1] = offset[1].max(-1_000_000.0).min(1_000_000.0);
    offset[2] = offset[2].max(-1_000_000.0).min(1_000_000.0);
    tint[0] = tint[0].clamp(0.0, 1.0);
    tint[1] = tint[1].clamp(0.0, 1.0);
    tint[2] = tint[2].clamp(0.0, 1.0);
    tint[3] = tint[3].clamp(0.0, 1.0);
    glow[0] = glow[0].clamp(0.0, 1.0);
    glow[1] = glow[1].clamp(0.0, 1.0);
    glow[2] = glow[2].clamp(0.0, 1.0);
    glow[3] = glow[3].clamp(0.0, 1.0);
    scale[0] = scale[0].max(0.0);
    scale[1] = scale[1].max(0.0);
    scale[2] = scale[2].max(0.0);
}

fn song_lua_rainbow_color(time: f32, period: f32, offset: f32) -> [f32; 3] {
    let hue = ((time + offset) / period.max(f32::EPSILON)).rem_euclid(1.0);
    let h = hue * 6.0;
    let x = 1.0 - (h.rem_euclid(2.0) - 1.0).abs();
    if h < 1.0 {
        [1.0, x, 0.0]
    } else if h < 2.0 {
        [x, 1.0, 0.0]
    } else if h < 3.0 {
        [0.0, 1.0, x]
    } else if h < 4.0 {
        [0.0, x, 1.0]
    } else if h < 5.0 {
        [x, 0.0, 1.0]
    } else {
        [1.0, 0.0, x]
    }
}

const SONG_LUA_TEXT_RAINBOW_COLORS: [[f32; 4]; 7] = [
    [1.0, 0.0, 0.4, 1.0],
    [0.8, 0.2, 0.6, 1.0],
    [0.4, 0.3, 0.5, 1.0],
    [0.2, 0.6, 1.0, 1.0],
    [0.2, 0.8, 0.8, 1.0],
    [0.2, 0.8, 0.4, 1.0],
    [1.0, 0.8, 0.2, 1.0],
];

fn song_lua_rainbow_scroll_attributes(text: &str, total_elapsed: f32) -> Vec<TextAttribute> {
    let char_count = text.chars().count();
    let mut out = Vec::with_capacity(char_count);
    if char_count == 0 {
        return out;
    }
    let first_color = ((total_elapsed / 0.2).floor() as usize) % SONG_LUA_TEXT_RAINBOW_COLORS.len();
    for index in 0..char_count {
        out.push(TextAttribute {
            start: index,
            length: 1,
            color: SONG_LUA_TEXT_RAINBOW_COLORS
                [(first_color + index) % SONG_LUA_TEXT_RAINBOW_COLORS.len()],
            vertex_colors: None,
            glow: None,
        });
    }
    out
}

fn song_lua_transparent_text_attributes(text: &str) -> Vec<TextAttribute> {
    let char_count = text.chars().count();
    if char_count == 0 {
        return Vec::new();
    }
    vec![TextAttribute {
        start: 0,
        length: char_count,
        color: [1.0, 1.0, 1.0, 0.0],
        vertex_colors: None,
        glow: None,
    }]
}

fn song_lua_text_attributes_have_glow(attributes: &[TextAttribute]) -> bool {
    attributes
        .iter()
        .any(|attr| attr.glow.is_some_and(|glow| glow[3] > f32::EPSILON))
}

fn song_lua_text_glow_attributes(
    text: &str,
    attributes: &[TextAttribute],
    glow: [f32; 4],
) -> Vec<TextAttribute> {
    let char_count = text.chars().count();
    if char_count == 0 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(attributes.len() + usize::from(glow[3] > f32::EPSILON));
    if glow[3] > f32::EPSILON {
        out.push(TextAttribute {
            start: 0,
            length: char_count,
            color: glow,
            vertex_colors: None,
            glow: None,
        });
    }
    for attr in attributes {
        let Some(glow) = attr.glow else {
            continue;
        };
        if glow[3] <= f32::EPSILON {
            continue;
        }
        out.push(TextAttribute {
            start: attr.start,
            length: attr.length,
            color: glow,
            vertex_colors: None,
            glow: None,
        });
    }
    out
}

fn song_lua_text_attributes_for_diffuse_mode(
    attributes: &[TextAttribute],
    color: [f32; 4],
    text: &str,
    mult_attrs_with_diffuse: bool,
) -> (Vec<TextAttribute>, [f32; 4]) {
    if attributes.is_empty() || mult_attrs_with_diffuse {
        return (attributes.to_vec(), color);
    }
    let char_count = text.chars().count();
    if char_count == 0 {
        return (attributes.to_vec(), color);
    }
    if color
        .iter()
        .all(|component| (*component - 1.0).abs() <= f32::EPSILON)
    {
        return (attributes.to_vec(), [1.0, 1.0, 1.0, 1.0]);
    }
    let mut out = Vec::with_capacity(attributes.len() + 1);
    out.push(TextAttribute {
        start: 0,
        length: char_count,
        color,
        vertex_colors: None,
        glow: None,
    });
    out.extend_from_slice(attributes);
    (out, [1.0, 1.0, 1.0, 1.0])
}

fn song_lua_overlay_camera_state(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    mut index: Option<usize>,
) -> Option<SongLuaOverlayState> {
    while let Some(current) = index {
        let overlay = overlays.get(current)?;
        let state = overlay_states.get(current).copied()?;
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
        ) && state.fov.is_some()
        {
            return Some(state);
        }
        index = overlay.parent_index;
    }
    None
}

fn song_lua_overlay_view_proj(
    camera_state: SongLuaOverlayState,
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Matrix4> {
    let mut fov_deg = camera_state.fov?;
    if !fov_deg.is_finite() || fov_deg <= f32::EPSILON {
        return None;
    }
    fov_deg = fov_deg.clamp(0.1, 179.9);
    let width = screen_width().max(1.0);
    let height = screen_height().max(1.0);
    let x_scale = width / overlay_space_width.max(1.0);
    let y_scale = height / overlay_space_height.max(1.0);
    let vanish = camera_state
        .vanishpoint
        .unwrap_or([0.5 * overlay_space_width, 0.5 * overlay_space_height]);
    let mut vanish_x = width - vanish[0] * x_scale;
    let mut vanish_y = height - vanish[1] * y_scale;
    vanish_x -= 0.5 * width;
    vanish_y -= 0.5 * height;

    let theta = 0.5 * fov_deg.to_radians();
    let dist = (0.5 * width / theta.tan()).max(1.0);
    let proj = Matrix4::frustum_rh_gl(
        (vanish_x - 0.5 * width) / dist,
        (vanish_x + 0.5 * width) / dist,
        (vanish_y + 0.5 * height) / dist,
        (vanish_y - 0.5 * height) / dist,
        1.0,
        dist + 1000.0,
    );
    let eye_x = -vanish_x + 0.5 * width;
    let eye_y = -vanish_y + 0.5 * height;
    let view = Matrix4::look_at_rh(
        Vector3::new(eye_x, eye_y, dist),
        Vector3::new(eye_x, eye_y, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
    );
    Some(proj * view)
}

fn song_lua_actor_multi_vertex_mesh(
    vertices: &Arc<[SongLuaOverlayMeshVertex]>,
    tint: [f32; 4],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) -> Arc<[MeshVertex]> {
    let mut out = Vec::with_capacity(vertices.len());
    for vertex in vertices.iter() {
        out.push(MeshVertex {
            pos: song_lua_actor_multi_vertex_pos(
                vertex.pos,
                x_scale,
                y_scale,
                actor_scale,
                effect_scale,
                rotation_z_deg,
                skew,
            ),
            color: song_lua_capture_tint(vertex.color, tint),
        });
    }
    Arc::from(out.into_boxed_slice())
}

fn song_lua_actor_multi_vertex_textured_mesh(
    vertices: &Arc<[SongLuaOverlayMeshVertex]>,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) -> Arc<[TexturedMeshVertex]> {
    let mut out = Vec::with_capacity(vertices.len());
    for vertex in vertices.iter() {
        let pos = song_lua_actor_multi_vertex_pos(
            vertex.pos,
            x_scale,
            y_scale,
            actor_scale,
            effect_scale,
            rotation_z_deg,
            skew,
        );
        out.push(TexturedMeshVertex {
            pos: [pos[0], pos[1], 0.0],
            uv: vertex.uv,
            tex_matrix_scale: [1.0, 1.0],
            color: vertex.color,
        });
    }
    Arc::from(out.into_boxed_slice())
}

fn song_lua_actor_multi_vertex_pos(
    pos: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) -> [f32; 2] {
    let scale = [
        x_scale * actor_scale[0] * effect_scale[0],
        y_scale * actor_scale[1] * effect_scale[1],
    ];
    let (sin_z, cos_z) = rotation_z_deg.to_radians().sin_cos();
    let mut x = pos[0] * scale[0];
    let mut y = -pos[1] * scale[1];
    if skew[0].abs() > f32::EPSILON {
        x += skew[0] * y;
    }
    if skew[1].abs() > f32::EPSILON {
        y += skew[1] * x;
    }
    [x * cos_z - y * sin_z, x * sin_z + y * cos_z]
}

fn song_lua_model_actor(
    layers: &[SongLuaOverlayModelLayer],
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    effect_offset: [f32; 3],
    tint: [f32; 4],
    glow: [f32; 4],
    blend: BlendMode,
    total_elapsed: f32,
) -> Option<Actor> {
    let mut children = Vec::with_capacity(layers.len());
    let offset = [
        state.x * x_scale + effect_offset[0] * x_scale,
        state.y * y_scale + effect_offset[1] * y_scale,
    ];
    for (idx, layer) in layers.iter().enumerate() {
        if !layer.draw.visible || !asset_manager.has_texture_key(layer.texture_key.as_ref()) {
            continue;
        }
        let scroll = song_lua_model_layer_scroll(layer, total_elapsed);
        let shift = match state.texcoord_offset {
            Some([dx, dy]) => [scroll[0] + dx, scroll[1] + dy],
            None => scroll,
        };
        let uv_offset = [layer.uv_offset[0] + shift[0], layer.uv_offset[1] + shift[1]];
        let uv_tex_shift = [
            layer.uv_tex_shift[0] + shift[0],
            layer.uv_tex_shift[1] + shift[1],
        ];
        let actor = Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset,
            world_z: song_lua_biased_world_z(state, effect_offset[2]),
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: song_lua_model_local_transform(
                layer.model_size,
                layer.draw,
                x_scale,
                y_scale,
                actor_scale,
                effect_scale,
                effect_rot,
                [state.skew_x, state.skew_y],
            ),
            texture: Arc::clone(&layer.texture_key),
            tint: song_lua_capture_tint(layer.draw.tint, tint),
            glow: [1.0, 1.0, 1.0, 0.0],
            vertices: Arc::clone(&layer.vertices),
            geom_cache_key: INVALID_TMESH_CACHE_KEY,
            uv_scale: layer.uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test: state.depth_test,
            visible: true,
            blend: if layer.draw.blend_add {
                BlendMode::Add
            } else {
                blend
            },
            z: song_lua_add_z(z, idx.min(i16::MAX as usize) as i16),
        };
        let glow_actor = song_lua_overlay_glow_actor(&actor, glow, state.text_glow_mode);
        children.push(actor);
        if let Some(glow_actor) = glow_actor {
            children.push(glow_actor);
        }
    }
    if children.is_empty() {
        None
    } else {
        Some(Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        })
    }
}

fn song_lua_model_layer_scroll(layer: &SongLuaOverlayModelLayer, total_elapsed: f32) -> [f32; 2] {
    if layer.uv_velocity == [0.0, 0.0] {
        return [0.0, 0.0];
    }
    let clock = layer
        .uv_cycle_seconds
        .filter(|total| *total > f32::EPSILON && total.is_finite())
        .map_or(total_elapsed, |total| {
            total_elapsed.rem_euclid(total) / total
        });
    [layer.uv_velocity[0] * clock, layer.uv_velocity[1] * clock]
}

fn song_lua_noteskin_actor(
    slots: &[SpriteSlot],
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    effect_offset: [f32; 3],
    tint: [f32; 4],
    glow: [f32; 4],
    blend: BlendMode,
    total_elapsed: f32,
    effect_beat: f32,
) -> Option<Actor> {
    let mut children = Vec::with_capacity(slots.len() * 2);
    let center = [
        state.x * x_scale + effect_offset[0] * x_scale,
        state.y * y_scale + effect_offset[1] * y_scale,
    ];
    for (idx, slot) in slots.iter().enumerate() {
        if !asset_manager.has_texture_key(slot.texture_key()) {
            continue;
        }
        let mut draw = slot.model_draw_at(total_elapsed, effect_beat);
        draw.pos[0] *= x_scale * actor_scale[0] * effect_scale[0];
        draw.pos[1] *= y_scale * actor_scale[1] * effect_scale[1];
        draw.pos[2] *= actor_scale[1].abs() * effect_scale[2];
        draw.rot[0] += effect_rot[0];
        draw.rot[1] += effect_rot[1];
        let frame = slot.frame_index(total_elapsed, effect_beat);
        let uv = song_lua_noteskin_slot_uv(slot, frame, total_elapsed, state.texcoord_offset);
        let base_size = song_lua_noteskin_slot_size(slot);
        let size = [
            base_size[0] * x_scale * actor_scale[0] * effect_scale[0],
            base_size[1] * y_scale * actor_scale[1] * effect_scale[1],
        ];
        if size[0].abs() <= f32::EPSILON || size[1].abs() <= f32::EPSILON {
            continue;
        }
        let layer_z = song_lua_add_z(z, idx.min(i16::MAX as usize) as i16);
        let actor = if slot.model.is_some() {
            noteskin_model_actor_from_draw(
                slot,
                draw,
                center,
                size,
                uv,
                -(slot.def.rotation_deg as f32 + effect_rot[2]),
                tint,
                blend,
                layer_z,
            )
        } else {
            song_lua_noteskin_sprite_actor(
                slot,
                draw,
                center,
                size,
                uv,
                effect_rot[2],
                tint,
                blend,
                layer_z,
            )
        };
        let Some(actor) = actor else {
            continue;
        };
        let glow_actor = song_lua_overlay_glow_actor(&actor, glow, state.text_glow_mode);
        children.push(actor);
        if let Some(glow_actor) = glow_actor {
            children.push(glow_actor);
        }
    }
    if children.is_empty() {
        None
    } else {
        Some(Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        })
    }
}

fn song_lua_noteskin_slot_uv(
    slot: &SpriteSlot,
    frame: usize,
    total_elapsed: f32,
    texcoord_offset: Option<[f32; 2]>,
) -> [f32; 4] {
    let mut uv = slot.uv_for_frame_at(frame, total_elapsed);
    if let Some([dx, dy]) = texcoord_offset {
        uv[0] += dx;
        uv[1] += dy;
        uv[2] += dx;
        uv[3] += dy;
    }
    uv
}

fn song_lua_noteskin_slot_size(slot: &SpriteSlot) -> [f32; 2] {
    if let Some(model) = slot.model.as_ref() {
        let size = model.size();
        if size[0] > f32::EPSILON && size[1] > f32::EPSILON {
            return size;
        }
    }
    slot.logical_size()
}

fn song_lua_noteskin_sprite_actor(
    slot: &SpriteSlot,
    draw: ModelDrawState,
    center: [f32; 2],
    size: [f32; 2],
    uv: [f32; 4],
    rotation_z: f32,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
) -> Option<Actor> {
    if !draw.visible {
        return None;
    }
    let size = [
        size[0] * draw.zoom[0].max(0.0),
        size[1] * draw.zoom[1].max(0.0),
    ];
    if size[0].abs() <= f32::EPSILON || size[1].abs() <= f32::EPSILON {
        return None;
    }
    Some(Actor::Sprite {
        align: [0.5, 0.5],
        offset: [center[0] + draw.pos[0], center[1] - draw.pos[1]],
        world_z: 0.0,
        size: [SizeSpec::Px(size[0]), SizeSpec::Px(size[1])],
        source: SpriteSource::Texture(slot.texture_key_shared()),
        tint: [
            tint[0] * draw.tint[0],
            tint[1] * draw.tint[1],
            tint[2] * draw.tint[2],
            tint[3] * draw.tint[3],
        ],
        glow: [1.0, 1.0, 1.0, 0.0],
        z,
        cell: None,
        grid: None,
        uv_rect: Some(uv),
        visible: true,
        flip_x: false,
        flip_y: false,
        cropleft: 0.0,
        cropright: 0.0,
        croptop: 0.0,
        cropbottom: 0.0,
        fadeleft: 0.0,
        faderight: 0.0,
        fadetop: 0.0,
        fadebottom: 0.0,
        blend: if draw.blend_add {
            BlendMode::Add
        } else {
            blend
        },
        mask_source: false,
        mask_dest: false,
        rot_x_deg: draw.rot[0],
        rot_y_deg: draw.rot[1],
        rot_z_deg: draw.rot[2] - slot.def.rotation_deg as f32 - rotation_z,
        local_offset: [0.0, 0.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        texcoordvelocity: None,
        animate: false,
        state_delay: 0.1,
        scale: [1.0, 1.0],
        shadow_len: [0.0, 0.0],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
        effect: EffectState::default(),
    })
}

fn song_lua_model_local_transform(
    model_size: [f32; 2],
    draw: SongLuaOverlayModelDraw,
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    effect_rot: [f32; 3],
    skew: [f32; 2],
) -> Matrix4 {
    let align_y = (0.5 - draw.vert_align) * model_size[1];
    let scale = Vector3::new(
        x_scale * actor_scale[0] * effect_scale[0] * draw.zoom[0],
        y_scale * actor_scale[1] * effect_scale[1] * draw.zoom[1],
        actor_scale[1].abs() * effect_scale[2] * draw.zoom[2],
    );
    Matrix4::from_translation(Vector3::new(
        draw.pos[0] * x_scale,
        -draw.pos[1] * y_scale,
        draw.pos[2],
    )) * song_lua_overlay_local_transform(
        [
            draw.rot[0] + effect_rot[0],
            draw.rot[1] + effect_rot[1],
            draw.rot[2] + effect_rot[2],
        ],
        skew[0],
        skew[1],
    ) * Matrix4::from_translation(Vector3::new(0.0, align_y, 0.0))
        * Matrix4::from_scale(scale)
        * Matrix4::from_scale(Vector3::new(1.0, -1.0, 1.0))
}

fn song_lua_song_meter_actor(
    state: SongLuaOverlayState,
    stream_state: SongLuaOverlayState,
    stream_width: f32,
    music_length_seconds: f32,
    x_scale: f32,
    y_scale: f32,
    z: i16,
    total_elapsed: f32,
) -> Option<Actor> {
    let progress = if music_length_seconds > f32::EPSILON {
        (total_elapsed / music_length_seconds).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let parent_scale = song_lua_overlay_axis_scale(state);
    let stream_scale = song_lua_overlay_axis_scale(stream_state);
    let full_width = stream_width * parent_scale[0].abs() * stream_scale[0].abs();
    let progress_width = full_width * progress;
    if progress_width <= f32::EPSILON {
        return None;
    }
    let stream_height = stream_state.size.map_or(1.0, |size| size[1].abs())
        * parent_scale[1].abs()
        * stream_scale[1].abs();
    let left = state.x + stream_state.x * parent_scale[0] - full_width * 0.5;
    let y = state.y + stream_state.y * parent_scale[1];
    let tint = [
        state.diffuse[0] * stream_state.diffuse[0],
        state.diffuse[1] * stream_state.diffuse[1],
        state.diffuse[2] * stream_state.diffuse[2],
        state.diffuse[3] * stream_state.diffuse[3],
    ];
    let mut actor = act!(quad:
        align(0.0, stream_state.valign):
        xy(left * x_scale, y * y_scale):
        zoomto(progress_width * x_scale, stream_height * y_scale):
        diffuse(tint[0], tint[1], tint[2], tint[3]):
        z(z)
    );
    if let Actor::Sprite {
        visible,
        blend,
        mask_source,
        mask_dest,
        ..
    } = &mut actor
    {
        *visible = state.visible && stream_state.visible;
        *blend = if stream_state.blend == SongLuaOverlayBlendMode::Alpha {
            song_lua_overlay_blend(state.blend)
        } else {
            song_lua_overlay_blend(stream_state.blend)
        };
        *mask_source = state.mask_source || stream_state.mask_source;
        *mask_dest = state.mask_dest || stream_state.mask_dest;
    }
    Some(actor)
}

#[inline(always)]
fn song_meter_progress(current_seconds: f32, first_second: f32, last_second: f32) -> f32 {
    if !current_seconds.is_finite() || !first_second.is_finite() || !last_second.is_finite() {
        return 0.0;
    }
    let duration = last_second - first_second;
    if duration <= f32::EPSILON {
        return 0.0;
    }
    ((current_seconds - first_second) / duration).clamp(0.0, 1.0)
}

fn song_lua_graph_display_actor(
    state: SongLuaOverlayState,
    body_values: &Arc<[f32]>,
    body_state: SongLuaOverlayState,
    line_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
) -> Option<Actor> {
    let mut children = Vec::with_capacity(2);
    if let Some(body) =
        song_lua_graph_display_body_actor(state, body_values, body_state, size, x_scale, y_scale, z)
    {
        children.push(body);
    }
    if let Some(line) =
        song_lua_graph_display_line_actor(state, body_values, line_state, size, x_scale, y_scale, z)
    {
        children.push(line);
    }
    match children.len() {
        0 => None,
        1 => children.pop(),
        _ => Some(Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        }),
    }
}

fn song_lua_graph_display_body_actor(
    state: SongLuaOverlayState,
    body_values: &[f32],
    body_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
) -> Option<Actor> {
    if !body_state.visible || body_state.diffuse[3] <= f32::EPSILON {
        return None;
    }
    let values = graph_display_values_or_default(body_values);
    let graph_scale = song_lua_overlay_axis_scale(state);
    let body_scale = song_lua_overlay_axis_scale(body_state);
    let width = size[0] * graph_scale[0].abs() * body_scale[0].abs();
    let height = size[1] * graph_scale[1].abs() * body_scale[1].abs();
    if width <= f32::EPSILON || height <= f32::EPSILON {
        return None;
    }
    let left = state.x - width * state.halign + body_state.x * graph_scale[0];
    let top =
        state.y - size[1] * graph_scale[1].abs() * state.valign + body_state.y * graph_scale[1];
    let tint = [
        state.diffuse[0] * body_state.diffuse[0],
        state.diffuse[1] * body_state.diffuse[1],
        state.diffuse[2] * body_state.diffuse[2],
        state.diffuse[3] * body_state.diffuse[3],
    ];
    let bottom = top + height;
    let mut vertices = Vec::with_capacity((values.len().saturating_sub(1)) * 6);
    for (index, pair) in values.windows(2).enumerate() {
        let x0 = left + width * index as f32 / (values.len() - 1) as f32;
        let x1 = left + width * (index + 1) as f32 / (values.len() - 1) as f32;
        let y0 = top + (1.0 - pair[0].clamp(0.0, 1.0)) * height;
        let y1 = top + (1.0 - pair[1].clamp(0.0, 1.0)) * height;
        push_graph_display_tri(
            &mut vertices,
            [x0 * x_scale, y0 * y_scale],
            [x0 * x_scale, bottom * y_scale],
            [x1 * x_scale, bottom * y_scale],
            tint,
        );
        push_graph_display_tri(
            &mut vertices,
            [x0 * x_scale, y0 * y_scale],
            [x1 * x_scale, bottom * y_scale],
            [x1 * x_scale, y1 * y_scale],
            tint,
        );
    }
    Some(Actor::Mesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        vertices: Arc::from(vertices.into_boxed_slice()),
        visible: state.visible && body_state.visible,
        blend: if body_state.blend == SongLuaOverlayBlendMode::Alpha {
            song_lua_overlay_blend(state.blend)
        } else {
            song_lua_overlay_blend(body_state.blend)
        },
        z,
    })
}

fn song_lua_graph_display_line_actor(
    state: SongLuaOverlayState,
    body_values: &[f32],
    line_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
) -> Option<Actor> {
    if !line_state.visible || line_state.diffuse[3] <= f32::EPSILON {
        return None;
    }
    let values = graph_display_values_or_default(body_values);
    let graph_scale = song_lua_overlay_axis_scale(state);
    let line_scale = song_lua_overlay_axis_scale(line_state);
    let width = size[0] * graph_scale[0].abs() * line_scale[0].abs();
    if width <= f32::EPSILON {
        return None;
    }
    let line_height = line_state.size.map_or(1.0, |line_size| line_size[1].abs())
        * graph_scale[1].abs()
        * line_scale[1].abs();
    let left = state.x - width * state.halign + line_state.x * graph_scale[0];
    let top = state.y - size[1] * graph_scale[1].abs() * state.valign;
    let y = top + size[1] * graph_scale[1].abs() * 0.5 + line_state.y * graph_scale[1];
    let tint = [
        state.diffuse[0] * line_state.diffuse[0],
        state.diffuse[1] * line_state.diffuse[1],
        state.diffuse[2] * line_state.diffuse[2],
        state.diffuse[3] * line_state.diffuse[3],
    ];
    let mut vertices = Vec::with_capacity((values.len().saturating_sub(1)) * 6);
    let stroke = line_height.max(1.0);
    for (index, pair) in values.windows(2).enumerate() {
        let x0 = left + width * index as f32 / (values.len() - 1) as f32;
        let x1 = left + width * (index + 1) as f32 / (values.len() - 1) as f32;
        let y0 = y + (0.5 - pair[0].clamp(0.0, 1.0)) * size[1] * graph_scale[1].abs();
        let y1 = y + (0.5 - pair[1].clamp(0.0, 1.0)) * size[1] * graph_scale[1].abs();
        push_graph_display_line_segment(
            &mut vertices,
            [x0 * x_scale, y0 * y_scale],
            [x1 * x_scale, y1 * y_scale],
            stroke * y_scale,
            tint,
        );
    }
    Some(Actor::Mesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        vertices: Arc::from(vertices.into_boxed_slice()),
        visible: state.visible && line_state.visible,
        blend: if line_state.blend == SongLuaOverlayBlendMode::Alpha {
            song_lua_overlay_blend(state.blend)
        } else {
            song_lua_overlay_blend(line_state.blend)
        },
        z,
    })
}

fn graph_display_values_or_default(values: &[f32]) -> &[f32] {
    static DEFAULT: [f32; 2] = [0.5, 0.5];
    if values.len() >= 2 { values } else { &DEFAULT }
}

fn push_graph_display_line_segment(
    out: &mut Vec<MeshVertex>,
    start: [f32; 2],
    end: [f32; 2],
    stroke: f32,
    color: [f32; 4],
) {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return;
    }
    let half = stroke * 0.5;
    let nx = -dy / len * half;
    let ny = dx / len * half;
    let a = [start[0] + nx, start[1] + ny];
    let b = [start[0] - nx, start[1] - ny];
    let c = [end[0] - nx, end[1] - ny];
    let d = [end[0] + nx, end[1] + ny];
    push_graph_display_tri(out, a, b, c, color);
    push_graph_display_tri(out, a, c, d, color);
}

fn push_graph_display_tri(
    out: &mut Vec<MeshVertex>,
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
    color: [f32; 4],
) {
    out.push(MeshVertex { pos: a, color });
    out.push(MeshVertex { pos: b, color });
    out.push(MeshVertex { pos: c, color });
}

fn song_lua_project_overlay_point(view_proj: Matrix4, point: [f32; 3]) -> Option<[f32; 2]> {
    let clip = view_proj * Vector4::new(point[0], point[1], point[2], 1.0);
    if !clip.w.is_finite() || clip.w <= f32::EPSILON {
        return None;
    }
    let inv_w = clip.w.recip();
    let ndc_x = clip.x * inv_w;
    let ndc_y = clip.y * inv_w;
    if !(ndc_x.is_finite() && ndc_y.is_finite()) {
        return None;
    }
    Some([
        (0.5 * ndc_x + 0.5) * screen_width(),
        (0.5 - 0.5 * ndc_y) * screen_height(),
    ])
}

fn song_lua_overlay_rect(
    state: SongLuaOverlayState,
    default_size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    size_scale_x: f32,
    size_scale_y: f32,
) -> Option<([f32; 2], [f32; 2])> {
    let (base_center, base_size) = if let Some([left, top, right, bottom]) = state.stretch_rect {
        (
            [
                0.5 * (left + right) * x_scale,
                0.5 * (top + bottom) * y_scale,
            ],
            [
                (right - left).abs() * x_scale * size_scale_x,
                (bottom - top).abs() * y_scale * size_scale_y,
            ],
        )
    } else {
        (
            [
                (0.5 - state.halign)
                    .mul_add(default_size[0] * x_scale * size_scale_x, state.x * x_scale),
                (0.5 - state.valign)
                    .mul_add(default_size[1] * y_scale * size_scale_y, state.y * y_scale),
            ],
            [
                default_size[0] * x_scale * size_scale_x,
                default_size[1] * y_scale * size_scale_y,
            ],
        )
    };
    if base_size[0] <= f32::EPSILON || base_size[1] <= f32::EPSILON {
        return None;
    }
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= f32::EPSILON || sy_crop <= f32::EPSILON {
        return None;
    }
    Some((
        [
            ((cl - cr) * base_size[0]).mul_add(0.5, base_center[0]),
            ((cb - ct) * base_size[1]).mul_add(0.5, base_center[1]),
        ],
        [base_size[0] * sx_crop, base_size[1] * sy_crop],
    ))
}

fn song_lua_overlay_uvs(
    state: SongLuaOverlayState,
    texture_key: Option<&str>,
    flip_x: bool,
    flip_y: bool,
    total_elapsed: f32,
) -> [[f32; 2]; 4] {
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let [
        mut uv_scale_x,
        mut uv_scale_y,
        mut uv_offset_x,
        mut uv_offset_y,
    ] = if let Some([u0, v0, u1, v1]) = song_lua_overlay_uv_rect(state, texture_key, total_elapsed)
    {
        [
            (u1 - u0).abs().max(1e-6),
            (v1 - v0).abs().max(1e-6),
            u0.min(u1),
            v0.min(v1),
        ]
    } else {
        [1.0, 1.0, 0.0, 0.0]
    };
    uv_offset_x += uv_scale_x * cl;
    uv_offset_y += uv_scale_y * ct;
    uv_scale_x *= (1.0 - cl - cr).max(0.0);
    uv_scale_y *= (1.0 - ct - cb).max(0.0);
    if flip_x {
        uv_offset_x += uv_scale_x;
        uv_scale_x = -uv_scale_x;
    }
    if flip_y {
        uv_offset_y += uv_scale_y;
        uv_scale_y = -uv_scale_y;
    }
    if let Some(velocity) = state.texcoord_velocity {
        uv_offset_x += velocity[0] * total_elapsed;
        uv_offset_y += velocity[1] * total_elapsed;
    }
    [
        [uv_offset_x, uv_offset_y],
        [uv_offset_x + uv_scale_x, uv_offset_y],
        [uv_offset_x + uv_scale_x, uv_offset_y + uv_scale_y],
        [uv_offset_x, uv_offset_y + uv_scale_y],
    ]
}

#[inline(always)]
fn song_lua_projected_edge_factor(t: f32, feather_l: f32, feather_r: f32) -> f32 {
    let mut left = 1.0;
    let mut right = 1.0;
    if feather_l > f32::EPSILON {
        left = ((t - 0.0) / feather_l).clamp(0.0, 1.0);
    }
    if feather_r > f32::EPSILON {
        right = ((1.0 - t) / feather_r).clamp(0.0, 1.0);
    }
    left.min(right)
}

#[inline(always)]
fn song_lua_projected_overlay_edge_fade(
    state: SongLuaOverlayState,
    flip_x: bool,
    flip_y: bool,
) -> [f32; 4] {
    let cl = state.cropleft.clamp(0.0, 1.0);
    let cr = state.cropright.clamp(0.0, 1.0);
    let ct = state.croptop.clamp(0.0, 1.0);
    let cb = state.cropbottom.clamp(0.0, 1.0);
    let sx_crop = (1.0 - cl - cr).max(0.0);
    let sy_crop = (1.0 - ct - cb).max(0.0);
    if sx_crop <= f32::EPSILON || sy_crop <= f32::EPSILON {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let fl = state.fadeleft.clamp(0.0, 1.0);
    let fr = state.faderight.clamp(0.0, 1.0);
    let ft = state.fadetop.clamp(0.0, 1.0);
    let fb = state.fadebottom.clamp(0.0, 1.0);

    let mut fl_size = (fl + state.cropleft.min(0.0)).max(0.0);
    let mut fr_size = (fr + state.cropright.min(0.0)).max(0.0);
    let mut ft_size = (ft + state.croptop.min(0.0)).max(0.0);
    let mut fb_size = (fb + state.cropbottom.min(0.0)).max(0.0);

    let sum_x = fl_size + fr_size;
    if sum_x > 0.0 && sx_crop < sum_x {
        let scale = sx_crop / sum_x;
        fl_size *= scale;
        fr_size *= scale;
    }

    let sum_y = ft_size + fb_size;
    if sum_y > 0.0 && sy_crop < sum_y {
        let scale = sy_crop / sum_y;
        ft_size *= scale;
        fb_size *= scale;
    }

    let mut fl_eff = (fl_size / sx_crop).clamp(0.0, 1.0);
    let mut fr_eff = (fr_size / sx_crop).clamp(0.0, 1.0);
    let mut ft_eff = (ft_size / sy_crop).clamp(0.0, 1.0);
    let mut fb_eff = (fb_size / sy_crop).clamp(0.0, 1.0);

    if flip_x {
        std::mem::swap(&mut fl_eff, &mut fr_eff);
    }
    if flip_y {
        std::mem::swap(&mut ft_eff, &mut fb_eff);
    }

    [fl_eff, fr_eff, ft_eff, fb_eff]
}

fn song_lua_projected_overlay_axis_slices(start_fade: f32, end_fade: f32) -> Vec<f32> {
    let mut out = vec![0.0];
    for value in [start_fade, 1.0 - end_fade, 1.0] {
        let value = value.clamp(0.0, 1.0);
        if out
            .last()
            .is_none_or(|last| (value - *last).abs() > f32::EPSILON)
        {
            out.push(value);
        }
    }
    out
}

#[inline(always)]
fn song_lua_projected_overlay_uv_point(uv: [[f32; 2]; 4], x: f32, y: f32) -> [f32; 2] {
    let top_u = song_lua_effect_lerp(uv[0][0], uv[1][0], x);
    let top_v = song_lua_effect_lerp(uv[0][1], uv[1][1], x);
    let bottom_u = song_lua_effect_lerp(uv[3][0], uv[2][0], x);
    let bottom_v = song_lua_effect_lerp(uv[3][1], uv[2][1], x);
    [
        song_lua_effect_lerp(top_u, bottom_u, y),
        song_lua_effect_lerp(top_v, bottom_v, y),
    ]
}

fn song_lua_overlay_vertex_color(
    state: SongLuaOverlayState,
    x: f32,
    y: f32,
    flip_x: bool,
    flip_y: bool,
    alpha: f32,
) -> [f32; 4] {
    let Some(colors) = state.vertex_colors else {
        return [1.0, 1.0, 1.0, alpha];
    };
    let x = if flip_x { 1.0 - x } else { x }.clamp(0.0, 1.0);
    let y = if flip_y { 1.0 - y } else { y }.clamp(0.0, 1.0);
    let mut out = [0.0; 4];
    for channel in 0..4 {
        let top = song_lua_effect_lerp(colors[0][channel], colors[1][channel], x);
        let bottom = song_lua_effect_lerp(colors[3][channel], colors[2][channel], x);
        out[channel] = song_lua_effect_lerp(top, bottom, y);
    }
    out[3] *= alpha;
    out
}

#[inline(always)]
fn song_lua_overlay_fold_xy_rot(
    mut flip_x: bool,
    mut flip_y: bool,
    mut size_x: f32,
    mut size_y: f32,
    rot_x_deg: f32,
    rot_y_deg: f32,
) -> (bool, bool, f32, f32) {
    let cos_y = rot_y_deg.to_radians().cos();
    size_x *= cos_y.abs();
    if cos_y.is_sign_negative() {
        flip_x = !flip_x;
    }

    let cos_x = rot_x_deg.to_radians().cos();
    size_y *= cos_x.abs();
    if cos_x.is_sign_negative() {
        flip_y = !flip_y;
    }

    (flip_x, flip_y, size_x, size_y)
}

#[inline(always)]
fn song_lua_overlay_local_transform(rot_deg: [f32; 3], skew_x: f32, skew_y: f32) -> Matrix4 {
    Matrix4::from_rotation_x(rot_deg[0].to_radians())
        * Matrix4::from_rotation_y(rot_deg[1].to_radians())
        * Matrix4::from_rotation_z(rot_deg[2].to_radians())
        * song_lua_player_skew_x_matrix(skew_x)
        * song_lua_player_skew_y_matrix(skew_y)
}

fn song_lua_flat_skewed_overlay_actor(
    texture: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
    center: [f32; 2],
    size: [f32; 2],
    rot_deg: [f32; 3],
    uv: [[f32; 2]; 4],
    state: SongLuaOverlayState,
    flip_x: bool,
    flip_y: bool,
    world_z: f32,
) -> Option<Actor> {
    let (flip_x, flip_y, size_x, size_y) =
        song_lua_overlay_fold_xy_rot(flip_x, flip_y, size[0], size[1], rot_deg[0], rot_deg[1]);
    let half_w = 0.5 * size_x;
    let half_h = 0.5 * size_y;
    if half_w <= f32::EPSILON || half_h <= f32::EPSILON {
        return None;
    }
    let edge_fade = song_lua_projected_overlay_edge_fade(state, flip_x, flip_y);
    let xs = song_lua_projected_overlay_axis_slices(edge_fade[0], edge_fade[1]);
    let ys = song_lua_projected_overlay_axis_slices(edge_fade[2], edge_fade[3]);
    let transform = Matrix4::from_translation(Vector3::new(center[0], center[1], 0.0))
        * song_lua_overlay_local_transform(rot_deg, state.skew_x, state.skew_y);
    let mut grid = Vec::with_capacity(xs.len() * ys.len());
    for &y in &ys {
        for &x in &xs {
            let local_x = song_lua_effect_lerp(-half_w, half_w, x);
            let local_y = song_lua_effect_lerp(-half_h, half_h, y);
            let point = transform * Vector4::new(local_x, local_y, 0.0, 1.0);
            let fade_x = song_lua_projected_edge_factor(x, edge_fade[0], edge_fade[1]);
            let fade_y = song_lua_projected_edge_factor(y, edge_fade[2], edge_fade[3]);
            grid.push(TexturedMeshVertex {
                pos: [point.x, point.y, 0.0],
                uv: song_lua_projected_overlay_uv_point(uv, x, y),
                tex_matrix_scale: [1.0, 1.0],
                color: song_lua_overlay_vertex_color(
                    state,
                    x,
                    y,
                    flip_x,
                    flip_y,
                    fade_x.min(fade_y),
                ),
            });
        }
    }
    let width = xs.len();
    let mut vertices = Vec::with_capacity((xs.len() - 1) * (ys.len() - 1) * 6);
    for y in 0..ys.len().saturating_sub(1) {
        for x in 0..xs.len().saturating_sub(1) {
            let tl = y * width + x;
            let tr = tl + 1;
            let bl = (y + 1) * width + x;
            let br = bl + 1;
            vertices.push(grid[tl]);
            vertices.push(grid[tr]);
            vertices.push(grid[br]);
            vertices.push(grid[tl]);
            vertices.push(grid[br]);
            vertices.push(grid[bl]);
        }
    }
    Some(Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint,
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices: Arc::from(vertices.into_boxed_slice()),
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: state.depth_test,
        visible: state.visible,
        blend,
        z,
    })
}

fn song_lua_projected_overlay_actor(
    texture: Arc<str>,
    tint: [f32; 4],
    blend: BlendMode,
    z: i16,
    center: [f32; 3],
    size: [f32; 2],
    rot_deg: [f32; 3],
    uv: [[f32; 2]; 4],
    state: SongLuaOverlayState,
    flip_x: bool,
    flip_y: bool,
    view_proj: Matrix4,
) -> Option<Actor> {
    let half_w = 0.5 * size[0];
    let half_h = 0.5 * size[1];
    if half_w <= f32::EPSILON || half_h <= f32::EPSILON {
        return None;
    }
    let edge_fade = song_lua_projected_overlay_edge_fade(state, flip_x, flip_y);
    let xs = song_lua_projected_overlay_axis_slices(edge_fade[0], edge_fade[1]);
    let ys = song_lua_projected_overlay_axis_slices(edge_fade[2], edge_fade[3]);
    let model = Matrix4::from_translation(Vector3::new(center[0], center[1], center[2]))
        * song_lua_overlay_local_transform(rot_deg, state.skew_x, state.skew_y);
    let mut grid = Vec::with_capacity(xs.len() * ys.len());
    for &y in &ys {
        for &x in &xs {
            let local_x = song_lua_effect_lerp(-half_w, half_w, x);
            let local_y = song_lua_effect_lerp(-half_h, half_h, y);
            let world = model * Vector4::new(local_x, local_y, 0.0, 1.0);
            let screen = song_lua_project_overlay_point(view_proj, [world.x, world.y, world.z])?;
            let fade_x = song_lua_projected_edge_factor(x, edge_fade[0], edge_fade[1]);
            let fade_y = song_lua_projected_edge_factor(y, edge_fade[2], edge_fade[3]);
            grid.push(TexturedMeshVertex {
                pos: [screen[0], screen[1], 0.0],
                uv: song_lua_projected_overlay_uv_point(uv, x, y),
                tex_matrix_scale: [1.0, 1.0],
                color: song_lua_overlay_vertex_color(
                    state,
                    x,
                    y,
                    flip_x,
                    flip_y,
                    fade_x.min(fade_y),
                ),
            });
        }
    }
    let width = xs.len();
    let mut vertices = Vec::with_capacity((xs.len() - 1) * (ys.len() - 1) * 6);
    for y in 0..ys.len().saturating_sub(1) {
        for x in 0..xs.len().saturating_sub(1) {
            let tl = y * width + x;
            let tr = tl + 1;
            let bl = (y + 1) * width + x;
            let br = bl + 1;
            vertices.push(grid[tl]);
            vertices.push(grid[tr]);
            vertices.push(grid[br]);
            vertices.push(grid[tl]);
            vertices.push(grid[br]);
            vertices.push(grid[bl]);
        }
    }
    Some(Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: state.z_bias,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint,
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices: Arc::from(vertices.into_boxed_slice()),
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: state.depth_test,
        visible: true,
        blend,
        z,
    })
}

type SongLuaActorList = SmallVec<[Actor; 2]>;

#[inline(always)]
fn one_song_lua_actor(actor: Actor) -> SongLuaActorList {
    let mut out = SmallVec::new();
    out.push(actor);
    out
}

fn build_song_lua_overlay_actor(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    camera_state: Option<SongLuaOverlayState>,
    asset_manager: &AssetManager,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
) -> Option<SongLuaActorList> {
    if !state.visible || !song_lua_overlay_has_visible_output(state) {
        return None;
    }
    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let overlay_scale = song_lua_overlay_axis_scale(state);
    let (size_scale_x, flip_x) = if overlay_scale[0] < 0.0 {
        (-overlay_scale[0], true)
    } else {
        (overlay_scale[0], false)
    };
    let (size_scale_y, flip_y) = if overlay_scale[1] < 0.0 {
        (-overlay_scale[1], true)
    } else {
        (overlay_scale[1], false)
    };
    let effect = song_lua_overlay_effect_state(state);
    let overlay_blend = song_lua_overlay_blend(state.blend);
    let perspective_view_proj = camera_state.and_then(|camera| {
        song_lua_overlay_view_proj(camera, overlay_space_width, overlay_space_height)
    });
    let finalize_actor =
        |actor, glow| song_lua_finalize_overlay_actor(state, actor, glow, x_scale, y_scale);
    match &overlay.kind {
        SongLuaOverlayKind::Actor => None,
        SongLuaOverlayKind::ActorFrame => None,
        SongLuaOverlayKind::ActorFrameTexture => None,
        SongLuaOverlayKind::ActorProxy { .. } => None,
        SongLuaOverlayKind::AftSprite { .. } => None,
        SongLuaOverlayKind::Sound { .. } => None,
        SongLuaOverlayKind::Sprite { texture_key, .. } => {
            let key = texture_key.as_ref();
            if !asset_manager.has_texture_key(key) {
                return None;
            }
            if let Some(view_proj) = perspective_view_proj {
                let size = song_lua_overlay_sprite_size(state, key)?;
                let (center, size) = song_lua_overlay_rect(
                    state,
                    size,
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    effect_time,
                    effect_beat,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_projected_overlay_actor(
                    Arc::clone(texture_key),
                    tint,
                    overlay_blend,
                    z,
                    [
                        center[0] + effect_offset[0] * x_scale,
                        center[1] + effect_offset[1] * y_scale,
                        effect_offset[2],
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(state, Some(key), flip_x, flip_y, total_elapsed),
                    state,
                    flip_x,
                    flip_y,
                    view_proj,
                )?;
                return Some(finalize_actor(actor, glow));
            }
            if (state.skew_x.abs() > f32::EPSILON
                || state.skew_y.abs() > f32::EPSILON
                || state.vertex_colors.is_some())
                && !state.mask_source
                && !state.mask_dest
            {
                let size = song_lua_overlay_sprite_size(state, key)?;
                let (center, size) = song_lua_overlay_rect(
                    state,
                    size,
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    effect_time,
                    effect_beat,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_flat_skewed_overlay_actor(
                    Arc::clone(texture_key),
                    tint,
                    overlay_blend,
                    z,
                    [
                        center[0] + effect_offset[0] * x_scale,
                        center[1] + effect_offset[1] * y_scale,
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(state, Some(key), flip_x, flip_y, total_elapsed),
                    state,
                    flip_x,
                    flip_y,
                    effect_offset[2],
                )?;
                return Some(finalize_actor(actor, glow));
            }
            let mut actor = if let Some([left, top, right, bottom]) = state.stretch_rect {
                act!(sprite(Arc::clone(texture_key)):
                    align(0.0, 0.0):
                    xy(left * x_scale, top * y_scale):
                    setsize(
                        (right - left).abs() * x_scale * size_scale_x,
                        (bottom - top).abs() * y_scale * size_scale_y
                    ):
                    z(z)
                )
            } else {
                let size = song_lua_overlay_sprite_size(state, key)?;
                act!(sprite(Arc::clone(texture_key)):
                    align(state.halign, state.valign):
                    xy(state.x * x_scale, state.y * y_scale):
                    setsize(
                        size[0] * x_scale * size_scale_x,
                        size[1] * y_scale * size_scale_y
                    ):
                    z(z)
                )
            };
            if let Actor::Sprite {
                tint,
                glow,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                offset,
                world_z,
                scale,
                uv_rect,
                texcoordvelocity,
                effect: actor_effect,
                flip_x: actor_flip_x,
                flip_y: actor_flip_y,
                visible,
                ..
            } = &mut actor
            {
                let mut effect_tint = state.diffuse;
                let mut effect_glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    effect_time,
                    effect_beat,
                    &mut effect_tint,
                    &mut effect_glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut effect_rot,
                );
                *tint = effect_tint;
                *glow = effect_glow;
                *cropleft = state.cropleft.clamp(0.0, 1.0);
                *cropright = state.cropright.clamp(0.0, 1.0);
                *croptop = state.croptop.clamp(0.0, 1.0);
                *cropbottom = state.cropbottom.clamp(0.0, 1.0);
                *fadeleft = state.fadeleft.clamp(0.0, 1.0);
                *faderight = state.faderight.clamp(0.0, 1.0);
                *fadetop = state.fadetop.clamp(0.0, 1.0);
                *fadebottom = state.fadebottom.clamp(0.0, 1.0);
                *blend = overlay_blend;
                *mask_source = state.mask_source;
                *mask_dest = state.mask_dest;
                *rot_x_deg = effect_rot[0];
                *rot_y_deg = effect_rot[1];
                *rot_z_deg = effect_rot[2];
                offset[0] += effect_offset[0] * x_scale;
                offset[1] += effect_offset[1] * y_scale;
                *world_z += song_lua_biased_world_z(state, effect_offset[2]);
                scale[0] *= effect_scale[0];
                scale[1] *= effect_scale[1];
                *uv_rect = song_lua_overlay_uv_rect(state, Some(key), total_elapsed);
                *texcoordvelocity = state.texcoord_velocity;
                *actor_effect = EffectState::default();
                *actor_flip_x ^= flip_x;
                *actor_flip_y ^= flip_y;
                *visible = state.visible;
            }
            let glow = if let Actor::Sprite { glow, .. } = &actor {
                *glow
            } else {
                state.glow
            };
            Some(finalize_actor(actor, glow))
        }
        SongLuaOverlayKind::BitmapText {
            font_name,
            text,
            stroke_color,
            attributes,
            ..
        } => {
            let content = if state.uppercase {
                TextContent::from(text.to_uppercase())
            } else {
                TextContent::from(text)
            };
            let font = if asset_manager.with_font(*font_name, |_| ()).is_some() {
                *font_name
            } else {
                "miso"
            };
            let mut color = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                effect_time,
                effect_beat,
                &mut color,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            let (text_attributes, color) = if state.rainbow_scroll {
                (
                    song_lua_rainbow_scroll_attributes(content.as_str(), total_elapsed),
                    color,
                )
            } else {
                song_lua_text_attributes_for_diffuse_mode(
                    attributes,
                    color,
                    content.as_str(),
                    state.mult_attrs_with_diffuse,
                )
            };
            Some(finalize_actor(
                Actor::Text {
                    align: [state.halign, state.valign],
                    offset: [
                        state.x * x_scale + effect_offset[0] * x_scale,
                        state.y * y_scale + effect_offset[1] * y_scale,
                    ],
                    local_transform: song_lua_overlay_local_transform(
                        effect_rot,
                        state.skew_x,
                        state.skew_y,
                    ),
                    color,
                    stroke_color: *stroke_color,
                    glow,
                    font,
                    content,
                    attributes: text_attributes,
                    align_text: state.text_align,
                    z,
                    scale: [
                        size_scale_x * x_scale * effect_scale[0],
                        size_scale_y * y_scale * effect_scale[1],
                    ],
                    fit_width: state.size.map(|size| size[0] * x_scale),
                    fit_height: state.size.map(|size| size[1] * y_scale),
                    line_spacing: state
                        .vert_spacing
                        .map(|value| ((value as f32) * y_scale).round() as i32),
                    wrap_width_pixels: state
                        .wrap_width_pixels
                        .map(|value| ((value as f32) * x_scale).round() as i32),
                    max_width: state.max_width.map(|value| value * x_scale),
                    max_height: state.max_height.map(|value| value * y_scale),
                    max_w_pre_zoom: state.max_w_pre_zoom && !state.max_dimension_uses_zoom,
                    max_h_pre_zoom: state.max_h_pre_zoom && !state.max_dimension_uses_zoom,
                    jitter: state.text_jitter,
                    distortion: state.text_distortion,
                    clip: None,
                    mask_dest: state.mask_dest,
                    blend: overlay_blend,
                    shadow_len: [0.0, 0.0],
                    shadow_color: [0.0, 0.0, 0.0, 0.5],
                    effect: EffectState::default(),
                },
                glow,
            ))
        }
        SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_key,
            ..
        } => {
            let mut tint = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                effect_time,
                effect_beat,
                &mut tint,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            if let Some(texture_key) = texture_key {
                let key = texture_key.as_ref();
                if !asset_manager.has_texture_key(key) {
                    return None;
                }
                let mesh = song_lua_actor_multi_vertex_textured_mesh(
                    vertices,
                    x_scale,
                    y_scale,
                    [size_scale_x, size_scale_y],
                    effect_scale,
                    effect_rot[2],
                    [state.skew_x, state.skew_y],
                );
                return Some(finalize_actor(
                    Actor::TexturedMesh {
                        align: [0.0, 0.0],
                        offset: [
                            state.x * x_scale + effect_offset[0] * x_scale,
                            state.y * y_scale + effect_offset[1] * y_scale,
                        ],
                        world_z: song_lua_biased_world_z(state, effect_offset[2]),
                        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                        local_transform: Matrix4::IDENTITY,
                        texture: Arc::clone(texture_key),
                        tint,
                        glow: [1.0, 1.0, 1.0, 0.0],
                        vertices: mesh,
                        geom_cache_key: INVALID_TMESH_CACHE_KEY,
                        uv_scale: [1.0, 1.0],
                        uv_offset: [0.0, 0.0],
                        uv_tex_shift: [0.0, 0.0],
                        depth_test: state.depth_test,
                        visible: state.visible,
                        blend: overlay_blend,
                        z,
                    },
                    glow,
                ));
            }
            let mesh = song_lua_actor_multi_vertex_mesh(
                vertices,
                tint,
                x_scale,
                y_scale,
                [size_scale_x, size_scale_y],
                effect_scale,
                effect_rot[2],
                [state.skew_x, state.skew_y],
            );
            Some(finalize_actor(
                Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [
                        state.x * x_scale + effect_offset[0] * x_scale,
                        state.y * y_scale + effect_offset[1] * y_scale,
                    ],
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    vertices: mesh,
                    visible: state.visible,
                    blend: overlay_blend,
                    z,
                },
                glow,
            ))
        }
        SongLuaOverlayKind::Model { layers } => {
            let mut tint = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                effect_time,
                effect_beat,
                &mut tint,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            song_lua_model_actor(
                layers,
                state,
                asset_manager,
                z,
                x_scale,
                y_scale,
                [size_scale_x, size_scale_y],
                effect_scale,
                effect_rot,
                effect_offset,
                tint,
                glow,
                overlay_blend,
                total_elapsed,
            )
            .map(one_song_lua_actor)
        }
        SongLuaOverlayKind::NoteskinActor { slots } => {
            let mut tint = state.diffuse;
            let mut glow = state.glow;
            let mut effect_offset = [0.0, 0.0, 0.0];
            let mut effect_scale = [1.0, 1.0, 1.0];
            let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
            song_lua_apply_overlay_effect(
                effect,
                state.rainbow,
                effect_time,
                effect_beat,
                &mut tint,
                &mut glow,
                &mut effect_offset,
                &mut effect_scale,
                &mut effect_rot,
            );
            song_lua_noteskin_actor(
                slots,
                state,
                asset_manager,
                z,
                x_scale,
                y_scale,
                [size_scale_x, size_scale_y],
                effect_scale,
                effect_rot,
                effect_offset,
                tint,
                glow,
                overlay_blend,
                total_elapsed,
                effect_beat,
            )
            .map(one_song_lua_actor)
        }
        SongLuaOverlayKind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds,
        } => {
            let actor = song_lua_song_meter_actor(
                state,
                *stream_state,
                *stream_width,
                *music_length_seconds,
                x_scale,
                y_scale,
                z,
                total_elapsed,
            )?;
            let glow = [
                state.glow[0] + stream_state.glow[0],
                state.glow[1] + stream_state.glow[1],
                state.glow[2] + stream_state.glow[2],
                state.glow[3].max(stream_state.glow[3]),
            ];
            Some(finalize_actor(actor, glow))
        }
        SongLuaOverlayKind::GraphDisplay {
            size,
            body_values,
            body_state,
            line_state,
        } => {
            let actor = song_lua_graph_display_actor(
                state,
                body_values,
                *body_state,
                *line_state,
                *size,
                x_scale,
                y_scale,
                z,
            )?;
            let glow = [
                state.glow[0] + body_state.glow[0].max(line_state.glow[0]),
                state.glow[1] + body_state.glow[1].max(line_state.glow[1]),
                state.glow[2] + body_state.glow[2].max(line_state.glow[2]),
                state.glow[3].max(body_state.glow[3].max(line_state.glow[3])),
            ];
            Some(finalize_actor(actor, glow))
        }
        SongLuaOverlayKind::Quad => {
            if let Some(view_proj) = perspective_view_proj {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    state.size.unwrap_or([1.0, 1.0]),
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    effect_time,
                    effect_beat,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_projected_overlay_actor(
                    Arc::from("__white"),
                    tint,
                    overlay_blend,
                    z,
                    [
                        center[0] + effect_offset[0] * x_scale,
                        center[1] + effect_offset[1] * y_scale,
                        effect_offset[2],
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(state, None, flip_x, flip_y, total_elapsed),
                    state,
                    flip_x,
                    flip_y,
                    view_proj,
                )?;
                return Some(finalize_actor(actor, glow));
            }
            if (state.skew_x.abs() > f32::EPSILON
                || state.skew_y.abs() > f32::EPSILON
                || state.vertex_colors.is_some())
                && !state.mask_source
                && !state.mask_dest
            {
                let (center, size) = song_lua_overlay_rect(
                    state,
                    state.size.unwrap_or([1.0, 1.0]),
                    x_scale,
                    y_scale,
                    size_scale_x,
                    size_scale_y,
                )?;
                let mut tint = state.diffuse;
                let mut glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut rot_deg = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    effect_time,
                    effect_beat,
                    &mut tint,
                    &mut glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut rot_deg,
                );
                let actor = song_lua_flat_skewed_overlay_actor(
                    Arc::from("__white"),
                    tint,
                    overlay_blend,
                    z,
                    [
                        center[0] + effect_offset[0] * x_scale,
                        center[1] + effect_offset[1] * y_scale,
                    ],
                    [size[0] * effect_scale[0], size[1] * effect_scale[1]],
                    rot_deg,
                    song_lua_overlay_uvs(state, None, flip_x, flip_y, total_elapsed),
                    state,
                    flip_x,
                    flip_y,
                    effect_offset[2],
                )?;
                return Some(finalize_actor(actor, glow));
            }
            let mut actor = if let Some([left, top, right, bottom]) = state.stretch_rect {
                act!(quad:
                    align(0.0, 0.0):
                    xy(left * x_scale, top * y_scale):
                    zoomto(
                        (right - left).abs() * x_scale * size_scale_x,
                        (bottom - top).abs() * y_scale * size_scale_y
                    ):
                    diffuse(state.diffuse[0], state.diffuse[1], state.diffuse[2], state.diffuse[3]):
                    z(z)
                )
            } else {
                let size = state.size.unwrap_or([1.0, 1.0]);
                act!(quad:
                    align(state.halign, state.valign):
                    xy(state.x * x_scale, state.y * y_scale):
                    zoomto(
                        size[0] * x_scale * size_scale_x,
                        size[1] * y_scale * size_scale_y
                    ):
                    diffuse(state.diffuse[0], state.diffuse[1], state.diffuse[2], state.diffuse[3]):
                    z(z)
                )
            };
            if let Actor::Sprite {
                visible,
                tint,
                glow,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                offset,
                world_z,
                scale,
                effect: actor_effect,
                flip_x: actor_flip_x,
                flip_y: actor_flip_y,
                ..
            } = &mut actor
            {
                let mut effect_tint = state.diffuse;
                let mut effect_glow = state.glow;
                let mut effect_offset = [0.0, 0.0, 0.0];
                let mut effect_scale = [1.0, 1.0, 1.0];
                let mut effect_rot = [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg];
                song_lua_apply_overlay_effect(
                    effect,
                    state.rainbow,
                    effect_time,
                    effect_beat,
                    &mut effect_tint,
                    &mut effect_glow,
                    &mut effect_offset,
                    &mut effect_scale,
                    &mut effect_rot,
                );
                *tint = effect_tint;
                *glow = effect_glow;
                *cropleft = state.cropleft.clamp(0.0, 1.0);
                *cropright = state.cropright.clamp(0.0, 1.0);
                *croptop = state.croptop.clamp(0.0, 1.0);
                *cropbottom = state.cropbottom.clamp(0.0, 1.0);
                *fadeleft = state.fadeleft.clamp(0.0, 1.0);
                *faderight = state.faderight.clamp(0.0, 1.0);
                *fadetop = state.fadetop.clamp(0.0, 1.0);
                *fadebottom = state.fadebottom.clamp(0.0, 1.0);
                *blend = overlay_blend;
                *mask_source = state.mask_source;
                *mask_dest = state.mask_dest;
                *rot_x_deg = effect_rot[0];
                *rot_y_deg = effect_rot[1];
                *rot_z_deg = effect_rot[2];
                offset[0] += effect_offset[0] * x_scale;
                offset[1] += effect_offset[1] * y_scale;
                *world_z += song_lua_biased_world_z(state, effect_offset[2]);
                scale[0] *= effect_scale[0];
                scale[1] *= effect_scale[1];
                *actor_effect = EffectState::default();
                *actor_flip_x ^= flip_x;
                *actor_flip_y ^= flip_y;
                *visible = state.visible;
            }
            let glow = if let Actor::Sprite { glow, .. } = &actor {
                *glow
            } else {
                state.glow
            };
            Some(finalize_actor(actor, glow))
        }
    }
}

fn song_lua_wrap_overlay_shadow(
    state: SongLuaOverlayState,
    mut actor: Actor,
    x_scale: f32,
    y_scale: f32,
) -> Actor {
    if state.shadow_len[0].abs() <= f32::EPSILON && state.shadow_len[1].abs() <= f32::EPSILON {
        return actor;
    }
    let len = [state.shadow_len[0] * x_scale, state.shadow_len[1] * y_scale];
    match &mut actor {
        Actor::Sprite {
            shadow_len,
            shadow_color,
            ..
        }
        | Actor::Text {
            shadow_len,
            shadow_color,
            ..
        } if shadow_len[0].abs() <= f32::EPSILON && shadow_len[1].abs() <= f32::EPSILON => {
            *shadow_len = len;
            *shadow_color = state.shadow_color;
            actor
        }
        _ => Actor::Shadow {
            len,
            color: state.shadow_color,
            child: Box::new(actor),
        },
    }
}

fn song_lua_overlay_glow_actor(
    actor: &Actor,
    glow: [f32; 4],
    text_glow_mode: SongLuaTextGlowMode,
) -> Option<Actor> {
    match actor {
        Actor::Sprite {
            align,
            offset,
            world_z,
            size,
            source,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            effect,
            ..
        } => {
            if glow[3] <= f32::EPSILON {
                return None;
            }
            if *mask_source && !*mask_dest {
                return None;
            }
            Some(Actor::Sprite {
                align: *align,
                offset: *offset,
                world_z: *world_z,
                size: *size,
                source: source.clone(),
                tint: glow,
                glow: [0.0, 0.0, 0.0, 0.0],
                z: *z,
                cell: *cell,
                grid: *grid,
                uv_rect: *uv_rect,
                visible: *visible,
                flip_x: *flip_x,
                flip_y: *flip_y,
                cropleft: *cropleft,
                cropright: *cropright,
                croptop: *croptop,
                cropbottom: *cropbottom,
                fadeleft: *fadeleft,
                faderight: *faderight,
                fadetop: *fadetop,
                fadebottom: *fadebottom,
                blend: BlendMode::Add,
                mask_source: false,
                mask_dest: *mask_dest,
                rot_x_deg: *rot_x_deg,
                rot_y_deg: *rot_y_deg,
                rot_z_deg: *rot_z_deg,
                local_offset: *local_offset,
                local_offset_rot_sin_cos: *local_offset_rot_sin_cos,
                texcoordvelocity: *texcoordvelocity,
                animate: *animate,
                state_delay: *state_delay,
                scale: *scale,
                shadow_len: [0.0, 0.0],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
                effect: *effect,
            })
        }
        Actor::Text {
            align,
            offset,
            local_transform,
            font,
            content,
            attributes: base_attributes,
            align_text,
            z,
            scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter: _,
            distortion,
            clip,
            mask_dest,
            effect,
            ..
        } => {
            let has_attr_glow = song_lua_text_attributes_have_glow(base_attributes);
            if glow[3] <= f32::EPSILON && !has_attr_glow {
                return None;
            }
            let (attributes, color, stroke_color) = if has_attr_glow {
                let attributes =
                    song_lua_text_glow_attributes(content.as_str(), base_attributes, glow);
                let stroke_color = (glow[3] > f32::EPSILON
                    && matches!(
                        text_glow_mode,
                        SongLuaTextGlowMode::Stroke | SongLuaTextGlowMode::Both
                    ))
                .then_some(glow);
                (attributes, [1.0, 1.0, 1.0, 1.0], stroke_color)
            } else {
                let mut attributes = base_attributes.clone();
                let (color, stroke_color) = match text_glow_mode {
                    SongLuaTextGlowMode::Inner => (glow, None),
                    SongLuaTextGlowMode::Both => (glow, Some(glow)),
                    SongLuaTextGlowMode::Stroke => {
                        attributes = song_lua_transparent_text_attributes(content.as_str());
                        ([1.0, 1.0, 1.0, 1.0], Some(glow))
                    }
                };
                (attributes, color, stroke_color)
            };
            Some(Actor::Text {
                align: *align,
                offset: *offset,
                local_transform: *local_transform,
                color,
                stroke_color,
                glow: [0.0, 0.0, 0.0, 0.0],
                font: *font,
                content: content.clone(),
                attributes,
                align_text: *align_text,
                z: *z,
                scale: *scale,
                fit_width: *fit_width,
                fit_height: *fit_height,
                line_spacing: *line_spacing,
                wrap_width_pixels: *wrap_width_pixels,
                max_width: *max_width,
                max_height: *max_height,
                max_w_pre_zoom: *max_w_pre_zoom,
                max_h_pre_zoom: *max_h_pre_zoom,
                jitter: false,
                distortion: *distortion,
                clip: *clip,
                mask_dest: *mask_dest,
                blend: BlendMode::Add,
                shadow_len: [0.0, 0.0],
                shadow_color: [0.0, 0.0, 0.0, 0.5],
                effect: *effect,
            })
        }
        Actor::TexturedMesh {
            align,
            offset,
            world_z,
            size,
            local_transform,
            texture,
            vertices,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            z,
            ..
        } => {
            if glow[3] <= f32::EPSILON {
                return None;
            }
            let mut glow_vertices = vertices.as_ref().to_vec();
            for vertex in &mut glow_vertices {
                vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
            }
            Some(Actor::TexturedMesh {
                align: *align,
                offset: *offset,
                world_z: *world_z,
                size: *size,
                local_transform: *local_transform,
                texture: texture.clone(),
                tint: [1.0, 1.0, 1.0, 0.0],
                glow,
                vertices: Arc::from(glow_vertices.into_boxed_slice()),
                geom_cache_key: INVALID_TMESH_CACHE_KEY,
                uv_scale: *uv_scale,
                uv_offset: *uv_offset,
                uv_tex_shift: *uv_tex_shift,
                depth_test: *depth_test,
                visible: *visible,
                blend: BlendMode::Add,
                z: *z,
            })
        }
        _ => None,
    }
}

fn song_lua_finalize_overlay_actor(
    state: SongLuaOverlayState,
    actor: Actor,
    glow: [f32; 4],
    x_scale: f32,
    y_scale: f32,
) -> SongLuaActorList {
    let glow_actor = song_lua_overlay_glow_actor(&actor, glow, state.text_glow_mode);
    let actor = song_lua_wrap_overlay_shadow(state, actor, x_scale, y_scale);
    let mut out = SmallVec::new();
    out.push(actor);
    if let Some(glow_actor) = glow_actor {
        out.push(glow_actor);
    }
    out
}

fn song_lua_player_skew_x_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, //
        amount, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

fn song_lua_player_skew_y_matrix(amount: f32) -> Matrix4 {
    Matrix4::from_cols_array(&[
        1.0, amount, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ])
}

#[inline(always)]
fn song_lua_fold_x_around_pivot(x: f32, pivot_x: f32, cos_y: f32) -> f32 {
    pivot_x + (x - pivot_x) * cos_y
}

fn song_lua_player_y_fold_actor(actor: Actor, pivot_x: f32, rotation_y_deg: f32) -> Actor {
    if !pivot_x.is_finite() || !rotation_y_deg.is_finite() || rotation_y_deg.abs() <= f32::EPSILON {
        return actor;
    }
    let cos_y = rotation_y_deg.to_radians().cos();
    match actor {
        Actor::Sprite {
            align,
            mut offset,
            world_z,
            size,
            source,
            tint,
            glow,
            z,
            cell,
            grid,
            uv_rect,
            visible,
            flip_x,
            flip_y,
            cropleft,
            cropright,
            croptop,
            cropbottom,
            fadeleft,
            faderight,
            fadetop,
            fadebottom,
            blend,
            mask_source,
            mask_dest,
            rot_x_deg,
            rot_y_deg,
            rot_z_deg,
            local_offset,
            local_offset_rot_sin_cos,
            texcoordvelocity,
            animate,
            state_delay,
            scale,
            shadow_len,
            shadow_color,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Sprite {
                align,
                offset,
                world_z,
                size,
                source,
                tint,
                glow,
                z,
                cell,
                grid,
                uv_rect,
                visible,
                flip_x,
                flip_y,
                cropleft,
                cropright,
                croptop,
                cropbottom,
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                blend,
                mask_source,
                mask_dest,
                rot_x_deg,
                rot_y_deg,
                rot_z_deg,
                local_offset,
                local_offset_rot_sin_cos,
                texcoordvelocity,
                animate,
                state_delay,
                scale,
                shadow_len,
                shadow_color,
                effect,
            }
        }
        Actor::Text {
            align,
            mut offset,
            local_transform,
            color,
            stroke_color,
            glow,
            font,
            content,
            attributes,
            align_text,
            z,
            mut scale,
            fit_width,
            fit_height,
            line_spacing,
            wrap_width_pixels,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            jitter,
            distortion,
            clip,
            mask_dest,
            blend,
            shadow_len,
            shadow_color,
            effect,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            scale[0] *= cos_y;
            Actor::Text {
                align,
                offset,
                local_transform,
                color,
                stroke_color,
                glow,
                font,
                content,
                attributes,
                align_text,
                z,
                scale,
                fit_width,
                fit_height,
                line_spacing,
                wrap_width_pixels,
                max_width,
                max_height,
                max_w_pre_zoom,
                max_h_pre_zoom,
                jitter,
                distortion,
                clip,
                mask_dest,
                blend,
                shadow_len,
                shadow_color,
                effect,
            }
        }
        Actor::Mesh {
            align,
            mut offset,
            size,
            vertices,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Mesh {
                align,
                offset,
                size,
                vertices,
                visible,
                blend,
                z,
            }
        }
        Actor::TexturedMesh {
            align,
            mut offset,
            world_z,
            size,
            local_transform,
            texture,
            tint,
            glow,
            vertices,
            geom_cache_key,
            uv_scale,
            uv_offset,
            uv_tex_shift,
            depth_test,
            visible,
            blend,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::TexturedMesh {
                align,
                offset,
                world_z,
                size,
                local_transform,
                texture,
                tint,
                glow,
                vertices,
                geom_cache_key,
                uv_scale,
                uv_offset,
                uv_tex_shift,
                depth_test,
                visible,
                blend,
                z,
            }
        }
        Actor::Frame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::Frame {
                align,
                offset,
                size,
                children: children
                    .into_iter()
                    .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                    .collect(),
                background,
                z,
            }
        }
        Actor::SharedFrame {
            mut offset,
            children,
            align,
            size,
            background,
            z,
            tint,
            blend,
        } => {
            offset[0] = song_lua_fold_x_around_pivot(offset[0], pivot_x, cos_y);
            Actor::SharedFrame {
                align,
                offset,
                size,
                children,
                background,
                z,
                tint,
                blend,
            }
        }
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: children
                .into_iter()
                .map(|child| song_lua_player_y_fold_actor(child, pivot_x, rotation_y_deg))
                .collect(),
        },
        Actor::CameraPush { view_proj } => Actor::CameraPush { view_proj },
        Actor::CameraPop => Actor::CameraPop,
        Actor::Shadow { len, color, child } => Actor::Shadow {
            len,
            color,
            child: Box::new(song_lua_player_y_fold_actor(
                *child,
                pivot_x,
                rotation_y_deg,
            )),
        },
    }
}

fn song_lua_player_transform_matrix(
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x_deg: f32,
    rotation_z_deg: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
) -> Option<Matrix4> {
    if !playfield_center_x.is_finite()
        || !target_x.is_finite()
        || !target_y.is_finite()
        || !rotation_x_deg.is_finite()
        || !rotation_z_deg.is_finite()
        || !skew_x.is_finite()
        || !skew_y.is_finite()
        || !zoom_x.is_finite()
        || !zoom_y.is_finite()
        || !zoom_z.is_finite()
    {
        return None;
    }
    let rotation_x_deg = if rotation_x_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_x_deg
    };
    let rotation_z_deg = if rotation_z_deg.abs() <= f32::EPSILON {
        0.0
    } else {
        rotation_z_deg
    };
    let skew_x = if skew_x.abs() <= f32::EPSILON {
        0.0
    } else {
        skew_x
    };
    let skew_y = if skew_y.abs() <= f32::EPSILON {
        0.0
    } else {
        skew_y
    };
    let zoom_x = if (zoom_x - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_x
    };
    let zoom_y = if (zoom_y - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_y
    };
    let zoom_z = if (zoom_z - 1.0).abs() <= f32::EPSILON {
        1.0
    } else {
        zoom_z
    };
    let translate_x = target_x - playfield_center_x;
    let translate_y = screen_center_y() - target_y;
    if rotation_x_deg.abs() <= f32::EPSILON
        && rotation_z_deg.abs() <= f32::EPSILON
        && skew_x.abs() <= f32::EPSILON
        && skew_y.abs() <= f32::EPSILON
        && (zoom_x - 1.0).abs() <= f32::EPSILON
        && (zoom_y - 1.0).abs() <= f32::EPSILON
        && (zoom_z - 1.0).abs() <= f32::EPSILON
        && translate_x.abs() <= f32::EPSILON
        && translate_y.abs() <= f32::EPSILON
    {
        return None;
    }

    let pivot_x = playfield_center_x - 0.5 * screen_width();
    let pivot_y = 0.5 * screen_height() - screen_center_y();
    // ITGmania actor transforms are authored in screen coordinates (Y down).
    // This matrix is applied in DeadSync world space (Y up), so Z rotation and
    // actor skews flip sign across the Y axis.
    let rotation_z_deg = -rotation_z_deg;
    let skew_x = -skew_x;
    let skew_y = -skew_y;
    Some(
        Matrix4::from_translation(Vector3::new(translate_x, translate_y, 0.0))
            * Matrix4::from_translation(Vector3::new(pivot_x, pivot_y, 0.0))
            * Matrix4::from_rotation_x(rotation_x_deg.to_radians())
            * Matrix4::from_rotation_z(rotation_z_deg.to_radians())
            * song_lua_player_skew_x_matrix(skew_x)
            * song_lua_player_skew_y_matrix(skew_y)
            * Matrix4::from_scale(Vector3::new(zoom_x, zoom_y, zoom_z))
            * Matrix4::from_translation(Vector3::new(-pivot_x, -pivot_y, 0.0)),
    )
}

fn apply_song_lua_player_transform(
    field_actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    out: &mut Vec<Actor>,
    z_shift: i16,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x_deg: f32,
    rotation_z_deg: f32,
    rotation_y_deg: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
) {
    #[inline(always)]
    fn push_actor_with_style(
        out: &mut Vec<Actor>,
        actor: Actor,
        tint: [f32; 4],
        blend: Option<BlendMode>,
        z_shift: i16,
    ) {
        if z_shift == 0 && tint == [1.0; 4] && blend.is_none() {
            out.push(actor);
        } else {
            out.push(song_lua_style_capture_actor(actor, tint, blend, z_shift));
        }
    }

    #[inline(always)]
    fn push_camera_scope(
        out: &mut Vec<Actor>,
        view_proj: Matrix4,
        children: &mut Vec<Actor>,
        tint: [f32; 4],
        blend: Option<BlendMode>,
        z_shift: i16,
    ) {
        if children.is_empty() {
            return;
        }
        push_actor_with_style(out, Actor::CameraPush { view_proj }, tint, blend, z_shift);
        for actor in children.drain(..) {
            push_actor_with_style(out, actor, tint, blend, z_shift);
        }
        push_actor_with_style(out, Actor::CameraPop, tint, blend, z_shift);
    }

    out.clear();
    if rotation_y_deg.is_finite() && rotation_y_deg.abs() > f32::EPSILON {
        out.extend(
            field_actors.drain(..).map(|actor| {
                song_lua_player_y_fold_actor(actor, playfield_center_x, rotation_y_deg)
            }),
        );
        std::mem::swap(field_actors, out);
        out.extend(
            hud_actors.drain(..).map(|actor| {
                song_lua_player_y_fold_actor(actor, playfield_center_x, rotation_y_deg)
            }),
        );
        std::mem::swap(hud_actors, out);
    }

    let Some(player_transform) = song_lua_player_transform_matrix(
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_z_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    ) else {
        out.reserve(field_actors.len().saturating_add(hud_actors.len()));
        for actor in hud_actors.drain(..) {
            push_actor_with_style(out, actor, [1.0; 4], None, z_shift);
        }
        for actor in field_actors.drain(..) {
            push_actor_with_style(out, actor, [1.0; 4], None, z_shift);
        }
        return;
    };
    // notefield::build may already wrap the lane render in a perspective camera.
    // Multiply those cameras in place, and only wrap plain HUD actors here, so
    // the Lua transform affects the whole bundle without being shadowed.
    let root_camera = Matrix4::orthographic_rh_gl(
        -0.5 * screen_width(),
        0.5 * screen_width(),
        -0.5 * screen_height(),
        0.5 * screen_height(),
        -4096.0,
        4096.0,
    ) * player_transform;
    if !field_actors.iter().any(|actor| {
        matches!(
            actor,
            Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop
        )
    }) {
        if !field_actors.is_empty() {
            hud_actors.extend(field_actors.drain(..));
        }
        out.reserve(hud_actors.len().saturating_add(2));
        push_camera_scope(out, root_camera, hud_actors, tint, blend, z_shift);
        return;
    }

    out.reserve(
        field_actors
            .len()
            .saturating_add(hud_actors.len())
            .saturating_add(4),
    );
    let mut field_camera_depth = 0usize;
    for actor in field_actors.drain(..) {
        match actor {
            Actor::Camera {
                view_proj,
                children,
            } => {
                if field_camera_depth == 0 {
                    push_camera_scope(out, root_camera, hud_actors, tint, blend, z_shift);
                }
                push_actor_with_style(
                    out,
                    Actor::CameraPush {
                        view_proj: view_proj * player_transform,
                    },
                    tint,
                    blend,
                    z_shift,
                );
                for child in children {
                    push_actor_with_style(out, child, tint, blend, z_shift);
                }
                push_actor_with_style(out, Actor::CameraPop, tint, blend, z_shift);
            }
            Actor::CameraPush { view_proj } => {
                if field_camera_depth == 0 {
                    push_camera_scope(out, root_camera, hud_actors, tint, blend, z_shift);
                }
                push_actor_with_style(
                    out,
                    Actor::CameraPush {
                        view_proj: view_proj * player_transform,
                    },
                    tint,
                    blend,
                    z_shift,
                );
                field_camera_depth = field_camera_depth.saturating_add(1);
            }
            Actor::CameraPop => {
                push_actor_with_style(out, Actor::CameraPop, tint, blend, z_shift);
                field_camera_depth = field_camera_depth.saturating_sub(1);
            }
            other if field_camera_depth > 0 => {
                push_actor_with_style(out, other, tint, blend, z_shift);
            }
            other => hud_actors.push(other),
        }
    }
    push_camera_scope(out, root_camera, hud_actors, tint, blend, z_shift);
}

fn song_lua_player_target_x(
    explicit_x: Option<f32>,
    player_state_x: f32,
    layout_center_x: f32,
    notefield_view: notefield::ViewOverride,
) -> f32 {
    explicit_x.unwrap_or(if notefield_view.force_center_1player {
        layout_center_x
    } else {
        player_state_x
    })
}

fn push_song_lua_layer_actors(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor],
    order_cache: &mut SongLuaOverlayOrderCache,
    local_overlay_states: &[SongLuaOverlayState],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    asset_manager: &AssetManager,
    space_width: f32,
    space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    order_scratch: &mut Vec<usize>,
    capture_states: &mut Vec<SongLuaOverlayState>,
    capture_order_scratch: &mut Vec<usize>,
) {
    let song_lua_overlay_base_z = song_lua_add_z(
        SONG_LUA_OVERLAY_LAYER_Z_BASE,
        song_lua_rounded_z(song_foreground_state.z),
    );
    out.reserve(overlays.len());
    song_lua_overlay_order_into(overlays, overlay_states, order_cache, None, order_scratch);
    for (draw_idx, idx) in order_scratch.iter().copied().enumerate() {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        if song_lua_overlay_aft_ancestor(overlays, idx).is_some() {
            continue;
        }
        let overlay_state = overlay_states
            .get(idx)
            .copied()
            .unwrap_or_else(|| SongLuaOverlayState::default());
        let actor = match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_proxy_source(target, proxy_sources)
                    .and_then(|source| {
                        song_lua_build_proxy_actor(
                            overlay_state,
                            song_lua_add_z(
                                song_lua_overlay_base_z,
                                draw_idx.min(i16::MAX as usize) as i16,
                            ),
                            source,
                            space_width,
                            space_height,
                        )
                    })
                    .map(one_song_lua_actor)
            }
            SongLuaOverlayKind::AftSprite { capture_name } => {
                let overlay_state = if let Some((leader, _)) =
                    song_lua_rgb_aft_group_for(overlays, overlay_states, order_scratch, idx)
                {
                    if leader != idx {
                        continue;
                    }
                    song_lua_combined_rgb_aft_state(overlay_state)
                } else {
                    overlay_state
                };
                if let Some(capture_index) =
                    song_lua_overlay_capture_index_by_name(overlays, capture_name)
                {
                    let source = song_lua_capture_children(
                        overlays,
                        overlay_states,
                        local_overlay_states,
                        order_cache,
                        asset_manager,
                        capture_index,
                        proxy_sources,
                        space_width,
                        space_height,
                        capture_states,
                        capture_order_scratch,
                    );
                    song_lua_build_capture_actor(
                        overlay,
                        overlay_state,
                        song_lua_add_z(
                            song_lua_overlay_base_z,
                            draw_idx.min(i16::MAX as usize) as i16,
                        ),
                        source,
                        space_width,
                        space_height,
                    )
                    .map(one_song_lua_actor)
                } else {
                    None
                }
            }
            _ => build_song_lua_overlay_actor(
                overlay,
                overlay_state,
                song_lua_overlay_camera_state(overlays, overlay_states, overlay.parent_index),
                asset_manager,
                song_lua_add_z(
                    song_lua_overlay_base_z,
                    draw_idx.min(i16::MAX as usize) as i16,
                ),
                space_width,
                space_height,
                effect_time,
                effect_beat,
                total_elapsed,
            ),
        };
        if let Some(actors) = actor {
            out.extend(actors);
        }
    }
}

pub fn push_actors(
    mut actors: &mut Vec<Actor>,
    state: &mut State,
    asset_manager: &AssetManager,
    view: ActorViewOverride,
) {
    let mut song_lua_overlay_order = std::mem::take(&mut state.song_lua_overlay_order);
    let mut song_lua_background_visual_layer_orders =
        std::mem::take(&mut state.song_lua_background_visual_layer_orders);
    let mut song_lua_foreground_visual_layer_orders =
        std::mem::take(&mut state.song_lua_foreground_visual_layer_orders);
    let mut song_lua_local_state_scratch = std::mem::take(&mut state.song_lua_local_state_scratch);
    let mut song_lua_overlay_state_scratch =
        std::mem::take(&mut state.song_lua_overlay_state_scratch);
    let mut song_lua_layer_local_state_scratch =
        std::mem::take(&mut state.song_lua_layer_local_state_scratch);
    let mut song_lua_layer_state_scratch = std::mem::take(&mut state.song_lua_layer_state_scratch);
    let mut song_lua_capture_state_scratch =
        std::mem::take(&mut state.song_lua_capture_state_scratch);
    let mut song_lua_order_scratch = std::mem::take(&mut state.song_lua_order_scratch);
    let mut song_lua_capture_order_scratch =
        std::mem::take(&mut state.song_lua_capture_order_scratch);
    let mut notefield_actor_scratch = std::mem::take(&mut state.notefield_actor_scratch);
    let mut notefield_hud_actor_scratch = std::mem::take(&mut state.notefield_hud_actor_scratch);
    let mut player_actor_scratch = std::mem::take(&mut state.player_actor_scratch);
    for actors in &mut player_actor_scratch {
        actors.clear();
    }

    let notefield_view = view.notefield;
    let hide_gameplay_hud = view.hide_gameplay_hud;
    let cfg = crate::config::get();
    let hud_snapshot = profile::gameplay_hud_snapshot();
    actors.reserve(96);
    let play_style = hud_snapshot.play_style;
    let player_side = hud_snapshot.player_side;
    let is_p2_single = profile_data::is_single_p2_side(play_style, player_side);
    let center_1player_notefield =
        cfg.center_1player_notefield || notefield_view.force_center_1player;
    let centered_single_notefield = play_style == profile_data::PlayStyle::Single
        && state.num_players() == 1
        && center_1player_notefield;
    let song_lua_visuals = state.song_lua_visuals();
    let song_lua_space_width = song_lua_overlay_space_width(state);
    let song_lua_space_height = song_lua_overlay_space_height(state);
    let player_color = color::decorative_rgba(state.player_color_index());
    song_lua_overlay_state_sets_into(
        state,
        &mut song_lua_local_state_scratch,
        &mut song_lua_overlay_state_scratch,
    );
    let mut proxy_requests =
        song_lua_proxy_requests(&song_lua_visuals.overlays, &song_lua_overlay_state_scratch);
    for layer in &song_lua_visuals.foreground_visual_layers {
        if state.current_music_time_display() < layer.start_second {
            continue;
        }
        song_lua_overlay_state_sets_from_into(
            state.current_music_time_display(),
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &mut song_lua_layer_local_state_scratch,
            &mut song_lua_layer_state_scratch,
        );
        song_lua_merge_proxy_requests(
            &mut proxy_requests,
            song_lua_proxy_requests(&layer.overlays, &song_lua_layer_state_scratch),
        );
    }
    let mut underlay_proxy_source = proxy_requests.underlay.then_some(Vec::new());
    let mut overlay_proxy_source = proxy_requests.overlay.then_some(Vec::new());
    // --- Background and Filter ---
    let underlay_start = actors.len();
    push_background(&mut actors, state, cfg.bg_brightness, cfg.gameplay_bg_color);
    for (layer_idx, layer) in song_lua_visuals.background_visual_layers.iter().enumerate() {
        if state.current_music_time_display() < layer.start_second {
            continue;
        }
        let Some(order_cache) = song_lua_background_visual_layer_orders.get_mut(layer_idx) else {
            continue;
        };
        song_lua_overlay_state_sets_from_into(
            state.current_music_time_display(),
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &mut song_lua_layer_local_state_scratch,
            &mut song_lua_layer_state_scratch,
        );
        let song_foreground_state = song_lua_song_foreground_state_from(
            state.current_music_time_display(),
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
        );
        push_song_lua_layer_actors(
            &mut actors,
            &layer.overlays,
            order_cache,
            &song_lua_layer_local_state_scratch,
            &song_lua_layer_state_scratch,
            song_foreground_state,
            &SongLuaScreenProxySources::default(),
            asset_manager,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            state.current_music_time_display(),
            state.current_beat(),
            state.total_elapsed_in_screen(),
            &mut song_lua_order_scratch,
            &mut song_lua_capture_state_scratch,
            &mut song_lua_capture_order_scratch,
        );
    }
    song_lua_capture_new_actors(&mut underlay_proxy_source, &mut actors, underlay_start);
    let cover_alpha = |player_idx: usize| -> f32 {
        if player_idx >= state.num_players() {
            return 0.0;
        }
        let profile_cover = f32::from(state.profiles()[player_idx].hide_song_bg);
        profile_cover
            .max(
                state
                    .effective_visibility_effects_for_player(player_idx)
                    .cover,
            )
            .clamp(0.0, 1.0)
    };
    let left_cover = cover_alpha(0);
    let right_cover = if state.num_players() > 1 {
        cover_alpha(1)
    } else {
        left_cover
    };
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    if left_cover > 0.0 || right_cover > 0.0 {
        if (left_cover - right_cover).abs() <= 0.001 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(sw, sh):
                diffuse(0.0, 0.0, 0.0, left_cover.max(right_cover)):
                z(-99)
            ));
        } else {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(cx, sh):
                faderight(0.1):
                diffuse(0.0, 0.0, 0.0, left_cover):
                z(-99)
            ));
            actors.push(act!(quad:
                align(0.0, 0.0): xy(cx, 0.0):
                zoomto(sw - cx, sh):
                fadeleft(0.1):
                diffuse(0.0, 0.0, 0.0, right_cover):
                z(-99)
            ));
        }
    }

    // ITGmania/Simply Love parity: ScreenSyncOverlay status text.
    if !hide_gameplay_hud {
        let overlay_start = actors.len();
        let status_line_count = if let Some((status_text, line_count)) = sync_overlay_text(state) {
            actors.push(act!(text:
                font("miso"):
                settext(status_text):
                align(0.5, 0.5):
                xy(screen_center_x(), screen_center_y() + 150.0):
                horizalign(center):
                shadowlength(2.0):
                strokecolor(0.0, 0.0, 0.0, 1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(2101)
            ));
            line_count
        } else {
            0
        };

        if let Some((flash, alpha)) = state.toggle_flash_text() {
            let y = if status_line_count == 0 {
                screen_center_y() + 150.0
            } else {
                screen_center_y() + 150.0 + 20.0 * status_line_count as f32
            };
            actors.push(act!(text:
                font("miso"):
                settext(flash):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                shadowlength(2.0):
                strokecolor(0.0, 0.0, 0.0, alpha):
                diffuse(1.0, 1.0, 1.0, alpha):
                z(2101)
            ));
        }

        if state.autosync_mode() != AutosyncMode::Off {
            let (old_offset, new_offset) = if state.autosync_mode() == AutosyncMode::Machine {
                (
                    state.initial_global_offset_seconds(),
                    state.global_offset_seconds(),
                )
            } else {
                (
                    state.initial_song_offset_seconds(),
                    state.song_offset_seconds(),
                )
            };
            let adjustments = cached_autosync_text(state, old_offset, new_offset);
            actors.push(act!(text:
                font("miso"):
                settext(adjustments):
                align(0.5, 0.5):
                xy(screen_center_x() + 160.0, screen_center_y()):
                horizalign(center):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(2101)
            ));
        }
        song_lua_capture_new_actors(&mut overlay_proxy_source, &mut actors, overlay_start);
    }

    // Hold START/BACK prompt (Simply Love parity: ScreenGameplay debug text).
    if !hide_gameplay_hud {
        let overlay_start = actors.len();
        const HOLD_FADE_IN_S: f32 = 1.0 / 8.0;
        const ABORT_FADE_OUT_S: f32 = 0.5;

        let y = screen_height() - 116.0;
        let exit_prompt = state.exit_prompt_state();
        let msg: Option<(String, f32)> = if gameplay_lobby_wait_text(state).is_some() {
            None
        } else if let (Some(key), Some(start)) =
            (exit_prompt.hold_to_exit_key, exit_prompt.hold_to_exit_start)
        {
            let s = match key {
                HoldToExitKey::Start => Some(tr("Gameplay", "ContinueHoldingStartGiveUp")),
                HoldToExitKey::Back => Some(tr("Gameplay", "ContinueHoldingBackGiveUp")),
            };
            let alpha = (start.elapsed().as_secs_f32() / HOLD_FADE_IN_S).clamp(0.0, 1.0);
            s.map(|text| (text.to_string(), alpha))
        } else if let Some(exit) = &exit_prompt.exit_transition {
            let t = exit.started_at.elapsed().as_secs_f32();
            match exit.kind {
                ExitTransitionKind::Out => {
                    let alpha = (1.0 - t / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
                    Some((
                        tr("Gameplay", "ContinueHoldingStartGiveUp").to_string(),
                        alpha,
                    ))
                }
                ExitTransitionKind::Cancel => {
                    Some((tr("Gameplay", "ContinueHoldingBackGiveUp").to_string(), 1.0))
                }
            }
        } else if let Some(at) = exit_prompt.hold_to_exit_aborted_at {
            let alpha = (1.0 - at.elapsed().as_secs_f32() / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
            Some((tr("Gameplay", "DontGoBack").to_string(), alpha))
        } else {
            None
        };

        if let Some((text, alpha)) = msg
            && alpha > 0.0
        {
            actors.push(act!(text:
                font("miso"):
                settext(text):
                align(0.5, 0.5):
                xy(screen_center_x(), y):
                zoom(0.75):
                shadowlength(2.0):
                diffuse(1.0, 1.0, 1.0, alpha):
                z(1000)
            ));
        }
        song_lua_capture_new_actors(&mut overlay_proxy_source, &mut actors, overlay_start);
    }

    if !hide_gameplay_hud {
        let overlay_start = actors.len();
        let lobby_snapshot = crate::game::online::lobbies::snapshot();
        if let Some(joined) = lobby_snapshot.joined_lobby.as_ref() {
            actors.extend(lobby_hud::build_panel(lobby_hud::RenderParams {
                screen_name: "ScreenGameplay",
                joined,
                z: 995,
                show_song_info: false,
                status_text: gameplay_lobby_hud_status_text(state),
            }));
        }
        song_lua_capture_new_actors(&mut overlay_proxy_source, &mut actors, overlay_start);
    }

    // The SMX live sensor display and input-driven pad display are positioned
    // relative to each player's notefield (see below, once per-player field
    // geometry has been computed).

    // Fade-to-black when giving up / backing out (Simply Love parity).
    let overlay_start = actors.len();
    if let Some(exit) = &state.exit_prompt_state().exit_transition {
        let alpha = exit_transition_alpha(exit);
        if alpha > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(screen_width(), screen_height()):
                diffuse(0.0, 0.0, 0.0, alpha):
                z(1500)
            ));
        }
    }
    song_lua_capture_new_actors(&mut overlay_proxy_source, &mut actors, overlay_start);

    let notefield_width = |player_idx: usize| -> f32 {
        let Some(ns) = state.noteskin_assets.noteskin[player_idx].as_ref() else {
            return 256.0;
        };
        let receptor_ns = state.noteskin_assets.receptor_noteskin[player_idx]
            .as_deref()
            .unwrap_or(ns);
        let cols = state
            .cols_per_player()
            .min(ns.column_xs.len())
            .min(receptor_ns.receptor_off.len());
        if cols == 0 {
            return 256.0;
        }
        let spacing_mult =
            spacing_multiplier_for_percent(state.profiles()[player_idx].spacing_percent);
        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        for x in ns.column_xs.iter().take(cols) {
            let xf = *x as f32 * spacing_mult;
            min_x = min_x.min(xf);
            max_x = max_x.max(xf);
        }

        // SL parity (GetNotefieldWidth): layout width is style/lane based and must
        // not shrink/grow with Mini (field zoom).
        let target_arrow_px = 64.0;
        let size = receptor_ns.receptor_off[0].size();
        let w = size[0].max(0) as f32;
        let h = size[1].max(0) as f32;
        let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
            w * (target_arrow_px / h)
        } else {
            w
        };
        (max_x - min_x) + arrow_w
    };

    let mut build_player_bundle =
        |player_idx: usize,
         profile: &profile_data::Profile,
         placement: notefield::FieldPlacement,
         requests: SongLuaPlayerProxyRequests| {
            let field_scratch = &mut notefield_actor_scratch[player_idx];
            let hud_scratch = &mut notefield_hud_actor_scratch[player_idx];
            let player_scratch = &mut player_actor_scratch[player_idx];
            let notefield::BuiltNotefield {
                layout_center_x,
                field_actors,
                judgment_actors,
                combo_actors,
            } = notefield::build_bundles(
                state,
                &state.noteskin_assets,
                &state.notefield_model_cache,
                profile,
                placement,
                play_style,
                center_1player_notefield,
                notefield::ProxyCaptureRequests {
                    note_field: requests.note_field,
                    judgment: requests.judgment,
                    combo: requests.combo,
                },
                notefield_view,
                field_scratch,
                hud_scratch,
            );
            let player_actor = &song_lua_visuals.player_actors[player_idx];
            let player_state = song_lua_player_render_state(state, player_idx);
            let player_transform = state.song_lua_player_transform(player_idx);
            let song_lua_active = !state.song().foreground_lua_changes.is_empty();
            let rotation_x = player_state.rot_x_deg + player_transform.rotation_x;
            let rotation_z = player_state.rot_z_deg + player_transform.rotation_z;
            let rotation_y = player_state.rot_y_deg + player_transform.rotation_y;
            let skew_x = player_transform.skew_x;
            let skew_y = player_transform.skew_y;
            let [player_scale_x, player_scale_y] = song_lua_overlay_axis_scale(player_state);
            let player_scale_z = song_lua_overlay_z_scale(player_state);
            let zoom_x = player_scale_x * player_transform.zoom_x;
            let zoom_y = player_scale_y * player_transform.zoom_y;
            let zoom_z = player_scale_z * player_transform.zoom_z;
            let target_x = song_lua_player_target_x(
                player_transform.x,
                player_state.x,
                layout_center_x,
                notefield_view,
            );
            let target_y = player_transform.y.unwrap_or(player_state.y);
            let z_shift = song_lua_player_layer_z(
                song_lua_active,
                player_actor,
                player_state,
                player_transform.z,
            );
            let player_blend = match player_state.blend {
                SongLuaOverlayBlendMode::Alpha => None,
                SongLuaOverlayBlendMode::Add => Some(BlendMode::Add),
                SongLuaOverlayBlendMode::Multiply => Some(BlendMode::Multiply),
                SongLuaOverlayBlendMode::Subtract => Some(BlendMode::Subtract),
            };
            let render_source_bundle = |mut field_bundle, mut hud_bundle| {
                let mut out = Vec::new();
                apply_song_lua_player_transform(
                    &mut field_bundle,
                    &mut hud_bundle,
                    &mut out,
                    z_shift,
                    player_state.diffuse,
                    player_blend,
                    layout_center_x,
                    target_x,
                    target_y,
                    rotation_x,
                    rotation_z,
                    rotation_y,
                    skew_x,
                    skew_y,
                    zoom_x,
                    zoom_y,
                    zoom_z,
                );
                out
            };
            let note_field_source = requests
                .note_field
                .then(|| {
                    render_source_bundle(song_lua_owned_segment_actors(field_actors), Vec::new())
                })
                .and_then(|actors| song_lua_player_child_proxy_source(actors, target_x, target_y));
            apply_song_lua_player_transform(
                field_scratch,
                hud_scratch,
                player_scratch,
                z_shift,
                player_state.diffuse,
                player_blend,
                layout_center_x,
                target_x,
                target_y,
                rotation_x,
                rotation_z,
                rotation_y,
                skew_x,
                skew_y,
                zoom_x,
                zoom_y,
                zoom_z,
            );
            let player_source = if requests.player {
                let source = song_lua_share_actor_source_in_place(player_scratch);
                if !player_state.visible {
                    player_scratch.clear();
                }
                source
            } else {
                if !player_state.visible {
                    player_scratch.clear();
                }
                None
            };
            let proxy_sources = [
                note_field_source,
                judgment_actors
                    .map(song_lua_shared_segment_actors)
                    .map(|actors| render_source_bundle(Vec::new(), actors))
                    .and_then(|actors| {
                        song_lua_player_child_proxy_source(actors, target_x, target_y)
                    }),
                combo_actors
                    .map(song_lua_shared_segment_actors)
                    .map(|actors| render_source_bundle(Vec::new(), actors))
                    .and_then(|actors| {
                        song_lua_player_child_proxy_source(actors, target_x, target_y)
                    }),
            ];
            (layout_center_x, player_source, proxy_sources)
        };

    let (
        has_p2_actors,
        p1_player_proxy_source,
        p2_player_proxy_source,
        p1_proxy_sources,
        p2_proxy_sources,
        playfield_center_x,
        per_player_fields,
    ): (
        bool,
        Option<Vec<Arc<[Actor]>>>,
        Option<Vec<Arc<[Actor]>>>,
        [Option<Vec<Arc<[Actor]>>>; 3],
        [Option<Vec<Arc<[Actor]>>>; 3],
        f32,
        [(usize, f32); 2],
    ) = match play_style {
        profile_data::PlayStyle::Versus => {
            let (p1_x, p1_player_source, p1_sources) = build_player_bundle(
                0,
                &state.profiles()[0],
                notefield::FieldPlacement::P1,
                proxy_requests.players[0],
            );
            let (p2_x, p2_player_source, p2_sources) = build_player_bundle(
                1,
                &state.profiles()[1],
                notefield::FieldPlacement::P2,
                proxy_requests.players[1],
            );
            (
                true,
                p1_player_source,
                p2_player_source,
                p1_sources,
                p2_sources,
                p1_x,
                [(0, p1_x), (1, p2_x)],
            )
        }
        _ => {
            let placement = if is_p2_single {
                notefield::FieldPlacement::P2
            } else {
                notefield::FieldPlacement::P1
            };
            let (nf_x, nf_player_source, nf_sources) = build_player_bundle(
                0,
                &state.profiles()[0],
                placement,
                proxy_requests.players[0],
            );
            player_actor_scratch[1].clear();
            (
                false,
                nf_player_source,
                None,
                nf_sources,
                [None, None, None],
                nf_x,
                [(0, nf_x), (usize::MAX, 0.0)],
            )
        }
    };
    let replacement_proxy_sources = [
        SongLuaPlayerProxySources {
            player: p1_player_proxy_source.as_deref(),
            note_field: p1_proxy_sources[0].as_deref(),
            judgment: p1_proxy_sources[1].as_deref(),
            combo: p1_proxy_sources[2].as_deref(),
        },
        SongLuaPlayerProxySources {
            player: p2_player_proxy_source.as_deref(),
            note_field: p2_proxy_sources[0].as_deref(),
            judgment: p2_proxy_sources[1].as_deref(),
            combo: p2_proxy_sources[2].as_deref(),
        },
    ];
    let replacement_active_players = song_lua_replacement_active_players(
        &song_lua_visuals.overlays,
        &song_lua_overlay_state_scratch,
        &replacement_proxy_sources,
    );

    // Danger overlay (Simply Love parity): red flashing in danger + green recovery, optional HideDanger.
    if !hide_gameplay_hud {
        let underlay_start = actors.len();
        let sw = screen_width();
        let sh = screen_height();
        let cx = screen_center_x();

        for player_idx in 0..state.num_players() {
            let hide_lifebar = state
                .profile(player_idx)
                .is_none_or(|profile| profile.hide_lifebar);
            let Some(rgba) = state.danger_overlay_rgba(player_idx, hide_lifebar) else {
                continue;
            };
            let (x, w, fl, fr) = match play_style {
                profile_data::PlayStyle::Double => (0.0, sw, 0.0, 0.0),
                profile_data::PlayStyle::Versus => {
                    if player_idx == 0 {
                        (0.0, cx, 0.0, 0.1)
                    } else {
                        (cx, sw - cx, 0.1, 0.0)
                    }
                }
                profile_data::PlayStyle::Single => {
                    if centered_single_notefield {
                        (0.0, sw, 0.0, 0.0)
                    } else if is_p2_single {
                        (cx, sw - cx, 0.1, 0.0)
                    } else {
                        (0.0, cx, 0.0, 0.1)
                    }
                }
            };

            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, 0.0):
                zoomto(w, sh):
                fadeleft(fl): faderight(fr):
                diffuse(rgba[0], rgba[1], rgba[2], rgba[3]):
                z(-99)
            ));
        }
        song_lua_capture_new_actors(&mut underlay_proxy_source, &mut actors, underlay_start);
    }

    // Background filter per-player (Simply Love parity): draw behind each notefield, not full-screen.
    let underlay_start = actors.len();
    for &(player_idx, field_x) in &per_player_fields {
        if player_idx == usize::MAX || player_idx >= state.num_players() {
            continue;
        }
        let filter_alpha = state.profiles()[player_idx].background_filter.alpha();
        if filter_alpha <= 0.0 {
            continue;
        }
        actors.push(act!(quad:
            align(0.5, 0.5): xy(field_x, screen_center_y()):
            zoomto(notefield_width(player_idx), screen_height()):
            diffuse(0.0, 0.0, 0.0, filter_alpha):
            z(-99)
        ));
    }
    song_lua_capture_new_actors(&mut underlay_proxy_source, &mut actors, underlay_start);

    // Simply Love parity: BGAnimations/ScreenGameplay underlay/Shared/Header.lua.
    // This top strip sits underneath the UpperNPSGraph and other HUD actors.
    if !hide_gameplay_hud {
        let underlay_start = actors.len();
        let header_rgba = gameplay_header_rgba(cfg.gameplay_bg_color);
        actors.push(act!(quad:
            align(0.5, 0.0): xy(screen_center_x(), 0.0):
            setsize(screen_width(), 80.0):
            diffuse(header_rgba[0], header_rgba[1], header_rgba[2], header_rgba[3]):
            z(83)
        ));
        song_lua_capture_new_actors(&mut underlay_proxy_source, &mut actors, underlay_start);
    }

    actors.reserve(
        player_actor_scratch[0]
            .len()
            .saturating_add(player_actor_scratch[1].len())
            .saturating_add(48),
    );
    if has_p2_actors {
        if !replacement_active_players[1] {
            actors.extend(player_actor_scratch[1].drain(..));
        } else {
            player_actor_scratch[1].clear();
        }
    }
    if !replacement_active_players[0] {
        actors.extend(player_actor_scratch[0].drain(..));
    } else {
        player_actor_scratch[0].clear();
    }
    if !hide_gameplay_hud {
        let underlay_tail_start = actors.len();
        let clamped_width = screen_width().clamp(640.0, 854.0);
        let score_x_p1 = screen_center_x() - clamped_width / 4.3;
        let score_x_p2 = screen_center_x() + clamped_width / 2.75;
        let diff_x_p1 = screen_center_x() - widescale(292.5, 342.5);
        let diff_x_p2 = screen_center_x() + widescale(292.5, 342.5);

        let mut players = [(0usize, profile_data::PlayerSide::P1, 0.0, 0.0, 0.0, 0.0); 2];
        let player_count = match play_style {
            profile_data::PlayStyle::Versus => {
                players[0] = (
                    0,
                    profile_data::PlayerSide::P1,
                    per_player_fields[0].1,
                    diff_x_p1,
                    score_x_p1,
                    score_x_p2,
                );
                players[1] = (
                    1,
                    profile_data::PlayerSide::P2,
                    per_player_fields[1].1,
                    diff_x_p2,
                    score_x_p2,
                    score_x_p1,
                );
                2
            }
            _ if is_p2_single => {
                players[0] = (
                    0,
                    profile_data::PlayerSide::P2,
                    per_player_fields[0].1,
                    diff_x_p2,
                    score_x_p2,
                    score_x_p1,
                );
                1
            }
            _ => {
                players[0] = (
                    0,
                    profile_data::PlayerSide::P1,
                    per_player_fields[0].1,
                    diff_x_p1,
                    score_x_p1,
                    score_x_p2,
                );
                1
            }
        };

        let is_ultrawide = screen_width() / screen_height().max(1.0) > (21.0 / 9.0);
        let graph_center_shift = widescale(45.0, 95.0);
        let graph = state.gameplay.density_graph_view();

        for &(player_idx, player_side, field_x, _, _, _) in &players[..player_count] {
            if !state.profiles()[player_idx].nps_graph_at_top {
                continue;
            }
            let graph_w = graph.top_w[player_idx];
            let graph_h = graph.top_h;
            let graph_mesh_h = graph.top_mesh_h(player_idx);
            if graph_w <= 0.0 || graph_h <= 0.0 || graph_mesh_h <= 0.0 {
                continue;
            }
            let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
            let x = if note_field_is_centered {
                screen_center_x() - graph_w * 0.5
            } else if player_side == profile_data::PlayerSide::P1 {
                screen_center_x() - graph_w - graph_center_shift
            } else {
                screen_center_x() + graph_center_shift
            };
            let y_bottom = 71.0;
            let y_top = y_bottom - graph_h;
            let y_mesh_top = y_bottom - graph_mesh_h;
            let graph_bg_alpha = if state.profiles()[player_idx].transparent_density_graph_bg {
                0.5
            } else {
                1.0
            };

            actors.push(act!(quad:
                align(0.0, 0.0): xy(x, y_top):
                zoomto(graph_w, graph_h):
                diffuse(30.0 / 255.0, 40.0 / 255.0, 47.0 / 255.0, graph_bg_alpha):
                z(84)
            ));

            if let Some(mesh) = &state.density_graph.top_mesh[player_idx]
                && !mesh.is_empty()
            {
                actors.push(Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [x, y_mesh_top],
                    size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_mesh_h)],
                    vertices: mesh.clone(),
                    visible: true,
                    blend: BlendMode::Alpha,
                    z: 85,
                });
            }

            let duration = (graph.last_second - graph.first_second).max(0.001_f32);
            let progress_w =
                (((state.current_music_time_display() - graph.first_second) / duration) * graph_w)
                    .clamp(0.0, graph_w);
            if progress_w > 0.0 {
                actors.push(act!(quad:
                    align(0.0, 0.0): xy(x, y_top):
                    zoomto(progress_w, graph_h):
                    diffuse(0.0, 0.0, 0.0, 0.85):
                    z(86)
                ));
            }
        }

        // SMX overlays are placed relative to each player's notefield, mirrored
        // by side: the FSR sensor display sits just outside the notefield's outer
        // edge (P1: left, P2: right) and the input mini-pad just outside the inner
        // edge (P1: right, P2: left). Build per-slot geometry (side + edges) here,
        // where the notefield layout is known.
        if cfg.smx_input {
            let is_doubles = play_style == profile_data::PlayStyle::Double;
            let is_centered_single = centered_single_notefield;
            let mut field_geom: [Option<(profile_data::PlayerSide, f32, f32)>; 2] = [None, None];
            for &(player_idx, player_side, field_x, ..) in &players[..player_count] {
                if player_idx < 2 {
                    let half_w = notefield_width(player_idx) * 0.5;
                    field_geom[player_idx] =
                        Some((player_side, field_x - half_w, field_x + half_w));
                }
            }
            if state.profiles()[0].smx_fsr_display || state.profiles()[1].smx_fsr_display {
                smx_profile::time_draw(|| {
                    push_smx_sensor_display(
                        &mut actors,
                        state,
                        &field_geom,
                        is_doubles,
                        is_centered_single,
                    )
                });
            }
            if state.profiles()[0].smx_pad_input_display
                || state.profiles()[1].smx_pad_input_display
            {
                push_smx_pad_input_display(
                    &mut actors,
                    state,
                    &field_geom,
                    is_doubles,
                    is_centered_single,
                );
            }
        }

        for &(player_idx, player_side, field_x, diff_x, score_x_normal, score_x_other) in
            &players[..player_count]
        {
            let profile = &state.profiles()[player_idx];
            let diff_x = difficulty_meter_x(
                state,
                profile,
                player_idx,
                player_side,
                field_x,
                notefield_width(player_idx),
                diff_x,
            );
            let chart = &state.charts()[player_idx];
            let difficulty_color =
                color::difficulty_rgba(&chart.difficulty, state.active_color_index());
            let meter_text = cached_meter_text(chart.meter);
            let meter_detail_text = color::difficulty_display_name_for_song(
                &chart.difficulty,
                &state.song().title,
                true,
            );

            // Difficulty Box
            let y = DIFFICULTY_METER_Y;
            actors.push(act!(quad:
                align(0.5, 0.5): xy(diff_x, y): zoomto(30.0, 30.0):
                diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0):
                z(90)
            ));
            let meter_y = if cfg.zmod_rating_box_text { -4.0 } else { 0.0 };
            actors.push(act!(text:
                font(current_machine_font_key(FontRole::Header)): settext(meter_text): align(0.5, 0.5): xy(diff_x, y + meter_y):
                zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0): z(90)
            ));
            if cfg.zmod_rating_box_text {
                actors.push(act!(text:
                    font("miso"):
                    settext(meter_detail_text):
                    align(0.5, 0.5): xy(diff_x, y + 9.5):
                    zoom(0.5):
                    diffuse(0.0, 0.0, 0.0, 1.0):
                    z(90)
                ));
            }

            // Score Display
            let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
            let nps_graph_at_top = state.profiles()[player_idx].nps_graph_at_top;
            let single_score_swapped = state.num_players() == 1
                && play_style != profile_data::PlayStyle::Double
                && nps_graph_at_top
                && !note_field_is_centered;
            let score_in_single_step_stats = profile.score_position
                == profile_data::ScorePosition::StepStatistics
                && !profile.step_statistics.is_empty()
                && play_style == profile_data::PlayStyle::Single
                && state.num_cols() <= 4;
            let score_in_versus_step_stats = profile.score_position
                == profile_data::ScorePosition::StepStatistics
                && !profile.step_statistics.is_empty()
                && play_style == profile_data::PlayStyle::Versus
                && is_wide()
                && !is_ultrawide;
            let step_stats_score_pos = if score_in_single_step_stats {
                Some(step_stats_score_pos(
                    player_side,
                    score_x_other,
                    note_field_is_centered,
                ))
            } else {
                None
            };
            let score_x = if let Some(pos) = step_stats_score_pos {
                pos.score_x
            } else if single_score_swapped {
                score_x_other
            } else {
                score_x_normal
            };
            let score_y = step_stats_score_pos.map_or(56.0, |pos| pos.score_y);
            let score_zoom = step_stats_score_pos.map_or(0.5, |_| 0.2);
            let hide_score_for_top_graph =
                state.num_players() > 1 && nps_graph_at_top && !is_ultrawide;

            if !profile.hide_score && !hide_score_for_top_graph && !score_in_versus_step_stats {
                let show_ex_score = profile.show_ex_score;
                let show_hard_ex_score = show_ex_score && profile.show_hard_ex_score;
                let (score_text, score_color) = if show_ex_score {
                    let blue_window_ms = player_blue_window_ms(state, player_idx);
                    let ex_percent = state.display_gameplay_ex_score_percent(
                        player_idx,
                        score_display_mode_from_profile(profile.score_display_mode),
                        blue_window_ms,
                    );
                    (
                        cached_score_2dp(ex_percent.max(0.0)),
                        color::JUDGMENT_RGBA[0],
                    )
                } else {
                    let score_percent = state.display_gameplay_itg_score_percent(
                        player_idx,
                        score_display_mode_from_profile(profile.score_display_mode),
                    );
                    (cached_score_2dp(score_percent), [1.0, 1.0, 1.0, 1.0])
                };

                let is_p2_side = player_side == profile_data::PlayerSide::P2;
                // Arrow Cloud parity: EX remains the "normal" score position/anchor.
                // H.EX is placed at a different x on P2 so it appears to the left of EX.
                actors.push(act!(text:
                    font(current_machine_font_key(FontRole::Numbers)): settext(score_text):
                    align(1.0, 1.0): xy(score_x, score_y):
                    zoom(score_zoom): horizalign(right):
                    diffuse(score_color[0], score_color[1], score_color[2], score_color[3]):
                    z(90)
                ));

                if show_hard_ex_score {
                    let blue_window_ms = player_blue_window_ms(state, player_idx);
                    let hard_ex_percent = state.display_gameplay_hard_ex_score_percent(
                        player_idx,
                        score_display_mode_from_profile(profile.score_display_mode),
                        blue_window_ms,
                    );
                    let hex = color::HARD_EX_SCORE_RGBA;
                    let (hard_ex_x, hard_ex_y) = if let Some(pos) = step_stats_score_pos {
                        (pos.hard_ex_x, pos.hard_ex_y)
                    } else if single_score_swapped {
                        let swapped_base = if is_p2_side {
                            screen_center_x() - clamped_width / 4.3
                        } else {
                            screen_center_x() + clamped_width / 4.3
                        };
                        (swapped_base + 115.0, score_y)
                    } else if is_p2_side {
                        // Arrow Cloud: HardEX uses /4.3 on P2 (while EX uses /2.75).
                        (screen_center_x() + clamped_width / 4.3, score_y)
                    } else {
                        (score_x, score_y)
                    };
                    let hard_ex_zoom = step_stats_score_pos.map_or(0.25, |_| 0.13);

                    if is_p2_side {
                        actors.push(act!(text:
                            font(current_machine_font_key(FontRole::Numbers)):
                            settext(cached_score_2dp(hard_ex_percent.max(0.0))):
                            align(1.0, 0.0): xy(hard_ex_x, hard_ex_y):
                            zoom(hard_ex_zoom): horizalign(right):
                            diffuse(hex[0], hex[1], hex[2], hex[3]):
                            z(90)
                        ));
                    } else {
                        actors.push(act!(text:
                            font(current_machine_font_key(FontRole::Numbers)):
                            settext(cached_score_2dp(hard_ex_percent.max(0.0))):
                            align(0.0, 0.0): xy(hard_ex_x, hard_ex_y):
                            zoom(hard_ex_zoom): horizalign(left):
                            diffuse(hex[0], hex[1], hex[2], hex[3]):
                            z(90)
                        ));
                    }
                }
            }
        }
        // Current BPM Display (1:1 with Simply Love)
        {
            let base_bpm = state
                .timing()
                .get_bpm_for_beat(state.current_beat_display());
            let music_rate = state.music_rate();
            let rate = if music_rate.is_finite() {
                music_rate as f64
            } else {
                1.0
            };
            let display_bpm = if base_bpm.is_finite() {
                f64::from(base_bpm) * rate
            } else {
                0.0
            };
            let bpm_text = cached_bpm_text(display_bpm, cfg.show_bpm_decimal);
            // Final world-space positions derived from analyzing the SM Lua transforms.
            // The parent frame is bottom-aligned to y=52, and its children are positioned
            // relative to that y-coordinate, with a zoom of 1.33 applied to the whole group.
            let frame_origin_y = 51.0;
            let frame_zoom = 1.33;
            // The BPM text is at y=0 relative to the frame's origin. Its final position is just the origin.
            let bpm_center_y = frame_origin_y;
            // The Rate text is at y=12 relative to the frame's origin. Its offset is scaled by the frame's zoom.
            let rate_center_y = 12.0f64.mul_add(frame_zoom, frame_origin_y);
            let bpm_final_zoom = 1.0 * frame_zoom;
            let rate_final_zoom = 0.5 * frame_zoom;
            let mut bpm_x = screen_center_x();
            let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
            if state.num_players() == 1
                && note_field_is_centered
                && state.profiles()[0].nps_graph_at_top
            {
                let side_shift = if player_side == profile_data::PlayerSide::P1 {
                    0.3
                } else {
                    -0.3
                };
                bpm_x = screen_center_x() + screen_width() * side_shift;
            }
            actors.push(act!(text:
                font("miso"): settext(bpm_text):
                align(0.5, 0.5): xy(bpm_x, bpm_center_y):
                zoom(bpm_final_zoom): horizalign(center): z(90)
            ));
            let music_rate = state.music_rate();
            let rate = if music_rate.is_finite() {
                music_rate
            } else {
                1.0
            };
            let rate_text = cached_rate_text(rate);
            actors.push(act!(text:
                font("miso"): settext(rate_text):
                align(0.5, 0.5): xy(bpm_x, rate_center_y):
                zoom(rate_final_zoom): horizalign(center): z(90)
            ));
        }
        // Song Title Box (SongMeter)
        {
            let w = widescale(310.0, 417.0);
            let h = 22.0;
            let box_cx = screen_center_x();
            let box_cy = 20.0;
            let box_left = box_cx - w * 0.5;
            actors.push(act!(quad:
                align(0.5, 0.5): xy(box_cx, box_cy): zoomto(w, h):
                diffuse(1.0, 1.0, 1.0, 1.0): z(90)
            ));
            actors.push(act!(quad:
                align(0.5, 0.5): xy(box_cx, box_cy): zoomto(w - 4.0, h - 4.0):
                diffuse(0.0, 0.0, 0.0, 1.0): z(91)
            ));
            let progress = song_meter_progress(
                song_time_ns_to_seconds(state.current_music_time_ns()),
                state.song().precise_first_second(),
                state.song().precise_last_second(),
            );
            if progress > f32::EPSILON {
                actors.push(act!(quad:
                    align(0.0, 0.5): xy(box_left + 2.0, box_cy): zoomto((w - 4.0) * progress, h - 4.0):
                    diffuse(player_color[0], player_color[1], player_color[2], 1.0): z(92)
                ));
            }
            let full_title = state.song_full_title.clone();
            actors.push(act!(text:
                font("miso"): settext(full_title): align(0.5, 0.5): xy(box_cx, box_cy):
                zoom(0.8): shadowlength(0.6): maxwidth(screen_width() / 2.5 - 10.0):
                horizalign(center): z(93)
            ));
        }
        // --- Life Meter ---
        {
            let player_life_color = |player_idx: usize| -> [f32; 4] {
                match play_style {
                    profile_data::PlayStyle::Versus => {
                        if player_idx == 0 {
                            color::decorative_rgba(state.active_color_index())
                        } else {
                            color::decorative_rgba(state.active_color_index() - 2)
                        }
                    }
                    _ => {
                        if is_p2_single {
                            color::decorative_rgba(state.active_color_index() - 2)
                        } else {
                            color::decorative_rgba(state.active_color_index())
                        }
                    }
                }
            };
            let rainbow_life_color = |elapsed: f32| -> [f32; 4] {
                let phase = elapsed * 2.0;
                let r = (phase + 0.0).sin() * 0.5 + 0.5;
                let g = (phase + std::f32::consts::TAU / 3.0).sin() * 0.5 + 0.5;
                let b = (phase + (2.0 * std::f32::consts::TAU) / 3.0).sin() * 0.5 + 0.5;
                [r, g, b, 1.0]
            };
            let responsive_life_color = |life: f32| -> [f32; 4] {
                let life = life.clamp(0.0, 1.0);
                if life >= 0.9 {
                    [0.0, 1.0, ((life - 0.9) * 10.0).clamp(0.0, 1.0), 1.0]
                } else if life >= 0.5 {
                    [((0.9 - life) * 2.5).clamp(0.0, 1.0), 1.0, 0.0, 1.0]
                } else {
                    [1.0, ((life - 0.2) * (10.0 / 3.0)).clamp(0.0, 1.0), 0.0, 1.0]
                }
            };
            let fill_life_color = |player_idx: usize, life: f32, dead: bool| -> [f32; 4] {
                let profile = &state.profiles()[player_idx];
                let is_hot = !dead && life >= 1.0;
                if is_hot {
                    if profile.rainbow_max {
                        rainbow_life_color(state.total_elapsed_in_screen())
                    } else {
                        [1.0, 1.0, 1.0, 1.0]
                    }
                } else if profile.responsive_colors {
                    responsive_life_color(life)
                } else {
                    player_life_color(player_idx)
                }
            };
            let show_standard_life_percent =
                screen_width() / screen_height().max(1.0) >= (16.0 / 9.0);

            let mut life_players = [(0usize, profile_data::PlayerSide::P1); 2];
            let life_player_count = match play_style {
                profile_data::PlayStyle::Versus => {
                    life_players[0] = (0, profile_data::PlayerSide::P1);
                    life_players[1] = (1, profile_data::PlayerSide::P2);
                    2
                }
                _ if is_p2_single => {
                    life_players[0] = (0, profile_data::PlayerSide::P2);
                    1
                }
                _ => {
                    life_players[0] = (0, profile_data::PlayerSide::P1);
                    1
                }
            };

            for &(player_idx, side) in &life_players[..life_player_count] {
                if state.profiles()[player_idx].hide_lifebar {
                    continue;
                }

                // Latch-to-zero for rendering the very frame we die.
                let player = &state.players()[player_idx];
                let dead = player.is_failing || player.life <= 0.0;
                let life_for_render = if dead {
                    0.0
                } else {
                    player.life.clamp(0.0, 1.0)
                };
                let is_hot = !dead && life_for_render >= 1.0;
                let life_color = fill_life_color(player_idx, life_for_render, dead);
                let life_percent = life_for_render * 100.0;
                let life_percent_text = cached_life_percent_text(life_percent);

                let lifebar_center_shift = if centered_single_notefield {
                    let clamped_width = screen_width().clamp(640.0, 854.0);
                    match side {
                        profile_data::PlayerSide::P1 => clamped_width * 0.25,
                        profile_data::PlayerSide::P2 => -clamped_width * 0.25,
                    }
                } else {
                    0.0
                };

                match state.profiles()[player_idx].lifemeter_type {
                    profile_data::LifeMeterType::Standard => {
                        let w = 136.0;
                        let h = 18.0;
                        let meter_cy = 20.0;
                        let meter_cx = screen_center_x()
                            + match play_style {
                                profile_data::PlayStyle::Versus => match side {
                                    profile_data::PlayerSide::P1 => -widescale(238.0, 288.0),
                                    profile_data::PlayerSide::P2 => widescale(238.0, 288.0),
                                },
                                _ => match side {
                                    profile_data::PlayerSide::P1 => -widescale(238.0, 288.0),
                                    profile_data::PlayerSide::P2 => widescale(238.0, 288.0),
                                },
                            };

                        // Frames/border
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0):
                            diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                        ));
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h):
                            diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                        ));

                        let filled_width = w * life_for_render;
                        // Never draw swoosh if dead OR nothing to fill.
                        if filled_width > 0.0 && !dead {
                            // Logic Parity:
                            // velocity = -(songposition:GetCurBPS() * 0.5)
                            // if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
                            let bps = state
                                .timing()
                                .get_bpm_for_beat(state.current_beat_display())
                                / 60.0;
                            let velocity_x = if state.beat_phase_paused() {
                                0.0
                            } else {
                                -(bps * 0.5)
                            };

                            let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                            // MeterSwoosh
                            actors.push(act!(sprite("swoosh.png"):
                                align(0.0, 0.5):
                                xy(meter_cx - w / 2.0, meter_cy):
                                zoomto(filled_width, h):
                                diffusealpha(swoosh_alpha):
                                texcoordvelocity(velocity_x, 0.0):
                                z(93)
                            ));

                            // MeterFill
                            actors.push(act!(quad:
                                align(0.0, 0.5):
                                xy(meter_cx - w / 2.0, meter_cy):
                                zoomto(filled_width, h):
                                diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                                z(92)
                            ));
                        }

                        if state.profiles()[player_idx].show_life_percent
                            && show_standard_life_percent
                            && !is_hot
                        {
                            let life_text_color = player_life_color(player_idx);
                            let (outer_x, inner_x, text_x, align_x) =
                                if side == profile_data::PlayerSide::P1 {
                                    (meter_cx - 76.0, meter_cx - 77.0, meter_cx - 77.0, 1.0)
                                } else {
                                    (meter_cx + 76.0, meter_cx + 77.0, meter_cx + 78.0, 0.0)
                                };
                            actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, meter_cy):
                            zoomto(44.0, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(94)
                        ));
                            actors.push(act!(quad:
                                align(align_x, 0.5): xy(inner_x, meter_cy):
                                zoomto(42.0, 16.0):
                                diffuse(0.0, 0.0, 0.0, 1.0):
                                z(95)
                            ));
                            actors.push(act!(text:
                            font("miso"): settext(life_percent_text.clone()):
                            align(align_x, 0.5): xy(text_x, meter_cy):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(96)
                        ));
                        }
                    }
                    profile_data::LifeMeterType::Surround => {
                        let sw = screen_width();
                        let sh = screen_height();
                        let w = sw * 0.5;
                        let h = sh - 80.0;
                        let y = 80.0;
                        let croptop = 1.0 - life_for_render;

                        if play_style == profile_data::PlayStyle::Double {
                            // Double: two quads flanking left/right, moving in unison.
                            actors.push(act!(quad:
                                align(0.0, 0.0): xy(0.0, y):
                                zoomto(w, h):
                                diffuse(0.2, 0.2, 0.2, 1.0):
                                faderight(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                            actors.push(act!(quad:
                                align(1.0, 0.0): xy(sw, y):
                                zoomto(w, h):
                                diffuse(0.2, 0.2, 0.2, 1.0):
                                fadeleft(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                            // Only one player in Double style.
                            break;
                        }

                        let mut surround_color = if state.profiles()[player_idx].responsive_colors {
                            let mut c = responsive_life_color(life_for_render);
                            c[3] = 0.2;
                            c
                        } else {
                            [0.2, 0.2, 0.2, 1.0]
                        };
                        if life_for_render >= 1.0 && state.profiles()[player_idx].rainbow_max {
                            let mut c = rainbow_life_color(state.total_elapsed_in_screen());
                            c[3] = if state.profiles()[player_idx].responsive_colors {
                                0.2
                            } else {
                                1.0
                            };
                            surround_color = c;
                        }

                        match side {
                            profile_data::PlayerSide::P1 => {
                                actors.push(act!(quad:
                                align(0.0, 0.0): xy(0.0, y):
                                zoomto(w + lifebar_center_shift, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                faderight(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                            }
                            profile_data::PlayerSide::P2 => {
                                actors.push(act!(quad:
                                align(1.0, 0.0): xy(sw, y):
                                zoomto(w - lifebar_center_shift, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                fadeleft(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                            }
                        }
                    }
                    profile_data::LifeMeterType::Vertical => {
                        let bar_w = 16.0;
                        let bar_h = 250.0;

                        let x = {
                            // SL: default to _screen.cx +/- SL_WideScale(302, 400).
                            let mut x = screen_center_x()
                                + match side {
                                    profile_data::PlayerSide::P1 => -widescale(302.0, 400.0),
                                    profile_data::PlayerSide::P2 => widescale(302.0, 400.0),
                                };

                            // SL: if double style, position next to notefield.
                            if play_style == profile_data::PlayStyle::Double {
                                let half_nf = notefield_width(player_idx) * 0.5;
                                x = screen_center_x()
                                    + match side {
                                        profile_data::PlayerSide::P1 => -(half_nf + 10.0),
                                        profile_data::PlayerSide::P2 => half_nf + 10.0,
                                    };
                            }

                            x + lifebar_center_shift
                        };

                        let cy = bar_h + 10.0;
                        // Frames/border
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x, cy): zoomto(bar_w + 2.0, bar_h + 2.0):
                            diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                        ));
                        actors.push(act!(quad:
                            align(0.5, 0.5): xy(x, cy): zoomto(bar_w, bar_h):
                            diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                        ));

                        let filled_h = bar_h * life_for_render;

                        // MeterFill
                        if filled_h > 0.0 {
                            actors.push(act!(quad:
                                align(0.0, 1.0):
                                xy(x - bar_w * 0.5, cy + bar_h * 0.5):
                                zoomto(bar_w, filled_h):
                                diffuse(life_color[0], life_color[1], life_color[2], 1.0):
                                z(92)
                            ));
                        }

                        // MeterSwoosh
                        if filled_h > 0.0 && !dead {
                            let bps = state
                                .timing()
                                .get_bpm_for_beat(state.current_beat_display())
                                / 60.0;
                            let velocity_x = if state.beat_phase_paused() {
                                0.0
                            } else {
                                -(bps * 0.5)
                            };
                            let swoosh_alpha = if is_hot { 1.0 } else { 0.2 };

                            actors.push(act!(sprite("swoosh.png"):
                                align(0.5, 0.5):
                                xy(x, (cy + bar_h * 0.5) - filled_h * 0.5):
                                zoomto(filled_h, bar_w):
                                diffusealpha(swoosh_alpha):
                                rotationz(90.0):
                                texcoordvelocity(velocity_x, 0.0):
                                z(93)
                            ));
                        }

                        if state.profiles()[player_idx].show_life_percent && !is_hot {
                            let life_text_color = player_life_color(player_idx);
                            let text_y = cy + bar_h * 0.5 - (bar_h * life_for_render);
                            let (outer_x, inner_x, text_x, align_x) =
                                if side == profile_data::PlayerSide::P1 {
                                    (x + 10.0, x + 11.0, x + 12.0, 0.0)
                                } else {
                                    (x - 11.0, x - 12.0, x - 13.0, 1.0)
                                };
                            actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, text_y):
                            zoomto(44.0, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(94)
                        ));
                            actors.push(act!(quad:
                                align(align_x, 0.5): xy(inner_x, text_y):
                                zoomto(42.0, 16.0):
                                diffuse(0.0, 0.0, 0.0, 1.0):
                                z(95)
                            ));
                            actors.push(act!(text:
                            font("miso"): settext(life_percent_text.clone()):
                            align(align_x, 0.5): xy(text_x, text_y):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], 1.0):
                            z(96)
                        ));
                        }
                    }
                }
            }
        }
        // Simply Love parity: keep Stage/Event text visible at the footer after intro animation ends.
        // On a song restart we skip the splode/text in-transition entirely, so make the footer
        // label appear immediately rather than waiting `INTRO_TEXT_SETTLE_SECONDS` of dead time.
        let intro_text = state.stage_intro_text.as_ref();
        let is_restart_label = intro_text.starts_with("RESTART ");
        if !intro_text.is_empty()
            && (is_restart_label || state.total_elapsed_in_screen() >= INTRO_TEXT_SETTLE_SECONDS)
        {
            let text_x = intro_text_target_x(
                state,
                asset_manager,
                state.stage_intro_text.as_ref(),
                play_style,
                player_side,
                cfg.center_1player_notefield,
            );
            actors.push(act!(text:
            font(current_machine_font_key(FontRole::Header)): settext(state.stage_intro_text.clone()):
            align(0.5, 0.5): xy(text_x, screen_height() - 30.0):
            zoom(0.4):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(110)
        ));
        }
        let p1_avatar = hud_snapshot
            .p1
            .avatar_texture_key
            .as_deref()
            .map(|texture_key| AvatarParams { texture_key });
        let p2_avatar = hud_snapshot
            .p2
            .avatar_texture_key
            .as_deref()
            .map(|texture_key| AvatarParams { texture_key });

        let p1_joined = hud_snapshot.p1.joined;
        let p2_joined = hud_snapshot.p2.joined;
        let p1_guest = hud_snapshot.p1.guest;
        let p2_guest = hud_snapshot.p2.guest;

        let (p1_footer_text, p1_footer_avatar) = if p1_joined {
            (
                Some(if p1_guest || hud_snapshot.p1.hide_username {
                    ""
                } else {
                    hud_snapshot.p1.display_name.as_str()
                }),
                if p1_guest { None } else { p1_avatar },
            )
        } else {
            (None, None)
        };
        let (p2_footer_text, p2_footer_avatar) = if p2_joined {
            (
                Some(if p2_guest || hud_snapshot.p2.hide_username {
                    ""
                } else {
                    hud_snapshot.p2.display_name.as_str()
                }),
                if p2_guest { None } else { p2_avatar },
            )
        } else {
            (None, None)
        };

        let (footer_left, footer_right, left_avatar, right_avatar) =
            if play_style == profile_data::PlayStyle::Versus {
                (
                    p1_footer_text,
                    p2_footer_text,
                    p1_footer_avatar,
                    p2_footer_avatar,
                )
            } else {
                match player_side {
                    profile_data::PlayerSide::P1 => (p1_footer_text, None, p1_footer_avatar, None),
                    profile_data::PlayerSide::P2 => (None, p2_footer_text, None, p2_footer_avatar),
                }
            };
        actors.push(screen_bar::build_no_background(ScreenBarParams {
            title: "",
            title_placement: screen_bar::ScreenBarTitlePlacement::Center,
            position: screen_bar::ScreenBarPosition::Bottom,
            transparent: true,
            fg_color: [1.0; 4],
            left_text: footer_left,
            center_text: None,
            right_text: footer_right,
            left_avatar,
            right_avatar,
        }));
        let show_step_stats = match play_style {
            profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => state
                .profiles()
                .first()
                .is_some_and(|p| !p.step_statistics.is_empty()),
            profile_data::PlayStyle::Versus => {
                state
                    .profiles()
                    .first()
                    .is_some_and(|p| !p.step_statistics.is_empty())
                    || state
                        .profiles()
                        .get(1)
                        .is_some_and(|p| !p.step_statistics.is_empty())
            }
        };
        if show_step_stats {
            if state.num_cols() <= 4 && play_style != profile_data::PlayStyle::Versus {
                gameplay_stats::push_step_stats(
                    &mut actors,
                    state,
                    asset_manager,
                    playfield_center_x,
                    player_side,
                );
            } else if play_style == profile_data::PlayStyle::Versus {
                gameplay_stats::push_versus_step_stats(&mut actors, state, asset_manager);
            } else if play_style == profile_data::PlayStyle::Double {
                gameplay_stats::push_double_step_stats(
                    &mut actors,
                    state,
                    asset_manager,
                    playfield_center_x,
                );
            }
        }
        song_lua_capture_new_actors(&mut underlay_proxy_source, &mut actors, underlay_tail_start);
    }
    let song_foreground_state = song_lua_song_foreground_state(state);
    let p1_proxy_slices = [
        p1_proxy_sources[0].as_deref(),
        p1_proxy_sources[1].as_deref(),
        p1_proxy_sources[2].as_deref(),
    ];
    let p2_proxy_slices = [
        p2_proxy_sources[0].as_deref(),
        p2_proxy_sources[1].as_deref(),
        p2_proxy_sources[2].as_deref(),
    ];
    let p1_player_proxy_slice = p1_player_proxy_source.as_deref();
    let p2_player_proxy_slice = p2_player_proxy_source.as_deref();
    let underlay_proxy_slice = underlay_proxy_source.as_deref();
    let overlay_proxy_slice = overlay_proxy_source.as_deref();
    let proxy_sources = SongLuaScreenProxySources {
        players: [
            SongLuaPlayerProxySources {
                player: p1_player_proxy_slice,
                note_field: p1_proxy_slices[0],
                judgment: p1_proxy_slices[1],
                combo: p1_proxy_slices[2],
            },
            SongLuaPlayerProxySources {
                player: p2_player_proxy_slice,
                note_field: p2_proxy_slices[0],
                judgment: p2_proxy_slices[1],
                combo: p2_proxy_slices[2],
            },
        ],
        underlay: underlay_proxy_slice,
        overlay: overlay_proxy_slice,
    };
    let main_layer_actors = {
        let mut out = Vec::new();
        push_song_lua_layer_actors(
            &mut out,
            &song_lua_visuals.overlays,
            &mut song_lua_overlay_order,
            &song_lua_local_state_scratch,
            &song_lua_overlay_state_scratch,
            song_foreground_state,
            &proxy_sources,
            asset_manager,
            song_lua_space_width,
            song_lua_space_height,
            state.current_music_time_display(),
            state.current_beat(),
            state.total_elapsed_in_screen(),
            &mut song_lua_order_scratch,
            &mut song_lua_capture_state_scratch,
            &mut song_lua_capture_order_scratch,
        );
        out
    };
    actors.extend(main_layer_actors);
    if let Some(actor) = build_foreground_media(
        state,
        &song_lua_overlay_state_scratch,
        &mut song_lua_layer_local_state_scratch,
        &mut song_lua_layer_state_scratch,
    ) {
        actors.push(actor);
    }
    for (layer_idx, layer) in song_lua_visuals.foreground_visual_layers.iter().enumerate() {
        if state.current_music_time_display() < layer.start_second {
            continue;
        }
        let Some(order_cache) = song_lua_foreground_visual_layer_orders.get_mut(layer_idx) else {
            continue;
        };
        song_lua_overlay_state_sets_from_into(
            state.current_music_time_display(),
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &mut song_lua_layer_local_state_scratch,
            &mut song_lua_layer_state_scratch,
        );
        let song_foreground_state = song_lua_song_foreground_state_from(
            state.current_music_time_display(),
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
        );
        let layer_actors = {
            let mut out = Vec::new();
            push_song_lua_layer_actors(
                &mut out,
                &layer.overlays,
                order_cache,
                &song_lua_layer_local_state_scratch,
                &song_lua_layer_state_scratch,
                song_foreground_state,
                &proxy_sources,
                asset_manager,
                layer.screen_width.max(1.0),
                layer.screen_height.max(1.0),
                state.current_music_time_display(),
                state.current_beat(),
                state.total_elapsed_in_screen(),
                &mut song_lua_order_scratch,
                &mut song_lua_capture_state_scratch,
                &mut song_lua_capture_order_scratch,
            );
            out
        };
        actors.extend(layer_actors);
    }
    state.song_lua_overlay_order = song_lua_overlay_order;
    state.song_lua_background_visual_layer_orders = song_lua_background_visual_layer_orders;
    state.song_lua_foreground_visual_layer_orders = song_lua_foreground_visual_layer_orders;
    state.song_lua_local_state_scratch = song_lua_local_state_scratch;
    state.song_lua_overlay_state_scratch = song_lua_overlay_state_scratch;
    state.song_lua_layer_local_state_scratch = song_lua_layer_local_state_scratch;
    state.song_lua_layer_state_scratch = song_lua_layer_state_scratch;
    state.song_lua_capture_state_scratch = song_lua_capture_state_scratch;
    state.song_lua_order_scratch = song_lua_order_scratch;
    state.song_lua_capture_order_scratch = song_lua_capture_order_scratch;
    state.notefield_actor_scratch = notefield_actor_scratch;
    state.notefield_hud_actor_scratch = notefield_hud_actor_scratch;
    state.player_actor_scratch = player_actor_scratch;
}

// ─── SMX sensor display profiling ──────────────────────────────────────────────
//
// Opt-in, zero-cost-when-off instrumentation to attribute the FSR visualizer's
// per-frame cost. Enable by running with `DEADSYNC_SMX_PROFILE=1`. Once a second
// it logs the rolling average and max for two regions:
//   read  — the throttled SDK get_test_data call (captures shared-state lock
//           wait + the clone); shows whether lock contention is the cost.
//   draw  — building the bar/text actors each frame.
// `n` is the sample count in the window (read should sit near 60/s after the
// throttle; draw tracks the frame rate).
mod smx_profile {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    struct Bucket {
        sum_ns: AtomicU64,
        max_ns: AtomicU64,
        count: AtomicU64,
    }

    impl Bucket {
        const fn new() -> Self {
            Self {
                sum_ns: AtomicU64::new(0),
                max_ns: AtomicU64::new(0),
                count: AtomicU64::new(0),
            }
        }

        fn record(&self, ns: u64) {
            self.sum_ns.fetch_add(ns, Ordering::Relaxed);
            self.max_ns.fetch_max(ns, Ordering::Relaxed);
            self.count.fetch_add(1, Ordering::Relaxed);
        }

        // Average (µs), max (µs), and sample count over the window, resetting it.
        fn take(&self) -> (f64, f64, u64) {
            let sum = self.sum_ns.swap(0, Ordering::Relaxed);
            let max = self.max_ns.swap(0, Ordering::Relaxed);
            let count = self.count.swap(0, Ordering::Relaxed);
            let avg_us = if count == 0 {
                0.0
            } else {
                sum as f64 / count as f64 / 1000.0
            };
            (avg_us, max as f64 / 1000.0, count)
        }
    }

    static READ: Bucket = Bucket::new();
    static DRAW: Bucket = Bucket::new();

    fn enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("DEADSYNC_SMX_PROFILE").is_ok_and(|v| !v.is_empty() && v != "0")
        })
    }

    fn time<T>(bucket: &Bucket, f: impl FnOnce() -> T) -> T {
        if !enabled() {
            return f();
        }
        let start = Instant::now();
        let out = f();
        bucket.record(start.elapsed().as_nanos() as u64);
        out
    }

    pub fn time_read<T>(f: impl FnOnce() -> T) -> T {
        time(&READ, f)
    }

    pub fn time_draw<T>(f: impl FnOnce() -> T) -> T {
        time(&DRAW, f)
    }

    /// Log the rolling window once a second. Cheap no-op when profiling is off.
    pub fn maybe_report() {
        if !enabled() {
            return;
        }
        static LAST: OnceLock<Mutex<Instant>> = OnceLock::new();
        let clock = LAST.get_or_init(|| Mutex::new(Instant::now()));
        let mut last = clock.lock().unwrap();
        if last.elapsed().as_secs_f32() < 1.0 {
            return;
        }
        *last = Instant::now();
        drop(last);

        let (read_avg, read_max, read_n) = READ.take();
        let (draw_avg, draw_max, draw_n) = DRAW.take();
        // `warn` so this opt-in diagnostic is visible at the default log level.
        log::warn!(
            "smx-profile: read avg={read_avg:.1}us max={read_max:.1}us n={read_n} | \
             draw avg={draw_avg:.1}us max={draw_max:.1}us n={draw_n}"
        );
    }
}

// ─── SMX sensor display ────────────────────────────────────────────────────────

// Gameplay panels in display order (L, D, U, R) matching pad layout.
const SMX_SENSOR_DISP_PANELS: [(usize, &str); 4] = [(3, "L"), (7, "D"), (1, "U"), (5, "R")];
const SMX_SENSOR_BAR_W: f32 = 8.0;
const SMX_SENSOR_BAR_H: f32 = 40.0;
const SMX_SENSOR_BAR_GAP: f32 = 3.0;
const SMX_SENSOR_MARGIN: f32 = 10.0;
// Lift the whole group above the bottom screen-bar footer (BAR_H = 32 in
// screen_bar) and its player avatar so the bars never sit on top of them.
// Kept low enough that the top numeric row clears the vertical life bar.
const SMX_SENSOR_FOOTER_CLEAR: f32 = 26.0;
// Live numeric pressure value sits just above each bar.
const SMX_SENSOR_VALUE_H: f32 = 9.0;
const SMX_SENSOR_VALUE_GAP: f32 = 2.0;
const SMX_SENSOR_VALUE_ZOOM: f32 = 0.28;
// Panel letter (L/D/U/R) drawn on the bar itself, near the bottom.
const SMX_SENSOR_LABEL_ZOOM: f32 = 0.32;
const SMX_SENSOR_LETTER_INSET: f32 = 2.0;
// Drop shadow keeps the letter legible over both the dark track and bright fill.
const SMX_SENSOR_LETTER_SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.9];
const SMX_SENSOR_VALUE_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const SMX_SENSOR_VALUE_IDLE_COLOR: [f32; 4] = [0.7, 0.7, 0.75, 0.6];
// FSR calibrated values are right-shifted by 2, so 0-1000 raw => 0-250 after calibration.
const SMX_SENSOR_VALUE_SCALE: f32 = 250.0;
const SMX_SENSOR_Z: f32 = 2102.0;

const SMX_SENSOR_TRACK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const SMX_SENSOR_FILL_IDLE: [f32; 4] = [0.25, 0.75, 0.25, 0.8];
const SMX_SENSOR_FILL_ACTIVE: [f32; 4] = [1.0, 1.0, 1.0, 0.9];
const SMX_SENSOR_THRESHOLD: [f32; 4] = [1.0, 0.45, 0.0, 1.0];
const SMX_SENSOR_BG: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
// Gaps between a player's notefield edge and an SMX overlay placed beside it.
// Outer = FSR sensor display (away from center); inner = input mini-pad (toward
// center). The doubles branches reuse the outer gap.
const SMX_OVERLAY_FIELD_GAP: f32 = 14.0;
const SMX_OVERLAY_INNER_GAP: f32 = 5.0;
// Extra rightward shift for the P2 FSR group so its outer (R) bar lines up with
// the P2 life meter; the versus notefields are not symmetric about center, so
// the outer gap alone leaves P2 short. Tunable.
const SMX_FSR_P2_NUDGE: f32 = 15.0;

/// X for an SMX overlay of width `w` placed `gap` outside a player's notefield.
/// `outer` = away from screen center (FSR sensor display); otherwise toward
/// center (input mini-pad). Mirrors by player side and clamps to stay on-screen.
fn smx_overlay_x(
    side: profile_data::PlayerSide,
    field_left: f32,
    field_right: f32,
    w: f32,
    outer: bool,
    gap: f32,
) -> f32 {
    let on_left = matches!(
        (side, outer),
        (profile_data::PlayerSide::P1, true) | (profile_data::PlayerSide::P2, false)
    );
    let x = if on_left {
        field_left - gap - w
    } else {
        field_right + gap
    };
    x.clamp(SMX_SENSOR_MARGIN, screen_width() - SMX_SENSOR_MARGIN - w)
}

// Width of one pad's 4-bar FSR group (unscaled).
fn smx_fsr_group_w() -> f32 {
    4.0 * SMX_SENSOR_BAR_W + 3.0 * SMX_SENSOR_BAR_GAP
}

// Enlarged, vertically-stacked layout for a centered single player: a big FSR
// group over a big mini-pad, centered in the open side gutter (P1 left, P2
// right). Returns (scale, fsr_x, fsr_top, mini_x, mini_y).
const SMX_CENTERED_SCALE: f32 = 2.0;
const SMX_CENTERED_STACK_GAP: f32 = 16.0;
// Gap between the two pads' groups in a Doubles pair.
const SMX_DOUBLES_PAIR_GAP: f32 = 10.0;
// Doubles stacks the FSR pair over the mini pair, centered on the playfield, with
// the top of the stack this fraction down the screen (clear of the side gutters
// so negative-Mini notes don't overlap). Tunable.
const SMX_DOUBLES_STACK_TOP_FRAC: f32 = 0.6;
const SMX_DOUBLES_STACK_GAP: f32 = 12.0;
// Extra downward nudge for the Doubles FSR pair only (mini stays put). Tunable.
const SMX_DOUBLES_FSR_Y_OFFSET: f32 = 5.0;
fn smx_centered_layout(
    side: profile_data::PlayerSide,
    field_left: f32,
    field_right: f32,
) -> (f32, f32, f32, f32, f32) {
    let scale = SMX_CENTERED_SCALE;
    let fsr_w = smx_fsr_group_w() * scale;
    let fsr_h = (SMX_SENSOR_VALUE_H + SMX_SENSOR_VALUE_GAP + SMX_SENSOR_BAR_H) * scale;
    let mini_w = (3.0 * SMX_PAD_INPUT_CELL + 2.0 * SMX_PAD_INPUT_GAP) * scale;
    let total_h = fsr_h + SMX_CENTERED_STACK_GAP + mini_w;
    let top_y = screen_center_y() - total_h * 0.5;
    let gutter_center = match side {
        profile_data::PlayerSide::P1 => field_left * 0.5,
        profile_data::PlayerSide::P2 => (field_right + screen_width()) * 0.5,
    };
    (
        scale,
        gutter_center - fsr_w * 0.5,
        top_y,
        gutter_center - mini_w * 0.5,
        top_y + fsr_h + SMX_CENTERED_STACK_GAP,
    )
}

fn push_smx_sensor_display(
    actors: &mut Vec<Actor>,
    state: &State,
    field_geom: &[Option<(profile_data::PlayerSide, f32, f32)>; 2],
    is_doubles: bool,
    is_centered_single: bool,
) {
    let bar_y = screen_height() - SMX_SENSOR_FOOTER_CLEAR - SMX_SENSOR_MARGIN - SMX_SENSOR_BAR_H;
    // Top of the numeric value row that sits above the bars (used for bg + values).
    let group_top = bar_y - SMX_SENSOR_VALUE_GAP - SMX_SENSOR_VALUE_H;
    let pad_group_w = smx_fsr_group_w();

    if is_centered_single {
        // Big FSR group stacked over the mini-pad in the open side gutter.
        for pad in 0..2usize {
            if !state.profiles()[pad].smx_fsr_display {
                continue;
            }
            let Some((side, field_left, field_right)) = field_geom[pad] else {
                continue;
            };
            let (scale, fsr_x, fsr_top, _, _) = smx_centered_layout(side, field_left, field_right);
            draw_smx_fsr_group(actors, state, pad, fsr_x, fsr_top, scale);
        }
        return;
    }

    if is_doubles {
        // One player drives both pads. Show both pad groups (pad 0 left, pad 1
        // right) beside each other, centered on the playfield with the stack top
        // 3/5 down the screen (under the judgement), clear of the gutters. Gated
        // on the doubles player's toggle (profile 0); sensor arrays are keyed by
        // SDK pad here (see on_enter).
        if !state.profiles()[0].smx_fsr_display {
            return;
        }
        let Some((_, field_left, _)) = field_geom[0] else {
            return;
        };
        // Centered in the left gutter (to the left of the wide notefield).
        let center_x = field_left * 0.5;
        let group_gap = SMX_DOUBLES_PAIR_GAP;
        let total_w = pad_group_w * 2.0 + group_gap;
        let start_x = center_x - total_w * 0.5;
        let top_y = screen_height() * SMX_DOUBLES_STACK_TOP_FRAC + SMX_DOUBLES_FSR_Y_OFFSET;
        for sdk_pad in 0..2usize {
            let gx = start_x + sdk_pad as f32 * (pad_group_w + group_gap);
            draw_smx_fsr_group(actors, state, sdk_pad, gx, top_y, 1.0);
        }
        return;
    }

    for pad in 0..2usize {
        if !state.profiles()[pad].smx_fsr_display {
            continue;
        }
        // Place this pad's group just outside the outer edge of its notefield.
        let Some((side, field_left, field_right)) = field_geom[pad] else {
            continue;
        };
        let mut group_x = smx_overlay_x(
            side,
            field_left,
            field_right,
            pad_group_w,
            true,
            SMX_OVERLAY_FIELD_GAP,
        );
        if side == profile_data::PlayerSide::P2 {
            group_x =
                (group_x + SMX_FSR_P2_NUDGE).min(screen_width() - SMX_SENSOR_MARGIN - pad_group_w);
        }
        draw_smx_fsr_group(actors, state, pad, group_x, group_top, 1.0);
    }
}

/// Draws one pad's FSR bar group with its value row top at `group_top`, scaled
/// by `scale`. `idx` indexes the sensor arrays (profile index in non-Doubles
/// modes, SDK pad in Doubles). No-op if no config.
fn draw_smx_fsr_group(
    actors: &mut Vec<Actor>,
    state: &State,
    idx: usize,
    group_x: f32,
    group_top: f32,
    scale: f32,
) {
    let Some(config) = state.smx_sensor_config[idx].as_ref() else {
        return;
    };
    let sensor_data = state.smx_sensor_data[idx].as_ref();
    let fsr = deadsync_smx::is_fsr(config);

    let bar_w = SMX_SENSOR_BAR_W * scale;
    let bar_h = SMX_SENSOR_BAR_H * scale;
    let bar_gap = SMX_SENSOR_BAR_GAP * scale;
    let bar_y = group_top + (SMX_SENSOR_VALUE_H + SMX_SENSOR_VALUE_GAP) * scale;
    let pad_group_w = 4.0 * bar_w + 3.0 * bar_gap;

    // Background behind this pad's label + bar group.
    let bg_pad = 3.0 * scale;
    push_smx_quad(
        actors,
        group_x - bg_pad,
        group_top - bg_pad,
        pad_group_w + bg_pad * 2.0,
        (bar_y + bar_h) - group_top + bg_pad * 2.0,
        SMX_SENSOR_BG,
        SMX_SENSOR_Z - 1.0,
    );

    for (slot, &(panel, label)) in SMX_SENSOR_DISP_PANELS.iter().enumerate() {
        let x = group_x + slot as f32 * (bar_w + bar_gap);

        // Panel high threshold (max across sensors for FSR), computed once and
        // used for both the active check and the threshold line.
        let threshold = if fsr {
            config.panel_settings[panel]
                .fsr_high_threshold
                .iter()
                .map(|&t| u16::from(t))
                .max()
                .unwrap_or(0)
        } else {
            u16::from(config.panel_settings[panel].load_cell_high_threshold)
        };
        let threshold_norm = (threshold as f32 / SMX_SENSOR_VALUE_SCALE).clamp(0.0, 1.0);

        let (value_norm, active, raw_value) = if let Some(data) = sensor_data {
            if data.have_data_from_panel[panel] {
                let max_val = if fsr {
                    data.sensor_level[panel]
                        .iter()
                        .map(|&v| if v <= 0 { 0u16 } else { (v >> 2) as u16 })
                        .max()
                        .unwrap_or(0)
                } else {
                    // Load-cell: no >>2 shift, clamp to 0-500 then scale to 250.
                    data.sensor_level[panel]
                        .iter()
                        .map(|&v| v.max(0).min(500) as u16)
                        .max()
                        .unwrap_or(0)
                };
                let norm = (max_val as f32 / SMX_SENSOR_VALUE_SCALE).clamp(0.0, 1.0);
                (norm, max_val >= threshold && threshold > 0, Some(max_val))
            } else {
                (0.0, false, None)
            }
        } else {
            (0.0, false, None)
        };

        // Track background.
        push_smx_quad(
            actors,
            x,
            bar_y,
            bar_w,
            bar_h,
            SMX_SENSOR_TRACK,
            SMX_SENSOR_Z,
        );

        // Pressure fill from bottom.
        let fill_h = value_norm * bar_h;
        if fill_h > 0.0 {
            let fill = if active {
                SMX_SENSOR_FILL_ACTIVE
            } else {
                SMX_SENSOR_FILL_IDLE
            };
            push_smx_quad(
                actors,
                x,
                bar_y + bar_h - fill_h,
                bar_w,
                fill_h,
                fill,
                SMX_SENSOR_Z + 1.0,
            );
        }

        // Threshold line.
        let threshold_h = 2.0_f32 * scale;
        let threshold_y = bar_y + (1.0 - threshold_norm) * bar_h - threshold_h * 0.5;
        push_smx_quad(
            actors,
            x,
            threshold_y,
            bar_w,
            threshold_h,
            SMX_SENSOR_THRESHOLD,
            SMX_SENSOR_Z + 2.0,
        );

        // Live pressure value centered above the bar (replaces the old letter
        // row); "--" when no sample has arrived for this panel yet.
        let (value_text, value_color) = match raw_value {
            Some(v) => (v.to_string(), SMX_SENSOR_VALUE_COLOR),
            None => ("--".to_string(), SMX_SENSOR_VALUE_IDLE_COLOR),
        };
        actors.push(act!(text:
            font(current_machine_font_key(FontRole::Normal)): settext(value_text):
            align(0.5, 0.0): xy(x + bar_w * 0.5, group_top):
            zoom(SMX_SENSOR_VALUE_ZOOM * scale):
            diffuse(value_color[0], value_color[1], value_color[2], value_color[3]):
            z(SMX_SENSOR_Z + 2.0)
        ));

        // Panel letter (L/D/U/R) drawn on the bar near its bottom; the drop
        // shadow keeps it legible over both the dark track and bright fill.
        actors.push(act!(text:
            font(current_machine_font_key(FontRole::Normal)): settext(label):
            align(0.5, 1.0):
            xy(x + bar_w * 0.5, bar_y + bar_h - SMX_SENSOR_LETTER_INSET * scale):
            zoom(SMX_SENSOR_LABEL_ZOOM * scale):
            shadowlength(1.0):
            shadowcolor(
                SMX_SENSOR_LETTER_SHADOW[0],
                SMX_SENSOR_LETTER_SHADOW[1],
                SMX_SENSOR_LETTER_SHADOW[2],
                SMX_SENSOR_LETTER_SHADOW[3]
            ):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(SMX_SENSOR_Z + 3.0)
        ));
    }
}

fn push_smx_quad(actors: &mut Vec<Actor>, x: f32, y: f32, w: f32, h: f32, c: [f32; 4], z: f32) {
    actors.push(act!(quad:
        align(0.0, 0.0): xy(x, y): zoomto(w, h):
        diffuse(c[0], c[1], c[2], c[3]): z(z)
    ));
}

// ─── SMX pad-input display ──────────────────────────────────────────────────
// A tiny per-pad layout whose panels light up straight from the live inputs we
// receive (like the input tester), independent of the FSR sensor display.

const SMX_PAD_INPUT_CELL: f32 = 9.0;
const SMX_PAD_INPUT_GAP: f32 = 1.5;
// One 4-panel pad as a 3x3 grid: (column offset, grid-x cell, grid-y cell) for
// Left/Down/Up/Right. Column order within a pad is L, D, U, R.
const SMX_PAD_INPUT_PANELS: [(usize, f32, f32); 4] =
    [(0, 0.0, 1.0), (1, 1.0, 2.0), (2, 1.0, 0.0), (3, 2.0, 1.0)];
const SMX_PAD_INPUT_BG: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
const SMX_PAD_INPUT_CELL_IDLE: [f32; 4] = [0.25, 0.25, 0.30, 0.7];
const SMX_PAD_INPUT_CELL_LIT: [f32; 4] = [1.0, 1.0, 1.0, 0.95];

fn push_smx_pad_input_display(
    actors: &mut Vec<Actor>,
    state: &State,
    field_geom: &[Option<(profile_data::PlayerSide, f32, f32)>; 2],
    is_doubles: bool,
    is_centered_single: bool,
) {
    let mini_w = 3.0 * SMX_PAD_INPUT_CELL + 2.0 * SMX_PAD_INPUT_GAP;
    // Vertically center the mini-pad on the FSR sensor display group, so the two
    // read as aligned when shown together (regardless of whether the FSR display
    // is actually shown). Lifted above the footer so it clears the avatar.
    let fsr_bottom = screen_height() - SMX_SENSOR_FOOTER_CLEAR - SMX_SENSOR_MARGIN;
    let fsr_group_h = SMX_SENSOR_BAR_H + SMX_SENSOR_VALUE_GAP + SMX_SENSOR_VALUE_H;
    let y0 = fsr_bottom - fsr_group_h * 0.5 - mini_w * 0.5;

    if is_centered_single {
        // Big mini-pad stacked under the FSR group in the open side gutter.
        for slot in 0..2usize {
            if slot * 4 >= state.num_cols() || !state.profiles()[slot].smx_pad_input_display {
                continue;
            }
            let Some((side, field_left, field_right)) = field_geom[slot] else {
                continue;
            };
            let (scale, _, _, mini_x, mini_y) = smx_centered_layout(side, field_left, field_right);
            draw_smx_mini_pad(actors, state, slot * 4, mini_x, mini_y, scale);
        }
        return;
    }

    if is_doubles {
        // One player drives both pads. Show both mini-pads (pad 0 left, pad 1
        // right) beside each other, centered on the playfield directly under the
        // FSR pair. Gated on the doubles player's toggle (profile 0).
        if !state.profiles()[0].smx_pad_input_display {
            return;
        }
        let Some((_, field_left, _)) = field_geom[0] else {
            return;
        };
        // Centered in the left gutter, aligned under the FSR pair.
        let center_x = field_left * 0.5;
        let group_gap = SMX_DOUBLES_PAIR_GAP;
        let total_w = mini_w * 2.0 + group_gap;
        let start_x = center_x - total_w * 0.5;
        // Below the FSR pair (which starts SMX_DOUBLES_STACK_TOP_FRAC down).
        let fsr_group_h = SMX_SENSOR_VALUE_H + SMX_SENSOR_VALUE_GAP + SMX_SENSOR_BAR_H;
        let mini_top =
            screen_height() * SMX_DOUBLES_STACK_TOP_FRAC + fsr_group_h + SMX_DOUBLES_STACK_GAP;
        // When the FSR pair is also shown, center each mini under its FSR group
        // above it; otherwise use the natural (tighter) mini-pair spacing so a
        // mini-only display doesn't look oddly spread out.
        let fsr_active = state.profiles()[0].smx_fsr_display;
        let fsr_group_w = smx_fsr_group_w();
        let fsr_start_x = center_x - (fsr_group_w * 2.0 + group_gap) * 0.5;
        for half in 0..2usize {
            let x0 = if fsr_active {
                let fsr_center =
                    fsr_start_x + half as f32 * (fsr_group_w + group_gap) + fsr_group_w * 0.5;
                fsr_center - mini_w * 0.5
            } else {
                start_x + half as f32 * (mini_w + group_gap)
            };
            draw_smx_mini_pad(actors, state, half * 4, x0, mini_top, 1.0);
        }
        return;
    }

    // Each active pad slot (0 = P1, 1 = P2) owns a 4-column block; gated on the
    // owning player's toggle and the columns actually existing. Placed just
    // outside the inner edge of that player's notefield (mirrors the FSR display
    // on the outer edge).
    for slot in 0..2usize {
        if slot * 4 >= state.num_cols() || !state.profiles()[slot].smx_pad_input_display {
            continue;
        }
        let Some((side, field_left, field_right)) = field_geom[slot] else {
            continue;
        };
        // P1 wants a tight 5px inner gap; P2 looked right at the original 14px
        // (versus notefields are not symmetric about center).
        let inner_gap = if side == profile_data::PlayerSide::P2 {
            SMX_OVERLAY_FIELD_GAP
        } else {
            SMX_OVERLAY_INNER_GAP
        };
        let x0 = smx_overlay_x(side, field_left, field_right, mini_w, false, inner_gap);
        draw_smx_mini_pad(actors, state, slot * 4, x0, y0, 1.0);
    }
}

/// Draws one input-driven mini-pad (4 panels lit from columns `base..base+4`)
/// at `x0, y0`, scaled by `scale`.
fn draw_smx_mini_pad(
    actors: &mut Vec<Actor>,
    state: &State,
    base: usize,
    x0: f32,
    y0: f32,
    scale: f32,
) {
    let cell = SMX_PAD_INPUT_CELL * scale;
    let gap = SMX_PAD_INPUT_GAP * scale;
    let mini_w = 3.0 * cell + 2.0 * gap;
    let bg_pad = 3.0 * scale;
    push_smx_quad(
        actors,
        x0 - bg_pad,
        y0 - bg_pad,
        mini_w + bg_pad * 2.0,
        mini_w + bg_pad * 2.0,
        SMX_PAD_INPUT_BG,
        SMX_SENSOR_Z - 1.0,
    );
    for &(col_off, gx, gy) in SMX_PAD_INPUT_PANELS.iter() {
        let cx = x0 + gx * (cell + gap);
        let cy = y0 + gy * (cell + gap);
        let pressed = state.lane_pressed(base + col_off);
        let color = if pressed {
            SMX_PAD_INPUT_CELL_LIT
        } else {
            SMX_PAD_INPUT_CELL_IDLE
        };
        push_smx_quad(actors, cx, cy, cell, cell, color, SMX_SENSOR_Z);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::TextAttribute;
    use deadsync_chart::{ArrowStats, StaminaCounts, TechCounts};

    fn empty_text_attributes() -> Arc<[TextAttribute]> {
        Arc::from([])
    }
    use deadlib_present::actors::{SizeSpec, TextAlign};

    fn test_sprite_kind(key: &str) -> SongLuaOverlayKind {
        SongLuaOverlayKind::Sprite {
            texture_path: std::path::PathBuf::from(key),
            texture_key: Arc::from(key),
        }
    }

    fn test_sprite_path_kind(path: std::path::PathBuf) -> SongLuaOverlayKind {
        let texture_key = Arc::from(path.to_string_lossy().into_owned());
        SongLuaOverlayKind::Sprite {
            texture_path: path,
            texture_key,
        }
    }

    #[test]
    fn song_meter_progress_uses_itg_first_second_anchor() {
        assert_eq!(song_meter_progress(-1.0, 2.0, 12.0), 0.0);
        assert_eq!(song_meter_progress(2.0, 2.0, 12.0), 0.0);
        assert!((song_meter_progress(7.0, 2.0, 12.0) - 0.5).abs() <= 1e-6);
        assert_eq!(song_meter_progress(12.0, 2.0, 12.0), 1.0);
    }

    fn ensure_i18n() {
        crate::assets::i18n::init("en");
    }

    fn test_chart(hash: &str) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 10,
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

    #[test]
    fn p2_solo_scorebox_data_uses_runtime_profile_and_chart() {
        for play_style in [
            profile_data::PlayStyle::Single,
            profile_data::PlayStyle::Double,
        ] {
            let charts = [
                Arc::new(test_chart("p1-hash")),
                Arc::new(test_chart("p2-hash")),
            ];
            let mut player_profiles: [profile_data::Profile; MAX_PLAYERS] =
                std::array::from_fn(|_| profile_data::Profile::default());
            player_profiles[0].display_scorebox = false;
            player_profiles[0].show_ex_score = false;
            player_profiles[1].display_scorebox = true;
            player_profiles[1].show_ex_score = true;
            player_profiles[1].groovestats_username = "p2-user".to_string();
            let session = GameplaySession {
                play_style: crate::game::gameplay_play_style_from_profile(play_style),
                player_side: crate::game::gameplay_player_side_from_profile(
                    profile_data::PlayerSide::P2,
                ),
                joined_sides: [false, true],
                active_profile_ids: [None, Some("p2-profile".to_string())],
                ..GameplaySession::default()
            };

            let runtime_profiles = gameplay_runtime_profile_data(&player_profiles, &session);
            let runtime_charts = gameplay_runtime_charts(&charts, &session);
            assert_eq!(runtime_charts[0].short_hash, "p2-hash");

            let data = gameplay_scorebox_data(&runtime_charts, &runtime_profiles, &session);
            let p1_idx = profile_data::player_side_index(profile_data::PlayerSide::P1);
            let p2_idx = profile_data::player_side_index(profile_data::PlayerSide::P2);
            assert!(!data.profile_snapshot[p1_idx].display_scorebox);
            assert!(data.profile_snapshot[p2_idx].display_scorebox);
            assert!(data.profile_snapshot[p2_idx].show_ex_score);
            assert_eq!(data.profile_snapshot[p2_idx].gs_username(), "p2-user");
            assert_eq!(
                data.profile_snapshot[p2_idx].persistent_profile_id(),
                Some("p2-profile")
            );
            assert!(data.side_snapshot[p2_idx].is_none());
        }
    }

    #[test]
    fn custom_gameplay_backdrop_covers_full_screen_under_song_ui() {
        let mut actors = Vec::new();
        let color = crate::config::Color::from_hex("#0c0c0c").unwrap();

        push_custom_gameplay_backdrop(&mut actors, color);

        let [
            Actor::Sprite {
                align,
                offset,
                size,
                source,
                tint,
                z,
                ..
            },
        ] = actors.as_slice()
        else {
            panic!("expected one custom backdrop actor");
        };
        assert_eq!(*align, [0.0, 0.0]);
        assert_eq!(*offset, [0.0, 0.0]);
        assert!(matches!(source, SpriteSource::Solid));
        assert_eq!(*tint, color.to_rgba());
        assert_eq!(*z, -99);
        match size {
            [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                assert_eq!(*w, screen_width());
                assert_eq!(*h, screen_height());
            }
            other => panic!("expected fixed screen size, got {other:?}"),
        }
    }

    #[test]
    fn black_gameplay_backdrop_preserves_legacy_header() {
        let mut actors = Vec::new();

        push_custom_gameplay_backdrop(&mut actors, crate::config::Color::BLACK);

        assert!(actors.is_empty());
        assert_eq!(
            gameplay_header_rgba(crate::config::Color::BLACK),
            [0.0, 0.0, 0.0, 0.85]
        );
    }

    #[test]
    fn custom_gameplay_backdrop_tints_header() {
        let color = crate::config::Color::from_hex("#0c0c0c").unwrap();

        assert_eq!(gameplay_header_rgba(color), color.to_rgba());
    }

    #[test]
    fn forced_center_view_uses_layout_player_x() {
        let view = notefield::ViewOverride {
            force_center_1player: true,
            ..notefield::ViewOverride::default()
        };

        assert_eq!(song_lua_player_target_x(None, 320.0, 800.0, view), 800.0);
    }

    #[test]
    fn forced_center_view_preserves_explicit_player_x() {
        let view = notefield::ViewOverride {
            force_center_1player: true,
            ..notefield::ViewOverride::default()
        };

        assert_eq!(
            song_lua_player_target_x(Some(640.0), 320.0, 800.0, view),
            640.0
        );
    }

    #[test]
    fn default_view_uses_player_state_x() {
        assert_eq!(
            song_lua_player_target_x(None, 320.0, 800.0, notefield::ViewOverride::default()),
            320.0
        );
    }

    #[test]
    fn difficulty_meter_overlap_catches_shifted_targets() {
        assert!(ranges_overlap(
            90.0,
            TARGET_ARROW_PIXEL_SIZE,
            56.0,
            DIFFICULTY_METER_SIZE
        ));
        assert!(!ranges_overlap(
            115.0,
            TARGET_ARROW_PIXEL_SIZE,
            56.0,
            DIFFICULTY_METER_SIZE
        ));
    }

    #[test]
    fn difficulty_meter_overlap_uses_profile_target_offset() {
        let mut profile = profile_data::Profile::default();

        assert!(!saved_targets_hit_meter(&profile, 4, DIFFICULTY_METER_Y));

        profile.note_field_offset_y = -50;
        assert!(saved_targets_hit_meter(&profile, 4, DIFFICULTY_METER_Y));
    }

    #[test]
    fn difficulty_meter_overlap_uses_profile_scroll_option() {
        let mut profile = profile_data::Profile {
            note_field_offset_y: -50,
            ..profile_data::Profile::default()
        };
        assert!(saved_targets_hit_meter(&profile, 4, DIFFICULTY_METER_Y));

        profile.scroll_option = profile_data::ScrollOption::Centered;
        assert!(!saved_targets_hit_meter(&profile, 4, DIFFICULTY_METER_Y));
    }

    #[test]
    fn side_difficulty_meter_uses_player_side() {
        assert_eq!(
            side_difficulty_meter_x(profile_data::PlayerSide::P1),
            DIFFICULTY_METER_SIZE * 0.5
        );
        assert_eq!(
            side_difficulty_meter_x(profile_data::PlayerSide::P2),
            screen_width() - DIFFICULTY_METER_SIZE * 0.5
        );
    }

    fn test_proxy_overlay(player_index: usize) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorProxy {
                target: SongLuaProxyTarget::Player { player_index },
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn test_capture_overlay(name: &str) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrameTexture,
            name: Some(name.to_string()),
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn test_capture_proxy_child(
        parent_index: usize,
        target: SongLuaProxyTarget,
    ) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorProxy { target },
            name: None,
            parent_index: Some(parent_index),
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }
    }

    fn test_aft_overlay(capture_name: &str, visible: bool) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::AftSprite {
                capture_name: capture_name.to_string(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                visible,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        }
    }

    fn test_rgb_aft_overlay(
        name: &str,
        capture_name: &str,
        diffuse: [f32; 4],
    ) -> SongLuaOverlayActor {
        let mut overlay = test_aft_overlay(capture_name, true);
        overlay.name = Some(name.to_string());
        overlay.initial_state.x = screen_width() * 0.5;
        overlay.initial_state.y = screen_height() * 0.5;
        overlay.initial_state.diffuse = diffuse;
        overlay.initial_state.blend = SongLuaOverlayBlendMode::Add;
        overlay
    }

    fn test_source_actor() -> Actor {
        Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: Vec::new(),
            background: None,
            z: 0,
        }
    }

    fn test_order_overlay(
        kind: SongLuaOverlayKind,
        parent_index: Option<usize>,
        draw_order: i32,
    ) -> SongLuaOverlayActor {
        SongLuaOverlayActor {
            kind,
            name: None,
            parent_index,
            initial_state: SongLuaOverlayState {
                draw_order,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        }
    }

    fn test_skewed_overlay_point(
        center: [f32; 2],
        local: [f32; 2],
        skew_x: f32,
        skew_y: f32,
    ) -> [f32; 2] {
        let y = skew_y.mul_add(local[0], local[1]);
        let x = skew_x.mul_add(y, local[0]);
        [center[0] + x, center[1] + y]
    }

    fn test_transform_point(matrix: Matrix4, local: [f32; 2]) -> [f32; 2] {
        let point = matrix * Vector4::new(local[0], local[1], 0.0, 1.0);
        [point.x, point.y]
    }

    fn first_textured_mesh_transform(actor: &Actor) -> Matrix4 {
        match actor {
            Actor::TexturedMesh {
                local_transform, ..
            } => *local_transform,
            Actor::Frame { children, .. } => children
                .iter()
                .find_map(|child| match child {
                    Actor::TexturedMesh {
                        local_transform, ..
                    } => Some(*local_transform),
                    _ => None,
                })
                .expect("expected textured mesh child"),
            _ => panic!("expected textured mesh actor"),
        }
    }

    trait SongLuaActorListTestExt {
        fn expect_actor(self, message: &str) -> Actor;
        fn expect_actors(self, message: &str) -> SongLuaActorList;
    }

    impl SongLuaActorListTestExt for Option<SongLuaActorList> {
        fn expect_actor(self, message: &str) -> Actor {
            let mut actors = self.expect_actors(message);
            assert_eq!(
                actors.len(),
                1,
                "{message}: expected one actor, got {}",
                actors.len()
            );
            actors.remove(0)
        }

        fn expect_actors(self, message: &str) -> SongLuaActorList {
            self.unwrap_or_else(|| panic!("{message}"))
        }
    }

    fn test_lobby_player(screen_name: &str, ready: bool) -> lobby_data::LobbyPlayer {
        lobby_data::LobbyPlayer {
            label: "Local".to_string(),
            ready,
            screen_name: screen_name.to_string(),
            judgments: None,
            score: None,
            ex_score: None,
        }
    }

    #[test]
    fn song_lua_player_rotation_z_matches_itg_screen_space() {
        let matrix = song_lua_player_transform_matrix(
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            90.0,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
        )
        .expect("rotation should produce a player transform");
        let point = test_transform_point(matrix, [10.0, 0.0]);
        assert!(point[0].abs() <= 0.000_1);
        assert!((point[1] + 10.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_player_skews_match_itg_screen_space() {
        let skew_x_matrix = song_lua_player_transform_matrix(
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            0.0,
            0.5,
            0.0,
            1.0,
            1.0,
            1.0,
        )
        .expect("skewx should produce a player transform");
        let point = test_transform_point(skew_x_matrix, [0.0, -20.0]);
        assert!((point[0] - 10.0).abs() <= 0.000_1);
        assert!((point[1] + 20.0).abs() <= 0.000_1);

        let skew_y_matrix = song_lua_player_transform_matrix(
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            0.0,
            0.0,
            0.5,
            1.0,
            1.0,
            1.0,
        )
        .expect("skewy should produce a player transform");
        let point = test_transform_point(skew_y_matrix, [20.0, 0.0]);
        assert!((point[0] - 20.0).abs() <= 0.000_1);
        assert!((point[1] + 10.0).abs() <= 0.000_1);
    }

    #[test]
    fn song_lua_note_field_proxy_source_preserves_camera_transform() {
        let segments = vec![Arc::<[Actor]>::from(vec![
            Actor::CameraPush {
                view_proj: Matrix4::IDENTITY,
            },
            test_source_actor(),
            Actor::CameraPop,
        ])];
        let mut field = song_lua_owned_segment_actors(segments);
        let mut hud = Vec::new();
        let mut out = Vec::new();

        apply_song_lua_player_transform(
            &mut field,
            &mut hud,
            &mut out,
            0,
            [1.0; 4],
            None,
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            0.0,
            0.0,
            0.5,
            0.0,
            1.0,
            1.0,
            1.0,
        );

        let Some(Actor::CameraPush { view_proj }) = out.first() else {
            panic!("expected transformed notefield camera");
        };
        let point = test_transform_point(*view_proj, [0.0, -20.0]);
        assert!((point[0] - 10.0).abs() <= 0.000_1);
        assert!((point[1] + 20.0).abs() <= 0.000_1);
    }

    fn test_joined_lobby(players: Vec<lobby_data::LobbyPlayer>) -> lobby_data::JoinedLobby {
        lobby_data::JoinedLobby {
            code: "ABCD".to_string(),
            players,
            song_info: None,
        }
    }

    #[test]
    fn song_lua_proxy_active_players_requires_a_render_source() {
        let overlays = vec![test_proxy_overlay(0)];
        let overlay_states = vec![SongLuaOverlayState::default()];
        let empty_sources = [
            SongLuaPlayerProxySources::default(),
            SongLuaPlayerProxySources::default(),
        ];

        assert_eq!(
            song_lua_proxy_active_players(&overlays, &overlay_states, &empty_sources),
            [false, false]
        );

        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let sources = [
            SongLuaPlayerProxySources {
                player: Some(source.as_slice()),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources::default(),
        ];
        assert_eq!(
            song_lua_proxy_active_players(&overlays, &overlay_states, &sources),
            [true, false]
        );
    }

    #[test]
    fn song_lua_proxy_requests_ignore_unreferenced_capture_children() {
        let overlays = vec![
            test_capture_overlay("cap"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
        ];
        let overlay_states = vec![SongLuaOverlayState::default(); overlays.len()];
        let requests = song_lua_proxy_requests(&overlays, &overlay_states);

        assert!(!requests.players[0].player);
        assert!(!requests.players[0].note_field);
        assert!(!requests.players[0].judgment);
        assert!(!requests.players[0].combo);
        assert!(!requests.underlay);
        assert!(!requests.overlay);
    }

    #[test]
    fn song_lua_proxy_requests_follow_visible_aft_capture_usage() {
        let overlays = vec![
            test_capture_overlay("cap"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Judgment { player_index: 0 }),
            test_aft_overlay("cap", true),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let requests = song_lua_proxy_requests(&overlays, &overlay_states);

        assert!(!requests.players[0].player);
        assert!(!requests.players[0].note_field);
        assert!(requests.players[0].judgment);
        assert!(!requests.players[0].combo);
    }

    #[test]
    fn song_lua_proxy_requests_skip_hidden_aft_capture_usage() {
        let overlays = vec![
            test_capture_overlay("cap"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Combo { player_index: 0 }),
            test_aft_overlay("cap", false),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let requests = song_lua_proxy_requests(&overlays, &overlay_states);

        assert!(!requests.players[0].combo);
    }

    #[test]
    fn song_lua_overlay_center_coords_stay_centered_under_actorframe() {
        let parent = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let child = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.x, 427.0);
        assert_eq!(composed.y, 240.0);
    }

    #[test]
    fn song_lua_overlay_root_actorframe_keeps_absolute_center_child() {
        let parent = SongLuaOverlayState::default();
        let child = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.x, 427.0);
        assert_eq!(composed.y, 240.0);
    }

    #[test]
    fn song_lua_overlay_local_offsets_still_compose_from_centered_actorframe() {
        let parent = SongLuaOverlayState {
            x: 427.0,
            y: 240.0,
            ..SongLuaOverlayState::default()
        };
        let child = SongLuaOverlayState {
            x: -180.0,
            y: 0.0,
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.x, 247.0);
        assert_eq!(composed.y, 240.0);
    }

    #[test]
    fn song_lua_overlay_texture_translate_stacks_from_parent() {
        let parent = SongLuaOverlayState {
            texcoord_offset: Some([0.25, 0.5]),
            ..SongLuaOverlayState::default()
        };
        let child = SongLuaOverlayState {
            texcoord_offset: Some([0.125, -0.25]),
            ..SongLuaOverlayState::default()
        };
        let composed = song_lua_overlay_compose_state(
            &SongLuaOverlayKind::ActorFrame,
            parent,
            child,
            854.0,
            480.0,
        );
        assert_eq!(composed.texcoord_offset, Some([0.375, 0.25]));
    }

    #[test]
    fn song_lua_aft_capture_uses_local_proxy_origin() {
        let root = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                x: 427.0,
                y: 240.0,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        };
        let mut capture = test_capture_overlay("cap");
        capture.parent_index = Some(0);
        let overlays = vec![
            root,
            capture,
            test_capture_proxy_child(1, SongLuaProxyTarget::Player { player_index: 0 }),
        ];
        let local_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let overlay_states =
            song_lua_overlay_states_from_local(&overlays, &local_states, 854.0, 480.0);
        assert_eq!(overlay_states[2].x, 427.0);
        assert_eq!(overlay_states[2].y, 240.0);

        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let proxy_sources = SongLuaScreenProxySources {
            players: [
                SongLuaPlayerProxySources {
                    player: Some(source.as_slice()),
                    ..SongLuaPlayerProxySources::default()
                },
                SongLuaPlayerProxySources::default(),
            ],
            ..SongLuaScreenProxySources::default()
        };
        let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut capture_states = Vec::new();
        let mut order_scratch = Vec::new();
        let actors = song_lua_capture_children(
            &overlays,
            &overlay_states,
            &local_states,
            &mut order_cache,
            &AssetManager::new(),
            1,
            &proxy_sources,
            854.0,
            480.0,
            &mut capture_states,
            &mut order_scratch,
        );

        match actors.as_slice() {
            [Actor::Frame { offset, .. }] => assert_eq!(*offset, [0.0, 0.0]),
            other => panic!("expected one capture proxy frame, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_actor_proxy_keeps_overlay_z_layer() {
        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let actor = song_lua_build_proxy_actor(
            SongLuaOverlayState::default(),
            1234,
            source.as_slice(),
            640.0,
            480.0,
        )
        .expect("actor proxy should render with a source");

        match actor {
            Actor::Frame { z, children, .. } => {
                assert_eq!(z, 1234);
                assert_eq!(children.len(), 1);
            }
            other => panic!("expected frame actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_actor_proxy_keeps_source_z_inside_proxy_layer() {
        let source = vec![Arc::<[Actor]>::from(vec![Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: vec![Actor::Frame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: Vec::new(),
                background: None,
                z: 96,
            }],
            background: None,
            z: 83,
        }])];
        let actor = song_lua_build_proxy_actor(
            SongLuaOverlayState::default(),
            1234,
            source.as_slice(),
            640.0,
            480.0,
        )
        .expect("actor proxy should render with a source");

        let Actor::Frame { z, children, .. } = actor else {
            panic!("expected proxy frame");
        };
        assert_eq!(z, 1234);
        let [Actor::SharedFrame { z, children, .. }] = children.as_slice() else {
            panic!("expected one shared proxy source");
        };
        assert_eq!(*z, 0);
        let [Actor::Frame { z, children, .. }] = children.as_ref() else {
            panic!("expected local source frame");
        };
        assert_eq!(*z, 0);
        let [Actor::Frame { z, .. }] = children.as_slice() else {
            panic!("expected local source child frame");
        };
        assert_eq!(*z, 0);
    }

    #[test]
    fn song_lua_actor_proxy_preserves_source_z_order_locally() {
        let mut low = test_source_actor();
        let mut high = test_source_actor();
        if let Actor::Frame { z, .. } = &mut low {
            *z = -20;
        }
        if let Actor::Frame { offset, z, .. } = &mut high {
            *offset = [99.0, 0.0];
            *z = 20;
        }
        let source = vec![Arc::<[Actor]>::from(vec![high, low])];
        let actor = song_lua_build_proxy_actor(
            SongLuaOverlayState::default(),
            1234,
            source.as_slice(),
            640.0,
            480.0,
        )
        .expect("actor proxy should render with a source");

        let Actor::Frame { children, .. } = actor else {
            panic!("expected proxy frame");
        };
        let [Actor::SharedFrame { children, .. }] = children.as_slice() else {
            panic!("expected one shared proxy source");
        };
        let [
            Actor::Frame {
                offset: first_offset,
                ..
            },
            Actor::Frame {
                offset: second_offset,
                z,
                ..
            },
        ] = children.as_ref()
        else {
            panic!("expected sorted local source frames");
        };
        assert_eq!(*first_offset, [0.0, 0.0]);
        assert_eq!(*second_offset, [99.0, 0.0]);
        assert_eq!(*z, 0);
        assert_eq!(
            children
                .iter()
                .map(|actor| match actor {
                    Actor::Frame { z, .. } => *z,
                    other => panic!("expected source frame, got {other:?}"),
                })
                .collect::<Vec<_>>(),
            [0, 0]
        );
    }

    #[test]
    fn song_lua_actor_proxy_keeps_camera_scope_around_sorted_source() {
        let mut low = test_source_actor();
        let mut high = test_source_actor();
        if let Actor::Frame { z, .. } = &mut low {
            *z = -20;
        }
        if let Actor::Frame { offset, z, .. } = &mut high {
            *offset = [99.0, 0.0];
            *z = 20;
        }
        let source = vec![Arc::<[Actor]>::from(vec![
            Actor::CameraPush {
                view_proj: Matrix4::IDENTITY,
            },
            high,
            low,
            Actor::CameraPop,
        ])];
        let actor = song_lua_build_proxy_actor(
            SongLuaOverlayState::default(),
            1234,
            source.as_slice(),
            640.0,
            480.0,
        )
        .expect("actor proxy should render with a source");

        let Actor::Frame { children, .. } = actor else {
            panic!("expected proxy frame");
        };
        let [Actor::SharedFrame { children, .. }] = children.as_slice() else {
            panic!("expected one shared proxy source");
        };
        let [
            Actor::CameraPush { .. },
            Actor::Frame {
                offset: first_offset,
                ..
            },
            Actor::Frame {
                offset: second_offset,
                z,
                ..
            },
            Actor::CameraPop,
        ] = children.as_ref()
        else {
            panic!("expected sorted actors inside original camera scope");
        };
        assert_eq!(*first_offset, [0.0, 0.0]);
        assert_eq!(*second_offset, [99.0, 0.0]);
        assert_eq!(*z, 0);
    }

    #[test]
    fn song_lua_capture_style_tints_sprite_glow() {
        let actor = Actor::Sprite {
            align: [0.5, 0.5],
            offset: [0.0, 0.0],
            world_z: 0.0,
            size: [SizeSpec::Px(16.0), SizeSpec::Px(16.0)],
            source: SpriteSource::Solid,
            tint: [0.8, 0.6, 0.4, 0.5],
            glow: [0.5, 0.25, 1.0, 0.4],
            z: 2,
            cell: None,
            grid: None,
            uv_rect: None,
            visible: true,
            flip_x: false,
            flip_y: false,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: 0.0,
            faderight: 0.0,
            fadetop: 0.0,
            fadebottom: 0.0,
            blend: BlendMode::Alpha,
            mask_source: false,
            mask_dest: false,
            rot_x_deg: 0.0,
            rot_y_deg: 0.0,
            rot_z_deg: 0.0,
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            texcoordvelocity: None,
            animate: false,
            state_delay: 0.0,
            scale: [1.0, 1.0],
            shadow_len: [0.0, 0.0],
            shadow_color: [0.2, 0.4, 0.6, 0.5],
            effect: EffectState::default(),
        };

        let styled =
            song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], Some(BlendMode::Add), 7);

        let Actor::Sprite {
            tint,
            glow,
            shadow_color,
            blend,
            z,
            ..
        } = styled
        else {
            panic!("expected sprite actor");
        };
        assert_eq!(tint, [0.4, 0.15, 0.040000003, 0.25]);
        assert_eq!(glow, [0.25, 0.0625, 0.1, 0.2]);
        assert_eq!(shadow_color, [0.1, 0.1, 0.060000002, 0.25]);
        assert_eq!(blend, BlendMode::Add);
        assert_eq!(z, 9);
    }

    #[test]
    fn song_lua_capture_style_tints_mesh_vertices() {
        let actor = Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            vertices: Arc::from(vec![MeshVertex {
                pos: [0.0, 0.0],
                color: [0.8, 0.6, 0.4, 0.5],
            }]),
            visible: true,
            blend: BlendMode::Alpha,
            z: 3,
        };

        let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

        let Actor::Mesh {
            vertices, blend, z, ..
        } = styled
        else {
            panic!("expected mesh actor");
        };
        assert_eq!(vertices[0].color, [0.4, 0.15, 0.040000003, 0.25]);
        assert_eq!(blend, BlendMode::Alpha);
        assert_eq!(z, 7);
    }

    #[test]
    fn song_lua_capture_style_tints_textured_mesh() {
        let actor = Actor::TexturedMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: 0.0,
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::from("mesh"),
            tint: [0.8, 0.6, 0.4, 0.5],
            glow: [0.5, 0.25, 1.0, 0.4],
            vertices: Arc::from(vec![TexturedMeshVertex::default(); 3]),
            geom_cache_key: INVALID_TMESH_CACHE_KEY,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: false,
            visible: true,
            blend: BlendMode::Alpha,
            z: 3,
        };

        let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

        let Actor::TexturedMesh {
            tint,
            glow,
            blend,
            z,
            ..
        } = styled
        else {
            panic!("expected textured mesh actor");
        };
        assert_eq!(tint, [0.4, 0.15, 0.040000003, 0.25]);
        assert_eq!(glow, [0.25, 0.0625, 0.1, 0.2]);
        assert_eq!(blend, BlendMode::Alpha);
        assert_eq!(z, 7);
    }

    #[test]
    fn song_lua_coincident_rgb_aft_uses_one_internal_blend_capture() {
        let overlays = vec![
            test_capture_overlay("CaptureAFT"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
            test_rgb_aft_overlay("AFTSpriteR", "CaptureAFT", [1.0, 0.0, 0.0, 1.0]),
            test_rgb_aft_overlay("AFTSpriteG", "CaptureAFT", [0.0, 1.0, 0.0, 1.0]),
            test_rgb_aft_overlay("AFTSpriteB", "CaptureAFT", [0.0, 0.0, 1.0, 1.0]),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let proxy_sources = SongLuaScreenProxySources {
            players: [
                SongLuaPlayerProxySources {
                    player: Some(source.as_slice()),
                    ..SongLuaPlayerProxySources::default()
                },
                SongLuaPlayerProxySources::default(),
            ],
            ..SongLuaScreenProxySources::default()
        };
        let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut out = Vec::new();
        let mut order_scratch = Vec::new();
        let mut capture_states = Vec::new();
        let mut capture_order_scratch = Vec::new();

        push_song_lua_layer_actors(
            &mut out,
            &overlays,
            &mut order_cache,
            &overlay_states,
            &overlay_states,
            SongLuaOverlayState::default(),
            &proxy_sources,
            &AssetManager::new(),
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            &mut order_scratch,
            &mut capture_states,
            &mut capture_order_scratch,
        );

        assert_eq!(out.len(), 1);
        let Actor::Frame { children, .. } = &out[0] else {
            panic!("expected combined AFT frame");
        };
        let [Actor::Frame { children, .. }] = children.as_slice() else {
            panic!("expected captured proxy frame");
        };
        let [Actor::SharedFrame { blend, tint, .. }] = children.as_slice() else {
            panic!("expected captured source frame");
        };
        assert_eq!(*blend, Some(BlendMode::Alpha));
        assert_eq!(*tint, [1.0; 4]);
    }

    #[test]
    fn song_lua_rgb_aft_keeps_split_channels_when_vibrating() {
        let mut red = test_rgb_aft_overlay("AFTSpriteR", "CaptureAFT", [1.0, 0.0, 0.0, 1.0]);
        red.initial_state.vibrate = true;
        red.initial_state.effect_magnitude = [10.0, 10.0, 10.0];
        let overlays = vec![
            test_capture_overlay("CaptureAFT"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
            red,
            test_rgb_aft_overlay("AFTSpriteG", "CaptureAFT", [0.0, 1.0, 0.0, 1.0]),
            test_rgb_aft_overlay("AFTSpriteB", "CaptureAFT", [0.0, 0.0, 1.0, 1.0]),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let proxy_sources = SongLuaScreenProxySources {
            players: [
                SongLuaPlayerProxySources {
                    player: Some(source.as_slice()),
                    ..SongLuaPlayerProxySources::default()
                },
                SongLuaPlayerProxySources::default(),
            ],
            ..SongLuaScreenProxySources::default()
        };
        let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut out = Vec::new();
        let mut order_scratch = Vec::new();
        let mut capture_states = Vec::new();
        let mut capture_order_scratch = Vec::new();

        push_song_lua_layer_actors(
            &mut out,
            &overlays,
            &mut order_cache,
            &overlay_states,
            &overlay_states,
            SongLuaOverlayState::default(),
            &proxy_sources,
            &AssetManager::new(),
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            &mut order_scratch,
            &mut capture_states,
            &mut capture_order_scratch,
        );

        assert_eq!(out.len(), 3);
    }

    #[test]
    fn song_lua_rgb_aft_keeps_zero_magnitude_vibrate_combined() {
        let mut red = test_rgb_aft_overlay("AFTSpriteR", "CaptureAFT", [1.0, 0.0, 0.0, 1.0]);
        red.initial_state.vibrate = true;
        let mut green = test_rgb_aft_overlay("AFTSpriteG", "CaptureAFT", [0.0, 1.0, 0.0, 1.0]);
        green.initial_state.vibrate = true;
        let mut blue = test_rgb_aft_overlay("AFTSpriteB", "CaptureAFT", [0.0, 0.0, 1.0, 1.0]);
        blue.initial_state.vibrate = true;
        let overlays = vec![
            test_capture_overlay("CaptureAFT"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
            red,
            green,
            blue,
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let proxy_sources = SongLuaScreenProxySources {
            players: [
                SongLuaPlayerProxySources {
                    player: Some(source.as_slice()),
                    ..SongLuaPlayerProxySources::default()
                },
                SongLuaPlayerProxySources::default(),
            ],
            ..SongLuaScreenProxySources::default()
        };
        let mut order_cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut out = Vec::new();
        let mut order_scratch = Vec::new();
        let mut capture_states = Vec::new();
        let mut capture_order_scratch = Vec::new();

        push_song_lua_layer_actors(
            &mut out,
            &overlays,
            &mut order_cache,
            &overlay_states,
            &overlay_states,
            SongLuaOverlayState::default(),
            &proxy_sources,
            &AssetManager::new(),
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            &mut order_scratch,
            &mut capture_states,
            &mut capture_order_scratch,
        );

        assert_eq!(out.len(), 1);
    }

    #[test]
    fn song_lua_kenpo_rgb_aft_initial_state_combines_if_present() {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let Some(root) = [
            manifest.join("../lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
            manifest.join("songs/ITL Online 2026/[11] KENPO SAITO (DX) [Scrypts]"),
            manifest.join("songs/lua-songs/[11] KENPO SAITO (DX) [Scrypts]"),
        ]
        .into_iter()
        .find(|root| root.join("template/main.lua").is_file()) else {
            return;
        };
        let entry = root.join("template/main.lua");
        let mut context =
            crate::game::parsing::song_lua::SongLuaCompileContext::new(&root, "KENPO SAITO");
        context.style_name = "double".to_string();
        let compiled = crate::game::parsing::song_lua::compile_song_lua(&entry, &context).unwrap();
        let states = compiled
            .overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let mut order_cache = song_lua_overlay_order_cache_from(&compiled.overlays, &[]);
        let mut order = Vec::new();
        song_lua_overlay_order_into(
            &compiled.overlays,
            &states,
            &mut order_cache,
            None,
            &mut order,
        );
        let red_index = compiled
            .overlays
            .iter()
            .position(|overlay| overlay.name.as_deref() == Some("AFTSpriteR"))
            .expect("KENPO sample should compile AFTSpriteR");

        let Some((leader, group)) =
            song_lua_rgb_aft_group_for(&compiled.overlays, &states, &order, red_index)
        else {
            panic!("KENPO initial RGB AFT state should combine before rgbsplit");
        };

        assert!(group.contains(&leader));
    }

    #[test]
    fn song_lua_player_child_proxy_source_is_player_local() {
        let origin = [screen_center_x(), screen_center_y()];
        let source =
            song_lua_player_child_proxy_source(vec![test_source_actor()], origin[0], origin[1])
                .expect("child proxy source should render");
        let actor = song_lua_build_proxy_actor(
            SongLuaOverlayState {
                x: origin[0],
                y: origin[1],
                ..SongLuaOverlayState::default()
            },
            0,
            source.as_slice(),
            screen_width(),
            screen_height(),
        )
        .expect("actor proxy should render with a source");

        let Actor::Frame {
            offset, children, ..
        } = actor
        else {
            panic!("expected proxy frame");
        };
        assert_eq!(offset, origin);
        let [Actor::SharedFrame { children, .. }] = children.as_slice() else {
            panic!("expected shared source frame");
        };
        let [Actor::Frame { offset, .. }] = children.as_ref() else {
            panic!("expected localized child source");
        };
        assert_eq!(*offset, [-origin[0], -origin[1]]);
    }

    #[test]
    fn song_lua_quad_keeps_zoomed_size_in_scale() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                zoom: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            321,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("quad overlay should render");

        match actor {
            Actor::Sprite {
                size,
                scale,
                z,
                visible,
                ..
            } => {
                let expected_scale = [
                    100.0 * 0.5 * screen_width() / 640.0,
                    50.0 * 0.5 * screen_height() / 480.0,
                ];
                assert_eq!(z, 321);
                assert!(visible);
                assert!((scale[0] - expected_scale[0]).abs() <= 0.000_1);
                assert!((scale[1] - expected_scale[1]).abs() <= 0.000_1);
                match size {
                    [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                        assert_eq!(w, 0.0);
                        assert_eq!(h, 0.0);
                    }
                    other => panic!("expected explicit quad size, got {other:?}"),
                }
            }
            other => panic!("expected sprite-backed quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_actor_multi_vertex_builds_mesh_overlay() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorMultiVertex {
                vertices: Arc::from(vec![
                    SongLuaOverlayMeshVertex {
                        pos: [0.0, 0.0],
                        color: [1.0, 0.0, 0.0, 1.0],
                        uv: [0.0, 0.0],
                    },
                    SongLuaOverlayMeshVertex {
                        pos: [10.0, 0.0],
                        color: [0.0, 1.0, 0.0, 1.0],
                        uv: [1.0, 0.0],
                    },
                    SongLuaOverlayMeshVertex {
                        pos: [0.0, 10.0],
                        color: [0.0, 0.0, 1.0, 1.0],
                        uv: [0.0, 1.0],
                    },
                ]),
                texture_path: None,
                texture_key: None,
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 40.0,
                y: 50.0,
                zoom_x: 2.0,
                diffuse: [0.5, 0.5, 0.5, 0.75],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            321,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("ActorMultiVertex overlay should render");

        let Actor::Mesh {
            offset,
            vertices,
            z,
            blend,
            ..
        } = actor
        else {
            panic!("expected mesh-backed ActorMultiVertex overlay");
        };
        assert_eq!(offset, [40.0, 50.0]);
        assert_eq!(z, 321);
        assert_eq!(blend, BlendMode::Alpha);
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[1].pos, [20.0, -0.0]);
        assert_eq!(vertices[2].pos, [0.0, -10.0]);
        assert_eq!(vertices[0].color, [0.5, 0.0, 0.0, 0.75]);
    }

    #[test]
    fn song_lua_actor_multi_vertex_builds_textured_mesh_overlay() {
        let texture_key = "song-lua-amv-texture.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(texture_key.clone(), image::RgbaImage::new(16, 16));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorMultiVertex {
                vertices: Arc::from(vec![
                    SongLuaOverlayMeshVertex {
                        pos: [0.0, 0.0],
                        color: [1.0, 1.0, 1.0, 1.0],
                        uv: [0.0, 0.0],
                    },
                    SongLuaOverlayMeshVertex {
                        pos: [16.0, 0.0],
                        color: [0.0, 1.0, 0.0, 1.0],
                        uv: [1.0, 0.0],
                    },
                    SongLuaOverlayMeshVertex {
                        pos: [0.0, 16.0],
                        color: [0.0, 0.0, 1.0, 0.5],
                        uv: [0.0, 1.0],
                    },
                ]),
                texture_path: Some(std::path::PathBuf::from(&texture_key)),
                texture_key: Some(Arc::from(texture_key.as_str())),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 12.0,
                y: 24.0,
                diffuse: [0.5, 0.25, 0.75, 0.5],
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            322,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("textured ActorMultiVertex overlay should render");

        let Actor::TexturedMesh {
            offset,
            texture,
            tint,
            vertices,
            z,
            blend,
            ..
        } = actor
        else {
            panic!("expected textured mesh-backed ActorMultiVertex overlay");
        };
        assert_eq!(offset, [12.0, 24.0]);
        assert_eq!(texture.as_ref(), texture_key.as_str());
        assert_eq!(tint, [0.5, 0.25, 0.75, 0.5]);
        assert_eq!(z, 322);
        assert_eq!(blend, BlendMode::Alpha);
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices[1].uv, [1.0, 0.0]);
        assert_eq!(vertices[2].color, [0.0, 0.0, 1.0, 0.5]);
    }

    #[test]
    fn song_lua_model_builds_textured_mesh_layers() {
        let texture_key = "song-lua-model-texture.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(texture_key.clone(), image::RgbaImage::new(16, 16));
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Model {
                layers: Arc::from(vec![SongLuaOverlayModelLayer {
                    texture_key: Arc::from(texture_key.as_str()),
                    vertices: Arc::from(vec![
                        TexturedMeshVertex {
                            pos: [0.0, 0.0, 0.0],
                            uv: [0.0, 0.0],
                            tex_matrix_scale: [1.0, 1.0],
                            color: [1.0, 1.0, 1.0, 1.0],
                        },
                        TexturedMeshVertex {
                            pos: [16.0, 0.0, 0.0],
                            uv: [1.0, 0.0],
                            tex_matrix_scale: [1.0, 1.0],
                            color: [1.0, 1.0, 1.0, 1.0],
                        },
                        TexturedMeshVertex {
                            pos: [0.0, 16.0, 0.0],
                            uv: [0.0, 1.0],
                            tex_matrix_scale: [1.0, 1.0],
                            color: [1.0, 1.0, 1.0, 1.0],
                        },
                    ]),
                    model_size: [16.0, 16.0],
                    uv_scale: [1.0, 1.0],
                    uv_offset: [0.125, 0.25],
                    uv_tex_shift: [0.0, 0.0],
                    uv_velocity: [0.0, -1.0],
                    uv_cycle_seconds: Some(2.0),
                    draw: SongLuaOverlayModelDraw {
                        pos: [2.0, 3.0, 4.0],
                        rot: [0.0, 0.0, 0.0],
                        zoom: [1.0, 1.0, 1.0],
                        tint: [1.0, 0.5, 0.25, 0.75],
                        vert_align: 0.5,
                        blend_add: false,
                        visible: true,
                    },
                }]),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 12.0,
                y: 24.0,
                texcoord_offset: Some([0.25, -0.125]),
                diffuse: [0.5, 0.25, 0.75, 0.5],
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            323,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            1.0,
        )
        .expect_actor("Model overlay should render");

        let Actor::Frame { children, .. } = actor else {
            panic!("expected frame-backed Model overlay");
        };
        assert_eq!(children.len(), 1);
        let Actor::TexturedMesh {
            offset,
            texture,
            tint,
            vertices,
            z,
            blend,
            uv_offset,
            uv_tex_shift,
            ..
        } = &children[0]
        else {
            panic!("expected textured mesh model layer");
        };
        assert_eq!(*offset, [12.0, 24.0]);
        assert_eq!(texture.as_ref(), texture_key.as_str());
        assert_eq!(*tint, [0.5, 0.125, 0.1875, 0.375]);
        assert_eq!(*z, 323);
        assert_eq!(*blend, BlendMode::Alpha);
        assert_eq!(*uv_offset, [0.375, -0.375]);
        assert_eq!(*uv_tex_shift, [0.25, -0.625]);
        assert_eq!(vertices.len(), 3);
    }

    #[test]
    fn song_lua_noteskin_actor_rotation_matches_noteskin_base_rotation() {
        let model_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/noteskins/dance/ddr-note/_down tap note model.txt");
        let slots = crate::game::parsing::noteskin::load_itg_model_slots_from_path(&model_path)
            .expect("ddr-note tap model should load");
        let mut rotated_slots = slots.iter().cloned().collect::<Vec<_>>();
        for slot in &mut rotated_slots {
            slot.set_rotation_deg(90);
        }
        let rotated_slots = Arc::<[SpriteSlot]>::from(rotated_slots.into_boxed_slice());
        let mut asset_manager = AssetManager::new();
        for slot in slots.iter().chain(rotated_slots.iter()) {
            asset_manager
                .queue_texture_upload(slot.texture_key().to_owned(), image::RgbaImage::new(16, 16));
        }

        let actor_rotation = song_lua_noteskin_actor(
            &slots,
            SongLuaOverlayState {
                rot_z_deg: 90.0,
                ..SongLuaOverlayState::default()
            },
            &asset_manager,
            323,
            1.0,
            1.0,
            [1.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 90.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 0.0],
            BlendMode::Alpha,
            0.0,
            0.0,
        )
        .expect("noteskin actor with song-lua rotation should render");
        let base_rotation = song_lua_noteskin_actor(
            &rotated_slots,
            SongLuaOverlayState::default(),
            &asset_manager,
            323,
            1.0,
            1.0,
            [1.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 0.0],
            BlendMode::Alpha,
            0.0,
            0.0,
        )
        .expect("noteskin actor with pre-rotated slots should render");
        let actor_matrix = first_textured_mesh_transform(&actor_rotation);
        let base_matrix = first_textured_mesh_transform(&base_rotation);
        let actor_cols = actor_matrix.to_cols_array();
        let base_cols = base_matrix.to_cols_array();

        assert!(
            actor_cols
                .iter()
                .zip(base_cols.iter())
                .all(|(left, right)| (left - right).abs() <= 0.000_1)
        );
    }

    #[test]
    fn song_lua_song_meter_display_builds_progress_quad() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::SongMeterDisplay {
                stream_width: 100.0,
                stream_state: SongLuaOverlayState {
                    zoom_y: 18.0,
                    diffuse: [1.0, 0.0, 0.0, 0.8],
                    ..SongLuaOverlayState::default()
                },
                music_length_seconds: 100.0,
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 20.0,
                diffuse: [0.5, 1.0, 1.0, 1.0],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            323,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            25.0,
        )
        .expect_actor("SongMeterDisplay overlay should render");

        match actor {
            Actor::Sprite {
                offset,
                scale,
                tint,
                z,
                visible,
                ..
            } => {
                assert_eq!(offset, [270.0, 20.0]);
                assert_eq!(scale, [25.0, 18.0]);
                assert_eq!(tint, [0.5, 0.0, 0.0, 0.8]);
                assert_eq!(z, 323);
                assert!(visible);
            }
            other => panic!("expected sprite-backed SongMeterDisplay quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_graph_display_builds_line_quad() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::GraphDisplay {
                size: [120.0, 60.0],
                body_values: Arc::from([0.5, 0.5]),
                body_state: SongLuaOverlayState {
                    visible: false,
                    ..SongLuaOverlayState::default()
                },
                line_state: SongLuaOverlayState {
                    y: 1.0,
                    diffuse: [0.8, 0.7, 0.6, 0.5],
                    ..SongLuaOverlayState::default()
                },
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 100.0,
                valign: 0.0,
                diffuse: [0.5, 1.0, 1.0, 1.0],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            324,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("GraphDisplay overlay should render");

        match actor {
            Actor::Mesh {
                vertices,
                z,
                visible,
                ..
            } => {
                assert_eq!(z, 324);
                assert_eq!(vertices.len(), 6);
                assert_eq!(vertices[0].pos, [260.0, 131.5]);
                assert_eq!(vertices[1].pos, [260.0, 130.5]);
                assert_eq!(vertices[2].pos, [380.0, 130.5]);
                assert_eq!(vertices[0].color, [0.4, 0.7, 0.6, 0.5]);
                assert!(visible);
            }
            other => panic!("expected mesh-backed GraphDisplay line, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_graph_display_builds_body_and_line_quads() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::GraphDisplay {
                size: [120.0, 60.0],
                body_values: Arc::from([0.25, 0.75]),
                body_state: SongLuaOverlayState {
                    diffuse: [0.2, 0.5, 1.0, 0.75],
                    ..SongLuaOverlayState::default()
                },
                line_state: SongLuaOverlayState {
                    y: 1.0,
                    diffuse: [0.8, 0.7, 0.6, 0.5],
                    ..SongLuaOverlayState::default()
                },
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 100.0,
                valign: 0.0,
                diffuse: [0.5, 1.0, 1.0, 1.0],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            324,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("GraphDisplay overlay should render");

        let Actor::Frame { children, .. } = actor else {
            panic!("expected GraphDisplay body and line frame");
        };
        assert_eq!(children.len(), 2);
        match &children[0] {
            Actor::Mesh {
                vertices, visible, ..
            } => {
                assert_eq!(vertices.len(), 6);
                assert_eq!(vertices[0].pos, [260.0, 145.0]);
                assert_eq!(vertices[1].pos, [260.0, 160.0]);
                assert_eq!(vertices[2].pos, [380.0, 160.0]);
                assert_eq!(vertices[5].pos, [380.0, 115.0]);
                assert_eq!(vertices[0].color, [0.1, 0.5, 1.0, 0.75]);
                assert!(*visible);
            }
            other => panic!("expected mesh-backed GraphDisplay body, got {other:?}"),
        }
        match &children[1] {
            Actor::Mesh { vertices, .. } => {
                assert_eq!(vertices.len(), 6);
                assert_eq!(vertices[0].color, [0.4, 0.7, 0.6, 0.5]);
            }
            other => panic!("expected mesh-backed GraphDisplay line, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_uses_textured_mesh_under_perspective_camera() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                rot_x_deg: 45.0,
                ..SongLuaOverlayState::default()
            },
            Some(SongLuaOverlayState {
                fov: Some(120.0),
                ..SongLuaOverlayState::default()
            }),
            &AssetManager::new(),
            654,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("perspective song lua quad should render");

        match actor {
            Actor::TexturedMesh { vertices, z, .. } => {
                assert_eq!(z, 654);
                assert_eq!(vertices.len(), 6);
            }
            other => panic!("expected projected textured mesh, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_applies_bounce_effect_offset_at_runtime() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                effect_mode: deadlib_present::anim::EffectMode::Bounce,
                effect_clock: deadlib_present::anim::EffectClock::Beat,
                effect_period: 2.0,
                effect_offset: 1.0,
                effect_magnitude: [10.0, 20.0, 5.0],
                z_bias: 2.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            777,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("effect quad should render");

        match actor {
            Actor::Sprite {
                offset,
                world_z,
                scale,
                z,
                ..
            } => {
                let x_scale = screen_width() / 640.0;
                let y_scale = screen_height() / 480.0;
                assert_eq!(z, 777);
                assert!((offset[0] - (320.0 + 10.0) * x_scale).abs() <= 0.000_1);
                assert!((offset[1] - (240.0 + 20.0) * y_scale).abs() <= 0.000_1);
                assert!((world_z - 7.5).abs() <= 0.000_1);
                assert!(scale[0] > 0.0);
                assert!(scale[1] > 0.0);
            }
            other => panic!("expected sprite-backed quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_applies_custom_effect_timing_at_runtime() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                effect_mode: deadlib_present::anim::EffectMode::Bob,
                effect_clock: deadlib_present::anim::EffectClock::Time,
                effect_period: 2.0,
                effect_timing: Some([0.0, 1.0, 0.0, 0.0, 1.0]),
                effect_magnitude: [10.0, 20.0, 5.0],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            778,
            640.0,
            480.0,
            0.5,
            0.0,
            0.0,
        )
        .expect_actor("custom-timed effect quad should render");

        match actor {
            Actor::Sprite {
                offset, world_z, z, ..
            } => {
                let x_scale = screen_width() / 640.0;
                let y_scale = screen_height() / 480.0;
                assert_eq!(z, 778);
                assert!((offset[0] - 320.0 * x_scale).abs() <= 0.000_1);
                assert!((offset[1] - 240.0 * y_scale).abs() <= 0.000_1);
                assert!(world_z.abs() <= 0.000_1);
            }
            other => panic!("expected sprite-backed quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_quad_applies_rainbow_tint_at_runtime() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                rainbow: true,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            779,
            640.0,
            480.0,
            0.5,
            0.0,
            0.5,
        )
        .expect_actor("rainbow quad should render");

        match actor {
            Actor::Sprite { tint, z, .. } => {
                assert_eq!(z, 779);
                assert_eq!(tint, [0.0, 1.0, 1.0, 1.0]);
            }
            other => panic!("expected rainbow sprite-backed quad, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_bitmaptext_applies_rainbow_scroll_at_runtime() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("ABC"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                rainbow_scroll: true,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            780,
            640.0,
            480.0,
            0.0,
            0.0,
            0.4,
        )
        .expect_actor("rainbow-scroll bitmap text should render");

        match actor {
            Actor::Text { attributes, z, .. } => {
                assert_eq!(z, 780);
                assert_eq!(attributes.len(), 3);
                assert_eq!(attributes[0].color, [0.4, 0.3, 0.5, 1.0]);
                assert_eq!(attributes[1].color, [0.2, 0.6, 1.0, 1.0]);
                assert_eq!(attributes[2].color, [0.2, 0.8, 0.8, 1.0]);
            }
            other => panic!("expected rainbow-scroll bitmap text actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_bitmaptext_respects_text_glow_mode_at_runtime() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("GLOW"),
                stroke_color: Some([0.0, 0.0, 0.0, 0.5]),
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actors = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                glow: [0.2, 0.3, 0.4, 0.5],
                text_glow_mode: SongLuaTextGlowMode::Stroke,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            781,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actors("text glow bitmap text should render");

        match actors.as_slice() {
            [
                _,
                Actor::Text {
                    color,
                    stroke_color,
                    attributes,
                    blend,
                    ..
                },
            ] => {
                assert_eq!(color, &[1.0, 1.0, 1.0, 1.0]);
                assert_eq!(stroke_color, &Some([0.2, 0.3, 0.4, 0.5]));
                assert_eq!(blend, &BlendMode::Add);
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].color, [1.0, 1.0, 1.0, 0.0]);
            }
            other => panic!("expected text plus stroke-only glow actors, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_bitmaptext_attribute_glow_adds_runtime_glow_pass() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("GLOW"),
                stroke_color: None,
                attributes: Arc::from([TextAttribute {
                    start: 1,
                    length: 2,
                    color: [1.0, 1.0, 1.0, 1.0],
                    vertex_colors: None,
                    glow: Some([0.7, 0.3, 0.9, 0.5]),
                }]),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actors = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            783,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actors("attribute glow bitmap text should render");

        match actors.as_slice() {
            [
                _,
                Actor::Text {
                    color,
                    stroke_color,
                    attributes,
                    blend,
                    ..
                },
            ] => {
                assert_eq!(color, &[1.0, 1.0, 1.0, 1.0]);
                assert_eq!(stroke_color, &None);
                assert_eq!(blend, &BlendMode::Add);
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].start, 1);
                assert_eq!(attributes[0].length, 2);
                assert_eq!(attributes[0].color, [0.7, 0.3, 0.9, 0.5]);
            }
            other => panic!("expected text plus attribute glow actors, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_setstate_uses_sheet_cell_size_at_runtime() {
        let key = "song-lua-test 4x3.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(5),
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            778,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("setstate sprite should render");

        match actor {
            Actor::Sprite {
                size, uv_rect, z, ..
            } => {
                let expected_w = 10.0 * screen_width() / 640.0;
                let expected_h = 10.0 * screen_height() / 480.0;
                assert_eq!(z, 778);
                assert_eq!(uv_rect, Some([0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0]));
                match size {
                    [SizeSpec::Px(w), SizeSpec::Px(h)] => {
                        assert!((w - expected_w).abs() <= 0.000_1);
                        assert!((h - expected_h).abs() <= 0.000_1);
                    }
                    other => panic!("expected explicit sprite size, got {other:?}"),
                }
            }
            other => panic!("expected sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_animation_advances_sheet_frames_at_runtime() {
        let key = "song-lua-animate 4x3.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(1),
                sprite_animate: true,
                sprite_loop: true,
                sprite_playback_rate: 1.0,
                sprite_state_delay: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            779,
            640.0,
            480.0,
            0.0,
            0.0,
            1.1,
        )
        .expect_actor("animated sprite should render");

        match actor {
            Actor::Sprite { uv_rect, z, .. } => {
                assert_eq!(z, 779);
                assert_eq!(uv_rect, Some([0.75, 0.0, 1.0, 1.0 / 3.0]));
            }
            other => panic!("expected animated sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_animation_applies_rate_and_loop_controls_at_runtime() {
        let key = "song-lua-animate-rate 4x3.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                sprite_state_index: Some(1),
                sprite_animate: true,
                sprite_loop: false,
                sprite_playback_rate: 2.0,
                sprite_state_delay: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            780,
            640.0,
            480.0,
            0.0,
            0.0,
            10.0,
        )
        .expect_actor("rate-controlled sprite should render");

        match actor {
            Actor::Sprite { uv_rect, z, .. } => {
                assert_eq!(z, 780);
                assert_eq!(uv_rect, Some([0.75, 2.0 / 3.0, 1.0, 1.0]));
            }
            other => panic!("expected animated sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_applies_texture_translate_to_uv_rect() {
        let key = "song-lua-translate.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                texture_wrapping: true,
                texcoord_offset: Some([0.25, -0.5]),
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            781,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("translated sprite should render");

        match actor {
            Actor::Sprite { uv_rect, z, .. } => {
                assert_eq!(z, 781);
                assert_eq!(uv_rect, Some([0.25, -0.5, 1.25, 0.5]));
            }
            other => panic!("expected translated sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_renders_vertex_diffuse_as_mesh() {
        let key = "song-lua-vertex-diffuse.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                vertex_colors: Some([
                    [1.0, 0.0, 0.0, 1.0],
                    [0.0, 1.0, 0.0, 1.0],
                    [1.0, 1.0, 0.0, 1.0],
                    [0.0, 0.0, 1.0, 1.0],
                ]),
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            782,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("vertex-diffuse sprite should render");

        match actor {
            Actor::TexturedMesh { vertices, z, .. } => {
                assert_eq!(z, 782);
                assert_eq!(vertices.len(), 6);
                assert_eq!(vertices[0].color, [1.0, 0.0, 0.0, 1.0]);
                assert_eq!(vertices[1].color, [0.0, 1.0, 0.0, 1.0]);
                assert_eq!(vertices[2].color, [1.0, 1.0, 0.0, 1.0]);
                assert_eq!(vertices[5].color, [0.0, 0.0, 1.0, 1.0]);
            }
            other => panic!("expected textured mesh-backed vertex diffuse, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_sprite_applies_fade_edges_at_runtime() {
        let key = "song-lua-fade-edges.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                fadeleft: 0.1,
                faderight: 0.2,
                fadetop: 0.3,
                fadebottom: 0.4,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            782,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("faded sprite should render");

        match actor {
            Actor::Sprite {
                fadeleft,
                faderight,
                fadetop,
                fadebottom,
                z,
                ..
            } => {
                assert_eq!(z, 782);
                assert!((fadeleft - 0.1).abs() <= 0.000_1);
                assert!((faderight - 0.2).abs() <= 0.000_1);
                assert!((fadetop - 0.3).abs() <= 0.000_1);
                assert!((fadebottom - 0.4).abs() <= 0.000_1);
            }
            other => panic!("expected faded sprite overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_skew_at_runtime() {
        let key = "song-lua-skew.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(key.clone(), image::RgbaImage::new(40, 30));
        let overlay = SongLuaOverlayActor {
            kind: test_sprite_kind(&key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &overlay,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                skew_x: 0.5,
                skew_y: 0.25,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            783,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("skewed sprite should render");

        match actor {
            Actor::TexturedMesh { vertices, z, .. } => {
                assert_eq!(z, 783);
                assert_eq!(vertices.len(), 6);
                let x_scale = screen_width() / 640.0;
                let y_scale = screen_height() / 480.0;
                let center = [320.0 * x_scale, 240.0 * y_scale];
                let half = [20.0 * x_scale, 15.0 * y_scale];
                let top_left = test_skewed_overlay_point(center, [-half[0], -half[1]], 0.5, 0.25);
                let bottom_right = test_skewed_overlay_point(center, [half[0], half[1]], 0.5, 0.25);
                assert!((vertices[0].pos[0] - top_left[0]).abs() <= 0.001);
                assert!((vertices[0].pos[1] - top_left[1]).abs() <= 0.001);
                assert!((vertices[2].pos[0] - bottom_right[0]).abs() <= 0.001);
                assert!((vertices[2].pos[1] - bottom_right[1]).abs() <= 0.001);
            }
            other => panic!("expected skewed textured mesh overlay, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_mask_flags_at_runtime() {
        let quad = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let quad_actor = build_song_lua_overlay_actor(
            &quad,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                mask_source: true,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            783,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("masked quad should render");

        match quad_actor {
            Actor::Sprite {
                mask_source,
                mask_dest,
                z,
                ..
            } => {
                assert_eq!(z, 783);
                assert!(mask_source);
                assert!(!mask_dest);
            }
            other => panic!("expected masked quad sprite, got {other:?}"),
        }

        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("MASK"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                mask_dest: true,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            784,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("masked text should render");

        match text_actor {
            Actor::Text { mask_dest, z, .. } => {
                assert_eq!(z, 784);
                assert!(mask_dest);
            }
            other => panic!("expected masked text actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_alignment_at_runtime() {
        let quad = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let quad_actor = build_song_lua_overlay_actor(
            &quad,
            SongLuaOverlayState {
                x: 100.0,
                y: 200.0,
                size: Some([80.0, 40.0]),
                halign: 0.0,
                valign: 1.0,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            785,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("aligned quad should render");

        match quad_actor {
            Actor::Sprite { align, z, .. } => {
                assert_eq!(z, 785);
                assert_eq!(align, [0.0, 1.0]);
            }
            other => panic!("expected aligned quad sprite, got {other:?}"),
        }

        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("ALIGN"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                halign: 1.0,
                valign: 0.0,
                text_align: TextAlign::Right,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            786,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("aligned text should render");

        match text_actor {
            Actor::Text {
                align,
                align_text,
                z,
                ..
            } => {
                assert_eq!(z, 786);
                assert_eq!(align, [1.0, 0.0]);
                assert_eq!(align_text, TextAlign::Right);
            }
            other => panic!("expected aligned text actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_runtime_actor_shadow() {
        let quad = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let quad_actor = build_song_lua_overlay_actor(
            &quad,
            SongLuaOverlayState {
                x: 100.0,
                y: 200.0,
                size: Some([80.0, 40.0]),
                shadow_len: [3.0, -4.0],
                shadow_color: [0.1, 0.2, 0.3, 0.4],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            787,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("shadowed quad should render");

        match quad_actor {
            Actor::Sprite {
                z,
                shadow_len,
                shadow_color,
                ..
            } => {
                assert_eq!(z, 787);
                assert_eq!(shadow_len, [3.0, -4.0]);
                assert_eq!(shadow_color, [0.1, 0.2, 0.3, 0.4]);
            }
            other => panic!("expected shadowed quad sprite, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_extra_blend_modes_at_runtime() {
        let sprite_key = "song-lua-multiply.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(sprite_key.clone(), image::RgbaImage::new(40, 30));

        let sprite = SongLuaOverlayActor {
            kind: test_sprite_kind(&sprite_key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let sprite_actor = build_song_lua_overlay_actor(
            &sprite,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                blend: SongLuaOverlayBlendMode::Multiply,
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            788,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("multiply sprite should render");

        match sprite_actor {
            Actor::Sprite { blend, z, .. } => {
                assert_eq!(z, 788);
                assert_eq!(blend, BlendMode::Multiply);
            }
            other => panic!("expected multiply sprite actor, got {other:?}"),
        }

        let quad = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let quad_actor = build_song_lua_overlay_actor(
            &quad,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                blend: SongLuaOverlayBlendMode::Subtract,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            789,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("subtract quad should render");

        match quad_actor {
            Actor::Sprite { blend, z, .. } => {
                assert_eq!(z, 789);
                assert_eq!(blend, BlendMode::Subtract);
            }
            other => panic!("expected subtract quad actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_wraps_runtime_actors_with_glow() {
        let sprite_key = "song-lua-glow.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(sprite_key.clone(), image::RgbaImage::new(32, 24));

        let sprite = SongLuaOverlayActor {
            kind: test_sprite_kind(&sprite_key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let sprite_actors = build_song_lua_overlay_actor(
            &sprite,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                glow: [0.1, 0.2, 0.3, 0.4],
                ..SongLuaOverlayState::default()
            },
            None,
            &asset_manager,
            790,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actors("glowing sprite should render");

        match sprite_actors.as_slice() {
            [
                Actor::Sprite { blend, z, .. },
                Actor::Sprite {
                    tint,
                    blend: glow_blend,
                    z: glow_z,
                    ..
                },
            ] => {
                assert_eq!(blend, &BlendMode::Alpha);
                assert_eq!(z, &790);
                assert_eq!(tint, &[0.1, 0.2, 0.3, 0.4]);
                assert_eq!(glow_blend, &BlendMode::Add);
                assert_eq!(glow_z, &790);
            }
            other => panic!("expected base sprite plus glow sprite actors, got {other:?}"),
        }

        let quad = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let quad_actors = build_song_lua_overlay_actor(
            &quad,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([100.0, 50.0]),
                diffuse: [1.0, 1.0, 1.0, 0.0],
                effect_mode: deadlib_present::anim::EffectMode::GlowShift,
                effect_color1: [0.3, 0.4, 0.5, 0.6],
                effect_color2: [0.1, 0.2, 0.3, 0.1],
                effect_period: 1.0,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            791,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actors("glowshift quad should render even with zero diffuse alpha");

        match quad_actors.as_slice() {
            [_, Actor::Sprite { tint, blend, .. }] => {
                assert_eq!(tint, &[0.3, 0.4, 0.5, 0.6]);
                assert_eq!(blend, &BlendMode::Add);
            }
            other => panic!("expected base quad plus glowshift sprite actors, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_projected_overlay_applies_fade_edges_at_runtime() {
        let sprite_key = "song-lua-projected-fade.png".to_string();
        let mut asset_manager = AssetManager::new();
        asset_manager.queue_texture_upload(sprite_key.clone(), image::RgbaImage::new(64, 32));

        let sprite = SongLuaOverlayActor {
            kind: test_sprite_kind(&sprite_key),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let actor = build_song_lua_overlay_actor(
            &sprite,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                diffuse: [0.8, 0.7, 0.6, 0.5],
                fadeleft: 0.25,
                faderight: 0.25,
                ..SongLuaOverlayState::default()
            },
            Some(SongLuaOverlayState {
                fov: Some(45.0),
                ..SongLuaOverlayState::default()
            }),
            &asset_manager,
            792,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("projected fading sprite should render");

        match actor {
            Actor::TexturedMesh {
                tint, vertices, z, ..
            } => {
                assert_eq!(z, 792);
                assert_eq!(tint, [0.8, 0.7, 0.6, 0.5]);
                assert_eq!(vertices.len(), 18);
                assert!(vertices.iter().all(|vertex| {
                    (vertex.color[0] - 1.0).abs() <= 0.000_1
                        && (vertex.color[1] - 1.0).abs() <= 0.000_1
                        && (vertex.color[2] - 1.0).abs() <= 0.000_1
                }));
                assert!(vertices.iter().any(|vertex| vertex.color[3] <= 0.000_1));
                assert!(
                    vertices
                        .iter()
                        .any(|vertex| (vertex.color[3] - 1.0).abs() <= 0.000_1)
                );
            }
            other => panic!("expected projected textured mesh, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_bitmaptext_layout_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("WRAP"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                wrap_width_pixels: Some(64),
                max_width: Some(80.0),
                max_height: Some(40.0),
                max_w_pre_zoom: true,
                max_h_pre_zoom: false,
                text_jitter: true,
                text_distortion: 0.5,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            787,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text layout should render");

        match text_actor {
            Actor::Text {
                wrap_width_pixels,
                max_width,
                max_height,
                max_w_pre_zoom,
                max_h_pre_zoom,
                jitter,
                distortion,
                z,
                ..
            } => {
                assert_eq!(z, 787);
                assert_eq!(wrap_width_pixels, Some(64));
                assert_eq!(max_width, Some(80.0));
                assert_eq!(max_height, Some(40.0));
                assert!(max_w_pre_zoom);
                assert!(!max_h_pre_zoom);
                assert!(jitter);
                assert_eq!(distortion, 0.5);
            }
            other => panic!("expected bitmap text actor with layout settings, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_bitmaptext_max_dimension_use_zoom_reaches_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("USEZOOM"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                max_width: Some(80.0),
                max_height: Some(40.0),
                max_w_pre_zoom: true,
                max_h_pre_zoom: true,
                max_dimension_uses_zoom: true,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            0,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text max dimension zoom should render");

        match text_actor {
            Actor::Text {
                max_w_pre_zoom,
                max_h_pre_zoom,
                ..
            } => {
                assert!(!max_w_pre_zoom);
                assert!(!max_h_pre_zoom);
            }
            other => panic!("expected bitmap text actor with max-dimension zoom, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_bitmaptext_attributes_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("ATTR"),
                stroke_color: None,
                attributes: Arc::from([TextAttribute {
                    start: 1,
                    length: 2,
                    color: [0.2, 0.4, 0.6, 0.8],
                    vertex_colors: None,
                    glow: None,
                }]),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            791,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text with attributes should render");

        match text_actor {
            Actor::Text { attributes, z, .. } => {
                assert_eq!(z, 791);
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].start, 1);
                assert_eq!(attributes[0].length, 2);
                assert_eq!(attributes[0].color, [0.2, 0.4, 0.6, 0.8]);
            }
            other => panic!("expected bitmap text actor with attributes, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_bitmaptext_attributes_can_ignore_actor_diffuse_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("ATTR"),
                stroke_color: None,
                attributes: Arc::from([TextAttribute {
                    start: 1,
                    length: 2,
                    color: [0.2, 0.4, 0.6, 0.8],
                    vertex_colors: None,
                    glow: None,
                }]),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                diffuse: [0.5, 0.6, 0.7, 0.9],
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            792,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text with non-multiplied attributes should render");

        match text_actor {
            Actor::Text {
                color,
                attributes,
                z,
                ..
            } => {
                assert_eq!(z, 792);
                assert_eq!(color, [1.0, 1.0, 1.0, 1.0]);
                assert_eq!(attributes.len(), 2);
                assert_eq!(attributes[0].start, 0);
                assert_eq!(attributes[0].length, 4);
                assert_eq!(attributes[0].color, [0.5, 0.6, 0.7, 0.9]);
                assert_eq!(attributes[1].start, 1);
                assert_eq!(attributes[1].length, 2);
                assert_eq!(attributes[1].color, [0.2, 0.4, 0.6, 0.8]);
            }
            other => {
                panic!("expected bitmap text actor with non-multiplied attributes, got {other:?}")
            }
        }
    }

    #[test]
    fn song_lua_overlay_applies_bitmaptext_uppercase_and_vertspacing_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("Mixed Case"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                uppercase: true,
                vert_spacing: Some(18),
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            788,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text uppercase and vertspacing should render");

        match text_actor {
            Actor::Text {
                content,
                line_spacing,
                z,
                ..
            } => {
                assert_eq!(z, 788);
                assert_eq!(content.as_str(), "MIXED CASE");
                assert_eq!(line_spacing, Some(18));
            }
            other => {
                panic!("expected bitmap text actor with uppercase and vertspacing, got {other:?}")
            }
        }
    }

    #[test]
    fn song_lua_overlay_applies_bitmaptext_skew_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("SKEW"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                skew_x: 0.15,
                skew_y: -0.35,
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            789,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text skew should render");

        match text_actor {
            Actor::Text {
                local_transform, z, ..
            } => {
                let actual = local_transform.to_cols_array();
                let expected =
                    song_lua_overlay_local_transform([0.0, 0.0, 0.0], 0.15, -0.35).to_cols_array();
                assert_eq!(z, 789);
                assert!(
                    actual
                        .iter()
                        .zip(expected.iter())
                        .all(|(a, b)| (a - b).abs() <= 0.000_1)
                );
            }
            other => panic!("expected skewed bitmap text actor, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_overlay_applies_bitmaptext_fit_size_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("FIT"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let text_actor = build_song_lua_overlay_actor(
            &text,
            SongLuaOverlayState {
                x: 320.0,
                y: 240.0,
                size: Some([120.0, 30.0]),
                ..SongLuaOverlayState::default()
            },
            None,
            &AssetManager::new(),
            790,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("bitmap text fit size should render");

        match text_actor {
            Actor::Text {
                fit_width,
                fit_height,
                z,
                ..
            } => {
                assert_eq!(z, 790);
                assert_eq!(fit_width, Some(120.0));
                assert_eq!(fit_height, Some(30.0));
            }
            other => panic!("expected bitmap text actor with fit size, got {other:?}"),
        }
    }

    #[test]
    fn song_lua_layer_detects_visible_sprite_texture() {
        let path = std::path::PathBuf::from("badapple.avi");
        let overlays = vec![SongLuaOverlayActor {
            kind: test_sprite_path_kind(path.clone()),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }];
        let states = vec![SongLuaOverlayState::default()];

        assert!(song_lua_has_visible_tex(&overlays, &states, path.as_path()));
    }

    #[test]
    fn song_lua_layer_ignores_hidden_sprite_texture() {
        let path = std::path::PathBuf::from("badapple.avi");
        let overlays = vec![SongLuaOverlayActor {
            kind: test_sprite_path_kind(path.clone()),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }];
        let states = vec![SongLuaOverlayState {
            visible: false,
            ..SongLuaOverlayState::default()
        }];

        assert!(!song_lua_has_visible_tex(
            &overlays,
            &states,
            path.as_path()
        ));
    }

    #[test]
    fn song_lua_overlay_order_sorts_siblings_by_draworder() {
        let overlays = vec![
            test_order_overlay(SongLuaOverlayKind::ActorFrame, None, 20),
            test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 10),
            test_order_overlay(SongLuaOverlayKind::Quad, Some(0), -5),
            test_order_overlay(SongLuaOverlayKind::Quad, None, -10),
        ];
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();

        assert_eq!(
            song_lua_overlay_order(&overlays, &states, None),
            [3, 0, 2, 1]
        );
    }

    #[test]
    fn song_lua_overlay_order_sorts_children_by_z_when_enabled() {
        let overlays = vec![
            SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    draw_by_z_position: true,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            },
            test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 100),
            test_order_overlay(SongLuaOverlayKind::Quad, Some(0), -100),
            test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 0),
        ];
        let mut states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        states[1].z = -20.0;
        states[2].z = 5.0;
        states[3].z = 0.0;

        assert_eq!(
            song_lua_overlay_order(&overlays, &states, None),
            [0, 1, 3, 2]
        );
    }

    #[test]
    fn song_lua_foreground_overlays_cover_notefield_layer() {
        let player_layer = song_lua_player_layer_z(
            true,
            &SongLuaCapturedActor::default(),
            SongLuaOverlayState::default(),
            0.0,
        );
        let highest_notefield_layer = song_lua_add_z(player_layer, 200);
        let foreground_layer = song_lua_add_z(SONG_LUA_OVERLAY_LAYER_Z_BASE, 0);

        assert!(
            highest_notefield_layer <= foreground_layer,
            "foreground Lua should draw over the isolated player/notefield subtree"
        );
    }

    #[test]
    fn song_lua_overlay_delta_applies_depth_filtering_and_draw_by_z() {
        let mut state = SongLuaOverlayState::default();
        apply_song_lua_overlay_delta(
            &mut state,
            &SongLuaOverlayStateDelta {
                depth_test: Some(true),
                draw_by_z_position: Some(true),
                texture_filtering: Some(false),
                ..SongLuaOverlayStateDelta::default()
            },
        );

        assert!(state.depth_test);
        assert!(state.draw_by_z_position);
        assert!(!state.texture_filtering);
    }

    #[test]
    fn gameplay_requires_wait_for_solo_joined_lobby() {
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", false)]);
        assert!(gameplay_requires_lobby_wait_for(Some(&joined)));
    }

    #[test]
    fn gameplay_wait_text_requires_ready_up_for_solo_lobby_player() {
        ensure_i18n();
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", false)]);

        let expected = format!(
            "{}\n{}",
            tr("Lobby", "WaitingForReadyUp"),
            tr("Gameplay", "PressStartToReadyUp"),
        );
        assert_eq!(
            gameplay_lobby_wait_text_for(&joined, false, None).as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn gameplay_wait_text_unlocks_once_solo_lobby_player_is_ready() {
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", true)]);

        assert_eq!(gameplay_lobby_wait_text_for(&joined, true, None), None);
    }
}
