use mlua::Value;

use crate::{read_f32, read_i32_value, read_string, truthy};

pub fn timing_window_arg_index(value: Value) -> Option<i32> {
    read_i32_value(value.clone()).or_else(|| {
        let text = read_string(value)?;
        let (_, suffix) = text.rsplit_once('W')?;
        suffix.parse::<i32>().ok()
    })
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
}
