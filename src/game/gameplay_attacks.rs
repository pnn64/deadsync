use crate::game::parsing::song_lua::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaColumnOffsetWindow, SongLuaEaseTarget,
    SongLuaEaseWindow, SongLuaMessageEvent, SongLuaModWindow, SongLuaOverlayEase,
    SongLuaOverlayMessageCommand, SongLuaOverlayState, SongLuaSpanMode, SongLuaTimeUnit,
};
#[cfg(test)]
use crate::game::parsing::song_lua::{
    SongLuaCompileContext, SongLuaDifficulty, SongLuaPlayerContext, SongLuaSpeedMod,
};
#[cfg(test)]
use deadsync_chart::ChartData;
use deadsync_chart::SongData;
use deadsync_gameplay::{
    AttackMaskWindow, SongLuaColumnOffsetWindowLike, SongLuaEaseMaskTarget, SongLuaEaseMaskWindow,
    SongLuaEaseWindowLike, SongLuaModWindowLike, SongLuaOverlayDeltaOverlap,
    SongLuaOverlayEaseWindowLike, SongLuaRuntimeEaseTarget, SongLuaRuntimeEaseTargetLike,
    SongLuaRuntimeSpanMode, SongLuaRuntimeSpanModeLike, SongLuaRuntimeTimeUnit,
    SongLuaRuntimeTimeUnitLike,
    apply_song_lua_player_actor_overrides,
    build_song_lua_hidden_players, build_song_lua_message_seconds,
    build_song_lua_note_hide_windows_for_players, build_song_lua_overlay_ease_window_for,
    build_song_lua_player_message_events, build_song_lua_player_runtime_windows,
    build_song_lua_runtime_visuals,
    build_song_lua_visual_layer_runtime as gameplay_build_song_lua_visual_layer_runtime,
    song_lua_hides_note_visual,
    effective_player_global_offset_seconds as gameplay_effective_player_global_offset_seconds,
    group_song_lua_overlay_eases,
    song_lua_compile_player_screen_x_like as gameplay_song_lua_compile_player_screen_x_like,
    song_lua_overlay_ease_cutoff_second as gameplay_song_lua_overlay_ease_cutoff_second,
    song_lua_time_to_second_like,
};
#[cfg(test)]
use deadsync_gameplay::build_song_lua_ease_windows_for_player as gameplay_build_song_lua_ease_windows_for_player;
use deadsync_gameplay::{
    SongLuaColumnOffsetWindowRuntime, SongLuaNoteHideWindowRuntime, SongLuaOverlayMessageRuntime,
};
use deadsync_profile as profile_data;
#[cfg(test)]
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingData;
use log::{debug, info, trace, warn};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use super::{
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplaySession, GameplaySongLuaData,
    GameplayViewport, MAX_PLAYERS, SongLuaOverlayEaseWindowRuntime, SongLuaRuntimeVisuals,
    SongLuaVisualLayerRuntime, State, completed_row_tap_feedback_plan, gameplay_is_single_p2_side,
    row_entry_for_cached_row, trigger_receptor_score_pulse, trigger_tap_judgment_explosion,
};

#[inline(always)]
pub(super) fn trigger_completed_row_tap_explosions(
    state: &mut State,
    player: usize,
    row_index: usize,
) {
    let Some(plan) = ({
        let Some(row_entry) = row_entry_for_cached_row(
            &state.chart_runtime.row_entries,
            &state.chart_runtime.row_indices.row_map_cache[player],
            row_index,
        ) else {
            return;
        };
        completed_row_tap_feedback_plan(&state.chart_runtime.notes, row_entry)
    }) else {
        return;
    };

    for &note_index in &plan.note_indices[..plan.note_count] {
        let note = &state.chart_runtime.notes[note_index];
        let column = note.column;
        if song_lua_hides_note_visual(state, player, column, note.beat) {
            if let Some(window_key) = plan.receptor_window {
                trigger_receptor_score_pulse(state, column, window_key);
            }
            continue;
        }
        trigger_tap_judgment_explosion(state, player, column, &plan.judgment);
    }
}

/// Bails on non-4/8-panel layouts because `rssp` parity only models those.
#[cfg(test)]
pub(super) fn test_song_lua_double_context(
    root: &std::path::Path,
    title: &str,
) -> SongLuaCompileContext {
    let mut context = SongLuaCompileContext::new(root, title);
    context.style_name = "double".to_string();
    context.players = [
        SongLuaPlayerContext {
            enabled: true,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
        SongLuaPlayerContext {
            enabled: false,
            difficulty: SongLuaDifficulty::Challenge,
            speedmod: SongLuaSpeedMod::X(2.0),
            ..SongLuaPlayerContext::default()
        },
    ];
    context
}

#[cfg(test)]
fn song_lua_difficulty_from_chart(difficulty: &str) -> SongLuaDifficulty {
    if difficulty.eq_ignore_ascii_case("beginner") {
        SongLuaDifficulty::Beginner
    } else if difficulty.eq_ignore_ascii_case("easy") || difficulty.eq_ignore_ascii_case("basic") {
        SongLuaDifficulty::Easy
    } else if difficulty.eq_ignore_ascii_case("medium") || difficulty.eq_ignore_ascii_case("standard") {
        SongLuaDifficulty::Medium
    } else if difficulty.eq_ignore_ascii_case("hard") || difficulty.eq_ignore_ascii_case("difficult") {
        SongLuaDifficulty::Hard
    } else if difficulty.eq_ignore_ascii_case("edit") {
        SongLuaDifficulty::Edit
    } else {
        SongLuaDifficulty::Challenge
    }
}

#[cfg(test)]
fn song_lua_speedmod_from_setting(speed: ScrollSpeedSetting) -> SongLuaSpeedMod {
    match speed {
        ScrollSpeedSetting::XMod(value) => SongLuaSpeedMod::X(value),
        ScrollSpeedSetting::CMod(value) => SongLuaSpeedMod::C(value),
        ScrollSpeedSetting::MMod(value) => SongLuaSpeedMod::M(value),
    }
}

#[cfg(test)]
pub(super) fn song_lua_compile_context(
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

impl SongLuaRuntimeTimeUnitLike for SongLuaTimeUnit {
    #[inline(always)]
    fn as_runtime_time_unit(self) -> SongLuaRuntimeTimeUnit {
        match self {
            SongLuaTimeUnit::Beat => SongLuaRuntimeTimeUnit::Beat,
            SongLuaTimeUnit::Second => SongLuaRuntimeTimeUnit::Second,
        }
    }
}

impl SongLuaRuntimeSpanModeLike for SongLuaSpanMode {
    #[inline(always)]
    fn as_runtime_span_mode(self) -> SongLuaRuntimeSpanMode {
        match self {
            SongLuaSpanMode::Len => SongLuaRuntimeSpanMode::Len,
            SongLuaSpanMode::End => SongLuaRuntimeSpanMode::End,
        }
    }
}

impl SongLuaRuntimeEaseTargetLike for SongLuaEaseTarget {
    fn as_runtime_ease_target(&self) -> SongLuaRuntimeEaseTarget<'_> {
        match self {
            SongLuaEaseTarget::Mod(target_name) => SongLuaRuntimeEaseTarget::Mod(target_name.as_str()),
            SongLuaEaseTarget::PlayerX => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerX)
            }
            SongLuaEaseTarget::PlayerY => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerY)
            }
            SongLuaEaseTarget::PlayerZ => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerZ)
            }
            SongLuaEaseTarget::PlayerRotationX => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerRotationX)
            }
            SongLuaEaseTarget::PlayerRotationZ => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerRotationZ)
            }
            SongLuaEaseTarget::PlayerRotationY => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerRotationY)
            }
            SongLuaEaseTarget::PlayerSkewX => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerSkewX)
            }
            SongLuaEaseTarget::PlayerSkewY => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerSkewY)
            }
            SongLuaEaseTarget::PlayerZoom => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerZoom)
            }
            SongLuaEaseTarget::PlayerZoomX => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerZoomX)
            }
            SongLuaEaseTarget::PlayerZoomY => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerZoomY)
            }
            SongLuaEaseTarget::PlayerZoomZ => {
                SongLuaRuntimeEaseTarget::Player(SongLuaEaseMaskTarget::PlayerZoomZ)
            }
            SongLuaEaseTarget::Function => SongLuaRuntimeEaseTarget::Function,
        }
    }
}

impl SongLuaOverlayDeltaOverlap for crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
    fn overlaps_song_lua_delta(&self, other: &Self) -> bool {
        song_lua_overlay_delta_overlaps(self, other)
    }
}

impl SongLuaEaseWindowLike for SongLuaEaseWindow {
    type Target = SongLuaEaseTarget;

    fn player(&self) -> Option<u8> {
        self.player
    }

    fn unit(&self) -> SongLuaRuntimeTimeUnit {
        self.unit.as_runtime_time_unit()
    }

    fn start(&self) -> f32 {
        self.start
    }

    fn limit(&self) -> f32 {
        self.limit
    }

    fn span_mode(&self) -> SongLuaRuntimeSpanMode {
        self.span_mode.as_runtime_span_mode()
    }

    fn target(&self) -> &Self::Target {
        &self.target
    }

    fn from(&self) -> f32 {
        self.from
    }

    fn to(&self) -> f32 {
        self.to
    }

    fn easing(&self) -> Option<&str> {
        self.easing.as_deref()
    }

    fn sustain(&self) -> Option<f32> {
        self.sustain
    }

    fn opt1(&self) -> Option<f32> {
        self.opt1
    }

    fn opt2(&self) -> Option<f32> {
        self.opt2
    }
}

impl SongLuaModWindowLike for SongLuaModWindow {
    fn player(&self) -> Option<u8> {
        self.player
    }

    fn unit(&self) -> SongLuaRuntimeTimeUnit {
        self.unit.as_runtime_time_unit()
    }

    fn start(&self) -> f32 {
        self.start
    }

    fn limit(&self) -> f32 {
        self.limit
    }

    fn span_mode(&self) -> SongLuaRuntimeSpanMode {
        self.span_mode.as_runtime_span_mode()
    }

    fn mods(&self) -> &str {
        &self.mods
    }
}

impl SongLuaColumnOffsetWindowLike for SongLuaColumnOffsetWindow {
    fn player(&self) -> usize {
        self.player
    }

    fn unit(&self) -> SongLuaRuntimeTimeUnit {
        self.unit.as_runtime_time_unit()
    }

    fn start(&self) -> f32 {
        self.start
    }

    fn limit(&self) -> f32 {
        self.limit
    }

    fn span_mode(&self) -> SongLuaRuntimeSpanMode {
        self.span_mode.as_runtime_span_mode()
    }

    fn column(&self) -> usize {
        self.column
    }

    fn from_y(&self) -> f32 {
        self.from_y
    }

    fn to_y(&self) -> f32 {
        self.to_y
    }

    fn easing(&self) -> Option<&str> {
        self.easing.as_deref()
    }

    fn sustain(&self) -> Option<f32> {
        self.sustain
    }

    fn opt1(&self) -> Option<f32> {
        self.opt1
    }

    fn opt2(&self) -> Option<f32> {
        self.opt2
    }
}

impl SongLuaOverlayEaseWindowLike<crate::game::parsing::song_lua::SongLuaOverlayStateDelta>
    for SongLuaOverlayEase
{
    fn overlay_index(&self) -> usize {
        self.overlay_index
    }

    fn unit(&self) -> SongLuaRuntimeTimeUnit {
        self.unit.as_runtime_time_unit()
    }

    fn start(&self) -> f32 {
        self.start
    }

    fn limit(&self) -> f32 {
        self.limit
    }

    fn span_mode(&self) -> SongLuaRuntimeSpanMode {
        self.span_mode.as_runtime_span_mode()
    }

    fn sustain(&self) -> Option<f32> {
        self.sustain
    }

    fn from(&self) -> &crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
        &self.from
    }

    fn to(&self) -> &crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
        &self.to
    }

    fn easing(&self) -> Option<&str> {
        self.easing.as_deref()
    }

    fn opt1(&self) -> Option<f32> {
        self.opt1
    }

    fn opt2(&self) -> Option<f32> {
        self.opt2
    }
}

#[cfg(test)]
pub(super) fn build_song_lua_ease_windows_for_player(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    player: usize,
    global_offset_seconds: f32,
    constant_windows: &[AttackMaskWindow],
) -> (Vec<SongLuaEaseMaskWindow>, usize) {
    gameplay_build_song_lua_ease_windows_for_player(
        &compiled.eases,
        timing_player,
        player,
        global_offset_seconds,
        constant_windows,
        |window| {
            if let SongLuaEaseTarget::Mod(target_name) = &window.target {
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
        },
    )
}

#[cfg(test)]
pub(super) fn build_song_lua_overlay_ease_windows(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let message_seconds = build_song_lua_message_seconds(
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

fn build_song_lua_overlay_ease_windows_with_events(
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
) -> Vec<SongLuaOverlayEaseWindowRuntime> {
    let mut out = Vec::new();
    for ease in &compiled.overlay_eases {
        if let Some(window) = build_song_lua_overlay_ease_window_for(
            ease,
            timing_player,
            global_offset_seconds,
            |start_second| {
                song_lua_overlay_ease_cutoff_second(compiled, ease, overlay_events, start_second)
            },
        ) {
            out.push(window);
        }
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

fn song_lua_overlay_ease_cutoff_second(
    compiled: &CompiledSongLua,
    ease: &SongLuaOverlayEase,
    overlay_events: &[Vec<SongLuaOverlayMessageRuntime>],
    start_second: f32,
) -> Option<f32> {
    let overlay = compiled.overlays.get(ease.overlay_index)?;
    let events = overlay_events.get(ease.overlay_index)?;
    let blocks = events
        .iter()
        .filter_map(|event| {
            let command = overlay.message_commands.get(event.command_index)?;
            Some((event.event_second, command))
        })
        .flat_map(|(event_second, command)| {
            command
                .blocks
                .iter()
                .map(move |block| (event_second, block.start, &block.delta))
        });
    gameplay_song_lua_overlay_ease_cutoff_second(start_second, &ease.from, &ease.to, blocks)
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

fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    profile: &profile_data::Profile,
    viewport: GameplayViewport,
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
    center_1player_notefield: bool,
) -> f32 {
    gameplay_song_lua_compile_player_screen_x_like(
        num_players,
        player_index,
        viewport,
        play_style,
        gameplay_is_single_p2_side(play_style, player_side),
        profile.note_field_offset_x as f32,
        center_1player_notefield,
    )
}

fn build_song_lua_visual_layer_runtime(
    song: &SongData,
    start_beat: f32,
    compiled: &CompiledSongLua,
    timing_player: &TimingData,
    machine_global_offset_seconds: f32,
) -> Option<SongLuaVisualLayerRuntime> {
    let start_second = song_lua_time_to_second_like(
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

    let message_seconds = build_song_lua_message_seconds(
        compiled.messages.iter().map(|message| message.beat),
        timing_player,
        machine_global_offset_seconds,
    );
    let overlay_events =
        build_song_lua_overlay_message_events_with_seconds(compiled, &message_seconds);
    let overlay_eases = build_song_lua_overlay_ease_windows_with_events(
        compiled,
        timing_player,
        machine_global_offset_seconds,
        &overlay_events,
    );

    let song_foreground_events = build_song_lua_actor_message_events_with_seconds(
        &compiled.messages,
        &message_seconds,
        &compiled.song_foreground.message_commands,
    );

    Some(gameplay_build_song_lua_visual_layer_runtime(
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
    SongLuaRuntimeVisuals,
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
            build_song_lua_runtime_visuals(
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
            ),
        );
    }

    let mut out_screen_width = screen_width;
    let mut out_screen_height = screen_height;

    if let Some(primary) = song_lua_data.primary.as_ref() {
        let compiled = &primary.compiled;
        let compile_ms = primary.compile_ms;
        let runtime_started = Instant::now();
        overlays = compiled.overlays.clone();
        let message_seconds = build_song_lua_message_seconds(
            compiled.messages.iter().map(|message| message.beat),
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
        apply_song_lua_player_actor_overrides(&mut player_actors, &compiled.player_actors);
        player_events = build_song_lua_player_message_events(&compiled.player_actors, |actor| {
            build_song_lua_actor_message_events_with_seconds(
                &compiled.messages,
                &message_seconds,
                &actor.message_commands,
            )
        });
        song_foreground = compiled.song_foreground.clone();
        song_foreground_events = build_song_lua_actor_message_events_with_seconds(
            &compiled.messages,
            &message_seconds,
            &compiled.song_foreground.message_commands,
        );
        hidden_players = build_song_lua_hidden_players(&compiled.hidden_players);
        note_hides = build_song_lua_note_hide_windows_for_players(
            compiled
                .note_hides
                .iter()
                .map(|hide| (hide.player, hide.column, hide.start_beat, hide.end_beat)),
        );

        let mut unsupported_targets = 0usize;
        let mut total_constant = 0usize;
        let mut total_eases = 0usize;
        let mut total_column_offsets = 0usize;
        for player in 0..num_players {
            let player_global_offset_seconds = gameplay_effective_player_global_offset_seconds(
                machine_global_offset_seconds,
                player_global_offset_shift_seconds,
                player,
            );
            let player_windows = build_song_lua_player_runtime_windows(
                &compiled.time_mods,
                &compiled.beat_mods,
                &compiled.eases,
                &compiled.column_offsets,
                timing_players[player].as_ref(),
                player,
                player_global_offset_seconds,
                |window| {
                    if let SongLuaEaseTarget::Mod(target_name) = &window.target {
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
                },
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
        build_song_lua_runtime_visuals(
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
