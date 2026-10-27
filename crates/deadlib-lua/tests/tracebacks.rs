use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

#[test]
fn message_types_extra_arguments_and_level_coercions_follow_lua51() {
    // i686's nonfinite integer conversion requests billions of fake frames.
    // Retain those resource cases in the bounded standalone traceback audit.
    let extra = if cfg!(target_pointer_width = "64") {
        ",1/0,-1/0,0/0,2^63,2^64"
    } else {
        ""
    };
    let source = r#"
        local tag={}
        local fn=function()end
        local co=coroutine.create(fn)
        local out={}
        local function save(...)
            local ok,value=pcall(debug.traceback,...)
            out[#out+1]=tostring(ok)
            if type(value)=='string'or type(value)=='number'then out[#out+1]=type(value)..':'..tostring(value)
            else out[#out+1]=type(value)..':'..tostring(value==tag)..':'..tostring(value==fn)end
        end
        save();save(nil);save(false);save(tag);save(fn);save(17)
        save('message');save('binary\000message',0);save('message','2')
        save('message',nil,'extra');save('message',2,'extra');save('message',2,3,4)
        save(tag,2,99);save(false,nil,17);save(nil,2)
        save(co);save(co,nil);save(co,'thread',0);save(co,tag,2,3)
        for _,level in ipairs({-3,-1,0,1,2,3,12,13,10000,1.9,-1.9@EXTRA@})do save('at level',level)end
        return table.concat(out,'\n---\n')
    "#;
    compare(source.replace("@EXTRA@", extra).as_bytes());
}

#[test]
fn huge_negative_levels_obey_the_instruction_quota_and_recover() {
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(1000));
    let program = compile(b"return debug.traceback('large',-2147483648)").unwrap();
    assert_eq!(
        vm.execute(&program).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    vm.collect_garbage().unwrap();
    let result = vm
        .execute(&compile(b"return debug.traceback('recovered',1)").unwrap())
        .unwrap()[0];
    let Value::String(id) = result else {
        panic!("traceback string")
    };
    assert!(
        vm.string_bytes(id)
            .unwrap()
            .starts_with(b"recovered\nstack traceback:")
    );
}

#[test]
fn error_handlers_capture_faulting_frames_before_unwind() {
    compare(
        br#"
        local function fail()error('message',0)end
        local function wrap()fail()end
        local ok,trace=xpcall(wrap,debug.traceback)
        local ok2,trace2=xpcall(function()wrap()end,function(e)
            collectgarbage()
            return debug.traceback(e,0)
        end)
        local tag={}
        local ok3,e=xpcall(function()error(tag,0)end,debug.traceback)
        return ok,trace,ok2,trace2,ok3,e==tag
    "#,
    );
}

#[test]
fn truncation_levels_and_lost_tail_records_match_reference_traces() {
    compare(
        br#"
        local out={}
        local function deep(n,level)
            if n==0 then return debug.traceback('deep',level)end
            local result=deep(n-1,level)
            return result
        end
        local function tail(n,level)
            if n==0 then return debug.traceback('tail',level)end
            return tail(n-1,level)
        end
        for _,n in ipairs({0,1,10,11,12,20,21,22,23,24,25,50,100})do
            for _,level in ipairs({-1,0,1,2,11,12,13,20,10000})do
                out[#out+1]=deep(n,level);out[#out+1]=tail(n,level)
            end
        end
        out[#out+1]=tail(30000,1)
        return table.concat(out,'\n---\n')
    "#,
    );
}

#[test]
fn traces_cover_new_suspended_normal_and_failed_coroutines() {
    compare(
        br#"
        local out={}
        local co=coroutine.create(function()
            local function f()coroutine.yield();error('failure',0)end
            return f()
        end)
        out[#out+1]=debug.traceback(co)
        coroutine.resume(co);out[#out+1]=debug.traceback(co,'suspended')
        coroutine.resume(co);out[#out+1]=debug.traceback(co,'failed',0)
        co=coroutine.create(function()return 1 end)
        coroutine.resume(co);out[#out+1]=debug.traceback(co)
        local parent
        parent=coroutine.create(function()
            local child=coroutine.create(function()
                out[#out+1]=debug.traceback(parent,'normal',0)
                out[#out+1]=debug.traceback(coroutine.running(),'current',0)
            end)
            assert(coroutine.resume(child))
        end)
        assert(coroutine.resume(parent))
        return table.concat(out,'\n---\n')
    "#,
    );
}

#[test]
fn concatenation_callbacks_see_each_formatted_frame_and_native_arguments() {
    compare(
        br#"
        local out={}
        local tag
        local function concat(a,b)
            local slots={}
            for i=1,20 do
                local n,v=debug.getlocal(2,i)
                if not n then break end
                slots[#slots+1]=n..':'..type(v)..':'..(type(v)=='string'and v or'')
            end
            out[#out+1]=table.concat(slots,'|')
            out[#out+1]=type(a)..':'..type(b)..':'..(type(b)=='string'and b or tostring(b==tag))
            collectgarbage()
            return tag
        end
        tag=setmetatable({},{__concat=concat})
        local function inner()local value=debug.traceback('message',tag,'suffix');return value end
        local function outer()local value=inner();return value end
        local value=outer()
        out[#out+1]=tostring(value==tag)
        return table.concat(out,'\n---\n')
    "#,
    );
}

#[test]
fn nested_traces_and_protected_concat_errors_keep_independent_jobs() {
    compare(
        br#"
        local out={}
        local failure={}
        local bad=setmetatable({},{__concat=function()error(failure,0)end})
        local tag=setmetatable({},{__concat=function(a,b)
            out[#out+1]=debug.traceback('nested',0)
            local ok,e=pcall(debug.traceback,'error',bad)
            out[#out+1]=tostring(ok)..':'..tostring(e==failure)
            collectgarbage()
            return 'converted'
        end})
        out[#out+1]=debug.traceback('outer',tag)
        local ok,e=pcall(debug.traceback,'error',bad)
        out[#out+1]=tostring(ok)..':'..tostring(e==failure)
        out[#out+1]=debug.traceback('recovered')
        return table.concat(out,'\n---\n')
    "#,
    );
}

#[test]
fn native_concat_failures_and_callback_argument_mutations_keep_lua_values() {
    compare(
        br#"
        local out={}
        for _,call in ipairs({
            function()return debug.traceback('message',{})end,
            function()return debug.traceback('message',nil,'extra')end,
            function()return debug.traceback('message',false,'extra')end,
        })do
            local ok,e=pcall(call);out[#out+1]=tostring(ok)..':'..e
        end
        local tag=setmetatable({},{__concat=function(a,b)
            assert(debug.setlocal(2,1,'rewritten')=='(*temporary)')
            return 'converted'
        end})
        out[#out+1]=debug.traceback('message',tag)
        return table.concat(out,'\n---\n')
    "#,
    );
}

#[test]
fn concat_callbacks_obey_yield_boundaries_and_preserve_parent_jobs() {
    compare(
        br#"
        local out={}
        local tag=setmetatable({},{__concat=function(a,b)
            local child=coroutine.create(function()
                collectgarbage();coroutine.yield('child');return 'finished'
            end)
            local ok,v=coroutine.resume(child);out[#out+1]=tostring(ok)..':'..v
            local ok,v=coroutine.resume(child);out[#out+1]=tostring(ok)..':'..v
            return 'converted'
        end})
        local parent=coroutine.create(function()return debug.traceback('parent',tag)end)
        local ok,value=coroutine.resume(parent)
        out[#out+1]=tostring(ok)..':'..value
        local bad=setmetatable({},{__concat=function()coroutine.yield('forbidden')end})
        parent=coroutine.create(function()return debug.traceback('yield',bad)end)
        local ok,value=coroutine.resume(parent)
        out[#out+1]=tostring(ok)..':'..type(value)..':'..coroutine.status(parent)
        return table.concat(out,'\n---\n')
    "#,
    );
}

#[test]
fn target_threads_stay_rooted_after_argument_overwrites_during_callbacks() {
    let mut vm = Vm::new();
    let program = compile(
        br#"
        local victim=coroutine.create(function()coroutine.yield()end)
        assert(coroutine.resume(victim))
        local weak=setmetatable({victim},{__mode='v'})
        local tag=setmetatable({},{__concat=function(a,b)
            assert(debug.setlocal(2,1,nil)=='(*temporary)')
            victim=nil;collectgarbage();assert(weak[1]~=nil)
            return 'converted'
        end})
        local trace=debug.traceback(victim,'target',tag)
        collectgarbage()
        return weak[1]==nil,type(trace)=='string'
    "#,
    )
    .unwrap();
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[Value::Boolean(true), Value::Boolean(true)]
    );
}

#[test]
fn instruction_abort_discards_jobs_and_releases_hidden_target_roots() {
    use deadlib_lua::ErrorKind;
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(1000));
    let program = compile(
        br#"
        victim=coroutine.create(function()coroutine.yield()end)
        assert(coroutine.resume(victim))
        weak=setmetatable({victim},{__mode='v'})
        worker=coroutine.create(function()
            local tag=setmetatable({},{__concat=function(a,b)
                debug.setlocal(2,1,nil);victim=nil;collectgarbage()
                while true do end
            end})
            return debug.traceback(victim,'target',tag)
        end)
        coroutine.resume(worker)
    "#,
    )
    .unwrap();
    assert_eq!(vm.execute(&program).unwrap_err().kind(), ErrorKind::Limit);
    vm.set_instruction_limit(Some(10_000));
    vm.collect_garbage().unwrap();
    let recover = compile(
        b"collectgarbage();return weak[1]==nil,coroutine.status(worker),coroutine.running()",
    )
    .unwrap();
    let values = vm.execute(&recover).unwrap().to_vec();
    assert_eq!(values[0], Value::Boolean(true));
    let Value::String(status) = values[1] else {
        panic!("worker status")
    };
    assert_eq!(vm.string_bytes(status).unwrap(), b"dead");
    assert_eq!(values[2], Value::Nil);
}

#[test]
fn nested_concat_tracebacks_match_the_native_call_depth_limit() {
    compare(
        br#"
        local visits=0
        local tag
        tag=setmetatable({},{__concat=function(a,b)
            visits=visits+1
            return debug.traceback('nested',tag)
        end})
        local ok,value=pcall(debug.traceback,'root',tag)
        return ok,type(value),visits,string.find(value,'C stack overflow')~=nil
    "#,
    );
}

#[test]
fn nonfinite_and_large_debug_indices_use_the_reference_integer_conversion() {
    compare(
        br#"
        local out={}
        local x=1;local function f()return x end
        for _,index in ipairs({1/0,-1/0,0/0,2^63,-2^63,2^64,2^32,2^32+1.9,-2^32-1.9})do
            local d=debug.getinfo(index,'S');out[#out+1]=d and d.what or'nil'
            local ok,n,v=pcall(debug.getlocal,index,1);out[#out+1]=tostring(ok)..':'..tostring(n)..':'..type(v)
            local n,v=debug.getupvalue(f,index);out[#out+1]=tostring(n)..':'..type(v)
            out[#out+1]=tostring((debug.setupvalue(f,index,1)))
        end
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn repeated_tracebacks_reclaim_intermediate_strings_and_bound_storage() {
    let mut vm = Vm::new();
    let program = compile(
        br#"
        local function f(n)return debug.traceback(n,1)end
        for i=1,1000 do local text=f(i);assert(string.find(text,'stack traceback:'))end
        return 42
    "#,
    )
    .unwrap();
    for _ in 0..20 {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(42.0)]);
        vm.collect_garbage().unwrap();
    }
    let stats = vm.heap_stats();
    assert!(stats.reserved_bytes < 4_000_000, "{stats:?}");
    assert!(stats.reclaimed_objects > 40_000, "{stats:?}");
    assert!(stats.functions < 20, "{stats:?}");
}

#[test]
fn unprotected_errors_keep_the_payload_without_rooting_discarded_stack_values() {
    let mut vm = Vm::new();
    let program = compile(
        br#"
        local victim={tag=17}
        weak=setmetatable({victim},{__mode='v'})
        error({tag=42},0)
    "#,
    )
    .unwrap();
    let error = vm.execute(&program).unwrap_err();
    let payload = error.value().unwrap();
    let root = vm.pin(payload).unwrap();
    vm.collect_garbage().unwrap();
    vm.set_global(b"payload", vm.rooted(root).unwrap()).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return weak[1]==nil,payload.tag").unwrap())
            .unwrap(),
        &[Value::Boolean(true), Value::Number(42.0)]
    );
    vm.unpin(root).unwrap();
}
fn compare(source: &[u8]) {
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(10_000_000));
    vm.set_output_capture(true);
    let program = compile(source).unwrap();
    let values = vm
        .execute(&program)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(source)))
        .to_vec();
    let actual: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Boolean(b),
            Value::Number(n) => Scalar::Number(n.to_bits()),
            Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            v => panic!(
                "return scalar observations, got {v:?}: {}",
                String::from_utf8_lossy(source)
            ),
        })
        .collect();
    // SAFETY: fixed local programs need the development oracle's debug library.
    let lua = unsafe { Lua::unsafe_new() };
    // Use a plain protected entry with no host error-handler value below it.
    // The normal mlua eval wrapper leaves that C function on the base stack,
    // which Lua 5.1 can expose through negative/lost-tail local queries.
    let reference: MultiValue = lua.exec_raw_lua(|raw| unsafe {
        use lua_reference::{FromLuaMulti, ffi};
        let state = raw.state();
        let start = ffi::lua_gettop(state);
        let mut status = ffi::luaL_loadbuffer(
            state,
            source.as_ptr().cast(),
            source.len(),
            c"=(deadlib-lua)".as_ptr(),
        );
        if status == 0 {
            status = ffi::lua_pcall(state, 0, ffi::LUA_MULTRET, 0);
        }
        if status != 0 {
            let mut len = 0;
            let text = ffi::lua_tolstring(state, -1, &mut len);
            let message = if text.is_null() {
                "non-string error".into()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(text.cast(), len)).into_owned()
            };
            ffi::lua_settop(state, start);
            panic!("reference: {message}: {}", String::from_utf8_lossy(source));
        }
        let values = MultiValue::from_stack_multi(ffi::lua_gettop(state) - start, raw).unwrap();
        ffi::lua_settop(state, start);
        values
    });
    let expected: Vec<_> = reference
        .into_iter()
        .map(|v| match v {
            Ref::Nil => Scalar::Nil,
            Ref::Boolean(b) => Scalar::Boolean(b),
            Ref::Number(n) => Scalar::Number(n.to_bits()),
            Ref::Integer(n) => Scalar::Number((n as f64).to_bits()),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            _ => panic!("return reference scalar observations"),
        })
        .collect();
    if actual != expected {
        for (a, b) in actual.iter().zip(&expected) {
            if let (Scalar::Bytes(a), Scalar::Bytes(b)) = (a, b)
                && a != b
            {
                let at = a
                    .iter()
                    .zip(b)
                    .position(|(a, b)| a != b)
                    .unwrap_or(a.len().min(b.len()));
                panic!(
                    "bytes differ at {at}: rust={:?}, reference={:?}: {}",
                    String::from_utf8_lossy(&a[at.saturating_sub(40)..(at + 80).min(a.len())]),
                    String::from_utf8_lossy(&b[at.saturating_sub(40)..(at + 80).min(b.len())]),
                    String::from_utf8_lossy(source)
                );
            }
        }
        assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
    }
}
