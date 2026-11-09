//! GameState modifier queries and commands over retained native option levels.
use super::super::{game_state, song_options};
use super::*;
#[derive(Clone, Copy)]
enum Operation {
    Query,
    Stage,
    Command,
    Song,
}
pub(in crate::rust_engine::managers) fn install(
    call: &mut HostCall<'_>,
    class: TableId,
    owner: TableId,
    catalog: TableId,
) -> Result<(), Error> {
    let players = raw(call, catalog, "player_options")?;
    let players = table(call, players)?;
    let mut currents = Vec::new();
    for i in 1..=crate::LUA_PLAYERS {
        let levels = call.raw_table_get(players, Value::Number(i as f64))?;
        let levels = table(call, levels)?;
        currents.push(call.raw_table_get(levels, Value::Number(4.0))?);
    }
    let songs = raw(call, catalog, "song_options")?;
    let songs = table(call, songs)?;
    currents.push(call.raw_table_get(songs, Value::Number(4.0))?);
    let group = raw(call, catalog, "option_operations")?;
    let apply = get(call, group, "Apply")?;
    let song = raw(call, catalog, "song_option_operations")?;
    let assign = get(call, song, "Assign")?;
    for (name, op) in [
        ("PlayerIsUsingModifier", Operation::Query),
        ("ApplyStageModifiers", Operation::Stage),
        ("ApplyGameCommand", Operation::Command),
        ("SetSongOptions", Operation::Song),
    ] {
        let mut captures = vec![
            if matches!(op, Operation::Query) {
                Value::Nil
            } else {
                Value::Table(owner)
            },
            call.captures()[1],
            call.captures()[2],
            call.captures()[3],
        ];
        if matches!(op, Operation::Query) {
            captures.extend_from_slice(&currents);
        } else {
            captures.push(if matches!(op, Operation::Song) {
                assign
            } else {
                apply
            });
        }
        let function = call.new_host_function_with_data(binding, &captures, op)?;
        set(class, name, function, call)?;
    }
    Ok(())
}
fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = *call
        .function_data::<Operation>()
        .expect("GameState modifier operation");
    match op {
        Operation::Query => {
            let Some(player) = game_state::player(call, arg(call, 0)) else {
                return call.push(Value::Boolean(false));
            };
            let text = text(call, arg(call, 1)).unwrap_or_default();
            let owner = table(call, call.captures()[4 + player])?;
            let song = table(call, call.captures()[6])?;
            let value = query(call, owner, song, &text)?;
            call.push(Value::Boolean(value))
        }
        Operation::Song => {
            let args = [arg(call, 0), arg(call, 1)];
            call.call(call.captures()[4], &args)?;
            effect(call)?;
            call.push(call.captures()[0])
        }
        Operation::Stage => {
            let player = game_state::player(call, arg(call, 0))
                .ok_or_else(|| call.error(b"Expected PlayerNumber"))?;
            let text =
                text(call, arg(call, 1)).ok_or_else(|| call.error(b"Expected modifier string"))?;
            apply(call, player, 1, &text)?;
            effect(call)?;
            call.push(call.captures()[0])
        }
        Operation::Command => {
            let text = text(call, arg(call, 0))
                .ok_or_else(|| call.error(b"Expected game command string"))?;
            let value = arg(call, 1);
            let player = if value == Value::Nil {
                None
            } else {
                Some(
                    match value {
                        Value::Number(n) => (n as i64)
                            .checked_sub(1)
                            .and_then(|n| usize::try_from(n).ok())
                            .filter(|i| *i < crate::LUA_PLAYERS),
                        _ => game_state::player(call, value),
                    }
                    .ok_or_else(|| call.error(b"Expected PlayerNumber"))?,
                )
            };
            let mut start = 0;
            let mut quote = None;
            for (index, ch) in text
                .char_indices()
                .chain(std::iter::once((text.len(), ';')))
            {
                if index == text.len() || ch == ';' && quote.is_none() {
                    let clause = &text[start..index];
                    let (name, text) = clause.split_once(',').unwrap_or((clause, ""));
                    let level = match name.trim() {
                        "mod" => Some(0),
                        "stagemod" => Some(1),
                        _ => None,
                    };
                    if let Some(level) = level {
                        for index in 0..crate::LUA_PLAYERS {
                            if player.is_none_or(|p| p == index) {
                                apply(call, index, level, text)?;
                            }
                        }
                    }
                    start = index + 1;
                } else if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() && matches!(ch, '\'' | '"') {
                    quote = Some(ch);
                }
            }
            effect(call)?;
            call.push(call.captures()[0])
        }
    }
}
fn apply(call: &mut HostCall<'_>, player: usize, level: usize, text: &str) -> Result<(), Error> {
    let text = call.try_intern(text.as_bytes())?;
    call.call(
        call.captures()[4],
        &[
            Value::Number(player as f64),
            Value::Number(level as f64),
            text,
        ],
    )
    .map(|_| ())
}
fn query(
    call: &mut HostCall<'_>,
    owner: TableId,
    song: TableId,
    text: &str,
) -> Result<bool, Error> {
    let requested = call.try_new_table()?;
    call.with_roots(&[Value::Table(requested)], |call| {
        let mut skin = None;
        for token in text.split(',') {
            let token = crate::strip_player_option_prefix(token).trim();
            let lower = token.to_ascii_lowercase();
            if !lower.ends_with("xmusic") {
                if skin_exists(call, token)? {
                    skin = Some(lower);
                } else {
                    parser::token(call, requested, token)?;
                }
            }
        }
        let requested_song = levels::copy(call, song)?;
        call.with_roots(&[Value::Table(requested_song)], |call| {
            song_options::apply(call, requested_song, text)?;
            for key in song_options::SWITCHES
                .into_iter()
                .chain(["__songlua_music_rate", "__songlua_haste"])
            {
                if raw(call, song, key)? != raw(call, requested_song, key)? {
                    return Ok(false);
                }
            }
            if let Some(skin) = skin {
                let actual = raw(call, owner, SKIN)?;
                if !string(call, actual)?.eq_ignore_ascii_case(&skin) {
                    return Ok(false);
                }
            }
            for (field, default) in [
                ("__songlua_time_spacing", 0.0),
                ("__songlua_speedmod_xmod", 1.0),
                ("__songlua_speedmod_cmod", 200.0),
                ("__songlua_speedmod_mmod", 0.0),
            ] {
                let expected = raw(call, requested, field)?;
                if expected != Value::Nil {
                    let expected = typed_float(call, expected, 0.0)?;
                    if raw_float(call, owner, field, default)? != expected {
                        return Ok(false);
                    }
                }
            }
            let current = nested(call, owner, STATE)?;
            call.with_roots(&[Value::Table(current)], |call| {
                let state = nested(call, requested, STATE)?;
                let mut equal = true;
                levels::entries(call, state, |call, key, expected| {
                    let key = string(call, key)?;
                    if !equal
                        || !crate::SONG_LUA_PLAYER_OPTION_CAPABILITIES
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(&key))
                    {
                        return Ok(());
                    }
                    let actual = raw(call, current, &key)?;
                    let actual = if actual == Value::Nil {
                        if key == "modtimersetting" {
                            Value::Number(3.0)
                        } else {
                            default_value(call, &key)?
                        }
                    } else {
                        actual
                    };
                    equal = actual == expected;
                    Ok(())
                })?;
                Ok(equal)
            })
        })
    })
}
