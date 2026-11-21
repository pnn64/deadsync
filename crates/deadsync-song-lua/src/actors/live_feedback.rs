//! Player::Update feedback, after its actor subtree and before song callbacks.
use super::*;
use sprites::number;

pub(super) type Statistics = Arc<Mutex<super::super::statistics::State>>;

pub(super) const NOTE_FIELD_CATALOG_KEY: &str = "native_note_field_catalog";

pub(super) fn note_field_catalog(call: &mut HostCall<'_>) -> Result<Option<TableId>, Error> {
    let helpers = table(call, call.captures()[1])?;
    let value = get_static(call, helpers, NOTE_FIELD_CATALOG_KEY.as_bytes())?;
    optional_table(call, value)
}

pub(super) fn attached_player(
    call: &mut HostCall<'_>,
    top: TableId,
    index: usize,
) -> Result<Option<TableId>, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let key = [
        "__songlua_top_screen_player_1",
        "__songlua_top_screen_player_2",
    ][index];
    let actor = raw(call, globals, key)?;
    let Some(actor) = optional_table(call, actor)? else {
        return Ok(None);
    };
    // Disabled players retain private compatibility handles but are not drawn.
    Ok(call
        .store_ref()
        .actors
        .get(&top)
        .is_some_and(|screen| screen.children.contains(&actor))
        .then_some(actor))
}

pub(super) fn startup_note_fields(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Some(catalog) = note_field_catalog(call)? else {
        return Ok(());
    };
    let globals = table(call, helper(call, 5)?)?;
    let top = get_static(call, globals, b"__songlua_top_screen")?;
    let Some(top) = optional_table(call, top)? else {
        return Ok(());
    };
    for index in 0..crate::LUA_PLAYERS {
        if let Some(actor) = attached_player(call, top, index)? {
            note_field_zoom(call, catalog, actor)?;
        }
    }
    Ok(())
}

pub(super) fn analytic_multitap_fields(call: &mut HostCall<'_>) -> Result<bool, Error> {
    let Some(catalog) = note_field_catalog(call)? else {
        return Ok(true);
    };
    let players = raw(call, catalog, "player_options")?;
    let players = table(call, players)?;
    let globals = table(call, helper(call, 5)?)?;
    let top = raw(call, globals, "__songlua_top_screen")?;
    let top = optional_table(call, top)?;
    for player in 0..crate::LUA_PLAYERS {
        if let Some(top) = top
            && attached_player(call, top, player)?.is_none()
        {
            continue;
        }
        let levels = call.raw_table_get(players, Value::Number((player + 1) as f64))?;
        let levels = table(call, levels)?;
        // The analytic path bakes neutral ArrowEffects and the startup parent.
        // Nonneutral or approaching field mods need the actual update/draw phases.
        for level in [3., 4.] {
            let options = call.raw_table_get(levels, Value::Number(level))?;
            let state = raw(call, table(call, options)?, "__songlua_player_option_state")?;
            let state = table(call, state)?;
            for key in [
                "mini",
                "tilt",
                "skew",
                "reverse",
                "reverse1",
                "split",
                "alternate",
                "cross",
                "centered",
            ] {
                if optional_float(call, state, key)?.unwrap_or(0.) != 0. {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(super) fn update(
    call: &mut HostCall<'_>,
    statistics: &Statistics,
    catalog: TableId,
    actor: TableId,
) -> Result<(), Error> {
    holds(call, statistics, actor)?;
    judgment(call, statistics, actor)?;
    hud_position(call, catalog, actor)?;
    note_field_zoom(call, catalog, actor)
}

fn raw(call: &mut HostCall<'_>, actor: TableId, name: &'static str) -> Result<Value, Error> {
    get_static(call, actor, name.as_bytes())
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
    key: &'static str,
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
        let function = sprites::field_named(call, actor, name)?;
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

fn current_option_state(
    call: &mut HostCall<'_>,
    catalog: TableId,
    player: usize,
) -> Result<Option<TableId>, Error> {
    let players = raw(call, catalog, "player_options")?;
    let levels = call.raw_table_get(table(call, players)?, Value::Number((player + 1) as f64))?;
    let options = call.raw_table_get(table(call, levels)?, Value::Number(4.))?;
    let state = raw(call, table(call, options)?, "__songlua_player_option_state")?;
    optional_table(call, state)
}

pub(super) fn hud_position(
    call: &mut HostCall<'_>,
    catalog: TableId,
    actor: TableId,
) -> Result<(), Error> {
    let Some(player) = player(call, actor)?.filter(|i| *i < crate::LUA_PLAYERS) else {
        return Ok(());
    };
    let judgment = child(call, actor, "Judgment")?;
    let combo = child(call, actor, "Combo")?;
    if judgment == Value::Nil && combo == Value::Nil {
        return Ok(());
    }
    call.with_roots(&[judgment, combo], |call| {
        let state = current_option_state(call, catalog, player)?;
        let mut read = |key| {
            state
                .map(|state| optional_float(call, state, key))
                .transpose()
                .map(|value| value.flatten().unwrap_or(0.))
        };
        let reverse = read("reverse")? + read("reverse1")?;
        let split = read("split")?;
        let alternate = read("alternate")?;
        let cross = read("cross")?;
        let centered = read("centered")?;
        let mini = read("mini")?;
        let tiny = read("tiny")?;
        let slots = raw(call, catalog, "game_state_slots")?;
        let slots = table(call, slots)?;
        let style = raw(call, slots, "style")?;
        let style = table(call, style)?;
        let style = call.raw_table_get(style, Value::Number(1.))?;
        let style = table(call, style)?;
        let style = raw(call, style, "__songlua_style_key")?;
        let style = assets::text(call, style)?;
        let columns = crate::song_lua_style_info(&style).columns;
        let reverse = deadsync_gameplay::scroll_reverse_percent_for_column(
            deadsync_gameplay::ScrollReverseOptions {
                reverse,
                split,
                alternate,
                cross,
            },
            0,
            columns,
        ) == 1.;

        let scale = 0.5_f32.powf(mini + tiny);
        for (value, standard_y) in [(judgment, -30.0), (combo, 30.0)] {
            let Some(frame) = optional_table(call, value)? else {
                continue;
            };
            // The supported Simply Love HUD has identical Centered endpoints.
            // Player::Update still interpolates complete TweenStates and uses
            // an exact full-Reverse comparison, not a halfway threshold.
            let transform = TweenPose::from(crate::SongLuaOverlayState {
                y: if reverse { -standard_y } else { standard_y },
                ..Default::default()
            });
            let mut destination = transform.lerp(transform, centered);
            let zoom = destination.zoom_x * if 1. < scale { 1. } else { scale };
            destination.zoom = zoom;
            destination.zoom_x = zoom;
            destination.zoom_y = zoom;
            destination.zoom_z = zoom;
            if let Some(node) = call.store().actors.get_mut(&frame) {
                // Native replaces the destination after this player's subtree
                // advances. Keep current pose and every queued tween intact.
                *node.destination_mut() = destination;
                node.sync_rotation_getters();
            }
        }
        Ok(())
    })
}

pub(super) fn note_field_zoom(
    call: &mut HostCall<'_>,
    catalog: TableId,
    actor: TableId,
) -> Result<(), Error> {
    let Some(player) = player(call, actor)? else {
        return Ok(());
    };
    if player >= crate::LUA_PLAYERS {
        return Ok(());
    }
    let mini = current_option_state(call, catalog, player)?
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
        write_note_field_zoom(call, actor, field, zoom)
    })
}

pub(super) fn draw_note_field(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let Some(catalog) = note_field_catalog(call)? else {
        return Ok(());
    };
    let Some(player) = player(call, actor)?.filter(|index| *index < crate::LUA_PLAYERS) else {
        return Ok(());
    };
    let Some(state) = current_option_state(call, catalog, player)? else {
        return Ok(());
    };
    let mini = optional_float(call, state, "mini")?.unwrap_or(0.);
    let tilt = optional_float(call, state, "tilt")?.unwrap_or(0.);
    let reverse = optional_float(call, state, "reverse")?.unwrap_or(0.)
        + optional_float(call, state, "reverse1")?.unwrap_or(0.);
    let split = optional_float(call, state, "split")?.unwrap_or(0.);
    let alternate = optional_float(call, state, "alternate")?.unwrap_or(0.);
    let cross = optional_float(call, state, "cross")?.unwrap_or(0.);
    let style = raw(call, actor, "__songlua_style_name")?;
    let style = assets::text(call, style)?;
    let reverse = deadsync_gameplay::scroll_reverse_percent_for_column(
        deadsync_gameplay::ScrollReverseOptions {
            reverse,
            split,
            alternate,
            cross,
        },
        0,
        crate::song_lua_style_info(&style).columns,
    ) > 0.5;
    let field = child(call, actor, "NoteField")?;
    let Some(field) = optional_table(call, field)? else {
        return Ok(());
    };
    call.with_roots(&[Value::Table(field)], |call| {
        // PlayerNoteFieldPositioner restores only Y. These destination writes
        // survive the draw; Player::Update resets only zoom before callbacks.
        // The renderer already owns the temporary Y shift and field camera.
        // Keep the float operation order of RageUtil::SCALE, including 0.9f-1.f.
        let tilt_zoom = if tilt > 0. {
            tilt * (0.9_f32 - 1.) + 1.
        } else {
            tilt * (0.9_f32 - 1.) / -1. + 1.
        };
        let zoom = (mini * (0.5 - 1.) + 1.) * tilt_zoom;
        write_note_field_zoom(call, actor, field, zoom)?;
        let rotation =
            ((tilt + 1.) * (-30. - 30.) / (1. - -1.) + 30.) * if reverse { -1. } else { 1. };
        write_note_field_scalar(call, field, 3, rotation)
    })
}

fn write_note_field_zoom(
    call: &mut HostCall<'_>,
    actor: TableId,
    field: TableId,
    zoom: f32,
) -> Result<(), Error> {
    for axis in [0, 1, 2] {
        // The native X/Y writes precede the authored OITG flag read.
        if axis == 2 && truthy(raw(call, actor, "__songlua_oitg_zoom_mode")?) {
            break;
        }
        write_note_field_scalar(call, field, axis, zoom)?;
    }
    Ok(())
}

fn write_note_field_scalar(
    call: &mut HostCall<'_>,
    field: TableId,
    axis: usize,
    value: f32,
) -> Result<(), Error> {
    let raw_pose = {
        let store = call.store();
        let live = store.clock.live.load(std::sync::atomic::Ordering::Relaxed);
        if let Some(node) = store.actors.get_mut(&field) {
            let write = |pose: &mut TweenPose| match axis {
                0 => pose.zoom_x = value,
                1 => pose.zoom_y = value,
                2 => pose.zoom_z = value,
                _ => pose.rot_x_deg = value,
            };
            write(node.destination_mut());
            if let Some(getters) = node
                .replay
                .as_mut()
                .and_then(|replay| replay.getters.as_mut())
            {
                write(getters);
            }
            if axis == 3 {
                node.state.rotation[0] = value;
            }
            !live || node.raw_live_epoch
        } else {
            true
        }
    };
    // Native positioner writes are observable state, not authored command
    // blocks. In particular, they must never become another renderer wrapper.
    if raw_pose {
        let key = [
            "__songlua_state_zoom_x",
            "__songlua_state_zoom_y",
            "__songlua_state_zoom_z",
            "__songlua_state_rot_x_deg",
        ][axis];
        song_replay::raw_set(call, field, key, Value::Number(value as f64))?;
    }
    Ok(())
}
