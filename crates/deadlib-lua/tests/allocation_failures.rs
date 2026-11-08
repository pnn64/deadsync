//! Real allocator rejection, confined to the thread executing each probe.
use deadlib_lua::{Error, ErrorKind, NativeCallStatus, Value, Vm};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{Allocations, probe};
use std::cell::Cell;

fn memory(vm: &Vm, error: &Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    let Some(Value::String(id)) = error.value() else {
        panic!("missing fixed Lua diagnostic")
    };
    assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
    assert_eq!(error.line(), 0);
}
fn matrix(mut case: impl FnMut(usize) -> Allocations, minimum: usize) {
    let control = case(0);
    assert_eq!(control.rejected, 0);
    assert!(control.attempts >= minimum, "{control:?}");
    eprintln!("ALLOCATION_FAILURE_POINTS {}", control.attempts);
    for nth in 1..=control.attempts {
        let failed = case(nth);
        assert!(
            failed.rejected > 0,
            "failure {nth} was not reached: {failed:?}"
        );
    }
}
#[test]
fn payload_and_every_userdata_arena_buffer_fail_without_publishing_an_object() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let before = vm.heap_stats().userdata;
            let (result, stats) = probe(nth, || vm.try_new_userdata(64));
            if nth == 0 {
                assert!(result.is_ok());
            } else {
                let error = result.unwrap_err();
                memory(&vm, &error);
                assert_eq!(vm.heap_stats().userdata, before);
                vm.collect_garbage().unwrap();
                memory(&vm, &error);
            }
            let Value::Userdata(id) = vm.try_new_userdata(64).unwrap() else {
                unreachable!()
            };
            assert_eq!(vm.userdata_bytes(id), Some([0u8; 64].as_slice()));
            stats
        },
        5,
    );
}
#[test]
fn table_arena_growth_is_atomic_and_recovers_after_each_failed_buffer() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            vm.new_table();
            vm.new_table();
            let before = vm.heap_stats().tables;
            let (result, stats) = probe(nth, || vm.try_new_table());
            if nth == 0 {
                assert!(result.is_ok());
            } else {
                memory(&vm, &result.unwrap_err());
                assert_eq!(vm.heap_stats().tables, before);
            }
            let table = vm.try_new_table().unwrap();
            vm.raw_table_set(table, Value::Number(1.), Value::Number(42.))
                .unwrap();
            vm.with_native_stack(|s| {
                s.push(Value::Table(table))?;
                s.raw_get_integer(1, 1)?;
                assert_eq!(s.value(-1), Some(Value::Number(42.)));
                Ok(())
            })
            .unwrap();
            stats
        },
        3,
    );
}
#[test]
fn host_root_arena_failure_keeps_the_value_and_allows_a_retry() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let value = vm.try_new_userdata(64).unwrap();
            let (result, stats) = probe(nth, || vm.pin(value));
            if nth == 0 {
                vm.unpin(result.unwrap()).unwrap();
            } else {
                memory(&vm, &result.unwrap_err());
            }
            let root = vm.pin(value).unwrap();
            vm.collect_garbage().unwrap();
            assert_eq!(vm.rooted(root).unwrap(), value);
            vm.unpin(root).unwrap();
            stats
        },
        4,
    );
}
#[test]
fn main_native_stack_growth_reports_memory_failure_without_moving_the_top() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let mut measured = Allocations::default();
            vm.with_native_stack(|s| {
                let (result, stats) = probe(nth, || s.set_top(4096));
                measured = stats;
                if nth == 0 {
                    result?;
                    assert_eq!(s.top(), 4096);
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(s.top(), 0);
                    s.set_top(4096)?;
                }
                s.set_top(0)
            })
            .unwrap();
            measured
        },
        1,
    );
}
#[test]
fn selected_native_stack_growth_reports_memory_failure_without_moving_the_top() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let mut measured = Allocations::default();
            vm.with_native_stack(|s| {
                let child = s.new_thread()?;
                let mut target = s.thread_stack(child)?;
                let (result, stats) = probe(nth, || target.set_top(4096));
                measured = stats;
                if nth == 0 {
                    result?;
                    assert_eq!(target.top(), 4096);
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(target.top(), 0);
                    target.set_top(4096)?;
                }
                target.set_top(0)
            })
            .unwrap();
            measured
        },
        1,
    );
}
#[test]
fn native_table_preallocation_and_metadata_fail_without_pushing_a_partial_table() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let mut measured = Allocations::default();
            vm.new_table();
            vm.new_table();
            let before = vm.heap_stats().tables;
            vm.with_native_stack(|s| {
                let (result, stats) = probe(nth, || s.create_table(8, 8));
                measured = stats;
                if nth == 0 {
                    result?;
                    assert_eq!(s.top(), 1);
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(s.top(), 0);
                    s.create_table(8, 8)?;
                }
                Ok(())
            })
            .unwrap();
            assert_eq!(vm.heap_stats().tables, before + 1);
            measured
        },
        6,
    );
}
#[test]
fn selected_thread_userdata_reservations_fail_without_pushing_a_partial_value() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let mut measured = Allocations::default();
            vm.with_native_stack(|s| {
                let child = s.new_thread()?;
                let mut target = s.thread_stack(child)?;
                let (result, stats) = probe(nth, || target.new_userdata(64));
                measured = stats;
                if nth == 0 {
                    result?;
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(target.top(), 0);
                    target.new_userdata(64)?;
                }
                assert_eq!(target.top(), 1);
                assert!(matches!(target.value(1), Some(Value::Userdata(_))));
                Ok(())
            })
            .unwrap();
            assert_eq!(vm.heap_stats().userdata, 1);
            measured
        },
        6,
    );
}
#[test]
fn selected_thread_table_reservations_fail_without_pushing_a_partial_value() {
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            vm.new_table();
            vm.new_table();
            let before = vm.heap_stats().tables;
            let mut measured = Allocations::default();
            vm.with_native_stack(|s| {
                let child = s.new_thread()?;
                let mut target = s.thread_stack(child)?;
                let (result, stats) = probe(nth, || target.create_table(8, 8));
                measured = stats;
                if nth == 0 {
                    result?;
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(target.top(), 0);
                    target.create_table(8, 8)?;
                }
                assert_eq!(target.top(), 1);
                assert!(matches!(target.value(1), Some(Value::Table(_))));
                Ok(())
            })
            .unwrap();
            assert_eq!(vm.heap_stats().tables, before + 1);
            measured
        },
        7,
    );
}
#[test]
fn allocation_failure_in_a_native_callback_returns_status_four_and_skips_the_handler() {
    fn allocate(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
        let nth = *call.function_data::<usize>().unwrap();
        let mut s = call.native_stack()?;
        let (result, stats) = probe(nth, || s.new_userdata(64));
        LAST.with(|cell| cell.set(stats));
        result?;
        s.finish(1)
    }
    thread_local! { static LAST: Cell<Allocations> = const { Cell::new(Allocations {
        attempts:0,rejected:0,deny_from:0,enabled:false,
    }) }; }
    matrix(
        |nth| {
            let mut vm = Vm::new_bare();
            let f = vm.new_host_function_with_data(allocate, &[], nth).unwrap();
            vm.with_native_stack(|s| {
                s.push_integer(17)?;
                s.push(f)?;
                let status = s.protected_call(0, None, Some(1))?;
                if nth == 0 {
                    assert_eq!(status, NativeCallStatus::Ok);
                } else {
                    assert_eq!(status, NativeCallStatus::MemoryError);
                    assert_eq!(s.top(), 2);
                }
                Ok(())
            })
            .unwrap();
            LAST.with(Cell::get)
        },
        5,
    );
}
#[test]
fn check_stack_returns_false_on_allocation_failure_and_keeps_its_window() {
    let mut vm = Vm::new_bare();
    vm.with_native_stack(|s| {
        let (ok, stats) = probe(1, || s.check_stack(4096));
        assert!(!ok);
        assert_eq!(stats.rejected, 1);
        assert_eq!(s.top(), 0);
        assert!(s.check_stack(4096));
        let child = s.new_thread()?;
        let mut target = s.thread_stack(child)?;
        let (ok, stats) = probe(1, || target.check_stack(4096));
        assert!(!ok);
        assert_eq!(stats.rejected, 1);
        assert_eq!(target.top(), 0);
        assert!(target.check_stack(4096));
        eprintln!("ALLOCATION_CHECK_STACK_FAILURE_POINTS 2");
        Ok(())
    })
    .unwrap();
}
