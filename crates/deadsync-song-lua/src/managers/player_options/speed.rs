use super::*;
const SPACING: &str = "__songlua_time_spacing";
const ACTIVE: &str = "__songlua_speedmod_active";
const EXPLICIT: &str = "__songlua_speedmod_explicit";
const KEYS: [&str; 4] = ["xmod", "cmod", "mmod", "_spacing"];
fn field_name(key: &str) -> &'static str {
    match key {
        "xmod" => "__songlua_speedmod_xmod",
        "cmod" => "__songlua_speedmod_cmod",
        "mmod" => "__songlua_speedmod_mmod",
        "amod" => "__songlua_speedmod_amod",
        "camod" => "__songlua_speedmod_camod",
        _ => unreachable!("speed mode keys come from bindings or the speed parser"),
    }
}
pub(super) fn set_mode(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    value: Option<f32>,
) -> Result<(), Error> {
    if let Some(value) = value {
        match key {
            "xmod" => {
                set(owner, SPACING, 0.0_f32, call)?;
                set(owner, field_name("cmod"), 200.0_f32, call)?;
                set(owner, field_name("mmod"), 0.0_f32, call)?;
            }
            "cmod" => {
                set(owner, SPACING, 1.0_f32, call)?;
                set(owner, field_name("xmod"), 1.0_f32, call)?;
                set(owner, field_name("mmod"), 0.0_f32, call)?;
            }
            "mmod" => {
                set(owner, SPACING, 0.0_f32, call)?;
                set(owner, field_name("xmod"), 1.0_f32, call)?;
                set(owner, field_name("cmod"), 200.0_f32, call)?;
            }
            _ => {}
        }
        set(owner, ACTIVE, key, call)?;
        set(owner, field_name(key), value, call)?;
        set(owner, EXPLICIT, Value::Nil, call)
    } else {
        set(owner, ACTIVE, "none", call)?;
        set(owner, field_name(key), Value::Nil, call)
    }
}
fn seed(call: &mut HostCall<'_>, owner: TableId) -> Result<(), Error> {
    let speeds = nested(call, owner, SPEEDS)?;
    for key in KEYS {
        if raw(call, speeds, key)? == Value::Nil {
            set(speeds, key, 1.0_f32, call)?;
        }
    }
    Ok(())
}
pub(super) fn approaches(call: &mut HostCall<'_>, owner: TableId, value: f32) -> Result<(), Error> {
    let speeds = nested(call, owner, SPEEDS)?;
    // The native Option::or evaluates the old xmod conversion even with Some.
    raw_float(call, speeds, "xmod", 1.0)?;
    for key in KEYS {
        set(speeds, key, value.max(0.0), call)?;
    }
    Ok(())
}
pub(super) fn mode(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<(), Error> {
    #[cfg(feature = "test-support")]
    let before = if audit(call).enabled()
        && (arg(call, 0) != Value::Nil || float(call, arg(call, 1)).is_some())
    {
        audit(call).speed_before(call, call.captures()[1], owner)?
    } else {
        None
    };
    let spacing = raw_float(call, owner, SPACING, 0.0)?;
    let max = raw_float(call, owner, field_name("mmod"), 0.0)?;
    let active = match key {
        "cmod" => spacing != 0.0,
        "mmod" => spacing == 0.0 && max != 0.0,
        _ => spacing == 0.0,
    };
    let speeds = nested(call, owner, SPEEDS)?;
    let previous = if active {
        [
            Value::Number(raw_float(call, owner, field_name(key), 1.0)? as f64),
            Value::Number(raw_float(call, speeds, key, 1.0)? as f64),
        ]
    } else {
        [Value::Nil; 2]
    };
    if let Some(value) = float(call, arg(call, 0)) {
        if key != "xmod" && (!value.is_finite() || value <= 0.0) {
            #[cfg(feature = "test-support")]
            audit(call).speed(
                call,
                call.captures()[1],
                owner,
                observations::Speed {
                    key,
                    before,
                    failed: true,
                    chained: chained(call),
                },
            )?;
            return Err(call.error(b"speed mod must be finite and greater than zero"));
        }
        set_mode(call, owner, key, Some(value))?;
        seed(call, owner)?;
    }
    if let Some(value) = float(call, arg(call, 1)) {
        if value < 0.0 {
            #[cfg(feature = "test-support")]
            audit(call).speed(
                call,
                call.captures()[1],
                owner,
                observations::Speed {
                    key,
                    before,
                    failed: true,
                    chained: chained(call),
                },
            )?;
            return Err(call.error(b"negative option speed"));
        }
        approaches(call, owner, value)?;
    }
    #[cfg(feature = "test-support")]
    audit(call).speed(
        call,
        call.captures()[1],
        owner,
        observations::Speed {
            key,
            before,
            failed: false,
            chained: chained(call),
        },
    )?;
    if call.arguments().len() > 1 && chained(call) {
        return call.push(Value::Table(owner));
    }
    call.push(previous[0])?;
    call.push(previous[1])
}
pub(super) fn field(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<(), Error> {
    #[cfg(feature = "test-support")]
    let before = if audit(call).enabled()
        && (arg(call, 0) != Value::Nil || float(call, arg(call, 1)).is_some())
    {
        audit(call).speed_before(call, call.captures()[1], owner)?
    } else {
        None
    };
    let (field, approach, default) = match key {
        "timespacing" => (SPACING, "_spacing", 0.0),
        "scrollspeed" => ("__songlua_speedmod_xmod", "xmod", 1.0),
        "scrollbpm" => ("__songlua_speedmod_cmod", "cmod", 200.0),
        _ => ("__songlua_speedmod_mmod", "mmod", 0.0),
    };
    let speeds = nested(call, owner, SPEEDS)?;
    let previous = raw_float(call, owner, field, default)?;
    let previous_speed = raw_float(call, speeds, approach, 1.0)?;
    if let Some(value) = float(call, arg(call, 0)) {
        set(owner, field, value, call)?;
        seed(call, owner)?;
        let spacing = raw_float(call, owner, SPACING, 0.0)?;
        let max = raw_float(call, owner, field_name("mmod"), 0.0)?;
        set(
            owner,
            ACTIVE,
            if spacing != 0.0 {
                "cmod"
            } else if max != 0.0 {
                "mmod"
            } else {
                "xmod"
            },
            call,
        )?;
    }
    if let Some(value) = float(call, arg(call, 1)) {
        if value < 0.0 {
            #[cfg(feature = "test-support")]
            audit(call).speed(
                call,
                call.captures()[1],
                owner,
                observations::Speed {
                    key,
                    before,
                    failed: true,
                    chained: chained(call),
                },
            )?;
            return Err(call.error(b"Arg must be greater than or equal to zero."));
        }
        set(speeds, approach, value, call)?;
    }
    #[cfg(feature = "test-support")]
    audit(call).speed(
        call,
        call.captures()[1],
        owner,
        observations::Speed {
            key,
            before,
            failed: false,
            chained: chained(call),
        },
    )?;
    if call.arguments().len() > 1 && chained(call) {
        return call.push(Value::Table(owner));
    }
    call.push(Value::Number(previous as f64))?;
    call.push(Value::Number(previous_speed as f64))
}
pub(super) fn other(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    initial: Option<f32>,
) -> Result<(), Error> {
    let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    if let Some(value) = call.arguments().get(offset).copied() {
        if value == Value::Nil {
            set_mode(call, owner, key, None)?;
        } else if let Some(value) = float(call, value) {
            set_mode(call, owner, key, Some(value))?;
        }
        return call.push(Value::Table(owner));
    }
    let active = raw(call, owner, ACTIVE)?;
    let value = if active == Value::Nil {
        initial.map_or(Value::Nil, |n| Value::Number(n as f64))
    } else if string(call, active)? == key {
        let value = raw(call, owner, field_name(key))?;
        if value == Value::Nil {
            Value::Nil
        } else {
            Value::Number(typed_float(call, value, 0.0)? as f64)
        }
    } else {
        Value::Nil
    };
    call.push(value)
}
