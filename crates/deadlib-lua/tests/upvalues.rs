use mlua::{Error, Function, Lua, MultiValue, Result, Value, ffi};

#[test]
fn restoring_an_upvalue_preserves_shared_cells_and_gc_roots() -> Result<()> {
    let lua = Lua::new();
    let (read, other, write): (Function, Function, Function) = lua
        .load("local x={}; return function() return x end, function() return x end, function(v) x=v end")
        .eval()?;
    let original: Value = read.call(())?;
    write.call::<()>(17)?;
    assert_eq!(other.call::<i32>(())?, 17);
    assert!(read.set_upvalue(1, original.clone())?);
    lua.gc_collect()?;
    assert_eq!(read.call::<Value>(())?, original);
    assert_eq!(other.call::<Value>(())?, original);
    write.call::<()>(false)?;
    assert_eq!(read.call::<bool>(())?, false);
    assert!(!read.set_upvalue(0, true)?);
    assert!(!read.set_upvalue(255, true)?);
    let native = lua.create_function(|_, ()| Ok(42))?;
    assert!(!native.set_upvalue(1, true)?);
    assert_eq!(native.call::<i32>(())?, 42);
    for bits in [(-0.0_f64).to_bits(), 0x7ff8_0000_0000_1234] {
        assert!(read.set_upvalue(1, f64::from_bits(bits))?);
        let value: f64 = other.call(())?;
        assert_eq!(value.to_bits(), bits);
    }
    Ok(())
}

#[test]
fn failed_upvalue_conversion_restores_stack_and_leaves_cell_unchanged() -> Result<()> {
    struct Fails;
    impl mlua::IntoLua for Fails {
        fn into_lua(self, _: &Lua) -> Result<Value> {
            Err(Error::runtime("cannot convert saved value"))
        }
    }
    let lua = Lua::new();
    let function: Function = lua.load("local x=17; return function() return x end").eval()?;
    lua.exec_raw_lua(|raw| -> Result<()> {
        // SAFETY: this scope owns the lock and reserves its marker slot.
        unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
            raw.push(42)?;
            let top = ffi::lua_gettop(raw.state());
            assert!(function.set_upvalue(1, Fails).is_err());
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(function.call::<i32>(())?, 17);
            assert!(function.set_upvalue(1, 23)?);
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>()?, 42);
        }
        Ok(())
    })?;
    assert_eq!(function.call::<i32>(())?, 23);
    Ok(())
}

#[test]
fn upvalue_names_values_and_aliases_survive_gc_and_reentrant_calls() -> Result<()> {
    let lua = Lua::new();
    let function: Function = lua
        .load("local first={}; local second=first; local n=-0.0; return function() return first,second,n end")
        .eval()?;
    let mut values = Vec::new();
    function.for_each_upvalue(|index, name, value| {
        let name_before = name.to_vec();
        lua.gc_collect()?;
        let returned = function.call::<MultiValue>(())?;
        assert_eq!(returned.len(), 3);
        assert_eq!(name, name_before);
        values.push((index, name.to_vec(), value));
        Ok(())
    })?;
    assert_eq!(
        values
            .iter()
            .map(|(i, name, _)| (*i, name.as_slice()))
            .collect::<Vec<_>>(),
        [
            (1, b"first".as_slice()),
            (2, b"second".as_slice()),
            (3, b"n".as_slice())
        ]
    );
    assert_eq!(values[0].2, values[1].2);
    assert!(matches!(&values[0].2, Value::Table(_)));
    // Lua versions with an integer subtype may fold the literal to integer zero.
    // For floating values, preserve exactly the bits returned by the function.
    assert_eq!(values[2].2, function.call::<MultiValue>(())?[2]);
    lua.gc_collect()?;
    assert_eq!(values[0].2, function.call::<MultiValue>(())?[0]);
    Ok(())
}

#[test]
fn upvalue_visitor_restores_the_callers_stack_on_success_error_and_panic() -> Result<()> {
    let lua = Lua::new();
    let function: Function = lua
        .load("local a,b=1,{}; return function() return a,b end")
        .eval()?;
    lua.exec_raw_lua(|raw| -> Result<()> {
        // SAFETY: this callback holds the VM lock. Reserve a marker slot and
        // inspect only this active state's stack. Pop the marker before return.
        unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
            raw.push(41)?;
            let top = ffi::lua_gettop(raw.state());
            function.for_each_upvalue(|_, _, _| Ok(()))?;
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            let mut visited = Vec::new();
            let error = function
                .for_each_upvalue(|index, _, _| {
                    visited.push(index);
                    Err(Error::runtime("upvalue visit failure"))
                })
                .unwrap_err();
            assert_eq!(error.to_string(), "runtime error: upvalue visit failure");
            assert_eq!(visited, [1]);
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            #[cfg(not(panic = "abort"))]
            {
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    function.for_each_upvalue(|_, _, _| panic!("upvalue visitor panic"))
                }));
                assert!(panic.is_err());
                assert_eq!(ffi::lua_gettop(raw.state()), top);
            }
            assert_eq!(raw.pop::<i32>()?, 41);
        }
        Ok(())
    })?;
    assert_eq!(lua.load("6 * 7").eval::<i32>()?, 42);
    Ok(())
}

#[test]
fn upvalue_visitors_can_nest_inside_coroutine_callbacks() -> Result<()> {
    let lua = Lua::new();
    let function: Function = lua
        .load("local a,b=17,23; return function() return a+b end")
        .eval()?;
    let callback = lua.create_function(move |_, marker: i32| {
        let mut outer = Vec::new();
        function.for_each_upvalue(|index, name, value| {
            let mut inner = Vec::new();
            function.for_each_upvalue(|index, _, value| {
                inner.push((index, value));
                Ok(())
            })?;
            assert_eq!(inner.len(), 2);
            outer.push((index, name.to_vec(), value));
            Ok(())
        })?;
        assert_eq!(outer.len(), 2);
        Ok(marker + function.call::<i32>(())?)
    })?;
    lua.globals().set("inspect", callback)?;
    lua.load("local co=coroutine.create(function() assert(inspect(2)==42) end); assert(coroutine.resume(co))")
        .exec()
}

#[test]
fn upvalue_numbers_preserve_zero_sign_and_nan_payloads() -> Result<()> {
    let lua = Lua::new();
    let factory: Function = lua
        .load("return function(n) return function() return n end end")
        .eval()?;
    for bits in [(-0.0_f64).to_bits(), 0x7ff8_0000_0000_1234, 0xfff8_0000_0000_4321] {
        let function: Function = factory.call(f64::from_bits(bits))?;
        let expected: f64 = function.call(())?;
        let mut values = Vec::new();
        function.for_each_upvalue(|_, _, value| {
            values.push(value);
            Ok(())
        })?;
        let [Value::Number(number)] = values.as_slice() else {
            panic!("expected captured float");
        };
        assert_eq!(number.to_bits(), expected.to_bits());
        #[cfg(feature = "lua51")]
        assert_eq!(expected.to_bits(), bits);
    }
    Ok(())
}

#[test]
fn empty_stripped_and_c_functions_can_be_inspected() -> Result<()> {
    let lua = Lua::new();
    let empty: Function = lua.load("return function() return 7 end").eval()?;
    let mut count = 0;
    empty.for_each_upvalue(|_, _, _| {
        count += 1;
        Ok(())
    })?;
    assert_eq!(count, 0);
    let c_function = lua.create_function(|_, ()| Ok(17))?;
    c_function.for_each_upvalue(|_, _, _| {
        count += 1;
        Ok(())
    })?;
    assert_eq!(count, 0, "native callback internals must stay private");
    #[cfg(not(feature = "luau"))]
    {
        let named: Function = lua
            .load("local captured=7; return function() return captured end")
            .eval()?;
        let stripped = lua.load(named.dump(true)).into_function()?;
        let mut visited = 0;
        stripped.for_each_upvalue(|index, _, value| {
            visited += 1;
            assert!(index > 0);
            // Loading dumped code leaves upvalue cells uninitialized.
            assert!(value.is_nil() || matches!(value, Value::Table(_)));
            Ok(())
        })?;
        assert_eq!(visited, named.info().num_upvalues);
    }
    Ok(())
}
