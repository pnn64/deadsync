use super::*;
use mlua::Lua;

#[path = "values_before.rs"]
mod before;

#[test]
fn integer_coercion_keeps_rounding_precision_and_invalid_input() {
    let lua = Lua::new();
    for (text, expected) in [
        ("42", Some(42)),
        ("  -1.5  ", Some(-2)),
        ("\u{2003}123.75\u{a0}", Some(124)),
        ("16777217", Some(16777217)),
        ("16777217.0", Some(16777216)),
        ("2147483648", Some(i32::MAX)),
        ("-2147483649", Some(i32::MIN)),
        ("NaN", Some(0)),
        ("inf", Some(i32::MAX)),
        ("-inf", Some(i32::MIN)),
        ("1e1000", Some(i32::MAX)),
        ("-0.5", Some(-1)),
        ("0x10", None),
        ("1\0", None),
        ("", None),
        ("not-a-number", None),
    ] {
        let value = Value::String(lua.create_string(text).unwrap());
        assert_eq!(read_i32_value(value.clone()), expected, "{text:?}");
        assert_eq!(read_i32_value(value.clone()), before::read_i32_value(value));
    }
    for value in [
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NEG_INFINITY),
        Value::Number(-0.0),
        Value::Number(-1.5),
        Value::Number(16777217.0),
        Value::Number(f64::MAX),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string([0xff, b'1']).unwrap()),
    ] {
        assert_eq!(read_i32_value(value.clone()), before::read_i32_value(value));
    }
    for number in -1024..=1024 {
        for text in [
            number.to_string(),
            format!(" {number}.49999999 "),
            format!("{number}e-3"),
        ] {
            let value = Value::String(lua.create_string(text).unwrap());
            assert_eq!(read_i32_value(value.clone()), before::read_i32_value(value));
        }
    }
    assert_eq!(read_i32_value(Value::Number(f64::NAN)), None);
    assert_eq!(read_i32_value(Value::Number(16777217.0)), Some(16777217));
}

#[test]
fn equality_keeps_utf8_numeric_and_identity_semantics() {
    struct Marker;
    impl mlua::UserData for Marker {}
    let lua = Lua::new();
    let other = Lua::new();
    let table = lua.create_table().unwrap();
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    let userdata = lua.create_userdata(Marker).unwrap();
    let values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(0),
        Value::Integer(1),
        Value::Integer(i64::MAX),
        Value::Integer(9007199254740993),
        Value::Number(9007199254740992.0),
        Value::Number(1.0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::String(lua.create_string("").unwrap()),
        Value::String(lua.create_string("1").unwrap()),
        Value::String(lua.create_string("hello\0world").unwrap()),
        Value::String(lua.create_string("é曲").unwrap()),
        Value::String(other.create_string("é曲").unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
        Value::String(lua.create_string([0xff, 0]).unwrap()),
        Value::String(other.create_string([0xff]).unwrap()),
        Value::Table(table.clone()),
        Value::Table(table),
        Value::Table(lua.create_table().unwrap()),
        Value::Function(function.clone()),
        Value::Function(function.clone()),
        Value::Thread(lua.create_thread(function).unwrap()),
        Value::UserData(userdata.clone()),
        Value::UserData(userdata),
        Value::LightUserData(mlua::LightUserData(std::ptr::null_mut())),
    ];
    lua.gc_collect().unwrap();
    other.gc_collect().unwrap();
    for left in &values {
        for right in &values {
            assert_eq!(
                lua_values_equal(left, right),
                before::lua_values_equal(left, right)
            );
        }
    }
    let invalid = Value::String(lua.create_string([0xff; 4096]).unwrap());
    assert!(!lua_values_equal(&invalid, &invalid));
    assert!(!lua_values_equal(
        &Value::Number(f64::NAN),
        &Value::Number(f64::NAN)
    ));
    for text in ["a".repeat(65536), "é曲".repeat(4096), "x\0y".repeat(4096)] {
        let left = Value::String(lua.create_string(&text).unwrap());
        let right = Value::String(other.create_string(&text).unwrap());
        assert!(lua_values_equal(&left, &right));
        let mut changed = text.into_bytes();
        changed[0] = b'z';
        let different = Value::String(other.create_string(changed).unwrap());
        assert!(!lua_values_equal(&left, &different));
        assert_eq!(
            lua_values_equal(&left, &different),
            before::lua_values_equal(&left, &different)
        );
    }
}

#[test]
fn hexadecimal_output_keeps_every_byte_and_uppercase_format() {
    use std::fmt::Write;
    let lua = Lua::new();
    for size in [0, 1, 8, 15, 16, 20, 31, 32, 64, 255, 256, 4096, 65536] {
        let bytes: Vec<_> = (0..size).map(|index| index as u8).collect();
        let value = Value::String(lua.create_string(&bytes).unwrap());
        let actual = lua_binary_to_hex(value.clone());
        let mut expected = String::new();
        for byte in &bytes {
            write!(&mut expected, "{byte:02X}").unwrap();
        }
        assert_eq!(actual, expected);
        assert_eq!(actual, before::lua_binary_to_hex(value));
        let decoded: Vec<_> = actual
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        assert_eq!(decoded, bytes);
    }
    for value in [
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(42),
        Value::Number(1.5),
        Value::Table(lua.create_table().unwrap()),
    ] {
        assert!(lua_binary_to_hex(value.clone()).is_empty());
        assert_eq!(
            lua_binary_to_hex(value.clone()),
            before::lua_binary_to_hex(value)
        );
    }
}

#[test]
fn conversion_allocation_contracts_hold_for_valid_and_binary_strings() {
    use crate::draw_capture::perf::allocations;
    let lua = Lua::new();
    for text in ["42", " 123.75 ", "NaN", "bad", "é曲"] {
        let value = Value::String(lua.create_string(text).unwrap());
        // mlua can grow its handle storage on first use. Measure warmed reads.
        std::hint::black_box(read_i32_value(value.clone()));
        std::hint::black_box(lua_values_equal(&value, &value));
        allocations::assert_no_churn(|| {
            std::hint::black_box(read_i32_value(value.clone()));
            std::hint::black_box(lua_values_equal(&value, &value));
        });
    }
    let invalid = Value::String(lua.create_string([0xff; 4096]).unwrap());
    let (old, prior) = allocations::measure(|| before::lua_values_equal(&invalid, &invalid));
    let (new, current) = allocations::measure(|| lua_values_equal(&invalid, &invalid));
    assert!(!old && !new);
    assert!(prior.allocs > 0);
    assert_eq!(current, allocations::Churn::default());
    for size in [0, 1, 20, 256, 4096] {
        let value = Value::String(lua.create_string(vec![0xff; size]).unwrap());
        for current in [false, true] {
            std::hint::black_box(if current {
                lua_binary_to_hex(value.clone())
            } else {
                before::lua_binary_to_hex(value.clone())
            });
            let (output, counts) = allocations::measure(|| {
                if current {
                    lua_binary_to_hex(value.clone())
                } else {
                    before::lua_binary_to_hex(value.clone())
                }
            });
            assert_eq!(output.len(), 2 * size);
            assert_eq!(counts.allocs, usize::from(size != 0));
            assert_eq!(counts.allocated_bytes, 2 * size);
            assert_eq!(counts.reallocs, 0);
            assert_eq!(counts.frees, 0);
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_values() {
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
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"input_bytes":input_bytes,"baseline_commit":"54bde073a"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, text) in [
        ("integer_integral", "42"),
        ("integer_fractional", "123.75"),
        ("integer_unicode_trim", "\u{2003}123.75\u{a0}"),
        ("integer_nonfinite", "1e1000"),
        ("integer_invalid", "not-a-number"),
        ("integer_number_control", ""),
    ] {
        for current in variants {
            measure(name, current, "value_reads", text.len(), 10000, || {
                let lua = Lua::new();
                let value = if text.is_empty() {
                    Value::Number(123.75)
                } else {
                    Value::String(lua.create_string(text).unwrap())
                };
                black_box(if current {
                    read_i32_value(value.clone())
                } else {
                    before::read_i32_value(value.clone())
                });
                let fixture = (lua, value);
                move || {
                    let (_, value) = black_box(&fixture);
                    black_box(if current {
                        read_i32_value(value.clone())
                    } else {
                        before::read_i32_value(value.clone())
                    });
                }
            });
        }
    }
    for (name, text, different, cross_vm, count) in [
        ("equal_short", b"hello".to_vec(), false, false, 10000),
        ("equal_ascii_same", vec![b'a'; 65536], false, false, 256),
        ("equal_ascii_different", vec![b'a'; 65536], true, false, 256),
        ("equal_ascii_cross_vm", vec![b'a'; 65536], false, true, 256),
        (
            "equal_utf8_same",
            "é曲".repeat(4096).into_bytes(),
            false,
            false,
            256,
        ),
        (
            "equal_utf8_cross_vm",
            "é曲".repeat(4096).into_bytes(),
            false,
            true,
            256,
        ),
        ("equal_invalid", vec![0xff; 4096], false, false, 2000),
        ("equal_number_control", Vec::new(), false, false, 100000),
    ] {
        for current in variants {
            measure(
                name,
                current,
                "value_comparisons",
                text.len(),
                count,
                || {
                    let lua = Lua::new();
                    let other = Lua::new();
                    let left = if text.is_empty() {
                        Value::Integer(42)
                    } else {
                        Value::String(lua.create_string(&text).unwrap())
                    };
                    let mut right_bytes = text.clone();
                    if different {
                        right_bytes[0] = b'b';
                    }
                    let right = if text.is_empty() {
                        Value::Number(42.0)
                    } else {
                        Value::String(
                            (if cross_vm { &other } else { &lua })
                                .create_string(right_bytes)
                                .unwrap(),
                        )
                    };
                    assert_eq!(
                        lua_values_equal(&left, &right),
                        before::lua_values_equal(&left, &right)
                    );
                    let fixture = (lua, other, left, right);
                    move || {
                        let (_, _, left, right) = black_box(&fixture);
                        black_box(if current {
                            lua_values_equal(left, right)
                        } else {
                            before::lua_values_equal(left, right)
                        });
                    }
                },
            );
        }
    }
    for (name, size, count) in [
        ("hex_empty", 0, 10000),
        ("hex_8", 8, 10000),
        ("hex_digest_20", 20, 10000),
        ("hex_digest_32", 32, 10000),
        ("hex_256", 256, 2000),
        ("hex_4096", 4096, 256),
        ("hex_65536", 65536, 128),
        ("hex_nonstring_control", 0, 10000),
        ("hex_api_digest_20", 20, 2000),
        ("hex_api_4096", 4096, 128),
    ] {
        for current in variants {
            measure(
                name,
                current,
                if name.starts_with("hex_api_") {
                    "lua_calls"
                } else {
                    "hex_encodings"
                },
                size,
                count,
                || {
                    let lua = Lua::new();
                    let bytes: Vec<_> = (0..size).map(|index| index as u8).collect();
                    let value = if name == "hex_nonstring_control" {
                        Value::Nil
                    } else {
                        Value::String(lua.create_string(bytes).unwrap())
                    };
                    // Same helper/create_string path used by the BinaryToHex binding.
                    let callback = lua
                        .create_function(move |lua, value: Value| {
                            let output = if current {
                                lua_binary_to_hex(value)
                            } else {
                                before::lua_binary_to_hex(value)
                            };
                            Ok(Value::String(lua.create_string(output.as_str())?))
                        })
                        .unwrap();
                    callback.call::<Value>(value.clone()).unwrap();
                    let fixture = (lua, value, callback);
                    move || {
                        let (_, value, callback) = black_box(&fixture);
                        if name.starts_with("hex_api_") {
                            black_box(callback.call::<Value>(value.clone()).unwrap());
                        } else {
                            black_box(if current {
                                lua_binary_to_hex(value.clone())
                            } else {
                                before::lua_binary_to_hex(value.clone())
                            });
                        }
                    }
                },
            );
        }
    }
}
