use super::*;
use std::hint::black_box;

#[path = "color_reads_baseline.rs"]
mod baseline;

fn read(args: &MultiValue, method: bool, old: bool) -> Option<[u32; 4]> {
    match (method, old) {
        (true, true) => baseline::read_color_args(args),
        (true, false) => read_color_args(args),
        (false, true) => baseline::read_color_call(args),
        (false, false) => read_color_call(args),
    }
    .map(|v| v.map(f32::to_bits))
}

fn args(lua: &Lua, actor: &Table, kind: &str, method: bool) -> MultiValue {
    let mut values = if method {
        vec![Value::Table(actor.clone())]
    } else {
        Vec::new()
    };
    values.extend(match kind {
        "rgb" => vec![Value::Number(0.25), Value::Number(0.5), Value::Number(0.75)],
        "rgba" => vec![
            Value::Number(0.25),
            Value::Number(0.5),
            Value::Number(0.75),
            Value::Number(0.1),
        ],
        "table" => vec![Value::Table(
            make_color_table(lua, [0.25, 0.5, 0.75, 0.1]).unwrap(),
        )],
        "invalid_table" => {
            let t = lua.create_table().unwrap();
            t.raw_set(1, true).unwrap();
            vec![Value::Table(t)]
        }
        "text" => vec![Value::String(lua.create_string("#12345678").unwrap())],
        "invalid_text" => vec![Value::String(lua.create_string([0xff]).unwrap())],
        "invalid_component" => vec![
            Value::Number(1.0),
            Value::Table(lua.create_table().unwrap()),
            Value::Number(1.0),
        ],
        _ => unreachable!(),
    });
    MultiValue::from_vec(values)
}

#[test]
fn lua_read_colors_preserve_tables_defaults_strings_offsets_and_raw_access() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let mut inputs = vec![
        vec![],
        vec![Value::Nil],
        vec![
            Value::Number(-0.0),
            Value::Integer(i64::MAX),
            Value::Number(1.0),
        ],
        vec![
            Value::Number(f64::NAN),
            Value::Number(1.0),
            Value::Number(1.0),
        ],
    ];
    for text in [
        "",
        "0.5",
        "bad",
        "#12345678",
        "1,0.5,0.25,0.75",
        " #ff0000 ",
        "éあ",
        "NaN",
    ] {
        inputs.push(vec![
            Value::String(lua.create_string(text).unwrap()),
            Value::Number(0.5),
            Value::Number(0.75),
        ]);
    }
    inputs.push(vec![Value::String(lua.create_string([0xff]).unwrap())]);
    for alpha in [
        Value::Nil,
        Value::Number(0.25),
        Value::String(lua.create_string(" NaN ").unwrap()),
        Value::Boolean(false),
    ] {
        let table = lua.create_table().unwrap();
        table.raw_set(1, 0.25).unwrap();
        table.raw_set(2, 0.5).unwrap();
        table.raw_set(3, 0.75).unwrap();
        table.raw_set(4, alpha).unwrap();
        let mt = lua
            .load("return {__index=function() error('must use raw access') end}")
            .eval::<Table>()
            .unwrap();
        table.set_metatable(Some(mt)).unwrap();
        inputs.push(vec![Value::Table(table)]);
    }
    for input in &inputs {
        for method in [false, true] {
            let mut values = if method {
                vec![Value::Table(actor.clone())]
            } else {
                vec![]
            };
            values.extend(input.iter().cloned());
            let args = MultiValue::from_vec(values);
            assert_eq!(read(&args, method, true), read(&args, method, false));
        }
    }
    let rgb = args(&lua, &actor, "rgb", false);
    assert_eq!(read_color_call(&rgb), Some([0.25, 0.5, 0.75, 1.0]));
    let numeric_string =
        MultiValue::from_vec(vec![Value::String(lua.create_string("0.5").unwrap())]);
    assert_eq!(read_color_call(&numeric_string), Some([1.0; 4]));
}

#[test]
fn lua_read_colors_match_generated_argument_lists_and_numeric_bits() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let values = [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(i64::MAX),
        Value::Number(-0.0),
        Value::Number(0.5),
        Value::Number(f64::MAX),
        Value::Number(f64::NAN),
        Value::String(lua.create_string(" 0.75 ").unwrap()),
        Value::String(lua.create_string("1e999").unwrap()),
        Value::String(lua.create_string("-NaN").unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
        Value::Table(lua.create_table().unwrap()),
        Value::Table(make_color_table(&lua, [0.1, 0.2, 0.3, 0.4]).unwrap()),
    ];
    let mut seed = 41u64;
    for _ in 0..5000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let len = (seed >> 32) as usize % 7;
        let mut input = Vec::new();
        for _ in 0..len {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            input.push(values[(seed >> 32) as usize % values.len()].clone());
        }
        for method in [false, true] {
            let mut args = if method {
                vec![Value::Table(actor.clone())]
            } else {
                vec![]
            };
            args.extend(input.iter().cloned());
            let args = MultiValue::from_vec(args);
            assert_eq!(read(&args, method, true), read(&args, method, false));
        }
    }
}

#[test]
fn lua_read_colors_do_not_clone_unique_table_inputs() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    for method in [false, true] {
        for kind in ["rgb", "rgba", "table", "invalid_component"] {
            let args = args(&lua, &actor, kind, method);
            lua.gc_stop();
            crate::perf::assert_no_churn(|| {
                black_box(read(black_box(&args), method, false));
            });
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_read_bench_colors() {
    for method in [false, true] {
        for kind in [
            "rgb",
            "rgba",
            "table",
            "invalid_table",
            "text",
            "invalid_text",
            "invalid_component",
        ] {
            let lua = Lua::new();
            let actor = lua.create_table().unwrap();
            let inputs = args(&lua, &actor, kind, method);
            assert_eq!(read(&inputs, method, true), read(&inputs, method, false));
            lua.gc_stop();
            for old in order() {
                crate::perf::measure_sampled(
                    &format!(
                        "color_{kind}_method_{method}_warm/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    64,
                    || {
                        for _ in 0..64 {
                            black_box(read(black_box(&inputs), method, old));
                        }
                    },
                );
            }
            if matches!(kind, "table" | "invalid_table") {
                for old in order() {
                    crate::perf::measure_sampled(
                        &format!(
                            "color_{kind}_method_{method}_cold/{}",
                            if old { "old" } else { "new" }
                        ),
                        64,
                        16,
                        || {
                            for _ in 0..16 {
                                let inputs = args(&lua, &actor, kind, method);
                                black_box(read(black_box(&inputs), method, old));
                            }
                        },
                    );
                }
            }
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
