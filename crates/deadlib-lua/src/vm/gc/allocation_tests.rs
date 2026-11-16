use super::*;
use crate::memory::tests::with_vm;

#[test]
fn pattern_preparation_and_backtracking_charge_retained_storage_after_each_failure() {
    fn run(nth: usize) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let mut bytes = b"[a]?".repeat(64);
            bytes.push(b'b');
            let pattern = vm.try_intern(&bytes).unwrap();
            let mut bytes = vec![b'a'; 64];
            bytes.push(b'b');
            let source = vm.try_intern(&bytes).unwrap();
            vm.reserve_registers(16).unwrap();
            vm.resize_registers(16);
            vm.registers[0] = Value::Native(strings::STR_FIND);
            vm.registers[1] = source;
            vm.registers[2] = pattern;
            let owned_before = vm.functions.memory().usage().unwrap().bytes;
            let charged_before = vm.gc.control_allocated;
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.string_native(strings::STR_FIND, 0, 2);
            points = attempts.get() - start;
            if nth != 0 {
                memory(vm, result.unwrap_err());
            } else {
                assert_eq!(result.unwrap(), 2);
                assert_eq!(&vm.registers[..2], &[Value::Number(1.), Value::Number(65.)]);
            }
            let retained = vm.functions.memory().usage().unwrap().bytes - owned_before;
            assert_eq!(vm.gc.control_allocated - charged_before, retained);
            reject.set(0);
            vm.registers[0] = Value::Native(strings::STR_FIND);
            vm.registers[1] = source;
            vm.registers[2] = pattern;
            assert_eq!(vm.string_native(strings::STR_FIND, 0, 2).unwrap(), 2);
            assert_eq!(&vm.registers[..2], &[Value::Number(1.), Value::Number(65.)]);
            let retained = vm.functions.memory().usage().unwrap().bytes - owned_before;
            assert_eq!(vm.gc.control_allocated - charged_before, retained);
        });
        points
    }
    let points = run(0);
    assert!(points >= 12, "points={points}");
    for nth in 1..=points {
        run(nth);
    }
    eprintln!("PATTERN_REJECTION_POINTS {points}");
}

fn memory(vm: &Vm, error: Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    assert_eq!(error.value(), Some(Value::String(vm.memory_error)));
}

fn one(vm: &Vm, item: Trace) -> Work {
    let mut work = Work::new_in(vm.functions.memory().clone());
    work.try_reserve_exact(1).unwrap();
    work.push(item);
    work
}

#[test]
fn rejected_table_traces_preserve_the_work_item_marks_and_slice_cursor() {
    for slice in [false, true] {
        with_vm(|vm, attempts, reject| {
            let table = vm.new_table();
            let mut children = Vec::new();
            for index in 1..=129 {
                let child = Value::Table(vm.new_table());
                children.push(child);
                vm.raw_set(table, Value::Number(f64::from(index)), child)
                    .unwrap();
            }
            vm.gc.phase = Phase::Propagate;
            let item = if slice {
                vm.tables.mark(table.0);
                vm.tables[slot_index(table.0)].trace = TableTrace::Pending(false, false);
                Trace::TableSlice(table, 128)
            } else {
                Trace::Value(Value::Table(table))
            };
            let mut work = one(vm, item);
            let trace = vm.tables[slot_index(table.0)].trace;
            let start = attempts.get();
            reject.set(start + 1);
            let error = vm.trace_some(&mut work, 1).unwrap_err();
            memory(vm, error);
            assert_eq!(&work[..], &[item]);
            assert_eq!(vm.tables.marked(table.0), slice);
            assert_eq!(vm.tables[slot_index(table.0)].trace, trace);
            assert!(children.iter().all(|&child| !vm.marked_value(child)));
            assert_eq!(attempts.get(), start + 1);
            reject.set(0);
            vm.trace_graph(&mut work).unwrap();
            let first = if slice { 64 } else { 0 };
            assert!(
                children[first..]
                    .iter()
                    .all(|&child| vm.marked_value(child))
            );
        });
    }
    eprintln!("COLLECTOR_REJECTION_POINTS 2");
}

#[test]
fn rejected_thread_scratch_and_frontier_growth_leave_registers_and_marks_unchanged() {
    for nth in 1..=2 {
        with_vm(|vm, attempts, reject| {
            vm.reserve_registers(193).unwrap();
            vm.resize_registers(193);
            vm.registers.fill(Value::Number(17.));
            vm.top = 193;
            vm.gc.phase = Phase::Propagate;
            let item = Trace::Value(Value::Thread(vm.thread));
            let mut work = one(vm, item);
            let start = attempts.get();
            reject.set(start + nth);
            let error = vm.trace_some(&mut work, 1).unwrap_err();
            memory(vm, error);
            assert_eq!(&work[..], &[item]);
            assert!(!vm.threads.marked(vm.thread.0));
            assert!(
                vm.registers
                    .iter()
                    .all(|&value| value == Value::Number(17.))
            );
            assert_eq!(attempts.get(), start + nth);
            reject.set(0);
            vm.trace_graph(&mut work).unwrap();
            assert!(vm.threads.marked(vm.thread.0));
        });
    }
    eprintln!("COLLECTOR_REJECTION_POINTS 2");
}

#[test]
fn every_collection_reservation_is_owned_recoverable_and_resumable() {
    fn run(nth: usize, wide_stack: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let table = vm.new_table();
            let root = vm.pin(Value::Table(table)).unwrap();
            let meta = vm.new_table();
            let mut children = Vec::new();
            for index in 1..=if wide_stack { 73 } else { 301 } {
                let child = vm.new_table();
                vm.set_metatable(Value::Table(child), Some(meta)).unwrap();
                vm.raw_set(child, Value::Number(1.), Value::Number(f64::from(index)))
                    .unwrap();
                vm.raw_set(table, Value::Number(f64::from(index)), Value::Table(child))
                    .unwrap();
                children.push(child);
            }
            // Both fixtures include cold liveness storage, independently of the
            // frontier growth required by initial tracing and atomic refresh.
            vm.reserve_registers(1).unwrap();
            vm.resize_registers(1);
            vm.registers[0] = Value::Table(table);
            vm.top = 1;
            if wide_stack {
                vm.reserve_registers(513).unwrap();
                vm.resize_registers(513);
                vm.registers[512] = Value::Table(table);
                vm.top = 513;
                let thread = vm.allocate_thread().unwrap();
                vm.raw_set(table, Value::Number(-1.), Value::Thread(thread))
                    .unwrap();
                let state = &mut vm.threads[slot_index(thread.0)];
                state.registers.try_reserve_exact(2049).unwrap();
                state.registers.resize(2049, Value::Nil);
                state.registers[2048] = Value::Table(table);
                state.top = 2049;
                state.api_stack = 0..2049;
                // Visit the current stack before discovering the larger parked
                // stack, so both scratch/frontier growth boundaries are tested.
                vm.pin(Value::Thread(vm.thread)).unwrap();
            }
            let dead = vm.new_table();
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.collect_cycle();
            points = attempts.get() - start;
            if nth == 0 {
                result.unwrap();
            } else {
                memory(vm, result.unwrap_err());
                assert!(matches!(vm.gc.phase, Phase::Pause | Phase::Propagate));
                assert_eq!(vm.gc.collections, 0);
                assert_eq!(points, nth);
                // No sweeping has happened, including the dead object's slot.
                assert!(vm.tables.contains(dead.0));
            }
            assert_eq!(vm.rooted(root).unwrap(), Value::Table(table));
            reject.set(0);
            if nth != 0 {
                // Resume the failed phase rather than restarting full collection.
                assert!(vm.advance_gc_some(usize::MAX).unwrap());
            }
            vm.finish_gc_cycle();
            assert!(!vm.tables.contains(dead.0));
            for (index, child) in children.into_iter().enumerate() {
                assert_eq!(
                    vm.tables[slot_index(table.0)].get(Value::Number((index + 1) as f64)),
                    Value::Table(child)
                );
                assert_eq!(
                    vm.tables[slot_index(child.0)].get(Value::Number(1.)),
                    Value::Number((index + 1) as f64)
                );
                assert_eq!(vm.metatable(Value::Table(child)), Some(meta));
            }
        });
        points
    }
    let mut total = 0;
    for wide_stack in [false, true] {
        let points = run(0, wide_stack);
        assert!(points >= 4, "wide_stack={wide_stack}, points={points}");
        for nth in 1..=points {
            run(nth, wide_stack);
        }
        total += points;
    }
    eprintln!("COLLECTOR_REJECTION_POINTS {total}");
}

#[test]
fn closing_a_marked_cell_needs_no_allocation_and_retains_its_pending_forced_scan() {
    with_vm(|vm, attempts, reject| {
        let child = vm.new_table();
        let leaf = vm.new_table();
        vm.raw_set(child, Value::Number(1.), Value::Table(leaf))
            .unwrap();
        vm.reserve_registers(1).unwrap();
        vm.resize_registers(1);
        vm.registers[0] = Value::Table(child);
        let cell = vm.capture_register(0).unwrap() as usize;
        vm.gc.phase = Phase::Propagate;
        vm.upvalues.mark(vm.upvalues.key(cell).unwrap());
        let start = attempts.get();
        reject.set(start + 1);
        vm.close_upvalues(0);
        assert_eq!(attempts.get(), start);
        assert!(matches!(vm.upvalues[cell], Cell::Closed(Value::Table(id)) if id == child));
        assert!(vm.tables.marked(child.0));
        assert!(!vm.tables.marked(leaf.0));
        assert_eq!(vm.gc.closed.len(), 1);
        let error = vm.advance_gc_some(1).unwrap_err();
        memory(vm, error);
        assert_eq!(vm.gc.closed.len(), 1);
        assert!(vm.gc.work.is_empty());
        reject.set(0);
        assert!(vm.advance_gc_some(usize::MAX).unwrap());
        assert!(vm.tables.contains(leaf.0));
        assert!(vm.gc.closed.is_empty());
        vm.finish_gc_cycle();
    });
    eprintln!("COLLECTOR_REJECTION_POINTS 1");
}

#[test]
fn failed_mutation_reservations_publish_no_metatable_environment_or_cell_value() {
    fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
        Ok(())
    }
    for kind in 0..6 {
        with_vm(|vm, attempts, reject| {
            let owner = vm.new_table();
            let target = vm.new_table();
            let thread = vm.allocate_thread().unwrap();
            let cell = slot_index(vm.upvalues.insert(Cell::Closed(Value::Number(7.))));
            let old_env = vm.thread_environment(thread);
            let program = crate::compile(b"return nil").unwrap();
            let function = vm.try_load_program(&program).unwrap();
            let native = vm.new_host_function(empty, &[]).unwrap();
            let Value::Native(native_id) = native else {
                unreachable!()
            };
            vm.gc.phase = Phase::Propagate;
            vm.tables.mark(owner.0);
            vm.threads.mark(thread.0);
            vm.upvalues.mark(vm.upvalues.key(cell).unwrap());
            vm.functions.mark(function.0);
            vm.native_closures.mark(native_id);
            let start = attempts.get();
            reject.set(start + 1);
            let result = match kind {
                0 => vm.set_metatable(Value::Table(owner), Some(target)),
                1 => vm.set_metatable(Value::Number(1.), Some(target)),
                2 => vm.set_thread_environment(thread, target),
                3 => vm.set_cell(cell, Value::Table(target)),
                4 => vm.set_closure_environment(Value::Function(function), target),
                _ => vm.set_closure_environment(native, target),
            };
            memory(vm, result.unwrap_err());
            assert_eq!(attempts.get(), start + 1);
            assert_eq!(vm.metatable(Value::Table(owner)), None);
            assert_eq!(vm.metatable(Value::Number(1.)), None);
            assert_eq!(vm.thread_environment(thread), old_env);
            assert_eq!(vm.cell_value(cell), Value::Number(7.));
            assert_eq!(
                vm.closure_environment(Value::Function(function)),
                Some(old_env)
            );
            assert_eq!(vm.closure_environment(native), Some(old_env));
            assert!(!vm.tables.marked(target.0));
            assert!(vm.gc.work.is_empty());
            reject.set(0);
            match kind {
                0 => vm.set_metatable(Value::Table(owner), Some(target)),
                1 => vm.set_metatable(Value::Number(1.), Some(target)),
                2 => vm.set_thread_environment(thread, target),
                3 => vm.set_cell(cell, Value::Table(target)),
                4 => vm.set_closure_environment(Value::Function(function), target),
                _ => vm.set_closure_environment(native, target),
            }
            .unwrap();
            assert!(!vm.gc.work.is_empty());
        });
    }
    eprintln!("COLLECTOR_REJECTION_POINTS 6");
}

#[test]
fn failed_pending_finalizer_root_reservation_retains_every_callback_for_retry() {
    use std::sync::{Arc, Mutex};
    fn record(call: &mut HostCall<'_>) -> Result<(), Error> {
        let log = call.function_data::<Arc<Mutex<Vec<u8>>>>().unwrap().clone();
        let Value::Userdata(id) = call.arguments()[0] else {
            unreachable!()
        };
        log.lock()
            .unwrap()
            .push(call.userdata_bytes(id).unwrap()[0]);
        Ok(())
    }
    let log = Arc::new(Mutex::new(Vec::<u8>::with_capacity(65)));
    with_vm(|vm, attempts, reject| {
        let meta = vm.new_table();
        vm.pin(Value::Table(meta)).unwrap();
        let callback = vm
            .new_host_function_with_data(record, &[], log.clone())
            .unwrap();
        let gc = vm.intern(b"__gc");
        vm.raw_set(meta, gc, callback).unwrap();
        let mut userdata = Vec::new();
        for index in 0..65 {
            let id = vm.try_allocate_userdata(1, vm.globals).unwrap();
            vm.userdata_bytes_mut(id).unwrap()[0] = index;
            vm.set_metatable(Value::Userdata(id), Some(meta)).unwrap();
            userdata.push(id);
        }
        vm.begin_gc_cycle().unwrap();
        let mut work = std::mem::take(&mut vm.gc.work);
        vm.trace_graph(&mut work).unwrap();
        vm.gc.work = work;
        assert!(vm.gc.work.capacity() < userdata.len());
        let start = attempts.get();
        reject.set(start + 1);
        let error = vm.finish_gc_marking().unwrap_err();
        memory(vm, error);
        assert_eq!(attempts.get(), start + 1);
        assert_eq!(vm.gc.phase, Phase::Propagate);
        assert_eq!(vm.gc.finalizers.iter().len(), userdata.len());
        assert!(
            userdata
                .iter()
                .all(|id| vm.userdata[slot_index(id.0)].finalized)
        );
        assert!(log.lock().unwrap().is_empty());
        reject.set(0);
        vm.finish_gc_marking().unwrap();
        vm.sweep_gc_some(usize::MAX);
        assert!(userdata.iter().all(|id| vm.userdata.contains(id.0)));
        vm.finish_gc_cycle();
        vm.collect_garbage().unwrap();
        assert_eq!(
            log.lock().unwrap().as_slice(),
            (0..65).rev().collect::<Vec<_>>()
        );
        vm.collect_garbage().unwrap();
        assert_eq!(vm.heap_stats().userdata, 0);
        assert_eq!(log.lock().unwrap().len(), 65);
    });
    eprintln!("COLLECTOR_REJECTION_POINTS 1");
}

#[test]
fn protected_collection_failures_bypass_handlers_and_preserve_current_and_selected_stacks() {
    use crate::NativeCallStatus;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    fn collect(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.function_data::<Arc<AtomicUsize>>()
            .unwrap()
            .fetch_add(1, Ordering::Relaxed);
        call.native_stack()?.gc(GcOperation::Collect)?;
        Ok(())
    }
    fn handler(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.function_data::<Arc<AtomicUsize>>()
            .unwrap()
            .fetch_add(1, Ordering::Relaxed);
        call.push(Value::Number(99.))
    }
    fn run(nth: usize, selected: bool) -> (usize, bool) {
        let entered = Arc::new(AtomicUsize::new(0));
        let handled = Arc::new(AtomicUsize::new(0));
        let mut points = 0;
        let mut failed_in_collection = false;
        with_vm(|vm, attempts, reject| {
            // Isolate explicit collection: the protected driver must enter the
            // callback before cold tracing storage is requested.
            vm.gc.stopped = true;
            let function = vm
                .new_host_function_with_data(collect, &[], entered.clone())
                .unwrap();
            let handler = vm
                .new_host_function_with_data(handler, &[], handled.clone())
                .unwrap();
            let root = vm.pin(function).unwrap();
            let handler_root = vm.pin(handler).unwrap();
            vm.with_native_stack(|stack| {
                let thread = if selected {
                    stack.new_thread()?
                } else {
                    stack.thread_id()
                };
                if selected {
                    let mut target = stack.thread_stack(thread)?;
                    target.push(Value::Number(73.))?;
                    target.push(handler)?;
                    target.push(function)?;
                } else {
                    stack.push(Value::Number(73.))?;
                    stack.push(handler)?;
                    stack.push(function)?;
                }
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let status = stack.protected_call_thread(thread, 0, Some(1), Some(2))?;
                points = attempts.get() - start;
                failed_in_collection = nth != 0 && entered.load(Ordering::Relaxed) != 0;
                let expected = if nth == 0 {
                    NativeCallStatus::Ok
                } else {
                    NativeCallStatus::MemoryError
                };
                assert_eq!(status, expected);
                assert_eq!(handled.load(Ordering::Relaxed), 0);
                if selected {
                    let mut target = stack.thread_stack(thread)?;
                    assert_eq!(target.top(), 3);
                    assert_eq!(target.value(1), Some(Value::Number(73.)));
                    assert_eq!(target.value(2), Some(handler));
                    if nth != 0 {
                        assert_eq!(target.to_bytes(-1)?, Some(b"not enough memory".as_slice()));
                    }
                } else {
                    assert_eq!(stack.top(), 3);
                    assert_eq!(stack.value(1), Some(Value::Number(73.)));
                    assert_eq!(stack.value(2), Some(handler));
                    if nth != 0 {
                        assert_eq!(stack.to_bytes(-1)?, Some(b"not enough memory".as_slice()));
                    }
                }
                reject.set(0);
                if selected {
                    stack.thread_stack(thread)?.push(function)?;
                } else {
                    stack.push(function)?;
                }
                assert_eq!(
                    stack.protected_call_thread(thread, 0, Some(1), Some(2))?,
                    NativeCallStatus::Ok
                );
                assert_eq!(handled.load(Ordering::Relaxed), 0);
                Ok(())
            })
            .unwrap();
            vm.unpin(root).unwrap();
            vm.unpin(handler_root).unwrap();
        });
        (points, failed_in_collection)
    }
    let mut total = 0;
    for selected in [false, true] {
        let (points, _) = run(0, selected);
        let mut collection_failures = 0;
        for nth in 1..=points {
            collection_failures += usize::from(run(nth, selected).1);
        }
        assert!(
            collection_failures >= 2,
            "cold frontier and liveness allocations must be rejected"
        );
        total += points;
    }
    eprintln!("COLLECTOR_REJECTION_POINTS {total}");
}

#[test]
fn builtin_environment_barrier_failure_preserves_the_old_environment_and_identity() {
    with_vm(|vm, attempts, reject| {
        let builtin = Value::Native(PRINT);
        let root = vm.pin(builtin).unwrap();
        let old = vm.try_new_table().unwrap();
        let new = vm.try_new_table().unwrap();
        vm.set_native_environment(PRINT, old).unwrap();
        let pointer = vm.object_pointer(builtin).unwrap();
        vm.gc.phase = Phase::Propagate;
        vm.gc.natives[PRINT as usize / 64] |= 1 << (PRINT % 64);
        let start = attempts.get();
        reject.set(start + 1);
        let error = vm.set_native_environment(PRINT, new).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Memory);
        assert_eq!(error.value(), Some(Value::String(vm.memory_error)));
        assert_eq!(vm.native_environment(PRINT), old);
        assert_eq!(vm.object_pointer(builtin), Some(pointer));
        assert!(!vm.tables.marked(new.0));
        assert_eq!(attempts.get(), start + 1);
        reject.set(0);
        vm.set_native_environment(PRINT, new).unwrap();
        assert_eq!(vm.native_environment(PRINT), new);
        assert!(vm.tables.marked(new.0));
        vm.collect_garbage().unwrap();
        assert!(vm.is_live_value(Value::Table(new)));
        assert_eq!(vm.object_pointer(builtin), Some(pointer));
        vm.unpin(root).unwrap();
    });
    eprintln!("COLLECTOR_REJECTION_POINTS 1");
}

#[test]
fn delivering_a_protected_memory_error_defers_automatic_collection_until_allocation() {
    with_vm(|vm, attempts, reject| {
        vm.reserve_registers(128).unwrap();
        vm.reserve_frames(8).unwrap();
        vm.gc.threshold = 0;
        let error = vm.memory_failure();
        let fixed = Value::String(vm.memory_error);
        vm.with_native_stack(|stack| {
            stack.push(Value::Number(73.))?;
            let start = attempts.get();
            reject.set(start + 1);
            let status = stack.load_reader(Some(b"=memory return"), |_| Err(error.clone()))?;
            assert_eq!(status, NativeLoadStatus::MemoryError);
            assert_eq!(stack.top(), 2);
            assert_eq!(stack.value(1), Some(Value::Number(73.)));
            assert_eq!(stack.value(2), Some(fixed));
            assert_eq!(attempts.get(), start);
            reject.set(0);
            stack.set_top(1)?;
            assert_eq!(stack.load_buffer(b"return 42", None)?, NativeLoadStatus::Ok);
            assert_eq!(stack.call(0, Some(1))?, 1);
            assert_eq!(stack.value(1), Some(Value::Number(73.)));
            assert_eq!(stack.value(2), Some(Value::Number(42.)));
            Ok(())
        })
        .unwrap();
        vm.collect_garbage().unwrap();
    });
}
