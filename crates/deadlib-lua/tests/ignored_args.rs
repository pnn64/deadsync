use mlua::{IgnoredArgs, IntoLuaMulti, Lua, MultiValue, Result, Value};

#[test]
fn checked_forwarding_preserves_arity_values_and_aliases() -> Result<()> {
    let lua = Lua::new();
    lua.globals()
        .set("echo", lua.create_function(|_, args: IgnoredArgs| Ok(args))?)?;
    lua.load(
        r#"
        local t, f, co = {}, function() end, coroutine.create(function() end)
        local args = {nil, false, 17, -0.0, string.char(255, 0), t, f, co, t, nil}
        local unpack = table.unpack or unpack
        local function check(n, ...)
            assert(select('#', ...) == n)
            local values = {...}
            for i = 1, n do assert(rawequal(values[i], args[i])) end
        end
        for n = 0, 10 do check(n, echo(unpack(args, 1, n))) end
        local thread = coroutine.create(function() check(10, echo(unpack(args, 1, 10))) end)
        assert(coroutine.resume(thread))
    "#,
    )
    .exec()
}

#[test]
fn converting_an_existing_buffer_preserves_both_slices() -> Result<()> {
    let lua = Lua::new();
    for count in [1, 9] {
        let values = MultiValue::from_vec(vec![Value::Integer(17); count]);
        let (first, second) = values.as_slices();
        let expected = (first.as_ptr(), first.len(), second.as_ptr(), second.len());
        let forwarded = IgnoredArgs::from(values).into_lua_multi(&lua)?;
        let (first, second) = forwarded.as_slices();
        assert_eq!(
            (first.as_ptr(), first.len(), second.as_ptr(), second.len()),
            expected
        );
    }
    Ok(())
}
