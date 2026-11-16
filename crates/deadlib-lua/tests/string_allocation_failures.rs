//! Persistent rejection of real string storage allocations and recovery paths.
use deadlib_lua::{Error, ErrorKind, NativeCallStatus, Value, Vm};
use std::cell::RefCell;
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{Allocations, probe};

fn matrix(mut case: impl FnMut(usize) -> Allocations) {
    let control = case(0);
    assert_eq!(control.rejected, 0);
    assert!(control.attempts > 0);
    eprintln!("ALLOCATION_FAILURE_POINTS {}", control.attempts);
    for nth in 1..=control.attempts {
        assert!(case(nth).rejected > 0, "unreached allocation {nth}");
    }
}
fn memory(vm: &Vm, error: &Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    assert_eq!(error.line(), 0);
    let Some(Value::String(id)) = error.value() else {
        panic!("missing diagnostic")
    };
    assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
}

#[test]
fn intern_failures_preserve_hash_entries_identities_and_allow_retry() {
    // Cross hash rebuilds, arena growth and mark-buffer growth.
    for count in [0, 3, 12, 27, 44, 75, 108, 236] {
        matrix(|nth| {
            let mut vm = Vm::new_bare();
            let mut roots = Vec::new();
            for i in 0..count {
                let value = vm.intern(format!("old-string-{i}").as_bytes());
                roots.push(vm.pin(value).unwrap());
            }
            let before = vm.heap_stats().strings;
            let (result, stats) = probe(nth, || vm.try_intern(b"new\0binary\xffstring"));
            if nth == 0 {
                assert!(result.is_ok());
            } else {
                let error = result.unwrap_err();
                memory(&vm, &error);
                assert_eq!(vm.heap_stats().strings, before);
                vm.collect_garbage().unwrap();
                memory(&vm, &error);
            }
            for (i, root) in roots.iter().enumerate() {
                assert_eq!(
                    vm.rooted(*root).unwrap(),
                    vm.intern(format!("old-string-{i}").as_bytes())
                );
            }
            let value = vm.try_intern(b"new\0binary\xffstring").unwrap();
            let (hit, allocations) = probe(1, || vm.try_intern(b"new\0binary\xffstring"));
            assert_eq!(hit.unwrap(), value);
            assert_eq!(allocations.attempts, 0);
            stats
        });
    }
}

#[test]
fn reclaimed_string_slots_remain_reusable_after_failed_insertion() {
    matrix(|nth| {
        let mut vm = Vm::new_bare();
        for i in 0..300 {
            vm.intern(format!("discarded-{i}").as_bytes());
        }
        vm.collect_garbage().unwrap();
        let before = vm.heap_stats().strings;
        let (result, stats) = probe(nth, || vm.try_intern(b"reused-string-slot"));
        if nth != 0 {
            memory(&vm, &result.unwrap_err());
            assert_eq!(vm.heap_stats().strings, before);
        } else {
            result.unwrap();
        }
        let value = vm.try_intern(b"reused-string-slot").unwrap();
        let root = vm.pin(value).unwrap();
        vm.collect_garbage().unwrap();
        assert_eq!(vm.rooted(root).unwrap(), value);
        stats
    });
}

#[test]
fn native_pushes_and_numeric_coercions_preserve_inputs_on_failure() {
    for selected in [false, true] {
        for number in [false, true] {
            matrix(|nth| {
                let mut vm = Vm::new_bare();
                let mut stats = Allocations::default();
                vm.with_native_stack(|s| {
                    if selected {
                        let thread = s.new_thread()?;
                        let mut s = s.thread_stack(thread)?;
                        s.push(Value::Number(1234.25))?;
                        let (result, measured) = probe(nth, || {
                            if number {
                                s.to_bytes(1).map(|_| ())
                            } else {
                                s.push_bytes(b"new\0string")
                            }
                        });
                        stats = measured;
                        if nth != 0 {
                            assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                            assert_eq!(s.top(), 1);
                            assert_eq!(s.value(1), Some(Value::Number(1234.25)));
                        } else {
                            result?;
                        }
                        s.push_bytes(b"new\0string")?;
                        assert_eq!(s.to_c_bytes(-1)?, Some(b"new\0string\0".as_slice()));
                    } else {
                        s.push(Value::Number(1234.25))?;
                        let (result, measured) = probe(nth, || {
                            if number {
                                s.to_bytes(1).map(|_| ())
                            } else {
                                s.push_bytes(b"new\0string")
                            }
                        });
                        stats = measured;
                        if nth != 0 {
                            assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                            assert_eq!(s.top(), 1);
                            assert_eq!(s.value(1), Some(Value::Number(1234.25)));
                        } else {
                            result?;
                        }
                        s.push_bytes(b"new\0string")?;
                        assert_eq!(s.to_c_bytes(-1)?, Some(b"new\0string\0".as_slice()));
                    }
                    Ok(())
                })
                .unwrap();
                stats
            });
        }
    }
}

thread_local! {
    static DENIAL: RefCell<Option<allocation_probe::Disarm>> = const { RefCell::new(None) };
}
fn arm(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
    let nth = *call.function_data::<usize>().unwrap();
    DENIAL.with(|slot| *slot.borrow_mut() = Some(allocation_probe::begin(nth)));
    Ok(())
}
fn disarm(_: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
    DENIAL.with(|slot| {
        slot.borrow_mut().take();
    });
    Ok(())
}

#[test]
fn lua_protection_survives_string_failures_while_allocator_keeps_rejecting() {
    for expression in [
        "string.sub(s,2,-2)",
        "string.rep(s,2)",
        "string.reverse(s)",
        "string.upper(s)",
        "string.lower(s)",
        "string.char(97,98,99,100)",
        "s..s",
        "s..1234.25",
        "string.sub(1234.25,2,-2)",
        "string.byte(string.rep(s,20),1,-1)",
        "string.find(s,'([A-Za-z]+)%-([A-Za-z]+)')",
        "string.match(s,'F?F?a?u?l?t?%-S?t?r?i?n?g?%-P?a?y?l?o?a?d?')",
        "string.match(s,'F.-l.*')",
        "string.match(s,'%f[%a](%a+)')",
        "string.match(s,'(%b--)')",
    ] {
        for extended in [false, true] {
            matrix(|nth| {
                eprintln!("STRING_FAILURE_CASE {expression} xpcall={extended} allocation={nth}");
                let mut vm = Vm::new();
                let f = vm.new_host_function_with_data(arm, &[], nth).unwrap();
                vm.set_global(b"arm", f).unwrap();
                let f = vm.new_host_function(disarm, &[]).unwrap();
                vm.set_global(b"disarm", f).unwrap();
                let invoke = if extended {
                    "xpcall(put,handler)"
                } else {
                    "pcall(put)"
                };
                // Warm the call/control vectors before arming. Use a distinct
                // string so the tested operation cannot hit the intern cache.
                let program = deadlib_lua::compile(format!(
                    "collectgarbage('stop');local s='warm';local handlers=0;local function handler()handlers=handlers+1;return 'handled' end;local function warm() return 1 end;{0};local function put() return {1} end;s='Fault-String-Payload';arm();local ok,e={2};disarm();return ok,e,handlers",
                    if extended { "xpcall(warm,handler)" } else { "pcall(warm)" }, expression, invoke
                ).as_bytes()).unwrap();
                let result = vm.execute(&program).map(|v| v.to_vec());
                DENIAL.with(|slot| {
                    slot.borrow_mut().take();
                });
                let stats = allocation_probe::statistics();
                let result = result.unwrap();
                assert_eq!(result[0], Value::Boolean(nth == 0), "{expression}, {nth}");
                assert_eq!(result[2], Value::Number(0.));
                if nth != 0 {
                    let Value::String(id) = result[1] else {
                        panic!("missing memory diagnostic")
                    };
                    assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
                }
                vm.collect_garbage().unwrap();
                stats
            });
        }
    }
}

#[test]
fn native_protection_returns_memory_status_without_calling_an_invalid_handler() {
    fn allocate(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
        arm(call)?;
        let mut s = call.native_stack()?;
        s.push_bytes(b"native-string-payload")?;
        s.finish(1)
    }
    matrix(|nth| {
        let mut vm = Vm::new_bare();
        let f = vm.new_host_function_with_data(allocate, &[], nth).unwrap();
        let mut stats = Allocations::default();
        vm.with_native_stack(|s| {
            s.push_integer(17)?;
            s.push(f)?;
            let result = s.protected_call(0, Some(1), Some(1));
            stats = allocation_probe::statistics();
            DENIAL.with(|slot| {
                slot.borrow_mut().take();
            });
            assert_eq!(
                result?,
                if nth == 0 {
                    NativeCallStatus::Ok
                } else {
                    NativeCallStatus::MemoryError
                }
            );
            Ok(())
        })
        .unwrap();
        stats
    });
}
