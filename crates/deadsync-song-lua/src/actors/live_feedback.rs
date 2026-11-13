//! Player::Update feedback, after its actor subtree and before song callbacks.
use super::*;
use sprites::number;

pub(super) type Statistics = Arc<Mutex<super::super::statistics::State>>;

pub(super) fn update(
    call: &mut HostCall<'_>,
    statistics: &Statistics,
    catalog: TableId,
    actor: TableId,
) -> Result<(), Error> {
    holds(call, statistics, actor)?;
    judgment(call, statistics, actor)?;
    note_field_zoom(call, catalog, actor)
}

fn raw(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_get(actor, key)
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}
fn index(call: &mut HostCall<'_>, value: Value) -> Result<usize, Error> {
    let n = number(call, value)?;
    if !n.is_finite() || n.trunc() < 0. || n.trunc() >= usize::MAX as f64 {
        return Err(call.error(b"player or sprite state index out of range"));
    }
    // The existing Lua 5.1 integer conversion truncates numeric arguments.
    Ok(n as usize)
}
fn player(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<usize>, Error> {
    let value = raw(call, actor, "__songlua_player_index")?;
    if value == Value::Nil {
        Ok(None)
    } else {
        index(call, value).map(Some)
    }
}
fn optional_float(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
) -> Result<Option<f32>, Error> {
    let value = raw(call, owner, key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        number(call, value).map(|n| Some(n as f32))
    }
}
fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}
fn child(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<Value, Error> {
    native_children::named_child(call, actor, name.as_bytes())
}
fn invoke(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &str,
    args: &[Value],
) -> Result<Value, Error> {
    call.with_roots(args, |call| {
        let function = sprites::field(call, actor, name)?;
        if !matches!(function, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(format!("{name} function expected").as_bytes()));
        }
        let arguments: Vec<_> = std::iter::once(Value::Table(actor))
            .chain(args.iter().copied())
            .collect();
        Ok(call
            .call(function, &arguments)?
            .first()
            .copied()
            .unwrap_or(Value::Nil))
    })
}
fn numeric_method(
    call: &mut HostCall<'_>,
    actor: TableId,
    name: &str,
    n: f64,
) -> Result<(), Error> {
    invoke(call, actor, name, &[Value::Number(n)]).map(|_| ())
}

fn holds(call: &mut HostCall<'_>, statistics: &Statistics, actor: TableId) -> Result<(), Error> {
    let Some(player) = player(call, actor)? else {
        return Ok(());
    };
    let Some(value) = statistics
        .lock()
        .unwrap()
        .players
        .as_ref()
        .and_then(|p| p.get(player))
        .copied()
    else {
        return Ok(());
    };
    let frames = raw(call, actor, "__songlua_hold_judgments")?;
    let Some(frames) = optional_table(call, frames)? else {
        return Ok(());
    };
    call.with_roots(&[Value::Table(frames)], |call| {
        for render in value
            .hold_judgments
            .iter()
            .enumerate()
            .take(call.raw_table_len(frames)?)
        {
            let (column, Some(render)) = render else {
                continue;
            };
            let frame = call.raw_table_get(frames, Value::Number((column + 1) as f64))?;
            let frame = table(call, frame)?;
            call.with_roots(&[Value::Table(frame)], |call| {
                let sprite = child(call, frame, "HoldJudgment")?;
                let sprite = table(call, sprite)?;
                call.with_roots(&[Value::Table(sprite)], |call| {
                    if optional_float(call, sprite, "__songlua_last_hold_seconds")?
                        == Some(render.started_at_screen_s)
                    {
                        return Ok(());
                    }
                    song_replay::raw_set(
                        call,
                        sprite,
                        "__songlua_last_hold_seconds",
                        Value::Number(render.started_at_screen_s as f64),
                    )?;
                    let state = match render.result {
                        deadsync_rules::note::HoldResult::Held => 0.,
                        deadsync_rules::note::HoldResult::LetGo => 1.,
                        deadsync_rules::note::HoldResult::Missed => return Ok(()),
                    };
                    chronological::ensure(call, sprite)?;
                    for name in ["stoptweening", "stopeffect"] {
                        invoke(call, sprite, name, &[])?;
                    }
                    invoke(call, sprite, "xy", &[Value::Number(0.), Value::Number(0.)])?;
                    numeric_method(call, sprite, "setstate", state)?;
                    let white = sprites::vector_value(call, &[1.; 4])?;
                    call.with_roots(&[white], |call| {
                        invoke(call, sprite, "diffuse", &[white])?;
                        for (name, n) in [
                            ("zoom", 0.4),
                            ("linear", 0.3),
                            ("zoom", 0.5),
                            ("sleep", 0.5),
                            ("linear", 0.),
                        ] {
                            numeric_method(call, sprite, name, n)?;
                        }
                        call.raw_table_set(
                            table(call, white)?,
                            Value::Number(4.),
                            Value::Number(0.),
                        )?;
                        invoke(call, sprite, "diffuse", &[white]).map(|_| ())
                    })
                })
            })?;
        }
        Ok(())
    })
}

fn judgment(call: &mut HostCall<'_>, statistics: &Statistics, actor: TableId) -> Result<(), Error> {
    let Some(player) = player(call, actor)? else {
        return Ok(());
    };
    let Some(value) = statistics
        .lock()
        .unwrap()
        .players
        .as_ref()
        .and_then(|p| p.get(player))
        .copied()
    else {
        return Ok(());
    };
    let Some(render) = value.last_judgment else {
        return Ok(());
    };
    if optional_float(call, actor, "__songlua_last_judgment_seconds")?
        == Some(render.started_at_screen_s)
    {
        return Ok(());
    }
    song_replay::raw_set(
        call,
        actor,
        "__songlua_last_judgment_seconds",
        Value::Number(render.started_at_screen_s as f64),
    )?;
    let frame = child(call, actor, "Judgment")?;
    let Some(frame) = optional_table(call, frame)? else {
        return Ok(());
    };
    call.with_roots(&[Value::Table(frame)], |call| {
        let sprite = child(call, frame, "JudgmentWithOffsets")?;
        let Some(sprite) = optional_table(call, sprite)? else {
            return Ok(());
        };
        call.with_roots(&[Value::Table(sprite)], |call| {
            chronological::ensure(call, sprite)?;
            let globals = table(call, helper(call, 5)?)?;
            let sl = raw(call, globals, "SL")?;
            let sl = table(call, sl)?;
            let p = raw(call, sl, if player == 0 { "P1" } else { "P2" })?;
            let mods = raw(call, table(call, p)?, "ActiveModifiers")?;
            let mods = table(call, mods)?;
            call.with_roots(&[Value::Table(mods)], |call| {
                let states = invoke(call, sprite, "GetNumStates", &[])?;
                let states = index(call, states)?;
                let mut ix = deadsync_rules::judgment::judge_grade_ix(render.judgment.grade);
                if states == 7 || states == 14 {
                    if ix == 0 {
                        if truthy(raw(call, mods, "ShowFaPlusWindow")?)
                            && render.judgment.window
                                != Some(deadsync_rules::judgment::TimingWindow::W0)
                            && !value.autoplay
                        {
                            ix = 1;
                        }
                    } else {
                        ix += 1;
                    }
                }
                if states == 12 || states == 14 {
                    ix = ix * 2 + usize::from(render.judgment.time_error_ms >= 0.);
                }
                for name in ["finishtweening", "stopeffect"] {
                    invoke(call, sprite, name, &[])?;
                }
                invoke(call, sprite, "visible", &[Value::Boolean(true)])?;
                numeric_method(call, sprite, "setstate", ix as f64)?;
                if truthy(raw(call, mods, "JudgmentTilt")?) {
                    let offset = render.judgment.time_error_ms;
                    let rotation =
                        if render.judgment.grade == deadsync_rules::judgment::JudgeGrade::Miss {
                            0.
                        } else {
                            offset.signum()
                                * (offset.abs() / 1000.).min(0.050)
                                * 300.
                                * optional_float(call, mods, "TiltMultiplier")?.unwrap_or(1.)
                        };
                    numeric_method(call, sprite, "rotationz", rotation as f64)?;
                }
                for (name, n) in [
                    ("zoom", 0.8),
                    ("decelerate", 0.1),
                    ("zoom", 0.75),
                    ("sleep", 0.6),
                    ("accelerate", 0.2),
                    ("zoom", 0.),
                ] {
                    numeric_method(call, sprite, name, n)?;
                }
                Ok(())
            })
        })
    })
}

fn note_field_zoom(call: &mut HostCall<'_>, catalog: TableId, actor: TableId) -> Result<(), Error> {
    let Some(player) = player(call, actor)? else {
        return Ok(());
    };
    if player >= crate::LUA_PLAYERS {
        return Ok(());
    }
    let players = raw(call, catalog, "player_options")?;
    let levels = call.raw_table_get(table(call, players)?, Value::Number((player + 1) as f64))?;
    let options = call.raw_table_get(table(call, levels)?, Value::Number(4.))?;
    let state = raw(call, table(call, options)?, "__songlua_player_option_state")?;
    let mini = optional_table(call, state)?
        .map(|state| optional_float(call, state, "mini"))
        .transpose()?
        .flatten()
        .unwrap_or(0.);
    let field = child(call, actor, "NoteField")?;
    let Some(field) = optional_table(call, field)? else {
        return Ok(());
    };
    call.with_roots(&[Value::Table(field)], |call| {
        let zoom = 1. - mini * 0.5;
        for axis in [0, 1, 2] {
            // The native X/Y writes precede the authored OITG flag read.
            if axis == 2 && truthy(raw(call, actor, "__songlua_oitg_zoom_mode")?) {
                break;
            }
            let name = ["zoom_x", "zoom_y", "zoom_z"][axis];
            let guard = call.store();
            if let Some(node) = guard.actors.get_mut(&field) {
                let dest = node.destination_mut();
                match axis {
                    0 => dest.zoom_x = zoom,
                    1 => dest.zoom_y = zoom,
                    _ => dest.zoom_z = zoom,
                }
                let mut delta = crate::SongLuaOverlayStateDelta::default();
                match axis {
                    0 => delta.zoom_x = Some(zoom),
                    1 => delta.zoom_y = Some(zoom),
                    _ => delta.zoom_z = Some(zoom),
                }
                capture::write(call, field, delta, false);
            } else {
                song_replay::raw_set(
                    call,
                    field,
                    &format!("__songlua_state_{name}"),
                    Value::Number(zoom as f64),
                )?;
            }
        }
        Ok(())
    })
}
