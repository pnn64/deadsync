use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

#[test]
fn multiple_pending_yields_report_invalid_resumption_without_a_rust_panic() {
    fn yielding(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.native_stack()?.yield_results(1)
    }
    fn selected(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let target = s.to_thread(1).unwrap();
        let value = s.value(2).unwrap();
        let text = s.intern(b"x");
        s.thread_stack(target)?.push(value)?;
        s.thread_stack(target)?.push(text)?;
        s.concatenate_thread(target, 2)?;
        s.finish(0)
    }
    let mut vm = Vm::new();
    let y = vm.new_host_function(yielding, &[]).unwrap();
    vm.set_global(b"yield_tail", y).unwrap();
    let f = vm.new_host_function(selected, &[]).unwrap();
    vm.set_global(b"selected", f).unwrap();
    vm.execute(&compile(br#"
for _,native in ipairs{false,true}do for n=0,3 do
 local co=coroutine.create(function()if native then return yield_tail(11,13,17)end;coroutine.yield(17)end)
 assert(coroutine.resume(co))
 local x=setmetatable({},{__concat=function()coroutine.yield(unpack({'one','two','three'},1,n));return'done'end})
 selected(co,x);collectgarbage()
 local ok,e=coroutine.resume(co);assert(not ok and e:find('invalid coroutine resume state',1,true))
 assert(coroutine.status(co)=='dead'and coroutine.resume(co)==false);collectgarbage()
end end
return 42
"#).unwrap()).unwrap();
    assert_eq!(
        vm.execute(&compile(b"collectgarbage();return 42").unwrap())
            .unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn a_selected_concat_yield_keeps_the_physical_top_below_its_base() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let target = s.to_thread(1).unwrap();
        let operand = s.value(2).unwrap();
        let text = s.intern(b"x");
        s.thread_stack(target)?.push(operand)?;
        s.thread_stack(target)?.push(text)?;
        s.concatenate_thread(target, 2)?;
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.top(), 2);
        let mut t = s.thread_stack(target)?;
        assert_eq!(t.top_offset(), -1);
        assert_eq!(t.top(), 0);
        assert_eq!(t.value(1), None);
        t.push(Value::Nil)?;
        assert_eq!(t.top_offset(), 0);
        t.push(Value::Number(41.))?;
        assert_eq!(t.top_offset(), 1);
        assert_eq!(t.value(1), Some(Value::Number(41.)));
        t.set_top(0)?;
        assert_eq!(t.top_offset(), 0);
        s.finish(0)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.execute(&compile(b"local co=coroutine.create(function()end);assert(coroutine.resume(co));invoke(co,setmetatable({},{__concat=function()coroutine.yield('one');return'done'end}));assert(coroutine.status(co)=='suspended');collectgarbage()").unwrap()).unwrap();
}

#[test]
fn selected_creation_and_concatenation_preserve_the_owner_and_target_prefix() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let target = s.to_thread(1).unwrap();
        let original = s.thread_stack(target)?.value(1);
        let child = s.new_thread_on(target)?;
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.thread_stack(target)?.top(), 2);
        assert_eq!(s.thread_stack(target)?.value(1), original);
        assert!(s.concatenate_thread(target, 5).is_err());
        assert_eq!(s.thread_stack(target)?.top(), 2);
        s.new_thread_on(owner)?;
        let text = s.intern(b"v");
        s.thread_stack(target)?.push(text)?;
        s.thread_stack(target)?.push(Value::Number(17.))?;
        s.concatenate_thread(target, 2)?;
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.thread_stack(target)?.top(), 3);
        s.move_thread_values(target, owner, 1)?;
        s.push(Value::Thread(child))?;
        s.finish(2)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.execute(&compile(b"local co=coroutine.create(function()end);local env={marker=31};debug.setfenv(co,env);local text,child=invoke(co);assert(text=='v17'and debug.getfenv(child)==env and coroutine.status(child)=='dead');collectgarbage()").unwrap()).unwrap();
}

#[test]
fn a_selected_concatenation_quota_error_restores_context_and_remains_fatal() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let owner = s.thread_id();
        let value = s.value(1).unwrap();
        let target = s.new_thread()?;
        let text = s.intern(b"x");
        s.thread_stack(target)?.push(value)?;
        s.thread_stack(target)?.push(text)?;
        let error = s.concatenate_thread(target, 2).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Limit);
        assert_eq!(s.thread_id(), owner);
        assert!(s.push(Value::Nil).is_err());
        Ok(())
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"invoke", f).unwrap();
    vm.set_instruction_limit(Some(200));
    let error = vm
        .execute(
            &compile(b"invoke(setmetatable({},{__concat=function()while true do end end}))")
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"collectgarbage();return 42").unwrap())
            .unwrap(),
        &[Value::Number(42.)]
    );
}
