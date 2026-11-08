use mlua::{Error, FromLua, Lua, Result, Table, Value, ffi};

#[test]
fn fixed_reads_preserve_bits_coercion_holes_and_raw_access() -> Result<()> {
    let lua = Lua::new();
    let table: Table = lua
        .load("setmetatable({-0.0,'2.5',nil,4,5}, {__index=function() error('raw access required') end})")
        .eval()?;
    let actual = table.raw_get_array::<Option<f64>, 4>()?;
    for (index, value) in actual.into_iter().enumerate() {
        let expected = table.raw_get::<Option<f64>>(index + 1)?;
        assert_eq!(value.map(f64::to_bits), expected.map(f64::to_bits));
    }
    assert_eq!(table.raw_get_array::<Value, 0>()?.len(), 0);
    let nan = f64::from_bits(0x7ff8_0000_0000_1234);
    table.raw_set(1, nan)?;
    assert_eq!(table.raw_get_array::<f64, 1>()?[0].to_bits(), nan.to_bits());
    let old = table.raw_get::<f32>(3).unwrap_err().to_string();
    assert_eq!(table.raw_get_array::<f32, 4>().unwrap_err().to_string(), old);
    Ok(())
}

struct Reentrant;
impl FromLua for Reentrant {
    fn from_lua(value: Value, lua: &Lua) -> Result<Self> {
        let index = usize::from_lua(value, lua)?;
        let table: Table = lua.globals().get("input")?;
        let calls: Table = lua.globals().get("calls")?;
        calls.raw_set(calls.raw_len() + 1, index)?;
        if index == 1 {
            lua.gc_collect()?;
            table.raw_set(2, 7)?;
            assert_eq!(table.raw_get_array::<usize, 2>()?, [1, 7]);
        } else {
            return Err(Error::runtime("stop after changed second slot"));
        }
        Ok(Self)
    }
}

#[test]
fn conversions_are_sequential_reentrant_and_stop_on_error() -> Result<()> {
    let lua = Lua::new();
    let table = lua.create_sequence_from([1, 2, 3])?;
    let calls = lua.create_table()?;
    lua.globals().set("input", table.clone())?;
    lua.globals().set("calls", calls.clone())?;
    assert!(table.raw_get_array::<Reentrant, 3>().is_err());
    assert_eq!(calls.raw_get_array::<usize, 2>()?, [1, 7]);
    assert_eq!(calls.raw_len(), 2);
    assert_eq!(table.raw_get::<usize>(3)?, 3);
    Ok(())
}

#[test]
fn arrays_keep_identity_and_restore_stack_after_error() -> Result<()> {
    let lua = Lua::new();
    let table: Table = lua.load("local t={}; return {t,t,false}").eval()?;
    lua.exec_raw_lua(|raw| -> Result<()> {
        // SAFETY: this scope holds the VM lock. Only its marker and stack
        // height are inspected, and the marker is popped before returning.
        unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
            raw.push(42)?;
            let top = ffi::lua_gettop(raw.state());
            let [a, b] = table.raw_get_array::<Table, 2>()?;
            lua.gc_collect()?;
            assert_eq!(a, b);
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert!(table.raw_get_array::<Table, 3>().is_err());
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>()?, 42);
        }
        Ok(())
    })
}

#[cfg(panic = "unwind")]
#[test]
fn checked_array_reads_resume_caught_rust_panics_and_restore_stack() -> Result<()> {
    let lua = Lua::new();
    lua.globals().set(
        "fail",
        lua.create_function(|_, ()| -> Result<()> { panic!("saved panic payload") })?,
    )?;
    let table: Table = lua
        .load("local ok, err=pcall(fail); assert(not ok); return {err}")
        .eval()?;
    lua.exec_raw_lua(|raw| -> Result<()> {
        // SAFETY: this scope holds the VM lock and owns its reserved marker.
        unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
            raw.push(42)?;
            let top = ffi::lua_gettop(raw.state());
            let panic =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| table.raw_get_array::<Value, 1>()))
                    .unwrap_err();
            assert_eq!(panic.downcast_ref::<&str>(), Some(&"saved panic payload"));
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>()?, 42);
        }
        Ok(())
    })
}
