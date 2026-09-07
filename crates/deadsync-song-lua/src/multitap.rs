use deadlib_present::anim::{EffectClock, EffectMode};
use mlua::{Lua, Table, Value};

use crate::{
    LUA_PLAYERS, SONG_LUA_DOUBLE_NOTE_COLUMNS, SONG_LUA_NOTE_COLUMNS, SongLuaCompileContext,
    SongLuaNoteskinResolver, SongLuaOverlayCompileActor, SongLuaOverlayEase,
    SongLuaOverlayMessageCommand, SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaSpanMode,
    SongLuaSpeedMod, SongLuaTimeUnit, THEME_RECEPTOR_Y_REV, THEME_RECEPTOR_Y_STD,
    named_overlay_indices_by_name, overlay_delta_intersection, overlay_descendants_by_parent,
    read_f32, song_lua_style_column_x,
};

pub const MULTITAP_PREVISIBLE_BEATS: f32 = 8.0;

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
        taps.sort_by(f32::total_cmp);
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
                push_overlay_sample_linear_ease(out, overlay_index, baseline, start, end, from, to);
            }
            (false, true) => {
                push_overlay_sample_instant_state(out, overlay_index, end, baseline, to);
            }
            (true, false) => push_overlay_sample_instant_visible(out, overlay_index, end, false),
            (false, false) => {}
        }
    }
}

// Near zero, next_up alone is too small to survive `first_tap - beat`.
const fn multitap_visible_start(first_tap: f32) -> f32 {
    (first_tap - MULTITAP_PREVISIBLE_BEATS.next_down())
        .max((first_tap - MULTITAP_PREVISIBLE_BEATS).next_up())
}

pub fn push_multitap_explosion_eases(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    descs: &[MultitapDesc],
    lane: usize,
) {
    let mut beats = descs
        .iter()
        .filter(|desc| desc.lane == lane)
        .flat_map(|desc| {
            [
                multitap_visible_start(desc.taps[0]),
                desc.taps[desc.taps.len() - 1].next_up(),
            ]
        })
        .collect::<Vec<_>>();
    beats.sort_by(f32::total_cmp);
    beats.dedup();
    let samples = beats
        .into_iter()
        .map(|beat| {
            let visible = descs
                .iter()
                .any(|desc| desc.lane == lane && calc_multitap_phase(desc, beat).visible);
            (
                beat,
                multitap_explosion_state(baseline, context, lane, visible),
            )
        })
        .collect::<Vec<_>>();
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
    // Preserve strict visibility/tap boundaries and the derivative change at
    // each bounce apex. The smallest representable following beat keeps the
    // authored `beat > tap` edge without introducing a sampling delay.
    let mut beats = vec![multitap_visible_start(desc.taps[0])];
    for (index, &tap) in desc.taps.iter().enumerate() {
        beats.push(tap);
        beats.push(tap.next_up());
        if let Some(&next) = desc.taps.get(index + 1) {
            beats.push(tap.midpoint(next));
        }
    }
    beats.sort_by(f32::total_cmp);
    beats.dedup();
    let mut frame_samples = Vec::new();
    let mut arrow_samples = Vec::new();
    let mut deco_samples = Vec::new();
    let mut deco_child_samples = deco_children
        .iter()
        .map(|(index, _)| (*index, Vec::new()))
        .collect::<Vec<_>>();
    for beat in beats {
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
    }
    let first_ease = out.len();
    push_overlay_sample_eases(out, frame_index, frame_baseline, &frame_samples);
    // Y follows a parabola; squash and the other components are piecewise
    // linear. Split only Y out of each bounce half and keep its exact curve.
    let mut parabolas = Vec::new();
    for ease in &mut out[first_ease..] {
        if ease.limit <= f32::EPSILON || ease.start <= desc.taps[0] {
            continue;
        }
        let (Some(from), Some(to)) = (ease.from.y, ease.to.y) else {
            continue;
        };
        let mut y = ease.clone();
        y.from = SongLuaOverlayStateDelta {
            y: Some(from),
            ..Default::default()
        };
        y.to = SongLuaOverlayStateDelta {
            y: Some(to),
            ..Default::default()
        };
        y.easing = Some(if to > from { "outQuad" } else { "inQuad" }.into());
        ease.from.y = None;
        ease.to.y = None;
        parabolas.push(y);
    }
    out.extend(parabolas);
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
        if !overlay_indices.contains_key(format!("MultitapFrameP{pn}").as_str()) {
            return Ok(None);
        }
        // The startup update already copied the Player and NoteField transforms
        // into their distinct wrappers. Preserve both, including NoteField Y.
        let rotation_x = lua
            .globals()
            .get::<Option<f32>>("MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST")
            .map_err(|error| error.to_string())?
            .map(|offset| offset + crate::host::arrow_effects_rotation_x(0))
            .unwrap_or(0.0);
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
            overlays[arrow_index].actor.initial_state.rot_x_deg += rotation_x;
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
            install_multitap_explosion_messages(lua, overlays, explosion_index, lane, pn)?;
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
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    explosion_index: usize,
    lane: usize,
    pn: usize,
) -> Result<(), String> {
    for index in std::iter::once(explosion_index).chain(overlay_descendants_by_parent(
        overlays.len(),
        explosion_index,
        |index| overlays[index].actor.parent_index,
    )) {
        for grade in ["W1", "W2", "W3", "W4", "W5"] {
            let table = &overlays[index].table;
            let mut commands = Vec::new();
            for name in ["Judgment", "Dim", grade] {
                if let Some(command) = table
                    .raw_get::<Option<mlua::Function>>(format!("{name}Command"))
                    .map_err(|error| error.to_string())?
                {
                    commands.push(command);
                }
            }
            if commands.is_empty() {
                continue;
            }
            let message = format!("__songlua_tap_{pn}_{lane}_{grade}");
            let key = format!("{message}Command");
            let invoke = lua
                .create_function(move |_, actor: Table| {
                    for command in &commands {
                        command.call::<Value>(actor.clone())?;
                    }
                    Ok(actor)
                })
                .map_err(|error| error.to_string())?;
            table
                .set(key.as_str(), invoke)
                .map_err(|error| error.to_string())?;
            let blocks = crate::capture_actor_command_preserving_state(lua, table, &key);
            table.raw_remove(key).map_err(|error| error.to_string())?;
            let blocks = blocks?;
            if !blocks.is_empty() {
                overlays[index]
                    .actor
                    .message_commands
                    .push(SongLuaOverlayMessageCommand {
                        message,
                        blocks,
                        aux: None,
                    });
            }
        }
    }
    Ok(())
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

#[must_use]
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
        let velocity = elasticity * 2.0f32.mul_add(-t, gap) / gap;
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

#[must_use]
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
    state.y = (THEME_RECEPTOR_Y_STD - THEME_RECEPTOR_Y_REV) * 0.5
        + multitap_y_offset(context, player, phase.pos);
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

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn multitap_explosion_state(
    baseline: SongLuaOverlayState,
    context: &SongLuaCompileContext,
    lane: usize,
    visible: bool,
) -> SongLuaOverlayState {
    let mut state = baseline;
    state.visible = visible;
    state.x = song_lua_style_column_x(&context.style_name, lane - 1);
    state.y = (THEME_RECEPTOR_Y_STD - THEME_RECEPTOR_Y_REV) * 0.5;
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
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
    multitap_pair(0x00ff_ffff, 0x00cc_cccc),
];
const MULTITAP_QTZN_SHADOW: [MultitapColorPair; 8] = [
    multitap_pair(0x00ff_6100, 0x00ff_0000),
    multitap_pair(0x0000_a2ff, 0x0000_f0ff),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00e2_f90f, 0x0009_a357),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00f1_db03, 0x00e6_7b02),
    multitap_pair(0x0033_fc7b, 0x0004_b8b6),
    multitap_pair(0x0033_fc7b, 0x0004_b8b6),
];
const MULTITAP_QTZN_NOTE: [MultitapColorPair; 8] = [
    multitap_pair(0x00ff_7c7c, 0x00ff_2121),
    multitap_pair(0x007e_86f4, 0x0024_32ec),
    multitap_pair(0x00be_77fb, 0x0090_18f8),
    multitap_pair(0x00fa_ff73, 0x00f7_ff11),
    multitap_pair(0x00f3_83bf, 0x00eb_2c93),
    multitap_pair(0x00ff_966d, 0x00ff_4d06),
    multitap_pair(0x0090_e3ff, 0x0043_d0ff),
    multitap_pair(0x0085_ff7c, 0x0030_ff20),
];
const MULTITAP_QTZN_COLOR: [MultitapColorPair; 8] = [
    multitap_pair(0x00ff_c5c5, 0x00ff_0000),
    multitap_pair(0x0000_00ff, 0x00c5_c5ff),
    multitap_pair(0x0000_ff00, 0x00c5_ffc5),
    multitap_pair(0x00ff_f617, 0x0064_6001),
    multitap_pair(0x0000_ff00, 0x00c5_ffc5),
    multitap_pair(0x0000_ff00, 0x00c5_ffc5),
    multitap_pair(0x0000_ff00, 0x00c5_ffc5),
    multitap_pair(0x0000_ff00, 0x00c5_ffc5),
];
const MULTITAP_QTZN_RAINBOW: [MultitapColorPair; 8] = [
    multitap_pair(0x00ff_6100, 0x00ff_0000),
    multitap_pair(0x0000_a2ff, 0x0000_f0ff),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
    multitap_pair(0x00fa_81d1, 0x007a_15fe),
];
const MULTITAP_QTZN_HORSE: [MultitapColorPair; 8] = [
    multitap_pair(0x00df_a9db, 0x00a9_6fba),
    multitap_pair(0x00fa_ba61, 0x00d4_9234),
    multitap_pair(0x0098_d3f1, 0x002c_78b6),
    multitap_pair(0x00fe_96b9, 0x00b7_366e),
    multitap_pair(0x00b6_b3d5, 0x0069_47bf),
    multitap_pair(0x00f0_e56e, 0x00ea_e6bf),
    multitap_pair(0x008b_7bff, 0x0050_3497),
    multitap_pair(0x00eb_e6ad, 0x00ed_b032),
];

const fn multitap_qtzn_tex(qtzn: u8) -> usize {
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

macro_rules! overlay_value_fields {
    ($visit:ident) => {
        $visit!(x);
        $visit!(y);
        $visit!(z);
        $visit!(z_bias);
        $visit!(draw_order);
        $visit!(draw_by_z_position);
        $visit!(halign);
        $visit!(valign);
        $visit!(text_align);
        $visit!(uppercase);
        $visit!(shadow_len);
        $visit!(shadow_color);
        $visit!(glow);
        $visit!(diffuse);
        $visit!(visible);
        $visit!(cropleft);
        $visit!(cropright);
        $visit!(croptop);
        $visit!(cropbottom);
        $visit!(fadeleft);
        $visit!(faderight);
        $visit!(fadetop);
        $visit!(fadebottom);
        $visit!(mask_source);
        $visit!(mask_dest);
        $visit!(zoom);
        $visit!(zoom_x);
        $visit!(zoom_y);
        $visit!(zoom_z);
        $visit!(basezoom);
        $visit!(basezoom_x);
        $visit!(basezoom_y);
        $visit!(basezoom_z);
        $visit!(rot_x_deg);
        $visit!(rot_y_deg);
        $visit!(rot_z_deg);
        $visit!(skew_x);
        $visit!(skew_y);
        $visit!(blend);
        $visit!(vibrate);
        $visit!(effect_magnitude);
        $visit!(effect_clock);
        $visit!(effect_mode);
        $visit!(effect_color1);
        $visit!(effect_color2);
        $visit!(effect_period);
        $visit!(effect_offset);
        $visit!(rainbow);
        $visit!(rainbow_scroll);
        $visit!(text_jitter);
        $visit!(text_distortion);
        $visit!(text_glow_mode);
        $visit!(mult_attrs_with_diffuse);
        $visit!(sprite_animate);
        $visit!(sprite_loop);
        $visit!(sprite_playback_rate);
        $visit!(sprite_state_delay);
        $visit!(max_w_pre_zoom);
        $visit!(max_h_pre_zoom);
        $visit!(max_dimension_uses_zoom);
        $visit!(depth_test);
        $visit!(texture_filtering);
        $visit!(texture_wrapping);
    };
}

macro_rules! overlay_option_fields {
    ($visit:ident) => {
        $visit!(fov);
        $visit!(vanishpoint);
        $visit!(vertex_colors);
        $visit!(effect_timing);
        $visit!(sprite_state_index);
        $visit!(vert_spacing);
        $visit!(wrap_width_pixels);
        $visit!(max_width);
        $visit!(max_height);
        $visit!(texcoord_offset);
        $visit!(custom_texture_rect);
        $visit!(texcoord_velocity);
        $visit!(size);
        $visit!(stretch_rect);
    };
}

#[must_use]
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
    overlay_value_fields!(copy_value_field);
    overlay_option_fields!(copy_option_field);
    overlay_delta_intersection(&out_from, &out_to)
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
