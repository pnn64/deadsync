use super::*;

pub(super) fn perspective(
    call: &mut HostCall<'_>,
    state: TableId,
    key: &str,
    value: f32,
) -> Result<bool, Error> {
    let (tilt, skew) = match key {
        "incoming" => (-value, value),
        "space" => (value, value),
        "hallway" => (-value, 0.0),
        "distant" => (value, 0.0),
        "overhead" => (0.0, 0.0),
        _ => return Ok(false),
    };
    put(call, state, "tilt", Value::Number(tilt as f64))?;
    put(call, state, "skew", Value::Number(skew as f64))?;
    Ok(true)
}
fn previous(
    call: &mut HostCall<'_>,
    state: TableId,
    speeds: TableId,
    key: &str,
) -> Result<[Value; 2], Error> {
    if !matches!(
        key,
        "overhead" | "incoming" | "space" | "hallway" | "distant" | "tilt" | "skew"
    ) {
        return Ok([
            Value::Number(get_float(call, state, key, 0.0)? as f64),
            Value::Number(get_float(call, speeds, key, 1.0)? as f64),
        ]);
    }
    let tilt = get_float(call, state, "tilt", 0.0)?;
    let skew = get_float(call, state, "skew", 0.0)?;
    let (value, key) = match key {
        "overhead" => return Ok([Value::Boolean(tilt == 0.0 && skew == 0.0), Value::Nil]),
        "incoming" if (skew > 0.0 && tilt < 0.0) || (skew < 0.0 && tilt > 0.0) => (skew, "skew"),
        "space" if (skew > 0.0 && tilt > 0.0) || (skew < 0.0 && tilt < 0.0) => (skew, "skew"),
        "hallway" if skew == 0.0 && tilt < 0.0 => (-tilt, "tilt"),
        "distant" if skew == 0.0 && tilt > 0.0 => (tilt, "tilt"),
        "tilt" => (tilt, "tilt"),
        "skew" => (skew, "skew"),
        _ => return Ok([Value::Nil; 2]),
    };
    Ok([
        Value::Number(value as f64),
        Value::Number(get_float(call, speeds, key, 1.0)? as f64),
    ])
}
fn keep_speed(call: &mut HostCall<'_>, speeds: TableId, key: &str) -> Result<(), Error> {
    let value = raw(call, speeds, key)?;
    if value == Value::Nil {
        set(speeds, key, 1.0_f32, call)
    } else {
        typed_float(call, value, 1.0).map(|_| ())
    }
}
pub(super) fn invoke(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<(), Error> {
    if matches!(
        key,
        "timespacing" | "scrollspeed" | "scrollbpm" | "maxscrollbpm"
    ) {
        return speed::field(call, owner, key);
    }
    if key != "failsetting"
        && key != "modtimersetting"
        && (player_option_default_string(key).is_some() || key == "batterylives")
    {
        return compatibility(call, owner, key);
    }
    let state = nested(call, owner, STATE)?;
    call.with_roots(&[Value::Table(state)], |call| {
        if key == "failsetting" {
            return fail(call, owner, state);
        }
        if key == "modtimersetting" {
            return timer(call, owner, state);
        }
        if player_option_uses_bool(key) && key != "overhead" {
            let previous = truthy(get(call, Value::Table(state), key)?);
            let chain = matches!(arg(call, 1), Value::Boolean(_));
            if let Value::Boolean(value) = arg(call, 0) {
                put(call, state, key, Value::Boolean(value))?;
                #[cfg(feature = "test-support")]
                audit(call).boolean(
                    call,
                    call.captures()[1],
                    owner,
                    observations::Boolean {
                        key,
                        previous,
                        current: value,
                        chained: chain,
                    },
                )?;
            }
            return call.push(if chain {
                Value::Table(owner)
            } else {
                Value::Boolean(previous)
            });
        }
        let speeds = nested(call, owner, SPEEDS)?;
        call.with_roots(&[Value::Table(speeds)], |call| {
            let previous = previous(call, state, speeds, key)?;
            if key == "overhead" {
                if truthy(arg(call, 0)) {
                    perspective(call, state, key, 0.0)?;
                }
            } else if let Some(value) = float(call, arg(call, 0)) {
                if perspective(call, state, key, value)? {
                    keep_speed(call, speeds, "tilt")?;
                    keep_speed(call, speeds, "skew")?;
                } else {
                    put(call, state, key, Value::Number(value as f64))?;
                    keep_speed(call, speeds, key)?;
                }
            }
            if let Some(value) = float(call, arg(call, 1)) {
                if value < 0.0 {
                    return Err(call.error(b"Arg must be greater than or equal to zero."));
                }
                if matches!(
                    key,
                    "incoming" | "space" | "hallway" | "distant" | "overhead"
                ) {
                    put(call, speeds, "tilt", Value::Number(value as f64))?;
                    put(call, speeds, "skew", Value::Number(value as f64))?;
                } else {
                    put(call, speeds, key, Value::Number(value as f64))?;
                }
            }
            if chained(call) {
                return call.push(Value::Table(owner));
            }
            call.push(previous[0])?;
            if key != "overhead" {
                call.push(previous[1])?;
            }
            Ok(())
        })
    })
}
fn compatibility(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<(), Error> {
    let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
    let speed = call.arguments().get(2).copied().unwrap_or(Value::Nil);
    // Fixed-tuple argument conversion happens before entering the native body.
    let speed = if speed == Value::Nil {
        None
    } else {
        Some(typed_float(call, speed, 1.0)?)
    };
    let state = nested(call, owner, STATE)?;
    call.with_roots(&[Value::Table(state)], |call| {
        if value == Value::Nil {
            let value = get(call, Value::Table(state), key)?;
            let value = if value == Value::Nil {
                default_value(call, key)?
            } else {
                value
            };
            return call.push(value);
        }
        let value = if player_option_default_string(key).is_some() {
            if matches!(value, Value::String(_)) {
                value
            } else {
                default_value(call, key)?
            }
        } else {
            Value::Number(float(call, value).unwrap_or(0.0) as f64)
        };
        put(call, state, key, value)?;
        let speeds = nested(call, owner, SPEEDS)?;
        call.with_roots(&[Value::Table(speeds)], |call| {
            let previous = get_float(call, speeds, key, 1.0)?;
            put(
                call,
                speeds,
                key,
                Value::Number(speed.unwrap_or(previous).max(0.0) as f64),
            )
        })?;
        call.push(Value::Table(owner))
    })
}
fn fail(call: &mut HostCall<'_>, owner: TableId, state: TableId) -> Result<(), Error> {
    use deadsync_gameplay::GameplayFailType;
    let previous = get_float(call, state, "failsetting", 0.0)?;
    let value = arg(call, 0);
    if value != Value::Nil {
        let mode = if matches!(value, Value::String(_)) {
            let name = string(call, value)?;
            (0..4)
                .filter_map(|i| GameplayFailType::from_value(i as f32))
                .find(|mode| {
                    name == mode.lua_name() || name.eq_ignore_ascii_case(&mode.lua_name()[9..])
                })
        } else {
            float(call, value).and_then(GameplayFailType::from_value)
        }
        .ok_or_else(|| call.error(b"Invalid FailType"))?;
        put(call, state, "failsetting", Value::Number(mode as u8 as f64))?;
    }
    let result = if chained(call) {
        Value::Table(owner)
    } else {
        let mode = GameplayFailType::from_value(previous)
            .ok_or_else(|| call.error(b"Invalid FailType"))?;
        call.try_intern(mode.lua_name().as_bytes())?
    };
    call.push(result)
}
fn timer(call: &mut HostCall<'_>, owner: TableId, state: TableId) -> Result<(), Error> {
    let previous = get(call, Value::Table(state), "modtimersetting")?;
    let previous = if previous == Value::Nil {
        3.0
    } else {
        number(call, previous, call.captures()[2])?
    };
    if !previous.is_finite() || previous <= -1.0 || previous >= 256.0 {
        return Err(call.error(b"unsigned option value out of range"));
    }
    let value = arg(call, 0);
    if value != Value::Nil {
        let index = enum_index(
            call,
            value,
            &crate::player_options::MOD_TIMER_NAMES,
            "ModTimerType",
        )
        .map_err(|_| call.error(b"Invalid ModTimerType"))?;
        put(call, state, "modtimersetting", Value::Number(index as f64))?;
    }
    let result = if chained(call) {
        Value::Table(owner)
    } else if let Some(name) = crate::player_options::MOD_TIMER_NAMES.get(previous as usize) {
        call.try_intern(name.as_bytes())?
    } else {
        Value::Nil
    };
    call.push(result)
}
