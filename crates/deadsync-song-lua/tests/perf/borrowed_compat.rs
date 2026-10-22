use super::*;

#[path = "borrowed_compat_before.rs"]
mod before;

fn prepare() -> Lua {
    let lua = Lua::new();
    install_default_stdlib_compat(&lua, Path::new(".")).unwrap();
    before::install(&lua).unwrap();
    lua
}

// Compare the public callbacks, including their result shape and error behavior.
fn outcome(lua: &Lua, name: &str, args: &MultiValue) -> Result<String, String> {
    let value = lua
        .globals()
        .get::<Function>(name)
        .unwrap()
        .call::<Value>(args)
        .map_err(|e| e.to_string())?;
    Ok(match value {
        Value::String(text) => format!("{:?}", text.as_bytes()),
        Value::Table(table) if name.starts_with("Var") => {
            let mut keys: Vec<_> = table
                .pairs::<String, Value>()
                .map(|v| {
                    let (key, value) = v.unwrap();
                    (key, value.type_name())
                })
                .collect();
            keys.sort();
            assert_eq!(
                table
                    .get::<Function>("GetIndex")
                    .unwrap()
                    .call::<i64>(&table)
                    .unwrap(),
                0
            );
            assert_eq!(
                table
                    .get::<Function>("GetName")
                    .unwrap()
                    .call::<String>(&table)
                    .unwrap(),
                ""
            );
            format!("{keys:?}")
        }
        Value::Table(table) => {
            assert!(table.metatable().is_none());
            let len = table.raw_len();
            let values: Vec<_> = table.sequence_values::<i64>().map(Result::unwrap).collect();
            format!("{len}:{values:?}")
        }
        other => panic!("unexpected callback result: {other:?}"),
    })
}

#[test]
fn borrowed_compat_callbacks_keep_defaults_binary_text_and_result_shapes() {
    let lua = prepare();
    let mut values: Vec<Value> = [
        "",
        "GameCommand",
        "gamecommand",
        "GameCommand\0",
        "song",
        "é曲🙂",
        "a\0b",
        " 5.1.0-alpha.7 ",
        "1..3",
        "9999999999999999999999.2.3",
        "a.b.c",
    ]
    .iter()
    .map(|s| Value::String(lua.create_string(s).unwrap()))
    .collect();
    values.extend([
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(42),
        Value::Number(f64::NAN),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string(b"bad\xff").unwrap()),
        Value::String(lua.create_string("x".repeat(4096)).unwrap()),
    ]);
    let hostile: Table = lua
        .load("return setmetatable({}, {__tostring=function() error('must not coerce') end})")
        .eval()
        .unwrap();
    values.push(Value::Table(hostile));
    values.push(Value::Function(
        lua.create_function(|_, ()| Ok(())).unwrap(),
    ));
    values.push(lua.load("return newproxy(true)").eval::<Value>().unwrap());
    values.push(Value::Thread(
        lua.create_thread(lua.load("return function() end").eval().unwrap())
            .unwrap(),
    ));
    for name in ["Var", "GetVersionParts"] {
        for value in &values {
            let args: MultiValue = [value.clone(), Value::Boolean(true)].into_iter().collect();
            assert_eq!(
                outcome(&lua, &format!("{name}Before"), &args),
                outcome(&lua, name, &args),
                "{name}: {value:?}"
            );
        }
        assert_eq!(
            outcome(&lua, &format!("{name}Before"), &MultiValue::new()),
            outcome(&lua, name, &MultiValue::new())
        );
    }
    // Every input slot keeps its independent fallback; replacement is literal.
    for text in &values {
        for needle in &values {
            for replacement in &values {
                let args: MultiValue = [text.clone(), needle.clone(), replacement.clone()]
                    .into_iter()
                    .collect();
                assert_eq!(
                    outcome(&lua, "cleanGSubBefore", &args),
                    outcome(&lua, "cleanGSub", &args)
                );
            }
        }
    }
    for source in [
        "return f()",
        "return f('a.b.a', '.', '%1')",
        "return f('éé', 'é')",
        "return f(string.char(255), '', 'x')",
        "return f('aaa', 'a', 'b', 99)",
    ] {
        let results = ["cleanGSubBefore", "cleanGSub"].map(|name| {
            lua.globals()
                .set("f", lua.globals().get::<Function>(name).unwrap())
                .unwrap();
            lua.load(source)
                .set_name("@borrowed-compat.lua")
                .eval::<String>()
                .map_err(|e| e.to_string())
        });
        assert_eq!(results[0], results[1]);
    }
    for name in ["VarBefore", "Var"] {
        lua.globals()
            .set("f", lua.globals().get::<Function>(name).unwrap())
            .unwrap();
        let result = lua
            .load(
                r#"
                local finalized = 0
                local function input()
                    local value = newproxy(true)
                    getmetatable(value).__tostring = function() error('must not coerce') end
                    getmetatable(value).__gc = function() finalized = finalized + 1 end
                    return value
                end
                local result = f(input())
                collectgarbage('collect')
                collectgarbage('collect')
                return result, finalized
                "#,
            )
            .eval::<(String, i64)>()
            .unwrap();
        assert_eq!(result, (String::new(), 1));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_borrowed_compat_callbacks() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, callback, expression) in [
        ("gsub_short", "cleanGSub", "f('folder/song.lua', '/', '-')"),
        ("gsub_unicode", "cleanGSub", "f('é曲🙂é', 'é', '🎵')"),
        (
            "gsub_long",
            "cleanGSub",
            "f(text, 'missing', 'replacement')",
        ),
        ("gsub_empty", "cleanGSub", "f('', '', '')"),
        ("gsub_defaults", "cleanGSub", "f(42, false, {})"),
        ("var_short", "Var", "f('song')"),
        ("var_long", "Var", "f(text)"),
        ("var_empty", "Var", "f('')"),
        ("var_invalid", "Var", "f(invalid)"),
        ("version_short", "GetVersionParts", "f('5.1.0-alpha')"),
        ("version_long", "GetVersionParts", "f(text)"),
        ("version_default", "GetVersionParts", "f()"),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = prepare();
                lua.globals().set("text", "x".repeat(4096)).unwrap();
                lua.globals()
                    .set("invalid", lua.create_string(b"bad\xff").unwrap())
                    .unwrap();
                let callback = if current {
                    callback.to_owned()
                } else {
                    format!("{callback}Before")
                };
                lua.globals()
                    .set("f", lua.globals().get::<Function>(callback).unwrap())
                    .unwrap();
                let run: Function = lua
                    .load(format!(
                        "return function() for i=1,8192 do {expression} end end"
                    ))
                    .eval()
                    .unwrap();
                run.call::<()>(()).unwrap();
                lua.gc_collect().unwrap();
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    black_box(run.call::<()>(()).unwrap());
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":8192,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"} else {"before"},"unit":"callbacks","baseline_commit":"be95399d3","samples":samples})
            );
        }
    }
}
