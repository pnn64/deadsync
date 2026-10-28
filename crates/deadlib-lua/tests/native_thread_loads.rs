use deadlib_lua::{Error, ErrorKind, HostCall, NativeLoadStatus, Value, Vm, compile};

#[test]
fn a_parser_error_after_yield_rejects_a_resume_without_a_frame() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let reader = s.value(1).unwrap();
        let empty = s.value(2).unwrap();
        let child = s.new_thread()?;
        s.thread_stack(child)?.push(empty)?;
        s.resume_thread(child, 0)?;
        let mut visits = 0;
        let status = s.load_reader_thread(child, Some(b"=reader-failure"), |target| {
            visits += 1;
            target.push(reader)?;
            target.push(Value::Number(f64::from(visits)))?;
            target.call_unprotected(1, Some(0))?;
            Ok((visits == 1).then(|| b"return function(".to_vec()))
        })?;
        assert_eq!(status, NativeLoadStatus::SyntaxError);
        assert_eq!(s.thread_status(child).unwrap() as u8, 1);
        assert_eq!(s.resume_thread(child, 0)? as u8, 2);
        s.finish(1)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.execute(&compile(b"local co=invoke(function(i)if i==1 then coroutine.yield(29)end end,function()end);local ok,e=coroutine.resume(co);assert(not ok and e=='cannot resume dead coroutine');collectgarbage();assert(17+29==46)").unwrap()).unwrap();
}

#[test]
fn reader_capacity_checks_preserve_both_stacks_and_same_thread_loading_works() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let child = s.new_thread()?;
        for _ in 0..8000 {
            s.thread_stack(child)?.push(Value::Number(17.))?;
        }
        let mut visited = false;
        assert!(
            s.load_reader_thread(child, None, |_| {
                visited = true;
                Ok(None)
            })
            .is_err()
        );
        assert!(!visited);
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.top(), 1);
        assert_eq!(s.thread_stack(child)?.top(), 8000);
        s.thread_stack(child)?.set_top(1)?;
        let mut bytes = Some(b"return 42".to_vec());
        let status = s.load_reader_thread(child, Some(b"=other"), |target| {
            assert_eq!(target.thread_id(), child);
            assert_eq!(target.value(1), Some(Value::Number(17.)));
            Ok(bytes.take())
        })?;
        assert_eq!(status, NativeLoadStatus::Ok);
        assert_eq!(s.thread_stack(child)?.top(), 2);
        assert_eq!(s.thread_id(), owner);
        let mut bytes = Some(b"return 43".to_vec());
        assert_eq!(
            s.load_reader_thread(owner, None, |_| Ok(bytes.take()))?,
            NativeLoadStatus::Ok
        );
        s.call(0, Some(1))?;
        assert_eq!(s.value(-1), Some(Value::Number(43.)));
        s.finish(1)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return invoke()").unwrap()).unwrap(),
        &[Value::Number(43.)]
    );
}

#[test]
fn reader_instruction_abort_restores_context_and_stays_fatal() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let body = s.value(1).unwrap();
        let child = s.new_thread()?;
        let error = s
            .load_reader_thread(child, None, |target| {
                target.push(body)?;
                target.call_unprotected(0, None)?;
                Ok(None)
            })
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Limit);
        assert_eq!(s.thread_id(), owner);
        assert!(s.push(Value::Nil).is_err());
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
