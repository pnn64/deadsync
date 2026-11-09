//! The existing actor-grid size must fit in the Rust VM's pinned roots.
use deadlib_lua::{Error, HostCall, Value, Vm};

#[test]
fn rust_handles_can_retain_a_native_sized_actor_grid() {
    let mut vm = Vm::new();
    let mut handles = Vec::new();
    for index in 0..(192 * 108) {
        let table = vm.try_new_table().unwrap();
        handles.push(
            vm.pin(Value::Table(table))
                .unwrap_or_else(|error| panic!("actor handle {index}: {error}")),
        );
    }
    assert_eq!(handles.len(), 20_736);
    vm.collect_garbage().unwrap();
    let Value::Table(table) = vm.rooted(handles[20_735]).unwrap() else {
        panic!("retained table")
    };
    let marker = vm.intern(b"marker");
    vm.raw_table_set(table, marker, Value::Number(19.)).unwrap();
    assert_eq!(vm.raw_table_get(table, marker).unwrap(), Value::Number(19.));
    for handle in handles {
        vm.unpin(handle).unwrap();
    }
}

#[test]
fn normal_c_calls_keep_the_native_stack_limit() {
    fn count(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let arguments = stack.top();
        stack.push(Value::Number(arguments as f64))?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(count, &[]).unwrap();
    let root = vm.pin(function).unwrap();
    let error = vm.call(function, &vec![Value::Nil; 8_001]).unwrap_err();
    assert!(
        error.message().contains("native stack overflow"),
        "{error:?}"
    );
    assert_eq!(
        vm.call(function, &[Value::Nil; 2]).unwrap(),
        &[Value::Number(2.)]
    );
    vm.unpin(root).unwrap();
}
