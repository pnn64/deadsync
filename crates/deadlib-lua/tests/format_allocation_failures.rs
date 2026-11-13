//! Reject all allocations during protected formatting buffers and callbacks.
use deadlib_lua::{GcOperation, NativeCallStatus, Value, Vm, compile};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{Allocations, probe};

#[test]
fn formatting_rejection_preserves_prefixes_skips_handlers_and_allows_reuse() {
    for source in [
        br#"local text=string.format('%99s|%-99s|%.2s','abc',string.rep('x',150),'abcdef');return #text==253"#.as_slice(),
        br#"local input=string.char(0,13,10,34,92,255)..string.rep('z',600);local text=string.format('%q',input);return #text==615"#.as_slice(),
        br#"local text=string.format('%+#099.99d|%#99.99x|%#99.99o|%-99c|%99c|%.99f|%#.99g|% .99e|%+g',-2147483648,427,1,65,0,1.7976931348623157e308,1e-300,-0,0/0);return #text>700"#.as_slice(),
        br#"local text=string.format('%%/%s/%s/%s/%s',string.rep('a',700),12345.5,string.rep('b',900),string.rep('c',1100));return #text==2712"#.as_slice(),
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
            assert!(control.attempts >= 4);
            for nth in 1..=control.attempts {
                assert!(run(nth).rejected > 0, "selected={selected}, nth={nth}");
            }
            eprintln!("FORMAT_GLOBAL_REJECTION_POINTS {}", control.attempts);
        }
    }
}
