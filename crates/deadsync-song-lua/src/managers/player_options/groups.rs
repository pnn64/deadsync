//! Chronological ModsGroup phases, with Lua owners held in traced captures.
use super::super::song_options;
use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Operation {
    Advance,
    Restart,
    ClearAttacks,
    Apply,
    RebuildAttacks,
}
#[derive(Clone)]
struct Binding {
    operation: Operation,
    clock: Arc<Mutex<i64>>,
}
pub(super) fn install(
    call: &mut HostCall<'_>,
    catalog: TableId,
    players: TableId,
    shared: [Value; 4],
) -> Result<(), Error> {
    let mut captures = shared.to_vec();
    for player in 1..=crate::LUA_PLAYERS {
        let levels = call.raw_table_get(players, Value::Number(player as f64))?;
        let levels = table(call, levels)?;
        for level in 1..=4 {
            captures.push(call.raw_table_get(levels, Value::Number(level as f64))?);
        }
    }
    let songs = raw(call, catalog, "song_options")?;
    let songs = table(call, songs)?;
    for level in 1..=4 {
        captures.push(call.raw_table_get(songs, Value::Number(level as f64))?);
    }
    let clock = Arc::new(Mutex::new(0));
    let operations = object(call, |call, operations| {
        for (name, operation) in [
            ("Advance", Operation::Advance),
            ("Restart", Operation::Restart),
            ("ClearAttacks", Operation::ClearAttacks),
            ("Apply", Operation::Apply),
            ("RebuildAttacks", Operation::RebuildAttacks),
        ] {
            let method = call.new_host_function_with_data(
                binding,
                &captures,
                Binding {
                    operation,
                    clock: clock.clone(),
                },
            )?;
            set(operations, name, method, call)?;
        }
        Ok(())
    })?;
    set(catalog, "option_operations", operations, call)
}
fn owners(call: &mut HostCall<'_>, start: usize) -> Result<[TableId; 4], Error> {
    Ok([
        table(call, call.captures()[start])?,
        table(call, call.captures()[start + 1])?,
        table(call, call.captures()[start + 2])?,
        table(call, call.captures()[start + 3])?,
    ])
}
fn binding(call: &mut HostCall<'_>) -> Result<(), Error> {
    let binding = call
        .function_data::<Binding>()
        .expect("option group operation")
        .clone();
    let players = [owners(call, 4)?, owners(call, 8)?];
    let songs = owners(call, 12)?;
    match binding.operation {
        Operation::Restart => {
            *binding.clock.lock().expect("option clock") = 0;
            for player in players {
                for owner in &player[1..] {
                    levels::assign(call, *owner, player[0])?;
                }
            }
            for owner in &songs[1..] {
                song_options::assign_fields(call, songs[0], *owner)?;
            }
            Ok(())
        }
        Operation::ClearAttacks => {
            for player in players {
                levels::assign(call, player[2], player[1])?;
            }
            song_options::assign_fields(call, songs[1], songs[2])
        }
        Operation::Apply | Operation::RebuildAttacks => {
            let player = enum_index(
                call,
                arg(call, 0),
                &["PlayerNumber_P1", "PlayerNumber_P2"],
                "PlayerNumber",
            )?;
            if matches!(binding.operation, Operation::Apply) {
                let level = enum_index(call, arg(call, 1), &song_options::LEVELS, "ModsLevel")?;
                let text = text(call, arg(call, 2))
                    .ok_or_else(|| call.error(b"Expected modifiers string"))?;
                for owner in &players[player][level..] {
                    parser::apply(call, *owner, &text)?;
                }
                for owner in &songs[level..] {
                    song_options::apply(call, *owner, &text)?;
                }
                Ok(())
            } else {
                let master = truthy(arg(call, 1));
                let texts = table(call, arg(call, 2))?;
                let mut modifiers = Vec::new();
                let mut i = 1;
                loop {
                    let value = call.raw_table_get(texts, Value::Number(i as f64))?;
                    if value == Value::Nil {
                        break;
                    }
                    modifiers.push(string(call, value)?);
                    i += 1;
                }
                levels::assign(call, players[player][2], players[player][1])?;
                if master {
                    song_options::assign_fields(call, songs[1], songs[2])?;
                }
                for text in modifiers {
                    parser::apply(call, players[player][2], &text)?;
                    if master {
                        song_options::apply(call, songs[2], &text)?;
                    }
                }
                Ok(())
            }
        }
        Operation::Advance => {
            let seconds = number(call, arg(call, 0), call.captures()[2])?;
            let explicit = arg(call, 1);
            let clock = binding.clock.lock().expect("option clock");
            let time = (seconds * 1_000_000.0).round() as i64;
            if !seconds.is_finite()
                || seconds < 0.0
                || seconds >= i64::MAX as f64 / 1_000_000.0
                || time < *clock
            {
                return Err(call.error(b"native option clock moved backwards"));
            }
            let elapsed = ((time - *clock) as f64 / 1_000_000.0) as f32;
            // Convert explicit deltas after the chronological validation and
            // release the Rust clock before any callback can reenter it.
            drop(clock);
            let deltas = if explicit == Value::Nil {
                [elapsed; crate::LUA_PLAYERS + 1]
            } else {
                let values = table(call, explicit)?;
                let mut deltas = [0.0; crate::LUA_PLAYERS + 1];
                for (i, delta) in deltas.iter_mut().enumerate() {
                    let value = call.raw_table_get(values, Value::Number((i + 1) as f64))?;
                    *delta = number(call, value, call.captures()[2])? as f32;
                }
                deltas
            };
            if deltas
                .iter()
                .any(|delta| !delta.is_finite() || *delta < 0.0)
            {
                return Err(call.error(b"invalid native option frame delta"));
            }
            *binding.clock.lock().expect("option clock") = time;
            song_options::advance(call, songs, deltas[0], call.captures()[2])?;
            let name = call.intern_static(b"PREFSMAN")?;
            let prefs = call.table_get(call.captures()[1], name)?;
            table(call, prefs)?;
            let rate_tweens = call.with_roots(&[prefs], |call| {
                let name = call.intern_static(b"GetPreference")?;
                let method = call.table_get(prefs, name)?;
                let method = function(call, method)?;
                let key = call.intern_static(b"RateModsAffectFGChanges")?;
                Ok(truthy(
                    call.call(method, &[prefs, key])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil),
                ))
            })?;
            let rate = if rate_tweens {
                let value = raw(call, songs[3], "__songlua_music_rate")?;
                number(call, value, call.captures()[2])? as f32
            } else {
                1.0
            };
            for (i, player) in players.into_iter().enumerate() {
                levels::advance(call, player, deltas[i + 1], rate)?;
            }
            Ok(())
        }
    }
}
