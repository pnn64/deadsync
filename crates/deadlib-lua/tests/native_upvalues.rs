use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn get(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let n = stack.number(2)? as usize;
    let before = stack.top();
    let name = stack.get_upvalue(1, n)?.map(<[u8]>::to_vec);
    let delta = stack.top() - before;
    let text = name
        .as_ref()
        .map(|bytes| stack.intern(bytes.split(|b| *b == 0).next().unwrap()));
    stack.push(text.unwrap_or(Value::Nil))?;
    if name.is_some() {
        stack.insert(-2)?;
    } else {
        stack.push(Value::Nil)?;
    }
    stack.push(Value::Number(delta as f64))?;
    stack.finish(3)
}
fn set(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let n = stack.number(2)? as usize;
    stack.push(stack.value(3).unwrap_or(Value::Nil))?;
    let before = stack.top();
    let name = stack.set_upvalue(1, n)?.map(<[u8]>::to_vec);
    let delta = stack.top() as isize - before as isize;
    let text = name
        .as_ref()
        .map(|bytes| stack.intern(bytes.split(|b| *b == 0).next().unwrap()));
    stack.push(text.unwrap_or(Value::Nil))?;
    stack.push(Value::Number(delta as f64))?;
    stack.finish(2)
}
fn machine() -> Vm {
    let mut vm = Vm::new();
    let get = vm.new_host_function(get, &[]).unwrap();
    let set = vm.new_host_function(set, &[]).unwrap();
    vm.set_global(b"hostget", get).unwrap();
    vm.set_global(b"hostset", set).unwrap();
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn named_lua_cells_share_mutation_across_open_closed_and_suspended_frames() {
    let mut vm = machine();
    run(
        &mut vm,
        br#"
local a={value=17}
local get=function()return a end
local name,value,delta=hostget(get,1)
assert(name=='a' and value==a and delta==1)
local replacement={value=42}
local name,delta=hostset(get,1,replacement)
assert(name=='a' and delta==-1 and a==replacement)
assert(hostset(get,2,17)==nil and get()==replacement)
assert(hostget(false,1)==nil and hostset(false,1,nil)==nil)
local co=coroutine.create(function()
 local captured={value=71}
 suspended=function()return captured end
 coroutine.yield()
 assert(captured.value==91)
 return captured
end)
assert(coroutine.resume(co))
assert(hostset(suspended,1,{value=91})=='captured')
collectgarbage()
local ok,closed=coroutine.resume(co)
assert(ok and suspended()==closed)
assert(hostset(suspended,1,replacement)=='captured' and suspended()==replacement)
collectgarbage();assert(suspended()==replacement)
"#,
    );
}

#[test]
fn native_slots_drive_builtin_execution_and_keep_replaced_values_rooted() {
    let mut vm = machine();
    run(
        &mut vm,
        br#"
local name,step=hostget(pairs,1);assert(name=='' and step==select(1,pairs({})))
local replacement=function(t,k)return next(t,k)end
assert(hostset(pairs,1,replacement)=='' and select(1,pairs({}))==replacement)
local total=0;for k,v in pairs({3,7})do total=total+v end;assert(total==10)
assert(hostset(pairs,1,step)=='')
local it=string.gmatch('a12b34','%d+')
assert(it()=='12' and select(2,hostget(it,3))==3)
assert(hostset(it,3,4)=='' and it()=='34')
local wrapper=coroutine.wrap(function()return 17 end)
local co=coroutine.create(function()return 91 end)
assert(hostset(wrapper,1,co)=='');co=nil;collectgarbage();assert(wrapper()==91)
local _,valid=hostget(newproxy,1)
assert(valid==getmetatable(valid) and valid.__mode=='kv')
local object=newproxy(true);local mt=getmetatable(object)
valid[mt]=nil;assert(not pcall(newproxy,object))
valid[mt]=true;assert(getmetatable(newproxy(object))==mt)
local weak=setmetatable({},{__mode='v'})
do local replacement={};weak[1]=replacement;assert(hostset(newproxy,1,replacement)=='')end
collectgarbage();assert(weak[1]~=nil)
assert(hostset(newproxy,1,valid)=='');collectgarbage();collectgarbage();assert(weak[1]==nil)
"#,
    );
}

#[test]
fn unreachable_builtin_capture_cycles_are_reclaimed_and_handles_become_invalid() {
    let mut vm = machine();
    let builtin = vm.global(b"pairs");
    run(
        &mut vm,
        br#"
local f=pairs
local cycle={owner=f}
weak=setmetatable({f,cycle},{__mode='v'})
assert(hostset(f,1,cycle)=='')
pairs=nil
"#,
    );
    vm.collect_garbage().unwrap();
    run(
        &mut vm,
        b"collectgarbage();assert(weak[1]==nil and weak[2]==nil)",
    );
    assert!(vm.call(builtin, &[]).is_err());
}

#[test]
fn upvalue_names_stay_alive_after_compiler_programs_and_host_roots_are_released() {
    let mut vm = machine();
    run(
        &mut vm,
        br#"
local retained_name_123456789=17
retained=function()return retained_name_123456789 end
"#,
    );
    // The temporary Program has been dropped; only the closure owns its binding.
    vm.collect_garbage().unwrap();
    run(
        &mut vm,
        br#"
assert(hostget(retained,1)=='retained_name_123456789')
assert(hostset(retained,1,{marker=42})=='retained_name_123456789')
collectgarbage();assert(retained().marker==42)
"#,
    );
}

#[test]
fn stack_bounds_invalid_captures_and_empty_writes_preserve_values() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert!(stack.get_upvalue(1, 0)?.is_none());
        assert!(stack.get_upvalue(1, usize::MAX)?.is_none());
        assert_eq!(stack.top(), 1);
        stack.set_top(8000)?;
        assert!(stack.get_upvalue(1, 2)?.is_none());
        assert_eq!(stack.top(), 8000);
        assert!(stack.get_upvalue(1, 1).is_err());
        assert_eq!(stack.top(), 8000);
        stack.set(-1, Value::Number(71.))?;
        assert_eq!(stack.set_upvalue(1, 1)?, Some(b"value\0".as_slice()));
        assert_eq!(stack.top(), 7999);
        stack.set_top(0)?;
        assert!(stack.set_upvalue(1, 1).is_err());
        stack.finish(0)
    }
    let mut vm = machine();
    let function = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"bounds", function).unwrap();
    run(
        &mut vm,
        b"local value=17;bounds(function()return value end);assert(value==71)",
    );
}

#[test]
fn pending_unprotected_errors_reject_upvalue_reads_and_writes_and_recover() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(stack.value(2).unwrap())?;
        let original = stack.call_unprotected(0, Some(0)).unwrap_err();
        assert_eq!(original.kind(), ErrorKind::Runtime);
        assert_eq!(
            stack.get_upvalue(1, 1).unwrap_err().value(),
            original.value()
        );
        assert_eq!(
            stack.set_upvalue(1, 1).unwrap_err().value(),
            original.value()
        );
        Ok(())
    }
    let mut vm = machine();
    let function = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"fail", function).unwrap();
    run(&mut vm,b"local value=17;local f=function()return value end;local marker={};local ok,e=pcall(fail,f,function()error(marker)end);assert(not ok and e==marker and value==17);assert(hostset(f,1,91)=='value' and f()==91)");
}
