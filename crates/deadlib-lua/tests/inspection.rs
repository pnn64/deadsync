use mlua::{Function, InspectionValue, Lua, Result, Table, Value};

#[test]
fn raw_inspection_preserves_bits_bytes_and_reference_identity() -> Result<()> {
    let lua = Lua::new();
    let table = lua.create_table()?;
    let child = lua.create_table()?;
    table.set("child", child.clone())?;
    table.set("bytes", lua.create_string([0, 255, 7])?)?;
    table.set("nan", f64::from_bits(0x7ff8_0000_0000_1234))?;
    match table.raw_get::<InspectionValue>("child")? {
        InspectionValue::Table(v) => assert_eq!(v, child),
        _ => panic!("table"),
    }
    match table.raw_get::<InspectionValue>("bytes")? {
        InspectionValue::Bytes(v) => assert_eq!(v.as_ref(), [0, 255, 7]),
        _ => panic!("bytes"),
    }
    match table.raw_get::<InspectionValue>("nan")? {
        InspectionValue::Number(v) => assert_eq!(v.to_bits(), 0x7ff8_0000_0000_1234),
        _ => panic!("number"),
    }
    #[cfg(any(feature = "lua54", feature = "lua55", feature = "lua53"))]
    {
        let n = (1_i64 << 60) + 3;
        table.set("integer", n)?;
        match table.raw_get::<InspectionValue>("integer")? {
            InspectionValue::Integer(v) => assert_eq!(v, n),
            _ => panic!("integer"),
        }
    }
    Ok(())
}

#[test]
fn typed_upvalue_inspection_is_reentrant_and_excludes_native_storage() -> Result<()> {
    let lua = Lua::new();
    let f: Function = lua
        .load("local n=-0.0;local s='\\255';local t={};return function() return n,s,t end")
        .eval()?;
    let mut names = Vec::new();
    f.for_each_upvalue_as::<InspectionValue>(|_, name, value| {
        lua.gc_collect()?;
        names.push(name.to_vec());
        if let InspectionValue::Table(t) = value {
            assert_eq!(f.call::<(Value, Value, Table)>(())?.2, t);
        }
        Ok(())
    })?;
    assert_eq!(names, [b"n".to_vec(), b"s".to_vec(), b"t".to_vec()]);
    let native = lua.create_function(|_, ()| Ok(7))?;
    native.for_each_upvalue_as::<InspectionValue>(|_, _, _| panic!("private C callback cell"))
}

#[cfg(panic = "unwind")]
#[test]
fn inspection_leaves_dormant_caught_panic_payloads_for_the_original_callback() -> Result<()> {
    let lua = Lua::new();
    lua.globals().set(
        "fail",
        lua.create_function(|_, ()| -> Result<()> { panic!("dormant callback panic") })?,
    )?;
    let (f, table): (Function, Table) = lua
        .load("local ok,payload=pcall(fail);assert(not ok);return function() return payload end,{payload}")
        .eval()?;
    assert!(matches!(
        table.raw_get::<InspectionValue>(1)?,
        InspectionValue::Other("userdata")
    ));
    f.for_each_upvalue_as::<InspectionValue>(|_, _, v| {
        assert!(matches!(v, InspectionValue::Other("userdata")));
        Ok(())
    })?;
    lua.gc_collect()?;
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f.call::<Value>(()))).unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"dormant callback panic"));
    assert_eq!(lua.load("6*7").eval::<i32>()?, 42);
    Ok(())
}
