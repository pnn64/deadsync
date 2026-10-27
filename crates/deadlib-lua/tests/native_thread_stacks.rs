use deadlib_lua::{Error, GcOperation, HostCall, NativeThreadStatus, Value, Vm, compile};

fn api(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    match stack.to_integer(1).unwrap() {
        0 => {
            stack.new_thread()?;
            stack.finish(1)
        }
        1 => {
            let to = stack.to_thread(2).unwrap();
            let count = stack.top() - 2;
            let from = stack.thread_id();
            stack.move_thread_values(from, to, count)?;
            stack.finish(0)
        }
        2 => {
            let co = stack.to_thread(2).unwrap();
            let values = {
                let s = stack.thread_stack(co)?;
                (1..=s.top())
                    .map(|i| s.value(i as isize).unwrap())
                    .collect::<Vec<_>>()
            };
            stack.push_integer(values.len() as isize)?;
            for v in &values {
                stack.push(*v)?;
            }
            stack.finish(values.len() + 1)
        }
        3 => {
            let from = stack.to_thread(2).unwrap();
            let to = stack.to_thread(3).unwrap();
            let n = stack.to_integer(4).unwrap() as usize;
            stack.move_thread_values(from, to, n)?;
            stack.finish(0)
        }
        4 => {
            let co = stack.to_thread(2).unwrap();
            let top = stack.to_integer(3).unwrap() as usize;
            stack.thread_stack(co)?.set_top(top)?;
            stack.finish(0)
        }
        5 => {
            let co = stack.to_thread(2).unwrap();
            let n = stack.thread_stack(co)?.top();
            stack.push_integer(n as isize)?;
            stack.finish(1)
        }
        _ => unreachable!(),
    }
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    let f = vm.new_host_function(api, &[]).unwrap();
    vm.set_global(b"api", f).unwrap();
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn native_empty_threads_accept_entries_prefixes_and_reuse_after_completion() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local co=api(0);assert(coroutine.status(co)=='dead' and api(5,co)==0)
local f=function(a,b)collectgarbage();return a+b end
api(1,co,'prefix',f);local ok,prefix,result=coroutine.resume(co,17,25)
assert(ok and prefix=='prefix' and result==42 and api(5,co)==0)
api(1,co,f);ok,result=coroutine.resume(co,21,21);assert(ok and result==42)
api(1,co,tonumber);ok,result=coroutine.resume(co,'42');assert(ok and result==42)
"#,
    );
}

#[test]
fn inactive_stack_moves_preserve_nil_order_identity_and_shared_heap_roots() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local a,b=api(0),api(0);local object={};local f=function()return 42 end
api(1,a,nil,false,42,'a\0b',object,f,a);api(3,a,b,5)
local n,x,y,z,fn,co=api(2,b);assert(n==5 and x==42 and y=='a\0b' and z==object and fn==f and co==a)
api(3,b,a,0);assert(api(5,a)==2 and api(5,b)==5);api(3,a,a,999999)
local weak=setmetatable({},{__mode='v'})
do local v={};weak[1]=v;api(1,b,v)end
collectgarbage();assert(weak[1]);api(4,b,0);collectgarbage();assert(weak[1]==nil)
local cycle=api(0);weak[2]=cycle;api(1,cycle,cycle);cycle=nil;collectgarbage();assert(weak[2]==nil)
"#,
    );
}

#[test]
fn yielded_stacks_accept_staged_values_and_normal_parents_expose_auxresume_window() {
    let mut vm = vm();
    fn current(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        s.push_thread()?;
        s.finish(1)
    }
    let f = vm.new_host_function(current, &[]).unwrap();
    vm.set_global(b"current", f).unwrap();
    run(
        &mut vm,
        br#"
local main=current()
local co=api(0)
api(1,co,function(a)
 local n,parent=api(2,main);assert(n==1 and parent==coroutine.running())
 local b,c=coroutine.yield(a,nil,false);return b,c
end)
local ok,a,b,c=coroutine.resume(co,17);assert(ok and a==17 and b==nil and c==false and api(5,co)==0)
api(1,co,91,92);collectgarbage();local n,x,y=api(2,co);assert(n==2 and x==91 and y==92)
ok,a,b=coroutine.resume(co,18,19);assert(ok and a==18 and b==19 and api(5,co)==0)
"#,
    );
}

#[test]
fn inactive_primitives_coerce_slots_and_keep_thread_global_pseudo_indexes() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let co = s.new_thread()?;
        let globals = s.value(-10002).unwrap();
        let registry = s.value(-10000).unwrap();
        let replacement = s.intern(b"different");
        {
            let mut t = s.thread_stack(co)?;
            assert_eq!(t.value(-10002), Some(globals));
            assert_eq!(t.value(-10000), Some(registry));
            t.push(Value::Number(17.))?;
            t.push(Value::Nil)?;
            t.push(Value::Boolean(false))?;
            t.push(replacement)?;
            t.insert(1)?;
            t.remove(2)?;
            t.set(2, Value::Number(42.))?;
            assert_eq!(t.to_number(-2), Some(42.));
            assert_eq!(t.to_c_bytes(-2)?, Some(b"42\0".as_slice()));
            assert_eq!(t.object_len(1)?, 9);
            assert_eq!(t.top(), 3);
            assert!(t.to_pointer(1).is_none());
            assert!(!t.push_thread()?);
            assert_eq!(t.to_thread(-1), Some(co));
        }
        s.gc(GcOperation::Collect)?;
        {
            let mut t = s.thread_stack(co)?;
            assert_eq!(t.top(), 4);
            assert_eq!(t.value(2).unwrap().type_name(), "string");
            t.set_top(0)?;
        }
        s.finish(1)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    run(
        &mut vm,
        b"local co=host();assert(type(co)=='thread' and coroutine.status(co)=='dead')",
    );
}

#[test]
fn rejected_moves_and_stale_thread_handles_leave_live_stacks_unchanged() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let a = s.new_thread()?;
        let b = s.new_thread()?;
        {
            let mut t = s.thread_stack(a)?;
            t.push(Value::Number(17.))?;
        }
        assert!(s.move_thread_values(a, b, 2).is_err());
        assert_eq!(s.thread_stack(a)?.top(), 1);
        assert_eq!(s.thread_stack(b)?.top(), 0);
        s.thread_stack(b)?.set_top(8000)?;
        assert!(s.move_thread_values(a, b, 1).is_err());
        assert_eq!(s.thread_stack(a)?.value(1), Some(Value::Number(17.)));
        s.move_thread_values(b, b, usize::MAX)?;
        s.thread_stack(b)?.set_top(0)?;
        s.set_top(0)?;
        s.gc(GcOperation::Collect)?;
        assert!(s.thread_stack(a).is_err());
        assert!(s.thread_stack(b).is_err());
        let current = s.thread_id();
        assert!(s.thread_stack(current).is_err());
        s.finish(0)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    run(&mut vm, b"host()");
}

#[test]
fn failed_stack_windows_retain_diagnostics_and_pop_without_resetting_error_status() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local co=coroutine.create(function()return nil+17 end)
assert(not coroutine.resume(co));local n,a,b,raw,located=api(2,co)
assert(n==4 and a==nil and b==nil and type(raw)=='string' and type(located)=='string')
api(4,co,0);assert(api(5,co)==0 and coroutine.status(co)=='dead')
api(1,co,function()return 42 end);assert(not coroutine.resume(co))
"#,
    );
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let s = call.native_stack()?;
        let co = s.to_thread(1).unwrap();
        assert_eq!(s.thread_status(co), Some(NativeThreadStatus::RuntimeError));
        s.finish(0)
    }
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"check_status", f).unwrap();
    run(&mut vm,b"local co=coroutine.create(function()error(17)end);assert(not coroutine.resume(co));check_status(co)");
}
