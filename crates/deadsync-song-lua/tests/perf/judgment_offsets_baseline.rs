// Frozen from f2d8fea821cefb4aa7655302b8e4da2430ca16ff; only test visibility differs.

use super::*;

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

pub(super) fn timing_offsets_from_value(value: Value) -> Vec<f32> {
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
