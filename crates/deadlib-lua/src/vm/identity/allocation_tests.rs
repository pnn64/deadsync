use super::*;
use crate::memory::tests::with_vm;

#[test]
fn identity_reservations_fail_before_publication_and_reuse_slots_without_allocation() {
    fn run(nth: usize, filled: usize) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            let mut arena = Arena::identified_in(vm.functions.memory().clone());
            let keys: Vec<_> = (0..filled).map(|n| arena.try_insert(n).unwrap()).collect();
            let pointers: Vec<_> = keys
                .iter()
                .map(|&key| arena.pointer(key).unwrap())
                .collect();
            let storage = arena.storage_bytes();
            let charged = arena.allocated;
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = arena.try_insert(1234);
            points = attempts.get() - start;
            if nth != 0 {
                assert_eq!(result.as_ref().unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(arena.live_len(), filled);
                assert_eq!(arena.len(), filled);
                assert_eq!(arena.allocated - charged, arena.storage_bytes() - storage);
            }
            for (index, (&key, &pointer)) in keys.iter().zip(&pointers).enumerate() {
                assert_eq!(arena.get(key), Some(&index));
                assert_eq!(arena.pointer(key), Some(pointer));
            }
            reject.set(0);
            let key = match result {
                Ok(key) => key,
                Err(_) => arena.try_insert(1234).unwrap(),
            };
            let pointer = arena.pointer(key).unwrap();
            assert!(!pointers.contains(&pointer));
            let start = attempts.get();
            reject.set(start + 1);
            assert_eq!(arena.remove(key), Some(1234));
            assert_eq!(arena.pointer(key), None);
            let replacement = arena.try_insert(4321).unwrap();
            assert_ne!(key, replacement);
            assert_eq!(arena.pointer(replacement), Some(pointer));
            assert_eq!(arena.pointer(key), None);
            assert_eq!(attempts.get(), start);
            reject.set(0);
        });
        points
    }
    let mut total = 0;
    // Cold storage, a new token page, and growth of the page descriptor buffer.
    for filled in [0, 64, 256] {
        let points = run(0, filled);
        assert!(points >= 4, "filled={filled}, points={points}");
        for nth in 1..=points {
            run(nth, filled);
        }
        total += points;
    }
    eprintln!("IDENTITY_REJECTION_POINTS {total}");
}

#[test]
fn cold_object_queries_on_current_and_selected_stacks_allocate_nothing() {
    fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
        Ok(())
    }
    with_vm(|vm, attempts, reject| {
        let table = Value::Table(vm.try_new_table().unwrap());
        let table_root = vm.pin(table).unwrap();
        let program = crate::compile(b"return 17").unwrap();
        let function = Value::Function(vm.try_load_program(&program).unwrap());
        let function_root = vm.pin(function).unwrap();
        let native = vm.new_host_function(empty, &[]).unwrap();
        let native_root = vm.pin(native).unwrap();
        let thread = vm.allocate_thread().unwrap();
        let thread_root = vm.pin(Value::Thread(thread)).unwrap();
        let builtin_root = vm.pin(Value::Native(PRINT)).unwrap();
        let userdata = vm.try_new_userdata(16).unwrap();
        let values = [
            table,
            function,
            native,
            Value::Thread(thread),
            userdata,
            Value::Native(PRINT),
        ];
        let start = attempts.get();
        reject.set(start + 1);
        let (pointers, stats) = crate::allocation_probe::probe(1, || {
            values.map(|value| vm.object_pointer(value).unwrap())
        });
        assert_eq!(stats.attempts, 0);
        assert_eq!(attempts.get(), start);
        for i in 0..pointers.len() {
            assert!(!pointers[..i].contains(&pointers[i]));
        }
        reject.set(0);
        vm.with_native_stack(|stack| {
            let prefix = stack.top();
            for &value in &values {
                stack.push(value)?;
                stack.thread_stack(thread)?.push(value)?;
            }
            let top = stack.top();
            let selected_top = stack.thread_stack(thread)?.top();
            let start = attempts.get();
            reject.set(start + 1);
            let ((), stats) = crate::allocation_probe::probe(1, || {
                for (i, &pointer) in pointers.iter().enumerate() {
                    let index = i as isize + 1;
                    assert_eq!(stack.to_pointer(prefix as isize + index), Some(pointer));
                    assert_eq!(
                        stack.thread_stack(thread).unwrap().to_pointer(index),
                        Some(pointer)
                    );
                }
                assert_eq!(stack.top(), top);
                assert_eq!(stack.thread_stack(thread).unwrap().top(), selected_top);
            });
            assert_eq!(stats.attempts, 0);
            assert_eq!(attempts.get(), start);
            reject.set(0);
            Ok(())
        })
        .unwrap();
        for root in [
            table_root,
            function_root,
            native_root,
            thread_root,
            builtin_root,
        ] {
            vm.unpin(root).unwrap();
        }
        vm.collect_garbage().unwrap();
        for (&value, &pointer) in values.iter().zip(&pointers) {
            assert_eq!(vm.object_pointer(value), Some(pointer));
        }
    });
}

#[test]
fn builtin_environment_storage_needs_no_allocation_and_tracks_only_live_builtins() {
    with_vm(|vm, attempts, reject| {
        let builtin = Value::Native(PRINT);
        let root = vm.pin(builtin).unwrap();
        let env = vm.try_new_table().unwrap();
        let pointer = vm.object_pointer(builtin).unwrap();
        let start = attempts.get();
        reject.set(start + 1);
        let ((), stats) = crate::allocation_probe::probe(1, || {
            vm.set_native_environment(PRINT, env).unwrap();
            assert_eq!(vm.native_environment(PRINT), env);
            assert_eq!(vm.object_pointer(builtin), Some(pointer));
        });
        assert_eq!(stats.attempts, 0);
        assert_eq!(attempts.get(), start);
        reject.set(0);
        vm.collect_garbage().unwrap();
        assert!(vm.is_live_value(Value::Table(env)));
        assert_eq!(vm.native_environment(PRINT), env);
        assert_eq!(vm.object_pointer(builtin), Some(pointer));
        vm.unpin(root).unwrap();
        vm.collect_garbage().unwrap();
        assert!(!vm.is_live_value(Value::Table(env)));
        assert_eq!(vm.object_pointer(builtin), None);
        assert_eq!(vm.builtin_envs[PRINT as usize], None);
        assert_eq!(vm.native_environment(PRINT), vm.initial_globals);
    });
}
