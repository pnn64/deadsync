use super::*;
use crate::{HostCall, Root};
use std::sync::{Arc, Mutex};

fn record(call: &mut HostCall<'_>) -> Result<(), Error> {
    let log = call.function_data::<Arc<Mutex<Vec<u8>>>>().unwrap().clone();
    let Value::Userdata(id) = call.arguments()[0] else {
        panic!("userdata argument")
    };
    log.lock()
        .unwrap()
        .push(call.userdata_bytes(id).unwrap()[0]);
    Ok(())
}

fn prepare(vm: &mut Vm, log: &Arc<Mutex<Vec<u8>>>, count: u8) -> Root {
    let meta = vm.try_new_table().unwrap();
    let root = vm.pin(Value::Table(meta)).unwrap();
    let function = vm
        .new_host_function_with_data(record, &[], log.clone())
        .unwrap();
    let key = vm.try_intern(b"__gc").unwrap();
    vm.raw_table_set(meta, key, function).unwrap();
    add_userdata(vm, meta, 0..count);
    root
}

fn add_userdata(vm: &mut Vm, meta: crate::TableId, values: std::ops::Range<u8>) {
    for i in values {
        let value = vm.try_new_userdata(1).unwrap();
        let Value::Userdata(id) = value else {
            unreachable!()
        };
        vm.userdata_bytes_mut(id).unwrap()[0] = i;
        vm.with_native_stack(|s| {
            s.set_top(0)?;
            s.push(value)?;
            s.push(Value::Table(meta))?;
            s.set_metatable(1)?;
            s.set_top(0)
        })
        .unwrap();
    }
}

#[test]
fn userdata_creation_reserves_owned_finalizer_storage_before_publication() {
    fn run(nth: usize, existing: usize) -> usize {
        let mut points = 0;
        with_vm(|vm, attempts, reject| {
            for _ in 0..existing {
                vm.try_new_userdata(1).unwrap();
            }
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = vm.try_new_userdata(7);
            points = attempts.get() - start;
            if nth == 0 {
                result.unwrap();
                assert_eq!(vm.heap_stats().userdata, existing + 1);
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.kind(), ErrorKind::Memory);
                assert_eq!(vm.heap_stats().userdata, existing);
                let Some(Value::String(id)) = error.value() else {
                    panic!("fixed memory string")
                };
                assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            }
            reject.set(0);
            let Value::Userdata(id) = vm.try_new_userdata(7).unwrap() else {
                unreachable!()
            };
            assert_eq!(vm.userdata_bytes(id), Some([0; 7].as_slice()));
        });
        points
    }
    let mut total = 0;
    for existing in [0, 4] {
        let points = run(0, existing);
        assert!(points >= 5);
        for nth in 1..=points {
            run(nth, existing);
        }
        total += points;
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn finalizer_discovery_uses_reserved_storage_and_preserves_creation_order_under_rejection() {
    let log = Arc::new(Mutex::new(Vec::with_capacity(19)));
    with_vm(|vm, attempts, reject| {
        vm.collect_garbage().unwrap();
        let root = prepare(vm, &log, 19);
        vm.with_native_stack(|s| {
            assert!(s.check_stack(128));
            Ok(())
        })
        .unwrap();
        let start = attempts.get();
        reject.set(start + 1);
        vm.collect_garbage().unwrap();
        assert_eq!(&*log.lock().unwrap(), &(0..19).rev().collect::<Vec<_>>());
        assert_eq!(
            attempts.get(),
            start,
            "no domain allocation during discovery/dispatch"
        );
        vm.collect_garbage().unwrap();
        assert_eq!(vm.heap_stats().userdata, 0);
        assert_eq!(log.lock().unwrap().len(), 19);
        reject.set(0);
        vm.unpin(root).unwrap();
    });
}

#[test]
fn failed_finalizer_dispatch_retains_pending_callbacks_and_idle_stack_for_retry() {
    let log = Arc::new(Mutex::new(Vec::with_capacity(17)));
    with_vm(|vm, attempts, reject| {
        vm.collect_garbage().unwrap();
        let root = prepare(vm, &log, 17);
        vm.with_native_stack(|s| {
            s.push(Value::Number(11.))?;
            s.push(Value::Number(22.))?;
            s.push(Value::Number(33.))
        })
        .unwrap();
        let start = attempts.get();
        reject.set(start + 1);
        assert_eq!(vm.collect_garbage().unwrap_err().kind(), ErrorKind::Memory);
        assert!(log.lock().unwrap().is_empty());
        vm.with_native_stack(|s| {
            assert_eq!(s.top(), 3);
            assert_eq!(s.value(1), Some(Value::Number(11.)));
            assert_eq!(s.value(3), Some(Value::Number(33.)));
            Ok(())
        })
        .unwrap();
        reject.set(0);
        vm.collect_garbage().unwrap();
        assert_eq!(&*log.lock().unwrap(), &(0..17).rev().collect::<Vec<_>>());
        vm.collect_garbage().unwrap();
        assert_eq!(log.lock().unwrap().len(), 17);
        vm.unpin(root).unwrap();
    });
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS 1");
}

#[test]
fn close_makes_progress_when_every_finalizer_dispatch_allocation_is_rejected() {
    let ledger = Ledger::default();
    let endpoint = Endpoint {
        ledger: &ledger,
        tag: 1,
    };
    // SAFETY: callback data outlives the VM, its destruction and the final owner.
    let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
    let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
    let log = Arc::new(Mutex::new(Vec::with_capacity(23)));
    prepare(&mut vm, &log, 23);
    vm.with_native_stack(|s| s.set_top(3)).unwrap();
    let start = ledger.attempts.get();
    ledger.reject_at.set(start + 1);
    drop(vm);
    assert_eq!(ledger.attempts.get() - start, 23);
    assert!(log.lock().unwrap().is_empty());
    assert_eq!(Arc::strong_count(&log), 1);
    assert_eq!(memory.usage().unwrap().blocks, 1);
    drop(memory);
    assert!(ledger.blocks.borrow().is_empty());
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS 23");
}

#[test]
fn pending_finalizers_keep_batch_order_when_consumed_queue_storage_is_reused() {
    fn finalizer(call: &mut HostCall<'_>) -> Result<(), Error> {
        let log = call
            .function_data::<Arc<Mutex<(Vec<u8>, bool)>>>()
            .unwrap()
            .clone();
        let Value::Userdata(id) = call.arguments()[0] else {
            unreachable!()
        };
        let mut log = log.lock().unwrap();
        log.0.push(call.userdata_bytes(id).unwrap()[0]);
        if log.1 { Err(Error::memory()) } else { Ok(()) }
    }
    let log = Arc::new(Mutex::new((Vec::<u8>::with_capacity(13), true)));
    with_vm(|vm, attempts, reject| {
        let meta = vm.try_new_table().unwrap();
        let root = vm.pin(Value::Table(meta)).unwrap();
        let function = vm
            .new_host_function_with_data(finalizer, &[], log.clone())
            .unwrap();
        let key = vm.try_intern(b"__gc").unwrap();
        vm.raw_table_set(meta, key, function).unwrap();
        add_userdata(vm, meta, 0..8);
        vm.with_native_stack(|s| {
            assert!(s.check_stack(128));
            Ok(())
        })
        .unwrap();
        for _ in 0..6 {
            assert_eq!(vm.collect_garbage().unwrap_err().kind(), ErrorKind::Memory);
        }
        assert_eq!(vm.heap_stats().userdata, 3);
        add_userdata(vm, meta, 10..15);
        log.lock().unwrap().1 = false;
        let start = attempts.get();
        reject.set(start + 1);
        vm.collect_garbage().unwrap();
        assert_eq!(
            log.lock().unwrap().0,
            [7, 6, 5, 4, 3, 2, 1, 0, 14, 13, 12, 11, 10]
        );
        assert_eq!(attempts.get(), start);
        vm.collect_garbage().unwrap();
        assert_eq!(vm.heap_stats().userdata, 0);
        reject.set(0);
        vm.unpin(root).unwrap();
    });
}
