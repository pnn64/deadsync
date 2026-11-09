//! Original ignored-argument contract over borrowed native argument registers.
use deadlib_lua::{Error, HostCall, Value, Vm, compile_named};
#[path = "support/baked_program.rs"]
mod api;

fn reader(vm: &mut Vm, borrowed: bool) -> Value {
    let f = vm
        .new_host_function_with_data(
            |call| {
                // Ordinary owned decoding is the independent allocation baseline; both
                // paths receive the VM's validated values, including ignored extras.
                if !*call.function_data::<bool>().unwrap() {
                    std::hint::black_box(call.arguments().to_vec());
                }
                call.push(Value::Number(17.))
            },
            &[],
            borrowed,
        )
        .unwrap();
    vm.pin(f).unwrap();
    f
}
fn capture(call: &mut HostCall<'_>) -> Result<(), Error> {
    let borrowed = *call.function_data::<bool>().unwrap();
    let owned = (!borrowed).then(|| call.arguments().to_vec());
    let len = call.arguments().len();
    let table = call.try_new_table()?;
    call.with_roots(&[Value::Table(table)], |call| {
        for i in 0..len {
            let value = owned
                .as_ref()
                .map_or_else(|| call.arguments()[i], |values| values[i]);
            call.raw_table_set(table, Value::Number((i + 1) as f64), value)?;
        }
        call.push(Value::Number(len as f64))?;
        call.push(Value::Table(table))
    })
}
#[test]
fn checked_unused_arguments_match_values_panics_and_reference_pressure() {
    let mut pressure = Vec::new();
    for current in [false, true] {
        let mut vm = Vm::new();
        let f = vm
            .new_host_function_with_data(capture, &[], current)
            .unwrap();
        vm.set_global(b"capture", f).unwrap();
        api::eval(
            &mut vm,
            r#"
            local t, f, co, ud = {}, function() end, coroutine.create(function() end), newproxy(true)
            local args = {nil, false, 17, -0.0, string.char(255,0), t, f, co, ud, nil}
            for n=0,10 do
                local count, values = capture(unpack(args,1,n))
                assert(count==n)
                for i=1,n do assert(rawequal(values[i],args[i])) end
            end
            local ok, n, values = coroutine.resume(coroutine.create(function() return capture(t, nil, ud) end))
            assert(ok and n==3 and values[1]==t and values[2]==nil and values[3]==ud)
        "#,
        );
        let target = api::table(&mut vm);
        let getter = reader(&mut vm, current);
        assert_eq!(vm.call(getter, &[target]).unwrap(), [Value::Number(17.)]);
        let mut held = Vec::new();
        for _ in 0..5000 {
            if let Ok(table) = vm.try_new_table() {
                held.push(vm.pin(Value::Table(table)).unwrap());
            } else {
                break;
            }
        }
        let args: Vec<_> = (0..4000).map(|_| target).collect();
        pressure.push(
            vm.call(getter, &args)
                .map(|values| values.to_vec())
                .map_err(|e| e.to_string()),
        );
        for root in held {
            vm.unpin(root).unwrap();
        }
        assert_eq!(vm.call(getter, &[target]).unwrap(), [Value::Number(17.)]);
    }
    assert_eq!(pressure[0], pressure[1]);
    #[cfg(panic = "unwind")]
    for current in [false, true] {
        let mut vm = Vm::new();
        let f = reader(&mut vm, current);
        vm.set_global(b"read", f).unwrap();
        let panic_arg = vm
            .new_host_function(
                |_| -> Result<(), Error> { panic!("ignored callback argument panic") },
                &[],
            )
            .unwrap();
        vm.set_global(b"panic_arg", panic_arg).unwrap();
        let source = compile_named(
            b"local _, p=pcall(panic_arg); return read({},p)",
            b"=ignored argument panic",
        )
        .unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            vm.execute(&source).map(|v| v.to_vec())
        }))
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>().copied(),
            Some("ignored callback argument panic")
        );
        // Rust panics bypass Lua protection and require discarding this VM. Retain
        // the original continuation check in a fresh instance after the unwind.
        drop(vm);
        let mut continuation = Vm::new();
        assert_eq!(
            api::eval(&mut continuation, "return 6*7"),
            [Value::Number(42.)]
        );
    }
    println!(
        "Validated values/order, nils, binary strings, coroutine inputs and reference-pressure parity/recovery (panic tests also run when built with unwinding)."
    );
}
