use deadlib_lua::{Error, GcOperation, HostCall, Value, Vm, compile};

#[test]
fn rejected_inactive_allocations_and_capture_writes_preserve_the_stack() {
    fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
        Ok(())
    }
    fn inspect(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let thread = s.new_thread()?;
        {
            let mut t = s.thread_stack(thread)?;
            t.push(Value::Number(17.))?;
            assert!(t.create_table(usize::MAX, 0).is_err());
            assert!(t.new_userdata(usize::MAX).is_err());
            assert!(t.push_closure(empty, 2, ()).is_err());
            assert_eq!(t.top(), 1);
            assert_eq!(t.value(1), Some(Value::Number(17.)));
            for _ in 1..256 {
                t.push(Value::Number(17.))?;
            }
            assert!(t.push_closure(empty, 256, ()).is_err());
            assert_eq!(t.top(), 256);
            t.set_top(0)?;
            t.create_table(0, 0)?;
            t.push(Value::Nil)?;
            t.push(Value::Boolean(true))?;
            assert!(t.raw_set(1).is_err());
            assert_eq!(t.top(), 3);
            assert_eq!(t.value(-1), Some(Value::Boolean(true)));
            t.set_top(1)?;
            t.push(Value::Boolean(true))?;
            assert!(t.set_metatable(1).is_err());
            assert!(t.set_environment(1).is_err());
            assert_eq!(t.top(), 2);
            t.set_top(0)?;
            t.push(Value::Number(29.))?;
            t.push_closure(empty, 1, ())?;
            t.push(Value::Number(31.))?;
            assert_eq!(t.set_upvalue(1, 2)?, None);
            assert_eq!(t.top(), 2);
            t.set_upvalue(1, 1)?;
        }
        // The closure is reachable only through the inactive thread's window.
        s.gc(GcOperation::Collect)?;
        let mut t = s.thread_stack(thread)?;
        assert_eq!(t.get_upvalue(1, 1)?, Some(b"\0".as_slice()));
        assert_eq!(t.value(-1), Some(Value::Number(31.)));
        t.set_top(0)?;
        s.finish(0)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(inspect, &[]).unwrap();
    vm.set_global(b"inspect", function).unwrap();
    vm.execute(&compile(b"inspect();collectgarbage()").unwrap())
        .unwrap();
}
