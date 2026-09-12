use super::*;
use std::hint::black_box;

#[path = "message_discovery_baseline.rs"]
mod baseline;

fn names(actor: &Table, old: bool) -> Result<Vec<String>, String> {
    if old {
        return baseline::command_names(actor);
    }
    let mut names = Vec::new();
    for_each_actor_message_command(actor, |name, _| names.push(name.to_owned()))
        .map_err(|err| err.to_string())?;
    names.sort_unstable();
    Ok(names)
}

fn fixture(lua: &Lua, fields: usize, commands: usize, kind: &str) -> Table {
    let actor = lua.create_table().unwrap();
    let noop: Function = lua.load("return function() end").eval().unwrap();
    for i in 0..fields {
        let value = if kind == "methods" || kind == "mixed" && i % 2 == 0 {
            Value::Function(noop.clone())
        } else {
            Value::Integer(i as i64)
        };
        actor
            .raw_set(format!("unrelated_field_{i}"), value)
            .unwrap();
    }
    for i in 0..commands {
        actor
            .raw_set(format!("Event{i}MessageCommand"), &noop)
            .unwrap();
    }
    actor
}

fn capture(lua: &Lua, actor: &Table, old: bool) -> Result<SongLuaCapturedMessageCommands, String> {
    if old {
        baseline::capture_actor_message_commands(lua, actor)
    } else {
        capture_actor_message_commands(lua, actor)
    }
}

#[test]
fn lua_state_message_discovery_preserves_filter_order_encoding_and_function_identity() {
    let lua = Lua::new();
    let actor = fixture(&lua, 256, 32, "mixed");
    let command: Function = lua.load("return function() end").eval().unwrap();
    for name in [
        "MessageCommand",
        "é\0MessageCommand",
        "RepeatMessageCommandMessageCommand",
        "NoMessageCommandSuffix",
        "lowercasemessagecommand",
    ] {
        actor.raw_set(name, &command).unwrap();
    }
    actor.raw_set("FakeMessageCommand", 42).unwrap();
    actor
        .raw_set(lua.create_string(b"\xffMessageCommand").unwrap(), &command)
        .unwrap();
    actor.raw_set(7, &command).unwrap();
    actor.raw_set(true, &command).unwrap();
    actor.raw_set(&actor, &command).unwrap();
    let mt: Table = lua
        .load(
            r#"return {
        __pairs=function() error('pairs') end,
        __index={InheritedMessageCommand=function() end},
    }"#,
        )
        .eval()
        .unwrap();
    actor.set_metatable(Some(mt)).unwrap();
    assert_eq!(names(&actor, true).unwrap(), names(&actor, false).unwrap());
    assert_eq!(names(&actor, false).unwrap().len(), 35);
    let mut old_order = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair.unwrap();
        let (Some(name), Value::Function(function)) = (read_string(key), value) else {
            continue;
        };
        if name.ends_with("MessageCommand") {
            old_order.push((name, function.to_pointer() as usize));
        }
    }
    let mut new_order = Vec::new();
    for_each_actor_message_command(&actor, |name, function| {
        new_order.push((name.to_owned(), function.to_pointer() as usize))
    })
    .unwrap();
    assert_eq!(old_order, new_order);
}

#[test]
fn lua_state_message_capture_preserves_sorted_snapshot_mutation_and_errors() {
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let (actor, events): (Table,Table) = lua.load(r#"
            local events = {}
            local actor = { Name='discovery' }
            actor.ZMessageCommand = function(self) events[#events+1]='Z'; self:x(30) end
            actor.AMessageCommand = function(self)
                events[#events+1]='A'; self:x(10)
                self.AddedMessageCommand = function() error('must not be discovered') end
            end
            actor.FailMessageCommand = function() error('discovery sentinel', 0) end
            actor.RepeatMessageCommandMessageCommand = function(self) events[#events+1]='Repeat'; self:x(20) end
            actor.NotAFunctionMessageCommand = false
            return actor, events
        "#).set_name("discovery-fixture").eval().unwrap();
        install_actor_transform_methods(&lua, &actor).unwrap();
        let result = capture(&lua, &actor, old).unwrap();
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].contains("discovery sentinel"));
        assert_eq!(
            result
                .commands
                .iter()
                .map(|c| c.message.as_str())
                .collect::<Vec<_>>(),
            ["A", "Repeat", "Z"]
        );
        outcomes.push((
            result,
            events
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap(),
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(outcomes[1].1, ["A", "Repeat", "Z"]);
}

#[test]
fn lua_state_message_cross_actor_capture_preserves_stable_effects() {
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let runtime =
            crate::create_song_runtime_table(&lua, &SongLuaCompileContext::new("", "")).unwrap();
        lua.globals()
            .raw_set(crate::SONG_LUA_RUNTIME_KEY, runtime)
            .unwrap();
        let source = fixture(&lua, 64, 0, "mixed");
        let target = lua.create_table().unwrap();
        install_actor_transform_methods(&lua, &target).unwrap();
        lua.globals().raw_set("target", &target).unwrap();
        source
            .raw_set(
                "MoveMessageCommand",
                lua.load("return function() target:x(42) end")
                    .eval::<Function>()
                    .unwrap(),
            )
            .unwrap();
        source
            .raw_set(
                "BadMessageCommand",
                lua.load("return function() error('ignored probe', 0) end")
                    .eval::<Function>()
                    .unwrap(),
            )
            .unwrap();
        let mut overlays: Vec<_> = [source, target]
            .into_iter()
            .map(|table| SongLuaOverlayCompileActor {
                table,
                actor: crate::SongLuaOverlayActor {
                    kind: (),
                    name: None,
                    parent_index: None,
                    initial_state: SongLuaOverlayState::default(),
                    message_commands: vec![],
                },
                message_sounds: vec![],
            })
            .collect();
        let mut dynamic = Vec::new();
        if old {
            baseline::capture_stable_cross_actor_message_commands(&lua, &mut overlays, |s| {
                dynamic.push(s)
            })
            .unwrap();
        } else {
            capture_stable_cross_actor_message_commands(&lua, &mut overlays, |s| dynamic.push(s))
                .unwrap();
        }
        assert_eq!(overlays[1].actor.message_commands.len(), 1);
        assert_eq!(overlays[1].actor.message_commands[0].message, "Move");
        outcomes.push((
            overlays
                .into_iter()
                .map(|a| a.actor.message_commands)
                .collect::<Vec<_>>(),
            dynamic,
        ));
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn lua_state_message_discovery_irrelevant_data_fields_have_no_churn() {
    let lua = Lua::new();
    let actor = fixture(&lua, 256, 0, "data");
    names(&actor, false).unwrap();
    crate::perf::assert_no_churn(|| assert!(names(&actor, false).unwrap().is_empty()));
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_state_bench_message_discovery() {
    for (fields, commands, kind) in [
        (0, 0, "data"),
        (64, 0, "data"),
        (256, 0, "data"),
        (64, 4, "data"),
        (256, 8, "data"),
        (256, 8, "methods"),
        (512, 32, "mixed"),
        (0, 64, "data"),
    ] {
        let lua = Lua::new();
        let actor = fixture(&lua, fields, commands, kind);
        assert_eq!(names(&actor, true).unwrap(), names(&actor, false).unwrap());
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
                    "discovery_{fields}_{commands}_{kind}/{}",
                    if old { "old" } else { "new" }
                ),
                if fields + commands == 0 { 512 } else { 64 },
                (fields + commands).max(1),
                || {
                    drop(black_box(names(black_box(&actor), black_box(old)).unwrap()));
                },
            );
        }
    }
    // Measure the caller too: discovery savings can be diluted by command capture.
    for (fields, commands, kind) in [(64, 4, "data"), (256, 8, "methods")] {
        let lua = Lua::new();
        let actor = fixture(&lua, fields, commands, kind);
        assert_eq!(
            capture(&lua, &actor, true).unwrap(),
            capture(&lua, &actor, false).unwrap()
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
                    "message_capture_{fields}_{commands}_{kind}/{}",
                    if old { "old" } else { "new" }
                ),
                16,
                commands,
                || {
                    drop(black_box(
                        capture(&lua, black_box(&actor), black_box(old)).unwrap(),
                    ));
                },
            );
        }
    }
}
