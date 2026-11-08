use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let original = stack.top();
    for index in 1..=original {
        stack.push(stack.value(index as isize).unwrap())?;
    }
    stack.call_unprotected(original - 1, None)?;
    let count = stack.top() - original;
    stack.finish(count)
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    let function = vm.new_host_function(invoke, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    vm
}
fn execute(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn outer_handlers_inspect_mutate_and_collect_fault_locals_before_unwinding() {
    let mut vm = vm();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local payload={};local escaped;local found,native=false,false
local function fault(value)
 local retained={tag=321};escaped=function()return retained end;error(value,0)
end
local ok,e=xpcall(function()host(fault,payload)end,function(e)
 for level=2,12 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==fault then
   for index=1,10 do
    local name,value=debug.getlocal(level,index)
    if name=='retained' then
     assert(value.tag==321)
     assert(debug.setlocal(level,index,{tag=654})=='retained');found=true
    end
   end
  elseif info.func==host then
   local _,value=debug.getlocal(level,2);native=value==payload
  end
 end
 collectgarbage();assert(escaped().tag==654);return e
end)
collectgarbage();return ok,e==payload,found,native,escaped().tag
"#
        ),
        [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Number(654.)
        ]
    );
}

#[test]
fn failures_cross_multiple_native_callbacks_without_losing_fault_frames() {
    let mut vm = vm();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local payload={};local leaf,middle;local count,found=0,0
leaf=function(value)local retained=17;error(value,0)end
middle=function(value)local retained=42;host(leaf,value);error('continued')end
local ok,e=xpcall(function()host(middle,payload)end,function(e)
 for level=2,16 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==host then count=count+1 end
  if info.func==leaf or info.func==middle then
   for index=1,6 do
    local name,value=debug.getlocal(level,index)
    if name=='retained' then found=found+value end
   end
  end
 end
 collectgarbage();return e
end)
return ok,e==payload,count,found,host(function()return 42 end)
"#
        ),
        [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Number(2.),
            Value::Number(59.),
            Value::Number(42.)
        ]
    );
}

#[test]
fn ignored_unprotected_errors_remain_pending_and_reject_stack_mutation() {
    fn ignore(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(stack.value(1).unwrap())?;
        let error = stack.call_unprotected(0, None).unwrap_err();
        assert_eq!(stack.top(), 1);
        assert!(!stack.check_stack(1));
        assert_eq!(stack.set_top(0).unwrap_err(), error);
        assert_eq!(stack.push(Value::Nil).unwrap_err(), error);
        assert_eq!(stack.set(1, Value::Nil).unwrap_err(), error);
        assert_eq!(stack.number(1).unwrap_err(), error);
        assert_eq!(stack.call(0, None).unwrap_err(), error);
        assert_eq!(stack.finish(0).unwrap_err(), error);
        assert_eq!(call.push(Value::Nil).unwrap_err(), error);
        Ok(())
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(ignore, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local payload={};local found=false
local function fault()local retained=42;error(payload,0)end
local ok,e=xpcall(function()host(fault)end,function(e)
 for level=2,10 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==fault then
   local name,value=debug.getlocal(level,1);found=name=='retained'and value==42
  end
 end
 collectgarbage();return e
end)
return ok,e==payload,found
"#
        ),
        [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(true)
        ]
    );
}

#[test]
fn reader_errors_inherit_outer_handlers_across_unprotected_calls_only() {
    fn protected(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.call_and_return(call.arguments()[0], &[])
    }
    let mut vm = vm();
    let function = vm.new_host_function(protected, &[]).unwrap();
    vm.set_global(b"protected", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local handled=0
local function reader()error('reader',0)end
local function handler(e)
 handled=handled+1;collectgarbage();assert(host(function()return 42 end)==42)
 return 'outer:'..e
end
local ok,a,b=xpcall(function()return host(function()return load(reader)end)end,handler)
assert(ok and a==nil and b=='outer:reader'and handled==1)
handled=0
local ok,a,b,c=xpcall(function()
 return host(function()return pcall(load,reader)end)
end,handler)
assert(ok and a and b==nil and c=='reader'and handled==0)
local ok,a,b=xpcall(function()
 return host(function()return protected(function()return load(reader)end)end)
end,handler)
return ok,a==nil,b=='reader',handled
"#
        ),
        [
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Number(0.)
        ]
    );
}

#[test]
fn successful_calls_preserve_prefixes_fixed_results_and_callable_objects() {
    fn fixed(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(Value::Number(17.))?;
        stack.push(stack.value(1).unwrap())?;
        assert_eq!(stack.call_unprotected(0, Some(4))?, 4);
        stack.finish(5)
    }
    let mut vm = vm();
    let function = vm.new_host_function(fixed, &[]).unwrap();
    vm.set_global(b"fixed", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local a,b,c,d,e=fixed(function()collectgarbage();return 42,nil,false end)
assert(a==17 and b==42 and c==nil and d==false and e==nil)
assert(select('#',fixed(function()return 1,2,3,4,5 end))==5)
local callable=setmetatable({},{__call=function(self,x)return x+1 end})
return host(callable,41),host(math.abs,-17),select('#',host(function()end))
"#
        ),
        [Value::Number(42.), Value::Number(17.), Value::Number(0.)]
    );
}

#[test]
fn instruction_abort_cleans_active_child_threads_and_allows_a_new_entry() {
    let mut vm = vm();
    vm.set_instruction_limit(Some(200));
    let error = vm
        .execute(
            &compile(
                b"host(function()child=coroutine.create(function()while true do end end);return coroutine.resume(child)end)",
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        execute(
            &mut vm,
            b"return coroutine.status(child)=='dead',host(function()return 42 end)",
        ),
        [Value::Boolean(true), Value::Number(42.)]
    );
}

#[test]
fn dead_coroutines_keep_unprotected_fault_locals_and_open_cells_inspectable() {
    let mut vm = vm();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local payload={};local escaped
local function fault()local retained={tag=17};escaped=function()return retained end;error(payload,0)end
local co=coroutine.create(function()return host(fault)end)
local ok,e=coroutine.resume(co);collectgarbage();local found=false
for level=0,12 do
 local info=debug.getinfo(co,level,'f');if not info then break end
 if info.func==fault then
  local name,value=debug.getlocal(co,level,1)
  assert(name=='retained'and value.tag==17)
  debug.setlocal(co,level,1,{tag=42});found=true
 end
end
collectgarbage();return ok,e==payload,found,escaped().tag,coroutine.status(co)=='dead'
"#
        ),
        [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Number(42.),
            Value::Boolean(true)
        ]
    );
}
