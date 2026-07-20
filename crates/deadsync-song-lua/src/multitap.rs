use deadlib_present::anim::{EffectClock, EffectMode};
use mlua::{Lua, Table, Value};

use crate::{
    LUA_PLAYERS, SONG_LUA_DOUBLE_NOTE_COLUMNS, SONG_LUA_NOTE_COLUMNS, SongLuaCompileContext,
    SongLuaMessageEvent, SongLuaNoteskinResolver, SongLuaOverlayActor, SongLuaOverlayCommandBlock,
    SongLuaOverlayCompileActor, SongLuaOverlayEase, SongLuaOverlayMessageCommand,
    SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaSpanMode, SongLuaSpeedMod,
    SongLuaTimeUnit, THEME_RECEPTOR_Y_STD, named_overlay_indices_by_name,
    overlay_delta_intersection, overlay_descendants_by_parent, read_f32, song_lua_style_column_x,
};

pub const MULTITAP_PREVISIBLE_BEATS: f32 = 8.0;
pub const MULTITAP_SAMPLE_STEP: f32 = 0.125;
pub const MULTITAP_HIDE_EPSILON_BEATS: f32 = 0.0001;

const MULTITAP_BASE_BOUNCE: f32 = 1.5;
const MULTITAP_ELASTICITY: f32 = 1.05;
const MULTITAP_SQUISHY: f32 = 0.2;
const MULTITAP_LANE_ROTATION: [f32; SONG_LUA_DOUBLE_NOTE_COLUMNS] =
    [90.0, 0.0, 180.0, 270.0, 90.0, 0.0, 180.0, 270.0];

#[derive(Clone)]
pub struct MultitapDesc {
    pub lane: usize,
    pub taps: Vec<f32>,
    pub peak: Option<f32>,
}

#[derive(Clone, Copy)]
pub struct MultitapPhase {
    pub pos: f32,
    pub squish: f32,
    pub lin: f32,
    pub qtc: u8,
    pub visible: bool,
}

pub fn read_multitap_descs(
    lua: &Lua,
    context: &SongLuaCompileContext,
) -> Result<Option<Vec<MultitapDesc>>, String> {
    let globals = lua.globals();
    let Some(multitaps) = globals
        .get::<Option<Table>>("multitaps")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let difficulty = context.players[0]
        .difficulty
        .sm_name()
        .trim_start_matches("Difficulty_");
    let table = multitaps
        .get::<Option<Table>>(difficulty)
        .map_err(|err| err.to_string())?
        .or_else(|| multitaps.get::<Option<Table>>("Challenge").ok().flatten());
    let Some(table) = table else {
        return Ok(None);
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(lane) = entry
            .get::<Option<i64>>("lane")
            .map_err(|err| err.to_string())?
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| (1..=SONG_LUA_DOUBLE_NOTE_COLUMNS).contains(value))
        else {
            continue;
        };
        let Some(taps_table) = entry
            .get::<Option<Table>>("taps")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        let mut taps = Vec::new();
        for tap in taps_table.sequence_values::<Value>() {
            if let Some(tap) = read_f32(tap.map_err(|err| err.to_string())?)
                && tap.is_finite()
            {
                taps.push(tap);
            }
        }
        if taps.is_empty() {
            continue;
        }
        taps.sort_by(|left, right| left.total_cmp(right));
        let peak = entry
            .get::<Value>("peak")
            .map_err(|err| err.to_string())
            .ok()
            .and_then(read_f32)
            .filter(|value| value.is_finite());
        out.push(MultitapDesc { lane, taps, peak });
    }
    Ok(Some(out))
}

pub fn push_multitap_arrow_sample(
    samples: &mut Vec<(f32, SongLuaOverlayState)>,
    beat: f32,
    baseline: SongLuaOverlayState,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    lane: usize,
    phase: MultitapPhase,
) {
    if phase.visible {
        samples.push((
            beat,
            multitap_arrow_state(baseline, noteskin_resolver, noteskin, lane, phase),
        ));
    }
}

pub fn push_overlay_sample_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    samples: &[(f32, SongLuaOverlayState)],
) {
    if let Some((start, state)) = samples.first().copied() {
        push_overlay_sample_instant_state(out, overlay_index, start, baseline, state);
    }
    for window in samples.windows(2) {
        let [(start, from), (end, to)] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        match (from.visible, to.visible) {
            (true, true) => {
                push_overlay_sample_linear_ease(out, overlay_index, baseline, start, end, from, to)
            }
            (false, true) => {
                push_overlay_sample_instant_state(out, overlay_index, end, baseline, to)
            }
            (true, false) => push_overlay_sample_instant_visible(out, overlay_index, end, false),
            (false, false) => {}
        }
    }
}

pub fn apply_multitap_field_state(
    state: &mut SongLuaOverlayState,
    context: &SongLuaCompileContext,
    player: usize,
) {
    state.visible = true;
    state.x = context.players[player].screen_x;
    state.y = context.players[player].screen_y;
    state.z = 0.0;
    state.zoom_x = 1.0;
    state.zoom_y = 1.0;
    state.zoom_z = 1.0;
}

pub fn push_multitap_explosion_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    descs: &[MultitapDesc],
    lane: usize,
) {
    let mut ranges = descs
        .iter()
        .filter(|desc| desc.lane == lane)
        .map(|desc| {
            (
                desc.taps[0] - MULTITAP_PREVISIBLE_BEATS,
                desc.taps[desc.taps.len() - 1] + MULTITAP_HIDE_EPSILON_BEATS,
            )
        })
        .collect::<Vec<_>>();
    if ranges.is_empty() {
        return;
    }
    ranges.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut samples = Vec::new();
    for (start, end) in ranges {
        let mut beat = start;
        loop {
            let visible = descs
                .iter()
                .any(|desc| desc.lane == lane && calc_multitap_phase(desc, beat).visible);
            samples.push((
                beat,
                multitap_explosion_state(baseline, context, lane, visible),
            ));
            if beat >= end - f32::EPSILON {
                break;
            }
            beat = (beat + MULTITAP_SAMPLE_STEP).min(end);
        }
    }
    samples.sort_by(|left, right| left.0.total_cmp(&right.0));
    samples.dedup_by(|left, right| (left.0 - right.0).abs() <= f32::EPSILON);
    push_overlay_sample_eases(out, overlay_index, baseline, &samples);
}

pub fn push_multitap_actor_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    frame_index: usize,
    frame_baseline: SongLuaOverlayState,
    arrow_index: usize,
    arrow_baseline: SongLuaOverlayState,
    deco_index: usize,
    deco_baseline: SongLuaOverlayState,
    deco_children: &[(usize, SongLuaOverlayState)],
    context: &SongLuaCompileContext,
    player: usize,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    desc: &MultitapDesc,
) {
    let start = desc.taps[0] - MULTITAP_PREVISIBLE_BEATS;
    let end = desc.taps[desc.taps.len() - 1] + MULTITAP_HIDE_EPSILON_BEATS;
    let mut frame_samples = Vec::new();
    let mut arrow_samples = Vec::new();
    let mut deco_samples = Vec::new();
    let mut deco_child_samples = deco_children
        .iter()
        .map(|(index, _)| (*index, Vec::new()))
        .collect::<Vec<_>>();
    let mut beat = start;
    loop {
        let phase = calc_multitap_phase(desc, beat);
        frame_samples.push((
            beat,
            multitap_frame_state(frame_baseline, context, player, desc.lane, phase),
        ));
        push_multitap_arrow_sample(
            &mut arrow_samples,
            beat,
            arrow_baseline,
            noteskin_resolver,
            noteskin,
            desc.lane,
            phase,
        );
        deco_samples.push((
            beat,
            multitap_deco_state(deco_baseline, noteskin_resolver, noteskin, phase),
        ));
        for ((_, baseline), (_, samples)) in deco_children.iter().zip(&mut deco_child_samples) {
            samples.push((
                beat,
                multitap_deco_child_state(*baseline, noteskin_resolver, noteskin, phase),
            ));
        }
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + MULTITAP_SAMPLE_STEP).min(end);
    }
    push_overlay_sample_eases(out, frame_index, frame_baseline, &frame_samples);
    push_overlay_sample_eases(out, arrow_index, arrow_baseline, &arrow_samples);
    push_overlay_sample_eases(out, deco_index, deco_baseline, &deco_samples);
    for ((_, baseline), (child_index, samples)) in deco_children.iter().zip(deco_child_samples) {
        push_overlay_sample_eases(out, child_index, *baseline, &samples);
    }
}

pub fn compile_multitap_update_overlays_for_actors<Kind, EnsureArrowVisual>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &mut Vec<SongLuaOverlayCompileActor<Kind>>,
    messages: &mut Vec<SongLuaMessageEvent>,
    noteskin_resolver: SongLuaNoteskinResolver,
    mut ensure_arrow_visual: EnsureArrowVisual,
) -> Result<Option<Vec<SongLuaOverlayEase>>, String>
where
    EnsureArrowVisual:
        FnMut(&mut Vec<SongLuaOverlayCompileActor<Kind>>, usize, &str) -> Result<(), String>,
{
    let Some(multitaps) = read_multitap_descs(lua, context)? else {
        return Ok(None);
    };
    if multitaps.is_empty() {
        return Ok(None);
    }
    let overlay_indices = named_overlay_indices_by_name(overlays.len(), |index| {
        overlays[index].actor.name.as_deref()
    });
    let mut out = Vec::new();
    for player in 0..LUA_PLAYERS {
        if !context.players[player].enabled {
            continue;
        }
        let pn = player + 1;
        let Some(&field_index) = overlay_indices.get(format!("MultitapFrameP{pn}").as_str()) else {
            return Ok(None);
        };
        apply_multitap_field_state(
            &mut overlays[field_index].actor.initial_state,
            context,
            player,
        );
        for (mti, desc) in multitaps.iter().enumerate() {
            let index = mti + 1;
            let Some(&frame_index) = overlay_indices.get(format!("MultitapP{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            let Some(&arrow_index) =
                overlay_indices.get(format!("MultitapArrowP{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            let Some(&deco_index) =
                overlay_indices.get(format!("MultitapDeco{pn}_{index}").as_str())
            else {
                return Ok(None);
            };
            overlays[arrow_index]
                .actor
                .initial_state
                .texcoord_offset
                .get_or_insert([0.0, 0.0]);
            let noteskin = multitap_arrow_noteskin(overlays, arrow_index, context, player)?;
            ensure_arrow_visual(overlays, arrow_index, &noteskin)?;
            let deco_children =
                overlay_descendants_by_parent(overlays.len(), deco_index, |index| {
                    overlays
                        .get(index)
                        .and_then(|overlay| overlay.actor.parent_index)
                })
                .into_iter()
                .map(|index| (index, overlays[index].actor.initial_state))
                .collect::<Vec<_>>();
            push_multitap_actor_eases(
                &mut out,
                frame_index,
                overlays[frame_index].actor.initial_state,
                arrow_index,
                overlays[arrow_index].actor.initial_state,
                deco_index,
                overlays[deco_index].actor.initial_state,
                &deco_children,
                context,
                player,
                noteskin_resolver,
                &noteskin,
                desc,
            );
        }
        for lane in 1..=SONG_LUA_NOTE_COLUMNS {
            let Some(&explosion_index) =
                overlay_indices.get(format!("MultitapExplosionP{pn}_{lane}").as_str())
            else {
                continue;
            };
            push_multitap_explosion_eases(
                &mut out,
                explosion_index,
                overlays[explosion_index].actor.initial_state,
                context,
                &multitaps,
                lane,
            );
            install_multitap_explosion_messages(
                overlays,
                messages,
                explosion_index,
                &multitaps,
                lane,
                pn,
            );
        }
    }
    Ok(Some(out))
}

fn multitap_arrow_noteskin<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    arrow_index: usize,
    context: &SongLuaCompileContext,
    player: usize,
) -> Result<String, String> {
    overlays[arrow_index]
        .table
        .get::<Option<String>>("__songlua_noteskin_name")
        .map_err(|err| err.to_string())
        .map(|noteskin| {
            noteskin
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| context.players[player].noteskin_name.clone())
        })
}

fn install_multitap_explosion_messages<Kind>(
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    messages: &mut Vec<SongLuaMessageEvent>,
    explosion_index: usize,
    descs: &[MultitapDesc],
    lane: usize,
    pn: usize,
) {
    let message = format!("__songlua_multitap_explosion_p{pn}_{lane}");
    let mut installed = false;
    for overlay_index in std::iter::once(explosion_index).chain(overlay_descendants_by_parent(
        overlays.len(),
        explosion_index,
        |index| {
            overlays
                .get(index)
                .and_then(|overlay| overlay.actor.parent_index)
        },
    )) {
        let blocks = multitap_explosion_command_blocks(&overlays[overlay_index].actor);
        if blocks.is_empty() {
            continue;
        }
        overlays[overlay_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                message: message.clone(),
                blocks,
            });
        installed = true;
    }
    if !installed {
        return;
    }
    push_multitap_explosion_message_events(messages, descs, lane, &message);
}

fn push_multitap_explosion_message_events(
    messages: &mut Vec<SongLuaMessageEvent>,
    descs: &[MultitapDesc],
    lane: usize,
    message: &str,
) {
    let mut beats = descs
        .iter()
        .filter(|desc| desc.lane == lane)
        .flat_map(|desc| desc.taps.iter().copied())
        .filter(|beat| beat.is_finite())
        .collect::<Vec<_>>();
    beats.sort_by(f32::total_cmp);
    beats.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
    messages.extend(beats.into_iter().map(|beat| SongLuaMessageEvent {
        beat,
        message: message.to_owned(),
        persists: false,
    }));
}

pub fn multitap_explosion_command_blocks<Kind>(
    actor: &SongLuaOverlayActor<Kind>,
) -> Vec<SongLuaOverlayCommandBlock> {
    let mut out = Vec::new();
    for message in ["Judgment", "Dim", "W1"] {
        if let Some(command) = actor
            .message_commands
            .iter()
            .find(|command| command.message.eq_ignore_ascii_case(message))
        {
            out.extend(command.blocks.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explosion_messages_append_sorted_unique_finite_lane_beats() {
        let descs = [
            MultitapDesc {
                lane: 3,
                taps: vec![2.0, f32::NAN, 1.0],
                peak: None,
            },
            MultitapDesc {
                lane: 4,
                taps: vec![0.5],
                peak: None,
            },
            MultitapDesc {
                lane: 3,
                taps: vec![1.0, 3.0],
                peak: None,
            },
        ];
        let mut messages = vec![SongLuaMessageEvent {
            beat: 0.0,
            message: "existing".to_owned(),
            persists: true,
        }];

        push_multitap_explosion_message_events(&mut messages, &descs, 3, "explosion");

        assert_eq!(
            messages.iter().map(|event| event.beat).collect::<Vec<_>>(),
            [0.0, 1.0, 2.0, 3.0]
        );
        assert!(
            messages[1..]
                .iter()
                .all(|event| event.message == "explosion" && !event.persists)
        );
    }
}

fn push_overlay_sample_linear_ease(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    start: f32,
    end: f32,
    from: SongLuaOverlayState,
    to: SongLuaOverlayState,
) {
    if from == to {
        return;
    }
    let Some((from, to)) = overlay_delta_pair_from_states(baseline, from, to) else {
        return;
    };
    out.push(SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: end - start,
        span_mode: SongLuaSpanMode::Len,
        from,
        to,
        easing: Some("linear".to_string()),
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

fn push_overlay_sample_instant_state(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    start: f32,
    baseline: SongLuaOverlayState,
    state: SongLuaOverlayState,
) {
    if state == baseline {
        return;
    }
    let Some((from, to)) = overlay_delta_pair_from_states(baseline, state, state) else {
        return;
    };
    out.push(SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: 0.0,
        span_mode: SongLuaSpanMode::Len,
        from,
        to,
        easing: None,
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

fn push_overlay_sample_instant_visible(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    start: f32,
    visible: bool,
) {
    out.push(SongLuaOverlayEase {
        overlay_index,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: 0.0,
        span_mode: SongLuaSpanMode::Len,
        from: SongLuaOverlayStateDelta {
            visible: Some(visible),
            ..SongLuaOverlayStateDelta::default()
        },
        to: SongLuaOverlayStateDelta {
            visible: Some(visible),
            ..SongLuaOverlayStateDelta::default()
        },
        easing: None,
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

pub fn calc_multitap_phase(desc: &MultitapDesc, beat: f32) -> MultitapPhase {
    let mut out = MultitapPhase {
        pos: 0.0,
        squish: 0.0,
        lin: 0.0,
        qtc: 0,
        visible: false,
    };
    if beat > desc.taps[desc.taps.len() - 1] {
        return out;
    }
    out.pos = desc.taps[0] - beat;
    out.qtc = calc_multitap_qtzn(Some(desc.taps[0]));
    out.visible = out.pos < MULTITAP_PREVISIBLE_BEATS;
    let mut elasticity = desc
        .peak
        .zip(desc.taps.get(1).copied())
        .map(|(peak, second)| peak / (second - desc.taps[0]))
        .unwrap_or(MULTITAP_BASE_BOUNCE);
    for index in 0..desc.taps.len() {
        if beat <= desc.taps[index] || index + 1 >= desc.taps.len() {
            break;
        }
        let gap = desc.taps[index + 1] - desc.taps[index];
        if gap <= f32::EPSILON {
            continue;
        }
        elasticity = desc
            .peak
            .map(|peak| peak / gap)
            .unwrap_or(elasticity * MULTITAP_ELASTICITY);
        let t = beat - desc.taps[index];
        out.pos = elasticity * t * (gap - t) / gap;
        let velocity = elasticity * (gap - 2.0 * t) / gap;
        out.squish = MULTITAP_SQUISHY * (velocity.abs() - 0.5);
        out.lin = t / gap;
        out.qtc = calc_multitap_qtzn(desc.taps.get(index + 1).copied());
        out.visible = true;
    }
    out
}

fn calc_multitap_qtzn(beat: Option<f32>) -> u8 {
    let Some(beat) = beat.filter(|value| value.is_finite()) else {
        return 0;
    };
    let d48 = (beat.mul_add(48.0, 0.5).floor() as i32) - (beat.floor() as i32 * 48);
    match d48 {
        d if d <= 0 || d >= 48 => 1,
        d if d % 24 == 0 => 2,
        d if d % 16 == 0 => 3,
        d if d % 12 == 0 => 4,
        d if d % 8 == 0 => 6,
        d if d % 6 == 0 => 8,
        d if d % 4 == 0 => 12,
        d if d % 3 == 0 => 16,
        d if d % 2 == 0 => 24,
        _ => 48,
    }
}

pub fn multitap_frame_state(
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    player: usize,
    lane: usize,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let mut state = baseline;
    state.visible = true;
    state.x = song_lua_style_column_x(&context.style_name, lane - 1);
    state.y = THEME_RECEPTOR_Y_STD + multitap_y_offset(context, player, phase.pos);
    state.z = 0.0;
    state.zoom_x = 1.0;
    state.zoom_y = 1.0 + phase.squish;
    state.zoom_z = 1.0;
    state.diffuse[3] = 1.0;
    state
}

fn multitap_y_offset(context: &SongLuaCompileContext, player: usize, pos_beats: f32) -> f32 {
    pos_beats * 64.0 * song_lua_speedmod_multiplier(context, player)
}

fn song_lua_speedmod_multiplier(context: &SongLuaCompileContext, player: usize) -> f32 {
    let player = &context.players[player];
    let reference_bpm = player.display_bpms[1].max(player.display_bpms[0]).max(1.0);
    let music_rate = if context.song_music_rate.is_finite() && context.song_music_rate > 0.0 {
        context.song_music_rate
    } else {
        1.0
    };
    let multiplier = match player.speedmod {
        SongLuaSpeedMod::X(value) => value,
        SongLuaSpeedMod::C(value) | SongLuaSpeedMod::M(value) | SongLuaSpeedMod::A(value) => {
            value / reference_bpm / music_rate
        }
    };
    if multiplier.is_finite() && multiplier > 0.0 {
        multiplier
    } else {
        1.0
    }
}

fn multitap_arrow_state(
    baseline: SongLuaOverlayState,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    lane: usize,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return SongLuaOverlayState {
            visible: false,
            ..baseline
        };
    }
    let mut state = baseline;
    state.visible = true;
    state.rot_z_deg = MULTITAP_LANE_ROTATION[lane - 1];
    state.diffuse = [0.4, 0.4, 0.4, 1.0];
    state.texcoord_offset = Some(multitap_qtzn_texcoord_offset(
        noteskin_resolver,
        noteskin,
        phase.qtc,
    ));
    state
}

pub fn multitap_deco_state(
    baseline: SongLuaOverlayState,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let (effect_color1, effect_color2) =
        multitap_deco_color_pair(noteskin_resolver, noteskin, phase.qtc);
    let mut state = baseline;
    state.visible = true;
    state.zoom = 1.0;
    state.z = 10.0;
    state.rot_z_deg = phase.lin * 180.0;
    state.effect_mode = EffectMode::DiffuseRamp;
    state.effect_clock = EffectClock::Beat;
    state.effect_color1 = effect_color1;
    state.effect_color2 = effect_color2;
    state.effect_period = 1.0;
    state
}

pub fn multitap_deco_child_state(
    baseline: SongLuaOverlayState,
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let (effect_color1, effect_color2) =
        multitap_deco_color_pair(noteskin_resolver, noteskin, phase.qtc);
    let mut state = baseline;
    state.effect_mode = EffectMode::DiffuseRamp;
    state.effect_clock = EffectClock::Beat;
    state.effect_color1 = effect_color1;
    state.effect_color2 = effect_color2;
    state.effect_period = 1.0;
    state
}

pub fn multitap_explosion_state(
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    lane: usize,
    visible: bool,
) -> SongLuaOverlayState {
    let mut state = baseline;
    state.visible = visible;
    state.x = song_lua_style_column_x(&context.style_name, lane - 1);
    state.y = THEME_RECEPTOR_Y_STD;
    state.z = 0.0;
    state.rot_z_deg = MULTITAP_LANE_ROTATION[lane - 1];
    state
}

type MultitapColorPair = ([f32; 4], [f32; 4]);

const fn multitap_rgb(hex: u32) -> [f32; 4] {
    [
        ((hex >> 16) & 0xff) as f32 / 255.0,
        ((hex >> 8) & 0xff) as f32 / 255.0,
        (hex & 0xff) as f32 / 255.0,
        1.0,
    ]
}

const fn multitap_pair(left: u32, right: u32) -> MultitapColorPair {
    (multitap_rgb(left), multitap_rgb(right))
}

const MULTITAP_QTZN_VIVID: [MultitapColorPair; 8] = [
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
    multitap_pair(0xffffff, 0xcccccc),
];
const MULTITAP_QTZN_SHADOW: [MultitapColorPair; 8] = [
    multitap_pair(0xff6100, 0xff0000),
    multitap_pair(0x00a2ff, 0x00f0ff),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xe2f90f, 0x09a357),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xf1db03, 0xe67b02),
    multitap_pair(0x33fc7b, 0x04b8b6),
    multitap_pair(0x33fc7b, 0x04b8b6),
];
const MULTITAP_QTZN_NOTE: [MultitapColorPair; 8] = [
    multitap_pair(0xff7c7c, 0xff2121),
    multitap_pair(0x7e86f4, 0x2432ec),
    multitap_pair(0xbe77fb, 0x9018f8),
    multitap_pair(0xfaff73, 0xf7ff11),
    multitap_pair(0xf383bf, 0xeb2c93),
    multitap_pair(0xff966d, 0xff4d06),
    multitap_pair(0x90e3ff, 0x43d0ff),
    multitap_pair(0x85ff7c, 0x30ff20),
];
const MULTITAP_QTZN_COLOR: [MultitapColorPair; 8] = [
    multitap_pair(0xffc5c5, 0xff0000),
    multitap_pair(0x0000ff, 0xc5c5ff),
    multitap_pair(0x00ff00, 0xc5ffc5),
    multitap_pair(0xfff617, 0x646001),
    multitap_pair(0x00ff00, 0xc5ffc5),
    multitap_pair(0x00ff00, 0xc5ffc5),
    multitap_pair(0x00ff00, 0xc5ffc5),
    multitap_pair(0x00ff00, 0xc5ffc5),
];
const MULTITAP_QTZN_RAINBOW: [MultitapColorPair; 8] = [
    multitap_pair(0xff6100, 0xff0000),
    multitap_pair(0x00a2ff, 0x00f0ff),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xfa81d1, 0x7a15fe),
    multitap_pair(0xfa81d1, 0x7a15fe),
];
const MULTITAP_QTZN_HORSE: [MultitapColorPair; 8] = [
    multitap_pair(0xdfa9db, 0xa96fba),
    multitap_pair(0xfaba61, 0xd49234),
    multitap_pair(0x98d3f1, 0x2c78b6),
    multitap_pair(0xfe96b9, 0xb7366e),
    multitap_pair(0xb6b3d5, 0x6947bf),
    multitap_pair(0xf0e56e, 0xeae6bf),
    multitap_pair(0x8b7bff, 0x503497),
    multitap_pair(0xebe6ad, 0xedb032),
];

fn multitap_qtzn_tex(qtzn: u8) -> usize {
    match qtzn {
        2 => 1,
        3 => 2,
        4 => 3,
        6 => 4,
        8 => 5,
        12 => 6,
        16 | 24 | 48 => 7,
        _ => 0,
    }
}

fn multitap_qtzn_texcoord_offset(
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    qtzn: u8,
) -> [f32; 2] {
    let tex = multitap_qtzn_tex(qtzn) as f32;
    let x = noteskin_resolver
        .metric_f(noteskin, "", "TapNoteNoteColorTextureCoordSpacingX")
        .unwrap_or(0.0);
    let y = noteskin_resolver
        .metric_f(noteskin, "", "TapNoteNoteColorTextureCoordSpacingY")
        .unwrap_or(0.0);
    [x * tex, y * tex]
}

fn multitap_deco_color_pair(
    noteskin_resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    qtzn: u8,
) -> MultitapColorPair {
    if noteskin_resolver
        .metric_b(noteskin, "", "TapNoteAnimationIsVivid")
        .unwrap_or(false)
    {
        return MULTITAP_QTZN_VIVID[0];
    }
    multitap_qtzn_color_table(noteskin)[multitap_qtzn_tex(qtzn)]
}

fn multitap_qtzn_color_table(noteskin: &str) -> &'static [MultitapColorPair; 8] {
    let noteskin = noteskin.to_ascii_lowercase();
    if noteskin.contains("color") {
        return &MULTITAP_QTZN_COLOR;
    }
    if noteskin.contains("rainbow") || noteskin.contains("solo") {
        return &MULTITAP_QTZN_RAINBOW;
    }
    if noteskin.contains("horse") || noteskin.contains("toonprints") {
        return &MULTITAP_QTZN_HORSE;
    }
    for key in [
        "cel",
        "cyber",
        "delta",
        "ddrlike",
        "enchantment",
        "excel",
        "metal",
        "onlyonecouples",
        "scalable",
        "spotlight",
        "vel",
        "vintage",
    ] {
        if noteskin.contains(key) {
            return &MULTITAP_QTZN_SHADOW;
        }
    }
    for key in [
        "ascii", "default", "easy", "exact", "lambda", "note", "retro", "trax",
    ] {
        if noteskin.contains(key) {
            return &MULTITAP_QTZN_NOTE;
        }
    }
    &MULTITAP_QTZN_VIVID
}

pub fn overlay_delta_pair_from_states(
    baseline: SongLuaOverlayState,
    from: SongLuaOverlayState,
    to: SongLuaOverlayState,
) -> Option<(SongLuaOverlayStateDelta, SongLuaOverlayStateDelta)> {
    let mut out_from = SongLuaOverlayStateDelta::default();
    let mut out_to = SongLuaOverlayStateDelta::default();
    macro_rules! copy_value_field {
        ($field:ident) => {
            if from.$field != baseline.$field || to.$field != baseline.$field {
                out_from.$field = Some(from.$field);
                out_to.$field = Some(to.$field);
            }
        };
    }
    macro_rules! copy_option_field {
        ($field:ident) => {
            if from.$field != baseline.$field || to.$field != baseline.$field {
                out_from.$field = from.$field;
                out_to.$field = to.$field;
            }
        };
    }
    copy_value_field!(x);
    copy_value_field!(y);
    copy_value_field!(z);
    copy_value_field!(z_bias);
    copy_value_field!(draw_order);
    copy_value_field!(draw_by_z_position);
    copy_value_field!(halign);
    copy_value_field!(valign);
    copy_value_field!(text_align);
    copy_value_field!(uppercase);
    copy_value_field!(shadow_len);
    copy_value_field!(shadow_color);
    copy_value_field!(glow);
    copy_option_field!(fov);
    copy_option_field!(vanishpoint);
    copy_value_field!(diffuse);
    copy_option_field!(vertex_colors);
    copy_value_field!(visible);
    copy_value_field!(cropleft);
    copy_value_field!(cropright);
    copy_value_field!(croptop);
    copy_value_field!(cropbottom);
    copy_value_field!(fadeleft);
    copy_value_field!(faderight);
    copy_value_field!(fadetop);
    copy_value_field!(fadebottom);
    copy_value_field!(mask_source);
    copy_value_field!(mask_dest);
    copy_value_field!(zoom);
    copy_value_field!(zoom_x);
    copy_value_field!(zoom_y);
    copy_value_field!(zoom_z);
    copy_value_field!(basezoom);
    copy_value_field!(basezoom_x);
    copy_value_field!(basezoom_y);
    copy_value_field!(basezoom_z);
    copy_value_field!(rot_x_deg);
    copy_value_field!(rot_y_deg);
    copy_value_field!(rot_z_deg);
    copy_value_field!(skew_x);
    copy_value_field!(skew_y);
    copy_value_field!(blend);
    copy_value_field!(vibrate);
    copy_value_field!(effect_magnitude);
    copy_value_field!(effect_clock);
    copy_value_field!(effect_mode);
    copy_value_field!(effect_color1);
    copy_value_field!(effect_color2);
    copy_value_field!(effect_period);
    copy_value_field!(effect_offset);
    copy_option_field!(effect_timing);
    copy_value_field!(rainbow);
    copy_value_field!(rainbow_scroll);
    copy_value_field!(text_jitter);
    copy_value_field!(text_distortion);
    copy_value_field!(text_glow_mode);
    copy_value_field!(mult_attrs_with_diffuse);
    copy_value_field!(sprite_animate);
    copy_value_field!(sprite_loop);
    copy_value_field!(sprite_playback_rate);
    copy_value_field!(sprite_state_delay);
    copy_option_field!(sprite_state_index);
    copy_option_field!(vert_spacing);
    copy_option_field!(wrap_width_pixels);
    copy_option_field!(max_width);
    copy_option_field!(max_height);
    copy_value_field!(max_w_pre_zoom);
    copy_value_field!(max_h_pre_zoom);
    copy_value_field!(max_dimension_uses_zoom);
    copy_value_field!(depth_test);
    copy_value_field!(texture_filtering);
    copy_value_field!(texture_wrapping);
    copy_option_field!(texcoord_offset);
    copy_option_field!(custom_texture_rect);
    copy_option_field!(texcoord_velocity);
    copy_option_field!(size);
    copy_option_field!(stretch_rect);
    overlay_delta_intersection(&out_from, &out_to)
}
