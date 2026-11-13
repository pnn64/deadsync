//! Original gameplay underlay files and their native load/Init/message order.
use super::*;

const ENVIRONMENT: &[u8] = b"__songlua_step_statistics_environment";

fn invoke(
    call: &mut HostCall<'_>,
    owner: TableId,
    name: &str,
    args: &[Value],
) -> Result<Value, Error> {
    call.with_roots(&[Value::Table(owner)], |call| {
        let function = field(call, owner, name)?;
        let mut values = Vec::with_capacity(args.len() + 1);
        values.push(Value::Table(owner));
        values.extend_from_slice(args);
        Ok(call
            .call(function, &values)?
            .first()
            .copied()
            .unwrap_or(Value::Nil))
    })
}

pub(super) fn registry(call: &mut HostCall<'_>, actor: TableId) -> Result<Value, Error> {
    let function = native_children::helper_function(call, b"native_registry")?;
    Ok(call.call(function, &[Value::Table(actor)])?[0])
}

pub(super) fn named(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<TableId, Error> {
    let children = registry(call, actor)?;
    call.with_roots(&[children], |call| {
        let value = get(call, table(call, children)?, name.as_bytes())?;
        table(call, value)
    })
}

fn environment(call: &mut HostCall<'_>) -> Result<Value, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let existing = get(call, globals, ENVIRONMENT)?;
    if existing != Value::Nil {
        table(call, existing)?;
        return Ok(existing);
    }
    let environment = source::environment(call, Value::Table(globals), true)?;
    call.with_roots(&[environment], |call| {
        let key = call.try_intern(ENVIRONMENT)?;
        call.raw_table_set(globals, key, environment)?;
        let captures = call.captures().to_vec();
        let loader = call.new_host_function(step_resource, &captures)?;
        set(call, table(call, environment)?, "LoadActor", loader)?;
        source::evaluate(call, source::HISTOGRAM, environment, &[])?;
        Ok(environment)
    })
}

fn step_resource(call: &mut HostCall<'_>) -> Result<(), Error> {
    let path = assets::text(call, arg(call, 0))
        .map_err(|_| call.error(b"native LoadActor requires a path"))?;
    let globals = table(call, helper(call, 5)?)?;
    let environment = get(call, globals, ENVIRONMENT)?;
    table(call, environment)?;
    let args = call.arguments().iter().skip(1).copied().collect::<Vec<_>>();
    let value = step(call, environment, &path, &args)?;
    call.push(value)
}

fn step(
    call: &mut HostCall<'_>,
    environment: Value,
    path: &str,
    args: &[Value],
) -> Result<Value, Error> {
    let name = path.strip_prefix("./").unwrap_or(path);
    let source = source::statistics(name).ok_or_else(|| {
        call.error(format!("unknown native StepStatistics resource {path}").as_bytes())
    })?;
    source::evaluate(call, source, environment, args)
}

fn note_field(call: &mut HostCall<'_>, path: &str, args: &[Value]) -> Result<Value, Error> {
    let source = source::note_field(path)
        .ok_or_else(|| call.error(format!("unknown native NoteField actor {path}").as_bytes()))?;
    let globals = helper(call, 5)?;
    let environment = source::environment(call, globals, true)?;
    call.with_roots(&[environment], |call| {
        let directory = path
            .rsplit_once('/')
            .map_or_else(String::new, |(parent, _)| format!("{parent}/"));
        let captures = call.captures().to_vec();
        let loader = call.new_host_function_with_data(note_resource, &captures, directory)?;
        set(call, table(call, environment)?, "LoadActor", loader)?;
        source::evaluate(call, source, environment, args)
    })
}

fn note_resource(call: &mut HostCall<'_>) -> Result<(), Error> {
    let directory = call.function_data::<String>().unwrap().clone();
    let child = assets::text(call, arg(call, 0))
        .map_err(|_| call.error(b"native NoteField LoadActor requires a path"))?;
    let args = call.arguments().iter().skip(1).copied().collect::<Vec<_>>();
    let path = format!("{directory}{}", child.trim_start_matches("./"));
    let value = note_field(call, &path, &args)?;
    call.push(value)
}

fn player(call: &HostCall<'_>, value: Value) -> Option<usize> {
    match value {
        Value::Number(value)
            if value >= 0. && value < crate::LUA_PLAYERS as f64 && value.fract() == 0. =>
        {
            Some(value as usize)
        }
        Value::String(id) => {
            let bytes = call.string_bytes(id)?;
            [b"P1".as_slice(), b"P2"]
                .iter()
                .position(|name| bytes.eq_ignore_ascii_case(name))
                .or_else(|| {
                    [b"PlayerNumber_P1".as_slice(), b"PlayerNumber_P2"]
                        .iter()
                        .position(|name| bytes.eq_ignore_ascii_case(name))
                })
        }
        _ => None,
    }
}

fn attach(
    call: &mut HostCall<'_>,
    data: &Load,
    underlay: TableId,
    definition: Value,
    lookup: bool,
    name: Option<&str>,
) -> Result<TableId, Error> {
    let actor = own(call, data, definition)?;
    parent(call, actor, underlay)?;
    initialize(call, actor)?;
    if lookup {
        owned_lookup(call, actor)?;
    }
    let name = match name {
        Some(name) => name.as_bytes().to_vec(),
        None => actor_name(call, actor)?,
    };
    let children = registry(call, underlay)?;
    call.with_roots(&[children], |call| {
        let key = call.try_intern(&name)?;
        call.raw_table_set(table(call, children)?, key, Value::Table(actor))
    })?;
    let count = call.raw_table_len(underlay)?;
    let mut found = false;
    for index in 1..=count {
        found |= call.raw_table_get(underlay, Value::Number(index as f64))? == Value::Table(actor);
    }
    if !found {
        call.raw_table_set(
            underlay,
            Value::Number((count + 1) as f64),
            Value::Table(actor),
        )?;
        call.store()
            .actors
            .get_mut(&underlay)
            .unwrap()
            .children
            .push(actor);
    }
    Ok(actor)
}

// These theme phases call the raw Begin/On handlers directly, without the
// authored playcommand wrapper. Enumerate children after each parent callback.
pub(super) fn command(call: &mut HostCall<'_>, actor: TableId, name: &[u8]) -> Result<(), Error> {
    let callback = get(call, actor, name)?;
    if callback != Value::Nil {
        if !matches!(callback, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(b"theme actor command must be a function"));
        }
        call.call(callback, &[Value::Table(actor)])?;
    }
    native_children::with_children(call, actor, |call, child| command(call, child, name))
}

pub(super) fn load(call: &mut HostCall<'_>, data: &Load) -> Result<(), Error> {
    let environment = environment(call)?;
    call.with_roots(&[environment], |call| {
        let globals = table(call, helper(call, 5)?)?;
        let screen = get_static(call, globals, b"__songlua_top_screen")?;
        let screen = table(call, screen)?;
        let underlay = named(call, screen, "Underlay")?;
        let game = member(call, globals, "GAMESTATE")?;
        let players = invoke(call, game, "GetHumanPlayers", &[])?;
        table(call, players)?;
        call.with_roots(&[players], |call| {
            call.store().startup_queues.get_or_insert_with(Vec::new);
            source::evaluate(call, source::UTF8, Value::Table(globals), &[])?;
            source::evaluate(call, source::TEXT_HELPERS, Value::Table(globals), &[])?;
            steps_displays::initialize(call, screen)?;
            for index in 1.. {
                let value =
                    call.raw_table_get(table(call, players)?, Value::Number(index as f64))?;
                if value == Value::Nil {
                    break;
                }
                let Some(player) = player(call, value) else {
                    continue;
                };
                let child = named(call, screen, &format!("PlayerOptionsP{}", player + 1))?;
                let game = member(call, globals, "GAMESTATE")?;
                let state = invoke(call, game, "GetPlayerState", &[value])?;
                call.with_roots(&[state], |call| {
                    let level = call.try_intern(b"ModsLevel_Current")?;
                    let text = invoke(
                        call,
                        table(call, state)?,
                        "GetPlayerOptionsString",
                        &[level],
                    )?;
                    let text = assets::text(call, text)?;
                    let text = call.try_intern(text.as_bytes())?;
                    set(call, child, "Text", text)
                })?;
            }
            let mut panes = Vec::new();
            for index in 1.. {
                let player =
                    call.raw_table_get(table(call, players)?, Value::Number(index as f64))?;
                if player == Value::Nil {
                    break;
                }
                let danger =
                    source::evaluate(call, source::DANGER, Value::Table(globals), &[player])?;
                if danger != Value::Nil {
                    panes.push(attach(call, data, underlay, danger, false, None)?);
                }
                let pane = step(call, environment, "default.lua", &[player])?;
                if pane != Value::Nil {
                    panes.push(attach(call, data, underlay, pane, true, None)?);
                }
                let container = note_field(call, "default.lua", &[player])?;
                panes.push(attach(call, data, underlay, container, true, None)?);
            }
            let meter = source::evaluate(call, source::SONG_INFO, Value::Table(globals), &[])?;
            panes.push(attach(
                call,
                data,
                underlay,
                meter,
                true,
                Some("SongMeter"),
            )?);
            reorder(call, underlay)?;
            set(
                call,
                underlay,
                "__songlua_owned_child_lookup",
                Value::Boolean(true),
            )?;
            for &pane in &panes {
                command(call, pane, b"BeginCommand")?;
            }
            for &pane in &panes {
                command(call, pane, b"OnCommand")?;
            }
            // Keep normal listener delivery; this internal song-change event
            // does not record an authored Broadcast or increment its counter.
            messages::dispatch(call, b"CurrentSongChanged", None, false)
        })
    })
}

fn reorder(call: &mut HostCall<'_>, underlay: TableId) -> Result<(), Error> {
    const ORDER: &[&[u8]] = &[
        b"DangerP1",
        b"StepStatsPaneP1",
        b"filter0",
        b"DangerP2",
        b"StepStatsPaneP2",
        b"filter1",
        b"Header",
        b"SongMeter",
        b"P1Score",
        b"difficulty0",
        b"life0",
        b"NoteFieldContainerP1",
        b"P2Score",
        b"difficulty1",
        b"life1",
        b"NoteFieldContainerP2",
        b"bpm",
    ];
    let mut children = Vec::new();
    native_children::with_children(call, underlay, |call, child| {
        let name = actor_name(call, child).unwrap_or_default();
        let role = get_static(call, child, b"__songlua_underlay_role")
            .ok()
            .and_then(|value| {
                if value == Value::Nil {
                    None
                } else {
                    assets::text(call, value).ok()
                }
            });
        let name = role
            .as_ref()
            .map_or(name.as_slice(), |role| role.as_bytes());
        let rank = ORDER.iter().position(|&role| name == role).unwrap_or(99);
        children.push((rank, child));
        Ok(())
    })?;
    children.sort_by_key(|&(rank, _)| rank);
    for (index, &(_, child)) in children.iter().enumerate() {
        call.raw_table_set(
            underlay,
            Value::Number((index + 1) as f64),
            Value::Table(child),
        )?;
    }
    call.store().actors.get_mut(&underlay).unwrap().children =
        children.into_iter().map(|(_, child)| child).collect();
    Ok(())
}
