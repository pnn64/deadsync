use deadlib_lua::{Error, HostCall, Value, Vm, compile};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::Relaxed},
};

struct Owner(Arc<AtomicUsize>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.fetch_add(1, Relaxed);
    }
}
fn empty(_: &mut HostCall<'_>) -> Result<(), Error> {
    Ok(())
}
fn exec(vm: &mut Vm, source: &str) {
    vm.execute(&compile(source.as_bytes()).unwrap()).unwrap();
}
fn state(
    vm: &mut Vm,
    owner: deadlib_lua::TableId,
    drops: &Arc<AtomicUsize>,
) -> deadlib_lua::TableId {
    let state = vm.try_new_table().unwrap();
    let function = vm
        .new_host_function_with_data(empty, &[Value::Table(owner)], Owner(drops.clone()))
        .unwrap();
    vm.raw_table_set(state, Value::Number(1.), function)
        .unwrap();
    vm.set_table_host_state(owner, Some(state)).unwrap();
    state
}

#[test]
fn private_state_survives_field_and_metatable_changes_and_collects_with_its_owner() {
    let mut vm = Vm::new();
    let drops = Arc::new(AtomicUsize::new(0));
    let owner = vm.try_new_table().unwrap();
    vm.set_global(b"owner", Value::Table(owner)).unwrap();
    let state = state(&mut vm, owner, &drops);
    exec(
        &mut vm,
        "assert(next(owner)==nil and #owner==0);setmetatable(owner,{__mode='kv'});collectgarbage('collect');setmetatable(owner,{});collectgarbage('collect')",
    );
    assert_eq!(vm.table_host_state(owner).unwrap(), Some(state));
    assert_eq!(drops.load(Relaxed), 0);
    exec(
        &mut vm,
        "weak=setmetatable({owner},{__mode='v'});owner=nil;collectgarbage('collect');collectgarbage('collect');assert(weak[1]==nil)",
    );
    assert_eq!(drops.load(Relaxed), 1);
    assert!(vm.table_host_state(owner).is_err());
}

#[test]
fn detaching_and_replacing_state_releases_the_previous_attachment() {
    let mut vm = Vm::new();
    let drops = Arc::new(AtomicUsize::new(0));
    let owner = vm.try_new_table().unwrap();
    let root = vm.pin(Value::Table(owner)).unwrap();
    state(&mut vm, owner, &drops);
    state(&mut vm, owner, &drops);
    vm.collect_garbage().unwrap();
    assert_eq!(drops.load(Relaxed), 1);
    vm.set_table_host_state(owner, None).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(drops.load(Relaxed), 2);
    assert_eq!(vm.table_host_state(owner).unwrap(), None);
    vm.unpin(root).unwrap();
}

#[test]
fn callback_attachments_trace_through_collecting_lua_reentry() {
    fn attach(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Table(owner) = call.arguments()[0] else {
            unreachable!()
        };
        let state = call.try_new_table()?;
        call.raw_table_set(state, Value::Number(1.), Value::Number(42.))?;
        call.set_table_host_state(owner, Some(state))?;
        call.call(call.arguments()[1], &[])?;
        assert_eq!(call.table_host_state(owner)?, Some(state));
        call.push(call.raw_table_get(state, Value::Number(1.))?)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(attach, &[]).unwrap();
    vm.set_global(b"attach", f).unwrap();
    exec(
        &mut vm,
        "local owner=setmetatable({},{__mode='kv'});assert(attach(owner,function()collectgarbage('collect')end)==42);assert(next(owner)==nil)",
    );
}

#[test]
fn collected_handles_do_not_replace_private_state() {
    let mut vm = Vm::new();
    let owner = vm.try_new_table().unwrap();
    let root = vm.pin(Value::Table(owner)).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let state = state(&mut vm, owner, &drops);
    let stale = vm.try_new_table().unwrap();
    vm.collect_garbage().unwrap();
    assert!(vm.set_table_host_state(owner, Some(stale)).is_err());
    assert!(vm.set_table_host_state(stale, Some(state)).is_err());
    assert!(vm.table_host_state(stale).is_err());
    assert_eq!(vm.table_host_state(owner).unwrap(), Some(state));
    vm.unpin(root).unwrap();
}
