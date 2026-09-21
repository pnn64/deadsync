use crate::SimplyLoveEffect as ThemeEffect;
use crate::act;
use crate::color;
use crate::fonts::machine_font_key;
use crate::i18n::{tr, tr_fmt, tr_fmt_into};
use crate::screens::Screen;
use crate::screens::components::gameplay::score_counter::{
    ScoreCounterParams, prewarm_score_counter_layout, push_score_counter, score_comparison_enabled,
    score_leader_alphas,
};
use crate::screens::components::gameplay::{
    FRAME_TEXT_BPM, FRAME_TEXT_LIFE_BASE, FRAME_TEXT_VERTEX_BUFFERS, gameplay_stats, notefield,
    step_stats_gifs,
};
use crate::screens::components::shared::banner as shared_banner;
use crate::screens::components::shared::density::{self, DensityHistCache};
use crate::screens::components::shared::heart_rate;
pub use crate::screens::components::shared::heart_rate::{HeartRatePlayerView, HeartRateView};
use crate::screens::components::shared::screen_bar::{self, AvatarParams, ScreenBarParams};
use crate::screens::components::shared::{gs_scorebox, lobby_hud};
use crate::screens::input as screen_input;
use crate::views::{GameplayInitView, GameplayRuntimeView, GameplayScoreRuntimeView};
use crate::visual_styles;
use deadlib_assets::AssetManager;
use deadlib_present::actors::{
    Actor, ActorResourceArena, InlineText, RetainedActorFrame, SizeSpec, TextAlign, TextContent,
};
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::compose::{
    ComposeScratch, TextLayoutCache, prewarm_prepared_inline_text_slot,
};
use deadlib_present::font;
use deadlib_present::space::widescale;
use deadlib_present::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width,
};
use deadlib_render_core::{BlendMode, MeshVertex};
use deadsync_assets::noteskin::{self, Noteskin};
use deadsync_chart::{
    ChartData, GameplayChartData, SongBackgroundChange, SongBackgroundChangeTarget, SongData,
};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::song_time::song_time_ns_to_seconds;
use deadsync_gameplay::{
    AUTOSYNC_OFFSET_SAMPLE_COUNT, AutosyncMode, CourseDisplayCarry, CourseDisplayTiming,
    CourseDisplayTotals, CourseLifeConfig, CrossoverRow, ExitTransitionKind,
    FantasticWindowOptions, GameplayAction, GameplayAudioSnapshot, GameplayConfig, GameplayExit,
    GameplayNoteskinData, GameplayNoteskinEffects, GameplayReceptorGlowBehavior,
    GameplayReceptorStepBehavior, GameplaySession, GameplayTween, GameplayViewport, HoldToExitKey,
    LeadInTiming, MINE_EXPLOSION_DURATION, RECEPTOR_STEP_WINDOWS, RECEPTOR_Y_OFFSET_FROM_CENTER,
    RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, ReplayInputEdge, ReplayOffsetSnapshot,
    TAP_EXPLOSION_WINDOWS, autosync_mode_status_line, blue_fantastic_window_ms,
    build_crossover_rows, exit_transition_alpha, handle_core_input, scroll_receptor_y,
    spacing_multiplier_for_percent, update_core,
};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_notefield::{
    BrokenRunLookup, CapturedActorScratch, HoldMeshScratch, ModelMeshCache, StreamProgressLookup,
};
use deadsync_noteskin::{
    NoteskinSlot, ReceptorGlowBehavior, ReceptorStepBehavior, Style, TweenType,
};
use deadsync_online::lobbies as lobby_data;
use deadsync_profile as profile_data;
use deadsync_profile_gameplay::{
    GameplayProfile, gameplay_pack_data, gameplay_runtime_profile_data, profile_side_from_gameplay,
    score_display_mode_from_profile, scroll_effects_from_option,
};
use deadsync_rules::note::Note;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingSegments;
use deadsync_score as score_data;
use deadsync_song_lua::playback::{song_lua_sound_paths, song_meter_progress};
pub type GameplayCoreState = deadsync_song_lua::playback::GameplayCoreState<
    deadsync_profile_gameplay::GameplayProfile,
    deadsync_assets::noteskin::SpriteSlot,
>;
type PreparedGameplaySongLua =
    deadsync_song_lua::playback::PreparedGameplaySongLua<deadsync_assets::noteskin::SpriteSlot>;
use deadsync_song_lua::playback::{
    NOTEFIELD_ACTOR_SCRATCH_CAPACITY, NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
    PLAYER_ACTOR_SCRATCH_CAPACITY,
};
use deadsync_theme::FontRole;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

/// Selects the entry cue during song preparation, including indexed restarts.
#[must_use]
pub fn start_sfx(restart_count: u32) -> Option<PathBuf> {
    if restart_count == 0 {
        deadsync_assets::audio_folder::random_sfx("assets/sounds/song_start")
    } else {
        deadsync_assets::audio_folder::indexed_sfx(
            "assets/sounds/song_start/restart",
            restart_count,
            "restart.ogg",
        )
    }
}

const TEXT_CACHE_LIMIT: usize = 8192;
const INTRO_TEXT_SETTLE_SECONDS: f32 = 1.49; // 0.5 + 0.66 + 0.33 (SL OnCommand chain)
const INTRO_TEXT_GETWIDTH_PAD: f32 = 0.25;
const DIFFICULTY_METER_Y: f32 = 56.0;
const DIFFICULTY_METER_SIZE: f32 = 30.0;
const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;
const DEFAULT_NOTEFIELD_WIDTH: f32 = 256.0;

fn notefield_layout_width(
    column_xs: &[i32],
    receptor_size: [i32; 2],
    cols: usize,
    spacing_multiplier: f32,
) -> f32 {
    let cols = cols.min(column_xs.len());
    if cols == 0 {
        return DEFAULT_NOTEFIELD_WIDTH;
    }
    let (min_x, max_x) =
        column_xs[..cols]
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min_x, max_x), &x| {
                let x = x as f32 * spacing_multiplier;
                (min_x.min(x), max_x.max(x))
            });
    let [width, height] = receptor_size.map(|value| value.max(0) as f32);
    let arrow_width = if height > 0.0 {
        width * (TARGET_ARROW_PIXEL_SIZE / height)
    } else {
        width
    };
    (max_x - min_x) + arrow_width
}

fn gameplay_notefield_widths(
    gameplay: &GameplayCoreState,
    assets: &GameplayNoteskinAssets,
) -> [f32; MAX_PLAYERS] {
    std::array::from_fn(|player| {
        let Some(noteskin) = assets.noteskin[player].as_deref() else {
            return DEFAULT_NOTEFIELD_WIDTH;
        };
        let receptor = assets.receptor_noteskin[player]
            .as_deref()
            .unwrap_or(noteskin);
        let cols = gameplay
            .cols_per_player()
            .min(noteskin.column_xs.len())
            .min(receptor.receptor_off.len());
        let Some(receptor) = receptor.receptor_off.first() else {
            return DEFAULT_NOTEFIELD_WIDTH;
        };
        notefield_layout_width(
            &noteskin.column_xs,
            receptor.size(),
            cols,
            spacing_multiplier_for_percent(gameplay.profiles()[player].spacing_percent),
        )
    })
}

pub use deadsync_notefield::ViewOverride as NotefieldViewOverride;

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

/// Score opacity for Simply Love's two-player "currently winning" treatment.
/// Compare the precise normal score values rather than their formatted or
/// predictive display values, matching the upstream theme's behavior.
pub fn gameplay_score_leader_alphas(state: &GameplayCoreState) -> [f32; MAX_PLAYERS] {
    let show_ex_score = [
        state.profiles()[0].show_ex_score,
        state.profiles()[1].show_ex_score,
    ];
    if !score_comparison_enabled(state.num_players(), show_ex_score) {
        return [1.0; MAX_PLAYERS];
    }

    let scores = if show_ex_score[0] {
        [
            state.display_ex_score_percent(0, player_blue_window_ms(state, 0)),
            state.display_ex_score_percent(1, player_blue_window_ms(state, 1)),
        ]
    } else {
        [
            state.display_itg_score_percent(0),
            state.display_itg_score_percent(1),
        ]
    };
    score_leader_alphas(scores)
}

#[derive(Clone, Copy, Debug)]
pub struct ActorViewOverride {
    pub notefield: NotefieldViewOverride,
    pub hide_gameplay_hud: bool,
    /// Whether chart-attack modifiers may affect the rendered player and
    /// notefield. Practice editing disables this while loop playback keeps the
    /// normal gameplay attack path.
    pub apply_attacks: bool,
    /// Whether song-owned backgrounds, foregrounds, and Lua may participate in
    /// composition. ITGmania unloads them while ScreenPractice is editing and
    /// reloads them for playback.
    pub show_song_visuals: bool,
    /// Alpha multiplier applied to SMX overlay actors (FSR sensor display and pad
    /// input display). Used to fade them in with the screen transition.
    pub smx_overlay_alpha: f32,
}

impl Default for ActorViewOverride {
    fn default() -> Self {
        Self {
            notefield: NotefieldViewOverride::default(),
            hide_gameplay_hud: false,
            apply_attacks: true,
            show_song_visuals: true,
            smx_overlay_alpha: 1.0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct CourseDisplayInfo {
    pub name: Arc<str>,
}

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
const TRANSITION_IN_DURATION: f32 = 2.0;
/// SL/zmod parity: when re-entering Gameplay as a restart, skip the splode +
/// stage-text in-transition (`ScreenGameplay in/default.lua` calls
/// `Hide` immediately when `SL.Global.GameplayReloadCheck` is true). Use a
/// short fade-from-black so the new gameplay frame doesn't pop in.
pub const TRANSITION_IN_RESTART_DURATION: f32 = 0.2;
/// Duration of the black-to-transparent fade that ends the in-transition.
/// The black holds solid for (`TRANSITION_IN_DURATION` - this), then lifts over this window.
pub const TRANSITION_IN_BLACK_FADE_DURATION: f32 = 0.6;
// Simply Love ScreenGameplay out.lua: sleep(0.5), linear(1.0).
pub const TRANSITION_OUT_DELAY: f32 = 0.5;
pub const TRANSITION_OUT_FADE_DURATION: f32 = 1.0;
const TRANSITION_OUT_DURATION: f32 = TRANSITION_OUT_DELAY + TRANSITION_OUT_FADE_DURATION;

#[derive(Clone, Copy, Debug, PartialEq)]
enum BackgroundTransition {
    CrossFade(f32),
    FadeCenterHorizontal,
    FadeCenterVertical,
    FadeDown,
    FadeLeft,
    FadeRight,
    FadeUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    SlideUp,
}

impl BackgroundTransition {
    const fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("CrossFade_Fastest") {
            Some(Self::CrossFade(0.5))
        } else if name.eq_ignore_ascii_case("CrossFade_Faster") {
            Some(Self::CrossFade(0.75))
        } else if name.eq_ignore_ascii_case("CrossFade") {
            Some(Self::CrossFade(1.0))
        } else if name.eq_ignore_ascii_case("FadeCenterHorizontal") {
            Some(Self::FadeCenterHorizontal)
        } else if name.eq_ignore_ascii_case("FadeCenterVertical") {
            Some(Self::FadeCenterVertical)
        } else if name.eq_ignore_ascii_case("FadeDown") {
            Some(Self::FadeDown)
        } else if name.eq_ignore_ascii_case("FadeLeft") {
            Some(Self::FadeLeft)
        } else if name.eq_ignore_ascii_case("FadeRight") {
            Some(Self::FadeRight)
        } else if name.eq_ignore_ascii_case("FadeUp") {
            Some(Self::FadeUp)
        } else if name.eq_ignore_ascii_case("SlideDown") {
            Some(Self::SlideDown)
        } else if name.eq_ignore_ascii_case("SlideLeft") {
            Some(Self::SlideLeft)
        } else if name.eq_ignore_ascii_case("SlideRight") {
            Some(Self::SlideRight)
        } else if name.eq_ignore_ascii_case("SlideUp") {
            Some(Self::SlideUp)
        } else {
            None
        }
    }

    const fn duration(self) -> f32 {
        match self {
            Self::CrossFade(duration) => duration,
            _ => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SongLayer2Event {
    start_second: f32,
    color: Option<[f32; 4]>,
}

pub struct DensityGraphRenderState {
    pub cache: [Option<DensityHistCache>; MAX_PLAYERS],
    pub mesh: [Option<Arc<Vec<MeshVertex>>>; MAX_PLAYERS],
    pub mesh_offset: [f32; MAX_PLAYERS],
    pub life_mesh: [Option<Arc<Vec<MeshVertex>>>; MAX_PLAYERS],
    pub life_mesh_offset: [f32; MAX_PLAYERS],
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

        let mesh: [Option<Arc<Vec<MeshVertex>>>; MAX_PLAYERS] = std::array::from_fn(|player| {
            if player >= state.num_players() || cache[player].is_none() {
                return None;
            }
            let mut mesh = None;
            density::update_density_hist_mesh_reusable(
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
            mesh_offset: [0.0; MAX_PLAYERS],
            life_mesh: std::array::from_fn(|_| None),
            life_mesh_offset: [0.0; MAX_PLAYERS],
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
                                .map(deadsync_noteskin::TapExplosion::duration),
                        );
                    }
                }
            }
        }

        let mine_duration = assets.mine_noteskin[player]
            .as_deref()
            .or_else(|| assets.noteskin[player].as_deref())
            .and_then(|ns| ns.mine_hit_explosion.as_ref())
            .map_or(
                MINE_EXPLOSION_DURATION,
                deadsync_noteskin::TapExplosion::duration,
            );
        effects.set_mine_explosion_duration(player, mine_duration);
    }
    effects
}

#[inline(always)]
const fn gameplay_tween(tween: TweenType) -> GameplayTween {
    match tween {
        TweenType::Linear => GameplayTween::Linear,
        TweenType::Accelerate => GameplayTween::Accelerate,
        TweenType::Decelerate => GameplayTween::Decelerate,
    }
}

#[inline(always)]
const fn gameplay_receptor_glow_behavior(
    behavior: ReceptorGlowBehavior,
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
const fn gameplay_receptor_step_behavior(
    behavior: ReceptorStepBehavior,
) -> GameplayReceptorStepBehavior {
    GameplayReceptorStepBehavior {
        duration: behavior.duration,
        zoom_start: behavior.zoom_start,
        zoom_end: behavior.zoom_end,
        tween: gameplay_tween(behavior.tween),
        interrupts: behavior.interrupts,
    }
}

pub const SMX_SENSOR_PANEL_COUNT: usize = 9;

/// Renderer-neutral sensor values prepared by the shell for one SMX panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmxSensorPanelView {
    pub threshold: u16,
    pub value: Option<u16>,
}

/// Renderer-neutral sensor snapshot prepared by the shell for one SMX pad.
///
/// `fsr` is retained so the shell can interpret later SDK samples without
/// retaining backend config types in the concrete theme state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmxSensorPadView {
    pub fsr: bool,
    pub panels: [SmxSensorPanelView; SMX_SENSOR_PANEL_COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameplayStepStatsMode {
    Hidden,
    Side,
    Versus,
    Double,
}

const fn gameplay_step_stats_mode(
    play_style: profile_data::PlayStyle,
    num_cols: usize,
    p1_enabled: bool,
    p2_enabled: bool,
) -> GameplayStepStatsMode {
    let enabled = match play_style {
        profile_data::PlayStyle::Single
        | profile_data::PlayStyle::Double
        | profile_data::PlayStyle::PumpSingle
        | profile_data::PlayStyle::PumpDouble => p1_enabled,
        profile_data::PlayStyle::Versus | profile_data::PlayStyle::PumpVersus => {
            p1_enabled || p2_enabled
        }
    };
    if !enabled {
        return GameplayStepStatsMode::Hidden;
    }
    if num_cols <= 5 && !play_style.is_versus() {
        GameplayStepStatsMode::Side
    } else {
        match play_style {
            profile_data::PlayStyle::Versus | profile_data::PlayStyle::PumpVersus => {
                GameplayStepStatsMode::Versus
            }
            profile_data::PlayStyle::Double | profile_data::PlayStyle::PumpDouble => {
                GameplayStepStatsMode::Double
            }
            profile_data::PlayStyle::Single | profile_data::PlayStyle::PumpSingle => {
                GameplayStepStatsMode::Hidden
            }
        }
    }
}

#[derive(Default)]
pub struct GameplayFrameScratch {
    lobby_hud_cache: lobby_hud::LobbyHudCache,
    lobby_hud_status_scratch: String,
    bpm_text: GameplayBpmTextPlan,
    presentation_skeleton: GameplayPresentationSkeleton,
}

const LIFE_BOUNCE_S: f32 = 0.1;
const LIFE_SMOOTH_S: f32 = 0.2;
const LIFE_PERCENT_FADE_S: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
struct LifeMeterVisual {
    from_life: f32,
    target_life: f32,
    tween_age: f32,
    hot_age: f32,
    is_hot: bool,
    vertical_swoosh_alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct LifePercentVisual {
    outer_w: f32,
    inner_w: f32,
    alpha: f32,
}

impl LifeMeterVisual {
    fn new(life: f32, dead: bool) -> Self {
        let target_life = visible_life(life, dead);
        let is_hot = !dead && target_life >= 1.0;
        Self {
            from_life: target_life,
            target_life,
            tween_age: LIFE_SMOOTH_S,
            hot_age: 0.0,
            is_hot,
            vertical_swoosh_alpha: 0.2,
        }
    }

    fn update(&mut self, life: f32, dead: bool, delta_time: f32) {
        let target_life = visible_life(life, dead);
        let is_hot = !dead && target_life >= 1.0;
        let delta_time = finite_nonnegative(delta_time);

        if target_life != self.target_life {
            self.from_life = self.target_life;
            self.vertical_swoosh_alpha = if target_life > self.target_life || target_life >= 1.0 {
                1.0
            } else {
                0.2
            };
            self.target_life = target_life;
            self.tween_age = 0.0;
        } else {
            self.tween_age += delta_time;
        }

        if is_hot != self.is_hot {
            self.is_hot = is_hot;
            self.hot_age = 0.0;
        } else {
            self.hot_age += delta_time;
        }
    }

    fn rendered_life(self, lifemeter_type: profile_data::LifeMeterType) -> f32 {
        let (duration, progress) = match lifemeter_type {
            profile_data::LifeMeterType::Surround => {
                (LIFE_SMOOTH_S, smooth_life_p(self.tween_age / LIFE_SMOOTH_S))
            }
            profile_data::LifeMeterType::Standard | profile_data::LifeMeterType::Vertical => (
                LIFE_BOUNCE_S,
                deadlib_present::anim::bouncebegin_p(self.tween_age / LIFE_BOUNCE_S),
            ),
        };
        if self.tween_age >= duration {
            self.target_life
        } else {
            (self.target_life - self.from_life).mul_add(progress, self.from_life)
        }
    }

    fn percent_visual(self) -> LifePercentVisual {
        if !self.is_hot {
            return LifePercentVisual {
                outer_w: 44.0,
                inner_w: 42.0,
                alpha: 1.0,
            };
        }
        let t = (self.hot_age / LIFE_PERCENT_FADE_S).clamp(0.0, 1.0);
        LifePercentVisual {
            outer_w: 52.0,
            inner_w: 50.0,
            alpha: 1.0 - t * t,
        }
    }
}

#[inline]
const fn visible_life(life: f32, dead: bool) -> f32 {
    if dead || !life.is_finite() {
        0.0
    } else {
        life.clamp(0.0, 1.0)
    }
}

#[inline]
const fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

#[inline]
fn smooth_life_p(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    if t <= 0.5 {
        2.0 * t * t
    } else {
        (2.0 * (1.0 - t)).mul_add(-(1.0 - t), 1.0)
    }
}

pub struct State {
    pub gameplay: GameplayCoreState,
    judgment_palettes: [deadsync_theme::color::JudgmentPalette; MAX_PLAYERS],
    /// Game-thread, song-lifetime identity for the fixed two HUD slots. Built
    /// during gameplay setup, read immutably thereafter, and dropped with the
    /// screen; there are no misses, eviction, synchronization, allocations, or
    /// per-frame maintenance.
    hud_snapshot: profile_data::GameplayHudSnapshot,
    pub noteskin_assets: GameplayNoteskinAssets,
    pub density_graph: DensityGraphRenderState,
    pub step_stats_extra_resolved: [crate::step_stats_gifs::ResolvedStepStatsExtra; MAX_PLAYERS],
    pub song_full_title: Arc<str>,
    pub stage_intro_text: Arc<str>,
    pub replay_status_text: Option<Arc<str>>,
    pub course_display_info: Option<CourseDisplayInfo>,
    pub(crate) gameplay_stats_text: gameplay_stats::GameplayStatsTextPlan,
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
    pub scorebox_profile_snapshot: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
    pub scorebox_side_snapshot: [Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
    rival_score_types: [Option<profile_data::MiniIndicatorScoreType>; MAX_PLAYERS],
    missed_target_handled: [bool; MAX_PLAYERS],
    scorebox_plans: [gs_scorebox::GameplayScoreboxPlan; MAX_PLAYERS],
    /// Whether an active scorebox still has an asynchronous leaderboard load
    /// to poll. The game thread updates this two-entry reduction only when a
    /// score snapshot changes; completed songs pay one boolean test per frame.
    scorebox_refresh_pending: bool,
    itl_cmod_warning: [bool; MAX_PLAYERS],
    runtime_view: GameplayRuntimeView,
    live_lobby_runtime: bool,
    lobby_hud_visibility: GameplayLobbyHudVisibility,
    pub lobby_music_started: bool,
    pub lobby_ready_p1: bool,
    pub lobby_ready_p2: bool,
    pub lobby_disconnect_hold_p1: Option<Instant>,
    pub lobby_disconnect_hold_p2: Option<Instant>,
    step_stats_mode: GameplayStepStatsMode,
    pub(crate) song_banner_key: Option<Arc<str>>,
    pub(crate) pack_banner_key: Option<Arc<str>>,
    /// Immutable song background texture identity resolved at screen entry.
    /// `SongBgWithMovieViz` frames clone this handle instead of allocating a new
    /// path string. There are no misses, growth, eviction, or gameplay-thread
    /// destruction; the single optional entry is released with the screen.
    song_background_key: Option<Arc<str>>,
    pub(crate) notefield_model_cache: [RefCell<ModelMeshCache>; MAX_PLAYERS],
    pub(crate) notefield_hold_mesh_scratch: [RefCell<HoldMeshScratch>; MAX_PLAYERS],
    pub(crate) notefield_capture_scratch: [RefCell<CapturedActorScratch>; MAX_PLAYERS],
    notefield_broken_run_lookup: [BrokenRunLookup; MAX_PLAYERS],
    notefield_stream_progress_lookup: [StreamProgressLookup; MAX_PLAYERS],
    /// Fixed song-lifetime width derived at screen entry from immutable noteskin
    /// geometry and profile spacing. The gameplay/render thread only reads this
    /// two-slot array; it has no misses, growth, synchronization, or eviction.
    notefield_widths: [f32; MAX_PLAYERS],
    /// Preferred modifier text is immutable for one song. Screen entry resolves
    /// the bounded global text cache once per player; gameplay only clones the
    /// stored `Arc` while the opening banner can still be visible.
    display_mods_text: [Arc<str>; MAX_PLAYERS],
    /// Music rate is immutable for one gameplay screen. Screen entry resolves
    /// its localized label once; gameplay reads the stored `Arc` directly and
    /// skips the actor entirely for the empty 1.0x label.
    rate_text: Arc<str>,
    /// Two fixed life-text slots owned by the gameplay thread for one song.
    /// Transition prewarm resolves the complete ASCII glyph domain. A miss
    /// writes at most six stack bytes and replaces one player's last value; a
    /// hit is one quantized-key comparison. There is no synchronization,
    /// allocation, or eviction, and destruction occurs with the gameplay state
    /// at screen transition. Six byte writes are the worst-case live-frame
    /// work.
    life_percent_text: GameplayLifeTextPlan,
    /// Fixed two-player, song-lifetime presentation state for Simply Love life
    /// tweens. The gameplay thread advances it once per update and actor
    /// construction reads it without allocation, synchronization, misses,
    /// eviction, or maintenance beyond constant work per active player.
    life_meter_visuals: [LifeMeterVisual; MAX_PLAYERS],
    /// One song-static logical width for the Stage/Event label. The game thread
    /// warms it during the transition, reads it without synchronization during
    /// gameplay, and drops it with the screen. There is no growth or eviction;
    /// a skipped transition causes at most one bounded font measurement miss.
    intro_text_width: Cell<Option<f32>>,
    notefield_judgment_assets: [notefield::ResolvedJudgmentAssets; MAX_PLAYERS],
    notefield_combo_assets: notefield::ResolvedComboMilestoneAssets,
    notefield_plans: [notefield::GameplayNotefieldPlan; MAX_PLAYERS],
    sync_overlay_text_cache: RefCell<SyncOverlayTextCache>,
    pub background_path_dirty: bool,
    pub background_changes: Vec<SongBackgroundChange>,
    /// Song-lifetime beat-to-seconds results parallel to `background_changes`.
    /// Built exactly once at screen entry so video timing never walks the chart
    /// timing map during a live frame. The two vectors have identical lengths
    /// and are immutable until screen destruction.
    background_change_start_seconds: Vec<f32>,
    pub next_background_change_ix: usize,
    /// Song-lifetime layer-2 timeline compiled at screen entry and owned by the
    /// gameplay thread. Capacity is exact and immutable; steady frames advance
    /// the adjacent cursor without misses, growth, pruning, or allocation.
    /// Backward seeks walk only crossed events. Storage is freed with the
    /// screen.
    song_layer2_events: Vec<SongLayer2Event>,
    next_song_layer2_event_ix: Cell<usize>,
    pub current_background_path: Option<PathBuf>,
    pub current_background_key: Option<Arc<str>>,
    pub background_allow_video: bool,
    pub background_texture_key: Arc<str>,
    pub previous_background_texture_key: Option<Arc<str>>,
    /// Screen-owned compiled transition state. Names are parsed when the shell
    /// changes the background; gameplay frames perform no lookup or allocation.
    /// Expiry saturates to one comparison and rewinds can reactivate it. There
    /// is no eviction or growth, and destruction happens with the screen.
    background_transition: Option<BackgroundTransition>,
    background_transition_expired: Cell<bool>,
    background_transition_start_time: f32,
    pub song_media: deadsync_song_lua::playback::SongMedia,
    smx_sensor_views: [Option<SmxSensorPadView>; 2],
    pub heart_rate_view: HeartRateView,
    heart_rate_generation: (u64, u64),
    pub(crate) heart_rate_text: heart_rate::HeartRateTextPlan,
    // Time banked toward the next shell-owned sensor refresh. Seeded to fire on
    // the first frame.
    smx_sensor_refresh_accum: f32,
    pub frame_scratch: Option<Box<GameplayFrameScratch>>,
    pub song_scratch: Option<Box<deadsync_song_lua::playback::FrameScratch>>,
    actor_resources: ActorResourceArena,
}

const STATIC_FILTER: usize = 0;
const STATIC_HEADER: usize = 1;
const STATIC_DIFFICULTY_P1: usize = 2;
const STATIC_DIFFICULTY_P2: usize = 3;
const STATIC_SONG_METER: usize = 4;
const STATIC_LIFE_P1: usize = 5;
const STATIC_LIFE_P2: usize = 6;
const STATIC_FOOTER: usize = 7;
const STATIC_NPS_P1: usize = 8;
const STATIC_NPS_P2: usize = 9;
const STATIC_FRAGMENT_COUNT: usize = 10;
const TOP_SCREEN_HUD_Z: i16 = 2101;

/// Fixed song-static gameplay presentation fragments.
///
/// Owner/thread model: gameplay `State`, used only by the game/render frame
/// loop. Lifetime/capacity: ten fixed slots for one song. Warmup: all visible
/// slots are built during the existing gameplay transition prewarm. A hit emits
/// one compact retained-frame wrapper; a miss builds that immutable slot once.
/// There is no eviction, scan, or live-frame pruning. Screen-size changes clear
/// the fixed slots because their absolute layout is no longer valid; normal
/// destruction occurs with gameplay state. Composition exposes retained-frame
/// hit/miss/saturation counters. Worst-case live work is one slot rebuild after
/// an external resize; steady gameplay only clones one `Arc` per visible slot.
#[derive(Default)]
struct GameplayPresentationSkeleton {
    screen_size: [u32; 2],
    initialized: bool,
    frames: [Option<Arc<RetainedActorFrame>>; STATIC_FRAGMENT_COUNT],
}

impl GameplayPresentationSkeleton {
    fn prepare(&mut self) {
        let screen_size = [screen_width().to_bits(), screen_height().to_bits()];
        if self.initialized && self.screen_size == screen_size {
            return;
        }
        self.screen_size = screen_size;
        self.initialized = true;
        self.frames.fill(None);
    }

    fn push(&mut self, slot: usize, out: &mut Vec<Actor>, build: impl FnOnce(&mut Vec<Actor>)) {
        let frame = self.frames[slot].get_or_insert_with(|| {
            let mut children = Vec::new();
            build(&mut children);
            Arc::new(RetainedActorFrame::new(children))
        });
        out.push(Actor::RetainedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            frame: Arc::clone(frame),
            z: 0,
            tint: [1.0; 4],
            blend: None,
            visible: true,
        });
    }
}

impl State {
    pub const fn machine_font(&self) -> deadsync_config::theme::MachineFont {
        self.runtime_view.policy.machine_font
    }

    #[inline(always)]
    pub fn judgment_palette(&self, player: usize) -> deadsync_theme::color::JudgmentPalette {
        self.judgment_palettes[player.min(MAX_PLAYERS - 1)]
    }

    /// Song-lifetime ITL warning state prepared by the shell on gameplay entry.
    /// Live score polling reuses this snapshot instead of re-reading profile or
    /// song-catalog runtime state every frame.
    #[inline(always)]
    pub const fn itl_cmod_warning_snapshot(&self) -> [bool; MAX_PLAYERS] {
        self.itl_cmod_warning
    }

    #[must_use]
    pub fn from_gameplay(
        gameplay: GameplayCoreState,
        noteskin_assets: GameplayNoteskinAssets,
    ) -> Self {
        let GameplayInitView {
            runtime,
            hud,
            judgment_palettes,
            scores,
            background_changes,
        } = GameplayInitView::default();
        Self::from_gameplay_with_screen_data(
            gameplay,
            noteskin_assets,
            Vec::new(),
            background_changes,
            Arc::from("EVENT"),
            None,
            None,
            Arc::from(""),
            None,
            scores.scorebox_profiles,
            scores.scorebox_snapshots,
            scores.rival_score_types,
            runtime,
            hud,
            judgment_palettes,
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
        scorebox_profile_snapshot: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
        scorebox_side_snapshot: [Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
        rival_score_types: [Option<profile_data::MiniIndicatorScoreType>; MAX_PLAYERS],
        runtime_view: GameplayRuntimeView,
        hud_snapshot: profile_data::GameplayHudSnapshot,
        judgment_palettes: [deadsync_theme::color::JudgmentPalette; MAX_PLAYERS],
    ) -> Self {
        let density_graph = DensityGraphRenderState::from_gameplay(&gameplay);
        let step_stats_profiles =
            std::array::from_fn(|player| gameplay.profiles()[player].0.clone());
        let step_stats_extra_resolved =
            step_stats_gifs::resolve_random_extras(&step_stats_profiles);
        let gameplay_stats_text = gameplay_stats::GameplayStatsTextPlan::from_gameplay(
            &gameplay,
            course_display_info.is_some(),
        );
        let notefield_judgment_assets = std::array::from_fn(|player| {
            notefield::ResolvedJudgmentAssets::from_profile(&step_stats_profiles[player])
        });
        let notefield_combo_assets = notefield::ResolvedComboMilestoneAssets::new();
        let notefield_plans = std::array::from_fn(|player| {
            notefield::gameplay_notefield_plan(
                &step_stats_profiles[player],
                &notefield_judgment_assets[player],
                gameplay.player_blue_window_ms(player) / 1000.0,
                judgment_palettes[player],
            )
        });
        let scorebox_plans = std::array::from_fn(|side| {
            gs_scorebox::GameplayScoreboxPlan::new_with_palette(
                scorebox_side_snapshot[side].as_ref(),
                &scorebox_profile_snapshot[side],
                runtime_view.policy.scorebox_pane_filter,
                judgment_palettes[side],
            )
        });
        let scorebox_refresh_pending = scorebox_refresh_pending_from(
            &scorebox_profile_snapshot,
            &scorebox_side_snapshot,
            &rival_score_types,
        );
        let song = gameplay.song();
        let song_full_title: Arc<str> =
            Arc::from(song.display_full_title(runtime_view.policy.translated_titles));
        let song_banner_key = song
            .banner_path
            .as_deref()
            .map(deadlib_assets::media_path_key);
        let pack_banner_key = pack_banner_path
            .as_deref()
            .map(deadlib_assets::media_path_key);
        let song_background_key = song
            .background_path
            .as_deref()
            .map(deadlib_assets::media_path_key);
        let notefield_model_cache =
            notefield_model_cache_from_assets(&noteskin_assets, gameplay.num_players());
        let notefield_hold_mesh_scratch = std::array::from_fn(|player| {
            let columns = usize::from(player < gameplay.num_players()) * gameplay.cols_per_player();
            RefCell::new(HoldMeshScratch::with_columns(columns))
        });
        let notefield_capture_scratch = std::array::from_fn(|player| {
            let active = player < gameplay.num_players();
            RefCell::new(CapturedActorScratch::with_capacities(
                usize::from(active) * NOTEFIELD_ACTOR_SCRATCH_CAPACITY,
                usize::from(active) * NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
            ))
        });
        let notefield_broken_run_lookup = std::array::from_fn(|player| {
            BrokenRunLookup::new(gameplay.measure_counter_segments(player))
        });
        let notefield_stream_progress_lookup = std::array::from_fn(|player| {
            StreamProgressLookup::new(gameplay.mini_indicator_stream_segments(player))
        });
        let notefield_widths = gameplay_notefield_widths(&gameplay, &noteskin_assets);
        let display_mods_text =
            std::array::from_fn(|player| notefield::preferred_mods_text(&gameplay, player));
        let rate_text = cached_rate_text(gameplay.music_rate());
        let bpm_text = GameplayBpmTextPlan::new(
            display_bpm(gameplay.current_bpm_display(), gameplay.music_rate()),
            runtime_view.policy.show_bpm_decimal,
        );
        let life_percent_text = GameplayLifeTextPlan::new();
        let life_meter_visuals = std::array::from_fn(|player| {
            let player = &gameplay.players()[player];
            LifeMeterVisual::new(player.life, player.is_failing || player.life <= 0.0)
        });
        let actor_resources = ActorResourceArena::default();
        notefield::prewarm_actor_resources(
            &actor_resources,
            &noteskin_assets,
            &step_stats_profiles,
            gameplay.num_players(),
        );
        for assets in notefield_judgment_assets
            .iter()
            .take(gameplay.num_players())
        {
            assets.prewarm();
        }
        let background_transition_start_time = gameplay.current_music_time_display();
        let next_background_change_ix = background_changes
            .iter()
            .take_while(|change| change.start_beat <= gameplay.current_beat())
            .count();
        let background_change_start_seconds = background_changes
            .iter()
            .map(|change| gameplay.music_time_for_beat(change.start_beat))
            .collect();
        let song_layer2_events = build_song_layer2_events(&gameplay);
        let next_song_layer2_event_ix = song_layer2_events
            .partition_point(|event| event.start_second <= gameplay.current_music_time_display());
        let song_scratch = Box::new(deadsync_song_lua::playback::FrameScratch::new(&gameplay));
        let frame_scratch = GameplayFrameScratch {
            lobby_hud_cache: lobby_hud::LobbyHudCache::default(),
            lobby_hud_status_scratch: String::with_capacity(128),
            bpm_text,
            presentation_skeleton: GameplayPresentationSkeleton::default(),
        };
        let song_media =
            deadsync_song_lua::playback::SongMedia::new(&gameplay, song_lua_sound_paths);
        let step_stats_mode = gameplay_step_stats_mode(
            hud_snapshot.play_style,
            gameplay.num_cols(),
            !step_stats_profiles[0].step_statistics.is_empty(),
            !step_stats_profiles[1].step_statistics.is_empty(),
        );
        let state = Self {
            gameplay,
            judgment_palettes,
            hud_snapshot,
            noteskin_assets,
            density_graph,
            step_stats_extra_resolved,
            song_full_title,
            stage_intro_text,
            replay_status_text,
            course_display_info,
            gameplay_stats_text,
            pack_group,
            pack_banner_path,
            scorebox_profile_snapshot,
            scorebox_side_snapshot,
            rival_score_types,
            missed_target_handled: [false; MAX_PLAYERS],
            scorebox_plans,
            scorebox_refresh_pending,
            itl_cmod_warning: [false; MAX_PLAYERS],
            live_lobby_runtime: runtime_view.lobby.snapshot.joined_lobby.is_some(),
            lobby_hud_visibility: GameplayLobbyHudVisibility::default(),
            runtime_view,
            lobby_music_started: false,
            lobby_ready_p1: false,
            lobby_ready_p2: false,
            lobby_disconnect_hold_p1: None,
            lobby_disconnect_hold_p2: None,
            step_stats_mode,
            song_banner_key,
            pack_banner_key,
            song_background_key,
            notefield_model_cache,
            notefield_hold_mesh_scratch,
            notefield_capture_scratch,
            notefield_broken_run_lookup,
            notefield_stream_progress_lookup,
            notefield_widths,
            display_mods_text,
            rate_text,
            life_percent_text,
            life_meter_visuals,
            intro_text_width: Cell::new(None),
            notefield_judgment_assets,
            notefield_combo_assets,
            notefield_plans,
            sync_overlay_text_cache: RefCell::new(SyncOverlayTextCache::default()),
            background_path_dirty: true,
            background_changes,
            background_change_start_seconds,
            next_background_change_ix,
            song_layer2_events,
            next_song_layer2_event_ix: Cell::new(next_song_layer2_event_ix),
            current_background_path: None,
            current_background_key: None,
            background_allow_video: false,
            background_texture_key: Arc::from("__black"),
            previous_background_texture_key: None,
            background_transition: None,
            background_transition_expired: Cell::new(false),
            background_transition_start_time,
            song_media,
            smx_sensor_views: [None, None],
            heart_rate_view: HeartRateView::default(),
            heart_rate_generation: (u64::MAX, u64::MAX),
            heart_rate_text: heart_rate::HeartRateTextPlan::default(),
            smx_sensor_refresh_accum: SMX_SENSOR_REFRESH_INTERVAL,
            frame_scratch: Some(Box::new(frame_scratch)),
            song_scratch: Some(song_scratch),
            actor_resources,
        };
        state
    }

    #[inline(always)]
    pub(crate) const fn notefield_judgment_assets(
        &self,
        player_idx: usize,
    ) -> &notefield::ResolvedJudgmentAssets {
        &self.notefield_judgment_assets[player_idx]
    }

    #[inline(always)]
    pub(crate) const fn notefield_plan(
        &self,
        player_idx: usize,
    ) -> &notefield::GameplayNotefieldPlan {
        &self.notefield_plans[player_idx]
    }

    /// Borrows this frame's offscreen passes in song-layer dependency order.
    pub fn song_frame(&self) -> &deadsync_song_lua::playback::FrameScratch {
        self.song_scratch
            .as_deref()
            .expect("song frame restored after composition")
    }

    pub fn render_targets(&self) -> &[deadlib_present::actors::RenderTarget] {
        self.song_scratch
            .as_ref()
            .map_or(&[], |scratch| scratch.render_targets())
    }

    #[inline(always)]
    pub const fn actor_resources(&self) -> &ActorResourceArena {
        &self.actor_resources
    }

    #[inline(always)]
    pub fn active_background_start_sec(&self) -> Option<f32> {
        active_background_start_sec(
            &self.background_change_start_seconds,
            self.next_background_change_ix,
        )
    }

    #[inline(always)]
    fn notefield_width(&self, player: usize) -> f32 {
        self.notefield_widths
            .get(player)
            .copied()
            .unwrap_or(DEFAULT_NOTEFIELD_WIDTH)
    }

    #[inline(always)]
    const fn display_mods_text(&self, player: usize) -> &Arc<str> {
        &self.display_mods_text[player]
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

fn banner_visibility(
    play_style: profile_data::PlayStyle,
    num_cols: usize,
    wide: bool,
    ultrawide: bool,
    first_mask: profile_data::StepStatisticsMask,
    second_mask: profile_data::StepStatisticsMask,
) -> (bool, bool) {
    if play_style.is_single() && num_cols <= play_style.cols_per_player() {
        (
            first_mask.contains(profile_data::StepStatisticsMask::SONG_BANNER),
            wide && first_mask.pack_info_enabled(),
        )
    } else if play_style.is_double() && wide && !ultrawide {
        (
            first_mask.contains(profile_data::StepStatisticsMask::SONG_BANNER),
            first_mask.pack_info_enabled(),
        )
    } else if play_style.is_versus() && wide && !ultrawide {
        (
            (first_mask | second_mask).contains(profile_data::StepStatisticsMask::SONG_BANNER),
            false,
        )
    } else {
        (false, false)
    }
}

/// Video banner sources that the current Gameplay Step Statistics layout will draw.
pub fn visible_banner_paths(state: &State) -> [Option<&Path>; 2] {
    let first_mask = state
        .profiles()
        .first()
        .map_or(profile_data::StepStatisticsMask::empty(), |p| {
            p.step_statistics
        });
    let second_mask = state
        .profiles()
        .get(1)
        .map_or(profile_data::StepStatisticsMask::empty(), |p| {
            p.step_statistics
        });
    let (song_visible, pack_visible) = banner_visibility(
        state.runtime_view.play_style,
        state.num_cols(),
        is_wide(),
        screen_width() / screen_height().max(1.0) > (21.0 / 9.0),
        first_mask,
        second_mask,
    );

    [
        if song_visible {
            state.song().banner_path.as_deref()
        } else {
            None
        },
        if pack_visible {
            state.pack_banner_path.as_deref()
        } else {
            None
        },
    ]
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
    let annotations = match cols_per_player {
        4 => {
            let (rows, row_to_beat, _) = build_crossover_rows::<4>(notes, note_range, col_start);
            deadsync_simfile::timing::crossover_annotations::<4>(
                &rows,
                &row_to_beat,
                timing_segments,
            )
        }
        8 => {
            let (rows, row_to_beat, _) = build_crossover_rows::<8>(notes, note_range, col_start);
            deadsync_simfile::timing::crossover_annotations::<8>(
                &rows,
                &row_to_beat,
                timing_segments,
            )
        }
        _ => return Vec::new(),
    };
    annotations
        .iter()
        .map(|annotation| CrossoverRow {
            beat: annotation.beat,
            column_mask: annotation.column_mask,
            crossover: annotation.crossover,
            bracket: annotation.bracket,
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
            skin.for_each_slot(|slot| {
                // Prewarming is required in optimized builds too. Keep the
                // side effect outside `debug_assert!`, which does not evaluate
                // its condition when debug assertions are disabled.
                let retained = cache.prewarm_slot(slot);
                debug_assert!(
                    retained,
                    "noteskin slot frame cache was sealed before prewarming completed"
                );
            });
        }
        cache.seal();
        cache.reset_stats();
    }
}

fn notefield_model_cache_slot_count(assets: &GameplayNoteskinAssets, player: usize) -> usize {
    let mut stable_ids = HashSet::new();
    for skin in [
        assets.noteskin[player].as_ref(),
        assets.mine_noteskin[player].as_ref(),
        assets.receptor_noteskin[player].as_ref(),
        assets.tap_explosion_noteskin[player].as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        skin.for_each_slot(|slot| {
            stable_ids.insert(slot.stable_id());
        });
    }
    stable_ids.len()
}

pub fn notefield_model_cache_from_assets(
    assets: &GameplayNoteskinAssets,
    num_players: usize,
) -> [RefCell<ModelMeshCache>; MAX_PLAYERS] {
    let cache: [RefCell<ModelMeshCache>; MAX_PLAYERS] = std::array::from_fn(|player| {
        RefCell::new(if player < num_players {
            ModelMeshCache::with_capacity(notefield_model_cache_slot_count(assets, player))
        } else {
            ModelMeshCache::default()
        })
    });
    prewarm_notefield_model_cache_slots(&cache, assets, num_players);
    cache
}

pub fn gameplay_noteskin_assets(
    cols_per_player: usize,
    num_players: usize,
    runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
) -> GameplayNoteskinAssets {
    use deadsync_noteskin::runtime::SkinPart;
    let style = Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
    // Compose once on the song-load worker. The selected slots then share the
    // existing prewarm, upload and song-lifetime ownership paths.
    let noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return None;
        }
        let profile = &runtime_profiles[player];
        let mut options = profile.current_player_options();
        profile_data::migrate_noteskin_parts(&mut options);
        let skin = options.noteskin.as_str();
        let mut result = noteskin::load_itg_skin_cached(&style, skin)
            .or_else(|error| {
                log::warn!("Cannot load noteskin '{skin}': {error}; using the bundled default");
                noteskin::load_itg_skin_cached(&style, profile_data::NoteSkin::DEFAULT_NAME)
            })
            .ok()?;
        for (part, selection) in [
            (SkinPart::Arrows, &options.arrow_noteskin),
            (SkinPart::Receptors, &options.receptor_noteskin),
            (SkinPart::HoldActive, &options.hold_active_noteskin),
            (SkinPart::HoldInactive, &options.hold_inactive_noteskin),
            (SkinPart::RollActive, &options.roll_active_noteskin),
            (SkinPart::RollInactive, &options.roll_inactive_noteskin),
            (SkinPart::TapExplosions, &options.tap_explosion_noteskin),
            (SkinPart::HoldExplosions, &options.hold_explosion_noteskin),
            (SkinPart::Mines, &options.mine_noteskin),
            (SkinPart::Lifts, &options.lift_noteskin),
        ] {
            let Some(selection) = selection else { continue };
            if selection.is_none_choice() {
                continue;
            }
            match noteskin::load_itg_skin_cached(&style, selection.as_str()) {
                Ok(source) => Arc::make_mut(&mut result).apply_part(&source, part),
                Err(error) => log::warn!(
                    "Cannot load {part:?} from '{selection}': {error}; using the base component"
                ),
            }
        }
        Some(result)
    });
    let tap_explosion_noteskin = std::array::from_fn(|player| {
        if runtime_profiles[player].tap_explosion_noteskin_hidden() {
            None
        } else {
            noteskin[player].clone()
        }
    });
    GameplayNoteskinAssets {
        mine_noteskin: noteskin.clone(),
        receptor_noteskin: noteskin.clone(),
        tap_explosion_noteskin,
        noteskin,
    }
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
    course_modifiers: Option<Arc<str>>,
    course_life_config: [CourseLifeConfig; MAX_PLAYERS],
    include_post_fail_passes: bool,
    course_display_info: Option<CourseDisplayInfo>,
    course_banner_path: Option<PathBuf>,
    combo_carry: [u32; MAX_PLAYERS],
    song_lua_data: PreparedGameplaySongLua,
    init_view: GameplayInitView,
) -> State {
    let GameplayInitView {
        runtime,
        hud,
        judgment_palettes,
        scores,
        background_changes,
    } = init_view;
    let cols_per_player = session.play_style.cols_per_player();
    let num_players = session.play_style.player_count();
    let runtime_profile_data = gameplay_runtime_profile_data(&player_profiles, &session);
    let noteskin_assets =
        gameplay_noteskin_assets(cols_per_player, num_players, &runtime_profile_data);
    let noteskin_data =
        noteskin_assets.gameplay_data(cols_per_player, num_players, &runtime_profile_data);
    let player_profiles = player_profiles.map(GameplayProfile::from);
    let song_lua_sound_paths = song_lua_sound_paths(&song_lua_data);
    let pack_data = gameplay_pack_data(
        &song,
        course_display_info.as_ref().map(|info| &info.name),
        course_banner_path.as_ref(),
    );
    let pack_group = pack_data.pack_group;
    let pack_banner_path = pack_data.pack_banner_path;
    let pack_sync_pref = pack_data.sync_pref;
    State::from_gameplay_with_screen_data(
        deadsync_gameplay::init_gameplay_runtime(
            song,
            charts,
            gameplay_charts,
            viewport,
            session,
            config,
            pack_sync_pref,
            scores.mini_indicator,
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
            course_modifiers,
            course_life_config,
            include_post_fail_passes,
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
        scores.scorebox_profiles,
        scores.scorebox_snapshots,
        scores.rival_score_types,
        runtime,
        hud,
        judgment_palettes,
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
const fn map_gameplay_action(action: GameplayAction) -> ThemeEffect {
    match action {
        GameplayAction::None => ThemeEffect::None,
        GameplayAction::Navigate(exit) => ThemeEffect::Navigate(screen_for_exit(exit)),
        GameplayAction::NavigateNoFade(exit) => ThemeEffect::NavigateNoFade(screen_for_exit(exit)),
    }
}

fn local_lobby_side_is_active(state: &State, side: profile_data::PlayerSide) -> bool {
    let [p1_joined, p2_joined] = state.runtime_view.joined;
    if !(p1_joined || p2_joined) {
        return state.runtime_view.player_side == side;
    }
    match side {
        profile_data::PlayerSide::P1 => p1_joined,
        profile_data::PlayerSide::P2 => p2_joined,
    }
}

#[derive(Debug)]
struct GameplayLobbyHudVisibility {
    visible: bool,
    menu_lr_chord: screen_input::MenuLrChordTracker,
}

impl Default for GameplayLobbyHudVisibility {
    fn default() -> Self {
        Self {
            visible: true,
            menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        }
    }
}

impl GameplayLobbyHudVisibility {
    fn handle_input(
        &mut self,
        ev: &InputEvent,
        joined_lobby: bool,
        waiting: bool,
        joined_sides: [bool; 2],
        fallback_side: profile_data::PlayerSide,
    ) -> bool {
        if !joined_lobby || waiting {
            self.menu_lr_chord = screen_input::MenuLrChordTracker::default();
            return false;
        }
        // Keep gameplay arrows available to the chart. Only explicitly mapped
        // menu buttons participate in the lobby-HUD chord.
        let side = match ev.action {
            VirtualAction::p1_menu_left | VirtualAction::p1_menu_right => {
                profile_data::PlayerSide::P1
            }
            VirtualAction::p2_menu_left | VirtualAction::p2_menu_right => {
                profile_data::PlayerSide::P2
            }
            _ => return false,
        };
        let [p1_joined, p2_joined] = joined_sides;
        let side_is_active = if p1_joined || p2_joined {
            match side {
                profile_data::PlayerSide::P1 => p1_joined,
                profile_data::PlayerSide::P2 => p2_joined,
            }
        } else {
            side == fallback_side
        };
        if !side_is_active || self.menu_lr_chord.update(ev).is_none() {
            return false;
        }
        self.visible = !self.visible;
        true
    }

    #[inline(always)]
    const fn should_render(&self, joined_lobby: bool, waiting: bool) -> bool {
        joined_lobby && (waiting || self.visible)
    }
}

fn intro_text_width_for_font(asset_manager: &AssetManager, font_key: &str, text: &str) -> f32 {
    asset_manager.with_fonts(|all_fonts| {
        asset_manager
            .with_font(font_key, |f| {
                font::measure_line_width_logical(f, text, all_fonts) as f32
            })
            .unwrap_or(0.0)
            .max(0.0)
    })
}

fn intro_text_width(asset_manager: &AssetManager, state: &State, text: &str) -> f32 {
    intro_text_width_for_font(
        asset_manager,
        machine_font_key(state.machine_font(), FontRole::Header),
        text,
    )
}

#[inline]
fn cached_intro_text_width(cache: &Cell<Option<f32>>, measure: impl FnOnce() -> f32) -> f32 {
    if let Some(width) = cache.get() {
        return width;
    }
    let width = measure();
    cache.set(Some(width));
    width
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
        && (play_style.is_double() || (play_style.is_single() && center_1player_notefield));
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
    let text_width = cached_intro_text_width(&state.intro_text_width, || {
        intro_text_width(asset_manager, state, text)
    });
    text_width
        .mul_add(INTRO_TEXT_GETWIDTH_PAD, notefield_width * 0.5)
        .mul_add(side_sign, screen_center_x())
}

fn push_system_stage_label(actors: &mut Vec<Actor>, state: &State, asset_manager: &AssetManager) {
    let intro_text = state.stage_intro_text.as_ref();
    let is_restart_label = intro_text.starts_with("RESTART ");
    if intro_text.is_empty()
        || (!is_restart_label && state.total_elapsed_in_screen() < INTRO_TEXT_SETTLE_SECONDS)
    {
        return;
    }
    let text_x = intro_text_target_x(
        state,
        asset_manager,
        intro_text,
        state.hud_snapshot.play_style,
        state.hud_snapshot.player_side,
        state.runtime_view.policy.center_single_notefield,
    );
    actors.push(act!(text:
        font(machine_font_key(state.machine_font(), FontRole::Header)): settext(state.stage_intro_text.clone()):
        align(0.5, 0.5): xy(text_x, screen_height() - 30.0):
        zoom(0.4):
        shadowlength(1.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(TOP_SCREEN_HUD_Z)
    ));
}

fn system_footer_player(
    player: &profile_data::GameplayHudPlayerSnapshot,
) -> Option<(&str, Option<AvatarParams<'_>>)> {
    player.joined.then(|| {
        let text = if player.guest || player.hide_username {
            ""
        } else {
            player.display_name.as_str()
        };
        let avatar = if player.guest {
            None
        } else {
            player
                .avatar_texture_key
                .as_deref()
                .map(|texture_key| AvatarParams { texture_key })
        };
        (text, avatar)
    })
}

fn push_system_profile_footer(
    actors: &mut Vec<Actor>,
    state: &State,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
    presentation_skeleton: &mut GameplayPresentationSkeleton,
) {
    let hud = &state.hud_snapshot;
    let p1_footer = system_footer_player(&hud.p1);
    let p2_footer = system_footer_player(&hud.p2);
    let (left_text, right_text, left_avatar, right_avatar) = if hud.play_style.is_versus() {
        (
            p1_footer.map(|footer| footer.0),
            p2_footer.map(|footer| footer.0),
            p1_footer.and_then(|footer| footer.1),
            p2_footer.and_then(|footer| footer.1),
        )
    } else {
        match hud.player_side {
            profile_data::PlayerSide::P1 => (
                p1_footer.map(|footer| footer.0),
                None,
                p1_footer.and_then(|footer| footer.1),
                None,
            ),
            profile_data::PlayerSide::P2 => (
                None,
                p2_footer.map(|footer| footer.0),
                None,
                p2_footer.and_then(|footer| footer.1),
            ),
        }
    };
    presentation_skeleton.push(STATIC_FOOTER, actors, |actors| {
        let mut footer = screen_bar::build_no_background(ScreenBarParams {
            visual_policy,
            title: "",
            title_placement: screen_bar::ScreenBarTitlePlacement::Center,
            position: screen_bar::ScreenBarPosition::Bottom,
            transparent: true,
            fg_color: [1.0; 4],
            left_text,
            center_text: None,
            right_text,
            left_avatar,
            right_avatar,
        });
        // ScreenBar's text is at local z=2. Keep the composed text on the
        // protected TopScreen layer, above song-local foreground/AFT sprites.
        match &mut footer {
            Actor::Frame { z, .. } | Actor::SharedFrame { z, .. } => {
                *z = TOP_SCREEN_HUD_Z - 2;
            }
            _ => unreachable!("screen bar builders always return frames"),
        }
        actors.push(footer);
    });
}

fn gameplay_player_index_for_side(state: &State, side: profile_data::PlayerSide) -> Option<usize> {
    if state.num_players() >= 2 {
        return Some(profile_data::player_side_index(side));
    }
    if state.num_players() == 0 || state.runtime_view.player_side != side {
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

fn gameplay_bpm_x(
    position: deadsync_config::theme::GameplayBpmPosition,
    num_players: usize,
    play_style: profile_data::PlayStyle,
    player_side: profile_data::PlayerSide,
    playfield_center_x: f32,
    field_width: f32,
    nps_graph_at_top: bool,
) -> f32 {
    if position == deadsync_config::theme::GameplayBpmPosition::NearField
        && num_players == 1
        && play_style.is_single()
    {
        let side = if player_side == profile_data::PlayerSide::P1 {
            1.0
        } else {
            -1.0
        };
        return playfield_center_x + side * field_width.mul_add(0.5, 20.0);
    }

    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
    if num_players == 1 && note_field_is_centered && nps_graph_at_top {
        let side_shift = if player_side == profile_data::PlayerSide::P1 {
            0.3
        } else {
            -0.3
        };
        return screen_width().mul_add(side_shift, screen_center_x());
    }

    screen_center_x()
}

fn offset_gameplay_hud_x(
    base_x: f32,
    player_side: profile_data::PlayerSide,
    note_field_offset_x: i32,
) -> f32 {
    let side_sign = if player_side == profile_data::PlayerSide::P1 {
        -1.0
    } else {
        1.0
    };
    base_x + side_sign * note_field_offset_x.clamp(0, 50) as f32
}

fn upper_nps_graph_x(
    player_side: profile_data::PlayerSide,
    notefield_x: f32,
    graph_w: f32,
    note_field_offset_x: i32,
) -> f32 {
    let base_x = if (notefield_x - screen_center_x()).abs() < 1.0 {
        graph_w.mul_add(-0.5, screen_center_x())
    } else if player_side == profile_data::PlayerSide::P1 {
        screen_center_x() - graph_w - widescale(45.0, 95.0)
    } else {
        screen_center_x() + widescale(45.0, 95.0)
    };
    offset_gameplay_hud_x(base_x, player_side, note_field_offset_x)
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
    let receptor_y_centered = f32::midpoint(receptor_y_normal, receptor_y_reverse);
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
        profile_data::PlayerSide::P2 => DIFFICULTY_METER_SIZE.mul_add(-0.5, screen_width()),
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
    let resting_x = offset_gameplay_hud_x(normal_x, player_side, profile.note_field_offset_x);
    if difficulty_meter_hits_targets(
        state,
        profile,
        player_idx,
        field_x,
        field_w,
        resting_x,
        DIFFICULTY_METER_Y,
    ) {
        side_difficulty_meter_x(player_side)
    } else {
        resting_x
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

fn append_lobby_machine_state(state: &State, effects: &mut Vec<ThemeEffect>) {
    if !lobby_data::can_update_machine_state(&state.runtime_view.lobby.snapshot) {
        return;
    }

    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    effects.push(crate::effects::lobby(
        crate::SimplyLoveLobbyRequest::UpdateMachineStats {
            screen_name: "ScreenGameplay",
            p1_ready,
            p2_ready,
            p1_stats: gameplay_lobby_player_stats(state, profile_data::PlayerSide::P1),
            p2_stats: gameplay_lobby_player_stats(state, profile_data::PlayerSide::P2),
        },
    ));
}

fn local_lobby_ready_tuple(state: &State) -> (bool, bool) {
    (
        local_lobby_side_is_active(state, profile_data::PlayerSide::P1) && state.lobby_ready_p1,
        local_lobby_side_is_active(state, profile_data::PlayerSide::P2) && state.lobby_ready_p2,
    )
}

fn local_lobby_players_ready(state: &State) -> bool {
    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    let mut any_active = false;
    let mut all_ready = true;
    if local_lobby_side_is_active(state, profile_data::PlayerSide::P1) {
        any_active = true;
        all_ready &= p1_ready;
    }
    if local_lobby_side_is_active(state, profile_data::PlayerSide::P2) {
        any_active = true;
        all_ready &= p2_ready;
    }
    any_active && all_ready
}

fn set_all_local_lobby_players_ready(state: &mut State, ready: bool) {
    state.lobby_ready_p1 = local_lobby_side_is_active(state, profile_data::PlayerSide::P1) && ready;
    state.lobby_ready_p2 = local_lobby_side_is_active(state, profile_data::PlayerSide::P2) && ready;
}

fn set_local_lobby_player_ready(state: &mut State, side: profile_data::PlayerSide) {
    match side {
        profile_data::PlayerSide::P1
            if local_lobby_side_is_active(state, profile_data::PlayerSide::P1) =>
        {
            state.lobby_ready_p1 = true;
        }
        profile_data::PlayerSide::P2
            if local_lobby_side_is_active(state, profile_data::PlayerSide::P2) =>
        {
            state.lobby_ready_p2 = true;
        }
        _ => {}
    }
}

const fn clear_lobby_disconnect_holds(state: &mut State) {
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
            if local_lobby_side_is_active(state, profile_data::PlayerSide::P1) =>
        {
            state.lobby_disconnect_hold_p1 = started_at;
        }
        profile_data::PlayerSide::P2
            if local_lobby_side_is_active(state, profile_data::PlayerSide::P2) =>
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

fn gameplay_requires_lobby_wait(state: &State) -> bool {
    lobby_data::gameplay_lobby_wait_required(
        state.runtime_view.lobby.snapshot.joined_lobby.as_ref(),
    )
}

fn write_gameplay_lobby_wait_text(
    joined: &lobby_data::JoinedLobby,
    local_players_ready: bool,
    reconnect_status_text: Option<&str>,
    text: &mut String,
) -> bool {
    text.clear();
    if let Some(reconnect) = reconnect_status_text {
        text.push_str(reconnect);
        return true;
    }

    let key = match lobby_data::gameplay_lobby_wait_status(joined, "ScreenGameplay") {
        lobby_data::GameplayLobbyWaitStatus::Ready => return false,
        lobby_data::GameplayLobbyWaitStatus::WaitingForReadyUp => "WaitingForReadyUp",
        lobby_data::GameplayLobbyWaitStatus::WaitingForSync => "WaitingForSync",
    };
    text.push_str(&tr("Lobby", key));
    if !local_players_ready {
        text.push('\n');
        text.push_str(&tr("Gameplay", "PressStartToReadyUp"));
    }
    true
}

fn gameplay_lobby_wait_active(state: &State) -> bool {
    if state.lobby_music_started {
        return false;
    }
    let Some(joined) = state.runtime_view.lobby.snapshot.joined_lobby.as_ref() else {
        return false;
    };
    state.runtime_view.lobby.reconnect_status_text.is_some()
        || lobby_data::gameplay_lobby_wait_status(joined, "ScreenGameplay")
            != lobby_data::GameplayLobbyWaitStatus::Ready
}

fn write_gameplay_lobby_hud_status(state: &State, text: &mut String) -> bool {
    text.clear();
    if !gameplay_lobby_wait_active(state) {
        return false;
    }
    let joined = state
        .runtime_view
        .lobby
        .snapshot
        .joined_lobby
        .as_ref()
        .expect("active lobby wait has a joined lobby");
    if !write_gameplay_lobby_wait_text(
        joined,
        local_lobby_players_ready(state),
        state.runtime_view.lobby.reconnect_status_text.as_deref(),
        text,
    ) {
        return false;
    }
    text.push('\n');
    if let Some(elapsed) = lobby_disconnect_hold_elapsed(state) {
        let remaining = (state.runtime_view.lobby.disconnect_hold_seconds - elapsed)
            .ceil()
            .max(0.0) as i32;
        let mut remaining_text = InlineText::new();
        let inserted = remaining_text.push_i32(remaining);
        debug_assert!(inserted, "every i32 fits in InlineText");
        tr_fmt_into(
            text,
            "Lobby",
            "DisconnectHoldingFormat",
            &[
                ("remaining", remaining_text.as_str()),
                ("s", if remaining == 1 { "" } else { "s" }),
            ],
        );
    } else {
        text.push_str(&tr("Lobby", "DisconnectBasicPrompt"));
    }
    true
}

pub const fn scorebox_snapshot_for_side(
    state: &State,
    side: profile_data::PlayerSide,
) -> Option<&score_data::CachedPlayerLeaderboardData> {
    state.scorebox_side_snapshot[profile_data::player_side_index(side)].as_ref()
}

pub const fn scorebox_profile_for_side(
    state: &State,
    side: profile_data::PlayerSide,
) -> &score_data::GameplayScoreboxProfileSnapshot {
    &state.scorebox_profile_snapshot[profile_data::player_side_index(side)]
}

pub fn push_scorebox_actors_for_side(
    actors: &mut Vec<Actor>,
    state: &State,
    side: profile_data::PlayerSide,
    center_x: f32,
    center_y: f32,
    zoom: f32,
) {
    state.scorebox_plans[profile_data::player_side_index(side)].push_actors(
        actors,
        center_x,
        center_y,
        zoom,
        state.current_music_time_display(),
    );
}

pub fn on_enter(state: &mut State) {
    state.lobby_music_started = false;
    set_all_local_lobby_players_ready(state, false);
    clear_lobby_disconnect_holds(state);

    if gameplay_requires_lobby_wait(state) {
        return;
    }

    set_all_local_lobby_players_ready(state, true);
    state.start_stage_music();
    state.lobby_music_started = true;
}

pub const fn on_exit(state: &mut State) {
    state.smx_sensor_views = [None, None];
}

#[inline(always)]
pub fn sync_runtime_view(state: &mut State, view: GameplayRuntimeView) {
    if state.runtime_view.policy.scorebox_pane_filter != view.policy.scorebox_pane_filter {
        state.scorebox_plans = std::array::from_fn(|side| {
            gs_scorebox::GameplayScoreboxPlan::new_with_palette(
                state.scorebox_side_snapshot[side].as_ref(),
                &state.scorebox_profile_snapshot[side],
                view.policy.scorebox_pane_filter,
                state.judgment_palettes[side],
            )
        });
    }
    state.runtime_view = view;
}

/// Whether this stage entered Gameplay from a joined lobby and therefore needs
/// live lobby/reconnect snapshots until the stage ends.
#[inline(always)]
pub const fn uses_live_lobby_runtime(state: &State) -> bool {
    state.live_lobby_runtime
}

/// Refresh only the stage's live lobby state. Gameplay policy, play style,
/// joined sides, and player side are fixed at stage construction.
#[inline(always)]
pub fn sync_lobby_runtime_view(state: &mut State, lobby: crate::views::SimplyLoveLobbyRuntimeView) {
    state.runtime_view.lobby = lobby;
}

#[inline(always)]
fn scorebox_refresh_pending_from(
    profiles: &[score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
    snapshots: &[Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
    rival_score_types: &[Option<profile_data::MiniIndicatorScoreType>; MAX_PLAYERS],
) -> bool {
    profiles.iter().zip(snapshots).zip(rival_score_types).any(
        |((profile, snapshot), rival_score_type)| {
            (profile.display_scorebox || rival_score_type.is_some())
                && profile.gs_active
                && snapshot.as_ref().is_some_and(|snapshot| snapshot.loading)
        },
    )
}

/// Whether gameplay still needs the shell to poll asynchronous score loading.
#[inline(always)]
pub const fn scorebox_refresh_pending(state: &State) -> bool {
    state.scorebox_refresh_pending
}

#[inline(always)]
pub fn sync_score_runtime_view(state: &mut State, view: GameplayScoreRuntimeView) {
    for (side, update) in view.scorebox_updates.into_iter().enumerate() {
        if update.is_some() {
            state.scorebox_side_snapshot[side] = update;
            state.scorebox_plans[side] = gs_scorebox::GameplayScoreboxPlan::new_with_palette(
                state.scorebox_side_snapshot[side].as_ref(),
                &state.scorebox_profile_snapshot[side],
                state.runtime_view.policy.scorebox_pane_filter,
                state.judgment_palettes[side],
            );
        }
    }
    for (player, rival_score) in view.rival_score_updates.into_iter().enumerate() {
        if let Some(rival_score) = rival_score {
            state
                .gameplay
                .set_mini_indicator_rival_score_percent(player, rival_score);
        }
    }
    state.scorebox_refresh_pending = scorebox_refresh_pending_from(
        &state.scorebox_profile_snapshot,
        &state.scorebox_side_snapshot,
        &state.rival_score_types,
    );
    state.itl_cmod_warning = view.itl_cmod_warning;
}

pub const fn rival_score_type_for_side(
    state: &State,
    side: profile_data::PlayerSide,
) -> Option<profile_data::MiniIndicatorScoreType> {
    state.rival_score_types[profile_data::player_side_index(side)]
}

/// Runs concrete-theme work that must happen before the deterministic gameplay
/// update. Returns `false` while an online lobby is still waiting for players.
///
/// Starting stage music only queues a gameplay audio command. The shell drains
/// that command before it samples the stream clock for [`update`].
pub fn prepare_update(state: &mut State, effects: &mut Vec<ThemeEffect>) -> bool {
    if !state.lobby_music_started {
        if lobby_disconnect_hold_elapsed(state)
            .is_some_and(|elapsed| elapsed >= state.runtime_view.lobby.disconnect_hold_seconds)
        {
            clear_lobby_disconnect_holds(state);
            effects.push(crate::effects::lobby(
                crate::SimplyLoveLobbyRequest::Disconnect,
            ));
            lobby_data::apply_local_lobby_disconnect(std::sync::Arc::make_mut(
                &mut state.runtime_view.lobby.snapshot,
            ));
            state.runtime_view.lobby.reconnect_status_text = None;
        }

        append_lobby_machine_state(state, effects);

        if gameplay_lobby_wait_active(state) {
            return false;
        }

        clear_lobby_disconnect_holds(state);
        set_all_local_lobby_players_ready(state, true);
        state.start_stage_music();
        state.lobby_music_started = true;
    }
    append_lobby_machine_state(state, effects);
    true
}

/// Advances deterministic gameplay using the shell-prepared audio snapshot.
/// Runtime audio commands remain queued until the shell executes them.
pub fn update(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    effects: &mut Vec<ThemeEffect>,
) {
    let action = update_core(state, delta_time, audio_snapshot, fallback_host_nanos);
    update_life_meter_visuals(state, delta_time);
    match action {
        GameplayAction::None => {
            if let Some(effect) = missed_target_effect(state) {
                effects.push(effect);
            }
        }
        action => effects.push(map_gameplay_action(action)),
    }
}

fn update_life_meter_visuals(state: &mut State, delta_time: f32) {
    for player in 0..state.gameplay.num_players().min(MAX_PLAYERS) {
        let runtime = &state.gameplay.players()[player];
        let life = runtime.life;
        let dead = runtime.is_failing || life <= 0.0;
        state.life_meter_visuals[player].update(life, dead, delta_time);
    }
}

fn missed_target_effect(state: &mut State) -> Option<ThemeEffect> {
    for player_idx in 0..state.gameplay.num_players().min(MAX_PLAYERS) {
        if state.missed_target_handled[player_idx] {
            continue;
        }
        let profile = &state.gameplay.profiles()[player_idx];
        let policy = profile.target_score_miss_policy;
        if matches!(
            policy,
            profile_data::TargetScoreMissPolicy::Nothing
                | profile_data::TargetScoreMissPolicy::DimMiniIndicator
        ) {
            continue;
        }
        let score_type = profile.mini_indicator_score_type;
        let target_score_percent = state
            .gameplay
            .mini_indicator_target_score_percent(player_idx);
        if !notefield::zmod_target_score_missed(
            &state.gameplay,
            player_idx,
            score_type,
            target_score_percent,
        ) {
            continue;
        }

        state.missed_target_handled[player_idx] = true;
        return Some(match policy {
            profile_data::TargetScoreMissPolicy::Fail => {
                state.gameplay.force_fail_player(player_idx);
                ThemeEffect::NavigateNoFade(Screen::Evaluation)
            }
            profile_data::TargetScoreMissPolicy::RestartSong => {
                ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::RestartGameplay)
            }
            profile_data::TargetScoreMissPolicy::Nothing
            | profile_data::TargetScoreMissPolicy::DimMiniIndicator => unreachable!(),
        });
    }
    None
}

/// The pads to drive for the FSR sensor display, as `(store_index, sdk_pad)`:
/// `store_index` is how the sensor arrays are keyed (profile index normally, SDK
/// pad in Doubles) and `sdk_pad` is the SDK pad to enable/read. `None` slots are
/// skipped. Returns all-`None` cheaply (before any config/session lookup) when no
/// player wants the display or SMX input is off, so the per-frame caller does no
/// further work.
pub fn smx_sensor_pad_plan(state: &State, smx_input: bool) -> [Option<(usize, usize)>; 2] {
    let mut out = [None, None];
    if !state.profiles()[0].smx_fsr_display && !state.profiles()[1].smx_fsr_display {
        return out;
    }
    if !smx_input {
        return out;
    }
    if state.runtime_view.play_style.is_double() {
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

pub fn smx_sensor_refresh_due(state: &mut State, delta_time: f32) -> bool {
    state.smx_sensor_refresh_accum += delta_time;
    if state.smx_sensor_refresh_accum < SMX_SENSOR_REFRESH_INTERVAL {
        return false;
    }
    // Keep the leftover so cadence stays steady, but cap it so a long stall
    // (load spike, alt-tab) can't bank up a burst of catch-up refreshes.
    state.smx_sensor_refresh_accum = (state.smx_sensor_refresh_accum - SMX_SENSOR_REFRESH_INTERVAL)
        .min(SMX_SENSOR_REFRESH_INTERVAL);
    true
}

pub fn smx_sensor_pad_view(state: &State, store_idx: usize) -> Option<SmxSensorPadView> {
    state.smx_sensor_views.get(store_idx).copied().flatten()
}

pub fn set_smx_sensor_pad_view(
    state: &mut State,
    store_idx: usize,
    view: Option<SmxSensorPadView>,
) {
    if let Some(slot) = state.smx_sensor_views.get_mut(store_idx) {
        *slot = view;
    }
}

pub const fn heart_rate_generation(state: &State) -> (u64, u64) {
    state.heart_rate_generation
}

pub fn set_heart_rate_view(state: &mut State, generation: (u64, u64), view: HeartRateView) {
    state.heart_rate_text.sync(view);
    state.heart_rate_view = view;
    state.heart_rate_generation = generation;
}

/// Refresh song-rate-derived presentation only on an explicit Practice rate
/// change. Ordinary gameplay never calls this from the frame loop.
pub fn sync_music_rate_text(state: &mut State) {
    let rate = state.music_rate();
    let max_nps = std::array::from_fn(|player| state.charts()[player].max_nps as f32);
    state.rate_text = cached_rate_text(rate);
    state.gameplay_stats_text.sync_music_rate(max_nps, rate);
}

pub fn runtime_profile_side(state: &State, player_idx: usize) -> profile_data::PlayerSide {
    profile_side_from_gameplay(state.runtime_player_side(player_idx))
}

pub const fn smx_sensor_profile_enabled(state: &State) -> bool {
    state.runtime_view.policy.smx_profile_enabled
}

pub fn record_smx_sensor_read_ns(state: &State, elapsed_ns: u64) {
    smx_profile::record_read(state.runtime_view.policy.smx_profile_enabled, elapsed_ns);
}

pub fn report_smx_sensor_profile(state: &State) {
    smx_profile::maybe_report(state.runtime_view.policy.smx_profile_enabled);
}

pub fn handle_input(state: &mut State, ev: &InputEvent, effects: &mut Vec<ThemeEffect>) {
    let lobby_waiting = gameplay_lobby_wait_active(state);
    let joined_lobby = state.runtime_view.lobby.snapshot.joined_lobby.is_some();
    let lobby_hud_toggled = state.lobby_hud_visibility.handle_input(
        ev,
        joined_lobby,
        lobby_waiting,
        state.runtime_view.joined,
        state.runtime_view.player_side,
    );
    if lobby_waiting {
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
        return;
    }
    if lobby_hud_toggled {
        return;
    }
    let action = handle_core_input(state, ev);
    map_gameplay_action(action).append_to(effects);
}

thread_local! {
    static RATE_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(128));
    static METER_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(64));
    static AUTOSYNC_TEXT_CACHE: RefCell<TextCache<AutosyncTextKey>> =
        RefCell::new(text_cache_with_capacity(256));
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
fn quantize_tenths_u32(value: f32) -> u32 {
    let value = if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    };
    ((value * 10.0).round()).clamp(0.0, u32::MAX as f32) as u32
}

#[inline(always)]
fn cached_rate_text(rate: f32) -> Arc<str> {
    let rate = if rate.is_finite() { rate } else { 1.0 };
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

fn owned_bpm_text(bpm: f64, show_decimal: bool) -> String {
    if !bpm.is_finite() {
        return "0".to_owned();
    }
    if !show_decimal {
        let rounded = bpm.round().max(0.0);
        return format!("{rounded:.0}");
    }
    let rounded_thousandth = (bpm * 1_000.0).round() / 1_000.0;
    let rounded_thousandth = rounded_thousandth.max(0.0);
    let mut text = format!("{rounded_thousandth:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[inline]
fn inline_bpm_text(bpm: f64, show_decimal: bool) -> Option<InlineText> {
    if !bpm.is_finite() {
        return InlineText::copy_from("0");
    }
    if !show_decimal {
        let rounded = bpm.round().max(0.0);
        if rounded > f64::from(u32::MAX) {
            return None;
        }
        let mut text = InlineText::new();
        return text.push_u32(rounded as u32).then_some(text);
    }

    let scaled = (bpm * 1_000.0).round().max(0.0);
    if scaled > f64::from(u32::MAX).mul_add(1_000.0, 999.0) {
        return None;
    }
    let scaled = scaled as u64;
    let whole = (scaled / 1_000) as u32;
    let fraction = (scaled % 1_000) as u16;
    let mut text = InlineText::new();
    if !text.push_u32(whole) || fraction == 0 {
        return (fraction == 0).then_some(text);
    }
    if !text.push_ascii(b'.') {
        return None;
    }
    let hundreds = (fraction / 100) as u8;
    let tens = ((fraction / 10) % 10) as u8;
    let ones = (fraction % 10) as u8;
    let wrote = text.push_ascii(b'0' + hundreds)
        && (fraction.is_multiple_of(100)
            || (text.push_ascii(b'0' + tens)
                && (fraction.is_multiple_of(10) || text.push_ascii(b'0' + ones))));
    wrote.then_some(text)
}

#[inline(always)]
fn display_bpm(base_bpm: f32, music_rate: f32) -> f64 {
    let rate = if music_rate.is_finite() {
        f64::from(music_rate)
    } else {
        1.0
    };
    if base_bpm.is_finite() {
        f64::from(base_bpm) * rate
    } else {
        0.0
    }
}

enum GameplayBpmText {
    Inline(InlineText),
    Shared(Arc<str>),
}

/// Song-owned current-BPM formatter for the game/render frame thread.
///
/// Capacity/lifetime: one last-value entry plus one prepared geometry slot for
/// the gameplay screen. Warmup happens during transition text prewarm. Normal
/// BPM values through `u32::MAX` format into 14 stack bytes; stable frames only
/// compare the key and copy that payload. There is no lookup, pruning,
/// synchronization, or eviction. Values too wide for the fixed payload retain
/// the previous owned-string behavior as a cold compatibility fallback. The
/// screen transition drops that fallback. Worst normal-frame work is ten
/// decimal digit writes plus the optional three fractional digits.
struct GameplayBpmTextPlan {
    key: (u64, bool),
    text: GameplayBpmText,
}

impl GameplayBpmTextPlan {
    fn new(bpm: f64, show_decimal: bool) -> Self {
        Self {
            key: (bpm.to_bits(), show_decimal),
            text: Self::format(bpm, show_decimal),
        }
    }

    #[inline(always)]
    fn resolve(&mut self, bpm: f64, show_decimal: bool) -> TextContent {
        let key = (bpm.to_bits(), show_decimal);
        if self.key != key {
            self.key = key;
            self.text = Self::format(bpm, show_decimal);
        }
        match &self.text {
            GameplayBpmText::Inline(text) => TextContent::frame_inline_slot(*text, FRAME_TEXT_BPM),
            GameplayBpmText::Shared(text) => TextContent::Shared(Arc::clone(text)),
        }
    }

    #[cold]
    fn format(bpm: f64, show_decimal: bool) -> GameplayBpmText {
        inline_bpm_text(bpm, show_decimal).map_or_else(
            || GameplayBpmText::Shared(Arc::from(owned_bpm_text(bpm, show_decimal))),
            GameplayBpmText::Inline,
        )
    }
}

impl Default for GameplayBpmTextPlan {
    fn default() -> Self {
        Self::new(0.0, false)
    }
}

struct GameplayLifeTextPlan {
    cached: [Cell<Option<(u16, InlineText)>>; MAX_PLAYERS],
}

impl GameplayLifeTextPlan {
    fn new() -> Self {
        Self {
            cached: std::array::from_fn(|_| Cell::new(None)),
        }
    }

    #[inline(always)]
    fn resolve(&self, life_percent: f32, player: usize) -> TextContent {
        let key = quantize_tenths_u32(life_percent).min(1_000) as u16;
        let player = player.min(MAX_PLAYERS - 1);
        let text = match self.cached[player].get() {
            Some((cached, text)) if cached == key => text,
            _ => {
                let mut text = InlineText::new();
                let whole = u32::from(key) / 10;
                let tenth = (key % 10) as u8;
                let wrote_whole = text.push_u32(whole);
                let wrote_dot = text.push_ascii(b'.');
                let wrote_tenth = text.push_ascii(b'0' + tenth);
                let wrote_percent = text.push_ascii(b'%');
                debug_assert!(wrote_whole && wrote_dot && wrote_tenth && wrote_percent);
                self.cached[player].set(Some((key, text)));
                text
            }
        };
        TextContent::frame_inline_slot(text, FRAME_TEXT_LIFE_BASE + player as u8)
    }
}

#[inline]
fn rainbow_life_color(elapsed: f32) -> [f32; 4] {
    let through = (finite_nonnegative(elapsed) / 2.0).rem_euclid(1.0);
    let between = ((through + 0.25) * std::f32::consts::TAU)
        .sin()
        .mul_add(0.5, 0.5);
    let phase = between * std::f32::consts::TAU;
    let r = phase.cos().mul_add(0.5, 0.5);
    let g = (phase + std::f32::consts::TAU / 3.0)
        .cos()
        .mul_add(0.5, 0.5);
    let b = (phase + 2.0 * std::f32::consts::TAU / 3.0)
        .cos()
        .mul_add(0.5, 0.5);
    [r, g, b, 1.0]
}

#[inline]
fn responsive_life_color(life: f32) -> [f32; 4] {
    let life = life.clamp(0.0, 1.0);
    if life >= 0.9 {
        [0.0, 1.0, ((life - 0.9) * 10.0).clamp(0.0, 1.0), 1.0]
    } else if life >= 0.5 {
        [((0.9 - life) * 2.5).clamp(0.0, 1.0), 1.0, 0.0, 1.0]
    } else {
        [1.0, ((life - 0.2) * (10.0 / 3.0)).clamp(0.0, 1.0), 0.0, 1.0]
    }
}

#[inline]
fn life_fill_color(
    profile: &profile_data::Profile,
    life: f32,
    dead: bool,
    elapsed: f32,
    fallback: impl FnOnce() -> [f32; 4],
) -> [f32; 4] {
    if !dead && life >= 1.0 {
        if profile.rainbow_max {
            rainbow_life_color(elapsed)
        } else {
            [1.0; 4]
        }
    } else if profile.responsive_colors {
        responsive_life_color(life)
    } else {
        fallback()
    }
}

#[inline]
fn surround_life_color(profile: &profile_data::Profile, life: f32, elapsed: f32) -> [f32; 4] {
    let mut color = if profile.responsive_colors {
        let mut color = responsive_life_color(life);
        color[3] = 0.2;
        color
    } else {
        [0.2, 0.2, 0.2, 1.0]
    };
    if life >= 1.0 && profile.rainbow_max {
        color = rainbow_life_color(elapsed);
        color[3] = if profile.responsive_colors { 0.2 } else { 1.0 };
    }
    color
}

#[inline]
fn visible_life_percent_text(
    text_plan: &GameplayLifeTextPlan,
    player: usize,
    life_percent: f32,
    lifemeter_type: profile_data::LifeMeterType,
    enabled: bool,
    standard_layout_visible: bool,
) -> Option<TextContent> {
    let visible = enabled
        && match lifemeter_type {
            profile_data::LifeMeterType::Standard => standard_layout_visible,
            profile_data::LifeMeterType::Vertical => true,
            profile_data::LifeMeterType::Surround => false,
        };
    visible.then(|| text_plan.resolve(life_percent, player))
}

#[inline]
fn life_swoosh_velocity_x(state: &State, player: usize) -> f32 {
    let song_position = state.notefield_song_position(player);
    if song_position.is_in_freeze || song_position.is_in_delay {
        0.0
    } else {
        -(song_position.bpm / 60.0 * 0.5)
    }
}

fn prewarm_life_text_slots(
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
    num_players: usize,
) {
    let life_glyphs =
        InlineText::copy_from(".%0123456789").expect("the life-percent glyph domain fits inline");
    for player in 0..num_players.min(MAX_PLAYERS) {
        prewarm_prepared_inline_text_slot(
            cache,
            scratch,
            fonts,
            "miso",
            life_glyphs,
            FRAME_TEXT_LIFE_BASE + player as u8,
            TextAlign::Left,
            FRAME_TEXT_VERTEX_BUFFERS,
        );
    }
}

fn prewarm_bpm_text_slot(
    cache: &mut TextLayoutCache,
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
) {
    let bpm_glyphs =
        InlineText::copy_from(".0123456789").expect("the BPM glyph domain fits inline");
    prewarm_prepared_inline_text_slot(
        cache,
        scratch,
        fonts,
        "miso",
        bpm_glyphs,
        FRAME_TEXT_BPM,
        TextAlign::Center,
        FRAME_TEXT_VERTEX_BUFFERS,
    );
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
fn quantized_offset_changed(start: f32, new: f32) -> bool {
    let delta = quantize_offset_seconds(new) - quantize_offset_seconds(start);
    !(delta.abs() < 0.000_1_f32)
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

fn sync_offset_overlay_message_from_values(
    initial_global_offset: f32,
    global_offset: f32,
    initial_song_offset: f32,
    song_offset: f32,
) -> Option<String> {
    let mut message = String::new();
    if let Some(global_line) =
        quantized_offset_change_line("Global Offset", initial_global_offset, global_offset)
    {
        message.push_str(&global_line);
    }
    if let Some(song_line) =
        quantized_offset_change_line("Song offset", initial_song_offset, song_offset)
    {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(&song_line);
    }
    (!message.is_empty()).then_some(message)
}

fn sync_offset_overlay_message(state: &State) -> Option<String> {
    sync_offset_overlay_message_from_values(
        state.initial_global_offset_seconds(),
        state.global_offset_seconds(),
        state.initial_song_offset_seconds(),
        state.song_offset_seconds(),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SyncOverlayTextKey {
    autoplay_enabled: bool,
    timing_tick_status: Option<&'static str>,
    autosync_status: Option<&'static str>,
    initial_global_offset_bits: u32,
    global_offset_bits: u32,
    initial_song_offset_bits: u32,
    song_offset_bits: u32,
}

#[derive(Clone, Copy)]
struct SyncOverlayTextInput<'a> {
    autoplay_enabled: bool,
    replay_status: Option<&'a Arc<str>>,
    timing_tick_status: Option<&'static str>,
    autosync_status: Option<&'static str>,
    initial_global_offset: f32,
    global_offset: f32,
    initial_song_offset: f32,
    song_offset: f32,
}

impl SyncOverlayTextInput<'_> {
    #[inline(always)]
    fn is_idle(self) -> bool {
        !self.autoplay_enabled
            && self.timing_tick_status.is_none()
            && self.autosync_status.is_none()
            && !quantized_offset_changed(self.initial_global_offset, self.global_offset)
            && !quantized_offset_changed(self.initial_song_offset, self.song_offset)
    }

    fn key(self) -> SyncOverlayTextKey {
        SyncOverlayTextKey {
            autoplay_enabled: self.autoplay_enabled,
            timing_tick_status: self.timing_tick_status,
            autosync_status: self.autosync_status,
            initial_global_offset_bits: quantize_offset_seconds(self.initial_global_offset)
                .to_bits(),
            global_offset_bits: quantize_offset_seconds(self.global_offset).to_bits(),
            initial_song_offset_bits: quantize_offset_seconds(self.initial_song_offset).to_bits(),
            song_offset_bits: quantize_offset_seconds(self.song_offset).to_bits(),
        }
    }
}

#[derive(Default)]
struct SyncOverlayTextCache {
    initialized: bool,
    key: Option<SyncOverlayTextKey>,
    replay_status: Option<Arc<str>>,
    value: Option<(Arc<str>, usize)>,
}

impl SyncOverlayTextCache {
    fn resolve(&mut self, input: SyncOverlayTextInput<'_>) -> Option<(Arc<str>, usize)> {
        let key = input.key();
        let replay_unchanged =
            self.replay_status.as_deref() == input.replay_status.map(AsRef::as_ref);
        if self.initialized && self.key == Some(key) && replay_unchanged {
            return self
                .value
                .as_ref()
                .map(|(text, line_count)| (Arc::clone(text), *line_count));
        }

        let value = compose_sync_overlay_text(input);
        self.initialized = true;
        self.key = Some(key);
        self.replay_status = input.replay_status.cloned();
        self.value = value
            .as_ref()
            .map(|(text, line_count)| (Arc::clone(text), *line_count));
        value
    }
}

fn autoplay_overlay_text() -> Arc<str> {
    static AUTOPLAY: OnceLock<Arc<str>> = OnceLock::new();
    Arc::clone(AUTOPLAY.get_or_init(|| Arc::from("AutoPlay")))
}

fn compose_sync_overlay_text(input: SyncOverlayTextInput<'_>) -> Option<(Arc<str>, usize)> {
    let mut lines = [""; 4];
    let mut line_count = 0usize;
    let mut total_len = 0usize;
    let sync_message = sync_offset_overlay_message_from_values(
        input.initial_global_offset,
        input.global_offset,
        input.initial_song_offset,
        input.song_offset,
    );
    if input.autoplay_enabled {
        let line = input.replay_status.map(AsRef::as_ref).unwrap_or("AutoPlay");
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = input.timing_tick_status {
        lines[line_count] = line;
        line_count += 1;
        total_len += line.len();
    }
    if let Some(line) = input.autosync_status {
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
    if line_count == 1 && input.autoplay_enabled {
        let text = input
            .replay_status
            .map(Arc::clone)
            .unwrap_or_else(autoplay_overlay_text);
        return Some((text, 1));
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

fn resolve_sync_overlay_text(
    cache: &RefCell<SyncOverlayTextCache>,
    input: SyncOverlayTextInput<'_>,
) -> Option<(Arc<str>, usize)> {
    if input.is_idle() {
        return None;
    }
    cache.borrow_mut().resolve(input)
}

fn sync_overlay_text(state: &State) -> Option<(Arc<str>, usize)> {
    let input = SyncOverlayTextInput {
        autoplay_enabled: state.autoplay_enabled(),
        replay_status: state.replay_status_text.as_ref(),
        timing_tick_status: state.timing_tick_status_line(),
        autosync_status: autosync_mode_status_line(state.autosync_mode()),
        initial_global_offset: state.initial_global_offset_seconds(),
        global_offset: state.global_offset_seconds(),
        initial_song_offset: state.initial_song_offset_seconds(),
        song_offset: state.song_offset_seconds(),
    };
    resolve_sync_overlay_text(&state.sync_overlay_text_cache, input)
}

fn push_sync_overlay(actors: &mut Vec<Actor>, state: &State) {
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
            z(TOP_SCREEN_HUD_Z)
        ));
        line_count
    } else {
        0
    };

    if let Some((flash, alpha)) = state.toggle_flash_text() {
        let y = if status_line_count == 0 {
            screen_center_y() + 150.0
        } else {
            20.0f32.mul_add(status_line_count as f32, screen_center_y() + 150.0)
        };
        actors.push(act!(text:
            font("miso"):
            settext(flash):
            align(0.5, 0.5):
            xy(screen_center_x(), y):
            shadowlength(2.0):
            strokecolor(0.0, 0.0, 0.0, alpha):
            diffuse(1.0, 1.0, 1.0, alpha):
            z(TOP_SCREEN_HUD_Z)
        ));
    }

    if state.autosync_mode() == AutosyncMode::Off {
        return;
    }
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
        z(TOP_SCREEN_HUD_Z)
    ));
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
    scratch: &mut ComposeScratch,
    fonts: &font::FontMap,
    state: &State,
) {
    let policy = state.runtime_view.policy;
    prewarm_score_counter_layout(
        cache,
        fonts,
        machine_font_key(state.machine_font(), FontRole::Numbers),
    );
    if !state.rate_text.is_empty() {
        cache.prewarm_text(fonts, "miso", state.rate_text.as_ref(), None);
    }
    prewarm_life_text_slots(cache, scratch, fonts, state.num_players());
    prewarm_bpm_text_slot(cache, scratch, fonts);
    deadsync_song_lua::playback::prewarm_text_layout(cache, fonts, &state.gameplay);
    for player in 0..state.num_players() {
        let chart = &state.charts()[player];
        let meter_text = cached_meter_text(chart.meter);
        cache.prewarm_text(
            fonts,
            machine_font_key(state.machine_font(), FontRole::Header),
            meter_text.as_ref(),
            None,
        );
        let detail = color::difficulty_display_name_for_song(
            &chart.difficulty,
            &state.song().title,
            policy.zmod_rating_box_text,
        );
        cache.prewarm_text(fonts, "miso", detail, None);
    }
    cache.prewarm_text(
        fonts,
        machine_font_key(state.machine_font(), FontRole::Header),
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
    if !state.runtime_view.policy.smx_input
        || !state
            .profiles()
            .iter()
            .take(state.num_players())
            .any(|profile| profile.smx_fsr_display)
    {
        return;
    }
    let font_name = machine_font_key(state.machine_font(), FontRole::Normal);
    cache.prewarm_u16_domain(fonts, font_name, 0, 500, None, TextAlign::Left);
    scratch.prewarm_draw_sort(16);
}

/// Reserve the presentation buffers behind the bounded per-player notefield
/// actor envelope before the song starts. Song Lua can still exceed this base
/// envelope and is measured separately by its fixture class.
pub fn prewarm_compose_storage(scratch: &mut ComposeScratch, state: &State) {
    let players = state.num_players();
    let draw_floor = PLAYER_ACTOR_SCRATCH_CAPACITY.saturating_mul(players);
    let sorted_sprite_floor = usize::from(players > 1).saturating_mul(draw_floor);
    let textured_mesh_floor =
        NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY.saturating_mul(players.saturating_add(1));
    scratch.retain_working_set_headroom(
        draw_floor,
        sorted_sprite_floor,
        textured_mesh_floor,
        NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
    );
}

// --- TRANSITIONS ---
pub fn in_transition(
    state: Option<&State>,
    asset_manager: &AssetManager,
    is_restart: bool,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) -> (Vec<Actor>, f32) {
    if let Some(state) = state {
        state
            .notefield_combo_assets
            .prewarm(&visual_policy.assets.effects);
    }
    if is_restart {
        if let Some(gs) = state {
            let _ = intro_text_target_x(
                gs,
                asset_manager,
                gs.stage_intro_text.as_ref(),
                gs.runtime_view.play_style,
                gs.runtime_view.player_side,
                gs.runtime_view.policy.center_single_notefield,
            );
        }
        // SL/zmod parity: on a song restart, skip the splode + stage-text
        // splash and run only a brief fade-from-black so the first gameplay
        // frame doesn't pop in. The "RESTART N" label still appears in the
        // gameplay footer overlay.
        let actor = act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(1200):
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
    let text_target_x = state.map_or_else(screen_center_x, |gs| {
        intro_text_target_x(
            gs,
            asset_manager,
            text.as_ref(),
            gs.runtime_view.play_style,
            gs.runtime_view.player_side,
            gs.runtime_view.policy.center_single_notefield,
        )
    });
    let splode_tex = visual_policy.assets.effects.gameplayin_splode;
    let minisplode_tex = visual_policy.assets.effects.gameplayin_minisplode;
    let splode_zoom_scale = visual_styles::effect_zoom_scale(splode_tex);
    let minisplode_zoom_scale = visual_styles::effect_zoom_scale(minisplode_tex);
    let mut mirrored_splode = act!(sprite(splode_tex):
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        diffuse(intro_color[0], intro_color[1], intro_color[2], 0.8):
        rotationz(-10.0): zoom(0.0):
        z(1201):
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
            z(1200):
            sleep(1.4):
            accelerate(0.6): alpha(0.0):
            linear(0.0): visible(false)
        ),
        act!(sprite(splode_tex):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 0.9):
            rotationz(10.0): zoom(0.0):
            z(1201):
            sleep(0.4):
            linear(0.6): rotationz(0.0): zoom(1.1 * splode_zoom_scale): alpha(0.0)
        ),
        mirrored_splode,
        act!(sprite(minisplode_tex):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            diffuse(intro_color[0], intro_color[1], intro_color[2], 1.0):
            rotationz(10.0): zoom(0.0):
            z(1201):
            sleep(0.4):
            decelerate(0.8): rotationz(0.0): zoom(0.9 * minisplode_zoom_scale): alpha(0.0)
        ),
        act!(text:
            font(machine_font_key(visual_policy.machine_font, FontRole::Header)): settext(text):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 0.0):
            z(1202):
            accelerate(0.5): alpha(1.0):
            sleep(0.66):
            accelerate(0.33): zoom(0.4): xy(text_target_x, screen_height() - 30.0):
            sleep((TRANSITION_IN_DURATION - INTRO_TEXT_SETTLE_SECONDS).max(0.0))
        ),
    ];
    (actors, TRANSITION_IN_DURATION)
}

#[must_use]
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

#[inline(always)]
fn white_texture_key() -> Arc<str> {
    static WHITE_TEXTURE_KEY: OnceLock<Arc<str>> = OnceLock::new();
    Arc::clone(WHITE_TEXTURE_KEY.get_or_init(|| Arc::from("__white")))
}

fn push_background(
    actors: &mut Vec<Actor>,
    state: &State,
    bg_brightness: f32,
    base_color: deadlib_present::color::Color,
) {
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    let cy = screen_center_y();
    let bg_brightness = bg_brightness.clamp(0.0, 1.0);

    // Solid base fill behind everything. This is what shows when the song has no
    // background image, and what the song background is dimmed toward as
    // BGBrightness drops on the default path.
    let mut base = shared_banner::cover_sprite(white_texture_key(), cx, cy, sw, sh, 1.0, -101);
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

#[inline(always)]
fn active_background_start_sec(start_seconds: &[f32], next_change_ix: usize) -> Option<f32> {
    next_change_ix
        .checked_sub(1)
        .and_then(|ix| start_seconds.get(ix))
        .copied()
}

pub fn begin_background_transition(
    state: &mut State,
    previous_texture_key: Arc<str>,
    transition_name: &str,
    start_time: f32,
) {
    let transition = if &*previous_texture_key == "__black" {
        None
    } else {
        BackgroundTransition::from_name(transition_name)
    };
    state.previous_background_texture_key = transition.map(|_| previous_texture_key);
    state.background_transition = transition;
    state.background_transition_expired.set(false);
    state.background_transition_start_time = start_time;
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
        if let Some(key) = state.song_background_key.as_ref() {
            actors.push(background_media_sprite(
                Arc::clone(key),
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
    let now = state.current_music_time_display();
    let start_time = state.background_transition_start_time;
    let Some((transition, progress)) = background_transition_frame(
        state.background_transition,
        &state.background_transition_expired,
        start_time,
        now,
    ) else {
        return;
    };
    let mut actor = background_media_sprite(
        key.clone(),
        [1.0, 1.0, 1.0, bg_brightness],
        BlendMode::Alpha,
        x,
        y,
        w,
        h,
    );
    apply_bgchange_transition(&mut actor, transition, progress, w, h);
    actors.push(actor);
}

fn background_transition_frame(
    transition: Option<BackgroundTransition>,
    expired: &Cell<bool>,
    start_time: f32,
    now: f32,
) -> Option<(BackgroundTransition, f32)> {
    let current = transition?;
    if expired.get() && now >= start_time + current.duration() {
        return None;
    }
    let progress = ((now - start_time) / current.duration()).clamp(0.0, 1.0);
    if progress >= 1.0 {
        expired.set(true);
        return None;
    }
    expired.set(false);
    Some((current, progress))
}

fn apply_bgchange_transition(
    actor: &mut Actor,
    transition: BackgroundTransition,
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
    match transition {
        BackgroundTransition::CrossFade(_) => tint[3] *= 1.0 - progress,
        BackgroundTransition::SlideLeft => {
            offset[0] = screen_w.mul_add(-progress, offset[0]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideRight => {
            offset[0] = screen_w.mul_add(progress, offset[0]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideUp => {
            offset[1] = screen_h.mul_add(-progress, offset[1]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideDown => {
            offset[1] = screen_h.mul_add(progress, offset[1]);
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::FadeUp => {
            *cropbottom = 1.6f32.mul_add(progress, -0.3);
            *fadebottom = 0.3;
        }
        BackgroundTransition::FadeDown => {
            *croptop = 1.6f32.mul_add(progress, -0.3);
            *fadetop = 0.3;
        }
        BackgroundTransition::FadeRight => {
            *cropleft = 1.6f32.mul_add(progress, -0.3);
            *fadeleft = 0.3;
        }
        BackgroundTransition::FadeLeft => {
            *cropright = 1.6f32.mul_add(progress, -0.3);
            *faderight = 0.3;
        }
        BackgroundTransition::FadeCenterHorizontal => {
            *croptop = 0.8f32.mul_add(progress, -0.3);
            *cropbottom = 0.8f32.mul_add(progress, -0.3);
            *fadetop = 0.3;
            *fadebottom = 0.3;
        }
        BackgroundTransition::FadeCenterVertical => {
            *cropleft = 0.8f32.mul_add(progress, -0.3);
            *cropright = 0.8f32.mul_add(progress, -0.3);
            *fadeleft = 0.3;
            *faderight = 0.3;
        }
    }
}

const LAYER2_FLASH_SECONDS: f32 = 0.6;

fn song_layer2_color(target: &SongBackgroundChangeTarget) -> Option<[f32; 4]> {
    let SongBackgroundChangeTarget::Animation(name) = target else {
        return None;
    };
    if name.eq_ignore_ascii_case("white flash") {
        Some([1.0; 4])
    } else if name.eq_ignore_ascii_case("yellow flash") {
        Some([1.0, 1.0, 160.0 / 255.0, 1.0])
    } else {
        None
    }
}

fn build_song_layer2_events(gameplay: &GameplayCoreState) -> Vec<SongLayer2Event> {
    gameplay
        .song()
        .background_layer2_changes
        .iter()
        .map(|change| SongLayer2Event {
            start_second: gameplay.timing().get_time_for_beat(change.start_beat),
            color: song_layer2_color(&change.target),
        })
        .collect()
}

fn song_layer2_animation_from(
    events: &[SongLayer2Event],
    next_event_ix: &Cell<usize>,
    now: f32,
) -> Option<[f32; 4]> {
    if !now.is_finite() {
        return None;
    }
    let mut next_ix = next_event_ix.get().min(events.len());
    while next_ix < events.len() && events[next_ix].start_second <= now {
        next_ix += 1;
    }
    while next_ix > 0 && events[next_ix - 1].start_second > now {
        next_ix -= 1;
    }
    next_event_ix.set(next_ix);

    let event = next_ix.checked_sub(1).and_then(|index| events.get(index))?;
    let elapsed = now - event.start_second;
    if !(0.0..=LAYER2_FLASH_SECONDS).contains(&elapsed) {
        return None;
    }
    let mut color = event.color?;
    let progress = (elapsed / LAYER2_FLASH_SECONDS).clamp(0.0, 1.0);
    color[3] *= 1.0 - progress * progress;
    Some(color)
}

fn push_layer2_bganimations(actors: &mut Vec<Actor>, state: &State) {
    let now = state.current_music_time_display();
    let Some(color) = song_layer2_animation_from(
        &state.song_layer2_events,
        &state.next_song_layer2_event_ix,
        now,
    ) else {
        return;
    };
    actors.push(act!(quad:
        align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
        setsize(screen_width() * 2.0, screen_height() * 2.0):
        diffuse(color[0], color[1], color[2], color[3]):
        z(-98)
    ));
}

fn custom_gameplay_backdrop_enabled(color: deadlib_present::color::Color) -> bool {
    color != deadlib_present::color::Color::BLACK
}

fn push_custom_gameplay_backdrop(actors: &mut Vec<Actor>, color: deadlib_present::color::Color) {
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

fn gameplay_header_rgba(color: deadlib_present::color::Color) -> [f32; 4] {
    if custom_gameplay_backdrop_enabled(color) {
        color.to_rgba()
    } else {
        [0.0, 0.0, 0.0, 0.85]
    }
}

/// Prepare Simply Love fragments; chart order and captures are chosen by shared playback.
pub fn frame_layers<'a>(
    state: &'a State,
    scratch: &'a mut GameplayFrameScratch,
    asset_manager: &'a AssetManager,
    view: ActorViewOverride,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) -> impl FnMut(
    deadsync_song_lua::playback::ScreenLayer,
    &mut Vec<Actor>,
    deadsync_song_lua::playback::FieldLayout,
) + 'a {
    use deadsync_song_lua::playback::{FieldLayout, ScreenLayer};
    let GameplayFrameScratch {
        lobby_hud_cache,
        lobby_hud_status_scratch,
        bpm_text,
        presentation_skeleton,
    } = scratch;
    presentation_skeleton.prepare();
    let play_style = state.hud_snapshot.play_style;
    let player_side = state.hud_snapshot.player_side;
    let is_p2_single = profile_data::is_single_p2_side(play_style, player_side);
    let runtime_player_is_p2 = profile_data::runtime_player_is_p2(play_style, player_side);
    let policy = state.runtime_view.policy;
    let notefield_view = view.notefield;
    let center_1player_notefield =
        policy.center_single_notefield || notefield_view.force_center_1player;
    let centered_single_notefield =
        play_style.is_single() && state.num_players() == 1 && center_1player_notefield;
    let player_color = color::decorative_rgba(state.player_color_index());
    move |layer,
          actors,
          FieldLayout {
              per_player_fields,
              playfield_center_x,
          }| match layer {
        ScreenLayer::Background => {
            push_background(
                actors,
                state,
                policy.background_brightness,
                policy.background_color,
            );
        }
        ScreenLayer::ExitPrompt => {
            const HOLD_FADE_IN_S: f32 = 1.0 / 8.0;
            const ABORT_FADE_OUT_S: f32 = 0.5;

            let y = screen_height() - 116.0;
            let exit_prompt = state.exit_prompt_state();
            let msg: Option<(Arc<str>, f32)> = if gameplay_lobby_wait_active(state) {
                None
            } else if let (Some(key), Some(start)) =
                (exit_prompt.hold_to_exit_key, exit_prompt.hold_to_exit_start)
            {
                let text = match key {
                    HoldToExitKey::Start => tr("Gameplay", "ContinueHoldingStartGiveUp"),
                    HoldToExitKey::Back => tr("Gameplay", "ContinueHoldingBackGiveUp"),
                };
                let alpha = (start.elapsed().as_secs_f32() / HOLD_FADE_IN_S).clamp(0.0, 1.0);
                Some((text, alpha))
            } else if let Some(exit) = &exit_prompt.exit_transition {
                let t = exit.started_at.elapsed().as_secs_f32();
                match exit.kind {
                    ExitTransitionKind::Out => {
                        let alpha = (1.0 - t / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
                        Some((tr("Gameplay", "ContinueHoldingStartGiveUp"), alpha))
                    }
                    ExitTransitionKind::Cancel => {
                        Some((tr("Gameplay", "ContinueHoldingBackGiveUp"), 1.0))
                    }
                }
            } else if let Some(at) = exit_prompt.hold_to_exit_aborted_at {
                let alpha = (1.0 - at.elapsed().as_secs_f32() / ABORT_FADE_OUT_S).clamp(0.0, 1.0);
                Some((tr("Gameplay", "DontGoBack"), alpha))
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
        }
        ScreenLayer::Lobby => {
            let joined_lobby = state.runtime_view.lobby.snapshot.joined_lobby.is_some();
            if state
                .lobby_hud_visibility
                .should_render(joined_lobby, gameplay_lobby_wait_active(state))
            {
                let has_status = write_gameplay_lobby_hud_status(state, lobby_hud_status_scratch);
                let joined = state
                    .runtime_view
                    .lobby
                    .snapshot
                    .joined_lobby
                    .as_ref()
                    .expect("checked joined lobby");
                lobby_hud::push_cached_panel(
                    actors,
                    lobby_hud_cache,
                    lobby_hud::CachedRenderParams {
                        screen_name: "ScreenGameplay",
                        joined,
                        z: 995,
                        show_song_info: false,
                        status_text: has_status.then_some(lobby_hud_status_scratch.as_str()),
                        joined_sides: state.runtime_view.joined,
                        player_side: state.runtime_view.player_side,
                    },
                );
            }
        }
        ScreenLayer::ExitFade => {
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
        }
        ScreenLayer::Danger => {
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
                    profile_data::PlayStyle::Double | profile_data::PlayStyle::PumpDouble => {
                        (0.0, sw, 0.0, 0.0)
                    }
                    profile_data::PlayStyle::Versus | profile_data::PlayStyle::PumpVersus => {
                        if player_idx == 0 {
                            (0.0, cx, 0.0, 0.1)
                        } else {
                            (cx, sw - cx, 0.1, 0.0)
                        }
                    }
                    profile_data::PlayStyle::Single | profile_data::PlayStyle::PumpSingle => {
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
        }
        ScreenLayer::Filter => {
            let has_background_filter = per_player_fields.iter().any(|&(player_idx, _)| {
                player_idx != usize::MAX
                    && player_idx < state.num_players()
                    && state.profiles()[player_idx].background_filter.alpha() > 0.0
            });
            if has_background_filter {
                presentation_skeleton.push(STATIC_FILTER, actors, |actors| {
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
                            zoomto(state.notefield_width(player_idx), screen_height()):
                            diffuse(0.0, 0.0, 0.0, filter_alpha):
                            z(-99)
                        ));
                    }
                });
            }
        }
        ScreenLayer::Header => {
            let header_rgba = gameplay_header_rgba(policy.background_color);
            presentation_skeleton.push(STATIC_HEADER, actors, |actors| {
                actors.push(act!(quad:
                    align(0.5, 0.0): xy(screen_center_x(), 0.0):
                    setsize(screen_width(), 80.0):
                    diffuse(header_rgba[0], header_rgba[1], header_rgba[2], header_rgba[3]):
                    z(83)
                ));
            });
        }
        ScreenLayer::Hud => {
            let clamped_width = screen_width().clamp(640.0, 854.0);
            let score_x_p1 = screen_center_x() - clamped_width / 4.3;
            let score_x_p2 = screen_center_x() + clamped_width / 2.75;
            let diff_x_p1 = screen_center_x() - widescale(292.5, 342.5);
            let diff_x_p2 = screen_center_x() + widescale(292.5, 342.5);

            let mut players = [(0usize, profile_data::PlayerSide::P1, 0.0, 0.0, 0.0, 0.0); 2];
            let player_count = match play_style {
                profile_data::PlayStyle::Versus | profile_data::PlayStyle::PumpVersus => {
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
                _ if runtime_player_is_p2 => {
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
                let x = upper_nps_graph_x(
                    player_side,
                    field_x,
                    graph_w,
                    state.profiles()[player_idx].note_field_offset_x,
                );
                let y_bottom = 71.0;
                let y_top = y_bottom - graph_h;
                let y_mesh_top = y_bottom - graph_mesh_h;
                let graph_bg_alpha = if state.profiles()[player_idx].transparent_density_graph_bg {
                    0.5
                } else {
                    1.0
                };

                let static_slot = [STATIC_NPS_P1, STATIC_NPS_P2][player_idx.min(1)];
                presentation_skeleton.push(static_slot, actors, |actors| {
                    actors.push(act!(quad:
                        align(0.0, 0.0): xy(x, y_top):
                        zoomto(graph_w, graph_h):
                        diffuse(30.0 / 255.0, 40.0 / 255.0, 47.0 / 255.0, graph_bg_alpha):
                        z(84)
                    ));
                });

                if let Some(mesh) = &state.density_graph.top_mesh[player_idx]
                    && !mesh.is_empty()
                {
                    actors.push(Actor::Mesh {
                        align: [0.0, 0.0],
                        offset: [x, y_mesh_top],
                        size: [SizeSpec::Px(graph_w), SizeSpec::Px(graph_mesh_h)],
                        tint: [1.0; 4],
                        vertices: mesh.clone(),
                        visible: true,
                        blend: BlendMode::Alpha,
                        z: 85,
                    });
                }

                let duration = (graph.last_second - graph.first_second).max(0.001_f32);
                let progress_w = (((state.current_music_time_display() - graph.first_second)
                    / duration)
                    * graph_w)
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
            if policy.smx_input {
                let is_doubles = play_style.is_double();
                let is_centered_single = centered_single_notefield;
                let mut field_geom: [Option<(profile_data::PlayerSide, f32, f32)>; 2] =
                    [None, None];
                for &(player_idx, player_side, field_x, ..) in &players[..player_count] {
                    if player_idx < 2 {
                        let half_w = state.notefield_width(player_idx) * 0.5;
                        field_geom[player_idx] =
                            Some((player_side, field_x - half_w, field_x + half_w));
                    }
                }
                // Combine shell-transition alpha (FadingIn/FadingOut) with the
                // in-gameplay exit animation alpha. The exit animation runs under
                // Idle shell state (NavigateNoFade paths: restart, back-out) so
                // view.smx_overlay_alpha alone doesn't cover it.
                let exit_alpha = state
                    .exit_prompt_state()
                    .exit_transition
                    .as_ref()
                    .map_or(1.0, |exit| 1.0 - exit_transition_alpha(exit));
                let smx_overlay_alpha = view.smx_overlay_alpha.min(exit_alpha);
                if state.profiles()[0].smx_fsr_display || state.profiles()[1].smx_fsr_display {
                    let before = actors.len();
                    smx_profile::time_draw(state.runtime_view.policy.smx_profile_enabled, || {
                        push_smx_sensor_display(
                            actors,
                            state,
                            &field_geom,
                            is_doubles,
                            is_centered_single,
                        );
                    });
                    if smx_overlay_alpha < 1.0 {
                        for a in &mut actors[before..] {
                            a.mul_alpha(smx_overlay_alpha);
                        }
                    }
                }
                if state.profiles()[0].smx_pad_input_display
                    || state.profiles()[1].smx_pad_input_display
                {
                    let before = actors.len();
                    push_smx_pad_input_display(
                        actors,
                        state,
                        &field_geom,
                        is_doubles,
                        is_centered_single,
                    );
                    if smx_overlay_alpha < 1.0 {
                        for a in &mut actors[before..] {
                            a.mul_alpha(smx_overlay_alpha);
                        }
                    }
                }
            }

            let score_alphas = gameplay_score_leader_alphas(state);
            for &(player_idx, player_side, field_x, diff_x, score_x_normal, score_x_other) in
                &players[..player_count]
            {
                let profile = &state.profiles()[player_idx];
                // Difficulty Box
                let y = DIFFICULTY_METER_Y;
                let static_slot = [STATIC_DIFFICULTY_P1, STATIC_DIFFICULTY_P2][player_idx.min(1)];
                presentation_skeleton.push(static_slot, actors, |actors| {
                let diff_x = difficulty_meter_x(
                    state,
                    profile,
                    player_idx,
                    player_side,
                    field_x,
                    state.notefield_width(player_idx),
                    diff_x,
                );
                let chart = &state.charts()[player_idx];
                let difficulty_color = color::difficulty_rgba_with_scheme(
                    &chart.difficulty,
                    state.active_color_index(),
                    policy.difficulty_color_scheme,
                );
                let meter_text = cached_meter_text(chart.meter);
                let meter_detail_text = color::difficulty_display_name_for_song(
                    &chart.difficulty,
                    &state.song().title,
                    true,
                );
                actors.push(act!(quad:
                    align(0.5, 0.5): xy(diff_x, y): zoomto(30.0, 30.0):
                    diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0):
                    z(90)
                ));
                let meter_y = if policy.zmod_rating_box_text {
                    -4.0
                } else {
                    0.0
                };
                actors.push(act!(text:
                    font(machine_font_key(state.machine_font(), FontRole::Header)): settext(meter_text): align(0.5, 0.5): xy(diff_x, y + meter_y):
                    zoom(0.4): diffuse(0.0, 0.0, 0.0, 1.0): z(90)
                ));
                if policy.zmod_rating_box_text {
                    actors.push(act!(text:
                        font("miso"):
                        settext(meter_detail_text):
                        align(0.5, 0.5): xy(diff_x, y + 9.5):
                        zoom(0.5):
                        diffuse(0.0, 0.0, 0.0, 1.0):
                        z(90)
                    ));
                }
            });

                // Score Display
                let note_field_is_centered = (field_x - screen_center_x()).abs() < 1.0;
                let nps_graph_at_top = state.profiles()[player_idx].nps_graph_at_top;
                let single_score_swapped = state.num_players() == 1
                    && !play_style.is_double()
                    && nps_graph_at_top
                    && !note_field_is_centered;
                let score_in_single_step_stats = profile.score_position
                    == profile_data::ScorePosition::StepStatistics
                    && !profile.step_statistics.is_empty()
                    && play_style.is_single()
                    && state.num_cols() <= 4;
                let score_in_versus_step_stats = profile.score_position
                    == profile_data::ScorePosition::StepStatistics
                    && !profile.step_statistics.is_empty()
                    && play_style.is_versus()
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
                    let (score_value, mut score_color) = if show_ex_score {
                        let blue_window_ms = player_blue_window_ms(state, player_idx);
                        let ex_percent = state.display_gameplay_ex_score_percent(
                            player_idx,
                            score_display_mode_from_profile(profile.score_display_mode),
                            blue_window_ms,
                        );
                        (
                            ex_percent.max(0.0),
                            state
                                .judgment_palette(player_idx)
                                .color(deadsync_theme::color::JudgmentColorRole::FantasticBlue),
                        )
                    } else {
                        let score_percent = state.display_gameplay_itg_score_percent(
                            player_idx,
                            score_display_mode_from_profile(profile.score_display_mode),
                        );
                        (score_percent, [1.0, 1.0, 1.0, 1.0])
                    };
                    score_color[3] *= score_alphas[player_idx];

                    let is_p2_side = player_side == profile_data::PlayerSide::P2;
                    // Arrow Cloud parity: EX remains the "normal" score position/anchor.
                    // H.EX is placed at a different x on P2 so it appears to the left of EX.
                    push_score_counter(
                        actors,
                        asset_manager.fonts(),
                        ScoreCounterParams {
                            value: score_value,
                            font: machine_font_key(state.machine_font(), FontRole::Numbers),
                            position: [score_x, score_y],
                            align: [1.0, 1.0],
                            text_align: TextAlign::Right,
                            zoom: score_zoom,
                            color: score_color,
                            z: 90,
                        },
                    );

                    if show_hard_ex_score {
                        let blue_window_ms = player_blue_window_ms(state, player_idx);
                        let hard_ex_percent = state.display_gameplay_hard_ex_score_percent(
                            player_idx,
                            score_display_mode_from_profile(profile.score_display_mode),
                            blue_window_ms,
                        );
                        let mut hex = color::HARD_EX_SCORE_RGBA;
                        hex[3] *= score_alphas[player_idx];
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

                        push_score_counter(
                            actors,
                            asset_manager.fonts(),
                            ScoreCounterParams {
                                value: hard_ex_percent.max(0.0),
                                font: machine_font_key(state.machine_font(), FontRole::Numbers),
                                position: [hard_ex_x, hard_ex_y],
                                align: if is_p2_side { [1.0, 0.0] } else { [0.0, 0.0] },
                                text_align: if is_p2_side {
                                    TextAlign::Right
                                } else {
                                    TextAlign::Left
                                },
                                zoom: hard_ex_zoom,
                                color: hex,
                                z: 90,
                            },
                        );
                    }
                }
            }
            // Current BPM Display (1:1 with Simply Love)
            {
                let display_bpm = display_bpm(state.current_bpm_display(), state.music_rate());
                let bpm_text = bpm_text.resolve(display_bpm, policy.show_bpm_decimal);
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
                let bpm_x = gameplay_bpm_x(
                    policy.bpm_position,
                    state.num_players(),
                    play_style,
                    player_side,
                    playfield_center_x,
                    state.notefield_width(0),
                    state.profiles()[0].nps_graph_at_top,
                );
                actors.push(act!(text:
                    font("miso"): settext(bpm_text):
                    align(0.5, 0.5): xy(bpm_x, bpm_center_y):
                    zoom(bpm_final_zoom): horizalign(center): z(90)
                ));
                if !state.rate_text.is_empty() {
                    actors.push(act!(text:
                        font("miso"): settext(Arc::clone(&state.rate_text)):
                        align(0.5, 0.5): xy(bpm_x, rate_center_y):
                        zoom(rate_final_zoom): horizalign(center): z(90)
                    ));
                }
            }
            // Song Title Box (SongMeter)
            {
                let w = widescale(310.0, 417.0);
                let h = 22.0;
                let box_cx = screen_center_x();
                let box_cy = 20.0;
                let box_left = box_cx - w * 0.5;
                presentation_skeleton.push(STATIC_SONG_METER, actors, |actors| {
                actors.push(act!(quad:
                    align(0.5, 0.5): xy(box_cx, box_cy): zoomto(w, h):
                    diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                ));
                actors.push(act!(quad:
                    align(0.5, 0.5): xy(box_cx, box_cy): zoomto(w - 4.0, h - 4.0):
                    diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                ));
                actors.push(act!(text:
                    font("miso"): settext(state.song_full_title.clone()): align(0.5, 0.5): xy(box_cx, box_cy):
                    zoom(0.8): shadowlength(0.6): maxwidth(screen_width() / 2.5 - 10.0):
                    horizalign(center): z(93)
                ));
            });
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
            }
            // --- Life Meter ---
            {
                let player_life_color = |player_idx: usize| -> [f32; 4] {
                    let color_index = match play_style {
                        profile_data::PlayStyle::Versus | profile_data::PlayStyle::PumpVersus => {
                            if player_idx == 0 {
                                state.active_color_index()
                            } else {
                                state.active_color_index() - 2
                            }
                        }
                        _ => {
                            if runtime_player_is_p2 {
                                state.active_color_index() - 2
                            } else {
                                state.active_color_index()
                            }
                        }
                    };
                    if visual_policy.srpg10_tint {
                        color::srpg10_rgba(color_index)
                    } else {
                        color::decorative_rgba(color_index)
                    }
                };
                let show_standard_life_percent =
                    screen_width() / screen_height().max(1.0) >= (16.0 / 9.0);

                let mut life_players = [(0usize, profile_data::PlayerSide::P1); 2];
                let life_player_count = match play_style {
                    profile_data::PlayStyle::Versus | profile_data::PlayStyle::PumpVersus => {
                        life_players[0] = (0, profile_data::PlayerSide::P1);
                        life_players[1] = (1, profile_data::PlayerSide::P2);
                        2
                    }
                    _ if runtime_player_is_p2 => {
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
                    let actual_life = visible_life(player.life, dead);
                    let profile = &state.profiles()[player_idx];
                    let life_visual = state.life_meter_visuals[player_idx];
                    let life_for_render = life_visual.rendered_life(profile.lifemeter_type);
                    let is_hot = life_visual.is_hot;
                    let percent_visual = life_visual.percent_visual();
                    let life_percent_text = visible_life_percent_text(
                        &state.life_percent_text,
                        player_idx,
                        actual_life * 100.0,
                        profile.lifemeter_type,
                        profile.show_life_percent,
                        show_standard_life_percent,
                    );
                    let static_life_slot = [STATIC_LIFE_P1, STATIC_LIFE_P2][player_idx.min(1)];

                    match profile.lifemeter_type {
                        profile_data::LifeMeterType::Standard => {
                            let life_color = life_fill_color(
                                profile,
                                actual_life,
                                dead,
                                life_visual.hot_age,
                                || player_life_color(player_idx),
                            );
                            let w = 136.0;
                            let h = 18.0;
                            let meter_cy = 20.0;
                            let meter_cx = screen_center_x()
                                + match play_style {
                                    profile_data::PlayStyle::Versus
                                    | profile_data::PlayStyle::PumpVersus => match side {
                                        profile_data::PlayerSide::P1 => -widescale(238.0, 288.0),
                                        profile_data::PlayerSide::P2 => widescale(238.0, 288.0),
                                    },
                                    _ => match side {
                                        profile_data::PlayerSide::P1 => -widescale(238.0, 288.0),
                                        profile_data::PlayerSide::P2 => widescale(238.0, 288.0),
                                    },
                                };

                            // Frames/border
                            presentation_skeleton.push(static_life_slot, actors, |actors| {
                                actors.push(act!(quad:
                                align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w + 4.0, h + 4.0):
                                diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                            ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(meter_cx, meter_cy): zoomto(w, h):
                                    diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                                ));
                            });

                            let filled_width = w * life_for_render;
                            // Never draw swoosh if dead OR nothing to fill.
                            if filled_width > 0.0 && !dead {
                                // Logic Parity:
                                // velocity = -(songposition:GetCurBPS() * 0.5)
                                // if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
                                let velocity_x = life_swoosh_velocity_x(state, player_idx);

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

                            if let Some(life_percent_text) = life_percent_text {
                                let life_text_color = player_life_color(player_idx);
                                let (outer_x, inner_x, text_x, align_x) =
                                    if side == profile_data::PlayerSide::P1 {
                                        (meter_cx - 76.0, meter_cx - 77.0, meter_cx - 77.0, 1.0)
                                    } else {
                                        (meter_cx + 76.0, meter_cx + 77.0, meter_cx + 78.0, 0.0)
                                    };
                                actors.push(act!(quad:
                            align(align_x, 0.5): xy(outer_x, meter_cy):
                            zoomto(percent_visual.outer_w, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], percent_visual.alpha):
                            z(94)
                        ));
                                actors.push(act!(quad:
                                    align(align_x, 0.5): xy(inner_x, meter_cy):
                                    zoomto(percent_visual.inner_w, 16.0):
                                    diffuse(0.0, 0.0, 0.0, percent_visual.alpha):
                                    z(95)
                                ));
                                actors.push(act!(text:
                            font("miso"): settext(life_percent_text):
                            align(align_x, 0.5): xy(text_x, meter_cy):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], percent_visual.alpha):
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

                            if play_style.is_double() {
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

                            let surround_color =
                                surround_life_color(profile, actual_life, life_visual.hot_age);

                            match side {
                                profile_data::PlayerSide::P1 => {
                                    actors.push(act!(quad:
                                align(0.0, 0.0): xy(0.0, y):
                                zoomto(w, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                faderight(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                                }
                                profile_data::PlayerSide::P2 => {
                                    actors.push(act!(quad:
                                align(1.0, 0.0): xy(sw, y):
                                zoomto(w, h):
                                diffuse(surround_color[0], surround_color[1], surround_color[2], surround_color[3]):
                                fadeleft(0.8):
                                croptop(croptop):
                                z(-98)
                            ));
                                }
                            }
                        }
                        profile_data::LifeMeterType::Vertical => {
                            let life_color = life_fill_color(
                                profile,
                                actual_life,
                                dead,
                                life_visual.hot_age,
                                || player_life_color(player_idx),
                            );
                            let bar_w = 16.0;
                            let bar_h = 250.0;

                            let x = {
                                // SL: default to _screen.cx +/- SL_WideScale(302, 400).
                                let mut x = screen_center_x()
                                    + match side {
                                        profile_data::PlayerSide::P1 => -widescale(302.0, 400.0),
                                        profile_data::PlayerSide::P2 => widescale(302.0, 400.0),
                                    };

                                // SL: Double and Center1Player sit next to the notefield.
                                if play_style.is_double() || centered_single_notefield {
                                    let half_nf = state.notefield_width(player_idx) * 0.5;
                                    x = screen_center_x()
                                        + match side {
                                            profile_data::PlayerSide::P1 => -(half_nf + 10.0),
                                            profile_data::PlayerSide::P2 => half_nf + 10.0,
                                        };
                                } else if screen_width() / screen_height().max(1.0) > (21.0 / 9.0)
                                    && state.num_players() > 1
                                    && !profile.step_statistics.is_empty()
                                {
                                    x = screen_center_x()
                                        + match side {
                                            profile_data::PlayerSide::P1 => -60.0,
                                            profile_data::PlayerSide::P2 => 60.0,
                                        };
                                }

                                x
                            };

                            let cy = bar_h + 10.0;
                            // Frames/border
                            presentation_skeleton.push(static_life_slot, actors, |actors| {
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(x, cy): zoomto(bar_w + 2.0, bar_h + 2.0):
                                    diffuse(1.0, 1.0, 1.0, 1.0): z(90)
                                ));
                                actors.push(act!(quad:
                                    align(0.5, 0.5): xy(x, cy): zoomto(bar_w, bar_h):
                                    diffuse(0.0, 0.0, 0.0, 1.0): z(91)
                                ));
                            });

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
                                let velocity_x = life_swoosh_velocity_x(state, player_idx);
                                let swoosh_alpha = life_visual.vertical_swoosh_alpha;

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

                            if let Some(life_percent_text) = life_percent_text {
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
                            zoomto(percent_visual.outer_w, 18.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], percent_visual.alpha):
                            z(94)
                        ));
                                actors.push(act!(quad:
                                    align(align_x, 0.5): xy(inner_x, text_y):
                                    zoomto(percent_visual.inner_w, 16.0):
                                    diffuse(0.0, 0.0, 0.0, percent_visual.alpha):
                                    z(95)
                                ));
                                actors.push(act!(text:
                            font("miso"): settext(life_percent_text):
                            align(align_x, 0.5): xy(text_x, text_y):
                            zoom(1.0):
                            diffuse(life_text_color[0], life_text_color[1], life_text_color[2], percent_visual.alpha):
                            z(96)
                        ));
                            }
                        }
                    }
                }
            }
            match state.step_stats_mode {
                GameplayStepStatsMode::Hidden => {}
                GameplayStepStatsMode::Side => gameplay_stats::push_step_stats(
                    actors,
                    state,
                    asset_manager,
                    playfield_center_x,
                    player_side,
                ),
                GameplayStepStatsMode::Versus => {
                    gameplay_stats::push_versus_step_stats(actors, state, asset_manager);
                }
                GameplayStepStatsMode::Double => {
                    gameplay_stats::push_double_step_stats(
                        actors,
                        state,
                        asset_manager,
                        playfield_center_x,
                    );
                }
            }
            gameplay_stats::push_heart_rates(actors, state, playfield_center_x);
        }
        ScreenLayer::System => {
            push_system_stage_label(actors, state, asset_manager);
            push_system_profile_footer(actors, state, visual_policy, presentation_skeleton);
            push_sync_overlay(actors, state);
        }
    }
}

pub fn draw_field(
    state: &State,
    asset_manager: &AssetManager,
    view: ActorViewOverride,
    arrow_effect_time_s: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
    request: deadsync_song_lua::playback::FieldFrame,
    notefield_camera_cache: &mut [deadsync_notefield::NotefieldCameraCache; MAX_PLAYERS],
    field_scratch: &mut Vec<Actor>,
    flat_draw_scratch: &mut Vec<deadlib_present::actors::FlatDraw>,
    hud_scratch: &mut Vec<Actor>,
    hud_flat_draw_scratch: &mut Vec<deadlib_present::actors::FlatDraw>,
) -> deadsync_notefield::BuiltNotefield {
    let play_style = state.hud_snapshot.play_style;
    let notefield_view = view.notefield;
    let center_1player_notefield =
        state.runtime_view.policy.center_single_notefield || notefield_view.force_center_1player;
    let player_idx = request.player;
    let placement = request.placement;
    let judgment_visible = request.judgment_visible;
    let combo_visible = request.combo_visible;
    let profile = &state.profiles()[player_idx];
    notefield::compose_frame(
        state,
        state.notefield_judgment_assets(player_idx),
        &state.notefield_combo_assets,
        state.notefield_plan(player_idx),
        player_idx,
        arrow_effect_time_s,
        &state.noteskin_assets,
        &visual_policy.assets.effects,
        state.actor_resources(),
        asset_manager.texture_context(),
        &state.notefield_model_cache,
        &state.notefield_hold_mesh_scratch,
        &state.notefield_capture_scratch,
        notefield_camera_cache,
        &state.notefield_broken_run_lookup[player_idx],
        &state.notefield_stream_progress_lookup[player_idx],
        profile,
        placement,
        play_style,
        center_1player_notefield,
        judgment_visible,
        combo_visible,
        view.show_song_visuals,
        view.apply_attacks,
        request.capture,
        state.itl_cmod_warning[player_idx],
        state.display_mods_text(player_idx),
        notefield_view,
        field_scratch,
        flat_draw_scratch,
        hud_scratch,
        hud_flat_draw_scratch,
    )
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

    fn time<T>(enabled: bool, bucket: &Bucket, f: impl FnOnce() -> T) -> T {
        if !enabled {
            return f();
        }
        let start = Instant::now();
        let out = f();
        bucket.record(start.elapsed().as_nanos() as u64);
        out
    }

    pub(super) fn record_read(enabled: bool, elapsed_ns: u64) {
        if enabled {
            READ.record(elapsed_ns);
        }
    }

    pub fn time_draw<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
        time(enabled, &DRAW, f)
    }

    /// Log the rolling window once a second. Cheap no-op when profiling is off.
    pub fn maybe_report(enabled: bool) {
        if !enabled {
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

#[inline(always)]
const fn smx_sensor_value_content(value: Option<u16>) -> (TextContent, [f32; 4]) {
    match value {
        Some(value) => (TextContent::prewarmed_u16(value, 0), SMX_SENSOR_VALUE_COLOR),
        None => (TextContent::Static("--"), SMX_SENSOR_VALUE_IDLE_COLOR),
    }
}

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
    3.0f32.mul_add(SMX_SENSOR_BAR_GAP, 4.0 * SMX_SENSOR_BAR_W)
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
    let mini_w = 2.0f32.mul_add(SMX_PAD_INPUT_GAP, 3.0 * SMX_PAD_INPUT_CELL) * scale;
    let total_h = fsr_h + SMX_CENTERED_STACK_GAP + mini_w;
    let top_y = screen_center_y() - total_h * 0.5;
    let gutter_center = match side {
        profile_data::PlayerSide::P1 => field_left * 0.5,
        profile_data::PlayerSide::P2 => f32::midpoint(field_right, screen_width()),
    };
    (
        scale,
        fsr_w.mul_add(-0.5, gutter_center),
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
        let total_w = pad_group_w.mul_add(2.0, group_gap);
        let start_x = center_x - total_w * 0.5;
        let top_y = screen_height().mul_add(SMX_DOUBLES_STACK_TOP_FRAC, SMX_DOUBLES_FSR_Y_OFFSET);
        for sdk_pad in 0..2usize {
            let gx = (sdk_pad as f32).mul_add(pad_group_w + group_gap, start_x);
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
    let Some(view) = state.smx_sensor_views[idx].as_ref() else {
        return;
    };

    let bar_w = SMX_SENSOR_BAR_W * scale;
    let bar_h = SMX_SENSOR_BAR_H * scale;
    let bar_gap = SMX_SENSOR_BAR_GAP * scale;
    let bar_y = (SMX_SENSOR_VALUE_H + SMX_SENSOR_VALUE_GAP).mul_add(scale, group_top);
    let pad_group_w = 3.0f32.mul_add(bar_gap, 4.0 * bar_w);

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
        let x = (slot as f32).mul_add(bar_w + bar_gap, group_x);

        // Panel high threshold (max across sensors for FSR), computed once and
        // used for both the active check and the threshold line.
        let panel_view = view.panels[panel];
        let threshold = panel_view.threshold;
        let threshold_norm = (f32::from(threshold) / SMX_SENSOR_VALUE_SCALE).clamp(0.0, 1.0);

        let raw_value = panel_view.value;
        let value_norm = raw_value
            .map_or(0.0, |value| f32::from(value) / SMX_SENSOR_VALUE_SCALE)
            .clamp(0.0, 1.0);
        let active = raw_value.is_some_and(|value| value >= threshold && threshold > 0);

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
        let threshold_y = threshold_h.mul_add(-0.5, (1.0 - threshold_norm).mul_add(bar_h, bar_y));
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
        let (value_text, value_color) = smx_sensor_value_content(raw_value);
        actors.push(act!(text:
            font(machine_font_key(state.machine_font(), FontRole::Normal)): settext(value_text):
            align(0.5, 0.0): xy(bar_w.mul_add(0.5, x), group_top):
            zoom(SMX_SENSOR_VALUE_ZOOM * scale):
            diffuse(value_color[0], value_color[1], value_color[2], value_color[3]):
            z(SMX_SENSOR_Z + 2.0)
        ));

        // Panel letter (L/D/U/R) drawn on the bar near its bottom; the drop
        // shadow keeps it legible over both the dark track and bright fill.
        actors.push(act!(text:
            font(machine_font_key(state.machine_font(), FontRole::Normal)): settext(label):
            align(0.5, 1.0):
            xy(bar_w.mul_add(0.5, x), SMX_SENSOR_LETTER_INSET.mul_add(-scale, bar_y + bar_h)):
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
    let mini_w = 2.0f32.mul_add(SMX_PAD_INPUT_GAP, 3.0 * SMX_PAD_INPUT_CELL);
    // Vertically center the mini-pad on the FSR sensor display group, so the two
    // read as aligned when shown together (regardless of whether the FSR display
    // is actually shown). Lifted above the footer so it clears the avatar.
    let fsr_bottom = screen_height() - SMX_SENSOR_FOOTER_CLEAR - SMX_SENSOR_MARGIN;
    let fsr_group_h = SMX_SENSOR_BAR_H + SMX_SENSOR_VALUE_GAP + SMX_SENSOR_VALUE_H;
    let y0 = fsr_group_h.mul_add(-0.5, fsr_bottom) - mini_w * 0.5;

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
        let mini_top = screen_height().mul_add(SMX_DOUBLES_STACK_TOP_FRAC, fsr_group_h)
            + SMX_DOUBLES_STACK_GAP;
        // When the FSR pair is also shown, center each mini under its FSR group
        // above it; otherwise use the natural (tighter) mini-pair spacing so a
        // mini-only display doesn't look oddly spread out.
        let fsr_active = state.profiles()[0].smx_fsr_display;
        let fsr_group_w = smx_fsr_group_w();
        let fsr_start_x = center_x - f32::midpoint(fsr_group_w * 2.0, group_gap);
        for half in 0..2usize {
            let x0 = if fsr_active {
                let fsr_center = fsr_group_w.mul_add(
                    0.5,
                    (half as f32).mul_add(fsr_group_w + group_gap, fsr_start_x),
                );
                fsr_center - mini_w * 0.5
            } else {
                (half as f32).mul_add(mini_w + group_gap, start_x)
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
    let mini_w = 2.0f32.mul_add(gap, 3.0 * cell);
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
        let cx = gx.mul_add(cell + gap, x0);
        let cy = gy.mul_add(cell + gap, y0);
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
    #[test]
    fn theme_hud_and_transition_stay_above_song_lua() {
        assert!(TOP_SCREEN_HUD_Z > deadsync_song_lua::playback::LUA_FOREGROUND_Z_MAX);
        let (transitions, _) = out_transition();
        let Actor::Sprite { z, .. } = &transitions[0] else {
            panic!("out transition is a black sprite");
        };
        assert!(*z > deadsync_song_lua::playback::LUA_FOREGROUND_Z_MAX);
    }

    use super::*;

    use deadlib_present::actors::SpriteSource;

    use deadlib_present::actors::SizeSpec;

    #[test]
    fn background_start_cache_tracks_the_active_timeline_cursor() {
        let start_seconds = [0.25, 4.5, 9.75];

        assert_eq!(active_background_start_sec(&start_seconds, 0), None);
        assert_eq!(active_background_start_sec(&start_seconds, 1), Some(0.25));
        assert_eq!(active_background_start_sec(&start_seconds, 2), Some(4.5));
        assert_eq!(active_background_start_sec(&start_seconds, 3), Some(9.75));
        assert_eq!(active_background_start_sec(&start_seconds, 4), None);
    }

    #[test]
    fn frame_scratch_pointer_transfer_preserves_storage() {
        let mut scratch = GameplayFrameScratch::default();
        scratch.lobby_hud_status_scratch = String::with_capacity(128);

        let mut owner = Some(Box::new(scratch));
        let address = std::ptr::from_ref(owner.as_deref().unwrap());

        let detached = owner.take().unwrap();
        assert_eq!(detached.lobby_hud_status_scratch.capacity(), 128);

        owner = Some(detached);

        assert_eq!(std::ptr::from_ref(owner.as_deref().unwrap()), address);
    }

    #[test]
    fn step_stats_mode_preserves_option_behavior() {
        use profile_data::PlayStyle::{Double, Single, Versus};

        let cases = [
            (Single, 4, false, false, GameplayStepStatsMode::Hidden),
            (Single, 4, true, false, GameplayStepStatsMode::Side),
            (Single, 8, true, false, GameplayStepStatsMode::Hidden),
            (Double, 4, true, false, GameplayStepStatsMode::Side),
            (Double, 8, true, false, GameplayStepStatsMode::Double),
            (Double, 8, false, true, GameplayStepStatsMode::Hidden),
            (Versus, 8, false, false, GameplayStepStatsMode::Hidden),
            (Versus, 8, true, false, GameplayStepStatsMode::Versus),
            (Versus, 8, false, true, GameplayStepStatsMode::Versus),
        ];
        for (style, cols, p1, p2, expected) in cases {
            assert_eq!(gameplay_step_stats_mode(style, cols, p1, p2), expected);
        }
    }

    #[test]
    fn notefield_width_preserves_lane_span_spacing_and_receptor_scale() {
        let columns = [-96, -32, 32, 96];
        assert_eq!(notefield_layout_width(&columns, [128, 128], 4, 1.0), 256.0);
        assert_eq!(notefield_layout_width(&columns, [128, 128], 4, 1.5), 352.0);
        assert_eq!(notefield_layout_width(&columns, [96, 48], 4, 1.0), 320.0);
        assert_eq!(
            notefield_layout_width(&columns, [128, 128], 0, 1.0),
            DEFAULT_NOTEFIELD_WIDTH
        );
    }

    #[test]
    fn smx_sensor_value_content_preserves_value_and_idle_display() {
        let (value, value_color) = smx_sensor_value_content(Some(500));
        let (idle, idle_color) = smx_sensor_value_content(None);

        assert_eq!(value.as_str(), "500");
        assert!(matches!(value, TextContent::PrewarmedU16 { domain: 0, .. }));
        assert_eq!(value_color, SMX_SENSOR_VALUE_COLOR);
        assert_eq!(idle.as_str(), "--");
        assert_eq!(idle_color, SMX_SENSOR_VALUE_IDLE_COLOR);
    }

    #[test]
    fn bpm_decimal_shows_authored_precision_without_trailing_zeroes() {
        assert_eq!(owned_bpm_text(f64::from(100.001_f32), true), "100.001");
        assert_eq!(owned_bpm_text(f64::from(133.33_f32), true), "133.33");
        assert_eq!(owned_bpm_text(100.000, true), "100");
        assert_eq!(owned_bpm_text(150.0, true), "150");
        assert_eq!(owned_bpm_text(100.001, false), "100");
    }

    #[test]
    fn song_owned_hud_text_matches_numeric_formatting() {
        let mut bpm_plan = GameplayBpmTextPlan::new(0.0, false);
        for (bpm, show_decimal) in [
            (150.0, false),
            (133.33, true),
            (100.001, true),
            (f64::NAN, true),
            (f64::INFINITY, false),
            (150.0, false),
        ] {
            assert_eq!(
                bpm_plan.resolve(bpm, show_decimal).as_str(),
                owned_bpm_text(bpm, show_decimal)
            );
        }
        for thousandths in 0..=1_000_000u32 {
            let bpm = f64::from(thousandths) / 1_000.0;
            assert_eq!(
                bpm_plan.resolve(bpm, true).as_str(),
                owned_bpm_text(bpm, true),
                "decimal BPM mismatch at {bpm}",
            );
        }
        for bpm in [
            -100.0,
            -0.000_6,
            -0.0,
            f64::from(u32::MAX),
            f64::from(u32::MAX) + 0.499,
            f64::from(u32::MAX) + 1.0,
            f64::MAX,
        ] {
            for show_decimal in [false, true] {
                assert_eq!(
                    bpm_plan.resolve(bpm, show_decimal).as_str(),
                    owned_bpm_text(bpm, show_decimal),
                    "BPM boundary mismatch at {bpm} decimal={show_decimal}",
                );
            }
        }
        assert!(matches!(
            bpm_plan.resolve(133.33, true),
            TextContent::FrameInline {
                slot: FRAME_TEXT_BPM,
                ..
            }
        ));
        let life_plan = GameplayLifeTextPlan::new();
        for key in 0..=1_000 {
            let life = key as f32 / 10.0;
            assert_eq!(
                life_plan.resolve(life, key as usize % MAX_PLAYERS).as_str(),
                format!("{:.1}%", key as f32 / 10.0)
            );
        }
        for life in [87.34, 87.35, f32::NAN, f32::INFINITY, 87.34] {
            let key = quantize_tenths_u32(life).min(1_000);
            assert_eq!(
                life_plan.resolve(life, 0).as_str(),
                format!("{:.1}%", key as f32 / 10.0)
            );
        }
    }

    #[test]
    fn life_percent_text_resolves_for_enabled_meter_layouts() {
        let text_plan = GameplayLifeTextPlan::new();
        assert!(
            visible_life_percent_text(
                &text_plan,
                0,
                87.3,
                profile_data::LifeMeterType::Standard,
                false,
                true,
            )
            .is_none()
        );
        assert!(
            visible_life_percent_text(
                &text_plan,
                0,
                87.3,
                profile_data::LifeMeterType::Surround,
                true,
                true,
            )
            .is_none()
        );
        assert_eq!(
            visible_life_percent_text(
                &text_plan,
                0,
                100.0,
                profile_data::LifeMeterType::Vertical,
                true,
                true,
            )
            .as_ref()
            .map(TextContent::as_str),
            Some("100.0%")
        );
        assert_eq!(
            visible_life_percent_text(
                &text_plan,
                1,
                87.3,
                profile_data::LifeMeterType::Vertical,
                true,
                false,
            )
            .as_ref()
            .map(TextContent::as_str),
            Some("87.3%")
        );
        assert!(matches!(
            visible_life_percent_text(
                &text_plan,
                1,
                87.3,
                profile_data::LifeMeterType::Vertical,
                true,
                false,
            ),
            Some(TextContent::FrameInline { slot, .. }) if slot == FRAME_TEXT_LIFE_BASE + 1
        ));
    }

    #[test]
    fn life_meter_visual_matches_theme_tween_and_finish_semantics() {
        let mut visual = LifeMeterVisual::new(0.5, false);
        visual.update(0.25, false, 0.0);
        assert_eq!(
            visual.rendered_life(profile_data::LifeMeterType::Standard),
            0.5
        );

        visual.update(0.25, false, LIFE_BOUNCE_S * 0.5);
        let bounce = deadlib_present::anim::bouncebegin_p(0.5);
        let expected = (0.25_f32 - 0.5).mul_add(bounce, 0.5);
        assert!(
            (visual.rendered_life(profile_data::LifeMeterType::Vertical) - expected).abs() <= 1e-6
        );

        visual.update(0.4, false, 0.0);
        assert_eq!(
            visual.rendered_life(profile_data::LifeMeterType::Standard),
            0.25
        );
        visual.update(0.4, false, LIFE_SMOOTH_S * 0.5);
        assert!(
            (visual.rendered_life(profile_data::LifeMeterType::Surround) - 0.325).abs() <= 1e-6
        );
    }

    #[test]
    fn life_meter_visual_matches_hot_percent_and_vertical_swoosh_commands() {
        let mut visual = LifeMeterVisual::new(0.9, false);
        visual.update(1.0, false, 0.0);
        assert_eq!(
            visual.percent_visual(),
            LifePercentVisual {
                outer_w: 52.0,
                inner_w: 50.0,
                alpha: 1.0,
            }
        );
        assert_eq!(visual.vertical_swoosh_alpha, 1.0);

        visual.update(1.0, false, LIFE_PERCENT_FADE_S * 0.5);
        assert_eq!(visual.percent_visual().alpha, 0.75);
        visual.update(0.8, false, 0.0);
        assert_eq!(visual.percent_visual().alpha, 1.0);
        assert_eq!(visual.vertical_swoosh_alpha, 0.2);
    }

    #[test]
    fn rainbow_life_color_matches_itgmania_actor_rainbow() {
        let start = rainbow_life_color(0.0);
        let quarter = rainbow_life_color(0.5);
        for (actual, expected) in start.into_iter().zip([1.0, 0.25, 0.25, 1.0]) {
            assert!((actual - expected).abs() <= 1e-6);
        }
        for (actual, expected) in quarter.into_iter().zip([0.0, 0.75, 0.75, 1.0]) {
            assert!((actual - expected).abs() <= 1e-6);
        }
    }

    #[test]
    fn scorebox_polling_stops_when_loading_finishes() {
        let mut profiles: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS] =
            std::array::from_fn(|_| deadsync_score::GameplayScoreboxProfileSnapshot::default());
        profiles[0].display_scorebox = true;
        profiles[0].gs_active = true;
        let mut snapshots = [
            Some(score_data::CachedPlayerLeaderboardData::loading()),
            None,
        ];
        let mut rival_score_types = [None; MAX_PLAYERS];

        assert!(scorebox_refresh_pending_from(
            &profiles,
            &snapshots,
            &rival_score_types,
        ));
        snapshots[0] = Some(score_data::CachedPlayerLeaderboardData {
            loading: false,
            data: None,
            error: None,
        });
        assert!(!scorebox_refresh_pending_from(
            &profiles,
            &snapshots,
            &rival_score_types,
        ));

        snapshots[0] = Some(score_data::CachedPlayerLeaderboardData::loading());
        profiles[0].display_scorebox = false;
        assert!(!scorebox_refresh_pending_from(
            &profiles,
            &snapshots,
            &rival_score_types,
        ));
        rival_score_types[0] = Some(profile_data::MiniIndicatorScoreType::Itg);
        assert!(scorebox_refresh_pending_from(
            &profiles,
            &snapshots,
            &rival_score_types,
        ));
        profiles[0].display_scorebox = true;
        profiles[0].gs_active = false;
        assert!(!scorebox_refresh_pending_from(
            &profiles,
            &snapshots,
            &rival_score_types,
        ));
    }

    #[test]
    fn surround_life_color_preserves_responsive_rainbow_alpha() {
        let profile = profile_data::Profile {
            lifemeter_type: profile_data::LifeMeterType::Surround,
            rainbow_max: true,
            responsive_colors: true,
            ..profile_data::Profile::default()
        };
        let elapsed = 1.25;
        let color = surround_life_color(&profile, 1.0, elapsed);
        let rainbow = rainbow_life_color(elapsed);

        assert_eq!(&color[..3], &rainbow[..3]);
        assert_eq!(color[3], 0.2);
    }

    #[test]
    fn rate_text_is_empty_only_for_normal_speed() {
        assert!(cached_rate_text(1.0).is_empty());
        assert!(cached_rate_text(f32::NAN).is_empty());
        assert!(cached_rate_text(f32::INFINITY).is_empty());
        assert!(!cached_rate_text(1.25).is_empty());
    }

    #[test]
    fn sync_overlay_cache_preserves_text_and_refreshes_on_input_changes() {
        let replay_status = Arc::<str>::from("Replay (AutoPlay)");
        let inputs = [
            SyncOverlayTextInput {
                autoplay_enabled: false,
                replay_status: None,
                timing_tick_status: None,
                autosync_status: None,
                initial_global_offset: 0.0,
                global_offset: 0.0,
                initial_song_offset: 0.0,
                song_offset: 0.0,
            },
            SyncOverlayTextInput {
                autoplay_enabled: true,
                replay_status: None,
                timing_tick_status: None,
                autosync_status: None,
                initial_global_offset: 0.0,
                global_offset: 0.0,
                initial_song_offset: 0.0,
                song_offset: 0.0,
            },
            SyncOverlayTextInput {
                autoplay_enabled: true,
                replay_status: Some(&replay_status),
                timing_tick_status: Some("Assist Tick"),
                autosync_status: Some("AutoSync Song"),
                initial_global_offset: -0.010,
                global_offset: -0.007,
                initial_song_offset: 0.002,
                song_offset: -0.001,
            },
        ];
        let mut cache = SyncOverlayTextCache::default();
        for input in inputs {
            let expected = compose_sync_overlay_text(input);
            let actual = cache.resolve(input);
            assert_eq!(
                actual.as_ref().map(|(text, lines)| (text.as_ref(), *lines)),
                expected
                    .as_ref()
                    .map(|(text, lines)| (text.as_ref(), *lines))
            );
            let repeated = cache.resolve(input);
            assert_eq!(
                repeated
                    .as_ref()
                    .map(|(text, lines)| (text.as_ref(), *lines)),
                expected
                    .as_ref()
                    .map(|(text, lines)| (text.as_ref(), *lines))
            );
            if let (Some((actual, _)), Some((repeated, _))) = (&actual, &repeated) {
                assert!(Arc::ptr_eq(actual, repeated));
            }
        }
    }

    #[test]
    fn idle_sync_overlay_bypasses_cache_without_changing_output() {
        let input = SyncOverlayTextInput {
            autoplay_enabled: false,
            replay_status: None,
            timing_tick_status: None,
            autosync_status: None,
            initial_global_offset: -0.012,
            global_offset: -0.012,
            initial_song_offset: 0.003,
            song_offset: 0.003,
        };
        let cache = RefCell::new(SyncOverlayTextCache::default());

        assert!(input.is_idle());
        assert!(compose_sync_overlay_text(input).is_none());
        assert!(resolve_sync_overlay_text(&cache, input).is_none());
        let cache = cache.borrow();
        assert!(!cache.initialized);
        assert!(cache.key.is_none());
        assert!(cache.value.is_none());
    }

    #[test]
    fn gameplay_presentation_skeleton_builds_each_slot_once() {
        let builds = std::cell::Cell::new(0usize);
        let mut skeleton = GameplayPresentationSkeleton::default();
        let mut actors = Vec::new();

        for _ in 0..2 {
            skeleton.push(STATIC_HEADER, &mut actors, |children| {
                builds.set(builds.get() + 1);
                children.push(Actor::CameraPop);
            });
        }

        assert_eq!(builds.get(), 1);
        let [
            Actor::RetainedFrame { frame: first, .. },
            Actor::RetainedFrame { frame: second, .. },
        ] = actors.as_slice()
        else {
            panic!("static slots should emit retained frame wrappers");
        };
        assert!(Arc::ptr_eq(first, second));
    }

    fn ensure_i18n() {
        crate::i18n::init_for_tests();
    }

    #[test]
    fn custom_gameplay_backdrop_covers_full_screen_under_song_ui() {
        let mut actors = Vec::new();
        let color = deadlib_present::color::Color::from_hex("#0c0c0c").unwrap();

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

        push_custom_gameplay_backdrop(&mut actors, deadlib_present::color::Color::BLACK);

        assert!(actors.is_empty());
        assert_eq!(
            gameplay_header_rgba(deadlib_present::color::Color::BLACK),
            [0.0, 0.0, 0.0, 0.85]
        );
    }

    #[test]
    fn custom_gameplay_backdrop_tints_header() {
        let color = deadlib_present::color::Color::from_hex("#0c0c0c").unwrap();

        assert_eq!(gameplay_header_rgba(color), color.to_rgba());
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
    fn intro_text_width_cache_measures_once() {
        let cache = Cell::new(None);
        let calls = Cell::new(0);
        let first = cached_intro_text_width(&cache, || {
            calls.set(calls.get() + 1);
            123.5
        });
        let second = cached_intro_text_width(&cache, || {
            calls.set(calls.get() + 1);
            999.0
        });

        assert_eq!(first, 123.5);
        assert_eq!(second, first);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn difficulty_meter_overlap_uses_profile_target_offset() {
        deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
            854.0, 480.0,
        ));
        let mut profile = profile_data::Profile::default();

        assert!(!saved_targets_hit_meter(&profile, 4, DIFFICULTY_METER_Y));

        profile.note_field_offset_y = -50;
        assert!(saved_targets_hit_meter(&profile, 4, DIFFICULTY_METER_Y));
    }

    #[test]
    fn difficulty_meter_overlap_uses_profile_scroll_option() {
        deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
            854.0, 480.0,
        ));
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
            DIFFICULTY_METER_SIZE.mul_add(-0.5, screen_width())
        );
    }

    #[test]
    fn gameplay_hud_offset_tracks_player_side() {
        assert_eq!(
            offset_gameplay_hud_x(200.0, profile_data::PlayerSide::P1, 20),
            180.0
        );
        assert_eq!(
            offset_gameplay_hud_x(200.0, profile_data::PlayerSide::P2, 20),
            220.0
        );
        assert_eq!(
            offset_gameplay_hud_x(200.0, profile_data::PlayerSide::P1, 0),
            200.0
        );
    }

    #[test]
    fn gameplay_hud_offset_clamps_to_profile_range() {
        assert_eq!(
            offset_gameplay_hud_x(200.0, profile_data::PlayerSide::P1, -10),
            200.0
        );
        assert_eq!(
            offset_gameplay_hud_x(200.0, profile_data::PlayerSide::P2, 75),
            250.0
        );
    }

    #[test]
    fn upper_nps_graph_tracks_centered_target_x_offset() {
        let center_x = screen_center_x();
        let graph_w = 226.0;

        assert_eq!(
            upper_nps_graph_x(profile_data::PlayerSide::P1, center_x, graph_w, 20),
            center_x - graph_w * 0.5 - 20.0
        );
        assert_eq!(
            upper_nps_graph_x(profile_data::PlayerSide::P2, center_x, graph_w, 20),
            center_x - graph_w * 0.5 + 20.0
        );
    }

    #[test]
    fn upper_nps_graph_tracks_side_target_x_offset() {
        let center_x = screen_center_x();
        let graph_w = 226.0;
        let center_shift = widescale(45.0, 95.0);

        assert_eq!(
            upper_nps_graph_x(profile_data::PlayerSide::P1, center_x - 100.0, graph_w, 20),
            center_x - graph_w - center_shift - 20.0
        );
        assert_eq!(
            upper_nps_graph_x(profile_data::PlayerSide::P2, center_x + 100.0, graph_w, 20),
            center_x + center_shift + 20.0
        );
    }

    #[test]
    fn doubles_bpm_ignores_position_option() {
        let center_x = screen_center_x();
        let double_field_width = 8.0 * 64.0;
        for side in [profile_data::PlayerSide::P1, profile_data::PlayerSide::P2] {
            let bpm_x = |position, nps_graph_at_top| {
                gameplay_bpm_x(
                    position,
                    1,
                    profile_data::PlayStyle::Double,
                    side,
                    center_x,
                    double_field_width,
                    nps_graph_at_top,
                )
            };

            assert_eq!(
                bpm_x(
                    deadsync_config::theme::GameplayBpmPosition::TopCenter,
                    false
                ),
                center_x
            );
            assert_eq!(
                bpm_x(
                    deadsync_config::theme::GameplayBpmPosition::NearField,
                    false
                ),
                center_x
            );

            let top_center = bpm_x(deadsync_config::theme::GameplayBpmPosition::TopCenter, true);
            let near_field = bpm_x(deadsync_config::theme::GameplayBpmPosition::NearField, true);
            assert_eq!(near_field, top_center);
            assert_ne!(top_center, center_x);
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

    fn lobby_hud_input_event(
        action: VirtualAction,
        pressed: bool,
        timestamp: Instant,
    ) -> InputEvent {
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: deadsync_core::input::InputSource::Keyboard,
            timestamp,
            timestamp_host_nanos: 0,
            stored_at: timestamp,
            emitted_at: timestamp,
        }
    }

    fn test_joined_lobby(players: Vec<lobby_data::LobbyPlayer>) -> lobby_data::JoinedLobby {
        lobby_data::JoinedLobby {
            code: "ABCD".to_string(),
            players,
            song_info: None,
        }
    }

    #[test]
    fn gameplay_requires_wait_for_solo_joined_lobby() {
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", false)]);
        assert!(lobby_data::gameplay_lobby_wait_required(Some(&joined)));
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
        let mut actual = String::new();
        assert!(write_gameplay_lobby_wait_text(
            &joined,
            false,
            None,
            &mut actual
        ));
        assert_eq!(actual, expected);
    }

    #[test]
    fn gameplay_wait_text_unlocks_once_solo_lobby_player_is_ready() {
        let joined = test_joined_lobby(vec![test_lobby_player("ScreenGameplay", true)]);

        let mut actual = String::new();
        assert!(!write_gameplay_lobby_wait_text(
            &joined,
            true,
            None,
            &mut actual
        ));
        assert!(actual.is_empty());
    }

    #[test]
    fn lobby_hud_visibility_starts_visible_for_each_gameplay_state() {
        let visibility = GameplayLobbyHudVisibility::default();

        assert!(visibility.should_render(true, false));
        assert!(!visibility.should_render(false, false));
    }

    #[test]
    fn lobby_hud_menu_lr_chord_toggles_once_per_press_cycle_for_both_sides() {
        let at = Instant::now();
        let mut visibility = GameplayLobbyHudVisibility::default();
        let p1 = [true, false];

        assert!(!visibility.handle_input(
            &lobby_hud_input_event(VirtualAction::p1_menu_left, true, at),
            true,
            false,
            p1,
            profile_data::PlayerSide::P1,
        ));
        assert!(visibility.handle_input(
            &lobby_hud_input_event(VirtualAction::p1_menu_right, true, at),
            true,
            false,
            p1,
            profile_data::PlayerSide::P1,
        ));
        assert!(!visibility.visible);

        assert!(!visibility.handle_input(
            &lobby_hud_input_event(VirtualAction::p1_menu_right, true, at),
            true,
            false,
            p1,
            profile_data::PlayerSide::P1,
        ));
        assert!(!visibility.visible);

        for (action, pressed) in [
            (VirtualAction::p1_menu_left, false),
            (VirtualAction::p1_menu_right, false),
            (VirtualAction::p1_menu_left, true),
        ] {
            assert!(!visibility.handle_input(
                &lobby_hud_input_event(action, pressed, at),
                true,
                false,
                p1,
                profile_data::PlayerSide::P1,
            ));
        }
        assert!(visibility.handle_input(
            &lobby_hud_input_event(VirtualAction::p1_menu_right, true, at),
            true,
            false,
            p1,
            profile_data::PlayerSide::P1,
        ));
        assert!(visibility.visible);

        for action in [VirtualAction::p1_menu_left, VirtualAction::p1_menu_right] {
            assert!(!visibility.handle_input(
                &lobby_hud_input_event(action, false, at),
                true,
                false,
                p1,
                profile_data::PlayerSide::P1,
            ));
        }
        let p2 = [false, true];
        assert!(!visibility.handle_input(
            &lobby_hud_input_event(VirtualAction::p2_menu_left, true, at),
            true,
            false,
            p2,
            profile_data::PlayerSide::P2,
        ));
        assert!(visibility.handle_input(
            &lobby_hud_input_event(VirtualAction::p2_menu_right, true, at),
            true,
            false,
            p2,
            profile_data::PlayerSide::P2,
        ));
        assert!(!visibility.visible);
    }

    #[test]
    fn lobby_hud_toggle_ignores_dance_arrows_inactive_sides_and_non_lobby_play() {
        let at = Instant::now();
        let mut visibility = GameplayLobbyHudVisibility::default();

        for action in [VirtualAction::p1_left, VirtualAction::p1_right] {
            assert!(!visibility.handle_input(
                &lobby_hud_input_event(action, true, at),
                true,
                false,
                [true, false],
                profile_data::PlayerSide::P1,
            ));
        }
        for action in [VirtualAction::p2_menu_left, VirtualAction::p2_menu_right] {
            assert!(!visibility.handle_input(
                &lobby_hud_input_event(action, true, at),
                true,
                false,
                [true, false],
                profile_data::PlayerSide::P1,
            ));
        }
        for action in [VirtualAction::p1_menu_left, VirtualAction::p1_menu_right] {
            assert!(!visibility.handle_input(
                &lobby_hud_input_event(action, true, at),
                false,
                false,
                [true, false],
                profile_data::PlayerSide::P1,
            ));
        }
        assert!(visibility.visible);
    }

    #[test]
    fn lobby_hud_remains_rendered_and_does_not_toggle_during_ready_wait() {
        let at = Instant::now();
        let mut visibility = GameplayLobbyHudVisibility {
            visible: false,
            ..GameplayLobbyHudVisibility::default()
        };

        for action in [VirtualAction::p1_menu_left, VirtualAction::p1_menu_right] {
            assert!(!visibility.handle_input(
                &lobby_hud_input_event(action, true, at),
                true,
                true,
                [true, false],
                profile_data::PlayerSide::P1,
            ));
        }
        assert!(!visibility.visible);
        assert!(visibility.should_render(true, true));
    }

    #[test]
    fn banner_visibility_matches_step_statistics_layouts() {
        let empty = profile_data::StepStatisticsMask::empty();
        let both = profile_data::StepStatisticsMask::SONG_BANNER
            | profile_data::StepStatisticsMask::PACK_BANNER;

        assert_eq!(
            banner_visibility(profile_data::PlayStyle::Single, 4, true, false, both, empty),
            (true, true)
        );
        assert_eq!(
            banner_visibility(
                profile_data::PlayStyle::Single,
                4,
                false,
                false,
                both,
                empty
            ),
            (true, false)
        );
        assert_eq!(
            banner_visibility(profile_data::PlayStyle::Versus, 4, true, false, empty, both,),
            (true, false)
        );
        assert_eq!(
            banner_visibility(profile_data::PlayStyle::Double, 8, true, true, both, empty),
            (false, false)
        );
    }
}
