//! Table writes under real allocator rejection; inputs and old values survive.
use deadlib_lua::{Error, ErrorKind, NativeCallStatus, NativeStack, Value, Vm};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{Allocations, probe};
use std::cell::{Cell, RefCell};
#[derive(Clone, Copy, Debug)]
enum Shape {
    EmptyHash,
    HashGrowth,
    ArrayMigration,
}
const SHAPES: [Shape; 3] = [Shape::EmptyHash, Shape::HashGrowth, Shape::ArrayMigration];
impl Shape {
    fn hints(self) -> (usize, usize) {
        match self {
            Self::EmptyHash => (0, 0),
            Self::HashGrowth => (0, 5),
            Self::ArrayMigration => (4, 0),
        }
    }
    fn key(self) -> i32 {
        match self {
            Self::EmptyHash | Self::HashGrowth => -6,
            Self::ArrayMigration => 5,
        }
    }
    fn points(self) -> usize {
        match self {
            Self::EmptyHash | Self::HashGrowth => 2,
            Self::ArrayMigration => 1,
        }
    }
    fn entries(self) -> Vec<i32> {
        match self {
            Self::EmptyHash => vec![],
            Self::HashGrowth => (1..=5).map(|n| -n).collect(),
            Self::ArrayMigration => (1..=4).chain(6..=20).collect(),
        }
    }
}
fn setup(vm: &mut Vm, shape: Shape) -> deadlib_lua::TableId {
    let table = vm
        .with_native_stack(|s| {
            s.set_top(0)?;
            let (a, r) = shape.hints();
            s.create_table(a, r)?;
            let Some(Value::Table(id)) = s.value(1) else {
                unreachable!()
            };
            Ok(id)
        })
        .unwrap();
    for key in shape.entries() {
        vm.raw_table_set(
            table,
            Value::Number(f64::from(key)),
            Value::Number(f64::from(key)),
        )
        .unwrap();
    }
    table
}
fn memory(vm: &Vm, error: &Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    let Some(Value::String(id)) = error.value() else {
        panic!("missing Lua memory diagnostic")
    };
    assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
    assert_eq!(error.line(), 0);
}
fn verify(vm: &mut Vm, table: deadlib_lua::TableId, shape: Shape, inserted: bool) {
    vm.with_native_stack(|s| {
        s.set_top(0)?;
        s.push(Value::Table(table))?;
        for key in shape.entries() {
            s.raw_get_integer(1, key)?;
            assert_eq!(s.value(-1), Some(Value::Number(f64::from(key))));
            s.set_top(1)?;
        }
        s.raw_get_integer(1, shape.key())?;
        assert_eq!(
            s.value(-1),
            Some(if inserted {
                Value::Number(42.)
            } else {
                Value::Nil
            })
        );
        s.set_top(1)
    })
    .unwrap();
}
fn each(mut action: impl FnMut(Shape, usize) -> Allocations) {
    for shape in SHAPES {
        let control = action(shape, 0);
        assert_eq!(control.rejected, 0);
        assert_eq!(control.attempts, shape.points());
        for nth in 1..=control.attempts {
            assert_eq!(action(shape, nth).rejected, 1, "{shape:?}/{nth}");
        }
        eprintln!("TABLE_MUTATION_FAILURE_POINTS {}", control.attempts);
    }
}
#[test]
fn vm_writes_return_memory_errors_and_leave_existing_entries_usable() {
    each(|shape, nth| {
        let mut vm = Vm::new_bare();
        let table = setup(&mut vm, shape);
        let (result, stats) = probe(nth, || {
            vm.raw_table_set(
                table,
                Value::Number(f64::from(shape.key())),
                Value::Number(42.),
            )
        });
        if nth == 0 {
            result.unwrap();
        } else {
            memory(&vm, &result.unwrap_err());
            verify(&mut vm, table, shape, false);
            vm.raw_table_set(
                table,
                Value::Number(f64::from(shape.key())),
                Value::Number(42.),
            )
            .unwrap();
        }
        verify(&mut vm, table, shape, true);
        stats
    });
}
fn fill_main(s: &mut NativeStack<'_, '_>, shape: Shape) -> Result<(), Error> {
    let (a, r) = shape.hints();
    s.create_table(a, r)?;
    for key in shape.entries() {
        s.push_integer(key as isize)?;
        s.raw_set_integer(1, key)?;
    }
    Ok(())
}
#[test]
fn main_integer_writes_retain_the_value_when_allocation_fails() {
    each(|shape, nth| {
        let mut vm = Vm::new_bare();
        let mut measured = Allocations::default();
        vm.with_native_stack(|s| {
            fill_main(s, shape)?;
            s.push_integer(42)?;
            let (result, stats) = probe(nth, || s.raw_set_integer(1, shape.key()));
            measured = stats;
            if nth == 0 {
                result?;
            } else {
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                assert_eq!(s.top(), 2);
                assert_eq!(s.value(-1), Some(Value::Number(42.)));
                s.raw_set_integer(1, shape.key())?;
            }
            assert_eq!(s.top(), 1);
            s.raw_get_integer(1, shape.key())?;
            assert_eq!(s.value(-1), Some(Value::Number(42.)));
            Ok(())
        })
        .unwrap();
        measured
    });
}
#[test]
fn main_raw_and_tagged_writes_retain_both_inputs_when_allocation_fails() {
    for raw in [true, false] {
        each(|shape, nth| {
            let mut vm = Vm::new_bare();
            let mut measured = Allocations::default();
            vm.with_native_stack(|s| {
                fill_main(s, shape)?;
                s.push_integer(shape.key() as isize)?;
                s.push_integer(42)?;
                let (result, stats) = probe(nth, || s.set_table(1, raw));
                measured = stats;
                if nth == 0 {
                    result?;
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(s.top(), 3);
                    assert_eq!(s.value(-2), Some(Value::Number(f64::from(shape.key()))));
                    assert_eq!(s.value(-1), Some(Value::Number(42.)));
                    s.set_table(1, raw)?;
                }
                assert_eq!(s.top(), 1);
                s.raw_get_integer(1, shape.key())?;
                assert_eq!(s.value(-1), Some(Value::Number(42.)));
                Ok(())
            })
            .unwrap();
            measured
        });
    }
}
#[test]
fn selected_raw_and_integer_writes_retain_their_inputs_when_allocation_fails() {
    for integer in [true, false] {
        each(|shape, nth| {
            let mut vm = Vm::new_bare();
            let mut measured = Allocations::default();
            vm.with_native_stack(|s| {
                let child = s.new_thread()?;
                let mut target = s.thread_stack(child)?;
                let (a, r) = shape.hints();
                target.create_table(a, r)?;
                for key in shape.entries() {
                    target.push(Value::Number(f64::from(key)))?;
                    target.raw_set_integer(1, key)?;
                }
                if !integer {
                    target.push(Value::Number(f64::from(shape.key())))?;
                }
                target.push(Value::Number(42.))?;
                let write = |target: &mut deadlib_lua::NativeThreadStack<'_>| {
                    if integer {
                        target.raw_set_integer(1, shape.key())
                    } else {
                        target.raw_set(1)
                    }
                };
                let (result, stats) = probe(nth, || write(&mut target));
                measured = stats;
                if nth == 0 {
                    result?;
                } else {
                    assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
                    assert_eq!(target.top(), if integer { 2 } else { 3 });
                    assert_eq!(target.value(-1), Some(Value::Number(42.)));
                    write(&mut target)?;
                }
                assert_eq!(target.top(), 1);
                target.raw_get_integer(1, shape.key())?;
                assert_eq!(target.value(-1), Some(Value::Number(42.)));
                Ok(())
            })
            .unwrap();
            measured
        });
    }
}
#[test]
fn a_native_write_failure_returns_status_four_without_calling_the_error_handler() {
    thread_local! {static LAST:Cell<Allocations>=const{Cell::new(Allocations{attempts:0,rejected:0,deny_from:0,enabled:false})};}
    fn write(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
        let (key, nth) = *call.function_data::<(i32, usize)>().unwrap();
        let mut s = call.native_stack()?;
        s.push_integer(42)?;
        let (result, stats) = probe(nth, || s.raw_set_integer(1, key));
        LAST.with(|cell| cell.set(stats));
        result?;
        s.finish(0)
    }
    each(|shape, nth| {
        let mut vm = Vm::new_bare();
        let table = setup(&mut vm, shape);
        let f = vm
            .new_host_function_with_data(write, &[], (shape.key(), nth))
            .unwrap();
        vm.with_native_stack(|s| {
            s.push_integer(17)?;
            s.push(f)?;
            s.push(Value::Table(table))?;
            let status = s.protected_call(1, Some(0), Some(2))?;
            assert_eq!(
                status,
                if nth == 0 {
                    NativeCallStatus::Ok
                } else {
                    NativeCallStatus::MemoryError
                }
            );
            Ok(())
        })
        .unwrap();
        verify(&mut vm, table, shape, nth == 0);
        LAST.with(Cell::get)
    });
}

#[test]
fn lua_protection_recovers_from_persistent_rejection_during_a_table_write() {
    thread_local! {
        static DENIAL:RefCell<Option<allocation_probe::Disarm>>=const{RefCell::new(None)};
    }
    fn arm(call: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
        let nth = *call.function_data::<usize>().unwrap();
        DENIAL.with(|slot| {
            *slot.borrow_mut() = Some(allocation_probe::begin(nth));
        });
        Ok(())
    }
    fn disarm(_call: &mut deadlib_lua::HostCall<'_>) -> Result<(), Error> {
        DENIAL.with(|slot| {
            slot.borrow_mut().take();
        });
        Ok(())
    }
    for extended in [false, true] {
        each(|shape, nth| {
            let mut vm = Vm::new();
            let first = setup(&mut vm, shape);
            vm.set_global(b"first", Value::Table(first)).unwrap();
            let second = setup(&mut vm, shape);
            vm.set_global(b"second", Value::Table(second)).unwrap();
            let f = vm.new_host_function_with_data(arm, &[], nth).unwrap();
            vm.set_global(b"arm", f).unwrap();
            let f = vm.new_host_function(disarm, &[]).unwrap();
            vm.set_global(b"disarm", f).unwrap();
            let invoke = if extended {
                "xpcall(put,handler)"
            } else {
                "pcall(put)"
            };
            let program=deadlib_lua::compile(format!(
            "collectgarbage('stop');local t=first;local handlers=0;local function handler()handlers=handlers+1;return 'handled' end;local function put()t[{0}]=42 end;assert({invoke});t=second;arm();local ok,e={invoke};disarm();return ok,e,t[{0}],first[{0}],handlers",shape.key()).as_bytes()).unwrap();
            let outcome = vm.execute(&program).map(|values| values.to_vec());
            DENIAL.with(|slot| {
                slot.borrow_mut().take();
            });
            let stats = allocation_probe::statistics();
            let result = outcome.unwrap();
            assert_eq!(result[0], Value::Boolean(nth == 0));
            if nth != 0 {
                let Value::String(id) = result[1] else {
                    panic!("memory diagnostic")
                };
                assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            }
            assert_eq!(
                result[2],
                if nth == 0 {
                    Value::Number(42.)
                } else {
                    Value::Nil
                }
            );
            assert_eq!(result[3], Value::Number(42.));
            assert_eq!(result[4], Value::Number(0.));
            verify(&mut vm, second, shape, nth == 0);
            stats
        });
    }
}
