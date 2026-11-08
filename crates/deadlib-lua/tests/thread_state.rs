use deadlib_lua::{Error, HostCall, HostThreadState, ThreadId, Value, Vm, compile};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

fn install(
    vm: &mut Vm,
    name: &[u8],
    function: fn(&mut HostCall<'_>) -> Result<(), Error>,
    captures: &[Value],
) {
    let function = vm.new_host_function(function, captures).unwrap();
    vm.set_global(name, function).unwrap();
}
fn execute(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn thread_identity_and_state_are_stable_independent_and_not_inherited() {
    struct State {
        owner: ThreadId,
        visits: AtomicUsize,
    }
    impl HostThreadState for State {}
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        if call.thread_state::<State>().is_none() {
            call.initialize_thread_state(State {
                owner: call.thread_id(),
                visits: AtomicUsize::new(0),
            })?;
        }
        let state = call.thread_state::<State>().unwrap();
        assert_eq!(state.owner, call.thread_id());
        let owner = state.owner;
        let visits = state.visits.fetch_add(1, Relaxed) + 1;
        call.push(Value::Thread(owner))?;
        call.push(Value::Number(visits as f64))
    }
    let mut vm = Vm::new();
    install(&mut vm, b"host", callback, &[]);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local main,a=host();local again,b=host()
assert(main==again and a==1 and b==2 and coroutine.running()==nil)
local co=coroutine.create(function()
 local own,n=host();assert(own==coroutine.running()and own~=main and n==1)
 coroutine.yield(own)
 local again,n=host();assert(again==own and n==2)
 return true
end)
local ok,own=coroutine.resume(co);assert(ok and own==co)
collectgarbage();local ok,value=coroutine.resume(co)
return ok,value,select(2,host())
"#
        ),
        [
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Number(3.)
        ]
    );
}

#[test]
fn type_slots_coexist_and_duplicate_initialization_never_replaces_state() {
    struct A(usize);
    impl HostThreadState for A {}
    struct B(usize);
    impl HostThreadState for B {}
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.initialize_thread_state(A(17))?;
        call.initialize_thread_state(B(42))?;
        assert!(call.initialize_thread_state(A(99)).is_err());
        let a = call.thread_state::<A>().unwrap().0;
        let b = call.thread_state::<B>().unwrap().0;
        call.push(Value::Number(a as f64))?;
        call.push(Value::Number(b as f64))
    }
    let mut vm = Vm::new();
    install(&mut vm, b"host", callback, &[]);
    assert_eq!(
        execute(&mut vm, b"return host()"),
        [Value::Number(17.), Value::Number(42.)]
    );
}

#[test]
fn suspended_thread_state_roots_survive_but_unreachable_self_cycles_collect() {
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct State(Vec<Value>);
    impl HostThreadState for State {
        fn roots(&self) -> &[Value] {
            &self.0
        }
        fn heap_bytes(&self) -> usize {
            self.0.capacity() * std::mem::size_of::<Value>()
        }
    }
    impl Drop for State {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Relaxed);
        }
    }
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let value = Value::Table(call.new_table());
        call.initialize_thread_state(State(vec![value, Value::Thread(call.thread_id())]))?;
        call.push(value)
    }
    DROPS.store(0, Relaxed);
    let mut vm = Vm::new();
    install(&mut vm, b"host", callback, &[]);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local weak=setmetatable({},{__mode='v'})
local co=coroutine.create(function()
 local value=host();weak[1]=value;value=nil;coroutine.yield()
end)
assert(coroutine.resume(co));collectgarbage();local retained=weak[1]~=nil
co=nil;collectgarbage();return retained,weak[1]==nil
"#
        ),
        [Value::Boolean(true), Value::Boolean(true)]
    );
    assert_eq!(DROPS.load(Relaxed), 1);
}

#[test]
fn state_stays_alive_through_shutdown_finalizers_and_drops_once_afterward() {
    static FREED: AtomicUsize = AtomicUsize::new(0);
    static FINALIZED: AtomicUsize = AtomicUsize::new(0);
    struct State;
    impl HostThreadState for State {}
    impl Drop for State {
        fn drop(&mut self) {
            FREED.fetch_add(1, Relaxed);
        }
    }
    fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.initialize_thread_state(State)
    }
    fn finalize(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert!(call.thread_state::<State>().is_some());
        assert_eq!(FREED.load(Relaxed), 0);
        FINALIZED.fetch_add(1, Relaxed);
        Ok(())
    }
    FREED.store(0, Relaxed);
    FINALIZED.store(0, Relaxed);
    {
        let mut vm = Vm::new();
        install(&mut vm, b"initialize", initialize, &[]);
        install(&mut vm, b"finalize", finalize, &[]);
        execute(
            &mut vm,
            b"initialize();kept=newproxy(true);getmetatable(kept).__gc=finalize",
        );
    }
    assert_eq!(FINALIZED.load(Relaxed), 1);
    assert_eq!(FREED.load(Relaxed), 1);
}

#[test]
fn invalid_root_rejection_leaves_the_type_slot_available() {
    struct State(Vec<Value>);
    impl HostThreadState for State {
        fn roots(&self) -> &[Value] {
            &self.0
        }
    }
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let value = Value::Table(call.new_table());
        let collect = call.captures()[0];
        call.call(collect, &[])?;
        assert!(call.initialize_thread_state(State(vec![value])).is_err());
        assert!(call.thread_state::<State>().is_none());
        call.initialize_thread_state(State(Vec::new()))?;
        call.push(Value::Boolean(true))
    }
    let mut vm = Vm::new();
    let collect = vm.global(b"collectgarbage");
    install(&mut vm, b"host", callback, &[collect]);
    assert_eq!(execute(&mut vm, b"return host()"), [Value::Boolean(true)]);
}
