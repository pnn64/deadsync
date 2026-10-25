use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
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
    let expected: Vec<_> = lua
        .load(source)
        .set_name("=(deadlib-lua)")
        .eval::<MultiValue>()
        .unwrap()
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
    assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
}

#[test]
fn proxies_keep_raw_metatables_environments_and_identity() {
    for source in [
        "local a,b=newproxy(),newproxy(false);local u=newproxy(true);local m=getmetatable(u);local v=newproxy(u);m.__metatable='protected';local w=newproxy(v);return type(a),a~=b,getmetatable(a),getmetatable(b),getmetatable(v),debug.getmetatable(w)==m,rawequal(u,v)",
        "local p=newproxy(true);local t={};debug.setmetatable(t,getmetatable(p));local q=newproxy(t);local a=pcall(newproxy,{});local b=pcall(newproxy,1);return getmetatable(p)==getmetatable(q),a,b",
        "debug.setfenv(newproxy,{marker=7});local p=newproxy(true);local before=debug.getfenv(p).marker;local e={marker=11};local r=debug.setfenv(p,e);local a=pcall(getfenv,p);local b=pcall(setfenv,p,{});return before,debug.getfenv(p)==e,r==p,a,b",
        "local p=newproxy(true);local q=newproxy(p);local t={[p]=1,[q]=2};t[p]=nil;local valid=pcall(next,t,p);local a=pcall(rawget,p,1);local b=pcall(setmetatable,p,{});return valid,t[q],a,b",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn userdata_dispatches_metamethods_with_lua51_arguments() {
    compare(
        br#"
        local u=newproxy(true);local v=newproxy(u);local m=getmetatable(u)
        m.__newindex=function(o,k,v)m[k]=v end
        m.__index=function(o,k)return m[k]end
        m.__call=function(o,a)return o==u,a end
        m.__eq=function(a,b)return true end
        m.__lt=function(a,b)return a==u end
        m.__len=function(a,b)return b==nil and 13 or 0 end
        m.__add=function(a,b)return a==u and b or 0 end
        m.__unm=function(a,b)return a==b and 17 or 0 end
        m.__concat=function(a,b)return 'joined' end
        m.__tostring=function(a)return 'proxy' end
        u.x=7;local a,b=u(9);local t=setmetatable({},m)
        return u.x,a,b,u==v,u==t,u<v,#u,u+5,-u,u..v,tostring(u)
    "#,
    );
}

#[test]
fn finalizers_run_once_in_reverse_creation_order_and_can_resurrect() {
    compare(
        br#"
        collectgarbage('stop');local log={};local p=newproxy(true);local m=getmetatable(p)
        debug.setfenv(p,{id=0})
        m.__gc=function(o)local i=debug.getfenv(o).id;log[#log+1]=i;if i==3 then rescued=o end end
        for i=1,20 do local u=newproxy(p);debug.setfenv(u,{id=i}) end
        p=nil;collectgarbage();local first=table.concat(log,',');collectgarbage()
        local id=debug.getfenv(rescued).id;rescued=nil;collectgarbage()
        return first,#log,id
    "#,
    );
    compare(br#"
        collectgarbage('stop');local p=newproxy(true);local m=getmetatable(p);local log={}
        debug.setfenv(p,{id=1});local q=newproxy(p);debug.setfenv(q,{id=2})
        m.__gc=function(o)log[#log+1]=debug.getfenv(o).id;m.__gc=function(o)log[#log+1]=10+debug.getfenv(o).id end end
        p,q=nil;collectgarbage();return table.concat(log,',')
    "#);
}

#[test]
fn reused_userdata_slots_preserve_finalization_order_across_collections() {
    compare(br#"
        collectgarbage('stop');local hash,count=0,0;local p=newproxy(true)
        getmetatable(p).__gc=function(o)local i=debug.getfenv(o).id;hash=(hash*31+i)%10000019;count=count+1 end
        for batch=1,100 do
            local kept={}
            for i=1,10+batch%20 do
                local u=newproxy(p);debug.setfenv(u,{id=batch*100+i})
                if (batch+i)%5==0 then kept[#kept+1]=u end
            end
            collectgarbage();kept=nil;collectgarbage();collectgarbage()
        end
        debug.setfenv(p,{id=0});p=nil;collectgarbage();return hash,count
    "#);
}

#[test]
fn weak_tables_distinguish_finalized_userdata_keys_and_values() {
    compare(br#"
        collectgarbage('stop');local p=newproxy(true);local k=setmetatable({},{__mode='k'});local v=setmetatable({},{__mode='v'})
        local seen=0;getmetatable(p).__gc=function(o)assert(k[o]==17 and v[1]==nil);seen=seen+1 end
        k[p]=17;v[1]=p;p=nil;collectgarbage();local key=next(k);local first=type(key);key=nil
        collectgarbage();return seen,first,next(k),next(v)
    "#);
    compare(br#"
        collectgarbage('stop');local p=newproxy(true);local m=getmetatable(p);setmetatable(m,{__mode='v'})
        local seen=0;m.__gc=function()seen=seen+1 end;collectgarbage();p=nil;collectgarbage();return seen,m.__gc
    "#);
}

#[test]
fn finalizer_errors_preserve_objects_and_leave_pending_work_recoverable() {
    compare(br#"
        collectgarbage('stop');local tag={};local n=0;local p=newproxy(true);local m=getmetatable(p)
        debug.setfenv(p,{id=1});local q=newproxy(p);debug.setfenv(q,{id=2})
        m.__gc=function(o)if debug.getfenv(o).id==2 then error(tag)end;n=n+1 end
        p,q=nil;local ok,e=pcall(collectgarbage);local before=n;collectgarbage();collectgarbage();return ok,e==tag,before,n
    "#);
    compare(br#"
        collectgarbage('stop');local log={};local w=setmetatable({},{__mode='k'})
        do local a=newproxy(true);w[a]=true;getmetatable(a).__gc=function()log[#log+1]=1 end
           local b=newproxy(true);getmetatable(b).__gc=function()error(7)end end
        pcall(collectgarbage)
        do local a=next(w);local c=newproxy(true);getmetatable(c).__gc=function()log[#log+1]=3 end;getmetatable(a).child=c end
        collectgarbage();local first=table.concat(log,',');collectgarbage();return first
    "#);
    compare(br#"
        collectgarbage('stop');local tag={};local n=0;local a=newproxy(true);getmetatable(a).__gc=function()n=n+1 end
        local b=newproxy(true);getmetatable(b).__gc=function()error(tag)end;a,b=nil
        local ok,e=xpcall(collectgarbage,function(e)collectgarbage();return e end);return ok,e==tag,n
    "#);
    compare(br#"
        collectgarbage('stop');local n=0;local p=newproxy(true);getmetatable(p).__gc=function()local ok=pcall(coroutine.yield,1);assert(not ok);n=n+1 end
        local c=coroutine.create(function()p=nil;collectgarbage();return n end);return coroutine.resume(c)
    "#);
}

#[test]
fn finalizers_can_collect_recursively_and_resume_child_threads() {
    compare(br#"
        collectgarbage('stop');local log={};local p=newproxy(true);local m=getmetatable(p)
        debug.setfenv(p,{id=0});for i=1,3 do local q=newproxy(p);debug.setfenv(q,{id=i})end
        m.__gc=function(o)local i=debug.getfenv(o).id;log[#log+1]=i;if i==3 then local q=newproxy(true);getmetatable(q).__gc=function()log[#log+1]=9 end;q=nil;collectgarbage()end end
        p=nil;collectgarbage();return table.concat(log,',')
    "#);
    compare(br#"
        collectgarbage('stop');local n=0;local p=newproxy(true);getmetatable(p).__gc=function()local c=coroutine.create(function()coroutine.yield(7);return 11 end);local a,b=coroutine.resume(c);local d,e=coroutine.resume(c);assert(a and d);n=b+e end
        p=nil;collectgarbage();return n
    "#);
    compare(br#"
        collectgarbage('stop');local result=0;local p=newproxy(true)
        getmetatable(p).__gc=function()local n=0;local function f()n=n+1;coroutine.resume(coroutine.create(f))end;coroutine.resume(coroutine.create(f));result=n end
        p=nil;collectgarbage();return result
    "#);
}

#[test]
fn automatic_finalization_restores_interrupted_lua_registers() {
    compare(br#"
        local n=0;local p=newproxy(true);getmetatable(p).__gc=function()n=n+1;local t={1,2,3};assert(#t==3)end
        for i=1,10000 do newproxy(p)end;local automatic=n>0;p=nil;collectgarbage();return automatic,n
    "#);
    compare(br#"
        local keep={};for i=1,10000 do keep[i]=i end
        collectgarbage('setpause',100);local p=newproxy(true);local nested=false
        getmetatable(p).__gc=function()if not nested then nested=true;collectgarbage();nested=false end end
        for i=1,10 do local u=newproxy(p);u=nil;local t={i};assert(t[1]==i)end
        return #keep
    "#);
    compare(br#"
        local keep={};for i=1,10000 do keep[i]=i end
        collectgarbage('setpause',100);local p=newproxy(true);local nested=false
        getmetatable(p).__gc=function()if not nested then nested=true;collectgarbage();nested=false end end
        local function g()local t={};for i=1,500 do t[i]={i}end;local u=newproxy(p);return unpack(t)end
        local function f(...)collectgarbage();local t={...};local n=0;for i=1,#t do n=n+t[i][1]end;return #t,n end
        return f(g())
    "#);
}

#[test]
fn host_userdata_storage_and_collection_validate_live_handles() {
    let mut vm = Vm::new();
    let baseline = vm.heap_stats().userdata;
    let value = vm.new_userdata(32);
    let Value::Userdata(id) = value else {
        unreachable!()
    };
    assert_eq!(vm.userdata_bytes(id).unwrap(), &[0; 32]);
    vm.userdata_bytes_mut(id).unwrap()[3] = 17;
    let root = vm.pin(value).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.userdata_bytes(id).unwrap()[3], 17);
    vm.unpin(root).unwrap();
    assert_eq!(vm.collect_garbage().unwrap().userdata, baseline);
    assert!(vm.userdata_bytes(id).is_none());
    assert!(vm.userdata_bytes_mut(id).is_none());
    assert!(vm.pin(value).is_err());
}

#[test]
fn host_collection_runs_finalizers_and_preserves_return_roots() {
    let mut vm = Vm::new();
    let value=vm.execute(&compile(b"collectgarbage('stop');n=0;local p=newproxy(true);getmetatable(p).__gc=function()n=n+1 end;local q=newproxy(true);getmetatable(q).__gc=function()collectgarbage()end;return p").unwrap()).unwrap()[0];
    vm.collect_garbage().unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.global(b"n"), Value::Number(0.0));
    vm.set_global(b"p", value).unwrap();
    vm.execute(&compile(b"p=nil;return nil").unwrap()).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.global(b"n"), Value::Number(1.0));
    vm.execute(&compile(b"tag={};local p=newproxy(true);getmetatable(p).__gc=function()error(tag)end;return nil").unwrap()).unwrap();
    let error = vm.collect_garbage().unwrap_err();
    assert_eq!(error.value(), Some(vm.global(b"tag")));
    vm.collect_garbage().unwrap();
    assert_eq!(
        vm.execute(&compile(b"return 6*7").unwrap()).unwrap(),
        &[Value::Number(42.0)]
    );
}

#[test]
fn vm_drop_finalizes_reachable_userdata_and_continues_after_errors() {
    let run=std::process::Command::new(env!("CARGO_BIN_EXE_deadlua")).args(["-e", "collectgarbage('stop');keep={};for i=1,3 do local p=newproxy(true);keep[i]=p;getmetatable(p).__gc=function()print(i);if i==3 then error(7)end;if i==2 then local q=newproxy(true);getmetatable(q).__gc=function()print(99)end end end end;print('done')"]).output().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        run.stdout,
        if cfg!(windows) {
            b"done\r\n3\r\n2\r\n1\r\n".as_slice()
        } else {
            b"done\n3\n2\n1\n".as_slice()
        }
    );
}

#[test]
fn instruction_abort_preserves_pending_finalizers_and_recovers_the_vm() {
    let mut vm = Vm::new();
    vm.execute(&compile(b"collectgarbage('stop');n=0;local a=newproxy(true);getmetatable(a).__gc=function()n=n+1 end;local b=newproxy(true);getmetatable(b).__gc=function()while true do end end;return nil").unwrap()).unwrap();
    vm.set_instruction_limit(Some(0));
    assert_eq!(
        vm.collect_garbage().unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(Some(100));
    assert_eq!(
        vm.collect_garbage().unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    vm.collect_garbage().unwrap();
    assert_eq!(vm.global(b"n"), Value::Number(1.0));
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.0)]
    );
    vm.execute(&compile(b"local p=newproxy(true);getmetatable(p).__gc=function()while true do end end;return nil").unwrap()).unwrap();
    vm.set_instruction_limit(Some(0));
    drop(vm);
}
