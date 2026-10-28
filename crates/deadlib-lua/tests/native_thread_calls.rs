use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

#[test]
fn an_api_call_between_yield_and_resume_restores_the_pending_lua_register_window() {
    fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let thread = s.to_thread(1).unwrap();
        let function = s.value(2).unwrap();
        let protected = s.value(3) == Some(Value::Boolean(true));
        s.thread_stack(thread)?.push(function)?;
        if protected {
            assert_eq!(s.protected_call_thread(thread, 0, None, None)? as u8, 0);
        } else {
            s.call_unprotected_thread(thread, 0, None)?;
        }
        s.finish(0)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(invoke, &[]).unwrap();
    vm.set_global(b"invoke", function).unwrap();
    vm.execute(&compile(br#"
for n=0,3 do for _,protected in ipairs{false,true}do
 local co=coroutine.create(function()end);assert(coroutine.resume(co))
 local saved
 invoke(co,function()
  local held={value=71};saved=function()return held.value end
  local a,b=coroutine.yield(unpack({17,19,23},1,n))
  local c,d,e,f,g,h=29,31,37,41,43,47
  return held.value,a,b,c+d+e+f+g+h
 end,protected)
 invoke(co,function()collectgarbage();return 53,59,61 end,false)
 collectgarbage();assert(saved()==71)
 local ok,a,b,c,d=coroutine.resume(co,67,73)
 assert(ok and a==71 and b==67 and c==73 and d==228 and coroutine.status(co)=='dead')
 local weak=setmetatable({co},{__mode='v'});co=nil;collectgarbage();assert(not weak[1]and saved()==71)
end end
"#).unwrap()).unwrap();
}

#[test]
fn target_call_validation_preserves_both_native_windows() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let child = s.new_thread()?;
        assert!(s.protected_call_thread(child, 0, None, None).is_err());
        let function = s.value(1).unwrap();
        s.thread_stack(child)?.push(function)?;
        assert!(s.protected_call_thread(child, 1, None, None).is_err());
        assert!(s.protected_call_thread(child, 0, Some(8001), None).is_err());
        assert!(s.protected_call_thread(child, 0, None, Some(2)).is_err());
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.top(), 2);
        let t = s.thread_stack(child)?;
        assert_eq!(t.top(), 1);
        assert_eq!(t.value(1), Some(function));
        let status = s.protected_call_thread(child, 0, None, None)?;
        assert_eq!(status as u8, 0);
        let t = s.thread_stack(child)?;
        assert_eq!(t.top(), 1);
        assert_eq!(t.value(1), Some(Value::Number(42.)));
        s.finish(0)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.execute(&compile(b"invoke(function()collectgarbage();return 42 end)").unwrap())
        .unwrap();
}

#[test]
fn target_instruction_abort_restores_context_and_remains_sticky() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let function = s.value(1).unwrap();
        let child = s.new_thread()?;
        s.thread_stack(child)?.push(function)?;
        let error = s.protected_call_thread(child, 0, None, None).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Limit);
        assert_eq!(s.thread_id(), owner);
        assert!(s.push(Value::Nil).is_err());
        // Ignoring the error does not turn the interrupted call into success.
        Ok(())
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.set_instruction_limit(Some(200));
    let error=vm.execute(&compile(b"invoke(function()local co=coroutine.create(function()while true do end end);coroutine.resume(co)end)").unwrap()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"collectgarbage();return 42").unwrap())
            .unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn a_completed_target_yield_keeps_the_host_usable() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let yielding = s.value(1).unwrap();
        let empty = s.value(2).unwrap();
        let child = s.new_thread()?;
        s.thread_stack(child)?.push(empty)?;
        s.resume_thread(child, 0)?;
        s.thread_stack(child)?.push(yielding)?;
        assert_eq!(s.protected_call_thread(child, 0, None, None)? as u8, 0);
        assert_eq!(s.thread_status(child).unwrap() as u8, 1);
        assert_eq!(s.thread_stack(child)?.value(1), Some(Value::Number(17.)));
        s.thread_stack(child)?.set_top(0)?;
        s.thread_stack(child)?.push(Value::Number(41.))?;
        assert_eq!(s.resume_thread(child, 1)? as u8, 0);
        assert_eq!(s.thread_stack(child)?.top(), 0);
        assert_eq!(s.thread_id(), owner);
        s.push(Value::Boolean(true))?;
        s.finish(1)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.execute(&compile(b"local called=false;assert(invoke(function()called=true;coroutine.yield(17)end,function()end));assert(called);collectgarbage()").unwrap()).unwrap();
}
