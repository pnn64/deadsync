use mlua::{MultiValue, Value};

use crate::read_f32;

pub fn version_parts(version: &str) -> [i64; 3] {
    let mut parts = [0_i64; 3];
    for (index, part) in version.trim().split('.').take(3).enumerate() {
        let digits = part
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .collect::<Vec<_>>();
        parts[index] = std::str::from_utf8(&digits)
            .ok()
            .and_then(|digits| digits.parse::<i64>().ok())
            .unwrap_or(0);
    }
    parts
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parts_parse_prefix_digits() {
        assert_eq!(version_parts("5.1.0-beta"), [5, 1, 0]);
        assert_eq!(version_parts(" 4.2 "), [4, 2, 0]);
        assert_eq!(version_parts("main"), [0, 0, 0]);
    }
}
