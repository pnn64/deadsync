use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn echo(call: &mut HostCall<'_>) -> Result<(), Error> {
    for index in 0..call.arguments().len() {
        let value = call.arguments()[index];
        call.push(value)?;
    }
    Ok(())
}
fn add(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.number(0)? + call.number(1)?;
    call.push(Value::Number(value))
}
fn execute(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn borrowed_arguments_stay_intact_while_multiple_results_are_written() {
    let mut vm = Vm::new();
    let function = vm.new_host_function(echo, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    assert_eq!(
        vm.call(
            function,
            &[Value::Number(17.), Value::Nil, Value::Boolean(false)]
        )
        .unwrap(),
        &[Value::Number(17.), Value::Nil, Value::Boolean(false)]
    );
    assert_eq!(execute(&mut vm,b"local t={};local a,b,c,d=host(t,nil,false,17);return a==t,b==nil,c,d,select('#',host()),select('#',host(nil,nil))"),
        [Value::Boolean(true),Value::Boolean(true),Value::Boolean(false),Value::Number(17.),Value::Number(0.),Value::Number(2.)]);
}

#[test]
fn registered_native_functions_support_numeric_strings_methods_and_tail_calls() {
    let mut vm = Vm::new();
    let function = vm.new_host_function(add, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    assert_eq!(execute(&mut vm,b"local f=host;local function tail(...)return f(...)end;return host('17',25),tail(19,'23'),type(host)" )[..2],
        [Value::Number(42.),Value::Number(42.)]);
    let Value::String(kind) = execute(&mut vm, b"return type(host)")[0] else {
        panic!()
    };
    assert_eq!(vm.string_bytes(kind), Some(b"function".as_slice()));
    assert_eq!(execute(&mut vm,b"local t=setmetatable({},{__call=host});local ok,e=pcall(t,17);return ok,type(e)=='string',string.find(e,'number expected',1,true)~=nil"),
        [Value::Boolean(false),Value::Boolean(true),Value::Boolean(true)]);
    let values=execute(&mut vm,b"local ok,e=pcall(function()return host(17,false)end);return ok,string.find(e,\"bad argument #2 to 'host'\",1,true)~=nil");
    assert_eq!(values, [Value::Boolean(false), Value::Boolean(true)]);
}

#[test]
fn captures_keep_identity_and_mutations_across_gc_and_coroutine_calls() {
    fn count(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Number(n) = call.captures()[0] else {
            panic!()
        };
        call.set_capture(0, Value::Number(n + 1.))?;
        call.push(Value::Number(n + 1.))?;
        let value = call.captures()[1];
        call.push(value)
    }
    let mut vm = Vm::new();
    let table = Value::Table(vm.new_table());
    let host = vm
        .new_host_function(count, &[Value::Number(0.), table])
        .unwrap();
    vm.set_global(b"host", host).unwrap();
    vm.set_global(b"original", table).unwrap();
    assert_eq!(execute(&mut vm,b"local a,t=host();collectgarbage();local b,u=host();local co=coroutine.create(function()collectgarbage();return host()end);local ok,c,v=coroutine.resume(co);return a,b,c,t==original,t==u,u==v,ok,debug.getinfo(host,'u').nups"),
        [Value::Number(1.),Value::Number(2.),Value::Number(3.),Value::Boolean(true),Value::Boolean(true),Value::Boolean(true),Value::Boolean(true),Value::Number(2.)]);
}

#[test]
fn result_values_and_captures_survive_return_hooks_and_error_handlers() {
    fn make(call: &mut HostCall<'_>) -> Result<(), Error> {
        let table = call.new_table();
        let key = call.intern(b"value");
        let value = call.arguments()[0];
        call.raw_table_set(table, key, value)?;
        call.set_capture(0, Value::Table(table))?;
        call.push(Value::Table(table))?;
        call.push(value)
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(make, &[Value::Nil]).unwrap();
    vm.set_global(b"host", host).unwrap();
    assert_eq!(execute(&mut vm,b"local calls,returns=0,0;debug.sethook(function(e)if debug.getinfo(2,'f').func==host then collectgarbage();if e=='call'then calls=calls+1 else returns=returns+1 end end end,'cr');local input={};local t,u=host(input);debug.sethook();collectgarbage();return t.value==input,u==input,calls,returns"),
        [Value::Boolean(true),Value::Boolean(true),Value::Number(1.),Value::Number(1.)]);
}

#[test]
fn host_errors_preserve_arbitrary_values_and_allow_subsequent_calls() {
    fn fail(call: &mut HostCall<'_>) -> Result<(), Error> {
        Err(call.raise(call.arguments()[0]))
    }
    fn message(call: &mut HostCall<'_>) -> Result<(), Error> {
        Err(call.error(b"binary\0\xff"))
    }
    let mut vm = Vm::new();
    let fail = vm.new_host_function(fail, &[]).unwrap();
    let message = vm.new_host_function(message, &[]).unwrap();
    vm.set_global(b"fail", fail).unwrap();
    vm.set_global(b"message", message).unwrap();
    assert_eq!(execute(&mut vm,b"local t={};local ok,e=pcall(fail,t);local ok2,e2=xpcall(function()fail(t)end,function(e)collectgarbage();return e end);return ok,e==t,ok2,e2==t,select('#',pcall(fail,nil))"),
        [Value::Boolean(false),Value::Boolean(true),Value::Boolean(false),Value::Boolean(true),Value::Number(2.)]);
    let error = vm.call(message, &[]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Runtime);
    assert_eq!(error.message_bytes(), b"binary\0\xff");
    assert_eq!(execute(&mut vm,b"local co=coroutine.create(function()fail(false)end);local ok,e=coroutine.resume(co);return ok,e,coroutine.status(co)=='dead'"),
        [Value::Boolean(false),Value::Boolean(false),Value::Boolean(true)]);
    assert_eq!(execute(&mut vm, b"return 42"), [Value::Number(42.)]);
}

#[test]
fn host_userdata_uses_native_environment_and_can_store_mutable_state() {
    fn data(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Userdata(id) = call.captures()[0] else {
            panic!()
        };
        let n = u64::from_le_bytes(call.userdata_bytes(id).unwrap().try_into().unwrap()) + 1;
        call.userdata_bytes_mut(id)
            .unwrap()
            .copy_from_slice(&n.to_le_bytes());
        let fresh = call.new_userdata(0);
        call.push(Value::Number(n as f64))?;
        call.push(fresh)
    }
    let mut vm = Vm::new();
    let state = vm.new_userdata(8);
    let host = vm.new_host_function(data, &[state]).unwrap();
    vm.set_global(b"host", host).unwrap();
    assert_eq!(execute(&mut vm,b"local env={};debug.setfenv(host,env);local n,u=host();collectgarbage();local m,v=host();return n,m,debug.getfenv(u)==env,debug.getfenv(v)==env,debug.getfenv(host)==env"),
        [Value::Number(1.),Value::Number(2.),Value::Boolean(true),Value::Boolean(true),Value::Boolean(true)]);
}

#[test]
fn unreachable_host_capture_cycles_are_reclaimed_and_registrations_are_distinct() {
    let mut vm = Vm::new();
    let before = vm.collect_garbage().unwrap();
    for _ in 0..100 {
        let table = vm.new_table();
        let host = vm.new_host_function(echo, &[Value::Table(table)]).unwrap();
        let other = vm.new_host_function(echo, &[]).unwrap();
        assert_ne!(host, other);
        let key = vm.intern(b"cycle");
        vm.raw_table_set(table, key, host).unwrap();
    }
    let after = vm.collect_garbage().unwrap();
    assert_eq!(after.native_closures, before.native_closures);
    assert_eq!(after.tables, before.tables);
}

#[test]
fn invalid_and_collected_captures_and_results_are_rejected() {
    fn invalid(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.push(Value::Native(u64::MAX))
    }
    let mut vm = Vm::new();
    assert!(
        vm.new_host_function(echo, &[Value::Native(u64::MAX)])
            .is_err()
    );
    let callback = vm.new_host_function(invalid, &[]).unwrap();
    assert!(vm.call(callback, &[]).is_err());
    assert!(vm.new_host_function(echo, &[Value::Nil; 61]).is_err());
    let dead = vm.new_host_function(echo, &[]).unwrap();
    vm.collect_garbage().unwrap();
    assert!(vm.new_host_function(echo, &[dead]).is_err());
    assert!(vm.call(dead, &[]).is_err());
}

#[test]
fn host_callbacks_work_as_pattern_replacements_and_metamethods() {
    fn replacement(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::String(id) = call.arguments()[0] else {
            panic!()
        };
        let byte = call.string_bytes(id).unwrap()[0];
        let value = call.intern(&[byte, 0, 255]);
        call.push(value)
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(replacement, &[]).unwrap();
    vm.set_global(b"host", host).unwrap();
    let values = execute(
        &mut vm,
        b"local s,n=string.gsub('abc','(.)',host);return s,n",
    );
    let Value::String(id) = values[0] else {
        panic!()
    };
    assert_eq!(vm.string_bytes(id).unwrap(), b"a\0\xffb\0\xffc\0\xff");
    assert_eq!(values[1], Value::Number(3.));
    let host = vm.new_host_function(echo, &[]).unwrap();
    vm.set_global(b"host", host).unwrap();
    assert_eq!(execute(&mut vm,b"local t=setmetatable({},{__call=host,__index=host});local a,b,c=t(17,nil);return a==t,b,c==nil,t.missing==t"),
        [Value::Boolean(true),Value::Number(17.),Value::Boolean(true),Value::Boolean(true)]);
}

#[test]
fn changing_captures_during_incremental_collection_preserves_new_edges() {
    fn swap(call: &mut HostCall<'_>) -> Result<(), Error> {
        let previous = call.captures()[0];
        let value = call.arguments()[0];
        call.set_capture(0, value)?;
        call.push(previous)
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(swap, &[Value::Nil]).unwrap();
    vm.set_global(b"swap", host).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
        collectgarbage('setstepmul',1)
        for i=1,2000 do
            local t={i};local previous=swap(t)
            if i>1 then assert(previous[1]==i-1)end
            t=nil;collectgarbage('step',0)
        end
        collectgarbage();local last=swap(nil);collectgarbage();return last[1]
    "#
        ),
        [Value::Number(2000.)]
    );
}

#[test]
fn native_result_bounds_and_instruction_abort_leave_captures_usable() {
    fn many(call: &mut HostCall<'_>) -> Result<(), Error> {
        let count = call.number(0)? as usize;
        for _ in 0..count {
            call.push(Value::Nil)?;
        }
        Ok(())
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(many, &[]).unwrap();
    vm.set_global(b"host", host).unwrap();
    assert_eq!(execute(&mut vm,b"local ok,e=pcall(host,8000);return select('#',host(7999)),ok,type(e)=='string',select('#',host(2))"),
        [Value::Number(7999.),Value::Boolean(false),Value::Boolean(true),Value::Number(2.)]);
    vm.set_instruction_limit(Some(100));
    assert_eq!(
        vm.execute(&compile(b"for i=1,1000 do host(2)end").unwrap())
            .unwrap_err()
            .kind(),
        ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    assert_eq!(
        execute(&mut vm, b"return select('#',host(3))"),
        [Value::Number(3.)]
    );
}

#[test]
fn native_callbacks_run_during_collection_and_vm_shutdown() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn finalize(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert!(matches!(call.arguments(), [Value::Userdata(_)]));
        CALLS.fetch_add(1, Relaxed);
        Ok(())
    }
    CALLS.store(0, Relaxed);
    {
        let mut vm = Vm::new();
        let host = vm.new_host_function(finalize, &[]).unwrap();
        vm.set_global(b"host", host).unwrap();
        execute(&mut vm,b"local p=newproxy(true);getmetatable(p).__gc=host;p=nil;collectgarbage();keep=newproxy(true);getmetatable(keep).__gc=host");
        assert_eq!(CALLS.load(Relaxed), 1);
    }
    assert_eq!(CALLS.load(Relaxed), 2);
}

#[test]
fn native_preload_modules_share_cached_table_identity() {
    fn loader(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Number(n) = call.captures()[0] else {
            panic!()
        };
        call.set_capture(0, Value::Number(n + 1.))?;
        let table = call.new_table();
        let name = call.arguments()[0];
        let key = call.intern(b"name");
        call.raw_table_set(table, key, name)?;
        let key = call.intern(b"loads");
        call.raw_table_set(table, key, Value::Number(n + 1.))?;
        call.push(Value::Table(table))
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(loader, &[Value::Number(0.)]).unwrap();
    vm.set_global(b"host", host).unwrap();
    assert_eq!(execute(&mut vm,b"package.preload.native_module=host;local a=require('native_module');collectgarbage();local b=require('native_module');return a==b,a.name=='native_module',a.loads,package.loaded.native_module==a"),
        [Value::Boolean(true),Value::Boolean(true),Value::Number(1.),Value::Boolean(true)]);
}

#[test]
fn cached_programs_bind_separate_native_functions_in_each_vm() {
    use deadlib_lua::{CacheStatus, ProgramCache};
    fn scaled(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Number(scale) = call.captures()[0] else {
            panic!()
        };
        let result = (call.number(0)? + call.number(1)?) * scale;
        call.push(Value::Number(result))
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!(
            "host-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let cache = ProgramCache::new(&directory);
    let source = b"return host(17,25)";
    let first = cache.compile(source).unwrap();
    let second = cache.compile(source).unwrap();
    assert_eq!(first.status, CacheStatus::Stored);
    assert_eq!(second.status, CacheStatus::Hit);
    let mut a = Vm::new();
    let mut b = Vm::new();
    let x = a.new_host_function(scaled, &[Value::Number(2.)]).unwrap();
    a.set_global(b"host", x).unwrap();
    let y = b.new_host_function(scaled, &[Value::Number(3.)]).unwrap();
    b.set_global(b"host", y).unwrap();
    assert_eq!(a.execute(&first.program).unwrap(), [Value::Number(84.)]);
    assert_eq!(b.execute(&second.program).unwrap(), [Value::Number(126.)]);
    a.collect_garbage().unwrap();
    b.collect_garbage().unwrap();
    assert_eq!(b.execute(&first.program).unwrap(), [Value::Number(126.)]);
    for entry in std::fs::read_dir(&directory).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        std::fs::remove_file(entry.path()).unwrap();
    }
    std::fs::remove_dir(directory).unwrap();
}

#[test]
#[cfg(panic = "unwind")]
fn rust_panics_bypass_lua_handlers_and_disable_lua_finalizers_on_drop() {
    use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
    static FINALIZED: AtomicUsize = AtomicUsize::new(0);
    fn fail(_: &mut HostCall<'_>) -> Result<(), Error> {
        panic!("host callback panic")
    }
    fn finalize(_: &mut HostCall<'_>) -> Result<(), Error> {
        FINALIZED.fetch_add(1, Relaxed);
        Ok(())
    }
    FINALIZED.store(0, Relaxed);
    let mut vm = Vm::new();
    let fail = vm.new_host_function(fail, &[]).unwrap();
    vm.set_global(b"fail", fail).unwrap();
    let finalize = vm.new_host_function(finalize, &[]).unwrap();
    vm.set_global(b"finalize", finalize).unwrap();
    let program=compile(b"keep=newproxy(true);getmetatable(keep).__gc=finalize;pcall(fail);error('panic was swallowed')").unwrap();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = vm.execute(&program);
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<&str>(), Some(&"host callback panic"));
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(vm))).unwrap();
    assert_eq!(FINALIZED.load(Relaxed), 0);
}
