use mlua::Value;

use crate::{read_f32, read_i32_value, read_string, truthy};

pub const SONG_LUA_TIMING_WINDOW_NAMES: [&str; 5] = [
    "TimingWindow_W1",
    "TimingWindow_W2",
    "TimingWindow_W3",
    "TimingWindow_W4",
    "TimingWindow_W5",
];

pub fn timing_window_arg_index(value: Value) -> Option<i32> {
    read_i32_value(value.clone()).or_else(|| {
        let text = read_string(value)?;
        let (_, suffix) = text.rsplit_once('W')?;
        suffix.parse::<i32>().ok()
    })
}

pub fn timing_window_name(value: Value) -> Option<&'static str> {
    let index = match value {
        Value::Integer(value) => i32::try_from(value).ok(),
        Value::Number(value) if value.is_finite() => Some(value.round() as i32),
        Value::String(text) => text
            .to_str()
            .ok()?
            .chars()
            .rev()
            .find(|ch| ch.is_ascii_digit())
            .and_then(|ch| ch.to_digit(10))
            .map(|value| value as i32),
        _ => None,
    }?;
    (1..=5)
        .contains(&index)
        .then(|| SONG_LUA_TIMING_WINDOW_NAMES[index as usize - 1])
}

pub fn timing_window_seconds(index: i32, mode: &str, tenms: bool) -> f32 {
    if mode.eq_ignore_ascii_case("FA+") && tenms && index == 1 {
        return 0.0085 + 0.0015;
    }
    let windows = if mode.eq_ignore_ascii_case("FA+") {
        [0.0135, 0.0215, 0.043, 0.135, 0.18]
    } else {
        [0.0215, 0.043, 0.102, 0.135, 0.18]
    };
    let index = index.clamp(1, windows.len() as i32) as usize - 1;
    windows[index] + 0.0015
}

pub fn worst_judgment_from_offsets(value: Value) -> i32 {
    let Value::Table(offsets) = value else {
        return 1;
    };
    let mut worst = 1;
    for pair in offsets.sequence_values::<Value>() {
        let Ok(value) = pair else {
            continue;
        };
        for offset in timing_offsets_from_value(value) {
            let abs = offset.abs();
            let judgment = (1..=5)
                .find(|window| abs <= timing_window_seconds(*window, "", false))
                .unwrap_or(5);
            worst = worst.max(judgment);
        }
    }
    worst
}

fn timing_offsets_from_value(value: Value) -> Vec<f32> {
    if let Some(offset) = read_f32(value.clone()) {
        return vec![offset];
    }
    let Value::Table(table) = value else {
        return Vec::new();
    };
    let mut offsets = Vec::new();
    if let Ok(value) = table.raw_get::<Value>(2)
        && let Some(offset) = read_f32(value)
    {
        offsets.push(offset);
    }
    if matches!(table.raw_get::<Value>(6), Ok(value) if truthy(&value))
        && let Ok(value) = table.raw_get::<Value>(7)
        && let Some(offset) = read_f32(value)
    {
        offsets.push(offset);
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_window_seconds_matches_itg_and_fa_plus() {
        assert!((timing_window_seconds(1, "", false) - 0.023).abs() <= 1e-6);
        assert!((timing_window_seconds(2, "", false) - 0.0445).abs() <= 1e-6);
        assert!((timing_window_seconds(1, "FA+", false) - 0.015).abs() <= 1e-6);
        assert!((timing_window_seconds(1, "FA+", true) - 0.010).abs() <= 1e-6);
        assert!((timing_window_seconds(99, "", false) - 0.1815).abs() <= 1e-6);
    }

    #[test]
    fn timing_window_name_accepts_numbers_and_names() {
        let lua = mlua::Lua::new();
        assert_eq!(
            timing_window_name(Value::Integer(1)),
            Some("TimingWindow_W1")
        );
        assert_eq!(
            timing_window_name(Value::String(lua.create_string("TimingWindow_W5").unwrap())),
            Some("TimingWindow_W5")
        );
        assert_eq!(timing_window_name(Value::Integer(8)), None);
    }
}
