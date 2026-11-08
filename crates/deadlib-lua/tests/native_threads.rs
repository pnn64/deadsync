use deadlib_lua::{
    Error, ErrorKind, HostCall, HostThreadState, NativeThreadStatus, ThreadId, Value, Vm, compile,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering::Relaxed},
};
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}
fn install(vm: &mut Vm, callback: fn(&mut HostCall<'_>) -> Result<(), Error>) {
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
}

#[test]
fn native_thread_values_and_statuses_distinguish_fresh_yielded_running_and_dead() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let target = stack.to_thread(1).unwrap_or(stack.thread_id());
        let status = stack.thread_status(target).unwrap();
        let main = stack.push_thread()?;
        assert_eq!(stack.to_thread(-1), Some(stack.thread_id()));
        assert_eq!(
            stack.thread_status(stack.thread_id()),
            Some(NativeThreadStatus::Ok)
        );
        stack.push(Value::Boolean(main))?;
        stack.push_integer(status as isize)?;
        stack.finish(3)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(
        &mut vm,
        br#"
local main,is_main,status=host();assert(is_main and status==0 and coroutine.running()==nil)
local co=coroutine.create(function()
 local self,is_main,s=host();assert(self==coroutine.running() and not is_main and s==0)
 assert(select(3,host(main))==0);coroutine.yield(42)
 assert(select(3,host())==0);return 17
end)
assert(select(3,host(co))==0)
assert(coroutine.resume(co));assert(select(3,host(co))==1)
assert(coroutine.resume(co));assert(select(3,host(co))==0)
assert(not coroutine.resume(co));assert(select(3,host(co))==0)
local failed=coroutine.create(function()error({})end)
assert(not coroutine.resume(failed));assert(select(3,host(failed))==2)
assert(not coroutine.resume(failed));assert(select(3,host(failed))==2)
local caught=coroutine.create(function()assert(not pcall(error,{}))end)
assert(coroutine.resume(caught));assert(select(3,host(caught))==0)
"#,
    );
}

struct Resource {
    roots: Vec<Value>,
    payload: Box<[u8]>,
    drops: Arc<AtomicUsize>,
}
impl HostThreadState for Resource {
    fn roots(&self) -> &[Value] {
        &self.roots
    }
    fn heap_bytes(&self) -> usize {
        self.payload.len() + self.roots.capacity() * std::mem::size_of::<Value>()
    }
}
impl Drop for Resource {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Relaxed);
    }
}

#[test]
fn never_resumed_thread_resources_retain_roots_survive_moves_and_collect_self_cycles() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let drops = call.function_data::<Arc<AtomicUsize>>().unwrap().clone();
        let mut stack = call.native_stack()?;
        let thread = stack.to_thread(1).unwrap();
        if stack.thread_state::<Resource>(thread).is_none() {
            let root = stack.value(2).unwrap();
            stack.initialize_thread_state(
                thread,
                Resource {
                    roots: vec![root, Value::Thread(thread)],
                    payload: vec![17; 37].into_boxed_slice(),
                    drops,
                },
            )?;
        }
        let state = stack.thread_state::<Resource>(thread).unwrap();
        let pointer = state.payload.as_ptr() as usize as u64;
        let root = state.roots[0];
        stack.push(Value::LightUserdata(pointer))?;
        stack.push(root)?;
        stack.finish(2)
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut vm = Vm::new();
    let host = vm
        .new_host_function_with_data(callback, &[], drops.clone())
        .unwrap();
    vm.set_global(b"host", host).unwrap();
    run(
        &mut vm,
        br#"
weak=setmetatable({},{__mode='v'})
local co=coroutine.create(function()error('resource lookup must not resume')end)
local root={};weak[1]=root;weak[2]=co
local pointer,value=host(co,root);assert(value==root and coroutine.status(co)=='suspended')
root,value=nil,nil;collectgarbage();assert(weak[1]~=nil)
owner=co;saved_pointer=pointer
"#,
    );
    let mut vm = Box::new(vm);
    run(
        &mut vm,
        br#"
local threads={};for i=1,600 do threads[i]=coroutine.create(function()end)end
collectgarbage();local pointer,value=host(owner)
assert(pointer==saved_pointer and value==weak[1] and coroutine.status(owner)=='suspended')
owner=nil;value=nil;collectgarbage();assert(weak[1]==nil and weak[2]==nil)
"#,
    );
    assert_eq!(drops.load(Relaxed), 1);
}

#[test]
fn inactive_type_slots_reject_duplicates_without_replacing_or_inheriting_resources() {
    struct A(usize);
    impl HostThreadState for A {}
    struct B(usize);
    impl HostThreadState for B {}
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let thread = stack.to_thread(1).unwrap();
        assert!(stack.thread_state::<A>(thread).is_none());
        stack.initialize_thread_state(thread, A(17))?;
        stack.initialize_thread_state(thread, B(42))?;
        assert!(stack.initialize_thread_state(thread, A(99)).is_err());
        assert_eq!(stack.thread_state::<A>(thread).unwrap().0, 17);
        assert_eq!(stack.thread_state::<B>(thread).unwrap().0, 42);
        assert!(stack.thread_state::<A>(stack.thread_id()).is_none());
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,b"local a=coroutine.create(function()end);local b=coroutine.create(function()end);assert(host(a)==a and host(b)==b)");
}

#[test]
fn collected_thread_ids_are_checked_and_rejected_state_is_dropped() {
    struct Data {
        thread: ThreadId,
        drops: Arc<AtomicUsize>,
    }
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let data = call.function_data::<Data>().unwrap();
        let thread = data.thread;
        let drops = data.drops.clone();
        let mut stack = call.native_stack()?;
        assert_eq!(stack.thread_status(thread), None);
        assert!(stack.thread_state::<Resource>(thread).is_none());
        assert!(
            stack
                .initialize_thread_state(
                    thread,
                    Resource {
                        roots: vec![],
                        payload: Box::new([]),
                        drops
                    }
                )
                .is_err()
        );
        stack.finish(0)
    }
    let mut vm = Vm::new();
    let values = run(&mut vm, b"return coroutine.create(function()end)");
    let Value::Thread(thread) = values[0] else {
        panic!("thread expected")
    };
    run(&mut vm, b"return 42");
    vm.collect_garbage().unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let host = vm
        .new_host_function_with_data(
            callback,
            &[],
            Data {
                thread,
                drops: drops.clone(),
            },
        )
        .unwrap();
    vm.set_global(b"host", host).unwrap();
    run(
        &mut vm,
        b"local replacement=coroutine.create(function()end);host(replacement)",
    );
    assert_eq!(drops.load(Relaxed), 1);
}

#[test]
fn captured_and_negative_indexes_convert_threads_without_coercing_other_types() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let stack = call.native_stack()?;
        assert_eq!(stack.to_thread(1), stack.to_thread(-1));
        assert_eq!(stack.to_thread(1), stack.to_thread(-10003));
        for index in [0, 2, -2, -10000, -10001, -10002, -10004] {
            assert!(stack.to_thread(index).is_none());
        }
        stack.finish(1)
    }
    let mut vm = Vm::new();
    let values = run(&mut vm, b"return coroutine.create(function()end)");
    let co = values[0];
    let host = vm.new_host_function(callback, &[co]).unwrap();
    vm.set_global(b"host", host).unwrap();
    vm.set_global(b"co", co).unwrap();
    run(&mut vm, b"assert(host(co)==co)");
}

#[test]
fn fatal_child_quota_cancellation_records_error_status_and_allows_main_recovery() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let stack = call.native_stack()?;
        let thread = stack.to_thread(1).unwrap();
        assert_eq!(
            stack.thread_status(thread),
            Some(NativeThreadStatus::RuntimeError)
        );
        assert_eq!(
            stack.thread_status(stack.thread_id()),
            Some(NativeThreadStatus::Ok)
        );
        stack.finish(0)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    vm.set_instruction_limit(Some(1000));
    let error = vm
        .execute(
            &compile(b"co=coroutine.create(function()while true do end end);coroutine.resume(co)")
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(run(&mut vm, b"host(co);return 42"), [Value::Number(42.)]);
}
