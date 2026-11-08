use deadlib_lua::{Error, ErrorKind, GcOperation, HostCall, Value, Vm, compile};

#[test]
fn selected_collection_needs_no_spare_public_slot_and_restores_its_owner() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let target = s.to_thread(1).unwrap();
        for _ in 0..8000 {
            s.thread_stack(target)?.push(Value::Number(17.))?;
        }
        assert_eq!(s.gc_thread(target, GcOperation::SetPause(139))?, 200);
        assert_eq!(s.gc_thread(target, GcOperation::Collect)?, 0);
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.top(), 1);
        assert_eq!(s.thread_stack(target)?.top(), 8000);
        assert_eq!(s.thread_stack(target)?.value(-1), Some(Value::Number(17.)));
        assert_eq!(s.gc_thread(owner, GcOperation::SetPause(200))?, 139);
        s.finish(0)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.execute(&compile(b"collectgarbage();collectgarbage('stop');local co=coroutine.create(function()end);assert(coroutine.resume(co));local visits=0;do local p=newproxy(true);getmetatable(p).__gc=function()assert(coroutine.running()==co);visits=visits+1 end end;invoke(co);assert(visits==1);collectgarbage()").unwrap()).unwrap();
}

#[test]
fn a_selected_finalizer_quota_error_restores_context_and_stays_fatal() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let target = s.to_thread(1).unwrap();
        let error = s.gc_thread(target, GcOperation::Collect).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Limit);
        assert_eq!(s.thread_id(), owner);
        assert!(s.push(Value::Nil).is_err());
        Ok(())
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.set_instruction_limit(Some(300));
    let e=vm.execute(&compile(b"collectgarbage();collectgarbage('stop');local co=coroutine.create(function()end);assert(coroutine.resume(co));do local p=newproxy(true);getmetatable(p).__gc=function()while true do end end end;invoke(co)").unwrap()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"collectgarbage();return 42").unwrap())
            .unwrap(),
        &[Value::Number(42.)]
    );
}
