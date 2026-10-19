// Frozen production version code from 7a4bdbb55; test module omitted.
use mlua::{MultiValue, Value};

use crate::{SONG_LUA_PRODUCT_VERSION, read_f32};

pub fn version_parts(version: &str) -> [i64; 3] {
    let mut parts = [0_i64; 3];
    for (index, part) in version.trim().split('.').take(3).enumerate() {
        let digits = part.bytes().take_while(u8::is_ascii_digit).count();
        parts[index] = part[..digits].parse::<i64>().unwrap_or(0);
    }
    parts
}

#[must_use]
pub fn version_args(args: &MultiValue) -> Vec<i64> {
    if let Some(Value::String(version)) = args.front()
        && let Ok(version) = version.to_str()
    {
        return version_parts(version.as_ref()).into_iter().collect();
    }
    args.iter()
        .filter_map(|value| read_f32(value.clone()))
        .map(|value| value.round() as i64)
        .collect()
}

#[must_use]
pub fn is_product_version(product_version: &str, args: &MultiValue) -> bool {
    let expected = version_args(args);
    if expected.is_empty() {
        return false;
    }
    let product = version_parts(product_version);
    expected
        .into_iter()
        .enumerate()
        .all(|(index, value)| product.get(index).is_some_and(|part| *part == value))
}

#[must_use]
pub fn song_lua_is_product_version(args: &MultiValue) -> bool {
    is_product_version(SONG_LUA_PRODUCT_VERSION, args)
}

#[must_use]
pub fn is_minimum_product_version(product_version: &str, args: &MultiValue) -> bool {
    let expected = version_args(args);
    if expected.is_empty() {
        return true;
    }
    let product = version_parts(product_version);
    for (index, expected) in expected.into_iter().enumerate() {
        let product = product.get(index).copied().unwrap_or(0);
        if product != expected {
            return product > expected;
        }
    }
    true
}

#[must_use]
pub fn song_lua_is_minimum_product_version(args: &MultiValue) -> bool {
    is_minimum_product_version(SONG_LUA_PRODUCT_VERSION, args)
}
