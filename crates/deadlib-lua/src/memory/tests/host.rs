use super::*;
use crate::{HostCall, HostThreadState, NativeCallStatus};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

// Active and inactive native windows have distinct borrowing APIs.
macro_rules! target {
    ($stack:ident, $thread:ident, $method:ident($($arg:expr),* $(,)?)) => {
        if $thread == $stack.thread_id() {
            $stack.$method($($arg),*)
        } else {
            $stack.thread_stack($thread)?.$method($($arg),*)
        }
    };
}

struct Data {
    drops: Arc<AtomicUsize>,
    roots: [Value; 1],
}
impl Drop for Data {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
impl HostThreadState for Data {
    fn roots(&self) -> &[Value] {
        &self.roots
    }
}
fn return_data(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.function_data::<Data>().unwrap().roots[0];
    call.push(value)
}
fn fixed_memory(vm: &Vm, error: Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    let Some(Value::String(id)) = error.value() else {
        panic!("fixed error string")
    };
    assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
}

#[test]
fn host_binding_failures_drop_owned_data_once_without_publishing_a_closure() {
    fn run(nth: usize) -> usize {
        let mut points = 0;
        let drops = Arc::new(AtomicUsize::new(0));
        with_vm(|vm, attempts, reject| {
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.new_host_function_with_data(
                return_data,
                &[Value::Number(42.)],
                Data {
                    drops: drops.clone(),
                    roots: [Value::Number(42.)],
                },
            );
            points = attempts.get() - start;
            if nth != 0 {
                fixed_memory(vm, result.unwrap_err());
                assert_eq!(vm.heap_stats().native_closures, 0);
                assert_eq!(drops.load(Ordering::Relaxed), 1);
            } else {
                result.unwrap();
                assert_eq!(drops.load(Ordering::Relaxed), 0);
            }
            reject.set(0);
            let function = vm
                .new_host_function_with_data(
                    return_data,
                    &[],
                    Data {
                        drops: drops.clone(),
                        roots: [Value::Number(73.)],
                    },
                )
                .unwrap();
            assert_eq!(vm.call(function, &[]).unwrap(), &[Value::Number(73.)]);
            vm.collect_garbage().unwrap();
            assert_eq!(drops.load(Ordering::Relaxed), 2);
        });
        assert_eq!(drops.load(Ordering::Relaxed), 2);
        points
    }
    let points = run(0);
    assert!(points >= 6);
    for nth in 1..=points {
        run(nth);
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {points}");
}

#[test]
fn thread_state_failures_preserve_both_stacks_and_drop_resources_once() {
    fn run(nth: usize, selected: bool) -> usize {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let retained = vm.try_new_table().unwrap();
            let root = vm.pin(Value::Table(retained)).unwrap();
            vm.with_native_stack(|stack| {
                stack.push(Value::Number(17.))?;
                let thread = if selected {
                    stack.new_thread()?
                } else {
                    stack.thread_id()
                };
                target!(stack, thread, push(Value::Number(25.)))?;
                let top = target!(stack, thread, top());
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let result = stack.initialize_thread_state(
                    thread,
                    Data {
                        drops: drops.clone(),
                        roots: [Value::Table(retained)],
                    },
                );
                points = attempts.get() - start;
                if nth != 0 {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert!(stack.thread_state::<Data>(thread).is_none());
                    assert_eq!(drops.load(Ordering::Relaxed), 1);
                } else {
                    result?;
                }
                assert_eq!(target!(stack, thread, top()), top);
                assert_eq!(target!(stack, thread, value(-1)), Some(Value::Number(25.)));
                assert_eq!(stack.value(1), Some(Value::Number(17.)));
                reject.set(0);
                if nth != 0 {
                    stack.initialize_thread_state(
                        thread,
                        Data {
                            drops: drops.clone(),
                            roots: [Value::Table(retained)],
                        },
                    )?;
                }
                assert_eq!(
                    stack.thread_state::<Data>(thread).unwrap().roots,
                    [Value::Table(retained)]
                );
                Ok(())
            })
            .unwrap();
            vm.unpin(root).unwrap();
            vm.collect_garbage().unwrap();
            vm.raw_table_set(retained, Value::Number(1.), Value::Number(42.))
                .unwrap();
        });
        assert_eq!(drops.load(Ordering::Relaxed), if nth == 0 { 1 } else { 2 });
        points
    }
    let mut total = 0;
    for selected in [false, true] {
        let points = run(0, selected);
        assert_eq!(points, 2);
        for nth in 1..=points {
            run(nth, selected);
        }
        total += points;
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn native_protected_scope_failure_returns_memory_status_and_retains_stack_prefix() {
    fn run(nth: usize, selected: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let function = vm
                .new_host_function(return_capture, &[Value::Number(42.)])
                .unwrap();
            let root = vm.pin(function).unwrap();
            vm.with_native_stack(|stack| {
                stack.push(Value::Number(73.))?;
                let thread = if selected {
                    stack.new_thread()?
                } else {
                    stack.thread_id()
                };
                target!(stack, thread, push(Value::Number(17.)))?;
                target!(stack, thread, push(function))?;
                let top = target!(stack, thread, top());
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let status = stack.protected_call_thread(thread, 0, Some(1), None)?;
                points = attempts.get() - start;
                assert_eq!(target!(stack, thread, top()), top);
                assert_eq!(target!(stack, thread, value(-2)), Some(Value::Number(17.)));
                if nth != 0 {
                    assert_eq!(status, NativeCallStatus::MemoryError);
                    let expected = Some(b"not enough memory".as_slice());
                    if selected {
                        assert_eq!(stack.thread_stack(thread)?.to_bytes(-1)?, expected);
                    } else {
                        assert_eq!(stack.to_bytes(-1)?, expected);
                    }
                } else {
                    assert_eq!(status, NativeCallStatus::Ok);
                    assert_eq!(target!(stack, thread, value(-1)), Some(Value::Number(42.)));
                }
                reject.set(0);
                target!(stack, thread, push(function))?;
                assert_eq!(
                    stack.protected_call_thread(thread, 0, Some(1), None)?,
                    NativeCallStatus::Ok
                );
                assert_eq!(target!(stack, thread, value(-1)), Some(Value::Number(42.)));
                Ok(())
            })
            .unwrap();
            vm.unpin(root).unwrap();
        });
        points
    }
    let mut total = 0;
    for selected in [false, true] {
        let points = run(0, selected);
        assert!(points > 0);
        for nth in 1..=points {
            run(nth, selected);
        }
        total += points;
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn builtin_body_workspace_and_return_failures_unwind_and_allow_reuse() {
    fn run(nth: usize) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let body = crate::NativeBuiltinBody::from_implementation_code(32).unwrap();
            let result = vm.with_native_stack(|stack| {
                stack.push(Value::Number(-42.))?;
                let level = stack.thread_call_level(stack.thread_id());
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let result = stack.call_builtin_body(body);
                points = attempts.get() - start;
                if let Err(error) = &result {
                    assert_eq!(error.kind(), ErrorKind::Memory);
                    assert_eq!(stack.top(), 1);
                    assert_eq!(stack.value(1), Some(Value::Number(-42.)));
                } else {
                    assert_eq!(result.as_ref().unwrap(), &1);
                    assert_eq!(stack.value(-1), Some(Value::Number(42.)));
                }
                assert_eq!(stack.thread_call_level(stack.thread_id()), level);
                result.map(|_| ())
            });
            if nth != 0 {
                fixed_memory(vm, result.unwrap_err());
            } else {
                result.unwrap();
            }
            vm.with_native_stack(|stack| {
                reject.set(0);
                stack.set_top(0)?;
                stack.push(Value::Number(-42.))?;
                assert_eq!(stack.call_builtin_body(body)?, 1);
                assert_eq!(stack.value(-1), Some(Value::Number(42.)));
                Ok(())
            })
            .unwrap();
        });
        points
    }
    let total = run(0);
    assert!(total >= 2);
    for nth in 1..=total {
        run(nth);
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn native_metamethod_scope_failures_unwind_and_allow_a_fresh_entry() {
    fn run(nth: usize, concat: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let function = vm
                .new_host_function(return_capture, &[Value::Number(42.)])
                .unwrap();
            let root = vm.pin(function).unwrap();
            let table = vm.try_new_table().unwrap();
            let table_root = vm.pin(Value::Table(table)).unwrap();
            let meta = vm.try_new_table().unwrap();
            let key = vm
                .try_intern(if concat { b"__concat" } else { b"__index" })
                .unwrap();
            vm.raw_table_set(meta, key, function).unwrap();
            vm.try_intern(b"missing").unwrap();
            vm.with_native_stack(|stack| {
                stack.push(Value::Table(table))?;
                stack.push(Value::Table(meta))?;
                stack.set_metatable(1)?;
                if concat {
                    stack.push(Value::Table(table))?;
                }
                Ok(())
            })
            .unwrap();
            let result = vm.with_native_stack(|stack| {
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let result = if concat {
                    stack.concatenate(2)
                } else {
                    stack.get_field(1, b"missing")
                };
                points = attempts.get() - start;
                result?;
                assert_eq!(stack.value(-1), Some(Value::Number(42.)));
                Ok(())
            });
            if nth != 0 {
                fixed_memory(vm, result.unwrap_err());
            } else {
                result.unwrap();
            }
            reject.set(0);
            vm.with_native_stack(|stack| {
                stack.set_top(0)?;
                stack.push(Value::Table(table))?;
                if concat {
                    stack.push(Value::Table(table))?;
                    stack.concatenate(2)?;
                } else {
                    stack.get_field(1, b"missing")?;
                }
                assert_eq!(stack.value(-1), Some(Value::Number(42.)));
                Ok(())
            })
            .unwrap();
            vm.unpin(table_root).unwrap();
            vm.unpin(root).unwrap();
        });
        points
    }
    let mut total = 0;
    for concat in [false, true] {
        let points = run(0, concat);
        assert!(points > 0);
        for nth in 1..=points {
            run(nth, concat);
        }
        total += points;
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn nested_native_protection_catches_argument_boundary_and_result_padding_failures() {
    fn protected(call: &mut HostCall<'_>) -> Result<(), Error> {
        let progress = call.function_data::<Arc<AtomicUsize>>().unwrap().clone();
        let function = call.captures()[0];
        let mut stack = call.native_stack()?;
        stack.push(Value::Number(73.))?;
        stack.push(function)?;
        stack.push(Value::Number(17.))?;
        stack.push(Value::Number(25.))?;
        progress.store(1, Ordering::Relaxed);
        let status = stack
            .protected_call(2, Some(4096), None)
            .expect("pcall must catch its own allocation failures");
        assert_eq!(stack.value(1), Some(Value::Number(73.)));
        if status == NativeCallStatus::MemoryError {
            assert_eq!(stack.top(), 2);
            assert_eq!(stack.to_bytes(-1)?, Some(b"not enough memory".as_slice()));
        } else {
            assert_eq!(status, NativeCallStatus::Ok);
            assert_eq!(stack.top(), 4097);
            assert_eq!(stack.value(2), Some(Value::Number(42.)));
            assert_eq!(stack.value(-1), Some(Value::Nil));
        }
        progress.store(2 + status as usize, Ordering::Relaxed);
        stack.set(-1, Value::Number(status as u8 as f64))?;
        stack.finish(1)
    }
    fn run(nth: usize) -> (usize, usize) {
        let mut points = 0;
        let progress = Arc::new(AtomicUsize::new(0));
        // Four live call frames exhaust the initial frame buffer, making the
        // nested protection boundary itself an allocation point as well.
        let program = crate::compile(b"local host=...; local function f(n) if n==0 then local x=host(); return x end local x=f(n-1); return x end local x=f(1); return x").unwrap();
        with_vm(|vm, attempts, reject| {
            let function = vm
                .new_host_function(return_capture, &[Value::Number(42.)])
                .unwrap();
            let caller = vm
                .new_host_function_with_data(protected, &[function], progress.clone())
                .unwrap();
            let root = vm.pin(caller).unwrap();
            let entry = Value::Function(vm.try_load_program(&program).unwrap());
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            match vm.call(entry, &[caller]) {
                Ok(values) => assert_eq!(values, &[Value::Number(if nth == 0 { 0. } else { 4. })]),
                Err(error) => {
                    assert_ne!(nth, 0);
                    fixed_memory(vm, error);
                    assert_ne!(progress.load(Ordering::Relaxed), 1);
                }
            }
            points = attempts.get() - start;
            reject.set(0);
            let state = progress.load(Ordering::Relaxed);
            assert_eq!(vm.call(entry, &[caller]).unwrap(), &[Value::Number(0.)]);
            progress.store(state, Ordering::Relaxed);
            vm.unpin(root).unwrap();
        });
        (points, progress.load(Ordering::Relaxed))
    }
    let (points, _) = run(0);
    let mut caught = 0;
    for nth in 1..=points {
        if run(nth).1 == 6 {
            caught += 1;
        }
    }
    assert!(caught >= 3, "argument buffer, boundary and result padding");
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {points}");
}
