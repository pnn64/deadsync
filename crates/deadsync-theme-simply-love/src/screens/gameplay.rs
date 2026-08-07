use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::sprite_sheet_dims;
use crate::assets::{FontRole, machine_font_key, visual_styles};
use crate::screens::components::gameplay::score_counter::{
    ScoreCounterParams, prewarm_score_counter_layout, push_score_counter,
};
use crate::screens::components::gameplay::{gameplay_stats, notefield, step_stats_gifs};
use crate::screens::components::shared::banner as shared_banner;
use crate::screens::components::shared::heart_rate;
pub use crate::screens::components::shared::heart_rate::{HeartRatePlayerView, HeartRateView};
use crate::screens::components::shared::screen_bar::{self, AvatarParams, ScreenBarParams};
use crate::screens::components::shared::{gs_scorebox, lobby_hud};
use crate::screens::{Screen, ThemeEffect};
use crate::views::{GameplayInitView, GameplayRuntimeView, GameplayScoreRuntimeView};
use deadlib_present::actors::{
    Actor, ActorResourceArena, RetainedActorFrame, SharedActorFrameScratch, SizeSpec, SpriteSource,
    TextAlign, TextAttribute, TextAttributes, TextContent,
};
use deadlib_present::anim::EffectState;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::color;
use deadlib_present::compose::{ComposeScratch, TextLayoutCache};
use deadlib_present::density::{self, DensityHistCache};
use deadlib_present::font;
use deadlib_present::space::widescale;
use deadlib_present::space::{
    is_wide, screen_center_x, screen_center_y, screen_height, screen_width,
};
use deadlib_render::{
    BlendMode, INVALID_TMESH_CACHE_KEY, MeshVertex, TMeshCacheKey, TexturedMeshVertex,
};
use deadsync_assets::noteskin::{self, Noteskin, SpriteSlot};
use deadsync_assets::song_lua::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaOverlayActor, SongLuaOverlayBlendMode,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaOverlayMeshVertex,
    SongLuaOverlayMessageCommand, SongLuaOverlayModelDraw, SongLuaOverlayModelLayer,
    SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaProxyTarget, SongLuaTextGlowMode,
    compile_song_lua,
};
use deadsync_chart::{
    ChartData, GameplayChartData, SongBackgroundChange, SongBackgroundChangeTarget, SongData,
};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::song_time::song_time_ns_to_seconds;
use deadsync_gameplay::{
    AUTOSYNC_OFFSET_SAMPLE_COUNT, AutosyncMode, CourseDisplayCarry, CourseDisplayTiming,
    CourseDisplayTotals, CrossoverRow, ExitTransitionKind, FantasticWindowOptions, GameplayAction,
    GameplayAudioSnapshot, GameplayConfig, GameplayExit, GameplayNoteskinData,
    GameplayNoteskinEffects, GameplayReceptorGlowBehavior, GameplayReceptorStepBehavior,
    GameplaySession, GameplayTween, GameplayViewport, HoldToExitKey, LeadInTiming,
    MINE_EXPLOSION_DURATION, RECEPTOR_STEP_WINDOWS, RECEPTOR_Y_OFFSET_FROM_CENTER,
    RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, ReplayInputEdge, ReplayOffsetSnapshot,
    SongLuaOverlayMessageRuntime, SongLuaRuntimeVisuals, TAP_EXPLOSION_WINDOWS,
    autosync_mode_status_line, blue_fantastic_window_ms, build_crossover_rows,
    exit_transition_alpha, handle_core_input, scroll_receptor_y, song_lua_ease_factor,
    spacing_multiplier_for_percent, update_core,
};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_notefield::{
    BrokenRunLookup, CapturedActorScratch, CapturedActorSource, FieldPlacement, HoldMeshScratch,
    ModelMeshCache, ModelMeshCacheStats, ProxyCaptureRequests, SongLuaPlayerTransformRequest,
    StreamProgressLookup, ViewOverride, noteskin_model_actor_from_draw,
    noteskin_model_actor_from_draw_cached, song_lua_player_skew_x_matrix,
    song_lua_player_skew_y_matrix, song_lua_player_transform_matrix, song_lua_player_y_fold_actor,
};
use deadsync_noteskin::{
    ModelDrawState, NoteskinSlot, ReceptorGlowBehavior, ReceptorStepBehavior, Style, TweenType,
};
use deadsync_online::lobbies as lobby_data;
use deadsync_profile as profile_data;
use deadsync_profile_gameplay::{
    GameplayProfile, SongLuaRuntimeOverlayStateDelta, gameplay_pack_data,
    gameplay_runtime_profile_data, profile_side_from_gameplay, score_display_mode_from_profile,
    scroll_effects_from_option, song_lua_compile_context, song_lua_runtime_column_offset_windows,
    song_lua_runtime_ease_windows, song_lua_runtime_mod_windows,
};
use deadsync_rules::note::Note;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingSegments;
use deadsync_score as score_data;
use glam::{Mat4 as Matrix4, Vec3 as Vector3, Vec4 as Vector4};
use smallvec::SmallVec;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::hash::Hasher;
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

pub type GameplayCoreState = deadsync_gameplay::GameplayRuntimeState<
    deadsync_profile_gameplay::GameplayProfile,
    deadsync_assets::song_lua::SongLuaOverlayActor,
    deadsync_song_lua::SongLuaCapturedActor,
    deadsync_gameplay::SongLuaRuntimeOverlayStateDelta<deadsync_song_lua::SongLuaOverlayStateDelta>,
>;

const TEXT_CACHE_LIMIT: usize = 8192;
type SongLuaOverlayEaseWindowRuntime =
    deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<SongLuaRuntimeOverlayStateDelta>;

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

#[derive(Clone, Debug, PartialEq)]
struct SongLuaSoundEvent {
    second: f32,
    path: PathBuf,
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

#[cfg(feature = "bench-support")]
pub struct GameplayNotefieldWidthBench {
    column_xs: [i32; 8],
    receptor_size: [i32; 2],
    spacing_multiplier: f32,
    cached: f32,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayNotefieldWidthBench {
    fn default() -> Self {
        let column_xs = [-224, -160, -96, -32, 32, 96, 160, 224];
        let receptor_size = [128, 128];
        let spacing_multiplier = 1.25;
        Self {
            cached: notefield_layout_width(
                &column_xs,
                receptor_size,
                column_xs.len(),
                spacing_multiplier,
            ),
            column_xs,
            receptor_size,
            spacing_multiplier,
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplayNotefieldWidthBench {
    const SAMPLES: usize = 256;

    pub fn old_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let width = notefield_layout_width(
                std::hint::black_box(&self.column_xs),
                std::hint::black_box(self.receptor_size),
                self.column_xs.len(),
                std::hint::black_box(self.spacing_multiplier),
            );
            checksum.rotate_left(7) ^ width.to_bits() as usize ^ sample
        })
    }

    pub fn new_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            checksum.rotate_left(7) ^ std::hint::black_box(self.cached).to_bits() as usize ^ sample
        })
    }
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

#[derive(Clone, Copy, Debug)]
pub struct ActorViewOverride {
    pub notefield: NotefieldViewOverride,
    pub hide_gameplay_hud: bool,
    /// Alpha multiplier applied to SMX overlay actors (FSR sensor display and pad
    /// input display). Used to fade them in with the screen transition.
    pub smx_overlay_alpha: f32,
}

impl Default for ActorViewOverride {
    fn default() -> Self {
        Self {
            notefield: NotefieldViewOverride::default(),
            hide_gameplay_hud: false,
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
/// The black holds solid for (TRANSITION_IN_DURATION - this), then lifts over this window.
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
    fn from_name(name: &str) -> Option<Self> {
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
fn gameplay_tween(tween: TweenType) -> GameplayTween {
    match tween {
        TweenType::Linear => GameplayTween::Linear,
        TweenType::Accelerate => GameplayTween::Accelerate,
        TweenType::Decelerate => GameplayTween::Decelerate,
    }
}

#[inline(always)]
fn gameplay_receptor_glow_behavior(behavior: ReceptorGlowBehavior) -> GameplayReceptorGlowBehavior {
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
fn gameplay_receptor_step_behavior(behavior: ReceptorStepBehavior) -> GameplayReceptorStepBehavior {
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

/// Song-lifetime state/order execution plan for one compiled overlay tree.
///
/// The gameplay frame thread owns it without synchronization. Screen entry
/// sizes every child list, records the only actors whose state can change,
/// propagates that set through descendants, and flattens immutable root order.
/// Gameplay performs no insertion, growth, eviction, pruning, or destruction;
/// dynamic ordering saturates at the compiled actor count and only re-sorts a
/// bounded sibling list after a key change. All storage is released at the
/// gameplay transition. The frame benchmark exposes full-scan versus planned
/// state updates and recursive versus flat ordering.
#[derive(Default)]
struct SongLuaOverlayOrderCache {
    child_lists: Vec<Vec<usize>>,
    dynamic_draw_order: Vec<bool>,
    // Song-lifetime execution plan: only these actors can change local state,
    // and only these composed states depend on changing local/ancestor state.
    dynamic_local_indices: Box<[usize]>,
    dynamic_composed_indices: Box<[usize]>,
    // Most Song Lua trees never mutate ordering fields. Flatten those trees at
    // screen entry so the frame loop can copy one contiguous index slice.
    static_root_order: Option<Box<[usize]>>,
    sort_modes: Vec<u8>,
    // Dynamic-capable lists usually keep the same keys for many frames. Remember
    // those keys so we only pay O(n log n) when their effective order changes.
    last_draw_orders: Vec<i32>,
    last_z_keys: Vec<u32>,
}

// Built once per song or visual layer so frame rendering does not repeatedly
// walk actor parents or resolve ActorFrameTexture names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
struct SongLuaOverlayIndex(Option<NonZeroUsize>);

impl SongLuaOverlayIndex {
    fn new(index: Option<usize>) -> Self {
        Self(
            index
                .and_then(|index| index.checked_add(1))
                .and_then(NonZeroUsize::new),
        )
    }

    fn get(self) -> Option<usize> {
        self.0.map(|index| index.get() - 1)
    }
}

/// Song-lifetime topology lookup for Song Lua overlay rendering.
///
/// The gameplay screen owns one immutable index for the root overlays and one
/// per visual layer. Construction runs during screen entry, groups AFT sprites
/// by capture name, stores each peer index once, and pre-sizes an inverse draw
/// position and resolved-group buffers. Static camera scopes are resolved at
/// screen entry; trees whose camera FOV can change retain the indexed ancestor
/// fallback. The game/render thread updates RGB buffers linearly before
/// rendering a layer; canonical three-channel groups are resolved once rather
/// than once per channel. There is no synchronization, gameplay allocation,
/// miss, eviction, or pruning path. Storage is bounded by the compiled overlay
/// count and is released with the gameplay screen.
/// `storage_bytes` exposes the retained index footprint for benchmarks.
#[derive(Default)]
struct SongLuaOverlayTopologyIndex {
    aft_ancestors: Vec<SongLuaOverlayIndex>,
    aft_sprite_targets: Vec<SongLuaOverlayIndex>,
    camera_ancestors: Vec<SongLuaOverlayIndex>,
    camera_states: Vec<SongLuaOverlayIndex>,
    dynamic_camera_scope: bool,
    aft_peer_groups: Box<[Box<[usize]>]>,
    draw_positions: Vec<usize>,
    rgb_aft_groups: Vec<Option<(usize, [usize; 3])>>,
}

impl SongLuaOverlayTopologyIndex {
    fn new(overlays: &[SongLuaOverlayActor]) -> Self {
        let aft_ancestors = (0..overlays.len())
            .map(|index| SongLuaOverlayIndex::new(song_lua_overlay_aft_ancestor(overlays, index)))
            .collect::<Vec<_>>();
        let aft_sprite_targets = overlays
            .iter()
            .map(|overlay| match &overlay.kind {
                SongLuaOverlayKind::AftSprite { capture_name } => SongLuaOverlayIndex::new(
                    song_lua_overlay_capture_index_by_name(overlays, capture_name),
                ),
                _ => SongLuaOverlayIndex::default(),
            })
            .collect();
        let camera_ancestors = overlays
            .iter()
            .map(|overlay| {
                SongLuaOverlayIndex::new(song_lua_overlay_camera_ancestor(
                    overlays,
                    overlay.parent_index,
                ))
            })
            .collect();
        let mut camera_states = vec![SongLuaOverlayIndex::default(); overlays.len()];
        Self::fill_camera_states(
            overlays,
            |index| overlays.get(index).map(|overlay| overlay.initial_state),
            &mut camera_states,
        );
        let dynamic_camera_scope = overlays.iter().any(|overlay| {
            matches!(
                overlay.kind,
                SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
            ) && overlay
                .message_commands
                .iter()
                .any(|command| command.blocks.iter().any(|block| block.delta.fov.is_some()))
        });
        let mut aft_sprite_groups = vec![SongLuaOverlayIndex::default(); overlays.len()];
        let mut aft_peer_groups: Vec<Vec<usize>> = Vec::new();
        for (index, overlay) in overlays.iter().enumerate() {
            let SongLuaOverlayKind::AftSprite { capture_name } = &overlay.kind else {
                continue;
            };
            let group = overlays[..index]
                .iter()
                .enumerate()
                .find_map(|(candidate_index, candidate)| {
                    let SongLuaOverlayKind::AftSprite {
                        capture_name: candidate_capture,
                    } = &candidate.kind
                    else {
                        return None;
                    };
                    candidate_capture
                        .eq_ignore_ascii_case(capture_name)
                        .then(|| aft_sprite_groups[candidate_index].get())
                        .flatten()
                })
                .unwrap_or_else(|| {
                    aft_peer_groups.push(Vec::new());
                    aft_peer_groups.len() - 1
                });
            aft_sprite_groups[index] = SongLuaOverlayIndex::new(Some(group));
            aft_peer_groups[group].push(index);
        }
        Self {
            aft_ancestors,
            aft_sprite_targets,
            camera_ancestors,
            camera_states,
            dynamic_camera_scope,
            aft_peer_groups: aft_peer_groups
                .into_iter()
                .map(Vec::into_boxed_slice)
                .collect(),
            draw_positions: vec![usize::MAX; overlays.len()],
            rgb_aft_groups: vec![None; overlays.len()],
        }
    }

    fn fill_camera_states(
        overlays: &[SongLuaOverlayActor],
        mut state_at: impl FnMut(usize) -> Option<SongLuaOverlayState>,
        camera_states: &mut [SongLuaOverlayIndex],
    ) {
        camera_states.fill(SongLuaOverlayIndex::default());
        for (index, overlay) in overlays.iter().enumerate() {
            let camera_index = overlay.parent_index.and_then(|parent_index| {
                let parent = overlays.get(parent_index)?;
                let parent_state = state_at(parent_index)?;
                if parent_index >= index {
                    return None;
                }
                if matches!(
                    parent.kind,
                    SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
                ) && parent_state.fov.is_some()
                {
                    Some(parent_index)
                } else {
                    camera_states[parent_index].get()
                }
            });
            camera_states[index] = SongLuaOverlayIndex::new(camera_index);
        }
    }

    fn include_camera_eases(
        &mut self,
        overlays: &[SongLuaOverlayActor],
        overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    ) {
        self.dynamic_camera_scope |= overlay_eases.iter().any(|ease| {
            overlays.get(ease.overlay_index).is_some_and(|overlay| {
                matches!(
                    overlay.kind,
                    SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
                ) && (ease.from.delta.fov.is_some() || ease.to.delta.fov.is_some())
            })
        });
    }

    #[cfg(test)]
    fn rebuild_camera_states(
        &mut self,
        overlays: &[SongLuaOverlayActor],
        overlay_states: &[SongLuaOverlayState],
    ) {
        Self::fill_camera_states(
            overlays,
            |index| overlay_states.get(index).copied(),
            &mut self.camera_states,
        );
    }

    #[inline(always)]
    fn camera_state(
        &self,
        overlay_states: &[SongLuaOverlayState],
        index: usize,
    ) -> Option<SongLuaOverlayState> {
        if self.dynamic_camera_scope {
            let mut candidate = self
                .camera_ancestors
                .get(index)
                .copied()
                .and_then(SongLuaOverlayIndex::get);
            while let Some(current) = candidate {
                let state = overlay_states.get(current).copied()?;
                if state.fov.is_some() {
                    return Some(state);
                }
                candidate = self
                    .camera_ancestors
                    .get(current)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get);
            }
            return None;
        }
        self.camera_states
            .get(index)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
            .and_then(|camera_index| overlay_states.get(camera_index))
            .copied()
    }

    fn prepare_rgb_draw_positions(&mut self, draw_order: &[usize]) {
        if self.aft_peer_groups.iter().all(|group| group.len() < 3) {
            return;
        }
        self.draw_positions.fill(usize::MAX);
        for (position, index) in draw_order.iter().copied().enumerate() {
            if let Some(slot) = self.draw_positions.get_mut(index) {
                *slot = position;
            }
        }
    }

    fn prepare_rgb_aft_groups(
        &mut self,
        overlay_states: &[SongLuaOverlayState],
        draw_order: &[usize],
    ) {
        self.prepare_rgb_draw_positions(draw_order);
        self.rgb_aft_groups.fill(None);
        let peer_groups = &self.aft_peer_groups;
        let draw_positions = &self.draw_positions;
        let resolved = &mut self.rgb_aft_groups;
        for peers in peer_groups {
            if peers.len() < 3 {
                continue;
            }
            if peers.len() == 3 {
                let Some(first) = peers.first().copied() else {
                    continue;
                };
                if let Some(group) = song_lua_rgb_aft_group_from_peers(
                    overlay_states,
                    first,
                    peers.iter().copied(),
                    draw_positions,
                ) {
                    for &index in peers.iter() {
                        resolved[index] = Some(group);
                    }
                }
                continue;
            }
            for &index in peers.iter() {
                resolved[index] = song_lua_rgb_aft_group_from_peers(
                    overlay_states,
                    index,
                    peers.iter().copied(),
                    draw_positions,
                );
            }
        }
    }

    #[inline(always)]
    fn rgb_aft_group(&self, index: usize) -> Option<(usize, [usize; 3])> {
        self.rgb_aft_groups.get(index).copied().flatten()
    }

    #[cfg(feature = "bench-support")]
    fn storage_bytes(&self) -> usize {
        let fixed = self
            .aft_ancestors
            .len()
            .saturating_add(self.aft_sprite_targets.len())
            .saturating_add(self.camera_ancestors.len())
            .saturating_add(self.camera_states.len())
            .saturating_mul(std::mem::size_of::<SongLuaOverlayIndex>());
        let positions = self
            .draw_positions
            .len()
            .saturating_mul(std::mem::size_of::<usize>());
        let resolved = self
            .rgb_aft_groups
            .len()
            .saturating_mul(std::mem::size_of::<Option<(usize, [usize; 3])>>());
        let groups = self
            .aft_peer_groups
            .len()
            .saturating_mul(std::mem::size_of::<Box<[usize]>>());
        let peers = self
            .aft_peer_groups
            .iter()
            .map(|group| group.len())
            .sum::<usize>()
            .saturating_mul(std::mem::size_of::<usize>());
        fixed
            .saturating_add(positions)
            .saturating_add(groups)
            .saturating_add(peers)
            .saturating_add(resolved)
    }
}

const PROJECTED_MESH_VERTEX_CAPACITY: usize = 54;
const SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS: usize = 64;

/// Screen-owned reusable storage for one dynamic Song Lua mesh.
///
/// One scratch slot is prewarmed for every compiled projected Sprite/Quad and
/// ActorMultiVertex during gameplay entry. The game/render thread is the sole
/// writer and clones the base/glow `Arc<Vec<_>>` buffers into actors each frame.
/// A normal next frame recovers each uniquely owned buffer, clears it, and
/// performs no allocation; if an older actor still owns a buffer, the slot
/// replaces it rather than mutating shared data. Capacity is fixed to the
/// compiled vertex count or the 4x4 fade grid's 54 triangle vertices, with no
/// eviction or pruning. Buffers and replaced generations are freed outside
/// their live actor use, ultimately with the screen. Replacement and capacity
/// counters are exposed to the benchmark; worst-case frame work is bounded by
/// that actor's compiled vertex count.
/// Static BitmapText uppercase content and the seven rainbow-scroll phases are
/// also compiled into this overlay-local song-lifetime storage during screen
/// entry. Rainbow phase storage is capped at 64 characters per actor; larger
/// text reuses one current-phase buffer sized from the compiled text instead of
/// retaining seven copies or allocating during gameplay. GraphDisplay body/line geometry and its two-child frame are sized
/// from the compiled point count. Static Model glow vertices and stable
/// renderer geometry keys are computed once per layer during entry.
/// NoteskinActor slots get an exact-capacity, sealed model cache plus one
/// pre-whitened glow array per model slot. A gameplay miss preserves output but
/// counts as failed prewarm and may allocate; registered slots never grow,
/// prune, or evict, and all model storage is destroyed with the song screen.
/// BitmapText gets two buffers sized for its compiled
/// spans plus one whole-text span: one for dynamic diffuse composition and one
/// for glow/stroke extraction. Gameplay frames only refill or clone these
/// prewarmed buffers. The fallback conversion also supports test-only builders
/// that do not provide screen scratch.
#[derive(Default)]
struct SongLuaProjectedMeshScratch {
    textured_vertices: Option<Arc<Vec<TexturedMeshVertex>>>,
    textured_glow_vertices: Option<Arc<Vec<TexturedMeshVertex>>>,
    mesh_vertices: Option<Arc<Vec<MeshVertex>>>,
    graph_body_vertices: Option<Arc<Vec<MeshVertex>>>,
    graph_line_vertices: Option<Arc<Vec<MeshVertex>>>,
    graph_body_key: Option<[u32; 10]>,
    graph_line_key: Option<[u32; 11]>,
    graph_frame: Option<SharedActorFrameScratch>,
    model_geometry_keys: Option<Vec<TMeshCacheKey>>,
    model_glow_vertices: Option<Vec<Arc<[TexturedMeshVertex]>>>,
    noteskin_model_cache: Option<ModelMeshCache>,
    noteskin_glow_vertices: Option<Vec<Option<Arc<[TexturedMeshVertex]>>>>,
    text_diffuse_attributes: Option<Arc<Vec<TextAttribute>>>,
    text_glow_attributes: Option<Arc<Vec<TextAttribute>>>,
    uppercase_text: Option<Arc<str>>,
    rainbow_text_attributes: Option<[Arc<[TextAttribute]>; SONG_LUA_TEXT_RAINBOW_COLORS.len()]>,
    capacity: usize,
    text_attribute_capacity: usize,
    replacements: u64,
}

impl SongLuaProjectedMeshScratch {
    fn textured(capacity: usize) -> Self {
        Self {
            textured_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            textured_glow_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            mesh_vertices: None,
            graph_body_vertices: None,
            graph_line_vertices: None,
            graph_body_key: None,
            graph_line_key: None,
            graph_frame: None,
            model_geometry_keys: None,
            model_glow_vertices: None,
            noteskin_model_cache: None,
            noteskin_glow_vertices: None,
            text_diffuse_attributes: None,
            text_glow_attributes: None,
            uppercase_text: None,
            rainbow_text_attributes: None,
            capacity,
            text_attribute_capacity: 0,
            replacements: 0,
        }
    }

    fn mesh(capacity: usize) -> Self {
        Self {
            textured_vertices: None,
            textured_glow_vertices: None,
            mesh_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            graph_body_vertices: None,
            graph_line_vertices: None,
            graph_body_key: None,
            graph_line_key: None,
            graph_frame: None,
            model_geometry_keys: None,
            model_glow_vertices: None,
            noteskin_model_cache: None,
            noteskin_glow_vertices: None,
            text_diffuse_attributes: None,
            text_glow_attributes: None,
            uppercase_text: None,
            rainbow_text_attributes: None,
            capacity,
            text_attribute_capacity: 0,
            replacements: 0,
        }
    }

    fn graph(capacity: usize) -> Self {
        Self {
            graph_body_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            graph_line_vertices: Some(Arc::new(Vec::with_capacity(capacity))),
            graph_frame: Some(SharedActorFrameScratch::with_capacity(2)),
            capacity,
            ..Self::default()
        }
    }

    fn model(layers: &[SongLuaOverlayModelLayer]) -> Self {
        let model_geometry_keys = layers
            .iter()
            .map(|layer| song_lua_model_geometry_key(&layer.vertices))
            .collect();
        let model_glow_vertices = layers
            .iter()
            .map(|layer| song_lua_static_glow_vertices(&layer.vertices))
            .collect();
        Self {
            model_geometry_keys: Some(model_geometry_keys),
            model_glow_vertices: Some(model_glow_vertices),
            ..Self::default()
        }
    }

    fn noteskin(slots: &[SpriteSlot]) -> Self {
        let mut model_cache = ModelMeshCache::with_capacity(slots.len());
        for slot in slots {
            model_cache.prewarm_slot(slot);
        }
        let noteskin_glow_vertices = slots
            .iter()
            .map(|slot| {
                model_cache
                    .model_geometry(slot)
                    .map(|(_, vertices)| song_lua_static_glow_vertices(&vertices))
            })
            .collect();
        model_cache.seal();
        Self {
            noteskin_model_cache: Some(model_cache),
            noteskin_glow_vertices: Some(noteskin_glow_vertices),
            ..Self::default()
        }
    }

    fn prewarm_text_attributes(&mut self, attribute_count: usize, text_char_count: usize) {
        let capacity = attribute_count.saturating_add(1).max(text_char_count);
        self.text_diffuse_attributes = Some(Arc::new(Vec::with_capacity(capacity)));
        self.text_glow_attributes = Some(Arc::new(Vec::with_capacity(capacity)));
        self.text_attribute_capacity = capacity;
    }

    fn update_projected(
        &mut self,
        grid: &[TexturedMeshVertex],
        width: usize,
        height: usize,
    ) -> Arc<Vec<TexturedMeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.textured_vertices,
            self.capacity,
            &mut self.replacements,
            |vertices| append_projected_mesh_vertices(grid, width, height, vertices),
        )
    }

    fn update_textured(
        &mut self,
        fill: impl FnOnce(&mut Vec<TexturedMeshVertex>),
    ) -> Arc<Vec<TexturedMeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.textured_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_textured_glow(
        &mut self,
        vertices: &[TexturedMeshVertex],
    ) -> Arc<Vec<TexturedMeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.textured_glow_vertices,
            self.capacity,
            &mut self.replacements,
            |out| {
                out.extend(vertices.iter().copied().map(|mut vertex| {
                    vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                    vertex
                }));
            },
        )
    }

    fn rainbow_attributes(&mut self, text: &str, total_elapsed: f32) -> TextAttributes {
        let phase = song_lua_rainbow_scroll_phase(total_elapsed);
        if let Some(attributes) = self.rainbow_text_attributes.as_ref() {
            return TextAttributes::from(Arc::clone(&attributes[phase]));
        }
        self.update_text_diffuse(|out| {
            append_song_lua_rainbow_scroll_attributes_at_phase(text, phase, out);
        })
    }

    fn update_mesh(&mut self, fill: impl FnOnce(&mut Vec<MeshVertex>)) -> Arc<Vec<MeshVertex>> {
        update_song_lua_shared_vec(
            &mut self.mesh_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_graph_body(
        &mut self,
        key: [u32; 10],
        fill: impl FnOnce(&mut Vec<MeshVertex>),
    ) -> Arc<Vec<MeshVertex>> {
        if self.graph_body_key == Some(key)
            && let Some(vertices) = self.graph_body_vertices.as_ref()
        {
            return Arc::clone(vertices);
        }
        self.graph_body_key = Some(key);
        update_song_lua_shared_vec(
            &mut self.graph_body_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_graph_line(
        &mut self,
        key: [u32; 11],
        fill: impl FnOnce(&mut Vec<MeshVertex>),
    ) -> Arc<Vec<MeshVertex>> {
        if self.graph_line_key == Some(key)
            && let Some(vertices) = self.graph_line_vertices.as_ref()
        {
            return Arc::clone(vertices);
        }
        self.graph_line_key = Some(key);
        update_song_lua_shared_vec(
            &mut self.graph_line_vertices,
            self.capacity,
            &mut self.replacements,
            fill,
        )
    }

    fn update_text_diffuse(
        &mut self,
        fill: impl FnOnce(&mut Vec<TextAttribute>),
    ) -> TextAttributes {
        TextAttributes::from(update_song_lua_shared_vec(
            &mut self.text_diffuse_attributes,
            self.text_attribute_capacity,
            &mut self.replacements,
            fill,
        ))
    }

    fn update_text_glow(&mut self, fill: impl FnOnce(&mut Vec<TextAttribute>)) -> TextAttributes {
        TextAttributes::from(update_song_lua_shared_vec(
            &mut self.text_glow_attributes,
            self.text_attribute_capacity,
            &mut self.replacements,
            fill,
        ))
    }

    #[cfg(feature = "bench-support")]
    fn storage_bytes(&self) -> usize {
        let textured = self
            .textured_vertices
            .as_ref()
            .map_or(0, |vertices| vertices.capacity())
            .saturating_mul(std::mem::size_of::<TexturedMeshVertex>());
        let plain = self
            .mesh_vertices
            .as_ref()
            .map_or(0, |vertices| vertices.capacity())
            .saturating_mul(std::mem::size_of::<MeshVertex>());
        let textured_glow = self
            .textured_glow_vertices
            .as_ref()
            .map_or(0, |vertices| vertices.capacity())
            .saturating_mul(std::mem::size_of::<TexturedMeshVertex>());
        let graph = self
            .graph_body_vertices
            .as_ref()
            .map_or(0, |vertices| vertices.capacity())
            .saturating_add(
                self.graph_line_vertices
                    .as_ref()
                    .map_or(0, |vertices| vertices.capacity()),
            )
            .saturating_mul(std::mem::size_of::<MeshVertex>());
        let graph_frame = self
            .graph_frame
            .as_ref()
            .map_or(0, SharedActorFrameScratch::capacity)
            .saturating_mul(std::mem::size_of::<Actor>());
        let model_glow = self.model_glow_vertices.as_ref().map_or(0, |layers| {
            layers
                .iter()
                .map(|vertices| {
                    vertices
                        .len()
                        .saturating_mul(std::mem::size_of::<TexturedMeshVertex>())
                })
                .sum()
        });
        let model_keys = self
            .model_geometry_keys
            .as_ref()
            .map_or(0, |keys| keys.capacity())
            .saturating_mul(std::mem::size_of::<TMeshCacheKey>());
        let noteskin_models = self.noteskin_glow_vertices.as_ref().map_or(0, |slots| {
            slots
                .iter()
                .flatten()
                .map(|vertices| {
                    vertices
                        .len()
                        .saturating_mul(std::mem::size_of::<TexturedMeshVertex>())
                        .saturating_mul(2)
                })
                .sum()
        });
        let dynamic_text = self
            .text_diffuse_attributes
            .as_ref()
            .map_or(0, |attributes| attributes.capacity())
            .saturating_add(
                self.text_glow_attributes
                    .as_ref()
                    .map_or(0, |attributes| attributes.capacity()),
            )
            .saturating_mul(std::mem::size_of::<TextAttribute>());
        let rainbow = self
            .rainbow_text_attributes
            .as_ref()
            .map_or(0, |phases| {
                phases.iter().map(|attributes| attributes.len()).sum()
            })
            .saturating_mul(std::mem::size_of::<TextAttribute>());
        textured
            .saturating_add(textured_glow)
            .saturating_add(plain)
            .saturating_add(graph)
            .saturating_add(graph_frame)
            .saturating_add(model_keys)
            .saturating_add(model_glow)
            .saturating_add(noteskin_models)
            .saturating_add(dynamic_text)
            .saturating_add(rainbow)
    }
}

fn song_lua_static_glow_vertices(vertices: &[TexturedMeshVertex]) -> Arc<[TexturedMeshVertex]> {
    Arc::from(
        vertices
            .iter()
            .copied()
            .map(|mut vertex| {
                vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                vertex
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

fn song_lua_model_geometry_key(vertices: &[TexturedMeshVertex]) -> TMeshCacheKey {
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(b"deadsync-song-lua-model-v1");
    hasher.write_usize(vertices.len());
    for vertex in vertices {
        for value in vertex.pos {
            hasher.write_u32(value.to_bits());
        }
        for value in vertex.uv {
            hasher.write_u32(value.to_bits());
        }
        for value in vertex.tex_matrix_scale {
            hasher.write_u32(value.to_bits());
        }
        for value in vertex.color {
            hasher.write_u32(value.to_bits());
        }
    }
    hasher.finish().max(1)
}

fn song_lua_glow_geometry_key(base: TMeshCacheKey) -> TMeshCacheKey {
    if base == INVALID_TMESH_CACHE_KEY {
        return INVALID_TMESH_CACHE_KEY;
    }
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(b"deadsync-song-lua-glow-v1");
    hasher.write_u64(base);
    hasher.finish().max(1)
}

fn update_song_lua_shared_vec<T>(
    shared: &mut Option<Arc<Vec<T>>>,
    capacity: usize,
    replacements: &mut u64,
    fill: impl FnOnce(&mut Vec<T>),
) -> Arc<Vec<T>> {
    let shared = shared.get_or_insert_with(|| Arc::new(Vec::with_capacity(capacity)));
    if Arc::get_mut(shared).is_none() {
        *replacements = (*replacements).saturating_add(1);
        *shared = Arc::new(Vec::with_capacity(capacity));
    }
    let values = Arc::get_mut(shared).expect("shared vector was just made unique");
    values.clear();
    fill(values);
    Arc::clone(shared)
}

fn song_lua_projected_mesh_scratch_for(
    overlays: &[SongLuaOverlayActor],
) -> Vec<SongLuaProjectedMeshScratch> {
    overlays
        .iter()
        .map(|overlay| {
            let mut scratch = match &overlay.kind {
                SongLuaOverlayKind::Sprite { .. } | SongLuaOverlayKind::Quad => {
                    SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY)
                }
                SongLuaOverlayKind::ActorMultiVertex {
                    vertices,
                    texture_key: Some(_),
                    ..
                } => SongLuaProjectedMeshScratch::textured(vertices.len()),
                SongLuaOverlayKind::ActorMultiVertex { vertices, .. } => {
                    SongLuaProjectedMeshScratch::mesh(vertices.len())
                }
                SongLuaOverlayKind::GraphDisplay { body_values, .. } => {
                    let vertex_count = graph_display_values_or_default(body_values)
                        .len()
                        .saturating_sub(1)
                        .saturating_mul(6);
                    SongLuaProjectedMeshScratch::graph(vertex_count)
                }
                SongLuaOverlayKind::Model { layers } => SongLuaProjectedMeshScratch::model(layers),
                SongLuaOverlayKind::NoteskinActor { slots } => {
                    SongLuaProjectedMeshScratch::noteskin(slots)
                }
                _ => SongLuaProjectedMeshScratch::default(),
            };
            if let SongLuaOverlayKind::BitmapText {
                text, attributes, ..
            } = &overlay.kind
            {
                let uppercase_text = Arc::<str>::from(text.to_uppercase());
                let char_count = text.chars().count();
                scratch.prewarm_text_attributes(
                    attributes.len(),
                    char_count.max(uppercase_text.chars().count()),
                );
                scratch.uppercase_text = Some(uppercase_text);
                if (1..=SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS).contains(&char_count) {
                    scratch.rainbow_text_attributes =
                        Some(song_lua_rainbow_scroll_phases(text.as_ref()));
                }
            }
            scratch
        })
        .collect()
}

#[derive(Default)]
struct SongLuaProxyRequestIndex {
    topology: SongLuaOverlayTopologyIndex,
    root_indices: Vec<usize>,
    capture_children: Vec<Vec<usize>>,
    proxy_indices: Vec<usize>,
}

/// Song-lifetime visitation marks for nested AFT/proxy traversals.
///
/// Owner/thread model: gameplay `State`, used only by the frame-building
/// thread. Lifetime: one song. Capacity is the largest compiled overlay layer
/// and is populated at screen entry. Each traversal advances a generation, so
/// gameplay frames neither clear the full array nor allocate a recursion stack.
/// A wrapped generation performs one bounded fill. Duplicate references and
/// cycles are skipped after their first visit; there is no eviction, pruning,
/// or destruction before the gameplay transition. Worst-case frame work is one
/// visit per compiled capture rather than one visit per reference.
#[derive(Default)]
struct SongLuaCaptureVisitScratch {
    marks: Vec<u32>,
    generation: u32,
}

impl SongLuaCaptureVisitScratch {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            marks: vec![0; capacity],
            generation: 0,
        }
    }

    fn begin(&mut self, required: usize) {
        debug_assert!(
            required <= self.marks.len(),
            "song-lifetime capture marks must cover every compiled overlay"
        );
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.marks.fill(0);
            self.generation = 1;
        }
    }

    fn visit(&mut self, index: usize) -> bool {
        let Some(mark) = self.marks.get_mut(index) else {
            return false;
        };
        if *mark == self.generation {
            return false;
        }
        *mark = self.generation;
        true
    }
}

impl SongLuaProxyRequestIndex {
    fn new(overlays: &[SongLuaOverlayActor]) -> Self {
        let topology = SongLuaOverlayTopologyIndex::new(overlays);
        let mut root_indices = Vec::with_capacity(overlays.len());
        let mut capture_children = vec![Vec::new(); overlays.len()];
        for (index, ancestor) in topology
            .aft_ancestors
            .iter()
            .copied()
            .map(SongLuaOverlayIndex::get)
            .enumerate()
        {
            if let Some(ancestor) = ancestor {
                if let Some(children) = capture_children.get_mut(ancestor) {
                    children.push(index);
                }
            } else {
                root_indices.push(index);
            }
        }
        let proxy_indices = overlays
            .iter()
            .enumerate()
            .filter_map(|(index, overlay)| {
                matches!(overlay.kind, SongLuaOverlayKind::ActorProxy { .. }).then_some(index)
            })
            .collect();
        Self {
            topology,
            root_indices,
            capture_children,
            proxy_indices,
        }
    }
}

// Amortizes message-history replay across monotonic gameplay frames. A seek
// before the last consumed event resets the cursor and replays from the start.
#[derive(Clone, Copy, Debug)]
struct SongLuaMessageStateCache {
    initialized: bool,
    next_event: usize,
    processed_until: f32,
    base_state: SongLuaOverlayState,
    active_command_index: Option<usize>,
    active_start_second: f32,
    active_next_block: usize,
    active_block_state: SongLuaOverlayState,
    active_last_elapsed: f32,
}

impl Default for SongLuaMessageStateCache {
    fn default() -> Self {
        Self {
            initialized: false,
            next_event: 0,
            processed_until: f32::NEG_INFINITY,
            base_state: SongLuaOverlayState::default(),
            active_command_index: None,
            active_start_second: 0.0,
            active_next_block: 0,
            active_block_state: SongLuaOverlayState::default(),
            active_last_elapsed: f32::NEG_INFINITY,
        }
    }
}

impl SongLuaMessageStateCache {
    #[inline(always)]
    fn reset(&mut self, initial_state: SongLuaOverlayState) {
        self.initialized = true;
        self.next_event = 0;
        self.processed_until = f32::NEG_INFINITY;
        self.base_state = initial_state;
        self.active_command_index = None;
        self.active_start_second = 0.0;
        self.reset_active_blocks(initial_state);
    }

    #[inline(always)]
    fn reset_active_blocks(&mut self, state: SongLuaOverlayState) {
        self.active_next_block = 0;
        self.active_block_state = state;
        self.active_last_elapsed = f32::NEG_INFINITY;
    }
}

fn song_lua_overlay_child_list_index(parent_index: Option<usize>) -> usize {
    parent_index.map_or(0, |idx| idx + 1)
}

fn song_lua_sort_static_children(overlays: &[SongLuaOverlayActor], children: &mut [usize]) {
    children.sort_by_key(|&idx| (overlays[idx].initial_state.draw_order, idx));
}

fn song_lua_push_static_order(
    child_lists: &[Vec<usize>],
    parent_index: Option<usize>,
    out: &mut Vec<usize>,
) {
    let list_idx = song_lua_overlay_child_list_index(parent_index);
    let Some(children) = child_lists.get(list_idx) else {
        return;
    };
    for &index in children {
        out.push(index);
        song_lua_push_static_order(child_lists, Some(index), out);
    }
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
    let mut dynamic_local = vec![false; overlays.len()];
    let mut has_dynamic_z_order = overlays
        .iter()
        .any(|overlay| overlay.initial_state.draw_by_z_position);
    for (idx, overlay) in overlays.iter().enumerate() {
        dynamic_local[idx] = !overlay.message_commands.is_empty();
        dynamic_actor_draw_order[idx] = overlay.message_commands.iter().any(|command| {
            command
                .blocks
                .iter()
                .any(|block| block.delta.draw_order.is_some())
        });
        has_dynamic_z_order |= overlay.message_commands.iter().any(|command| {
            command
                .blocks
                .iter()
                .any(|block| block.delta.z.is_some() || block.delta.draw_by_z_position.is_some())
        });
    }
    for ease in overlay_eases {
        if ease.overlay_index < dynamic_actor_draw_order.len() {
            dynamic_local[ease.overlay_index] = true;
            if ease.from.delta.draw_order.is_some() || ease.to.delta.draw_order.is_some() {
                dynamic_actor_draw_order[ease.overlay_index] = true;
            }
            has_dynamic_z_order |= ease.from.delta.z.is_some()
                || ease.to.delta.z.is_some()
                || ease.from.delta.draw_by_z_position.is_some()
                || ease.to.delta.draw_by_z_position.is_some();
        }
    }

    let dynamic_local_indices = dynamic_local
        .iter()
        .enumerate()
        .filter_map(|(index, dynamic)| dynamic.then_some(index))
        .collect::<Box<[_]>>();
    let mut dynamic_composed = Vec::with_capacity(overlays.len());
    let mut dynamic_composed_indices = Vec::new();
    for (index, overlay) in overlays.iter().enumerate() {
        let dynamic = dynamic_local[index]
            || overlay
                .parent_index
                .and_then(|parent| dynamic_composed.get(parent))
                .copied()
                .unwrap_or(false);
        dynamic_composed.push(dynamic);
        if dynamic {
            dynamic_composed_indices.push(index);
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
    let static_root_order =
        if dynamic_actor_draw_order.iter().any(|dynamic| *dynamic) || has_dynamic_z_order {
            None
        } else {
            let mut order = Vec::with_capacity(overlays.len());
            song_lua_push_static_order(&child_lists, None, &mut order);
            Some(order.into_boxed_slice())
        };
    let sort_modes = vec![SONG_LUA_CHILD_ORDER_STATIC; child_lists.len()];
    SongLuaOverlayOrderCache {
        child_lists,
        dynamic_draw_order,
        dynamic_local_indices,
        dynamic_composed_indices: dynamic_composed_indices.into_boxed_slice(),
        static_root_order,
        sort_modes,
        last_draw_orders: vec![0; overlays.len()],
        last_z_keys: vec![0; overlays.len()],
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

// One dense notefield frame's reusable actor envelope. Reserving this at
// gameplay setup keeps later density spikes from geometrically growing and
// copying the large `Actor` enum on the render thread.
const NOTEFIELD_ACTOR_SCRATCH_CAPACITY: usize = 384;
const NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY: usize = 32;
const PLAYER_ACTOR_SCRATCH_CAPACITY: usize =
    NOTEFIELD_ACTOR_SCRATCH_CAPACITY + NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY;

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub const BENCH_NOTEFIELD_ACTOR_SCRATCH_CAPACITY: usize = NOTEFIELD_ACTOR_SCRATCH_CAPACITY;
#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub const BENCH_NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY: usize = NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY;

fn gameplay_actor_scratch(active_players: usize, capacity: usize) -> [Vec<Actor>; MAX_PLAYERS] {
    std::array::from_fn(|player| {
        if player < active_players {
            Vec::with_capacity(capacity)
        } else {
            Vec::new()
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GameplayStepStatsMode {
    Hidden,
    Side,
    Versus,
    Double,
}

#[cfg(any(test, feature = "bench-support"))]
impl GameplayStepStatsMode {
    const fn checksum(self) -> u64 {
        match self {
            Self::Hidden => 0,
            Self::Side => 1,
            Self::Versus => 2,
            Self::Double => 3,
        }
    }
}

const fn gameplay_step_stats_mode(
    play_style: profile_data::PlayStyle,
    num_cols: usize,
    p1_enabled: bool,
    p2_enabled: bool,
) -> GameplayStepStatsMode {
    let enabled = match play_style {
        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => p1_enabled,
        profile_data::PlayStyle::Versus => p1_enabled || p2_enabled,
    };
    if !enabled {
        return GameplayStepStatsMode::Hidden;
    }
    if num_cols <= 4 && !matches!(play_style, profile_data::PlayStyle::Versus) {
        GameplayStepStatsMode::Side
    } else {
        match play_style {
            profile_data::PlayStyle::Versus => GameplayStepStatsMode::Versus,
            profile_data::PlayStyle::Double => GameplayStepStatsMode::Double,
            profile_data::PlayStyle::Single => GameplayStepStatsMode::Hidden,
        }
    }
}

/// Song-lifetime activation index for one visual-layer list.
///
/// The gameplay thread owns both buffers. Screen entry stores every start once
/// and reserves the complete active-index capacity. Steady frames compare only
/// the adjacent start boundary; a single crossing shifts at most the bounded
/// active list, while a multi-layer seek rebuilds it once in O(n log n), always
/// preserving source draw order. There are no gameplay allocations, misses,
/// eviction, pruning, synchronization, or destruction before screen exit. The
/// layer-activity benchmark records steady and worst-sample cost; storage drops
/// with the screen.
#[derive(Default)]
struct SongLuaLayerActivity {
    starts: Box<[(f32, usize)]>,
    active: Vec<usize>,
    next_start: usize,
}

impl SongLuaLayerActivity {
    fn new(starts: impl IntoIterator<Item = f32>, now: f32) -> Self {
        let mut starts = starts
            .into_iter()
            .enumerate()
            .map(|(index, start)| (start, index))
            .collect::<Vec<_>>();
        starts.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        let mut activity = Self {
            active: Vec::with_capacity(starts.len()),
            starts: starts.into_boxed_slice(),
            next_start: 0,
        };
        activity.sync(now);
        activity
    }

    #[inline]
    fn sync(&mut self, now: f32) -> &[usize] {
        let old_next_start = self.next_start;
        while let Some(&(start, _)) = self.starts.get(self.next_start) {
            // Match the old `if now < start { continue; }` behavior, including
            // its treatment of NaN as active.
            if now < start {
                break;
            }
            self.next_start += 1;
        }
        while self.next_start > 0 && now < self.starts[self.next_start - 1].0 {
            self.next_start -= 1;
        }
        match self.next_start.abs_diff(old_next_start) {
            0 => {}
            1 if self.next_start > old_next_start => {
                let index = self.starts[old_next_start].1;
                let insert_at = self.active.binary_search(&index).unwrap_or_else(|at| at);
                self.active.insert(insert_at, index);
            }
            1 => {
                let index = self.starts[self.next_start].1;
                if let Ok(remove_at) = self.active.binary_search(&index) {
                    self.active.remove(remove_at);
                }
            }
            _ => {
                self.active.clear();
                self.active.extend(
                    self.starts[..self.next_start]
                        .iter()
                        .map(|(_, index)| *index),
                );
                self.active.sort_unstable();
            }
        }
        &self.active
    }
}

/// Game-thread, song-lifetime scratch detached while one actor frame is built.
///
/// The box is allocated and every variable-capacity field is prewarmed at
/// gameplay entry. A frame moves only the owning pointer, so there are no
/// misses, growth, pruning, synchronization, or destruction on the live path.
/// The owner is restored before returning and dropped at screen transition.
/// The frame-orchestration benchmark covers allocation counts and worst-sample
/// cost; steady-state work is bounded to one pointer take and restore.
#[derive(Default)]
struct GameplayFrameScratch {
    lobby_hud_cache: lobby_hud::LobbyHudCache,
    lobby_hud_status_scratch: String,
    /// Stable frames compare one compact key and clone the retained handle; a
    /// timing-segment change resolves the prewarmed bounded shared cache.
    bpm_text: GameplayBpmTextPlan,
    song_lua_overlay_order: SongLuaOverlayOrderCache,
    song_lua_background_visual_layer_orders: Vec<SongLuaOverlayOrderCache>,
    song_lua_foreground_visual_layer_orders: Vec<SongLuaOverlayOrderCache>,
    song_lua_background_layer_activity: SongLuaLayerActivity,
    song_lua_foreground_layer_activity: SongLuaLayerActivity,
    song_lua_proxy_request_index: SongLuaProxyRequestIndex,
    song_lua_background_overlay_topology_indices: Vec<SongLuaOverlayTopologyIndex>,
    song_lua_foreground_proxy_request_indices: Vec<SongLuaProxyRequestIndex>,
    song_lua_aft_capture_scratch: SongLuaAftCaptureScratch,
    song_lua_background_aft_capture_scratch: Vec<SongLuaAftCaptureScratch>,
    song_lua_foreground_aft_capture_scratch: Vec<SongLuaAftCaptureScratch>,
    song_lua_projected_mesh_scratch: Vec<SongLuaProjectedMeshScratch>,
    song_lua_background_projected_mesh_scratch: Vec<Vec<SongLuaProjectedMeshScratch>>,
    song_lua_foreground_projected_mesh_scratch: Vec<Vec<SongLuaProjectedMeshScratch>>,
    song_lua_message_state_cache: Vec<SongLuaMessageStateCache>,
    song_lua_background_layer_message_state_cache: Vec<Vec<SongLuaMessageStateCache>>,
    song_lua_foreground_layer_message_state_cache: Vec<Vec<SongLuaMessageStateCache>>,
    song_lua_player_message_state_cache: [SongLuaMessageStateCache; MAX_PLAYERS],
    song_lua_song_foreground_message_state_cache: SongLuaMessageStateCache,
    song_lua_background_song_foreground_message_state_cache: Vec<SongLuaMessageStateCache>,
    song_lua_foreground_song_foreground_message_state_cache: Vec<SongLuaMessageStateCache>,
    song_lua_local_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_overlay_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_background_layer_local_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_background_layer_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_foreground_layer_local_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_foreground_layer_state_scratch: Vec<Vec<SongLuaOverlayState>>,
    song_lua_capture_state_scratch: Vec<SongLuaOverlayState>,
    song_lua_order_scratch: Vec<usize>,
    song_lua_capture_order_scratch: Vec<usize>,
    song_lua_capture_visit_scratch: SongLuaCaptureVisitScratch,
    song_lua_proxy_actor_scratch: Option<SongLuaProxyActorScratch>,
    notefield_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    notefield_hud_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    player_actor_scratch: [Vec<Actor>; MAX_PLAYERS],
    presentation_skeleton: GameplayPresentationSkeleton,
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub struct GameplayFrameOrchestrationBenchmark {
    legacy_scratch: GameplayFrameScratch,
    boxed_scratch: Option<Box<GameplayFrameScratch>>,
    layer_count: usize,
    layer_scratch: [Vec<Vec<()>>; 6],
    play_style: profile_data::PlayStyle,
    num_cols: usize,
    p1_step_stats: bool,
    p2_step_stats: bool,
    step_stats_mode: GameplayStepStatsMode,
}

#[cfg(any(test, feature = "bench-support"))]
impl GameplayFrameOrchestrationBenchmark {
    pub fn new(layer_count: usize) -> Self {
        let scratch = || {
            let mut scratch = GameplayFrameScratch::default();
            scratch.lobby_hud_status_scratch = String::with_capacity(128);
            scratch.song_lua_order_scratch = Vec::with_capacity(256);
            scratch.song_lua_capture_order_scratch = Vec::with_capacity(256);
            scratch.song_lua_capture_state_scratch = Vec::with_capacity(256);
            scratch
        };
        let play_style = profile_data::PlayStyle::Versus;
        let num_cols = 8;
        let p1_step_stats = false;
        let p2_step_stats = true;
        Self {
            legacy_scratch: scratch(),
            boxed_scratch: Some(Box::new(scratch())),
            layer_count,
            layer_scratch: std::array::from_fn(|_| (0..layer_count).map(|_| Vec::new()).collect()),
            play_style,
            num_cols,
            p1_step_stats,
            p2_step_stats,
            step_stats_mode: gameplay_step_stats_mode(
                play_style,
                num_cols,
                p1_step_stats,
                p2_step_stats,
            ),
        }
    }

    fn scratch_checksum(scratch: &GameplayFrameScratch) -> u64 {
        (scratch.lobby_hud_status_scratch.capacity()
            + scratch.song_lua_order_scratch.capacity()
            + scratch.song_lua_capture_order_scratch.capacity()
            + scratch.song_lua_capture_state_scratch.capacity()) as u64
    }

    pub fn scratch_fields_legacy(&mut self) -> u64 {
        macro_rules! detach {
            ($($field:ident),+ $(,)?) => {
                $(let $field = std::mem::take(&mut self.legacy_scratch.$field);)+
            };
        }
        detach!(
            lobby_hud_cache,
            lobby_hud_status_scratch,
            song_lua_overlay_order,
            song_lua_background_visual_layer_orders,
            song_lua_foreground_visual_layer_orders,
            song_lua_proxy_request_index,
            song_lua_background_overlay_topology_indices,
            song_lua_foreground_proxy_request_indices,
            song_lua_aft_capture_scratch,
            song_lua_background_aft_capture_scratch,
            song_lua_foreground_aft_capture_scratch,
            song_lua_projected_mesh_scratch,
            song_lua_background_projected_mesh_scratch,
            song_lua_foreground_projected_mesh_scratch,
            song_lua_message_state_cache,
            song_lua_background_layer_message_state_cache,
            song_lua_foreground_layer_message_state_cache,
            song_lua_player_message_state_cache,
            song_lua_song_foreground_message_state_cache,
            song_lua_background_song_foreground_message_state_cache,
            song_lua_foreground_song_foreground_message_state_cache,
            song_lua_local_state_scratch,
            song_lua_overlay_state_scratch,
            song_lua_background_layer_local_state_scratch,
            song_lua_background_layer_state_scratch,
            song_lua_foreground_layer_local_state_scratch,
            song_lua_foreground_layer_state_scratch,
            song_lua_capture_state_scratch,
            song_lua_order_scratch,
            song_lua_capture_order_scratch,
            song_lua_capture_visit_scratch,
            song_lua_proxy_actor_scratch,
            notefield_actor_scratch,
            notefield_hud_actor_scratch,
            player_actor_scratch,
            presentation_skeleton,
        );
        std::hint::black_box((
            &lobby_hud_cache,
            &lobby_hud_status_scratch,
            &song_lua_overlay_order,
            &song_lua_background_visual_layer_orders,
            &song_lua_foreground_visual_layer_orders,
            &song_lua_proxy_request_index,
            &song_lua_background_overlay_topology_indices,
            &song_lua_foreground_proxy_request_indices,
            &song_lua_aft_capture_scratch,
            &song_lua_background_aft_capture_scratch,
            &song_lua_foreground_aft_capture_scratch,
            &song_lua_projected_mesh_scratch,
            &song_lua_background_projected_mesh_scratch,
            &song_lua_foreground_projected_mesh_scratch,
            &song_lua_message_state_cache,
            &song_lua_background_layer_message_state_cache,
            &song_lua_foreground_layer_message_state_cache,
            &song_lua_player_message_state_cache,
            &song_lua_song_foreground_message_state_cache,
            &song_lua_background_song_foreground_message_state_cache,
            &song_lua_foreground_song_foreground_message_state_cache,
            &song_lua_local_state_scratch,
            &song_lua_overlay_state_scratch,
            &song_lua_background_layer_local_state_scratch,
            &song_lua_background_layer_state_scratch,
            &song_lua_foreground_layer_local_state_scratch,
            &song_lua_foreground_layer_state_scratch,
            &song_lua_capture_state_scratch,
            &song_lua_order_scratch,
            &song_lua_capture_order_scratch,
            &song_lua_capture_visit_scratch,
            &song_lua_proxy_actor_scratch,
            &notefield_actor_scratch,
            &notefield_hud_actor_scratch,
            &player_actor_scratch,
            &presentation_skeleton,
        ));
        let checksum = (lobby_hud_status_scratch.capacity()
            + song_lua_order_scratch.capacity()
            + song_lua_capture_order_scratch.capacity()
            + song_lua_capture_state_scratch.capacity()) as u64;
        macro_rules! restore {
            ($($field:ident),+ $(,)?) => {
                $(self.legacy_scratch.$field = $field;)+
            };
        }
        restore!(
            lobby_hud_cache,
            lobby_hud_status_scratch,
            song_lua_overlay_order,
            song_lua_background_visual_layer_orders,
            song_lua_foreground_visual_layer_orders,
            song_lua_proxy_request_index,
            song_lua_background_overlay_topology_indices,
            song_lua_foreground_proxy_request_indices,
            song_lua_aft_capture_scratch,
            song_lua_background_aft_capture_scratch,
            song_lua_foreground_aft_capture_scratch,
            song_lua_projected_mesh_scratch,
            song_lua_background_projected_mesh_scratch,
            song_lua_foreground_projected_mesh_scratch,
            song_lua_message_state_cache,
            song_lua_background_layer_message_state_cache,
            song_lua_foreground_layer_message_state_cache,
            song_lua_player_message_state_cache,
            song_lua_song_foreground_message_state_cache,
            song_lua_background_song_foreground_message_state_cache,
            song_lua_foreground_song_foreground_message_state_cache,
            song_lua_local_state_scratch,
            song_lua_overlay_state_scratch,
            song_lua_background_layer_local_state_scratch,
            song_lua_background_layer_state_scratch,
            song_lua_foreground_layer_local_state_scratch,
            song_lua_foreground_layer_state_scratch,
            song_lua_capture_state_scratch,
            song_lua_order_scratch,
            song_lua_capture_order_scratch,
            song_lua_capture_visit_scratch,
            song_lua_proxy_actor_scratch,
            notefield_actor_scratch,
            notefield_hud_actor_scratch,
            player_actor_scratch,
            presentation_skeleton,
        );
        checksum
    }

    pub fn scratch_pointer(&mut self) -> u64 {
        let scratch = std::hint::black_box(
            self.boxed_scratch
                .take()
                .expect("benchmark scratch is restored after every frame"),
        );
        let checksum = Self::scratch_checksum(&scratch);
        self.boxed_scratch = Some(std::hint::black_box(scratch));
        checksum
    }

    pub fn layer_resize_legacy(&mut self) -> u64 {
        let layer_count = std::hint::black_box(self.layer_count);
        for layers in &mut self.layer_scratch {
            layers.resize_with(layer_count, Vec::new);
        }
        self.layer_checksum()
    }

    pub fn fixed_layer_lengths(&self) -> u64 {
        self.layer_checksum()
    }

    fn layer_checksum(&self) -> u64 {
        self.layer_scratch.iter().map(Vec::len).sum::<usize>() as u64
    }

    pub fn step_stats_legacy(&self) -> u64 {
        gameplay_step_stats_mode(
            std::hint::black_box(self.play_style),
            std::hint::black_box(self.num_cols),
            std::hint::black_box(self.p1_step_stats),
            std::hint::black_box(self.p2_step_stats),
        )
        .checksum()
    }

    pub const fn cached_step_stats(&self) -> u64 {
        self.step_stats_mode.checksum()
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaLayerActivityBenchmark {
    starts: Vec<f32>,
    activity: SongLuaLayerActivity,
    now: f32,
}

#[cfg(feature = "bench-support")]
impl SongLuaLayerActivityBenchmark {
    pub fn new(layer_count: usize, active_count: usize) -> Self {
        let layer_count = layer_count.max(1);
        let starts = (0..layer_count)
            .map(|index| ((index * 37) % layer_count) as f32)
            .collect::<Vec<_>>();
        let now = active_count.min(layer_count).saturating_sub(1) as f32;
        Self {
            activity: SongLuaLayerActivity::new(starts.iter().copied(), now),
            starts,
            now,
        }
    }

    pub fn full_scans(&self) -> u64 {
        let mut checksum = 0u64;
        for pass in 0..5u64 {
            for (index, start) in self.starts.iter().copied().enumerate() {
                if !(self.now < start) {
                    checksum = checksum.rotate_left(7) ^ index as u64 ^ pass;
                }
            }
        }
        checksum
    }

    pub fn active_cursor(&mut self) -> u64 {
        let active = self.activity.sync(self.now);
        let mut checksum = 0u64;
        for pass in 0..5u64 {
            for &index in active {
                checksum = checksum.rotate_left(7) ^ index as u64 ^ pass;
            }
        }
        checksum
    }
}

pub struct State {
    pub gameplay: GameplayCoreState,
    /// Game-thread, song-lifetime identity for the fixed two HUD slots. Built
    /// during gameplay setup, read immutably thereafter, and dropped with the
    /// screen; there are no misses, eviction, synchronization, allocations, or
    /// per-frame maintenance. The `gameplay_hud_snapshot` benchmark covers its
    /// old/new allocation and worst-sample costs.
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
    pub lobby_music_started: bool,
    pub lobby_ready_p1: bool,
    pub lobby_ready_p2: bool,
    pub lobby_disconnect_hold_p1: Option<Instant>,
    pub lobby_disconnect_hold_p2: Option<Instant>,
    step_stats_mode: GameplayStepStatsMode,
    pub(crate) song_banner_key: Option<Arc<str>>,
    pub(crate) pack_banner_key: Option<Arc<str>>,
    /// Immutable song background texture identity resolved at screen entry.
    /// SongBgWithMovieViz frames clone this handle instead of allocating a new
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
    /// Exact 0.0%-100.0% lookup table prepared at screen entry. Gameplay uses a
    /// direct quantized index; the fixed song-lifetime storage has no misses,
    /// growth, eviction, synchronization, or live-frame destruction.
    life_percent_text: GameplayLifeTextPlan,
    /// One song-static logical width for the Stage/Event label. The game thread
    /// warms it during the transition, reads it without synchronization during
    /// gameplay, and drops it with the screen. There is no growth or eviction;
    /// a skipped transition causes at most one bounded font measurement miss.
    intro_text_width: Cell<Option<f32>>,
    notefield_judgment_assets: [notefield::ResolvedJudgmentAssets; MAX_PLAYERS],
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
    /// screen; the benchmark records traversal cost and allocations.
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
    pub song_lua_sound_paths: Vec<PathBuf>,
    song_lua_sound_events: Vec<SongLuaSoundEvent>,
    next_song_lua_sound_event_ix: usize,
    active_song_lua_video_paths: Vec<PathBuf>,
    static_song_lua_video_path_count: usize,
    foreground_media_initialized: bool,
    next_foreground_change_ix: usize,
    current_foreground_path: Option<PathBuf>,
    current_foreground_key: Option<Arc<str>>,
    song_lua_foreground_owner_index: SongLuaForegroundOwnerIndex,
    smx_sensor_views: [Option<SmxSensorPadView>; 2],
    pub heart_rate_view: HeartRateView,
    heart_rate_generation: u64,
    pub(crate) heart_rate_text: heart_rate::HeartRateTextPlan,
    // Time banked toward the next shell-owned sensor refresh. Seeded to fire on
    // the first frame.
    smx_sensor_refresh_accum: f32,
    frame_scratch: Option<Box<GameplayFrameScratch>>,
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
    pub fn machine_font(&self) -> crate::config::MachineFont {
        self.runtime_view.policy.machine_font
    }

    /// Song-lifetime ITL warning state prepared by the shell on gameplay entry.
    /// Live score polling reuses this snapshot instead of re-reading profile or
    /// song-catalog runtime state every frame.
    #[inline(always)]
    pub const fn itl_cmod_warning_snapshot(&self) -> [bool; MAX_PLAYERS] {
        self.itl_cmod_warning
    }

    pub fn from_gameplay(
        gameplay: GameplayCoreState,
        noteskin_assets: GameplayNoteskinAssets,
    ) -> Self {
        let GameplayInitView {
            runtime,
            hud,
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
        let notefield_plans = std::array::from_fn(|player| {
            notefield::gameplay_notefield_plan(
                &step_stats_profiles[player],
                &notefield_judgment_assets[player],
                gameplay.player_blue_window_ms(player) / 1000.0,
            )
        });
        let scorebox_plans = std::array::from_fn(|side| {
            gs_scorebox::GameplayScoreboxPlan::new(
                scorebox_side_snapshot[side].as_ref(),
                &scorebox_profile_snapshot[side],
                runtime_view.policy.scorebox_pane_filter,
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
            .map(crate::assets::media_path_key);
        let pack_banner_key = pack_banner_path
            .as_deref()
            .map(crate::assets::media_path_key);
        let song_background_key = song
            .background_path
            .as_deref()
            .map(crate::assets::media_path_key);
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
        let mut song_lua_proxy_request_index =
            SongLuaProxyRequestIndex::new(&song_lua_visuals.overlays);
        song_lua_proxy_request_index
            .topology
            .include_camera_eases(&song_lua_visuals.overlays, &song_lua_visuals.overlay_eases);
        let song_lua_background_overlay_topology_indices: Vec<SongLuaOverlayTopologyIndex> =
            song_lua_visuals
                .background_visual_layers
                .iter()
                .map(|layer| {
                    let mut topology = SongLuaOverlayTopologyIndex::new(&layer.overlays);
                    topology.include_camera_eases(&layer.overlays, &layer.overlay_eases);
                    topology
                })
                .collect();
        let song_lua_foreground_proxy_request_indices: Vec<SongLuaProxyRequestIndex> =
            song_lua_visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| {
                    let mut index = SongLuaProxyRequestIndex::new(&layer.overlays);
                    index
                        .topology
                        .include_camera_eases(&layer.overlays, &layer.overlay_eases);
                    index
                })
                .collect();
        let song_lua_aft_capture_scratch = SongLuaAftCaptureScratch::new(
            &song_lua_visuals.overlays,
            &song_lua_proxy_request_index.topology,
        );
        let song_lua_background_aft_capture_scratch = song_lua_visuals
            .background_visual_layers
            .iter()
            .zip(song_lua_background_overlay_topology_indices.iter())
            .map(|(layer, topology)| SongLuaAftCaptureScratch::new(&layer.overlays, topology))
            .collect();
        let song_lua_foreground_aft_capture_scratch = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .zip(song_lua_foreground_proxy_request_indices.iter())
            .map(|(layer, index)| SongLuaAftCaptureScratch::new(&layer.overlays, &index.topology))
            .collect();
        let song_lua_proxy_count = song_lua_proxy_request_index.proxy_indices.len()
            + song_lua_foreground_proxy_request_indices
                .iter()
                .map(|index| index.proxy_indices.len())
                .sum::<usize>();
        let song_lua_projected_mesh_scratch =
            song_lua_projected_mesh_scratch_for(&song_lua_visuals.overlays);
        let song_lua_background_projected_mesh_scratch = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| song_lua_projected_mesh_scratch_for(&layer.overlays))
            .collect();
        let song_lua_foreground_projected_mesh_scratch = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| song_lua_projected_mesh_scratch_for(&layer.overlays))
            .collect();
        let song_lua_message_state_cache =
            vec![SongLuaMessageStateCache::default(); song_lua_visuals.overlays.len()];
        let song_lua_background_layer_message_state_cache = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| vec![SongLuaMessageStateCache::default(); layer.overlays.len()])
            .collect();
        let song_lua_foreground_layer_message_state_cache = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| vec![SongLuaMessageStateCache::default(); layer.overlays.len()])
            .collect();
        let song_lua_background_song_foreground_message_state_cache = vec![
            SongLuaMessageStateCache::default();
            song_lua_visuals.background_visual_layers.len()
        ];
        let song_lua_foreground_song_foreground_message_state_cache = vec![
            SongLuaMessageStateCache::default();
            song_lua_visuals.foreground_visual_layers.len()
        ];
        let song_lua_max_overlay_count = std::iter::once(song_lua_visuals.overlays.len())
            .chain(
                song_lua_visuals
                    .background_visual_layers
                    .iter()
                    .map(|layer| layer.overlays.len()),
            )
            .chain(
                song_lua_visuals
                    .foreground_visual_layers
                    .iter()
                    .map(|layer| layer.overlays.len()),
            )
            .max()
            .unwrap_or(0);
        let (song_lua_local_state_scratch, song_lua_overlay_state_scratch) =
            song_lua_overlay_initial_state_sets(
                &song_lua_visuals.overlays,
                song_lua_visuals.screen_width,
                song_lua_visuals.screen_height,
            );
        let (
            song_lua_background_layer_local_state_scratch,
            song_lua_background_layer_state_scratch,
        ) = song_lua_visuals
            .background_visual_layers
            .iter()
            .map(|layer| {
                song_lua_overlay_initial_state_sets(
                    &layer.overlays,
                    layer.screen_width,
                    layer.screen_height,
                )
            })
            .unzip();
        let (
            song_lua_foreground_layer_local_state_scratch,
            song_lua_foreground_layer_state_scratch,
        ) = song_lua_visuals
            .foreground_visual_layers
            .iter()
            .map(|layer| {
                song_lua_overlay_initial_state_sets(
                    &layer.overlays,
                    layer.screen_width,
                    layer.screen_height,
                )
            })
            .unzip();
        let song_lua_sound_events = song_lua_sound_events(song_lua_visuals);
        let active_song_lua_video_paths = song_lua_video_paths(song_lua_visuals);
        let static_song_lua_video_path_count = active_song_lua_video_paths.len();
        let song_lua_foreground_owner_index = SongLuaForegroundOwnerIndex::new(song_lua_visuals);
        let song_lua_now = gameplay.current_music_time_display();
        let song_lua_background_layer_activity = SongLuaLayerActivity::new(
            song_lua_visuals
                .background_visual_layers
                .iter()
                .map(|layer| layer.start_second),
            song_lua_now,
        );
        let song_lua_foreground_layer_activity = SongLuaLayerActivity::new(
            song_lua_visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| layer.start_second),
            song_lua_now,
        );
        let active_players = gameplay.num_players();
        let notefield_actor_scratch =
            gameplay_actor_scratch(active_players, NOTEFIELD_ACTOR_SCRATCH_CAPACITY);
        let notefield_hud_actor_scratch =
            gameplay_actor_scratch(active_players, NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY);
        let player_actor_scratch =
            gameplay_actor_scratch(active_players, PLAYER_ACTOR_SCRATCH_CAPACITY);
        let step_stats_mode = gameplay_step_stats_mode(
            hud_snapshot.play_style,
            gameplay.num_cols(),
            !step_stats_profiles[0].step_statistics.is_empty(),
            !step_stats_profiles[1].step_statistics.is_empty(),
        );
        let frame_scratch = GameplayFrameScratch {
            lobby_hud_cache: lobby_hud::LobbyHudCache::default(),
            lobby_hud_status_scratch: String::with_capacity(128),
            bpm_text,
            song_lua_overlay_order,
            song_lua_background_visual_layer_orders,
            song_lua_foreground_visual_layer_orders,
            song_lua_background_layer_activity,
            song_lua_foreground_layer_activity,
            song_lua_proxy_request_index,
            song_lua_background_overlay_topology_indices,
            song_lua_foreground_proxy_request_indices,
            song_lua_aft_capture_scratch,
            song_lua_background_aft_capture_scratch,
            song_lua_foreground_aft_capture_scratch,
            song_lua_projected_mesh_scratch,
            song_lua_background_projected_mesh_scratch,
            song_lua_foreground_projected_mesh_scratch,
            song_lua_message_state_cache,
            song_lua_background_layer_message_state_cache,
            song_lua_foreground_layer_message_state_cache,
            song_lua_player_message_state_cache: [SongLuaMessageStateCache::default(); MAX_PLAYERS],
            song_lua_song_foreground_message_state_cache: SongLuaMessageStateCache::default(),
            song_lua_background_song_foreground_message_state_cache,
            song_lua_foreground_song_foreground_message_state_cache,
            song_lua_local_state_scratch,
            song_lua_overlay_state_scratch,
            song_lua_background_layer_local_state_scratch,
            song_lua_background_layer_state_scratch,
            song_lua_foreground_layer_local_state_scratch,
            song_lua_foreground_layer_state_scratch,
            song_lua_capture_state_scratch: Vec::with_capacity(song_lua_max_overlay_count),
            song_lua_order_scratch: Vec::with_capacity(song_lua_max_overlay_count),
            song_lua_capture_order_scratch: Vec::with_capacity(song_lua_max_overlay_count),
            song_lua_capture_visit_scratch: SongLuaCaptureVisitScratch::with_capacity(
                song_lua_max_overlay_count,
            ),
            song_lua_proxy_actor_scratch: (song_lua_proxy_count != 0).then(|| {
                SongLuaProxyActorScratch::with_proxy_capacity(active_players, song_lua_proxy_count)
            }),
            notefield_actor_scratch,
            notefield_hud_actor_scratch,
            player_actor_scratch,
            presentation_skeleton: GameplayPresentationSkeleton::default(),
        };
        debug_assert_eq!(
            frame_scratch
                .song_lua_background_layer_local_state_scratch
                .len(),
            song_lua_visuals.background_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch.song_lua_background_layer_state_scratch.len(),
            song_lua_visuals.background_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch
                .song_lua_background_layer_message_state_cache
                .len(),
            song_lua_visuals.background_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch
                .song_lua_foreground_layer_local_state_scratch
                .len(),
            song_lua_visuals.foreground_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch.song_lua_foreground_layer_state_scratch.len(),
            song_lua_visuals.foreground_visual_layers.len()
        );
        debug_assert_eq!(
            frame_scratch
                .song_lua_foreground_layer_message_state_cache
                .len(),
            song_lua_visuals.foreground_visual_layers.len()
        );
        let mut state = Self {
            gameplay,
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
            intro_text_width: Cell::new(None),
            notefield_judgment_assets,
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
            song_lua_sound_paths,
            song_lua_sound_events,
            next_song_lua_sound_event_ix: 0,
            active_song_lua_video_paths,
            static_song_lua_video_path_count,
            foreground_media_initialized: false,
            next_foreground_change_ix: 0,
            current_foreground_path: None,
            current_foreground_key: None,
            song_lua_foreground_owner_index,
            smx_sensor_views: [None, None],
            heart_rate_view: HeartRateView::default(),
            heart_rate_generation: u64::MAX,
            heart_rate_text: heart_rate::HeartRateTextPlan::default(),
            smx_sensor_refresh_accum: SMX_SENSOR_REFRESH_INTERVAL,
            frame_scratch: Some(Box::new(frame_scratch)),
            actor_resources,
        };
        refresh_foreground_media(&mut state);
        state
    }

    pub fn reset_notefield_model_cache_stats(&self) {
        for cache in &self.notefield_model_cache {
            cache.borrow_mut().reset_stats();
        }
    }

    #[inline(always)]
    pub(crate) fn notefield_judgment_assets(
        &self,
        player_idx: usize,
    ) -> &notefield::ResolvedJudgmentAssets {
        &self.notefield_judgment_assets[player_idx]
    }

    #[inline(always)]
    pub(crate) fn notefield_plan(&self, player_idx: usize) -> &notefield::GameplayNotefieldPlan {
        &self.notefield_plans[player_idx]
    }

    #[inline(always)]
    pub fn actor_resources(&self) -> &ActorResourceArena {
        &self.actor_resources
    }

    #[inline(always)]
    pub fn active_background_start_sec(&self) -> Option<f32> {
        active_background_start_sec(
            &self.background_change_start_seconds,
            self.next_background_change_ix,
        )
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

    #[inline(always)]
    fn notefield_width(&self, player: usize) -> f32 {
        self.notefield_widths
            .get(player)
            .copied()
            .unwrap_or(DEFAULT_NOTEFIELD_WIDTH)
    }

    #[inline(always)]
    fn display_mods_text(&self, player: usize) -> &Arc<str> {
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
    match play_style {
        profile_data::PlayStyle::Single if num_cols <= 4 => (
            first_mask.contains(profile_data::StepStatisticsMask::SONG_BANNER),
            wide && first_mask.pack_info_enabled(),
        ),
        profile_data::PlayStyle::Double if num_cols > 4 && wide && !ultrawide => (
            first_mask.contains(profile_data::StepStatisticsMask::SONG_BANNER),
            first_mask.pack_info_enabled(),
        ),
        profile_data::PlayStyle::Versus if wide && !ultrawide => (
            (first_mask | second_mask).contains(profile_data::StepStatisticsMask::SONG_BANNER),
            false,
        ),
        _ => (false, false),
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

pub(crate) fn notefield_model_cache_from_assets(
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
    context: &deadsync_assets::song_lua::SongLuaCompileContext,
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
    context: &deadsync_assets::song_lua::SongLuaCompileContext,
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

fn song_lua_sound_paths(data: &GameplaySongLuaData) -> Vec<PathBuf> {
    deadsync_song_lua::compiled_song_lua_sound_paths(
        data.primary
            .iter()
            .map(|primary| &primary.compiled)
            .chain(data.background_layers.iter().map(|layer| &layer.compiled))
            .chain(data.foreground_layers.iter().map(|layer| &layer.compiled)),
    )
}

fn song_lua_video_paths<CapturedActor, StateDelta>(
    visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor, CapturedActor, StateDelta>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    deadsync_song_lua::push_song_lua_video_paths(&visuals.overlays, &mut seen, &mut paths);
    for layer in &visuals.background_visual_layers {
        deadsync_song_lua::push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    for layer in &visuals.foreground_visual_layers {
        deadsync_song_lua::push_song_lua_video_paths(&layer.overlays, &mut seen, &mut paths);
    }
    paths
}

fn push_song_lua_sound_events(
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    out: &mut Vec<SongLuaSoundEvent>,
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
                let second = event.event_second + block.start;
                if second.is_finite() {
                    out.push(SongLuaSoundEvent {
                        second,
                        path: sound_path.clone(),
                    });
                }
            }
        }
    }
}

fn song_lua_sound_events<CapturedActor, StateDelta>(
    visuals: &SongLuaRuntimeVisuals<SongLuaOverlayActor, CapturedActor, StateDelta>,
) -> Vec<SongLuaSoundEvent> {
    let mut events = Vec::new();
    push_song_lua_sound_events(&visuals.overlays, &visuals.overlay_events, &mut events);
    for layer in &visuals.background_visual_layers {
        push_song_lua_sound_events(&layer.overlays, &layer.overlay_events, &mut events);
    }
    for layer in &visuals.foreground_visual_layers {
        push_song_lua_sound_events(&layer.overlays, &layer.overlay_events, &mut events);
    }
    events.sort_by(|a, b| a.second.total_cmp(&b.second));
    events
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
        deadsync_profile_gameplay::build_song_lua_overlay_message_events_with_seconds(
            compiled,
            &message_seconds,
        );
    let overlay_eases = deadsync_profile_gameplay::build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    );
    let song_foreground_events =
        deadsync_profile_gameplay::build_song_lua_actor_message_events_for_commands(
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
    let mut note_hides: [deadsync_gameplay::SongLuaNoteHideWindows; MAX_PLAYERS] =
        std::array::from_fn(|_| deadsync_gameplay::SongLuaNoteHideWindows::default());
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
            deadsync_profile_gameplay::build_song_lua_overlay_message_events_with_seconds(
                compiled,
                &message_seconds,
            );
        let overlay_runtime_eases =
            deadsync_profile_gameplay::build_song_lua_overlay_ease_windows_with_events(
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
                deadsync_profile_gameplay::build_song_lua_actor_message_events_for_commands(
                    &compiled.messages,
                    &message_seconds,
                    &actor.message_commands,
                )
            },
        );
        song_foreground = compiled.song_foreground.clone();
        song_foreground_events =
            deadsync_profile_gameplay::build_song_lua_actor_message_events_for_commands(
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
    init_view: GameplayInitView,
) -> State {
    let GameplayInitView {
        runtime,
        hud,
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_intro_text_width(asset_manager: &AssetManager, text: &str) -> f32 {
    intro_text_width_for_font(asset_manager, "miso", text)
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
    let text_width = cached_intro_text_width(&state.intro_text_width, || {
        intro_text_width(asset_manager, state, text)
    });
    screen_center_x() + (notefield_width * 0.5 + text_width * INTRO_TEXT_GETWIDTH_PAD) * side_sign
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
    position: crate::config::GameplayBpmPosition,
    num_players: usize,
    play_style: profile_data::PlayStyle,
    player_side: profile_data::PlayerSide,
    playfield_center_x: f32,
    field_width: f32,
    nps_graph_at_top: bool,
) -> f32 {
    if position == crate::config::GameplayBpmPosition::NearField
        && num_players == 1
        && play_style == profile_data::PlayStyle::Single
    {
        let side = if player_side == profile_data::PlayerSide::P1 {
            1.0
        } else {
            -1.0
        };
        return playfield_center_x + side * (field_width * 0.5 + 20.0);
    }

    let note_field_is_centered = (playfield_center_x - screen_center_x()).abs() < 1.0;
    if num_players == 1 && note_field_is_centered && nps_graph_at_top {
        let side_shift = if player_side == profile_data::PlayerSide::P1 {
            0.3
        } else {
            -0.3
        };
        return screen_center_x() + screen_width() * side_shift;
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
        screen_center_x() - graph_w * 0.5
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct DifficultyMeterBench {
    profile: profile_data::Profile,
    cached: bool,
}

#[cfg(feature = "bench-support")]
impl Default for DifficultyMeterBench {
    fn default() -> Self {
        let profile = profile_data::Profile {
            note_field_offset_y: -50,
            ..profile_data::Profile::default()
        };
        let cached = saved_targets_hit_meter(&profile, 8, DIFFICULTY_METER_Y);
        Self { profile, cached }
    }
}

#[cfg(feature = "bench-support")]
impl DifficultyMeterBench {
    const SAMPLES: usize = 256;

    pub fn old_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let hit =
                saved_targets_hit_meter(std::hint::black_box(&self.profile), 8, DIFFICULTY_METER_Y);
            checksum.rotate_left(7) ^ hit as usize ^ sample
        })
    }

    pub fn new_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            checksum.rotate_left(7) ^ std::hint::black_box(self.cached) as usize ^ sample
        })
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

fn update_lobby_machine_state(state: &State) -> ThemeEffect {
    if !lobby_data::can_update_machine_state(&state.runtime_view.lobby.snapshot) {
        return ThemeEffect::None;
    }

    let (p1_ready, p2_ready) = local_lobby_ready_tuple(state);
    crate::effects::lobby(crate::SimplyLoveLobbyRequest::UpdateMachineStats {
        screen_name: "ScreenGameplay",
        p1_ready,
        p2_ready,
        p1_stats: gameplay_lobby_player_stats(state, profile_data::PlayerSide::P1),
        p2_stats: gameplay_lobby_player_stats(state, profile_data::PlayerSide::P2),
    })
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
        let remaining_text = remaining.to_string();
        text.push_str(&tr_fmt(
            "Lobby",
            "DisconnectHoldingFormat",
            &[
                ("remaining", remaining_text.as_str()),
                ("s", if remaining == 1 { "" } else { "s" }),
            ],
        ));
    } else {
        text.push_str(&tr("Lobby", "DisconnectBasicPrompt"));
    }
    true
}

pub fn scorebox_snapshot_for_side(
    state: &State,
    side: profile_data::PlayerSide,
) -> Option<&score_data::CachedPlayerLeaderboardData> {
    state.scorebox_side_snapshot[profile_data::player_side_index(side)].as_ref()
}

pub fn scorebox_profile_for_side(
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
        state.runtime_view.policy.srpg10_scorebox,
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

pub fn on_exit(state: &mut State) {
    state.smx_sensor_views = [None, None];
}

#[inline(always)]
pub fn sync_runtime_view(state: &mut State, view: GameplayRuntimeView) {
    if state.runtime_view.policy.scorebox_pane_filter != view.policy.scorebox_pane_filter {
        state.scorebox_plans = std::array::from_fn(|side| {
            gs_scorebox::GameplayScoreboxPlan::new(
                state.scorebox_side_snapshot[side].as_ref(),
                &state.scorebox_profile_snapshot[side],
                view.policy.scorebox_pane_filter,
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

/// Advance the active simfile foreground with a bidirectional cursor. File
/// existence and texture-key resolution happen only on entry or when a seek
/// crosses a foreground boundary, never on steady gameplay frames.
pub fn refresh_foreground_media(state: &mut State) -> bool {
    let beat = state.current_beat();
    let changes = &state.song().foreground_changes;
    let mut next_ix = state.next_foreground_change_ix.min(changes.len());
    if !state.foreground_media_initialized {
        next_ix = changes.partition_point(|change| change.start_beat <= beat);
    } else {
        while next_ix < changes.len() && changes[next_ix].start_beat <= beat {
            next_ix += 1;
        }
        while next_ix > 0 && changes[next_ix - 1].start_beat > beat {
            next_ix -= 1;
        }
        if next_ix == state.next_foreground_change_ix {
            return false;
        }
    }

    let next_path = next_ix
        .checked_sub(1)
        .and_then(|ix| changes.get(ix))
        .map(|change| &change.path)
        .filter(|path| path.is_file())
        .cloned();
    let changed = next_path != state.current_foreground_path;
    state.foreground_media_initialized = true;
    state.next_foreground_change_ix = next_ix;
    if !changed {
        return false;
    }

    state.current_foreground_key = next_path.as_deref().map(crate::assets::media_path_key);
    state.current_foreground_path = next_path;
    state
        .song_lua_foreground_owner_index
        .select(state.current_foreground_path.as_deref());
    state
        .active_song_lua_video_paths
        .truncate(state.static_song_lua_video_path_count);
    if let Some(path) = state.current_foreground_path.as_ref()
        && deadlib_assets::dynamic::is_dynamic_video_path(path)
        && !state
            .active_song_lua_video_paths
            .iter()
            .any(|existing| existing == path)
    {
        state.active_song_lua_video_paths.push(path.clone());
    }
    true
}

#[inline(always)]
pub fn active_song_lua_video_paths(state: &State) -> &[PathBuf] {
    &state.active_song_lua_video_paths
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
fn current_foreground_media(state: &State) -> Option<(&Path, Arc<str>)> {
    Some((
        state.current_foreground_path.as_deref()?,
        state.current_foreground_key.as_ref()?.clone(),
    ))
}

#[inline(always)]
pub fn sync_score_runtime_view(state: &mut State, view: GameplayScoreRuntimeView) {
    for (side, update) in view.scorebox_updates.into_iter().enumerate() {
        if update.is_some() {
            state.scorebox_side_snapshot[side] = update;
            state.scorebox_plans[side] = gs_scorebox::GameplayScoreboxPlan::new(
                state.scorebox_side_snapshot[side].as_ref(),
                &state.scorebox_profile_snapshot[side],
                state.runtime_view.policy.scorebox_pane_filter,
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

pub fn rival_score_type_for_side(
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
pub fn prepare_update(state: &mut State) -> (bool, ThemeEffect) {
    let mut effect = ThemeEffect::None;
    if !state.lobby_music_started {
        if lobby_disconnect_hold_elapsed(state)
            .is_some_and(|elapsed| elapsed >= state.runtime_view.lobby.disconnect_hold_seconds)
        {
            clear_lobby_disconnect_holds(state);
            effect = crate::effects::lobby(crate::SimplyLoveLobbyRequest::Disconnect);
            lobby_data::apply_local_lobby_disconnect(std::sync::Arc::make_mut(
                &mut state.runtime_view.lobby.snapshot,
            ));
            state.runtime_view.lobby.reconnect_status_text = None;
        }

        effect = crate::effects::sequence(effect, update_lobby_machine_state(state));

        if gameplay_lobby_wait_active(state) {
            return (false, effect);
        }

        clear_lobby_disconnect_holds(state);
        set_all_local_lobby_players_ready(state, true);
        state.start_stage_music();
        state.lobby_music_started = true;
    }
    effect = crate::effects::sequence(effect, update_lobby_machine_state(state));
    (true, effect)
}

/// Advances deterministic gameplay using the shell-prepared audio snapshot.
/// Runtime audio commands remain queued until the shell executes them.
pub fn update(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
) -> ThemeEffect {
    let action = update_core(state, delta_time, audio_snapshot, fallback_host_nanos);
    match action {
        GameplayAction::None => missed_target_effect(state).unwrap_or(ThemeEffect::None),
        action => map_gameplay_action(action),
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

fn song_lua_sound_time_crossed(previous: f32, now: f32, event_second: f32) -> bool {
    if !event_second.is_finite() {
        return false;
    }
    let starts_at_zero = previous <= 0.0 && event_second.abs() <= f32::EPSILON;
    (event_second > previous || starts_at_zero) && event_second <= now
}

/// Visits concrete Song-Lua sound events crossed by this gameplay update.
/// Sound identity and scheduling remain theme-owned; playback is supplied by
/// the shell so this screen never reaches into the audio stream runtime.
pub fn for_each_song_lua_sound_event(
    state: &mut State,
    previous: f32,
    now: f32,
    mut visit: impl FnMut(&Path),
) {
    visit_scheduled_song_lua_sound_events(
        &state.song_lua_sound_events,
        &mut state.next_song_lua_sound_event_ix,
        previous,
        now,
        &mut visit,
    );
}

fn visit_scheduled_song_lua_sound_events(
    events: &[SongLuaSoundEvent],
    next_event_ix: &mut usize,
    previous: f32,
    now: f32,
    visit: &mut impl FnMut(&Path),
) {
    if !previous.is_finite() || !now.is_finite() {
        return;
    }
    if now < previous {
        // Practice does not dispatch Song-Lua sounds, but gameplay clock
        // correction and future seekable modes still need deterministic replay
        // after moving backwards.
        *next_event_ix = events.partition_point(|event| event.second < now);
        return;
    }
    while let Some(event) = events.get(*next_event_ix) {
        if event.second > now {
            break;
        }
        if song_lua_sound_time_crossed(previous, now, event.second) {
            visit(&event.path);
        }
        *next_event_ix += 1;
    }
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
    if state.runtime_view.play_style == profile_data::PlayStyle::Double {
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

pub fn heart_rate_generation(state: &State) -> u64 {
    state.heart_rate_generation
}

pub fn set_heart_rate_view(state: &mut State, generation: u64, view: HeartRateView) {
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

pub fn smx_sensor_profile_enabled(state: &State) -> bool {
    state.runtime_view.policy.smx_profile_enabled
}

pub fn record_smx_sensor_read_ns(state: &State, elapsed_ns: u64) {
    smx_profile::record_read(state.runtime_view.policy.smx_profile_enabled, elapsed_ns);
}

pub fn report_smx_sensor_profile(state: &State) {
    smx_profile::maybe_report(state.runtime_view.policy.smx_profile_enabled);
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if gameplay_lobby_wait_active(state) {
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
        return ThemeEffect::None;
    }
    let action = handle_core_input(state, ev);
    map_gameplay_action(action)
}

thread_local! {
    static RATE_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(128));
    static BPM_TEXT_CACHE: RefCell<TextCache<(u64, bool)>> = RefCell::new(text_cache_with_capacity(512));
    static METER_TEXT_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(64));
    static AUTOSYNC_TEXT_CACHE: RefCell<TextCache<AutosyncTextKey>> =
        RefCell::new(text_cache_with_capacity(256));
}

#[cfg(feature = "bench-support")]
thread_local! {
    static BENCH_LIFE_PERCENT_TEXT_CACHE: RefCell<TextCache<u32>> =
        RefCell::new(text_cache_with_capacity(1024));
    static BENCH_RECENT_BPM_TEXT_CACHE: RefCell<RecentTextCache<(u64, bool)>> =
        RefCell::new(RecentTextCache::default());
    static BENCH_RECENT_LIFE_PERCENT_TEXT_CACHE: RefCell<RecentTextCache<u32>> =
        RefCell::new(RecentTextCache::default());
}

#[cfg(feature = "bench-support")]
struct RecentTextCache<K> {
    entries: [Option<(K, Arc<str>)>; 2],
    next: usize,
}

#[cfg(feature = "bench-support")]
impl<K> Default for RecentTextCache<K> {
    fn default() -> Self {
        Self {
            entries: [None, None],
            next: 0,
        }
    }
}

#[cfg(feature = "bench-support")]
impl<K: Copy + Eq> RecentTextCache<K> {
    #[inline(always)]
    fn get_or_insert_with(&mut self, key: K, build: impl FnOnce() -> Arc<str>) -> Arc<str> {
        if let Some((_, text)) = self
            .entries
            .iter()
            .flatten()
            .find(|(cached_key, _)| *cached_key == key)
        {
            return Arc::clone(text);
        }
        let text = build();
        self.entries[self.next] = Some((key, Arc::clone(&text)));
        self.next = (self.next + 1) % self.entries.len();
        text
    }
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

#[inline(always)]
fn shared_cached_bpm_text(bpm: f64, show_decimal: bool) -> Arc<str> {
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
    let rounded_thousandth = (bpm * 1_000.0).round() / 1_000.0;
    let rounded_thousandth = rounded_thousandth.max(0.0);
    let key = (rounded_thousandth.to_bits(), true);
    cached_text(&BPM_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        let mut text = format!("{rounded_thousandth:.3}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    })
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

struct GameplayBpmTextPlan {
    key: (u64, bool),
    text: Arc<str>,
}

impl GameplayBpmTextPlan {
    fn new(bpm: f64, show_decimal: bool) -> Self {
        Self {
            key: (bpm.to_bits(), show_decimal),
            text: shared_cached_bpm_text(bpm, show_decimal),
        }
    }

    #[inline(always)]
    fn resolve(&mut self, bpm: f64, show_decimal: bool) -> Arc<str> {
        let key = (bpm.to_bits(), show_decimal);
        if self.key != key {
            self.key = key;
            self.text = shared_cached_bpm_text(bpm, show_decimal);
        }
        Arc::clone(&self.text)
    }
}

impl Default for GameplayBpmTextPlan {
    fn default() -> Self {
        Self::new(0.0, false)
    }
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn cached_bpm_text_legacy(bpm: f64, show_decimal: bool) -> Arc<str> {
    let key = (bpm.to_bits(), show_decimal);
    BENCH_RECENT_BPM_TEXT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .get_or_insert_with(key, || shared_cached_bpm_text(bpm, show_decimal))
    })
}

const LIFE_PERCENT_TEXT_COUNT: usize = 1_001;

struct GameplayLifeTextPlan {
    values: Box<[Arc<str>; LIFE_PERCENT_TEXT_COUNT]>,
}

impl GameplayLifeTextPlan {
    fn new() -> Self {
        Self {
            values: Box::new(std::array::from_fn(|key| {
                Arc::from(format!("{:.1}%", key as f32 / 10.0))
            })),
        }
    }

    #[inline(always)]
    fn resolve(&self, life_percent: f32) -> Arc<str> {
        let key = quantize_tenths_u32(life_percent).min((LIFE_PERCENT_TEXT_COUNT - 1) as u32);
        Arc::clone(&self.values[key as usize])
    }
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn shared_cached_life_percent_text(life_percent: f32) -> Arc<str> {
    let key = quantize_tenths_u32(life_percent);
    cached_text(
        &BENCH_LIFE_PERCENT_TEXT_CACHE,
        key,
        TEXT_CACHE_LIMIT,
        || format!("{:.1}%", key as f32 / 10.0),
    )
}

#[cfg(feature = "bench-support")]
#[inline(always)]
fn cached_life_percent_text_legacy(life_percent: f32) -> Arc<str> {
    let key = quantize_tenths_u32(life_percent);
    BENCH_RECENT_LIFE_PERCENT_TEXT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .get_or_insert_with(key, || shared_cached_life_percent_text(life_percent))
    })
}

#[inline]
fn rainbow_life_color(elapsed: f32) -> [f32; 4] {
    let phase = elapsed * 2.0;
    let r = phase.sin() * 0.5 + 0.5;
    let g = (phase + std::f32::consts::TAU / 3.0).sin() * 0.5 + 0.5;
    let b = (phase + (2.0 * std::f32::consts::TAU) / 3.0).sin() * 0.5 + 0.5;
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
    life_percent: f32,
    lifemeter_type: profile_data::LifeMeterType,
    enabled: bool,
    standard_layout_visible: bool,
    is_hot: bool,
) -> Option<Arc<str>> {
    let visible = enabled
        && !is_hot
        && match lifemeter_type {
            profile_data::LifeMeterType::Standard => standard_layout_visible,
            profile_data::LifeMeterType::Vertical => true,
            profile_data::LifeMeterType::Surround => false,
        };
    visible.then(|| text_plan.resolve(life_percent))
}

#[cfg(feature = "bench-support")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameplayHudTextBenchmarkSnapshot {
    pub bpm: Arc<str>,
    pub life: Arc<str>,
    pub overlay: Arc<str>,
    pub overlay_line_count: usize,
}

/// Exercises the current steady autoplay HUD text path: shared-cache hits for
/// dynamic numeric text plus reconstruction of the unchanged status overlay.
#[cfg(feature = "bench-support")]
pub fn benchmark_gameplay_hud_text_legacy(
    bpm: f64,
    show_bpm_decimal: bool,
    life_percent: f32,
    autoplay_line: &str,
) -> GameplayHudTextBenchmarkSnapshot {
    let bpm = shared_cached_bpm_text(bpm, show_bpm_decimal);
    let life = shared_cached_life_percent_text(life_percent);
    let mut overlay = String::with_capacity(autoplay_line.len());
    overlay.push_str(autoplay_line);
    GameplayHudTextBenchmarkSnapshot {
        bpm,
        life,
        overlay: Arc::from(overlay),
        overlay_line_count: 1,
    }
}

#[cfg(feature = "bench-support")]
pub struct GameplayHudTextBenchmarkCache {
    overlay: SyncOverlayTextCache,
    autoplay_line: Arc<str>,
    bpm: GameplayBpmTextPlan,
    life: GameplayLifeTextPlan,
}

#[cfg(feature = "bench-support")]
impl GameplayHudTextBenchmarkCache {
    pub fn new(autoplay_line: &str) -> Self {
        Self {
            overlay: SyncOverlayTextCache::default(),
            autoplay_line: Arc::from(autoplay_line),
            bpm: GameplayBpmTextPlan::new(0.0, false),
            life: GameplayLifeTextPlan::new(),
        }
    }

    pub fn snapshot(
        &mut self,
        bpm: f64,
        show_bpm_decimal: bool,
        life_percent: f32,
    ) -> GameplayHudTextBenchmarkSnapshot {
        let (overlay, overlay_line_count) = self
            .overlay
            .resolve(SyncOverlayTextInput {
                autoplay_enabled: true,
                replay_status: Some(&self.autoplay_line),
                timing_tick_status: None,
                autosync_status: None,
                initial_global_offset: 0.0,
                global_offset: 0.0,
                initial_song_offset: 0.0,
                song_offset: 0.0,
            })
            .expect("autoplay produces one overlay line");
        GameplayHudTextBenchmarkSnapshot {
            bpm: self.bpm.resolve(bpm, show_bpm_decimal),
            life: self.life.resolve(life_percent),
            overlay,
            overlay_line_count,
        }
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayLifemeterOptionBench {
    hidden_percent: profile_data::Profile,
    surround: profile_data::Profile,
    life_text: GameplayLifeTextPlan,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayLifemeterOptionBench {
    fn default() -> Self {
        Self {
            hidden_percent: profile_data::Profile {
                lifemeter_type: profile_data::LifeMeterType::Standard,
                show_life_percent: false,
                ..profile_data::Profile::default()
            },
            surround: profile_data::Profile {
                lifemeter_type: profile_data::LifeMeterType::Surround,
                rainbow_max: true,
                responsive_colors: true,
                ..profile_data::Profile::default()
            },
            life_text: GameplayLifeTextPlan::new(),
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplayLifemeterOptionBench {
    const SAMPLES: usize = 256;

    pub fn old_hidden_percent_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = cached_life_percent_text_legacy(std::hint::black_box(87.3));
            std::hint::black_box(text);
            checksum.rotate_left(7) ^ sample
        })
    }

    pub fn new_hidden_percent_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = visible_life_percent_text(
                &self.life_text,
                std::hint::black_box(87.3),
                self.hidden_percent.lifemeter_type,
                self.hidden_percent.show_life_percent,
                true,
                false,
            );
            std::hint::black_box(text);
            checksum.rotate_left(7) ^ sample
        })
    }

    pub fn old_surround_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let elapsed = (frame.wrapping_mul(Self::SAMPLES).wrapping_add(sample)) as f32 / 120.0;
            let unused = life_fill_color(&self.surround, 1.0, false, elapsed, || [1.0; 4]);
            std::hint::black_box(unused);
            checksum.rotate_left(7)
                ^ rgba_checksum(surround_life_color(&self.surround, 1.0, elapsed))
                ^ sample
        })
    }

    pub fn new_surround_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let elapsed = (frame.wrapping_mul(Self::SAMPLES).wrapping_add(sample)) as f32 / 120.0;
            checksum.rotate_left(7)
                ^ rgba_checksum(surround_life_color(&self.surround, 1.0, elapsed))
                ^ sample
        })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayBpmCacheBench {
    plan: GameplayBpmTextPlan,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayBpmCacheBench {
    fn default() -> Self {
        Self {
            plan: GameplayBpmTextPlan::new(175.25, true),
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplayBpmCacheBench {
    const SAMPLES: usize = 256;

    pub fn legacy_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = cached_bpm_text_legacy(std::hint::black_box(175.25), true);
            checksum.rotate_left(7) ^ std::hint::black_box(text.len()) ^ sample
        })
    }

    pub fn planned_frame(&mut self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = self.plan.resolve(std::hint::black_box(175.25), true);
            checksum.rotate_left(7) ^ std::hint::black_box(text.len()) ^ sample
        })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayLifeTextBench {
    plan: GameplayLifeTextPlan,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayLifeTextBench {
    fn default() -> Self {
        Self {
            plan: GameplayLifeTextPlan::new(),
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplayLifeTextBench {
    const SAMPLES: usize = 256;

    pub fn legacy_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = cached_life_percent_text_legacy(std::hint::black_box(87.3));
            checksum.rotate_left(7) ^ std::hint::black_box(text.len()) ^ sample
        })
    }

    pub fn planned_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = self.plan.resolve(std::hint::black_box(87.3));
            checksum.rotate_left(7) ^ std::hint::black_box(text.len()) ^ sample
        })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayScorePollingBench {
    profiles: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS],
    snapshots: [Option<score_data::CachedPlayerLeaderboardData>; MAX_PLAYERS],
    rival_score_types: [Option<profile_data::MiniIndicatorScoreType>; MAX_PLAYERS],
    pending: bool,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayScorePollingBench {
    fn default() -> Self {
        let mut profiles: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS] =
            std::array::from_fn(|_| Default::default());
        profiles[0].display_scorebox = true;
        profiles[0].gs_active = true;
        let snapshots = [
            Some(score_data::CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: None,
            }),
            None,
        ];
        let rival_score_types = [None; MAX_PLAYERS];
        let pending = scorebox_refresh_pending_from(&profiles, &snapshots, &rival_score_types);
        Self {
            profiles,
            snapshots,
            rival_score_types,
            pending,
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplayScorePollingBench {
    const SAMPLES: usize = 256;

    pub fn scan_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let pending = scorebox_refresh_pending_from(
                std::hint::black_box(&self.profiles),
                std::hint::black_box(&self.snapshots),
                std::hint::black_box(&self.rival_score_types),
            );
            checksum.rotate_left(7) ^ usize::from(pending) ^ sample
        })
    }

    pub fn flagged_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let pending = std::hint::black_box(self.pending);
            checksum.rotate_left(7) ^ usize::from(pending) ^ sample
        })
    }
}

#[cfg(feature = "bench-support")]
#[inline]
fn rgba_checksum(rgba: [f32; 4]) -> usize {
    rgba.into_iter().fold(0usize, |checksum, value| {
        checksum.rotate_left(7) ^ value.to_bits() as usize
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayRateTextBench {
    rate: f32,
    cached: Arc<str>,
}

#[cfg(feature = "bench-support")]
impl Default for GameplayRateTextBench {
    fn default() -> Self {
        let rate = 1.25;
        Self {
            rate,
            cached: cached_rate_text(rate),
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplayRateTextBench {
    const SAMPLES: usize = 256;

    pub fn old_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = cached_rate_text(std::hint::black_box(self.rate));
            checksum.rotate_left(7) ^ text.len() ^ sample
        })
    }

    pub fn new_frame(&self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let text = Arc::clone(std::hint::black_box(&self.cached));
            checksum.rotate_left(7) ^ text.len() ^ sample
        })
    }
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplaySyncOverlayIdleBenchmark {
    legacy_cache: SyncOverlayTextCache,
    gated_cache: SyncOverlayTextCache,
    input: SyncOverlayTextInput<'static>,
}

#[cfg(feature = "bench-support")]
impl Default for GameplaySyncOverlayIdleBenchmark {
    fn default() -> Self {
        Self {
            legacy_cache: SyncOverlayTextCache::default(),
            gated_cache: SyncOverlayTextCache::default(),
            input: SyncOverlayTextInput {
                autoplay_enabled: false,
                replay_status: None,
                timing_tick_status: None,
                autosync_status: None,
                initial_global_offset: -0.012,
                global_offset: -0.012,
                initial_song_offset: 0.003,
                song_offset: 0.003,
            },
        }
    }
}

#[cfg(feature = "bench-support")]
impl GameplaySyncOverlayIdleBenchmark {
    const SAMPLES: usize = 256;

    pub fn legacy_frame(&mut self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let value = std::hint::black_box(&mut self.legacy_cache).resolve(self.input);
            checksum.rotate_left(5) ^ value.map_or(0, |(text, lines)| text.len() + lines) ^ sample
        })
    }

    pub fn gated_frame(&mut self, frame: usize) -> usize {
        (0..Self::SAMPLES).fold(frame, |checksum, sample| {
            let input = std::hint::black_box(self.input);
            let value = if input.is_idle() {
                None
            } else {
                self.gated_cache.resolve(input)
            };
            checksum.rotate_left(5) ^ value.map_or(0, |(text, lines)| text.len() + lines) ^ sample
        })
    }
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
    for text in state.life_percent_text.values.iter() {
        cache.prewarm_text(fonts, "miso", text.as_ref(), None);
    }
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
        let Some(gameplay_chart) = state.gameplay_chart(player) else {
            continue;
        };
        for &(_, bpm) in &gameplay_chart.timing_segments.bpms {
            let text = shared_cached_bpm_text(
                f64::from(bpm.max(0.0)) * f64::from(state.music_rate()),
                policy.show_bpm_decimal,
            );
            cache.prewarm_text(fonts, "miso", text.as_ref(), None);
        }
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

// --- TRANSITIONS ---
pub fn in_transition(
    state: Option<&State>,
    asset_manager: &AssetManager,
    is_restart: bool,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) -> (Vec<Actor>, f32) {
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
            font(machine_font_key(visual_policy.machine_font, FontRole::Header)): settext(text):
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

#[inline(always)]
fn white_texture_key() -> Arc<str> {
    static WHITE_TEXTURE_KEY: OnceLock<Arc<str>> = OnceLock::new();
    Arc::clone(WHITE_TEXTURE_KEY.get_or_init(|| Arc::from("__white")))
}

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

/// Focused old/new harness for the immutable SongBgWithMovieViz texture key.
#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub struct GameplayBackgroundKeyBenchmark {
    path: PathBuf,
    cached: Arc<str>,
}

#[cfg(any(test, feature = "bench-support"))]
impl GameplayBackgroundKeyBenchmark {
    pub fn new() -> Self {
        let path = PathBuf::from("Songs/Benchmark Pack/Benchmark Song/background image.png");
        let cached = crate::assets::media_path_key(&path);
        Self { path, cached }
    }

    pub fn legacy_frame(&self) -> usize {
        let key = crate::assets::media_path_key(std::hint::black_box(&self.path));
        std::hint::black_box(key.len())
    }

    pub fn prewarmed_frame(&self) -> usize {
        let key = Arc::clone(std::hint::black_box(&self.cached));
        std::hint::black_box(key.len())
    }

    pub fn behavior_matches(&self) -> bool {
        crate::assets::media_path_key(&self.path).as_ref() == self.cached.as_ref()
    }
}

#[cfg(test)]
mod gameplay_background_key_tests {
    use super::GameplayBackgroundKeyBenchmark;

    #[test]
    fn prewarmed_movie_visualizer_background_key_preserves_identity() {
        assert!(GameplayBackgroundKeyBenchmark::new().behavior_matches());
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

#[cfg(any(test, feature = "bench-support"))]
fn background_transition_frame_legacy(
    transition_name: &str,
    start_time: f32,
    now: f32,
) -> Option<(BackgroundTransition, f32)> {
    let transition = BackgroundTransition::from_name(transition_name)?;
    let progress = ((now - start_time) / transition.duration()).clamp(0.0, 1.0);
    (progress < 1.0).then_some((transition, progress))
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
            offset[0] -= screen_w * progress;
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideRight => {
            offset[0] += screen_w * progress;
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideUp => {
            offset[1] -= screen_h * progress;
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::SlideDown => {
            offset[1] += screen_h * progress;
            tint[3] *= 1.0 - progress;
        }
        BackgroundTransition::FadeUp => {
            *cropbottom = -0.3 + 1.6 * progress;
            *fadebottom = 0.3;
        }
        BackgroundTransition::FadeDown => {
            *croptop = -0.3 + 1.6 * progress;
            *fadetop = 0.3;
        }
        BackgroundTransition::FadeRight => {
            *cropleft = -0.3 + 1.6 * progress;
            *fadeleft = 0.3;
        }
        BackgroundTransition::FadeLeft => {
            *cropright = -0.3 + 1.6 * progress;
            *faderight = 0.3;
        }
        BackgroundTransition::FadeCenterHorizontal => {
            *croptop = -0.3 + 0.8 * progress;
            *cropbottom = -0.3 + 0.8 * progress;
            *fadetop = 0.3;
            *fadebottom = 0.3;
        }
        BackgroundTransition::FadeCenterVertical => {
            *cropleft = -0.3 + 0.8 * progress;
            *cropright = -0.3 + 0.8 * progress;
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

#[cfg(any(test, feature = "bench-support"))]
fn song_layer2_animation_legacy(events: &[SongLayer2Event], now: f32) -> Option<[f32; 4]> {
    let (event, elapsed) = events.iter().rev().find_map(|event| {
        let elapsed = now - event.start_second;
        (0.0..=LAYER2_FLASH_SECONDS)
            .contains(&elapsed)
            .then_some((event, elapsed))
    })?;
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

const SONG_LUA_FG_OWNER_ROOT: u8 = 0;
const SONG_LUA_FG_OWNER_BACKGROUND: u8 = 1;
const SONG_LUA_FG_OWNER_FOREGROUND: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SongLuaForegroundOwnerRef {
    source: u8,
    layer_index: usize,
    overlay_index: usize,
}

struct SongLuaForegroundOwnerPath {
    path: PathBuf,
    owner_start: usize,
    owner_end: usize,
}

/// Path-specific Song Lua ownership index for the active simfile foreground.
///
/// The gameplay thread owns immutable, exactly-sized path and owner tables built
/// at screen entry. Foreground events select a path with binary search; gameplay
/// frames read only its contiguous owner range. Misses saturate to an empty
/// range, there is no growth, eviction, synchronization, or live-frame pruning,
/// and storage is freed at screen exit. The foreground-owner benchmark covers
/// full scans versus indexed reads, allocation counts, and worst-sample cost.
#[derive(Default)]
struct SongLuaForegroundOwnerIndex {
    paths: Box<[SongLuaForegroundOwnerPath]>,
    owners: Box<[SongLuaForegroundOwnerRef]>,
    active_start: usize,
    active_end: usize,
}

impl SongLuaForegroundOwnerIndex {
    fn new(
        visuals: &SongLuaRuntimeVisuals<
            SongLuaOverlayActor,
            SongLuaCapturedActor,
            SongLuaRuntimeOverlayStateDelta,
        >,
    ) -> Self {
        let capacity = visuals.overlays.len()
            + visuals
                .background_visual_layers
                .iter()
                .map(|layer| layer.overlays.len())
                .sum::<usize>()
            + visuals
                .foreground_visual_layers
                .iter()
                .map(|layer| layer.overlays.len())
                .sum::<usize>();
        let mut entries = Vec::with_capacity(capacity);
        Self::push_entries(&mut entries, SONG_LUA_FG_OWNER_ROOT, 0, &visuals.overlays);
        for (layer_index, layer) in visuals.background_visual_layers.iter().enumerate() {
            Self::push_entries(
                &mut entries,
                SONG_LUA_FG_OWNER_BACKGROUND,
                layer_index,
                &layer.overlays,
            );
        }
        for (layer_index, layer) in visuals.foreground_visual_layers.iter().enumerate() {
            Self::push_entries(
                &mut entries,
                SONG_LUA_FG_OWNER_FOREGROUND,
                layer_index,
                &layer.overlays,
            );
        }
        entries.sort_by(|left, right| {
            left.0
                .cmp(right.0)
                .then_with(|| left.1.source.cmp(&right.1.source))
                .then_with(|| left.1.layer_index.cmp(&right.1.layer_index))
                .then_with(|| left.1.overlay_index.cmp(&right.1.overlay_index))
        });

        let mut paths = Vec::new();
        let mut owners = Vec::with_capacity(entries.len());
        let mut cursor = 0;
        while cursor < entries.len() {
            let owner_start = owners.len();
            let path = entries[cursor].0.to_path_buf();
            while cursor < entries.len() && entries[cursor].0 == path.as_path() {
                owners.push(entries[cursor].1);
                cursor += 1;
            }
            paths.push(SongLuaForegroundOwnerPath {
                path,
                owner_start,
                owner_end: owners.len(),
            });
        }
        Self {
            paths: paths.into_boxed_slice(),
            owners: owners.into_boxed_slice(),
            active_start: 0,
            active_end: 0,
        }
    }

    fn push_entries<'a>(
        out: &mut Vec<(&'a Path, SongLuaForegroundOwnerRef)>,
        source: u8,
        layer_index: usize,
        overlays: &'a [SongLuaOverlayActor],
    ) {
        out.extend(
            overlays
                .iter()
                .enumerate()
                .filter_map(|(overlay_index, overlay)| match &overlay.kind {
                    SongLuaOverlayKind::Sprite { texture_path, .. } => Some((
                        texture_path.as_path(),
                        SongLuaForegroundOwnerRef {
                            source,
                            layer_index,
                            overlay_index,
                        },
                    )),
                    _ => None,
                }),
        );
    }

    fn select(&mut self, path: Option<&Path>) {
        let Some(path) = path else {
            self.active_start = 0;
            self.active_end = 0;
            return;
        };
        let Ok(index) = self
            .paths
            .binary_search_by(|candidate| candidate.path.as_path().cmp(path))
        else {
            self.active_start = 0;
            self.active_end = 0;
            return;
        };
        self.active_start = self.paths[index].owner_start;
        self.active_end = self.paths[index].owner_end;
    }

    fn owns(
        &self,
        now: f32,
        visuals: &SongLuaRuntimeVisuals<
            SongLuaOverlayActor,
            SongLuaCapturedActor,
            SongLuaRuntimeOverlayStateDelta,
        >,
        overlay_states: &[SongLuaOverlayState],
        background_layer_states: &[Vec<SongLuaOverlayState>],
        foreground_layer_states: &[Vec<SongLuaOverlayState>],
    ) -> bool {
        self.owners[self.active_start..self.active_end]
            .iter()
            .any(|owner| {
                let state = match owner.source {
                    SONG_LUA_FG_OWNER_ROOT => overlay_states.get(owner.overlay_index),
                    SONG_LUA_FG_OWNER_BACKGROUND => visuals
                        .background_visual_layers
                        .get(owner.layer_index)
                        .filter(|layer| !(now < layer.start_second))
                        .and_then(|_| background_layer_states.get(owner.layer_index))
                        .and_then(|states| states.get(owner.overlay_index)),
                    SONG_LUA_FG_OWNER_FOREGROUND => visuals
                        .foreground_visual_layers
                        .get(owner.layer_index)
                        .filter(|layer| !(now < layer.start_second))
                        .and_then(|_| foreground_layer_states.get(owner.layer_index))
                        .and_then(|states| states.get(owner.overlay_index)),
                    _ => None,
                };
                state.is_some_and(|state| state.visible && state.diffuse[3] > f32::EPSILON)
            })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaForegroundOwnerBenchmark {
    visuals: SongLuaRuntimeVisuals<
        SongLuaOverlayActor,
        SongLuaCapturedActor,
        SongLuaRuntimeOverlayStateDelta,
    >,
    states: Vec<SongLuaOverlayState>,
    path: PathBuf,
    index: SongLuaForegroundOwnerIndex,
}

#[cfg(feature = "bench-support")]
impl SongLuaForegroundOwnerBenchmark {
    pub fn new(overlay_count: usize) -> Self {
        let overlay_count = overlay_count.max(1);
        let path = PathBuf::from("foreground-target.avi");
        let overlays = (0..overlay_count)
            .map(|index| {
                let texture_path = if index + 1 == overlay_count {
                    path.clone()
                } else {
                    PathBuf::from("other-texture.png")
                };
                SongLuaOverlayActor {
                    kind: SongLuaOverlayKind::Sprite {
                        texture_key: Arc::from(texture_path.to_string_lossy().into_owned()),
                        texture_path,
                    },
                    name: None,
                    parent_index: None,
                    initial_state: SongLuaOverlayState::default(),
                    message_commands: Vec::new(),
                }
            })
            .collect::<Vec<_>>();
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect();
        let visuals = SongLuaRuntimeVisuals {
            overlays,
            overlay_eases: Vec::new(),
            overlay_ease_ranges: Vec::new(),
            overlay_events: Vec::new(),
            background_visual_layers: Vec::new(),
            foreground_visual_layers: Vec::new(),
            player_actors: std::array::from_fn(|_| SongLuaCapturedActor::default()),
            player_events: std::array::from_fn(|_| Vec::new()),
            song_foreground: SongLuaCapturedActor::default(),
            song_foreground_events: Vec::new(),
            hidden_players: [false; MAX_PLAYERS],
            note_hides: std::array::from_fn(|_| Default::default()),
            column_offsets: std::array::from_fn(|_| Vec::new()),
            screen_width: 640.0,
            screen_height: 480.0,
        };
        let mut index = SongLuaForegroundOwnerIndex::new(&visuals);
        index.select(Some(path.as_path()));
        Self {
            visuals,
            states,
            path,
            index,
        }
    }

    pub fn full_scan(&self) -> u64 {
        u64::from(song_lua_has_visible_tex(
            &self.visuals.overlays,
            &self.states,
            &self.path,
        ))
    }

    pub fn indexed(&self) -> u64 {
        u64::from(self.index.owns(1.0, &self.visuals, &self.states, &[], &[]))
    }
}

#[cfg(any(test, feature = "bench-support"))]
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

fn active_foreground_media(state: &State) -> Option<(&Path, Arc<str>)> {
    current_foreground_media(state)
}

fn build_foreground_media(
    state: &State,
    overlay_states: &[SongLuaOverlayState],
    background_layer_states: &[Vec<SongLuaOverlayState>],
    foreground_layer_states: &[Vec<SongLuaOverlayState>],
) -> Option<Actor> {
    let (_, texture_key) = active_foreground_media(state)?;
    if state.song_lua_foreground_owner_index.owns(
        state.current_music_time_display(),
        state.song_lua_visuals(),
        overlay_states,
        background_layer_states,
        foreground_layer_states,
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
    deadsync_song_lua::song_lua_valid_sprite_state_index(state.sprite_state_index)
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
        return Some(deadsync_song_lua::sprite_animation_state_from(
            start,
            total_elapsed,
            state.sprite_playback_rate,
            state.sprite_state_delay,
            total,
            state.sprite_loop,
        ));
    }
    (state.sprite_animate || song_lua_valid_sprite_state_index(state).is_some()).then_some(start)
}

fn song_lua_overlay_sprite_size(state: SongLuaOverlayState, texture_key: &str) -> Option<[f32; 2]> {
    if let Some(size) = state.size {
        return Some(size);
    }
    let tex = crate::assets::texture_dims(texture_key)?;
    let (width, height) = deadsync_song_lua::sprite_image_frame_size(
        Some((tex.w as f32, tex.h as f32)),
        state.sprite_animate,
        state.sprite_state_index,
        Some(sprite_sheet_dims(texture_key)),
    )?;
    Some([width, height])
}

fn song_lua_overlay_uv_rect(
    state: SongLuaOverlayState,
    texture_key: Option<&str>,
    total_elapsed: f32,
) -> Option<[f32; 4]> {
    let state_index = texture_key
        .and_then(|texture_key| song_lua_sprite_sheet_index(state, texture_key, total_elapsed));
    let sheet_dims = texture_key.map(sprite_sheet_dims);
    deadsync_song_lua::sprite_texture_rect_with_offset(
        state.custom_texture_rect,
        state_index,
        sheet_dims,
        state.texcoord_offset,
    )
}

#[inline(always)]
fn song_lua_overlay_axis_scale(state: SongLuaOverlayState) -> [f32; 2] {
    deadsync_song_lua::overlay_state_axis_scale(state)
}

#[inline(always)]
fn song_lua_overlay_z_scale(state: SongLuaOverlayState) -> f32 {
    deadsync_song_lua::overlay_state_z_scale(state)
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

#[cfg(any(test, feature = "bench-support"))]
fn song_lua_overlay_local_states_all_into(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    out: &mut Vec<SongLuaOverlayState>,
) {
    out.clear();
    out.reserve(overlays.len());
    message_caches.resize(overlays.len(), SongLuaMessageStateCache::default());
    for (idx, overlay) in overlays.iter().enumerate() {
        out.push(song_lua_overlay_render_state_from(
            now,
            idx,
            overlay,
            overlay_events,
            overlay_eases,
            overlay_ease_ranges,
            &mut message_caches[idx],
        ));
    }
}

fn song_lua_overlay_local_states_into(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    dynamic_indices: &[usize],
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    out: &mut Vec<SongLuaOverlayState>,
) -> bool {
    let mut changed = false;
    if out.len() != overlays.len() {
        out.clear();
        out.extend(overlays.iter().map(|overlay| overlay.initial_state));
        changed = true;
    }
    message_caches.resize(overlays.len(), SongLuaMessageStateCache::default());
    for &idx in dynamic_indices {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        let next = song_lua_overlay_render_state_from(
            now,
            idx,
            overlay,
            overlay_events,
            overlay_eases,
            overlay_ease_ranges,
            &mut message_caches[idx],
        );
        changed |= out[idx] != next;
        out[idx] = next;
    }
    changed
}

#[cfg(test)]
fn song_lua_overlay_states_from_local(
    overlays: &[SongLuaOverlayActor],
    local_states: &[SongLuaOverlayState],
    screen_width: f32,
    screen_height: f32,
) -> Vec<SongLuaOverlayState> {
    let mut out = Vec::with_capacity(overlays.len());
    song_lua_overlay_states_from_local_all_into(
        overlays,
        local_states,
        screen_width,
        screen_height,
        &mut out,
    );
    out
}

fn song_lua_overlay_states_from_local_all_into(
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

fn song_lua_overlay_states_from_local_into(
    overlays: &[SongLuaOverlayActor],
    local_states: &[SongLuaOverlayState],
    dynamic_indices: &[usize],
    screen_width: f32,
    screen_height: f32,
    out: &mut Vec<SongLuaOverlayState>,
) {
    if out.len() != overlays.len() {
        song_lua_overlay_states_from_local_all_into(
            overlays,
            local_states,
            screen_width,
            screen_height,
            out,
        );
        return;
    }
    for &idx in dynamic_indices {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        let local = local_states.get(idx).copied().unwrap_or_default();
        out[idx] = overlay
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
    }
}

fn song_lua_overlay_initial_state_sets(
    overlays: &[SongLuaOverlayActor],
    screen_width: f32,
    screen_height: f32,
) -> (Vec<SongLuaOverlayState>, Vec<SongLuaOverlayState>) {
    let local = overlays
        .iter()
        .map(|overlay| overlay.initial_state)
        .collect::<Vec<_>>();
    let mut composed = Vec::with_capacity(overlays.len());
    song_lua_overlay_states_from_local_all_into(
        overlays,
        &local,
        screen_width,
        screen_height,
        &mut composed,
    );
    (local, composed)
}

fn song_lua_overlay_state_sets_from_into(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    screen_width: f32,
    screen_height: f32,
    order_cache: &SongLuaOverlayOrderCache,
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    local_out: &mut Vec<SongLuaOverlayState>,
    overlay_out: &mut Vec<SongLuaOverlayState>,
) {
    if overlays.is_empty() {
        message_caches.clear();
        local_out.clear();
        overlay_out.clear();
        return;
    }
    song_lua_overlay_state_sets_active_into(
        now,
        overlays,
        overlay_events,
        overlay_eases,
        overlay_ease_ranges,
        screen_width,
        screen_height,
        order_cache,
        message_caches,
        local_out,
        overlay_out,
    );
}

fn song_lua_overlay_state_sets_active_into(
    now: f32,
    overlays: &[SongLuaOverlayActor],
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    screen_width: f32,
    screen_height: f32,
    order_cache: &SongLuaOverlayOrderCache,
    message_caches: &mut Vec<SongLuaMessageStateCache>,
    local_out: &mut Vec<SongLuaOverlayState>,
    overlay_out: &mut Vec<SongLuaOverlayState>,
) {
    let local_changed = song_lua_overlay_local_states_into(
        now,
        overlays,
        overlay_events,
        overlay_eases,
        overlay_ease_ranges,
        &order_cache.dynamic_local_indices,
        message_caches,
        local_out,
    );
    if local_changed || overlay_out.len() != overlays.len() {
        song_lua_overlay_states_from_local_into(
            overlays,
            local_out,
            &order_cache.dynamic_composed_indices,
            screen_width,
            screen_height,
            overlay_out,
        );
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

fn song_lua_proxy_active_players_indexed(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
) -> [bool; 2] {
    let mut out = [false; 2];
    for &overlay_index in &index.proxy_indices {
        let SongLuaOverlayKind::ActorProxy { target } = &overlays[overlay_index].kind else {
            continue;
        };
        let player_index = match target {
            SongLuaProxyTarget::Player { player_index }
            | SongLuaProxyTarget::NoteField { player_index } => *player_index,
            _ => continue,
        };
        if player_index >= out.len() || !song_lua_proxy_target_has_source(target, proxy_sources) {
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

#[cfg(feature = "bench-support")]
fn song_lua_capture_replaces_player_indexed_legacy(
    overlays: &[SongLuaOverlayActor],
    capture_index: usize,
    player_index: usize,
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
    capture_stack: &mut SmallVec<[usize; 8]>,
) -> bool {
    if capture_stack.contains(&capture_index) {
        return false;
    }
    capture_stack.push(capture_index);
    let replaced = index
        .capture_children
        .get(capture_index)
        .into_iter()
        .flatten()
        .any(|&overlay_index| match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                matches!(
                    target,
                    SongLuaProxyTarget::Player { player_index: target_player }
                        | SongLuaProxyTarget::NoteField { player_index: target_player }
                        if *target_player == player_index
                ) && song_lua_proxy_target_has_source(target, proxy_sources)
            }
            SongLuaOverlayKind::AftSprite { .. } => index
                .topology
                .aft_sprite_targets
                .get(overlay_index)
                .copied()
                .and_then(SongLuaOverlayIndex::get)
                .is_some_and(|nested_capture| {
                    song_lua_capture_replaces_player_indexed_legacy(
                        overlays,
                        nested_capture,
                        player_index,
                        proxy_sources,
                        index,
                        capture_stack,
                    )
                }),
            _ => false,
        });
    capture_stack.pop();
    replaced
}

#[cfg(feature = "bench-support")]
fn song_lua_replacement_active_players_indexed_legacy(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
) -> [bool; 2] {
    let mut out =
        song_lua_proxy_active_players_indexed(overlays, overlay_states, proxy_sources, index);
    for (overlay_index, capture_index) in index
        .topology
        .aft_sprite_targets
        .iter()
        .copied()
        .map(SongLuaOverlayIndex::get)
        .enumerate()
    {
        let Some(capture_index) = capture_index else {
            continue;
        };
        let Some(overlay_state) = overlay_states.get(overlay_index) else {
            continue;
        };
        if !overlay_state.visible || overlay_state.diffuse[3] <= f32::EPSILON {
            continue;
        }
        for player_index in 0..out.len() {
            let mut capture_stack = SmallVec::<[usize; 8]>::new();
            if song_lua_capture_replaces_player_indexed_legacy(
                overlays,
                capture_index,
                player_index,
                proxy_sources,
                index,
                &mut capture_stack,
            ) {
                out[player_index] = true;
            }
        }
    }
    out
}

fn song_lua_replacement_active_players_indexed(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> [bool; 2] {
    if index.proxy_indices.is_empty() {
        return [false; 2];
    }
    song_lua_replacement_active_players_indexed_active(
        overlays,
        overlay_states,
        proxy_sources,
        index,
        visit_scratch,
    )
}

fn song_lua_replacement_active_players_indexed_active(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    proxy_sources: &[SongLuaPlayerProxySources<'_>; 2],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> [bool; 2] {
    let mut out =
        song_lua_proxy_active_players_indexed(overlays, overlay_states, proxy_sources, index);
    let mut requests = SongLuaScreenProxyRequests::default();
    visit_scratch.begin(overlays.len());
    for (overlay_index, capture_index) in index
        .topology
        .aft_sprite_targets
        .iter()
        .copied()
        .map(SongLuaOverlayIndex::get)
        .enumerate()
    {
        let Some(capture_index) = capture_index else {
            continue;
        };
        let Some(overlay_state) = overlay_states.get(overlay_index) else {
            continue;
        };
        if !song_lua_overlay_is_visible(*overlay_state) {
            continue;
        }
        song_lua_collect_capture_requests_indexed(
            overlays,
            overlay_states,
            capture_index,
            index,
            &mut requests,
            visit_scratch,
        );
    }
    for (player_index, active) in out.iter_mut().enumerate() {
        let player_requests = requests.players[player_index];
        *active |= (player_requests.player && proxy_sources[player_index].player.is_some())
            || (player_requests.note_field && proxy_sources[player_index].note_field.is_some());
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

fn song_lua_overlay_camera_ancestor(
    overlays: &[SongLuaOverlayActor],
    mut index: Option<usize>,
) -> Option<usize> {
    while let Some(current) = index {
        let overlay = overlays.get(current)?;
        if matches!(
            overlay.kind,
            SongLuaOverlayKind::ActorFrame | SongLuaOverlayKind::ActorFrameTexture
        ) {
            return Some(current);
        }
        index = overlay.parent_index;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SongLuaScreenProxyRequests {
    players: [SongLuaPlayerProxyRequests; 2],
    underlay: bool,
    overlay: bool,
}

type SongLuaSingleSource = [Arc<[Actor]>; 1];
type SongLuaActorSegments = SmallVec<[Arc<[Actor]>; 5]>;

const SONG_LUA_SCREEN_CAPTURE_SLOTS: usize = 64;
const SONG_LUA_SCREEN_CAPTURE_CAPACITY: usize = 32;
const SONG_LUA_PROXY_FRAME_BANKS: usize = 2;
const SONG_LUA_PROXY_SEGMENTS_PER_ACTOR: usize = 5;
const SONG_LUA_AFT_FRAME_BANKS: usize = 2;
const SONG_LUA_PLAYER_PROXY_SOURCE_COUNT: usize = 4;
const SONG_LUA_FIELD_PROXY_SOURCE: usize = 0;
const SONG_LUA_JUDGMENT_PROXY_SOURCE: usize = 1;
const SONG_LUA_COMBO_PROXY_SOURCE: usize = 2;
const SONG_LUA_PLAYER_PROXY_SOURCE: usize = 3;

struct SongLuaProxyJoinScratch {
    sources: [Arc<[Actor]>; SONG_LUA_PROXY_SEGMENTS_PER_ACTOR],
    _replacements: u64,
}

impl SongLuaProxyJoinScratch {
    fn new() -> Self {
        Self {
            sources: std::array::from_fn(|index| Self::new_source(index + 1)),
            _replacements: 0,
        }
    }

    fn new_source(len: usize) -> Arc<[Actor]> {
        Arc::from(
            (0..len)
                .map(|_| Actor::SharedFrame {
                    align: [0.0, 0.0],
                    offset: [0.0, 0.0],
                    size: [SizeSpec::Fill, SizeSpec::Fill],
                    children: Arc::from([]),
                    background: None,
                    z: 0,
                    tint: [1.0; 4],
                    blend: None,
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }

    fn refill(
        &mut self,
        segment_count: usize,
        diffuse: [f32; 4],
        blend: Option<BlendMode>,
        mut segment: impl FnMut(usize) -> Arc<[Actor]>,
    ) -> Arc<[Actor]> {
        let source = &mut self.sources[segment_count - 1];
        if Arc::strong_count(source) != 1 {
            *source = Self::new_source(segment_count);
            self._replacements = self._replacements.saturating_add(1);
        }
        let actors =
            Arc::get_mut(source).expect("proxy join source is uniquely owned after replacement");
        for (index, actor) in actors.iter_mut().enumerate() {
            let Actor::SharedFrame {
                children,
                tint,
                blend: actor_blend,
                ..
            } = actor
            else {
                panic!("proxy join source must remain shared frames");
            };
            *children = segment(index);
            *tint = diffuse;
            *actor_blend = blend;
        }
        Arc::clone(source)
    }
}

/// Song-lifetime backing for Song Lua ActorProxy sources.
///
/// Owner/thread model: gameplay `State`, exclusively mutated during actor
/// composition on the game/render frame thread. Lifetime: one song, allocated
/// only when a compiled overlay contains a proxy. Capacity/warmup: 64 bounded
/// screen segments, four pre-sized player sources per active player, and up to
/// five normalized source segments per compiled proxy and in-flight frame are
/// reserved at screen entry, together with the outer proxy frame that joins
/// those segments. Two banks let composition proceed while the renderer still
/// owns the preceding frame. A
/// miss after slot saturation bypasses insertion and uses the owned fallback;
/// child-vector overflow grows only the affected slot. There is no lookup,
/// eviction, scan, pruning, or mid-song destruction. Replacements/growths are
/// tracked by each backing slot. Worst-case steady frame work is linear in
/// actors already emitted for the requested captures.
struct SongLuaProxyActorBank {
    screen: [SharedActorFrameScratch; SONG_LUA_SCREEN_CAPTURE_SLOTS],
    screen_used: usize,
    players: [[SharedActorFrameScratch; SONG_LUA_PLAYER_PROXY_SOURCE_COUNT]; MAX_PLAYERS],
    proxy_segments: Vec<SharedActorFrameScratch>,
    proxy_frames: Vec<SongLuaProxyJoinScratch>,
    proxy_groups_used: usize,
}

impl SongLuaProxyActorBank {
    fn new(active_players: usize, proxy_count: usize) -> Self {
        Self {
            screen: std::array::from_fn(|_| {
                SharedActorFrameScratch::with_capacity(SONG_LUA_SCREEN_CAPTURE_CAPACITY)
            }),
            screen_used: 0,
            players: std::array::from_fn(|player| {
                let active = player < active_players;
                [
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * NOTEFIELD_ACTOR_SCRATCH_CAPACITY,
                    ),
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
                    ),
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
                    ),
                    SharedActorFrameScratch::with_capacity(
                        usize::from(active) * PLAYER_ACTOR_SCRATCH_CAPACITY,
                    ),
                ]
            }),
            proxy_segments: (0..proxy_count.saturating_mul(SONG_LUA_PROXY_SEGMENTS_PER_ACTOR))
                .map(|index| {
                    SharedActorFrameScratch::with_capacity(
                        if index % SONG_LUA_PROXY_SEGMENTS_PER_ACTOR == 0 {
                            PLAYER_ACTOR_SCRATCH_CAPACITY
                        } else {
                            SONG_LUA_SCREEN_CAPTURE_CAPACITY
                        },
                    )
                })
                .collect(),
            proxy_frames: (0..proxy_count)
                .map(|_| SongLuaProxyJoinScratch::new())
                .collect(),
            proxy_groups_used: 0,
        }
    }

    fn begin_frame(&mut self) {
        self.screen_used = 0;
        self.proxy_groups_used = 0;
        for player in &mut self.players {
            for source in player {
                source.clear();
            }
        }
    }
}

struct SongLuaProxyActorScratch {
    banks: [SongLuaProxyActorBank; SONG_LUA_PROXY_FRAME_BANKS],
    active_bank: usize,
    frame_banks: usize,
}

impl SongLuaProxyActorScratch {
    #[cfg(any(test, feature = "bench-support"))]
    fn new(active_players: usize) -> Self {
        Self::with_proxy_capacity(active_players, 1)
    }

    #[cfg(feature = "bench-support")]
    fn with_frame_banks(active_players: usize, frame_banks: usize) -> Self {
        Self::with_capacity_and_banks(active_players, 1, frame_banks)
    }

    fn with_proxy_capacity(active_players: usize, proxy_count: usize) -> Self {
        Self::with_capacity_and_banks(active_players, proxy_count, SONG_LUA_PROXY_FRAME_BANKS)
    }

    fn with_capacity_and_banks(
        active_players: usize,
        proxy_count: usize,
        frame_banks: usize,
    ) -> Self {
        let frame_banks = frame_banks.clamp(1, SONG_LUA_PROXY_FRAME_BANKS);
        Self {
            banks: std::array::from_fn(|_| SongLuaProxyActorBank::new(active_players, proxy_count)),
            active_bank: frame_banks - 1,
            frame_banks,
        }
    }

    fn begin_frame(&mut self) {
        self.active_bank = (self.active_bank + 1) % self.frame_banks;
        self.banks[self.active_bank].begin_frame();
    }

    fn next_screen(&mut self) -> Option<&mut SharedActorFrameScratch> {
        let bank = &mut self.banks[self.active_bank];
        let slot = bank.screen.get_mut(bank.screen_used)?;
        bank.screen_used += 1;
        Some(slot)
    }

    fn player(&mut self, player: usize, source: usize) -> Option<&mut SharedActorFrameScratch> {
        self.banks[self.active_bank]
            .players
            .get_mut(player)?
            .get_mut(source)
    }

    fn reserve_proxy_group(&mut self) -> Option<(usize, usize)> {
        let bank = &mut self.banks[self.active_bank];
        let group = bank.proxy_groups_used;
        bank.proxy_groups_used = bank.proxy_groups_used.saturating_add(1);
        bank.proxy_frames.get(group)?;
        Some((
            group,
            group.saturating_mul(SONG_LUA_PROXY_SEGMENTS_PER_ACTOR),
        ))
    }

    fn normalize_segment(&mut self, segment: &Arc<[Actor]>, slot_index: usize) -> Arc<[Actor]> {
        let bank = &mut self.banks[self.active_bank];
        song_lua_normalize_proxy_segment(segment, bank.proxy_segments.get_mut(slot_index))
    }

    fn join_proxy_segments(
        &mut self,
        group: usize,
        segment_start: usize,
        source: &[Arc<[Actor]>],
        diffuse: [f32; 4],
        blend: Option<BlendMode>,
    ) -> Arc<[Actor]> {
        let bank = &mut self.banks[self.active_bank];
        let proxy_frames = &mut bank.proxy_frames;
        let proxy_segments = &mut bank.proxy_segments;
        proxy_frames[group].refill(source.len(), diffuse, blend, |index| {
            song_lua_normalize_proxy_segment(
                &source[index],
                proxy_segments.get_mut(segment_start + index),
            )
        })
    }
}

fn song_lua_normalize_proxy_segment(
    segment: &Arc<[Actor]>,
    slot: Option<&mut SharedActorFrameScratch>,
) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    let Some(slot) = slot else {
        return song_lua_proxy_source_segment_owned(segment);
    };
    let (offset, actors) = song_lua_proxy_segment_actors(segment);
    slot.refill(offset, |children| {
        song_lua_proxy_local_children_into(actors.iter().cloned(), children);
    })
    .unwrap_or_else(|| Arc::from([]))
}

/// Song-lifetime backing for ActorFrameTexture capture output.
///
/// Owner/thread model: gameplay `State`, exclusively mutated during actor
/// composition on the game/render frame thread. Lifetime: one song. Capacity:
/// each AFT sprite has two frame banks sized at screen entry from its compiled
/// capture descendants, including the maximum two actors emitted by glow and
/// model layers. Warmup occurs during screen initialization. Gameplay misses
/// grow only the affected slot and retain that high-water mark; there is no
/// lookup, eviction, pruning, or gameplay destruction. Two banks cover the
/// renderer retaining the preceding frame. Per-slot replacement/growth counts
/// are available from `SharedActorFrameScratch::stats`. Worst-case frame work
/// remains linear in the already-required captured actors.
#[derive(Default)]
struct SongLuaAftCaptureScratch {
    slots: Vec<Option<[SharedActorFrameScratch; SONG_LUA_AFT_FRAME_BANKS]>>,
    active_bank: usize,
}

impl SongLuaAftCaptureScratch {
    fn new(overlays: &[SongLuaOverlayActor], topology: &SongLuaOverlayTopologyIndex) -> Self {
        let slots = overlays
            .iter()
            .enumerate()
            .map(|(index, overlay)| {
                matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }).then(|| {
                    let capacity = song_lua_aft_capture_capacity(overlays, topology, index);
                    std::array::from_fn(|_| SharedActorFrameScratch::with_capacity(capacity))
                })
            })
            .collect();
        Self {
            slots,
            active_bank: SONG_LUA_AFT_FRAME_BANKS - 1,
        }
    }

    fn begin_frame(&mut self) {
        self.active_bank = (self.active_bank + 1) % SONG_LUA_AFT_FRAME_BANKS;
    }

    fn overlay(&mut self, index: usize) -> Option<&mut SharedActorFrameScratch> {
        self.slots
            .get_mut(index)?
            .as_mut()
            .map(|banks| &mut banks[self.active_bank])
    }
}

fn song_lua_aft_actor_capacity(kind: &SongLuaOverlayKind) -> usize {
    match kind {
        SongLuaOverlayKind::Actor
        | SongLuaOverlayKind::ActorFrame
        | SongLuaOverlayKind::ActorFrameTexture
        | SongLuaOverlayKind::AftSprite { .. }
        | SongLuaOverlayKind::Sound { .. } => 0,
        SongLuaOverlayKind::ActorProxy { .. } => 1,
        SongLuaOverlayKind::Model { layers } => layers.len().saturating_mul(2),
        SongLuaOverlayKind::NoteskinActor { slots } => slots.len().saturating_mul(2),
        _ => 2,
    }
}

fn song_lua_aft_capture_capacity(
    overlays: &[SongLuaOverlayActor],
    topology: &SongLuaOverlayTopologyIndex,
    aft_sprite_index: usize,
) -> usize {
    let Some(capture_index) = topology
        .aft_sprite_targets
        .get(aft_sprite_index)
        .copied()
        .and_then(SongLuaOverlayIndex::get)
    else {
        return 0;
    };
    overlays
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            topology
                .aft_ancestors
                .get(*index)
                .copied()
                .and_then(SongLuaOverlayIndex::get)
                == Some(capture_index)
        })
        .fold(2usize, |capacity, (_, overlay)| {
            capacity.saturating_add(song_lua_aft_actor_capacity(&overlay.kind))
        })
}

#[derive(Clone, Copy)]
struct SongLuaCaptureTransform {
    z_shift: i16,
    tint: [f32; 4],
    blend: Option<BlendMode>,
    playfield_center_x: f32,
    target_x: f32,
    target_y: f32,
    rotation_x: f32,
    rotation_z: f32,
    rotation_y: f32,
    skew_x: f32,
    skew_y: f32,
    zoom_x: f32,
    zoom_y: f32,
    zoom_z: f32,
}

#[inline(always)]
fn song_lua_overlay_is_visible(state: SongLuaOverlayState) -> bool {
    state.visible && state.diffuse[3] > f32::EPSILON
}

#[inline(always)]
fn song_lua_capture_new_actors(
    dest: &mut Option<SongLuaActorSegments>,
    actors: &mut Vec<Actor>,
    start: usize,
    scratch: Option<&mut SongLuaProxyActorScratch>,
) {
    let Some(dest) = dest.as_mut() else {
        return;
    };
    let children = scratch
        .and_then(SongLuaProxyActorScratch::next_screen)
        .and_then(|scratch| scratch.capture_range(actors, start))
        .or_else(|| song_lua_capture_new_actors_owned(actors, start));
    let Some(children) = children else { return };
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

fn song_lua_capture_new_actors_owned(
    actors: &mut Vec<Actor>,
    start: usize,
) -> Option<Arc<[Actor]>> {
    if start >= actors.len() {
        return None;
    }
    Some(Arc::from_iter(actors.drain(start..)))
}

#[cfg(test)]
fn song_lua_player_child_proxy_source(
    actors: &mut Vec<Actor>,
    origin_x: f32,
    origin_y: f32,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    scratch
        .refill([-origin_x, -origin_y], |children| children.append(actors))
        .map(|source| [source])
}

fn song_lua_share_actor_source_in_place(
    actors: &mut Vec<Actor>,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    let children = scratch.capture_range(actors, 0)?;
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
    Some([children])
}

#[cfg(feature = "bench-support")]
fn song_lua_shared_segment_actor(segment: Arc<[Actor]>) -> Actor {
    Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: segment,
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    }
}

fn song_lua_render_captured_source(
    field_source: Option<&CapturedActorSource>,
    hud_source: Option<&CapturedActorSource>,
    transform: SongLuaCaptureTransform,
    scratch: &mut SharedActorFrameScratch,
) -> Option<SongLuaSingleSource> {
    if field_source.is_none() && hud_source.is_none() {
        return None;
    }
    let field_len = field_source.map_or(0, |source| {
        source
            .iter()
            .map(|segment| song_lua_captured_segment_actors(segment).len())
            .sum()
    });
    let hud_len = hud_source.map_or(0, |source| {
        source
            .iter()
            .map(|segment| song_lua_captured_segment_actors(segment).len())
            .sum()
    });
    let field_has_camera = field_source.is_some_and(|source| {
        source.iter().any(|segment| {
            song_lua_captured_segment_actors(segment)
                .iter()
                .any(|actor| {
                    matches!(
                        actor,
                        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop
                    )
                })
        })
    });
    let field_actors = field_source
        .into_iter()
        .flat_map(|source| source.iter())
        .flat_map(|segment| song_lua_captured_segment_actors(segment).iter().cloned());
    let hud_actors = hud_source
        .into_iter()
        .flat_map(|source| source.iter())
        .flat_map(|segment| song_lua_captured_segment_actors(segment).iter().cloned());
    scratch
        .refill([-transform.target_x, -transform.target_y], |out| {
            append_song_lua_player_transform(
                field_actors,
                hud_actors,
                field_len,
                hud_len,
                field_has_camera,
                out,
                transform.z_shift,
                transform.tint,
                transform.blend,
                transform.playfield_center_x,
                transform.target_x,
                transform.target_y,
                transform.rotation_x,
                transform.rotation_z,
                transform.rotation_y,
                transform.skew_x,
                transform.skew_y,
                transform.zoom_x,
                transform.zoom_y,
                transform.zoom_z,
            );
        })
        .map(|source| [source])
}

fn song_lua_captured_segment_actors(segment: &[Actor]) -> &[Actor] {
    match segment {
        [
            Actor::Frame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children,
                background: None,
                z: 0,
            },
        ] => children,
        _ => segment,
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn bench_song_lua_proxy_capture_cycles_legacy(players: usize, cycles: usize) -> usize {
    let players = players.clamp(1, 2);
    let mut root_actors = Vec::with_capacity(96);
    let mut player_scratch: [Vec<Actor>; 2] = std::array::from_fn(|_| Vec::with_capacity(16));
    let mut checksum = 0usize;

    for _ in 0..cycles {
        root_actors.clear();
        let mut underlay_source = Some(SongLuaActorSegments::new());
        let mut overlay_source = Some(SongLuaActorSegments::new());
        for _ in 0..5 {
            let start = root_actors.len();
            root_actors.extend((0..4).map(|_| Actor::CameraPop));
            song_lua_capture_new_actors_legacy(&mut underlay_source, &mut root_actors, start);
        }
        for _ in 0..4 {
            let start = root_actors.len();
            root_actors.extend((0..4).map(|_| Actor::CameraPop));
            song_lua_capture_new_actors_legacy(&mut overlay_source, &mut root_actors, start);
        }

        for scratch in player_scratch.iter_mut().take(players) {
            let transform = SongLuaCaptureTransform {
                z_shift: 0,
                tint: [1.0; 4],
                blend: None,
                playfield_center_x: screen_center_x(),
                target_x: screen_center_x() + 16.0,
                target_y: screen_center_y(),
                rotation_x: 0.0,
                rotation_z: 0.0,
                rotation_y: 0.0,
                skew_x: 0.0,
                skew_y: 0.0,
                zoom_x: 1.0,
                zoom_y: 1.0,
                zoom_z: 1.0,
            };
            let segment = || Arc::<[Actor]>::from_iter((0..8).map(|_| Actor::CameraPop));
            let field_capture = [segment()];
            let judgment_capture = [segment()];
            let combo_capture = [segment()];
            let field_source =
                song_lua_render_captured_source_legacy(Some(&field_capture), None, transform);
            let judgment_source =
                song_lua_render_captured_source_legacy(None, Some(&judgment_capture), transform);
            let combo_source =
                song_lua_render_captured_source_legacy(None, Some(&combo_capture), transform);
            scratch.clear();
            scratch.extend((0..12).map(|_| Actor::CameraPop));
            let player_source = song_lua_share_actor_source_in_place_legacy(scratch);
            checksum = checksum.wrapping_add(
                field_source.as_ref().map_or(0, |source| source.len())
                    + judgment_source.as_ref().map_or(0, |source| source.len())
                    + combo_source.as_ref().map_or(0, |source| source.len())
                    + player_source.as_ref().map_or(0, |source| source.len()),
            );
        }
        checksum = checksum.wrapping_add(
            root_actors.len()
                + underlay_source.as_ref().map_or(0, SmallVec::len)
                + overlay_source.as_ref().map_or(0, SmallVec::len),
        );
    }
    checksum
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn bench_song_lua_proxy_capture_cycles_screen_reuse(players: usize, cycles: usize) -> usize {
    let players = players.clamp(1, MAX_PLAYERS);
    let mut root_actors = Vec::with_capacity(96);
    let mut player_scratch: [Vec<Actor>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::with_capacity(16));
    let mut proxy_scratch = SongLuaProxyActorScratch::new(players);
    let mut checksum = 0usize;

    for _ in 0..cycles {
        root_actors.clear();
        proxy_scratch.begin_frame();
        let mut underlay_source = Some(SongLuaActorSegments::new());
        let mut overlay_source = Some(SongLuaActorSegments::new());
        for _ in 0..5 {
            let start = root_actors.len();
            root_actors.extend((0..4).map(|_| Actor::CameraPop));
            song_lua_capture_new_actors(
                &mut underlay_source,
                &mut root_actors,
                start,
                Some(&mut proxy_scratch),
            );
        }
        for _ in 0..4 {
            let start = root_actors.len();
            root_actors.extend((0..4).map(|_| Actor::CameraPop));
            song_lua_capture_new_actors(
                &mut overlay_source,
                &mut root_actors,
                start,
                Some(&mut proxy_scratch),
            );
        }

        for scratch in player_scratch.iter_mut().take(players) {
            let transform = song_lua_proxy_bench_transform();
            let segment = || Arc::<[Actor]>::from_iter((0..8).map(|_| Actor::CameraPop));
            let field_capture = [segment()];
            let judgment_capture = [segment()];
            let combo_capture = [segment()];
            let field_source =
                song_lua_render_captured_source_legacy(Some(&field_capture), None, transform);
            let judgment_source =
                song_lua_render_captured_source_legacy(None, Some(&judgment_capture), transform);
            let combo_source =
                song_lua_render_captured_source_legacy(None, Some(&combo_capture), transform);
            scratch.clear();
            scratch.extend((0..12).map(|_| Actor::CameraPop));
            let player_source = song_lua_share_actor_source_in_place_legacy(scratch);
            checksum = checksum.wrapping_add(
                field_source.as_ref().map_or(0, |source| source.len())
                    + judgment_source.as_ref().map_or(0, |source| source.len())
                    + combo_source.as_ref().map_or(0, |source| source.len())
                    + player_source.as_ref().map_or(0, |source| source.len()),
            );
        }
        checksum = checksum.wrapping_add(
            root_actors.len()
                + underlay_source.as_ref().map_or(0, SmallVec::len)
                + overlay_source.as_ref().map_or(0, SmallVec::len),
        );
    }
    checksum
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn bench_song_lua_proxy_capture_cycles(players: usize, cycles: usize) -> usize {
    bench_song_lua_proxy_capture_cycles_with_banks(players, cycles, SONG_LUA_PROXY_FRAME_BANKS)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn bench_song_lua_proxy_capture_cycles_single_bank(players: usize, cycles: usize) -> usize {
    bench_song_lua_proxy_capture_cycles_with_banks(players, cycles, 1)
}

#[cfg(feature = "bench-support")]
fn bench_song_lua_proxy_capture_cycles_with_banks(
    players: usize,
    cycles: usize,
    frame_banks: usize,
) -> usize {
    let players = players.clamp(1, MAX_PLAYERS);
    let mut root_actors = Vec::with_capacity(96);
    let mut retained_root_actors = Vec::with_capacity(96);
    let mut player_actors: [Vec<Actor>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::with_capacity(16));
    let mut retained_player_actors: [Vec<Actor>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::with_capacity(16));
    let mut capture_sources: [[SharedActorFrameScratch; 3]; MAX_PLAYERS] =
        std::array::from_fn(|_| std::array::from_fn(|_| SharedActorFrameScratch::with_capacity(8)));
    let mut proxy_scratch = SongLuaProxyActorScratch::with_frame_banks(players, frame_banks);
    let mut current_sources: Vec<SongLuaSingleSource> = Vec::with_capacity(players * 4);
    let mut retained_sources: Vec<SongLuaSingleSource> = Vec::with_capacity(players * 4);
    let mut checksum = 0usize;

    for _ in 0..cycles {
        retained_root_actors.clear();
        std::mem::swap(&mut root_actors, &mut retained_root_actors);
        root_actors.clear();
        for player in 0..players {
            retained_player_actors[player].clear();
            std::mem::swap(
                &mut player_actors[player],
                &mut retained_player_actors[player],
            );
            player_actors[player].clear();
        }
        retained_sources.clear();
        std::mem::swap(&mut current_sources, &mut retained_sources);
        current_sources.clear();
        proxy_scratch.begin_frame();
        let mut underlay_source = Some(SongLuaActorSegments::new());
        let mut overlay_source = Some(SongLuaActorSegments::new());
        for _ in 0..5 {
            let start = root_actors.len();
            root_actors.extend((0..4).map(|_| Actor::CameraPop));
            song_lua_capture_new_actors(
                &mut underlay_source,
                &mut root_actors,
                start,
                Some(&mut proxy_scratch),
            );
        }
        for _ in 0..4 {
            let start = root_actors.len();
            root_actors.extend((0..4).map(|_| Actor::CameraPop));
            song_lua_capture_new_actors(
                &mut overlay_source,
                &mut root_actors,
                start,
                Some(&mut proxy_scratch),
            );
        }

        for player in 0..players {
            let transform = song_lua_proxy_bench_transform();
            let [field_capture, judgment_capture, combo_capture] = &mut capture_sources[player];
            let field_capture = [field_capture
                .refill([0.0, 0.0], |out| {
                    out.extend((0..8).map(|_| Actor::CameraPop));
                })
                .expect("benchmark field capture is populated")];
            let judgment_capture = [judgment_capture
                .refill([0.0, 0.0], |out| {
                    out.extend((0..8).map(|_| Actor::CameraPop));
                })
                .expect("benchmark judgment capture is populated")];
            let combo_capture = [combo_capture
                .refill([0.0, 0.0], |out| {
                    out.extend((0..8).map(|_| Actor::CameraPop));
                })
                .expect("benchmark combo capture is populated")];
            let field_source = song_lua_render_captured_source(
                Some(&field_capture),
                None,
                transform,
                proxy_scratch
                    .player(player, SONG_LUA_FIELD_PROXY_SOURCE)
                    .expect("active player has field scratch"),
            );
            let judgment_source = song_lua_render_captured_source(
                None,
                Some(&judgment_capture),
                transform,
                proxy_scratch
                    .player(player, SONG_LUA_JUDGMENT_PROXY_SOURCE)
                    .expect("active player has judgment scratch"),
            );
            let combo_source = song_lua_render_captured_source(
                None,
                Some(&combo_capture),
                transform,
                proxy_scratch
                    .player(player, SONG_LUA_COMBO_PROXY_SOURCE)
                    .expect("active player has combo scratch"),
            );
            let actors = &mut player_actors[player];
            actors.clear();
            actors.extend((0..12).map(|_| Actor::CameraPop));
            let player_source = song_lua_share_actor_source_in_place(
                actors,
                proxy_scratch
                    .player(player, SONG_LUA_PLAYER_PROXY_SOURCE)
                    .expect("active player has player scratch"),
            );
            for source in [field_source, judgment_source, combo_source, player_source] {
                checksum = checksum.wrapping_add(source.as_ref().map_or(0, |source| source.len()));
                if let Some(source) = source {
                    current_sources.push(source);
                }
            }
        }
        checksum = checksum.wrapping_add(
            root_actors.len()
                + underlay_source.as_ref().map_or(0, SmallVec::len)
                + overlay_source.as_ref().map_or(0, SmallVec::len),
        );
    }
    checksum
}

#[cfg(feature = "bench-support")]
fn song_lua_proxy_bench_transform() -> SongLuaCaptureTransform {
    SongLuaCaptureTransform {
        z_shift: 0,
        tint: [1.0; 4],
        blend: None,
        playfield_center_x: screen_center_x(),
        target_x: screen_center_x() + 16.0,
        target_y: screen_center_y(),
        rotation_x: 0.0,
        rotation_z: 0.0,
        rotation_y: 0.0,
        skew_x: 0.0,
        skew_y: 0.0,
        zoom_x: 1.0,
        zoom_y: 1.0,
        zoom_z: 1.0,
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_capture_new_actors_legacy(
    dest: &mut Option<SongLuaActorSegments>,
    actors: &mut Vec<Actor>,
    start: usize,
) {
    let Some(dest) = dest.as_mut() else { return };
    let Some(children) = song_lua_capture_new_actors_owned(actors, start) else {
        return;
    };
    dest.push(Arc::clone(&children));
    actors.push(song_lua_shared_segment_actor(children));
}

#[cfg(feature = "bench-support")]
fn song_lua_share_actor_source_in_place_legacy(
    actors: &mut Vec<Actor>,
) -> Option<SongLuaSingleSource> {
    let children = song_lua_capture_new_actors_owned(actors, 0)?;
    actors.push(song_lua_shared_segment_actor(Arc::clone(&children)));
    Some([children])
}

#[cfg(feature = "bench-support")]
fn song_lua_render_captured_source_legacy(
    field_source: Option<&CapturedActorSource>,
    hud_source: Option<&CapturedActorSource>,
    transform: SongLuaCaptureTransform,
) -> Option<SongLuaSingleSource> {
    if field_source.is_none() && hud_source.is_none() {
        return None;
    }
    let field_len = field_source.map_or(0, |source| source.iter().map(|part| part.len()).sum());
    let hud_len = hud_source.map_or(0, |source| source.len());
    let field_has_camera = field_source.is_some_and(|source| {
        source.iter().any(|part| {
            part.iter().any(|actor| {
                matches!(
                    actor,
                    Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop
                )
            })
        })
    });
    let field_actors = field_source
        .into_iter()
        .flat_map(|source| source.iter())
        .flat_map(|part| part.iter().cloned());
    let hud_actors = hud_source
        .into_iter()
        .flat_map(|source| source.iter())
        .map(|part| song_lua_shared_segment_actor(Arc::clone(part)));
    let mut out = Vec::new();
    append_song_lua_player_transform(
        field_actors,
        hud_actors,
        field_len,
        hud_len,
        field_has_camera,
        &mut out,
        transform.z_shift,
        transform.tint,
        transform.blend,
        transform.playfield_center_x,
        transform.target_x,
        transform.target_y,
        transform.rotation_x,
        transform.rotation_z,
        transform.rotation_y,
        transform.skew_x,
        transform.skew_y,
        transform.zoom_x,
        transform.zoom_y,
        transform.zoom_z,
    );
    if out.is_empty() {
        None
    } else {
        Some([Arc::from([Actor::Frame {
            align: [0.0, 0.0],
            offset: [-transform.target_x, -transform.target_y],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: out,
            background: None,
            z: 0,
        }])])
    }
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

#[cfg(any(test, feature = "bench-support"))]
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

#[cfg(any(test, feature = "bench-support"))]
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

#[cfg(feature = "bench-support")]
fn song_lua_collect_capture_requests_indexed_legacy(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    capture_index: usize,
    index: &SongLuaProxyRequestIndex,
    requests: &mut SongLuaScreenProxyRequests,
    capture_stack: &mut SmallVec<[usize; 8]>,
) {
    if capture_stack.contains(&capture_index) {
        return;
    }
    capture_stack.push(capture_index);
    let Some(children) = index.capture_children.get(capture_index) else {
        capture_stack.pop();
        return;
    };
    for &overlay_index in children {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(requests, target);
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                if let Some(nested_capture) = index
                    .topology
                    .aft_sprite_targets
                    .get(overlay_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                {
                    song_lua_collect_capture_requests_indexed_legacy(
                        overlays,
                        overlay_states,
                        nested_capture,
                        index,
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

#[cfg(feature = "bench-support")]
fn song_lua_proxy_requests_indexed_legacy(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
) -> SongLuaScreenProxyRequests {
    let mut requests = SongLuaScreenProxyRequests::default();
    let mut capture_stack = SmallVec::<[usize; 8]>::new();
    for &overlay_index in &index.root_indices {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(&mut requests, target);
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                if let Some(capture_index) = index
                    .topology
                    .aft_sprite_targets
                    .get(overlay_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                {
                    song_lua_collect_capture_requests_indexed_legacy(
                        overlays,
                        overlay_states,
                        capture_index,
                        index,
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

fn song_lua_collect_capture_requests_indexed(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    capture_index: usize,
    index: &SongLuaProxyRequestIndex,
    requests: &mut SongLuaScreenProxyRequests,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) {
    if !visit_scratch.visit(capture_index) {
        return;
    }
    let Some(children) = index.capture_children.get(capture_index) else {
        return;
    };
    for &overlay_index in children {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(requests, target);
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                if let Some(nested_capture) = index
                    .topology
                    .aft_sprite_targets
                    .get(overlay_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                {
                    song_lua_collect_capture_requests_indexed(
                        overlays,
                        overlay_states,
                        nested_capture,
                        index,
                        requests,
                        visit_scratch,
                    );
                }
            }
            _ => {}
        }
    }
}

fn song_lua_proxy_requests_indexed(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> SongLuaScreenProxyRequests {
    if index.proxy_indices.is_empty() {
        return SongLuaScreenProxyRequests::default();
    }
    song_lua_proxy_requests_indexed_active(overlays, overlay_states, index, visit_scratch)
}

fn song_lua_proxy_requests_indexed_active(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    index: &SongLuaProxyRequestIndex,
    visit_scratch: &mut SongLuaCaptureVisitScratch,
) -> SongLuaScreenProxyRequests {
    let mut requests = SongLuaScreenProxyRequests::default();
    visit_scratch.begin(overlays.len());
    for &overlay_index in &index.root_indices {
        let Some(overlay_state) = overlay_states.get(overlay_index).copied() else {
            continue;
        };
        if !song_lua_overlay_is_visible(overlay_state) {
            continue;
        }
        match &overlays[overlay_index].kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                song_lua_mark_proxy_target(&mut requests, target);
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                if let Some(capture_index) = index
                    .topology
                    .aft_sprite_targets
                    .get(overlay_index)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get)
                {
                    song_lua_collect_capture_requests_indexed(
                        overlays,
                        overlay_states,
                        capture_index,
                        index,
                        &mut requests,
                        visit_scratch,
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

#[cfg(any(test, feature = "bench-support"))]
fn song_lua_build_proxy_actor(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Arc<[Actor]>],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    song_lua_build_proxy_actor_with_scratch(
        state,
        z,
        source,
        overlay_space_width,
        overlay_space_height,
        None,
    )
}

fn song_lua_build_proxy_actor_with_scratch(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Arc<[Actor]>],
    overlay_space_width: f32,
    overlay_space_height: f32,
    mut scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = Some(song_lua_overlay_blend(state.blend));
    let offset = [
        state.x * screen_width() / overlay_space_width.max(1.0),
        state.y * screen_height() / overlay_space_height.max(1.0),
    ];
    if let [segment] = source {
        let slot_index = scratch
            .as_deref_mut()
            .and_then(SongLuaProxyActorScratch::reserve_proxy_group)
            .map(|(_, start)| start);
        return Some(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset,
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: match (scratch.as_deref_mut(), slot_index) {
                (Some(scratch), Some(slot_index)) => scratch.normalize_segment(segment, slot_index),
                _ => song_lua_proxy_source_segment_owned(segment),
            },
            background: None,
            z,
            tint: state.diffuse,
            blend,
        });
    }
    song_lua_build_proxy_frame_actor_with_scratch(
        state,
        z,
        source,
        overlay_space_width,
        overlay_space_height,
        scratch,
    )
}

#[cfg(feature = "bench-support")]
fn song_lua_build_proxy_frame_actor(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Arc<[Actor]>],
    overlay_space_width: f32,
    overlay_space_height: f32,
) -> Option<Actor> {
    song_lua_build_proxy_frame_actor_with_scratch(
        state,
        z,
        source,
        overlay_space_width,
        overlay_space_height,
        None,
    )
}

fn song_lua_build_proxy_frame_actor_with_scratch(
    state: SongLuaOverlayState,
    z: i16,
    source: &[Arc<[Actor]>],
    overlay_space_width: f32,
    overlay_space_height: f32,
    mut scratch: Option<&mut SongLuaProxyActorScratch>,
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON || source.is_empty() {
        return None;
    }
    let blend = Some(song_lua_overlay_blend(state.blend));
    let offset = [
        state.x * screen_width() / overlay_space_width.max(1.0),
        state.y * screen_height() / overlay_space_height.max(1.0),
    ];
    if source.len() <= SONG_LUA_PROXY_SEGMENTS_PER_ACTOR
        && let Some((group, proxy_segment_start)) = scratch
            .as_deref_mut()
            .and_then(SongLuaProxyActorScratch::reserve_proxy_group)
    {
        let children = scratch
            .as_deref_mut()
            .expect("reserved proxy group requires scratch")
            .join_proxy_segments(group, proxy_segment_start, source, state.diffuse, blend);
        return Some(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset,
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z,
            tint: [1.0; 4],
            blend: None,
        });
    }

    let mut children = Vec::with_capacity(source.len());
    for segment in source {
        children.push(Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: song_lua_proxy_source_segment_owned(segment),
            background: None,
            z: 0,
            tint: state.diffuse,
            blend,
        });
    }
    Some(Actor::Frame {
        align: [0.0, 0.0],
        offset,
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z,
    })
}

fn song_lua_proxy_source_segment_owned(segment: &Arc<[Actor]>) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    let (offset, actors) = song_lua_proxy_segment_actors(segment);
    let mut children = Vec::with_capacity(actors.len());
    song_lua_proxy_local_children_into(actors.iter().cloned(), &mut children);
    if offset == [0.0, 0.0] {
        Arc::from(children)
    } else {
        Arc::from([Actor::Frame {
            align: [0.0, 0.0],
            offset,
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        }])
    }
}

fn song_lua_proxy_segment_actors(segment: &[Actor]) -> ([f32; 2], &[Actor]) {
    let [
        Actor::Frame {
            align,
            offset,
            size,
            children,
            background,
            z,
        },
    ] = segment
    else {
        return ([0.0, 0.0], segment);
    };
    if *align == [0.0, 0.0]
        && matches!(*size, [SizeSpec::Fill, SizeSpec::Fill])
        && background.is_none()
        && *z == 0
    {
        (*offset, children)
    } else {
        ([0.0, 0.0], segment)
    }
}

fn song_lua_proxy_actor_has_z(actor: &Actor) -> bool {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. } => *z != 0,
        Actor::Frame { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::SharedFrame { z, children, .. } => {
            *z != 0 || children.iter().any(song_lua_proxy_actor_has_z)
        }
        Actor::RetainedFrame { z, frame, .. } => {
            *z != 0 || frame.children().iter().any(song_lua_proxy_actor_has_z)
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
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::Frame { z, .. }
        | Actor::SharedFrame { z, .. }
        | Actor::RetainedFrame { z, .. } => *z,
        Actor::Shadow { child, .. } => song_lua_proxy_actor_z(child),
        Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop => 0,
    }
}

fn song_lua_proxy_local_children_into(children: impl Iterator<Item = Actor>, out: &mut Vec<Actor>) {
    out.extend(children);
    song_lua_proxy_local_children_in_place(out);
}

fn song_lua_proxy_local_children_in_place(children: &mut [Actor]) {
    let mut run_start = 0;
    for index in 0..children.len() {
        if matches!(children[index], Actor::CameraPush { .. } | Actor::CameraPop) {
            song_lua_proxy_local_run(&mut children[run_start..index]);
            song_lua_proxy_zero_local_z(&mut children[index]);
            run_start = index + 1;
        }
    }
    song_lua_proxy_local_run(&mut children[run_start..]);
}

fn song_lua_proxy_local_run(children: &mut [Actor]) {
    if children.len() <= 8 || children.len() > PLAYER_ACTOR_SCRATCH_CAPACITY {
        // Tiny and overflow runs avoid auxiliary setup while retaining stable
        // equal-z ordering and a hard zero-allocation fallback.
        for index in 1..children.len() {
            let z = song_lua_proxy_actor_z(&children[index]);
            let mut insert = index;
            while insert > 0 && song_lua_proxy_actor_z(&children[insert - 1]) > z {
                children.swap(insert - 1, insert);
                insert -= 1;
            }
        }
    } else {
        // Sort compact indices by (z, original position), then apply the
        // permutation to the large Actor values. This is stable and uses no
        // heap-backed merge buffer.
        let mut order = [0u16; PLAYER_ACTOR_SCRATCH_CAPACITY];
        let mut target = [0u16; PLAYER_ACTOR_SCRATCH_CAPACITY];
        for (index, slot) in order[..children.len()].iter_mut().enumerate() {
            *slot = index as u16;
        }
        order[..children.len()].sort_unstable_by_key(|&index| {
            (song_lua_proxy_actor_z(&children[index as usize]), index)
        });
        for (new_index, &old_index) in order[..children.len()].iter().enumerate() {
            target[old_index as usize] = new_index as u16;
        }
        for index in 0..children.len() {
            while target[index] as usize != index {
                let swap_index = target[index] as usize;
                children.swap(index, swap_index);
                target.swap(index, swap_index);
            }
        }
    }
    for child in children {
        song_lua_proxy_zero_local_z(child);
    }
}

fn song_lua_proxy_zero_local_z(actor: &mut Actor) {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::RetainedFrame { z, .. } => *z = 0,
        Actor::Frame { z, children, .. } => {
            *z = 0;
            song_lua_proxy_local_children_in_place(children);
        }
        Actor::SharedFrame { z, children, .. } => {
            *z = 0;
            *children = song_lua_proxy_source_segment_owned(children);
        }
        Actor::Camera { children, .. } => song_lua_proxy_local_children_in_place(children),
        Actor::Shadow { child, .. } => song_lua_proxy_zero_local_z(child),
        Actor::CameraPush { .. } | Actor::CameraPop => {}
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_proxy_source_segment_legacy(segment: &Arc<[Actor]>) -> Arc<[Actor]> {
    if !segment.iter().any(song_lua_proxy_actor_has_z) {
        return Arc::clone(segment);
    }
    Arc::from(song_lua_proxy_local_children_legacy(
        segment.iter().cloned(),
    ))
}

#[cfg(feature = "bench-support")]
fn song_lua_proxy_local_children_legacy(children: impl Iterator<Item = Actor>) -> Vec<Actor> {
    let mut children = children.collect::<Vec<_>>();
    if children
        .iter()
        .any(|actor| matches!(actor, Actor::CameraPush { .. } | Actor::CameraPop))
    {
        let mut out = Vec::with_capacity(children.len());
        let mut run = Vec::new();
        for actor in children {
            if matches!(actor, Actor::CameraPush { .. } | Actor::CameraPop) {
                out.extend(song_lua_proxy_local_children_legacy(run.drain(..)));
                out.push(song_lua_proxy_local_actor(actor));
            } else {
                run.push(actor);
            }
        }
        out.extend(song_lua_proxy_local_children_legacy(run.drain(..)));
        return out;
    }
    children.sort_by_key(song_lua_proxy_actor_z);
    children
        .into_iter()
        .map(song_lua_proxy_local_actor)
        .collect()
}

#[cfg(feature = "bench-support")]
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
            tint,
            vertices,
            visible,
            blend,
            ..
        } => Actor::Mesh {
            align,
            offset,
            size,
            tint,
            vertices,
            visible,
            blend,
            z: 0,
        },
        Actor::ReusableMesh {
            align,
            offset,
            size,
            tint,
            vertices,
            visible,
            blend,
            ..
        } => Actor::ReusableMesh {
            align,
            offset,
            size,
            tint,
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
        Actor::ReusableTexturedMesh {
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
        } => Actor::ReusableTexturedMesh {
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
            children: song_lua_proxy_local_children_legacy(children.into_iter()),
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
            children: song_lua_proxy_source_segment_legacy(&children),
            background,
            z: 0,
            tint,
            blend,
        },
        Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            tint,
            blend,
            visible,
            ..
        } => Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            z: 0,
            tint,
            blend,
            visible,
        },
        Actor::Camera {
            view_proj,
            children,
        } => Actor::Camera {
            view_proj,
            children: song_lua_proxy_local_children_legacy(children.into_iter()),
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
    if parent_index.is_none()
        && let Some(static_order) = order_cache.static_root_order.as_deref()
    {
        out.extend_from_slice(static_order);
        return;
    }
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
    if list_idx >= order_cache.child_lists.len() || order_cache.child_lists[list_idx].is_empty() {
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
        let mut changed = order_cache.sort_modes[list_idx] != SONG_LUA_CHILD_ORDER_Z;
        for &idx in &order_cache.child_lists[list_idx] {
            let z = overlay_states
                .get(idx)
                .map_or(overlays[idx].initial_state.z, |state| state.z);
            let key = z.to_bits();
            if order_cache.last_z_keys[idx] != key {
                order_cache.last_z_keys[idx] = key;
                changed = true;
            }
        }
        if changed {
            order_cache.child_lists[list_idx].sort_by(|&left, &right| {
                let left_z = f32::from_bits(order_cache.last_z_keys[left]);
                let right_z = f32::from_bits(order_cache.last_z_keys[right]);
                left_z.total_cmp(&right_z).then_with(|| left.cmp(&right))
            });
        }
        order_cache.sort_modes[list_idx] = SONG_LUA_CHILD_ORDER_Z;
    } else if order_cache
        .dynamic_draw_order
        .get(list_idx)
        .copied()
        .unwrap_or(false)
    {
        let mut changed = order_cache.sort_modes[list_idx] != SONG_LUA_CHILD_ORDER_DRAW;
        for &idx in &order_cache.child_lists[list_idx] {
            let draw_order = overlay_states
                .get(idx)
                .map_or(overlays[idx].initial_state.draw_order, |state| {
                    state.draw_order
                });
            if order_cache.last_draw_orders[idx] != draw_order {
                order_cache.last_draw_orders[idx] = draw_order;
                changed = true;
            }
        }
        if changed {
            order_cache.child_lists[list_idx]
                .sort_by_key(|&idx| (order_cache.last_draw_orders[idx], idx));
        }
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
        let child_list_idx = song_lua_overlay_child_list_index(Some(idx));
        if order_cache
            .child_lists
            .get(child_list_idx)
            .is_some_and(|children| !children.is_empty())
        {
            song_lua_push_order(overlays, overlay_states, order_cache, Some(idx), out);
        }
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_push_order_legacy(
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
        song_lua_push_order_legacy(overlays, overlay_states, order_cache, Some(idx), out);
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

fn song_lua_capture_children_into(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    asset_manager: &AssetManager,
    capture_index: usize,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    mut proxy_actor_scratch: Option<&mut SongLuaProxyActorScratch>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    capture_states: &mut Vec<SongLuaOverlayState>,
    order_scratch: &mut Vec<usize>,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
) {
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
        if topology_index
            .aft_ancestors
            .get(idx)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
            != Some(capture_index)
        {
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
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                if let Some(actor) =
                    song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                        song_lua_build_proxy_frame_actor_with_scratch(
                            overlay_state,
                            draw_idx.min(i16::MAX as usize) as i16,
                            source,
                            overlay_space_width,
                            overlay_space_height,
                            proxy_actor_scratch.as_deref_mut(),
                        )
                    })
                {
                    out.push(actor);
                }
            }
            _ => {
                let z = draw_idx.min(i16::MAX as usize) as i16;
                if append_song_lua_multi_actor_overlay(
                    out,
                    overlay,
                    overlay_state,
                    asset_manager,
                    z,
                    overlay_space_width,
                    overlay_space_height,
                    0.0,
                    0.0,
                    0.0,
                    projected_mesh_scratch.get_mut(idx),
                )
                .is_none()
                    && let Some(actors) = build_song_lua_overlay_actor_with_scratch(
                        overlay,
                        overlay_state,
                        topology_index.camera_state(capture_states, idx),
                        asset_manager,
                        z,
                        overlay_space_width,
                        overlay_space_height,
                        0.0,
                        0.0,
                        0.0,
                        projected_mesh_scratch.get_mut(idx),
                    )
                {
                    out.extend(actors);
                }
            }
        }
    }
}

#[cfg(test)]
fn song_lua_capture_children(
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    local_overlay_states: &[SongLuaOverlayState],
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &SongLuaOverlayTopologyIndex,
    asset_manager: &AssetManager,
    capture_index: usize,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    overlay_space_width: f32,
    overlay_space_height: f32,
    capture_states: &mut Vec<SongLuaOverlayState>,
    order_scratch: &mut Vec<usize>,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
) -> Vec<Actor> {
    let mut out = Vec::new();
    song_lua_capture_children_into(
        &mut out,
        overlays,
        overlay_states,
        local_overlay_states,
        order_cache,
        topology_index,
        asset_manager,
        capture_index,
        proxy_sources,
        None,
        overlay_space_width,
        overlay_space_height,
        capture_states,
        order_scratch,
        projected_mesh_scratch,
    );
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

fn song_lua_overlay_apply_blocks_cached(
    state: SongLuaOverlayState,
    blocks: &[SongLuaOverlayCommandBlock],
    elapsed: f32,
    next_block: &mut usize,
    block_state: &mut SongLuaOverlayState,
    last_elapsed: &mut f32,
) -> SongLuaOverlayState {
    if !elapsed.is_finite() {
        return state;
    }
    if elapsed < *last_elapsed {
        *next_block = 0;
        *block_state = state;
    }
    *last_elapsed = elapsed;

    while let Some(block) = blocks.get(*next_block) {
        if elapsed < block.start {
            break;
        }
        if block.duration <= f32::EPSILON || elapsed >= block.start + block.duration {
            apply_song_lua_overlay_delta(block_state, &block.delta);
            *next_block += 1;
            continue;
        }
        let target = song_lua_overlay_state_with_delta(*block_state, &block.delta);
        let t = song_lua_ease_factor(
            block.easing.as_deref(),
            ((elapsed - block.start) / block.duration).clamp(0.0, 1.0),
            block.opt1,
            block.opt2,
        );
        return song_lua_overlay_state_lerp(*block_state, target, t, &block.delta);
    }
    *block_state
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
        debug_assert_eq!(ease.overlay_index, overlay_index);
        // Grouped ease ranges are sorted by start time.
        if now < ease.start_second {
            break;
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

#[cfg(any(test, feature = "bench-support"))]
fn apply_song_lua_overlay_runtime_eases_legacy(
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
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let events = overlay_events.get(overlay_index).map(Vec::as_slice);
    let has_events = events.is_some_and(|events| !events.is_empty());
    let has_eases = overlay_ease_ranges
        .get(overlay_index)
        .is_some_and(|range| !range.is_empty());
    if !has_events && !has_eases {
        return overlay.initial_state;
    }
    song_lua_overlay_render_state_dynamic(
        now,
        overlay_index,
        overlay,
        events,
        overlay_eases,
        overlay_ease_ranges,
        message_cache,
    )
}

fn song_lua_overlay_render_state_dynamic(
    now: f32,
    overlay_index: usize,
    overlay: &SongLuaOverlayActor,
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let current = song_lua_message_state_cached(
        now,
        overlay.initial_state,
        &overlay.message_commands,
        events,
        message_cache,
    );
    apply_song_lua_overlay_runtime_eases_for(
        now,
        overlay_index,
        overlay_eases,
        overlay_ease_ranges,
        current,
    )
}

#[cfg(feature = "bench-support")]
fn song_lua_overlay_render_state_from_legacy(
    now: f32,
    overlay_index: usize,
    overlay: &SongLuaOverlayActor,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    overlay_ease_ranges: &[std::ops::Range<usize>],
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    song_lua_overlay_render_state_dynamic(
        now,
        overlay_index,
        overlay,
        overlay_events.get(overlay_index).map(Vec::as_slice),
        overlay_eases,
        overlay_ease_ranges,
        message_cache,
    )
}

fn song_lua_message_state_legacy(
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

fn song_lua_message_state_cached(
    now: f32,
    initial_state: SongLuaOverlayState,
    message_commands: &[SongLuaOverlayMessageCommand],
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let Some(events) = events else {
        cache.reset(initial_state);
        return initial_state;
    };
    if !now.is_finite() {
        return song_lua_message_state_legacy(now, initial_state, message_commands, Some(events));
    }
    if !cache.initialized || now < cache.processed_until {
        cache.reset(initial_state);
    }

    while let Some(event) = events.get(cache.next_event) {
        if event.event_second > now {
            break;
        }
        cache.next_event += 1;
        cache.processed_until = event.event_second;
        if message_commands.get(event.command_index).is_none() {
            continue;
        }
        if let Some(active_command_index) = cache.active_command_index
            && let Some(active_command) = message_commands.get(active_command_index)
        {
            let command_base = cache.base_state;
            cache.base_state = song_lua_overlay_apply_blocks_cached(
                command_base,
                &active_command.blocks,
                event.event_second - cache.active_start_second,
                &mut cache.active_next_block,
                &mut cache.active_block_state,
                &mut cache.active_last_elapsed,
            );
        }
        cache.active_command_index = Some(event.command_index);
        cache.active_start_second = event.event_second;
        cache.reset_active_blocks(cache.base_state);
    }

    let Some(command) = cache
        .active_command_index
        .and_then(|command_index| message_commands.get(command_index))
    else {
        return cache.base_state;
    };
    song_lua_overlay_apply_blocks_cached(
        cache.base_state,
        &command.blocks,
        now - cache.active_start_second,
        &mut cache.active_next_block,
        &mut cache.active_block_state,
        &mut cache.active_last_elapsed,
    )
}

fn song_lua_player_render_state(
    state: &State,
    player_index: usize,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let song_lua_visuals = state.song_lua_visuals();
    let Some(actor) = song_lua_visuals.player_actors.get(player_index) else {
        return SongLuaOverlayState::default();
    };
    song_lua_captured_actor_state_from(
        state.current_music_time_display(),
        actor,
        song_lua_visuals
            .player_events
            .get(player_index)
            .map(Vec::as_slice),
        message_cache,
    )
}

fn song_lua_song_foreground_state_from(
    now: f32,
    song_foreground: &SongLuaCapturedActor,
    events: &[SongLuaOverlayMessageRuntime],
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    song_lua_captured_actor_state_from(now, song_foreground, Some(events), message_cache)
}

fn song_lua_captured_actor_state_from(
    now: f32,
    actor: &SongLuaCapturedActor,
    events: Option<&[SongLuaOverlayMessageRuntime]>,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    if events.is_none_or(<[_]>::is_empty) {
        message_cache.initialized = false;
        return actor.initial_state;
    }
    song_lua_message_state_cached(
        now,
        actor.initial_state,
        &actor.message_commands,
        events,
        message_cache,
    )
}

fn song_lua_song_foreground_state(
    state: &State,
    message_cache: &mut SongLuaMessageStateCache,
) -> SongLuaOverlayState {
    let song_lua_visuals = state.song_lua_visuals();
    song_lua_song_foreground_state_from(
        state.current_music_time_display(),
        &song_lua_visuals.song_foreground,
        song_lua_visuals.song_foreground_events.as_slice(),
        message_cache,
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
            tint: actor_tint,
            vertices,
            visible,
            blend: actor_blend,
            z,
        } => Actor::Mesh {
            align,
            offset,
            size,
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            vertices,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
        Actor::ReusableMesh {
            align,
            offset,
            size,
            tint,
            vertices,
            visible,
            blend: actor_blend,
            z,
        } => Actor::ReusableMesh {
            align,
            offset,
            size,
            tint: song_lua_capture_tint(tint, capture_tint),
            vertices,
            visible,
            blend: blend.unwrap_or(actor_blend),
            z: song_lua_add_z(z, z_shift),
        },
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
        Actor::ReusableTexturedMesh {
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
        } => Actor::ReusableTexturedMesh {
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
        Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            z,
            tint: actor_tint,
            blend: actor_blend,
            visible,
        } => Actor::RetainedFrame {
            align,
            offset,
            size,
            frame,
            z: song_lua_add_z(z, z_shift),
            tint: song_lua_capture_tint(actor_tint, capture_tint),
            blend: blend.or(actor_blend),
            visible,
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
        Actor::Shadow {
            len,
            color,
            mut child,
        } => {
            let actor = std::mem::replace(child.as_mut(), Actor::CameraPop);
            *child = song_lua_style_capture_actor(actor, capture_tint, blend, z_shift);
            Actor::Shadow {
                len,
                color: song_lua_capture_tint(color, capture_tint),
                child,
            }
        }
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

#[cfg(any(test, feature = "bench-support"))]
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
    let peers = overlays
        .iter()
        .enumerate()
        .filter_map(|(candidate_index, candidate)| {
            let SongLuaOverlayKind::AftSprite {
                capture_name: candidate_capture,
            } = &candidate.kind
            else {
                return None;
            };
            candidate_capture
                .eq_ignore_ascii_case(capture_name)
                .then_some(candidate_index)
        });
    let group = song_lua_rgb_aft_channels_from_peers(overlay_states, index, peers)?;
    let leader = draw_order
        .iter()
        .copied()
        .find(|idx| group.contains(idx))
        .unwrap_or(index);
    Some((leader, group))
}

fn song_lua_rgb_aft_group_from_peers(
    overlay_states: &[SongLuaOverlayState],
    index: usize,
    peers: impl IntoIterator<Item = usize>,
    draw_positions: &[usize],
) -> Option<(usize, [usize; 3])> {
    let group = song_lua_rgb_aft_channels_from_peers(overlay_states, index, peers)?;
    let leader = group
        .iter()
        .copied()
        .min_by_key(|index| draw_positions.get(*index).copied().unwrap_or(usize::MAX))
        .unwrap_or(index);
    Some((leader, group))
}

fn song_lua_rgb_aft_channels_from_peers(
    overlay_states: &[SongLuaOverlayState],
    index: usize,
    peers: impl IntoIterator<Item = usize>,
) -> Option<[usize; 3]> {
    let state = overlay_states.get(index).copied().unwrap_or_default();
    let channel = song_lua_rgb_aft_channel(state)?;
    let norm = song_lua_rgb_aft_norm_state(state);
    let mut group = [usize::MAX; 3];
    group[channel] = index;
    for idx in peers {
        if idx == index {
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
    Some(group)
}

fn song_lua_combined_rgb_aft_state(mut state: SongLuaOverlayState) -> SongLuaOverlayState {
    // ITGmania blends the finished AFT texture, not each captured actor.
    // Three aligned R/G/B additive sprites reconstruct that texture exactly,
    // so the render-target approximation should keep child blend modes intact.
    state.diffuse = [1.0, 1.0, 1.0, state.diffuse[3]];
    state.blend = SongLuaOverlayBlendMode::Alpha;
    state
}

fn song_lua_shift_capture_z(actor: &mut Actor, z_shift: i16) {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::SharedFrame { z, .. }
        | Actor::RetainedFrame { z, .. } => *z = song_lua_add_z(*z, z_shift),
        Actor::Frame { children, z, .. } => {
            *z = song_lua_add_z(*z, z_shift);
            for child in children {
                song_lua_shift_capture_z(child, z_shift);
            }
        }
        Actor::Camera { children, .. } => {
            for child in children {
                song_lua_shift_capture_z(child, z_shift);
            }
        }
        Actor::Shadow { child, .. } => song_lua_shift_capture_z(child, z_shift),
        Actor::CameraPush { .. } | Actor::CameraPop => {}
    }
}

fn song_lua_build_shared_capture(
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    scratch: &mut SharedActorFrameScratch,
    fill: impl FnOnce(&mut Vec<Actor>),
) -> Option<Actor> {
    if !state.visible || state.diffuse[3] <= f32::EPSILON {
        return None;
    }
    let blend = match state.blend {
        SongLuaOverlayBlendMode::Alpha => None,
        SongLuaOverlayBlendMode::Add => Some(BlendMode::Add),
        SongLuaOverlayBlendMode::Multiply => Some(BlendMode::Multiply),
        SongLuaOverlayBlendMode::Subtract => Some(BlendMode::Subtract),
    };
    let extra_offset = song_lua_capture_channel_offset(
        overlay.name.as_deref(),
        state,
        overlay_space_width,
        overlay_space_height,
    );
    let view_proj = song_lua_capture_transform_matrix(
        state,
        extra_offset,
        overlay_space_width,
        overlay_space_height,
    )
    .map(|transform| {
        glam::camera::rh::proj::opengl::orthographic(
            -0.5 * screen_width(),
            0.5 * screen_width(),
            -0.5 * screen_height(),
            0.5 * screen_height(),
            -1.0,
            1.0,
        ) * transform
    });
    let offset = view_proj.map_or(extra_offset, |_| [0.0, 0.0]);
    let children = scratch.refill(offset, |out| {
        if let Some(view_proj) = view_proj {
            out.push(Actor::CameraPush { view_proj });
        }
        let source_start = out.len();
        fill(out);
        if out.len() == source_start {
            out.clear();
            return;
        }
        for actor in &mut out[source_start..] {
            song_lua_shift_capture_z(actor, z);
        }
        if view_proj.is_some() {
            out.push(Actor::CameraPop);
        }
    })?;
    Some(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children,
        background: None,
        z: 0,
        tint: state.diffuse,
        blend,
    })
}

#[cfg(any(test, feature = "bench-support"))]
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
            view_proj: glam::camera::rh::proj::opengl::orthographic(
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
    song_lua_rainbow_scroll_attributes_at_phase(text, song_lua_rainbow_scroll_phase(total_elapsed))
}

fn song_lua_rainbow_scroll_attributes_at_phase(
    text: &str,
    first_color: usize,
) -> Vec<TextAttribute> {
    let char_count = text.chars().count();
    let mut out = Vec::with_capacity(char_count);
    append_song_lua_rainbow_scroll_attributes_at_phase(text, first_color, &mut out);
    out
}

fn append_song_lua_rainbow_scroll_attributes_at_phase(
    text: &str,
    first_color: usize,
    out: &mut Vec<TextAttribute>,
) {
    let char_count = text.chars().count();
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
}

#[inline(always)]
fn song_lua_rainbow_scroll_phase(total_elapsed: f32) -> usize {
    ((total_elapsed / 0.2).floor() as usize) % SONG_LUA_TEXT_RAINBOW_COLORS.len()
}

fn song_lua_rainbow_scroll_phases(
    text: &str,
) -> [Arc<[TextAttribute]>; SONG_LUA_TEXT_RAINBOW_COLORS.len()] {
    std::array::from_fn(|phase| {
        Arc::from(song_lua_rainbow_scroll_attributes_at_phase(text, phase).into_boxed_slice())
    })
}

fn song_lua_transparent_text_attributes(
    text: &str,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> TextAttributes {
    let char_count = text.chars().count();
    if char_count == 0 {
        return TextAttributes::default();
    }
    let fill = |out: &mut Vec<TextAttribute>| {
        out.push(TextAttribute {
            start: 0,
            length: char_count,
            color: [1.0, 1.0, 1.0, 0.0],
            vertex_colors: None,
            glow: None,
        });
    };
    if let Some(scratch) = scratch {
        scratch.update_text_glow(fill)
    } else {
        let mut out = Vec::with_capacity(1);
        fill(&mut out);
        out.into()
    }
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
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> TextAttributes {
    let char_count = text.chars().count();
    if char_count == 0 {
        return TextAttributes::default();
    }
    let fill = |out: &mut Vec<TextAttribute>| {
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
    };
    if let Some(scratch) = scratch {
        scratch.update_text_glow(fill)
    } else {
        let mut out = Vec::with_capacity(attributes.len() + usize::from(glow[3] > f32::EPSILON));
        fill(&mut out);
        out.into()
    }
}

fn song_lua_text_attributes_for_diffuse_mode(
    attributes: &Arc<[TextAttribute]>,
    color: [f32; 4],
    text: &str,
    mult_attrs_with_diffuse: bool,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> (TextAttributes, [f32; 4]) {
    if attributes.is_empty() || mult_attrs_with_diffuse {
        return (TextAttributes::from(Arc::clone(attributes)), color);
    }
    let char_count = text.chars().count();
    if char_count == 0 {
        return (TextAttributes::from(Arc::clone(attributes)), color);
    }
    if color
        .iter()
        .all(|component| (*component - 1.0).abs() <= f32::EPSILON)
    {
        return (
            TextAttributes::from(Arc::clone(attributes)),
            [1.0, 1.0, 1.0, 1.0],
        );
    }
    let fill = |out: &mut Vec<TextAttribute>| {
        out.push(TextAttribute {
            start: 0,
            length: char_count,
            color,
            vertex_colors: None,
            glow: None,
        });
        out.extend_from_slice(attributes);
    };
    let attributes = if let Some(scratch) = scratch {
        scratch.update_text_diffuse(fill)
    } else {
        let mut out = Vec::with_capacity(attributes.len() + 1);
        fill(&mut out);
        out.into()
    };
    (attributes, [1.0, 1.0, 1.0, 1.0])
}

#[cfg(any(test, feature = "bench-support"))]
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

#[cfg(any(test, feature = "bench-support"))]
fn song_lua_overlay_camera_state_indexed(
    overlay_states: &[SongLuaOverlayState],
    topology_index: &SongLuaOverlayTopologyIndex,
    overlay_index: usize,
) -> Option<SongLuaOverlayState> {
    let mut candidate = topology_index
        .camera_ancestors
        .get(overlay_index)
        .copied()
        .and_then(SongLuaOverlayIndex::get);
    while let Some(current) = candidate {
        let state = overlay_states.get(current).copied()?;
        if state.fov.is_some() {
            return Some(state);
        }
        candidate = topology_index
            .camera_ancestors
            .get(current)
            .copied()
            .and_then(SongLuaOverlayIndex::get);
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
    let proj = glam::camera::rh::proj::opengl::frustum(
        (vanish_x - 0.5 * width) / dist,
        (vanish_x + 0.5 * width) / dist,
        (vanish_y + 0.5 * height) / dist,
        (vanish_y - 0.5 * height) / dist,
        1.0,
        dist + 1000.0,
    );
    let eye_x = -vanish_x + 0.5 * width;
    let eye_y = -vanish_y + 0.5 * height;
    let view = glam::camera::rh::view::look_at_mat4(
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
    append_song_lua_actor_multi_vertex_mesh(
        &mut out,
        vertices,
        tint,
        x_scale,
        y_scale,
        actor_scale,
        effect_scale,
        rotation_z_deg,
        skew,
    );
    Arc::from(out.into_boxed_slice())
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_actor_multi_vertex_mesh(
    out: &mut Vec<MeshVertex>,
    vertices: &[SongLuaOverlayMeshVertex],
    tint: [f32; 4],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) {
    out.reserve(vertices.len());
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
    append_song_lua_actor_multi_vertex_textured_mesh(
        &mut out,
        vertices,
        x_scale,
        y_scale,
        actor_scale,
        effect_scale,
        rotation_z_deg,
        skew,
    );
    Arc::from(out.into_boxed_slice())
}

#[allow(clippy::too_many_arguments)]
fn append_song_lua_actor_multi_vertex_textured_mesh(
    out: &mut Vec<TexturedMeshVertex>,
    vertices: &[SongLuaOverlayMeshVertex],
    x_scale: f32,
    y_scale: f32,
    actor_scale: [f32; 2],
    effect_scale: [f32; 3],
    rotation_z_deg: f32,
    skew: [f32; 2],
) {
    out.reserve(vertices.len());
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

#[allow(clippy::too_many_arguments)]
fn append_song_lua_model_actors(
    out: &mut impl Extend<Actor>,
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
    prewarmed_geometry_keys: Option<&[TMeshCacheKey]>,
    prewarmed_glow_vertices: Option<&[Arc<[TexturedMeshVertex]>]>,
) -> bool {
    let mut emitted = false;
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
            geom_cache_key: prewarmed_geometry_keys
                .and_then(|keys| keys.get(idx))
                .copied()
                .unwrap_or(INVALID_TMESH_CACHE_KEY),
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
        let glow_actor = song_lua_overlay_glow_actor_with_static_vertices(
            &actor,
            glow,
            state.text_glow_mode,
            None,
            prewarmed_glow_vertices.and_then(|vertices| vertices.get(idx)),
        );
        out.extend([actor]);
        emitted = true;
        if let Some(glow_actor) = glow_actor {
            out.extend([glow_actor]);
        }
    }
    emitted
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

#[allow(clippy::too_many_arguments)]
fn append_song_lua_noteskin_actors(
    out: &mut impl Extend<Actor>,
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
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> bool {
    let mut emitted = false;
    let (mut model_cache, glow_vertices) = match scratch {
        Some(scratch) => (
            scratch.noteskin_model_cache.as_mut(),
            scratch.noteskin_glow_vertices.as_deref(),
        ),
        None => (None, None),
    };
    let center = [
        state.x * x_scale + effect_offset[0] * x_scale,
        state.y * y_scale + effect_offset[1] * y_scale,
    ];
    for (idx, slot) in slots.iter().enumerate() {
        if !asset_manager.has_texture_key(slot.texture_key()) {
            continue;
        }
        let mut draw = model_cache.as_deref_mut().map_or_else(
            || slot.model_draw_at(total_elapsed, effect_beat),
            |cache| cache.draw_at(slot, total_elapsed, effect_beat),
        );
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
            if let Some(cache) = model_cache.as_deref_mut() {
                noteskin_model_actor_from_draw_cached(
                    slot,
                    draw,
                    center,
                    size,
                    uv,
                    -(slot.def.rotation_deg as f32 + effect_rot[2]),
                    tint,
                    blend,
                    layer_z,
                    cache,
                )
            } else {
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
            }
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
        let glow_actor = song_lua_overlay_glow_actor_with_static_vertices(
            &actor,
            glow,
            state.text_glow_mode,
            None,
            glow_vertices
                .and_then(|vertices| vertices.get(idx))
                .and_then(Option::as_ref),
        );
        out.extend([actor]);
        emitted = true;
        if let Some(glow_actor) = glow_actor {
            out.extend([glow_actor]);
        }
    }
    emitted
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
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
) -> Option<SongLuaActorList> {
    let mut out = SongLuaActorList::new();
    append_song_lua_noteskin_actors(
        &mut out,
        slots,
        state,
        asset_manager,
        z,
        x_scale,
        y_scale,
        actor_scale,
        effect_scale,
        effect_rot,
        effect_offset,
        tint,
        glow,
        blend,
        total_elapsed,
        effect_beat,
        None,
    )
    .then_some(out)
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

#[allow(clippy::too_many_arguments)]
fn song_lua_graph_display_actor(
    state: SongLuaOverlayState,
    body_values: &Arc<[f32]>,
    body_state: SongLuaOverlayState,
    line_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
    mut scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    let reuse_graph = scratch
        .as_ref()
        .is_some_and(|scratch| scratch.graph_frame.is_some());
    if reuse_graph
        && let Some(frame) = scratch
            .as_deref_mut()
            .and_then(|scratch| scratch.graph_frame.as_mut())
    {
        // Release last frame's child mesh Arcs before refilling their buffers.
        frame.clear();
    }
    let mut children = SmallVec::<[Actor; 2]>::new();
    if let Some(body) = song_lua_graph_display_body_actor(
        state,
        body_values,
        body_state,
        size,
        x_scale,
        y_scale,
        z,
        if reuse_graph {
            scratch.as_deref_mut()
        } else {
            None
        },
    ) {
        children.push(body);
    }
    if let Some(line) = song_lua_graph_display_line_actor(
        state,
        body_values,
        line_state,
        size,
        x_scale,
        y_scale,
        z,
        if reuse_graph {
            scratch.as_deref_mut()
        } else {
            None
        },
    ) {
        children.push(line);
    }
    match children.len() {
        0 => None,
        1 => children.pop(),
        _ if reuse_graph => {
            let shared = scratch
                .and_then(|scratch| scratch.graph_frame.as_mut())
                .and_then(|frame| frame.refill([0.0, 0.0], |out| out.extend(children.drain(..))))
                .expect("visible GraphDisplay body and line must refill the shared frame");
            Some(Actor::SharedFrame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: shared,
                background: None,
                z: 0,
                tint: [1.0, 1.0, 1.0, 1.0],
                blend: None,
            })
        }
        _ => Some(Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: children.into_vec(),
            background: None,
            z: 0,
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn song_lua_graph_display_body_actor(
    state: SongLuaOverlayState,
    body_values: &[f32],
    body_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
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
    let geometry_key = [
        left.to_bits(),
        top.to_bits(),
        width.to_bits(),
        height.to_bits(),
        tint[0].to_bits(),
        tint[1].to_bits(),
        tint[2].to_bits(),
        tint[3].to_bits(),
        x_scale.to_bits(),
        y_scale.to_bits(),
    ];
    let fill = |vertices: &mut Vec<MeshVertex>| {
        for (index, pair) in values.windows(2).enumerate() {
            let x0 = left + width * index as f32 / (values.len() - 1) as f32;
            let x1 = left + width * (index + 1) as f32 / (values.len() - 1) as f32;
            let y0 = top + (1.0 - pair[0].clamp(0.0, 1.0)) * height;
            let y1 = top + (1.0 - pair[1].clamp(0.0, 1.0)) * height;
            push_graph_display_tri(
                vertices,
                [x0 * x_scale, y0 * y_scale],
                [x0 * x_scale, bottom * y_scale],
                [x1 * x_scale, bottom * y_scale],
                tint,
            );
            push_graph_display_tri(
                vertices,
                [x0 * x_scale, y0 * y_scale],
                [x1 * x_scale, bottom * y_scale],
                [x1 * x_scale, y1 * y_scale],
                tint,
            );
        }
    };
    let visible = state.visible && body_state.visible;
    let blend = if body_state.blend == SongLuaOverlayBlendMode::Alpha {
        song_lua_overlay_blend(state.blend)
    } else {
        song_lua_overlay_blend(body_state.blend)
    };
    Some(if let Some(scratch) = scratch {
        Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0, 1.0, 1.0, 1.0],
            vertices: scratch.update_graph_body(geometry_key, fill),
            visible,
            blend,
            z,
        }
    } else {
        let mut vertices = Vec::with_capacity((values.len().saturating_sub(1)) * 6);
        fill(&mut vertices);
        Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0; 4],
            vertices: Arc::from(vertices.into_boxed_slice()),
            visible,
            blend,
            z,
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn song_lua_graph_display_line_actor(
    state: SongLuaOverlayState,
    body_values: &[f32],
    line_state: SongLuaOverlayState,
    size: [f32; 2],
    x_scale: f32,
    y_scale: f32,
    z: i16,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
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
    let height = size[1] * graph_scale[1].abs();
    let y = top + height * 0.5 + line_state.y * graph_scale[1];
    let tint = [
        state.diffuse[0] * line_state.diffuse[0],
        state.diffuse[1] * line_state.diffuse[1],
        state.diffuse[2] * line_state.diffuse[2],
        state.diffuse[3] * line_state.diffuse[3],
    ];
    let stroke = line_height.max(1.0);
    let geometry_key = [
        left.to_bits(),
        y.to_bits(),
        width.to_bits(),
        height.to_bits(),
        stroke.to_bits(),
        tint[0].to_bits(),
        tint[1].to_bits(),
        tint[2].to_bits(),
        tint[3].to_bits(),
        x_scale.to_bits(),
        y_scale.to_bits(),
    ];
    let fill = |vertices: &mut Vec<MeshVertex>| {
        for (index, pair) in values.windows(2).enumerate() {
            let x0 = left + width * index as f32 / (values.len() - 1) as f32;
            let x1 = left + width * (index + 1) as f32 / (values.len() - 1) as f32;
            let y0 = y + (0.5 - pair[0].clamp(0.0, 1.0)) * height;
            let y1 = y + (0.5 - pair[1].clamp(0.0, 1.0)) * height;
            push_graph_display_line_segment(
                vertices,
                [x0 * x_scale, y0 * y_scale],
                [x1 * x_scale, y1 * y_scale],
                stroke * y_scale,
                tint,
            );
        }
    };
    let visible = state.visible && line_state.visible;
    let blend = if line_state.blend == SongLuaOverlayBlendMode::Alpha {
        song_lua_overlay_blend(state.blend)
    } else {
        song_lua_overlay_blend(line_state.blend)
    };
    Some(if let Some(scratch) = scratch {
        Actor::ReusableMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0, 1.0, 1.0, 1.0],
            vertices: scratch.update_graph_line(geometry_key, fill),
            visible,
            blend,
            z,
        }
    } else {
        let mut vertices = Vec::with_capacity((values.len().saturating_sub(1)) * 6);
        fill(&mut vertices);
        Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            tint: [1.0; 4],
            vertices: Arc::from(vertices.into_boxed_slice()),
            visible,
            blend,
            z,
        }
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

fn song_lua_projected_overlay_axis_slices(start_fade: f32, end_fade: f32) -> SmallVec<[f32; 4]> {
    let mut out: SmallVec<[f32; 4]> = SmallVec::new();
    out.push(0.0);
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

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaMessageStateBenchmark {
    initial_state: SongLuaOverlayState,
    commands: Vec<SongLuaOverlayMessageCommand>,
    events: Vec<SongLuaOverlayMessageRuntime>,
    cache: SongLuaMessageStateCache,
}

#[cfg(feature = "bench-support")]
impl SongLuaMessageStateBenchmark {
    pub fn new(event_count: usize) -> Self {
        let command = SongLuaOverlayMessageCommand {
            message: "Tick".to_string(),
            blocks: vec![
                SongLuaOverlayCommandBlock {
                    start: 0.0,
                    duration: 0.05,
                    easing: Some("inOutQuad".to_string()),
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        x: Some(100.0),
                        y: Some(-25.0),
                        ..SongLuaOverlayStateDelta::default()
                    },
                },
                SongLuaOverlayCommandBlock {
                    start: 0.05,
                    duration: 0.0,
                    easing: None,
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        draw_order: Some(7),
                        ..SongLuaOverlayStateDelta::default()
                    },
                },
            ],
        };
        Self {
            initial_state: SongLuaOverlayState::default(),
            commands: vec![command],
            events: (0..event_count)
                .map(|index| SongLuaOverlayMessageRuntime {
                    event_second: index as f32 * 0.125,
                    command_index: 0,
                })
                .collect(),
            cache: SongLuaMessageStateCache::default(),
        }
    }

    pub fn long_command(block_count: usize) -> Self {
        let command = SongLuaOverlayMessageCommand {
            message: "LongCommand".to_string(),
            blocks: (0..block_count)
                .map(|index| SongLuaOverlayCommandBlock {
                    start: index as f32 * 0.01,
                    duration: 0.005,
                    easing: Some("inOutQuad".to_string()),
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        x: Some(index as f32),
                        y: (index % 8 == 0).then_some(-(index as f32)),
                        ..SongLuaOverlayStateDelta::default()
                    },
                })
                .collect(),
        };
        Self {
            initial_state: SongLuaOverlayState::default(),
            commands: vec![command],
            events: vec![SongLuaOverlayMessageRuntime {
                event_second: 0.0,
                command_index: 0,
            }],
            cache: SongLuaMessageStateCache::default(),
        }
    }

    pub fn legacy_frame(&self, now: f32) -> f32 {
        let state = song_lua_message_state_legacy(
            now,
            self.initial_state,
            &self.commands,
            Some(&self.events),
        );
        state.x + state.y + state.z + state.draw_order as f32
    }

    pub fn cached_frame(&mut self, now: f32) -> f32 {
        let state = song_lua_message_state_cached(
            now,
            self.initial_state,
            &self.commands,
            Some(&self.events),
            &mut self.cache,
        );
        state.x + state.y + state.z + state.draw_order as f32
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaStaticStateBenchmark {
    overlay: SongLuaOverlayActor,
    events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    ranges: Vec<std::ops::Range<usize>>,
    legacy_cache: SongLuaMessageStateCache,
    fast_cache: SongLuaMessageStateCache,
}

#[cfg(feature = "bench-support")]
impl SongLuaStaticStateBenchmark {
    pub fn new() -> Self {
        Self {
            overlay: SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Actor,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    x: 123.0,
                    y: -45.0,
                    diffuse: [0.25, 0.5, 0.75, 0.875],
                    draw_order: 17,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            },
            events: vec![Vec::new()],
            ranges: vec![0..0],
            legacy_cache: SongLuaMessageStateCache::default(),
            fast_cache: SongLuaMessageStateCache::default(),
        }
    }

    pub fn legacy_frame(&mut self, now: f32) -> u64 {
        let state = song_lua_overlay_render_state_from_legacy(
            now,
            0,
            &self.overlay,
            &self.events,
            &[],
            &self.ranges,
            &mut self.legacy_cache,
        );
        song_lua_overlay_state_checksum(state)
    }

    pub fn static_frame(&mut self, now: f32) -> u64 {
        let state = song_lua_overlay_render_state_from(
            now,
            0,
            &self.overlay,
            &self.events,
            &[],
            &self.ranges,
            &mut self.fast_cache,
        );
        song_lua_overlay_state_checksum(state)
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaNoScriptBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime>,
    overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    overlay_states: Vec<SongLuaOverlayState>,
    actor_events: Vec<SongLuaOverlayMessageRuntime>,
    order_cache: SongLuaOverlayOrderCache,
    legacy_state_caches: Vec<SongLuaMessageStateCache>,
    fast_state_caches: Vec<SongLuaMessageStateCache>,
    legacy_local_states: Vec<SongLuaOverlayState>,
    fast_local_states: Vec<SongLuaOverlayState>,
    legacy_states: Vec<SongLuaOverlayState>,
    fast_states: Vec<SongLuaOverlayState>,
    proxy_index: SongLuaProxyRequestIndex,
    legacy_visit: SongLuaCaptureVisitScratch,
    fast_visit: SongLuaCaptureVisitScratch,
    actor: SongLuaCapturedActor,
    legacy_message_caches: [SongLuaMessageStateCache; 3],
    fast_message_caches: [SongLuaMessageStateCache; 3],
}

#[cfg(feature = "bench-support")]
impl SongLuaNoScriptBenchmark {
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
            overlay_events: Vec::new(),
            overlay_eases: Vec::new(),
            overlay_ease_ranges: Vec::new(),
            overlay_states: Vec::new(),
            actor_events: Vec::new(),
            order_cache: SongLuaOverlayOrderCache::default(),
            legacy_state_caches: Vec::new(),
            fast_state_caches: Vec::new(),
            legacy_local_states: Vec::new(),
            fast_local_states: Vec::new(),
            legacy_states: Vec::new(),
            fast_states: Vec::new(),
            proxy_index: SongLuaProxyRequestIndex::new(&[]),
            legacy_visit: SongLuaCaptureVisitScratch::with_capacity(0),
            fast_visit: SongLuaCaptureVisitScratch::with_capacity(0),
            actor: SongLuaCapturedActor {
                initial_state: SongLuaOverlayState {
                    x: 123.0,
                    y: -45.0,
                    draw_order: 17,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            },
            legacy_message_caches: std::array::from_fn(|_| SongLuaMessageStateCache::default()),
            fast_message_caches: std::array::from_fn(|_| SongLuaMessageStateCache::default()),
        }
    }

    pub fn legacy_state_frame(&mut self, now: f32) -> u64 {
        song_lua_overlay_state_sets_active_into(
            now,
            &self.overlays,
            &self.overlay_events,
            &self.overlay_eases,
            &self.overlay_ease_ranges,
            640.0,
            480.0,
            &self.order_cache,
            &mut self.legacy_state_caches,
            &mut self.legacy_local_states,
            &mut self.legacy_states,
        );
        self.legacy_state_caches.len() as u64
            | ((self.legacy_local_states.len() as u64) << 16)
            | ((self.legacy_states.len() as u64) << 32)
    }

    pub fn fast_state_frame(&mut self, now: f32) -> u64 {
        song_lua_overlay_state_sets_from_into(
            now,
            &self.overlays,
            &self.overlay_events,
            &self.overlay_eases,
            &self.overlay_ease_ranges,
            640.0,
            480.0,
            &self.order_cache,
            &mut self.fast_state_caches,
            &mut self.fast_local_states,
            &mut self.fast_states,
        );
        self.fast_state_caches.len() as u64
            | ((self.fast_local_states.len() as u64) << 16)
            | ((self.fast_states.len() as u64) << 32)
    }

    pub fn legacy_proxy_frame(&mut self) -> u64 {
        let requests = song_lua_proxy_requests_indexed_active(
            &self.overlays,
            &self.overlay_states,
            &self.proxy_index,
            &mut self.legacy_visit,
        );
        let players = song_lua_replacement_active_players_indexed_active(
            &self.overlays,
            &self.overlay_states,
            &[SongLuaPlayerProxySources::default(); 2],
            &self.proxy_index,
            &mut self.legacy_visit,
        );
        SongLuaProxyRequestBenchmark::checksum(requests)
            | (SongLuaCaptureTraversalBenchmark::player_checksum(players) << 16)
    }

    pub fn fast_proxy_frame(&mut self) -> u64 {
        let requests = song_lua_proxy_requests_indexed(
            &self.overlays,
            &self.overlay_states,
            &self.proxy_index,
            &mut self.fast_visit,
        );
        let players = song_lua_replacement_active_players_indexed(
            &self.overlays,
            &self.overlay_states,
            &[SongLuaPlayerProxySources::default(); 2],
            &self.proxy_index,
            &mut self.fast_visit,
        );
        SongLuaProxyRequestBenchmark::checksum(requests)
            | (SongLuaCaptureTraversalBenchmark::player_checksum(players) << 16)
    }

    pub fn legacy_message_frame(&mut self, now: f32) -> u64 {
        self.legacy_message_caches
            .iter_mut()
            .fold(0, |checksum, cache| {
                checksum.rotate_left(7)
                    ^ song_lua_overlay_state_checksum(song_lua_message_state_cached(
                        now,
                        self.actor.initial_state,
                        &self.actor.message_commands,
                        Some(&self.actor_events),
                        cache,
                    ))
            })
    }

    pub fn fast_message_frame(&mut self, now: f32) -> u64 {
        self.fast_message_caches
            .iter_mut()
            .fold(0, |checksum, cache| {
                checksum.rotate_left(7)
                    ^ song_lua_overlay_state_checksum(song_lua_captured_actor_state_from(
                        now,
                        &self.actor,
                        Some(&self.actor_events),
                        cache,
                    ))
            })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct GameplayInactiveFrameBenchmark {
    transition_name: String,
    transition: BackgroundTransition,
    transition_expired: Cell<bool>,
    layer2_events: Vec<SongLayer2Event>,
    layer2_cursor: Cell<usize>,
    overlays: Vec<SongLuaOverlayActor>,
    overlay_states: Vec<SongLuaOverlayState>,
    legacy_actors: Vec<Actor>,
    fast_actors: Vec<Actor>,
    legacy_order_cache: SongLuaOverlayOrderCache,
    fast_order_cache: SongLuaOverlayOrderCache,
    legacy_topology: SongLuaOverlayTopologyIndex,
    fast_topology: SongLuaOverlayTopologyIndex,
    legacy_order: Vec<usize>,
    fast_order: Vec<usize>,
    legacy_aft: SongLuaAftCaptureScratch,
    fast_aft: SongLuaAftCaptureScratch,
}

#[cfg(feature = "bench-support")]
impl GameplayInactiveFrameBenchmark {
    pub fn new(layer2_event_count: usize) -> Self {
        let layer2_events = (0..layer2_event_count)
            .map(|index| SongLayer2Event {
                start_second: index as f32 * 0.25,
                color: Some(if index.is_multiple_of(2) {
                    [1.0; 4]
                } else {
                    [1.0, 1.0, 160.0 / 255.0, 1.0]
                }),
            })
            .collect();
        Self {
            transition_name: "FadeCenterVertical".to_string(),
            transition: BackgroundTransition::FadeCenterVertical,
            transition_expired: Cell::new(false),
            layer2_events,
            layer2_cursor: Cell::new(0),
            overlays: Vec::new(),
            overlay_states: Vec::new(),
            legacy_actors: Vec::new(),
            fast_actors: Vec::new(),
            legacy_order_cache: SongLuaOverlayOrderCache::default(),
            fast_order_cache: SongLuaOverlayOrderCache::default(),
            legacy_topology: SongLuaOverlayTopologyIndex::default(),
            fast_topology: SongLuaOverlayTopologyIndex::default(),
            legacy_order: Vec::new(),
            fast_order: Vec::new(),
            legacy_aft: SongLuaAftCaptureScratch::default(),
            fast_aft: SongLuaAftCaptureScratch::default(),
        }
    }

    pub fn expired_transition_legacy(&self, now: f32) -> u64 {
        background_transition_frame_legacy(&self.transition_name, 0.0, now)
            .map_or(0, |(_, progress)| u64::from(progress.to_bits()))
    }

    pub fn expired_transition_compiled(&self, now: f32) -> u64 {
        background_transition_frame(Some(self.transition), &self.transition_expired, 0.0, now)
            .map_or(0, |(_, progress)| u64::from(progress.to_bits()))
    }

    pub fn expired_layer2_legacy(&self, now: f32) -> u64 {
        song_layer2_animation_legacy(&self.layer2_events, now)
            .map_or(0, |color| u64::from(color[3].to_bits()))
    }

    pub fn expired_layer2_cursor(&self, now: f32) -> u64 {
        song_layer2_animation_from(&self.layer2_events, &self.layer2_cursor, now)
            .map_or(0, |color| u64::from(color[3].to_bits()))
    }

    pub fn empty_layer_legacy(&mut self) -> u64 {
        let _ = prepare_active_song_lua_layer(
            &mut self.legacy_actors,
            &self.overlays,
            &self.overlay_states,
            SongLuaOverlayState::default(),
            &mut self.legacy_order_cache,
            &mut self.legacy_topology,
            &mut self.legacy_order,
            &mut self.legacy_aft,
        );
        self.legacy_actors.len() as u64 | ((self.legacy_order.len() as u64) << 32)
    }

    pub fn empty_layer_fast(&mut self) -> u64 {
        let _ = prepare_song_lua_layer(
            &mut self.fast_actors,
            &self.overlays,
            &self.overlay_states,
            SongLuaOverlayState::default(),
            &mut self.fast_order_cache,
            &mut self.fast_topology,
            &mut self.fast_order,
            &mut self.fast_aft,
        );
        self.fast_actors.len() as u64 | ((self.fast_order.len() as u64) << 32)
    }

    pub fn expired_layer2_now(&self) -> f32 {
        self.layer2_events
            .last()
            .map_or(1.0, |event| event.start_second + 1.0)
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaStatePlanBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    ranges: Vec<std::ops::Range<usize>>,
    plan: SongLuaOverlayOrderCache,
    legacy_caches: Vec<SongLuaMessageStateCache>,
    planned_caches: Vec<SongLuaMessageStateCache>,
    legacy_local: Vec<SongLuaOverlayState>,
    legacy_composed: Vec<SongLuaOverlayState>,
    planned_local: Vec<SongLuaOverlayState>,
    planned_composed: Vec<SongLuaOverlayState>,
}

#[cfg(feature = "bench-support")]
impl SongLuaStatePlanBenchmark {
    pub fn new(actor_count: usize) -> Self {
        let actor_count = actor_count.max(1);
        let mut events = Vec::with_capacity(actor_count);
        let overlays = (0..actor_count)
            .map(|index| {
                let group_start = index / 16 * 16;
                let dynamic = index == group_start && (index / 16).is_multiple_of(4);
                events.push(if dynamic {
                    vec![SongLuaOverlayMessageRuntime {
                        event_second: 0.0,
                        command_index: 0,
                    }]
                } else {
                    Vec::new()
                });
                SongLuaOverlayActor {
                    kind: if index == group_start {
                        SongLuaOverlayKind::ActorFrame
                    } else {
                        SongLuaOverlayKind::Quad
                    },
                    name: None,
                    parent_index: (index != group_start).then_some(group_start),
                    initial_state: SongLuaOverlayState {
                        x: index as f32 * 0.25,
                        y: -(index as f32) * 0.125,
                        diffuse: [0.75, 0.875, 1.0, 1.0],
                        ..SongLuaOverlayState::default()
                    },
                    message_commands: dynamic
                        .then(|| {
                            vec![SongLuaOverlayMessageCommand {
                                message: "Tick".to_string(),
                                blocks: vec![SongLuaOverlayCommandBlock {
                                    start: 0.0,
                                    duration: 1.0,
                                    easing: Some("inOutQuad".to_string()),
                                    opt1: None,
                                    opt2: None,
                                    delta: SongLuaOverlayStateDelta {
                                        x: Some(index as f32 + 100.0),
                                        diffuse: Some([0.5, 0.75, 1.0, 1.0]),
                                        ..SongLuaOverlayStateDelta::default()
                                    },
                                }],
                            }]
                        })
                        .unwrap_or_default(),
                }
            })
            .collect::<Vec<_>>();
        let ranges = vec![0..0; actor_count];
        let plan = song_lua_overlay_order_cache_from(&overlays, &[]);
        let (legacy_local, legacy_composed) =
            song_lua_overlay_initial_state_sets(&overlays, 640.0, 480.0);
        let (planned_local, planned_composed) =
            song_lua_overlay_initial_state_sets(&overlays, 640.0, 480.0);
        Self {
            overlays,
            events,
            ranges,
            plan,
            legacy_caches: vec![SongLuaMessageStateCache::default(); actor_count],
            planned_caches: vec![SongLuaMessageStateCache::default(); actor_count],
            legacy_local,
            legacy_composed,
            planned_local,
            planned_composed,
        }
    }

    pub fn legacy_frame(&mut self, now: f32) -> u64 {
        song_lua_overlay_local_states_all_into(
            now,
            &self.overlays,
            &self.events,
            &[],
            &self.ranges,
            &mut self.legacy_caches,
            &mut self.legacy_local,
        );
        song_lua_overlay_states_from_local_all_into(
            &self.overlays,
            &self.legacy_local,
            640.0,
            480.0,
            &mut self.legacy_composed,
        );
        Self::checksum(&self.legacy_composed)
    }

    pub fn planned_frame(&mut self, now: f32) -> u64 {
        song_lua_overlay_state_sets_from_into(
            now,
            &self.overlays,
            &self.events,
            &[],
            &self.ranges,
            640.0,
            480.0,
            &self.plan,
            &mut self.planned_caches,
            &mut self.planned_local,
            &mut self.planned_composed,
        );
        Self::checksum(&self.planned_composed)
    }

    pub fn planned_always_compose_frame(&mut self, now: f32) -> u64 {
        let _ = song_lua_overlay_local_states_into(
            now,
            &self.overlays,
            &self.events,
            &[],
            &self.ranges,
            &self.plan.dynamic_local_indices,
            &mut self.planned_caches,
            &mut self.planned_local,
        );
        song_lua_overlay_states_from_local_into(
            &self.overlays,
            &self.planned_local,
            &self.plan.dynamic_composed_indices,
            640.0,
            480.0,
            &mut self.planned_composed,
        );
        Self::checksum(&self.planned_composed)
    }

    fn checksum(states: &[SongLuaOverlayState]) -> u64 {
        states.iter().fold(0, |checksum, state| {
            checksum.rotate_left(7) ^ song_lua_overlay_state_checksum(*state)
        })
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_overlay_state_checksum(state: SongLuaOverlayState) -> u64 {
    u64::from(state.x.to_bits())
        ^ u64::from(state.y.to_bits()).rotate_left(11)
        ^ u64::from(state.diffuse[3].to_bits()).rotate_left(23)
        ^ (state.draw_order as u32 as u64).rotate_left(37)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaEaseBenchmark {
    eases: Vec<SongLuaOverlayEaseWindowRuntime>,
    ranges: Vec<std::ops::Range<usize>>,
}

#[cfg(feature = "bench-support")]
impl SongLuaEaseBenchmark {
    pub fn new(future_ease_count: usize) -> Self {
        let eases = (0..future_ease_count)
            .map(|index| {
                let start_second = 10.0 + index as f32 * 0.25;
                let delta = |x| SongLuaRuntimeOverlayStateDelta {
                    overlap_mask: 1,
                    delta: SongLuaOverlayStateDelta {
                        x: Some(x),
                        ..SongLuaOverlayStateDelta::default()
                    },
                };
                SongLuaOverlayEaseWindowRuntime {
                    overlay_index: 0,
                    start_second,
                    end_second: start_second + 0.1,
                    sustain_end_second: f32::MAX,
                    cutoff_second: None,
                    from: delta(index as f32),
                    to: delta(index as f32 + 1.0),
                    easing: Some("inOutQuad".to_string()),
                    opt1: None,
                    opt2: None,
                }
            })
            .collect::<Vec<_>>();
        Self {
            ranges: vec![0..eases.len()],
            eases,
        }
    }

    pub fn legacy_frame(&self, now: f32) -> f32 {
        apply_song_lua_overlay_runtime_eases_legacy(
            now,
            0,
            &self.eases,
            &self.ranges,
            SongLuaOverlayState::default(),
        )
        .x
    }

    pub fn bounded_frame(&self, now: f32) -> f32 {
        apply_song_lua_overlay_runtime_eases_for(
            now,
            0,
            &self.eases,
            &self.ranges,
            SongLuaOverlayState::default(),
        )
        .x
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaProxyRequestBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    states: Vec<SongLuaOverlayState>,
    index: SongLuaProxyRequestIndex,
    visit_scratch: SongLuaCaptureVisitScratch,
}

#[cfg(feature = "bench-support")]
impl SongLuaProxyRequestBenchmark {
    pub fn new(capture_count: usize, children_per_capture: usize, reference_count: usize) -> Self {
        let capture_count = capture_count.max(1);
        let mut overlays =
            Vec::with_capacity(capture_count * (children_per_capture + 1) + reference_count);
        let mut capture_names = Vec::with_capacity(capture_count);
        for capture in 0..capture_count {
            let capture_index = overlays.len();
            let name = format!("Capture{capture}");
            capture_names.push(name.clone());
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrameTexture,
                name: Some(name),
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            });
            for child in 0..children_per_capture {
                overlays.push(SongLuaOverlayActor {
                    kind: SongLuaOverlayKind::ActorProxy {
                        target: SongLuaProxyTarget::Judgment {
                            player_index: child % 2,
                        },
                    },
                    name: None,
                    parent_index: Some(capture_index),
                    initial_state: SongLuaOverlayState::default(),
                    message_commands: Vec::new(),
                });
            }
        }
        for reference in 0..reference_count {
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::AftSprite {
                    capture_name: capture_names[reference % capture_names.len()].clone(),
                },
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            });
        }
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let index = SongLuaProxyRequestIndex::new(&overlays);
        Self {
            visit_scratch: SongLuaCaptureVisitScratch::with_capacity(overlays.len()),
            overlays,
            states,
            index,
        }
    }

    pub fn legacy_frame(&self) -> u64 {
        Self::checksum(song_lua_proxy_requests(&self.overlays, &self.states))
    }

    pub fn repeated_indexed_frame(&self) -> u64 {
        Self::checksum(song_lua_proxy_requests_indexed_legacy(
            &self.overlays,
            &self.states,
            &self.index,
        ))
    }

    pub fn indexed_frame(&mut self) -> u64 {
        Self::checksum(song_lua_proxy_requests_indexed(
            &self.overlays,
            &self.states,
            &self.index,
            &mut self.visit_scratch,
        ))
    }

    fn checksum(requests: SongLuaScreenProxyRequests) -> u64 {
        let mut bits = 0u64;
        for (player_index, player) in requests.players.iter().enumerate() {
            let offset = player_index * 4;
            bits |= u64::from(player.player) << offset;
            bits |= u64::from(player.note_field) << (offset + 1);
            bits |= u64::from(player.judgment) << (offset + 2);
            bits |= u64::from(player.combo) << (offset + 3);
        }
        bits |= u64::from(requests.underlay) << 8;
        bits | (u64::from(requests.overlay) << 9)
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaCaptureTraversalBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    states: Vec<SongLuaOverlayState>,
    index: SongLuaProxyRequestIndex,
    visit_scratch: SongLuaCaptureVisitScratch,
    source: [Arc<[Actor]>; 1],
}

#[cfg(feature = "bench-support")]
impl SongLuaCaptureTraversalBenchmark {
    pub fn new(depth: usize, reference_count: usize) -> Self {
        let depth = depth.max(1);
        let mut overlays = Vec::with_capacity(depth * 2 + reference_count + 2);
        let mut capture_indices = Vec::with_capacity(depth);
        let mut capture_names = Vec::with_capacity(depth);
        for capture in 0..depth {
            let name = format!("NestedCapture{capture}");
            capture_indices.push(overlays.len());
            capture_names.push(name.clone());
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrameTexture,
                name: Some(name),
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            });
        }
        for capture in 0..depth.saturating_sub(1) {
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::AftSprite {
                    capture_name: capture_names[capture + 1].clone(),
                },
                name: None,
                parent_index: Some(capture_indices[capture]),
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            });
        }
        let last_capture = capture_indices[depth - 1];
        for player_index in 0..2 {
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorProxy {
                    target: SongLuaProxyTarget::NoteField { player_index },
                },
                name: None,
                parent_index: Some(last_capture),
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            });
        }
        for _ in 0..reference_count.max(1) {
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::AftSprite {
                    capture_name: capture_names[0].clone(),
                },
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState::default(),
                message_commands: Vec::new(),
            });
        }
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect();
        let index = SongLuaProxyRequestIndex::new(&overlays);
        Self {
            visit_scratch: SongLuaCaptureVisitScratch::with_capacity(overlays.len()),
            source: [Arc::from([Actor::CameraPop])],
            overlays,
            states,
            index,
        }
    }

    pub fn legacy_requests(&self) -> u64 {
        SongLuaProxyRequestBenchmark::checksum(song_lua_proxy_requests_indexed_legacy(
            &self.overlays,
            &self.states,
            &self.index,
        ))
    }

    pub fn deduped_requests(&mut self) -> u64 {
        SongLuaProxyRequestBenchmark::checksum(song_lua_proxy_requests_indexed(
            &self.overlays,
            &self.states,
            &self.index,
            &mut self.visit_scratch,
        ))
    }

    pub fn legacy_replacements(&self) -> u64 {
        let proxy_sources = [
            SongLuaPlayerProxySources {
                note_field: Some(&self.source),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources {
                note_field: Some(&self.source),
                ..SongLuaPlayerProxySources::default()
            },
        ];
        Self::player_checksum(song_lua_replacement_active_players_indexed_legacy(
            &self.overlays,
            &self.states,
            &proxy_sources,
            &self.index,
        ))
    }

    pub fn fused_replacements(&mut self) -> u64 {
        let proxy_sources = [
            SongLuaPlayerProxySources {
                note_field: Some(&self.source),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources {
                note_field: Some(&self.source),
                ..SongLuaPlayerProxySources::default()
            },
        ];
        Self::player_checksum(song_lua_replacement_active_players_indexed(
            &self.overlays,
            &self.states,
            &proxy_sources,
            &self.index,
            &mut self.visit_scratch,
        ))
    }

    fn player_checksum(players: [bool; 2]) -> u64 {
        u64::from(players[0]) | (u64::from(players[1]) << 1)
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaTopologyBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    states: Vec<SongLuaOverlayState>,
    draw_order: Vec<usize>,
    index: SongLuaOverlayTopologyIndex,
}

#[cfg(feature = "bench-support")]
impl SongLuaTopologyBenchmark {
    pub fn new(group_count: usize, chain_depth: usize, reference_count: usize) -> Self {
        let group_count = group_count.max(1);
        let chain_depth = chain_depth.max(2);
        let mut overlays = Vec::with_capacity(group_count * chain_depth + reference_count);
        let mut capture_names = Vec::with_capacity(group_count);
        for group in 0..group_count {
            let capture_index = overlays.len();
            let capture_name = format!("TopologyCapture{group}");
            capture_names.push(capture_name.clone());
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrameTexture,
                name: Some(capture_name),
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    fov: Some(40.0 + group as f32),
                    x: group as f32,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            });
            let camera_index = overlays.len();
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: Some(capture_index),
                initial_state: SongLuaOverlayState {
                    fov: (!group.is_multiple_of(2)).then_some(20.0 + group as f32),
                    x: -(group as f32),
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            });
            let mut parent_index = camera_index;
            for _ in 2..chain_depth {
                let overlay_index = overlays.len();
                overlays.push(SongLuaOverlayActor {
                    kind: SongLuaOverlayKind::Actor,
                    name: None,
                    parent_index: Some(parent_index),
                    initial_state: SongLuaOverlayState::default(),
                    message_commands: Vec::new(),
                });
                parent_index = overlay_index;
            }
        }
        for reference in 0..reference_count {
            let capture_name = &capture_names[capture_names.len() - 1 - reference % group_count];
            let mut diffuse = [0.0, 0.0, 0.0, 1.0];
            diffuse[reference % 3] = 1.0;
            overlays.push(SongLuaOverlayActor {
                kind: SongLuaOverlayKind::AftSprite {
                    capture_name: if reference.is_multiple_of(2) {
                        capture_name.to_ascii_lowercase()
                    } else {
                        capture_name.clone()
                    },
                },
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    diffuse,
                    blend: SongLuaOverlayBlendMode::Add,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            });
        }
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let index = SongLuaOverlayTopologyIndex::new(&overlays);
        Self {
            draw_order: (0..overlays.len()).collect(),
            overlays,
            states,
            index,
        }
    }

    pub fn legacy_aft_targets(&self) -> u64 {
        self.overlays
            .iter()
            .filter_map(|overlay| match &overlay.kind {
                SongLuaOverlayKind::AftSprite { capture_name } => Some(
                    song_lua_overlay_capture_index_by_name(&self.overlays, capture_name),
                ),
                _ => None,
            })
            .fold(0, Self::index_checksum)
    }

    pub fn indexed_aft_targets(&self) -> u64 {
        self.overlays
            .iter()
            .enumerate()
            .filter(|(_, overlay)| matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }))
            .map(|(overlay_index, _)| self.index.aft_sprite_targets[overlay_index].get())
            .fold(0, Self::index_checksum)
    }

    pub fn legacy_aft_ancestors(&self) -> u64 {
        (0..self.overlays.len())
            .map(|overlay_index| song_lua_overlay_aft_ancestor(&self.overlays, overlay_index))
            .fold(0, Self::index_checksum)
    }

    pub fn indexed_aft_ancestors(&self) -> u64 {
        self.index
            .aft_ancestors
            .iter()
            .copied()
            .map(SongLuaOverlayIndex::get)
            .fold(0, Self::index_checksum)
    }

    pub fn legacy_camera_states(&self) -> u64 {
        self.overlays
            .iter()
            .map(|overlay| {
                song_lua_overlay_camera_state(&self.overlays, &self.states, overlay.parent_index)
            })
            .fold(0, Self::camera_checksum)
    }

    pub fn indexed_camera_states(&self) -> u64 {
        (0..self.overlays.len())
            .map(|overlay_index| {
                song_lua_overlay_camera_state_indexed(&self.states, &self.index, overlay_index)
            })
            .fold(0, Self::camera_checksum)
    }

    pub fn precomputed_camera_states(&self) -> u64 {
        (0..self.overlays.len())
            .map(|overlay_index| self.index.camera_state(&self.states, overlay_index))
            .fold(0, Self::camera_checksum)
    }

    pub fn legacy_rgb_aft_groups(&self) -> u64 {
        self.overlays
            .iter()
            .enumerate()
            .filter(|(_, overlay)| matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }))
            .map(|(index, _)| {
                song_lua_rgb_aft_group_for(&self.overlays, &self.states, &self.draw_order, index)
            })
            .fold(0, Self::rgb_group_checksum)
    }

    pub fn prepared_rgb_aft_groups(&mut self) -> u64 {
        self.index
            .prepare_rgb_aft_groups(&self.states, &self.draw_order);
        self.overlays
            .iter()
            .enumerate()
            .filter(|(_, overlay)| matches!(overlay.kind, SongLuaOverlayKind::AftSprite { .. }))
            .map(|(index, _)| self.index.rgb_aft_group(index))
            .fold(0, Self::rgb_group_checksum)
    }

    pub fn topology_bytes_per_overlay(&self) -> usize {
        self.index.storage_bytes() / self.overlays.len().max(1)
    }

    fn index_checksum(checksum: u64, index: Option<usize>) -> u64 {
        checksum.rotate_left(5) ^ index.map_or(0, |index| index as u64 + 1)
    }

    fn camera_checksum(checksum: u64, state: Option<SongLuaOverlayState>) -> u64 {
        let bits = state.map_or(0, |state| {
            u64::from(state.fov.unwrap_or_default().to_bits())
                ^ u64::from(state.x.to_bits()).rotate_left(17)
        });
        checksum.rotate_left(5) ^ bits
    }

    fn rgb_group_checksum(checksum: u64, group: Option<(usize, [usize; 3])>) -> u64 {
        let Some((leader, group)) = group else {
            return checksum.rotate_left(5);
        };
        group.into_iter().fold(leader as u64 + 1, |bits, index| {
            bits.rotate_left(11) ^ index as u64 + 1
        }) ^ checksum.rotate_left(5)
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaOrderBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    states: Vec<SongLuaOverlayState>,
    cache: SongLuaOverlayOrderCache,
    out: Vec<usize>,
}

#[cfg(feature = "bench-support")]
impl SongLuaOrderBenchmark {
    pub fn new(actor_count: usize) -> Self {
        let overlays = (0..actor_count)
            .map(|index| SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    draw_order: ((index * 37) % actor_count.max(1)) as i32,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            })
            .collect::<Vec<_>>();
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        if let Some(dynamic) = cache.dynamic_draw_order.first_mut() {
            *dynamic = true;
        }
        cache.static_root_order = None;
        Self {
            out: Vec::with_capacity(actor_count),
            overlays,
            states,
            cache,
        }
    }

    pub fn legacy_frame(&mut self) -> usize {
        self.out.clear();
        self.out.reserve(self.overlays.len());
        song_lua_push_order_legacy(
            &self.overlays,
            &self.states,
            &mut self.cache,
            None,
            &mut self.out,
        );
        self.checksum()
    }

    pub fn cached_frame(&mut self) -> usize {
        song_lua_overlay_order_into(
            &self.overlays,
            &self.states,
            &mut self.cache,
            None,
            &mut self.out,
        );
        self.checksum()
    }

    pub fn legacy_changing_frame(&mut self, tick: usize) -> usize {
        self.change_draw_order(tick);
        self.legacy_frame()
    }

    pub fn cached_changing_frame(&mut self, tick: usize) -> usize {
        self.change_draw_order(tick);
        self.cached_frame()
    }

    fn change_draw_order(&mut self, tick: usize) {
        if self.states.is_empty() {
            return;
        }
        let index = tick % self.states.len();
        self.states[index].draw_order = ((tick.wrapping_mul(17)) % 4_096) as i32;
    }

    fn checksum(&self) -> usize {
        self.out
            .iter()
            .enumerate()
            .fold(0usize, |sum, (position, index)| {
                sum.wrapping_add((position + 1).wrapping_mul(*index + 1))
            })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaStaticOrderBenchmark {
    overlays: Vec<SongLuaOverlayActor>,
    states: Vec<SongLuaOverlayState>,
    cache: SongLuaOverlayOrderCache,
    out: Vec<usize>,
}

#[cfg(feature = "bench-support")]
impl SongLuaStaticOrderBenchmark {
    pub fn new(actor_count: usize) -> Self {
        let overlays = (0..actor_count)
            .map(|index| SongLuaOverlayActor {
                kind: if index.is_multiple_of(4) {
                    SongLuaOverlayKind::ActorFrame
                } else {
                    SongLuaOverlayKind::Quad
                },
                name: None,
                parent_index: (index > 0).then(|| (index - 1) / 4),
                initial_state: SongLuaOverlayState {
                    draw_order: ((index * 37) % actor_count.max(1)) as i32,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            })
            .collect::<Vec<_>>();
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        Self {
            cache: song_lua_overlay_order_cache_from(&overlays, &[]),
            out: Vec::with_capacity(actor_count),
            overlays,
            states,
        }
    }

    pub fn recursive_frame(&mut self) -> usize {
        self.out.clear();
        song_lua_push_order(
            &self.overlays,
            &self.states,
            &mut self.cache,
            None,
            &mut self.out,
        );
        self.checksum()
    }

    pub fn flat_frame(&mut self) -> usize {
        song_lua_overlay_order_into(
            &self.overlays,
            &self.states,
            &mut self.cache,
            None,
            &mut self.out,
        );
        self.checksum()
    }

    fn checksum(&self) -> usize {
        self.out
            .iter()
            .enumerate()
            .fold(0usize, |sum, (position, index)| {
                sum.wrapping_add((position + 1).wrapping_mul(*index + 1))
            })
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaProjectedMeshBenchmark {
    scratch: SongLuaProjectedMeshScratch,
}

#[cfg(feature = "bench-support")]
impl Default for SongLuaProjectedMeshBenchmark {
    fn default() -> Self {
        Self {
            scratch: SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY),
        }
    }
}

#[cfg(feature = "bench-support")]
impl SongLuaProjectedMeshBenchmark {
    pub fn legacy_frame(&self, start_fade: f32, end_fade: f32) -> Arc<[TexturedMeshVertex]> {
        fn axis(start_fade: f32, end_fade: f32) -> Vec<f32> {
            let mut out: Vec<f32> = vec![0.0];
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

        let xs = axis(start_fade, end_fade);
        let ys = axis(end_fade, start_fade);
        let mut grid = Vec::with_capacity(xs.len() * ys.len());
        append_benchmark_projected_grid(&xs, &ys, &mut grid);
        let mut vertices =
            Vec::with_capacity(xs.len().saturating_sub(1) * ys.len().saturating_sub(1) * 6);
        append_projected_mesh_vertices(&grid, xs.len(), ys.len(), &mut vertices);
        Arc::from(vertices.into_boxed_slice())
    }

    pub fn reused_frame(&mut self, start_fade: f32, end_fade: f32) -> Arc<Vec<TexturedMeshVertex>> {
        let xs = song_lua_projected_overlay_axis_slices(start_fade, end_fade);
        let ys = song_lua_projected_overlay_axis_slices(end_fade, start_fade);
        let mut grid = SmallVec::<[TexturedMeshVertex; 16]>::new();
        append_benchmark_projected_grid(&xs, &ys, &mut grid);
        self.scratch.update_projected(&grid, xs.len(), ys.len())
    }

    pub fn storage_bytes(&self) -> usize {
        self.scratch.storage_bytes()
    }

    pub fn replacements(&self) -> u64 {
        self.scratch.replacements
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaMultiActorEmitBenchmark {
    out: Vec<Actor>,
    actor_count: usize,
}

#[cfg(feature = "bench-support")]
impl SongLuaMultiActorEmitBenchmark {
    pub fn new(actor_count: usize) -> Self {
        Self {
            out: Vec::with_capacity(actor_count),
            actor_count,
        }
    }

    pub fn legacy_frame(&self) -> u64 {
        let mut out = SongLuaActorList::new();
        if self.actor_count > 2 {
            out.reserve(self.actor_count);
        }
        for _ in 0..self.actor_count {
            out.push(Actor::CameraPop);
        }
        out.len() as u64
    }

    pub fn direct_frame(&mut self) -> u64 {
        self.out.clear();
        for _ in 0..self.actor_count {
            self.out.push(Actor::CameraPop);
        }
        self.out.len() as u64
    }

    pub fn storage_bytes(&self) -> usize {
        self.out
            .capacity()
            .saturating_mul(std::mem::size_of::<Actor>())
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaUppercaseTextBenchmark {
    source: Arc<str>,
    uppercase: Arc<str>,
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaTextAttributeBenchmark {
    text: Arc<str>,
    long_text: Arc<str>,
    attributes: Arc<[TextAttribute]>,
    rainbow_phases: [Arc<[TextAttribute]>; SONG_LUA_TEXT_RAINBOW_COLORS.len()],
    scratch: SongLuaProjectedMeshScratch,
    long_rainbow_scratch: SongLuaProjectedMeshScratch,
}

#[cfg(feature = "bench-support")]
impl SongLuaTextAttributeBenchmark {
    pub fn new(text: &str, attribute_count: usize) -> Self {
        let char_count = text.chars().count().max(1);
        let attributes = (0..attribute_count)
            .map(|index| TextAttribute {
                start: index % char_count,
                length: 1,
                color: [0.25, 0.5, 0.75, 1.0],
                vertex_colors: None,
                glow: Some([0.75, 0.25, 0.5, 0.75]),
            })
            .collect::<Vec<_>>();
        let long_text = text.repeat(4);
        assert!(long_text.chars().count() > SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS);
        let mut scratch = SongLuaProjectedMeshScratch::default();
        scratch.prewarm_text_attributes(attribute_count, char_count);
        let mut long_rainbow_scratch = SongLuaProjectedMeshScratch::default();
        long_rainbow_scratch.prewarm_text_attributes(attribute_count, long_text.chars().count());
        Self {
            text: Arc::from(text),
            long_text: Arc::from(long_text),
            attributes: Arc::from(attributes.into_boxed_slice()),
            rainbow_phases: song_lua_rainbow_scroll_phases(text),
            scratch,
            long_rainbow_scratch,
        }
    }

    pub fn legacy_static_frame(&self) -> u64 {
        let attributes = std::hint::black_box(self.attributes.as_ref().to_vec());
        song_lua_text_attribute_checksum(&attributes)
    }

    pub fn shared_static_frame(&self) -> u64 {
        let attributes = TextAttributes::from(Arc::clone(&self.attributes));
        song_lua_text_attribute_checksum(attributes.as_slice())
    }

    pub fn legacy_rainbow_frame(&self, text: &str, total_elapsed: f32) -> u64 {
        let attributes = song_lua_rainbow_scroll_attributes(text, total_elapsed);
        song_lua_text_attribute_checksum(&attributes)
    }

    pub fn prewarmed_rainbow_frame(&self, total_elapsed: f32) -> u64 {
        let phase = song_lua_rainbow_scroll_phase(total_elapsed);
        let attributes = Arc::clone(&self.rainbow_phases[phase]);
        song_lua_text_attribute_checksum(&attributes)
    }

    pub fn legacy_long_rainbow_frame(&self, total_elapsed: f32) -> u64 {
        let attributes = song_lua_rainbow_scroll_attributes(&self.long_text, total_elapsed);
        song_lua_text_attribute_checksum(&attributes)
    }

    pub fn reused_long_rainbow_frame(&mut self, total_elapsed: f32) -> u64 {
        let attributes = self
            .long_rainbow_scratch
            .rainbow_attributes(&self.long_text, total_elapsed);
        song_lua_text_attribute_checksum(attributes.as_slice())
    }

    pub fn legacy_diffuse_frame(&self) -> u64 {
        let (attributes, color) = song_lua_text_attributes_for_diffuse_mode(
            &self.attributes,
            [0.5, 0.625, 0.75, 0.875],
            &self.text,
            false,
            None,
        );
        song_lua_text_attribute_checksum(attributes.as_slice())
            ^ u64::from(color[3].to_bits()).rotate_left(17)
    }

    pub fn reused_diffuse_frame(&mut self) -> u64 {
        let (attributes, color) = song_lua_text_attributes_for_diffuse_mode(
            &self.attributes,
            [0.5, 0.625, 0.75, 0.875],
            &self.text,
            false,
            Some(&mut self.scratch),
        );
        song_lua_text_attribute_checksum(attributes.as_slice())
            ^ u64::from(color[3].to_bits()).rotate_left(17)
    }

    pub fn legacy_glow_frame(&self) -> u64 {
        let attributes = song_lua_text_glow_attributes(
            &self.text,
            &self.attributes,
            [0.125, 0.25, 0.5, 0.75],
            None,
        );
        song_lua_text_attribute_checksum(attributes.as_slice())
    }

    pub fn reused_glow_frame(&mut self) -> u64 {
        let attributes = song_lua_text_glow_attributes(
            &self.text,
            &self.attributes,
            [0.125, 0.25, 0.5, 0.75],
            Some(&mut self.scratch),
        );
        song_lua_text_attribute_checksum(attributes.as_slice())
    }

    pub fn legacy_stroke_frame(&self) -> u64 {
        let attributes = song_lua_transparent_text_attributes(&self.text, None);
        song_lua_text_attribute_checksum(attributes.as_slice())
    }

    pub fn reused_stroke_frame(&mut self) -> u64 {
        let attributes = song_lua_transparent_text_attributes(&self.text, Some(&mut self.scratch));
        song_lua_text_attribute_checksum(attributes.as_slice())
    }

    pub fn storage_bytes(&self) -> usize {
        self.rainbow_phases
            .iter()
            .map(|attributes| attributes.len())
            .sum::<usize>()
            .saturating_mul(std::mem::size_of::<TextAttribute>())
    }

    pub fn dynamic_storage_bytes(&self) -> usize {
        self.scratch
            .text_diffuse_attributes
            .as_ref()
            .map_or(0, |attributes| attributes.capacity())
            .saturating_add(
                self.scratch
                    .text_glow_attributes
                    .as_ref()
                    .map_or(0, |attributes| attributes.capacity()),
            )
            .saturating_mul(std::mem::size_of::<TextAttribute>())
    }

    pub fn long_rainbow_storage_bytes(&self) -> usize {
        self.long_rainbow_scratch.storage_bytes()
    }

    pub fn replacements(&self) -> u64 {
        self.scratch.replacements
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_text_attribute_checksum(attributes: &[TextAttribute]) -> u64 {
    attributes
        .iter()
        .fold(attributes.len() as u64, |checksum, attribute| {
            checksum.rotate_left(7)
                ^ attribute.start as u64
                ^ (attribute.length as u64).rotate_left(13)
                ^ u64::from(attribute.color[0].to_bits()).rotate_left(29)
                ^ u64::from(attribute.color[3].to_bits()).rotate_left(43)
        })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaNoteskinModelBenchmark {
    slot: SpriteSlot,
    cache: ModelMeshCache,
    glow_vertices: Arc<[TexturedMeshVertex]>,
}

#[cfg(feature = "bench-support")]
impl SongLuaNoteskinModelBenchmark {
    pub fn new(vertex_count: usize, timeline_segments: usize) -> Self {
        let vertex_count = vertex_count.max(3);
        let mut slot = noteskin::test_model_slot();
        let vertices = (0..vertex_count)
            .map(|index| deadsync_noteskin::ModelVertex {
                pos: [
                    (index % 8) as f32 * 8.0,
                    (index / 8) as f32 * 8.0,
                    (index % 3) as f32,
                ],
                uv: [(index % 8) as f32 / 7.0, (index / 8) as f32 / 12.0],
                tex_matrix_scale: [1.0, 1.0],
            })
            .collect::<Vec<_>>();
        slot.model = Some(Arc::new(deadsync_noteskin::ModelMesh {
            vertices: Arc::from(vertices.into_boxed_slice()),
            bounds: [0.0, 0.0, 0.0, 64.0, 96.0, 2.0],
        }));
        let mut timeline = Vec::with_capacity(timeline_segments);
        let mut from = ModelDrawState::default();
        for segment in 0..timeline_segments {
            let to = ModelDrawState {
                pos: [segment as f32 * 0.25, segment as f32 * -0.125, 0.0],
                rot: [segment as f32, 0.0, segment as f32 * 3.0],
                zoom: [1.0 + segment as f32 * 0.001, 1.0, 1.0],
                ..from
            };
            timeline.push(deadsync_noteskin::ModelTweenSegment {
                start: segment as f32 * 0.05,
                duration: 0.05,
                tween: TweenType::Linear,
                from,
                to,
            });
            from = to;
        }
        slot.model_timeline = Arc::from(timeline.into_boxed_slice());

        let mut cache = ModelMeshCache::with_capacity(1);
        cache.prewarm_slot(&slot);
        let (_, vertices) = cache
            .model_geometry(&slot)
            .expect("benchmark model geometry should prewarm");
        let glow_vertices = song_lua_static_glow_vertices(&vertices);
        cache.seal();
        Self {
            slot,
            cache,
            glow_vertices,
        }
    }

    pub fn legacy_geometry_frame(&self) -> u64 {
        let actor = noteskin_model_actor_from_draw(
            &self.slot,
            ModelDrawState::default(),
            [320.0, 240.0],
            [64.0, 96.0],
            [0.0, 0.0, 1.0, 1.0],
            0.0,
            [0.5, 0.75, 1.0, 0.8],
            BlendMode::Alpha,
            123,
        )
        .expect("benchmark model should render");
        song_lua_textured_actor_checksum(&actor)
    }

    pub fn prewarmed_geometry_frame(&mut self) -> u64 {
        let actor = self.cached_actor(ModelDrawState::default());
        song_lua_textured_actor_checksum(&actor)
    }

    pub fn legacy_glow_frame(&mut self) -> u64 {
        let actor = self.cached_actor(ModelDrawState::default());
        let glow = song_lua_overlay_glow_actor(
            &actor,
            [0.2, 0.4, 0.8, 0.75],
            SongLuaTextGlowMode::Inner,
            None,
        )
        .expect("benchmark glow should render");
        song_lua_textured_actor_checksum(&glow)
    }

    pub fn prewarmed_glow_frame(&mut self) -> u64 {
        let actor = self.cached_actor(ModelDrawState::default());
        let glow = song_lua_overlay_glow_actor_with_static_vertices(
            &actor,
            [0.2, 0.4, 0.8, 0.75],
            SongLuaTextGlowMode::Inner,
            None,
            Some(&self.glow_vertices),
        )
        .expect("benchmark glow should render");
        song_lua_textured_actor_checksum(&glow)
    }

    pub fn legacy_tween_frame(&self, time: f32) -> u64 {
        song_lua_model_draw_checksum(self.slot.model_draw_at(time, time * 4.0))
    }

    pub fn cursor_tween_frame(&mut self, time: f32) -> u64 {
        self.cache.begin_frame();
        song_lua_model_draw_checksum(self.cache.draw_at(&self.slot, time, time * 4.0))
    }

    pub fn storage_bytes(&self) -> usize {
        self.glow_vertices
            .len()
            .saturating_mul(std::mem::size_of::<TexturedMeshVertex>())
            .saturating_mul(2)
    }

    pub fn cache_keys(&mut self) -> [TMeshCacheKey; 2] {
        let actor = self.cached_actor(ModelDrawState::default());
        let Actor::TexturedMesh {
            geom_cache_key: base,
            ..
        } = actor
        else {
            unreachable!("benchmark model should emit a textured mesh")
        };
        [base, song_lua_glow_geometry_key(base)]
    }

    fn cached_actor(&mut self, draw: ModelDrawState) -> Actor {
        noteskin_model_actor_from_draw_cached(
            &self.slot,
            draw,
            [320.0, 240.0],
            [64.0, 96.0],
            [0.0, 0.0, 1.0, 1.0],
            0.0,
            [0.5, 0.75, 1.0, 0.8],
            BlendMode::Alpha,
            123,
            &mut self.cache,
        )
        .expect("benchmark model should render")
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_model_draw_checksum(draw: ModelDrawState) -> u64 {
    u64::from(draw.pos[0].to_bits())
        ^ u64::from(draw.pos[1].to_bits()).rotate_left(11)
        ^ u64::from(draw.rot[0].to_bits()).rotate_left(23)
        ^ u64::from(draw.rot[2].to_bits()).rotate_left(37)
        ^ u64::from(draw.zoom[0].to_bits()).rotate_left(49)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaTexturedGlowBenchmark {
    texture: Arc<str>,
    immutable_vertices: Arc<[TexturedMeshVertex]>,
    prewarmed_static_vertices: Arc<[TexturedMeshVertex]>,
    reusable_vertices: Arc<Vec<TexturedMeshVertex>>,
    scratch: SongLuaProjectedMeshScratch,
}

#[cfg(feature = "bench-support")]
impl SongLuaTexturedGlowBenchmark {
    pub fn new(vertex_count: usize) -> Self {
        let vertices = (0..vertex_count.max(3))
            .map(|index| TexturedMeshVertex {
                pos: [index as f32 * 0.25, -(index as f32) * 0.125, 0.0],
                uv: [0.25, 0.75],
                tex_matrix_scale: [1.0, 1.0],
                color: [0.25, 0.5, 0.75, 0.5 + (index % 2) as f32 * 0.5],
            })
            .collect::<Vec<_>>();
        let prewarmed_static_vertices = Arc::from(
            vertices
                .iter()
                .copied()
                .map(|mut vertex| {
                    vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                    vertex
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        Self {
            texture: Arc::from("song-lua-glow-benchmark"),
            immutable_vertices: Arc::from(vertices.clone().into_boxed_slice()),
            prewarmed_static_vertices,
            reusable_vertices: Arc::new(vertices),
            scratch: SongLuaProjectedMeshScratch::textured(vertex_count.max(3)),
        }
    }

    pub fn legacy_frame(&self) -> u64 {
        let actor = song_lua_benchmark_textured_actor(
            Arc::clone(&self.texture),
            Arc::clone(&self.immutable_vertices),
        );
        let glow = song_lua_overlay_glow_actor(
            &actor,
            [0.2, 0.4, 0.8, 0.75],
            SongLuaTextGlowMode::Inner,
            None,
        )
        .expect("benchmark glow must render");
        song_lua_textured_actor_checksum(&glow)
    }

    pub fn reused_frame(&mut self) -> u64 {
        let actor = Actor::ReusableTexturedMesh {
            align: [0.0, 0.0],
            offset: [12.0, -8.0],
            world_z: 0.25,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: Matrix4::IDENTITY,
            texture: Arc::clone(&self.texture),
            tint: [0.5, 0.75, 1.0, 0.8],
            glow: [1.0, 1.0, 1.0, 0.0],
            vertices: Arc::clone(&self.reusable_vertices),
            geom_cache_key: INVALID_TMESH_CACHE_KEY,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: true,
            visible: true,
            blend: BlendMode::Alpha,
            z: 123,
        };
        let glow = song_lua_overlay_glow_actor(
            &actor,
            [0.2, 0.4, 0.8, 0.75],
            SongLuaTextGlowMode::Inner,
            Some(&mut self.scratch),
        )
        .expect("benchmark glow must render");
        song_lua_textured_actor_checksum(&glow)
    }

    pub fn prewarmed_static_frame(&self) -> u64 {
        let actor = song_lua_benchmark_textured_actor(
            Arc::clone(&self.texture),
            Arc::clone(&self.immutable_vertices),
        );
        let glow = song_lua_overlay_glow_actor_with_static_vertices(
            &actor,
            [0.2, 0.4, 0.8, 0.75],
            SongLuaTextGlowMode::Inner,
            None,
            Some(&self.prewarmed_static_vertices),
        )
        .expect("benchmark glow must render");
        song_lua_textured_actor_checksum(&glow)
    }

    pub fn storage_bytes(&self) -> usize {
        self.scratch.storage_bytes()
    }

    pub fn static_storage_bytes(&self) -> usize {
        self.prewarmed_static_vertices
            .len()
            .saturating_mul(std::mem::size_of::<TexturedMeshVertex>())
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaWhiteTextureKeyBenchmark;

#[cfg(feature = "bench-support")]
impl SongLuaWhiteTextureKeyBenchmark {
    pub fn legacy_frame() -> u64 {
        let texture: Arc<str> = Arc::from(std::hint::black_box("__white"));
        song_lua_text_checksum(texture.as_ref())
    }

    pub fn shared_frame() -> u64 {
        let texture = white_texture_key();
        song_lua_text_checksum(texture.as_ref())
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaGraphDisplayBenchmark {
    values: Arc<[f32]>,
    scratch: SongLuaProjectedMeshScratch,
}

#[cfg(feature = "bench-support")]
impl SongLuaGraphDisplayBenchmark {
    pub fn new(point_count: usize) -> Self {
        let point_count = point_count.max(2);
        let values = (0..point_count)
            .map(|index| (index % 17) as f32 / 16.0)
            .collect::<Vec<_>>();
        Self {
            values: Arc::from(values.into_boxed_slice()),
            scratch: SongLuaProjectedMeshScratch::graph((point_count - 1) * 6),
        }
    }

    pub fn legacy_frame(&self) -> u64 {
        self.frame(None)
    }

    pub fn rebuilt_frame(&mut self) -> u64 {
        self.scratch.graph_body_key = None;
        self.scratch.graph_line_key = None;
        self.cached_frame()
    }

    pub fn cached_frame(&mut self) -> u64 {
        let values = Arc::clone(&self.values);
        let actor = song_lua_graph_display_actor(
            Self::state(),
            &values,
            Self::body_state(),
            Self::line_state(),
            [320.0, 120.0],
            1.0,
            1.0,
            321,
            Some(&mut self.scratch),
        )
        .expect("benchmark graph must render");
        song_lua_graph_actor_checksum(&actor)
    }

    pub fn storage_bytes(&self) -> usize {
        self.scratch.storage_bytes()
    }

    pub fn replacements(&self) -> u64 {
        self.scratch.replacements.saturating_add(
            self.scratch
                .graph_frame
                .as_ref()
                .map_or(0, |frame| frame.stats().replacements),
        )
    }

    pub fn growths(&self) -> u64 {
        self.scratch
            .graph_frame
            .as_ref()
            .map_or(0, |frame| frame.stats().growths)
    }

    fn frame(&self, scratch: Option<&mut SongLuaProjectedMeshScratch>) -> u64 {
        let actor = song_lua_graph_display_actor(
            Self::state(),
            &self.values,
            Self::body_state(),
            Self::line_state(),
            [320.0, 120.0],
            1.0,
            1.0,
            321,
            scratch,
        )
        .expect("benchmark graph must render");
        song_lua_graph_actor_checksum(&actor)
    }

    fn state() -> SongLuaOverlayState {
        SongLuaOverlayState {
            x: 320.0,
            y: 120.0,
            diffuse: [0.5, 0.75, 1.0, 0.8],
            ..SongLuaOverlayState::default()
        }
    }

    fn body_state() -> SongLuaOverlayState {
        SongLuaOverlayState {
            diffuse: [0.2, 0.5, 0.75, 0.9],
            ..SongLuaOverlayState::default()
        }
    }

    fn line_state() -> SongLuaOverlayState {
        SongLuaOverlayState {
            y: 1.0,
            size: Some([1.0, 2.0]),
            diffuse: [0.8, 0.7, 0.6, 0.5],
            ..SongLuaOverlayState::default()
        }
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_graph_actor_checksum(actor: &Actor) -> u64 {
    fn fold_vertices(checksum: &mut u64, vertices: &[MeshVertex]) {
        for vertex in vertices {
            *checksum = checksum.rotate_left(7)
                ^ u64::from(vertex.pos[0].to_bits())
                ^ u64::from(vertex.pos[1].to_bits()).rotate_left(19)
                ^ u64::from(vertex.color[3].to_bits()).rotate_left(37);
        }
    }

    fn fold_actor(checksum: &mut u64, actor: &Actor) {
        match actor {
            Actor::Mesh { vertices, .. } => fold_vertices(checksum, vertices),
            Actor::ReusableMesh { vertices, .. } => fold_vertices(checksum, vertices),
            Actor::Frame { children, .. } => {
                for child in children {
                    fold_actor(checksum, child);
                }
            }
            Actor::SharedFrame { children, .. } => {
                for child in children.iter() {
                    fold_actor(checksum, child);
                }
            }
            _ => unreachable!("graph benchmark emitted an unexpected actor"),
        }
    }

    let mut checksum = 0;
    fold_actor(&mut checksum, actor);
    checksum
}

#[cfg(feature = "bench-support")]
fn song_lua_benchmark_textured_actor(
    texture: Arc<str>,
    vertices: Arc<[TexturedMeshVertex]>,
) -> Actor {
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [12.0, -8.0],
        world_z: 0.25,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture,
        tint: [0.5, 0.75, 1.0, 0.8],
        glow: [1.0, 1.0, 1.0, 0.0],
        vertices,
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: true,
        visible: true,
        blend: BlendMode::Alpha,
        z: 123,
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_textured_actor_checksum(actor: &Actor) -> u64 {
    let vertices: &[TexturedMeshVertex] = match actor {
        Actor::TexturedMesh { vertices, .. } => vertices,
        Actor::ReusableTexturedMesh { vertices, .. } => vertices,
        _ => unreachable!("glow builder returned an unexpected actor"),
    };
    vertices
        .iter()
        .fold(vertices.len() as u64, |checksum, vertex| {
            checksum.rotate_left(7)
                ^ u64::from(vertex.pos[0].to_bits())
                ^ u64::from(vertex.color[0].to_bits()).rotate_left(19)
                ^ u64::from(vertex.color[3].to_bits()).rotate_left(37)
        })
}

#[cfg(feature = "bench-support")]
impl SongLuaUppercaseTextBenchmark {
    pub fn new(source: &str) -> Self {
        Self {
            source: Arc::from(source),
            uppercase: Arc::from(source.to_uppercase()),
        }
    }

    pub fn legacy_frame(&self) -> u64 {
        song_lua_text_checksum(TextContent::from(self.source.to_uppercase()).as_str())
    }

    pub fn cached_frame(&self) -> u64 {
        song_lua_text_checksum(TextContent::from(Arc::clone(&self.uppercase)).as_str())
    }

    pub fn storage_bytes(&self) -> usize {
        self.uppercase.len()
    }
}

#[cfg(feature = "bench-support")]
fn song_lua_text_checksum(text: &str) -> u64 {
    text.as_bytes().iter().fold(0u64, |checksum, byte| {
        checksum
            .wrapping_mul(16777619)
            .wrapping_add(u64::from(*byte))
    })
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaActorBuildBenchmark {
    proxy_source: [Arc<[Actor]>; 1],
    segmented_proxy_source: [Arc<[Actor]>; 3],
    local_z_proxy_source: [Arc<[Actor]>; 1],
    camera_proxy_source: [Arc<[Actor]>; 1],
    captured_proxy_source: [Arc<[Actor]>; 1],
    proxy_scratch: SongLuaProxyActorScratch,
    mesh_source: Arc<[SongLuaOverlayMeshVertex]>,
    captured_mesh_vertices: Arc<[MeshVertex]>,
    mesh_scratch: SongLuaProjectedMeshScratch,
    legacy_capture_shadow: Actor,
    reused_capture_shadow: Actor,
    legacy_fold_shadow: Actor,
    reused_fold_shadow: Actor,
}

#[cfg(feature = "bench-support")]
impl SongLuaActorBuildBenchmark {
    pub fn new(vertex_count: usize) -> Self {
        let vertex_count = vertex_count.max(3);
        let mesh_source = (0..vertex_count)
            .map(|index| SongLuaOverlayMeshVertex {
                pos: [index as f32 * 0.25, -(index as f32) * 0.125],
                color: [0.25, 0.5, 0.75, 1.0],
                uv: [0.0, 0.0],
            })
            .collect::<Vec<_>>();
        let captured_mesh_vertices = mesh_source
            .iter()
            .map(|vertex| MeshVertex {
                pos: vertex.pos,
                color: vertex.color,
            })
            .collect::<Vec<_>>();
        Self {
            proxy_source: [Arc::from([Actor::CameraPop])],
            segmented_proxy_source: std::array::from_fn(|_| Arc::from([Actor::CameraPop])),
            local_z_proxy_source: [Arc::from([
                Self::tagged_proxy_actor(30.0, 3),
                Self::tagged_proxy_actor(10.0, 1),
                Self::tagged_proxy_actor(20.0, 2),
            ])],
            camera_proxy_source: [Arc::from([
                Self::tagged_proxy_actor(30.0, 3),
                Actor::CameraPush {
                    view_proj: Matrix4::IDENTITY,
                },
                Self::tagged_proxy_actor(20.0, 2),
                Self::tagged_proxy_actor(10.0, 1),
                Actor::CameraPop,
                Self::tagged_proxy_actor(40.0, 4),
            ])],
            captured_proxy_source: [Arc::from([Actor::Frame {
                align: [0.0, 0.0],
                offset: [7.0, -3.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: (0..16)
                    .rev()
                    .map(|index| Self::tagged_proxy_actor(index as f32, index as i16))
                    .collect(),
                background: None,
                z: 0,
            }])],
            proxy_scratch: SongLuaProxyActorScratch::with_capacity_and_banks(0, 3, 1),
            mesh_source: Arc::from(mesh_source.into_boxed_slice()),
            captured_mesh_vertices: Arc::from(captured_mesh_vertices.into_boxed_slice()),
            mesh_scratch: SongLuaProjectedMeshScratch::mesh(vertex_count),
            legacy_capture_shadow: Self::shadow_actor(),
            reused_capture_shadow: Self::shadow_actor(),
            legacy_fold_shadow: Self::shadow_actor(),
            reused_fold_shadow: Self::shadow_actor(),
        }
    }

    pub fn legacy_proxy_frame(&self) -> u64 {
        Self::proxy_checksum(
            song_lua_build_proxy_frame_actor(
                SongLuaOverlayState {
                    x: 24.0,
                    y: -12.0,
                    ..SongLuaOverlayState::default()
                },
                321,
                &self.proxy_source,
                screen_width(),
                screen_height(),
            )
            .expect("benchmark proxy must render"),
        )
    }

    pub fn compact_proxy_frame(&self) -> u64 {
        Self::proxy_checksum(
            song_lua_build_proxy_actor(
                SongLuaOverlayState {
                    x: 24.0,
                    y: -12.0,
                    ..SongLuaOverlayState::default()
                },
                321,
                &self.proxy_source,
                screen_width(),
                screen_height(),
            )
            .expect("benchmark proxy must render"),
        )
    }

    pub fn legacy_segmented_proxy_frame(&mut self) -> u64 {
        self.proxy_scratch.begin_frame();
        let (_, segment_start) = self
            .proxy_scratch
            .reserve_proxy_group()
            .expect("benchmark proxy group is prewarmed");
        let mut children = Vec::with_capacity(self.segmented_proxy_source.len());
        for (index, segment) in self.segmented_proxy_source.iter().enumerate() {
            children.push(Actor::SharedFrame {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: self
                    .proxy_scratch
                    .normalize_segment(segment, segment_start + index),
                background: None,
                z: 0,
                tint: [1.0; 4],
                blend: Some(BlendMode::Alpha),
            });
        }
        Self::proxy_checksum(Actor::Frame {
            align: [0.0, 0.0],
            offset: [24.0, -12.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 321,
        })
    }

    pub fn prewarmed_segmented_proxy_frame(&mut self) -> u64 {
        self.proxy_scratch.begin_frame();
        Self::proxy_checksum(
            song_lua_build_proxy_frame_actor_with_scratch(
                Self::proxy_state(),
                321,
                &self.segmented_proxy_source,
                screen_width(),
                screen_height(),
                Some(&mut self.proxy_scratch),
            )
            .expect("benchmark proxy must render"),
        )
    }

    pub fn legacy_local_z_proxy_frame(&self) -> u64 {
        Self::proxy_checksum(Self::legacy_normalized_proxy(&self.local_z_proxy_source[0]))
    }

    pub fn reused_local_z_proxy_frame(&mut self) -> u64 {
        self.proxy_scratch.begin_frame();
        Self::proxy_checksum(
            song_lua_build_proxy_actor_with_scratch(
                Self::proxy_state(),
                321,
                &self.local_z_proxy_source,
                screen_width(),
                screen_height(),
                Some(&mut self.proxy_scratch),
            )
            .expect("benchmark proxy must render"),
        )
    }

    pub fn legacy_camera_proxy_frame(&self) -> u64 {
        Self::proxy_checksum(Self::legacy_normalized_proxy(&self.camera_proxy_source[0]))
    }

    pub fn reused_camera_proxy_frame(&mut self) -> u64 {
        self.proxy_scratch.begin_frame();
        Self::proxy_checksum(
            song_lua_build_proxy_actor_with_scratch(
                Self::proxy_state(),
                321,
                &self.camera_proxy_source,
                screen_width(),
                screen_height(),
                Some(&mut self.proxy_scratch),
            )
            .expect("benchmark proxy must render"),
        )
    }

    pub fn legacy_captured_proxy_frame(&self) -> u64 {
        Self::proxy_checksum(Self::legacy_normalized_proxy(
            &self.captured_proxy_source[0],
        ))
    }

    pub fn reused_captured_proxy_frame(&mut self) -> u64 {
        self.proxy_scratch.begin_frame();
        Self::proxy_checksum(
            song_lua_build_proxy_actor_with_scratch(
                Self::proxy_state(),
                321,
                &self.captured_proxy_source,
                screen_width(),
                screen_height(),
                Some(&mut self.proxy_scratch),
            )
            .expect("benchmark proxy must render"),
        )
    }

    pub fn legacy_group_frame(&self) -> u64 {
        let mut children = Vec::with_capacity(1);
        children.push(Actor::CameraPop);
        let actor = Actor::Frame {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children,
            background: None,
            z: 0,
        };
        u64::from(matches!(actor, Actor::Frame { .. }))
    }

    pub fn inline_group_frame(&self) -> u64 {
        let mut actors = SongLuaActorList::new();
        actors.push(Actor::CameraPop);
        actors.len() as u64
    }

    pub fn legacy_mesh_frame(&self) -> u64 {
        let vertices = song_lua_actor_multi_vertex_mesh(
            &self.mesh_source,
            [0.5, 0.75, 1.0, 0.8],
            1.25,
            0.75,
            [1.1, 0.9],
            [0.8, 1.2, 1.0],
            17.0,
            [0.1, -0.05],
        );
        Self::mesh_checksum(&vertices)
    }

    pub fn reused_mesh_frame(&mut self) -> u64 {
        let vertices = self.mesh_scratch.update_mesh(|out| {
            append_song_lua_actor_multi_vertex_mesh(
                out,
                &self.mesh_source,
                [0.5, 0.75, 1.0, 0.8],
                1.25,
                0.75,
                [1.1, 0.9],
                [0.8, 1.2, 1.0],
                17.0,
                [0.1, -0.05],
            );
        });
        Self::mesh_checksum(&vertices)
    }

    pub fn legacy_captured_mesh_frame(&self) -> u64 {
        let capture_tint = [0.5, 0.75, 0.25, 0.8];
        let vertices = self
            .captured_mesh_vertices
            .iter()
            .copied()
            .map(|mut vertex| {
                vertex.color = song_lua_capture_tint(vertex.color, capture_tint);
                vertex
            })
            .collect::<Vec<_>>();
        Self::captured_mesh_checksum(Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            tint: [1.0; 4],
            vertices: Arc::from(vertices.into_boxed_slice()),
            visible: true,
            blend: BlendMode::Alpha,
            z: 17,
        })
    }

    pub fn shared_captured_mesh_frame(&self) -> u64 {
        Self::captured_mesh_checksum(song_lua_style_capture_actor(
            Actor::Mesh {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                tint: [1.0; 4],
                vertices: Arc::clone(&self.captured_mesh_vertices),
                visible: true,
                blend: BlendMode::Alpha,
                z: 0,
            },
            [0.5, 0.75, 0.25, 0.8],
            None,
            17,
        ))
    }

    pub fn legacy_capture_shadow_frame(&mut self) -> u64 {
        let actor = std::mem::replace(&mut self.legacy_capture_shadow, Actor::CameraPop);
        let actor = Self::legacy_style_shadow(actor, [0.9, 0.8, 0.7, 0.95], 1);
        let checksum = Self::shadow_checksum(&actor);
        self.legacy_capture_shadow = actor;
        checksum
    }

    pub fn reused_capture_shadow_frame(&mut self) -> u64 {
        let actor = std::mem::replace(&mut self.reused_capture_shadow, Actor::CameraPop);
        let actor = song_lua_style_capture_actor(actor, [0.9, 0.8, 0.7, 0.95], None, 1);
        let checksum = Self::shadow_checksum(&actor);
        self.reused_capture_shadow = actor;
        checksum
    }

    pub fn legacy_fold_shadow_frame(&mut self) -> u64 {
        let actor = std::mem::replace(&mut self.legacy_fold_shadow, Actor::CameraPop);
        let actor = Self::legacy_fold_shadow(actor, 100.0, 15.0);
        let checksum = Self::shadow_checksum(&actor);
        self.legacy_fold_shadow = actor;
        checksum
    }

    pub fn reused_fold_shadow_frame(&mut self) -> u64 {
        let actor = std::mem::replace(&mut self.reused_fold_shadow, Actor::CameraPop);
        let actor = song_lua_player_y_fold_actor(actor, 100.0, 15.0);
        let checksum = Self::shadow_checksum(&actor);
        self.reused_fold_shadow = actor;
        checksum
    }

    pub fn mesh_storage_bytes(&self) -> usize {
        self.mesh_scratch.storage_bytes()
    }

    fn proxy_state() -> SongLuaOverlayState {
        SongLuaOverlayState {
            x: 24.0,
            y: -12.0,
            ..SongLuaOverlayState::default()
        }
    }

    fn tagged_proxy_actor(x: f32, z: i16) -> Actor {
        Actor::Mesh {
            align: [0.0, 0.0],
            offset: [x, 0.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            tint: [1.0; 4],
            vertices: Arc::from([]),
            visible: true,
            blend: BlendMode::Alpha,
            z,
        }
    }

    fn shadow_actor() -> Actor {
        Actor::Shadow {
            len: [2.0, -2.0],
            color: [0.1, 0.2, 0.3, 0.5],
            child: Box::new(Self::tagged_proxy_actor(140.0, 3)),
        }
    }

    fn legacy_style_shadow(actor: Actor, capture_tint: [f32; 4], z_shift: i16) -> Actor {
        let Actor::Shadow { len, color, child } = actor else {
            unreachable!("benchmark actor must remain a shadow");
        };
        Actor::Shadow {
            len,
            color: song_lua_capture_tint(color, capture_tint),
            child: Box::new(song_lua_style_capture_actor(
                *child,
                capture_tint,
                None,
                z_shift,
            )),
        }
    }

    fn legacy_fold_shadow(actor: Actor, pivot_x: f32, rotation_y_deg: f32) -> Actor {
        let Actor::Shadow { len, color, child } = actor else {
            unreachable!("benchmark actor must remain a shadow");
        };
        Actor::Shadow {
            len,
            color,
            child: Box::new(song_lua_player_y_fold_actor(
                *child,
                pivot_x,
                rotation_y_deg,
            )),
        }
    }

    fn shadow_checksum(actor: &Actor) -> u64 {
        let Actor::Shadow { len, color, child } = actor else {
            unreachable!("benchmark actor must remain a shadow");
        };
        let mut checksum = u64::from(len[0].to_bits())
            ^ u64::from(len[1].to_bits()).rotate_left(7)
            ^ u64::from(color[3].to_bits()).rotate_left(13);
        if let Actor::Mesh {
            offset, tint, z, ..
        } = child.as_ref()
        {
            checksum ^= u64::from(offset[0].to_bits()).rotate_left(19);
            checksum ^= u64::from(tint[3].to_bits()).rotate_left(23);
            checksum ^= (*z as u16 as u64).rotate_left(29);
        }
        checksum
    }

    fn legacy_normalized_proxy(segment: &Arc<[Actor]>) -> Actor {
        Actor::SharedFrame {
            align: [0.0, 0.0],
            offset: [24.0, -12.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: song_lua_proxy_source_segment_legacy(segment),
            background: None,
            z: 321,
            tint: [1.0; 4],
            blend: Some(BlendMode::Alpha),
        }
    }

    fn proxy_checksum(actor: Actor) -> u64 {
        fn fold(actor: &Actor, offset: [f32; 2], z: i32, checksum: &mut u64) {
            let (local_offset, local_z, children): ([f32; 2], i32, Option<&[Actor]>) = match actor {
                Actor::Frame {
                    offset,
                    z,
                    children,
                    ..
                } => (*offset, i32::from(*z), Some(children)),
                Actor::SharedFrame {
                    offset,
                    z,
                    children,
                    ..
                } => (*offset, i32::from(*z), Some(children)),
                Actor::Camera { children, .. } => ([0.0; 2], 0, Some(children)),
                Actor::Shadow { child, .. } => {
                    fold(child, offset, z, checksum);
                    return;
                }
                Actor::Sprite {
                    offset: local_offset,
                    z: local_z,
                    ..
                }
                | Actor::Text {
                    offset: local_offset,
                    z: local_z,
                    ..
                }
                | Actor::Mesh {
                    offset: local_offset,
                    z: local_z,
                    ..
                }
                | Actor::ReusableMesh {
                    offset: local_offset,
                    z: local_z,
                    ..
                }
                | Actor::TexturedMesh {
                    offset: local_offset,
                    z: local_z,
                    ..
                }
                | Actor::ReusableTexturedMesh {
                    offset: local_offset,
                    z: local_z,
                    ..
                } => {
                    let offset = [offset[0] + local_offset[0], offset[1] + local_offset[1]];
                    *checksum = checksum.rotate_left(7)
                        ^ u64::from(offset[0].to_bits())
                        ^ u64::from(offset[1].to_bits()).rotate_left(19)
                        ^ ((z + i32::from(*local_z)) as u32 as u64).rotate_left(37);
                    return;
                }
                _ => {
                    *checksum = checksum.rotate_left(7)
                        ^ u64::from(offset[0].to_bits())
                        ^ u64::from(offset[1].to_bits()).rotate_left(19)
                        ^ (z as u32 as u64).rotate_left(37)
                        ^ u64::from(matches!(actor, Actor::CameraPush { .. }));
                    return;
                }
            };
            let offset = [offset[0] + local_offset[0], offset[1] + local_offset[1]];
            let z = z + local_z;
            for child in children.expect("frame-like proxy actor must have children") {
                fold(child, offset, z, checksum);
            }
        }

        let mut checksum = 0;
        fold(&actor, [0.0; 2], 0, &mut checksum);
        checksum
    }

    fn mesh_checksum(vertices: &[MeshVertex]) -> u64 {
        vertices
            .iter()
            .fold(vertices.len() as u64, |checksum, vertex| {
                checksum.rotate_left(7)
                    ^ u64::from(vertex.pos[0].to_bits())
                    ^ u64::from(vertex.pos[1].to_bits()).rotate_left(19)
                    ^ u64::from(vertex.color[3].to_bits()).rotate_left(37)
            })
    }

    fn captured_mesh_checksum(actor: Actor) -> u64 {
        let Actor::Mesh {
            tint, vertices, z, ..
        } = actor
        else {
            unreachable!("capture tint benchmark must emit an immutable mesh");
        };
        vertices.iter().fold(z as u16 as u64, |checksum, vertex| {
            let color = song_lua_capture_tint(vertex.color, tint);
            checksum.rotate_left(7)
                ^ u64::from(vertex.pos[0].to_bits())
                ^ u64::from(color[0].to_bits()).rotate_left(19)
                ^ u64::from(color[3].to_bits()).rotate_left(37)
        })
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub struct SongLuaScratchPrewarmBenchmark {
    main_count: usize,
    background_counts: Vec<usize>,
    foreground_counts: Vec<usize>,
    local_states: Vec<SongLuaOverlayState>,
    overlay_states: Vec<SongLuaOverlayState>,
    background_local_states: Vec<Vec<SongLuaOverlayState>>,
    background_states: Vec<Vec<SongLuaOverlayState>>,
    foreground_local_states: Vec<Vec<SongLuaOverlayState>>,
    foreground_states: Vec<Vec<SongLuaOverlayState>>,
    capture_states: Vec<SongLuaOverlayState>,
    order: Vec<usize>,
    capture_order: Vec<usize>,
}

#[cfg(any(test, feature = "bench-support"))]
impl SongLuaScratchPrewarmBenchmark {
    pub fn cold(
        main_count: usize,
        background_counts: &[usize],
        foreground_counts: &[usize],
    ) -> Self {
        Self::new(main_count, background_counts, foreground_counts, false)
    }

    pub fn prewarmed(
        main_count: usize,
        background_counts: &[usize],
        foreground_counts: &[usize],
    ) -> Self {
        Self::new(main_count, background_counts, foreground_counts, true)
    }

    fn new(
        main_count: usize,
        background_counts: &[usize],
        foreground_counts: &[usize],
        prewarmed: bool,
    ) -> Self {
        let max_count = std::iter::once(main_count)
            .chain(background_counts.iter().copied())
            .chain(foreground_counts.iter().copied())
            .max()
            .unwrap_or(0);
        let state_vec = |count| {
            if prewarmed {
                Vec::with_capacity(count)
            } else {
                Vec::new()
            }
        };
        let layer_vec = |counts: &[usize]| {
            if prewarmed {
                counts.iter().map(|&count| state_vec(count)).collect()
            } else {
                Vec::new()
            }
        };
        Self {
            main_count,
            background_counts: background_counts.to_vec(),
            foreground_counts: foreground_counts.to_vec(),
            local_states: state_vec(main_count),
            overlay_states: state_vec(main_count),
            background_local_states: layer_vec(background_counts),
            background_states: layer_vec(background_counts),
            foreground_local_states: layer_vec(foreground_counts),
            foreground_states: layer_vec(foreground_counts),
            capture_states: state_vec(max_count),
            order: if prewarmed {
                Vec::with_capacity(max_count)
            } else {
                Vec::new()
            },
            capture_order: if prewarmed {
                Vec::with_capacity(max_count)
            } else {
                Vec::new()
            },
        }
    }

    pub fn opening_frame(&mut self) -> usize {
        self.local_states
            .resize(self.main_count, SongLuaOverlayState::default());
        self.overlay_states
            .resize(self.main_count, SongLuaOverlayState::default());
        Self::fill_layers(&self.background_counts, &mut self.background_local_states);
        Self::fill_layers(&self.background_counts, &mut self.background_states);
        Self::fill_layers(&self.foreground_counts, &mut self.foreground_local_states);
        Self::fill_layers(&self.foreground_counts, &mut self.foreground_states);
        let max_count = std::iter::once(self.main_count)
            .chain(self.background_counts.iter().copied())
            .chain(self.foreground_counts.iter().copied())
            .max()
            .unwrap_or(0);
        self.capture_states
            .resize(max_count, SongLuaOverlayState::default());
        self.order.extend(0..max_count);
        self.capture_order.extend(0..max_count);
        self.local_states.len()
            + self.overlay_states.len()
            + self
                .background_local_states
                .iter()
                .map(Vec::len)
                .sum::<usize>()
            + self.background_states.iter().map(Vec::len).sum::<usize>()
            + self
                .foreground_local_states
                .iter()
                .map(Vec::len)
                .sum::<usize>()
            + self.foreground_states.iter().map(Vec::len).sum::<usize>()
            + self.capture_states.len()
            + self.order.len()
            + self.capture_order.len()
    }

    fn fill_layers(counts: &[usize], layers: &mut Vec<Vec<SongLuaOverlayState>>) {
        layers.resize_with(counts.len(), Vec::new);
        for (&count, states) in counts.iter().zip(layers) {
            states.resize(count, SongLuaOverlayState::default());
        }
    }

    pub fn storage_bytes(&self) -> usize {
        let state_bytes = std::mem::size_of::<SongLuaOverlayState>();
        let index_bytes = std::mem::size_of::<usize>();
        let nested_state_capacity = |layers: &[Vec<SongLuaOverlayState>]| {
            layers.iter().map(Vec::capacity).sum::<usize>() * state_bytes
        };
        (self.local_states.capacity()
            + self.overlay_states.capacity()
            + self.capture_states.capacity())
            * state_bytes
            + nested_state_capacity(&self.background_local_states)
            + nested_state_capacity(&self.background_states)
            + nested_state_capacity(&self.foreground_local_states)
            + nested_state_capacity(&self.foreground_states)
            + (self.order.capacity() + self.capture_order.capacity()) * index_bytes
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct SongLuaAftCaptureBenchmark {
    overlay: SongLuaOverlayActor,
    state: SongLuaOverlayState,
    source: Vec<Actor>,
    banks: [SharedActorFrameScratch; SONG_LUA_AFT_FRAME_BANKS],
    active_bank: usize,
    retained: Option<Actor>,
}

#[cfg(feature = "bench-support")]
impl SongLuaAftCaptureBenchmark {
    pub fn new(actor_count: usize) -> Self {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::AftSprite {
                capture_name: "bench".to_string(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let state = SongLuaOverlayState {
            x: 0.5 * screen_width(),
            y: 0.5 * screen_height(),
            diffuse: [0.75, 0.5, 0.25, 0.8],
            blend: SongLuaOverlayBlendMode::Add,
            ..SongLuaOverlayState::default()
        };
        let source = (0..actor_count)
            .map(|index| Actor::Frame {
                align: [0.0, 0.0],
                offset: [index as f32, 0.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: Vec::new(),
                background: None,
                z: index.min(i16::MAX as usize) as i16,
            })
            .collect();
        Self {
            overlay,
            state,
            source,
            banks: std::array::from_fn(|_| SharedActorFrameScratch::with_capacity(actor_count)),
            active_bank: SONG_LUA_AFT_FRAME_BANKS - 1,
            retained: None,
        }
    }

    pub fn old_frame(&mut self) -> u64 {
        let source = self.source.to_vec();
        let actor = song_lua_build_capture_actor(
            &self.overlay,
            self.state,
            100,
            source,
            screen_width(),
            screen_height(),
        )
        .expect("benchmark capture source is visible and nonempty");
        std::hint::black_box(&actor);
        let checksum = self.checksum();
        self.retained = Some(actor);
        checksum
    }

    pub fn shared_frame(&mut self) -> u64 {
        self.active_bank = (self.active_bank + 1) % SONG_LUA_AFT_FRAME_BANKS;
        let source = &self.source;
        let actor = song_lua_build_shared_capture(
            &self.overlay,
            self.state,
            100,
            screen_width(),
            screen_height(),
            &mut self.banks[self.active_bank],
            |out| out.extend(source.iter().cloned()),
        )
        .expect("benchmark capture source is visible and nonempty");
        std::hint::black_box(&actor);
        let checksum = self.checksum();
        self.retained = Some(actor);
        checksum
    }

    pub fn storage_bytes(&self) -> usize {
        self.banks
            .iter()
            .map(|bank| bank.capacity().saturating_mul(std::mem::size_of::<Actor>()))
            .sum()
    }

    fn checksum(&self) -> u64 {
        self.source.iter().fold(
            u64::from(self.state.diffuse[3].to_bits()),
            |checksum, actor| {
                let z = match actor {
                    Actor::Frame { z, .. } => song_lua_add_z(*z, 100),
                    _ => 0,
                };
                checksum.rotate_left(7) ^ z as u16 as u64
            },
        )
    }
}

#[cfg(feature = "bench-support")]
fn append_benchmark_projected_grid(
    xs: &[f32],
    ys: &[f32],
    grid: &mut impl Extend<TexturedMeshVertex>,
) {
    for &y in ys {
        for &x in xs {
            grid.extend([TexturedMeshVertex {
                pos: [x * 640.0, y * 480.0, 0.0],
                uv: [x, y],
                tex_matrix_scale: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, x.min(y)],
            }]);
        }
    }
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

fn append_projected_mesh_vertices(
    grid: &[TexturedMeshVertex],
    width: usize,
    height: usize,
    vertices: &mut Vec<TexturedMeshVertex>,
) {
    vertices.reserve(width.saturating_sub(1) * height.saturating_sub(1) * 6);
    for y in 0..height.saturating_sub(1) {
        for x in 0..width.saturating_sub(1) {
            let tl = y * width + x;
            let tr = tl + 1;
            let bl = (y + 1) * width + x;
            let br = bl + 1;
            vertices
                .extend_from_slice(&[grid[tl], grid[tr], grid[br], grid[tl], grid[br], grid[bl]]);
        }
    }
}

struct SongLuaProjectedMeshParams {
    texture: Arc<str>,
    tint: [f32; 4],
    glow: [f32; 4],
    world_z: f32,
    depth_test: bool,
    visible: bool,
    blend: BlendMode,
    z: i16,
}

fn song_lua_projected_mesh_actor_from_grid(
    params: SongLuaProjectedMeshParams,
    grid: &[TexturedMeshVertex],
    width: usize,
    height: usize,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Actor {
    if let Some(scratch) = scratch {
        return Actor::ReusableTexturedMesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            world_z: params.world_z,
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            local_transform: Matrix4::IDENTITY,
            texture: params.texture,
            tint: params.tint,
            glow: params.glow,
            vertices: scratch.update_projected(grid, width, height),
            geom_cache_key: INVALID_TMESH_CACHE_KEY,
            uv_scale: [1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_tex_shift: [0.0, 0.0],
            depth_test: params.depth_test,
            visible: params.visible,
            blend: params.blend,
            z: params.z,
        };
    }
    let mut vertices = Vec::with_capacity(
        width
            .saturating_sub(1)
            .saturating_mul(height.saturating_sub(1))
            .saturating_mul(6),
    );
    append_projected_mesh_vertices(grid, width, height, &mut vertices);
    Actor::TexturedMesh {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        world_z: params.world_z,
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        local_transform: Matrix4::IDENTITY,
        texture: params.texture,
        tint: params.tint,
        glow: params.glow,
        vertices: Arc::from(vertices.into_boxed_slice()),
        geom_cache_key: INVALID_TMESH_CACHE_KEY,
        uv_scale: [1.0, 1.0],
        uv_offset: [0.0, 0.0],
        uv_tex_shift: [0.0, 0.0],
        depth_test: params.depth_test,
        visible: params.visible,
        blend: params.blend,
        z: params.z,
    }
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
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
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
    let mut grid = SmallVec::<[TexturedMeshVertex; 16]>::new();
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
    Some(song_lua_projected_mesh_actor_from_grid(
        SongLuaProjectedMeshParams {
            world_z,
            depth_test: state.depth_test,
            visible: state.visible,
            glow: [1.0, 1.0, 1.0, 0.0],
            texture,
            tint,
            blend,
            z,
        },
        &grid,
        xs.len(),
        ys.len(),
        scratch,
    ))
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
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
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
    let mut grid = SmallVec::<[TexturedMeshVertex; 16]>::new();
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
    Some(song_lua_projected_mesh_actor_from_grid(
        SongLuaProjectedMeshParams {
            world_z: state.z_bias,
            depth_test: state.depth_test,
            visible: true,
            glow: [1.0, 1.0, 1.0, 0.0],
            texture,
            tint,
            blend,
            z,
        },
        &grid,
        xs.len(),
        ys.len(),
        scratch,
    ))
}

type SongLuaActorList = SmallVec<[Actor; 2]>;

/// Emits compiled multi-output overlays directly into the caller's reused
/// frame buffer. Returning `None` means the overlay is a single-output kind and
/// should continue through the compact inline builder.
#[allow(clippy::too_many_arguments)]
fn append_song_lua_multi_actor_overlay(
    out: &mut Vec<Actor>,
    overlay: &SongLuaOverlayActor,
    state: SongLuaOverlayState,
    asset_manager: &AssetManager,
    z: i16,
    overlay_space_width: f32,
    overlay_space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<bool> {
    if !matches!(
        overlay.kind,
        SongLuaOverlayKind::Model { .. } | SongLuaOverlayKind::NoteskinActor { .. }
    ) {
        return None;
    }
    if !state.visible || !song_lua_overlay_has_visible_output(state) {
        return Some(false);
    }

    let x_scale = screen_width() / overlay_space_width.max(1.0);
    let y_scale = screen_height() / overlay_space_height.max(1.0);
    let overlay_scale = song_lua_overlay_axis_scale(state);
    let actor_scale = [overlay_scale[0].abs(), overlay_scale[1].abs()];
    let effect = song_lua_overlay_effect_state(state);
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
    let blend = song_lua_overlay_blend(state.blend);

    Some(match &overlay.kind {
        SongLuaOverlayKind::Model { layers } => {
            let (geometry_keys, glow_vertices) =
                scratch.as_deref().map_or((None, None), |scratch| {
                    (
                        scratch.model_geometry_keys.as_deref(),
                        scratch.model_glow_vertices.as_deref(),
                    )
                });
            append_song_lua_model_actors(
                out,
                layers,
                state,
                asset_manager,
                z,
                x_scale,
                y_scale,
                actor_scale,
                effect_scale,
                effect_rot,
                effect_offset,
                tint,
                glow,
                blend,
                total_elapsed,
                geometry_keys,
                glow_vertices,
            )
        }
        SongLuaOverlayKind::NoteskinActor { slots } => append_song_lua_noteskin_actors(
            out,
            slots,
            state,
            asset_manager,
            z,
            x_scale,
            y_scale,
            actor_scale,
            effect_scale,
            effect_rot,
            effect_offset,
            tint,
            glow,
            blend,
            total_elapsed,
            effect_beat,
            scratch,
        ),
        _ => false,
    })
}

fn build_song_lua_overlay_actor_with_scratch(
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
    mut projected_mesh_scratch: Option<&mut SongLuaProjectedMeshScratch>,
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
    let finalize_actor = |actor, glow, scratch| {
        song_lua_finalize_overlay_actor(state, actor, glow, x_scale, y_scale, scratch)
    };
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
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
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
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
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
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
        SongLuaOverlayKind::BitmapText {
            font_name,
            text,
            stroke_color,
            attributes,
            ..
        } => {
            let content = if state.uppercase {
                projected_mesh_scratch
                    .as_deref()
                    .and_then(|scratch| scratch.uppercase_text.as_ref())
                    .map_or_else(
                        || TextContent::from(text.to_uppercase()),
                        |uppercase| TextContent::from(Arc::clone(uppercase)),
                    )
            } else {
                TextContent::from(text)
            };
            let font = if asset_manager.with_font(font_name, |_| ()).is_some() {
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
                let attributes = projected_mesh_scratch.as_deref_mut().map_or_else(
                    || song_lua_rainbow_scroll_attributes(content.as_str(), total_elapsed).into(),
                    |scratch| scratch.rainbow_attributes(content.as_str(), total_elapsed),
                );
                (attributes, color)
            } else {
                song_lua_text_attributes_for_diffuse_mode(
                    attributes,
                    color,
                    content.as_str(),
                    state.mult_attrs_with_diffuse,
                    projected_mesh_scratch.as_deref_mut(),
                )
            };
            let actor = Actor::Text {
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
            };
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
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
                let mesh_actor = if let Some(scratch) = projected_mesh_scratch.as_deref_mut() {
                    let mesh = scratch.update_textured(|out| {
                        append_song_lua_actor_multi_vertex_textured_mesh(
                            out,
                            vertices,
                            x_scale,
                            y_scale,
                            [size_scale_x, size_scale_y],
                            effect_scale,
                            effect_rot[2],
                            [state.skew_x, state.skew_y],
                        );
                    });
                    Actor::ReusableTexturedMesh {
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
                    }
                } else {
                    let mesh = song_lua_actor_multi_vertex_textured_mesh(
                        vertices,
                        x_scale,
                        y_scale,
                        [size_scale_x, size_scale_y],
                        effect_scale,
                        effect_rot[2],
                        [state.skew_x, state.skew_y],
                    );
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
                    }
                };
                return Some(finalize_actor(
                    mesh_actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
            }
            let mesh_actor = if let Some(scratch) = projected_mesh_scratch.as_deref_mut() {
                let mesh = scratch.update_mesh(|out| {
                    append_song_lua_actor_multi_vertex_mesh(
                        out,
                        vertices,
                        tint,
                        x_scale,
                        y_scale,
                        [size_scale_x, size_scale_y],
                        effect_scale,
                        effect_rot[2],
                        [state.skew_x, state.skew_y],
                    );
                });
                Actor::ReusableMesh {
                    align: [0.0, 0.0],
                    offset: [
                        state.x * x_scale + effect_offset[0] * x_scale,
                        state.y * y_scale + effect_offset[1] * y_scale,
                    ],
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    tint: [1.0; 4],
                    vertices: mesh,
                    visible: state.visible,
                    blend: overlay_blend,
                    z,
                }
            } else {
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
                Actor::Mesh {
                    align: [0.0, 0.0],
                    offset: [
                        state.x * x_scale + effect_offset[0] * x_scale,
                        state.y * y_scale + effect_offset[1] * y_scale,
                    ],
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    tint: [1.0; 4],
                    vertices: mesh,
                    visible: state.visible,
                    blend: overlay_blend,
                    z,
                }
            };
            Some(finalize_actor(
                mesh_actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
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
            let mut out = SongLuaActorList::new();
            let (geometry_keys, glow_vertices) =
                projected_mesh_scratch
                    .as_deref()
                    .map_or((None, None), |scratch| {
                        (
                            scratch.model_geometry_keys.as_deref(),
                            scratch.model_glow_vertices.as_deref(),
                        )
                    });
            append_song_lua_model_actors(
                &mut out,
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
                geometry_keys,
                glow_vertices,
            )
            .then_some(out)
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
            let mut out = SongLuaActorList::new();
            append_song_lua_noteskin_actors(
                &mut out,
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
                projected_mesh_scratch.as_deref_mut(),
            )
            .then_some(out)
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
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
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
                **line_state,
                *size,
                x_scale,
                y_scale,
                z,
                projected_mesh_scratch.as_deref_mut(),
            )?;
            let glow = [
                state.glow[0] + body_state.glow[0].max(line_state.glow[0]),
                state.glow[1] + body_state.glow[1].max(line_state.glow[1]),
                state.glow[2] + body_state.glow[2].max(line_state.glow[2]),
                state.glow[3].max(body_state.glow[3].max(line_state.glow[3])),
            ];
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
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
                    white_texture_key(),
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
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
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
                    white_texture_key(),
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
                    projected_mesh_scratch.as_deref_mut(),
                )?;
                return Some(finalize_actor(
                    actor,
                    glow,
                    projected_mesh_scratch.as_deref_mut(),
                ));
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
            Some(finalize_actor(
                actor,
                glow,
                projected_mesh_scratch.as_deref_mut(),
            ))
        }
    }
}

#[cfg(test)]
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
    build_song_lua_overlay_actor_with_scratch(
        overlay,
        state,
        camera_state,
        asset_manager,
        z,
        overlay_space_width,
        overlay_space_height,
        effect_time,
        effect_beat,
        total_elapsed,
        None,
    )
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
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> Option<Actor> {
    song_lua_overlay_glow_actor_with_static_vertices(actor, glow, text_glow_mode, scratch, None)
}

fn song_lua_overlay_glow_actor_with_static_vertices(
    actor: &Actor,
    glow: [f32; 4],
    text_glow_mode: SongLuaTextGlowMode,
    mut scratch: Option<&mut SongLuaProjectedMeshScratch>,
    prewarmed_static_vertices: Option<&Arc<[TexturedMeshVertex]>>,
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
            let has_attr_glow = song_lua_text_attributes_have_glow(base_attributes.as_slice());
            if glow[3] <= f32::EPSILON && !has_attr_glow {
                return None;
            }
            let (attributes, color, stroke_color) = if has_attr_glow {
                let attributes = song_lua_text_glow_attributes(
                    content.as_str(),
                    base_attributes.as_slice(),
                    glow,
                    scratch.as_deref_mut(),
                );
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
                        attributes = song_lua_transparent_text_attributes(
                            content.as_str(),
                            scratch.as_deref_mut(),
                        );
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
                font,
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
            geom_cache_key,
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
            let glow_vertices = scratch
                .as_deref_mut()
                .map(|scratch| scratch.update_textured_glow(vertices.as_ref()));
            let actor = if let Some(vertices) = prewarmed_static_vertices {
                Actor::TexturedMesh {
                    align: *align,
                    offset: *offset,
                    world_z: *world_z,
                    size: *size,
                    local_transform: *local_transform,
                    texture: texture.clone(),
                    tint: [1.0, 1.0, 1.0, 0.0],
                    glow,
                    vertices: Arc::clone(vertices),
                    geom_cache_key: song_lua_glow_geometry_key(*geom_cache_key),
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                    depth_test: *depth_test,
                    visible: *visible,
                    blend: BlendMode::Add,
                    z: *z,
                }
            } else if let Some(vertices) = glow_vertices {
                Actor::ReusableTexturedMesh {
                    align: *align,
                    offset: *offset,
                    world_z: *world_z,
                    size: *size,
                    local_transform: *local_transform,
                    texture: texture.clone(),
                    tint: [1.0, 1.0, 1.0, 0.0],
                    glow,
                    vertices,
                    geom_cache_key: INVALID_TMESH_CACHE_KEY,
                    uv_scale: *uv_scale,
                    uv_offset: *uv_offset,
                    uv_tex_shift: *uv_tex_shift,
                    depth_test: *depth_test,
                    visible: *visible,
                    blend: BlendMode::Add,
                    z: *z,
                }
            } else {
                let mut glow_vertices = vertices.as_ref().to_vec();
                for vertex in &mut glow_vertices {
                    vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                }
                Actor::TexturedMesh {
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
                }
            };
            Some(actor)
        }
        Actor::ReusableTexturedMesh {
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
            let glow_vertices = scratch.as_deref_mut().map_or_else(
                || {
                    let mut out = Vec::with_capacity(vertices.len());
                    out.extend(vertices.iter().copied().map(|mut vertex| {
                        vertex.color = [1.0, 1.0, 1.0, vertex.color[3]];
                        vertex
                    }));
                    Arc::new(out)
                },
                |scratch| scratch.update_textured_glow(vertices),
            );
            Some(Actor::ReusableTexturedMesh {
                align: *align,
                offset: *offset,
                world_z: *world_z,
                size: *size,
                local_transform: *local_transform,
                texture: texture.clone(),
                tint: [1.0, 1.0, 1.0, 0.0],
                glow,
                vertices: glow_vertices,
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
    scratch: Option<&mut SongLuaProjectedMeshScratch>,
) -> SongLuaActorList {
    let glow_actor = song_lua_overlay_glow_actor(&actor, glow, state.text_glow_mode, scratch);
    let actor = song_lua_wrap_overlay_shadow(state, actor, x_scale, y_scale);
    let mut out = SmallVec::new();
    out.push(actor);
    if let Some(glow_actor) = glow_actor {
        out.push(glow_actor);
    }
    out
}

#[inline(always)]
fn push_song_lua_capture_actor(
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

#[allow(clippy::too_many_arguments)]
fn append_song_lua_player_transform<F, H>(
    field_actors: F,
    hud_actors: H,
    field_len: usize,
    hud_len: usize,
    field_has_camera: bool,
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
) where
    F: IntoIterator<Item = Actor>,
    H: IntoIterator<Item = Actor>,
{
    let fold_y = |actor| {
        if rotation_y_deg.is_finite() && rotation_y_deg.abs() > f32::EPSILON {
            song_lua_player_y_fold_actor(actor, playfield_center_x, rotation_y_deg)
        } else {
            actor
        }
    };
    let Some(player_transform) = song_lua_player_transform_matrix(SongLuaPlayerTransformRequest {
        screen_width: screen_width(),
        screen_height: screen_height(),
        screen_center_y: screen_center_y(),
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
    }) else {
        out.reserve(field_len.saturating_add(hud_len));
        for actor in hud_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, [1.0; 4], None, z_shift);
        }
        for actor in field_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, [1.0; 4], None, z_shift);
        }
        return;
    };

    let root_camera = glam::camera::rh::proj::opengl::orthographic(
        -0.5 * screen_width(),
        0.5 * screen_width(),
        -0.5 * screen_height(),
        0.5 * screen_height(),
        -4096.0,
        4096.0,
    ) * player_transform;
    out.reserve(field_len.saturating_add(hud_len).saturating_add(4));
    if !field_has_camera {
        if field_len + hud_len == 0 {
            return;
        }
        push_song_lua_capture_actor(
            out,
            Actor::CameraPush {
                view_proj: root_camera,
            },
            tint,
            blend,
            z_shift,
        );
        for actor in hud_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, tint, blend, z_shift);
        }
        for actor in field_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, tint, blend, z_shift);
        }
        push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
        return;
    }

    let mut root_open = hud_len > 0;
    if root_open {
        push_song_lua_capture_actor(
            out,
            Actor::CameraPush {
                view_proj: root_camera,
            },
            tint,
            blend,
            z_shift,
        );
        for actor in hud_actors.into_iter().map(fold_y) {
            push_song_lua_capture_actor(out, actor, tint, blend, z_shift);
        }
    }
    let mut field_camera_depth = 0usize;
    for actor in field_actors.into_iter().map(fold_y) {
        match actor {
            Actor::Camera {
                view_proj,
                children,
            } => {
                if field_camera_depth == 0 && root_open {
                    push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
                    root_open = false;
                }
                push_song_lua_capture_actor(
                    out,
                    Actor::CameraPush {
                        view_proj: view_proj * player_transform,
                    },
                    tint,
                    blend,
                    z_shift,
                );
                for child in children {
                    push_song_lua_capture_actor(out, child, tint, blend, z_shift);
                }
                push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
            }
            Actor::CameraPush { view_proj } => {
                if field_camera_depth == 0 && root_open {
                    push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
                    root_open = false;
                }
                push_song_lua_capture_actor(
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
                push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
                field_camera_depth = field_camera_depth.saturating_sub(1);
            }
            other if field_camera_depth > 0 => {
                push_song_lua_capture_actor(out, other, tint, blend, z_shift);
            }
            other => {
                if !root_open {
                    push_song_lua_capture_actor(
                        out,
                        Actor::CameraPush {
                            view_proj: root_camera,
                        },
                        tint,
                        blend,
                        z_shift,
                    );
                    root_open = true;
                }
                push_song_lua_capture_actor(out, other, tint, blend, z_shift);
            }
        }
    }
    if root_open {
        push_song_lua_capture_actor(out, Actor::CameraPop, tint, blend, z_shift);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_song_lua_player_transform_legacy(
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
    out.clear();
    let field_len = field_actors.len();
    let hud_len = hud_actors.len();
    let field_has_camera = field_actors.iter().any(|actor| {
        matches!(
            actor,
            Actor::Camera { .. } | Actor::CameraPush { .. } | Actor::CameraPop
        )
    });
    append_song_lua_player_transform(
        field_actors.drain(..),
        hud_actors.drain(..),
        field_len,
        hud_len,
        field_has_camera,
        out,
        z_shift,
        tint,
        blend,
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_z_deg,
        rotation_y_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    );
}

#[inline(always)]
fn song_lua_player_transform_is_direct_identity(transform: SongLuaCaptureTransform) -> bool {
    transform.z_shift == 0
        && transform.tint == [1.0; 4]
        && transform.blend.is_none()
        && screen_width().is_finite()
        && screen_height().is_finite()
        && screen_center_y().is_finite()
        && transform.playfield_center_x.is_finite()
        && transform.target_x.is_finite()
        && transform.target_y.is_finite()
        && transform.rotation_x.is_finite()
        && transform.rotation_x.abs() <= f32::EPSILON
        && transform.rotation_z.is_finite()
        && transform.rotation_z.abs() <= f32::EPSILON
        && transform.rotation_y.is_finite()
        && transform.rotation_y.abs() <= f32::EPSILON
        && transform.skew_x.is_finite()
        && transform.skew_x.abs() <= f32::EPSILON
        && transform.skew_y.is_finite()
        && transform.skew_y.abs() <= f32::EPSILON
        && transform.zoom_x.is_finite()
        && (transform.zoom_x - 1.0).abs() <= f32::EPSILON
        && transform.zoom_y.is_finite()
        && (transform.zoom_y - 1.0).abs() <= f32::EPSILON
        && transform.zoom_z.is_finite()
        && (transform.zoom_z - 1.0).abs() <= f32::EPSILON
        && (transform.target_x - transform.playfield_center_x).abs() <= f32::EPSILON
        && (screen_center_y() - transform.target_y).abs() <= f32::EPSILON
}

#[allow(clippy::too_many_arguments)]
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
    let direct_identity = song_lua_player_transform_is_direct_identity(SongLuaCaptureTransform {
        z_shift,
        tint,
        blend,
        playfield_center_x,
        target_x,
        target_y,
        rotation_x: rotation_x_deg,
        rotation_z: rotation_z_deg,
        rotation_y: rotation_y_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    });
    if direct_identity {
        out.clear();
        out.reserve(field_actors.len().saturating_add(hud_actors.len()));
        out.append(hud_actors);
        out.append(field_actors);
        return;
    }

    apply_song_lua_player_transform_legacy(
        field_actors,
        hud_actors,
        out,
        z_shift,
        tint,
        blend,
        playfield_center_x,
        target_x,
        target_y,
        rotation_x_deg,
        rotation_z_deg,
        rotation_y_deg,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
    );
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_present_identity_notefield_legacy(
    field_actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    out: &mut Vec<Actor>,
) {
    apply_song_lua_player_transform_legacy(
        field_actors,
        hud_actors,
        out,
        0,
        [1.0; 4],
        None,
        screen_center_x(),
        screen_center_x(),
        screen_center_y(),
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        1.0,
        1.0,
    );
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_present_identity_notefield(
    field_actors: &mut Vec<Actor>,
    hud_actors: &mut Vec<Actor>,
    out: &mut Vec<Actor>,
) {
    apply_song_lua_player_transform(
        field_actors,
        hud_actors,
        out,
        0,
        [1.0; 4],
        None,
        screen_center_x(),
        screen_center_x(),
        screen_center_y(),
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        1.0,
        1.0,
    );
}

#[inline(always)]
fn append_player_actors(out: &mut Vec<Actor>, player_scratch: &mut Vec<Actor>) {
    out.append(player_scratch);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlayerActorAssembly {
    Buffered,
    DirectIdentity,
}

#[derive(Clone, Copy, Debug)]
struct PlayerActorSegment {
    player: usize,
    assembly: PlayerActorAssembly,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayActorSegments {
    insert: usize,
    players: [Option<PlayerActorSegment>; 2],
}

impl GameplayActorSegments {
    pub const fn empty(insert: usize) -> Self {
        Self {
            insert,
            players: [None; 2],
        }
    }

    pub fn slices<'a>(&self, state: &'a State, actors: &'a [Actor]) -> [&'a [Actor]; 6] {
        let insert = self.insert.min(actors.len());
        let empty = &actors[0..0];
        let mut slices = [empty; 6];
        let scratch = state
            .frame_scratch
            .as_deref()
            .expect("gameplay frame scratch is restored before segment access");
        slices[0] = &actors[..insert];
        for (slot, segment) in self.players.iter().enumerate() {
            let Some(segment) = segment else {
                continue;
            };
            let first = 1 + slot * 2;
            match segment.assembly {
                PlayerActorAssembly::Buffered => {
                    slices[first] = &scratch.player_actor_scratch[segment.player];
                }
                PlayerActorAssembly::DirectIdentity => {
                    slices[first] = &scratch.notefield_hud_actor_scratch[segment.player];
                    slices[first + 1] = &scratch.notefield_actor_scratch[segment.player];
                }
            }
        }
        slices[5] = &actors[insert..];
        slices
    }
}

#[inline(always)]
fn player_actor_assembly_for_transform(
    requests_player_proxy: bool,
    visible: bool,
    transform: SongLuaCaptureTransform,
) -> PlayerActorAssembly {
    if !requests_player_proxy && visible && song_lua_player_transform_is_direct_identity(transform)
    {
        PlayerActorAssembly::DirectIdentity
    } else {
        PlayerActorAssembly::Buffered
    }
}

#[inline(always)]
fn player_actor_bundle_len(
    assembly: PlayerActorAssembly,
    field_scratch: &[Actor],
    hud_scratch: &[Actor],
    player_scratch: &[Actor],
) -> usize {
    match assembly {
        PlayerActorAssembly::Buffered => player_scratch.len(),
        PlayerActorAssembly::DirectIdentity => {
            field_scratch.len().saturating_add(hud_scratch.len())
        }
    }
}

#[inline(always)]
fn append_player_actor_bundle(
    out: &mut Vec<Actor>,
    assembly: PlayerActorAssembly,
    field_scratch: &mut Vec<Actor>,
    hud_scratch: &mut Vec<Actor>,
    player_scratch: &mut Vec<Actor>,
) {
    match assembly {
        PlayerActorAssembly::Buffered => append_player_actors(out, player_scratch),
        PlayerActorAssembly::DirectIdentity => {
            out.append(hud_scratch);
            out.append(field_scratch);
        }
    }
}

#[inline(always)]
fn clear_player_actor_bundle(
    field_scratch: &mut Vec<Actor>,
    hud_scratch: &mut Vec<Actor>,
    player_scratch: &mut Vec<Actor>,
) {
    field_scratch.clear();
    hud_scratch.clear();
    player_scratch.clear();
}

#[inline(always)]
#[cfg(any(test, feature = "bench-support"))]
#[allow(clippy::extend_with_drain)] // Preserve the slower baseline used by the transfer benchmark.
fn append_player_actors_legacy(out: &mut Vec<Actor>, player_scratch: &mut Vec<Actor>) {
    out.extend(player_scratch.drain(..));
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_append_player_actors_legacy(
    out: &mut Vec<Actor>,
    player_scratch: &mut Vec<Actor>,
) {
    append_player_actors_legacy(out, player_scratch);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_append_player_actors(out: &mut Vec<Actor>, player_scratch: &mut Vec<Actor>) {
    append_player_actors(out, player_scratch);
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn benchmark_append_direct_identity_player_actors(
    out: &mut Vec<Actor>,
    field_scratch: &mut Vec<Actor>,
    hud_scratch: &mut Vec<Actor>,
    player_scratch: &mut Vec<Actor>,
) {
    player_scratch.clear();
    append_player_actor_bundle(
        out,
        PlayerActorAssembly::DirectIdentity,
        field_scratch,
        hud_scratch,
        player_scratch,
    );
}

fn song_lua_player_target_x(
    explicit_x: Option<f32>,
    player_state_x: f32,
    layout_center_x: f32,
    notefield_view: ViewOverride,
) -> f32 {
    explicit_x.unwrap_or(if notefield_view.force_center_1player {
        layout_center_x
    } else {
        player_state_x
    })
}

fn prepare_song_lua_layer(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &mut SongLuaOverlayTopologyIndex,
    order_scratch: &mut Vec<usize>,
    aft_capture_scratch: &mut SongLuaAftCaptureScratch,
) -> Option<i16> {
    if overlays.is_empty() {
        order_scratch.clear();
        return None;
    }
    Some(prepare_active_song_lua_layer(
        out,
        overlays,
        overlay_states,
        song_foreground_state,
        order_cache,
        topology_index,
        order_scratch,
        aft_capture_scratch,
    ))
}

fn prepare_active_song_lua_layer(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &mut SongLuaOverlayTopologyIndex,
    order_scratch: &mut Vec<usize>,
    aft_capture_scratch: &mut SongLuaAftCaptureScratch,
) -> i16 {
    aft_capture_scratch.begin_frame();
    let base_z = song_lua_add_z(
        SONG_LUA_OVERLAY_LAYER_Z_BASE,
        song_lua_rounded_z(song_foreground_state.z),
    );
    out.reserve(overlays.len());
    song_lua_overlay_order_into(overlays, overlay_states, order_cache, None, order_scratch);
    topology_index.prepare_rgb_aft_groups(overlay_states, order_scratch);
    base_z
}

fn push_song_lua_layer_actors(
    out: &mut Vec<Actor>,
    overlays: &[SongLuaOverlayActor],
    order_cache: &mut SongLuaOverlayOrderCache,
    topology_index: &mut SongLuaOverlayTopologyIndex,
    local_overlay_states: &[SongLuaOverlayState],
    overlay_states: &[SongLuaOverlayState],
    song_foreground_state: SongLuaOverlayState,
    proxy_sources: &SongLuaScreenProxySources<'_>,
    mut proxy_actor_scratch: Option<&mut SongLuaProxyActorScratch>,
    asset_manager: &AssetManager,
    space_width: f32,
    space_height: f32,
    effect_time: f32,
    effect_beat: f32,
    total_elapsed: f32,
    order_scratch: &mut Vec<usize>,
    capture_states: &mut Vec<SongLuaOverlayState>,
    capture_order_scratch: &mut Vec<usize>,
    aft_capture_scratch: &mut SongLuaAftCaptureScratch,
    projected_mesh_scratch: &mut [SongLuaProjectedMeshScratch],
) {
    let Some(song_lua_overlay_base_z) = prepare_song_lua_layer(
        out,
        overlays,
        overlay_states,
        song_foreground_state,
        order_cache,
        topology_index,
        order_scratch,
        aft_capture_scratch,
    ) else {
        return;
    };
    for (draw_idx, idx) in order_scratch.iter().copied().enumerate() {
        let Some(overlay) = overlays.get(idx) else {
            continue;
        };
        if topology_index
            .aft_ancestors
            .get(idx)
            .copied()
            .and_then(SongLuaOverlayIndex::get)
            .is_some()
        {
            continue;
        }
        let overlay_state = overlay_states
            .get(idx)
            .copied()
            .unwrap_or_else(SongLuaOverlayState::default);
        let z = song_lua_add_z(
            song_lua_overlay_base_z,
            draw_idx.min(i16::MAX as usize) as i16,
        );
        match &overlay.kind {
            SongLuaOverlayKind::ActorProxy { target } => {
                if let Some(actor) =
                    song_lua_proxy_source(target, proxy_sources).and_then(|source| {
                        song_lua_build_proxy_actor_with_scratch(
                            overlay_state,
                            z,
                            source,
                            space_width,
                            space_height,
                            proxy_actor_scratch.as_deref_mut(),
                        )
                    })
                {
                    out.push(actor);
                }
            }
            SongLuaOverlayKind::AftSprite { .. } => {
                let overlay_state = if let Some((leader, _)) = topology_index.rgb_aft_group(idx) {
                    if leader != idx {
                        continue;
                    }
                    song_lua_combined_rgb_aft_state(overlay_state)
                } else {
                    overlay_state
                };
                let capture_index = topology_index
                    .aft_sprite_targets
                    .get(idx)
                    .copied()
                    .and_then(SongLuaOverlayIndex::get);
                if let (Some(capture_index), Some(capture_scratch)) =
                    (capture_index, aft_capture_scratch.overlay(idx))
                {
                    if let Some(actor) = song_lua_build_shared_capture(
                        overlay,
                        overlay_state,
                        z,
                        space_width,
                        space_height,
                        capture_scratch,
                        |source| {
                            song_lua_capture_children_into(
                                source,
                                overlays,
                                overlay_states,
                                local_overlay_states,
                                order_cache,
                                topology_index,
                                asset_manager,
                                capture_index,
                                proxy_sources,
                                proxy_actor_scratch.as_deref_mut(),
                                space_width,
                                space_height,
                                capture_states,
                                capture_order_scratch,
                                projected_mesh_scratch,
                            );
                        },
                    ) {
                        out.push(actor);
                    }
                }
            }
            _ => {
                if append_song_lua_multi_actor_overlay(
                    out,
                    overlay,
                    overlay_state,
                    asset_manager,
                    z,
                    space_width,
                    space_height,
                    effect_time,
                    effect_beat,
                    total_elapsed,
                    projected_mesh_scratch.get_mut(idx),
                )
                .is_none()
                    && let Some(actors) = build_song_lua_overlay_actor_with_scratch(
                        overlay,
                        overlay_state,
                        topology_index.camera_state(overlay_states, idx),
                        asset_manager,
                        z,
                        space_width,
                        space_height,
                        effect_time,
                        effect_beat,
                        total_elapsed,
                        projected_mesh_scratch.get_mut(idx),
                    )
                {
                    out.extend(actors);
                }
            }
        }
    }
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &mut State,
    asset_manager: &AssetManager,
    view: ActorViewOverride,
    arrow_effect_time_s: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let _ = push_actors_impl(
        actors,
        state,
        asset_manager,
        view,
        arrow_effect_time_s,
        visual_policy,
        false,
    );
}

pub fn push_actors_segmented(
    actors: &mut Vec<Actor>,
    state: &mut State,
    asset_manager: &AssetManager,
    view: ActorViewOverride,
    arrow_effect_time_s: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) -> GameplayActorSegments {
    push_actors_impl(
        actors,
        state,
        asset_manager,
        view,
        arrow_effect_time_s,
        visual_policy,
        true,
    )
}

fn push_actors_impl(
    actors: &mut Vec<Actor>,
    state: &mut State,
    asset_manager: &AssetManager,
    view: ActorViewOverride,
    arrow_effect_time_s: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
    segmented: bool,
) -> GameplayActorSegments {
    let mut frame_scratch = state
        .frame_scratch
        .take()
        .expect("gameplay frame scratch is restored after every actor build");
    let GameplayFrameScratch {
        lobby_hud_cache,
        lobby_hud_status_scratch,
        bpm_text,
        song_lua_overlay_order,
        song_lua_background_visual_layer_orders,
        song_lua_foreground_visual_layer_orders,
        song_lua_background_layer_activity,
        song_lua_foreground_layer_activity,
        song_lua_proxy_request_index,
        song_lua_background_overlay_topology_indices,
        song_lua_foreground_proxy_request_indices,
        song_lua_aft_capture_scratch,
        song_lua_background_aft_capture_scratch,
        song_lua_foreground_aft_capture_scratch,
        song_lua_projected_mesh_scratch,
        song_lua_background_projected_mesh_scratch,
        song_lua_foreground_projected_mesh_scratch,
        song_lua_message_state_cache,
        song_lua_background_layer_message_state_cache,
        song_lua_foreground_layer_message_state_cache,
        song_lua_player_message_state_cache,
        song_lua_song_foreground_message_state_cache,
        song_lua_background_song_foreground_message_state_cache,
        song_lua_foreground_song_foreground_message_state_cache,
        song_lua_local_state_scratch,
        song_lua_overlay_state_scratch,
        song_lua_background_layer_local_state_scratch,
        song_lua_background_layer_state_scratch,
        song_lua_foreground_layer_local_state_scratch,
        song_lua_foreground_layer_state_scratch,
        song_lua_capture_state_scratch,
        song_lua_order_scratch,
        song_lua_capture_order_scratch,
        song_lua_capture_visit_scratch,
        song_lua_proxy_actor_scratch,
        notefield_actor_scratch,
        notefield_hud_actor_scratch,
        player_actor_scratch,
        presentation_skeleton,
    } = frame_scratch.as_mut();
    presentation_skeleton.prepare();
    for actors in player_actor_scratch.iter_mut() {
        actors.clear();
    }
    if let Some(scratch) = song_lua_proxy_actor_scratch.as_mut() {
        scratch.begin_frame();
    }

    let notefield_view = view.notefield;
    let hide_gameplay_hud = view.hide_gameplay_hud;
    actors.reserve(96);
    let play_style = state.hud_snapshot.play_style;
    let player_side = state.hud_snapshot.player_side;
    let is_p2_single = profile_data::is_single_p2_side(play_style, player_side);
    let runtime_player_is_p2 = profile_data::runtime_player_is_p2(play_style, player_side);
    let policy = state.runtime_view.policy;
    let center_1player_notefield =
        policy.center_single_notefield || notefield_view.force_center_1player;
    let centered_single_notefield = play_style == profile_data::PlayStyle::Single
        && state.num_players() == 1
        && center_1player_notefield;
    let song_lua_visuals = state.song_lua_visuals();
    let song_lua_space_width = song_lua_overlay_space_width(state);
    let song_lua_space_height = song_lua_overlay_space_height(state);
    let player_color = color::decorative_rgba(state.player_color_index());
    let song_lua_now = state.current_music_time_display();
    song_lua_overlay_state_sets_from_into(
        song_lua_now,
        &song_lua_visuals.overlays,
        &song_lua_visuals.overlay_events,
        &song_lua_visuals.overlay_eases,
        &song_lua_visuals.overlay_ease_ranges,
        song_lua_visuals.screen_width,
        song_lua_visuals.screen_height,
        &song_lua_overlay_order,
        song_lua_message_state_cache,
        song_lua_local_state_scratch,
        song_lua_overlay_state_scratch,
    );
    let song_lua_background_active_layers = song_lua_background_layer_activity.sync(song_lua_now);
    let song_lua_foreground_active_layers = song_lua_foreground_layer_activity.sync(song_lua_now);
    for &layer_idx in song_lua_background_active_layers {
        let layer = &song_lua_visuals.background_visual_layers[layer_idx];
        let local_states = &mut song_lua_background_layer_local_state_scratch[layer_idx];
        let layer_states = &mut song_lua_background_layer_state_scratch[layer_idx];
        let message_caches = &mut song_lua_background_layer_message_state_cache[layer_idx];
        song_lua_overlay_state_sets_from_into(
            song_lua_now,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &song_lua_background_visual_layer_orders[layer_idx],
            message_caches,
            local_states,
            layer_states,
        );
    }
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let local_states = &mut song_lua_foreground_layer_local_state_scratch[layer_idx];
        let layer_states = &mut song_lua_foreground_layer_state_scratch[layer_idx];
        let message_caches = &mut song_lua_foreground_layer_message_state_cache[layer_idx];
        song_lua_overlay_state_sets_from_into(
            song_lua_now,
            &layer.overlays,
            &layer.overlay_events,
            &layer.overlay_eases,
            &layer.overlay_ease_ranges,
            layer.screen_width,
            layer.screen_height,
            &song_lua_foreground_visual_layer_orders[layer_idx],
            message_caches,
            local_states,
            layer_states,
        );
    }
    let mut proxy_requests = song_lua_proxy_requests_indexed(
        &song_lua_visuals.overlays,
        &song_lua_overlay_state_scratch,
        &song_lua_proxy_request_index,
        song_lua_capture_visit_scratch,
    );
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let layer_states = &song_lua_foreground_layer_state_scratch[layer_idx];
        let request_index = &song_lua_foreground_proxy_request_indices[layer_idx];
        song_lua_merge_proxy_requests(
            &mut proxy_requests,
            song_lua_proxy_requests_indexed(
                &layer.overlays,
                layer_states,
                request_index,
                song_lua_capture_visit_scratch,
            ),
        );
    }
    let mut underlay_proxy_source = proxy_requests
        .underlay
        .then_some(SongLuaActorSegments::new());
    let mut overlay_proxy_source = proxy_requests
        .overlay
        .then_some(SongLuaActorSegments::new());
    // --- Background and Filter ---
    let underlay_start = actors.len();
    push_background(
        actors,
        state,
        policy.background_brightness,
        policy.background_color,
    );
    for &layer_idx in song_lua_background_active_layers {
        let layer = &song_lua_visuals.background_visual_layers[layer_idx];
        let local_states = &song_lua_background_layer_local_state_scratch[layer_idx];
        let layer_states = &song_lua_background_layer_state_scratch[layer_idx];
        let Some(order_cache) = song_lua_background_visual_layer_orders.get_mut(layer_idx) else {
            continue;
        };
        let Some(topology_index) = song_lua_background_overlay_topology_indices.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(aft_capture_scratch) = song_lua_background_aft_capture_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(projected_mesh_scratch) =
            song_lua_background_projected_mesh_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let song_foreground_state = song_lua_song_foreground_state_from(
            song_lua_now,
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
            &mut song_lua_background_song_foreground_message_state_cache[layer_idx],
        );
        push_song_lua_layer_actors(
            actors,
            &layer.overlays,
            order_cache,
            topology_index,
            local_states,
            layer_states,
            song_foreground_state,
            &SongLuaScreenProxySources::default(),
            None,
            asset_manager,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            song_lua_now,
            state.current_beat(),
            state.total_elapsed_in_screen(),
            song_lua_order_scratch,
            song_lua_capture_state_scratch,
            song_lua_capture_order_scratch,
            aft_capture_scratch,
            projected_mesh_scratch,
        );
    }
    song_lua_capture_new_actors(
        &mut underlay_proxy_source,
        actors,
        underlay_start,
        song_lua_proxy_actor_scratch.as_mut(),
    );
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
        song_lua_capture_new_actors(
            &mut overlay_proxy_source,
            actors,
            overlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
        );
    }

    // Hold START/BACK prompt (Simply Love parity: ScreenGameplay debug text).
    if !hide_gameplay_hud {
        let overlay_start = actors.len();
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
        song_lua_capture_new_actors(
            &mut overlay_proxy_source,
            actors,
            overlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
        );
    }

    if !hide_gameplay_hud {
        let overlay_start = actors.len();
        if state.runtime_view.lobby.snapshot.joined_lobby.is_some() {
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
        song_lua_capture_new_actors(
            &mut overlay_proxy_source,
            actors,
            overlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
        );
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
    song_lua_capture_new_actors(
        &mut overlay_proxy_source,
        actors,
        overlay_start,
        song_lua_proxy_actor_scratch.as_mut(),
    );

    let mut build_player_bundle =
        |player_idx: usize,
         profile: &profile_data::Profile,
         placement: FieldPlacement,
         requests: SongLuaPlayerProxyRequests| {
            let field_scratch = &mut notefield_actor_scratch[player_idx];
            let hud_scratch = &mut notefield_hud_actor_scratch[player_idx];
            let player_scratch = &mut player_actor_scratch[player_idx];
            let deadsync_notefield::BuiltNotefield {
                layout_center_x,
                field_actors,
                judgment_actors,
                combo_actors,
            } = notefield::compose_frame(
                state,
                state.notefield_judgment_assets(player_idx),
                state.notefield_plan(player_idx),
                player_idx,
                arrow_effect_time_s,
                &state.noteskin_assets,
                &visual_policy.assets.effects,
                state.actor_resources(),
                &state.notefield_model_cache,
                &state.notefield_hold_mesh_scratch,
                &state.notefield_capture_scratch,
                &state.notefield_broken_run_lookup[player_idx],
                &state.notefield_stream_progress_lookup[player_idx],
                profile,
                placement,
                play_style,
                center_1player_notefield,
                ProxyCaptureRequests {
                    note_field: requests.note_field,
                    judgment: requests.judgment,
                    combo: requests.combo,
                },
                state.itl_cmod_warning[player_idx],
                state.display_mods_text(player_idx),
                notefield_view,
                field_scratch,
                hud_scratch,
            );
            let player_actor = &song_lua_visuals.player_actors[player_idx];
            let player_state = song_lua_player_render_state(
                state,
                player_idx,
                &mut song_lua_player_message_state_cache[player_idx],
            );
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
            let capture_transform = SongLuaCaptureTransform {
                z_shift,
                tint: player_state.diffuse,
                blend: player_blend,
                playfield_center_x: layout_center_x,
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
            };
            let note_field_source = field_actors.as_ref().and_then(|source| {
                let scratch = song_lua_proxy_actor_scratch
                    .as_mut()?
                    .player(player_idx, SONG_LUA_FIELD_PROXY_SOURCE)?;
                song_lua_render_captured_source(Some(source), None, capture_transform, scratch)
            });
            let assembly = player_actor_assembly_for_transform(
                requests.player,
                player_state.visible,
                capture_transform,
            );
            if assembly == PlayerActorAssembly::DirectIdentity {
                player_scratch.clear();
            } else {
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
            }
            let player_source = if assembly == PlayerActorAssembly::Buffered {
                if requests.player {
                    let source = song_lua_proxy_actor_scratch
                        .as_mut()
                        .and_then(|scratch| {
                            scratch.player(player_idx, SONG_LUA_PLAYER_PROXY_SOURCE)
                        })
                        .and_then(|scratch| {
                            song_lua_share_actor_source_in_place(player_scratch, scratch)
                        });
                    if !player_state.visible {
                        player_scratch.clear();
                    }
                    source
                } else {
                    if !player_state.visible {
                        player_scratch.clear();
                    }
                    None
                }
            } else {
                None
            };
            let proxy_sources = [
                note_field_source,
                judgment_actors.as_ref().and_then(|source| {
                    let scratch = song_lua_proxy_actor_scratch
                        .as_mut()?
                        .player(player_idx, SONG_LUA_JUDGMENT_PROXY_SOURCE)?;
                    song_lua_render_captured_source(None, Some(source), capture_transform, scratch)
                }),
                combo_actors.as_ref().and_then(|source| {
                    let scratch = song_lua_proxy_actor_scratch
                        .as_mut()?
                        .player(player_idx, SONG_LUA_COMBO_PROXY_SOURCE)?;
                    song_lua_render_captured_source(None, Some(source), capture_transform, scratch)
                }),
            ];
            (layout_center_x, player_source, proxy_sources, assembly)
        };

    let (
        has_p2_actors,
        p1_player_proxy_source,
        p2_player_proxy_source,
        p1_proxy_sources,
        p2_proxy_sources,
        p1_actor_assembly,
        p2_actor_assembly,
        playfield_center_x,
        per_player_fields,
    ): (
        bool,
        Option<SongLuaSingleSource>,
        Option<SongLuaSingleSource>,
        [Option<SongLuaSingleSource>; 3],
        [Option<SongLuaSingleSource>; 3],
        PlayerActorAssembly,
        PlayerActorAssembly,
        f32,
        [(usize, f32); 2],
    ) = match play_style {
        profile_data::PlayStyle::Versus => {
            let (p1_x, p1_player_source, p1_sources, p1_assembly) = build_player_bundle(
                0,
                &state.profiles()[0],
                FieldPlacement::P1,
                proxy_requests.players[0],
            );
            let (p2_x, p2_player_source, p2_sources, p2_assembly) = build_player_bundle(
                1,
                &state.profiles()[1],
                FieldPlacement::P2,
                proxy_requests.players[1],
            );
            (
                true,
                p1_player_source,
                p2_player_source,
                p1_sources,
                p2_sources,
                p1_assembly,
                p2_assembly,
                p1_x,
                [(0, p1_x), (1, p2_x)],
            )
        }
        _ => {
            let placement = if runtime_player_is_p2 {
                FieldPlacement::P2
            } else {
                FieldPlacement::P1
            };
            let (nf_x, nf_player_source, nf_sources, nf_assembly) = build_player_bundle(
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
                nf_assembly,
                PlayerActorAssembly::Buffered,
                nf_x,
                [(0, nf_x), (usize::MAX, 0.0)],
            )
        }
    };
    let replacement_proxy_sources = [
        SongLuaPlayerProxySources {
            player: p1_player_proxy_source
                .as_ref()
                .map(|source| source.as_slice()),
            note_field: p1_proxy_sources[0].as_ref().map(|source| source.as_slice()),
            judgment: p1_proxy_sources[1].as_ref().map(|source| source.as_slice()),
            combo: p1_proxy_sources[2].as_ref().map(|source| source.as_slice()),
        },
        SongLuaPlayerProxySources {
            player: p2_player_proxy_source
                .as_ref()
                .map(|source| source.as_slice()),
            note_field: p2_proxy_sources[0].as_ref().map(|source| source.as_slice()),
            judgment: p2_proxy_sources[1].as_ref().map(|source| source.as_slice()),
            combo: p2_proxy_sources[2].as_ref().map(|source| source.as_slice()),
        },
    ];
    let replacement_active_players = song_lua_replacement_active_players_indexed(
        &song_lua_visuals.overlays,
        &song_lua_overlay_state_scratch,
        &replacement_proxy_sources,
        &song_lua_proxy_request_index,
        song_lua_capture_visit_scratch,
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
        song_lua_capture_new_actors(
            &mut underlay_proxy_source,
            actors,
            underlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
        );
    }

    // Background filter per-player (Simply Love parity): draw behind each notefield, not full-screen.
    let underlay_start = actors.len();
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
    song_lua_capture_new_actors(
        &mut underlay_proxy_source,
        actors,
        underlay_start,
        song_lua_proxy_actor_scratch.as_mut(),
    );

    // Simply Love parity: BGAnimations/ScreenGameplay underlay/Shared/Header.lua.
    // This top strip sits underneath the UpperNPSGraph and other HUD actors.
    if !hide_gameplay_hud {
        let underlay_start = actors.len();
        let header_rgba = gameplay_header_rgba(policy.background_color);
        presentation_skeleton.push(STATIC_HEADER, actors, |actors| {
            actors.push(act!(quad:
                align(0.5, 0.0): xy(screen_center_x(), 0.0):
                setsize(screen_width(), 80.0):
                diffuse(header_rgba[0], header_rgba[1], header_rgba[2], header_rgba[3]):
                z(83)
            ));
        });
        song_lua_capture_new_actors(
            &mut underlay_proxy_source,
            actors,
            underlay_start,
            song_lua_proxy_actor_scratch.as_mut(),
        );
    }

    let p1_actor_count = player_actor_bundle_len(
        p1_actor_assembly,
        &notefield_actor_scratch[0],
        &notefield_hud_actor_scratch[0],
        &player_actor_scratch[0],
    );
    let p2_actor_count = player_actor_bundle_len(
        p2_actor_assembly,
        &notefield_actor_scratch[1],
        &notefield_hud_actor_scratch[1],
        &player_actor_scratch[1],
    );
    let player_actor_capacity = if segmented {
        0
    } else {
        p1_actor_count.saturating_add(p2_actor_count)
    };
    actors.reserve(player_actor_capacity.saturating_add(48));
    let segment_insert = actors.len();
    let mut segment_players = [None; 2];
    if has_p2_actors {
        if !replacement_active_players[1] {
            if segmented {
                segment_players[0] = Some(PlayerActorSegment {
                    player: 1,
                    assembly: p2_actor_assembly,
                });
            } else {
                append_player_actor_bundle(
                    actors,
                    p2_actor_assembly,
                    &mut notefield_actor_scratch[1],
                    &mut notefield_hud_actor_scratch[1],
                    &mut player_actor_scratch[1],
                );
            }
        } else {
            clear_player_actor_bundle(
                &mut notefield_actor_scratch[1],
                &mut notefield_hud_actor_scratch[1],
                &mut player_actor_scratch[1],
            );
        }
    }
    if !replacement_active_players[0] {
        if segmented {
            segment_players[1] = Some(PlayerActorSegment {
                player: 0,
                assembly: p1_actor_assembly,
            });
        } else {
            append_player_actor_bundle(
                actors,
                p1_actor_assembly,
                &mut notefield_actor_scratch[0],
                &mut notefield_hud_actor_scratch[0],
                &mut player_actor_scratch[0],
            );
        }
    } else {
        clear_player_actor_bundle(
            &mut notefield_actor_scratch[0],
            &mut notefield_hud_actor_scratch[0],
            &mut player_actor_scratch[0],
        );
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
        if policy.smx_input {
            let is_doubles = play_style == profile_data::PlayStyle::Double;
            let is_centered_single = centered_single_notefield;
            let mut field_geom: [Option<(profile_data::PlayerSide, f32, f32)>; 2] = [None, None];
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
                    )
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
                let difficulty_color =
                    color::difficulty_rgba(&chart.difficulty, state.active_color_index());
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
                let (score_value, score_color) = if show_ex_score {
                    let blue_window_ms = player_blue_window_ms(state, player_idx);
                    let ex_percent = state.display_gameplay_ex_score_percent(
                        player_idx,
                        score_display_mode_from_profile(profile.score_display_mode),
                        blue_window_ms,
                    );
                    (ex_percent.max(0.0), color::JUDGMENT_RGBA[0])
                } else {
                    let score_percent = state.display_gameplay_itg_score_percent(
                        player_idx,
                        score_display_mode_from_profile(profile.score_display_mode),
                    );
                    (score_percent, [1.0, 1.0, 1.0, 1.0])
                };

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
                match play_style {
                    profile_data::PlayStyle::Versus => {
                        if player_idx == 0 {
                            color::decorative_rgba(state.active_color_index())
                        } else {
                            color::decorative_rgba(state.active_color_index() - 2)
                        }
                    }
                    _ => {
                        if runtime_player_is_p2 {
                            color::decorative_rgba(state.active_color_index() - 2)
                        } else {
                            color::decorative_rgba(state.active_color_index())
                        }
                    }
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
                let life_for_render = if dead {
                    0.0
                } else {
                    player.life.clamp(0.0, 1.0)
                };
                let is_hot = !dead && life_for_render >= 1.0;
                let profile = &state.profiles()[player_idx];
                let life_percent_text = visible_life_percent_text(
                    &state.life_percent_text,
                    life_for_render * 100.0,
                    profile.lifemeter_type,
                    profile.show_life_percent,
                    show_standard_life_percent,
                    is_hot,
                );

                let lifebar_center_shift = if centered_single_notefield {
                    let clamped_width = screen_width().clamp(640.0, 854.0);
                    match side {
                        profile_data::PlayerSide::P1 => clamped_width * 0.25,
                        profile_data::PlayerSide::P2 => -clamped_width * 0.25,
                    }
                } else {
                    0.0
                };
                let static_life_slot = [STATIC_LIFE_P1, STATIC_LIFE_P2][player_idx.min(1)];

                match profile.lifemeter_type {
                    profile_data::LifeMeterType::Standard => {
                        let life_color = life_fill_color(
                            profile,
                            life_for_render,
                            dead,
                            state.total_elapsed_in_screen(),
                            || player_life_color(player_idx),
                        );
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
                            let bps = state.current_bpm_display() / 60.0;
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
                            font("miso"): settext(life_percent_text):
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

                        let surround_color = surround_life_color(
                            profile,
                            life_for_render,
                            state.total_elapsed_in_screen(),
                        );

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
                        let life_color = life_fill_color(
                            profile,
                            life_for_render,
                            dead,
                            state.total_elapsed_in_screen(),
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

                            // SL: if double style, position next to notefield.
                            if play_style == profile_data::PlayStyle::Double {
                                let half_nf = state.notefield_width(player_idx) * 0.5;
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
                            let bps = state.current_bpm_display() / 60.0;
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
                            font("miso"): settext(life_percent_text):
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
                policy.center_single_notefield,
            );
            actors.push(act!(text:
            font(machine_font_key(state.machine_font(), FontRole::Header)): settext(state.stage_intro_text.clone()):
            align(0.5, 0.5): xy(text_x, screen_height() - 30.0):
            zoom(0.4):
            shadowlength(1.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(110)
        ));
        }
        let hud_snapshot = &state.hud_snapshot;
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
        presentation_skeleton.push(STATIC_FOOTER, actors, |actors| {
            actors.push(screen_bar::build_no_background(ScreenBarParams {
                visual_policy,
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
        });
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
        song_lua_capture_new_actors(
            &mut underlay_proxy_source,
            actors,
            underlay_tail_start,
            song_lua_proxy_actor_scratch.as_mut(),
        );
    }
    let song_foreground_state =
        song_lua_song_foreground_state(state, song_lua_song_foreground_message_state_cache);
    let p1_proxy_slices = [
        p1_proxy_sources[0].as_ref().map(|source| source.as_slice()),
        p1_proxy_sources[1].as_ref().map(|source| source.as_slice()),
        p1_proxy_sources[2].as_ref().map(|source| source.as_slice()),
    ];
    let p2_proxy_slices = [
        p2_proxy_sources[0].as_ref().map(|source| source.as_slice()),
        p2_proxy_sources[1].as_ref().map(|source| source.as_slice()),
        p2_proxy_sources[2].as_ref().map(|source| source.as_slice()),
    ];
    let p1_player_proxy_slice = p1_player_proxy_source
        .as_ref()
        .map(|source| source.as_slice());
    let p2_player_proxy_slice = p2_player_proxy_source
        .as_ref()
        .map(|source| source.as_slice());
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
    push_song_lua_layer_actors(
        actors,
        &song_lua_visuals.overlays,
        song_lua_overlay_order,
        &mut song_lua_proxy_request_index.topology,
        &song_lua_local_state_scratch,
        &song_lua_overlay_state_scratch,
        song_foreground_state,
        &proxy_sources,
        song_lua_proxy_actor_scratch.as_mut(),
        asset_manager,
        song_lua_space_width,
        song_lua_space_height,
        state.current_music_time_display(),
        state.current_beat(),
        state.total_elapsed_in_screen(),
        song_lua_order_scratch,
        song_lua_capture_state_scratch,
        song_lua_capture_order_scratch,
        song_lua_aft_capture_scratch,
        song_lua_projected_mesh_scratch,
    );
    if let Some(actor) = build_foreground_media(
        state,
        &song_lua_overlay_state_scratch,
        &song_lua_background_layer_state_scratch,
        &song_lua_foreground_layer_state_scratch,
    ) {
        actors.push(actor);
    }
    for &layer_idx in song_lua_foreground_active_layers {
        let layer = &song_lua_visuals.foreground_visual_layers[layer_idx];
        let local_states = &song_lua_foreground_layer_local_state_scratch[layer_idx];
        let layer_states = &song_lua_foreground_layer_state_scratch[layer_idx];
        let Some(order_cache) = song_lua_foreground_visual_layer_orders.get_mut(layer_idx) else {
            continue;
        };
        let Some(topology_index) = song_lua_foreground_proxy_request_indices.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(aft_capture_scratch) = song_lua_foreground_aft_capture_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let Some(projected_mesh_scratch) =
            song_lua_foreground_projected_mesh_scratch.get_mut(layer_idx)
        else {
            continue;
        };
        let song_foreground_state = song_lua_song_foreground_state_from(
            song_lua_now,
            &layer.song_foreground,
            layer.song_foreground_events.as_slice(),
            &mut song_lua_foreground_song_foreground_message_state_cache[layer_idx],
        );
        push_song_lua_layer_actors(
            actors,
            &layer.overlays,
            order_cache,
            &mut topology_index.topology,
            local_states,
            layer_states,
            song_foreground_state,
            &proxy_sources,
            song_lua_proxy_actor_scratch.as_mut(),
            asset_manager,
            layer.screen_width.max(1.0),
            layer.screen_height.max(1.0),
            song_lua_now,
            state.current_beat(),
            state.total_elapsed_in_screen(),
            song_lua_order_scratch,
            song_lua_capture_state_scratch,
            song_lua_capture_order_scratch,
            aft_capture_scratch,
            projected_mesh_scratch,
        );
    }
    state.frame_scratch = Some(frame_scratch);
    GameplayActorSegments {
        insert: segment_insert,
        players: segment_players,
    }
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
fn smx_sensor_value_content(value: Option<u16>) -> (TextContent, [f32; 4]) {
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
    let Some(view) = state.smx_sensor_views[idx].as_ref() else {
        return;
    };

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
        let panel_view = view.panels[panel];
        let threshold = panel_view.threshold;
        let threshold_norm = (threshold as f32 / SMX_SENSOR_VALUE_SCALE).clamp(0.0, 1.0);

        let raw_value = panel_view.value;
        let value_norm = raw_value
            .map_or(0.0, |value| value as f32 / SMX_SENSOR_VALUE_SCALE)
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
        let (value_text, value_color) = smx_sensor_value_content(raw_value);
        actors.push(act!(text:
            font(machine_font_key(state.machine_font(), FontRole::Normal)): settext(value_text):
            align(0.5, 0.0): xy(x + bar_w * 0.5, group_top):
            zoom(SMX_SENSOR_VALUE_ZOOM * scale):
            diffuse(value_color[0], value_color[1], value_color[2], value_color[3]):
            z(SMX_SENSOR_Z + 2.0)
        ));

        // Panel letter (L/D/U/R) drawn on the bar near its bottom; the drop
        // shadow keeps it legible over both the dark track and bright fill.
        actors.push(act!(text:
            font(machine_font_key(state.machine_font(), FontRole::Normal)): settext(label):
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

    fn workspace_root() -> std::path::PathBuf {
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if manifest.join("assets").is_dir() {
            manifest
        } else {
            manifest.join("../..")
        }
    }

    fn empty_text_attributes() -> Arc<[TextAttribute]> {
        Arc::from([])
    }
    use deadlib_present::actors::{SizeSpec, TextAlign};

    #[test]
    fn gameplay_actor_scratch_presizes_only_active_players() {
        let scratch = gameplay_actor_scratch(1, 384);

        assert!(scratch.iter().all(Vec::is_empty));
        assert!(scratch[0].capacity() >= 384);
        assert_eq!(scratch[1].capacity(), 0);
    }

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
        scratch.song_lua_order_scratch = Vec::with_capacity(64);
        let mut owner = Some(Box::new(scratch));
        let address = std::ptr::from_ref(owner.as_deref().unwrap());

        let detached = owner.take().unwrap();
        assert_eq!(detached.lobby_hud_status_scratch.capacity(), 128);
        assert!(detached.song_lua_order_scratch.capacity() >= 64);
        owner = Some(detached);

        assert_eq!(std::ptr::from_ref(owner.as_deref().unwrap()), address);
    }

    #[test]
    fn fixed_layer_scratch_matches_legacy_maintenance() {
        let mut benchmark = GameplayFrameOrchestrationBenchmark::new(7);

        assert_eq!(benchmark.layer_resize_legacy(), 42);
        assert_eq!(benchmark.fixed_layer_lengths(), 42);
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
    fn song_lua_prewarmed_scratch_matches_cold_opening_frame() {
        let mut cold = SongLuaScratchPrewarmBenchmark::cold(64, &[32, 96], &[48]);
        let mut prewarmed = SongLuaScratchPrewarmBenchmark::prewarmed(64, &[32, 96], &[48]);

        assert_eq!(cold.opening_frame(), prewarmed.opening_frame());
        assert!(prewarmed.storage_bytes() > 0);
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

    fn test_message_command(delta: SongLuaOverlayStateDelta) -> SongLuaOverlayMessageCommand {
        SongLuaOverlayMessageCommand {
            message: String::new(),
            blocks: vec![SongLuaOverlayCommandBlock {
                start: 0.0,
                duration: 0.75,
                easing: Some("inOutQuad".to_string()),
                opt1: None,
                opt2: None,
                delta,
            }],
        }
    }

    #[test]
    fn song_lua_message_state_cache_matches_replay_across_advances_and_seeks() {
        let commands = vec![
            test_message_command(SongLuaOverlayStateDelta {
                x: Some(100.0),
                draw_order: Some(5),
                ..SongLuaOverlayStateDelta::default()
            }),
            test_message_command(SongLuaOverlayStateDelta {
                y: Some(-40.0),
                z: Some(12.0),
                ..SongLuaOverlayStateDelta::default()
            }),
        ];
        let events = (0..128)
            .map(|index| SongLuaOverlayMessageRuntime {
                event_second: index as f32 * 0.5,
                command_index: index % commands.len(),
            })
            .collect::<Vec<_>>();
        let initial = SongLuaOverlayState {
            x: 7.0,
            y: 11.0,
            ..SongLuaOverlayState::default()
        };
        let mut cache = SongLuaMessageStateCache::default();

        for now in [-1.0, 0.0, 0.125, 1.0, 7.25, 31.75, 63.75, 24.25, 24.5, 63.9] {
            let expected = song_lua_message_state_legacy(now, initial, &commands, Some(&events));
            let actual =
                song_lua_message_state_cached(now, initial, &commands, Some(&events), &mut cache);
            assert_eq!(actual, expected, "now={now}");
        }
    }

    #[test]
    fn song_lua_static_overlay_state_skips_runtime_caches() {
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Actor,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState {
                x: 123.0,
                y: -45.0,
                diffuse: [0.25, 0.5, 0.75, 0.875],
                draw_order: 17,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        };
        let events = vec![Vec::new()];
        let ranges = vec![0..0];
        let mut dynamic_cache = SongLuaMessageStateCache::default();
        let mut static_cache = SongLuaMessageStateCache::default();

        for now in [-1.0, 0.0, 42.0, 10_000.0] {
            let expected = song_lua_overlay_render_state_dynamic(
                now,
                0,
                &overlay,
                Some(&events[0]),
                &[],
                &ranges,
                &mut dynamic_cache,
            );
            let actual = song_lua_overlay_render_state_from(
                now,
                0,
                &overlay,
                &events,
                &[],
                &ranges,
                &mut static_cache,
            );
            assert_eq!(actual, expected, "now={now}");
        }
        assert!(dynamic_cache.initialized);
        assert!(!static_cache.initialized);
    }

    #[test]
    fn song_lua_empty_overlay_state_clears_reused_outputs() {
        let mut message_caches = vec![SongLuaMessageStateCache::default()];
        let mut local_states = vec![SongLuaOverlayState::default()];
        let mut states = vec![SongLuaOverlayState::default()];

        song_lua_overlay_state_sets_from_into(
            42.0,
            &[],
            &[],
            &[],
            &[],
            640.0,
            480.0,
            &SongLuaOverlayOrderCache::default(),
            &mut message_caches,
            &mut local_states,
            &mut states,
        );

        assert!(message_caches.is_empty());
        assert!(local_states.is_empty());
        assert!(states.is_empty());
    }

    #[test]
    fn song_lua_proxy_free_analysis_skips_capture_visits() {
        let overlays = vec![SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Quad,
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        }];
        let states = vec![SongLuaOverlayState::default()];
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
        let sources = [SongLuaPlayerProxySources::default(); 2];

        assert_eq!(
            song_lua_proxy_requests_indexed(&overlays, &states, &index, &mut visit_scratch),
            SongLuaScreenProxyRequests::default()
        );
        assert_eq!(
            song_lua_replacement_active_players_indexed(
                &overlays,
                &states,
                &sources,
                &index,
                &mut visit_scratch,
            ),
            [false; 2]
        );
        assert_eq!(visit_scratch.generation, 0);
    }

    #[test]
    fn song_lua_empty_captured_timeline_returns_initial_state() {
        let actor = SongLuaCapturedActor {
            initial_state: SongLuaOverlayState {
                x: 123.0,
                y: -45.0,
                draw_order: 17,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        };
        let mut expected_cache = SongLuaMessageStateCache::default();
        let mut fast_cache = SongLuaMessageStateCache::default();

        for now in [-1.0, 0.0, 42.0, 10_000.0] {
            let expected = song_lua_message_state_cached(
                now,
                actor.initial_state,
                &actor.message_commands,
                Some(&[]),
                &mut expected_cache,
            );
            let actual =
                song_lua_captured_actor_state_from(now, &actor, Some(&[]), &mut fast_cache);
            assert_eq!(actual, expected, "now={now}");
        }
        assert!(!fast_cache.initialized);
    }

    #[test]
    fn compiled_background_transitions_match_names_expiry_and_seek() {
        for name in [
            "CrossFade_Fastest",
            "CrossFade_Faster",
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
        ] {
            let transition = BackgroundTransition::from_name(name).expect("known transition");
            let expired = Cell::new(false);
            let duration = transition.duration();
            for now in [
                0.0,
                duration * 0.5,
                duration,
                duration + 10.0,
                duration * 0.25,
            ] {
                assert_eq!(
                    background_transition_frame(Some(transition), &expired, 0.0, now),
                    background_transition_frame_legacy(name, 0.0, now),
                    "name={name}, now={now}"
                );
            }
        }
        assert_eq!(BackgroundTransition::from_name("NotATransition"), None);
    }

    #[test]
    fn song_layer2_cursor_matches_reverse_scan_across_seeks() {
        let events = vec![
            SongLayer2Event {
                start_second: 1.0,
                color: Some([1.0; 4]),
            },
            SongLayer2Event {
                start_second: 2.0,
                color: None,
            },
            SongLayer2Event {
                start_second: 3.0,
                color: Some([1.0, 1.0, 160.0 / 255.0, 1.0]),
            },
        ];
        let cursor = Cell::new(0);

        for now in [-1.0, 1.0, 1.3, 1.7, 2.1, 3.25, 4.0, 1.2, 3.6] {
            assert_eq!(
                song_layer2_animation_from(&events, &cursor, now),
                song_layer2_animation_legacy(&events, now),
                "now={now}"
            );
        }
    }

    #[test]
    fn song_lua_layer_activity_matches_full_scan_across_boundaries_and_seeks() {
        let starts = [5.0, 1.0, 9.0, 3.0, 3.0];
        let mut activity = SongLuaLayerActivity::new(starts, f32::NEG_INFINITY);
        for now in [-1.0, 1.0, 3.0, 8.0, 12.0, 2.0, f32::NAN, 0.0, 9.0] {
            let expected = starts
                .iter()
                .enumerate()
                .filter_map(|(index, start)| (!(now < *start)).then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(activity.sync(now), expected, "now={now}");
        }
    }

    #[test]
    fn empty_song_lua_layer_skips_preparation_without_changing_output() {
        let mut legacy_actors = vec![Actor::CameraPop];
        let mut fast_actors = legacy_actors.clone();
        let mut legacy_order_cache = SongLuaOverlayOrderCache::default();
        let mut fast_order_cache = SongLuaOverlayOrderCache::default();
        let mut legacy_topology = SongLuaOverlayTopologyIndex::default();
        let mut fast_topology = SongLuaOverlayTopologyIndex::default();
        let mut legacy_order = vec![7];
        let mut fast_order = legacy_order.clone();
        let mut legacy_aft = SongLuaAftCaptureScratch::default();
        let mut fast_aft = SongLuaAftCaptureScratch::default();

        let _ = prepare_active_song_lua_layer(
            &mut legacy_actors,
            &[],
            &[],
            SongLuaOverlayState::default(),
            &mut legacy_order_cache,
            &mut legacy_topology,
            &mut legacy_order,
            &mut legacy_aft,
        );
        assert_eq!(
            prepare_song_lua_layer(
                &mut fast_actors,
                &[],
                &[],
                SongLuaOverlayState::default(),
                &mut fast_order_cache,
                &mut fast_topology,
                &mut fast_order,
                &mut fast_aft,
            ),
            None
        );

        assert_eq!(fast_actors.len(), legacy_actors.len());
        assert!(matches!(fast_actors.as_slice(), [Actor::CameraPop]));
        assert_eq!(fast_order, legacy_order);
    }

    #[test]
    fn song_lua_state_plan_matches_full_scan_across_advances_and_seeks() {
        let overlays = vec![
            test_order_overlay(SongLuaOverlayKind::ActorFrame, None, 0),
            test_order_overlay(SongLuaOverlayKind::Quad, Some(0), 0),
            SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    x: 10.0,
                    diffuse: [0.75, 1.0, 1.0, 1.0],
                    ..SongLuaOverlayState::default()
                },
                message_commands: vec![test_message_command(SongLuaOverlayStateDelta {
                    x: Some(100.0),
                    diffuse: Some([0.5, 0.5, 1.0, 1.0]),
                    ..SongLuaOverlayStateDelta::default()
                })],
            },
            test_order_overlay(SongLuaOverlayKind::Quad, Some(2), 0),
        ];
        let events = vec![
            Vec::new(),
            Vec::new(),
            vec![SongLuaOverlayMessageRuntime {
                event_second: 0.0,
                command_index: 0,
            }],
            Vec::new(),
        ];
        let ranges = vec![0..0; overlays.len()];
        let plan = song_lua_overlay_order_cache_from(&overlays, &[]);
        assert_eq!(&*plan.dynamic_local_indices, &[2]);
        assert_eq!(&*plan.dynamic_composed_indices, &[2, 3]);
        let mut full_caches = vec![SongLuaMessageStateCache::default(); overlays.len()];
        let mut planned_caches = vec![SongLuaMessageStateCache::default(); overlays.len()];
        let (mut full_local, mut full_composed) =
            song_lua_overlay_initial_state_sets(&overlays, 640.0, 480.0);
        let (mut planned_local, mut planned_composed) =
            song_lua_overlay_initial_state_sets(&overlays, 640.0, 480.0);

        for now in [-1.0, 0.0, 0.25, 0.75, 4.0, 0.5, 8.0] {
            song_lua_overlay_local_states_all_into(
                now,
                &overlays,
                &events,
                &[],
                &ranges,
                &mut full_caches,
                &mut full_local,
            );
            song_lua_overlay_states_from_local_all_into(
                &overlays,
                &full_local,
                640.0,
                480.0,
                &mut full_composed,
            );
            song_lua_overlay_state_sets_from_into(
                now,
                &overlays,
                &events,
                &[],
                &ranges,
                640.0,
                480.0,
                &plan,
                &mut planned_caches,
                &mut planned_local,
                &mut planned_composed,
            );
            assert_eq!(planned_local, full_local, "local states at {now}");
            assert_eq!(planned_composed, full_composed, "composed states at {now}");
        }
    }

    #[test]
    fn song_lua_message_block_cursor_matches_replay_across_block_rewinds() {
        let command = SongLuaOverlayMessageCommand {
            message: "LongCommand".to_string(),
            blocks: (0..128)
                .map(|index| SongLuaOverlayCommandBlock {
                    start: index as f32 * 0.25,
                    duration: 0.2,
                    easing: Some("inOutQuad".to_string()),
                    opt1: None,
                    opt2: None,
                    delta: SongLuaOverlayStateDelta {
                        x: Some(index as f32 * 3.0),
                        y: (index % 5 == 0).then_some(-(index as f32)),
                        ..SongLuaOverlayStateDelta::default()
                    },
                })
                .collect(),
        };
        let commands = vec![command];
        let events = vec![SongLuaOverlayMessageRuntime {
            event_second: 1.0,
            command_index: 0,
        }];
        let initial = SongLuaOverlayState {
            x: 11.0,
            y: 7.0,
            ..SongLuaOverlayState::default()
        };
        let mut cache = SongLuaMessageStateCache::default();

        for now in [0.0, 1.0, 1.1, 8.75, 24.4, 32.8, 9.25, 9.3, 31.0] {
            let expected = song_lua_message_state_legacy(now, initial, &commands, Some(&events));
            let actual =
                song_lua_message_state_cached(now, initial, &commands, Some(&events), &mut cache);
            assert_eq!(actual, expected, "now={now}");
        }
    }

    #[test]
    fn song_lua_ease_future_cutoff_matches_full_range_scan() {
        let overlay_eases = (0..128)
            .map(|index| {
                let start_second = 2.0 + index as f32 * 0.5;
                let runtime_delta = |x| SongLuaRuntimeOverlayStateDelta {
                    overlap_mask: 1,
                    delta: SongLuaOverlayStateDelta {
                        x: Some(x),
                        ..SongLuaOverlayStateDelta::default()
                    },
                };
                SongLuaOverlayEaseWindowRuntime {
                    overlay_index: 0,
                    start_second,
                    end_second: start_second + 0.25,
                    sustain_end_second: f32::MAX,
                    cutoff_second: (index % 7 == 0).then_some(start_second + 1.0),
                    from: runtime_delta(index as f32),
                    to: runtime_delta(index as f32 + 10.0),
                    easing: Some("inOutQuad".to_string()),
                    opt1: None,
                    opt2: None,
                }
            })
            .collect::<Vec<_>>();
        let ranges = vec![0..overlay_eases.len()];
        let initial = SongLuaOverlayState {
            x: -5.0,
            ..SongLuaOverlayState::default()
        };

        for now in [-1.0, 2.0, 2.125, 7.0, 17.25, 66.0, 100.0] {
            let current =
                apply_song_lua_overlay_runtime_eases_for(now, 0, &overlay_eases, &ranges, initial);
            let expected = apply_song_lua_overlay_runtime_eases_legacy(
                now,
                0,
                &overlay_eases,
                &ranges,
                initial,
            );
            assert_eq!(current, expected, "now={now}",);
        }
    }

    #[test]
    fn song_lua_dynamic_order_cache_tracks_draw_and_z_key_changes() {
        let mut overlays = (0..8)
            .map(|index| SongLuaOverlayActor {
                kind: SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: SongLuaOverlayState {
                    draw_order: index,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        cache.dynamic_draw_order[0] = true;
        cache.static_root_order = None;
        let mut actual = Vec::new();

        for changed_index in [0usize, 5, 2, 7] {
            states[changed_index].draw_order = 20 - changed_index as i32;
            song_lua_overlay_order_into(&overlays, &states, &mut cache, None, &mut actual);
            let mut expected = (0..overlays.len()).collect::<Vec<_>>();
            expected.sort_by_key(|&index| (states[index].draw_order, index));
            assert_eq!(actual, expected);
        }

        overlays[0].initial_state.draw_by_z_position = true;
        states[0].draw_by_z_position = true;
        for overlay in overlays.iter_mut().skip(1) {
            overlay.parent_index = Some(0);
        }
        let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        for (index, state) in states.iter_mut().enumerate().skip(1) {
            state.z = (index % 3) as f32;
        }
        song_lua_overlay_order_into(&overlays, &states, &mut cache, None, &mut actual);
        let mut expected_children = (1..overlays.len()).collect::<Vec<_>>();
        expected_children.sort_by(|&left, &right| {
            states[left]
                .z
                .total_cmp(&states[right].z)
                .then_with(|| left.cmp(&right))
        });
        let mut expected = vec![0];
        expected.extend(expected_children);
        assert_eq!(actual, expected);
    }

    #[test]
    fn song_lua_static_order_flatten_matches_recursive_tree() {
        let overlays = (0..64)
            .map(|index| SongLuaOverlayActor {
                kind: SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: (index > 0).then(|| (index - 1) / 4),
                initial_state: SongLuaOverlayState {
                    draw_order: ((index * 37) % 17) as i32,
                    ..SongLuaOverlayState::default()
                },
                message_commands: Vec::new(),
            })
            .collect::<Vec<_>>();
        let states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let mut cache = song_lua_overlay_order_cache_from(&overlays, &[]);
        let mut recursive = Vec::with_capacity(overlays.len());
        let mut flat = Vec::with_capacity(overlays.len());

        song_lua_push_order(&overlays, &states, &mut cache, None, &mut recursive);
        song_lua_overlay_order_into(&overlays, &states, &mut cache, None, &mut flat);

        assert!(cache.static_root_order.is_some());
        assert_eq!(flat, recursive);
    }

    #[test]
    fn projected_overlay_bounded_scratch_stays_inline() {
        for (left, right) in [(0.0, 0.0), (0.25, 0.0), (0.25, 0.5), (1.0, 1.0)] {
            let slices = song_lua_projected_overlay_axis_slices(left, right);
            assert!(!slices.spilled(), "fade=({left}, {right})");
            assert!((2..=4).contains(&slices.len()));
            assert_eq!(slices.first(), Some(&0.0));
            assert_eq!(slices.last(), Some(&1.0));
        }
    }

    #[test]
    fn song_meter_progress_uses_itg_first_second_anchor() {
        assert_eq!(song_meter_progress(-1.0, 2.0, 12.0), 0.0);
        assert_eq!(song_meter_progress(2.0, 2.0, 12.0), 0.0);
        assert!((song_meter_progress(7.0, 2.0, 12.0) - 0.5).abs() <= 1e-6);
        assert_eq!(song_meter_progress(12.0, 2.0, 12.0), 1.0);
    }

    #[test]
    fn bpm_decimal_shows_authored_precision_without_trailing_zeroes() {
        assert_eq!(
            shared_cached_bpm_text(f64::from(100.001_f32), true).as_ref(),
            "100.001"
        );
        assert_eq!(
            shared_cached_bpm_text(f64::from(133.33_f32), true).as_ref(),
            "133.33"
        );
        assert_eq!(shared_cached_bpm_text(100.000, true).as_ref(), "100");
        assert_eq!(shared_cached_bpm_text(150.0, true).as_ref(), "150");
        assert_eq!(shared_cached_bpm_text(100.001, false).as_ref(), "100");
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
                bpm_plan.resolve(bpm, show_decimal),
                shared_cached_bpm_text(bpm, show_decimal)
            );
        }
        let life_plan = GameplayLifeTextPlan::new();
        for life in [87.34, 87.35, 0.0, 100.0, f32::NAN, 87.34] {
            let key = quantize_tenths_u32(life).min(1_000);
            assert_eq!(
                life_plan.resolve(life).as_ref(),
                format!("{:.1}%", key as f32 / 10.0)
            );
        }
    }

    #[test]
    fn life_percent_text_only_resolves_for_visible_meter_layouts() {
        let text_plan = GameplayLifeTextPlan::new();
        assert!(
            visible_life_percent_text(
                &text_plan,
                87.3,
                profile_data::LifeMeterType::Standard,
                false,
                true,
                false,
            )
            .is_none()
        );
        assert!(
            visible_life_percent_text(
                &text_plan,
                87.3,
                profile_data::LifeMeterType::Surround,
                true,
                true,
                false,
            )
            .is_none()
        );
        assert!(
            visible_life_percent_text(
                &text_plan,
                100.0,
                profile_data::LifeMeterType::Vertical,
                true,
                true,
                true,
            )
            .is_none()
        );
        assert_eq!(
            visible_life_percent_text(
                &text_plan,
                87.3,
                profile_data::LifeMeterType::Vertical,
                true,
                false,
                false,
            )
            .as_deref(),
            Some("87.3%")
        );
    }

    #[test]
    fn scorebox_polling_stops_when_loading_finishes() {
        let mut profiles: [score_data::GameplayScoreboxProfileSnapshot; MAX_PLAYERS] =
            std::array::from_fn(|_| Default::default());
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
    fn bulk_player_actor_append_matches_legacy_order_and_reuses_source_capacity() {
        let make_source = || {
            (0..64)
                .map(|index| Actor::CameraPush {
                    view_proj: Matrix4::from_translation(Vector3::new(index as f32, 0.0, 0.0)),
                })
                .collect::<Vec<_>>()
        };
        let mut legacy_source = make_source();
        let mut bulk_source = make_source();
        let legacy_capacity = legacy_source.capacity();
        let bulk_capacity = bulk_source.capacity();
        let mut legacy = vec![Actor::CameraPop];
        let mut bulk = vec![Actor::CameraPop];

        append_player_actors_legacy(&mut legacy, &mut legacy_source);
        append_player_actors(&mut bulk, &mut bulk_source);

        let positions = |actors: &[Actor]| {
            actors
                .iter()
                .map(|actor| match actor {
                    Actor::CameraPop => -1.0,
                    Actor::CameraPush { view_proj } => view_proj.w_axis.x,
                    _ => panic!("unexpected actor kind"),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(&bulk), positions(&legacy));
        assert!(legacy_source.is_empty());
        assert!(bulk_source.is_empty());
        assert_eq!(legacy_source.capacity(), legacy_capacity);
        assert_eq!(bulk_source.capacity(), bulk_capacity);
    }

    #[test]
    fn identity_notefield_presentation_matches_legacy_order_and_capacity() {
        let make_actors = |count: usize, base: f32| {
            (0..count)
                .map(|index| Actor::CameraPush {
                    view_proj: Matrix4::from_translation(Vector3::new(
                        base + index as f32,
                        0.0,
                        0.0,
                    )),
                })
                .collect::<Vec<_>>()
        };
        let mut legacy_field = make_actors(64, 0.0);
        let mut legacy_hud = make_actors(8, 1_000.0);
        let mut direct_field = make_actors(64, 0.0);
        let mut direct_hud = make_actors(8, 1_000.0);
        let direct_field_capacity = direct_field.capacity();
        let direct_hud_capacity = direct_hud.capacity();
        let mut legacy = vec![Actor::CameraPop];
        let mut direct = vec![Actor::CameraPop];

        apply_song_lua_player_transform_legacy(
            &mut legacy_field,
            &mut legacy_hud,
            &mut legacy,
            0,
            [1.0; 4],
            None,
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
        );
        apply_song_lua_player_transform(
            &mut direct_field,
            &mut direct_hud,
            &mut direct,
            0,
            [1.0; 4],
            None,
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
        );

        let positions = |actors: &[Actor]| {
            actors
                .iter()
                .map(|actor| match actor {
                    Actor::CameraPush { view_proj } => view_proj.w_axis.x,
                    _ => panic!("unexpected actor kind"),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(&direct), positions(&legacy));
        assert!(direct_field.is_empty());
        assert!(direct_hud.is_empty());
        assert_eq!(direct_field.capacity(), direct_field_capacity);
        assert_eq!(direct_hud.capacity(), direct_hud_capacity);
    }

    #[test]
    fn direct_identity_player_bundle_matches_buffered_output_and_fallback_boundaries() {
        let identity = SongLuaCaptureTransform {
            z_shift: 0,
            tint: [1.0; 4],
            blend: None,
            playfield_center_x: screen_center_x(),
            target_x: screen_center_x(),
            target_y: screen_center_y(),
            rotation_x: 0.0,
            rotation_z: 0.0,
            rotation_y: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
        };
        assert_eq!(
            player_actor_assembly_for_transform(false, true, identity),
            PlayerActorAssembly::DirectIdentity
        );
        assert_eq!(
            player_actor_assembly_for_transform(true, true, identity),
            PlayerActorAssembly::Buffered
        );
        assert_eq!(
            player_actor_assembly_for_transform(false, false, identity),
            PlayerActorAssembly::Buffered
        );
        assert_eq!(
            player_actor_assembly_for_transform(
                false,
                true,
                SongLuaCaptureTransform {
                    target_x: identity.target_x + 1.0,
                    ..identity
                },
            ),
            PlayerActorAssembly::Buffered
        );

        let make_actors = |count: usize, base: f32| {
            (0..count)
                .map(|index| Actor::CameraPush {
                    view_proj: Matrix4::from_translation(Vector3::new(
                        base + index as f32,
                        0.0,
                        0.0,
                    )),
                })
                .collect::<Vec<_>>()
        };
        let mut buffered_field = make_actors(64, 0.0);
        let mut buffered_hud = make_actors(8, 1_000.0);
        let mut buffered_player = Vec::with_capacity(72);
        let mut buffered_out = vec![Actor::CameraPop];
        apply_song_lua_player_transform(
            &mut buffered_field,
            &mut buffered_hud,
            &mut buffered_player,
            0,
            [1.0; 4],
            None,
            screen_center_x(),
            screen_center_x(),
            screen_center_y(),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
        );
        append_player_actor_bundle(
            &mut buffered_out,
            PlayerActorAssembly::Buffered,
            &mut buffered_field,
            &mut buffered_hud,
            &mut buffered_player,
        );

        let mut direct_field = make_actors(64, 0.0);
        let mut direct_hud = make_actors(8, 1_000.0);
        let mut direct_player = Vec::with_capacity(72);
        let direct_field_capacity = direct_field.capacity();
        let direct_hud_capacity = direct_hud.capacity();
        let direct_player_capacity = direct_player.capacity();
        let mut direct_out = vec![Actor::CameraPop];
        append_player_actor_bundle(
            &mut direct_out,
            PlayerActorAssembly::DirectIdentity,
            &mut direct_field,
            &mut direct_hud,
            &mut direct_player,
        );

        let positions = |actors: &[Actor]| {
            actors
                .iter()
                .map(|actor| match actor {
                    Actor::CameraPop => -1.0,
                    Actor::CameraPush { view_proj } => view_proj.w_axis.x,
                    _ => panic!("unexpected actor kind"),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(&direct_out), positions(&buffered_out));
        assert!(direct_field.is_empty());
        assert!(direct_hud.is_empty());
        assert!(direct_player.is_empty());
        assert_eq!(direct_field.capacity(), direct_field_capacity);
        assert_eq!(direct_hud.capacity(), direct_hud_capacity);
        assert_eq!(direct_player.capacity(), direct_player_capacity);

        direct_field.extend(make_actors(2, 0.0));
        direct_hud.extend(make_actors(2, 1_000.0));
        direct_player.extend(make_actors(2, 2_000.0));
        clear_player_actor_bundle(&mut direct_field, &mut direct_hud, &mut direct_player);
        assert!(direct_field.is_empty());
        assert!(direct_hud.is_empty());
        assert!(direct_player.is_empty());
        assert_eq!(direct_field.capacity(), direct_field_capacity);
        assert_eq!(direct_hud.capacity(), direct_hud_capacity);
        assert_eq!(direct_player.capacity(), direct_player_capacity);
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

    #[test]
    fn song_lua_sound_crossing_keeps_zero_and_forward_edge_semantics() {
        assert!(song_lua_sound_time_crossed(0.0, 0.0, 0.0));
        assert!(song_lua_sound_time_crossed(1.0, 2.0, 2.0));
        assert!(!song_lua_sound_time_crossed(1.0, 2.0, 1.0));
        assert!(!song_lua_sound_time_crossed(1.0, 2.0, f32::NAN));
    }

    #[test]
    fn song_lua_sound_schedule_advances_without_revisiting_old_events() {
        let events = [
            SongLuaSoundEvent {
                second: 0.0,
                path: PathBuf::from("zero.ogg"),
            },
            SongLuaSoundEvent {
                second: 1.0,
                path: PathBuf::from("one.ogg"),
            },
            SongLuaSoundEvent {
                second: 2.0,
                path: PathBuf::from("two.ogg"),
            },
        ];
        let mut next_event_ix = 0;
        let mut visited = Vec::new();

        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.0, 0.0, &mut |path| {
            visited.push(path.to_path_buf())
        });
        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.0, 1.5, &mut |path| {
            visited.push(path.to_path_buf())
        });
        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.0, 1.5, &mut |path| {
            visited.push(path.to_path_buf())
        });

        assert_eq!(
            visited,
            [PathBuf::from("zero.ogg"), PathBuf::from("one.ogg")]
        );
        assert_eq!(next_event_ix, 2);
    }

    #[test]
    fn song_lua_sound_schedule_rewinds_after_a_backward_seek() {
        let events = [
            SongLuaSoundEvent {
                second: 1.0,
                path: PathBuf::from("one.ogg"),
            },
            SongLuaSoundEvent {
                second: 2.0,
                path: PathBuf::from("two.ogg"),
            },
        ];
        let mut next_event_ix = events.len();
        let mut visited = Vec::new();

        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 2.0, 0.5, &mut |_| {});
        visit_scheduled_song_lua_sound_events(&events, &mut next_event_ix, 0.5, 1.5, &mut |path| {
            visited.push(path.to_path_buf())
        });

        assert_eq!(visited, [PathBuf::from("one.ogg")]);
        assert_eq!(next_event_ix, 1);
    }

    fn ensure_i18n() {
        crate::assets::i18n::init_for_tests();
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
        let view = ViewOverride {
            force_center_1player: true,
            ..ViewOverride::default()
        };

        assert_eq!(song_lua_player_target_x(None, 320.0, 800.0, view), 800.0);
    }

    #[test]
    fn forced_center_view_preserves_explicit_player_x() {
        let view = ViewOverride {
            force_center_1player: true,
            ..ViewOverride::default()
        };

        assert_eq!(
            song_lua_player_target_x(Some(640.0), 320.0, 800.0, view),
            640.0
        );
    }

    #[test]
    fn default_view_uses_player_state_x() {
        assert_eq!(
            song_lua_player_target_x(None, 320.0, 800.0, ViewOverride::default()),
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
                bpm_x(crate::config::GameplayBpmPosition::TopCenter, false),
                center_x
            );
            assert_eq!(
                bpm_x(crate::config::GameplayBpmPosition::NearField, false),
                center_x
            );

            let top_center = bpm_x(crate::config::GameplayBpmPosition::TopCenter, true);
            let near_field = bpm_x(crate::config::GameplayBpmPosition::NearField, true);
            assert_eq!(near_field, top_center);
            assert_ne!(top_center, center_x);
        }
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

    fn first_textured_mesh_transform(actors: &[Actor]) -> Matrix4 {
        actors
            .iter()
            .find_map(|actor| match actor {
                Actor::TexturedMesh {
                    local_transform, ..
                } => Some(*local_transform),
                Actor::Frame { children, .. } => children.iter().find_map(|child| match child {
                    Actor::TexturedMesh {
                        local_transform, ..
                    } => Some(*local_transform),
                    _ => None,
                }),
                _ => None,
            })
            .expect("expected textured mesh actor")
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
    fn song_lua_note_field_proxy_source_preserves_camera_transform() {
        let segments = [Arc::<[Actor]>::from([
            Actor::CameraPush {
                view_proj: Matrix4::IDENTITY,
            },
            test_source_actor(),
            Actor::CameraPop,
        ])];
        let mut out = Vec::new();

        append_song_lua_player_transform(
            segments.iter().flat_map(|segment| segment.iter().cloned()),
            std::iter::empty(),
            3,
            0,
            true,
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
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
        assert_eq!(
            song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch,),
            requests
        );

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
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
        assert_eq!(
            song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch,),
            requests
        );

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
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());
        assert_eq!(
            song_lua_proxy_requests_indexed(&overlays, &overlay_states, &index, &mut visit_scratch,),
            requests
        );

        assert!(!requests.players[0].combo);
    }

    #[test]
    fn song_lua_capture_marks_match_nested_duplicate_and_cycle_behavior() {
        let mut nested = test_aft_overlay("capture-b", true);
        nested.parent_index = Some(0);
        let mut cycle = test_aft_overlay("capture-a", true);
        cycle.parent_index = Some(1);
        let overlays = vec![
            test_capture_overlay("Capture-A"),
            test_capture_overlay("Capture-B"),
            nested,
            test_capture_proxy_child(1, SongLuaProxyTarget::NoteField { player_index: 0 }),
            cycle,
            test_aft_overlay("capture-a", true),
            test_aft_overlay("capture-a", true),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let expected = song_lua_proxy_requests(&overlays, &overlay_states);
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

        for _ in 0..2 {
            assert_eq!(
                song_lua_proxy_requests_indexed(
                    &overlays,
                    &overlay_states,
                    &index,
                    &mut visit_scratch,
                ),
                expected
            );
        }
        assert!(expected.players[0].note_field);

        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let sources = [
            SongLuaPlayerProxySources {
                note_field: Some(source.as_slice()),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources::default(),
        ];
        assert_eq!(
            song_lua_replacement_active_players_indexed(
                &overlays,
                &overlay_states,
                &sources,
                &index,
                &mut visit_scratch,
            ),
            [true, false]
        );
    }

    #[test]
    fn song_lua_proxy_index_matches_nested_player_replacement() {
        let overlays = vec![
            test_capture_overlay("PlayerCapture"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
            test_aft_overlay("playercapture", true),
        ];
        let overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let source = vec![Arc::<[Actor]>::from(vec![test_source_actor()])];
        let sources = [
            SongLuaPlayerProxySources {
                player: Some(source.as_slice()),
                ..SongLuaPlayerProxySources::default()
            },
            SongLuaPlayerProxySources::default(),
        ];
        let expected = song_lua_replacement_active_players(&overlays, &overlay_states, &sources);
        let index = SongLuaProxyRequestIndex::new(&overlays);
        let mut visit_scratch = SongLuaCaptureVisitScratch::with_capacity(overlays.len());

        assert_eq!(
            song_lua_replacement_active_players_indexed(
                &overlays,
                &overlay_states,
                &sources,
                &index,
                &mut visit_scratch,
            ),
            expected
        );
        assert_eq!(expected, [true, false]);
    }

    #[test]
    fn song_lua_topology_index_matches_dynamic_camera_and_aft_walks() {
        let mut capture = test_capture_overlay("CaptureA");
        capture.initial_state.fov = Some(50.0);
        let mut nearer_camera = test_order_overlay(SongLuaOverlayKind::ActorFrame, Some(1), 0);
        nearer_camera.initial_state.fov = None;
        let overlays = vec![
            capture,
            test_order_overlay(SongLuaOverlayKind::Actor, Some(0), 0),
            nearer_camera,
            test_order_overlay(SongLuaOverlayKind::Actor, Some(2), 0),
            test_order_overlay(SongLuaOverlayKind::Actor, Some(3), 0),
            test_aft_overlay("capturea", true),
            test_aft_overlay("missing", true),
            test_capture_overlay("OtherCapture"),
            test_order_overlay(SongLuaOverlayKind::Actor, Some(usize::MAX), 0),
        ];
        let mut overlay_states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let mut topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
        topology_index.rebuild_camera_states(&overlays, &overlay_states);

        for (overlay_index, overlay) in overlays.iter().enumerate() {
            let indexed_camera = song_lua_overlay_camera_state_indexed(
                &overlay_states,
                &topology_index,
                overlay_index,
            );
            let legacy_camera =
                song_lua_overlay_camera_state(&overlays, &overlay_states, overlay.parent_index);
            assert_eq!(
                topology_index.aft_ancestors[overlay_index].get(),
                song_lua_overlay_aft_ancestor(&overlays, overlay_index),
                "AFT ancestor diverged for overlay {overlay_index}",
            );
            assert_eq!(
                indexed_camera, legacy_camera,
                "camera state diverged for overlay {overlay_index}",
            );
            assert_eq!(
                topology_index.camera_state(&overlay_states, overlay_index),
                legacy_camera,
                "prepared camera state diverged for overlay {overlay_index}",
            );
            let expected_target = match &overlay.kind {
                SongLuaOverlayKind::AftSprite { capture_name } => {
                    song_lua_overlay_capture_index_by_name(&overlays, capture_name)
                }
                _ => None,
            };
            assert_eq!(
                topology_index.aft_sprite_targets[overlay_index].get(),
                expected_target,
                "AFT target diverged for overlay {overlay_index}",
            );
        }

        overlay_states[2].fov = Some(35.0);
        topology_index.rebuild_camera_states(&overlays, &overlay_states);
        assert_eq!(
            song_lua_overlay_camera_state_indexed(&overlay_states, &topology_index, 4),
            song_lua_overlay_camera_state(&overlays, &overlay_states, Some(3)),
        );
        assert_eq!(
            song_lua_overlay_camera_state_indexed(&overlay_states, &topology_index, 4)
                .and_then(|state| state.fov),
            Some(35.0),
        );
        assert_eq!(
            topology_index
                .camera_state(&overlay_states, 4)
                .and_then(|state| state.fov),
            Some(35.0),
        );
    }

    #[test]
    fn song_lua_dynamic_camera_scope_keeps_indexed_fallback() {
        let mut outer = test_order_overlay(SongLuaOverlayKind::ActorFrame, None, 0);
        outer.initial_state.fov = Some(50.0);
        let mut inner = test_order_overlay(SongLuaOverlayKind::ActorFrame, Some(0), 0);
        inner.message_commands = vec![test_message_command(SongLuaOverlayStateDelta {
            fov: Some(25.0),
            ..SongLuaOverlayStateDelta::default()
        })];
        let overlays = vec![
            outer,
            inner,
            test_order_overlay(SongLuaOverlayKind::Actor, Some(1), 0),
        ];
        let mut states = overlays
            .iter()
            .map(|overlay| overlay.initial_state)
            .collect::<Vec<_>>();
        let topology = SongLuaOverlayTopologyIndex::new(&overlays);
        assert!(topology.dynamic_camera_scope);
        assert_eq!(
            topology.camera_state(&states, 2),
            song_lua_overlay_camera_state_indexed(&states, &topology, 2),
        );
        assert_eq!(
            topology
                .camera_state(&states, 2)
                .and_then(|state| state.fov),
            Some(50.0),
        );

        states[1].fov = Some(25.0);
        assert_eq!(
            topology.camera_state(&states, 2),
            song_lua_overlay_camera_state_indexed(&states, &topology, 2),
        );
        assert_eq!(
            topology
                .camera_state(&states, 2)
                .and_then(|state| state.fov),
            Some(25.0),
        );
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
        let topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
        let mut capture_states = Vec::new();
        let mut order_scratch = Vec::new();
        let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);
        let actors = song_lua_capture_children(
            &overlays,
            &overlay_states,
            &local_states,
            &mut order_cache,
            &topology_index,
            &AssetManager::new(),
            1,
            &proxy_sources,
            854.0,
            480.0,
            &mut capture_states,
            &mut order_scratch,
            &mut projected_mesh_scratch,
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

        let Actor::SharedFrame { z, children, .. } = actor else {
            panic!("expected direct shared proxy actor");
        };
        assert_eq!(z, 1234);
        assert_eq!(children.len(), 1);
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

        let Actor::SharedFrame { z, children, .. } = actor else {
            panic!("expected direct shared proxy actor");
        };
        assert_eq!(z, 1234);
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

        let Actor::SharedFrame { children, .. } = actor else {
            panic!("expected direct shared proxy actor");
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

        let Actor::SharedFrame { children, .. } = actor else {
            panic!("expected direct shared proxy actor");
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
    fn song_lua_proxy_prewarm_reuses_local_z_camera_storage() {
        let mut low = test_source_actor();
        let mut high = test_source_actor();
        if let Actor::Frame { z, .. } = &mut low {
            *z = -20;
        }
        if let Actor::Frame { offset, z, .. } = &mut high {
            *offset = [99.0, 0.0];
            *z = 20;
        }
        let source = [Arc::<[Actor]>::from(vec![
            Actor::CameraPush {
                view_proj: Matrix4::IDENTITY,
            },
            high,
            low,
            Actor::CameraPop,
        ])];
        let mut scratch = SongLuaProxyActorScratch::with_capacity_and_banks(0, 1, 1);

        for _ in 0..2 {
            scratch.begin_frame();
            let actor = song_lua_build_proxy_actor_with_scratch(
                SongLuaOverlayState::default(),
                1234,
                &source,
                640.0,
                480.0,
                Some(&mut scratch),
            )
            .expect("prewarmed proxy should render");
            let Actor::SharedFrame { z, children, .. } = actor else {
                panic!("expected prewarmed shared proxy frame");
            };
            assert_eq!(z, 1234);
            let [Actor::Frame { children, .. }] = children.as_ref() else {
                panic!("expected reusable normalized source backing");
            };
            let [
                Actor::CameraPush { .. },
                Actor::Frame {
                    offset: first_offset,
                    z: first_z,
                    ..
                },
                Actor::Frame {
                    offset: second_offset,
                    z: second_z,
                    ..
                },
                Actor::CameraPop,
            ] = children.as_slice()
            else {
                panic!("expected camera markers around the sorted local run");
            };
            assert_eq!(*first_offset, [0.0, 0.0]);
            assert_eq!(*second_offset, [99.0, 0.0]);
            assert_eq!([*first_z, *second_z], [0, 0]);
        }

        let bank = &scratch.banks[0];
        assert_eq!(bank.proxy_segments[0].stats().growths, 0);
        assert_eq!(bank.proxy_segments[0].stats().replacements, 0);
    }

    #[test]
    fn song_lua_proxy_prewarm_reuses_segment_join_frame() {
        let source: [Arc<[Actor]>; 3] = std::array::from_fn(|_| Arc::from([Actor::CameraPop]));
        let mut scratch = SongLuaProxyActorScratch::with_capacity_and_banks(0, 1, 1);

        for _ in 0..2 {
            scratch.begin_frame();
            let actor = song_lua_build_proxy_frame_actor_with_scratch(
                SongLuaOverlayState::default(),
                1234,
                &source,
                640.0,
                480.0,
                Some(&mut scratch),
            )
            .expect("prewarmed segmented proxy should render");
            let Actor::SharedFrame { z, children, .. } = actor else {
                panic!("expected prewarmed outer proxy frame");
            };
            assert_eq!(z, 1234);
            assert_eq!(children.len(), source.len());
            assert!(
                children
                    .iter()
                    .all(|actor| matches!(actor, Actor::SharedFrame { .. }))
            );
        }

        assert_eq!(scratch.banks[0].proxy_frames[0]._replacements, 0);
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
    fn song_lua_capture_style_preserves_shadow_and_styles_child() {
        let actor = Actor::Shadow {
            len: [2.0, -3.0],
            color: [0.8, 0.6, 0.4, 0.5],
            child: Box::new(Actor::Mesh {
                align: [0.0, 0.0],
                offset: [0.0, 0.0],
                size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
                tint: [0.8, 0.6, 0.4, 0.5],
                vertices: Arc::from([]),
                visible: true,
                blend: BlendMode::Alpha,
                z: 3,
            }),
        };

        let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

        let Actor::Shadow { len, color, child } = styled else {
            panic!("expected shadow actor");
        };
        assert_eq!(len, [2.0, -3.0]);
        assert_eq!(color, [0.4, 0.15, 0.040000003, 0.25]);
        let Actor::Mesh { tint, z, .. } = child.as_ref() else {
            panic!("expected styled mesh child");
        };
        assert_eq!(*tint, [0.4, 0.15, 0.040000003, 0.25]);
        assert_eq!(*z, 7);
    }

    #[test]
    fn song_lua_capture_style_shares_mesh_vertices_and_composes_tint() {
        let vertices = Arc::<[MeshVertex]>::from(vec![MeshVertex {
            pos: [0.0, 0.0],
            color: [0.8, 0.6, 0.4, 0.5],
        }]);
        let actor = Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, 0.0],
            size: [SizeSpec::Px(1.0), SizeSpec::Px(1.0)],
            tint: [0.8, 0.6, 0.4, 0.5],
            vertices: Arc::clone(&vertices),
            visible: true,
            blend: BlendMode::Alpha,
            z: 3,
        };

        let styled = song_lua_style_capture_actor(actor, [0.5, 0.25, 0.1, 0.5], None, 4);

        let Actor::Mesh {
            vertices: styled_vertices,
            tint,
            blend,
            z,
            ..
        } = styled
        else {
            panic!("expected mesh actor");
        };
        assert!(Arc::ptr_eq(&styled_vertices, &vertices));
        assert_eq!(styled_vertices[0].color, [0.8, 0.6, 0.4, 0.5]);
        assert_eq!(tint, [0.4, 0.15, 0.040000003, 0.25]);
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
    fn shared_aft_capture_preserves_style_offset_and_nested_z() {
        let overlay = test_aft_overlay("CaptureAFT", true);
        let state = SongLuaOverlayState {
            x: 0.5 * screen_width(),
            y: 0.5 * screen_height(),
            diffuse: [0.5, 0.25, 0.1, 0.5],
            blend: SongLuaOverlayBlendMode::Add,
            ..SongLuaOverlayState::default()
        };
        let source = Actor::Frame {
            align: [0.0, 0.0],
            offset: [3.0, 4.0],
            size: [SizeSpec::Fill, SizeSpec::Fill],
            children: vec![Actor::Frame {
                align: [0.0, 0.0],
                offset: [5.0, 6.0],
                size: [SizeSpec::Fill, SizeSpec::Fill],
                children: Vec::new(),
                background: None,
                z: 4,
            }],
            background: None,
            z: 3,
        };
        let old = song_lua_build_capture_actor(
            &overlay,
            state,
            7,
            vec![source.clone()],
            screen_width(),
            screen_height(),
        )
        .expect("legacy AFT capture");
        let mut scratch = SharedActorFrameScratch::with_capacity(3);
        let new = song_lua_build_shared_capture(
            &overlay,
            state,
            7,
            screen_width(),
            screen_height(),
            &mut scratch,
            |children| children.push(source),
        )
        .expect("shared AFT capture");

        let Actor::Frame {
            offset: old_offset,
            children: old_children,
            ..
        } = old
        else {
            panic!("expected untransformed legacy capture frame");
        };
        let [
            Actor::Frame {
                z: old_z,
                children: old_nested,
                ..
            },
        ] = old_children.as_slice()
        else {
            panic!("expected legacy captured frame");
        };
        let [
            Actor::Frame {
                z: old_nested_z, ..
            },
        ] = old_nested.as_slice()
        else {
            panic!("expected legacy nested frame");
        };

        let Actor::SharedFrame {
            tint,
            blend,
            children,
            ..
        } = new
        else {
            panic!("expected shared capture frame");
        };
        let [
            Actor::Frame {
                offset: new_offset,
                children: new_children,
                ..
            },
        ] = children.as_ref()
        else {
            panic!("expected reusable inner frame");
        };
        let [
            Actor::Frame {
                z: new_z,
                children: new_nested,
                ..
            },
        ] = new_children.as_slice()
        else {
            panic!("expected shared captured frame");
        };
        let [
            Actor::Frame {
                z: new_nested_z, ..
            },
        ] = new_nested.as_slice()
        else {
            panic!("expected shared nested frame");
        };

        assert_eq!(*new_offset, old_offset);
        assert_eq!((*new_z, *new_nested_z), (*old_z, *old_nested_z));
        assert_eq!(tint, state.diffuse);
        assert_eq!(blend, Some(BlendMode::Add));
        assert_eq!(scratch.stats().growths, 0);
    }

    #[test]
    fn aft_capture_scratch_prewarms_both_frame_banks() {
        let overlays = vec![
            test_capture_overlay("CaptureAFT"),
            test_capture_proxy_child(0, SongLuaProxyTarget::Player { player_index: 0 }),
            test_aft_overlay("CaptureAFT", true),
        ];
        let topology = SongLuaOverlayTopologyIndex::new(&overlays);
        let scratch = SongLuaAftCaptureScratch::new(&overlays, &topology);
        let banks = scratch.slots[2].as_ref().expect("AFT scratch banks");

        assert_eq!(song_lua_aft_capture_capacity(&overlays, &topology, 2), 3);
        assert!(banks.iter().all(|bank| bank.capacity() >= 3));
        assert!(banks.iter().all(|bank| bank.stats().growths == 0));
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
        let mut topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
        let mut out = Vec::new();
        let mut order_scratch = Vec::new();
        let mut capture_states = Vec::new();
        let mut capture_order_scratch = Vec::new();
        let mut aft_capture_scratch = SongLuaAftCaptureScratch::new(&overlays, &topology_index);
        let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);

        push_song_lua_layer_actors(
            &mut out,
            &overlays,
            &mut order_cache,
            &mut topology_index,
            &overlay_states,
            &overlay_states,
            SongLuaOverlayState::default(),
            &proxy_sources,
            None,
            &AssetManager::new(),
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            &mut order_scratch,
            &mut capture_states,
            &mut capture_order_scratch,
            &mut aft_capture_scratch,
            &mut projected_mesh_scratch,
        );

        assert_eq!(out.len(), 1);
        let Actor::SharedFrame { children, .. } = &out[0] else {
            panic!("expected shared combined AFT frame");
        };
        let [Actor::Frame { children, .. }] = children.as_ref() else {
            panic!("expected reusable capture frame");
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
        let mut topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
        let mut out = Vec::new();
        let mut order_scratch = Vec::new();
        let mut capture_states = Vec::new();
        let mut capture_order_scratch = Vec::new();
        let mut aft_capture_scratch = SongLuaAftCaptureScratch::new(&overlays, &topology_index);
        let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);

        push_song_lua_layer_actors(
            &mut out,
            &overlays,
            &mut order_cache,
            &mut topology_index,
            &overlay_states,
            &overlay_states,
            SongLuaOverlayState::default(),
            &proxy_sources,
            None,
            &AssetManager::new(),
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            &mut order_scratch,
            &mut capture_states,
            &mut capture_order_scratch,
            &mut aft_capture_scratch,
            &mut projected_mesh_scratch,
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
        let mut topology_index = SongLuaOverlayTopologyIndex::new(&overlays);
        let mut out = Vec::new();
        let mut order_scratch = Vec::new();
        let mut capture_states = Vec::new();
        let mut capture_order_scratch = Vec::new();
        let mut aft_capture_scratch = SongLuaAftCaptureScratch::new(&overlays, &topology_index);
        let mut projected_mesh_scratch = song_lua_projected_mesh_scratch_for(&overlays);

        push_song_lua_layer_actors(
            &mut out,
            &overlays,
            &mut order_cache,
            &mut topology_index,
            &overlay_states,
            &overlay_states,
            SongLuaOverlayState::default(),
            &proxy_sources,
            None,
            &AssetManager::new(),
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            0.0,
            &mut order_scratch,
            &mut capture_states,
            &mut capture_order_scratch,
            &mut aft_capture_scratch,
            &mut projected_mesh_scratch,
        );

        assert_eq!(out.len(), 1);
    }

    #[test]
    fn song_lua_rgb_aft_fixture_initial_state_combines() {
        let manifest = workspace_root();
        let root = manifest.join("tests/fixtures/song_lua");
        let entry = root.join("aft.lua");
        assert!(entry.is_file(), "missing fixture: {}", entry.display());
        let mut context =
            deadsync_assets::song_lua::SongLuaCompileContext::new(&root, "RGB AFT Fixture");
        context.style_name = "double".to_string();
        let compiled = deadsync_assets::song_lua::compile_song_lua(&entry, &context).unwrap();
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
            .expect("fixture should compile AFTSpriteR");

        let legacy = song_lua_rgb_aft_group_for(&compiled.overlays, &states, &order, red_index);
        let mut topology = SongLuaOverlayTopologyIndex::new(&compiled.overlays);
        topology.prepare_rgb_aft_groups(&states, &order);
        let prepared = topology.rgb_aft_group(red_index);
        assert_eq!(prepared, legacy);
        let Some((leader, group)) = legacy else {
            panic!("fixture RGB AFT state should combine before rgbsplit");
        };

        assert!(group.contains(&leader));
        for index in group {
            assert_eq!(topology.rgb_aft_group(index), prepared);
        }
    }

    #[test]
    fn song_lua_proxy_scratch_rotates_while_prior_frame_is_retained() {
        let mut scratch = SongLuaProxyActorScratch::new(1);
        scratch.begin_frame();
        let first = scratch
            .next_screen()
            .expect("first frame has screen capture storage")
            .refill([0.0, 0.0], |actors| actors.push(Actor::CameraPop))
            .expect("first capture is populated");

        scratch.begin_frame();
        let second_slot = scratch
            .next_screen()
            .expect("second frame has screen capture storage");
        let second = second_slot
            .refill([0.0, 0.0], |actors| actors.push(Actor::CameraPop))
            .expect("second capture is populated");
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second_slot.stats().replacements, 0);

        drop(first);
        drop(second);
        scratch.begin_frame();
        let first_bank_slot = scratch
            .next_screen()
            .expect("released first bank is reusable");
        assert!(
            first_bank_slot
                .refill([0.0, 0.0], |actors| actors.push(Actor::CameraPop))
                .is_some()
        );
        assert_eq!(first_bank_slot.stats().replacements, 0);
    }

    #[test]
    fn song_lua_hud_proxy_flattens_identity_capture_without_retaining_it() {
        let mut capture_scratch = SharedActorFrameScratch::with_capacity(2);
        let capture = [capture_scratch
            .refill([0.0, 0.0], |actors| {
                actors.extend([Actor::CameraPop, Actor::CameraPop]);
            })
            .expect("HUD capture is populated")];
        let mut proxy_scratch = SharedActorFrameScratch::with_capacity(2);
        let transform = SongLuaCaptureTransform {
            z_shift: 0,
            tint: [1.0; 4],
            blend: None,
            playfield_center_x: screen_center_x(),
            target_x: screen_center_x(),
            target_y: screen_center_y(),
            rotation_x: 0.0,
            rotation_z: 0.0,
            rotation_y: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
        };

        let proxy =
            song_lua_render_captured_source(None, Some(&capture), transform, &mut proxy_scratch)
                .expect("HUD proxy is populated");

        assert_eq!(Arc::strong_count(&capture[0]), 2);
        let [Actor::Frame { children, .. }] = proxy[0].as_ref() else {
            panic!("proxy scratch keeps one frame wrapper");
        };
        assert_eq!(children.len(), 2);
        assert!(
            children
                .iter()
                .all(|actor| matches!(actor, Actor::CameraPop))
        );
    }

    #[test]
    fn song_lua_player_child_proxy_source_is_player_local() {
        let origin = [screen_center_x(), screen_center_y()];
        let mut actors = vec![test_source_actor()];
        let mut scratch = SharedActorFrameScratch::with_capacity(1);
        let source =
            song_lua_player_child_proxy_source(&mut actors, origin[0], origin[1], &mut scratch)
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

        let Actor::SharedFrame {
            offset, children, ..
        } = actor
        else {
            panic!("expected direct shared proxy actor");
        };
        assert_eq!(offset, origin);
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

        let mut scratch = SongLuaProjectedMeshScratch::mesh(3);
        let build_reused = |scratch: &mut SongLuaProjectedMeshScratch| {
            build_song_lua_overlay_actor_with_scratch(
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
                Some(scratch),
            )
            .expect_actor("ActorMultiVertex overlay should reuse its mesh")
        };
        let Actor::ReusableMesh {
            vertices: reused_vertices,
            tint,
            ..
        } = build_reused(&mut scratch)
        else {
            panic!("expected reusable ActorMultiVertex mesh");
        };
        assert_eq!(vertices.len(), reused_vertices.len());
        for (expected, actual) in vertices.iter().zip(reused_vertices.iter()) {
            assert_eq!(expected.pos, actual.pos);
            assert_eq!(expected.color, actual.color);
        }
        assert_eq!(tint, [1.0; 4]);
        let buffer_ptr = Arc::as_ptr(&reused_vertices);
        drop(reused_vertices);
        let Actor::ReusableMesh {
            vertices: next_vertices,
            ..
        } = build_reused(&mut scratch)
        else {
            panic!("expected reusable ActorMultiVertex mesh");
        };
        assert_eq!(Arc::as_ptr(&next_vertices), buffer_ptr);
        assert_eq!(scratch.replacements, 0);
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

        let mut scratch = SongLuaProjectedMeshScratch::textured(3);
        let reused = build_song_lua_overlay_actor_with_scratch(
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
            Some(&mut scratch),
        )
        .expect_actor("textured ActorMultiVertex overlay should reuse its mesh");
        let Actor::ReusableTexturedMesh {
            offset: reused_offset,
            tint: reused_tint,
            vertices: reused_vertices,
            ..
        } = reused
        else {
            panic!("expected reusable textured ActorMultiVertex mesh");
        };
        assert_eq!(reused_offset, offset);
        assert_eq!(reused_tint, tint);
        assert_eq!(reused_vertices.as_slice(), vertices.as_ref());
        drop(reused_vertices);

        let build_glow = |scratch: &mut SongLuaProjectedMeshScratch| {
            build_song_lua_overlay_actor_with_scratch(
                &overlay,
                SongLuaOverlayState {
                    x: 12.0,
                    y: 24.0,
                    diffuse: [0.5, 0.25, 0.75, 0.5],
                    glow: [0.2, 0.4, 0.8, 0.75],
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
                Some(scratch),
            )
            .expect_actors("glowing ActorMultiVertex should reuse both meshes")
        };
        let glowing = build_glow(&mut scratch);
        let [
            Actor::ReusableTexturedMesh {
                vertices: base_vertices,
                ..
            },
            Actor::ReusableTexturedMesh {
                vertices: glow_vertices,
                blend: glow_blend,
                ..
            },
        ] = glowing.as_slice()
        else {
            panic!("expected reusable base and glow meshes, got {glowing:?}");
        };
        assert_eq!(*glow_blend, BlendMode::Add);
        assert_ne!(Arc::as_ptr(base_vertices), Arc::as_ptr(glow_vertices));
        for (source, glow_vertex) in vertices.iter().zip(glow_vertices.iter()) {
            assert_eq!(glow_vertex.pos, source.pos);
            assert_eq!(glow_vertex.uv, source.uv);
            assert_eq!(glow_vertex.color, [1.0, 1.0, 1.0, source.color[3]]);
        }
        let base_ptr = Arc::as_ptr(base_vertices);
        let glow_ptr = Arc::as_ptr(glow_vertices);
        drop(glowing);

        let next = build_glow(&mut scratch);
        let [
            Actor::ReusableTexturedMesh {
                vertices: next_base,
                ..
            },
            Actor::ReusableTexturedMesh {
                vertices: next_glow,
                ..
            },
        ] = next.as_slice()
        else {
            panic!("expected reusable base and glow meshes, got {next:?}");
        };
        assert_eq!(Arc::as_ptr(next_base), base_ptr);
        assert_eq!(Arc::as_ptr(next_glow), glow_ptr);
        assert_eq!(scratch.replacements, 0);
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
        } = &actor
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

        let SongLuaOverlayKind::Model { layers } = &overlay.kind else {
            unreachable!("test overlay is a model");
        };
        let multi_layer = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Model {
                layers: Arc::from(vec![
                    layers[0].clone(),
                    layers[0].clone(),
                    layers[0].clone(),
                ]),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let multi_state = SongLuaOverlayState {
            x: 12.0,
            y: 24.0,
            glow: [0.25, 0.5, 0.75, 0.5],
            ..SongLuaOverlayState::default()
        };
        let legacy = build_song_lua_overlay_actor(
            &multi_layer,
            multi_state,
            None,
            &asset_manager,
            323,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            1.0,
        )
        .expect_actors("multi-layer model should render");
        let mut direct = Vec::with_capacity(legacy.len());
        assert_eq!(
            append_song_lua_multi_actor_overlay(
                &mut direct,
                &multi_layer,
                multi_state,
                &asset_manager,
                323,
                screen_width(),
                screen_height(),
                0.0,
                0.0,
                1.0,
                None,
            ),
            Some(true)
        );
        assert_eq!(legacy.len(), 6);
        assert_eq!(format!("{legacy:?}"), format!("{direct:?}"));

        let mut scratches = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&multi_layer));
        let mut model_scratch = scratches.pop().expect("model scratch should be prewarmed");
        let prewarmed = model_scratch
            .model_glow_vertices
            .as_ref()
            .expect("model glow vertices should be compiled during entry")
            .clone();
        assert_eq!(prewarmed.len(), 3);
        let mut warmed = Vec::with_capacity(legacy.len());
        let mut append_warmed = |out: &mut Vec<Actor>| {
            out.clear();
            assert_eq!(
                append_song_lua_multi_actor_overlay(
                    out,
                    &multi_layer,
                    multi_state,
                    &asset_manager,
                    323,
                    screen_width(),
                    screen_height(),
                    0.0,
                    0.0,
                    1.0,
                    Some(&mut model_scratch),
                ),
                Some(true)
            );
        };
        append_warmed(&mut warmed);
        let mut normalized = warmed.clone();
        for actor in &mut normalized {
            if let Actor::TexturedMesh { geom_cache_key, .. } = actor {
                *geom_cache_key = INVALID_TMESH_CACHE_KEY;
            }
        }
        assert_eq!(format!("{legacy:?}"), format!("{normalized:?}"));
        for (layer_index, prewarmed_vertices) in prewarmed.iter().enumerate() {
            let Actor::TexturedMesh {
                geom_cache_key: base_key,
                ..
            } = &warmed[layer_index * 2]
            else {
                panic!("expected prewarmed static model base mesh");
            };
            let Actor::TexturedMesh {
                vertices,
                geom_cache_key: glow_key,
                blend,
                ..
            } = &warmed[layer_index * 2 + 1]
            else {
                panic!("expected prewarmed static model glow mesh");
            };
            assert_ne!(*base_key, INVALID_TMESH_CACHE_KEY);
            assert_ne!(*glow_key, INVALID_TMESH_CACHE_KEY);
            assert_ne!(base_key, glow_key);
            assert_eq!(*blend, BlendMode::Add);
            assert!(Arc::ptr_eq(vertices, prewarmed_vertices));
        }
        append_warmed(&mut warmed);
        for (layer_index, prewarmed_vertices) in prewarmed.iter().enumerate() {
            let Actor::TexturedMesh { vertices, .. } = &warmed[layer_index * 2 + 1] else {
                panic!("expected prewarmed static model glow mesh");
            };
            assert!(Arc::ptr_eq(vertices, prewarmed_vertices));
        }
    }

    #[test]
    fn song_lua_noteskin_actor_rotation_matches_noteskin_base_rotation() {
        let model_path =
            workspace_root().join("assets/noteskins/dance/ddr-note/_down tap note model.txt");
        let slots = deadsync_assets::noteskin::load_itg_model_slots_from_path(&model_path)
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
        let mut direct_rotation = Vec::with_capacity(actor_rotation.len());
        assert!(append_song_lua_noteskin_actors(
            &mut direct_rotation,
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
            None,
        ));
        assert_eq!(
            format!("{actor_rotation:?}"),
            format!("{direct_rotation:?}")
        );
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

        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::NoteskinActor {
                slots: Arc::clone(&slots),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let state = SongLuaOverlayState {
            rot_z_deg: 90.0,
            glow: [0.25, 0.5, 0.75, 0.5],
            ..SongLuaOverlayState::default()
        };
        let legacy = build_song_lua_overlay_actor(
            &overlay,
            state,
            None,
            &asset_manager,
            323,
            screen_width(),
            screen_height(),
            0.0,
            0.0,
            1.0,
        )
        .expect_actors("legacy noteskin model should render");
        let mut scratches = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
        let scratch = scratches
            .first_mut()
            .expect("noteskin model scratch should prewarm");
        let prewarmed_glow = scratch
            .noteskin_glow_vertices
            .as_ref()
            .expect("noteskin glow geometry should prewarm")
            .clone();
        let mut warmed = Vec::with_capacity(legacy.len());
        assert_eq!(
            append_song_lua_multi_actor_overlay(
                &mut warmed,
                &overlay,
                state,
                &asset_manager,
                323,
                screen_width(),
                screen_height(),
                0.0,
                0.0,
                1.0,
                Some(scratch),
            ),
            Some(true)
        );
        let mut normalized = warmed.clone();
        for actor in &mut normalized {
            if let Actor::TexturedMesh { geom_cache_key, .. } = actor {
                *geom_cache_key = INVALID_TMESH_CACHE_KEY;
            }
        }
        assert_eq!(format!("{legacy:?}"), format!("{normalized:?}"));
        for (slot_index, actors) in warmed.chunks_exact(2).enumerate() {
            let [
                Actor::TexturedMesh {
                    geom_cache_key: base_key,
                    ..
                },
                Actor::TexturedMesh {
                    vertices,
                    geom_cache_key: glow_key,
                    blend,
                    ..
                },
            ] = actors
            else {
                panic!("expected prewarmed noteskin base/glow pair");
            };
            assert_ne!(*base_key, INVALID_TMESH_CACHE_KEY);
            assert_ne!(*glow_key, INVALID_TMESH_CACHE_KEY);
            assert_ne!(base_key, glow_key);
            assert_eq!(*blend, BlendMode::Add);
            let expected = prewarmed_glow[slot_index]
                .as_ref()
                .expect("rendered model slot should have prewarmed glow geometry");
            assert!(Arc::ptr_eq(vertices, expected));
        }
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
                line_state: Box::new(SongLuaOverlayState {
                    y: 1.0,
                    diffuse: [0.8, 0.7, 0.6, 0.5],
                    ..SongLuaOverlayState::default()
                }),
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
                line_state: Box::new(SongLuaOverlayState {
                    y: 1.0,
                    diffuse: [0.8, 0.7, 0.6, 0.5],
                    ..SongLuaOverlayState::default()
                }),
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
    fn song_lua_graph_display_reuses_prewarmed_meshes_and_frame() {
        let values = Arc::from([0.25, 0.75]);
        let state = SongLuaOverlayState {
            x: 320.0,
            y: 100.0,
            valign: 0.0,
            diffuse: [0.5, 1.0, 1.0, 1.0],
            ..SongLuaOverlayState::default()
        };
        let body_state = SongLuaOverlayState {
            diffuse: [0.2, 0.5, 1.0, 0.75],
            ..SongLuaOverlayState::default()
        };
        let line_state = SongLuaOverlayState {
            y: 1.0,
            diffuse: [0.8, 0.7, 0.6, 0.5],
            ..SongLuaOverlayState::default()
        };
        let mut scratch = SongLuaProjectedMeshScratch::graph(6);
        let build = |scratch: &mut SongLuaProjectedMeshScratch| {
            song_lua_graph_display_actor(
                state,
                &values,
                body_state,
                line_state,
                [120.0, 60.0],
                1.0,
                1.0,
                324,
                Some(scratch),
            )
            .expect("GraphDisplay should render")
        };

        let actor = build(&mut scratch);
        let Actor::SharedFrame { children, .. } = &actor else {
            panic!("expected reused GraphDisplay shared frame, got {actor:?}");
        };
        let [
            Actor::Frame {
                children: graph_children,
                ..
            },
        ] = children.as_ref()
        else {
            panic!("expected GraphDisplay identity frame");
        };
        let [
            Actor::ReusableMesh {
                vertices: body_vertices,
                ..
            },
            Actor::ReusableMesh {
                vertices: line_vertices,
                ..
            },
        ] = graph_children.as_slice()
        else {
            panic!("expected reusable GraphDisplay body and line meshes");
        };
        assert_eq!(body_vertices[0].pos, [260.0, 145.0]);
        assert_eq!(body_vertices[5].pos, [380.0, 115.0]);
        assert_eq!(line_vertices[0].color, [0.4, 0.7, 0.6, 0.5]);
        let body_ptr = Arc::as_ptr(body_vertices);
        let line_ptr = Arc::as_ptr(line_vertices);
        drop(actor);

        let next = build(&mut scratch);
        let Actor::SharedFrame { children, .. } = &next else {
            panic!("expected reused GraphDisplay shared frame");
        };
        let [
            Actor::Frame {
                children: graph_children,
                ..
            },
        ] = children.as_ref()
        else {
            panic!("expected GraphDisplay identity frame");
        };
        let [
            Actor::ReusableMesh {
                vertices: next_body,
                ..
            },
            Actor::ReusableMesh {
                vertices: next_line,
                ..
            },
        ] = graph_children.as_slice()
        else {
            panic!("expected reusable GraphDisplay body and line meshes");
        };
        assert_eq!(Arc::as_ptr(next_body), body_ptr);
        assert_eq!(Arc::as_ptr(next_line), line_ptr);
        assert_eq!(scratch.replacements, 0);
        assert_eq!(
            scratch.graph_frame.as_ref().unwrap().stats().replacements,
            0
        );
        assert_eq!(scratch.graph_frame.as_ref().unwrap().stats().growths, 0);

        let changed_state = SongLuaOverlayState {
            x: state.x + 10.0,
            ..state
        };
        let changed = song_lua_graph_display_actor(
            changed_state,
            &values,
            body_state,
            line_state,
            [120.0, 60.0],
            1.0,
            1.0,
            324,
            Some(&mut scratch),
        )
        .expect("changed GraphDisplay should render");
        let Actor::SharedFrame { children, .. } = &changed else {
            panic!("expected changed GraphDisplay shared frame");
        };
        let [
            Actor::Frame {
                children: graph_children,
                ..
            },
        ] = children.as_ref()
        else {
            panic!("expected changed GraphDisplay identity frame");
        };
        let [
            Actor::ReusableMesh { vertices: body, .. },
            Actor::ReusableMesh { vertices: line, .. },
        ] = graph_children.as_slice()
        else {
            panic!("expected changed reusable GraphDisplay meshes");
        };
        assert_eq!(body[0].pos, [270.0, 145.0]);
        assert_ne!(Arc::as_ptr(body), body_ptr);
        assert_ne!(Arc::as_ptr(line), line_ptr);
        assert_eq!(scratch.replacements, 2);
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
            Actor::TexturedMesh {
                texture,
                vertices,
                z,
                ..
            } => {
                assert_eq!(z, 654);
                assert_eq!(vertices.len(), 6);
                assert!(Arc::ptr_eq(&texture, &white_texture_key()));
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
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
        let actor = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actor("rainbow-scroll bitmap text should render");

        match actor {
            Actor::Text { attributes, z, .. } => {
                assert_eq!(z, 780);
                assert!(matches!(attributes, TextAttributes::Shared(_)));
                assert_eq!(attributes.len(), 3);
                assert_eq!(attributes[0].color, [0.4, 0.3, 0.5, 1.0]);
                assert_eq!(attributes[1].color, [0.2, 0.6, 1.0, 1.0]);
                assert_eq!(attributes[2].color, [0.2, 0.8, 0.8, 1.0]);
            }
            other => panic!("expected rainbow-scroll bitmap text actor, got {other:?}"),
        }
    }

    #[test]
    fn long_song_lua_rainbow_text_uses_prewarmed_current_phase_buffer() {
        let text = "R".repeat(SONG_LUA_RAINBOW_TEXT_PREWARM_MAX_CHARS + 17);
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::from(text.as_str()),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
        assert!(scratch[0].rainbow_text_attributes.is_none());
        assert!(scratch[0].text_attribute_capacity >= text.chars().count());

        let actor = build_song_lua_overlay_actor_with_scratch(
            &overlay,
            SongLuaOverlayState {
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
            scratch.first_mut(),
        )
        .expect_actor("long rainbow-scroll bitmap text should render");

        let Actor::Text {
            attributes: TextAttributes::Reusable(attributes),
            ..
        } = actor
        else {
            panic!("expected text with reusable rainbow attributes");
        };
        assert_eq!(attributes.len(), text.chars().count());
        assert_eq!(attributes[0].color, [0.4, 0.3, 0.5, 1.0]);
        assert_eq!(attributes[1].color, [0.2, 0.6, 1.0, 1.0]);
        assert_eq!(scratch[0].replacements, 0);
    }

    #[test]
    fn song_lua_bitmaptext_shares_compiled_attributes() {
        let compiled: Arc<[TextAttribute]> = Arc::from([TextAttribute {
            start: 1,
            length: 2,
            color: [0.2, 0.4, 0.6, 0.8],
            vertex_colors: None,
            glow: None,
        }]);
        let overlay = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("ATTR"),
                stroke_color: None,
                attributes: Arc::clone(&compiled),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
        let actor = build_song_lua_overlay_actor_with_scratch(
            &overlay,
            SongLuaOverlayState::default(),
            None,
            &AssetManager::new(),
            781,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
            scratch.first_mut(),
        )
        .expect_actor("compiled text attributes should render");

        let Actor::Text {
            attributes: TextAttributes::Shared(rendered),
            ..
        } = actor
        else {
            panic!("expected text with shared attributes");
        };
        assert!(Arc::ptr_eq(&compiled, &rendered));
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
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
        let actors = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actors("text glow bitmap text should render");

        let first_ptr = match actors.as_slice() {
            [
                _,
                Actor::Text {
                    color,
                    stroke_color,
                    attributes: TextAttributes::Reusable(attributes),
                    blend,
                    ..
                },
            ] => {
                assert_eq!(color, &[1.0, 1.0, 1.0, 1.0]);
                assert_eq!(stroke_color, &Some([0.2, 0.3, 0.4, 0.5]));
                assert_eq!(blend, &BlendMode::Add);
                assert_eq!(attributes.len(), 1);
                assert_eq!(attributes[0].color, [1.0, 1.0, 1.0, 0.0]);
                Arc::as_ptr(attributes)
            }
            other => panic!("expected text plus stroke-only glow actors, got {other:?}"),
        };
        drop(actors);

        let actors = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actors("prewarmed text glow should render again");
        let [
            _,
            Actor::Text {
                attributes: TextAttributes::Reusable(attributes),
                ..
            },
        ] = actors.as_slice()
        else {
            panic!("expected reusable stroke-only glow attributes");
        };
        assert_eq!(Arc::as_ptr(attributes), first_ptr);
        assert_eq!(scratch[0].replacements, 0);
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
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&overlay));
        let actors = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actors("attribute glow bitmap text should render");

        let first_ptr = match actors.as_slice() {
            [
                _,
                Actor::Text {
                    color,
                    stroke_color,
                    attributes: TextAttributes::Reusable(attributes),
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
                Arc::as_ptr(attributes)
            }
            other => panic!("expected text plus attribute glow actors, got {other:?}"),
        };
        drop(actors);

        let actors = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actors("prewarmed attribute glow should render again");
        let [
            _,
            Actor::Text {
                attributes: TextAttributes::Reusable(attributes),
                ..
            },
        ] = actors.as_slice()
        else {
            panic!("expected reusable attribute glow attributes");
        };
        assert_eq!(Arc::as_ptr(attributes), first_ptr);
        assert_eq!(scratch[0].replacements, 0);
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
        let state = SongLuaOverlayState {
            x: 320.0,
            y: 240.0,
            diffuse: [0.8, 0.7, 0.6, 0.5],
            fadeleft: 0.25,
            faderight: 0.25,
            ..SongLuaOverlayState::default()
        };
        let camera = Some(SongLuaOverlayState {
            fov: Some(45.0),
            ..SongLuaOverlayState::default()
        });
        let actor = build_song_lua_overlay_actor(
            &sprite,
            state,
            camera,
            &asset_manager,
            792,
            640.0,
            480.0,
            0.0,
            0.0,
            0.0,
        )
        .expect_actor("projected fading sprite should render");

        let legacy_vertices = match actor {
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
                vertices
            }
            other => panic!("expected projected textured mesh, got {other:?}"),
        };

        let mut scratch = SongLuaProjectedMeshScratch::textured(PROJECTED_MESH_VERTEX_CAPACITY);
        let build_reused = |scratch: &mut SongLuaProjectedMeshScratch| {
            build_song_lua_overlay_actor_with_scratch(
                &sprite,
                state,
                camera,
                &asset_manager,
                792,
                640.0,
                480.0,
                0.0,
                0.0,
                0.0,
                Some(scratch),
            )
            .expect_actor("projected fading sprite should reuse its mesh")
        };
        let Actor::ReusableTexturedMesh {
            vertices: reused_vertices,
            ..
        } = build_reused(&mut scratch)
        else {
            panic!("expected reusable projected textured mesh");
        };
        assert_eq!(legacy_vertices.as_ref(), reused_vertices.as_slice());
        let buffer_ptr = Arc::as_ptr(&reused_vertices);
        drop(reused_vertices);
        let Actor::ReusableTexturedMesh {
            vertices: next_vertices,
            ..
        } = build_reused(&mut scratch)
        else {
            panic!("expected reusable projected textured mesh");
        };
        assert_eq!(Arc::as_ptr(&next_vertices), buffer_ptr);
        assert_eq!(scratch.replacements, 0);
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
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&text));
        let text_actor = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actor("bitmap text with non-multiplied attributes should render");

        let first_ptr = match text_actor {
            Actor::Text {
                color,
                attributes: TextAttributes::Reusable(attributes),
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
                Arc::as_ptr(&attributes)
            }
            other => {
                panic!("expected bitmap text actor with non-multiplied attributes, got {other:?}")
            }
        };

        let text_actor = build_song_lua_overlay_actor_with_scratch(
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
            scratch.first_mut(),
        )
        .expect_actor("prewarmed diffuse attributes should render again");
        let Actor::Text {
            attributes: TextAttributes::Reusable(attributes),
            ..
        } = text_actor
        else {
            panic!("expected reusable diffuse attributes");
        };
        assert_eq!(Arc::as_ptr(&attributes), first_ptr);
        assert_eq!(scratch[0].replacements, 0);
    }

    #[test]
    fn song_lua_overlay_applies_bitmaptext_uppercase_and_vertspacing_at_runtime() {
        let text = SongLuaOverlayActor {
            kind: SongLuaOverlayKind::BitmapText {
                font_name: "miso",
                font_path: std::path::PathBuf::from("Fonts/Common Normal.ini"),
                text: Arc::<str>::from("Mixed Straße"),
                stroke_color: None,
                attributes: empty_text_attributes(),
            },
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let mut scratch = song_lua_projected_mesh_scratch_for(std::slice::from_ref(&text))
            .pop()
            .expect("bitmap text should have overlay scratch");
        let cached = Arc::clone(
            scratch
                .uppercase_text
                .as_ref()
                .expect("uppercase text should be prewarmed"),
        );
        let build = |scratch: &mut SongLuaProjectedMeshScratch| {
            build_song_lua_overlay_actor_with_scratch(
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
                Some(scratch),
            )
            .expect_actor("bitmap text uppercase and vertspacing should render")
        };
        let text_actor = build(&mut scratch);

        match text_actor {
            Actor::Text {
                content,
                line_spacing,
                z,
                ..
            } => {
                assert_eq!(z, 788);
                assert_eq!(content.as_str(), "MIXED STRASSE");
                let TextContent::Shared(shared) = content else {
                    panic!("prewarmed uppercase text should stay shared");
                };
                assert!(Arc::ptr_eq(&shared, &cached));
                assert_eq!(line_spacing, Some(18));
            }
            other => {
                panic!("expected bitmap text actor with uppercase and vertspacing, got {other:?}")
            }
        }
        let Actor::Text { content, .. } = build(&mut scratch) else {
            panic!("second uppercase frame should remain text");
        };
        let TextContent::Shared(shared) = content else {
            panic!("second uppercase frame should stay shared");
        };
        assert!(Arc::ptr_eq(&shared, &cached));
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
    fn song_lua_foreground_owner_index_matches_visibility_and_layer_start() {
        let path = std::path::PathBuf::from("badapple.avi");
        let layer = |start_second: f32, overlay: SongLuaOverlayActor| {
            deadsync_gameplay::SongLuaVisualLayerRuntime {
                start_second,
                screen_width: 640.0,
                screen_height: 480.0,
                overlays: vec![overlay],
                overlay_eases: Vec::new(),
                overlay_ease_ranges: vec![0..0],
                overlay_events: vec![Vec::new()],
                song_foreground: SongLuaCapturedActor::default(),
                song_foreground_events: Vec::new(),
            }
        };
        let overlay = || SongLuaOverlayActor {
            kind: test_sprite_path_kind(path.clone()),
            name: None,
            parent_index: None,
            initial_state: SongLuaOverlayState::default(),
            message_commands: Vec::new(),
        };
        let visuals = SongLuaRuntimeVisuals {
            overlays: Vec::new(),
            overlay_eases: Vec::new(),
            overlay_ease_ranges: Vec::new(),
            overlay_events: Vec::new(),
            background_visual_layers: vec![layer(5.0, overlay())],
            foreground_visual_layers: vec![layer(10.0, overlay())],
            player_actors: std::array::from_fn(|_| SongLuaCapturedActor::default()),
            player_events: std::array::from_fn(|_| Vec::new()),
            song_foreground: SongLuaCapturedActor::default(),
            song_foreground_events: Vec::new(),
            hidden_players: [false; MAX_PLAYERS],
            note_hides: std::array::from_fn(|_| Default::default()),
            column_offsets: std::array::from_fn(|_| Vec::new()),
            screen_width: 640.0,
            screen_height: 480.0,
        };
        let mut index = SongLuaForegroundOwnerIndex::new(&visuals);
        index.select(Some(path.as_path()));
        let root_states = Vec::new();
        let mut background_states = vec![vec![SongLuaOverlayState::default()]];
        let mut foreground_states = vec![vec![SongLuaOverlayState {
            visible: false,
            ..SongLuaOverlayState::default()
        }]];

        assert!(!index.owns(
            4.99,
            &visuals,
            &root_states,
            &background_states,
            &foreground_states,
        ));
        assert!(index.owns(
            5.0,
            &visuals,
            &root_states,
            &background_states,
            &foreground_states,
        ));
        background_states[0][0].visible = false;
        assert!(!index.owns(
            9.99,
            &visuals,
            &root_states,
            &background_states,
            &foreground_states,
        ));
        foreground_states[0][0].visible = true;
        assert!(index.owns(
            10.0,
            &visuals,
            &root_states,
            &background_states,
            &foreground_states,
        ));

        index.select(Some(Path::new("not-owned.avi")));
        assert!(!index.owns(
            10.0,
            &visuals,
            &root_states,
            &background_states,
            &foreground_states,
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
