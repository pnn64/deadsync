use deadlib_lua::{Error, GcOperation, HostCall, NativeHook, Value, Vm, compile};

fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
    let hook = call.captures()[0];
    let mut s = call.native_stack()?;
    let thread = s.to_thread(1).unwrap_or(s.thread_id());
    let mask = s.to_integer(2).unwrap_or(3) as i32;
    s.set_hook(thread, NativeHook::Host(hook), mask, 0)?;
    s.finish(0)
}

fn target(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut s = call.native_stack()?;
    s.push(s.value(-10003).unwrap())?;
    s.push(s.value(-10001).unwrap())?;
    s.finish(2)
}

#[test]
fn external_hook_pseudo_indexes_use_the_interrupted_native_closure() {
    fn hook(call: &mut HostCall<'_>) -> Result<(), Error> {
        // The Rust callback's own binding remains distinct from C API context.
        assert_eq!(call.captures(), &[Value::Number(599.)]);
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        s.get_info(e.frame, b"f")?;
        s.get_environment(-1)?;
        let env = s.value(-1).unwrap();
        s.get_field(-1, b"context")?;
        if !s.to_boolean(-1) {
            return Ok(());
        }
        assert_eq!(s.value(-10001), Some(env));
        assert_eq!(s.value(-10004), None);
        if e.event == 0 {
            assert_eq!(s.value(-10003), Some(Value::Number(17.)));
            s.set(-10003, Value::Number(29.))?;
        } else {
            assert_eq!(s.value(-10003), Some(Value::Number(29.)));
        }
        s.gc(GcOperation::Collect)?;
        Ok(())
    }
    let mut vm = Vm::new();
    let h = vm.new_host_function(hook, &[Value::Number(599.)]).unwrap();
    let set = vm.new_host_function(install, &[h]).unwrap();
    vm.set_global(b"install", set).unwrap();
    let f = vm.new_host_function(target, &[Value::Number(17.)]).unwrap();
    vm.set_global(b"target", f).unwrap();
    vm.execute(
        &compile(
            br#"
local env={context=true};debug.setfenv(target,env)
debug.setfenv(install,{context=false})
install()
local value,actual=target();assert(value==29 and actual==env)
debug.sethook()
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn hook_allocations_and_environment_writes_follow_the_interrupted_lua_function() {
    fn hook(call: &mut HostCall<'_>) -> Result<(), Error> {
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        s.get_info(e.frame, b"f")?;
        s.get_environment(-1)?;
        let env = s.value(-1).unwrap();
        s.get_field(-1, b"context")?;
        if !s.to_boolean(-1) {
            return Ok(());
        }
        assert_eq!(s.value(-10001), Some(env));
        s.new_userdata(8)?;
        s.get_environment(-1)?;
        assert_eq!(s.value(-1), Some(env));
        s.push_closure(|_| Ok(()), 0, ())?;
        s.get_environment(-1)?;
        assert_eq!(s.value(-1), Some(env));
        s.push(env)?;
        s.get_field(-1, b"replacement")?;
        let replacement = s.value(-1).unwrap();
        s.set(-10001, replacement)?;
        s.gc(GcOperation::Collect)?;
        Ok(())
    }
    let mut vm = Vm::new();
    let h = vm.new_host_function(hook, &[]).unwrap();
    let set = vm.new_host_function(install, &[h]).unwrap();
    vm.set_global(b"install", set).unwrap();
    vm.execute(
        &compile(
            br#"
local f=assert(loadstring('return answer'))
local after={answer=42};setfenv(f,{context=true,answer=17,replacement=after})
debug.setfenv(install,{})
install(nil,1)
assert(f()==42 and getfenv(f)==after)
debug.sethook()
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn child_native_access_mutates_a_waiting_hooks_real_closure() {
    fn inspector(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let parent = s.to_thread(1).unwrap();
        let old = s.value(2).unwrap();
        let replacement = s.value(3).unwrap();
        let mut p = s.thread_stack(parent)?;
        assert_eq!(p.value(-10001), Some(old));
        assert_eq!(p.value(-10003), Some(Value::Number(17.)));
        p.set(-10001, replacement)?;
        p.set(-10003, Value::Number(31.))?;
        s.gc(GcOperation::Collect)?;
        s.finish(0)
    }
    fn hook(call: &mut HostCall<'_>) -> Result<(), Error> {
        let inspector = call.captures()[0];
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        s.get_info(e.frame, b"f")?;
        s.get_environment(-1)?;
        let env = s.value(-1).unwrap();
        s.get_field(-1, b"context")?;
        if !s.to_boolean(-1) {
            return Ok(());
        }
        s.push(env)?;
        s.get_field(-1, b"replacement")?;
        let replacement = s.value(-1).unwrap();
        let parent = s.thread_id();
        let child = s.new_thread()?;
        let mut c = s.thread_stack(child)?;
        for value in [inspector, Value::Thread(parent), env, replacement] {
            c.push(value)?;
        }
        s.resume_thread(child, 3)?;
        Ok(())
    }
    let mut vm = Vm::new();
    let inspector = vm.new_host_function(inspector, &[]).unwrap();
    let h = vm.new_host_function(hook, &[inspector]).unwrap();
    let set = vm.new_host_function(install, &[h]).unwrap();
    vm.set_global(b"install", set).unwrap();
    let f = vm.new_host_function(target, &[Value::Number(17.)]).unwrap();
    vm.set_global(b"target", f).unwrap();
    vm.execute(
        &compile(
            br#"
local after={};debug.setfenv(target,{context=true,replacement=after})
install(nil,1)
local value,env=target();assert(value==31 and env==after)
debug.sethook()
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn suspended_lua_hook_context_keeps_its_function_environment() {
    fn yielding(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        s.yield_results(0)
    }
    fn inspect(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let co = s.to_thread(1).unwrap();
        let old = s.value(2).unwrap();
        let new = s.value(3).unwrap();
        let mut c = s.thread_stack(co)?;
        assert_eq!(c.value(-10001), Some(old));
        assert_eq!(c.value(-10003), None);
        c.set(-10001, new)?;
        s.gc(GcOperation::Collect)?;
        s.finish(0)
    }
    let mut vm = Vm::new();
    let h = vm.new_host_function(yielding, &[]).unwrap();
    let set = vm.new_host_function(install, &[h]).unwrap();
    vm.set_global(b"install", set).unwrap();
    let inspect = vm.new_host_function(inspect, &[]).unwrap();
    vm.set_global(b"inspect", inspect).unwrap();
    vm.execute(
        &compile(
            br#"
local f=assert(loadstring('return answer'))
local before={answer=17};local after={answer=42};setfenv(f,before)
local co=coroutine.create(f);debug.setfenv(co,{different='thread globals'})
install(co,4)
assert(coroutine.resume(co))
inspect(co,before,after)
local ok,n=coroutine.resume(co);assert(ok and n==42 and getfenv(f)==after)
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn empty_native_thread_rejects_replacing_a_nonexistent_calling_environment() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let co = s.new_thread()?;
        s.create_table(0, 0)?;
        let env = s.value(-1).unwrap();
        let mut c = s.thread_stack(co)?;
        let globals = c.value(-10002);
        assert_eq!(
            c.set(-10001, env).unwrap_err().message(),
            "no calling environment"
        );
        assert_eq!(c.value(-10002), globals);
        assert_eq!(c.top(), 0);
        s.finish(0)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.call(f, &[]).unwrap();
}
