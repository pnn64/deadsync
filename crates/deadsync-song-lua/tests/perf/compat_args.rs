use super::*;
use crate::version::{is_minimum_product_version, is_product_version};
use mlua::{Lua, MultiValue, Table, Value};

#[path = "compat_args_before.rs"]
mod before;
#[allow(dead_code)]
#[path = "compat_args_version_before.rs"]
mod before_version;

fn strings(table: Table) -> (usize, Vec<Vec<u8>>) {
    assert!(table.metatable().is_none());
    let len = table.raw_len();
    let values: Vec<_> = table
        .sequence_values::<mlua::LuaString>()
        .map(|value| value.unwrap().as_bytes().to_vec())
        .collect();
    assert_eq!(table.pairs::<Value, Value>().count(), values.len());
    (len, values)
}

#[test]
fn borrowed_split_arguments_keep_unicode_defaults_and_argument_order() {
    let lua = Lua::new();
    let texts: Vec<Value> = [
        "",
        "a,b,c",
        ",a,,b,",
        "é曲",
        "a\0b",
        "\r\n",
        "🙂é",
        "a::b::::c",
    ]
    .into_iter()
    .map(|s| Value::String(lua.create_string(s).unwrap()))
    .chain([
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(42),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
    ])
    .collect();
    let separators: Vec<Value> = ["", ",", "::", "é", "\0", "\r\n", "missing"]
        .into_iter()
        .map(|s| Value::String(lua.create_string(s).unwrap()))
        .chain([
            Value::Nil,
            Value::Boolean(false),
            Value::Number(1.5),
            Value::String(lua.create_string([0xff]).unwrap()),
        ])
        .collect();
    for text in &texts {
        for separator in &separators {
            let args: MultiValue = [text.clone(), separator.clone()].into_iter().collect();
            let expected = strings(before::string_split(&lua, &args).unwrap());
            assert_eq!(strings(split_args(&lua, &args, 0, 1).unwrap()), expected);
            let reversed: MultiValue = [separator.clone(), text.clone()].into_iter().collect();
            assert_eq!(
                strings(before::global_split(&lua, &reversed).unwrap()),
                expected
            );
            assert_eq!(
                strings(split_args(&lua, &reversed, 1, 0).unwrap()),
                expected
            );
        }
    }
    for args in [MultiValue::new(), [texts[1].clone()].into_iter().collect()] {
        assert_eq!(
            strings(split_args(&lua, &args, 0, 1).unwrap()),
            strings(before::string_split(&lua, &args).unwrap())
        );
        assert_eq!(
            strings(split_args(&lua, &args, 1, 0).unwrap()),
            strings(before::global_split(&lua, &args).unwrap())
        );
    }
    let callback = lua
        .create_function(|lua, args: MultiValue| split_args(lua, &args, 0, 1))
        .unwrap();
    let text = lua.create_string("é曲").unwrap();
    let actual = strings(callback.call::<Table>((text.clone(), "")).unwrap());
    assert_eq!(
        actual,
        (2, vec!["é".as_bytes().to_vec(), "曲".as_bytes().to_vec()])
    );
    assert_eq!(text.as_bytes().as_ref(), "é曲".as_bytes());
}

#[test]
fn borrowed_basename_keeps_paths_binary_fallbacks_and_lua_calls() {
    let lua = Lua::new();
    for (text, expected) in [
        ("", ""),
        ("song.png", "song.png"),
        ("a/b/c.png", "c.png"),
        ("a\\b\\c.png", "c.png"),
        ("a/b/", "b"),
        ("///", ""),
        ("é/曲.lua", "曲.lua"),
        ("a\0b/c\0d", "c\0d"),
        ("a/.hidden", ".hidden"),
        ("C:\\", "C:"),
    ] {
        let value = Value::String(lua.create_string(text).unwrap());
        let old = before::basename(&lua, value.clone()).unwrap();
        let new = basename(&lua, value.clone()).unwrap();
        let (Value::String(old), Value::String(new)) = (old, new) else {
            panic!("string output");
        };
        assert_eq!(old.as_bytes().as_ref(), expected.as_bytes());
        assert_eq!(new.as_bytes().as_ref(), old.as_bytes().as_ref());
    }
    for value in [
        Value::Nil,
        Value::Boolean(false),
        Value::Number(42.0),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
    ] {
        for current in [false, true] {
            let Value::String(output) = (if current {
                basename(&lua, value.clone())
            } else {
                before::basename(&lua, value.clone())
            })
            .unwrap() else {
                panic!("string output");
            };
            assert!(output.as_bytes().is_empty());
        }
    }
    let callbacks = [false, true].map(|current| {
        lua.create_function(move |lua, value: Value| {
            if current {
                basename(lua, value)
            } else {
                before::basename(lua, value)
            }
        })
        .unwrap()
    });
    for source in [
        "return f('folder/é.png/')",
        "return f(string.char(255))",
        "return f(42)",
        "return f()",
        "return f(setmetatable({}, {__tostring=function() error('must not run') end}))",
    ] {
        let outcomes = callbacks.each_ref().map(|callback| {
            lua.globals().set("f", callback.clone()).unwrap();
            lua.load(source)
                .set_name("@compat-args.lua")
                .eval::<String>()
                .map_err(|error| error.to_string())
        });
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn streamed_version_comparisons_keep_coercion_order_and_extra_components() {
    let lua = Lua::new();
    let pool = vec![
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(0),
        Value::Integer(1),
        Value::Integer(5),
        Value::Number(0.5),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::String(lua.create_string("5.1.0-beta").unwrap()),
        Value::String(lua.create_string("NaN").unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
    ];
    let products = [
        "5.1.0",
        "0.0.0",
        "5.1.0.9",
        "",
        "main",
        "-1.2.3",
        "\u{2003}5.1\u{a0}",
        "999999999999999999999.1.2",
    ];
    for first in &pool {
        for second in &pool {
            for third in &pool {
                let args: MultiValue = [first.clone(), second.clone(), third.clone()]
                    .into_iter()
                    .collect();
                for product in products {
                    assert_eq!(
                        is_product_version(product, &args),
                        before_version::is_product_version(product, &args)
                    );
                    assert_eq!(
                        is_minimum_product_version(product, &args),
                        before_version::is_minimum_product_version(product, &args)
                    );
                }
            }
        }
    }
    for values in [
        vec![],
        vec![Value::Integer(5)],
        vec![
            Value::Integer(5),
            Value::Integer(1),
            Value::Integer(0),
            Value::Integer(0),
        ],
        vec![Value::Integer(i64::MAX), Value::Number(f64::MAX)],
        vec![Value::Nil, Value::String(lua.create_string("inf").unwrap())],
        vec![Value::Integer(5); 64],
    ] {
        let args: MultiValue = values.into_iter().collect();
        for product in products {
            assert_eq!(
                is_product_version(product, &args),
                before_version::is_product_version(product, &args)
            );
            assert_eq!(
                is_minimum_product_version(product, &args),
                before_version::is_minimum_product_version(product, &args)
            );
        }
    }
    let extra_zero: MultiValue = [5, 1, 0, 0].map(Value::Integer).into_iter().collect();
    assert!(!is_product_version("5.1.0", &extra_zero));
    assert!(is_minimum_product_version("5.1.0", &extra_zero));
    let first_string: MultiValue = [
        Value::String(lua.create_string("5.1.0").unwrap()),
        Value::Integer(99),
    ]
    .into_iter()
    .collect();
    assert!(is_product_version("5.1.0", &first_string));
    assert!(is_minimum_product_version("5.1.0", &MultiValue::new()));
    assert!(!is_product_version("5.1.0", &MultiValue::new()));
}

#[test]
fn borrowed_compat_arguments_remove_only_temporary_copy_allocations() {
    use crate::draw_capture::perf::allocations;
    use std::hint::black_box;
    let lua = Lua::new();
    let args: MultiValue = [
        Value::String(lua.create_string("a,b,c").unwrap()),
        Value::String(lua.create_string(",").unwrap()),
    ]
    .into_iter()
    .collect();
    black_box(before::string_split(&lua, &args).unwrap());
    black_box(split_args(&lua, &args, 0, 1).unwrap());
    let (_, old) = allocations::measure(|| {
        black_box(before::string_split(&lua, &args).unwrap());
    });
    let (_, new) = allocations::measure(|| {
        black_box(split_args(&lua, &args, 0, 1).unwrap());
    });
    assert_eq!(old.allocs, new.allocs + 2);
    assert_eq!(old.reallocs, new.reallocs);
    assert!(old.allocated_bytes > new.allocated_bytes);
    let value = Value::String(lua.create_string("folder/song.png").unwrap());
    black_box(before::basename(&lua, value.clone()).unwrap());
    black_box(basename(&lua, value.clone()).unwrap());
    let (_, old) = allocations::measure(|| {
        black_box(before::basename(&lua, value.clone()).unwrap());
    });
    let (_, new) = allocations::measure(|| {
        black_box(basename(&lua, value.clone()).unwrap());
    });
    assert_eq!(old.allocs, 1);
    assert_eq!(new, allocations::Churn::default());
    for args in [
        [Value::Integer(5), Value::Integer(1), Value::Integer(0)]
            .into_iter()
            .collect::<MultiValue>(),
        [Value::String(lua.create_string("5.1.0").unwrap())]
            .into_iter()
            .collect(),
    ] {
        black_box(before_version::is_product_version("5.1.0", &args));
        black_box(is_product_version("5.1.0", &args));
        let (_, old) = allocations::measure(|| {
            black_box(before_version::is_product_version("5.1.0", &args));
            black_box(before_version::is_minimum_product_version("5.1.0", &args));
        });
        let (_, new) = allocations::measure(|| {
            black_box(is_product_version("5.1.0", &args));
            black_box(is_minimum_product_version("5.1.0", &args));
        });
        assert_eq!(old.allocs, 2);
        assert_eq!(new, allocations::Churn::default());
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_compat_args() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
        input_bytes: usize,
        count: usize,
        mut setup: impl FnMut() -> W,
    ) {
        let mut samples = Vec::new();
        for sample in 0..10 {
            let mut work = setup();
            let start = Instant::now();
            let start_cycles = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..count {
                    work();
                }
            });
            let used = cycles() - start_cycles;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":count,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"input_bytes":input_bytes,"baseline_commit":"7a4bdbb55"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, text, separator, api, reversed, count) in [
        (
            "split_core_short",
            b"a,b,c".to_vec(),
            ",",
            false,
            false,
            2000,
        ),
        (
            "split_core_long",
            "abcdefgh,".repeat(512).into_bytes(),
            ",",
            false,
            false,
            64,
        ),
        (
            "split_core_chars",
            "é曲".repeat(16).into_bytes(),
            "",
            false,
            false,
            256,
        ),
        (
            "split_core_no_match",
            "abcdefgh".repeat(1024).into_bytes(),
            "|",
            false,
            false,
            256,
        ),
        ("split_api_short", b"a,b,c".to_vec(), ",", true, false, 2000),
        (
            "split_api_long",
            "abcdefgh,".repeat(512).into_bytes(),
            ",",
            true,
            false,
            64,
        ),
        (
            "split_global_short",
            b"a,b,c".to_vec(),
            ",",
            true,
            true,
            2000,
        ),
        (
            "split_global_chars",
            "é曲".repeat(16).into_bytes(),
            "",
            true,
            true,
            256,
        ),
        ("split_api_invalid", vec![0xff; 32], ",", true, false, 2000),
    ] {
        for current in variants {
            measure(
                name,
                current,
                if api {
                    "lua_split_calls"
                } else {
                    "split_tables"
                },
                text.len() + separator.len(),
                count,
                || {
                    let lua = Lua::new();
                    let text = Value::String(lua.create_string(&text).unwrap());
                    let separator = Value::String(lua.create_string(separator).unwrap());
                    let args: MultiValue = if reversed {
                        [separator, text]
                    } else {
                        [text, separator]
                    }
                    .into_iter()
                    .collect();
                    let callback = lua
                        .create_function(move |lua, args: MultiValue| {
                            if current {
                                split_args(
                                    lua,
                                    &args,
                                    usize::from(reversed),
                                    usize::from(!reversed),
                                )
                            } else if reversed {
                                before::global_split(lua, &args)
                            } else {
                                before::string_split(lua, &args)
                            }
                        })
                        .unwrap();
                    callback.call::<Table>(&args).unwrap();
                    if !api {
                        black_box(
                            if current {
                                split_args(&lua, &args, 0, 1)
                            } else {
                                before::string_split(&lua, &args)
                            }
                            .unwrap(),
                        );
                    }
                    let fixture = (lua, args, callback);
                    move || {
                        let (lua, args, callback) = black_box(&fixture);
                        if api {
                            black_box(callback.call::<Table>(args).unwrap());
                        } else {
                            black_box(
                                if current {
                                    split_args(lua, args, 0, 1)
                                } else {
                                    before::string_split(lua, args)
                                }
                                .unwrap(),
                            );
                        }
                    }
                },
            );
        }
    }
    for (name, text, api, numeric) in [
        ("basename_core_name", b"song.png".to_vec(), false, false),
        (
            "basename_core_path",
            b"Themes/default/graphics/screen/thing.png".to_vec(),
            false,
            false,
        ),
        (
            "basename_core_long",
            format!("{}thing.png", "folder/".repeat(2048)).into_bytes(),
            false,
            false,
        ),
        ("basename_core_invalid", vec![0xff; 32], false, false),
        (
            "basename_api_path",
            b"Themes/default/graphics/screen/thing.png".to_vec(),
            true,
            false,
        ),
        (
            "basename_api_long",
            format!("{}thing.png", "folder/".repeat(2048)).into_bytes(),
            true,
            false,
        ),
        ("basename_api_nonstring", Vec::new(), true, true),
    ] {
        for current in variants {
            measure(
                name,
                current,
                if api {
                    "lua_basename_calls"
                } else {
                    "basenames"
                },
                text.len(),
                2000,
                || {
                    let lua = Lua::new();
                    let value = if numeric {
                        Value::Number(42.0)
                    } else {
                        Value::String(lua.create_string(&text).unwrap())
                    };
                    let callback = lua
                        .create_function(move |lua, value: Value| {
                            if current {
                                basename(lua, value)
                            } else {
                                before::basename(lua, value)
                            }
                        })
                        .unwrap();
                    callback.call::<Value>(value.clone()).unwrap();
                    if !api {
                        black_box(
                            if current {
                                basename(&lua, value.clone())
                            } else {
                                before::basename(&lua, value.clone())
                            }
                            .unwrap(),
                        );
                    }
                    let fixture = (lua, value, callback);
                    move || {
                        let (lua, value, callback) = black_box(&fixture);
                        if api {
                            black_box(callback.call::<Value>(value.clone()).unwrap());
                        } else {
                            black_box(
                                if current {
                                    basename(lua, value.clone())
                                } else {
                                    before::basename(lua, value.clone())
                                }
                                .unwrap(),
                            );
                        }
                    }
                },
            );
        }
    }
    for (label, kind, api) in [
        ("string", 0, false),
        ("numbers", 1, false),
        ("early", 2, false),
        ("empty", 3, false),
        ("api_string", 0, true),
        ("api_numbers", 1, true),
    ] {
        for minimum in [false, true] {
            let name = format!(
                "version_{}_{label}",
                if minimum { "minimum" } else { "exact" }
            );
            for current in variants {
                measure(
                    &name,
                    current,
                    if api {
                        "lua_version_calls"
                    } else {
                        "version_checks"
                    },
                    if kind == 0 { 5 } else { 0 },
                    if api { 2000 } else { 10000 },
                    || {
                        let lua = Lua::new();
                        let args: MultiValue = match kind {
                            0 => vec![Value::String(lua.create_string("5.1.0").unwrap())],
                            1 => vec![Value::Integer(5), Value::Integer(1), Value::Integer(0)],
                            2 => vec![Value::Integer(7); 64],
                            _ => Vec::new(),
                        }
                        .into_iter()
                        .collect();
                        let callback = lua
                            .create_function(move |_, args: MultiValue| {
                                Ok(match (minimum, current) {
                                    (false, false) => {
                                        before_version::is_product_version("5.1.0", &args)
                                    }
                                    (false, true) => is_product_version("5.1.0", &args),
                                    (true, false) => {
                                        before_version::is_minimum_product_version("5.1.0", &args)
                                    }
                                    (true, true) => is_minimum_product_version("5.1.0", &args),
                                })
                            })
                            .unwrap();
                        callback.call::<bool>(&args).unwrap();
                        if !api {
                            black_box(match (minimum, current) {
                                (false, false) => {
                                    before_version::is_product_version("5.1.0", &args)
                                }
                                (false, true) => is_product_version("5.1.0", &args),
                                (true, false) => {
                                    before_version::is_minimum_product_version("5.1.0", &args)
                                }
                                (true, true) => is_minimum_product_version("5.1.0", &args),
                            });
                        }
                        let fixture = (lua, args, callback);
                        move || {
                            let (_, args, callback) = black_box(&fixture);
                            if api {
                                black_box(callback.call::<bool>(args).unwrap());
                            } else {
                                black_box(match (minimum, current) {
                                    (false, false) => {
                                        before_version::is_product_version(black_box("5.1.0"), args)
                                    }
                                    (false, true) => is_product_version(black_box("5.1.0"), args),
                                    (true, false) => before_version::is_minimum_product_version(
                                        black_box("5.1.0"),
                                        args,
                                    ),
                                    (true, true) => {
                                        is_minimum_product_version(black_box("5.1.0"), args)
                                    }
                                });
                            }
                        }
                    },
                );
            }
        }
    }
}
