use mlua::{Error, IntoLua, Lua, Result, Table, Value, ffi};

#[test]
fn batches_preserve_raw_order_nil_identity_and_numeric_bits() -> Result<()> {
    let lua = Lua::new();
    let table: Table = lua
        .load("setmetatable({}, {__newindex=function() error('raw only') end})")
        .eval()?;
    let child = lua.create_table()?;
    let nan = f64::from_bits(0x7ff8_0000_0000_1234);
    let rows = [
        ("child", Value::Table(child.clone())),
        ("alias", Value::Table(child.clone())),
        ("zero", Value::Number(-0.0)),
        ("nan", Value::Number(nan)),
        ("removed", Value::Boolean(false)),
        ("removed", Value::Nil),
        ("bytes", Value::String(lua.create_string([0, 255, 4])?)),
    ];
    table.raw_set_many(rows.clone())?;
    let before = lua.create_table()?;
    for (key, value) in rows {
        before.raw_set(key, value)?;
    }
    let keys = |t: &Table| {
        t.pairs::<String, Value>()
            .map(|r| r.map(|(k, _)| k))
            .collect::<Result<Vec<_>>>()
    };
    assert_eq!(keys(&table)?, keys(&before)?);
    lua.gc_collect()?;
    assert_eq!(table.raw_get::<Table>("child")?, child);
    assert_eq!(table.raw_get::<Table>("alias")?, child);
    assert_eq!(table.raw_get::<f64>("zero")?.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(table.raw_get::<f64>("nan")?.to_bits(), nan.to_bits());
    assert!(table.raw_get::<Value>("removed")?.is_nil());
    assert_eq!(
        table.raw_get::<mlua::String>("bytes")?.as_bytes().as_ref(),
        [0, 255, 4]
    );
    Ok(())
}

struct Converted(usize);
impl IntoLua for Converted {
    fn into_lua(self, lua: &Lua) -> Result<Value> {
        let table: Table = lua.globals().get("TARGET")?;
        if self.0 == 1 {
            assert_eq!(table.raw_get::<Option<i32>>("one")?, None);
            lua.gc_collect()?;
            table.raw_set_many([("nested", 7)])?;
        } else {
            assert_eq!(table.raw_get::<i32>("one")?, 1);
            assert_eq!(table.raw_get::<i32>("nested")?, 7);
            return Err(Error::runtime("second conversion failed"));
        }
        Ok(Value::Integer(self.0 as _))
    }
}

#[test]
fn writes_and_conversions_are_reentrant_sequential_and_stop_before_next_pair() -> Result<()> {
    let lua = Lua::new();
    let table = lua.create_table()?;
    lua.globals().set("TARGET", table.clone())?;
    let mut pulled = 0;
    let rows = [
        ("one", Converted(1)),
        ("two", Converted(2)),
        ("three", Converted(3)),
    ]
    .into_iter()
    .inspect(|_| pulled += 1);
    let error = table.raw_set_many(rows).unwrap_err();
    assert_eq!(error.to_string(), "runtime error: second conversion failed");
    assert_eq!(pulled, 2);
    assert_eq!(table.raw_get::<i32>("one")?, 1);
    assert!(table.raw_get::<Value>("two")?.is_nil());
    assert!(table.raw_get::<Value>("three")?.is_nil());
    Ok(())
}

#[test]
fn invalid_keys_preserve_prior_writes_and_restore_the_callers_stack() -> Result<()> {
    let lua = Lua::new();
    let table = lua.create_table()?;
    lua.exec_raw_lua(|raw| -> Result<()> {
        // SAFETY: the VM is locked and this scope owns its reserved marker.
        unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
            raw.push(42)?;
            let top = ffi::lua_gettop(raw.state());
            for invalid in [Value::Nil, Value::Number(f64::NAN)] {
                let old = table.raw_set(invalid.clone(), 3).unwrap_err().to_string();
                let error = table
                    .raw_set_many([(Value::Integer(1), 9), (invalid, 3), (Value::Integer(2), 7)])
                    .unwrap_err();
                assert_eq!(error.to_string(), old);
                assert_eq!(table.raw_get::<i32>(1)?, 9);
                assert!(table.raw_get::<Value>(2)?.is_nil());
                assert_eq!(ffi::lua_gettop(raw.state()), top);
            }
            assert_eq!(raw.pop::<i32>()?, 42);
        }
        Ok(())
    })
}

#[cfg(panic = "unwind")]
#[test]
fn iterator_panic_restores_stack_and_keeps_the_completed_write() -> Result<()> {
    let lua = Lua::new();
    let table = lua.create_table()?;
    lua.exec_raw_lua(|raw| -> Result<()> {
        // SAFETY: only this scope's marker and the locked stack are inspected.
        unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
            raw.push(17)?;
            let top = ffi::lua_gettop(raw.state());
            let mut count = 0;
            let rows = std::iter::from_fn(|| {
                count += 1;
                if count == 2 {
                    panic!("iterator failure");
                }
                Some(("first", 9))
            });
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| table.raw_set_many(rows)))
                .unwrap_err();
            assert_eq!(error.downcast_ref::<&str>(), Some(&"iterator failure"));
            assert_eq!(table.raw_get::<i32>("first")?, 9);
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>()?, 17);
        }
        Ok(())
    })
}

#[cfg(not(feature = "luau"))]
#[test]
fn hook_presence_tracks_current_thread_and_global_hook_installation() -> Result<()> {
    let lua = Lua::new();
    assert!(!lua.has_hook());
    lua.set_hook(mlua::HookTriggers::EVERY_LINE, |_, _| Ok(mlua::VmState::Continue))?;
    assert!(lua.has_hook());
    lua.remove_hook();
    assert!(!lua.has_hook());
    lua.set_global_hook(mlua::HookTriggers::EVERY_LINE, |_, _| Ok(mlua::VmState::Continue))?;
    assert!(lua.has_hook());
    lua.remove_global_hook();
    lua.remove_hook();
    assert!(!lua.has_hook());
    Ok(())
}
