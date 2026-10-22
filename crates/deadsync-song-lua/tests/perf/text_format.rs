use super::{
    format_number_and_suffix, seconds_to_hhmmss, seconds_to_mmss, seconds_to_mmss_ms_ms,
    seconds_to_mss, seconds_to_mss_ms_ms,
};
use crate::theme_colors::color_to_hex;
use mlua::{Lua, MultiValue, Value};

#[path = "text_format_before.rs"]
mod before;

type Clock = fn(f64) -> String;
const CLOCKS: [(&str, Clock, Clock); 5] = [
    ("hhmmss", before::seconds_to_hhmmss, seconds_to_hhmmss),
    ("mss", before::seconds_to_mss, seconds_to_mss),
    ("mmss", before::seconds_to_mmss, seconds_to_mmss),
    (
        "mss_centis",
        before::seconds_to_mss_ms_ms,
        seconds_to_mss_ms_ms,
    ),
    (
        "mmss_centis",
        before::seconds_to_mmss_ms_ms,
        seconds_to_mmss_ms_ms,
    ),
];

#[test]
fn clock_formats_keep_truncation_padding_signs_and_nonfinite_values() {
    for (_, old, new) in CLOCKS {
        for index in -4096..=4096 {
            let value = f64::from(index) * 0.125;
            assert_eq!(new(value), old(value), "seconds={value}");
        }
        for value in [
            -0.0,
            0.0,
            0.00999999,
            0.01,
            0.9999999,
            59.999999,
            60.0,
            3599.999999,
            3600.0,
            -1.25,
            -65.75,
            f64::NAN,
        ] {
            assert_eq!(new(value), old(value), "seconds={value}");
        }
        // The unchanged time-parts helper can overflow for huge finite inputs
        // in checked builds; release formatting retains its existing wrapping.
        if !cfg!(debug_assertions) {
            for value in [
                1e12,
                -1e12,
                f64::MAX,
                f64::MIN,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ] {
                assert_eq!(new(value), old(value), "seconds={value}");
            }
            let mut bits = 0x1234_5678_9abc_def0_u64;
            for _ in 0..1024 {
                bits ^= bits << 13;
                bits ^= bits >> 7;
                bits ^= bits << 17;
                let value = f64::from_bits(bits);
                assert_eq!(new(value), old(value), "bits={bits:#x}");
            }
        }
    }
    assert_eq!(seconds_to_hhmmss(65.75), "00:01:05");
    assert_eq!(seconds_to_mss(65.75), "1:05");
    assert_eq!(seconds_to_mmss(65.75), "01:05");
    assert_eq!(seconds_to_mss_ms_ms(65.75), "1:05.75");
    assert_eq!(seconds_to_mmss_ms_ms(65.75), "01:05.75");
    assert_eq!(seconds_to_mss_ms_ms(-65.75), "-1:-5.00");
}

#[test]
fn ordinal_formats_keep_negative_values_teen_suffixes_and_integer_extremes() {
    for value in -4096..=4096 {
        assert_eq!(
            format_number_and_suffix(value),
            before::format_number_and_suffix(value)
        );
    }
    for (value, expected) in [
        (0, "0th"),
        (1, "1st"),
        (2, "2nd"),
        (3, "3rd"),
        (11, "11th"),
        (12, "12th"),
        (13, "13th"),
        (21, "21st"),
        (111, "111th"),
        (112, "112th"),
        (113, "113th"),
        (-1, "-1th"),
        (i64::MIN, "-9223372036854775808th"),
        (i64::MAX, "9223372036854775807th"),
    ] {
        assert_eq!(format_number_and_suffix(value), expected);
    }
    for digits in 1..=18 {
        let value = 10_i64.pow(digits);
        for value in [value - 1, value, value + 1, -value - 1, -value, -value + 1] {
            assert_eq!(
                format_number_and_suffix(value),
                before::format_number_and_suffix(value)
            );
        }
    }
}

#[test]
fn rgba_hex_keeps_truncation_clamping_alpha_and_nonfinite_components() {
    assert_eq!(color_to_hex([0.25, 0.5, 0.75, 1.0]), "3F7FBFFF");
    assert_eq!(
        color_to_hex([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0]),
        "00FF0000"
    );
    let values = [
        f32::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        f32::MIN_POSITIVE,
        0.25,
        0.5,
        0.75,
        1.0,
        1.5,
        f32::INFINITY,
        f32::NAN,
    ];
    for r in values {
        for g in values {
            for b in values {
                for a in values {
                    let color = [r, g, b, a];
                    assert_eq!(color_to_hex(color), before::color_to_hex(color));
                }
            }
        }
    }
    for index in 0..=255 {
        let value = index as f32 / 255.0;
        for bits in [
            value.to_bits().saturating_sub(1),
            value.to_bits(),
            value.to_bits() + 1,
        ] {
            for component in 0..4 {
                let mut color = [0.0, 0.25, 0.5, 1.0];
                color[component] = f32::from_bits(bits);
                assert_eq!(color_to_hex(color), before::color_to_hex(color));
            }
        }
    }
}

#[test]
fn formatters_use_one_exact_allocation_and_keep_lua_callback_contracts() {
    use crate::draw_capture::perf::allocations;
    use std::hint::black_box;
    for (_, _, new) in CLOCKS {
        for value in [0.0, 65.75, -65.75, 1e6] {
            let (text, counts) = allocations::measure(|| black_box(new(value)));
            assert_eq!(counts.allocs, 1);
            assert_eq!(counts.reallocs, 0);
            assert_eq!(counts.allocated_bytes, text.len());
        }
    }
    for value in [1, 12345678, i64::MIN, i64::MAX] {
        let (old, prior) =
            allocations::measure(|| black_box(before::format_number_and_suffix(value)));
        let (new, current) = allocations::measure(|| black_box(format_number_and_suffix(value)));
        assert_eq!(old, new);
        assert_eq!(current.allocs, 1);
        assert_eq!(current.reallocs, 0);
        assert_eq!(current.allocated_bytes, new.len());
        if value == i64::MIN || value == 12345678 {
            assert!(prior.reallocs > 0);
        }
    }
    let (text, counts) = allocations::measure(|| color_to_hex([0.25, 0.5, 0.75, 1.0]));
    assert_eq!(text.len(), 8);
    assert_eq!(counts.allocs, 1);
    assert_eq!(counts.allocated_bytes, 8);
    assert_eq!(counts.reallocs, 0);
    let lua = Lua::new();
    for (_, old, new) in CLOCKS {
        let callbacks = [old, new].map(|format| {
            lua.create_function(move |lua, value: f64| {
                Ok(Value::String(lua.create_string(format(value))?))
            })
            .unwrap()
        });
        for source in [
            "return f(65.75)",
            "return f(-65.75)",
            "return f('1.25')",
            "return f({})",
            "return f()",
        ] {
            let outcomes = callbacks.each_ref().map(|callback| {
                lua.globals().set("f", callback.clone()).unwrap();
                lua.load(source)
                    .set_name("@text-format.lua")
                    .eval::<String>()
                    .map_err(|error| error.to_string())
            });
            assert_eq!(outcomes[0], outcomes[1]);
        }
    }
    let callbacks = [
        before::format_number_and_suffix as fn(i64) -> String,
        format_number_and_suffix,
    ]
    .map(|format| {
        lua.create_function(move |lua, value: i64| {
            Ok(Value::String(lua.create_string(format(value))?))
        })
        .unwrap()
    });
    for source in [
        "return f(21)",
        "return f(-1)",
        "return f('123')",
        "return f(9223372036854775807)",
        "return f(true)",
        "return f()",
    ] {
        let outcomes = callbacks.each_ref().map(|callback| {
            lua.globals().set("f", callback.clone()).unwrap();
            lua.load(source)
                .set_name("@text-format.lua")
                .eval::<String>()
                .map_err(|error| error.to_string())
        });
        assert_eq!(outcomes[0], outcomes[1]);
    }
    let callbacks = [before::color_to_hex as fn([f32; 4]) -> String, color_to_hex].map(|format| {
        lua.create_function(move |_, args: MultiValue| {
            Ok(format(
                args.front()
                    .cloned()
                    .and_then(crate::read_color_value)
                    .unwrap_or([1.0; 4]),
            ))
        })
        .unwrap()
    });
    for source in [
        "return f({.25,.5,.75,1})",
        "return f({1,0,0})",
        "return f('1,0,0,1')",
        "return f(string.char(255))",
        "return f()",
    ] {
        let outcomes = callbacks.each_ref().map(|callback| {
            lua.globals().set("f", callback.clone()).unwrap();
            lua.load(source)
                .set_name("@text-format.lua")
                .eval::<String>()
                .map_err(|error| error.to_string())
        });
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_text_format() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
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
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"baseline_commit":"95f55da32"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (label, old, new) in CLOCKS {
        for (case, seconds, api) in [
            ("normal", 65.75, false),
            ("large", 1e12, false),
            ("negative", -65.75, false),
            ("api", 65.75, true),
        ] {
            let name = format!("clock_{label}_{case}");
            for current in variants {
                measure(
                    &name,
                    current,
                    if api {
                        "lua_clock_calls"
                    } else {
                        "clock_strings"
                    },
                    if api { 2000 } else { 10000 },
                    || {
                        let lua = Lua::new();
                        let formatter = if current { new } else { old };
                        let callback = lua
                            .create_function(move |lua, value: f64| {
                                Ok(Value::String(lua.create_string(formatter(value))?))
                            })
                            .unwrap();
                        callback.call::<Value>(seconds).unwrap();
                        black_box(formatter(seconds));
                        let fixture = (lua, callback);
                        move || {
                            let (_, callback) = black_box(&fixture);
                            if api {
                                black_box(callback.call::<Value>(black_box(seconds)).unwrap());
                            } else {
                                black_box(formatter(black_box(seconds)));
                            }
                        }
                    },
                );
            }
        }
    }
    for (label, value, api) in [
        ("small", 1, false),
        ("resized", 12345678, false),
        ("minimum", i64::MIN, false),
        ("maximum", i64::MAX, false),
        ("api_small", 1, true),
        ("api_resized", 12345678, true),
    ] {
        let name = format!("ordinal_{label}");
        for current in variants {
            measure(
                &name,
                current,
                if api {
                    "lua_ordinal_calls"
                } else {
                    "ordinal_strings"
                },
                if api { 2000 } else { 10000 },
                || {
                    let lua = Lua::new();
                    let formatter = if current {
                        format_number_and_suffix as fn(i64) -> String
                    } else {
                        before::format_number_and_suffix
                    };
                    let callback = lua
                        .create_function(move |lua, value: i64| {
                            Ok(Value::String(lua.create_string(formatter(value))?))
                        })
                        .unwrap();
                    if api {
                        callback.call::<Value>(value).unwrap();
                    }
                    black_box(formatter(value));
                    let fixture = (lua, callback);
                    move || {
                        let (_, callback) = black_box(&fixture);
                        if api {
                            black_box(callback.call::<Value>(black_box(value)).unwrap());
                        } else {
                            black_box(formatter(black_box(value)));
                        }
                    }
                },
            );
        }
    }
    for (label, color, api) in [
        ("ordinary", [0.25, 0.5, 0.75, 1.0], false),
        (
            "nonfinite",
            [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0],
            false,
        ),
        ("api", [0.25, 0.5, 0.75, 1.0], true),
    ] {
        let name = format!("color_{label}");
        for current in variants {
            measure(
                &name,
                current,
                if api {
                    "lua_color_calls"
                } else {
                    "color_strings"
                },
                if api { 2000 } else { 10000 },
                || {
                    let lua = Lua::new();
                    let formatter = if current {
                        color_to_hex as fn([f32; 4]) -> String
                    } else {
                        before::color_to_hex
                    };
                    let value = Value::Table(lua.create_sequence_from(color).unwrap());
                    let callback = lua
                        .create_function(move |_, args: MultiValue| {
                            Ok(formatter(
                                args.front()
                                    .cloned()
                                    .and_then(crate::read_color_value)
                                    .unwrap_or([1.0; 4]),
                            ))
                        })
                        .unwrap();
                    callback.call::<Value>(value.clone()).unwrap();
                    black_box(formatter(color));
                    let fixture = (lua, value, callback);
                    move || {
                        let (_, value, callback) = black_box(&fixture);
                        if api {
                            black_box(callback.call::<Value>(value.clone()).unwrap());
                        } else {
                            black_box(formatter(black_box(color)));
                        }
                    }
                },
            );
        }
    }
}
