use super::*;
use std::hint::black_box;

#[path = "broadcast_storage_baseline.rs"]
mod baseline;

fn getter(lua: &Lua, table: &Table, value: &str, old: bool) -> Function {
    if old {
        baseline::set_string_method(lua, table, "GetText", value).unwrap();
    } else {
        set_string_method(lua, table, "GetText", value).unwrap();
    }
    table.get("GetText").unwrap()
}

fn broadcast(lua: &Lua, message: &str, params: Option<Value>, old: bool) -> mlua::Result<()> {
    if old {
        baseline::broadcast_song_lua_message(lua, message, params)
    } else {
        broadcast_song_lua_message(lua, message, params)
    }
}

fn judgment(lua: &Lua, params: &Table, old: bool) -> mlua::Result<()> {
    if old {
        baseline::normalize_judgment_params(lua, params)
    } else {
        normalize_judgment_params(lua, params)
    }
}

fn install_broadcast(lua: &Lua, old: bool) {
    lua.globals()
        .set(
            "broadcast",
            lua.create_function(move |lua, (message, params): (String, Option<Value>)| {
                broadcast(lua, &message, params, old)
            })
            .unwrap(),
        )
        .unwrap();
}

#[test]
fn broadcast_storage_getters_preserve_bytes_capture_and_all_argument_shapes() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    let values = [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(i64::MAX),
        Value::Number(f64::NAN),
        Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
        Value::Table(table.clone()),
        Value::Function(function.clone()),
        Value::Thread(lua.create_thread(function).unwrap()),
    ];
    for text in [
        String::new(),
        "TapNoteType_Tap".into(),
        "\u{e9}\0\u{65e5}".into(),
        "long text ".repeat(512),
    ] {
        for old in [true, false] {
            let mut source = text.clone();
            let method = getter(&lua, &table, &source, old);
            source.clear();
            table.set("text", "changed after construction").unwrap();
            let retained = method.call::<mlua::LuaString>(&table).unwrap();
            for count in [0, 1, 2, 8, 64, 257] {
                let args = (0..count)
                    .map(|i| values[i % values.len()].clone())
                    .collect::<MultiValue>();
                let actual = method.call::<mlua::LuaString>(&args).unwrap();
                assert_eq!(actual.as_bytes().as_ref(), text.as_bytes());
                assert_eq!(retained.as_bytes().as_ref(), text.as_bytes());
            }
            // Replacing the table method cannot alter already captured getters.
            getter(&lua, &table, "replacement", old);
            assert_eq!(
                method
                    .call::<mlua::LuaString>(())
                    .unwrap()
                    .as_bytes()
                    .as_ref(),
                text.as_bytes()
            );
        }
    }
}

#[test]
fn broadcast_storage_getters_preserve_newindex_callbacks_and_partial_errors() {
    let lua = Lua::new();
    for fail in [false, true] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            lua.globals().set("fail", fail).unwrap();
            let table = lua
                .load(
                    r#"seen='';return setmetatable({}, {__newindex=function(t,k,v)
                seen=k..':'..v(t,nil,false,{});rawset(t,k,v)
                if fail then error('setter failed after write') end
            end})"#,
                )
                .set_name("getter-write")
                .eval::<Table>()
                .unwrap();
            let result = if old {
                baseline::set_string_method(&lua, &table, "GetText", "captured\0text")
            } else {
                set_string_method(&lua, &table, "GetText", "captured\0text")
            };
            outcomes.push((
                result.err().map(|e| e.to_string()),
                lua.globals().get::<String>("seen").unwrap(),
                table
                    .get::<Function>("GetText")
                    .unwrap()
                    .call::<String>(())
                    .unwrap(),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(outcomes[0].1, "GetText:captured\0text");
    }
}

#[test]
fn broadcast_storage_warm_getters_have_zero_churn_for_short_and_long_strings() {
    let lua = Lua::new();
    lua.gc_stop();
    let table = lua.create_table().unwrap();
    for text in [
        "".into(),
        "TapNoteType_Tap".into(),
        "\u{e9}\0\u{65e5}".into(),
        "x".repeat(4096),
    ] {
        let old = getter(&lua, &table, &text, true);
        let new = getter(&lua, &table, &text, false);
        for _ in 0..8 {
            black_box(old.call::<mlua::LuaString>(&table).unwrap());
            black_box(new.call::<mlua::LuaString>(&table).unwrap());
        }
        crate::perf::assert_no_churn(|| {
            for _ in 0..64 {
                black_box(
                    new.call::<mlua::LuaString>((&table, Value::Nil, false))
                        .unwrap(),
                );
            }
        });
        crate::perf::assert_reduced_churn(
            || {
                for _ in 0..64 {
                    black_box(old.call::<mlua::LuaString>(&table).unwrap());
                }
            },
            || {
                for _ in 0..64 {
                    black_box(new.call::<mlua::LuaString>(&table).unwrap());
                }
            },
        );
    }
}

#[test]
fn broadcast_storage_actor_snapshot_preserves_mutation_nested_calls_and_error_restoration() {
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        install_broadcast(&lua, old);
        lua.load(r#"log={};params={marker=17};__songlua_active_broadcast='outside'
            function logit(s) log[#log+1]=s..':'..__songlua_active_broadcast end
            a={TickMessageCommand=function(self,p)
                assert(p==params);logit('a');registry[2]=replacement;registry[3]=nil
                broadcast('Inner',p);logit('a-after')
            end,InnerMessageCommand=function(self,p) assert(p==params);logit('inner-a') end}
            b={TickMessageCommand=function(self,p) assert(p==params);logit('b');if fail then error('actor failed') end end}
            c={TickMessageCommand=function() logit('c') end}
            replacement={TickMessageCommand=function() logit('replacement') end,
                InnerMessageCommand=function() logit('inner-replacement') end}
            registry={a,b,c};__songlua_actor_registry=registry"#).set_name("broadcast-mutation").exec().unwrap();
        for fail in [false, true] {
            lua.globals().set("fail", fail).unwrap();
            lua.load("log={};registry[1]=a;registry[2]=b;registry[3]=c")
                .exec()
                .unwrap();
            let params = lua.globals().get::<Table>("params").unwrap();
            let error = broadcast(&lua, "Tick", Some(Value::Table(params)), old)
                .err()
                .map(|e| e.to_string());
            let state = lua.load("return table.concat(log,'|'),__songlua_active_broadcast,next(a.__songlua_active_commands)==nil,next(b.__songlua_active_commands)==nil").eval::<(String,String,bool,bool)>().unwrap();
            assert!(state.0.contains("inner-replacement:Inner"));
            assert!(state.0.contains("a-after:Tick|b:Tick"));
            assert_eq!(state.0.ends_with("c:Tick"), !fail);
            assert_eq!(
                (&state.1, state.2, state.3),
                (&"outside".to_string(), true, true)
            );
            outcomes.push((error, state));
        }
    }
    assert_eq!(outcomes[..2], outcomes[2..]);
}

#[test]
fn broadcast_storage_actor_order_filters_gaps_and_inline_spill_boundaries_match_parent() {
    let lua = Lua::new();
    for count in [0, 1, 8, 32, 33, 64, 257] {
        for gaps in [false, true] {
            let mut outcomes = Vec::new();
            for old in [true, false] {
                install_broadcast(&lua, old);
                lua.globals().set("count", count).unwrap();
                lua.globals().set("gaps", gaps).unwrap();
                lua.load(r#"seen={};local r={};for i=1,count do
                    r[i]=i%5==0 and false or {TickMessageCommand=function(self,p) seen[#seen+1]=i;assert(p==23) end}
                    if i%5==0 then r[i]=false end
                end
                if gaps and count>1 then r[2]=nil end
                __songlua_actor_registry=r;__songlua_active_broadcast=nil"#).exec().unwrap();
                broadcast(&lua, " \t", None, old).unwrap();
                assert!(
                    lua.globals()
                        .get::<Value>(ACTIVE_BROADCAST_KEY)
                        .unwrap()
                        .is_nil()
                );
                broadcast(&lua, "Tick", Some(Value::Integer(23)), old).unwrap();
                outcomes.push(
                    lua.load("return table.concat(seen,','),__songlua_active_broadcast==nil")
                        .eval::<(String, bool)>()
                        .unwrap(),
                );
            }
            assert_eq!(outcomes[0], outcomes[1]);
            assert!(outcomes[0].1);
            let expected = (1..=count)
                .take(if gaps && count > 1 { 1 } else { count })
                .filter(|i| i % 5 != 0)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            assert_eq!(outcomes[0].0, expected);
        }
    }
}

fn runtime_fixture() -> Lua {
    let lua = Lua::new();
    lua.gc_stop();
    lua.globals()
        .set(crate::SONG_LUA_RUNTIME_KEY, lua.create_table().unwrap())
        .unwrap();
    set_compile_song_runtime_values(&lua, 0.0, 0.0).unwrap();
    lua
}

fn actor_fixture(count: usize, listeners: bool) -> Lua {
    let lua = runtime_fixture();
    let registry = lua.create_table_with_capacity(count, 0).unwrap();
    let callback = lua
        .load("return function(self,p) end")
        .eval::<Function>()
        .unwrap();
    for index in 1..=count {
        let actor = lua.create_table().unwrap();
        if listeners {
            actor.set("TickMessageCommand", callback.clone()).unwrap();
        }
        registry.raw_set(index, actor).unwrap();
    }
    lua.globals()
        .set("__songlua_actor_registry", registry)
        .unwrap();
    for _ in 0..8 {
        broadcast(&lua, "Tick", Some(Value::Integer(23)), false).unwrap();
    }
    lua
}

#[test]
fn broadcast_storage_actor_snapshots_and_scope_keys_reduce_warm_churn() {
    for listeners in [false, true] {
        for count in [0, 1, 8, 32] {
            let lua = actor_fixture(count, listeners);
            // The snapshot carrier is allocation-free. Listener execution still
            // needs one long Lua scope key per callback, down from three.
            if !listeners || count == 0 {
                crate::perf::assert_no_churn(|| {
                    for _ in 0..16 {
                        broadcast(&lua, "Tick", Some(Value::Integer(23)), false).unwrap();
                    }
                });
            }
            if count > 0 {
                crate::perf::assert_reduced_churn(
                    || {
                        for _ in 0..16 {
                            broadcast(&lua, "Tick", Some(Value::Integer(23)), true).unwrap();
                        }
                    },
                    || {
                        for _ in 0..16 {
                            broadcast(&lua, "Tick", Some(Value::Integer(23)), false).unwrap();
                        }
                    },
                );
            }
        }
    }
}

fn judgment_fixture(count: usize, normalized: bool) -> (Lua, Table) {
    let lua = runtime_fixture();
    let params = lua.create_table().unwrap();
    let notes = lua.create_table_with_capacity(count, 0).unwrap();
    for index in 1..=count {
        let note = lua.create_table().unwrap();
        if normalized {
            note.set("GetTapNoteType", true).unwrap();
        } else {
            note.set("TapNoteType", "TapNoteType_Tap").unwrap();
        }
        notes.raw_set(index, note).unwrap();
    }
    params.set("Notes", notes).unwrap();
    // Populate default keys and grow mlua's reference pool outside measurements.
    if normalized {
        for _ in 0..8 {
            judgment(&lua, &params, false).unwrap();
        }
    }
    (lua, params)
}

fn note_summary(lua: &Lua, params: &Table) -> String {
    lua.globals().set("params", params.clone()).unwrap();
    lua.load(r#"local out={tostring(params.Player),tostring(params.TapNoteScore),tostring(params.HoldNoteScore),tostring(params.TapNoteOffset)}
        for k,n in pairs(params.Notes) do
            local r=n:GetTapNoteResult();assert(r==n:GetHoldNoteResult());assert(r==n.TapNoteResult and r==n.HoldNoteResult)
            out[#out+1]=tostring(k)..':'..n:GetTapNoteType()..':'..n:GetTapNoteSource()..':'..n:GetTapNoteSubType()..':'..n:GetPlayerNumber()..':'..n:GetAttackModifiers()..':'..tostring(n:GetHoldDuration())..':'..tostring(n:GetAttackDuration())..':'..tostring(n:GetKeysoundIndex())..':'..tostring(r:GetHeld())..':'..tostring(r:GetHidden())..':'..tostring(r:GetTapNoteOffset())..':'..r:GetTapNoteScore()
        end
        table.sort(out);return table.concat(out,'|')"#).eval().unwrap()
}

#[test]
fn broadcast_storage_judgment_shapes_defaults_fields_and_aliases_match_parent() {
    for count in [0, 1, 4, 16, 17, 64] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let (lua, params) = judgment_fixture(count, false);
            let notes = params.get::<Table>("Notes").unwrap();
            let pointer = notes.to_pointer();
            if count > 0 {
                lua.globals().set("notes", notes.clone()).unwrap();
                lua.load(r#"notes[1]={TapNoteType=false,Type='TapNoteType_HoldHead',Source='source',SubType='Roll',Held=true,Hidden=true,Offset=-0.125,Score='score',HoldDuration=2.5,AttackDuration=3.25,AttackModifiers='\195\169\0mods',KeysoundIndex=4}
                    if #notes>1 then notes[2]='TapNoteType_Mine' end"#).exec().unwrap();
            }
            judgment(&lua, &params, old).unwrap();
            assert_eq!(params.get::<Table>("Notes").unwrap().to_pointer(), pointer);
            let before = note_summary(&lua, &params);
            judgment(&lua, &params, old).unwrap();
            assert_eq!(note_summary(&lua, &params), before);
            outcomes.push(before);
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
    for shape in [
        "nil",
        "false",
        "42",
        "{}",
        "{[5]='TapNoteType_Lift',named={Type='TapNoteType_Fake'}}",
    ] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let params = lua
                .load(format!("return {{FirstTrack=3,Notes={shape}}}"))
                .eval::<Table>()
                .unwrap();
            judgment(&lua, &params, old).unwrap();
            outcomes.push(note_summary(&lua, &params));
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[test]
fn broadcast_storage_judgment_snapshot_preserves_mutations_and_partial_errors() {
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let params = lua
            .load(
                r#"notes={};a=setmetatable({}, {__index=function(t,k)
            if k=='GetTapNoteType' then notes[2]={replacement=true};notes[3]={inserted=true} end
            return nil
        end});b={Type='TapNoteType_Mine'};notes[1]=a;notes[2]=b;return {Notes=notes}"#,
            )
            .eval::<Table>()
            .unwrap();
        judgment(&lua, &params, old).unwrap();
        outcomes.push(lua.load("return notes[1]==a,notes[2]==b,type(b.GetTapNoteType),notes[3].GetTapNoteType==nil,b:GetTapNoteType()").eval::<(bool,bool,String,bool,String)>().unwrap());
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(
        outcomes[0],
        (
            true,
            true,
            "function".into(),
            true,
            "TapNoteType_Mine".into()
        )
    );
    for failure in ["field", "write"] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            lua.globals().set("failure", failure).unwrap();
            let params=lua.load(r#"log={};a=setmetatable({Type='TapNoteType_Tap'}, {
                __index=function(t,k) log[#log+1]='read:'..k;if failure=='field' and k=='TapNoteSource' then error('field failed') end end,
                __newindex=function(t,k,v) log[#log+1]='write:'..k;rawset(t,k,v);if failure=='write' and k=='GetTapNoteSource' then error('write failed') end end});
                b={};return {Notes={a,b}}"#).set_name("note-error").eval::<Table>().unwrap();
            let error = judgment(&lua, &params, old).unwrap_err().to_string();
            outcomes.push((error,lua.load("return table.concat(log,'|'),type(a.GetTapNoteType),type(a.GetTapNoteSource),type(b.GetTapNoteType),a.TapNoteResult==a.HoldNoteResult").eval::<(String,String,String,String,bool)>().unwrap()));
        }
        assert_eq!(outcomes[0], outcomes[1]);
        assert_eq!(outcomes[0].1.3, "nil");
        assert!(outcomes[0].1.4);
    }
}

#[test]
fn broadcast_storage_judgment_preserves_eager_field_errors_and_invalid_utf8_fallbacks() {
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let params=lua.load("return {Notes={{TapNoteType=string.char(255),Type='alias',TapNoteSource=7,Source='source'}}}").eval::<Table>().unwrap();
        judgment(&lua, &params, old).unwrap();
        outcomes.push(note_summary(&lua, &params));
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert!(outcomes[0].contains("alias:source"));
    let mut errors = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let params=lua.load("return setmetatable({Notes={{TapNoteType='present'}}},{__index=function(t,k) if k=='TapNoteType' then error('eager fallback read') end end})").set_name("eager-fallback").eval::<Table>().unwrap();
        errors.push(judgment(&lua, &params, old).unwrap_err().to_string());
        assert!(
            params
                .get::<Table>("Notes")
                .unwrap()
                .get::<Table>(1)
                .unwrap()
                .get::<Value>("GetTapNoteType")
                .unwrap()
                .is_nil()
        );
    }
    assert_eq!(errors[0], errors[1]);
    assert!(errors[0].contains("eager fallback read"));
}

#[test]
fn broadcast_storage_warm_judgment_snapshots_have_zero_churn_up_to_16_notes() {
    for count in [1, 4, 16] {
        let (lua, params) = judgment_fixture(count, true);
        crate::perf::assert_no_churn(|| {
            for _ in 0..32 {
                judgment(&lua, &params, false).unwrap();
            }
        });
        crate::perf::assert_reduced_churn(
            || {
                for _ in 0..32 {
                    judgment(&lua, &params, true).unwrap();
                }
            },
            || {
                for _ in 0..32 {
                    judgment(&lua, &params, false).unwrap();
                }
            },
        );
    }
}

#[test]
#[ignore = "manual old/new broadcast CPU, throughput and allocator benchmark"]
fn broadcast_storage_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, text, args) in [
        ("empty", String::new(), 1),
        ("short", "TapNoteType_Tap".into(), 1),
        ("unicode", "\u{e9}\0\u{65e5}".into(), 3),
        ("long128", "x".repeat(128), 1),
        ("long4096", "x".repeat(4096), 1),
        ("short_noargs", "TapNoteType_Tap".into(), 0),
    ] {
        for old in order {
            let lua = Lua::new();
            lua.gc_stop();
            let table = lua.create_table().unwrap();
            let method = getter(&lua, &table, &text, old);
            let mode = if old { "old" } else { "new" };
            // Bound uncollected long-string output while retaining GC-free timings.
            crate::perf::measure_sampled(
                &format!("getter/call/{label}/{mode}"),
                if label == "long4096" { 64 } else { 1024 },
                64,
                || {
                    for _ in 0..64 {
                        let result = match args {
                            0 => method.call::<mlua::LuaString>(()),
                            1 => method.call::<mlua::LuaString>(&table),
                            _ => method.call::<mlua::LuaString>((&table, Value::Nil, false)),
                        };
                        black_box(result.unwrap());
                    }
                },
            );
        }
    }
    for (label, text) in [
        ("short", "TapNoteType_Tap".into()),
        ("long4096", "x".repeat(4096)),
    ] {
        for call in [false, true] {
            for old in order {
                let mode = if old { "old" } else { "new" };
                let action = if call { "build_first_call" } else { "build" };
                crate::perf::measure_sampled_with_setup(
                    &format!("getter/{action}/{label}/{mode}"),
                    128,
                    1,
                    || {
                        let lua = Lua::new();
                        lua.gc_stop();
                        let table = lua.create_table().unwrap();
                        (lua, table)
                    },
                    |(lua, table)| {
                        let method = getter(lua, table, &text, old);
                        if call {
                            black_box(method.call::<mlua::LuaString>(&*table).unwrap());
                        }
                    },
                );
            }
        }
    }
    for listeners in [false, true] {
        for count in [0, 1, 8, 32, 33, 128] {
            for old in order {
                let lua = actor_fixture(count, listeners);
                let mode = if old { "old" } else { "new" };
                let label = if listeners {
                    "listeners"
                } else {
                    "no_listeners"
                };
                crate::perf::measure_sampled(
                    &format!("broadcast/{label}/{count}/{mode}"),
                    256,
                    16,
                    || {
                        for _ in 0..16 {
                            broadcast(&lua, "Tick", Some(Value::Integer(23)), old).unwrap();
                        }
                    },
                );
            }
        }
    }
    for count in [1, 4, 16, 17, 64] {
        for old in order {
            let (lua, params) = judgment_fixture(count, true);
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(
                &format!("judgment/prepared/{count}/{mode}"),
                512,
                32 * count,
                || {
                    for _ in 0..32 {
                        judgment(&lua, &params, old).unwrap();
                    }
                },
            );
        }
    }
    for count in [1, 4, 16, 17, 64] {
        for old in order {
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled_with_setup(
                &format!("judgment/fresh/{count}/{mode}"),
                128,
                count,
                || judgment_fixture(count, false),
                |(lua, params)| judgment(lua, params, old).unwrap(),
            );
        }
    }
    for old in order {
        let mode = if old { "old" } else { "new" };
        crate::perf::measure_sampled_with_setup(
            &format!("broadcast/judgment_8actors_4notes/{mode}"),
            128,
            1,
            || {
                let (lua, params) = judgment_fixture(4, false);
                let registry = lua.create_table().unwrap();
                let callback=lua.load("return function(self,p) for _,n in pairs(p.Notes) do local a,b,c=n:GetTapNoteType(),n:GetPlayerNumber(),n:GetAttackModifiers() end end").eval::<Function>().unwrap();
                for i in 1..=8 {
                    let actor = lua.create_table().unwrap();
                    actor
                        .set("JudgmentMessageCommand", callback.clone())
                        .unwrap();
                    actor_active_commands(&lua, &actor).unwrap();
                    actor_command_queue(&lua, &actor).unwrap();
                    registry.raw_set(i, actor).unwrap();
                }
                lua.globals()
                    .set("__songlua_actor_registry", registry)
                    .unwrap();
                (lua, params)
            },
            |(lua, params)| {
                broadcast(lua, "Judgment", Some(Value::Table(params.clone())), old).unwrap()
            },
        );
    }
}
