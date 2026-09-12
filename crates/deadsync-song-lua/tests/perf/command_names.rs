use super::*;
use std::hint::black_box;

#[path = "command_names_baseline.rs"]
mod baseline;

fn install(lua: &Lua, actor: &Table, old: bool) {
    if old {
        baseline::install_actor_command_methods(lua, actor)
    } else {
        install_actor_command_methods(lua, actor)
    }
    .unwrap();
}

fn drain(lua: &Lua, actor: &Table, old: bool) -> mlua::Result<()> {
    if old {
        baseline::drain_actor_command_queue(lua, actor)
    } else {
        drain_actor_command_queue(lua, actor)
    }
}

fn broadcast(lua: &Lua, name: &str, params: Option<Value>, old: bool) -> mlua::Result<()> {
    if old {
        baseline::broadcast_song_lua_message(lua, name, params)
    } else {
        broadcast_song_lua_message(lua, name, params)
    }
}

#[test]
fn lua_command_names_preserve_bytes_boundaries_and_allocation_limits() {
    for suffix in ["Command", "MessageCommand"] {
        for name in [
            String::new(),
            "Update".into(),
            "é\0曲".into(),
            "x".repeat(128 - suffix.len()),
            "x".repeat(129 - suffix.len()),
            "曲".repeat(4096),
        ] {
            let expected = format!("{name}{suffix}");
            let actual = ActorCommandName::new(&name, suffix);
            assert_eq!(actual.as_str(), expected);
            assert_eq!(
                matches!(actual, ActorCommandName::Inline { .. }),
                expected.len() <= 128
            );
            if expected.len() <= 128 {
                crate::perf::assert_no_churn(|| {
                    drop(black_box(ActorCommandName::new(&name, suffix)))
                });
            } else {
                crate::perf::assert_churn_budget(1, expected.len(), || {
                    drop(black_box(ActorCommandName::new(&name, suffix)))
                });
            }
            let lua = Lua::new();
            let table = lua.create_table().unwrap();
            table.set(actual, 42).unwrap();
            assert_eq!(table.raw_get::<i32>(expected.as_str()).unwrap(), 42);
        }
    }
}

#[test]
fn lua_command_names_preserve_installed_methods_hierarchy_and_recurrence() {
    for name in ["Tick".to_owned(), "é\0曲".to_owned(), "long".repeat(80)] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let (root, child, leaf, events): (Table, Table, Table, Table) = lua
                .load(
                    r#"
                local events={}
                local leaf={Name='leaf'}
                local child={leaf, Name='child',__songlua_actor_type='ActorFrame'}
                local root={child,Name='root',__songlua_actor_type='ActorFrame'}
                return root,child,leaf,events
            "#,
                )
                .eval()
                .unwrap();
            lua.globals().set("events", &events).unwrap();
            for actor in [&root, &child, &leaf] {
                install(&lua, actor, old);
            }
            let callback: Function = lua
                .load("return function(self,p) events[#events+1]=self.Name..':'..tostring(p) end")
                .eval()
                .unwrap();
            for actor in [&root, &child, &leaf] {
                actor
                    .get::<Function>("addcommand")
                    .unwrap()
                    .call::<()>((actor, name.as_str(), &callback))
                    .unwrap();
                let found = actor
                    .get::<Function>("GetCommand")
                    .unwrap()
                    .call::<Function>((actor, name.as_str()))
                    .unwrap();
                assert_eq!(found.to_pointer(), callback.to_pointer());
            }
            for method in [
                "playcommand",
                "propagatecommand",
                "playcommandonchildren",
                "playcommandonleaves",
            ] {
                root.get::<Function>(method)
                    .unwrap()
                    .call::<()>((&root, name.as_str(), method))
                    .unwrap();
            }
            root.get::<Function>("propagate")
                .unwrap()
                .call::<()>((&root, true))
                .unwrap();
            root.get::<Function>("playcommand")
                .unwrap()
                .call::<()>((&root, name.as_str(), "propagated"))
                .unwrap();
            root.get::<Function>("queuecommand")
                .unwrap()
                .call::<()>((&root, name.as_str()))
                .unwrap();
            let recurring: Function = lua
                .load("return function(self) self:sleep(0.25); self:queuecommand('Recurring') end")
                .eval()
                .unwrap();
            root.get::<Function>("addcommand")
                .unwrap()
                .call::<()>((&root, "Recurring", recurring))
                .unwrap();
            root.get::<Function>("propagate")
                .unwrap()
                .call::<()>((&root, false))
                .unwrap();
            root.get::<Function>("playcommand")
                .unwrap()
                .call::<()>((&root, "Recurring"))
                .unwrap();
            assert_eq!(
                root.get::<String>("__songlua_recurring_update_command")
                    .unwrap(),
                "RecurringCommand"
            );
            assert_eq!(
                root.get::<f64>("__songlua_recurring_update_interval")
                    .unwrap(),
                0.25
            );
            root.get::<Function>("removecommand")
                .unwrap()
                .call::<()>((&root, name.as_str()))
                .unwrap();
            assert!(
                root.get::<Function>("GetCommand")
                    .unwrap()
                    .call::<Value>((&root, name.as_str()))
                    .unwrap()
                    .is_nil()
            );
            outcomes.push(
                events
                    .sequence_values::<String>()
                    .collect::<mlua::Result<Vec<_>>>()
                    .unwrap(),
            );
        }
        assert_eq!(outcomes[0], outcomes[1]);
        assert!(outcomes[1].iter().any(|s| s == "leaf:playcommandonleaves"));
        assert!(outcomes[1].iter().any(|s| s == "root:playcommand"));
    }
}

#[test]
fn lua_command_names_preserve_conversion_lookup_and_callback_errors() {
    for method in [
        "GetCommand",
        "addcommand",
        "removecommand",
        "playcommand",
        "queuecommand",
        "propagatecommand",
        "playcommandonchildren",
        "playcommandonleaves",
    ] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let actor = lua.create_table().unwrap();
            install(&lua, &actor, old);
            let callback: Function = lua
                .load("return function() error('command sentinel',0) end")
                .eval()
                .unwrap();
            actor.raw_set("FailCommand", &callback).unwrap();
            let function = actor.get::<Function>(method).unwrap();
            let mut results = Vec::new();
            for name in [
                Value::String(lua.create_string("Fail").unwrap()),
                Value::String(lua.create_string([255, 0]).unwrap()),
                Value::Integer(123),
                Value::Nil,
                Value::Boolean(false),
            ] {
                results.push(
                    function
                        .call::<Value>((&actor, name, &callback))
                        .map(|v| v.type_name().to_owned())
                        .map_err(|e| e.to_string()),
                );
            }
            let mt = lua.create_table().unwrap();
            mt.raw_set(
                "__index",
                lua.create_function(|_, (_table, key): (Table, String)| -> mlua::Result<Value> {
                    if key == "MissingCommand" {
                        Err(mlua::Error::runtime("lookup sentinel"))
                    } else {
                        Ok(Value::Nil)
                    }
                })
                .unwrap(),
            )
            .unwrap();
            actor.set_metatable(Some(mt)).unwrap();
            results.push(
                function
                    .call::<Value>((&actor, "Missing", &callback))
                    .map(|v| v.type_name().to_owned())
                    .map_err(|e| e.to_string()),
            );
            outcomes.push(results);
        }
        assert_eq!(outcomes[0], outcomes[1], "{method}");
    }
}

#[test]
fn lua_command_names_preserve_broadcast_scope_long_keys_params_and_errors() {
    for name in ["Judgment".to_owned(), "é\0曲".to_owned(), "Long".repeat(80)] {
        for fail in [false, true] {
            let mut outcomes = Vec::new();
            for old in [true, false] {
                let lua = Lua::new();
                let actor = lua.create_table().unwrap();
                song_lua_actor_registry(&lua)
                    .unwrap()
                    .raw_set(1, &actor)
                    .unwrap();
                begin_overlay_update_capture(&lua, HashMap::new());
                lua.globals()
                    .raw_set(ACTIVE_BROADCAST_KEY, "before")
                    .unwrap();
                let expected = name.clone();
                actor
                    .set(
                        format!("{name}MessageCommand"),
                        lua.create_function(move |lua, (_actor, params): (Table, Table)| {
                            assert_eq!(
                                lua.globals().raw_get::<String>(ACTIVE_BROADCAST_KEY)?,
                                expected
                            );
                            let capture =
                                lua.app_data_ref::<SongLuaOverlayUpdateCapture>().unwrap();
                            assert_eq!(
                                capture.active_broadcast.as_deref(),
                                Some(expected.as_str())
                            );
                            assert_eq!(
                                capture
                                    .active_broadcast_command
                                    .as_ref()
                                    .unwrap()
                                    .to_str()?
                                    .as_ref(),
                                format!("{expected}MessageCommand")
                            );
                            params.raw_set("seen", true)?;
                            if fail {
                                Err(mlua::Error::runtime("broadcast sentinel"))
                            } else {
                                Ok(())
                            }
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let params = lua.create_table().unwrap();
                let result = broadcast(&lua, &name, Some(Value::Table(params.clone())), old)
                    .map_err(|e| e.to_string());
                assert!(params.raw_get::<bool>("seen").unwrap());
                assert_eq!(
                    lua.globals().get::<String>(ACTIVE_BROADCAST_KEY).unwrap(),
                    "before"
                );
                let capture = lua.app_data_ref::<SongLuaOverlayUpdateCapture>().unwrap();
                assert!(capture.active_broadcast.is_none());
                assert!(capture.active_broadcast_command.is_none());
                outcomes.push((
                    result,
                    params.raw_get::<Option<String>>("Player").unwrap(),
                    capture.runtime_broadcasts.clone(),
                ));
            }
            assert_eq!(outcomes[0], outcomes[1]);
        }
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_command_bench_names() {
    for (len, suffix) in [
        (0, "Command"),
        (8, "Command"),
        (121, "Command"),
        (122, "Command"),
        (4096, "Command"),
        (8, "MessageCommand"),
        (114, "MessageCommand"),
        (115, "MessageCommand"),
    ] {
        let name = "x".repeat(len);
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("name_{len}_{suffix}/{}", if old { "old" } else { "new" }),
                512,
                1,
                || {
                    if black_box(old) {
                        drop(black_box(if suffix == "Command" {
                            baseline::command_name(black_box(&name))
                        } else {
                            baseline::message_name(black_box(&name))
                        }));
                    } else {
                        drop(black_box(ActorCommandName::new(black_box(&name), suffix)));
                    }
                },
            );
        }
    }
    for (method, len) in [
        ("GetCommand", 8),
        ("GetCommand", 200),
        ("playcommand", 8),
        ("queuecommand", 8),
    ] {
        let lua = Lua::new();
        let name = lua.create_string("x".repeat(len)).unwrap();
        let callback: Function = lua.load("return function() end").eval().unwrap();
        let actors = [lua.create_table().unwrap(), lua.create_table().unwrap()];
        for (actor, old) in actors.iter().zip([true, false]) {
            install(&lua, actor, old);
            actor
                .raw_set(format!("{}Command", name.to_str().unwrap()), &callback)
                .unwrap();
        }
        let methods = actors
            .each_ref()
            .map(|a| a.get::<Function>(method).unwrap());
        for i in 0..2 {
            methods[i].call::<Value>((&actors[i], &name)).unwrap();
        }
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [1, 0]
        } else {
            [0, 1]
        };
        for i in order {
            crate::perf::measure_sampled(
                &format!(
                    "method_{method}_{len}/{}",
                    if i == 0 { "old" } else { "new" }
                ),
                256,
                1,
                || {
                    drop(black_box(
                        methods[i].call::<Value>((&actors[i], &name)).unwrap(),
                    ));
                },
            );
        }
    }
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let queue = actor_command_queue(&lua, &actor).unwrap();
    let names: Vec<_> = (0..64)
        .map(|i| lua.create_string(format!("cmd{i}")).unwrap())
        .collect();
    let refill = || {
        for (i, name) in names.iter().enumerate() {
            queue.raw_set(i + 1, name).unwrap();
        }
    };
    for old in [true, false] {
        refill();
        drain(&lua, &actor, old).unwrap();
        assert_eq!(queue.raw_len(), 0);
    }
    lua.gc_collect().unwrap();
    lua.gc_stop();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for old in order {
        crate::perf::measure_sampled(
            &format!("named_queue_64/{}", if old { "old" } else { "new" }),
            64,
            64,
            || {
                refill();
                drain(&lua, black_box(&actor), black_box(old)).unwrap();
            },
        );
    }
}
