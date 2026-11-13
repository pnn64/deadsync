//! Reject all allocations during protected traceback formatting and callbacks.
use deadlib_lua::{GcOperation, NativeCallStatus, Value, Vm, compile};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{Allocations, probe};

#[test]
fn traceback_rejection_preserves_prefixes_skips_handlers_and_allows_reuse() {
    for source in [
        br#"local function nested() return debug.traceback(42,0) end
            return type(nested()) == 'string'"#
            .as_slice(),
        br#"local depth = 0
            local tag
            tag = setmetatable({}, {__concat = function()
                depth = depth + 1
                if depth < 6 then return debug.traceback('',2147483647,tag,false) end
                return 'done'
            end})
            local text = debug.traceback('',2147483647,tag,false)
            return type(text) == 'string' and depth == 6"#
            .as_slice(),
    ] {
        let program = compile(source).unwrap();
        for selected in [false, true] {
            let run = |nth| {
                let mut vm = Vm::new();
                let function = Value::Function(vm.try_load_program(&program).unwrap());
                let diagnostic = vm.intern(b"not enough memory");
                let mut measured = Allocations::default();
                vm.with_native_stack(|stack| {
                    stack.gc(GcOperation::Stop)?;
                    stack.set_top(0)?;
                    let thread = if selected {
                        stack.new_thread()?
                    } else {
                        stack.thread_id()
                    };
                    if selected {
                        let mut target = stack.thread_stack(thread)?;
                        target.push(Value::Number(73.))?;
                        target.push(Value::Number(99.))?;
                        target.push(function)?;
                    } else {
                        stack.push(Value::Number(73.))?;
                        stack.push(Value::Number(99.))?;
                        stack.push(function)?;
                    }
                    let (status, stats) = probe(nth, || {
                        stack.protected_call_thread(thread, 0, Some(1), Some(2))
                    });
                    measured = stats;
                    assert_eq!(
                        status?,
                        if nth == 0 {
                            NativeCallStatus::Ok
                        } else {
                            NativeCallStatus::MemoryError
                        }
                    );
                    let expected = if nth == 0 {
                        Value::Boolean(true)
                    } else {
                        diagnostic
                    };
                    let (top, values) = if selected {
                        let target = stack.thread_stack(thread)?;
                        (
                            target.top(),
                            [target.value(1), target.value(2), target.value(3)],
                        )
                    } else {
                        (
                            stack.top(),
                            [stack.value(1), stack.value(2), stack.value(3)],
                        )
                    };
                    assert_eq!(top, 3);
                    assert_eq!(
                        values,
                        [
                            Some(Value::Number(73.)),
                            Some(Value::Number(99.)),
                            Some(expected)
                        ]
                    );
                    if selected {
                        let mut target = stack.thread_stack(thread)?;
                        target.set_top(2)?;
                        target.push(function)?;
                    } else {
                        stack.set_top(2)?;
                        stack.push(function)?;
                    }
                    assert_eq!(
                        stack.protected_call_thread(thread, 0, Some(1), Some(2))?,
                        NativeCallStatus::Ok
                    );
                    let value = if selected {
                        stack.thread_stack(thread)?.value(3)
                    } else {
                        stack.value(3)
                    };
                    assert_eq!(value, Some(Value::Boolean(true)));
                    Ok(())
                })
                .unwrap_or_else(|error| panic!("selected={selected}, nth={nth}: {error:?}"));
                measured
            };
            let control = run(0);
            assert_eq!(control.rejected, 0);
            assert!(control.attempts > 20);
            for nth in 1..=control.attempts {
                assert!(run(nth).rejected > 0, "selected={selected}, nth={nth}");
            }
            eprintln!("TRACEBACK_GLOBAL_REJECTION_POINTS {}", control.attempts);
        }
    }
}
