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

#[test]
fn literal_substitution_batches_match_stepped_substitution() {
    // Literal replacements batch matches until the first buffer flush. A
    // configured budget keeps one native step per match, so both modes must
    // produce identical strings, counts and diagnostics.
    let source = br#"
local out={}
local function add(...)
 local n=select('#',...)
 for i=1,n do out[#out+1]=tostring((select(i,...))) end
 out[#out+1]='|'
end
local long=string.rep('ab-cd ',400)
local subjects={'',' not ID( ( 1 < 2 ) and ID(x) )','hello world',long,long..'tail'}
local cases={
 {'ID',''},{'ID','identity'},{'o','0'},{'%s+','_'},{'(%w+)','<%1>'},
 {'%w+','%0%0'},{'','-'},{'x*','.'},{'^ab','AB'},{'^','>'},{'b',7},
 {'()','%1'},{'(a)(b)','%2%1%%'},{'z','none'},{'[%-]',string.rep('#',600)},
}
for _,subject in ipairs(subjects) do
 for _,case in ipairs(cases) do
  for _,limit in ipairs{false,0,1,3,1000} do
   if limit then add(string.gsub(subject,case[1],case[2],limit))
   else add(string.gsub(subject,case[1],case[2])) end
  end
 end
end
add(pcall(string.gsub,'abc','(b)','%2'))
add(pcall(string.gsub,long,'d','%9'))
add(pcall(string.gsub,'abc','(','x'))
return table.concat(out,',')
"#;
    let program = compile(source).unwrap();
    let mut results = Vec::new();
    for ordinary in [false, true] {
        let mut vm = Vm::new();
        if ordinary {
            vm.set_instruction_limit(Some(u64::MAX));
        }
        let values = vm.execute(&program).unwrap();
        let [Value::String(id)] = values[..] else {
            panic!("substitution digest is a string")
        };
        results.push(vm.string_bytes(id).unwrap().to_vec());
    }
    assert!(results[0].len() > 100_000);
    assert_eq!(results[0], results[1]);
}

#[test]
fn unary_math_fast_results_match_observed_dispatch_bit_for_bit() {
    let program = compile(
        br#"
local functions={math.abs,math.floor,math.ceil,math.sqrt,math.sin,math.cos,
  math.tan,math.exp,math.log,math.log10,math.asin,math.acos,math.atan,
  math.sinh,math.cosh,math.tanh,math.deg,math.rad}
local numbers={-0.0,0.0,-2.5,0.25,2.5,0/0,1/0,-1/0}
local results={}
for _,f in ipairs(functions) do
  for _,x in ipairs(numbers) do
    results[#results+1]=f(x,'ignored extra argument')
  end
end
return unpack(results)
"#,
    )
    .unwrap();
    let mut outputs = Vec::new();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(u64::MAX));
        }
        outputs.push(
            vm.execute(&program)
                .unwrap()
                .iter()
                .map(|value| match value {
                    Value::Number(number) => number.to_bits(),
                    other => panic!("unary math returned non-number: {other:?}"),
                })
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(outputs[0].len(), 144);
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0][0], 0.0f64.to_bits()); // abs(-0) is +0.
    assert_eq!(outputs[0][4 * 8], (-0.0f64).to_bits()); // sin(-0) is -0.
    assert_eq!(outputs[0][17 * 8], (-0.0f64).to_bits()); // rad(-0) is -0.
}

#[test]
fn unary_math_non_numeric_arguments_and_hooks_keep_native_behavior() {
    check_both(
        br#"
for _,value in ipairs{false,{},'not a number'} do
  assert(not pcall(math.sqrt,value))
end
assert(math.floor('42.9')==42)
assert(math.abs(-42,'ignored')==42)
assert(not pcall(math.sin))
local callable=setmetatable({},{__call=math.abs})
assert(not pcall(callable,-42))
local fn=math.abs
local calls,returns=0,0
debug.sethook(function(event)
  if debug.getinfo(2,'f').func==fn then
    if event=='call' then calls=calls+1 end
    if event=='return' then returns=returns+1 end
  end
end,'cr')
local result=fn(-42)
debug.sethook()
assert(calls==1 and returns==1)
local function tail(x) return math.abs(x) end
assert(result==42)
return tail(-42)
"#,
    );
}

#[test]
fn raw_scalar_builtins_preserve_identity_coercion_and_errors() {
    check_both(
        br#"
local t={[1]=42,[false]=-0.0}
local other=setmetatable({},{__index=function() error('metamethod ran') end})
local nan=0/0
assert(rawequal(0,-0.0) and not rawequal(nan,nan))
assert(rawequal(t,t) and not rawequal(t,other))
assert(rawget(t,1)==42 and rawget(t,2)==nil)
assert(1/rawget(t,false)==-math.huge)
assert(rawget(other,'missing')==nil and rawget(t,nan)==nil)
assert(1/tonumber(-0.0)==-math.huge)
assert(tonumber(nan)~=tonumber(nan))
assert(tonumber('42')==42 and tonumber('ff',16)==255)
assert(not pcall(rawget,42,1) and not pcall(rawget,t))
assert(not pcall(rawequal,1) and tonumber(false)==nil)
return 42
"#,
    );
}

#[test]
fn raw_scalar_builtins_still_emit_native_call_and_return_hooks() {
    let source = br#"
local functions={rawequal,rawget,tonumber}
local calls={0,0,0};local returns={0,0,0}
debug.sethook(function(event)
  local active=debug.getinfo(2,'f').func
  for i=1,3 do
    if active==functions[i] then
      if event=='call' then calls[i]=calls[i]+1 end
      if event=='return' then returns[i]=returns[i]+1 end
    end
  end
end,'cr')
assert(functions[1](4,4))
assert(functions[2]({42},1)==42)
assert(functions[3](42)==42)
debug.sethook()
for i=1,3 do assert(calls[i]==1 and returns[i]==1) end
return 42
"#;
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        [Value::Number(42.)]
    );
}
