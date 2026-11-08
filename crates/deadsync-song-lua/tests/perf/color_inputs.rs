use super::*;
use crate::top_screen::option_row_default_text;

#[allow(dead_code, unused_imports)]
#[path = "color_inputs_before.rs"]
mod before;
#[path = "color_inputs_option_before.rs"]
mod before_option;

fn bits(value: Option<[f32; 4]>) -> Option<[u32; 4]> {
    value.map(|color| color.map(f32::to_bits))
}
type NameColor = fn(&str) -> Option<[f32; 4]>;
type ValueColor = fn(Value) -> Option<[f32; 4]>;
const NAMES: [&str; 50] = [
    "Beginner",
    "Difficulty_Beginner",
    "Easy",
    "Difficulty_Easy",
    "Freestyle",
    "Medium",
    "Difficulty_Medium",
    "HalfDouble",
    "Hard",
    "Difficulty_Hard",
    "Crazy",
    "Challenge",
    "Difficulty_Challenge",
    "Nightmare",
    "Edit",
    "Difficulty_Edit",
    "Couple",
    "Routine",
    "Stage_1st",
    "Stage_2nd",
    "Stage_3rd",
    "Stage_4th",
    "Stage_5th",
    "Stage_6th",
    "Stage_Next",
    "Stage_Final",
    "Stage_Extra1",
    "Stage_Extra2",
    "Stage_Nonstop",
    "Stage_Oni",
    "Stage_Endless",
    "Stage_Event",
    "Stage_Demo",
    "JudgmentLine_W1",
    "JudgmentLine_W2",
    "JudgmentLine_W3",
    "JudgmentLine_W4",
    "JudgmentLine_W5",
    "JudgmentLine_Held",
    "JudgmentLine_Miss",
    "JudgmentLine_MaxCombo",
    "",
    "unknown",
    "stage_1st",
    "HARD",
    " Hard",
    "Hard ",
    "Hard\0",
    "é曲",
    "Ｓｔａｇｅ",
];
const NAME_READERS: [(NameColor, NameColor); 3] = [
    (before::custom_difficulty_color, custom_difficulty_color),
    (before::stage_color, stage_color),
    (before::judgment_line_color, judgment_line_color),
];
const VALUE_READERS: [(ValueColor, ValueColor); 3] = [
    (
        before::custom_difficulty_color_value,
        custom_difficulty_color_value,
    ),
    (before::stage_color_value, stage_color_value),
    (before::judgment_line_color_value, judgment_line_color_value),
];

#[test]
fn fixed_named_colors_keep_all_aliases_fallbacks_and_float_bits() {
    for (old, new) in NAME_READERS {
        for name in NAMES {
            assert_eq!(bits(new(name)), bits(old(name)), "name={name:?}");
            for index in 0..name.len() {
                if !name.is_char_boundary(index) {
                    continue;
                }
                let mut changed = name.as_bytes().to_vec();
                changed[index] = b'?';
                if let Ok(changed) = std::str::from_utf8(&changed) {
                    assert_eq!(bits(new(changed)), bits(old(changed)));
                }
            }
        }
    }
    for player in [0, 1, 2, usize::MAX] {
        assert_eq!(
            song_lua_player_dark_color(player).map(f32::to_bits),
            before::song_lua_player_dark_color(player).map(f32::to_bits)
        );
    }
    for difficulty in -16..=16 {
        for decorative in [false, true] {
            assert_eq!(
                song_lua_difficulty_color(difficulty, decorative).map(f32::to_bits),
                before::song_lua_difficulty_color(difficulty, decorative).map(f32::to_bits)
            );
        }
    }
    assert_eq!(
        bits(stage_color("Stage_1st")),
        bits(Some([0.0, 1.0, 199.0 / 255.0, 1.0]))
    );
    assert!(stage_color("stage_1st").is_none());
    assert!(custom_difficulty_color("Hard ").is_none());
    assert!(judgment_line_color("JudgmentLine_W6").is_none());
}

#[test]
fn borrowed_color_names_keep_types_gc_roots_and_steps_callbacks() {
    let lua = Lua::new();
    let mut values: Vec<_> = NAMES
        .into_iter()
        .map(|text| Value::String(lua.create_string(text).unwrap()))
        .collect();
    values.extend([
        Value::Nil,
        Value::Boolean(true),
        Value::Integer(1),
        Value::Number(f64::NAN),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string([0xff; 32]).unwrap()),
    ]);
    lua.gc_collect().unwrap();
    for (old, new) in VALUE_READERS {
        for value in &values {
            assert_eq!(bits(new(value.clone())), bits(old(value.clone())));
        }
    }
    let callbacks = [
        before::steps_or_trail_color as ValueColor,
        steps_or_trail_color,
    ];
    for source in [
        "calls=0; return {GetDifficulty=function(self) calls=calls+1;return 'Hard' end}",
        "calls=0; return setmetatable({}, {__index=function(self,key) calls=calls+1; return function(self) calls=calls+1; return 'Easy' end end})",
        "calls=0; return {GetDifficulty=function() calls=calls+1;error('probe') end}",
        "calls=0; return {GetDifficulty=false}",
    ] {
        let outcomes = callbacks.map(|callback| {
            let value = lua.load(source).eval::<Value>().unwrap();
            let color = callback(value);
            (bits(color), lua.globals().get::<i64>("calls").unwrap())
        });
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn option_default_matching_keeps_case_and_custom_fallbacks() {
    for name in [
        "SpeedModType",
        "SpeedMod",
        "Mini",
        "Perspective",
        "NoteSkin",
        "NoteSkinVariant",
        "MusicRate",
        "JudgmentGraphic",
        "ComboFont",
        "BackgroundFitMode",
        "unknown",
        "",
        "é曲",
        "SpeedMod\0",
    ] {
        for variant in [
            name.to_string(),
            name.to_ascii_uppercase(),
            name.to_ascii_lowercase(),
            format!(" {name}"),
            format!("{name} "),
        ] {
            assert_eq!(
                option_row_default_text(&variant),
                before_option::option_row_default_text(&variant),
                "name={variant:?}"
            );
        }
        for (index, byte) in name.bytes().enumerate() {
            if !byte.is_ascii_alphabetic() {
                continue;
            }
            let mut variant = name.as_bytes().to_vec();
            variant[index] = byte.to_ascii_uppercase();
            let variant = std::str::from_utf8(&variant).unwrap();
            assert_eq!(
                option_row_default_text(variant),
                before_option::option_row_default_text(variant)
            );
        }
    }
    for name in ["unknown".repeat(1024), "é曲".repeat(1024)] {
        assert_eq!(
            option_row_default_text(&name),
            before_option::option_row_default_text(&name)
        );
    }
    assert_eq!(option_row_default_text("sPeEdMoDtYpE"), "X");
    assert_eq!(option_row_default_text("nOtEsKiNvArIaNt"), "default");
    assert_eq!(option_row_default_text("MiNi"), "0%");
}

#[test]
fn color_name_and_option_allocations_drop_temporary_strings() {
    use crate::draw_capture::perf::allocations;
    use std::hint::black_box;
    let lua = Lua::new();
    for ((old, new), name) in
        VALUE_READERS
            .into_iter()
            .zip(["Hard", "Stage_1st", "JudgmentLine_W1"])
    {
        let value = Value::String(lua.create_string(name).unwrap());
        black_box(old(value.clone()));
        black_box(new(value.clone()));
        let (_, prior) = allocations::measure(|| {
            black_box(old(value.clone()));
        });
        let (_, current) = allocations::measure(|| {
            black_box(new(value.clone()));
        });
        assert_eq!(prior.allocs, 1);
        assert_eq!(current, allocations::Churn::default());
    }
    for (name, output) in [
        ("SpeedModType", "X"),
        ("NoteSkinVariant", "default"),
        ("unknown", ""),
    ] {
        let (old, prior) =
            allocations::measure(|| black_box(before_option::option_row_default_text(name)));
        let (new, current) = allocations::measure(|| black_box(option_row_default_text(name)));
        assert_eq!(old, new);
        assert_eq!(new, output);
        assert_eq!(prior.allocs, current.allocs + 1);
        assert_eq!(current.reallocs, 0);
        assert_eq!(current.allocated_bytes, output.len());
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_color_inputs() {
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
            serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"samples":samples,"unit":unit,"baseline_commit":"552763528"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, kind, input) in [
        ("fixed_custom", 0, "Hard"),
        ("fixed_stage", 1, "Stage_1st"),
        ("fixed_judgment", 2, "JudgmentLine_W1"),
        ("fixed_dark_player", 3, ""),
        ("fixed_edit", 4, ""),
        ("fixed_unknown", 1, "Stage_Unknown"),
    ] {
        for current in variants {
            measure(name, current, "named_colors", 100000, || {
                move || {
                    let result = match kind {
                        0 => {
                            if current {
                                custom_difficulty_color(black_box(input))
                            } else {
                                before::custom_difficulty_color(black_box(input))
                            }
                        }
                        1 => {
                            if current {
                                stage_color(black_box(input))
                            } else {
                                before::stage_color(black_box(input))
                            }
                        }
                        2 => {
                            if current {
                                judgment_line_color(black_box(input))
                            } else {
                                before::judgment_line_color(black_box(input))
                            }
                        }
                        3 => Some(if current {
                            song_lua_player_dark_color(black_box(0))
                        } else {
                            before::song_lua_player_dark_color(black_box(0))
                        }),
                        _ => Some(if current {
                            song_lua_difficulty_color(black_box(5), black_box(false))
                        } else {
                            before::song_lua_difficulty_color(black_box(5), black_box(false))
                        }),
                    };
                    black_box(result);
                }
            });
        }
    }
    for (name, kind, bytes, numeric) in [
        ("name_custom", 0, b"Hard".to_vec(), false),
        ("name_custom_unknown", 0, b"unknown".to_vec(), false),
        ("name_stage", 1, b"Stage_1st".to_vec(), false),
        ("name_stage_unknown", 1, b"unknown".to_vec(), false),
        ("name_judgment", 2, b"JudgmentLine_W1".to_vec(), false),
        ("name_judgment_unknown", 2, b"unknown".to_vec(), false),
        ("name_invalid", 1, vec![0xff; 32], false),
        ("name_nonstring", 1, Vec::new(), true),
    ] {
        for current in variants {
            measure(name, current, "color_name_reads", 10000, || {
                let lua = Lua::new();
                let value = if numeric {
                    Value::Integer(1)
                } else {
                    Value::String(lua.create_string(&bytes).unwrap())
                };
                let (old, new) = VALUE_READERS[kind];
                let reader = if current { new } else { old };
                black_box(reader(value.clone()));
                let fixture = (lua, value);
                move || {
                    let (_, value) = black_box(&fixture);
                    black_box(reader(value.clone()));
                }
            });
        }
    }
    for (name, kind, input) in [
        ("api_custom", 0, "Hard"),
        ("api_stage", 1, "Stage_1st"),
        ("api_judgment_stroke", 2, "JudgmentLine_W1"),
        ("api_steps", 3, "Hard"),
    ] {
        for current in variants {
            measure(name, current, "lua_color_calls", 2000, || {
                let lua = Lua::new();
                let value = if kind == 3 {
                    lua.load("return {GetDifficulty=function(self) return 'Hard' end}")
                        .eval::<Value>()
                        .unwrap()
                } else {
                    Value::String(lua.create_string(input).unwrap())
                };
                let callback = lua
                    .create_function(move |lua, args: MultiValue| {
                        let color = if kind == 3 {
                            args.front().cloned().and_then(if current {
                                steps_or_trail_color
                            } else {
                                before::steps_or_trail_color
                            })
                        } else {
                            let (old, new) = VALUE_READERS[kind];
                            args.front()
                                .cloned()
                                .and_then(if current { new } else { old })
                        };
                        let color = if kind == 2 {
                            color.map(|color| tone_color(color, 0.5))
                        } else {
                            color
                        };
                        crate::make_color_table(
                            lua,
                            color.unwrap_or(if kind == 1 || kind == 2 {
                                [0.0, 0.0, 0.0, 1.0]
                            } else {
                                [1.0; 4]
                            }),
                        )
                    })
                    .unwrap();
                for _ in 0..4 {
                    callback.call::<Table>(value.clone()).unwrap();
                }
                let fixture = (lua, value, callback);
                move || {
                    let (_, value, callback) = black_box(&fixture);
                    black_box(callback.call::<Table>(value.clone()).unwrap());
                }
            });
        }
    }
    for (name, input) in [
        ("option_speed", "SpeedModType".to_string()),
        ("option_mini", "Mini".to_string()),
        ("option_perspective", "Perspective".to_string()),
        ("option_noteskin", "NoteSkinVariant".to_string()),
        ("option_custom", "JudgmentGraphic".to_string()),
        ("option_unknown", "unknown".to_string()),
        ("option_empty", String::new()),
        ("option_long_unknown", "unknown".repeat(1024)),
    ] {
        for current in variants {
            measure(name, current, "option_defaults", 10000, || {
                let input = input.clone();
                move || {
                    black_box(if current {
                        option_row_default_text(black_box(&input))
                    } else {
                        before_option::option_row_default_text(black_box(&input))
                    });
                }
            });
        }
    }
    for (name, input) in [
        ("parser_hex_control", "#FF5D47"),
        ("parser_csv_control", "1,.5,.25,1"),
    ] {
        for current in variants {
            measure(name, current, "color_parses", 10000, || {
                move || {
                    black_box(if current {
                        parse_color_text(black_box(input))
                    } else {
                        before::parse_color_text(black_box(input))
                    });
                }
            });
        }
    }
}
