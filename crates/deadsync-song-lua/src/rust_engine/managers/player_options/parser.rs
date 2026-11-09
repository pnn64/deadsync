use super::*;
use crate::player_options::{
    option_blocks_skin, parse_option_float, parse_player_option_amount, parse_player_speed_option,
    with_normalized_player_option_key,
};

pub(super) fn apply(call: &mut HostCall<'_>, owner: TableId, text: &str) -> Result<(), Error> {
    for part in text.split(',') {
        token(call, owner, part)?;
    }
    Ok(())
}
fn string_skin(call: &mut HostCall<'_>, owner: TableId, name: &str) -> Result<bool, Error> {
    let exists = skin_exists(call, name)?;
    if exists {
        set(owner, SKIN_OVERRIDE, name, call)?;
        set(owner, SKIN, name, call)?;
        effect(call)?;
    }
    Ok(exists)
}
pub(super) fn reset(call: &mut HostCall<'_>, owner: TableId) -> Result<(), Error> {
    let state = nested(call, owner, STATE)?;
    let speeds = nested(call, owner, SPEEDS)?;
    call.with_roots(&[Value::Table(state), Value::Table(speeds)], |call| {
        let mut key = Value::Nil;
        loop {
            let pair = call
                .call(call.captures()[3], &[Value::Table(state), key])?
                .to_vec();
            key = pair.first().copied().unwrap_or(Value::Nil);
            if key == Value::Nil {
                break;
            }
            call.with_roots(&[key], |call| {
                let name = string(call, key)?;
                if name == "modtimersetting" {
                    set(state, name.as_str(), 3_i32, call)?;
                    set(speeds, name, Value::Nil, call)?;
                } else {
                    let value = default_value(call, &name)?;
                    set(state, name.as_str(), value, call)?;
                    set(speeds, name, 1.0_f32, call)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    })?;
    speed::set_mode(call, owner, "xmod", Some(1.0))?;
    set(owner, "__songlua_speedmod_explicit", false, call)?;
    set(owner, SKIN, "", call)?;
    set(owner, SKIN_OVERRIDE, Value::Nil, call)?;
    let windows = call.try_new_table()?;
    set(owner, WINDOWS, windows, call)?;
    speed::approaches(call, owner, 1.0)
}
fn clear_skin(call: &mut HostCall<'_>, owner: TableId) -> Result<(), Error> {
    let skins = get(call, call.captures()[1], "NOTESKIN")?;
    table(call, skins)?;
    call.with_roots(&[skins], |call| {
        let exists = get(call, skins, "DoesNoteSkinExist")?;
        let exists = function(call, exists)?;
        call.with_roots(&[exists], |call| {
            let default = call.try_intern(crate::SONG_LUA_DEFAULT_NOTESKIN_NAME.as_bytes())?;
            let found = truthy(
                call.call(exists, &[skins, default])?
                    .first()
                    .copied()
                    .unwrap_or(Value::Nil),
            );
            let name = if found {
                crate::SONG_LUA_DEFAULT_NOTESKIN_NAME.to_owned()
            } else {
                let default = call.try_intern(b"default")?;
                if truthy(
                    call.call(exists, &[skins, default])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil),
                ) {
                    "default".into()
                } else {
                    let method = get(call, skins, "GetNoteSkinNames")?;
                    let method = function(call, method)?;
                    let names = call
                        .call(method, &[skins])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil);
                    let names = table(call, names)?;
                    let value = call.raw_table_get(names, Value::Number(2.0))?;
                    if value == Value::Nil {
                        String::new()
                    } else {
                        string(call, value)?
                    }
                }
            };
            set(owner, SKIN, name.as_str(), call)?;
            set(owner, SKIN_OVERRIDE, name, call)
        })
    })
}
pub(super) fn token(call: &mut HostCall<'_>, owner: TableId, raw_text: &str) -> Result<(), Error> {
    let mut name = "";
    let mut amount = 1.0;
    let mut approach = 1.0;
    for part in raw_text.trim().split(' ').filter(|part| !part.is_empty()) {
        name = part;
        if part.eq_ignore_ascii_case("no") {
            amount = 0.0;
        } else if part.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-') {
            if part.ends_with('*') {
                return Ok(());
            }
            amount = parse_player_option_amount(&part.to_ascii_lowercase()).unwrap_or(0.0);
        } else if let Some(prefix) = part.strip_prefix('*')
            && let Some(value) = parse_option_float(prefix)
        {
            approach = if value.is_finite() { value } else { 1.0 };
        }
    }
    if name.is_empty() {
        return Ok(());
    }
    if let Some((key, value)) = parse_player_speed_option(name) {
        let speeds = nested(call, owner, SPEEDS)?;
        set(owner, "__songlua_speedmod_active", key, call)?;
        set(owner, "__songlua_speedmod_explicit", Value::Nil, call)?;
        set(owner, format!("__songlua_speedmod_{key}"), value, call)?;
        match key {
            "xmod" | "cmod" => {
                set(
                    owner,
                    "__songlua_time_spacing",
                    f32::from(key == "cmod"),
                    call,
                )?;
                set(owner, "__songlua_speedmod_mmod", 0.0_f32, call)?;
                set(speeds, key, approach, call)?;
                set(speeds, "_spacing", approach, call)?;
            }
            "mmod" => {
                set(owner, "__songlua_time_spacing", 0.0_f32, call)?;
                set(speeds, key, approach, call)?;
            }
            _ => speed::set_mode(call, owner, key, Some(value))?,
        }
        return Ok(());
    }
    let lower = name.to_ascii_lowercase();
    if !option_blocks_skin(&lower, amount, false) && string_skin(call, owner, &lower)? {
        return Ok(());
    }
    if !option_blocks_skin(&lower, amount, true)
        && string_skin(call, owner, &raw_text.trim().to_ascii_lowercase())?
    {
        return Ok(());
    }
    with_normalized_player_option_key(name, |key| {
        let key = match key {
            "noattacks" => "noattack",
            "randomattacks" => "randattack",
            _ => key,
        };
        if key.is_empty() {
            return Ok(());
        }
        if key == "clearall" {
            reset(call, owner)?;
            return clear_skin(call, owner);
        }
        if key == "noteskin" && amount <= 0.5 {
            set(owner, SKIN, crate::SONG_LUA_DEFAULT_NOTESKIN_NAME, call)?;
            return set(
                owner,
                SKIN_OVERRIDE,
                crate::SONG_LUA_DEFAULT_NOTESKIN_NAME,
                call,
            );
        }
        let state = nested(call, owner, STATE)?;
        call.with_roots(&[Value::Table(state)], |call| {
            if let Some(mode) = deadsync_gameplay::GameplayFailType::from_mod(key) {
                return put(call, state, "failsetting", Value::Number(mode as u8 as f64));
            }
            let timer = match key {
                "modtimergame" => Some(0),
                "modtimerbeat" => Some(1),
                "modtimersong" => Some(2),
                "modtimerdefault" => Some(3),
                _ => None,
            };
            if let Some(timer) = timer {
                return put(call, state, "modtimersetting", Value::Number(timer as f64));
            }
            if scalars::perspective(call, state, key, amount)? {
                let speeds = nested(call, owner, SPEEDS)?;
                return call.with_roots(&[Value::Table(speeds)], |call| {
                    put(call, speeds, "tilt", Value::Number(approach as f64))?;
                    put(call, speeds, "skew", Value::Number(approach as f64))
                });
            }
            let value = if player_option_uses_bool(key) {
                Value::Boolean(amount > 0.5)
            } else {
                Value::Number(amount as f64)
            };
            put(call, state, key, value)?;
            let speeds = nested(call, owner, SPEEDS)?;
            put(call, speeds, key, Value::Number(approach as f64))
        })
    })
}
