//! Native PlayerOptions value copying, text output and current-value approach.
use super::*;
use crate::player_options::{
    OPTION_STRING_NAMES, SONG_LUA_PLAYER_OPTION_CAPABILITIES,
    SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES,
};

fn part(parts: &mut Vec<String>, name: &str, value: f32) {
    if value != 0.0 && value.is_finite() {
        parts.push(if value == 1.0 {
            name.into()
        } else {
            format!("{:.0}% {name}", (value * 100.0).round_ties_even())
        });
    }
}
fn index(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<Option<usize>, Error> {
    let value = raw(call, owner, key)?;
    if value == Value::Nil {
        return Ok(None);
    }
    let value = number(call, value, call.captures()[2])?;
    if value.is_finite() && value > -1.0 && value < usize::MAX as f64 + 1.0 {
        Ok(Some(value as usize))
    } else {
        Err(call.error(b"unsigned option value out of range"))
    }
}
pub(super) fn parts(call: &mut HostCall<'_>, owner: TableId) -> Result<Vec<String>, Error> {
    let state = nested(call, owner, STATE)?;
    call.with_roots(&[Value::Table(state)], |call| {
        let mut parts = vec!["NoHideLights".into()];
        let mode = raw(call, owner, "__songlua_speedmod_active")?;
        if mode != Value::Nil {
            let mode = string(call, mode)?;
            let value = raw(call, owner, &format!("__songlua_speedmod_{mode}"))?;
            let value = if value == Value::Nil {
                None
            } else {
                Some(typed_float(call, value, 0.0)?)
            };
            let explicit = raw(call, owner, "__songlua_speedmod_explicit")?;
            let explicit = explicit == Value::Nil || truthy(explicit);
            if let Some(value) = value.filter(|v| mode != "xmod" || *v != 1.0 || explicit) {
                parts.push(match mode.as_str() {
                    "cmod" => format!("C{value:.0}"),
                    "mmod" => format!("m{value:.0}"),
                    "amod" => format!("A{value:.0}"),
                    "camod" => format!("CA{value:.0}"),
                    _ => format!(
                        "{}x",
                        format!("{value:.2}")
                            .trim_end_matches('0')
                            .trim_end_matches('.')
                    ),
                });
            }
        }
        if let Some(mode) = index(call, state, "failsetting")?
            && let Some(name) = ["", "FailImmediateContinue", "FailAtEnd", "FailOff"].get(mode)
            && !name.is_empty()
        {
            parts.push((*name).into());
        }
        if let Some(mode) = index(call, state, "modtimersetting")?
            && let Some(name) = ["ModTimerGame", "ModTimerBeat", "ModTimerSong"].get(mode)
        {
            parts.push((*name).into());
        }
        let mut key = String::with_capacity(32);
        for &name in OPTION_STRING_NAMES {
            key.clear();
            key.push_str(match name {
                "RandomAttacks" => "randattack",
                "NoAttacks" => "noattack",
                _ => name,
            });
            key.make_ascii_lowercase();
            let value = raw(call, state, &key)?;
            let value = if let Value::Boolean(value) = value {
                f32::from(value)
            } else {
                float(call, value).unwrap_or(0.0)
            };
            part(&mut parts, name, value);
            if name == "Cosecant" {
                let mut name = String::with_capacity(32);
                let mut digits = itoa::Buffer::new();
                for col in 1..=16 {
                    let column = digits.format(col);
                    for prefix in SONG_LUA_PLAYER_OPTION_MULTICOL_PREFIXES {
                        name.clear();
                        name.push_str(prefix);
                        name.push_str(column);
                        key.clear();
                        key.push_str(&name);
                        key.make_ascii_lowercase();
                        let value = raw_float(call, state, &key, 0.0)?;
                        part(&mut parts, &name, value);
                    }
                }
            }
        }
        for name in SONG_LUA_PLAYER_OPTION_CAPABILITIES {
            key.clear();
            key.push_str(name);
            key.make_ascii_lowercase();
            if player_option_uses_bool(&key)
                && !OPTION_STRING_NAMES.contains(name)
                && truthy(raw(call, state, &key)?)
            {
                parts.push((*name).into());
            }
        }
        let tilt = raw_float(call, state, "tilt", 0.0)?;
        let skew = raw_float(call, state, "skew", 0.0)?;
        match (tilt, skew) {
            (0.0, 0.0) => parts.push("Overhead".into()),
            (tilt, 0.0) => part(
                &mut parts,
                if tilt > 0.0 { "Distant" } else { "Hallway" },
                tilt.abs(),
            ),
            (tilt, skew) if (tilt - skew).abs() < 0.0001 => part(&mut parts, "Space", skew),
            (tilt, skew) if (tilt + skew).abs() < 0.0001 => part(&mut parts, "Incoming", skew),
            _ => {
                part(&mut parts, "Skew", skew);
                part(&mut parts, "Tilt", tilt);
            }
        }
        let skin = raw(call, owner, SKIN)?;
        let skin = if skin == Value::Nil {
            String::new()
        } else {
            string(call, skin)?
        };
        if !skin.is_empty() && skin != crate::SONG_LUA_DEFAULT_NOTESKIN_NAME {
            let mut name = skin.clone();
            if let Some(first) = skin.chars().next() {
                let code = first as u32;
                let upper = match code {
                    0x61..=0x7a | 0xe0..=0xf6 | 0xf8..=0xfe => code - 0x20,
                    _ => code,
                };
                if let Some(upper) = char::from_u32(upper).filter(|upper| *upper != first) {
                    name.replace_range(..first.len_utf8(), &upper.to_string());
                }
            }
            parts.push(name);
        }
        Ok(parts)
    })
}

pub(super) fn entries(
    call: &mut HostCall<'_>,
    source: TableId,
    mut visit: impl FnMut(&mut HostCall<'_>, Value, Value) -> Result<(), Error>,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(source)], |call| {
        let mut key = Value::Nil;
        loop {
            let pair = call.call(call.captures()[3], &[Value::Table(source), key])?;
            key = pair.first().copied().unwrap_or(Value::Nil);
            if key == Value::Nil {
                break;
            }
            let value = pair.get(1).copied().unwrap_or(Value::Nil);
            call.with_roots(&[key, value], |call| visit(call, key, value))?;
        }
        Ok(())
    })
}
pub(super) fn copy(call: &mut HostCall<'_>, source: TableId) -> Result<TableId, Error> {
    object(call, |call, target| {
        entries(call, source, |call, k, v| call.raw_table_set(target, k, v))
    })
}
pub(super) fn assign(
    call: &mut HostCall<'_>,
    destination: TableId,
    source: TableId,
) -> Result<(), Error> {
    let skin = raw(call, destination, SKIN)?;
    let override_ = raw(call, destination, SKIN_OVERRIDE)?;
    call.with_roots(&[skin, override_, Value::Table(source)], |call| {
        parser::reset(call, destination)?;
        set(destination, SKIN, skin, call)?;
        set(destination, SKIN_OVERRIDE, override_, call)?;
        for key in [STATE, SPEEDS, WINDOWS] {
            if let Value::Table(value) = raw(call, source, key)? {
                let target = nested(call, destination, key)?;
                call.with_roots(&[Value::Table(target)], |call| {
                    entries(call, value, |call, k, v| call.raw_table_set(target, k, v))
                })?;
            }
        }
        for key in [
            "__songlua_time_spacing",
            "__songlua_speedmod_active",
            "__songlua_speedmod_explicit",
            "__songlua_speedmod_xmod",
            "__songlua_speedmod_cmod",
            "__songlua_speedmod_mmod",
            "__songlua_speedmod_amod",
            "__songlua_speedmod_camod",
        ] {
            let value = raw(call, source, key)?;
            set(destination, key, value, call)?;
        }
        let skin = raw(call, source, SKIN)?;
        if skin != Value::Nil && !string(call, skin)?.is_empty() {
            for key in [SKIN, SKIN_OVERRIDE] {
                let value = raw(call, source, key)?;
                set(destination, key, value, call)?;
            }
        }
        Ok(())
    })
}
pub(super) fn set_options(
    call: &mut HostCall<'_>,
    owners: [TableId; 4],
    level: usize,
    text: &str,
    #[cfg(feature = "test-support")] observations: &observations::Observations,
) -> Result<(), Error> {
    let replacement = object(call, |call, owner| {
        parser::reset(call, owner)?;
        parser::apply(call, owner, text)
    })?;
    call.with_roots(&[Value::Table(replacement)], |call| {
        #[cfg(feature = "test-support")]
        let previous = skin_name(call, owners[level])?;
        if level != 2 {
            assign(call, owners[3], replacement)?;
        }
        for owner in owners.iter().take(3).skip(level) {
            assign(call, *owner, replacement)?;
        }
        #[cfg(feature = "test-support")]
        observations.skin(
            call,
            call.captures()[1],
            owners[level],
            "setplayeroptions",
            previous,
        )?;
        effect(call)
    })
}
pub(super) fn advance(
    call: &mut HostCall<'_>,
    owners: [TableId; 4],
    delta: f32,
    rate: f32,
) -> Result<(), Error> {
    let current = nested(call, owners[3], STATE)?;
    let target = nested(call, owners[2], STATE)?;
    let speeds = nested(call, owners[2], SPEEDS)?;
    call.with_roots(
        &[
            Value::Table(current),
            Value::Table(target),
            Value::Table(speeds),
        ],
        |call| {
            let mut keys = smallvec::SmallVec::<[String; 16]>::new();
            for owner in [current, target] {
                entries(call, owner, |call, key, _| {
                    keys.push(string(call, key)?);
                    Ok(())
                })?;
            }
            keys.sort_unstable();
            keys.dedup();
            for key in keys {
                let value = raw(call, target, &key)?;
                let value = if value == Value::Nil {
                    default_value(call, &key)?
                } else {
                    value
                };
                if matches!(value, Value::Boolean(_) | Value::String(_))
                    || matches!(
                        key.as_str(),
                        "modtimersetting" | "failsetting" | "batterylives" | "visualdelay"
                    )
                {
                    set(current, key, value, call)?;
                } else if let Some(target) = float(call, value) {
                    let value = raw_float(call, current, &key, 0.0)?;
                    let speed = raw_float(call, speeds, &key, 1.0)?;
                    let value = super::super::song_options::approach(
                        call,
                        value,
                        target,
                        rate * delta * speed,
                    )?;
                    set(current, key, value, call)?;
                }
            }
            for (field, key, default) in [
                ("__songlua_time_spacing", "_spacing", 0.0),
                ("__songlua_speedmod_xmod", "xmod", 1.0),
                ("__songlua_speedmod_cmod", "cmod", 200.0),
                ("__songlua_speedmod_mmod", "mmod", 0.0),
            ] {
                let value = raw_float(call, owners[3], field, default)?;
                let target = raw_float(call, owners[2], field, default)?;
                let speed = raw_float(call, speeds, key, 1.0)?;
                let amount = if key == "cmod" {
                    delta * speed * 150.0
                } else {
                    rate * delta * speed
                };
                let value = super::super::song_options::approach(call, value, target, amount)?;
                set(owners[3], field, value, call)?;
            }
            let mode = if raw_float(call, owners[3], "__songlua_time_spacing", 0.0)? != 0.0 {
                "cmod"
            } else if raw_float(call, owners[3], "__songlua_speedmod_mmod", 0.0)? != 0.0 {
                "mmod"
            } else {
                "xmod"
            };
            set(owners[3], "__songlua_speedmod_active", mode, call)?;
            for key in [SKIN, SKIN_OVERRIDE] {
                let value = raw(call, owners[2], key)?;
                set(owners[3], key, value, call)?;
            }
            let windows = raw(call, owners[2], WINDOWS)?;
            let windows = if let Value::Table(windows) = windows {
                Value::Table(copy(call, windows)?)
            } else {
                windows
            };
            set(owners[3], WINDOWS, windows, call)
        },
    )
}
