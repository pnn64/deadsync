use super::*;
use std::hint::black_box;

#[path = "text_arguments_baseline.rs"]
mod baseline;

fn format_owned(lua: &Lua, mut args: MultiValue, old: bool) -> mlua::Result<String> {
    if old {
        baseline::lua_format_text(lua, &args)
    } else {
        lua_format_text_in_place(lua, &mut args)
    }
}

fn args_for(lua: &Lua, count: usize, receiver: bool) -> MultiValue {
    let mut args = MultiValue::new();
    if receiver {
        args.push_back(Value::Table(lua.create_table().unwrap()));
    }
    if count > 0 {
        args.push_back(Value::String(
            lua.create_string(
                std::iter::repeat_n("%d", count - 1)
                    .collect::<Vec<_>>()
                    .join(":"),
            )
            .unwrap(),
        ));
        args.extend((1..count).map(|i| Value::Integer(i as i64)));
    }
    args
}

#[test]
fn lua_buffer_pass_text_arguments_preserve_arity_receiver_skipping_and_wrapped_buffers() {
    let lua = Lua::new();
    for receiver in [false, true] {
        for count in [0, 1, 2, 4, 5, 8, 17, 64] {
            let mut args = args_for(&lua, count, receiver);
            // Exercise a VecDeque whose logical elements do not begin at slot 0.
            args.push_front(Value::Nil);
            args.pop_front();
            let expected = baseline::lua_format_text(&lua, &args).unwrap();
            assert_eq!(format_owned(&lua, args.clone(), false).unwrap(), expected);
            let before = format!("{args:?}");
            assert_eq!(lua_format_text_in_place(&lua, &mut args).unwrap(), expected);
            assert_eq!(format!("{args:?}"), before);
            assert_eq!(lua_format_text(&lua, &args).unwrap(), expected);
        }
    }
}

#[test]
fn lua_buffer_pass_text_arguments_preserve_custom_results_errors_and_lookup_order() {
    let lua = Lua::new();
    let globals = lua.globals();
    let original = globals.get::<Value>("string").unwrap();
    let args = args_for(&lua, 3, true);
    for value in [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(42),
        Value::Number(0.125),
        Value::String(lua.create_string("long\0é".repeat(64)).unwrap()),
        Value::String(lua.create_string([255, 0]).unwrap()),
    ] {
        let table = lua.create_table().unwrap();
        let function = lua
            .create_function(move |_, args: MultiValue| {
                assert_eq!(args.len(), 3);
                assert!(matches!(args.front(), Some(Value::String(_))));
                Ok(value.clone())
            })
            .unwrap();
        table.set("format", function).unwrap();
        globals.set("string", table).unwrap();
        let mut in_place = args.clone();
        let before = format!("{in_place:?}");
        assert_eq!(
            lua_format_text_in_place(&lua, &mut in_place).map_err(|e| e.to_string()),
            format_owned(&lua, args.clone(), true).map_err(|e| e.to_string())
        );
        assert_eq!(format!("{in_place:?}"), before);
    }
    globals.set("string", Value::Nil).unwrap();
    for receiver in [false, true] {
        assert_eq!(
            format_owned(&lua, args_for(&lua, 0, receiver), false).unwrap(),
            ""
        );
        assert_eq!(
            format_owned(&lua, args_for(&lua, 0, receiver), true).unwrap(),
            ""
        );
    }
    assert_eq!(
        format_owned(&lua, args.clone(), false)
            .unwrap_err()
            .to_string(),
        format_owned(&lua, args.clone(), true)
            .unwrap_err()
            .to_string()
    );
    globals.set("string", original).unwrap();
    let mut bad = MultiValue::new();
    bad.push_back(Value::String(lua.create_string("%d").unwrap()));
    bad.push_back(Value::String(lua.create_string("bad integer").unwrap()));
    assert_eq!(
        format_owned(&lua, bad.clone(), false)
            .unwrap_err()
            .to_string(),
        format_owned(&lua, bad, true).unwrap_err().to_string()
    );
}

#[test]
fn lua_buffer_pass_text_arguments_keep_receiver_alive_during_formatter_gc() {
    for old in [true, false] {
        let lua = Lua::new();
        let weak: Table = lua
            .load("return setmetatable({}, {__mode = 'v'})")
            .eval()
            .unwrap();
        let receiver = lua.create_table().unwrap();
        weak.set(1, receiver.clone()).unwrap();
        let globals = lua.globals();
        globals.set("receiver_weak", weak.clone()).unwrap();
        let string = lua.create_table().unwrap();
        let format: Function = lua
            .load(
                r#"return function(...)
            assert(select('#', ...) == 1)
            collectgarbage('collect')
            assert(receiver_weak[1] ~= nil, 'receiver collected too early')
            return 'alive'
        end"#,
            )
            .eval()
            .unwrap();
        string.set("format", format).unwrap();
        globals.set("string", string).unwrap();
        let args = MultiValue::from_vec(vec![Value::Table(receiver), Value::Integer(1)]);
        assert_eq!(format_owned(&lua, args, old).unwrap(), "alive");
        lua.gc_collect().unwrap();
        assert!(matches!(weak.raw_get::<Value>(1).unwrap(), Value::Nil));
    }
}

#[test]
fn lua_buffer_pass_text_arguments_preserve_settextf_updates_identity_and_error_state() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        install_actor_visual_text_methods(&lua, &actor).unwrap();
        if old {
            baseline::install_settextf(&lua, &actor).unwrap();
        }
        let method = actor.get::<Function>("settextf").unwrap();
        let result = method.call::<Table>((&actor, "score %04d", 12)).unwrap();
        assert_eq!(result.to_pointer(), actor.to_pointer());
        assert_eq!(actor.get::<String>("Text").unwrap(), "score 0012");
        assert!(method.call::<Value>((&actor, "%d", "invalid")).is_err());
        assert_eq!(actor.get::<String>("Text").unwrap(), "score 0012");
        method.call::<Value>(&actor).unwrap();
        assert_eq!(actor.get::<String>("Text").unwrap(), "");
    }
}

#[test]
fn lua_buffer_pass_text_arguments_keep_values_alive_through_text_setter() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        install_actor_visual_text_methods(&lua, &actor).unwrap();
        if old {
            baseline::install_settextf(&lua, &actor).unwrap();
        }
        let globals = lua.globals();
        let weak: Table = lua
            .load("return setmetatable({}, {__mode = 'v'})")
            .eval()
            .unwrap();
        globals.set("argument_weak", weak.clone()).unwrap();
        let mt: Table = lua
            .load(
                r#"return {__newindex = function(self, key, value)
            if key == 'Text' then
                collectgarbage('collect')
                assert(argument_weak[1] ~= nil, 'receiver released before Text setter')
                assert(argument_weak[2] ~= nil, 'value released before Text setter')
            end
            rawset(self, key, value)
        end}"#,
            )
            .eval()
            .unwrap();
        actor.set_metatable(Some(mt)).unwrap();
        let string: Table = lua
            .load("return {format = function(...) return 'formatted' end}")
            .eval()
            .unwrap();
        globals.set("string", string).unwrap();
        let method = actor.get::<Function>("settextf").unwrap();
        let receiver = lua.create_table().unwrap();
        let value = lua.create_table().unwrap();
        weak.set(1, receiver.clone()).unwrap();
        weak.set(2, value.clone()).unwrap();
        let args = MultiValue::from_vec(vec![
            Value::Table(receiver),
            Value::String(lua.create_string("ignored").unwrap()),
            Value::Table(value),
        ]);
        method.call::<Value>(args).unwrap();
        assert_eq!(actor.raw_get::<String>("Text").unwrap(), "formatted");
    }
}

#[test]
fn lua_buffer_pass_text_arguments_custom_formatter_needs_no_second_buffer() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table
        .set(
            "format",
            lua.load("return function(...) return nil end")
                .eval::<Function>()
                .unwrap(),
        )
        .unwrap();
    lua.globals().set("string", table).unwrap();
    for count in [2, 8, 64] {
        let mut args = args_for(&lua, count, true);
        drop(format_owned(&lua, args.clone(), false).unwrap());
        lua.gc_stop();
        crate::perf::assert_no_churn(|| {
            drop(black_box(
                lua_format_text_in_place(&lua, &mut args).unwrap(),
            ))
        });
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_buffer_pass_bench_text_arguments() {
    for receiver in [false, true] {
        for count in [0, 1, 2, 4, 8, 64] {
            let lua = Lua::new();
            let template = args_for(&lua, count, receiver);
            assert_eq!(
                format_owned(&lua, template.clone(), true).unwrap(),
                format_owned(&lua, template.clone(), false).unwrap()
            );
            lua.gc_collect().unwrap();
            lua.gc_stop();
            let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
                [false, true]
            } else {
                [true, false]
            };
            for old in order {
                crate::perf::measure_sampled(
                    &format!(
                        "text_arguments_{}_{count}/{}",
                        if receiver { "method" } else { "plain" },
                        if old { "old" } else { "new" }
                    ),
                    128,
                    1,
                    || {
                        // Both sides include the same incoming argument allocation.
                        drop(black_box(
                            format_owned(
                                black_box(&lua),
                                black_box(&template).clone(),
                                black_box(old),
                            )
                            .unwrap(),
                        ));
                    },
                );
            }
        }
    }
    for old in if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    } {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        install_actor_visual_text_methods(&lua, &actor).unwrap();
        if old {
            baseline::install_settextf(&lua, &actor).unwrap();
        }
        let method = actor.get::<Function>("settextf").unwrap();
        method.call::<Value>((&actor, "score %04d", 123)).unwrap();
        lua.gc_collect().unwrap();
        lua.gc_stop();
        crate::perf::measure_sampled(
            &format!(
                "text_arguments_settextf/{}",
                if old { "old" } else { "new" }
            ),
            128,
            1,
            || {
                drop(black_box(
                    method
                        .call::<Value>((black_box(&actor), "score %04d", black_box(123)))
                        .unwrap(),
                ));
            },
        );
    }
}
