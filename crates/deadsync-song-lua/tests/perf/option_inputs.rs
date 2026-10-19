use super::*;
use serde_json::{Value as Json, json};

#[allow(dead_code, unused_imports)]
#[path = "option_inputs_before.rs"]
mod before;
#[path = "option_inputs_theme_before.rs"]
mod before_theme;

const METHODS: [&str; 12] = [
    "Theme",
    "EditorNoteskin",
    "DefaultFailType",
    "LongAndMarathonTime",
    "MusicWheelSpeed",
    "VideoRenderer",
    "GlobalOffsetSeconds",
    "VisualDelaySeconds",
    "MemoryCards",
    "CustomSongsMaxSeconds",
    "CustomSongsMaxMegabytes",
    "CustomSongsLoadTimeout",
];

macro_rules! values_snapshot {
    ($values:expr, $module:ident) => {
        match $values {
            $module::SongLuaOptionValues::Str(values) => json!(["strings", values]),
            $module::SongLuaOptionValues::Bool(values) => json!(["booleans", values]),
            $module::SongLuaOptionValues::Int(values) => json!(["integers", values]),
            $module::SongLuaOptionValues::Number(values) => json!([
                "numbers",
                values.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            ]),
        }
    };
}
macro_rules! spec_snapshot {
    ($row:expr, $module:ident) => {{
        let row = $row;
        let spec = row.spec;
        json!([
            row.row_name,
            row.pref_name,
            values_snapshot!(spec.choices, $module),
            spec.values.map(|values| values_snapshot!(values, $module)),
            spec.layout_type,
            spec.select_type,
            spec.one_choice_for_all_players,
            spec.export_on_change,
            spec.hide_on_disable,
            spec.reload_row_messages,
            spec.broadcast_on_export
        ])
    }};
}

#[test]
fn operator_specs_keep_all_fields_case_rules_kinds_and_original_fallbacks() {
    let mut methods: Vec<_> = METHODS.into_iter().map(str::to_owned).collect();
    methods.extend([
        String::new(),
        "unknown".into(),
        "é曲".into(),
        "Theme\0".into(),
        " Theme".into(),
        "Theme ".into(),
    ]);
    methods.extend([63, 64, 65, 8192].map(|len| "A".repeat(len)));
    for method in methods {
        let mixed: String = method
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i % 2 == 0 {
                    c.to_ascii_uppercase()
                } else {
                    c.to_ascii_lowercase()
                }
            })
            .collect();
        for name in [
            &method,
            &method.to_ascii_lowercase(),
            &method.to_ascii_uppercase(),
            &mixed,
        ] {
            for kind in [
                None,
                Some("Long"),
                Some("Marathon"),
                Some("mArAtHoN"),
                Some(""),
                Some("é\0"),
            ] {
                assert_eq!(
                    spec_snapshot!(operator_menu_option_row_spec(name, kind), super),
                    spec_snapshot!(before::operator_menu_option_row_spec(name, kind), before),
                    "name={name:?}, kind={kind:?}"
                );
            }
        }
    }
    let marathon = operator_menu_option_row_spec("LONGANDMARATHONTIME", Some("mArAtHoN"));
    assert_eq!(marathon.row_name, "mArAtHoN Time");
    assert_eq!(
        marathon.pref_name.as_deref(),
        Some("MarathonVerSongSeconds")
    );
    assert_eq!(
        operator_menu_option_row_spec("UnknownÉ", None).row_name,
        "UnknownÉ"
    );
}

fn table_values(table: Table) -> Vec<Json> {
    assert!(table.metatable().is_none());
    let len = table.raw_len();
    assert_eq!(table.clone().pairs::<Value, Value>().count(), len);
    assert!(matches!(table.raw_get::<Value>(0).unwrap(), Value::Nil));
    assert!(matches!(
        table.raw_get::<Value>(len + 1).unwrap(),
        Value::Nil
    ));
    (1..=len)
        .map(|index| match table.raw_get::<Value>(index).unwrap() {
            Value::String(text) => json!(["string", text.as_bytes().to_vec()]),
            Value::Boolean(value) => json!(["bool", value]),
            Value::Integer(value) => json!(["integer", value]),
            Value::Number(value) => json!(["number", value.to_bits()]),
            other => panic!("unexpected array value {other:?}"),
        })
        .collect()
}

#[test]
fn dense_option_arrays_keep_types_lengths_values_and_independent_mutation() {
    let lua = Lua::new();
    macro_rules! compare {
        ($kind:ident, $values:expr) => {{
            let old =
                before::create_lua_option_array(&lua, before::SongLuaOptionValues::$kind($values))
                    .unwrap();
            let new = create_lua_option_array(&lua, SongLuaOptionValues::$kind($values)).unwrap();
            assert_eq!(table_values(new.clone()), table_values(old));
            assert_eq!(new.raw_len(), $values.len());
        }};
    }
    compare!(Str, &[] as &[&str]);
    compare!(Bool, &[] as &[bool]);
    compare!(Int, &[] as &[i64]);
    compare!(Number, &[] as &[f64]);
    compare!(Str, &["", "a\0b", "é曲", "😀", "same", "same"]);
    compare!(Bool, &[false, true, false]);
    compare!(Int, &[i64::MIN, -1, 0, 1, i64::MAX]);
    compare!(
        Number,
        &[
            f64::NEG_INFINITY,
            -0.0,
            0.0,
            f64::INFINITY,
            f64::NAN,
            1.23456789012345e-200
        ]
    );
    compare!(Str, OPTION_OFFSET_MS);
    compare!(Number, OPTION_OFFSET_SECONDS_VALUES);
    // Lua 5.1 length searches depend on array capacity when authored code makes holes.
    for mask in 0..32 {
        let old = before::create_lua_option_array(
            &lua,
            before::SongLuaOptionValues::Int(&[0, 1, 2, 3, 4]),
        )
        .unwrap();
        let new =
            create_lua_option_array(&lua, SongLuaOptionValues::Int(&[0, 1, 2, 3, 4])).unwrap();
        for index in 1..=5 {
            if mask & (1 << (index - 1)) != 0 {
                old.raw_set(index, Value::Nil).unwrap();
                new.raw_set(index, Value::Nil).unwrap();
            }
        }
        assert_eq!(new.raw_len(), old.raw_len(), "hole mask={mask}");
    }
    const INTS65: [i64; 65] = [1; 65];
    for count in 0..=65 {
        for pattern in 0..16 {
            let old = before::create_lua_option_array(
                &lua,
                before::SongLuaOptionValues::Int(&INTS65[..count]),
            )
            .unwrap();
            let new =
                create_lua_option_array(&lua, SongLuaOptionValues::Int(&INTS65[..count])).unwrap();
            for index in 1..=count {
                if pattern & (1 << ((index - 1) % 4)) != 0 {
                    old.raw_set(index, Value::Nil).unwrap();
                    new.raw_set(index, Value::Nil).unwrap();
                }
            }
            assert_eq!(
                new.raw_len(),
                old.raw_len(),
                "length={count}, pattern={pattern}"
            );
            old.raw_set(count + 1, true).unwrap();
            new.raw_set(count + 1, true).unwrap();
            assert_eq!(
                new.raw_len(),
                old.raw_len(),
                "extended length={count}, pattern={pattern}"
            );
        }
    }
    let first = create_lua_option_array(&lua, SongLuaOptionValues::Bool(&[false, true])).unwrap();
    let second = create_lua_option_array(&lua, SongLuaOptionValues::Bool(&[false, true])).unwrap();
    first.raw_set(1, true).unwrap();
    first.raw_set(2, Value::Nil).unwrap();
    lua.gc_collect().unwrap();
    assert!(!second.raw_get::<bool>(1).unwrap());
    assert!(second.raw_get::<bool>(2).unwrap());
}

fn theme_fixture(lua: &Lua, current: bool) -> Table {
    let theme =
        crate::create_theme_table(lua, &crate::SongLuaCompileContext::new("", "option-inputs"))
            .unwrap();
    if !current {
        let old = before_theme::create_availability_table(lua).unwrap();
        for name in ["DoesThemeExist", "IsThemeSelectable", "DoesLanguageExist"] {
            theme.set(name, old.get::<Function>(name).unwrap()).unwrap();
        }
    }
    theme
}

#[test]
fn availability_callbacks_keep_validation_dispatch_and_gc_roots() {
    let lua = Lua::new();
    let old = theme_fixture(&lua, false);
    let new = theme_fixture(&lua, true);
    let mut values: Vec<_> = [
        "en",
        "EN",
        "EnGlIsH",
        "english",
        SONG_LUA_THEME_NAME,
        "SIMPLY LOVE",
        "",
        "unknown",
        " en",
        "en ",
        "en\0",
        "é曲",
        "ＥＮ",
    ]
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
    for method in ["DoesThemeExist", "IsThemeSelectable", "DoesLanguageExist"] {
        let prior: Function = old.get(method).unwrap();
        let current: Function = new.get(method).unwrap();
        for value in &values {
            assert_eq!(
                current.call::<bool>((new.clone(), value.clone())).unwrap(),
                prior.call::<bool>((old.clone(), value.clone())).unwrap()
            );
            assert_eq!(
                current.call::<bool>(value.clone()).unwrap(),
                prior.call::<bool>(value.clone()).unwrap()
            );
        }
        assert!(!current.call::<bool>(()).unwrap());
        assert!(!current.call::<bool>(new.clone()).unwrap());
    }
    lua.globals().set("THEME", new).unwrap();
    lua.load(r#"
        assert(THEME:DoesLanguageExist('EnGlIsH'))
        assert(THEME.DoesLanguageExist('EN'))
        assert(not THEME:DoesLanguageExist(' en'))
        assert(not THEME:DoesLanguageExist(setmetatable({}, {__tostring=function() error('no coercion') end})))
        assert(THEME:IsThemeSelectable(THEME:GetCurThemeName()))
    "#).exec().unwrap();
}

#[test]
fn operator_row_callbacks_keep_choices_selection_and_saved_preferences() {
    for current in [false, true] {
        let lua = Lua::new();
        let rows = if current {
            create_operator_menu_option_rows_table(&lua)
        } else {
            before::create_operator_menu_option_rows_table(&lua)
        }
        .unwrap();
        lua.globals().set("ROWS", rows).unwrap();
        lua.load(
            r#"
            PREFSMAN={SetPreference=function(self,key,value) SAVED_KEY=key;SAVED_VALUE=value end}
            local row=ROWS:LongAndMarathonTime('mArAtHoN')
            assert(row.Name=='mArAtHoN Time')
            assert(row.LayoutType=='ShowOneInRow' and row.OneChoiceForAllPlayers)
            local list={false,false,false}
            assert(row:LoadSelections(list)==list and list[1]==true)
            list[1]=false;list[2]=true
            row:SaveSelections(list)
            assert(SAVED_KEY=='MarathonVerSongSeconds' and SAVED_VALUE==row.Values[2])
            local card=ROWS:MemoryCards()
            assert(card.Choices[1]=='Off' and card.Values[1]==false and card.Values[2]==true)
            local unknown=ROWS["UnknownÉ"]()
            assert(unknown.Name=='UnknownÉ' and unknown.Choices[1]=='Off')
            local enabled=card:EnabledForPlayers()
            assert(enabled[1]=='PlayerNumber_P1' and enabled[2]=='PlayerNumber_P2')
            collectgarbage('collect')
            assert(row.Values[2]==SAVED_VALUE)
        "#,
        )
        .exec()
        .unwrap();
    }
}

#[test]
fn option_and_availability_allocations_remove_temporary_text_and_array_growth() {
    use crate::draw_capture::perf::allocations;
    use std::hint::black_box;
    for name in [
        "Theme",
        "LongAndMarathonTime",
        "CustomSongsMaxMegabytes",
        "unknown",
    ] {
        let (old, prior) = allocations::measure(|| {
            before::operator_menu_option_row_spec(black_box(name), Some("Marathon"))
        });
        let (new, current) = allocations::measure(|| {
            operator_menu_option_row_spec(black_box(name), Some("Marathon"))
        });
        assert_eq!(spec_snapshot!(new, super), spec_snapshot!(old, before));
        assert_eq!(prior.allocs, current.allocs + 1);
        assert_eq!(prior.allocated_bytes, current.allocated_bytes + name.len());
    }
    let lua = Lua::new();
    let values: &[i64] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    black_box(
        before::create_lua_option_array(&lua, before::SongLuaOptionValues::Int(values)).unwrap(),
    );
    black_box(create_lua_option_array(&lua, SongLuaOptionValues::Int(values)).unwrap());
    let (old, prior) = allocations::measure(|| {
        before::create_lua_option_array(&lua, before::SongLuaOptionValues::Int(values)).unwrap()
    });
    let (new, current) = allocations::measure(|| {
        create_lua_option_array(&lua, SongLuaOptionValues::Int(values)).unwrap()
    });
    assert_eq!(table_values(new), table_values(old));
    assert_eq!(prior.allocs, current.allocs);
    assert_eq!(prior.reallocs, 4);
    assert_eq!(current.reallocs, 0);
    assert!(current.allocated_bytes < prior.allocated_bytes);
    for (method, input, removed) in [
        ("DoesLanguageExist", "EnGlIsH", 2),
        ("DoesThemeExist", SONG_LUA_THEME_NAME, 1),
        ("IsThemeSelectable", SONG_LUA_THEME_NAME, 1),
    ] {
        let old = theme_fixture(&lua, false);
        let new = theme_fixture(&lua, true);
        let prior: Function = old.get(method).unwrap();
        let current: Function = new.get(method).unwrap();
        let text = lua.create_string(input).unwrap();
        lua.gc_collect().unwrap();
        for _ in 0..4 {
            prior.call::<bool>((old.clone(), text.clone())).unwrap();
            current.call::<bool>((new.clone(), text.clone())).unwrap();
        }
        let (old_result, prior_counts) =
            allocations::measure(|| prior.call::<bool>((old.clone(), text.clone())).unwrap());
        let (new_result, current_counts) =
            allocations::measure(|| current.call::<bool>((new.clone(), text.clone())).unwrap());
        assert_eq!(old_result, new_result);
        assert_eq!(prior_counts.allocs, current_counts.allocs + removed);
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_option_inputs() {
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
            let begin = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..count {
                    work();
                }
            });
            let used = cycles() - begin;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(json!({"ns":nanos,"thread_cycles":used,"units":count,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            json!({"workload":name,"variant":if current{"after"}else{"before"},"samples":samples,"unit":unit,"baseline_commit":"af4d42368"})
        );
    }
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, input, kind) in [
        ("spec_theme", "Theme".to_string(), None),
        (
            "spec_max_megabytes",
            "CustomSongsMaxMegabytes".to_string(),
            None,
        ),
        (
            "spec_marathon",
            "LongAndMarathonTime".to_string(),
            Some("mArAtHoN"),
        ),
        ("spec_unknown", "unknown".to_string(), None),
        ("spec_empty", String::new(), None),
        ("spec_boundary64", "A".repeat(64), None),
        ("spec_long65", "A".repeat(65), None),
    ] {
        for current in variants {
            measure(name, current, "operator_specs", 10000, || {
                let input = input.clone();
                move || {
                    if current {
                        black_box(operator_menu_option_row_spec(
                            black_box(&input),
                            black_box(kind),
                        ));
                    } else {
                        black_box(before::operator_menu_option_row_spec(
                            black_box(&input),
                            black_box(kind),
                        ));
                    }
                }
            });
        }
    }
    const INTS65: [i64; 65] = {
        let mut a = [0; 65];
        let mut i = 0;
        while i < 65 {
            a[i] = i as i64;
            i += 1;
        }
        a
    };
    for (name, kind) in [
        ("array_empty", 0),
        ("array_one", 1),
        ("array_three", 2),
        ("array_ten", 3),
        ("array_65", 4),
        ("array_strings", 5),
        ("array_bools", 6),
        ("array_numbers", 7),
    ] {
        for current in variants {
            measure(name, current, "option_arrays", 1000, || {
                let lua = Lua::new();
                let strings: Vec<_> = OPTION_OFFSET_MS
                    .iter()
                    .map(|text| lua.create_string(text).unwrap())
                    .collect();
                let make = move |lua: &Lua| {
                    if current {
                        create_lua_option_array(
                            lua,
                            match kind {
                                0 => SongLuaOptionValues::Int(&[]),
                                1 => SongLuaOptionValues::Int(&INTS65[..1]),
                                2 => SongLuaOptionValues::Int(&INTS65[..3]),
                                3 => SongLuaOptionValues::Int(&INTS65[..10]),
                                4 => SongLuaOptionValues::Int(&INTS65),
                                5 => SongLuaOptionValues::Str(OPTION_OFFSET_MS),
                                6 => SongLuaOptionValues::Bool(OPTION_FALSE_TRUE),
                                _ => SongLuaOptionValues::Number(OPTION_OFFSET_SECONDS_VALUES),
                            },
                        )
                    } else {
                        before::create_lua_option_array(
                            lua,
                            match kind {
                                0 => before::SongLuaOptionValues::Int(&[]),
                                1 => before::SongLuaOptionValues::Int(&INTS65[..1]),
                                2 => before::SongLuaOptionValues::Int(&INTS65[..3]),
                                3 => before::SongLuaOptionValues::Int(&INTS65[..10]),
                                4 => before::SongLuaOptionValues::Int(&INTS65),
                                5 => before::SongLuaOptionValues::Str(OPTION_OFFSET_MS),
                                6 => before::SongLuaOptionValues::Bool(OPTION_FALSE_TRUE),
                                _ => before::SongLuaOptionValues::Number(
                                    OPTION_OFFSET_SECONDS_VALUES,
                                ),
                            },
                        )
                    }
                };
                lua.gc_collect().unwrap();
                for _ in 0..4 {
                    black_box(make(&lua).unwrap());
                }
                let fixture = (lua, strings);
                move || {
                    let (lua, _) = black_box(&fixture);
                    black_box(make(lua).unwrap());
                }
            });
        }
    }
    for (name, method, kind) in [
        ("row_theme", "Theme", None),
        ("row_offset", "GlobalOffsetSeconds", None),
        ("row_marathon", "LongAndMarathonTime", Some("Marathon")),
        ("row_unknown", "Unknown", None),
    ] {
        for current in variants {
            measure(name, current, "operator_row_calls", 256, || {
                let lua = Lua::new();
                let rows = if current {
                    create_operator_menu_option_rows_table(&lua)
                } else {
                    before::create_operator_menu_option_rows_table(&lua)
                }
                .unwrap();
                let callback: Function = rows.get(method).unwrap();
                let argument = kind
                    .map(|text| Value::String(lua.create_string(text).unwrap()))
                    .unwrap_or(Value::Nil);
                lua.gc_collect().unwrap();
                for _ in 0..4 {
                    black_box(
                        callback
                            .call::<Table>((rows.clone(), argument.clone()))
                            .unwrap(),
                    );
                }
                let fixture = (lua, rows, callback, argument);
                move || {
                    let (_, rows, callback, argument) = black_box(&fixture);
                    black_box(
                        callback
                            .call::<Table>((rows.clone(), argument.clone()))
                            .unwrap(),
                    );
                }
            });
        }
    }
    for (name, method, input) in [
        ("language_en", "DoesLanguageExist", b"en".to_vec()),
        ("language_english", "DoesLanguageExist", b"EnGlIsH".to_vec()),
        ("language_unknown", "DoesLanguageExist", b"unknown".to_vec()),
        ("language_invalid", "DoesLanguageExist", vec![0xff; 32]),
        (
            "theme_exists",
            "DoesThemeExist",
            SONG_LUA_THEME_NAME.as_bytes().to_vec(),
        ),
        (
            "theme_selectable",
            "IsThemeSelectable",
            SONG_LUA_THEME_NAME.as_bytes().to_vec(),
        ),
    ] {
        for current in variants {
            measure(name, current, "availability_calls", 4000, || {
                let lua = Lua::new();
                let theme = theme_fixture(&lua, current);
                let callback: Function = theme.get(method).unwrap();
                let argument = lua.create_string(&input).unwrap();
                lua.gc_collect().unwrap();
                for _ in 0..4 {
                    black_box(
                        callback
                            .call::<bool>((theme.clone(), argument.clone()))
                            .unwrap(),
                    );
                }
                let fixture = (lua, theme, callback, argument);
                move || {
                    let (_, theme, callback, argument) = black_box(&fixture);
                    black_box(
                        callback
                            .call::<bool>((theme.clone(), argument.clone()))
                            .unwrap(),
                    );
                }
            });
        }
    }
}
