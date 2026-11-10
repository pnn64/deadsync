use deadlib_lua::{Value, Vm, compile};

// A configured budget retains ordinary native dispatch without changing hook
// scheduling. These short programs cannot exhaust the reference's budget.
fn check_both(source: &[u8]) {
    let program = compile(source).unwrap();
    for ordinary in [false, true] {
        let mut vm = Vm::new();
        if ordinary {
            vm.set_instruction_limit(Some(u64::MAX));
        }
        assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.)]);
    }
}

#[test]
fn successful_assert_preserves_aliases_open_results_and_tail_results() {
    check_both(
        br#"
local function pack(...) return {n=select('#',...),...} end
local function tail(...) return assert(...) end
local t={};local f=function()end;local nan=0/0
for _,value in ipairs{true,0,'',t,f,nan} do
 local a,b,c=assert(value,nil,55)
 assert((a==value or a~=a and value~=value) and b==nil and c==55)
 local result=pack(assert(value,nil,t,f,nan))
 assert(result.n==5 and result[2]==nil and result[3]==t and result[4]==f)
 assert(result[5]~=result[5])
end
local result=pack(tail(t,nil,f))
assert(result.n==3 and result[1]==t and result[2]==nil and result[3]==f)
local callable=setmetatable({},{__call=assert})
local a,b=callable('extra')
assert(a==callable and b=='extra')
local co=coroutine.create(function() return tail(true,nil,5) end)
local ok,a,b,c=coroutine.resume(co)
assert(ok and a==true and b==nil and c==5)
return 42
"#,
    );
}

#[test]
fn next_preserves_keys_deleted_controls_and_captured_iterator_identity() {
    check_both(
        br#"
local object={};local fn=function()end
local t={[false]=91,[object]=object,[fn]=fn,['a\0b']=92}
for i=1,129 do t[i]=i end
local seen={};local key=nil;local n=0
while true do
 local value;key,value=next(t,key)
 if key==nil then break end
 assert(not seen[key]);seen[key]=true;n=n+1
 if key==object or key==fn then assert(value==key) end
 t[key]=nil
 collectgarbage()
end
assert(n==133 and next(t)==nil)
local hash={first=1,second=2}
local dead=next(hash);hash[dead]=nil
for i=1,500 do hash['new'..i]=i end
local ok,err=pcall(next,hash,dead)
assert(not ok and string.find(err,"invalid key to 'next'",1,true))
local saved=next
next=function()error('replaced next must not be called by pairs')end
local sum=0
for key,value in pairs{4,5,6} do sum=sum+value end
next=saved
assert(sum==15)
local callable=setmetatable({7,8},{__call=next})
local a,b=callable(nil)
assert(a==1 and b==7)
a,b=next(callable,1,'ignored',false)
assert(a==2 and b==8)
a,b=next(callable,2)
assert(a==nil and b==nil)
return 42
"#,
    );
}

#[test]
fn invalid_native_calls_keep_diagnostics_error_objects_and_vm_reuse() {
    for source in [
        b"\nassert()".as_slice(),
        b"\nassert(false)",
        b"\nassert(nil,'custom error')",
        b"\nassert(false,{})",
        b"\nnext(42,nil)",
        b"\nnext({},'missing')",
        b"\nnext({},0/0)",
        b"\nnext({},{},'ignored')",
    ] {
        let program = compile(source).unwrap();
        let mut results = Vec::new();
        for ordinary in [false, true] {
            let mut vm = Vm::new();
            if ordinary {
                vm.set_instruction_limit(Some(u64::MAX));
            }
            let error = vm.execute(&program).unwrap_err();
            let object = error.value().map(|value| {
                let Value::String(id) = value else {
                    panic!("native diagnostic is a string")
                };
                vm.string_bytes(id).unwrap().to_vec()
            });
            results.push((
                error.kind(),
                error.line(),
                error.message_bytes().to_vec(),
                object,
            ));
            assert_eq!(
                vm.execute(&compile(b"return 42").unwrap()).unwrap(),
                [Value::Number(42.)]
            );
        }
        assert_eq!(results[0], results[1], "{source:?}");
    }
}

#[test]
fn call_hooks_still_receive_native_assert_and_next_activations() {
    let source = br#"
local check,advance=assert,next
local ac,ar,nc,nr=0,0,0,0
debug.sethook(function(event)
 local info=debug.getinfo(2,'f')
 if info.func==check then
  if event=='call' then ac=ac+1 elseif event=='return' then ar=ar+1 end
 elseif info.func==advance then
  if event=='call' then nc=nc+1 elseif event=='return' then nr=nr+1 end
 end
end,'cr')
local a,b=check(42,43)
local k,v=advance({44},nil)
debug.sethook()
check(a==42 and b==43 and k==1 and v==44)
check(ac==1 and ar==1 and nc==1 and nr==1)
return 42
"#;
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        [Value::Number(42.)]
    );
}
