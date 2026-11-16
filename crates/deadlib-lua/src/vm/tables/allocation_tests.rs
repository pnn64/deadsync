use super::*;
use crate::{NativeCallStatus, memory::tests::with_vm};

#[test]
fn table_insert_reports_each_owned_storage_failure_and_can_retry() {
    fn run(nth: usize, position: i32) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let table = vm.try_new_table().unwrap();
            vm.reserve_registers(16).unwrap();
            vm.resize_registers(16);
            vm.registers[..4].copy_from_slice(&[
                Value::Native(TAB_INSERT),
                Value::Table(table),
                Value::Number(f64::from(position)),
                Value::Number(42.),
            ]);
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.table_native(TAB_INSERT, 0, 3);
            points = attempts.get() - start;
            if nth != 0 {
                let error = result.unwrap_err();
                assert_eq!(error.kind(), ErrorKind::Memory);
                assert_eq!(error.value(), Some(Value::String(vm.memory_error)));
                assert_eq!(vm.raw_index(table, i64::from(position)), Value::Nil);
            } else {
                assert_eq!(result.unwrap(), 0);
            }
            reject.set(0);
            assert_eq!(vm.table_native(TAB_INSERT, 0, 3).unwrap(), 0);
            assert_eq!(vm.raw_index(table, i64::from(position)), Value::Number(42.));
        });
        points
    }
    let mut total = 0;
    for position in [0, 1] {
        let points = run(0, position);
        assert!(points > 0);
        for nth in 1..=points {
            run(nth, position);
        }
        total += points;
    }
    eprintln!("LIBRARY_REJECTION_POINTS {total}");
}

#[test]
fn nested_sorts_recover_after_owned_control_growth_and_skip_invalid_memory_handlers() {
    let program = crate::compile(
        br#"
        local function ordered(depth)
            local t = {2, 1}
            table.sort(t, function(a, b)
                if depth > 0 and not ordered(depth - 1) then return false end
                return a < b
            end)
            return t[1] == 1 and t[2] == 2
        end
        return ordered(5)
    "#,
    )
    .unwrap();
    fn run(program: &Program, nth: usize, selected: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            vm.gc_control(GcOperation::Stop).unwrap();
            vm.open_tables().unwrap();
            let function = Value::Function(vm.try_load_program(program).unwrap());
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
            assert!(vm.sorts.is_empty());
            assert!(vm.open_upvalues.is_empty());
            assert!(vm.threads.iter().all(|state| state.sorts.is_empty()));
        });
        points
    }
    let mut total = 0;
    for selected in [false, true] {
        let points = run(&program, 0, selected);
        assert!(points >= 20, "points={points}");
        for nth in 1..=points {
            run(&program, nth, selected);
        }
        total += points;
    }
    eprintln!("LIBRARY_REJECTION_POINTS {total}");
}
