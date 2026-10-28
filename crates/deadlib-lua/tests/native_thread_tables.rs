use deadlib_lua::{Error, HostCall, Value, Vm, compile};

fn yielding(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.native_stack()?.yield_results(1)
}

fn selected(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let owner = stack.thread_id();
    let target = stack.to_thread(1).unwrap();
    let mode = stack.to_integer(2).unwrap();
    let left = stack.value(3).unwrap();
    let right = stack.value(4).unwrap();
    let value = stack.value(5).unwrap();
    let before = stack.thread_stack(target)?.top();
    let status = stack.thread_status(target).unwrap();
    stack.thread_stack(target)?.push(left)?;
    if matches!(mode, 0 | 2 | 4 | 5) {
        stack.thread_stack(target)?.push(right)?;
    }
    if matches!(mode, 2 | 3) {
        stack.thread_stack(target)?.push(value)?;
    }
    let result = match mode {
        0 => {
            stack.get_table_thread(target, -2, false)?;
            stack.thread_stack(target)?.value(-1).unwrap()
        }
        1 => {
            stack.get_field_thread(target, -1, b"field")?;
            stack.thread_stack(target)?.value(-1).unwrap()
        }
        2 => {
            stack.set_table_thread(target, -3, false)?;
            Value::Nil
        }
        3 => {
            stack.set_field_thread(target, -2, b"field")?;
            Value::Nil
        }
        4 => Value::Boolean(stack.equal_thread(target, -2, -1)?),
        5 => Value::Boolean(stack.less_than_thread(target, -2, -1)?),
        _ => unreachable!(),
    };
    assert_eq!(stack.thread_id(), owner);
    let after = stack.thread_stack(target)?.top();
    let new_status = stack.thread_status(target).unwrap();
    stack.push(Value::Number(before as f64))?;
    stack.push(Value::Number(after as f64))?;
    stack.push(Value::Number(status as u8 as f64))?;
    stack.push(Value::Number(new_status as u8 as f64))?;
    stack.push(result)?;
    stack.thread_stack(target)?.set_top(before)?;
    stack.finish(5)
}

#[test]
fn selected_table_methods_preserve_context_and_restore_completed_call_bases() {
    let mut vm = Vm::new();
    for (name, function) in [
        (b"boundary_table_thread".as_slice(), selected as _),
        (b"boundary_execution_yield_tail".as_slice(), yielding as _),
    ] {
        let function = vm.new_host_function(function, &[]).unwrap();
        vm.set_global(name, function).unwrap();
    }
    let corpus = include_bytes!("native-boundary/thread-tables.lua");
    let end = corpus
        .windows(b"local function token".len())
        .position(|part| part == b"local function token")
        .unwrap();
    let mut source = corpus[..end].to_vec();
    source.extend_from_slice(b"assert(count==240);collectgarbage('restart');return 42");
    assert_eq!(
        vm.execute(&compile(&source).unwrap()).unwrap(),
        &[Value::Number(42.)]
    );
}
