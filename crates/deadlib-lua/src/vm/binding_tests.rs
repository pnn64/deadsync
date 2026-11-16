use super::*;

#[test]
fn loadstring_binding_failures_return_nil_and_the_fixed_memory_value() {
    fn run(nth: usize) -> usize {
        let mut points = 0;
        crate::memory::tests::with_vm(|vm, attempts, rejection| {
            // Prepare the call boundary before rejecting the protected binding.
            vm.reserve_registers(128).unwrap();
            vm.reserve_frames(8).unwrap();
            let source = vm
                .try_intern(b"local alpha='new literal';return function()return alpha end")
                .unwrap();
            let fixed = Value::String(vm.memory_error);
            let start = attempts.get();
            rejection.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.call(Value::Native(LOAD_STRING), &[source]).unwrap();
            points = attempts.get() - start;
            if nth != 0 {
                assert_eq!(result, &[Value::Nil, fixed]);
                assert_eq!(vm.functions.live_len(), 0);
                assert_eq!(vm.programs.live_len(), 0);
            } else {
                assert!(matches!(result, [Value::Function(_)]));
            }
            rejection.set(0);
            let result = vm.call(Value::Native(LOAD_STRING), &[source]).unwrap();
            assert!(matches!(result, [Value::Function(_)]));
        });
        points
    }
    let total = run(0);
    assert!(total >= 12);
    for nth in 1..=total {
        run(nth);
    }
    eprintln!("BINDING_LOADER_REJECTION_POINTS {total}");
}

#[test]
fn native_load_binding_and_prototype_retention_failures_return_status_four() {
    fn run(nth: usize) -> usize {
        let mut points = 0;
        crate::memory::tests::with_vm(|vm, attempts, rejection| {
            vm.reserve_registers(128).unwrap();
            vm.reserve_frames(8).unwrap();
            let fixed = Value::String(vm.memory_error);
            vm.with_native_stack(|s| {
                s.push(Value::Number(73.))?;
                let start = attempts.get();
                rejection.set(if nth == 0 { 0 } else { start + nth });
                let status = s.load_buffer(
                    b"local alpha=42;return function()return alpha end",
                    Some(b"=binding"),
                )?;
                points = attempts.get() - start;
                if nth != 0 {
                    assert_eq!(status, NativeLoadStatus::MemoryError);
                    assert_eq!(s.value(1), Some(Value::Number(73.)));
                    assert_eq!(s.value(2), Some(fixed));
                    assert_eq!(s.top(), 2);
                } else {
                    assert_eq!(status, NativeLoadStatus::Ok);
                }
                rejection.set(0);
                s.set_top(1)?;
                assert_eq!(s.load_buffer(b"return 42", None)?, NativeLoadStatus::Ok);
                Ok(())
            })
            .unwrap();
        });
        points
    }
    let total = run(0);
    assert!(total >= 5);
    // Active parser roots still have a separate fallibility migration. These
    // final requests cover closure-arena publication and the owned prototype map.
    for nth in total - 4..=total {
        run(nth);
    }
    eprintln!("BINDING_LOADER_REJECTION_POINTS 5");
}
