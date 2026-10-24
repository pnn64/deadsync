use deadlib_lua::{ErrorKind, Value, Vm, compile};
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
    let values = vm
        .execute(&compile(source).unwrap())
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(source)))
        .to_vec();
    let actual: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Boolean(b),
            Value::Number(n) => Scalar::Number(n.to_bits()),
            Value::String(s) => Scalar::Bytes(vm.string_bytes(s).unwrap().to_vec()),
            _ => panic!("return scalar observations"),
        })
        .collect();
    // SAFETY: the development oracle runs only the fixed, local test programs;
    // its unrestricted standard libraries are required for debug environment APIs.
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
            Ref::Integer(n) => Scalar::Number((n as f64).to_bits()),
            Ref::Number(n) => Scalar::Number(n.to_bits()),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            _ => panic!("return reference scalars"),
        })
        .collect();
    assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
}

#[test]
fn create_resume_yield_status_and_exact_result_counts() {
    for source in [
        "local c=coroutine.create(function(a,...)local x,y=coroutine.yield(a,...);return x,y,nil end);local s=coroutine.status(c);local a,b,d,e=coroutine.resume(c,1,nil,3);local t=coroutine.status(c);local f,g,h,i=coroutine.resume(c,7,8);return coroutine.running(),type(c),s,a,b,d,e,t,f,g,h,i,coroutine.status(c)",
        "local c=coroutine.create(function()coroutine.yield();return end);local a=select('#',coroutine.resume(c));local b=select('#',coroutine.resume(c));return a,b,coroutine.resume(c)",
        "local c=coroutine.create(function(...)return ... end);return coroutine.resume(c,1,nil,3,nil)",
        "local c=coroutine.create(function()return coroutine.yield(1,2)end);local a,b,d=coroutine.resume(c);local e,f,g=coroutine.resume(c,3,4);return a,b,d,e,f,g,coroutine.status(c)",
        "local c=coroutine.create(function(self)return coroutine.running()==self,coroutine.status(self),coroutine.resume(self)end);return coroutine.resume(c,c)",
        "local c=coroutine.create(function()return 7 end);local t={[c]='x'};return t[c],rawequal(c,c),coroutine.create(function()end)==c,string.find(tostring(c),'thread:')~=nil",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn wrappers_share_heap_and_keep_independent_native_identity() {
    for source in [
        "local f=coroutine.wrap(function(a)for i=1,3 do a=coroutine.yield(a+i)end;return a end);return type(f),f(10),f(20),f(30),f(40)",
        "local a=coroutine.wrap(function()coroutine.yield(1);return 2 end);local b=coroutine.wrap(function()return 3 end);local m=string.gmatch('ab','.');return a==b,a==m,m(),a(),b(),m(),a()",
        "local f=coroutine.wrap(function()coroutine.yield();return 1,nil,3,nil end);local a=select('#',f());local b,c,d,e=f();return a,b,c,d,e,pcall(f)",
        "local t={};local f=coroutine.wrap(function(x)coroutine.yield(x);x.n=3;return x end);return f(t)==t,f()==t,t.n",
        "local c; c=coroutine.create(function()return error(c)end);local a,b=coroutine.resume(c);return a,b==c,coroutine.status(c)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn nested_resumes_preserve_normal_status_and_protected_calls() {
    for source in [
        "local a,b; b=coroutine.create(function()return coroutine.status(a),coroutine.status(b),coroutine.resume(a)end);a=coroutine.create(function()local x,y,z,w=coroutine.resume(b);return x,y,z,w,coroutine.status(b)end);return coroutine.resume(a)",
        "local c=coroutine.create(function()coroutine.yield(7);return 9 end);local a,b,d=pcall(coroutine.resume,c);local e,f,g=pcall(coroutine.resume,c);return a,b,d,e,f,g",
        "local c=coroutine.create(function()return 4 end);local f=coroutine.wrap(function()return coroutine.resume(c)end);return pcall(f)",
        "local c=coroutine.create(function()error('child',0)end);local a,b=coroutine.resume(c);local d,e=pcall(coroutine.resume,c);return a,b,d,e,coroutine.status(c)",
        "local n=0;local function f()n=n+1;local c=coroutine.create(f);local ok,s=coroutine.resume(c);return s end;local ok,s=coroutine.resume(coroutine.create(f));return ok,n,s",
        "local n=0;local function f()n=n+1;local c=coroutine.create(f);local ok,s=coroutine.resume(c);return s end;local a,b,s=pcall(function()return coroutine.resume(coroutine.create(f))end);return a,b,n,s",
        "local n=0;local function f()n=n+1;local c=coroutine.create(f);local ok,s=coroutine.resume(c);return s end;local a,b=xpcall(function()error(1)end,function()local ok,s=coroutine.resume(coroutine.create(f));return s end);return a,b,n",
        "local n=0;local function f()n=n+1;local c=coroutine.create(f);local ok,s=coroutine.resume(c);return s end;local s=string.gsub('a','.',function()local ok,s=coroutine.resume(coroutine.create(f));return s end);return n,s",
        "local failed;local n=0;local function f()n=n+1;local c=coroutine.create(f);local ok,s=coroutine.resume(c);if not ok then failed=c end;return s end;local ok,s=coroutine.resume(coroutine.create(f));local status=coroutine.status(failed);local a,b=coroutine.resume(failed);return status,a,b,n",
        "local target=coroutine.create(function()local x=coroutine.yield(3);return x end);coroutine.resume(target);local n=0;local function f()n=n+1;if n==199 then return coroutine.resume(target,7)end;local a,b,c=coroutine.resume(coroutine.create(f));return b,c end;coroutine.resume(coroutine.create(f));local s=coroutine.status(target);local a,b=coroutine.resume(target,9);return s,a,b,n",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn native_jobs_survive_child_resumes_and_thread_metatables_are_shared() {
    for source in [
        "local c=coroutine.create(function(s)return string.gsub(s,'.','%0%0')end);return string.gsub('x','.',function(s)local ok,r=coroutine.resume(c,s);return r end)",
        "local t={3,1,2};table.sort(t,function(a,b)local co=coroutine.create(function()return a<b end);local ok,v=coroutine.resume(co);return v end);return table.concat(t,',')",
        "local c=coroutine.create(function()return coroutine.yield(3)end);local s=table.foreachi({1,2},function(k,v)local ok,n=coroutine.resume(c,v);return n end);return s,coroutine.status(c)",
        "local a=coroutine.create(function()end);local b=coroutine.create(function()end);local mt={__index=function(t,k)return k end,__tostring=function()return 'co' end,__eq=function()return true end};debug.setmetatable(a,mt);return getmetatable(b)==mt,a.x,b.y,tostring(b),a==b",
        "local f=coroutine.wrap(function()local a=0;local b={};return xpcall(function()return coroutine.yield()end,function(e)local c=coroutine.create(function()return 17 end);local ok,n=coroutine.resume(c);return n end)end);return f()",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn generated_resume_sequences_and_recursive_generators_preserve_state() {
    compare(
        br#"
        local hash,steps=0,0
        for case=1,300 do
            local state=case
            local c=coroutine.create(function(seed)
                local function update(n)state=state+n;return state end
                for i=1,case%13 do
                    local v,w=coroutine.yield(update,seed+i,nil)
                    state=state+(v or 0)+(w or 0)
                end
                return state,case,nil
            end)
            local ok,fn,n=coroutine.resume(c,case*3)
            while coroutine.status(c)=='suspended' do
                assert(ok and type(fn)=='function')
                hash=(hash*17+fn(5)+n)%10000019
                steps=steps+1
                ok,fn,n=coroutine.resume(c,case%7,case%11)
            end
            assert(ok and type(fn)=='number' and n==case)
            hash=(hash*31+fn+n)%10000019
        end
        local function generate(t,k)
            if k==0 then coroutine.yield(t)
            else for i=1,5 do t[k]=i;generate(t,k-1) end end
        end
        local first,count=nil,0
        for t in coroutine.wrap(function()generate({},4)end) do
            first=first or t;assert(first==t);count=count+1
        end
        return hash,steps,count
    "#,
    );
}

#[test]
fn open_cells_remain_shared_across_suspended_and_normal_threads() {
    for source in [
        "local c=coroutine.create(function()local n=10;local function f(v)n=n+v;return n end;coroutine.yield(f);return f(1)end);local a,f=coroutine.resume(c);local n=f(4);local b,m=coroutine.resume(c);return a,n,b,m,f(5)",
        "local f;local c=coroutine.create(function()local n=10;f=function()n=n+1;return n end;error(7)end);local a,b=coroutine.resume(c);local d=coroutine.resume(c,1,2,3,4,5,6);return a,b,d,f(),f()",
        "local c;local f;local a=coroutine.create(function()local n=10;f=function()n=n+1;return n end;c=coroutine.create(function()return f()end);local ok,v=coroutine.resume(c);return ok,v,n end);return coroutine.resume(a)",
        "local n=1;local a=coroutine.create(function()n=n+1;coroutine.yield(n);n=n+1;return n end);local b=coroutine.create(function()n=n+10;return n end);local p,q=coroutine.resume(a);local r,s=coroutine.resume(b);local t,u=coroutine.resume(a);return p,q,r,s,t,u,n",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn lua51_rejects_yields_through_native_and_metamethod_boundaries() {
    for source in [
        "return pcall(coroutine.yield,1)",
        "local c=coroutine.create(function()return pcall(coroutine.yield,1)end);return coroutine.resume(c)",
        "local c=coroutine.create(function()return pcall(function()coroutine.yield(1)end)end);return coroutine.resume(c)",
        "local c=coroutine.create(function()return string.gsub('a','.',coroutine.yield)end);return coroutine.resume(c)",
        "local c=coroutine.create(function()local t=setmetatable({},{__index=function()return coroutine.yield(3)end});return t.x end);return coroutine.resume(c)",
        "local c=coroutine.create(function()local t=setmetatable({},{__call=coroutine.yield});return t()end);local a,b=coroutine.resume(c);return a,type(b),coroutine.resume(c,9)",
        "local c=coroutine.create(function()return table.foreach({1},function()coroutine.yield(1)end)end);return coroutine.resume(c)",
        "local c=coroutine.create(function()return table.sort({2,1},function()return coroutine.yield(true)end)end);return coroutine.resume(c)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn thread_environments_and_debug_environments_follow_lua51() {
    for source in [
        "local c=coroutine.create(function()local original=getfenv(1);setfenv(0,{n=19});coroutine.yield(getfenv(0),original);return loadstring('return n')()end);local ok,t,g=coroutine.resume(c);local a,n=coroutine.resume(c);return ok,t.n,g==_G,a,n,getfenv(0)==_G",
        "local t={n=15};local c=coroutine.create(function()coroutine.yield(getfenv(0));return loadstring('return n')()end);local s=debug.setfenv(c,t);local a,b=coroutine.resume(c);local d,e=coroutine.resume(c);return s==c,debug.getfenv(c)==t,a,b==t,d,e",
        "local f=coroutine.wrap(function()end);local t={};return debug.getfenv(f)==_G,debug.setfenv(f,t)==f,debug.getfenv(f)==t,getfenv(f)==_G,debug.getfenv(1)",
        "local t={};local m=string.gmatch('ab','.');debug.setfenv(m,t);return debug.getfenv(m)==t,m(),m(),getfenv(m)==_G",
        "local original=debug.getfenv(print);local t={};debug.setfenv(print,t);local ok=debug.getfenv(print)==t;debug.setfenv(print,original);return ok,getfenv(print)==_G",
        "local original=getfenv(0);local t={};setfenv(0,t);local c=coroutine.create(function()end);local f=coroutine.wrap(function()end);local m=string.gmatch('a','.');local a,b,d=debug.getfenv(c)==t,debug.getfenv(f)==original,debug.getfenv(m)==original;setfenv(0,original);return a,b,d",
        "local t={};local original=debug.getfenv(coroutine.wrap);debug.setfenv(coroutine.wrap,t);local f=coroutine.wrap(function()end);debug.setfenv(coroutine.wrap,original);local a=debug.getfenv(f)==t;local original=debug.getfenv(string.gmatch);debug.setfenv(string.gmatch,t);local m=string.gmatch('ab','.');debug.setfenv(string.gmatch,original);return a,debug.getfenv(m)==t,getfenv(f)==_G,getfenv(m)==_G",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn failed_wrappers_preserve_error_identity_and_source_locations() {
    for source in [
        "local t={};local f=coroutine.wrap(function()coroutine.yield(1);error(t)end);local x=f();local a,b=pcall(f);local c,d=pcall(f);return x,a,b==t,c,d",
        "local f=coroutine.wrap(function()error('bad')end);return pcall(f)",
        "local f=coroutine.wrap(function()error(17)end);return pcall(f)",
        "local f=coroutine.wrap(function()error(nil)end);return pcall(f)",
        "local f=coroutine.wrap(function()coroutine.yield(3);error('bad',0)end);local g=coroutine.wrap(function()return f()end);local a=g();return a,pcall(g)",
        "local c=coroutine.create(function()local s=string.gsub('abc','.',function()error('failed')end);return s end);return coroutine.resume(c)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn suspended_threads_persist_across_host_executions_and_quota_abort_restores_main() {
    let mut vm = Vm::new();
    vm.execute(&compile(b"c=coroutine.create(function()local n=7;f=function()n=n+1;return n end;coroutine.yield();return n end);assert(coroutine.resume(c))").unwrap()).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return f(),coroutine.status(c)").unwrap())
            .unwrap()[0],
        Value::Number(8.0)
    );
    assert_eq!(
        vm.execute(&compile(b"local ok,n=coroutine.resume(c);return ok,n,f()").unwrap())
            .unwrap(),
        &[Value::Boolean(true), Value::Number(8.0), Value::Number(9.0)]
    );
    vm.execute(&compile(b"idle=coroutine.create(function()local x=coroutine.yield(3);return x end);assert(coroutine.resume(idle))").unwrap()).unwrap();
    vm.set_instruction_limit(Some(1000));
    let e=vm.execute(&compile(b"bad=coroutine.create(function()local n=10;g=function()return n end;while true do n=n+1 end end);coroutine.resume(bad)").unwrap()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Limit);
    let values = vm
        .execute(
            &compile(
                b"return coroutine.running(),coroutine.status(bad),g()>10,coroutine.status(idle)",
            )
            .unwrap(),
        )
        .unwrap()
        .to_vec();
    assert_eq!(values[0], Value::Nil);
    assert_eq!(values[2], Value::Boolean(true));
    let Value::String(dead) = values[1] else {
        panic!("thread status is a string")
    };
    let Value::String(suspended) = values[3] else {
        panic!("thread status is a string")
    };
    assert_eq!(vm.string_bytes(dead), Some(b"dead".as_slice()));
    assert_eq!(vm.string_bytes(suspended), Some(b"suspended".as_slice()));
    assert_eq!(
        vm.execute(&compile(b"return coroutine.resume(idle,4)").unwrap())
            .unwrap(),
        &[Value::Boolean(true), Value::Number(4.0)]
    );
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.0)]
    );
}
