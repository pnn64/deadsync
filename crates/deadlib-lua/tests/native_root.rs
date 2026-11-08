use deadlib_lua::{GcOperation, NativeCallStatus, NativeLoadStatus, NativeThreadStatus, Value, Vm};

#[test]
fn root_stack_has_no_activation_and_retains_collector_roots_between_entries() {
    let mut vm = Vm::new_bare();
    let retained = vm
        .with_native_stack(|stack| {
            let main = stack.thread_id();
            assert_eq!(stack.top(), 0);
            assert_eq!(stack.thread_call_level(main), Some(0));
            assert!(stack.get_stack(main, 0)?.is_none());
            stack.create_table(0, 1)?;
            stack.push_integer(42)?;
            stack.raw_set_integer(1, 7)?;
            Ok(stack.value(1).unwrap())
        })
        .unwrap();
    vm.collect_garbage().unwrap();
    vm.with_native_stack(|stack| {
        assert_eq!(stack.top(), 1);
        assert_eq!(stack.value(1), Some(retained));
        stack.raw_get_integer(1, 7)?;
        assert_eq!(stack.value(2), Some(Value::Number(42.)));
        stack.gc(GcOperation::Collect)?;
        assert_eq!(stack.value(1), Some(retained));
        Ok(())
    })
    .unwrap();
    vm.with_native_stack(|stack| {
        assert_eq!(stack.top(), 2);
        assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
        Ok(())
    })
    .unwrap();
}

#[test]
fn root_parser_calls_protection_and_recovery_keep_the_prefix() {
    let mut vm = Vm::new();
    vm.with_native_stack(|stack| {
        stack.push_integer(17)?;
        assert_eq!(
            stack.load_buffer(
                b"assert(debug.getinfo(2)==nil);return 42,nil,false",
                Some(b"=root")
            )?,
            NativeLoadStatus::Ok
        );
        assert_eq!(stack.protected_call(0, None, None)?, NativeCallStatus::Ok);
        assert_eq!(stack.top(), 4);
        assert_eq!(stack.value(1), Some(Value::Number(17.)));
        assert_eq!(stack.value(2), Some(Value::Number(42.)));
        assert_eq!(stack.value(3), Some(Value::Nil));
        assert_eq!(stack.value(4), Some(Value::Boolean(false)));
        stack.set_top(1)?;
        for (depth, status) in [
            (198, NativeLoadStatus::Ok),
            (199, NativeLoadStatus::SyntaxError),
        ] {
            let source = format!("return {}1{}", "(".repeat(depth), ")".repeat(depth));
            assert_eq!(
                stack.load_buffer(source.as_bytes(), Some(b"=depth"))?,
                status
            );
            assert_eq!(stack.top(), 2);
            stack.set_top(1)?;
        }
        assert_eq!(
            stack.load_buffer(b"error('root failure',0)", None)?,
            NativeLoadStatus::Ok
        );
        assert_eq!(
            stack.protected_call(0, Some(0), None)?,
            NativeCallStatus::RuntimeError
        );
        assert_eq!(stack.to_bytes(-1)?, Some(b"root failure".as_slice()));
        assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
        stack.set_top(1)?;
        assert_eq!(stack.load_buffer(b"return 91", None)?, NativeLoadStatus::Ok);
        stack.call_unprotected(0, Some(1))?;
        assert_eq!(stack.value(-1), Some(Value::Number(91.)));
        Ok(())
    })
    .unwrap();
    vm.with_native_stack(|stack| {
        assert_eq!(stack.top(), 2);
        assert_eq!(stack.value(1), Some(Value::Number(17.)));
        Ok(())
    })
    .unwrap();
}

#[test]
fn unprotected_root_errors_unwind_and_leave_a_usable_error_stack() {
    let mut vm = Vm::new();
    let error = vm
        .with_native_stack(|stack| {
            stack.push_integer(17)?;
            stack.load_buffer(b"error('root failure',0)", None)?;
            stack.call_unprotected(0, None)
        })
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Runtime);
    vm.with_native_stack(|stack| {
        assert_eq!(stack.top(), 1);
        assert_eq!(stack.to_bytes(1)?, Some(b"root failure".as_slice()));
        assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
        assert!(stack.get_stack(stack.thread_id(), 0)?.is_none());
        stack.set_top(0)?;
        stack.load_buffer(b"return 42", None)?;
        stack.call_unprotected(0, None)?;
        assert_eq!(stack.value(1), Some(Value::Number(42.)));
        Ok(())
    })
    .unwrap();
}

#[test]
fn root_can_resume_and_retain_a_coroutine_across_entries() {
    let mut vm = Vm::new();
    let child = vm
        .with_native_stack(|stack| {
            let child = stack.new_thread()?;
            stack.load_buffer(b"coroutine.yield(17);return 42", None)?;
            stack.move_thread_values(stack.thread_id(), child, 1)?;
            assert_eq!(stack.resume_thread(child, 0)?, NativeThreadStatus::Yielded);
            assert_eq!(
                stack.thread_stack(child)?.value(1),
                Some(Value::Number(17.))
            );
            Ok(child)
        })
        .unwrap();
    vm.collect_garbage().unwrap();
    vm.with_native_stack(|stack| {
        stack.thread_stack(child)?.set_top(0)?;
        assert_eq!(stack.resume_thread(child, 0)?, NativeThreadStatus::Ok);
        assert_eq!(
            stack.thread_stack(child)?.value(1),
            Some(Value::Number(42.))
        );
        assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
        Ok(())
    })
    .unwrap();
}

#[test]
fn raw_builtin_bodies_execute_without_a_current_closure() {
    let mut vm = Vm::new();
    let math = vm.get_global(b"math").unwrap();
    vm.with_native_stack(|stack| {
        stack.push(math)?;
        stack.get_field(1, b"abs")?;
        let body = stack.builtin_body(-1).unwrap();
        stack.set_top(0)?;
        stack.push_integer(-42)?;
        assert_eq!(stack.call_builtin_body(body)?, 1);
        assert_eq!(stack.value(-1), Some(Value::Number(42.)));
        assert!(stack.get_stack(stack.thread_id(), 0)?.is_none());
        assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
        Ok(())
    })
    .unwrap();
}

#[test]
fn root_entry_rejects_an_active_standalone_context() {
    let mut vm = Vm::new();
    vm.begin_standalone().unwrap();
    assert!(vm.with_native_stack(|_| Ok(())).is_err());
    vm.end_standalone().unwrap();
    vm.with_native_stack(|_| Ok(())).unwrap();
}

#[test]
fn root_instruction_limits_escape_even_when_rust_ignores_the_protected_error() {
    let mut vm = Vm::new_bare();
    vm.set_instruction_limit(Some(10));
    let error = vm
        .with_native_stack(|stack| {
            stack.load_buffer(b"while true do end", None)?;
            assert_eq!(
                stack.protected_call(0, None, None).unwrap_err().kind(),
                deadlib_lua::ErrorKind::Limit
            );
            assert!(stack.push(Value::Nil).is_err());
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    vm.set_instruction_limit(None);
    vm.with_native_stack(|stack| {
        assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
        stack.set_top(0)?;
        stack.load_buffer(b"return 42", None)?;
        stack.call_unprotected(0, Some(1))?;
        assert_eq!(stack.value(-1), Some(Value::Number(42.)));
        Ok(())
    })
    .unwrap();
}

#[test]
fn root_protected_calls_charge_the_same_c_depth_as_lua51() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    fn recurse(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), deadlib_lua::Error> {
        let count = call.function_data::<Arc<AtomicUsize>>().unwrap();
        count.fetch_add(1, Ordering::Relaxed);
        let function = call.arguments()[0];
        let mut stack = call.native_stack()?;
        stack.push(function)?;
        stack.push(function)?;
        stack.call_unprotected(1, Some(0))?;
        stack.finish(0)
    }
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            for status_api in [false, true] {
                let mut vm = Vm::new_bare();
                let count = Arc::new(AtomicUsize::new(0));
                let function = vm
                    .new_host_function_with_data(recurse, &[], count.clone())
                    .unwrap();
                vm.with_native_stack(|stack| {
                    stack.push(function)?;
                    stack.push(function)?;
                    if status_api {
                        assert_eq!(
                            stack.protected_call(1, Some(0), None)?,
                            NativeCallStatus::RuntimeError
                        );
                    } else {
                        assert_eq!(
                            stack.call(1, Some(0)).unwrap_err().message(),
                            "C stack overflow"
                        );
                    }
                    assert_eq!(count.load(Ordering::Relaxed), 199);
                    assert_eq!(stack.thread_call_level(stack.thread_id()), Some(0));
                    assert_eq!(stack.to_bytes(-1)?, Some(b"C stack overflow".as_slice()));
                    Ok(())
                })
                .unwrap();
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
