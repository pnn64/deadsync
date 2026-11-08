use deadlib_lua::{Error, HostCall, Value, Vm, compile};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::Relaxed},
};

fn execute(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}
struct Owner(Arc<AtomicUsize>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.fetch_add(1, Relaxed);
    }
}
fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
    Ok(())
}

#[test]
fn private_binding_data_is_typed_and_does_not_occupy_lua_upvalues() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert!(call.function_data::<String>().is_none());
        let value = *call.function_data::<usize>().unwrap();
        assert_eq!(call.captures(), [Value::Number(17.)]);
        call.push(Value::Number(value as f64))
    }
    fn ordinary(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert!(call.function_data::<usize>().is_none());
        Ok(())
    }
    let mut vm = Vm::new();
    let host = vm
        .new_host_function_with_data(callback, &[Value::Number(17.)], 42usize)
        .unwrap();
    let plain = vm.new_host_function(ordinary, &[]).unwrap();
    vm.set_global(b"host", host).unwrap();
    vm.set_global(b"plain", plain).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            b"collectgarbage();plain();return host(),debug.getinfo(host,'u').nups,select('#',debug.getupvalue(host,1)),select('#',debug.setupvalue(host,1,99))",
        ),
        [Value::Number(42.), Value::Number(1.), Value::Number(0.), Value::Number(0.)]
    );
}

#[test]
fn closure_data_survives_live_capture_cycles_and_drops_when_the_cycle_collects() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let owner = call.function_data::<Owner>().unwrap().0.clone();
        assert_eq!(owner.load(Relaxed), 0);
        let collect = call.captures()[1];
        call.call(collect, &[])?;
        assert_eq!(owner.load(Relaxed), 0);
        call.push(call.captures()[0])
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut vm = Vm::new();
    let table = vm.new_table();
    let collect = vm.global(b"collectgarbage");
    let host = vm
        .new_host_function_with_data(
            callback,
            &[Value::Table(table), collect],
            Owner(drops.clone()),
        )
        .unwrap();
    let key = vm.intern(b"cycle");
    vm.raw_table_set(table, key, host).unwrap();
    vm.set_global(b"host", host).unwrap();
    assert_eq!(
        execute(&mut vm, b"return host().cycle==host"),
        [Value::Boolean(true)]
    );
    vm.collect_garbage().unwrap();
    assert_eq!(drops.load(Relaxed), 0);
    vm.set_global(b"host", Value::Nil).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(drops.load(Relaxed), 1);
    drop(vm);
    assert_eq!(drops.load(Relaxed), 1);
}

#[test]
fn private_data_remains_alive_during_lua_shutdown_finalizers() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let (owner, finalized) = call.function_data::<(Owner, Arc<AtomicUsize>)>().unwrap();
        assert_eq!(owner.0.load(Relaxed), 0);
        finalized.fetch_add(1, Relaxed);
        Ok(())
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let finalized = Arc::new(AtomicUsize::new(0));
    {
        let mut vm = Vm::new();
        let function = vm
            .new_host_function_with_data(callback, &[], (Owner(drops.clone()), finalized.clone()))
            .unwrap();
        vm.set_global(b"host", function).unwrap();
        execute(&mut vm, b"kept=newproxy(true);getmetatable(kept).__gc=host");
    }
    assert_eq!(finalized.load(Relaxed), 1);
    assert_eq!(drops.load(Relaxed), 1);
}

#[test]
fn rejected_registrations_release_owned_data_and_do_not_publish_a_closure() {
    let mut vm = Vm::new();
    let before = vm.collect_garbage().unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    assert!(
        vm.new_host_function_with_data(empty, &[Value::Native(u64::MAX)], Owner(drops.clone()))
            .is_err()
    );
    assert!(
        vm.new_host_function_with_data(empty, &[Value::Nil; 256], Owner(drops.clone()))
            .is_err()
    );
    assert_eq!(drops.load(Relaxed), 2);
    assert_eq!(
        vm.collect_garbage().unwrap().native_closures,
        before.native_closures
    );
}

#[test]
fn native_stack_builds_a_closure_with_all_255_traced_captures() {
    fn child(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert_eq!(*call.function_data::<usize>().unwrap(), 99);
        let mut stack = call.native_stack()?;
        assert_eq!(stack.number(-10003)?, 1.);
        assert_eq!(stack.number(-10257)?, 255.);
        assert_eq!(stack.value(-10258), None);
        stack.set(-10257, Value::Number(42.))?;
        stack.push(stack.value(-10257).unwrap())?;
        stack.finish(1)
    }
    fn factory(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        for n in 1..=255 {
            stack.push(Value::Number(f64::from(n)))?;
        }
        stack.push_closure(child, 255, 99usize)?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(factory, &[]).unwrap();
    vm.set_global(b"factory", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            b"local f=factory();collectgarbage();return debug.getinfo(f,'u').nups,f()",
        ),
        [Value::Number(255.), Value::Number(42.)]
    );
}

#[test]
fn pseudo_indexes_distinguish_registry_closure_environment_and_thread_globals() {
    fn snapshot(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(stack.value(-10000).unwrap())?;
        stack.push(stack.value(-10001).unwrap())?;
        stack.push(stack.value(-10002).unwrap())?;
        stack.finish(3)
    }
    fn factory(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let environment = stack.value(1).unwrap();
        let old = stack.value(-10001);
        assert!(stack.set(-10001, Value::Nil).is_err());
        assert_eq!(stack.value(-10001), old);
        stack.set(-10001, environment)?;
        stack.push_closure(snapshot, 0, ())?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(factory, &[]).unwrap();
    vm.set_global(b"factory", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local environment={};local f=factory(environment);collectgarbage()
local r,e,g=f()
return r==debug.getregistry(),e==environment,g==_G,debug.getfenv(f)==environment,
debug.getfenv(factory)==environment
"#
        ),
        [Value::Boolean(true); 5]
    );
}

#[test]
fn failed_stack_closure_creation_preserves_the_stack_and_releases_data() {
    fn factory(call: &mut HostCall<'_>) -> Result<(), Error> {
        let drops = Arc::new(AtomicUsize::new(0));
        let mut stack = call.native_stack()?;
        stack.push(Value::Number(42.))?;
        assert!(stack.push_closure(empty, 2, Owner(drops.clone())).is_err());
        assert_eq!(stack.top(), 1);
        assert_eq!(drops.load(Relaxed), 1);
        stack.set_top(256)?;
        assert!(
            stack
                .push_closure(empty, 256, Owner(drops.clone()))
                .is_err()
        );
        assert_eq!(stack.top(), 256);
        assert_eq!(stack.value(1), Some(Value::Number(42.)));
        assert_eq!(drops.load(Relaxed), 2);
        stack.set_top(8000)?;
        assert!(stack.push_closure(empty, 0, Owner(drops.clone())).is_err());
        assert_eq!(stack.top(), 8000);
        assert_eq!(drops.load(Relaxed), 3);
        stack.set_top(0)?;
        stack.finish(0)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(factory, &[]).unwrap();
    vm.set_global(b"factory", function).unwrap();
    assert!(execute(&mut vm, b"return factory()").is_empty());
}

#[test]
fn boxed_binding_object_size_is_included_in_heap_accounting() {
    let mut vm = Vm::new();
    let before = vm.heap_stats();
    let function = vm
        .new_host_function_with_data(empty, &[], [0u8; 4096])
        .unwrap();
    vm.set_global(b"host", function).unwrap();
    let after = vm.heap_stats();
    assert!(after.live_bytes >= before.live_bytes + 4096);
}
