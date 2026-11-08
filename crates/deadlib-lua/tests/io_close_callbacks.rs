use deadlib_lua::{Error, HostCall, Value, Vm, compile};

fn close_body(call: &mut HostCall<'_>) -> Result<(), Error> {
    assert_eq!(call.function_data::<u64>(), Some(&1234));
    assert!(call.captures().is_empty());
    let mut stack = call.native_stack()?;
    assert_eq!(stack.top(), 3);
    stack.get_field(-10001, b"marker")?;
    stack.finish(1)
}
#[test]
fn close_bodies_keep_binding_data_separate_from_the_callers_lua_state() {
    let mut vm = Vm::new();
    let callback = vm
        .new_host_function_with_data(close_body, &[Value::Number(777.)], 1234u64)
        .unwrap();
    vm.set_global(b"close_body", callback).unwrap();
    vm.execute(
        &compile(
            br#"
local file=assert(io.tmpfile());local original=debug.getfenv(file)
local env=debug.getfenv(io.close)
debug.setfenv(io.close,{marker=29})
debug.setfenv(close_body,{marker=999})
debug.setfenv(file,{__close=close_body})
assert(io.close(file)==29 and io.type(file)=='file')
debug.setfenv(io.close,env);debug.setfenv(file,original);assert(file:close())
"#,
        )
        .unwrap(),
    )
    .unwrap();
}
