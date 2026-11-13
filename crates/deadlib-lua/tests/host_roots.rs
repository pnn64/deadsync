use deadlib_lua::{Error, HostCall, Value, Vm, compile};

#[test]
fn callbacks_created_during_host_calls_retain_typed_data_and_traced_values() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Data(Arc<AtomicUsize>);
    impl Drop for Data {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert!(call.function_data::<Data>().is_some());
        let captured = call.captures()[0];
        let key = call.try_intern(b"answer")?;
        let value = call.table_get(captured, key)?;
        call.push(value)
    }
    fn factory(call: &mut HostCall<'_>) -> Result<(), Error> {
        let data = Data(call.function_data::<Arc<AtomicUsize>>().unwrap().clone());
        let function = call.new_host_function_with_data(read, &[call.arguments()[0]], data)?;
        call.with_roots(&[function], |call| {
            call.call(call.arguments()[1], &[])?;
            call.push(function)
        })
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut vm = Vm::new();
    let factory = vm
        .new_host_function_with_data(factory, &[], drops.clone())
        .unwrap();
    vm.set_global(b"factory", factory).unwrap();
    vm.execute(
        &compile(
            br#"
        local weak=setmetatable({}, {__mode='v'})
        local value={answer=42}; weak[1]=value
        local f=factory(value,function() collectgarbage('collect') end)
        value=nil; collectgarbage('collect'); assert(f()==42 and weak[1])
        f=nil; collectgarbage('collect'); collectgarbage('collect'); assert(not weak[1])
        retained=factory({answer=17},function() collectgarbage('collect') end)
        assert(retained()==17)
    "#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    drop(vm);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

fn scoped(call: &mut HostCall<'_>) -> Result<(), Error> {
    let producer = call.arguments()[0];
    let collect = call.arguments()[1];
    let fail = call.arguments()[2] == Value::Boolean(true);
    let value = call.call(producer, &[])?.first().copied().unwrap();
    call.with_roots(&[value], |call| {
        call.call(collect, &[])?;
        // Nested scopes must not release their parent's roots.
        let nested = Value::Table(call.try_new_table()?);
        call.with_roots(&[nested], |call| call.call(collect, &[]).map(|_| ()))?;
        call.call(collect, &[])?;
        let Value::Table(table) = value else {
            unreachable!()
        };
        let key = call.try_intern(b"answer")?;
        assert_eq!(call.raw_table_get(table, key)?, Value::Number(42.0));
        if fail {
            Err(call.raise(value))
        } else {
            call.push(value)
        }
    })
}

#[test]
fn scoped_roots_survive_reentry_and_release_after_success_and_error() {
    let mut vm = Vm::new();
    let callback = vm.new_host_function(scoped, &[]).unwrap();
    vm.set_global(b"scoped", callback).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                br#"
        local weak = setmetatable({}, {__mode = 'v'})
        local function produce() local t = {answer = 42}; weak[1] = t; return t end
        local function collect() collectgarbage(); assert(weak[1]) end
        local result = scoped(produce, collect, false)
        assert(result == weak[1]); result = nil
        collectgarbage(); collectgarbage(); assert(weak[1] == nil)
        local ok, err = pcall(scoped, produce, collect, true)
        assert(not ok and err == weak[1]); err = nil
        collectgarbage(); collectgarbage(); assert(weak[1] == nil)
        return true
    "#
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn partial_root_setup_failure_releases_earlier_roots() {
    // Retain a stale handle solely as non-traced Rust data for an invalid-input
    // check. Never use it as a Lua value after it has been collected.
    let mut vm = Vm::new();
    let stale = vm.try_new_table().unwrap();
    vm.execute(&compile(b"collectgarbage(); collectgarbage()").unwrap())
        .unwrap();
    fn invalid(call: &mut HostCall<'_>) -> Result<(), Error> {
        let stale = *call.function_data::<deadlib_lua::TableId>().unwrap();
        let value = call
            .call(call.arguments()[0], &[])?
            .first()
            .copied()
            .unwrap();
        let error = call
            .with_roots(&[value, Value::Table(stale)], |_| -> Result<(), Error> {
                panic!("invalid root must reject setup before running the closure")
            })
            .unwrap_err();
        assert!(error.to_string().contains("invalid"));
        Ok(())
    }
    let callback = vm.new_host_function_with_data(invalid, &[], stale).unwrap();
    vm.set_global(b"invalid", callback).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                br#"
        local weak = setmetatable({}, {__mode = 'v'})
        invalid(function() local t = {}; weak[1] = t; return t end)
        collectgarbage(); collectgarbage()
        return weak[1] == nil
    "#
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true)]
    );
}
