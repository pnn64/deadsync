use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
    let function = call.arguments()[0];
    let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
    call.call_and_return(function, &[value])
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    let function = vm.new_host_function(invoke, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn nested_calls_preserve_outer_locals_and_open_results() {
    let mut vm = vm();
    assert_eq!(run(&mut vm,b"local n=17;local function f(x)n=n+x;return n,nil,false,42 end;local a,b,c,d=host(f,25);return a,b,c,d,n,select('#',host(function()end))"),
        [Value::Number(42.),Value::Nil,Value::Boolean(false),Value::Number(42.),Value::Number(42.),Value::Number(0.)]);
    assert_eq!(run(&mut vm,b"return host(math.abs,-17),host(setmetatable({},{__call=function(self,x)return x+1 end}),41)"),
        [Value::Number(17.),Value::Number(42.)]);
}

#[test]
fn rust_can_inspect_results_and_append_across_repeated_calls() {
    fn twice(call: &mut HostCall<'_>) -> Result<(), Error> {
        let function = call.arguments()[0];
        call.push(Value::Number(7.))?;
        let first = call.call(function, &[Value::Number(17.)])?[0];
        call.push(first)?;
        call.call_and_return(function, &[Value::Number(25.)])?;
        let value = call.arguments()[1];
        call.push(value)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(twice, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    assert_eq!(
        run(
            &mut vm,
            b"return host(function(x)collectgarbage();return x,nil,false end,42)"
        ),
        [
            Value::Number(7.),
            Value::Number(17.),
            Value::Number(25.),
            Value::Nil,
            Value::Boolean(false),
            Value::Number(42.)
        ]
    );
}

#[test]
fn nested_runtime_errors_preserve_identity_and_leave_outer_frames_usable() {
    fn recover(call: &mut HostCall<'_>) -> Result<(), Error> {
        let f = call.arguments()[0];
        let value = call.arguments()[1];
        let error = call.call(f, &[value]).unwrap_err();
        call.push(error.value().unwrap())?;
        call.push(call.arguments()[1])
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(recover, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    assert_eq!(run(&mut vm,b"local t={};local localvalue=17;local a,b=host(function(x)local escaped=x;kept=function()return escaped end;error(x,0)end,t);collectgarbage();return a==t,b==t,kept()==t,localvalue"),
        [Value::Boolean(true),Value::Boolean(true),Value::Boolean(true),Value::Number(17.)]);
}

#[test]
fn callbacks_share_the_outer_quota_even_when_rust_ignores_the_error() {
    fn swallow(call: &mut HostCall<'_>) -> Result<(), Error> {
        let function = call.arguments()[0];
        let _ = call.call(function, &[]);
        call.push(Value::Number(42.))
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(swallow, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    vm.set_instruction_limit(Some(100));
    assert_eq!(
        vm.execute(&compile(b"return pcall(host,function()while true do end end)").unwrap())
            .unwrap_err()
            .kind(),
        ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    assert_eq!(
        run(&mut vm, b"return host(function()return 17 end)"),
        [Value::Number(42.)]
    );
}

#[test]
fn coroutine_callers_and_resumed_children_return_to_the_correct_rust_frame() {
    let mut vm = vm();
    assert_eq!(run(&mut vm,b"local co=coroutine.create(function()local a,b=host(function(x)local child=coroutine.create(function()coroutine.yield(x+1);return x+2 end);local ok,a=coroutine.resume(child);local ok2,b=coroutine.resume(child);return a,b end,40);coroutine.yield(a,b);return host(function(x)return x end,43)end);local ok,a,b=coroutine.resume(co);local ok2,c=coroutine.resume(co);return ok,a,b,ok2,c,coroutine.status(co)=='dead'"),
        [Value::Boolean(true),Value::Number(41.),Value::Number(42.),Value::Boolean(true),Value::Number(43.),Value::Boolean(true)]);
}

#[test]
fn retained_child_results_and_errors_survive_collection_and_nested_finalizers() {
    fn retain(call: &mut HostCall<'_>) -> Result<(), Error> {
        let function = call.arguments()[0];
        let result = call.call(function, &[Value::Boolean(false)])?[0];
        call.set_capture(0, result)?;
        let error = call.call(function, &[Value::Boolean(true)]).unwrap_err();
        call.push(error.value().unwrap())?;
        call.call_and_return(function, &[Value::Nil])?;
        call.push(call.captures()[0])
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(retain, &[Value::Nil]).unwrap();
    vm.set_global(b"host", f).unwrap();
    assert_eq!(run(&mut vm,b"local finalized=0;local e,t=host(function(mode)if mode==nil then collectgarbage();return 42 elseif mode then local p=newproxy(true);getmetatable(p).__gc=function()finalized=finalized+1 end;error({kind='error'},0)else return {kind='result'}end end);collectgarbage();return e.kind,t,finalized"),
        [vm.intern(b"error"),Value::Number(42.),Value::Number(1.)]);
    assert_eq!(run(&mut vm,b"local e,n,t=host(function(mode)if mode==nil then collectgarbage();return 42 elseif mode then error({kind='error'},0)else return {kind='result'}end end);return e.kind,n,t.kind"),
        [vm.intern(b"error"),Value::Number(42.),vm.intern(b"result")]);
}

#[test]
fn quota_abort_cleans_child_coroutines_and_allows_a_new_entry() {
    let mut vm = vm();
    vm.set_instruction_limit(Some(200));
    let program = compile(b"owner=coroutine.create(function()return host(function()child=coroutine.create(function()while true do end end);return coroutine.resume(child)end)end);return coroutine.resume(owner)").unwrap();
    assert_eq!(vm.execute(&program).unwrap_err().kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        run(
            &mut vm,
            b"return coroutine.status(owner),coroutine.status(child),host(function()return 42 end)"
        ),
        [vm.intern(b"dead"), vm.intern(b"dead"), Value::Number(42.)]
    );
}

#[test]
fn nested_open_results_obey_the_host_stack_bound() {
    fn many(call: &mut HostCall<'_>) -> Result<(), Error> {
        let count = call.number(0)? as usize;
        for _ in 0..count {
            call.push(Value::Nil)?;
        }
        Ok(())
    }
    let mut vm = vm();
    let function = vm.new_host_function(many, &[]).unwrap();
    vm.set_global(b"many", function).unwrap();
    assert_eq!(
        run(&mut vm, b"return select('#',host(many,7998))"),
        [Value::Number(7998.)]
    );
    assert_eq!(run(&mut vm,b"local ok,e=pcall(host,many,7999);return ok,string.find(e,'too many host function results',1,true)~=nil"),[Value::Boolean(false),Value::Boolean(true)]);
    assert_eq!(
        run(&mut vm, b"return host(function()return 42 end)"),
        [Value::Number(42.)]
    );
}

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

// SAFETY: the fixed oracle supplies at most four arguments. Copying them and
// pushing the status fits a C function's reserved stack. lua_pcall contains Lua
// errors; the remaining operations do not allocate or cross Rust drop state.
unsafe extern "C-unwind" fn reference_probe(
    state: *mut lua_reference::ffi::lua_State,
) -> std::ffi::c_int {
    use lua_reference::ffi;
    unsafe {
        let original = ffi::lua_gettop(state);
        for index in 1..=original {
            ffi::lua_pushvalue(state, index);
        }
        let status = ffi::lua_pcall(state, original - 1, ffi::LUA_MULTRET, 0);
        ffi::lua_pushboolean(state, i32::from(status == ffi::LUA_OK));
        ffi::lua_insert(state, original + 1);
        ffi::lua_gettop(state) - original
    }
}
fn probe(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut args = [Value::Nil; 3];
    let count = call.arguments().len() - 1;
    args[..count].copy_from_slice(&call.arguments()[1..]);
    let function = call.arguments()[0];
    match call.call(function, &args[..count]) {
        Ok(values) => {
            let mut copied = [Value::Nil; 8];
            let count = values.len();
            copied[..count].copy_from_slice(values);
            call.push(Value::Boolean(true))?;
            for value in &copied[..count] {
                call.push(*value)?;
            }
            Ok(())
        }
        Err(error) => {
            call.push(Value::Boolean(false))?;
            call.push(error.value().unwrap())
        }
    }
}
fn compare(source: &str) {
    let mut vm = Vm::new();
    let function = vm.new_host_function(probe, &[]).unwrap();
    vm.set_global(b"probe", function).unwrap();
    let program = deadlib_lua::compile_named(source.as_bytes(), b"=host-reentry").unwrap();
    let values = vm.execute(&program).unwrap().to_vec();
    let actual: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(v) => Scalar::Boolean(v),
            Value::Number(v) => Scalar::Number(v.to_bits()),
            Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            _ => panic!("observe object identity in Lua"),
        })
        .collect();
    // SAFETY: fixed developer programs use debug inspection and this protected
    // C-stack probe; they do not load arbitrary native modules.
    let lua = unsafe { lua_reference::Lua::unsafe_new() };
    let function = unsafe { lua.create_c_function(reference_probe) }.unwrap();
    lua.globals().set("probe", function).unwrap();
    let expected: Vec<_> = lua
        .load(source)
        .set_name("=host-reentry")
        .eval::<lua_reference::MultiValue>()
        .unwrap()
        .into_iter()
        .map(|v| match v {
            lua_reference::Value::Nil => Scalar::Nil,
            lua_reference::Value::Boolean(v) => Scalar::Boolean(v),
            lua_reference::Value::Number(v) => Scalar::Number(v.to_bits()),
            lua_reference::Value::Integer(v) => Scalar::Number((v as f64).to_bits()),
            lua_reference::Value::String(v) => Scalar::Bytes(v.as_bytes().to_vec()),
            _ => panic!("observe reference identity in Lua"),
        })
        .collect();
    assert_eq!(actual, expected, "{source}");
}

#[test]
fn protected_reentry_matches_the_c_api_for_values_errors_and_live_native_slots() {
    for source in [
        "return probe(function()end)",
        "return probe(function(a,b)return a,nil,b,false end,17,42)",
        "return probe(math.abs,-17)",
        "return probe(false),probe(nil),probe(17)",
        "return probe(math.abs,false)",
        "return probe(setmetatable({},{__call=function(self,x)return x+1 end}),41)",
        "local t={};local ok,e=probe(function(x)error(x,0)end,t);collectgarbage();return ok,e==t",
        "return probe(function()error('binary\\000\\255',0)end)",
        "local n=0;local ok,e=xpcall(function()return probe(function()error('child',0)end)end,function()n=n+1 end);return ok,e,n",
        "return probe(function()return debug.getinfo(2,'f').func==probe,debug.getinfo(2,'S').what,getfenv(2)==_G end)",
        "return probe(function(x)local a,b=debug.getlocal(2,2);debug.setlocal(2,2,99);local c,d=debug.getlocal(2,2);return a,b,c,d,x end,17)",
        "local function f(n)if n==0 then return 42 end;return probe(f,n-1)end;return probe(f,3)",
        "local co=coroutine.create(function()return probe(function()return coroutine.yield(17)end)end);return coroutine.resume(co)",
        "local co=coroutine.create(function()return probe(function(x)local child=coroutine.create(function()coroutine.yield(x);return x+1 end);local a,b=coroutine.resume(child);local c,d=coroutine.resume(child);return a,b,c,d end,41)end);return coroutine.resume(co)",
        "local events={};local child=function()return 42 end;debug.sethook(function(event)local f=debug.getinfo(2,'f').func;if f==probe or f==child then events[#events+1]=event..(f==probe and ':host' or ':child') end end,'cr');local ok,n=probe(child);debug.sethook();return ok,n,table.concat(events,',')",
        "local count=0;local ok,a,b=xpcall(function()return probe(function()return load(function()error('reader',0)end)end)end,function(e)count=count+1;return 'outer:'..e end);return ok,a,b,count",
        "local ok,e=probe(function()local p=newproxy(true);getmetatable(p).__gc=function()error('finalizer',0)end;p=nil;collectgarbage()end);return ok,e",
    ] {
        compare(source);
    }
}

#[test]
fn recursive_reentry_reports_lua_stack_overflow_without_losing_the_parent() {
    // Reentry intentionally retains Rust frames. Give the test enough host
    // stack to reach Lua's independent 200-native-call limit in debug builds.
    std::thread::Builder::new().stack_size(16*1024*1024).spawn(|| {
        compare("local count=0;local function f()count=count+1;local ok,e=probe(f);if not ok then return e end;return e end;return probe(f)");
    }).unwrap().join().unwrap();
}

#[test]
#[cfg(panic = "unwind")]
fn a_nested_rust_panic_bypasses_both_boundaries_and_skips_drop_finalizers() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    static FINALIZED: AtomicUsize = AtomicUsize::new(0);
    fn fail(_: &mut HostCall<'_>) -> Result<(), Error> {
        panic!("nested host callback panic")
    }
    fn finalize(_: &mut HostCall<'_>) -> Result<(), Error> {
        FINALIZED.fetch_add(1, Relaxed);
        Ok(())
    }
    let mut vm = vm();
    for (name, callback) in [
        (b"fail".as_slice(), fail as deadlib_lua::HostFunction),
        (b"finalize".as_slice(), finalize),
    ] {
        let function = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, function).unwrap();
    }
    let program = compile(b"keep=newproxy(true);getmetatable(keep).__gc=finalize;pcall(host,function()pcall(fail)end);error('panic was swallowed')").unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = vm.execute(&program);
    }))
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"nested host callback panic")
    );
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(vm))).unwrap();
    assert_eq!(FINALIZED.load(Relaxed), 0);
}
