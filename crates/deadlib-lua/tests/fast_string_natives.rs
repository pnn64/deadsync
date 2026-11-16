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
