use crate::game::parsing::song_lua::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaCompileContext, SongLuaDifficulty,
    SongLuaEaseTarget, SongLuaMessageEvent, SongLuaModWindow, SongLuaOverlayActor,
    SongLuaOverlayEase, SongLuaOverlayMessageCommand, SongLuaOverlayState, SongLuaPlayerContext,
    SongLuaSpanMode, SongLuaSpeedMod, SongLuaTimeUnit,
};
use deadsync_chart::SongData;
use deadsync_chart::{ChartData, GameplayChartData};
pub(super) use deadsync_gameplay::parse_song_lua_runtime_mods;
#[cfg(test)]
pub(super) use deadsync_gameplay::song_lua_ease_window_value;
use deadsync_gameplay::{
    ActiveAttackRefreshInput, ActiveAttackRefreshState, ChartAttackTransformPlayer,
    GameplayAttackMode, SongLuaCompilePlayStyle, append_song_lua_ease_targets,
    apply_chart_attack_transforms as apply_chart_attack_transforms_to_notes,
    begin_outro_attack_visual_clear, build_attack_mask_windows as build_mask_windows_from_attacks,
    build_attack_windows_for_mode, effective_attack_accel_effects,
    effective_attack_perspective_effects, effective_attack_scroll_effects,
    effective_attack_scroll_speed, effective_attack_visibility_effects,
    effective_attack_visual_effects, group_song_lua_overlay_eases, offset_song_lua_message_events,
    offset_song_lua_overlay_eases, player_chart_changes_for_options, refresh_active_attack_player,
    song_lua_compile_player_screen_x as gameplay_song_lua_compile_player_screen_x,
    song_lua_extend_column_offset_tails, song_lua_extend_ease_tails, song_lua_message_second,
    song_lua_sustain_end_second as gameplay_song_lua_sustain_end_second,
    song_lua_target_matches_player, song_lua_time_to_second as gameplay_song_lua_time_to_second,
    song_lua_window_seconds as gameplay_song_lua_window_seconds,
};
pub(super) use deadsync_gameplay::{
    AttackMaskWindow, SongLuaEaseMaskTarget, SongLuaEaseMaskWindow, SongLuaPlayerTransform,
    SongLuaPlayerTransformValues, SongLuaRuntimeSpanMode, SongLuaRuntimeTimeUnit,
};
pub use deadsync_gameplay::{
    SongLuaColumnOffsetWindowRuntime, SongLuaNoteHideWindowRuntime, SongLuaOverlayMessageRuntime,
};
use deadsync_profile as profile_data;
use deadsync_rules::note::Note;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingData;
use log::{debug, info, trace, warn};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use super::{
    AccelEffects, AppearanceEffects, ChartAttackEffects, GameplaySession, GameplayViewport,
    MAX_PLAYERS, MiniAttackMode, PerspectiveEffects, ScrollEffects, State, VisibilityEffects,
    VisualEffects, effective_mini_percent, perspective_effects_from_profile,
    scroll_effects_from_option, spacing_multiplier_for_percent,
};

#[inline(always)]
pub(super) fn gameplay_attack_mode(attack_mode: profile_data::AttackMode) -> GameplayAttackMode {
    match attack_mode {
        profile_data::AttackMode::Off => GameplayAttackMode::Off,
        profile_data::AttackMode::On => GameplayAttackMode::On,
        profile_data::AttackMode::Random => GameplayAttackMode::Random,
    }
}

pub type SongLuaOverlayEaseWindowRuntime = deadsync_gameplay::SongLuaOverlayEaseWindowRuntime<
    crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
>;

#[derive(Clone, Debug)]
pub struct SongLuaVisualLayerRuntime {
    pub start_second: f32,
    pub screen_width: f32,
    pub screen_height: f32,
    pub overlays: Vec<SongLuaOverlayActor>,
    pub overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime>,
    pub overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    pub overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    pub song_foreground: SongLuaCapturedActor,
    pub song_foreground_events: Vec<SongLuaOverlayMessageRuntime>,
}

#[derive(Clone, Debug)]
pub struct GameplayCompiledSongLua {
    pub compiled: CompiledSongLua,
    pub compile_ms: f64,
}

#[derive(Clone, Debug)]
pub struct GameplaySongLuaLayer {
    pub start_beat: f32,
    pub compiled: CompiledSongLua,
}

#[derive(Clone, Debug, Default)]
pub struct GameplaySongLuaData {
    pub primary: Option<GameplayCompiledSongLua>,
    pub background_layers: Vec<GameplaySongLuaLayer>,
    pub foreground_layers: Vec<GameplaySongLuaLayer>,
}

pub(super) fn build_attack_mask_windows_for_player(
    chart_attacks: Option<&str>,
    attack_mode: profile_data::AttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let attacks = build_attack_windows_for_mode(
        chart_attacks,
        gameplay_attack_mode(attack_mode),
        player,
        base_seed,
        song_length_seconds,
    );
    if attacks.is_empty() {
        return Vec::new();
    }
    build_mask_windows_from_attacks(&attacks)
}

#[inline(always)]
fn song_lua_runtime_span_mode(span_mode: SongLuaSpanMode) -> SongLuaRuntimeSpanMode {
    match span_mode {
        SongLuaSpanMode::Len => SongLuaRuntimeSpanMode::Len,
        SongLuaSpanMode::End => SongLuaRuntimeSpanMode::End,
    }
}

#[inline(always)]
fn song_lua_runtime_time_unit(unit: SongLuaTimeUnit) -> SongLuaRuntimeTimeUnit {
    match unit {
        SongLuaTimeUnit::Beat => SongLuaRuntimeTimeUnit::Beat,
        SongLuaTimeUnit::Second => SongLuaRuntimeTimeUnit::Second,
    }
}

#[inline(always)]
fn song_lua_time_to_second(
    unit: SongLuaTimeUnit,
    value: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> f32 {
    gameplay_song_lua_time_to_second(
        song_lua_runtime_time_unit(unit),
        value,
        timing_player,
        global_offset_seconds,
    )
}

fn song_lua_message_seconds(
    messages: &[SongLuaMessageEvent],
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<Option<f32>> {
    messages
        .iter()
        .map(|message| song_lua_message_second(message.beat, timing_player, global_offset_seconds))
        .collect()
}

fn song_lua_window_seconds(
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<(f32, f32)> {
    gameplay_song_lua_window_seconds(
        song_lua_runtime_time_unit(unit),
        start,
        limit,
        song_lua_runtime_span_mode(span_mode),
        timing_player,
        global_offset_seconds,
    )
}

fn song_lua_sustain_end_second(
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    sustain: Option<f32>,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    end_second: f32,
) -> f32 {
    gameplay_song_lua_sustain_end_second(
        song_lua_runtime_time_unit(unit),
        start,
        limit,
        song_lua_runtime_span_mode(span_mode),
        sustain,
        timing_player,
        global_offset_seconds,
        end_second,
    )
}

fn build_song_lua_constant_window(
    window: &SongLuaModWindow,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<AttackMaskWindow> {
    let (start_second, end_second) = song_lua_window_seconds(
        window.unit,
        window.start,
        window.limit,
        window.span_mode,
        timing_player,
        global_offset_seconds,
    )?;
    if end_second <= start_second {
        return None;
    }
    let mods = parse_song_lua_runtime_mods(&window.mods);
    if !mods.has_runtime_mask_effect() {
        return None;
    }
    Some(AttackMaskWindow {
        start_second,
        end_second,
        sustain_end_second: f32::MAX,
        persist_after_end: true,
        clear_all: mods.clear_all,
        chart: ChartAttackEffects::default(),
        accel: mods.accel,
        visual: mods.visual,
        visual_speed: mods.visual_speed,
        appearance: mods.appearance,
        appearance_speed: mods.appearance_speed,
        visibility: mods.visibility,
        scroll: mods.scroll,
        scroll_approach_speed: mods.scroll_approach_speed,
        perspective: mods.perspective,
        scroll_speed: mods.scroll_speed,
        mini_percent: mods.mini_percent,
        mini_mode: MiniAttackMode::Delta,
        mini_speed: mods.mini_speed,
    })
}

pub(super) fn build_song_lua_constant_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let mut out = Vec::new();
    for window in &compiled.time_mods {
        if song_lua_target_matches_player(window.player, player)
            && let Some(window) =
                build_song_lua_constant_window(window, timing_player, global_offset_seconds)
        {
            out.push(window);
        }
    }
    for window in &compiled.beat_mods {
        if song_lua_target_matches_player(window.player, player)
            && let Some(window) =
                build_song_lua_constant_window(window, timing_player, global_offset_seconds)
        {
            out.push(window);
        }
    }
    out
}

pub(super) fn build_song_lua_ease_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[AttackMaskWindow],
) -> (Vec<SongLuaEaseMaskWindow>, usize) {
    let mut out = Vec::new();
    let mut unsupported_targets = 0usize;
    for window in &compiled.eases {
        if !song_lua_target_matches_player(window.player, player) {
            continue;
        }
        let Some((start_second, end_second)) = song_lua_window_seconds(
            window.unit,
            window.start,
            window.limit,
            window.span_mode,
            timing_player,
            global_offset_seconds,
        ) else {
            continue;
        };
        let sustain_end_second = song_lua_sustain_end_second(
            window.unit,
            window.start,
            window.limit,
            window.span_mode,
            window.sustain,
            timing_player,
            global_offset_seconds,
            end_second,
        );
        if sustain_end_second <= start_second {
            continue;
        }
        match &window.target {
            SongLuaEaseTarget::Mod(target_name) => {
                if !append_song_lua_ease_targets(
                    &mut out,
                    start_second,
                    end_second,
                    sustain_end_second,
                    target_name,
                    window.from,
                    window.to,
                    window.easing.as_deref(),
                    window.opt1,
                    window.opt2,
                ) {
                    unsupported_targets += 1;
                    debug!(
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
            SongLuaEaseTarget::PlayerX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZ => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZ,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerRotationX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerRotationX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerRotationZ => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerRotationZ,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerRotationY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerRotationY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerSkewX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerSkewX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerSkewY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerSkewY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoom => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoom,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoomX => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoomX,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoomY => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoomY,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::PlayerZoomZ => out.push(SongLuaEaseMaskWindow {
                start_second,
                end_second,
                sustain_end_second,
                target: SongLuaEaseMaskTarget::PlayerZoomZ,
                from: window.from,
                to: window.to,
                easing: window.easing.clone(),
                opt1: window.opt1,
                opt2: window.opt2,
            }),
            SongLuaEaseTarget::Function => {}
        }
    }
    song_lua_extend_ease_tails(&mut out, constant_windows);
    (out, unsupported_targets)
}

pub(super) fn build_song_lua_column_offset_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
) -> Vec<SongLuaColumnOffsetWindowRuntime> {
    let mut out = Vec::new();
    for window in &compiled.column_offsets {
        if window.player != player {
            continue;
        }
        let Some((start_second, end_second)) = song_lua_window_seconds(
            window.unit,
            window.start,
            window.limit,
            window.span_mode,
            timing_player,
            global_offset_seconds,
        ) else {
            continue;
        };
        let sustain_end_second = song_lua_sustain_end_second(
            window.unit,
            window.start,
            window.limit,
            window.span_mode,
            window.sustain,
            timing_player,
            global_offset_seconds,
            end_second,
        );
        out.push(SongLuaColumnOffsetWindowRuntime {
            column: window.column,
            start_second,
            end_second,
            sustain_end_second,
            from_y: window.from_y,
            to_y: window.to_y,
            easing: window.easing.clone(),
            opt1: window.opt1,
            opt2: window.opt2,
        });
    }
    song_lua_extend_column_offset_tails(&mut out);
    out
}

#[cfg(test)]
pub(super) fn build_song_lua_overlay_ease_windows(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let message_seconds =
        song_lua_message_seconds(&compiled.messages, timing_player, global_offset_seconds);
    let overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        global_offset_seconds,
        &overlay_events,
    )
}

fn build_song_lua_overlay_ease_windows_with_events(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let mut out = Vec::new();
    for ease in &compiled.overlay_eases {
        let Some((start_second, end_second)) = song_lua_window_seconds(
            ease.unit,
            ease.start,
            ease.limit,
            ease.span_mode,
            timing_player,
            global_offset_seconds,
        ) else {
            continue;
        };
        if end_second < start_second {
            continue;
        }
        let sustain_end_second = song_lua_sustain_end_second(
            ease.unit,
            ease.start,
            ease.limit,
            ease.span_mode,
            ease.sustain,
            timing_player,
            global_offset_seconds,
            end_second,
        );
        let cutoff_second =
            song_lua_overlay_ease_cutoff_second(compiled, ease, overlay_events, start_second);
        out.push(SongLuaOverlayEaseWindowRuntime {
            overlay_index: ease.overlay_index,
            start_second,
            end_second,
            sustain_end_second,
            cutoff_second,
            from: ease.from,
            to: ease.to,
            easing: ease.easing.clone(),
            opt1: ease.opt1,
            opt2: ease.opt2,
        });
    }
    out
}

fn build_song_lua_overlay_message_events_with_seconds(
    compiled: &CompiledSongLua,
    message_seconds: &[Option<f32>],
) -> Vec<Vec<SongLuaOverlayMessageRuntime>> {
    compiled
        .overlays
        .iter()
        .map(|overlay| {
            build_song_lua_actor_message_events_with_seconds(
                &compiled.messages,
                message_seconds,
                &overlay.message_commands,
            )
        })
        .collect()
}

fn build_song_lua_actor_message_events_with_seconds(
    messages: &[SongLuaMessageEvent],
    message_seconds: &[Option<f32>],
    commands: &[SongLuaOverlayMessageCommand],
) -> Vec<SongLuaOverlayMessageRuntime> {
    let command_indices = song_lua_message_command_indices(commands);
    let mut out = Vec::new();
    for (idx, message) in messages.iter().enumerate() {
        let Some(event_second) = message_seconds.get(idx).copied().flatten() else {
            continue;
        };
        let Some(&command_index) = command_indices.get(&message.message.to_ascii_lowercase())
        else {
            continue;
        };
        out.push(SongLuaOverlayMessageRuntime {
            event_second,
            command_index,
        });
    }
    out
}

fn song_lua_message_command_indices(
    commands: &[SongLuaOverlayMessageCommand],
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (idx, command) in commands.iter().enumerate() {
        out.entry(command.message.to_ascii_lowercase())
            .or_insert(idx);
    }
    out
}

fn song_lua_overlay_ease_cutoff_second(
    compiled: &CompiledSongLua,
    ease: &SongLuaOverlayEase,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    start_second: f32,
) -> Option<f32> {
    const SAME_TICK_CUTOFF_EPSILON: f32 = 0.001;

    let overlay = compiled.overlays.get(ease.overlay_index)?;
    let mut cutoff_second: Option<f32> = None;
    let events = overlay_events.get(ease.overlay_index)?;
    for event in events {
        let event_second = event.event_second;
        if !event_second.is_finite() || event_second < start_second {
            continue;
        }
        let Some(command) = overlay.message_commands.get(event.command_index) else {
            continue;
        };
        for block in &command.blocks {
            if !song_lua_overlay_delta_overlaps(&ease.from, &block.delta)
                && !song_lua_overlay_delta_overlaps(&ease.to, &block.delta)
            {
                continue;
            }
            let block_second = event_second + block.start.max(0.0);
            if !block_second.is_finite() || block_second <= start_second + SAME_TICK_CUTOFF_EPSILON
            {
                continue;
            }
            cutoff_second = Some(match cutoff_second {
                Some(current) => current.min(block_second),
                None => block_second,
            });
        }
    }
    cutoff_second
}

fn song_lua_overlay_delta_overlaps(
    left: &crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
    right: &crate::game::parsing::song_lua::SongLuaOverlayStateDelta,
) -> bool {
    macro_rules! overlap {
        ($field:ident) => {
            if left.$field.is_some() && right.$field.is_some() {
                return true;
            }
        };
    }
    overlap!(x);
    overlap!(y);
    overlap!(z);
    overlap!(halign);
    overlap!(valign);
    overlap!(text_align);
    overlap!(uppercase);
    overlap!(shadow_len);
    overlap!(shadow_color);
    overlap!(glow);
    overlap!(diffuse);
    overlap!(visible);
    overlap!(cropleft);
    overlap!(cropright);
    overlap!(croptop);
    overlap!(cropbottom);
    overlap!(fadeleft);
    overlap!(faderight);
    overlap!(fadetop);
    overlap!(fadebottom);
    overlap!(mask_source);
    overlap!(mask_dest);
    overlap!(zoom);
    overlap!(zoom_x);
    overlap!(zoom_y);
    overlap!(zoom_z);
    overlap!(basezoom);
    overlap!(basezoom_x);
    overlap!(basezoom_y);
    overlap!(rot_x_deg);
    overlap!(rot_y_deg);
    overlap!(rot_z_deg);
    overlap!(skew_x);
    overlap!(skew_y);
    overlap!(blend);
    overlap!(vibrate);
    overlap!(effect_magnitude);
    overlap!(effect_mode);
    overlap!(effect_color1);
    overlap!(effect_color2);
    overlap!(effect_period);
    overlap!(effect_timing);
    overlap!(vert_spacing);
    overlap!(wrap_width_pixels);
    overlap!(max_width);
    overlap!(max_height);
    overlap!(max_w_pre_zoom);
    overlap!(max_h_pre_zoom);
    overlap!(texture_wrapping);
    overlap!(texcoord_offset);
    overlap!(custom_texture_rect);
    overlap!(texcoord_velocity);
    overlap!(size);
    overlap!(stretch_rect);
    false
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
fn song_lua_speedmod_from_setting(speed: ScrollSpeedSetting) -> SongLuaSpeedMod {
    match speed {
        ScrollSpeedSetting::XMod(value) => SongLuaSpeedMod::X(value),
        ScrollSpeedSetting::CMod(value) => SongLuaSpeedMod::C(value),
        ScrollSpeedSetting::MMod(value) => SongLuaSpeedMod::M(value),
    }
}

#[inline(always)]
fn song_lua_compile_play_style(play_style: profile_data::PlayStyle) -> SongLuaCompilePlayStyle {
    match play_style {
        profile_data::PlayStyle::Single => SongLuaCompilePlayStyle::Single,
        profile_data::PlayStyle::Versus => SongLuaCompilePlayStyle::Versus,
        profile_data::PlayStyle::Double => SongLuaCompilePlayStyle::Double,
    }
}

fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    profile: &profile_data::Profile,
    viewport: GameplayViewport,
    play_style: profile_data::PlayStyle,
    player_side: profile_data::PlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    gameplay_song_lua_compile_player_screen_x(
        num_players,
        player_index,
        viewport,
        song_lua_compile_play_style(play_style),
        profile_data::is_single_p2_side(play_style, player_side),
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
    let screen_width = viewport.width();
    let screen_height = viewport.height();
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
        profile_data::PlayStyle::Single => "single",
        profile_data::PlayStyle::Versus => "versus",
        profile_data::PlayStyle::Double => "double",
    }
    .to_string();
    context.global_offset_seconds = machine_global_offset_seconds;
    context.screen_width = screen_width;
    context.screen_height = screen_height;
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
        screen_x: if player < num_players {
            song_lua_compile_player_screen_x(
                num_players,
                player,
                &player_profiles[player],
                viewport,
                play_style,
                player_side,
                center_1player_notefield,
            )
        } else {
            viewport.center_x()
        },
        screen_y: viewport.center_y(),
    });
    context
}

fn build_song_lua_visual_layer_runtime(
    song: &SongData,
    start_beat: f32,
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    machine_global_offset_seconds: f32,
) -> Option<SongLuaVisualLayerRuntime> {
    let start_second = song_lua_time_to_second(
        SongLuaTimeUnit::Beat,
        start_beat,
        timing_player,
        machine_global_offset_seconds,
    );
    if !start_second.is_finite() {
        warn!(
            "Skipping song lua visual layer for '{}' at beat {:.3}: invalid start time",
            song.title, start_beat
        );
        return None;
    }

    let message_seconds = song_lua_message_seconds(
        &compiled.messages,
        timing_player,
        machine_global_offset_seconds,
    );
    let mut overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    let mut overlay_eases = build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        machine_global_offset_seconds,
        &overlay_events,
    );
    offset_song_lua_overlay_eases(&mut overlay_eases, start_second);
    let (overlay_eases, overlay_ease_ranges) =
        group_song_lua_overlay_eases(compiled.overlays.len(), overlay_eases);

    for events in &mut overlay_events {
        offset_song_lua_message_events(events, start_second);
    }

    let mut song_foreground_events = build_song_lua_actor_message_events_with_seconds(
        &compiled.messages,
        &message_seconds,
        &compiled.song_foreground.message_commands,
    );
    offset_song_lua_message_events(&mut song_foreground_events, start_second);

    Some(SongLuaVisualLayerRuntime {
        start_second,
        screen_width: compiled.screen_width,
        screen_height: compiled.screen_height,
        overlays: compiled.overlays.clone(),
        overlay_eases,
        overlay_ease_ranges,
        overlay_events,
        song_foreground: compiled.song_foreground.clone(),
        song_foreground_events,
    })
}

pub(super) fn build_song_lua_runtime_windows(
    song: &SongData,
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    num_players: usize,
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    machine_global_offset_seconds: f32,
    viewport: GameplayViewport,
    session: &GameplaySession,
    center_1player_notefield: bool,
    player_global_offset_shift_seconds: &[f32; MAX_PLAYERS],
    song_lua_data: GameplaySongLuaData,
) -> (
    [Vec<AttackMaskWindow>; MAX_PLAYERS],
    [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS],
    Vec<SongLuaOverlayActor>,
    Vec<SongLuaOverlayEaseWindowRuntime>,
    Vec<std::ops::Range<usize>>,
    Vec<Vec<SongLuaOverlayMessageRuntime>>,
    Vec<SongLuaVisualLayerRuntime>,
    Vec<SongLuaVisualLayerRuntime>,
    [SongLuaCapturedActor; MAX_PLAYERS],
    [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS],
    SongLuaCapturedActor,
    Vec<SongLuaOverlayMessageRuntime>,
    [bool; MAX_PLAYERS],
    [Vec<SongLuaNoteHideWindowRuntime>; MAX_PLAYERS],
    [Vec<SongLuaColumnOffsetWindowRuntime>; MAX_PLAYERS],
    f32,
    f32,
) {
    let mut constant_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut ease_windows: [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut overlays = Vec::new();
    let mut overlay_eases = Vec::new();
    let mut overlay_ease_ranges = Vec::new();
    let mut overlay_events = Vec::new();
    let mut background_visual_layers = Vec::new();
    let mut foreground_visual_layers = Vec::new();
    let play_style = session.play_style;
    let player_side = session.player_side;
    // Default player actor x/y must match StepMania's (SCREEN_CENTER_X, SCREEN_CENTER_Y)
    // origin so that, when no song.lua override is present, the gameplay player
    // transform path produces a zero translation. Without this, every non-lua song
    // would translate the playfield by (-playfield_center_x, +screen_center_y),
    // shoving it up and to the left.
    let default_player_actor = |player_index: usize| SongLuaCapturedActor {
        initial_state: SongLuaOverlayState {
            x: if player_index < num_players {
                song_lua_compile_player_screen_x(
                    num_players,
                    player_index,
                    &player_profiles[player_index],
                    viewport,
                    play_style,
                    player_side,
                    center_1player_notefield,
                )
            } else {
                viewport.center_x()
            },
            y: viewport.center_y(),
            ..SongLuaOverlayState::default()
        },
        message_commands: Vec::new(),
    };
    let mut player_actors: [SongLuaCapturedActor; MAX_PLAYERS] =
        std::array::from_fn(default_player_actor);
    let mut player_events: [Vec<SongLuaOverlayMessageRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut song_foreground = SongLuaCapturedActor::default();
    let mut song_foreground_events = Vec::new();
    let mut hidden_players = [false; MAX_PLAYERS];
    let mut note_hides: [Vec<SongLuaNoteHideWindowRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut column_offsets: [Vec<SongLuaColumnOffsetWindowRuntime>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let screen_width = viewport.width();
    let screen_height = viewport.height();

    if song_lua_data.primary.is_none()
        && song_lua_data.background_layers.is_empty()
        && song_lua_data.foreground_layers.is_empty()
    {
        return (
            constant_windows,
            ease_windows,
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
            screen_width,
            screen_height,
        );
    }

    let mut out_screen_width = screen_width;
    let mut out_screen_height = screen_height;

    if let Some(primary) = song_lua_data.primary.as_ref() {
        let compiled = &primary.compiled;
        let compile_ms = primary.compile_ms;
        let runtime_started = Instant::now();
        overlays = compiled.overlays.clone();
        let message_seconds = song_lua_message_seconds(
            &compiled.messages,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        );
        overlay_events =
            build_song_lua_overlay_message_events_with_seconds(&compiled, &message_seconds);
        let overlay_runtime_eases = build_song_lua_overlay_ease_windows_with_events(
            &compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
            &overlay_events,
        );
        (overlay_eases, overlay_ease_ranges) =
            group_song_lua_overlay_eases(compiled.overlays.len(), overlay_runtime_eases);
        player_actors[..compiled.player_actors.len()].clone_from_slice(&compiled.player_actors);
        for (player, actor) in compiled.player_actors.iter().enumerate() {
            player_events[player] = build_song_lua_actor_message_events_with_seconds(
                &compiled.messages,
                &message_seconds,
                &actor.message_commands,
            );
        }
        song_foreground = compiled.song_foreground.clone();
        song_foreground_events = build_song_lua_actor_message_events_with_seconds(
            &compiled.messages,
            &message_seconds,
            &compiled.song_foreground.message_commands,
        );
        hidden_players[..compiled.hidden_players.len()].copy_from_slice(&compiled.hidden_players);
        for hide in &compiled.note_hides {
            if hide.player < MAX_PLAYERS {
                note_hides[hide.player].push(SongLuaNoteHideWindowRuntime {
                    column: hide.column,
                    start_beat: hide.start_beat,
                    end_beat: hide.end_beat,
                });
            }
        }

        let mut unsupported_targets = 0usize;
        let mut total_constant = 0usize;
        let mut total_eases = 0usize;
        let mut total_column_offsets = 0usize;
        for player in 0..num_players {
            let player_global_offset_seconds =
                machine_global_offset_seconds + player_global_offset_shift_seconds[player];
            constant_windows[player] = build_song_lua_constant_windows_for_player(
                &compiled,
                timing_players[player].as_ref(),
                player,
                player_global_offset_seconds,
            );
            let (player_eases, player_unsupported_targets) = build_song_lua_ease_windows_for_player(
                &compiled,
                timing_players[player].as_ref(),
                player,
                player_global_offset_seconds,
                &constant_windows[player],
            );
            unsupported_targets += player_unsupported_targets;
            total_constant += constant_windows[player].len();
            total_eases += player_eases.len();
            ease_windows[player] = player_eases;
            column_offsets[player] = build_song_lua_column_offset_windows_for_player(
                &compiled,
                timing_players[player].as_ref(),
                player,
                player_global_offset_seconds,
            );
            total_column_offsets += column_offsets[player].len();
        }

        let runtime_ms = runtime_started.elapsed().as_secs_f64() * 1000.0;
        if total_constant > 0
            || total_eases > 0
            || total_column_offsets > 0
            || !overlays.is_empty()
            || !overlay_eases.is_empty()
            || !compiled.messages.is_empty()
            || !compiled.sound_paths.is_empty()
            || compiled.info.unsupported_perframes > 0
            || compiled.info.unsupported_function_eases > 0
            || compiled.info.unsupported_function_actions > 0
            || !compiled.info.skipped_message_command_captures.is_empty()
            || unsupported_targets > 0
        {
            info!(
                "Compiled gameplay lua for '{}' (constants={}, eases={}, column_offsets={}, overlay_eases={}, overlays={}, messages={}, sound_assets={}, unsupported_targets={}, function_eases={}, function_actions={}, perframes={}, skipped_message_commands={}, compile_ms={compile_ms:.3}, runtime_ms={runtime_ms:.3}).",
                song.title,
                total_constant,
                total_eases,
                total_column_offsets,
                overlay_eases.len(),
                overlays.len(),
                compiled.messages.len(),
                compiled.sound_paths.len(),
                unsupported_targets,
                compiled.info.unsupported_function_eases,
                compiled.info.unsupported_function_actions,
                compiled.info.unsupported_perframes,
                compiled.info.skipped_message_command_captures.len(),
            );
            log_song_lua_runtime_debug(
                song.title.as_str(),
                &compiled,
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
        if let Some(layer) = build_song_lua_visual_layer_runtime(
            song,
            layer_data.start_beat,
            compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        ) {
            background_visual_layers.push(layer);
        }
    }

    for layer_data in &song_lua_data.foreground_layers {
        let compiled = &layer_data.compiled;
        if let Some(layer) = build_song_lua_visual_layer_runtime(
            song,
            layer_data.start_beat,
            compiled,
            timing_players[0].as_ref(),
            machine_global_offset_seconds,
        ) {
            foreground_visual_layers.push(layer);
        }
    }

    (
        constant_windows,
        ease_windows,
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
    )
}

fn log_song_lua_runtime_debug(
    song_title: &str,
    compiled: &CompiledSongLua,
    overlay_eases: &[SongLuaOverlayEaseWindowRuntime],
    messages: &[SongLuaMessageEvent],
    hidden_players: &[bool; MAX_PLAYERS],
    total_constant: usize,
    total_eases: usize,
    total_column_offsets: usize,
    unsupported_targets: usize,
) {
    debug!(
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

    let mut message_counts = BTreeMap::<&str, usize>::new();
    for event in messages {
        *message_counts.entry(event.message.as_str()).or_default() += 1;
    }
    if !message_counts.is_empty() {
        debug!(
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
        debug!(
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
        debug!(
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
        debug!(
            "Song lua unsupported function action captures for '{}': {}",
            song_title,
            compiled
                .info
                .unsupported_function_action_captures
                .join(" | ")
        );
    }
    if !compiled.info.unsupported_function_ease_captures.is_empty() {
        debug!(
            "Song lua unsupported function ease captures for '{}': {}",
            song_title,
            compiled.info.unsupported_function_ease_captures.join(" | ")
        );
    }
    if !compiled.info.unsupported_perframe_captures.is_empty() {
        debug!(
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
        debug!(
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
        trace!(
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
        trace!(
            "Song lua message[{index}] for '{}': beat={:.3} message='{}' persists={}",
            song_title, event.beat, event.message, event.persists
        );
    }
}

#[inline(always)]
pub(super) fn player_changes_chart(
    chart: &GameplayChartData,
    profile: &profile_data::Profile,
) -> bool {
    player_chart_changes_for_options(
        super::chart_effects_from_profile(profile).has_note_masks(),
        super::gameplay_turn_option_from_profile(profile.turn_option),
        chart.chart_attacks.as_deref(),
        gameplay_attack_mode(profile.attack_mode),
    )
}

pub(super) fn apply_chart_attacks_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile_data::Profile; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
) {
    let players = std::array::from_fn(|player| ChartAttackTransformPlayer {
        chart_attacks: gameplay_charts[player].chart_attacks.as_deref(),
        attack_mode: gameplay_attack_mode(player_profiles[player].attack_mode),
        timing_player: timing_players[player].as_ref(),
    });
    apply_chart_attack_transforms_to_notes(
        notes,
        note_ranges,
        cols_per_player,
        num_players,
        &players,
        base_seed,
        song_length_seconds,
    );
}

#[inline(always)]
pub(super) fn base_appearance_effects(profile: &profile_data::Profile) -> AppearanceEffects {
    AppearanceEffects::from_mask_bits(profile.appearance_effects_active_mask.bits())
}

#[inline(always)]
pub(super) fn begin_outro_attack_clear(state: &mut State) {
    begin_outro_attack_visual_clear(
        &mut state.attacks_cleared_for_outro,
        state.num_players,
        &state.active_attack_visual,
        &mut state.outro_attack_visual,
    );
}

#[inline(always)]
fn base_visual_effects(profile: &profile_data::Profile) -> VisualEffects {
    VisualEffects::from_mask_bits(profile.visual_effects_active_mask.bits())
}

#[inline(always)]
fn store_song_lua_player_transforms(
    state: &mut State,
    player: usize,
    values: SongLuaPlayerTransformValues,
) {
    let SongLuaPlayerTransform {
        x,
        y,
        z,
        rotation_x,
        rotation_z,
        rotation_y,
        skew_x,
        skew_y,
        zoom_x,
        zoom_y,
        zoom_z,
        confusion_y_offset,
    } = values.resolve();
    state.song_lua_player_x[player] = x;
    state.song_lua_player_y[player] = y;
    state.song_lua_player_z[player] = z;
    state.song_lua_player_rotation_x[player] = rotation_x;
    state.song_lua_player_rotation_z[player] = rotation_z;
    state.song_lua_player_rotation_y[player] = rotation_y;
    state.song_lua_player_skew_x[player] = skew_x;
    state.song_lua_player_skew_y[player] = skew_y;
    state.song_lua_player_zoom_x[player] = zoom_x;
    state.song_lua_player_zoom_y[player] = zoom_y;
    state.song_lua_player_zoom_z[player] = zoom_z;
    state.song_lua_player_confusion_y_offset[player] = confusion_y_offset;
}

pub(super) fn refresh_active_attack_masks(state: &mut State, delta_time: f32) {
    for player in 0..state.num_players {
        let now = state.current_music_time_visible[player];
        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now,
                delta_time,
                attacks_cleared_for_outro: state.attacks_cleared_for_outro,
                base_appearance: base_appearance_effects(&state.player_profiles[player]),
                base_visual: base_visual_effects(&state.player_profiles[player]),
                base_scroll: scroll_effects_from_option(
                    state.player_profiles[player].scroll_option,
                ),
                base_mini_percent: state.player_profiles[player].mini_percent as f32,
                attack_windows: &state.attack_mask_windows[player],
                song_lua_ease_windows: &state.song_lua_ease_windows[player],
            },
            ActiveAttackRefreshState {
                attack_current_appearance: state.attack_current_appearance[player],
                active_attack_visual: state.active_attack_visual[player],
                active_attack_visibility: state.active_attack_visibility[player],
                active_attack_scroll: state.active_attack_scroll[player],
                active_attack_mini_percent: state.active_attack_mini_percent[player],
                outro_attack_visual: state.outro_attack_visual[player],
            },
        );
        state.attack_target_appearance[player] = output.attack_target_appearance;
        state.attack_speed_appearance[player] = output.attack_speed_appearance;
        state.attack_current_appearance[player] = output.attack_current_appearance;
        state.outro_attack_visual[player] = output.outro_attack_visual;
        state.active_attack_clear_all[player] = output.active_attack_clear_all;
        state.active_attack_chart[player] = output.active_attack_chart;
        state.active_attack_accel[player] = output.active_attack_accel;
        state.active_attack_visual[player] = output.active_attack_visual;
        state.active_attack_appearance[player] = output.active_attack_appearance;
        state.active_attack_visibility[player] = output.active_attack_visibility;
        state.active_attack_scroll[player] = output.active_attack_scroll;
        state.active_attack_perspective[player] = output.active_attack_perspective;
        state.active_attack_scroll_speed[player] = output.active_attack_scroll_speed;
        state.active_attack_mini_percent[player] = output.active_attack_mini_percent;
        store_song_lua_player_transforms(state, player, output.player_transform);
    }
}

#[inline(always)]
fn player_attack_base_cleared(state: &State, player_idx: usize) -> bool {
    player_idx < state.num_players && state.active_attack_clear_all[player_idx]
}

#[inline(always)]
pub fn effective_accel_effects_for_player(state: &State, player_idx: usize) -> AccelEffects {
    if player_idx >= state.num_players {
        return AccelEffects::default();
    }
    effective_attack_accel_effects(
        player_attack_base_cleared(state, player_idx),
        state.player_profiles[player_idx]
            .accel_effects_active_mask
            .bits(),
        state.active_attack_accel[player_idx],
    )
}

#[inline(always)]
pub fn effective_visual_effects_for_player(state: &State, player_idx: usize) -> VisualEffects {
    if player_idx >= state.num_players {
        return VisualEffects::default();
    }
    effective_attack_visual_effects(
        player_attack_base_cleared(state, player_idx),
        state.player_profiles[player_idx]
            .visual_effects_active_mask
            .bits(),
        state.active_attack_visual[player_idx],
    )
}

#[inline(always)]
pub fn effective_appearance_effects_for_player(
    state: &State,
    player_idx: usize,
) -> AppearanceEffects {
    if player_idx >= state.num_players {
        return AppearanceEffects::default();
    }
    state.active_attack_appearance[player_idx]
}

#[inline(always)]
pub fn effective_visibility_effects_for_player(
    state: &State,
    player_idx: usize,
) -> VisibilityEffects {
    if player_idx >= state.num_players {
        return VisibilityEffects::default();
    }
    effective_attack_visibility_effects(state.active_attack_visibility[player_idx])
}

#[inline(always)]
pub fn active_chart_attack_effects_for_player(
    state: &State,
    player_idx: usize,
) -> ChartAttackEffects {
    if player_idx >= state.num_players {
        return ChartAttackEffects::default();
    }
    state.active_attack_chart[player_idx]
}

#[inline(always)]
pub fn effective_scroll_effects_for_player(state: &State, player_idx: usize) -> ScrollEffects {
    if player_idx >= state.num_players {
        return ScrollEffects::default();
    }
    effective_attack_scroll_effects(
        player_attack_base_cleared(state, player_idx),
        scroll_effects_from_option(state.player_profiles[player_idx].scroll_option),
        state.active_attack_scroll[player_idx],
    )
}

#[inline(always)]
pub fn effective_perspective_effects_for_player(
    state: &State,
    player_idx: usize,
) -> PerspectiveEffects {
    if player_idx >= state.num_players {
        return PerspectiveEffects::default();
    }
    effective_attack_perspective_effects(
        player_attack_base_cleared(state, player_idx),
        perspective_effects_from_profile(state.player_profiles[player_idx].perspective),
        state.active_attack_perspective[player_idx],
    )
}

#[inline(always)]
pub(super) fn effective_visual_mask_for_player(state: &State, player_idx: usize) -> u16 {
    effective_visual_effects_for_player(state, player_idx).to_mask_bits()
}

#[inline(always)]
pub fn effective_mini_percent_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return 0.0;
    }
    effective_mini_percent(
        state.active_attack_mini_percent[player_idx],
        state.player_profiles[player_idx].mini_percent as f32,
        player_attack_base_cleared(state, player_idx),
    )
}

/// Multiplier applied to the noteskin's per-column lateral offsets to
/// realise the Spacing player option (zmod parity, proportional model).
/// `1.0 + spacing_percent / 100`.
#[inline(always)]
pub fn effective_spacing_multiplier_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return 1.0;
    }
    spacing_multiplier_for_percent(state.player_profiles[player_idx].spacing_percent)
}

#[inline(always)]
pub fn effective_scroll_speed_for_player(state: &State, player_idx: usize) -> ScrollSpeedSetting {
    if player_idx >= state.num_players {
        return ScrollSpeedSetting::default();
    }
    effective_attack_scroll_speed(
        player_attack_base_cleared(state, player_idx),
        state.active_attack_scroll_speed[player_idx],
        state.scroll_speed[player_idx],
    )
}
