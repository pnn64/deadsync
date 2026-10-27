use deadlib_lua::{Error, ErrorKind, GcOperation, HostCall, Value, Vm, compile};

fn install(vm: &mut Vm, callback: fn(&mut HostCall<'_>) -> Result<(), Error>) {
    let function = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn collector_controls_and_count_parts_preserve_the_native_window() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let original = stack.value(1);
        assert_eq!(stack.gc(GcOperation::Stop)?, 0);
        let whole = stack.gc(GcOperation::CountKilobytes)?;
        let part = stack.gc(GcOperation::CountRemainder)?;
        assert!(whole >= 0 && (0..1024).contains(&part));
        assert_eq!(stack.gc(GcOperation::CountKilobytes)?, whole);
        assert_eq!(stack.gc(GcOperation::CountRemainder)?, part);
        let pause = stack.gc(GcOperation::SetPause(200))?;
        let multiplier = stack.gc(GcOperation::SetStepMultiplier(200))?;
        for value in [i32::MIN, -1, 0, 17, i32::MAX, 200] {
            let old = stack.gc(GcOperation::SetPause(value))?;
            assert_eq!(old, 200);
            assert_eq!(stack.gc(GcOperation::SetPause(200))?, value);
            let old = stack.gc(GcOperation::SetStepMultiplier(value))?;
            assert_eq!(old, 200);
            assert_eq!(stack.gc(GcOperation::SetStepMultiplier(200))?, value);
        }
        assert_eq!(stack.gc(GcOperation::Restart)?, 0);
        stack.gc(GcOperation::SetPause(pause))?;
        stack.gc(GcOperation::SetStepMultiplier(multiplier))?;
        assert_eq!(stack.top(), 1);
        assert_eq!(stack.value(1), original);
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm, b"local marker={};assert(host(marker)==marker)");
}

#[test]
fn finalizers_reenter_and_inspect_mutate_the_real_native_caller() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert_eq!(stack.gc(GcOperation::Collect)?, 0);
        assert_eq!(stack.top(), 2);
        assert_eq!(stack.value(1), stack.value(2));
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(
        &mut vm,
        br#"
collectgarbage('stop')
local original,replacement={},{}
do local p=newproxy(true);getmetatable(p).__gc=function()
 assert(debug.getinfo(2,'f').func==host)
 assert(select(2,debug.getlocal(2,1))==original)
 assert(select(2,debug.getlocal(2,2))==replacement)
 assert(debug.getlocal(2,3)==nil)
 assert(debug.setlocal(2,1,replacement))
 local co=coroutine.create(function()coroutine.yield(17);return 42 end)
 local ok,value=coroutine.resume(co);assert(ok and value==17)
 ok,value=coroutine.resume(co);assert(ok and value==42)
 collectgarbage()
end end
assert(host(original,replacement)==replacement)
"#,
    );
}

#[test]
fn finalizer_errors_keep_fault_frames_and_make_later_api_operations_sticky() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let error = stack.gc(GcOperation::Collect).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Runtime);
        assert_eq!(
            stack.gc(GcOperation::Stop).unwrap_err().value(),
            error.value()
        );
        assert_eq!(stack.push(Value::Nil).unwrap_err().value(), error.value());
        Ok(())
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(
        &mut vm,
        br#"
collectgarbage('stop')
local marker,argument={},{}
do local p=newproxy(true);getmetatable(p).__gc=function()
 local retained=marker
 error(retained)
end end
local observed=false
local ok,e=xpcall(function()host(argument)end,function(e)
 for level=2,12 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==host then observed=select(2,debug.getlocal(level,1))==argument end
 end
 return e
end)
assert(not ok and e==marker and observed)
collectgarbage();recovered=true
"#,
    );
    assert_eq!(vm.global(b"recovered"), Value::Boolean(true));
}

#[test]
fn native_collection_uses_the_outer_instruction_limit_and_recovers() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.gc(GcOperation::Collect)?;
        stack.finish(0)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    vm.set_instruction_limit(Some(1000));
    let error=vm.execute(&compile(b"collectgarbage('stop');do local p=newproxy(true);getmetatable(p).__gc=function()while true do end end end;host()").unwrap()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(run(&mut vm, b"return 42"), [Value::Number(42.)]);
    vm.collect_garbage().unwrap();
}

#[test]
fn discarded_native_arguments_are_not_kept_by_finalizer_scratch_storage() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.set_top(0)?;
        stack.gc(GcOperation::Collect)?;
        assert_eq!(stack.top(), 0);
        stack.finish(0)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(
        &mut vm,
        br#"
collectgarbage('stop')
local weak=setmetatable({},{__mode='v'})
local finalized=false
do local p=newproxy(true);getmetatable(p).__gc=function()
 assert(weak[1]==nil);finalized=true
end end
local function argument()local t={};weak[1]=t;return t end
host(argument())
assert(weak[1]==nil and finalized)
"#,
    );
}

#[test]
fn explicit_steps_run_finalizers_and_restore_native_stack_results() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let old = stack.gc(GcOperation::SetStepMultiplier(0))?;
        assert_eq!(stack.gc(GcOperation::Step(0))?, 1);
        stack.gc(GcOperation::SetStepMultiplier(old))?;
        assert_eq!(stack.top(), 1);
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,b"collectgarbage('stop');local finalized=false;do local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end end;local marker={};assert(host(marker)==marker and finalized)");
}
