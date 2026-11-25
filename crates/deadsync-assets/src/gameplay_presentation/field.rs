//! Profile-selected notefield HUD assets and frame inputs shared by gameplay presenters.

use super::text_slots::*;
use crate::{gameplay_stage::GameplayCoreState as State, noteskin::SpriteSlot};
use deadlib_present::actors::{ActorResourceArena, SpriteSource};
use deadsync_core::hud_style::NotefieldHudStyle;
use deadsync_core::hud_style::{FieldFeedbackStyle, GameplayHoldAtlas, GameplayTapAtlas};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    FantasticWindowOptions, GameplayErrorBarTrim, blue_fantastic_window_ms,
    gameplay_error_bar_trim_max_window_ix,
};
use deadsync_notefield::{
    BrokenRunLookup, ComboHudFrame, ComboMilestoneAssets, CounterHudFrame, ErrorBarHudFrame,
    ErrorBarModes, IndicatorSprite, JudgmentHudFrame, LayoutMiniIndicatorPosition,
    MeasureCounterOptions, MiniHudFrame, NotefieldComposeRequest, NotefieldHudComposeResult,
    NotefieldHudFrameView, NotefieldOptions, PreparedNotefield, StreamProgressLookup,
    TapJudgmentHudFrame, TapJudgmentSprite, ZmodLayoutParams, compose_notefield_hud,
};
use deadsync_profile as profile_data;
use deadsync_song_lua::gameplay_presentation::FieldHudBuffers;
use deadsync_song_lua::playback::FieldFrame;
use std::cell::Cell;

#[path = "field_prewarm.rs"]
mod prewarm;
#[path = "field_text.rs"]
mod text;
#[path = "field_zmod.rs"]
mod zmod;

pub use prewarm::prewarm_text;
pub use text::preferred_mods_text;
use text::{
    cached_int_i32, error_bar_text_label, offset_ms_text, zmod_measure_counter_text,
    zmod_run_timer_fmt,
};
use zmod::{
    zmod_combo_font_name, zmod_indicator_mode, zmod_mini_indicator_text, zmod_mini_indicator_zoom,
    zmod_resolved_combo_color, zmod_small_combo_font,
};

const DEFAULT_NOTEFIELD_WIDTH: f32 = 256.0;
pub const TARGET_ARROW_PIXEL_SIZE: f32 = 64.0;

pub fn gameplay_notefield_widths(
    game: &State,
    assets: &crate::gameplay_noteskin::GameplayNoteskinAssets,
) -> [f32; MAX_PLAYERS] {
    std::array::from_fn(|player| {
        let Some(noteskin) = assets.noteskin[player].as_deref() else {
            return DEFAULT_NOTEFIELD_WIDTH;
        };
        let receptor = assets.receptor_noteskin[player]
            .as_deref()
            .unwrap_or(noteskin);
        let cols = game
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
            deadsync_gameplay::spacing_multiplier_for_percent(
                game.profiles()[player].spacing_percent,
            ),
        )
    })
}

pub fn notefield_layout_width(
    column_xs: &[i32],
    receptor_size: [i32; 2],
    cols: usize,
    spacing: f32,
) -> f32 {
    let cols = cols.min(column_xs.len());
    if cols == 0 {
        return DEFAULT_NOTEFIELD_WIDTH;
    }
    let (min_x, max_x) =
        column_xs[..cols]
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min_x, max_x), &x| {
                let x = x as f32 * spacing;
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

#[inline(always)]
fn player_blue_window_ms(state: &State, player_idx: usize) -> f32 {
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

// --- CONSTANTS ---

const TEXT_CACHE_LIMIT: usize = 8192;

#[inline(always)]
const fn column_flash_dimmed(brightness: profile_data::ColumnFlashBrightness) -> bool {
    matches!(brightness, profile_data::ColumnFlashBrightness::Dimmed)
}

#[inline(always)]
fn resolved_judgment_texture(
    profile: &profile_data::Profile,
) -> Option<&'static deadlib_assets::TextureChoice> {
    crate::textures::resolve_texture_choice_entry(
        profile.judgment_graphic.texture_key(),
        crate::textures::judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_hold_judgment_texture(
    profile: &profile_data::Profile,
) -> Option<&'static deadlib_assets::TextureChoice> {
    crate::textures::resolve_texture_choice_entry(
        profile.hold_judgment_graphic.texture_key(),
        crate::textures::hold_judgment_texture_choices(),
    )
}

#[inline(always)]
fn resolved_held_miss_texture(
    profile: &profile_data::Profile,
) -> Option<&'static deadlib_assets::TextureChoice> {
    crate::textures::resolve_texture_choice_entry(
        profile.held_miss_graphic.texture_key(),
        crate::textures::held_miss_texture_choices(),
    )
}

#[inline(always)]
fn sprite_frame_size(texture_key: &str, fallback: [f32; 2]) -> [f32; 2] {
    let Some(meta) = deadlib_assets::texture_dims(texture_key) else {
        return fallback;
    };
    let (w, h) = deadlib_assets::texture_source_frame_dims_from_real(texture_key, meta.w, meta.h);
    [w as f32, h as f32]
}

#[inline(always)]
fn judgment_frame_size(texture_key: &str) -> [f32; 2] {
    sprite_frame_size(texture_key, [0.0, 76.0])
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct JudgmentSpriteMetadata {
    frame_size: [f32; 2],
    frame_cols: usize,
    frame_rows: usize,
}

#[derive(Clone, Copy)]
struct CachedJudgmentSpriteMetadata {
    registry_generation: u64,
    metadata: JudgmentSpriteMetadata,
}

/// Profile-stable judgment asset choices plus texture-registry-aware metadata.
///
/// Choices are fixed for the gameplay screen lifetime. Sprite metadata is
/// refreshed after any texture-registry change, preserving late registration
/// and live texture replacement while keeping the steady frame path allocation
/// free.
pub struct ResolvedJudgmentAssets {
    judgment: Option<&'static deadlib_assets::TextureChoice>,
    hold_judgment: Option<&'static deadlib_assets::TextureChoice>,
    held_miss: Option<&'static deadlib_assets::TextureChoice>,
    held_miss_scale: Option<f32>,
    judgment_sprite: Cell<Option<CachedJudgmentSpriteMetadata>>,
    hold_judgment_sprite: Cell<Option<CachedJudgmentSpriteMetadata>>,
    held_miss_sprite: Cell<Option<CachedJudgmentSpriteMetadata>>,
}

impl ResolvedJudgmentAssets {
    pub fn from_profile(profile: &profile_data::Profile) -> Self {
        let held_miss = resolved_held_miss_texture(profile);
        Self {
            judgment: resolved_judgment_texture(profile),
            hold_judgment: resolved_hold_judgment_texture(profile),
            held_miss,
            held_miss_scale: held_miss.map(|texture| {
                if deadlib_assets::parse_texture_hints(texture.key.as_ref()).doubleres {
                    0.5
                } else {
                    1.0
                }
            }),
            judgment_sprite: Cell::new(None),
            hold_judgment_sprite: Cell::new(None),
            held_miss_sprite: Cell::new(None),
        }
    }

    #[inline(always)]
    pub const fn judgment(&self) -> Option<&'static deadlib_assets::TextureChoice> {
        self.judgment
    }

    #[inline(always)]
    pub const fn hold_judgment(&self) -> Option<&'static deadlib_assets::TextureChoice> {
        self.hold_judgment
    }

    #[inline(always)]
    pub fn held_miss(&self) -> Option<(&'static deadlib_assets::TextureChoice, f32)> {
        self.held_miss.zip(self.held_miss_scale)
    }

    #[inline(always)]
    fn judgment_sprite_metadata(&self) -> Option<JudgmentSpriteMetadata> {
        let registry_generation = deadlib_assets::texture_registry_generation();
        self.judgment_sprite_metadata_for_generation(registry_generation, |texture| {
            let (frame_cols, frame_rows) =
                deadlib_assets::parse_sprite_sheet_dims(texture.key.as_ref());
            JudgmentSpriteMetadata {
                frame_size: judgment_frame_size(texture.key.as_ref()),
                frame_cols: frame_cols as usize,
                frame_rows: frame_rows as usize,
            }
        })
    }

    #[inline(always)]
    fn judgment_sprite_metadata_for_generation(
        &self,
        registry_generation: u64,
        resolve: impl FnOnce(&deadlib_assets::TextureChoice) -> JudgmentSpriteMetadata,
    ) -> Option<JudgmentSpriteMetadata> {
        cached_sprite_metadata(
            self.judgment,
            &self.judgment_sprite,
            registry_generation,
            resolve,
        )
    }

    #[inline(always)]
    fn hold_judgment_sprite_metadata(&self) -> Option<JudgmentSpriteMetadata> {
        cached_sprite_metadata(
            self.hold_judgment,
            &self.hold_judgment_sprite,
            deadlib_assets::texture_registry_generation(),
            |texture| resolve_sprite_metadata(texture, [0.0; 2]),
        )
    }

    #[inline(always)]
    fn held_miss_sprite_metadata(&self) -> Option<JudgmentSpriteMetadata> {
        cached_sprite_metadata(
            self.held_miss,
            &self.held_miss_sprite,
            deadlib_assets::texture_registry_generation(),
            |texture| resolve_sprite_metadata(texture, [0.0; 2]),
        )
    }

    pub fn prewarm(&self) {
        let _ = self.judgment_sprite_metadata();
        let _ = self.hold_judgment_sprite_metadata();
        let _ = self.held_miss_sprite_metadata();
    }
}

fn resolve_sprite_metadata(
    texture: &deadlib_assets::TextureChoice,
    fallback: [f32; 2],
) -> JudgmentSpriteMetadata {
    let (frame_cols, frame_rows) = deadlib_assets::parse_sprite_sheet_dims(texture.key.as_ref());
    let frame_size = deadlib_assets::texture_dims(texture.key.as_ref()).map_or(fallback, |meta| {
        [
            meta.w as f32 / frame_cols.max(1) as f32,
            meta.h as f32 / frame_rows.max(1) as f32,
        ]
    });
    JudgmentSpriteMetadata {
        // Indicator SpriteBuilders previously used their texture's native
        // atlas-cell size. Preserve that exact contract; held-miss scaling
        // already handles the authored `(doubleres)` option separately.
        frame_size,
        frame_cols: frame_cols as usize,
        frame_rows: frame_rows as usize,
    }
}

fn cached_sprite_metadata(
    texture: Option<&deadlib_assets::TextureChoice>,
    cache: &Cell<Option<CachedJudgmentSpriteMetadata>>,
    registry_generation: u64,
    resolve: impl FnOnce(&deadlib_assets::TextureChoice) -> JudgmentSpriteMetadata,
) -> Option<JudgmentSpriteMetadata> {
    let texture = texture?;
    if let Some(cached) = cache.get()
        && cached.registry_generation == registry_generation
    {
        return Some(cached.metadata);
    }
    let metadata = resolve(texture);
    cache.set(Some(CachedJudgmentSpriteMetadata {
        registry_generation,
        metadata,
    }));
    Some(metadata)
}

#[inline(always)]
const fn gameplay_error_bar_trim(trim: profile_data::ErrorBarTrim) -> GameplayErrorBarTrim {
    match trim {
        profile_data::ErrorBarTrim::Off => GameplayErrorBarTrim::Off,
        profile_data::ErrorBarTrim::Fantastic => GameplayErrorBarTrim::Fantastic,
        profile_data::ErrorBarTrim::Excellent => GameplayErrorBarTrim::Excellent,
        profile_data::ErrorBarTrim::Great => GameplayErrorBarTrim::Great,
    }
}

#[inline(always)]
const fn error_bar_trim_max_window_ix(trim: profile_data::ErrorBarTrim) -> usize {
    gameplay_error_bar_trim_max_window_ix(gameplay_error_bar_trim(trim))
}

#[inline(always)]
fn zmod_layout_params(
    profile: &profile_data::Profile,
    style: NotefieldHudStyle,
    has_judgment_texture: bool,
) -> ZmodLayoutParams {
    // Zmod SL-Layout.lua: hasErrorBar checks multiple flags.
    let mut error_bar_mask = profile.error_bar_active_mask;
    if error_bar_mask.is_empty() {
        error_bar_mask =
            profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
    }
    let has_error_bar = !error_bar_mask.is_empty();
    let mini_indicator_position = match profile.mini_indicator_position {
        profile_data::MiniIndicatorPosition::Default => LayoutMiniIndicatorPosition::Default,
        profile_data::MiniIndicatorPosition::UnderUpArrow => {
            LayoutMiniIndicatorPosition::UnderUpArrow
        }
    };
    ZmodLayoutParams {
        judgment_height: style.judgment_height,
        has_error_bar,
        has_judgment_texture,
        error_bar_up: profile.error_bar_up,
        has_measure_counter: profile.measure_counter != profile_data::MeasureCounter::None,
        measure_counter_up: profile.measure_counter_up,
        broken_run: profile.broken_run,
        mini_indicator_position,
    }
}

/// Immutable HUD presentation inputs for one player and song.
///
/// The gameplay/render thread owns this fixed two-slot screen cache. It is
/// populated at gameplay entry from the immutable profile and resolved assets,
/// then read without synchronization, misses, growth, or eviction. Destruction
/// happens with the gameplay screen. A live frame performs one indexed borrow;
/// all clamps, masks, option translation, and font selection happen at warmup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayNotefieldPlan {
    pub hud_style: NotefieldHudStyle,
    pub options: NotefieldOptions,
    pub small_combo_font: &'static str,
    pub combo_font: Option<&'static str>,
    uses_default_tap: bool,
    tap_atlas: Option<GameplayTapAtlas>,
    uses_default_hold: bool,
    hold_atlas: Option<GameplayHoldAtlas>,
}

impl GameplayNotefieldPlan {
    pub fn install_feedback(
        &mut self,
        style: &'static FieldFeedbackStyle,
        tap: GameplayTapAtlas,
        hold: GameplayHoldAtlas,
    ) {
        self.hud_style.field_feedback = Some(style);
        self.tap_atlas = self.uses_default_tap.then_some(tap);
        self.hold_atlas = self.uses_default_hold.then_some(hold);
    }
}

pub fn gameplay_notefield_plan(
    profile: &profile_data::Profile,
    judgment_assets: &ResolvedJudgmentAssets,
    blue_fantastic_window_s: f32,
    style: NotefieldHudStyle,
) -> GameplayNotefieldPlan {
    let error_bar_mask = {
        let mut mask = profile.error_bar_active_mask;
        if mask.is_empty() {
            mask =
                profile_data::error_bar_mask_from_style(profile.error_bar, profile.error_bar_text);
        }
        mask
    };
    let has_judgment_texture = judgment_assets.judgment().is_some();
    let mini_indicator_position = match profile.mini_indicator_position {
        profile_data::MiniIndicatorPosition::Default => LayoutMiniIndicatorPosition::Default,
        profile_data::MiniIndicatorPosition::UnderUpArrow => {
            LayoutMiniIndicatorPosition::UnderUpArrow
        }
    };
    let options = NotefieldOptions {
        error_bar: !error_bar_mask.is_empty(),
        error_bar_text: error_bar_mask.contains(profile_data::ErrorBarMask::TEXT),
        combo_visible: !profile.hide_combo,
        judgment_offset: [
            profile
                .judgment_offset_x
                .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
                as f32,
            profile
                .judgment_offset_y
                .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
                as f32,
        ],
        combo_offset: [
            profile
                .combo_offset_x
                .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
                as f32,
            profile
                .combo_offset_y
                .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
                as f32,
        ],
        error_bar_offset: [
            profile
                .error_bar_offset_x
                .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
                as f32,
            profile
                .error_bar_offset_y
                .clamp(profile_data::HUD_OFFSET_MIN, profile_data::HUD_OFFSET_MAX)
                as f32,
        ],
        zmod_layout: zmod_layout_params(profile, style, has_judgment_texture),
        has_judgment_texture,
        error_bar_up: profile.error_bar_up,
        column_flash_compact: profile.column_flash_size == profile_data::ColumnFlashSize::Compact,
        column_flash_dimmed: column_flash_dimmed(profile.column_flash_brightness),
        hide_combo_explosions: profile.hide_combo_explosions,
        judgment_back: profile.judgment_back,
        show_fa_plus_window: profile.show_fa_plus_window,
        fa_plus_10ms_blue_window: profile.fa_plus_10ms_blue_window,
        split_15_10ms: profile.split_15_10ms,
        custom_fantastic_window: profile.custom_fantastic_window,
        judgment_tilt_enabled: profile.judgment_tilt,
        judgment_animation: match profile.judgment_animation {
            profile_data::JudgmentAnimation::Default => {
                deadsync_notefield::TapJudgmentAnimation::Default
            }
            profile_data::JudgmentAnimation::Still => {
                deadsync_notefield::TapJudgmentAnimation::Still
            }
            profile_data::JudgmentAnimation::Itg => deadsync_notefield::TapJudgmentAnimation::Itg,
        },
        judgment_tilt_min_ms: profile.tilt_min_threshold_ms as f32,
        judgment_tilt_max_ms: profile.tilt_max_threshold_ms as f32,
        judgment_tilt_multiplier: profile.tilt_multiplier,
        blue_fantastic_window_s,
        error_bar_modes: ErrorBarModes {
            colorful: error_bar_mask.contains(profile_data::ErrorBarMask::COLORFUL),
            monochrome: error_bar_mask.contains(profile_data::ErrorBarMask::MONOCHROME),
            highlight: error_bar_mask.contains(profile_data::ErrorBarMask::HIGHLIGHT),
            average: error_bar_mask.contains(profile_data::ErrorBarMask::AVERAGE),
        },
        error_bar_max_window_ix: error_bar_trim_max_window_ix(profile.error_bar_trim),
        monochrome_background: profile.background_filter.is_off(),
        error_bar_multi_tick: profile.error_bar_multi_tick,
        text_error_bar_centered: profile.text_error_bar_centered,
        short_average_error_bar: profile.short_average_error_bar_enabled,
        center_tick: profile.center_tick,
        error_ms_display: profile.error_ms_display,
        long_error_bar_enabled: profile.long_error_bar_enabled,
        long_error_bar_intensity: profile_data::clamp_long_error_bar_intensity(
            profile.long_error_bar_intensity,
        ),
        measure_counter: (profile.measure_counter != profile_data::MeasureCounter::None).then_some(
            MeasureCounterOptions {
                lookahead: profile.measure_counter_lookahead.min(4),
                multiplier: profile.measure_counter.multiplier(),
                vertical: profile.measure_counter_vert,
                left: profile.measure_counter_left,
                broken_run: profile.broken_run,
                run_timer: profile.run_timer,
            },
        ),
        mini_indicator_position,
        mini_indicator_zoom: zmod_mini_indicator_zoom(profile.mini_indicator_size),
        counter_left: profile.measure_counter_left,
    };
    GameplayNotefieldPlan {
        hud_style: style,
        options,
        small_combo_font: zmod_small_combo_font(profile.combo_font),
        combo_font: zmod_combo_font_name(profile.combo_font),
        uses_default_tap: profile.judgment_graphic.texture_key()
            == Some(profile_data::JudgmentGraphic::DEFAULT_KEY),
        tap_atlas: None,
        uses_default_hold: profile.hold_judgment_graphic.texture_key()
            == Some(profile_data::HoldJudgmentGraphic::DEFAULT_KEY),
        hold_atlas: None,
    }
}

pub fn compose_hud(
    state: &State,
    judgment_assets: &ResolvedJudgmentAssets,
    combo_milestone_assets: Option<&ComboMilestoneAssets>,
    notefield_plan: &GameplayNotefieldPlan,
    frame: &FieldFrame<'_>,
    request: &NotefieldComposeRequest<'_, SpriteSlot>,
    prepared: &PreparedNotefield<'_, SpriteSlot>,
    player_color: [f32; 4],
    textures: &(impl deadlib_present::texture::TextureContext + ?Sized),
    broken_run_lookup: &BrokenRunLookup,
    stream_progress_lookup: &StreamProgressLookup,
    buffers: FieldHudBuffers<'_>,
) -> NotefieldHudComposeResult {
    let actor_resources = buffers.resources;
    let player_idx = frame.player;
    let profile = &state.profiles()[player_idx].0;
    let hold_judgment_texture = judgment_assets.hold_judgment();
    let held_miss_texture = judgment_assets.held_miss();
    let p = &state.players()[player_idx];

    let style = &request.hud_style;
    let judgment_texture = judgment_assets.judgment();
    let options = &request.options;
    let elapsed_screen = request.visual.elapsed_screen_s;
    let frame_plan = prepared.frame_plan;
    let col_start = frame_plan.col_start;
    let num_cols = frame_plan.num_cols;
    let mc_font_name = notefield_plan.small_combo_font;
    let blind_active = prepared.blind_active;

    let combo_active = frame.combo_visible
        && NotefieldHudFrameView::combo_active(
            request.view.hide_combo,
            blind_active,
            options.combo_visible,
        );
    let combo_frame = combo_active.then(|| {
        let milestone_assets = combo_milestone_assets;
        let combo_color = if p.miss_combo < style.combo_feedback.threshold
            && p.combo >= style.combo_feedback.threshold
        {
            zmod_resolved_combo_color(state, p, profile, player_idx)
        } else {
            [1.0; 4]
        };
        ComboHudFrame {
            milestones: &p.combo_milestones,
            milestone_assets,
            combo: p.combo,
            miss_combo: p.miss_combo,
            player_color,
            combo_color,
            font: notefield_plan.combo_font,
            number_text_slot: FRAME_TEXT_COMBO_BASE + player_idx as u8,
            label_text_slot: FRAME_TEXT_COMBO_LABEL_BASE + player_idx as u8,
            recent_tap_age: style.field_feedback.and_then(|_| {
                p.last_judgment
                    .as_ref()
                    .map(|tap| (elapsed_screen - tap.started_at_screen_s).max(0.0))
            }),
            transform: frame.combo_transform,
        }
    });

    let offset_indicator_active = options.error_ms_display
        && ErrorBarHudFrame::offset_active(
            p.offset_indicator_text,
            elapsed_screen,
            style.error_bar.offset_indicator_duration,
        );
    let error_bar_frame = NotefieldHudFrameView::error_active(
        blind_active,
        options.error_bar,
        offset_indicator_active,
    )
    .then(|| ErrorBarHudFrame {
        mono_ticks: &p.error_bar_mono_ticks,
        color_ticks: &p.error_bar_color_ticks,
        average_ticks: &p.error_bar_avg_ticks,
        color_bar_started_at: p.error_bar_color_bar_started_at,
        average_bar_started_at: p.error_bar_avg_bar_started_at,
        flash_early: &p.error_bar_color_flash_early,
        flash_late: &p.error_bar_color_flash_late,
        timing_windows_s: state.timing_profile_windows_s(),
        offset_indicator: p.offset_indicator_text,
        long_average_tick: p.error_bar_long_avg_tick,
        long_average_active: p.error_bar_long_avg_visible,
        text: p.error_bar_text,
        frame_text_slot: FRAME_TEXT_ERROR_BASE
            + player_idx as u8 * deadsync_notefield::ERROR_BAR_TEXT_SLOTS_PER_PLAYER,
        offset_text: offset_ms_text,
        text_label: error_bar_text_label,
    });

    let counter_frame = options.measure_counter.map(|_| CounterHudFrame {
        segments: state.measure_counter_segments(player_idx),
        broken_run_lookup,
        current_bpm: state.current_bpm_display(),
        font: mc_font_name,
        frame_text_slot: FRAME_TEXT_COUNTER_BASE
            + player_idx as u8 * deadsync_notefield::COUNTER_TEXT_SLOTS_PER_PLAYER,
        counter_text: zmod_measure_counter_text,
        timer_text: zmod_run_timer_fmt,
    });
    let mini_frame =
        zmod_mini_indicator_text(state, p, profile, player_idx, stream_progress_lookup).map(
            |(text, color)| MiniHudFrame {
                text,
                color,
                failed: p.is_failing || p.life <= 0.0,
                font: mc_font_name,
            },
        );

    let tap = if frame.judgment_visible
        && !blind_active
        && let Some(render) = p.last_judgment.as_ref()
    {
        use deadsync_notefield::TapJudgmentAnimation;
        let theme_atlas = notefield_plan.tap_atlas.filter(|atlas| {
            frame.judgment_texture.is_none()
                && textures.texture_handle(atlas.key) != deadlib_render_core::INVALID_TEXTURE_HANDLE
        });
        let animation = match (options.judgment_animation, theme_atlas) {
            (TapJudgmentAnimation::Default, Some(atlas)) => {
                TapJudgmentAnimation::Tween(atlas.tween)
            }
            (animation, _) => animation,
        };
        let sprite = if !TapJudgmentHudFrame::render_active(render, elapsed_screen, animation) {
            None
        } else if let Some(texture) = frame.judgment_texture {
            Some(TapJudgmentSprite {
                source: SpriteSource::TextureHandle {
                    key: texture.key.clone(),
                    handle: textures.texture_handle(&texture.key),
                    generation: textures.texture_registry_generation(),
                },
                frame_size: texture.frame_size,
                frame_cols: texture.frame_cols,
                frame_rows: texture.frame_rows,
            })
        } else if let Some(atlas) = theme_atlas {
            Some(TapJudgmentSprite {
                source: SpriteSource::static_texture(atlas.key),
                frame_size: atlas.frame_size,
                frame_cols: atlas.frame_cols,
                frame_rows: atlas.frame_rows,
            })
        } else {
            judgment_texture
                .zip(judgment_assets.judgment_sprite_metadata())
                .map(|(texture, sprite)| TapJudgmentSprite {
                    source: texture.actor_texture_source(actor_resources, textures),
                    frame_size: sprite.frame_size,
                    frame_cols: sprite.frame_cols,
                    frame_rows: sprite.frame_rows,
                })
        };
        sprite.map(|sprite| TapJudgmentHudFrame {
            render,
            sprite,
            animation,
            transform: frame.judgment_transform,
        })
    } else {
        None
    };
    let held_misses = if frame.judgment_visible && !blind_active && held_miss_texture.is_some() {
        state.held_miss_judgments_for_columns(col_start, num_cols)
    } else {
        &[]
    };
    let hold_judgments = if frame.judgment_visible
        && !blind_active
        && (hold_judgment_texture.is_some() || notefield_plan.hold_atlas.is_some())
    {
        state.hold_judgments_for_columns(col_start, num_cols)
    } else {
        &[]
    };
    let held_miss_sprite = if held_misses.iter().any(Option::is_some)
        && let Some((texture, scale)) = held_miss_texture
        && let Some(sprite) = judgment_assets.held_miss_sprite_metadata()
    {
        Some(IndicatorSprite {
            source: texture.actor_texture_source(actor_resources, textures),
            frame_size: sprite.frame_size,
            frame_cols: sprite.frame_cols,
            frame_rows: sprite.frame_rows,
            scale,
        })
    } else {
        None
    };
    let (hold_sprite, hold_style) = if !hold_judgments.iter().any(Option::is_some) {
        (None, None)
    } else if let Some(atlas) = notefield_plan.hold_atlas.filter(|atlas| {
        textures.texture_handle(atlas.key) != deadlib_render_core::INVALID_TEXTURE_HANDLE
    }) {
        (
            Some(IndicatorSprite {
                source: SpriteSource::static_texture(atlas.key),
                frame_size: atlas.frame_size,
                frame_cols: 1,
                frame_rows: 2,
                scale: 1.0,
            }),
            Some(atlas.style),
        )
    } else {
        let sprite = hold_judgment_texture
            .zip(judgment_assets.hold_judgment_sprite_metadata())
            .map(|(texture, sprite)| IndicatorSprite {
                source: texture.actor_texture_source(actor_resources, textures),
                frame_size: sprite.frame_size,
                frame_cols: sprite.frame_cols,
                frame_rows: sprite.frame_rows,
                scale: 1.0,
            });
        (sprite, None)
    };
    let judgment_frame = (tap.is_some() || held_miss_sprite.is_some() || hold_sprite.is_some())
        .then_some(JudgmentHudFrame {
            tap,
            held_misses,
            held_miss_sprite,
            hold_judgments,
            hold_sprite,
            hold_style,
        });
    let hud_frame = NotefieldHudFrameView {
        combo: combo_frame,
        error_bar: error_bar_frame,
        counter: counter_frame,
        mini: mini_frame,
        judgment: judgment_frame,
    };
    compose_notefield_hud(
        buffers.actors,
        buffers.draws,
        request,
        prepared,
        &hud_frame,
        buffers.capture,
    )
}

pub fn prewarm_hud_actor_resources(
    arena: &ActorResourceArena,
    profiles: [&profile_data::Profile; MAX_PLAYERS],
    num_players: usize,
) {
    for profile in profiles.into_iter().take(num_players) {
        for texture in [
            resolved_judgment_texture(profile),
            resolved_hold_judgment_texture(profile),
            resolved_held_miss_texture(profile),
        ]
        .into_iter()
        .flatten()
        {
            let _ = texture.actor_texture_source(arena, &deadlib_assets::METADATA_TEXTURE_CONTEXT);
        }
    }
    arena.lock_growth();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_judgment_metadata_refreshes_only_when_registry_generation_changes() {
        let choice = deadlib_assets::TextureChoice::new("judgement.png".into(), String::new());
        let cache = Cell::new(None);
        let first = JudgmentSpriteMetadata {
            frame_size: [128.0, 64.0],
            frame_cols: 2,
            frame_rows: 7,
        };
        let refreshed = JudgmentSpriteMetadata {
            frame_size: [256.0, 128.0],
            frame_cols: 4,
            frame_rows: 8,
        };

        assert_eq!(
            cached_sprite_metadata(Some(&choice), &cache, 10, |_| first),
            Some(first)
        );
        assert_eq!(
            cached_sprite_metadata(Some(&choice), &cache, 10, |_| {
                panic!("same registry generation must reuse cached metadata")
            }),
            Some(first)
        );
        assert_eq!(
            cached_sprite_metadata(Some(&choice), &cache, 11, |_| refreshed),
            Some(refreshed)
        );
    }

    #[test]
    fn judgment_frame_size_uses_logical_atlas_frame_dims() {
        let censored = "judgements/Test Censored 1x7 (doubleres).png";
        let tight_censored = "judgements/Test Censored Tight 1x7 (doubleres).png";
        let love = "judgements/Test Love 2x7 (doubleres).png";
        deadlib_assets::register_texture_dims(censored, 600, 1400);
        deadlib_assets::register_texture_dims(tight_censored, 600, 1050);
        deadlib_assets::register_texture_dims(love, 880, 1036);

        assert_eq!(judgment_frame_size(censored), [300.0, 100.0]);
        assert_eq!(judgment_frame_size(tight_censored), [300.0, 75.0]);
        assert_eq!(judgment_frame_size(love), [220.0, 74.0]);

        let visible_art_h = 68.0 / 2.0;
        let original_drawn_art_h = visible_art_h * judgment_frame_size(censored)[1] / 100.0;
        let tight_drawn_art_h = visible_art_h * judgment_frame_size(tight_censored)[1] / 75.0;
        assert_eq!(original_drawn_art_h, tight_drawn_art_h);
    }
}
