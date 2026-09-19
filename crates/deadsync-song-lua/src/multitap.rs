use deadlib_present::anim::{EffectClock, EffectMode};
use mlua::{Lua, Table, Value};
use std::sync::Arc;

use crate::{
    LUA_PLAYERS, SONG_LUA_DOUBLE_NOTE_COLUMNS, SONG_LUA_NOTE_COLUMNS, SongLuaCompileContext,
    SongLuaNoteskinResolver, SongLuaOverlayCompileActor, SongLuaOverlayEase, SongLuaOverlayKind,
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
    push_overlay_sample_eases_iter(out, overlay_index, baseline, samples.iter().copied());
}

fn push_overlay_sample_eases_iter(
    out: &mut Vec<SongLuaOverlayEase>,
    overlay_index: usize,
    baseline: SongLuaOverlayState,
    samples: impl IntoIterator<Item = (f32, SongLuaOverlayState)>,
) {
    let mut samples = samples.into_iter();
    let Some(mut previous) = samples.next() else {
        return;
    };
    push_overlay_sample_instant_state(out, overlay_index, previous.0, baseline, previous.1);
    for current in samples {
        let (start, from) = previous;
        let (end, to) = current;
        previous = current;
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
    // Consume states as they are produced; only adjacent samples are needed.
    let samples = beats.into_iter().map(|beat| {
        let visible = descs
            .iter()
            .any(|desc| desc.lane == lane && multitap_is_visible(desc, beat));
        (
            beat,
            multitap_explosion_state(baseline, context, lane, visible),
        )
    });
    push_overlay_sample_eases_iter(out, overlay_index, baseline, samples);
}

fn multitap_is_visible(desc: &MultitapDesc, beat: f32) -> bool {
    if beat > desc.taps[desc.taps.len() - 1] {
        return false;
    }
    if desc.taps[0] - beat < MULTITAP_PREVISIBLE_BEATS {
        return true;
    }
    if beat <= desc.taps[0] {
        return false;
    }
    // Unordered floating-point comparisons can reach the bounce loop even
    // outside the usual visibility interval. Keep public callers' NaN behavior.
    calc_multitap_phase(desc, beat).visible
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
    numbered: bool,
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
    for &beat in &beats {
        let phase = calc_multitap_phase(desc, beat);
        if context.player_timing[player].is_none() {
            frame_samples.push((
                beat,
                multitap_frame_state(frame_baseline, context, player, desc.lane, beat, phase),
            ));
        }
        push_multitap_arrow_sample(
            &mut arrow_samples,
            beat,
            arrow_baseline,
            noteskin_resolver,
            noteskin,
            desc.lane,
            phase,
        );
        if numbered && phase.visible {
            // The numbered script brightens the arrow after each tap. Keep
            // strict `beat > tap` edges, just like its Lua countdown.
            let hit = desc.taps.partition_point(|tap| *tap < beat);
            let diffuse = 0.4 + 0.6 * hit as f32 / (desc.taps.len() - 1).max(1) as f32;
            if let Some((_, arrow)) = arrow_samples.last_mut() {
                arrow.diffuse = [diffuse, diffuse, diffuse, 1.0];
            }
        }
        deco_samples.push((
            beat,
            if numbered {
                multitap_count_state(deco_baseline, noteskin_resolver, noteskin, desc, beat)
            } else {
                multitap_deco_state(deco_baseline, noteskin_resolver, noteskin, phase)
            },
        ));
        for ((_, baseline), (_, samples)) in deco_children.iter().zip(&mut deco_child_samples) {
            samples.push((
                beat,
                multitap_deco_child_state(*baseline, noteskin_resolver, noteskin, phase),
            ));
        }
    }
    if context.player_timing[player].is_some() {
        // Only motion needs extra samples. Keep noteskin/color resolution at
        // the authored boundaries, and bake the timing curve before gameplay.
        // Sampling only appends, so the original boundary indices stay valid.
        for index in 1..beats.len() {
            let start = beats[index - 1];
            let end = beats[index];
            sample_multitap_y(&mut beats, context, player, desc, start, end, 0);
        }
        beats.sort_by(f32::total_cmp);
        beats.dedup();
        frame_samples = beats
            .into_iter()
            .map(|beat| {
                (
                    beat,
                    multitap_frame_state(
                        frame_baseline,
                        context,
                        player,
                        desc.lane,
                        beat,
                        calc_multitap_phase(desc, beat),
                    ),
                )
            })
            .collect();
    }
    let first_ease = out.len();
    push_overlay_sample_eases(out, frame_index, frame_baseline, &frame_samples);
    if context.player_timing[player].is_none() {
        split_multitap_y_eases(out, first_ease, desc.taps[0]);
    }
    push_overlay_sample_eases(out, arrow_index, arrow_baseline, &arrow_samples);
    push_overlay_sample_eases(out, deco_index, deco_baseline, &deco_samples);
    for ((_, baseline), (child_index, samples)) in deco_children.iter().zip(deco_child_samples) {
        push_overlay_sample_eases(out, child_index, *baseline, &samples);
    }
}

fn sample_multitap_y(
    beats: &mut Vec<f32>,
    context: &SongLuaCompileContext,
    player: usize,
    desc: &MultitapDesc,
    start: f32,
    end: f32,
    depth: u8,
) {
    let mid = start.midpoint(end);
    if depth == 16 || end - start <= 1.0 / 1024.0 || mid <= start || mid >= end {
        return;
    }
    let y = |beat| multitap_y_offset(context, player, beat, calc_multitap_phase(desc, beat).pos);
    let from = y(start);
    let to = y(end);
    // Probe quarters too: a speed ramp multiplied by a bounce can be cubic,
    // and a midpoint alone can miss curvature or a scroll boundary.
    let tolerance = 0.005 * song_lua_speedmod_multiplier(context, player).max(1.0);
    let linear = end - start <= 0.125
        && [0.25, 0.5, 0.75]
            .into_iter()
            .all(|t| (y(start + (end - start) * t) - (from + (to - from) * t)).abs() <= tolerance);
    if linear {
        return;
    }
    beats.push(mid);
    sample_multitap_y(beats, context, player, desc, start, mid, depth + 1);
    sample_multitap_y(beats, context, player, desc, mid, end, depth + 1);
}

fn split_multitap_y_eases(out: &mut Vec<SongLuaOverlayEase>, first_ease: usize, first_tap: f32) {
    // Y follows a parabola; squash and the other components are piecewise
    // linear. Split only Y out of each bounce half and keep its exact curve.
    // Visit only the original frame eases, appending Y curves in the same order.
    let end = out.len();
    for index in first_ease..end {
        let ease = &mut out[index];
        if ease.limit <= f32::EPSILON || ease.start <= first_tap {
            continue;
        }
        let (Some(from), Some(to)) = (ease.from.y, ease.to.y) else {
            continue;
        };
        let y = SongLuaOverlayEase {
            overlay_index: ease.overlay_index,
            unit: ease.unit,
            start: ease.start,
            limit: ease.limit,
            span_mode: ease.span_mode,
            from: SongLuaOverlayStateDelta {
                y: Some(from),
                ..Default::default()
            },
            to: SongLuaOverlayStateDelta {
                y: Some(to),
                ..Default::default()
            },
            easing: Some(if to > from { "outQuad" } else { "inQuad" }.into()),
            sustain: ease.sustain,
            opt1: ease.opt1,
            opt2: ease.opt2,
        };
        ease.from.y = None;
        ease.to.y = None;
        out.push(y);
    }
}

pub fn compile_multitap_update_overlays_for_actors<Slot, Vertex, Attribute, EnsureArrowVisual>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &mut Vec<SongLuaOverlayCompileActor<SongLuaOverlayKind<Slot, Vertex, Attribute>>>,
    noteskin_resolver: SongLuaNoteskinResolver,
    mut ensure_arrow_visual: EnsureArrowVisual,
) -> Result<Option<Vec<SongLuaOverlayEase>>, String>
where
    EnsureArrowVisual: FnMut(
        &mut Vec<SongLuaOverlayCompileActor<SongLuaOverlayKind<Slot, Vertex, Attribute>>>,
        usize,
        &str,
    ) -> Result<(), String>,
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
            let numbered =
                overlay_indices.contains_key(format!("MultitapTextP{pn}_{index}").as_str());
            let Some(&deco_index) = overlay_indices.get(
                if numbered {
                    format!("MultitapTextP{pn}_{index}")
                } else {
                    format!("MultitapDeco{pn}_{index}")
                }
                .as_str(),
            ) else {
                return Ok(None);
            };
            if numbered {
                let SongLuaOverlayKind::BitmapText { text_changes, .. } =
                    &mut overlays[deco_index].actor.kind
                else {
                    return Ok(None);
                };
                *text_changes = std::iter::once((
                    multitap_visible_start(desc.taps[0]),
                    Arc::from(desc.taps.len().to_string()),
                ))
                .chain(
                    desc.taps
                        .iter()
                        .take(desc.taps.len().saturating_sub(2))
                        .enumerate()
                        .map(|(tap, beat)| {
                            (
                                beat.next_up(),
                                Arc::from((desc.taps.len() - tap - 1).to_string()),
                            )
                        }),
                )
                .collect();
            }
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
                numbered,
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
    out.visible = out.pos < MULTITAP_PREVISIBLE_BEATS;
    let mut elasticity = desc
        .peak
        .zip(desc.taps.get(1).copied())
        .map(|(peak, second)| peak / (second - desc.taps[0]))
        .unwrap_or(MULTITAP_BASE_BOUNCE);
    // Earlier bounces only contribute to elasticity. Evaluate geometry and
    // quantization once, for the last applicable interval. Keep the repeated
    // multiplications in order so authored bounce heights stay bit-for-bit equal.
    let mut bounce = None;
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
        bounce = Some((index, gap));
    }
    if let Some((index, gap)) = bounce {
        let t = beat - desc.taps[index];
        out.pos = elasticity * t * (gap - t) / gap;
        let velocity = elasticity * 2.0f32.mul_add(-t, gap) / gap;
        out.squish = MULTITAP_SQUISHY * (velocity.abs() - 0.5);
        out.lin = t / gap;
        out.qtc = calc_multitap_qtzn(desc.taps.get(index + 1).copied());
        out.visible = true;
    } else {
        out.qtc = calc_multitap_qtzn(Some(desc.taps[0]));
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
    beat: f32,
    phase: MultitapPhase,
) -> SongLuaOverlayState {
    if !phase.visible {
        return baseline;
    }
    let mut state = baseline;
    state.visible = true;
    state.x = song_lua_style_column_x(&context.style_name, lane - 1);
    state.y = (THEME_RECEPTOR_Y_STD - THEME_RECEPTOR_Y_REV) * 0.5
        + multitap_y_offset(context, player, beat, phase.pos);
    state.z = 0.0;
    state.zoom_x = 1.0;
    state.zoom_y = 1.0 + phase.squish;
    state.zoom_z = 1.0;
    state.diffuse[3] = 1.0;
    state
}

fn multitap_y_offset(
    context: &SongLuaCompileContext,
    player: usize,
    beat: f32,
    pos_beats: f32,
) -> f32 {
    if let Some(timing) = &context.player_timing[player] {
        let seconds = timing.get_time_for_beat_exact(beat);
        if let SongLuaSpeedMod::C(value) = context.players[player].speedmod {
            return (timing.get_time_for_beat(beat + pos_beats) - timing.get_time_for_beat(beat))
                * value
                / 60.0
                / crate::song_music_rate(context)
                * 64.0;
        }
        return (timing.get_displayed_beat(beat + pos_beats) - timing.get_displayed_beat(beat))
            * timing.get_speed_multiplier(beat, seconds)
            * 64.0
            * song_lua_speedmod_multiplier(context, player);
    }
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

fn multitap_count_state(
    baseline: SongLuaOverlayState,
    resolver: SongLuaNoteskinResolver,
    noteskin: &str,
    desc: &MultitapDesc,
    beat: f32,
) -> SongLuaOverlayState {
    if !multitap_is_visible(desc, beat) {
        return baseline;
    }
    let hit = desc.taps.partition_point(|tap| *tap < beat);
    let mut state = baseline;
    state.visible = desc.taps.len().saturating_sub(hit) > 1;
    if state.visible {
        let qtzn = calc_multitap_qtzn(desc.taps.get(hit + 1).copied());
        (state.effect_color1, state.effect_color2) =
            multitap_deco_color_pair(resolver, noteskin, qtzn);
        state.zoom = 1.0;
        state.effect_mode = EffectMode::DiffuseRamp;
        state.effect_clock = EffectClock::Beat;
        state.effect_period = 1.0;
    }
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
    // Short names avoid a lowercase allocation. For unusually long names,
    // retain str's substring search instead of rescanning every byte per key.
    if noteskin.len() > 16 {
        let lowercase = noteskin.to_ascii_lowercase();
        return multitap_color_table(|needle| lowercase.contains(needle));
    }
    multitap_color_table(|needle| {
        noteskin
            .as_bytes()
            .windows(needle.len())
            .any(|candidate| candidate.eq_ignore_ascii_case(needle.as_bytes()))
    })
}

fn multitap_color_table(contains: impl Fn(&str) -> bool) -> &'static [MultitapColorPair; 8] {
    if contains("color") {
        return &MULTITAP_QTZN_COLOR;
    }
    if contains("rainbow") || contains("solo") {
        return &MULTITAP_QTZN_RAINBOW;
    }
    if contains("horse") || contains("toonprints") {
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
        if contains(key) {
            return &MULTITAP_QTZN_SHADOW;
        }
    }
    for key in [
        "ascii", "default", "easy", "exact", "lambda", "note", "retro", "trax",
    ] {
        if contains(key) {
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
    fn multitap_timed_actor_curves_cover_refined_bounce_intervals() {
        use deadsync_rules::timing::{SpeedSegment, SpeedUnit, TimingData, TimingSegments};

        let mut context = SongLuaCompileContext::new(".", "Refined multitap boundaries");
        context.player_timing[0] = Some(TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                speeds: vec![SpeedSegment {
                    beat: 4.0,
                    ratio: 2.0,
                    delay: 4.0,
                    unit: SpeedUnit::Beats,
                }],
                ..Default::default()
            },
            &[],
        ));
        let baseline = SongLuaOverlayState {
            visible: false,
            ..Default::default()
        };
        for taps in [vec![4.0, 6.0, 9.0], vec![4.0, 4.0, 6.0, 9.0]] {
            let desc = MultitapDesc {
                lane: 1,
                taps,
                peak: Some(1.5),
            };
            let mut out = Vec::new();
            push_multitap_actor_eases(
                &mut out,
                1,
                baseline,
                2,
                baseline,
                3,
                baseline,
                &[],
                &context,
                0,
                SongLuaNoteskinResolver::default(),
                "default",
                &desc,
                false,
            );
            let curves: Vec<_> = out
                .iter()
                .filter(|ease| {
                    ease.overlay_index == 1 && ease.from.y.is_some() && ease.to.y.is_some()
                })
                .collect();
            assert!(curves.len() > desc.taps.len() * 3);
            assert!(
                curves
                    .iter()
                    .any(|ease| ease.start >= 8.0 && ease.limit > 0.0)
            );
            for ease in curves {
                for (beat, y) in [
                    (ease.start, ease.from.y.unwrap()),
                    (ease.start + ease.limit, ease.to.y.unwrap()),
                ] {
                    let expected = multitap_frame_state(
                        baseline,
                        &context,
                        0,
                        desc.lane,
                        beat,
                        calc_multitap_phase(&desc, beat),
                    );
                    assert!(
                        (y - expected.y).abs() < 0.001,
                        "beat={beat}, y={y}, expected={}",
                        expected.y
                    );
                }
            }
        }
    }

    #[test]
    fn multitap_travel_obeys_chart_timing_and_speedmod() {
        use deadsync_rules::timing::{
            ScrollSegment, SpeedSegment, SpeedUnit, TimingData, TimingSegments,
        };
        let mut context = SongLuaCompileContext::new(".", "Multitap timing");
        context.player_timing[0] = Some(TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0), (8.0, 240.0)],
                scrolls: vec![
                    ScrollSegment {
                        beat: 0.0,
                        ratio: 0.5,
                    },
                    ScrollSegment {
                        beat: 8.0,
                        ratio: 0.25,
                    },
                ],
                speeds: vec![
                    SpeedSegment {
                        beat: 0.0,
                        ratio: 0.5,
                        delay: 0.0,
                        unit: SpeedUnit::Beats,
                    },
                    SpeedSegment {
                        beat: 4.0,
                        ratio: 1.0,
                        delay: 4.0,
                        unit: SpeedUnit::Beats,
                    },
                ],
                ..Default::default()
            },
            &[],
        ));
        context.players[0].display_bpms = [120.0, 240.0];
        for (speed, rate, expected) in [
            (SongLuaSpeedMod::X(1.0), 1.0, 72.0),
            (SongLuaSpeedMod::X(2.0), 1.5, 144.0),
            (SongLuaSpeedMod::M(480.0), 2.0, 72.0),
            (SongLuaSpeedMod::C(120.0), 1.0, 192.0),
            (SongLuaSpeedMod::C(120.0), 2.0, 96.0),
        ] {
            context.players[0].speedmod = speed;
            context.song_music_rate = rate;
            assert!((multitap_y_offset(&context, 0, 6.0, 4.0) - expected).abs() < 0.001);
            let lua = Lua::new();
            let globals = lua.globals();
            globals
                .set(
                    crate::SONG_LUA_RUNTIME_KEY,
                    crate::create_song_runtime_table(&lua, &context).unwrap(),
                )
                .unwrap();
            crate::set_compile_song_runtime_values(&lua, 6.0, 3.0).unwrap();
            globals
                .set(
                    "ArrowEffects",
                    crate::host::create_arrow_effects_table(&lua, &context, |_| "single".into())
                        .unwrap(),
                )
                .unwrap();
            let (method, value) = match speed {
                SongLuaSpeedMod::X(value) => ("XMod", value),
                SongLuaSpeedMod::M(value) => ("MMod", value),
                SongLuaSpeedMod::C(value) => ("CMod", value),
                _ => unreachable!(),
            };
            globals.set("speed_method", method).unwrap();
            globals.set("speed_value", value).unwrap();
            let actual: f32 = lua
                .load(
                    r#"
                local options = {__songlua_reference_bpm=240}
                options[speed_method] = function() return speed_value end
                local ps = {GetPlayerNumber=function() return "PlayerNumber_P1" end,
                    GetPlayerOptions=function() return options end}
                return ArrowEffects.GetYOffset(ps, 1, 10) - ArrowEffects.GetYOffset(ps, 1, 6)
            "#,
                )
                .eval()
                .unwrap();
            assert!(
                (actual - expected).abs() < 0.001,
                "Lua ArrowEffects {speed:?}"
            );
        }
    }

    #[test]
    fn explosion_eases_ignore_other_lanes_and_merge_overlapping_visibility() {
        let descs = [
            MultitapDesc {
                lane: 3,
                taps: vec![1.0, 2.0],
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
        let baseline = SongLuaOverlayState {
            visible: false,
            ..Default::default()
        };
        let context = SongLuaCompileContext::new(".", "Multitap visibility");
        let mut actual = Vec::new();
        let mut expected = Vec::new();
        push_multitap_explosion_eases(&mut actual, 7, baseline, &context, &descs, 3);
        push_multitap_explosion_eases(&mut expected, 7, baseline, &context, &descs[2..], 3);
        assert!(!actual.is_empty());
        assert_eq!(actual, expected);
    }
}

#[cfg(test)]
#[path = "../tests/perf/multitap_work.rs"]
mod multitap_work_perf;

#[cfg(test)]
#[path = "../tests/perf/multitap_compile.rs"]
mod multitap_compile_perf;
