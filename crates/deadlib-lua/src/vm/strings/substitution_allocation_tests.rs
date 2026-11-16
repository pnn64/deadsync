use super::*;
use crate::{NativeCallStatus, memory::tests::with_vm};

#[test]
fn substitution_controls_buffers_and_actions_recover_after_each_owned_failure() {
    let long = format!(
        "local text,count=string.gsub(string.rep('a',{}),'.','%0x');return #text=={} and count=={}",
        io::LINE_BUFFER + 3,
        2 * (io::LINE_BUFFER + 3),
        io::LINE_BUFFER + 3
    );
    let callbacks = format!(
        "local calls=0;debug.setmetatable(false,{{__concat=function()calls=calls+1;return 'C'end}});local text,count=string.gsub('abc','.',function()debug.setlocal(2,3,false);return string.rep('O',{})end);return #text>0 and count==3 and calls>0",
        io::LINE_BUFFER + 1
    );
    for source in [
        br#"local text,count=string.gsub('abc','(.)','%1%1');return text=='aabbcc' and count==3"#.as_slice(),
        br#"local function replace(depth)
                return string.gsub('a','a',function() if depth==0 then return 'z' end;return replace(depth-1) end)
            end
            local text,count=replace(5);return text=='z' and count==1"#.as_slice(),
        long.as_bytes(),callbacks.as_bytes(),
    ] {
        let program = crate::compile(source).unwrap();
        let mut total = 0;
        for selected in [false, true] {
            let run = |nth: usize| {
                let mut points = 0;
                with_vm(|vm, attempts, reject| {
                    vm.gc_control(GcOperation::Stop).unwrap();
                    vm.open_strings().unwrap();
                    let debug_table=vm.new_table();
                    for (name,id) in [(b"setlocal".as_slice(),debug::SET_LOCAL),(b"setmetatable".as_slice(),DEBUG_SET_META)] {
                        let key=vm.intern(name);
                        vm.raw_set(debug_table,key,Value::Native(id)).unwrap();
                    }
                    vm.set_global(b"debug",Value::Table(debug_table)).unwrap();
                    let function = Value::Function(vm.try_load_program(&program).unwrap());
                    let diagnostic = Value::String(vm.memory_error);
                    vm.with_native_stack(|stack| {
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
                        let start = attempts.get();
                        reject.set(if nth == 0 { 0 } else { start + nth });
                        let status = stack.protected_call_thread(thread, 0, Some(1), Some(2))?;
                        points = attempts.get() - start;
                        assert_eq!(
                            status,
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
                        reject.set(0);
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
                    assert!(vm.substitutions.is_empty());
                    assert!(vm.threads.iter().all(|thread| thread.substitutions.is_empty()));
                    assert!(vm.open_upvalues.is_empty());
                });
                points
            };
            let points = run(0);
            // Even the short literal case allocates pattern, output and control
            // storage. Sweep the measured requests, not a traceback-specific count.
            assert!(points >= 4, "points={points}");
            for nth in 1..=points {
                run(nth);
            }
            total += points;
        }
        eprintln!("SUBSTITUTION_REJECTION_POINTS {total}");
    }
}
