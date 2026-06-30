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
pub fn read_boolish(value: Value) -> Option<bool> {
    match value {
        Value::Boolean(value) => Some(value),
        Value::Integer(value) => Some(value != 0),
        Value::Number(value) => Some(value != 0.0),
        Value::String(text) => {
            let text = text.to_str().ok()?.trim().to_string();
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
        Value::Integer(value) => Some(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        Value::Number(value) if value.is_finite() => Some(
            value
                .round()
                .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32,
        ),
        Value::String(text) => text
            .to_str()
            .ok()?
            .trim()
            .parse::<i32>()
            .ok()
            .or_else(|| read_f32(Value::String(text)).map(|value| value.round() as i32)),
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
    let text = read_string(value)?;
    if text.eq_ignore_ascii_case("len") {
        Some(SongLuaSpanMode::Len)
    } else if text.eq_ignore_ascii_case("end") {
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
pub fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

pub fn lua_values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::Integer(left), Value::Integer(right)) => left == right,
        (Value::Integer(left), Value::Number(right)) => (*left as f64) == *right,
        (Value::Number(left), Value::Integer(right)) => *left == (*right as f64),
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::String(left), Value::String(right)) => left
            .to_str()
            .ok()
            .zip(right.to_str().ok())
            .is_some_and(|(left, right)| left == right),
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
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes.iter().copied() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[inline(always)]
pub fn player_index_from_value(value: &Value) -> Option<usize> {
    match value {
        Value::Integer(value) => usize::try_from(*value).ok().filter(|v| *v < LUA_PLAYERS),
        Value::Number(value) => {
            if *value >= 0.0 && *value < LUA_PLAYERS as f64 {
                Some(*value as usize)
            } else {
                None
            }
        }
        Value::String(text) => match text.to_str().ok()?.as_ref() {
            "P1" => Some(0),
            "P2" => Some(1),
            "PlayerNumber_P1" => Some(0),
            "PlayerNumber_P2" => Some(1),
            _ => None,
        },
        _ => None,
    }
}

#[inline(always)]
pub fn player_number_name(player: usize) -> &'static str {
    match player {
        0 => "PlayerNumber_P1",
        1 => "PlayerNumber_P2",
        _ => unreachable!("song lua only exposes two player numbers"),
    }
}
