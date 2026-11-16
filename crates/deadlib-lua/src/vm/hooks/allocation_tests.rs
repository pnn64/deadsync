use super::*;
use crate::{NativeCallStatus, NativeHook, memory::tests::with_vm};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn memory(vm: &Vm, error: Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    assert_eq!(error.value(), Some(Value::String(vm.memory_error)));
}

fn count_hook(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.function_data::<Arc<AtomicUsize>>()
        .unwrap()
        .fetch_add(1, Ordering::Relaxed);
    Ok(())
}

#[test]
fn native_hook_preparation_rejects_each_owned_window_before_publication() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        count_hook(call)?;
        let mut stack = call.native_stack()?;
        assert_eq!(stack.top(), 129);
        stack.set_top(1)?;
        Ok(())
    }
    fn run(nth: usize, len: usize) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let entered = Arc::new(AtomicUsize::new(0));
            let Value::Native(id) = vm
                .new_host_function_with_data(callback, &[], entered.clone())
                .unwrap()
            else {
                unreachable!()
            };
            let mut registers = Buffer::new_in(vm.registers.memory().clone());
            registers.try_reserve_exact(len).unwrap();
            for i in 0..len {
                registers.push(Value::Number(i as f64));
            }
            vm.registers = registers;
            let mut frames = Buffer::new_in(vm.frames.memory().clone());
            frames.try_reserve_exact(1).unwrap();
            frames.push(Frame {
                function: FrameKind::Native(0),
                pc: 7,
                base: 0,
                stack_start: 0,
                result: 0,
                wanted: 0,
                barriers: 0,
                vararg_start: 0,
                vararg_count: 129,
                tail_calls: 0,
                c_level_bias: 0,
            });
            vm.frames = frames;
            vm.top = len;
            vm.active_native = Some(0);
            let before = vm.registers.to_vec();
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.host_hook(id, b"call", None, 0);
            points = attempts.get() - start;
            if nth != 0 {
                memory(vm, result.unwrap_err());
                assert_eq!(entered.load(Ordering::Relaxed), 0);
                assert_eq!(&*vm.registers, before);
            } else {
                result.unwrap();
                assert_eq!(entered.load(Ordering::Relaxed), 1);
                assert_eq!(&vm.registers[..len], before);
                assert!(vm.registers[len..].iter().all(|v| *v == Value::Nil));
            }
            assert_eq!(vm.frames.len(), 1);
            assert_eq!(vm.frames[0].pc, 7);
            assert_eq!(vm.active_native, Some(0));
            assert_eq!(vm.top, len);
            assert!(vm.hooks.allowed());
            assert!(vm.hooks.host_running.is_none());
            assert!(vm.hooks.host_owner.is_none());
            assert!(vm.hooks.hidden.is_empty());
            assert!(vm.hooks.popped.is_empty());
            reject.set(0);
            vm.host_hook(id, b"call", None, 0).unwrap();
            // A warmed hook can pop and restore both windows while every new
            // allocation is rejected. No callback-private allocation is used.
            let start = attempts.get();
            reject.set(start + 1);
            vm.host_hook(id, b"call", None, 0).unwrap();
            assert_eq!(attempts.get(), start);
            assert_eq!(&vm.registers[..len], before);
        });
        points
    }
    let mut total = 0;
    for len in [33, 385] {
        let points = run(0, len);
        assert_eq!(
            points, 3,
            "frame plus register/popped or hidden/popped growth"
        );
        for nth in 1..=points {
            run(nth, len);
        }
        total += points;
    }
    eprintln!("HOOK_REJECTION_POINTS {total}");
}

#[test]
fn protected_hook_failures_bypass_handlers_and_allow_current_and_selected_thread_reuse() {
    fn run(nth: usize, selected: bool, lua: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            vm.gc_control(GcOperation::Stop).unwrap();
            let entered = Arc::new(AtomicUsize::new(0));
            let handled = Arc::new(AtomicUsize::new(0));
            let hook = vm
                .new_host_function_with_data(count_hook, &[], entered.clone())
                .unwrap();
            let handler = vm
                .new_host_function_with_data(count_hook, &[], handled.clone())
                .unwrap();
            let program = crate::compile(
                b"local function f(...) return arg[1], arg.n end; return f(17,29,42)",
            )
            .unwrap();
            let function = Value::Function(vm.try_load_program(&program).unwrap());
            let roots = [hook, handler, function].map(|value| vm.pin(value).unwrap());
            let hook_table = lua.then(|| vm.hook_table().unwrap());
            let memory_error = Value::String(vm.memory_error);
            let mut selected_thread = vm.thread;
            vm.with_native_stack(|stack| {
                stack.set_top(0)?;
                let thread = if selected {
                    stack.new_thread()?
                } else {
                    stack.thread_id()
                };
                selected_thread = thread;
                stack.set_hook(
                    thread,
                    NativeHook::Host(hook),
                    i32::from(CALL | RETURN | LINE),
                    0,
                )?;
                if let Some(table) = hook_table {
                    stack.push(Value::Table(table))?;
                    stack.push(Value::LightUserdata(thread.0))?;
                    stack.push(hook)?;
                    stack.set_table(-3, true)?;
                    stack.set_top(if selected { 1 } else { 0 })?;
                    stack.set_hook(thread, NativeHook::Lua, i32::from(CALL | RETURN | LINE), 0)?;
                }
                macro_rules! target {
                    ($method:ident($($arg:expr),* $(,)?)) => {
                        if selected {
                            stack.thread_stack(thread)?.$method($($arg),*)
                        } else {
                            stack.$method($($arg),*)
                        }
                    };
                }
                target!(push(Value::Number(73.)))?;
                target!(push(handler))?;
                target!(push(function))?;
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let status = stack.protected_call_thread(thread, 0, Some(2), Some(2))?;
                points = attempts.get() - start;
                assert_eq!(handled.load(Ordering::Relaxed), 0);
                assert_eq!(target!(value(1)), Some(Value::Number(73.)));
                assert_eq!(target!(value(2)), Some(handler));
                if nth == 0 {
                    assert_eq!(status, NativeCallStatus::Ok);
                    assert!(entered.load(Ordering::Relaxed) > 0);
                    assert_eq!(target!(top()), 4);
                    assert_eq!(target!(value(-2)), Some(Value::Number(17.)));
                    assert_eq!(target!(value(-1)), Some(Value::Number(3.)));
                } else {
                    assert_eq!(status, NativeCallStatus::MemoryError);
                    assert_eq!(target!(top()), 3);
                    assert_eq!(target!(value(-1)), Some(memory_error));
                }
                reject.set(0);
                let before = entered.load(Ordering::Relaxed);
                target!(push(function))?;
                assert_eq!(
                    stack.protected_call_thread(thread, 0, Some(2), Some(2))?,
                    NativeCallStatus::Ok
                );
                assert!(entered.load(Ordering::Relaxed) > before);
                assert_eq!(handled.load(Ordering::Relaxed), 0);
                assert_eq!(target!(value(-2)), Some(Value::Number(17.)));
                assert_eq!(target!(value(-1)), Some(Value::Number(3.)));
                Ok(())
            })
            .unwrap();
            let hooks = if selected {
                &vm.threads[slot_index(selected_thread.0)].hooks
            } else {
                &vm.hooks
            };
            assert!(hooks.allowed());
            assert!(hooks.hidden.is_empty());
            assert!(hooks.popped.is_empty());
            for root in roots {
                vm.unpin(root).unwrap();
            }
        });
        points
    }
    let mut total = 0;
    for selected in [false, true] {
        for lua in [false, true] {
            let points = run(0, selected, lua);
            assert!(points > 0);
            for nth in 1..=points {
                run(nth, selected, lua);
            }
            total += points;
        }
    }
    eprintln!("HOOK_REJECTION_POINTS {total}");
}

#[test]
fn instruction_hook_reservation_keeps_the_pending_instruction_and_counter_on_failure() {
    with_vm(|vm, attempts, reject| {
        vm.gc_control(GcOperation::Stop).unwrap();
        let program = crate::compile(b"return 17").unwrap();
        let function = vm.try_load_program(&program).unwrap();
        vm.push_frame(function, 0, 0, 1).unwrap();
        let mut frames = Buffer::new_in(vm.frames.memory().clone());
        frames.try_reserve_exact(1).unwrap();
        frames.push(vm.frames[0]);
        vm.frames = frames;
        vm.hooks.mask = COUNT;
        vm.hooks.count = 1;
        vm.hooks.ticks = 1;
        let start = attempts.get();
        reject.set(start + 1);
        let error = vm.hook_instruction(0, 0, 1).unwrap_err();
        memory(vm, error);
        assert_eq!(attempts.get(), start + 1);
        assert_eq!(vm.frames.len(), 1);
        assert_eq!(vm.frames[0].pc, 0);
        assert_eq!(vm.hooks.ticks, 1);
        assert!(vm.hooks.last.is_none());
        reject.set(0);
        assert!(vm.hook_instruction(0, 0, 1).unwrap());
        assert_eq!(vm.frames.len(), 2);
        let result = vm.drive();
        assert_eq!(vm.finish_execution(result).unwrap(), &[Value::Number(17.)]);
    });
    eprintln!("HOOK_REJECTION_POINTS 1");
}

#[test]
fn tail_hook_reservation_preserves_both_activations_and_argument_copies_for_retry() {
    with_vm(|vm, attempts, reject| {
        vm.gc_control(GcOperation::Stop).unwrap();
        let program = crate::compile(b"return ...").unwrap();
        let function = vm.try_load_program(&program).unwrap();
        vm.push_frame(function, 0, 0, OPEN).unwrap();
        let call = vm.registers.len();
        vm.reserve_registers(call + 4).unwrap();
        vm.registers.extend_from_slice(&[
            Value::Function(function),
            Value::Number(17.),
            Value::Number(29.),
            Value::Number(42.),
        ]);
        vm.push_frame(function, call, 3, OPEN).unwrap();
        vm.hooks.tail = Some(1);
        let mut registers = Buffer::new_in(vm.registers.memory().clone());
        registers.try_reserve_exact(vm.registers.len()).unwrap();
        registers.extend_from_slice(&vm.registers);
        vm.registers = registers;
        let before = vm.registers.to_vec();
        let start = attempts.get();
        reject.set(start + 1);
        let error = vm.merge_hook_tail(1).unwrap_err();
        memory(vm, error);
        assert_eq!(attempts.get(), start + 1);
        assert_eq!(vm.frames.len(), 2);
        assert_eq!(vm.hooks.tail, Some(1));
        assert_eq!(&*vm.registers, before);
        reject.set(0);
        vm.merge_hook_tail(1).unwrap();
        assert_eq!(vm.frames.len(), 1);
        assert!(vm.hooks.tail.is_none());
        let result = vm.drive();
        assert_eq!(
            vm.finish_execution(result).unwrap(),
            &[Value::Number(17.), Value::Number(29.), Value::Number(42.)]
        );
    });
    eprintln!("HOOK_REJECTION_POINTS 1");
}

#[test]
fn debug_hook_storage_and_result_failures_do_not_replace_installed_controls() {
    fn run(nth: usize, set: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            vm.gc_control(GcOperation::Stop).unwrap();
            let entered = Arc::new(AtomicUsize::new(0));
            let hook = vm
                .new_host_function_with_data(count_hook, &[], entered)
                .unwrap();
            let Value::Native(id) = hook else {
                unreachable!()
            };
            let mask = vm.try_intern(b"crl").unwrap();
            let mut registers = Buffer::new_in(vm.registers.memory().clone());
            registers
                .try_reserve_exact(if set { 4 } else { 1 })
                .unwrap();
            if set {
                registers.extend_from_slice(&[
                    Value::Native(SET_HOOK),
                    hook,
                    mask,
                    Value::Number(7.),
                ]);
            } else {
                registers.push(Value::Nil);
            }
            vm.registers = registers;
            vm.hooks.callback = Callback::Host(id);
            vm.hooks.mask = RETURN;
            vm.hooks.count = 13;
            vm.hooks.ticks = 11;
            let before = vm.registers.to_vec();
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.hook_native(
                if set { SET_HOOK } else { GET_HOOK },
                0,
                if set { 3 } else { 0 },
            );
            points = attempts.get() - start;
            if nth != 0 {
                memory(vm, result.unwrap_err());
                assert_eq!(&*vm.registers, before);
                assert!(matches!(vm.hooks.callback, Callback::Host(old) if old == id));
                assert_eq!(
                    (vm.hooks.mask, vm.hooks.count, vm.hooks.ticks),
                    (RETURN, 13, 11)
                );
            } else {
                result.unwrap();
            }
            reject.set(0);
            if set {
                vm.hook_native(SET_HOOK, 0, 3).unwrap();
                assert!(matches!(vm.hooks.callback, Callback::Lua));
                assert_eq!(
                    (vm.hooks.mask, vm.hooks.count, vm.hooks.ticks),
                    (CALL | RETURN | LINE | COUNT, 7, 7)
                );
            }
            assert_eq!(vm.hook_native(GET_HOOK, 0, 0).unwrap(), 3);
            if set {
                assert_eq!(vm.registers[0], hook);
            } else {
                let Value::String(text) = vm.registers[0] else {
                    panic!("external hook marker")
                };
                assert_eq!(vm.string_bytes(text), Some(b"external hook".as_slice()));
            }
        });
        points
    }
    let mut total = 0;
    for set in [false, true] {
        let points = run(0, set);
        assert!(points > 0);
        for nth in 1..=points {
            run(nth, set);
        }
        total += points;
    }
    eprintln!("HOOK_REJECTION_POINTS {total}");
}
