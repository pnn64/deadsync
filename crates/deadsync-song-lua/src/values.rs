use mlua::Value;
use std::collections::HashMap;
use std::ffi::c_void;

use crate::{LUA_PLAYERS, SongLuaSpanMode};

pub const SONG_LUA_EASING_NAME_KEY: &str = "__songlua_easing_name";

#[inline(always)]
pub fn read_f32(value: Value) -> Option<f32> {
    match value {
        Value::Integer(value) => Some(value as f32),
        Value::Number(value) => {
            let value = value as f32;
            value.is_finite().then_some(value)
        }
        Value::String(text) => text.to_str().ok()?.trim().parse::<f32>().ok(),
        _ => None,
    }
}

#[inline(always)]
pub fn read_f64(value: Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(value as f64),
        Value::Number(value) => value.is_finite().then_some(value),
        Value::String(text) => text.to_str().ok()?.trim().parse::<f64>().ok(),
        _ => None,
    }
}

#[inline(always)]
pub fn read_boolish(value: Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(value),
        Value::Integer(value) => Some(value != 0),
        Value::Number(value) => Some(value != 0.0),
        Value::String(text) => {
            let text = text.to_str().ok()?;
            let text = text.trim();
            if text.eq_ignore_ascii_case("true")
                || text.eq_ignore_ascii_case("yes")
                || text.eq_ignore_ascii_case("on")
            {
                Some(true)
            } else if text.eq_ignore_ascii_case("false")
                || text.eq_ignore_ascii_case("no")
                || text.eq_ignore_ascii_case("off")
            {
                Some(false)
            } else {
                text.parse::<f32>().ok().map(|value| value != 0.0)
            }
        }
        _ => None,
    }
}

#[inline(always)]
pub fn read_string(value: Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        _ => None,
    }
}

#[inline(always)]
pub fn read_u32_value(value: Value) -> Option<u32> {
    match value {
        Value::Integer(value) if value >= 0 => u32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() && value >= 0.0 && value.fract() == 0.0 => {
            u32::try_from(value as u64).ok()
        }
        _ => None,
    }
}

#[inline(always)]
pub fn read_i32_value(value: Value) -> Option<i32> {
    match value {
        Value::Integer(value) => Some(value.clamp(i32::MIN.into(), i32::MAX.into()) as i32),
        Value::Number(value) if value.is_finite() => Some(
            value
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ),
        Value::String(text) => {
            let text = text.to_str().ok()?;
            let text = text.trim();
            text.parse::<i32>()
                .ok()
                .or_else(|| text.parse::<f32>().ok().map(|value| value.round() as i32))
        }
        _ => None,
    }
}

#[inline(always)]
pub fn read_player(value: Value) -> Option<u8> {
    match value {
        Value::Integer(value) if (1..=2).contains(&value) => Some(value as u8),
        Value::Number(value) if (1.0..=2.0).contains(&value) => Some(value as u8),
        _ => None,
    }
}

#[inline(always)]
pub fn read_span_mode(value: Value) -> Option<SongLuaSpanMode> {
    let Value::String(text) = value else {
        return None;
    };
    let text = text.as_bytes();
    if text.eq_ignore_ascii_case(b"len") {
        Some(SongLuaSpanMode::Len)
    } else if text.eq_ignore_ascii_case(b"end") {
        Some(SongLuaSpanMode::End)
    } else {
        None
    }
}

#[inline(always)]
pub fn read_easing_name(
    value: Value,
    easing_names: &HashMap<*const c_void, String>,
) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_str().ok()?.to_string()),
        Value::Function(function) => easing_names.get(&function.to_pointer()).cloned(),
        Value::Table(table) => easing_names.get(&table.to_pointer()).cloned().or_else(|| {
            table
                .raw_get::<Option<String>>(SONG_LUA_EASING_NAME_KEY)
                .ok()
                .flatten()
        }),
        _ => None,
    }
}

#[inline(always)]
pub const fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

#[inline(always)]
pub fn lua_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Integer(left), Value::Integer(right)) => left == right,
        (Value::Integer(left), Value::Number(right)) => (*left as f64) == *right,
        (Value::Number(left), Value::Integer(right)) => *left == (*right as f64),
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => string_values_equal(left, right),
        (Value::Table(left), Value::Table(right)) => left.to_pointer() == right.to_pointer(),
        _ => false,
    }
}

pub fn lua_binary_to_hex(value: Value) -> String {
    let Value::String(text) = value else {
        return String::new();
    };
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let bytes = text.as_bytes();
    let mut out = vec![0u8; bytes.len() * 2];
    for (output, &byte) in out.chunks_exact_mut(2).zip(bytes.iter()) {
        output[0] = HEX[(byte >> 4) as usize];
        output[1] = HEX[(byte & 0xf) as usize];
    }
    String::from_utf8(out).expect("hexadecimal lookup table contains only ASCII")
}

#[inline(always)]
pub fn player_index_from_value(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) => usize::try_from(*value).ok().filter(|v| *v < LUA_PLAYERS),
        Value::Number(value) => {
            if *value >= 0.0 && *value < LUA_PLAYERS as f64 && value.fract() == 0.0 {
                Some(*value as usize)
            } else {
                None
            }
        }
        Value::String(text) => {
            let text = text.as_bytes();
            if text.eq_ignore_ascii_case(b"P1") || text.eq_ignore_ascii_case(b"PlayerNumber_P1") {
                Some(0)
            } else if text.eq_ignore_ascii_case(b"P2")
                || text.eq_ignore_ascii_case(b"PlayerNumber_P2")
            {
                Some(1)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[inline(always)]
#[must_use]
pub fn player_number_name(player: usize) -> &'static str {
    match player {
        0 => "PlayerNumber_P1",
        1 => "PlayerNumber_P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}

#[inline(never)]
fn string_values_equal(left: &mlua::LuaString, right: &mlua::LuaString) -> bool {
    // Equal bytes need one UTF-8 check; invalid strings remain unequal,
    // including two handles referring to the same Lua string.
    let left = left.as_bytes();
    let right = right.as_bytes();
    left.len() == right.len()
        && (std::ptr::eq(left.as_ptr(), right.as_ptr()) || left[..] == right[..])
        && std::str::from_utf8(&left).is_ok()
}

#[cfg(test)]
#[path = "../tests/perf/values.rs"]
mod perf;

#[cfg(test)]
#[path = "../tests/perf/value_dispatch.rs"]
mod dispatch_perf;
