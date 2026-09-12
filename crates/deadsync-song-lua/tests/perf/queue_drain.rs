use super::*;
use std::hint::black_box;

#[path = "queue_drain_baseline.rs"]
mod baseline;

fn drain(lua: &Lua, actor: &Table, old: bool) -> mlua::Result<()> {
    if old {
        baseline::drain_actor_command_queue(lua, actor)
    } else {
        drain_actor_command_queue(lua, actor)
    }
}

#[test]
fn lua_state_queue_preserves_callback_visibility_mutation_and_recursion() {
    let script = r#"
        local events = {}
        local actor = {
            __songlua_actor_type = 'ActorFrame',
            __songlua_capture_cursor = 3.5,
            __songlua_capture_immediate_start = 'original',
            __songlua_recurring_update_start_cursor = 11,
            __songlua_recurring_update_exact_interval = 12,
            __songlua_command_queue = {'A', 'B', 'A'},
        }
        local function record(self, name)
            events[#events+1] = name .. ':' .. table.concat(self.__songlua_command_queue, ',')
                .. ':' .. tostring(self.__songlua_capture_immediate_start)
        end
        actor.ACommand = function(self)
            record(self, 'A')
            self.__songlua_command_queue[#self.__songlua_command_queue+1] = 'C'
            self.__songlua_capture_cursor = 7
        end
        actor.BCommand = function(self)
            record(self, 'B')
            -- Replace an entry while the caller retains the same queue table.
            self.__songlua_command_queue[1] = 'D'
        end
        actor.CCommand = function(self) record(self, 'C') end
        actor.DCommand = function(self) record(self, 'D') end
        actor[1] = { ACommand = function() events[#events+1] = 'child:A' end }
        return actor, events
    "#;
    let mut outcomes = Vec::new();
    for old in [true, false] {
        let lua = Lua::new();
        let (actor, events): (Table, Table) = lua.load(script).eval().unwrap();
        let queue = actor_command_queue(&lua, &actor).unwrap();
        drain(&lua, &actor, old).unwrap();
        assert_eq!(queue.raw_len(), 0);
        assert_eq!(
            actor_command_queue(&lua, &actor).unwrap().to_pointer(),
            queue.to_pointer()
        );
        assert_eq!(
            actor
                .get::<String>("__songlua_capture_immediate_start")
                .unwrap(),
            "original"
        );
        assert_eq!(
            actor
                .get::<i32>("__songlua_recurring_update_start_cursor")
                .unwrap(),
            11
        );
        assert_eq!(
            actor
                .get::<i32>("__songlua_recurring_update_exact_interval")
                .unwrap(),
            12
        );
        assert!(actor_active_commands(&lua, &actor).unwrap().is_empty());
        outcomes.push(
            events
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap(),
        );
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert_eq!(
        outcomes[1],
        ["A:B,A:3.5", "B:A,C:7.0", "D:C:7.0", "C::7.0", "child:A"]
    );
}

#[test]
fn lua_state_queue_preserves_startup_deferral_and_raw_table_access() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor: Table = lua
            .load(
                r#"
            local q = setmetatable({'Missing', 'Missing'}, {
                __len = function() error('length metamethod') end,
                __index = function() error('index metamethod') end,
                __newindex = function() error('newindex metamethod') end,
            })
            return {__songlua_command_queue=q}
        "#,
            )
            .eval()
            .unwrap();
        lua.set_app_data(SongLuaStartupQueues(Vec::new()));
        drain(&lua, &actor, old).unwrap();
        drain(&lua, &actor, old).unwrap();
        let queued = lua.remove_app_data::<SongLuaStartupQueues>().unwrap();
        assert_eq!(queued.0.len(), 1);
        assert_eq!(queued.0[0].to_pointer(), actor.to_pointer());
        assert_eq!(actor_command_queue(&lua, &actor).unwrap().raw_len(), 2);
        drain(&lua, &actor, old).unwrap();
        assert_eq!(actor_command_queue(&lua, &actor).unwrap().raw_len(), 0);
        // Missing queues are created, even when there is nothing to dispatch.
        let empty = lua.create_table().unwrap();
        drain(&lua, &empty, old).unwrap();
        assert!(
            empty
                .raw_get::<Table>("__songlua_command_queue")
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn lua_state_queue_preserves_errors_holes_numeric_names_and_partial_progress() {
    for body in [
        "return {__songlua_command_queue={true, 'Later'}}",
        "return {__songlua_command_queue={'Missing', true, 'Later'}}",
        "return {__songlua_command_queue={'Fail', 'Later'}, FailCommand=function() error('queue sentinel', 0) end}",
        "return {__songlua_command_queue={'Missing'}, __songlua_capture_cursor={}}",
        "return {__songlua_command_queue={nil, 'Missing', 'Later'}}",
        "return {__songlua_command_queue={123, 'Missing'}, ['123Command']=function(self) self.called=true end}",
        "return {__songlua_command_queue={string.char(255), 'Missing'}}",
    ] {
        let mut outcomes = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let actor: Table = lua.load(body).set_name("queue-fixture").eval().unwrap();
            actor
                .raw_set("__songlua_capture_immediate_start", "before")
                .unwrap();
            let result = drain(&lua, &actor, old).map_err(|error| error.to_string());
            let queue = actor_command_queue(&lua, &actor).unwrap();
            let remaining: Vec<_> = (1..=3)
                .map(|i| match queue.raw_get::<Value>(i).unwrap() {
                    Value::String(s) => format!("{:?}", s.as_bytes().as_ref()),
                    v => format!("{v:?}"),
                })
                .collect();
            outcomes.push((
                result,
                remaining,
                actor
                    .get::<String>("__songlua_capture_immediate_start")
                    .unwrap(),
                actor.get::<Option<bool>>("called").unwrap(),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1], "{body}");
    }
}

#[test]
fn lua_state_queue_warm_empty_drain_has_no_churn() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    drain(&lua, &actor, false).unwrap();
    crate::perf::assert_no_churn(|| drain(&lua, &actor, false).unwrap());
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_state_bench_queue_drain() {
    for (count, callbacks) in [
        (0, false),
        (1, false),
        (8, false),
        (64, false),
        (256, false),
        (1, true),
        (8, true),
        (32, true),
    ] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let queue = actor_command_queue(&lua, &actor).unwrap();
        let names: Vec<_> = (0..count)
            .map(|i| lua.create_string(format!("q{i}")).unwrap())
            .collect();
        let callback: Function = lua
            .load("return function(self) self.hits = self.hits + 1 end")
            .eval()
            .unwrap();
        actor.raw_set("hits", 0).unwrap();
        if callbacks {
            for i in 0..count {
                actor.raw_set(format!("q{i}Command"), &callback).unwrap();
            }
        }
        let refill = || {
            actor.raw_set("hits", 0).unwrap();
            for (i, name) in names.iter().enumerate() {
                queue.raw_set(i + 1, name).unwrap();
            }
        };
        for old in [true, false] {
            refill();
            drain(&lua, &actor, old).unwrap();
            assert_eq!(queue.raw_len(), 0);
            assert_eq!(
                actor.get::<usize>("hits").unwrap(),
                if callbacks { count } else { 0 }
            );
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
                &format!(
                    "queue_{count}_callbacks_{callbacks}/{}",
                    if old { "old" } else { "new" }
                ),
                if count <= 8 { 512 } else { 32 },
                count.max(1),
                || {
                    refill();
                    drain(&lua, black_box(&actor), black_box(old)).unwrap();
                },
            );
        }
    }
}
