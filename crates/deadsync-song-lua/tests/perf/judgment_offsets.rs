use super::*;
use mlua::{Lua, Table};
use std::hint::black_box;

#[path = "judgment_offsets_baseline.rs"]
mod baseline;

fn assert_offsets(value: Value) {
    let old: Vec<_> = baseline::timing_offsets_from_value(value.clone())
        .into_iter()
        .map(f32::to_bits)
        .collect();
    let new: Vec<_> = timing_offsets_from_value(value).map(f32::to_bits).collect();
    assert_eq!(old, new);
}

#[test]
fn lua_stream_offsets_preserve_numeric_rules_record_order_and_truthiness() {
    let lua = Lua::new();
    let values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(0),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::Number(0.02),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NAN),
        Value::String(lua.create_string(" NaN ").unwrap()),
        Value::String(lua.create_string("-inf").unwrap()),
        Value::String(lua.create_string("0.03").unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
        Value::Table(lua.create_table().unwrap()),
    ];
    for value in &values {
        assert_offsets(value.clone());
    }
    for first in &values {
        for second in &values {
            for enabled in &values {
                let record = lua.create_table().unwrap();
                record.raw_set(2, first.clone()).unwrap();
                record.raw_set(6, enabled.clone()).unwrap();
                record.raw_set(7, second.clone()).unwrap();
                assert_offsets(Value::Table(record));
            }
        }
    }
}

fn offsets(lua: &Lua, count: usize, mode: usize) -> Table {
    let out = lua.create_table().unwrap();
    for index in 0..count {
        let offset = ((index * 71 % 401) as f64 - 200.0) * 0.001;
        let value = match mode {
            0 => Value::Number(offset),
            1 | 2 => {
                let record = lua.create_table().unwrap();
                record.raw_set(2, offset).unwrap();
                record.raw_set(6, mode == 2).unwrap();
                record.raw_set(7, -offset * 0.9).unwrap();
                Value::Table(record)
            }
            _ => Value::Boolean(false),
        };
        out.raw_set(index + 1, value).unwrap();
    }
    out
}

#[test]
fn lua_stream_worst_judgment_preserves_boundaries_holes_and_raw_lookup() {
    let lua = Lua::new();
    for mode in 0..4 {
        for count in [0, 1, 32, 1024] {
            let table = offsets(&lua, count, mode);
            assert_eq!(
                worst_judgment_from_offsets(Value::Table(table.clone())),
                baseline::worst_judgment_from_offsets(Value::Table(table.clone()))
            );
            if count > 1 {
                table.raw_set(2, Value::Nil).unwrap();
                assert_eq!(
                    worst_judgment_from_offsets(Value::Table(table.clone())),
                    baseline::worst_judgment_from_offsets(Value::Table(table))
                );
            }
        }
    }
    for window in 1..=5 {
        let threshold = timing_window_seconds(window, "", false);
        for bits in [
            threshold.to_bits() - 1,
            threshold.to_bits(),
            threshold.to_bits() + 1,
        ] {
            for sign in [-1.0, 1.0] {
                let table = lua
                    .create_sequence_from([f32::from_bits(bits) * sign])
                    .unwrap();
                assert_eq!(
                    worst_judgment_from_offsets(Value::Table(table.clone())),
                    baseline::worst_judgment_from_offsets(Value::Table(table))
                );
            }
        }
    }
    let table = lua
        .load("return setmetatable({}, {__index=function() error('must use raw lookup') end})")
        .eval::<Table>()
        .unwrap();
    assert_offsets(Value::Table(table));
}

#[test]
fn lua_stream_judgment_scan_has_no_allocation_churn() {
    let lua = Lua::new();
    let tables: Vec<_> = (0..4).map(|mode| offsets(&lua, 128, mode)).collect();
    for table in &tables {
        black_box(worst_judgment_from_offsets(Value::Table(table.clone())));
    }
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        for table in &tables {
            black_box(worst_judgment_from_offsets(Value::Table(table.clone())));
        }
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_stream_bench_judgment_offsets() {
    let lua = Lua::new();
    for (count, mode) in [
        (0, 0),
        (1, 0),
        (32, 0),
        (1024, 0),
        (32, 1),
        (1024, 1),
        (32, 2),
        (1024, 2),
        (1024, 3),
    ] {
        let table = offsets(&lua, count, mode);
        let old = baseline::worst_judgment_from_offsets(Value::Table(table.clone()));
        assert_eq!(
            old,
            worst_judgment_from_offsets(Value::Table(table.clone()))
        );
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "judgment_{count}_mode_{mode}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                count.max(1),
                || {
                    let value = Value::Table(black_box(&table).clone());
                    black_box(if old {
                        baseline::worst_judgment_from_offsets(value)
                    } else {
                        worst_judgment_from_offsets(value)
                    });
                },
            );
        }
    }
}
