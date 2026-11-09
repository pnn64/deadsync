//! Original caller-stack checks using deadlib-lua's protected native calls.
use deadlib_lua::{Error, HostCall, NativeCallStatus, Value, Vm, compile_named};

#[test]
fn raw_read_keeps_the_callers_stack_on_success_and_error() {
    fn success(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(Value::Number(17.))?;
        stack.finish(1)
    }
    fn failure(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(Value::Number(99.))?;
        Err(stack.error(b"raw read probe"))
    }
    let mut vm = Vm::new();
    let success = vm.new_host_function(success, &[]).unwrap();
    let success_root = vm.pin(success).unwrap();
    let failure = vm.new_host_function(failure, &[]).unwrap();
    let failure_root = vm.pin(failure).unwrap();
    vm.with_native_stack(|stack| {
        assert!(stack.check_stack(8));
        stack.push(Value::Number(42.))?;
        let top = stack.top();
        stack.push(success)?;
        assert_eq!(
            stack.protected_call(0, Some(1), None)?,
            NativeCallStatus::Ok
        );
        let value = stack.value(-1);
        assert_eq!(value, Some(Value::Number(17.)));
        stack.set_top(top)?;
        assert_eq!(stack.top(), top);
        stack.push(failure)?;
        let error = stack.protected_call(0, Some(0), None)?;
        assert_eq!(error, NativeCallStatus::RuntimeError);
        // A protected error replaces only its function/arguments. The caller's
        // prefix remains intact and the error occupies one result slot.
        assert_eq!(stack.top(), top + 1);
        assert_eq!(stack.value(1), Some(Value::Number(42.)));
        stack.set_top(top)?;
        assert_eq!(stack.top(), top);
        assert_eq!(stack.value(-1), Some(Value::Number(42.)));
        stack.set_top(top - 1)?;
        Ok(())
    })
    .unwrap();
    vm.unpin(success_root).unwrap();
    vm.unpin(failure_root).unwrap();
    let code = compile_named(b"return 6*7", b"=raw stack continuation").unwrap();
    assert_eq!(vm.execute(&code).unwrap(), &[Value::Number(42.)]);
    // Rust unwinding has a different explicit contract in the Rust VM: discard
    // that VM. Keep the original panic probe in its own session accordingly.
    let mut panic_vm = Vm::new();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = panic_vm.with_native_stack::<()>(|stack| {
            assert!(stack.check_stack(8));
            stack.push(Value::Number(42.))?;
            stack.push(Value::Number(99.))?;
            panic!("raw stack unwind probe");
        });
    }));
    assert!(panic.is_err());
    drop(panic_vm);
}
