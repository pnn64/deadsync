//! Mr. Sandman retains 192*108 native actors in one Lua VM.
#[test]
fn rust_handles_can_retain_a_native_sized_actor_grid() {
    let lua = mlua::Lua::new();
    let mut handles = Vec::new();
    for index in 0..(192*108) {
        handles.push(lua.create_table().unwrap_or_else(|error| panic!("actor handle {index}: {error}")));
    }
    assert_eq!(handles.len(), 20_736);
    handles[20_735].raw_set("marker", 19).unwrap();
    assert_eq!(handles[20_735].raw_get::<i32>("marker").unwrap(), 19);
}

#[test]
fn normal_c_calls_keep_the_native_stack_limit() {
    let lua = mlua::Lua::new();
    let function = lua.create_function(|_, args: mlua::MultiValue| Ok(args.len())).unwrap();
    let args = mlua::MultiValue::from_vec(vec![mlua::Value::Nil; 8_001]);
    assert!(matches!(function.call::<usize>(args), Err(mlua::Error::StackError)));
}
