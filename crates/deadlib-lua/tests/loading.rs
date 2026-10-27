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
        .unwrap_or_else(|e| panic!("reference: {e}: {}", String::from_utf8_lossy(source)))
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

#[test]
fn binary_headers_and_names_report_native_load_errors() {
    compare(
        br#"
      local header='\27Lua'..string.char(81,0,1,4,8,4,8,0)
      local values={'\27',header:sub(1,11),header:sub(1,4)..'Z'..header:sub(6),
        header..string.rep('\0',8)..string.rep('\255',4)}
      local out={}
      for _,bytes in ipairs(values)do
        for _,name in ipairs({'=probe','@a.lua','plain','a\0ignored',''})do
          local f,e=loadstring(bytes,name);out[#out+1]=f==nil;out[#out+1]=e
        end
        local f,e=loadstring(bytes);out[#out+1]=f==nil;out[#out+1]=e
      end
      return unpack(out)
    "#,
    );
}

#[test]
fn dumps_round_trip_closures_varargs_scopes_and_thread_environments() {
    compare(br#"
      local a,b=20,30
      local original=function(x,...)
        if x=='set' then a=10+b;b=b+1 else return a,... end
      end
      local bytes=string.dump(original,true,newproxy())
      assert(bytes==string.dump(original,false))
      local f=assert(loadstring(bytes,'=ignored'));local out={f()==nil}
      out[#out+1]=debug.setupvalue(f,1,'hi');out[#out+1]=f()
      out[#out+1]=debug.setupvalue(f,2,13);f('set');out[#out+1]=f();f('set');out[#out+1]=f()
      local once=false;local g=assert(load(function()if not once then once=true;return bytes end end))
      out[#out+1]=g()==nil;out[#out+1]=a;out[#out+1]=b
      local global=_G;local env=setmetatable({x=17},{__index=global})
      local h=loadstring('return x');setfenv(h,{x=3});local encoded=string.dump(h)
      setfenv(0,env);local k=assert(loadstring(encoded));out[#out+1]=k();out[#out+1]=getfenv(k)==env
      return unpack(out)
    "#);
    compare(
        br#"
      local function f(a,...)return a,arg.n,arg[1],arg[2]end
      local g=assert(loadstring(string.dump(f)));return g(1,2,3)
    "#,
    );
}

#[test]
fn dump_argument_errors_keep_native_messages_and_return_arity() {
    compare(br#"
      local out={}
      for _,args in ipairs({{},{false},{true},{3},{'abc'},{{}},{print},{string.dump}})do
        local ok,e=pcall(string.dump,unpack(args));out[#out+1]=ok;out[#out+1]=e
      end
      local function f()return 7 end
      out[#out+1]=select('#',string.dump(f));out[#out+1]=type(string.dump(f));out[#out+1]=assert(loadstring(string.dump(f)))()
      return unpack(out)
    "#);
}

#[test]
fn dump_return_hooks_see_the_retained_function_and_result_slots() {
    compare(
        br#"
      local out={};local function f()return 9 end
      debug.sethook(function(e)
        if e=='return' and debug.getinfo(2,'f').func==string.dump then
          for i=1,4 do local n,v=debug.getlocal(2,i);out[#out+1]=tostring(n)..':'..type(v)end
          collectgarbage()
        end
      end,'r')
      local bytes=string.dump(f,newproxy(),newproxy());debug.sethook()
      return assert(loadstring(bytes))(),table.concat(out,'|')
    "#,
    );
}

#[test]
fn long_dump_relays_retain_the_branch_scope_and_source_line() {
    compare(br#"
      local body='local x=0\nif x==1 then\nlocal hidden=9\n'..string.rep('x=x+1\n',80000)..'end\nreturn x'
      local original=assert(loadstring(body,'=relay scope'))
      local f=assert(loadstring(string.dump(original)))
      local valid=true;local visits=0
      debug.sethook(function()
        local d=debug.getinfo(2,'fl')
        if d.func==f then
          visits=visits+1;local name=debug.getlocal(2,2)
          if name=='hidden' or (d.currentline>2 and d.currentline<80004) then valid=false end
        end
      end,'',1)
      local result=f();debug.sethook()
      return result,valid,visits>0
    "#);
}

#[test]
fn binary_readers_stop_at_errors_and_preserve_the_initial_probe() {
    compare(br#"
      local header='\27Lua'..string.char(81,0,1,4,8,4,8,0)
      local bad=header..string.rep('\0',8)..string.rep('\255',4)..'unused'
      local out={}
      for _,empty in ipairs({false,true})do for size=1,4 do
        local at,calls=0,0
        local f,e=load(function()
          calls=calls+1;collectgarbage()
          if empty and calls==1 then return ''end
          local s=bad:sub(at+1,at+size);at=at+#s;return s
        end,'=reader')
        out[#out+1]=f==nil;out[#out+1]=e;out[#out+1]=calls;out[#out+1]=at
      end end
      local marker={};local calls=0
      local ok,f,e=pcall(load,function()calls=calls+1;if calls==1 then return '\27Lua'else error(marker,0)end end)
      assert(ok)
      out[#out+1]=f==nil;out[#out+1]=e==marker;out[#out+1]=calls
      return unpack(out)
    "#);
}

#[test]
fn reader_load_preserves_chunks_eof_coercions_and_result_counts() {
    for source in [
        "local n=0;local f,e=load(function(...)assert(select('#',...)==0);n=n+1;if n==1 then return 'return ' elseif n==2 then return 17 end end);return type(f),e,f(),n",
        "local n=0;local f=load(function()n=n+1;if n==1 then return '' elseif n==2 then return 'return 5' end end);return f(),n",
        "local n=0;local f=load(function()n=n+1;return '' end);return select('#',f()),n",
        "local n=0;local f=load(function()n=n+1;return nil,'ignored' end);return select('#',f()),n",
        "local n=0;local f=load(function()n=n+1;if n==1 then return 'return 7, nil, 9',false end end);return f(),select('#',f()),n",
        "local f=load(string.gmatch('return 23','.'));return f()",
        "local f=load(coroutine.wrap(function()coroutine.yield('return ');coroutine.yield('31')end));return f()",
    ] {
        compare(source.as_bytes());
    }
    compare(br#"local parts={'return "','\000','a','"'};local n=0;local f=load(function()n=n+1;return parts[n]end);return f(),n"#);
}

#[test]
fn reader_errors_are_protected_and_keep_arbitrary_lua_objects() {
    for source in [
        "local tag={};local ok,f,e=pcall(load,function()error(tag)end);return ok,f,e==tag",
        "local tag=newproxy();local ok,f,e=pcall(load,function()error(tag,0)end);return ok,f,e==tag",
        "local ok,f,e=pcall(load,function()error(-0,0)end);return ok,f,e",
        "local ok,f,e=pcall(load,function()error(nil,0)end);return ok,select('#',f,e),f,e",
        "local ok,f,e=pcall(load,function()return {}end);return ok,f,type(e),string.find(e,'reader function must return a string',1,true)~=nil",
        "local a=pcall(load,'return 1');local b=pcall(load,setmetatable({},{__call=function()end}));local c=pcall(load,function()end,{});return a,b,c",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn reader_runtime_errors_inherit_the_active_xpcall_handler() {
    compare(br#"
        local tag,handled={},{};local calls=0
        local ok,f,e=xpcall(function()return load(function()error(tag)end)end,function(e)assert(e==tag);calls=calls+1;return handled end)
        return ok,f,e==handled,calls
    "#);
    compare(br#"
        local tag={};local calls=0
        local ok,protected,f,e=xpcall(function()return pcall(load,function()error(tag)end)end,function()calls=calls+1;return 'handled'end)
        return ok,protected,f,e==tag,calls
    "#);
    compare(
        br#"
        local tag,handled={},{};local calls=0
        local ok,f,e=xpcall(function()return load(function()error(tag)end)end,function(e)
            calls=calls+1;collectgarbage();local once=false
            local inner=load(function()if once then return nil end;once=true;return 'return 37'end)
            assert(inner()==37);return handled
        end)
        return ok,f,e==handled,calls
    "#,
    );
    compare(br#"
        local calls=0;local ok,f,e=xpcall(function()local n=0;return load(function()n=n+1;if n==1 then return ') 'end end)end,function()calls=calls+1;return 'handled'end)
        return ok,f,type(e),calls
    "#);
    compare(br#"
        local calls=0;local ok,f,e=xpcall(function()return load(function()return {}end)end,function()calls=calls+1;return 'handled'end)
        return ok,f,e,calls
    "#);
}

#[test]
fn reader_calls_stop_at_early_errors_and_cross_token_boundaries() {
    compare(br#"
        local firsts={') ','local 1 ','break; ','return function()return ... '}
        local rows={}
        for _,s in ipairs(firsts)do local n=0;local f,e=load(function()n=n+1;if n==1 then return s end;error('unused',0)end);rows[#rows+1]=n..':'..type(f)..':'..type(e)end
        local tag={};local n=0;local ok,f,e=pcall(load,function()n=n+1;if n==1 then return ')'end;error(tag)end)
        return table.concat(rows,','),n,ok,f,e==tag
    "#);
    compare(br#"
        local cases={'return 1+2','local t={a=1,b,[2]=3};return t.a','return function(a,...)return a,... end',') ','local 1 ','break; ','return function()return ... end','f\n()','return {a=}','return 1 2','return ...','local function f(a,)end'}
        local rows={}
        for k,s in ipairs(cases)do
            for split=0,#s do
                local parts={string.sub(s,1,split),string.sub(s,split+1)};local n=0
                local ok,f,e=pcall(load,function()n=n+1;return parts[n]end)
                rows[#rows+1]=k..'/'..split..'/'..n..'/'..tostring(ok)..'/'..type(f)..'/'..type(e)
            end
        end
        return table.concat(rows,';')
    "#);
}

#[test]
fn local_limits_stop_reader_requests_at_the_declaration() {
    compare(br#"
        local prefix='';for i=1,201 do prefix=prefix..'local a'..i..'=0; 'end
        local n=0;local tag={};local ok,f,e=pcall(load,function()n=n+1;if n==1 then return prefix end;error(tag)end)
        return ok,type(f),type(e),e==tag,n
    "#);
    compare(br#"
        local function locals(n)local s='';for i=1,n do s=s..'local a'..i..'=0; 'end;return s end
        local function params(n)local t={};for i=1,n do t[i]='p'..i end;return table.concat(t,',')end
        local cases={
            'return function('..params(201)..')return 0 end ',
            'return function('..params(200)..',...)return 0 end ',
            'local t={};function t:f('..params(199)..',...)return 0 end ',
            locals(197)..'for i=1,2 do end ',
            locals(197)..'for k in pairs({})do end ',
            locals(196)..'for i=1,2 do end ',
            locals(100)..'do '..locals(100)..'end;do '..locals(100)..'end ',
            locals(199)..'local function f()local a=1 end '
        }
        local rows={}
        for _,s in ipairs(cases)do
            local tag={};local n=0;local ok,f,e=pcall(load,function()n=n+1;if n==1 then return s end;error(tag)end)
            rows[#rows+1]=tostring(ok)..'/'..type(f)..'/'..type(e)..'/'..tostring(e==tag)..'/'..n
        end
        return table.concat(rows,';')
    "#);
}

#[test]
fn loaded_chunks_inherit_thread_globals_after_the_reader_finishes() {
    compare(
        br#"
        local original=_G;local changed=setmetatable({x=17},{__index=original});local n=0
        local f=load(function()n=n+1;if n==1 then return 'return x' end;setfenv(0,changed)end)
        return getfenv(f)==changed,f(),getfenv(load)==changed
    "#,
    );
    compare(
        br#"
        x=11;debug.setfenv(load,{x=99});local n=0
        local reader=function()n=n+1;if n==1 then return 'return x' end end
        setfenv(reader,{x=23});local f=load(reader);return getfenv(f)==_G,f()
    "#,
    );
}

#[test]
fn reader_state_survives_collection_and_retains_the_reserved_slot() {
    compare(br#"
        collectgarbage('stop');local n=0;local p=newproxy(true);getmetatable(p).__gc=function()n=n+1 end
        local calls=0;local seen={};local f=load(function()calls=calls+1;collectgarbage();seen[calls]=n;if calls==1 then return 'return 7' end end,nil,newproxy(p),newproxy(p))
        return seen[1],seen[2],n,f()
    "#);
    compare(br#"
        local source='local t={x="'..string.rep('a',10000)..'"};return #t.x,17';local at=0;local captured={n=7}
        local f=load(function()collectgarbage();assert(captured.n==7);if at>=#source then return nil end;local s=string.sub(source,at+1,at+100);at=at+100;return s end)
        collectgarbage();return f()
    "#);
}

#[test]
fn nested_loads_share_heap_without_recursive_interpreter_entry() {
    compare(br#"
        local done=false
        local f=load(function()if done then return nil end;done=true;local once=false
            local inner=load(function()if once then return nil end;once=true;return 'return "return 37"' end)
            collectgarbage();return inner()
        end)
        return f()
    "#);
    compare(br#"
        local function compile(n)
            local once=false
            return load(function()if once then return nil end;once=true;if n==0 then return 'return 1' end;local f=compile(n-1);return 'return '..(f()+1)end)
        end
        return compile(50)()
    "#);
    compare(br#"
        local visits=0
        local function nest(n)
            local once=false
            return load(function()if once then return nil end;once=true;visits=visits+1;if n==0 then return 'return 1'end;local f,e=nest(n-1);if not f then error(e,0)end;return 'return '..(f()+1)end)
        end
        local ok,f,e=pcall(nest,300);return ok,type(f),type(e),visits
    "#);
}

#[test]
fn generated_chunk_widths_preserve_closures_varargs_and_binary_literals() {
    compare(br#"
        local source=[==[--[=[comment]=]
            local data="\000ab\255"..[=[x]=]
            local function fact(n)if n<2 then return 1 end;return n*fact(n-1)end
            return function(n,...)collectgarbage();return fact(n)+#data,... end
        ]==]
        local hash=0
        for width=1,13 do
            local at=0;local f=load(function()if at>=#source then return nil end;local s=string.sub(source,at+1,at+width);at=at+width;return s end)
            local fn=f();collectgarbage();local a,b,c,d=fn(5,7,nil,9);assert(c==nil)
            hash=(hash*31+a+b+d)%10000019
        end
        return hash
    "#);
}

#[test]
fn one_byte_readers_handle_large_comments_literals_and_constructor_lists() {
    compare(br#"
        local sources={
            '--[=['..string.rep('x',100000)..']=]\nreturn 17',
            'return "'..string.rep('x',100000)..'"',
            'local t={'..string.rep('1,',1000)..'};return #t'
        }
        local out={}
        for k,s in ipairs(sources)do
            local at=0;local f=load(function()at=at+1;if at>#s then return nil end;return string.sub(s,at,at)end)
            collectgarbage();local v=f();out[k]=type(v)=='string' and #v or v;assert(at==#s+1)
        end
        return out[1],out[2],out[3]
    "#);
}

#[test]
fn repeated_reader_loads_reclaim_code_closures_and_transient_state() {
    let mut vm = Vm::new();
    let program=compile(b"for i=1,1000 do local once=false;local f=load(function()if once then return nil end;once=true;return 'return '..i end);assert(f()==i)end;return nil").unwrap();
    let mut maximum = 0;
    for _ in 0..8 {
        vm.execute(&program).unwrap();
        let stats = vm.collect_garbage().unwrap();
        maximum = maximum.max(stats.reserved_bytes);
        assert!(stats.functions < 10);
        assert!(stats.programs < 10);
    }
    assert!(
        maximum < 4_000_000,
        "reusable storage grows with peak live data: {maximum}"
    );
    assert!(vm.heap_stats().reclaimed_objects > 20_000);
}

#[test]
fn reader_yields_are_caught_and_child_coroutines_can_yield() {
    compare(br#"
        local c=coroutine.create(function()return load(function()return coroutine.yield('return 1')end)end)
        local ok,f,e=coroutine.resume(c);return ok,f,type(e),coroutine.status(c)
    "#);
    compare(br#"
        local n=0;local c=coroutine.create(function()local f=load(function()n=n+1;if n>1 then return nil end;local child=coroutine.create(function()coroutine.yield('return 41')end);local ok,s=coroutine.resume(child);assert(ok);collectgarbage();return s end);return f()end)
        return coroutine.resume(c)
    "#);
}

#[test]
fn instruction_abort_discards_reader_jobs_and_restores_main() {
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(1000));
    let program = compile(b"return load(function()return ' ' end)").unwrap();
    assert_eq!(
        vm.execute(&program).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.0)]
    );
    let program=compile(b"local co=coroutine.create(function()return load(function()return ' 'end)end);return coroutine.resume(co)").unwrap();
    vm.set_instruction_limit(Some(1000));
    assert_eq!(
        vm.execute(&program).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"return coroutine.running(),42").unwrap())
            .unwrap(),
        &[Value::Nil, Value::Number(42.0)]
    );
}
