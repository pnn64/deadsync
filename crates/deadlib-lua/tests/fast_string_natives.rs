use deadlib_lua::{Value, Vm, compile};

fn check_with_and_without_budget(source: &[u8]) {
    let program = compile(source).unwrap();
    for budgeted in [false, true] {
        let mut vm = Vm::new();
        if budgeted {
            vm.set_instruction_limit(Some(u64::MAX));
        }
        assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    }
}

#[test]
fn string_method_lookup_observes_replacement_and_inherited_handlers() {
    check_with_and_without_budget(
        br#"
local original=debug.getmetatable('')
local methods={sub=string.sub}
local mt={__index=methods}
debug.setmetatable('',mt)
local position=1
local function next_byte(source)
    local byte=source:sub(position,position)
    position=position+1
    return byte
end
assert(next_byte('abc')=='a' and position==2)
methods.sub=function(s,first,last) return 'changed' end
assert(next_byte('abc')=='changed' and position==3)
methods.sub=nil
local inherited=0
setmetatable(methods,{__index=function(t,key)
    assert(t==methods and key=='sub')
    inherited=inherited+1
    return string.sub
end})
assert(next_byte('abc')=='c' and position==4 and inherited==1)
mt.__index=function(s,key)
    assert(s=='abc' and key=='sub')
    return function() return 'handled' end
end
assert(next_byte('abc')=='handled' and position==5)
mt.__index=nil
assert(not pcall(next_byte,'abc') and position==5)
debug.setmetatable('',original)
return 42
"#,
    );
}

#[test]
fn single_string_argument_preserves_raw_bytes_and_open_results() {
    check_with_and_without_budget(
        br#"
local bytes=string.char(0,127,128,255)
assert(string.len('')==0 and string.len(bytes)==4)
assert(string.byte(bytes)==0)
assert(string.byte(string.sub(bytes,2))==127)
assert(string.byte(string.sub(bytes,3))==128)
assert(string.byte(string.sub(bytes,4))==255)
assert(select('#',string.byte(''))==0)
assert(select('#',string.byte(bytes))==1)
local a,b=string.byte('')
assert(a==nil and b==nil)
local function tail_byte(value) return string.byte(value) end
local function tail_len(value) return string.len(value) end
assert(select('#',tail_byte(''))==0)
assert(select('#',tail_byte(bytes))==1 and tail_byte(bytes)==0)
assert(tail_len(bytes)==4)
local function count_results(...) return select('#',...),... end
local n,first=count_results(string.byte(''))
assert(n==0 and first==nil)
n,first=count_results(string.byte(bytes))
assert(n==1 and first==0)
return 42
"#,
    );
}

#[test]
fn other_argument_counts_coercions_and_errors_use_native_dispatch() {
    check_with_and_without_budget(
        br#"
assert(string.len(12345)==5)
assert(string.byte(12345)==string.byte('12345'))
assert(string.len('abc','ignored')==3)
assert(string.byte('abc',2)==98)
assert(string.byte('abc',nil)==97)
local a,b=string.byte('abc',2,3)
assert(a==98 and b==99)
local callable=setmetatable({},{__call=string.len})
assert(not pcall(callable,'abc'))
return 42
"#,
    );

    for source in [
        b"\nreturn string.len()".as_slice(),
        b"\nreturn string.byte()",
        b"\nreturn string.len(false)",
        b"\nreturn string.byte({})",
        b"\nreturn string.byte('abc',false)",
    ] {
        let program = compile(source).unwrap();
        let mut errors = Vec::new();
        for budgeted in [false, true] {
            let mut vm = Vm::new();
            if budgeted {
                vm.set_instruction_limit(Some(u64::MAX));
            }
            let error = vm.execute(&program).unwrap_err();
            errors.push((error.kind(), error.line(), error.message_bytes().to_vec()));
        }
        assert_eq!(errors[0], errors[1], "{source:?}");
    }
}

#[test]
fn string_char_preserves_variadic_bytes_coercions_and_error_order() {
    check_with_and_without_budget(
        br#"
local bytes={}
for i=1,2048 do bytes[i]=(i*37)%256 end
local text=string.char(unpack(bytes))
assert(#text==2048 and string.char()=='')
for i=1,#text do assert(text:byte(i)==bytes[i]) end
assert(string.char(-0.9,'255',' 1.9 ')=='\0\255\1')
local ok,err=pcall(string.char,1,256,false)
assert(not ok and err:find('#2',1,true) and err:find('invalid value',1,true))
ok,err=pcall(string.char,1,false,256)
assert(not ok and err:find('#2',1,true) and err:find('number expected',1,true))
return 42
"#,
    );
}

#[test]
fn hooks_observe_string_natives_with_empty_byte_returns() {
    let source = br#"
local len,byte=string.len,string.byte
local calls={0,0};local returns={0,0}
debug.sethook(function(event)
 local active=debug.getinfo(2,'f').func
 local i=(active==len and 1) or (active==byte and 2)
 if i then
  if event=='call' then calls[i]=calls[i]+1 end
  if event=='return' then returns[i]=returns[i]+1 end
 end
end,'cr')
assert(len('a\0b')==3)
assert(byte('\255')==255)
assert(select('#',byte(''))==0)
debug.sethook()
assert(calls[1]==1 and returns[1]==1)
assert(calls[2]==2 and returns[2]==2)
return 42
"#;
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        [Value::Number(42.0)]
    );
}

#[test]
fn interned_substrings_keep_adjusted_bounds_and_call_hooks() {
    check_with_and_without_budget(
        br#"
collectgarbage('stop')
local sub=string.sub
local source='ab\0cd\255ef'
local suffix='d\255ef'
local function slice(first,last)
    local result=sub(source,first,last)
    return result
end
local function compile_reader()
    local finished=false
    return assert(load(function()
        if finished then return nil end
        finished=true
        assert(slice(-4,100)==suffix)
        assert(slice(2.9,4.9)=='b\0c')
        assert(slice(4,3)=='')
        return 'return 42'
    end))
end
assert(compile_reader()()==42)
local calls,returns=0,0
debug.sethook(function(event)
    if debug.getinfo(2,'f').func == sub then
        if event=='call' then calls=calls+1 else returns=returns+1 end
    end
end,'cr')
assert(compile_reader()()==42)
debug.sethook()
assert(calls==3 and returns==3)
return 42
"#,
    );
}
