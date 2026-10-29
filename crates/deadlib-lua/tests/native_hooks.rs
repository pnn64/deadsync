use deadlib_lua::{Error, GcOperation, HostCall, NativeHook, Value, Vm, compile};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
    let function = call.captures()[0];
    let mut s = call.native_stack()?;
    let thread = s.to_thread(1).unwrap_or(s.thread_id());
    let mask = s.to_integer(2).unwrap_or(7) as i32;
    let count = s.to_integer(3).unwrap_or(0) as i32;
    s.set_hook(thread, NativeHook::Host(function), mask, count)?;
    s.finish(0)
}

fn setup(callback: deadlib_lua::HostFunction) -> Vm {
    let mut vm = Vm::new();
    let hook = vm.new_host_function(callback, &[]).unwrap();
    let set = vm.new_host_function(install, &[hook]).unwrap();
    vm.set_global(b"install", set).unwrap();
    vm
}

#[test]
fn controls_preserve_signed_counts_narrow_masks_and_validate_callback_types() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let function = call.arguments()[0];
        let mut s = call.native_stack()?;
        let co = s.new_thread()?;
        for (mask, count) in [(256, -3), (-1, 0), (8, -17), (0, 29)] {
            s.set_hook(co, NativeHook::Host(function), mask, count)?;
            let h = s.hook(co)?;
            assert_eq!(h.count, count);
            assert_eq!(h.mask, mask as u8);
            assert_eq!(
                h.function,
                if mask == 0 {
                    NativeHook::None
                } else {
                    NativeHook::Host(function)
                }
            );
        }
        assert!(s.set_hook(co, NativeHook::Host(Value::Nil), 1, 0).is_err());
        assert!(
            s.set_hook(co, NativeHook::Host(s.value(2).unwrap()), 1, 0)
                .is_err()
        );
        s.set_top(2)?;
        s.gc(GcOperation::Collect)?;
        assert!(s.hook(co).is_err());
        s.finish(0)
    }
    let mut vm = setup(|_| Ok(()));
    let h = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"controls", h).unwrap();
    vm.execute(&compile(b"controls(install,function()end)").unwrap())
        .unwrap();
}

#[test]
fn native_callbacks_have_no_visible_call_frame_and_can_mutate_parameters() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        let current = s.get_stack(s.thread_id(), 0)?.unwrap();
        if e.event != 4 {
            assert_eq!(e.frame, current);
        }
        let info = s.get_info(e.frame, b"S")?;
        if e.event == 0 && info.what == Some(b"Lua\0".as_slice()) {
            assert_eq!(s.value(1), Some(Value::Number(17.)));
            s.set(1, Value::Number(29.))?;
            s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        }
        Ok(())
    }
    let mut vm = setup(callback);
    let result = vm
        .execute(&compile(b"local function f(x)return x end;install(nil,1);return f(17)").unwrap())
        .unwrap();
    assert_eq!(result, &[Value::Number(29.)]);
}

#[test]
fn hook_pops_retain_physical_values_without_retaining_collector_roots() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mode = *call.function_data::<u8>().unwrap();
        let mut s = call.native_stack()?;
        s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        s.set_top(1)?;
        if mode == 1 {
            s.push_integer(71)?;
            s.set_top(1)?;
        } else if mode == 2 {
            s.gc(GcOperation::Collect)?;
        }
        Ok(())
    }
    for (mode, tail) in [
        (0u8, [Value::Number(29.), Value::Number(42.)]),
        (1, [Value::Number(71.), Value::Number(42.)]),
        (2, [Value::Nil, Value::Nil]),
    ] {
        let mut vm = Vm::new();
        let hook = vm.new_host_function_with_data(callback, &[], mode).unwrap();
        let set = vm.new_host_function(install, &[hook]).unwrap();
        vm.set_global(b"install", set).unwrap();
        assert_eq!(
            vm.execute(
                &compile(
                    b"local f=function(x,y,z)return x,y,z end;install(nil,1);return f(17,29,42)"
                )
                .unwrap()
            )
            .unwrap(),
            &[Value::Number(17.), tail[0], tail[1]]
        );
    }
}

#[test]
fn hook_errors_retain_identity_and_delivery_recovers_after_protected_unwind() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        if e.event == 0 && s.get_info(e.frame, b"S")?.what == Some(b"Lua\0".as_slice()) {
            s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
            return Err(s.raise(s.value(1).unwrap()));
        }
        Ok(())
    }
    let mut vm = setup(callback);
    vm.execute(
        &compile(
            br#"
local token={};local function f(x) return x end
local function run() install(nil,1);return f(token) end
local ok,e=pcall(run);assert(not ok and e==token)
assert(f(17)==17)
ok,e=pcall(run);assert(not ok and e==token)
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn external_return_hook_errors_escape_completed_protected_builtins() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let event = call.hook_event().unwrap();
        let target = call.captures()[0];
        let token = call.captures()[1];
        if event.event != 1 {
            return Ok(());
        }
        let mut stack = call.native_stack()?;
        let top = stack.top();
        stack.get_info(event.frame, b"f")?;
        let matches = stack.value(-1) == Some(target);
        stack.set_top(top)?;
        if matches {
            stack.set_hook(stack.thread_id(), NativeHook::None, 0, 0)?;
            stack.gc(GcOperation::Collect)?;
            return Err(stack.raise(token));
        }
        Ok(())
    }
    for (target, action) in [
        (b"pcall".as_slice(), "pcall(function()return 17 end)"),
        (
            b"xpcall".as_slice(),
            "xpcall(function()return 17 end,function()return 'expired' end)",
        ),
        (
            b"load".as_slice(),
            "load((function()local n=0;return function()n=n+1;if n==1 then return 'return 17'end end end)())",
        ),
    ] {
        let mut vm = Vm::new();
        let token = vm.execute(&compile(b"return {}").unwrap()).unwrap()[0];
        vm.set_global(b"token", token).unwrap();
        let target = vm.global(target);
        let hook = vm.new_host_function(callback, &[target, token]).unwrap();
        let set = vm.new_host_function(install, &[hook]).unwrap();
        vm.set_global(b"install", set).unwrap();
        let source = format!("install(nil,2);return pcall(function()return {action} end)");
        assert_eq!(
            vm.execute(&compile(source.as_bytes()).unwrap()).unwrap(),
            &[Value::Boolean(false), token]
        );
    }
}

#[test]
fn external_hooks_survive_collection_and_are_inherited_without_lua_registry_entries() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        if e.event == 0 && s.get_info(e.frame, b"S")?.what == Some(b"Lua\0".as_slice()) {
            s.gc(GcOperation::Collect)?;
            s.set(1, Value::Number(42.))?;
        }
        Ok(())
    }
    let mut vm = setup(callback);
    vm.execute(
        &compile(
            br#"
install(nil,1)
local co=coroutine.create(function(x)return x end)
assert(debug.gethook(co)=='external hook')
collectgarbage();local ok,n=coroutine.resume(co,17);assert(ok and n==42)
debug.sethook();assert(debug.gethook()==nil)
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn count_callbacks_do_not_reenter_and_restore_pushed_stack_values() {
    type Events = Arc<Mutex<usize>>;
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let events = call.function_data::<Events>().unwrap().clone();
        let e = call.hook_event().unwrap();
        assert_eq!(e.event, 3);
        *events.lock().unwrap() += 1;
        let mut s = call.native_stack()?;
        let f = s.value(1).unwrap();
        if matches!(f, Value::Function(_)) {
            s.push(f)?;
            s.push_integer(17)?;
            s.call_unprotected(1, Some(1))?;
            assert_eq!(s.value(-1), Some(Value::Number(18.)));
        }
        s.push_integer(99)?;
        s.gc(GcOperation::Collect)?;
        Ok(())
    }
    let events = Events::default();
    let mut vm = Vm::new();
    let hook = vm
        .new_host_function_with_data(callback, &[], events.clone())
        .unwrap();
    let set = vm.new_host_function(install, &[hook]).unwrap();
    vm.set_global(b"install", set).unwrap();
    vm.execute(
        &compile(
            br#"
local f=function(x)return x+1 end
install(nil,8,1)
local sum=0;for i=1,20 do sum=sum+i end
debug.sethook();assert(sum==210)
"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(*events.lock().unwrap() > 20);
}

#[test]
fn native_return_hook_uses_the_original_native_arguments_and_selected_results() {
    fn target(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        s.push_integer(29)?;
        s.finish(1)
    }
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let e = call.hook_event().unwrap();
        let mut s = call.native_stack()?;
        if e.event == 1 && s.value(1) == Some(Value::Number(17.)) {
            assert_eq!(s.top(), 2);
            assert_eq!(s.value(2), Some(Value::Number(29.)));
            s.set(2, Value::Number(42.))?;
            s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        }
        Ok(())
    }
    let mut vm = setup(callback);
    let f = vm.new_host_function(target, &[]).unwrap();
    vm.set_global(b"target", f).unwrap();
    assert_eq!(
        vm.execute(&compile(b"install(nil,2);return target(17)").unwrap())
            .unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn line_hooks_yield_without_suspending_rust_and_resume_the_pending_instruction() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let e = call.hook_event().unwrap();
        assert_eq!(e.event, 2);
        let mut s = call.native_stack()?;
        s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        s.yield_results(0)
    }
    let mut vm = setup(callback);
    vm.execute(
        &compile(
            br#"
local co=coroutine.create(function(x)local y=x+1;return y end)
install(co,4)
assert(select('#',coroutine.resume(co,17))==1)
assert(coroutine.status(co)=='suspended')
local ok,n=coroutine.resume(co,99);assert(ok and n==18)
assert(coroutine.status(co)=='dead')
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn pushed_hook_temporaries_alias_lua_and_native_debug_locals() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let event = call.hook_event().unwrap();
        if event.event == 4 {
            return Ok(());
        }
        let mut s = call.native_stack()?;
        let slot = s.top() + 1;
        s.create_table(0, 0)?;
        let table = s.value(-1).unwrap();
        let name = s.get_local(event.frame, slot as i32)?.unwrap();
        assert_eq!(s.debug_bytes(name), Some(b"(*temporary)\0".as_slice()));
        assert_eq!(s.value(-1), Some(table));
        s.push_integer(29)?;
        s.set_local(event.frame, slot as i32)?;
        assert_eq!(s.value(slot as isize), Some(Value::Number(29.)));
        s.gc(GcOperation::Collect)?;
        Ok(())
    }
    let mut vm = setup(callback);
    vm.execute(
        &compile(
            br#"
install(nil,15,1)
local function f(x)return math.abs(x)+1 end
assert(f(-17)==18)
debug.sethook()
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn active_hook_binding_survives_disable_and_collection_then_releases() {
    struct Binding(Arc<AtomicUsize>);
    impl Drop for Binding {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let drops = call.function_data::<Binding>().unwrap().0.clone();
        let mut s = call.native_stack()?;
        s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        s.gc(GcOperation::Collect)?;
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        Ok(())
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut vm = Vm::new();
    let hook = vm
        .new_host_function_with_data(callback, &[], Binding(drops.clone()))
        .unwrap();
    let set = vm.new_host_function(install, &[hook]).unwrap();
    vm.set_global(b"install", set).unwrap();
    vm.execute(
        &compile(
            br#"
local co=coroutine.create(function()return 42 end)
install(co,1);install=nil;collectgarbage()
local ok,n=coroutine.resume(co);assert(ok and n==42)
co=nil;collectgarbage();collectgarbage()
"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn hook_reentry_shares_quota_and_recovers_after_abort() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let f = call.captures()[0];
        let mut s = call.native_stack()?;
        s.push(f)?;
        // Ignoring a resource failure cannot make the callback succeed.
        let _ = s.call(0, None);
        Ok(())
    }
    let mut vm = Vm::new();
    let program = compile(b"while true do end").unwrap();
    let f = Value::Function(vm.load_program(&program));
    let hook = vm.new_host_function(callback, &[f]).unwrap();
    let set = vm.new_host_function(install, &[hook]).unwrap();
    vm.set_global(b"install", set).unwrap();
    vm.set_instruction_limit(Some(1000));
    let error = vm
        .execute(&compile(b"install(nil,8,1);local x=17;return x+1").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    vm.set_instruction_limit(None);
    // Disable before starting another Lua entry, which would itself be hooked.
    let clear = vm
        .new_host_function(
            |call| {
                let mut s = call.native_stack()?;
                s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
                s.finish(0)
            },
            &[],
        )
        .unwrap();
    vm.call(clear, &[]).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn hook_resume_arguments_remain_visible_and_rooted_until_stack_normalization() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let calls = call.function_data::<Arc<AtomicUsize>>().unwrap().clone();
        let mut s = call.native_stack()?;
        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            return s.yield_results(0);
        }
        assert_eq!(s.top(), 3);
        s.gc(GcOperation::Collect)?;
        s.get_field(3, b"kept")?;
        assert_eq!(s.to_integer(-1), Some(42));
        s.set_hook(s.thread_id(), NativeHook::None, 0, 0)?;
        Ok(())
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let mut vm = Vm::new();
    let hook = vm
        .new_host_function_with_data(callback, &[], calls.clone())
        .unwrap();
    let set = vm.new_host_function(install, &[hook]).unwrap();
    vm.set_global(b"install", set).unwrap();
    vm.execute(
        &compile(
            br#"
local co=coroutine.create(function(x)local y=x+1;return y end)
install(co,4)
assert(select('#',coroutine.resume(co,17))==1)
local ok,n=coroutine.resume(co,{kept=42});assert(ok and n==18)
"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
