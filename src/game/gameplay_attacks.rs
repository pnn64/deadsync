use crate::game::parsing::song_lua::{
    CompiledSongLua, SongLuaCapturedActor, SongLuaEaseTarget, SongLuaMessageEvent,
    SongLuaModWindow, SongLuaOverlayEase, SongLuaOverlayMessageCommand, SongLuaOverlayState,
    SongLuaSpanMode, SongLuaTimeUnit,
};
use deadsync_chart::SongData;
use deadsync_gameplay::{
    AttackMaskWindow, SongLuaCompilePlayStyle, SongLuaEaseMaskTarget, SongLuaEaseMaskWindow,
    SongLuaRuntimeEaseAppend, SongLuaRuntimeEaseTarget, SongLuaRuntimeSpanMode,
    SongLuaRuntimeTimeUnit, append_song_lua_runtime_ease_window,
    build_song_lua_column_offset_window_runtime, build_song_lua_constant_attack_mask_window,
    build_song_lua_message_command_indices, build_song_lua_note_hide_window_runtime,
    build_song_lua_overlay_ease_window_runtime, build_song_lua_overlay_message_runtime,
    effective_player_global_offset_seconds as gameplay_effective_player_global_offset_seconds,
    group_song_lua_overlay_eases, offset_song_lua_message_events, offset_song_lua_overlay_eases,
    song_lua_compile_player_screen_x as gameplay_song_lua_compile_player_screen_x,
    song_lua_extend_column_offset_tails, song_lua_extend_ease_tails,
    song_lua_message_command_index, song_lua_message_second,
    song_lua_sustain_end_second as gameplay_song_lua_sustain_end_second,
    song_lua_target_matches_player, song_lua_time_to_second as gameplay_song_lua_time_to_second,
    song_lua_window_seconds as gameplay_song_lua_window_seconds,
};
use deadsync_gameplay::{
    SongLuaColumnOffsetWindowRuntime, SongLuaNoteHideWindowRuntime, SongLuaOverlayMessageRuntime,
};
use deadsync_profile as profile_data;
use deadsync_rules::timing::TimingData;
use log::{debug, info, trace, warn};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use super::{
    GameplayInputPlayStyle, GameplayInputPlayerSide, GameplaySession, GameplaySongLuaData,
    GameplayViewport, MAX_PLAYERS, SongLuaOverlayEaseWindowRuntime, SongLuaRuntimeVisuals,
    SongLuaVisualLayerRuntime, gameplay_is_single_p2_side,
};

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
    build_song_lua_constant_attack_mask_window(start_second, end_second, &window.mods)
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

fn song_lua_runtime_ease_target(target: &SongLuaEaseTarget) -> SongLuaRuntimeEaseTarget<'_> {
    match target {
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
        let target = song_lua_runtime_ease_target(&window.target);
        if append_song_lua_runtime_ease_window(
            &mut out,
            start_second,
            end_second,
            sustain_end_second,
            target,
            window.from,
            window.to,
            window.easing.as_deref(),
            window.opt1,
            window.opt2,
        ) == SongLuaRuntimeEaseAppend::Unsupported
        {
            unsupported_targets += 1;
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
        out.push(build_song_lua_column_offset_window_runtime(
            window.column,
            start_second,
            end_second,
            sustain_end_second,
            window.from_y,
            window.to_y,
            window.easing.as_deref(),
            window.opt1,
            window.opt2,
        ));
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
        out.push(build_song_lua_overlay_ease_window_runtime(
            ease.overlay_index,
            start_second,
            end_second,
            sustain_end_second,
            cutoff_second,
            ease.from,
            ease.to,
            ease.easing.as_deref(),
            ease.opt1,
            ease.opt2,
        ));
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
    let command_indices = build_song_lua_message_command_indices(
        commands
            .iter()
            .enumerate()
            .map(|(idx, command)| (idx, command.message.as_str())),
    );
    let mut out = Vec::new();
    for (idx, message) in messages.iter().enumerate() {
        let Some(event_second) = message_seconds.get(idx).copied().flatten() else {
            continue;
        };
        let Some(command_index) =
            song_lua_message_command_index(&command_indices, &message.message)
        else {
            continue;
        };
        out.push(build_song_lua_overlay_message_runtime(
            event_second,
            command_index,
        ));
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
fn song_lua_compile_play_style(play_style: GameplayInputPlayStyle) -> SongLuaCompilePlayStyle {
    match play_style {
        GameplayInputPlayStyle::Single => SongLuaCompilePlayStyle::Single,
        GameplayInputPlayStyle::Versus => SongLuaCompilePlayStyle::Versus,
        GameplayInputPlayStyle::Double => SongLuaCompilePlayStyle::Double,
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
            SongLuaRuntimeVisuals {
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
            },
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
                note_hides[hide.player].push(build_song_lua_note_hide_window_runtime(
                    hide.column,
                    hide.start_beat,
                    hide.end_beat,
                ));
            }
        }

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
        SongLuaRuntimeVisuals {
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
            screen_width: out_screen_width,
            screen_height: out_screen_height,
        },
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
