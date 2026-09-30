use super::*;
use mlua::FromLua;
use std::hint::black_box;

#[path = "frame_capture_baseline.rs"]
mod baseline;

fn call(
    lua: &Lua,
    actor: &Table,
    function: &Function,
    params: Option<Value>,
    old: bool,
) -> mlua::Result<()> {
    if old {
        baseline::call_actor_function(lua, actor, function, params)
    } else {
        call_actor_function(lua, actor, function, params)
    }
}

#[test]
fn frame_capture_directory_scope_preserves_values_coercions_errors_and_callback_mutations() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let prior = lua.create_table().unwrap();
    let function: Function = lua
        .load(
            r#"
        return function(self,param)
            self.calls=self.calls+1
            self.seen=tostring(__songlua_script_dir)
            self.param=param
            self.__songlua_script_dir='changed during callback'
            __songlua_script_dir='changed global during callback'
            return 99, false
        end
    "#,
        )
        .eval()
        .unwrap();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(42),
        Value::Number(-0.25),
        Value::Table(actor.clone()),
    ];
    for text in [
        "",
        " \t\n",
        "\u{2003}\u{a0}",
        "Songs/Pack/Song",
        " Songs/\u{e9}\0/ ",
        &"x".repeat(4096),
    ] {
        values.push(Value::String(lua.create_string(text).unwrap()));
    }
    values.push(Value::String(lua.create_string([255, 0]).unwrap()));
    for value in values {
        let mut results = Vec::new();
        for old in [true, false] {
            actor.raw_set("calls", 0).unwrap();
            actor.raw_set("seen", Value::Nil).unwrap();
            actor
                .raw_set("__songlua_script_dir", value.clone())
                .unwrap();
            lua.globals()
                .raw_set("__songlua_script_dir", &prior)
                .unwrap();
            let result = call(&lua, &actor, &function, Some(Value::Number(0.125)), old)
                .map_err(|err| err.to_string());
            let calls = actor.raw_get::<i64>("calls").unwrap();
            let seen = actor.raw_get::<Option<String>>("seen").unwrap();
            let restored = lua
                .globals()
                .raw_get::<Value>("__songlua_script_dir")
                .unwrap();
            // Blank/missing directories bypass the scope; a callback can leave
            // its global mutation in place. All other successful calls restore.
            let restored = match restored {
                Value::Table(table) => table.to_pointer() == prior.to_pointer(),
                _ => false,
            };
            if calls == 1 {
                assert_eq!(actor.raw_get::<f64>("param").unwrap(), 0.125);
            }
            results.push((result, calls, seen, restored));
        }
        assert_eq!(results[0], results[1]);
        let expected = String::from_lua(value.clone(), &lua)
            .map(|text| !text.trim().is_empty())
            .map_err(|err| err.to_string());
        let actual = ActorScriptDir::from_lua(value, &lua)
            .map(|dir| matches!(dir, ActorScriptDir::Text(_)))
            .map_err(|err| err.to_string());
        assert_eq!(actual, expected);
    }
}

#[test]
fn frame_capture_directory_scope_restores_nested_calls_and_partial_errors_after_gc() {
    for old in [true, false] {
        let lua = Lua::new();
        let parent = lua.create_table().unwrap();
        let child = lua.create_table().unwrap();
        let outer = "Songs/Outer/".repeat(64);
        let inner = "Songs/Inner/\u{e9}/".repeat(64);
        parent
            .raw_set("__songlua_script_dir", outer.as_str())
            .unwrap();
        child
            .raw_set("__songlua_script_dir", inner.as_str())
            .unwrap();
        lua.globals().raw_set("__songlua_script_dir", 7).unwrap();
        let inner_callback = lua
            .create_function(move |lua, child: Table| {
                assert_eq!(lua.globals().get::<String>("__songlua_script_dir")?, inner);
                child.raw_set("__songlua_script_dir", Value::Nil)?;
                lua.gc_collect()?;
                child.raw_set("partial", 19)?;
                Err::<(), _>(mlua::Error::RuntimeError("inner sentinel".into()))
            })
            .unwrap();
        let outer_callback = lua
            .create_function({
                let child = child.clone();
                move |lua, parent: Table| {
                    assert_eq!(lua.globals().get::<String>("__songlua_script_dir")?, outer);
                    assert!(call(lua, &child, &inner_callback, None, old).is_err());
                    assert_eq!(lua.globals().get::<String>("__songlua_script_dir")?, outer);
                    parent.raw_set("__songlua_script_dir", Value::Nil)?;
                    lua.gc_collect()?;
                    assert_eq!(lua.globals().get::<String>("__songlua_script_dir")?, outer);
                    Ok(())
                }
            })
            .unwrap();
        call(&lua, &parent, &outer_callback, None, old).unwrap();
        assert_eq!(
            lua.globals()
                .raw_get::<i64>("__songlua_script_dir")
                .unwrap(),
            7
        );
        assert_eq!(child.raw_get::<i64>("partial").unwrap(), 19);
    }
}

#[test]
fn frame_capture_directory_scope_preserves_metamethod_order_and_restore_error_precedence() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    actor
        .raw_set("__songlua_script_dir", "Songs/Scoped")
        .unwrap();
    let writes = Rc::new(std::cell::RefCell::new(Vec::new()));
    let meta = lua.create_table().unwrap();
    meta.raw_set(
        "__index",
        lua.create_function(|_, (_, key): (Table, String)| {
            assert_eq!(key, "__songlua_script_dir");
            Ok("inherited prior")
        })
        .unwrap(),
    )
    .unwrap();
    meta.raw_set(
        "__newindex",
        lua.create_function({
            let writes = writes.clone();
            move |_, (_, key, value): (Table, String, String)| {
                assert_eq!(key, "__songlua_script_dir");
                let mut writes = writes.borrow_mut();
                writes.push(value);
                if writes.len() == 2 {
                    return Err(mlua::Error::RuntimeError("restore sentinel".into()));
                }
                Ok(())
            }
        })
        .unwrap(),
    )
    .unwrap();
    lua.globals().set_metatable(Some(meta)).unwrap();
    let callback: Function = lua
        .load("return function(self) self.partial=true; error('callback sentinel',0) end")
        .eval()
        .unwrap();
    let mut results = Vec::new();
    for old in [true, false] {
        writes.borrow_mut().clear();
        let error = call(&lua, &actor, &callback, None, old)
            .unwrap_err()
            .to_string();
        assert!(error.contains("restore sentinel"));
        assert!(actor.raw_get::<bool>("partial").unwrap());
        results.push((error, writes.borrow().clone()));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1].1, ["Songs/Scoped", "inherited prior"]);
}

struct DirectoryFixture {
    lua: Lua,
    actor: Table,
    function: Function,
}

impl DirectoryFixture {
    fn new(len: usize) -> Self {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let dir = format!("Songs/Pack/{}", "x".repeat(len));
        actor.raw_set("__songlua_script_dir", dir).unwrap();
        actor.raw_set("calls", 0).unwrap();
        let function: Function=lua.load("return function(self,dt) assert(__songlua_script_dir==self.__songlua_script_dir); assert(dt==0.125); self.calls=self.calls+1 end").eval().unwrap();
        lua.globals()
            .raw_set("__songlua_script_dir", "previous directory")
            .unwrap();
        Self {
            lua,
            actor,
            function,
        }
    }
    fn batch(&self, old: bool) {
        for _ in 0..64 {
            call(
                &self.lua,
                &self.actor,
                &self.function,
                Some(Value::Number(0.125)),
                old,
            )
            .unwrap();
        }
    }
}

#[test]
fn frame_capture_warm_directory_callbacks_have_no_allocation_churn_at_all_lengths() {
    for len in [0, 24, 96, 512, 4096] {
        let fixture = DirectoryFixture::new(len);
        fixture.batch(false);
        crate::perf::assert_no_churn(|| fixture.batch(false));
        crate::perf::assert_reduced_churn(|| fixture.batch(true), || fixture.batch(false));
        assert_eq!(fixture.actor.raw_get::<i64>("calls").unwrap(), 256);
        assert_eq!(
            fixture
                .lua
                .globals()
                .raw_get::<String>("__songlua_script_dir")
                .unwrap(),
            "previous directory"
        );
    }
}

fn clear(capture: &mut SongLuaOverlayUpdateCapture) {
    for &index in &capture.touched {
        capture.values[index].clear();
        capture.final_values[index].clear();
        capture.scheduled[index].clear();
        capture.touched_flags[index] = false;
    }
    capture.touched.clear();
}
fn clear_old(capture: &mut baseline::SongLuaOverlayUpdateCapture) {
    for &index in &capture.touched {
        capture.values[index].clear();
        capture.final_values[index].clear();
        capture.scheduled[index].clear();
        capture.touched_flags[index] = false;
    }
    capture.touched.clear();
}

const TARGETS: [SongLuaOverlayUpdateTarget; 4] = [
    SongLuaOverlayUpdateTarget::X,
    SongLuaOverlayUpdateTarget::Y,
    SongLuaOverlayUpdateTarget::ZoomX,
    SongLuaOverlayUpdateTarget::Visible,
];

fn capture_batch(actors: &[Table], capture: &mut SongLuaOverlayUpdateCapture) {
    for tick in 0..16 {
        for actor in actors {
            for target in TARGETS {
                black_box(capture.record(
                    actor,
                    0.0,
                    target,
                    SongLuaOverlayUpdateValue::F32(tick as f32),
                ));
            }
        }
        clear(capture);
    }
}
fn capture_batch_old(actors: &[Table], capture: &mut baseline::SongLuaOverlayUpdateCapture) {
    for tick in 0..16 {
        for actor in actors {
            for target in TARGETS {
                black_box(capture.record(
                    actor,
                    0.0,
                    target,
                    SongLuaOverlayUpdateValue::F32(tick as f32),
                ));
            }
        }
        clear_old(capture);
    }
}

#[test]
fn frame_capture_fast_actor_lookup_preserves_missing_actors_first_touch_and_stateful_writes() {
    let lua = Lua::new();
    let actors: Vec<_> = (0..129).map(|_| lua.create_table().unwrap()).collect();
    let indices = || {
        actors[..128]
            .iter()
            .enumerate()
            .rev()
            .map(|(i, a)| (a.to_pointer() as usize, i))
    };
    let mut old = baseline::SongLuaOverlayUpdateCapture::new(indices().collect());
    let mut new = SongLuaOverlayUpdateCapture::new(indices().collect());
    for message in [None, Some("Pulse"), Some("\u{e9}\0Changed")] {
        old.active_broadcast = message.map(str::to_owned);
        new.active_broadcast = message.map(str::to_owned);
        for actor in [127, 0, 17, 127, 128, 31] {
            for (i, target) in TARGETS.into_iter().enumerate() {
                let value = SongLuaOverlayUpdateValue::F32(i as f32);
                assert_eq!(
                    new.record(&actors[actor], -0.0, target, value.clone()),
                    old.record(&actors[actor], -0.0, target, value)
                );
                let value = SongLuaOverlayUpdateValue::Bool(false);
                assert_eq!(
                    new.record_scheduled(
                        &actors[actor],
                        2.5,
                        0.25,
                        0.5,
                        Some("linear".into()),
                        Some(0.75),
                        target,
                        value.clone()
                    ),
                    old.record_scheduled(
                        &actors[actor],
                        2.5,
                        0.25,
                        0.5,
                        Some("linear".into()),
                        Some(0.75),
                        target,
                        value
                    )
                );
            }
        }
        assert_eq!(new.touched, old.touched);
        assert_eq!(new.touched, [127, 0, 17, 31]);
        assert_eq!(new.touched_flags, old.touched_flags);
        assert_eq!(new.values, old.values);
        assert_eq!(new.final_values, old.final_values);
        assert_eq!(new.stateful_messages, old.stateful_messages);
        assert_eq!(new.stateful_writes, old.stateful_writes);
        assert!(new.active_broadcast_command.is_none());
        assert!(old.active_broadcast_command.is_none());
        assert_eq!(new.runtime_broadcasts, old.runtime_broadcasts);
        for (new, old) in new.scheduled.iter().zip(&old.scheduled) {
            // Compare all fields without requiring a public Debug implementation.
            for (a, b) in new.iter().zip(old) {
                assert_eq!(
                    (
                        a.delay_seconds,
                        a.duration_seconds,
                        &a.easing,
                        a.opt1,
                        a.target,
                        &a.value
                    ),
                    (
                        b.delay_seconds,
                        b.duration_seconds,
                        &b.easing,
                        b.opt1,
                        b.target,
                        &b.value
                    )
                );
            }
            assert_eq!(new.len(), old.len());
        }
        clear(&mut new);
        clear_old(&mut old);
    }
    new.active_broadcast = None;
    old.active_broadcast = None;
    capture_batch(&actors[..128], &mut new);
    capture_batch_old(&actors[..128], &mut old);
    crate::perf::assert_no_churn(|| capture_batch(&actors[..128], &mut new));
    crate::perf::assert_no_churn(|| capture_batch_old(&actors[..128], &mut old));
    // The compatibility entry point retains index ownership and duplicate-key
    // behavior while the compiler can build the internal map directly.
    begin_overlay_update_capture(&lua, HashMap::from([(actors[0].to_pointer() as usize, 0)]));
    assert_eq!(
        lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
            .unwrap()
            .touch(&actors[0]),
        Some(0)
    );
    end_overlay_update_capture(&lua);
}

#[test]
#[ignore = "manual paired release benchmark; run serially"]
fn frame_capture_hot_path_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for len in [0, 24, 96, 512, 4096] {
        for old in order {
            let fixture = DirectoryFixture::new(len);
            fixture.batch(old);
            crate::perf::measure_sampled(
                &format!("script_scope_{len}/{}", if old { "old" } else { "new" }),
                128,
                64,
                || fixture.batch(old),
            );
        }
    }
    let lua = Lua::new();
    for count in [1, 32, 128, 512] {
        let actors: Vec<_> = (0..count).map(|_| lua.create_table().unwrap()).collect();
        for old in order {
            if old {
                let mut capture = baseline::SongLuaOverlayUpdateCapture::new(
                    actors
                        .iter()
                        .enumerate()
                        .map(|(i, a)| (a.to_pointer() as usize, i))
                        .collect(),
                );
                capture_batch_old(&actors, &mut capture);
                crate::perf::measure_sampled(
                    &format!("actor_capture_{count}/old"),
                    64,
                    count * 4 * 16,
                    || capture_batch_old(&actors, &mut capture),
                );
            } else {
                let mut capture = SongLuaOverlayUpdateCapture::new(
                    actors
                        .iter()
                        .enumerate()
                        .map(|(i, a)| (a.to_pointer() as usize, i))
                        .collect(),
                );
                capture_batch(&actors, &mut capture);
                crate::perf::measure_sampled(
                    &format!("actor_capture_{count}/new"),
                    64,
                    count * 4 * 16,
                    || capture_batch(&actors, &mut capture),
                );
            }
        }
    }
}
