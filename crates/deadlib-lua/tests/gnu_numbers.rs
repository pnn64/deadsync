#![cfg(all(target_os = "linux", target_env = "gnu"))]
use deadlib_lua::{Value, Vm, compile};

#[test]
fn numeric_reads_preserve_glibc_partial_tokens_and_the_next_read() {
    let source = br#"
      local function scan(text)
        local f=assert(io.tmpfile());assert(f:write(text));assert(f:seek('set',0))
        local value=f:read('*n');local position=assert(f:seek())
        local tail=f:read('*a');assert(f:close());return value,position,tail
      end
      for _,case in ipairs{
        {'1e+!',1,3,'!'},{'0x1p-!',1,5,'!'},{'0x.p1',0,3,'p1'},
        {'0x!',false,2,'!'},{'infi!',false,5,''},{'nan(!',false,5,''},
        {'nan(x y)',false,6,'y)'},{'nP)rest',false,2,')rest'}
      }do
        local value,position,tail=scan(case[1])
        assert(value==(case[2]or nil) and position==case[3] and tail==case[4])
      end
      local value,position,tail=scan('nan(snan)!')
      assert(value~=value and position==9 and tail=='!')
      local value,position,tail=scan('-0x.')
      assert(value==0 and 1/value==-math.huge and position==4 and tail=='')
      return true
    "#;
    assert_eq!(
        Vm::new().execute(&compile(source).unwrap()).unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn binary_math_checks_use_gcc_order_and_preserve_method_argument_names() {
    let source = br#"
      for _,name in ipairs{'pow','fmod','mod','atan2'}do
        local f=assert(loadstring('return math.'..name..'(false,false)'))
        local ok,message=pcall(f)
        assert(not ok and message:find("bad argument #2 to '"..name.."' (number expected, got boolean)",1,true))
        local t={f=math[name]}
        local ok,message=pcall(function()return t:f(false)end)
        assert(not ok and message:find("bad argument #1 to 'f' (number expected, got boolean)",1,true))
        t.f=setmetatable({},{__call=math[name]})
        local ok,message=pcall(function()return t:f(1)end)
        assert(not ok and message:find("bad argument #1 to 'f' (number expected, got table)",1,true))
      end
      return true
    "#;
    assert_eq!(
        Vm::new().execute(&compile(source).unwrap()).unwrap(),
        &[Value::Boolean(true)]
    );
}
