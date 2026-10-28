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
fn library_return_hooks_share_retained_temporaries_and_the_result_suffix() {
    compare(
        br#"
local function pack(...)return {n=select('#',...),...}end
local function scalar(v)
 local t=type(v)
 if t=='string'or t=='number'or t=='boolean'or t=='nil'then return t..':'..tostring(v)end
 if t=='function'then local d=debug.getinfo(v,'Su');return t..':'..d.what..':'..d.nups end
 if t=='userdata'then return t..':'..tostring(io.type(v))end
 return t
end
local tag={};local out={};local f=(function(x)return function()return x end end)(17)
local tests={
 {table.concat,pack({17,29},':')},{table.getn,pack({17,29},tag)},
 {table.maxn,pack({17,29},tag)},{table.insert,pack({},17)},
 {table.insert,pack({},1,17)},{table.remove,pack({17,29},1,tag)},
 {table.remove,pack({},99,tag)},{table.sort,pack({29,17},nil,tag)},
 {table.foreach,pack({17},function()return tag end,tag)},
 {table.foreachi,pack({17},function()return tag end,tag)},
 {debug.getregistry,pack(17,tag)},{debug.setmetatable,pack({},nil,tag)},
 {debug.getinfo,pack(f,'SufL',tag)},{debug.getlocal,pack(0,1,tag)},
 {debug.setlocal,pack(0,1,coroutine.create(function()end),tag)},
 {debug.getupvalue,pack(f,1,tag)},{debug.setupvalue,pack(f,1,29,tag)},
 {debug.gethook,pack(nil,tag)},{debug.traceback,pack('message',999,tag)},
 {os.difftime,pack(12345,12300,tag)},{os.date,pack('!%Y-%m-%d',1234567890,tag)},
 {os.time,pack({year=2009,month=2,day=13,hour=12,min=0,sec=0,isdst=false},tag)},
 {io.type,pack(newproxy(true),tag)},
}
local file=assert(io.tmpfile());file:write('17\nhello\n')
local methods=getmetatable(file).__index
for _,case in ipairs{
 {methods.read,pack(file,'*n','*l')},{methods.write,pack(file,17,'abc')},
 {methods.seek,pack(file,'set',0,tag)},{methods.flush,pack(file,tag)},
 {methods.setvbuf,pack(file,'full',128,tag)},{methods.lines,pack(file,tag)},
 {io.type,pack(file,tag)},
}do tests[#tests+1]=case end
for _,case in ipairs(tests)do
 for mode=0,2 do
  file:seek('set',0)
  local fn,args=case[1],case[2]
  local co=coroutine.create(function()return fn(unpack(args,1,args.n))end)
  debug.sethook(co,function(e)
   if e=='return'and debug.getinfo(2,'f').func==fn then
    local n=0
    for i=1,40 do local name,v=debug.getlocal(2,i);if not name then break end
     n=i;out[#out+1]=name..'='..scalar(v)
    end
    out[#out+1]='top='..n
    local slot=mode==1 and 1 or n-1
    if mode~=0 and slot>0 then assert(debug.setlocal(2,slot,'hook-token'))end
    collectgarbage()
   end
  end,'r')
  local r=pack(coroutine.resume(co))
  for i=1,r.n do out[#out+1]='result='..scalar(r[i])end
 end
end
assert(file:close())
return table.concat(out,'|')
"#,
    );
}

#[test]
fn extra_setupvalue_arguments_and_nil_io_lines_follow_native_stack_operations() {
    compare(
        br#"
local token={};local x=17;local function f()return x end
local name=debug.setupvalue(f,1,29,token)
local co=coroutine.create(function()
 local ok,e=pcall(io.lines,nil,token)
 return ok,type(e),e:match('FILE%* expected, got nil')~=nil
end)
local ok,a,b,c=coroutine.resume(co)
return name,f()==token,ok,a,b,c
"#,
    );
}

#[test]
fn implicit_line_and_close_arguments_report_closed_default_files() {
    compare(
        br#"
local input,output=io.input(),io.output();local out={}
local f=assert(io.tmpfile());io.input(f);io.output(f);assert(f:close())
for _,fn in ipairs{io.lines,io.close}do
 local ok,e=pcall(fn);out[#out+1]=tostring(ok)..':'..e:match('attempt to use a closed file')
end
io.input(input);io.output(output)
return table.concat(out,'|')
"#,
    );
}

#[test]
fn failed_open_retains_a_real_closed_userdata_then_releases_popped_roots() {
    compare(
        br#"
local weak=setmetatable({},{__mode='v'});local observed,released
local co=coroutine.create(function()
 return io.open('absent-library-return-stacks-file','r')
end)
debug.sethook(co,function(e)
 if e=='return'and debug.getinfo(2,'f').func==io.open then
  local _,f=debug.getlocal(2,3);observed=io.type(f)=='closed file';weak[1]=f;f=nil
  assert(debug.setlocal(2,3,nil));collectgarbage();released=weak[1]==nil
 end
end,'r')
local ok,f,e,n=coroutine.resume(co)
collectgarbage()
return observed,released,ok,f,type(e),type(n),weak[1]==nil
"#,
    );
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
