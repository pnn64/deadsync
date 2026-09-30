use super::*;
use mlua::FromLua;
use std::hint::black_box;

#[path = "update_dispatch_baseline.rs"]
pub(crate) mod baseline;

fn dispatch(lua: &Lua, root: &Value, delta: f64, old: bool) -> mlua::Result<()> {
    if old {
        baseline::run_actor_compile_update_functions_with_delta(lua, root, delta)
    } else {
        run_actor_compile_update_functions_with_delta(lua, root, delta)
    }
}

fn invalidate(lua: &Lua, old: bool) {
    if old {
        baseline::invalidate(lua);
    } else {
        invalidate_compile_update_plan(lua);
    }
}

fn recurring(lua: &Lua, actor: &Table, delta: f64, enabled: bool, old: bool) -> mlua::Result<()> {
    if old {
        baseline::run_recurring_update(lua, actor, delta, enabled)
    } else {
        run_recurring_update(lua, actor, delta, enabled)
    }
}

#[test]
fn update_dispatch_field_text_preserves_coercions_utf8_and_spill_boundaries() {
    fn check<const N: usize>(lua: &Lua, table: &Table, value: Value)
    where
        [u8; N]: smallvec::Array<Item = u8>,
    {
        table.raw_set("field", value.clone()).unwrap();
        let expected = table
            .raw_get::<Option<String>>("field")
            .map_err(|err| err.to_string());
        let actual = table
            .raw_get::<Option<LuaFieldText<N>>>("field")
            .map(|text| text.map(|text| text.as_str().to_owned()))
            .map_err(|err| err.to_string());
        assert_eq!(actual, expected);
        let expected = String::from_lua(value.clone(), lua).map_err(|err| err.to_string());
        let actual = LuaFieldText::<N>::from_lua(value, lua)
            .map(|text| text.as_str().to_owned())
            .map_err(|err| err.to_string());
        assert_eq!(actual, expected);
    }
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(17),
        Value::Number(-0.25),
        Value::Table(table.clone()),
    ];
    for len in [0, 1, 31, 32, 33, 127, 128, 129, 4096] {
        values.push(Value::String(lua.create_string("x".repeat(len)).unwrap()));
    }
    for bytes in [b"UTF-8\0\xc3\xa9".as_slice(), &[255, 0], &[b'a', 0xc3]] {
        values.push(Value::String(lua.create_string(bytes).unwrap()));
    }
    for value in values {
        check::<32>(&lua, &table, value.clone());
        check::<128>(&lua, &table, value);
    }
}

#[test]
fn update_dispatch_actor_types_preserve_case_errors_and_dynamic_lookup() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    for value in [
        Value::Nil,
        Value::Integer(123),
        Value::Boolean(true),
        Value::String(lua.create_string("bItMaPtExT").unwrap()),
        Value::String(lua.create_string("RollingNumbers").unwrap()),
        Value::String(lua.create_string("ActorFrame\0").unwrap()),
        Value::String(lua.create_string("x".repeat(4096)).unwrap()),
        Value::String(lua.create_string([255]).unwrap()),
    ] {
        actor.raw_set("__songlua_actor_type", value).unwrap();
        for expected in ["ActorFrame", "BITMAPTEXT", "123", "", &"x".repeat(4096)] {
            assert_eq!(
                actor_type_is(&actor, expected).map_err(|err| err.to_string()),
                baseline::actor_type_is(&actor, expected).map_err(|err| err.to_string())
            );
        }
        assert_eq!(
            actor_is_bitmap_text(&actor).map_err(|err| err.to_string()),
            baseline::actor_is_bitmap_text(&actor).map_err(|err| err.to_string())
        );
    }
    actor.raw_set("__songlua_actor_type", Value::Nil).unwrap();
    let meta: Table = lua.load("local n=0; return {__index=function(_,key) n=n+1; if n%2==1 then return 'BitmapText' else return 'Actor' end end}").eval().unwrap();
    actor.set_metatable(Some(meta)).unwrap();
    assert!(actor_is_bitmap_text(&actor).unwrap());
    assert!(!actor_is_bitmap_text(&actor).unwrap());
    assert!(actor_is_bitmap_text(&actor).unwrap());
}

#[test]
fn update_dispatch_keeps_recurring_preorder_callback_postorder_and_rates() {
    let mut results = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let (root, events): (Table, Table) = lua
            .load(
                r#"
            local events={}
            local function make(name,rate,children)
                local a=children or {}
                a.__songlua_actor_type='ActorFrame'
                a.__songlua_update_rate=rate
                a.__songlua_recurring_update_command='Tick'
                a.Tick=function() events[#events+1]=name..':tick' end
                a.__songlua_update_function=function(_,dt) events[#events+1]=name..':'..dt end
                return a
            end
            return make('root',2,{make('child',0.5,{make('leaf',3)})}),events
        "#,
            )
            .eval()
            .unwrap();
        dispatch(&lua, &Value::Table(root), 0.125, old).unwrap();
        results.push(
            events
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap(),
        );
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(
        results[1],
        [
            "root:tick",
            "child:tick",
            "leaf:tick",
            "leaf:0.375",
            "child:0.125",
            "root:0.25"
        ]
    );
}

#[test]
fn update_dispatch_finishes_current_plan_after_callback_invalidates_and_rebuilds_next_frame() {
    for old in [true, false] {
        let lua = Lua::new();
        let root = lua.create_table().unwrap();
        let child = lua.create_table().unwrap();
        let events = Rc::new(std::cell::RefCell::new(Vec::new()));
        let callback = lua
            .create_function({
                let root = root.clone();
                let events = events.clone();
                move |lua, (_actor, dt): (Table, f64)| {
                    events.borrow_mut().push(("child", dt));
                    root.raw_set(1, Value::Nil)?;
                    root.raw_set("__songlua_update_rate", 3.0)?;
                    invalidate(lua, old);
                    Ok(())
                }
            })
            .unwrap();
        child
            .raw_set("__songlua_update_function", callback)
            .unwrap();
        root.raw_set(1, child).unwrap();
        root.raw_set(
            "__songlua_update_function",
            lua.create_function({
                let events = events.clone();
                move |_, (_actor, dt): (Table, f64)| {
                    events.borrow_mut().push(("root", dt));
                    Ok(())
                }
            })
            .unwrap(),
        )
        .unwrap();
        let root = Value::Table(root);
        dispatch(&lua, &root, 0.25, old).unwrap();
        lua.gc_collect().unwrap();
        dispatch(&lua, &root, 0.25, old).unwrap();
        assert_eq!(
            *events.borrow(),
            [("child", 0.25), ("root", 0.25), ("root", 0.75)]
        );
    }
}

#[test]
fn update_dispatch_recurring_keeps_boundaries_caps_changes_and_command_errors() {
    for name in [
        "Tick".to_owned(),
        "\u{e9}\0command".to_owned(),
        "x".repeat(128),
        "x".repeat(129),
        "x".repeat(4096),
    ] {
        let mut results = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let actor = lua.create_table().unwrap();
            actor
                .raw_set("__songlua_recurring_update_command", name.as_str())
                .unwrap();
            actor
                .raw_set("__songlua_recurring_update_interval", 0.25)
                .unwrap();
            actor.raw_set("runs", 0).unwrap();
            actor.raw_set(name.as_str(), lua.load("return function(self) self.runs=self.runs+1; if self.runs==2 then self.__songlua_recurring_update_interval=0.125 end; if self.runs==3 then error('sentinel',0) end end").eval::<Function>().unwrap()).unwrap();
            let mut trace = Vec::new();
            for delta in [0.0, -1.0, 0.25, 0.001, 0.5, 100.0] {
                recurring(&lua, &actor, delta, true, old).unwrap();
                trace.push((
                    actor.raw_get::<i32>("runs").unwrap(),
                    actor
                        .raw_get::<Option<f64>>("__songlua_recurring_update_time_left")
                        .unwrap(),
                    actor.raw_get::<Option<bool>>(UPDATE_CMD_ERROR_KEY).unwrap(),
                ));
            }
            assert_eq!(trace[2].0, 0, "exact sleep boundary must defer the command");
            assert_eq!(trace[3].0, 1);
            assert_eq!(trace[5].0 - trace[4].0, 64);
            results.push(trace);
        }
        assert_eq!(results[0], results[1]);
    }
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    for value in [
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(17),
        Value::String(lua.create_string([255]).unwrap()),
    ] {
        actor
            .raw_set("__songlua_recurring_update_command", value)
            .unwrap();
        for enabled in [false, true] {
            assert_eq!(
                recurring(&lua, &actor, 1.0, enabled, false).map_err(|err| err.to_string()),
                recurring(&lua, &actor, 1.0, enabled, true).map_err(|err| err.to_string())
            );
        }
    }
}

struct Fixture {
    lua: Lua,
    root: Value,
}

impl Fixture {
    fn new(count: usize, commands: bool) -> Self {
        let lua = Lua::new();
        let root = lua.create_table().unwrap();
        root.raw_set("__songlua_actor_type", "ActorFrame").unwrap();
        let update: Function = lua
            .load("return function(self,dt) self.elapsed=self.elapsed+dt end")
            .eval()
            .unwrap();
        let tick: Function = lua
            .load("return function(self) self.ticks=self.ticks+1 end")
            .eval()
            .unwrap();
        for i in 1..=count {
            let actor = lua.create_table().unwrap();
            actor.raw_set("__songlua_actor_type", "Actor").unwrap();
            actor.raw_set("elapsed", 0.0).unwrap();
            actor.raw_set("ticks", 0).unwrap();
            actor.raw_set("__songlua_update_function", &update).unwrap();
            if commands {
                actor
                    .raw_set("__songlua_recurring_update_command", "Tick")
                    .unwrap();
                actor
                    .raw_set("__songlua_recurring_update_interval", 0.25)
                    .unwrap();
                actor.raw_set("Tick", &tick).unwrap();
            }
            actor.raw_set("Pulse", &tick).unwrap();
            actor_active_commands(&lua, &actor).unwrap();
            actor_command_queue(&lua, &actor).unwrap();
            root.raw_set(i, actor).unwrap();
        }
        Self {
            lua,
            root: Value::Table(root),
        }
    }

    fn batch(&self, old: bool) {
        for _ in 0..64 {
            dispatch(&self.lua, &self.root, 1.0 / 60.0, old).unwrap();
        }
    }

    fn message(&self, old: bool) {
        let Value::Table(root) = &self.root else {
            unreachable!()
        };
        for _ in 0..16 {
            if old {
                baseline::run_actor_message_with_params(&self.lua, root, "Pulse", None).unwrap();
            } else {
                run_actor_message_with_params(&self.lua, root, "Pulse", None).unwrap();
            }
        }
    }
}

#[test]
fn update_dispatch_warm_plan_and_waiting_commands_and_type_checks_have_no_churn() {
    let fixture = Fixture::new(32, false);
    fixture.batch(false);
    crate::perf::assert_no_churn(|| fixture.batch(false));
    crate::perf::assert_reduced_churn(|| fixture.batch(true), || fixture.batch(false));
    let actor = fixture.lua.create_table().unwrap();
    actor
        .raw_set("__songlua_recurring_update_command", "Tick")
        .unwrap();
    actor
        .raw_set("__songlua_recurring_update_interval", 100.0)
        .unwrap();
    actor
        .raw_set("__songlua_actor_type", "RollingNumbers")
        .unwrap();
    recurring(&fixture.lua, &actor, 0.001, true, false).unwrap();
    crate::perf::assert_no_churn(|| {
        for _ in 0..64 {
            recurring(&fixture.lua, &actor, 0.001, true, false).unwrap();
            assert!(actor_is_bitmap_text(&actor).unwrap());
            assert!(actor_type_is(&actor, "rollingnumbers").unwrap());
        }
    });
}

#[test]
fn update_dispatch_message_hierarchy_matches_old_and_reduces_churn() {
    for count in [0, 1, 32, 128] {
        let fixture = Fixture::new(count, false);
        fixture.message(true);
        fixture.message(false);
        let Value::Table(root) = &fixture.root else {
            unreachable!()
        };
        for actor in root.sequence_values::<Table>() {
            assert_eq!(actor.unwrap().raw_get::<i64>("ticks").unwrap(), 32);
        }
        if count > 0 {
            let old = Fixture::new(count, false);
            let new = Fixture::new(count, false);
            old.lua.gc_stop();
            new.lua.gc_stop();
            old.message(true);
            new.message(false);
            crate::perf::assert_reduced_churn(|| old.message(true), || new.message(false));
        }
    }
}

#[test]
#[ignore = "manual release benchmark; run serially"]
fn update_dispatch_hot_path_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [1, 32, 128] {
        for commands in [false, true] {
            for old in order {
                let fixture = Fixture::new(count, commands);
                fixture.batch(old);
                crate::perf::measure_sampled(
                    &format!(
                        "dispatch_{count}_commands_{commands}/{}",
                        if old { "old" } else { "new" }
                    ),
                    16,
                    count * 64,
                    || fixture.batch(old),
                );
            }
        }
        for old in order {
            let fixture = Fixture::new(count, false);
            fixture.lua.gc_stop();
            fixture.message(old);
            crate::perf::measure_sampled(
                &format!("messages_{count}/{}", if old { "old" } else { "new" }),
                16,
                count * 16,
                || fixture.message(old),
            );
        }
    }
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    actor
        .raw_set("__songlua_recurring_update_command", "TickCommand")
        .unwrap();
    actor
        .raw_set("__songlua_recurring_update_interval", 1.0e20)
        .unwrap();
    actor
        .raw_set("__songlua_actor_type", "RollingNumbers")
        .unwrap();
    for old in order {
        crate::perf::measure_sampled(
            &format!("waiting_command/{}", if old { "old" } else { "new" }),
            512,
            64,
            || {
                for _ in 0..64 {
                    recurring(&lua, &actor, 0.001, true, old).unwrap();
                }
            },
        );
        crate::perf::measure_sampled(
            &format!("actor_types/{}", if old { "old" } else { "new" }),
            512,
            128,
            || {
                for _ in 0..64 {
                    black_box(if old {
                        baseline::actor_type_is(&actor, "rollingnumbers")
                    } else {
                        actor_type_is(&actor, "rollingnumbers")
                    })
                    .unwrap();
                    black_box(if old {
                        baseline::actor_is_bitmap_text(&actor)
                    } else {
                        actor_is_bitmap_text(&actor)
                    })
                    .unwrap();
                }
            },
        );
    }
}
