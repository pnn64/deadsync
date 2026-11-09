use super::*;
use crate::{NativeCallStatus, memory::tests::with_vm};

#[test]
fn pattern_locale_invalidation_retries_without_stale_entries_and_collection_reclaims_keys() {
    with_vm(|vm, attempts, reject| {
        let Value::String(pattern) = vm.try_intern(b"^a").unwrap() else {
            unreachable!()
        };
        let Value::String(source) = vm.try_intern(b"a ^a").unwrap() else {
            unreachable!()
        };
        let root = vm.pin(Value::String(pattern)).unwrap();
        let source_root = vm.pin(Value::String(source)).unwrap();
        for (anchor, range) in [(true, (0, 1)), (false, (2, 4))] {
            let found = vm
                .with_pattern(pattern, anchor, source, |p, s, w| p.find(s, 0, w))
                .unwrap()
                .unwrap();
            assert_eq!((found.start, found.end), range);
        }
        assert_eq!(vm.patterns.values().count(), 2);
        let storage = vm.patterns.storage_bytes();
        // Exercise revision invalidation without racing the process-global locale.
        vm.pattern_locale = crate::locale::revision().wrapping_sub(1);
        let start = attempts.get();
        reject.set(start + 1);
        let error = vm.prepare_pattern(pattern, true).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Memory);
        assert_eq!(error.value(), Some(Value::String(vm.memory_error)));
        assert_eq!(vm.patterns.values().count(), 0);
        assert_eq!(vm.patterns.storage_bytes(), storage);
        assert_eq!(attempts.get(), start + 1);
        reject.set(0);
        vm.prepare_pattern(pattern, true).unwrap();
        vm.prepare_pattern(pattern, false).unwrap();
        vm.collect_garbage().unwrap();
        assert_eq!(vm.patterns.values().count(), 2);
        vm.unpin(root).unwrap();
        vm.unpin(source_root).unwrap();
        vm.collect_garbage().unwrap();
        assert_eq!(vm.patterns.values().count(), 0);
        assert!(!vm.is_live_value(Value::String(pattern)));
    });
    eprintln!("PATTERN_REJECTION_POINTS 1");
}

#[test]
fn protected_pattern_failures_preserve_current_and_selected_prefixes_and_allow_retry() {
    fn run(nth: usize, selected: bool) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            vm.gc_control(GcOperation::Stop).unwrap();
            let source = vm.try_intern(b"aaaaaaaaab").unwrap();
            let pattern = vm
                .try_intern(b"[a]?[a]?[a]?[a]?[a]?[a]?[a]?[a]?[a]?b")
                .unwrap();
            let diagnostic = Value::String(vm.memory_error);
            vm.with_native_stack(|stack| {
                let thread = if selected {
                    stack.new_thread()?
                } else {
                    stack.thread_id()
                };
                if selected {
                    let mut target = stack.thread_stack(thread)?;
                    target.push(Value::Number(73.))?;
                    target.push(Value::Native(STR_FIND))?;
                    target.push(source)?;
                    target.push(pattern)?;
                } else {
                    stack.push(Value::Number(73.))?;
                    stack.push(Value::Native(STR_FIND))?;
                    stack.push(source)?;
                    stack.push(pattern)?;
                }
                let start = attempts.get();
                reject.set(if nth == 0 { 0 } else { start + nth });
                let status = stack
                    .protected_call_thread(thread, 2, Some(2), None)
                    .unwrap_or_else(|error| {
                        panic!("protected call selected={selected}, nth={nth}: {error:?}")
                    });
                points = attempts.get() - start;
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
                assert_eq!(values[0], Some(Value::Number(73.)));
                if nth == 0 {
                    assert_eq!(status, NativeCallStatus::Ok);
                    assert_eq!(top, 3);
                    assert_eq!(values[1], Some(Value::Number(1.)));
                    assert_eq!(values[2], Some(Value::Number(10.)));
                } else {
                    assert_eq!(status, NativeCallStatus::MemoryError);
                    assert_eq!(top, 2);
                    assert_eq!(values[1], Some(diagnostic));
                }
                reject.set(0);
                if selected {
                    let mut target = stack.thread_stack(thread)?;
                    target.set_top(1)?;
                    target.push(Value::Native(STR_FIND))?;
                    target.push(source)?;
                    target.push(pattern)?;
                } else {
                    stack.set_top(1)?;
                    stack.push(Value::Native(STR_FIND))?;
                    stack.push(source)?;
                    stack.push(pattern)?;
                }
                assert_eq!(
                    stack.protected_call_thread(thread, 2, Some(2), None)?,
                    NativeCallStatus::Ok
                );
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
                        Some(Value::Number(1.)),
                        Some(Value::Number(10.))
                    ]
                );
                Ok(())
            })
            .unwrap_or_else(|error| {
                panic!("stack inspection selected={selected}, nth={nth}: {error:?}")
            });
        });
        points
    }
    let mut total = 0;
    for selected in [false, true] {
        let points = run(0, selected);
        assert!(points >= 8, "points={points}");
        for nth in 1..=points {
            run(nth, selected);
        }
        total += points;
    }
    eprintln!("PATTERN_REJECTION_POINTS {total}");
}
