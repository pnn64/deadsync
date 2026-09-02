use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn fixture(name: &str) -> Value {
    let path = fixture_path(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("invalid {}: {error}", path.display()))
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/itgmania-actors")
        .join(format!("{name}.json"))
}

pub fn samples(fixture: &Value) -> &[Value] {
    fixture["samples"].as_array().expect("fixture samples")
}

pub fn actor<'a>(sample: &'a Value, name: &str) -> &'a Value {
    sample["actors"]
        .as_array()
        .expect("sample actors")
        .iter()
        .find(|actor| actor["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("sample has no actor {name:?}"))
}

pub fn f32_at(value: &Value, field: &str) -> f32 {
    value[field]
        .as_f64()
        .unwrap_or_else(|| panic!("{field:?} is not numeric")) as f32
}

pub fn f32_array<const N: usize>(value: &Value) -> [f32; N] {
    let values = value.as_array().expect("expected numeric array");
    assert_eq!(values.len(), N, "numeric array length");
    std::array::from_fn(|index| values[index].as_f64().expect("numeric array item") as f32)
}

/// Convert native `RectF` storage (left, top, right, bottom) to DeadSync's
/// actor field order (left, right, top, bottom).
pub fn actor_rect(value: &Value) -> [f32; 4] {
    let [left, top, right, bottom] = f32_array(value);
    [left, right, top, bottom]
}

pub fn matrix(value: &Value) -> [[f32; 4]; 4] {
    let rows = value.as_array().expect("matrix rows");
    assert_eq!(rows.len(), 4, "matrix row count");
    std::array::from_fn(|row| f32_array(&rows[row]))
}

pub fn assert_matrix(actual: [[f32; 4]; 4], expected: [[f32; 4]; 4], context: &str) {
    for row in 0..4 {
        assert_array_ulp(
            actual[row],
            expected[row],
            32,
            &format!("{context} row {row}"),
        );
    }
}

pub fn assert_f32(actual: f32, expected: f32, context: &str) {
    assert_f32_ulp(actual, expected, 8, context);
}

pub fn assert_f32_ulp(actual: f32, expected: f32, max_ulp: u32, context: &str) {
    if actual.to_bits() == expected.to_bits() {
        return;
    }
    let distance = ulp_distance(actual, expected);
    assert!(
        distance <= max_ulp,
        "{context}: expected {expected:?} ({:#010x}), got {actual:?} ({:#010x}); {distance} ULP",
        expected.to_bits(),
        actual.to_bits(),
    );
}

pub fn assert_array<const N: usize>(actual: [f32; N], expected: [f32; N], context: &str) {
    for index in 0..N {
        assert_f32(
            actual[index],
            expected[index],
            &format!("{context}[{index}]"),
        );
    }
}

pub fn assert_array_ulp<const N: usize>(
    actual: [f32; N],
    expected: [f32; N],
    max_ulp: u32,
    context: &str,
) {
    for index in 0..N {
        assert_f32_ulp(
            actual[index],
            expected[index],
            max_ulp,
            &format!("{context}[{index}]"),
        );
    }
}

fn ulp_distance(a: f32, b: f32) -> u32 {
    if a.is_nan() || b.is_nan() {
        return u32::MAX;
    }
    ordered_bits(a).abs_diff(ordered_bits(b))
}

fn ordered_bits(value: f32) -> u32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 == 0 {
        bits | 0x8000_0000
    } else {
        !bits
    }
}
