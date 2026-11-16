use deadlib_lua::{Error, HostCall, Value, Vm, compile};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Ledger {
    entries: Vec<f64>,
    closed: Arc<AtomicUsize>,
}
impl Drop for Ledger {
    fn drop(&mut self) {
        self.closed.fetch_add(1000, Ordering::Relaxed);
    }
}

fn record(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.number(0)?;
    let Some(ledger) = call.host_state_mut::<Ledger>() else {
        return Err(call.error(b"ledger is not installed"));
    };
    ledger.entries.push(value);
    let count = ledger.entries.len();
    call.push(Value::Number(count as f64))
}

// The borrow of the state ends before the nested Lua call, which records again.
fn record_around(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.host_state_mut::<Ledger>().unwrap().entries.push(-1.0);
    let function = call.arguments()[0];
    call.call(function, &[])?;
    let ledger = call.host_state_mut::<Ledger>().unwrap();
    ledger.entries.push(-2.0);
    let count = ledger.entries.len();
    call.push(Value::Number(count as f64))
}

fn finalized(call: &mut HostCall<'_>) -> Result<(), Error> {
    let ledger = call.host_state::<Ledger>().expect("state during close");
    ledger.closed.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

fn vm(closed: &Arc<AtomicUsize>) -> Vm {
    let mut vm = Vm::new();
    assert!(
        vm.set_host_state(Box::new(Ledger {
            entries: Vec::new(),
            closed: closed.clone(),
        }))
        .is_none()
    );
    for (name, callback) in [
        (b"record".as_slice(), record as deadlib_lua::HostFunction),
        (b"record_around", record_around),
        (b"finalized", finalized),
    ] {
        let function = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, function).unwrap();
    }
    vm
}

#[test]
fn callbacks_and_nested_callbacks_share_one_state_without_locking() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut vm = vm(&closed);
    let program = compile(
        b"record(1); local n = record_around(function() record(2); record(3) end); return n, record(4)",
    )
    .unwrap();
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[Value::Number(5.0), Value::Number(6.0)]
    );
    assert_eq!(
        vm.host_state::<Ledger>().unwrap().entries,
        [1.0, -1.0, 2.0, 3.0, -2.0, 4.0]
    );
    // Top-level Rust code edits the same value between executions.
    vm.host_state_mut::<Ledger>().unwrap().entries.clear();
    vm.execute(&compile(b"record(9)").unwrap()).unwrap();
    assert_eq!(vm.host_state::<Ledger>().unwrap().entries, [9.0]);
}

#[test]
fn another_type_or_a_taken_state_is_reported_as_absent() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut vm = vm(&closed);
    assert!(vm.host_state::<u32>().is_none());
    assert!(vm.host_state_mut::<String>().is_none());
    let previous = vm.set_host_state(Box::new(7_u32)).expect("replaced ledger");
    assert!(previous.downcast_ref::<Ledger>().is_some());
    assert_eq!(vm.host_state::<u32>(), Some(&7));
    let error = vm
        .execute(&compile(b"record(1)").unwrap())
        .expect_err("ledger was replaced");
    assert!(
        error.to_string().contains("ledger is not installed"),
        "{error}"
    );
    assert_eq!(
        vm.take_host_state().unwrap().downcast_ref::<u32>(),
        Some(&7)
    );
    assert!(vm.take_host_state().is_none());
    assert_eq!(closed.load(Ordering::Relaxed), 0);
    drop(previous);
    assert_eq!(closed.load(Ordering::Relaxed), 1000);
}

#[test]
fn closing_finalizers_still_reach_the_state_before_it_is_dropped() {
    let closed = Arc::new(AtomicUsize::new(0));
    let mut vm = vm(&closed);
    vm.execute(
        &compile(b"keep = newproxy(true); getmetatable(keep).__gc = function() finalized() end")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(closed.load(Ordering::Relaxed), 0);
    drop(vm);
    // One finalizer observed the live state, then the VM released it.
    assert_eq!(closed.load(Ordering::Relaxed), 1001);
}
