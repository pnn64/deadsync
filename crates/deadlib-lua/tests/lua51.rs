use deadlib_lua::{ErrorKind, Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as ReferenceValue};

#[derive(Debug, PartialEq)]
enum Observed {
    Nil,
    Boolean(bool),
    Number(u64),
    NaN,
    String(Vec<u8>),
}

fn number(n: f64) -> Observed {
    if n.is_nan() {
        Observed::NaN
    } else {
        Observed::Number(n.to_bits())
    }
}

fn rust(vm: &mut Vm, source: &[u8]) -> Vec<Observed> {
    let program = compile(source)
        .unwrap_or_else(|e| panic!("compile {e}: {}", String::from_utf8_lossy(source)));
    program.verify().unwrap();
    let values = vm
        .execute(&program)
        .unwrap_or_else(|e| panic!("execute {e}: {}", String::from_utf8_lossy(source)))
        .to_vec();
    values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Observed::Nil,
            Value::Boolean(b) => Observed::Boolean(b),
            Value::Number(n) => number(n),
            Value::String(id) => Observed::String(vm.string_bytes(id).unwrap().to_vec()),
            _ => panic!("test must expose object identity through Lua comparisons"),
        })
        .collect()
}

fn official(lua: &Lua, source: &[u8]) -> Vec<Observed> {
    lua.load(source)
        .set_name("=(deadlib-lua)")
        .eval::<MultiValue>()
        .unwrap()
        .into_iter()
        .map(|v| match v {
            ReferenceValue::Nil => Observed::Nil,
            ReferenceValue::Boolean(b) => Observed::Boolean(b),
            ReferenceValue::Integer(n) => number(n as f64),
            ReferenceValue::Number(n) => number(n),
            ReferenceValue::String(s) => Observed::String(s.as_bytes().to_vec()),
            _ => panic!("test must return scalar observations"),
        })
        .collect()
}

fn check(source: &[u8]) {
    assert_eq!(
        rust(&mut Vm::new(), source),
        official(&Lua::new(), source),
        "{}",
        String::from_utf8_lossy(source)
    );
}

fn check_debug(source: &[u8]) {
    // SAFETY: these fixed oracle programs only use the debug metatable functions;
    // they never load native modules or expose unsafe host pointers.
    let lua = unsafe { Lua::unsafe_new() };
    assert_eq!(
        rust(&mut Vm::new(), source),
        official(&lua, source),
        "{}",
        String::from_utf8_lossy(source)
    );
}

#[test]
fn iteration_preserves_keys_deletions_and_raw_access() {
    for source in [
        "local t={1,2,3,a=4,b=5,[false]=6}; local n,s=0,0; for k,v in pairs(t) do n=n+1; s=s+v end; return n,s",
        "local t={1,false,3,nil,5}; local n,s=0,0; for k,v in ipairs(t) do n=n+1; s=s+k end; return n,s",
        "local t={1,2,3,a=4,b=5,[false]=6}; local n=0; for k,v in pairs(t) do t[k]=nil; n=n+1 end; return n,next(t)",
        "local t={}; for i=1,100 do t['x'..i]=i end; local n,s=0,0; for k,v in pairs(t) do t[k]=nil; n=n+1; s=s+v end; return n,s,next(t)",
        "local a={}; local b=function() end; local t={[a]=7,[b]=9}; local n=0; for k,v in pairs(t) do assert(t[k]==v); n=n+v end; return n",
        "local f,t,k=pairs({}); return f==next,type(t),k",
        "local t={}; for i=1,300 do t.x=i; t.x=nil; t['a'..i]=i; t['a'..i]=nil end; t.final=8; local n=0; for k,v in pairs(t) do n=n+v end; return n",
        "local t={1,2}; local f=next; next=function() error('replaced') end; local g=pairs(t); return f==g",
        "local ok,f,t,k=pcall(ipairs,{}); return pcall(next,{},'absent')==false,pcall(pairs,false)==false,ok,type(f),type(t),k",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn protected_calls_preserve_error_objects_and_arity() {
    for source in [
        "return pcall(function(a,b) return a,nil,b end,1,3)",
        "return pcall(function() end),select('#',pcall(function() end))",
        "local e={}; local ok,value=pcall(error,e); return ok,value==e",
        "return pcall(error,nil),pcall(error,false),pcall(error,42,0)",
        "return pcall(function() error('a\\000b',0) end)",
        "return pcall(assert,false,'custom'),pcall(assert,true,2,3)",
        "local ok,e=pcall(function() return nil+1 end); return ok,type(e)",
        "local function f(n) if n==0 then error(false) end; local ok,e=pcall(f,n-1); return ok,e end; return f(50)",
        "local f; local ok,e=pcall(function() local x=8; f=function(n) x=x+n; return x end; error(false) end); return ok,e,f(2),f(3)",
        "local e={}; return xpcall(function() return 1,nil,3 end,function(v) return e end)",
        "local e={}; local marker={}; local ok,v=xpcall(function() error(e) end,function(v) assert(v==e); return marker,99 end); return ok,v==marker",
        "return xpcall(function() error(false) end,function(e) return e end)",
        "return xpcall(function() error('first',0) end,function(e) error('handler',0) end)",
        "return xpcall(function() error(false) end,false),xpcall(function() return 7 end,false)",
        "local e={}; local ok,v=xpcall(function() error(e) end,function(v) local ok,w=pcall(error,v); return ok==false and w==e end); return ok,v",
        "local e={}; local getter; local ok,v=xpcall(function() local x=7; getter=function(n) x=x+n; return x end; error(e) end,function(v) assert(v==e); return getter(3) end); return ok,v,getter(2)",
        "local function f() return pcall(function() return 7,nil,9 end) end; return f()",
        "return pcall(function() error('custom') end)",
        "return pcall(function() assert(false,'custom') end)",
        "local function f() error('bad',2) end; return pcall(function() f() end)",
        "return pcall(function() error(42) end),pcall(function() error(42,0) end)",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn environments_and_dynamic_compilation_share_state() {
    for source in [
        "local function f() return value end; local e={value=7}; return setfenv(f,e)==f,getfenv(f)==e,f(),getfenv(0)==_G",
        "local original=_G; local function f() setfenv(1,{getfenv=getfenv}); return getfenv()==original end; return f()",
        "local e={x=9}; local function f() return function() return x end end; setfenv(f,e); local g=f(); return getfenv(g)==e,g()",
        "local original=_G; local e=setmetatable({x=8},{__index=original}); local function f() setfenv(1,e); return x,type(x),getfenv()==e end; return f()",
        "return getfenv(assert)==_G,pcall(setfenv,assert,{})==false,pcall(getfenv,-1)==false",
        "return getfenv(nil)==getfenv(),getfenv(nil)==_G",
        "local original=_G; local fresh={}; setfenv(0,fresh); return getfenv(0)==fresh,getfenv(assert)==fresh,getfenv(1)==original",
        "local f=loadstring('local x=0; return function(n) x=x+n; return x end')(); return f(3),f(4)",
        "local e={x=7}; local f=loadstring('return x'); setfenv(f,e); return f(),getfenv(f)==e",
        "local original=_G; local fresh={x=17}; setfenv(0,fresh); local f=loadstring('return x'); return f(),getfenv(f)==fresh,getfenv(1)==original",
        "local f,e=loadstring('invalid + source','test'); return f,type(e)",
        "local f=loadstring('local x=...; return x*2'); return f(21)",
        "local e={}; local ok,v=xpcall(function() error(e) end,function(v) return getfenv(1)==_G and v==e end); return ok,v",
        "local e=setmetatable({},{__index=_G}); local function f() error(false) end; setfenv(f,e); return xpcall(f,function(v) return getfenv(2)==_G and getfenv(3)==e end)",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn named_chunks_and_error_levels_match_reference_locations() {
    let source: &[u8] =
        b"local function f(n)\n if n==0 then error('bad',2) end\n f(n-1)\nend\nreturn pcall(f,3)";
    for name in [
        "=test".to_string(),
        "@/test.lua".into(),
        "first line\nsecond line".into(),
        format!("={}", "x".repeat(100)),
        format!("@/prefix/{}.lua", "x".repeat(100)),
        "x".repeat(100),
    ] {
        let program = deadlib_lua::compile_named(source, name.as_bytes()).unwrap();
        let mut vm = Vm::new();
        let values = vm.execute(&program).unwrap().to_vec();
        let lua = Lua::new();
        let reference = lua
            .load(source)
            .set_name(name.clone())
            .eval::<MultiValue>()
            .unwrap();
        assert_eq!(values[0], Value::Boolean(false));
        let Value::String(id) = values[1] else {
            panic!("string error expected");
        };
        let ReferenceValue::String(expected) = &reference[1] else {
            panic!("reference string error expected");
        };
        assert_eq!(
            vm.string_bytes(id).unwrap(),
            expected.as_bytes().as_ref(),
            "{name}"
        );
    }
}

#[test]
fn metatable_indexing_protection_and_calls() {
    for source in [
        "local backing={x=7}; local t=setmetatable({y=false},{__index=backing}); return t.x,t.y,t.z,rawget(t,'x')",
        "local mt={}; local t=setmetatable({},mt); return getmetatable(t)==mt,setmetatable(t,nil)==t,getmetatable(t)",
        "local t=setmetatable({},{__metatable=false}); return getmetatable(t),pcall(setmetatable,t,{})==false",
        "local n=0; local t=setmetatable({x=false},{__index=function(t,k) n=n+1; return k..n end}); return t.x,t.a,t.b,n",
        "local out={}; local t=setmetatable({x=1},{__newindex=out}); t.x=2; t.y=3; return t.x,t.y,out.y,rawget(t,'y')",
        "local n=0; local t=setmetatable({x=1},{__newindex=function(t,k,v) n=n+1; rawset(t,k,v*2) end}); t.x=2; t.y=3; t.y=4; return n,t.x,t.y",
        "local mt={}; local a=setmetatable({},mt); local b={}; mt.__index=b; b.x=9; return a.x",
        "local t={}; setmetatable(t,{__index=t,__newindex=t}); return pcall(function() return t.x end)==false,pcall(function() t.x=3 end)==false",
        "local f=setmetatable({n=3},{__call=function(self,a,...) return self.n+a,... end}); return f(4,nil,9)",
        "local f=setmetatable({},{__call=function(self,...) return select('#',...),... end}); local function g(...) return f(...) end; return g(1,2,3)",
        "local f=setmetatable({},{__call=assert}); return f(7)==f",
        "local callable=setmetatable({},{__call=function() return 8 end}); local f=setmetatable({},{__call=callable}); return pcall(f)==false",
        "local t=setmetatable({1,2,3},{__len=function() return 999 end,__pairs=function() error('bad') end}); local n=0; for k,v in pairs(t) do n=n+v end; return #t,n",
        "setmetatable(_G,{__index=function(_,k) if k=='missing' then return 42 end end}); return missing,rawget(_G,'missing')",
        "local out={}; setmetatable(_G,{__newindex=out}); unknown=7; return out.unknown,rawget(_G,'unknown')",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn metatable_arithmetic_comparison_and_observable_arguments() {
    for source in [
        "local mt={__add=function(a,b) return a.x+b.x end,__sub=function(a,b) return a.x-b.x end,__mul=function(a,b) return a.x*b.x end,__div=function(a,b) return a.x/b.x end,__mod=function(a,b) return a.x%b.x end,__pow=function(a,b) return a.x^b.x end}; local a=setmetatable({x=7},mt); local b=setmetatable({x=2},mt); return a+b,a-b,a*b,a/b,a%b,a^b",
        "local a=setmetatable({},{__unm=function(a,b) return a==b end}); return -a",
        "local a=setmetatable({},{__add=function(a,b) return type(a),type(b) end}); return a+3,3+a",
        "local mt={__concat=function(a,b) return type(a)..':'..type(b) end}; local a=setmetatable({},mt); return a..3,3..a",
        "local f=function(a,b) return 'truthy',7 end; local a=setmetatable({},{__eq=f}); local b=setmetatable({},{__eq=f}); return a==b,a~=b,rawequal(a,b),a==a",
        "local a=setmetatable({},{__eq=function() return true end}); local b=setmetatable({},{__eq=function() return true end}); return a==b,a~=b",
        "local mt={__lt=function(a,b) return a.x<b.x end}; local a=setmetatable({x=1},mt); local b=setmetatable({x=2},mt); return a<b,a<=b,a>b,a>=b,a<=a",
        "local mt={__lt=function() return false end,__le=function() return true end}; local a=setmetatable({},mt); local b=setmetatable({},mt); return a<b,a<=b,a>b,a>=b",
        "local a=setmetatable({},{__lt=function() return true end}); local b=setmetatable({},{__lt=function() return true end}); return pcall(function() return a<b end)==false",
        "local t=setmetatable({},{__add=assert}); return t+1==t,1+t==1",
        "local handler=setmetatable({},{__call=function(self,a,b) return a.x+b.x end}); local a=setmetatable({x=2},{__add=handler}); local b={x=3}; return a+b",
        "local e={}; local t=setmetatable({},{__index=function() error(e) end}); local ok,v=pcall(function() return t.x end); return ok,v==e",
    ] {
        // A numeric literal adjacent to '..' is a lexical error in 5.1.
        check(source.replace("3..a", "3 ..a").as_bytes());
    }
}

#[test]
fn primitive_metatables_and_tostring_follow_lua51() {
    for source in [
        "debug.setmetatable(false,{__index=function(v,k) return not v and k end}); return false.x, getmetatable(true)==getmetatable(false)",
        "debug.setmetatable(0,{__len=function(v,other) return v,other end}); return #7",
        "debug.setmetatable(nil,{__len=function(v,other) return v==false and other==nil end}); return #false",
        "debug.setmetatable(0,{__add=function() return 999 end}); return 1+2,'3'+4",
        "local mt={__metatable='locked'}; local t=setmetatable({},mt); return debug.getmetatable(t)==mt,debug.setmetatable(t,nil),getmetatable(t)",
        "local t=setmetatable({},{__tostring=function(v) return 'custom' end}); return tostring(t),tostring(nil),tostring(false),tostring('bytes')",
        "local t=setmetatable({},{__tostring=function(v) return v end}); return tostring(t)==t",
        "local t=setmetatable({},{__tostring=assert}); return tostring(t)==t",
        "local env=setmetatable({x=7},{__index=_G}); local f; f=function() local unused=f[env]; return x end; debug.setmetatable(f,{__index=setfenv}); local value=f(); debug.setmetatable(f,nil); return value",
    ] {
        check_debug(source.replace("false.x", "(false).x").as_bytes());
    }
}

#[test]
fn print_caches_tostring_and_preserves_output_before_errors() {
    let mut vm = Vm::new();
    vm.set_output_capture(true);
    rust(
        &mut vm,
        b"print('hello',1,false); print(); print('a\\000b')",
    );
    assert_eq!(vm.output(), Some(b"hello\t1\tfalse\n\na\n".as_slice()));
    vm.set_output_capture(true);
    rust(&mut vm,b"local old=tostring; tostring=function(v) tostring=function() return 'new' end; return old(v) end; print(1,2); print(3)");
    assert_eq!(vm.output(), Some(b"1\t2\nnew\n".as_slice()));
    vm.set_output_capture(true);
    let values=rust(&mut vm,b"tostring=function(v) if v==2 then return {} end; return v end; local ok,e=pcall(print,1,2); return ok,type(e)");
    assert_eq!(
        values,
        vec![
            Observed::Boolean(false),
            Observed::String(b"string".to_vec())
        ]
    );
    assert_eq!(vm.output(), Some(b"1".as_slice()));
}

#[test]
fn generated_metamethod_dispatch_preserves_order_and_identity() {
    let operators = [
        ("+", "__add"),
        ("-", "__sub"),
        ("*", "__mul"),
        ("/", "__div"),
        ("%", "__mod"),
        ("^", "__pow"),
        ("..", "__concat"),
        ("==", "__eq"),
        ("~=", "__eq"),
        ("<", "__lt"),
        ("<=", "__le"),
        (" > ", "__lt"),
        (" >= ", "__le"),
    ];
    let handlers = ["nil", "h", "g", "false", "proxy"];
    let lua = Lua::new();
    let mut vm = Vm::new();
    for (op, event) in operators {
        for left in handlers {
            for right in handlers {
                let source = format!(
                    r#"
                    local log=''; local a,b
                    local function h(x,y) log=log..'h'; return x end
                    local function g(x,y) log=log..'g'; return false end
                    local proxy=setmetatable({{}},{{__call=function(_,x,y) log=log..'p'; return y end}})
                    a=setmetatable({{}},{{{event}={left}}}); b=setmetatable({{}},{{{event}={right}}})
                    local ok,value=pcall(function() return a {op} b end)
                    local scalar=ok and (type(value)=='boolean' or type(value)=='number' or type(value)=='string')
                    return ok,type(value),scalar and value or nil,rawequal(value,a),rawequal(value,b),log
                "#
                );
                assert_eq!(
                    rust(&mut vm, source.as_bytes()),
                    official(&lua, source.as_bytes()),
                    "{event}: {left}, {right}"
                );
            }
        }
    }
}

#[test]
fn expression_order_and_binary64() {
    for source in [
        "return -2^2, 2^3^2, 3+4*5, (3+4)*5, -0, 1/-0, 1/0, 0/0",
        "return -3%2, 3%-2, -3%-2, 1%0, 1%math.huge, math.huge%1",
        "return nil or false or 0, 0 and 'yes', false and error('bad'), true or error('bad')",
        "return '1' + ' 2 ', -'3.5', '0x10' + 1, '1e2' / 2, 0 == -0, (0/0) == (0/0)",
        "return 0xabcdef, 0x123456789abcdef, 0Xffffffffffffffff, 0x1p2",
        "return 'a'..'b'..'c', #('ab'..'cd'), 1..'a'",
        "return not nil, not false, not 0, 1 < 2, 1 <= 1, 3 >= 2, 3 > 4, 'a' < 'b'",
        "return 'a\\000b' < 'a\\000c', 'a\\000b' == 'a\\000b', #'a\\000b'",
    ] {
        // Adjacent numeral/concatenation is intentionally malformed in 5.1.
        if source.contains("1..") {
            continue;
        }
        check(source.as_bytes());
    }
}

#[test]
fn byte_strings_and_long_delimiters() {
    for source in [
        b"return '\\a\\b\\f\\n\\r\\t\\v\\\\\\\"\\\'\\000\\255'".as_slice(),
        b"return '\\q\\z\\x41', '\\12a3', '\\1234'",
        b"return [=[\r\nalpha\n\rbeta\rgamma\ndelta]=], [[x]]",
        b"--[==[ ignored ]=] and [[ and \r\n ]==]\nreturn 'ok'",
        b"return 'before\\\r\nafter', '\xff\x00\xfe'",
        b"return [==[a]=]b]==]",
    ] {
        check(source);
    }
}

#[test]
fn locals_assignments_and_scopes() {
    for source in [
        "x=3; local x=x+1; do local x=x+1; y=x end; return x,y,_G.x",
        "local a,b,c=1,2; a,b=b,a; return a,b,c",
        "local a=0; a,a=1,2; return a",
        "local a,b=10,20; local a,b=b,a; return a,b",
        "local t={10,20,30}; local i=1; i,t[i]=2,99; return i,t[1],t[2]",
        "local t={}; local u=t; t,t.x={},7; return u.x,t.x,t==u,u==u",
        "local t={}; local a=t; a.x,a.x=1,2; return t.x",
        "local a,b,c=unpack({4,5,6}); return a,b,c",
        "local a,b,c=1,unpack({4,5,6}); return a,b,c",
        "local a,b=unpack({}); return a,b",
        "local a=1,assert(true),rawset(_G,'side',9); return a,side",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn control_flow_and_numeric_for() {
    for source in [
        "local sum=0; for i=1,100 do sum=sum+i; i=999 end; return sum",
        "local sum=0; for i='10','1','-2' do sum=sum+i end; return sum",
        "local i=0; while i<100 do i=i+1; if i==7 then break end end; return i",
        "local i=0; repeat local done=i==5; i=i+1 until done; return i",
        "local x=0; if false then x=1 elseif true then x=2 else x=3 end; return x",
        "local x=0; for i=1,10 do for j=1,10 do if j==3 then break end; x=x+1 end end; return x",
        "local n=0; for i=0,1,math.huge do n=n+1 end; return n",
        "local n=0; for i=1,0,0 do n=n+1; if n==3 then break end end; return n",
        "local n=0; for i=0/0,3 do n=n+1 end; return n",
        "local n=0; for i=1,0/0 do n=n+1 end; return n",
        "local n=0; for i=1,3,0/0 do n=n+1 end; return n",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn tables_calls_and_open_results() {
    for source in [
        "local t={1,2,3,x=4,[true]=5}; return #t,t[2],t.x,t[true],t[false]",
        "local t={}; t[3]=30; t[1]=10; t[2]=20; return #t,t[1],t[2],t[3]",
        "local t={}; t[-0]=1; t[0]=2; return t[-0],t[0],t[0/0],t[nil]",
        "local t={}; local key={}; t[key]=3; t[assert]=4; return t[key],t[{}],t[assert]",
        "local t={1,2,3}; t[2]=nil; return t[2],t[3],t==t,rawequal(t,t)",
        "return unpack({1,false,'a'})",
        "return 0,unpack({1,2,3})",
        "return (unpack({1,2,3})),unpack({})",
        "return select('#',unpack({1,2,3})),select(2,unpack({1,2,3}))",
        "return assert(1,2,3)",
        "local t={method=assert}; return t:method(9)==t",
        "local a,b,c,d=assert(unpack({1,2,3,4})); return a,b,c,d",
        "return math.floor(-2.5),math.ceil(-2.5),math.abs(-2),math.min(3,1,2),math.max(3,1,2)",
        "local t={1,[1]=2}; return t[1]",
        "local t={unpack({1,2,3})}; return #t,unpack(t)",
        "local t={0,unpack({1,2,3})}; return #t,unpack(t)",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn closures_recursion_upvalues_and_varargs() {
    for source in [
        "local function fib(n) if n<2 then return n end; return fib(n-1)+fib(n-2) end; return fib(15)",
        "local function pair() local x=0; return function(n) x=x+n; return x end,function() return x end end; local a,b=pair(); return a(3),b(),a(4),b()",
        "local x=1; local a=function() return x end; x=7; local x=99; return a(),x",
        "local f; do local x=7; f=function() x=x+1; return x end end; local x=99; return f(),f(),x",
        "local function outer() local x=4; return function() return function(n) x=x+n; return x end end end; local g=outer()(); return g(3),g(4)",
        "local t={}; for i=1,3 do t[i]=function() return i end end; return t[1](),t[2](),t[3]()",
        "local t={}; local n=0; while n<3 do local x=n; t[n+1]=function() return x end; n=n+1 end; return t[1](),t[2](),t[3]()",
        "local f; repeat local x=3; f=function() return x end until true; return f()",
        "local f; while true do local x=8; f=function() return x end; break end; return f()",
        "local function f(a,b,...) return a,b,select('#',...),... end; return f(1,2,3,nil,5)",
        "local function f(...) return arg.n,arg[1],arg[2] end; return f(3,4)",
        "local function f(...) return arg,... end; return f(3,4)",
        "local function f(a,b) return a,b end; return f(1),f(1,2,3)",
        "local function f(...) return ... end; local a,b,c=f(1,2,3); return a,b,c,f(f(4,5,6))",
        "local t={x=7}; function t:add(n) self.x=self.x+n; return self.x end; return t:add(3),t:add(4)",
        "local function iter(s,k) if k<s then return k+1,(k+1)*2 end end; local n=0; for k,v in iter,5,0 do n=n+k+v end; return n",
        "local function iter(s,k) if k==nil then return false,7 end end; local n=0; for k,v in iter do n=n+v end; return n",
        "local function f() return 1,nil,3 end; return f(),(f()),select('#',f())",
        "local function f() end; local a,b=f(); return a,b,select('#',f())",
        "local function f(n,...) if n==0 then return ... end; return f(n-1,...) end; return f(100000,2,nil,4)",
        "local function f(n) local x=n; if n==0 then return function() return x end end; saved=function() return x end; return f(n-1) end; local g=f(3); return g(),saved()",
        "local function g(...) return arg.n,arg[1],arg[2] end; local function f(n,...) if n==0 then return g(...) end; return f(n-1,...) end; return f(7,3,4)",
    ] {
        check(source.as_bytes());
    }
}

#[test]
fn escaped_closures_keep_programs_and_shared_cells_alive() {
    let mut vm = Vm::new();
    rust(
        &mut vm,
        b"local x=0; inc=function(n) x=x+n; return x end; read=function() return x end",
    );
    assert_eq!(
        rust(&mut vm, b"return inc(7),read(),inc(3),read()"),
        vec![number(7.0), number(7.0), number(10.0), number(10.0)]
    );
    let program = compile(b"local x=8; saved=function() return x end; error('failure')").unwrap();
    assert!(vm.execute(&program).is_err());
    assert_eq!(
        rust(&mut vm, b"return saved(),read()"),
        vec![number(8.0), number(10.0)]
    );
}

#[test]
fn malformed_source_matches_official_acceptance() {
    for source in [
        "return 1..'x'",
        "return 1e+",
        "return 123abc",
        "return 0x",
        "return 0x1.2",
        "return 'bad\\256'",
        "return 'bad\n'",
        "return [=[unfinished",
        "return [=bad",
        "--[[unfinished",
        "return 1; return 2",
        ";return 1",
        "local a=1;;return a",
        "return (1)=2",
        "local 2=3",
        "if true return 1 end",
        "while true do break; x=1 end",
        "local f=assert; f\n(1)",
        "break",
        "return true // 2",
        "return 2 & 1",
        "goto label",
    ] {
        let reference = Lua::new().load(source).into_function().is_ok();
        assert!(!reference, "fixture unexpectedly accepted: {source}");
        assert!(
            compile(source.as_bytes()).is_err(),
            "accepted invalid source: {source}"
        );
    }
}

#[test]
fn vm_state_persists_and_errors_leave_it_usable() {
    let mut vm = Vm::new();
    assert_eq!(
        rust(&mut vm, b"saved={}; saved.x=41; return saved.x"),
        vec![number(41.0)]
    );
    assert_eq!(
        rust(&mut vm, b"saved.x=saved.x+1; return saved.x"),
        vec![number(42.0)]
    );
    let err = vm
        .execute(&compile(b"local x = 1;\nreturn x + false").unwrap())
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Runtime);
    assert_eq!(err.line(), 2);
    assert_eq!(rust(&mut vm, b"return saved.x"), vec![number(42.0)]);
    vm.set_instruction_limit(Some(100));
    assert_eq!(
        vm.execute(&compile(b"while true do end").unwrap())
            .unwrap_err()
            .kind(),
        ErrorKind::Limit
    );
    assert_eq!(rust(&mut vm, b"return saved.x"), vec![number(42.0)]);
}

#[test]
fn independent_programs_and_vms() {
    let p = compile(b"counter=(counter or 0)+1; return counter, 'bytes'").unwrap();
    let mut a = Vm::new();
    let mut b = Vm::new();
    a.execute(&p).unwrap();
    a.execute(&p).unwrap();
    assert_eq!(b.execute(&p).unwrap()[0], Value::Number(1.0));
    assert_eq!(a.execute(&p).unwrap()[0], Value::Number(3.0));
}

#[test]
fn deterministic_expression_differential_fuzz() {
    fn next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }
    fn expression(state: &mut u64, depth: u8) -> String {
        if depth == 0 {
            return (next(state) % 31).to_string();
        }
        let op = [
            "+", "-", "*", "/", "%", "^", "<", "<=", "==", "~=", "and", "or",
        ][(next(state) % 12) as usize];
        // Comparison/logical expressions are isolated from arithmetic parents.
        if matches!(op, "<" | "<=" | "==" | "~=" | "and" | "or") {
            format!("({} {op} {})", next(state) % 31, next(state) % 31)
        } else {
            format!(
                "({} {op} {})",
                expression_number(state, depth - 1),
                expression_number(state, depth - 1)
            )
        }
    }
    fn expression_number(state: &mut u64, depth: u8) -> String {
        if depth == 0 {
            return (next(state) % 31).to_string();
        }
        let op = ["+", "-", "*", "/", "%"][(next(state) % 5) as usize];
        format!(
            "({} {op} {})",
            expression_number(state, depth - 1),
            expression_number(state, depth - 1)
        )
    }
    let mut state = 0x87ac24e51298ab91;
    let lua = Lua::new();
    let mut vm = Vm::new();
    for _ in 0..1000 {
        let source = format!("return {}", expression(&mut state, 3));
        assert_eq!(
            rust(&mut vm, source.as_bytes()),
            official(&lua, source.as_bytes()),
            "{source}"
        );
    }
}

#[test]
fn adversarial_input_has_bounded_nesting() {
    let sources = [
        format!("return {}1{}", "(".repeat(1000), ")".repeat(1000)),
        format!("return {}1", "x+".repeat(10000)),
        format!("{}{}", "do ".repeat(1000), "end ".repeat(1000)),
    ];
    for source in sources {
        assert_eq!(
            compile(source.as_bytes()).unwrap_err().kind(),
            ErrorKind::Limit
        );
    }
    let folded = format!("return {}1", "1+".repeat(10000));
    assert_eq!(
        Vm::new()
            .execute(&compile(folded.as_bytes()).unwrap())
            .unwrap(),
        &[Value::Number(10001.0)]
    );
}

#[test]
fn arbitrary_bytes_and_generated_constructor_boundaries() {
    let mut state = 0xd15ea5e5_713baabc_u64;
    for length in 0..256 {
        for _ in 0..20 {
            let source: Vec<_> = (0..length)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    state as u8
                })
                .collect();
            if let Ok(program) = compile(&source) {
                program.verify().unwrap();
            }
        }
    }
    for length in [49, 50, 51, 99, 100, 101] {
        let fields = (1..=length)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let source =
            format!("local t={{{fields},[1]=999,[{length}]=999}}; return #t,t[1],t[{length}]");
        check(source.as_bytes());
    }
}
