use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn execute(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn scalar_array_overwrites_preserve_raw_set_metamethods_and_gc() {
    let mut vm = Vm::new();
    let Value::Table(table) = execute(&mut vm, b"return {11, 22, 33}")[0] else {
        panic!("expected table");
    };
    vm.set_global(b"target", Value::Table(table)).unwrap();

    for (key, value) in [
        (1.0, Value::Nil),
        (2.0, Value::Boolean(false)),
        (3.0, Value::Number(42.0)),
        (2.0, Value::LightUserdata(0x1234)),
    ] {
        vm.raw_table_set(table, Value::Number(key), value).unwrap();
        assert_eq!(vm.raw_table_get(table, Value::Number(key)).unwrap(), value);
    }

    assert_eq!(
        execute(
            &mut vm,
            br#"
local calls = 0
setmetatable(target, {__newindex = function(t, k, v)
    calls = calls + 1
    rawset(t, k, v)
end})
target[2] = 12 -- non-nil scalar, no __newindex
target[1] = 23 -- nil array slot, invokes __newindex
target[1] = false -- non-nil scalar again
target[3] = {sentinel = 91} -- object value still follows GC barriers
collectgarbage()
assert(target[3].sentinel == 91)
target[3] = 7
target[4] = 10 -- absent array slot, invokes __newindex
target[2.5] = 25 -- non-integer key, invokes __newindex
local nil_ok = pcall(rawset, target, nil, 1)
local nan_ok = pcall(rawset, target, 0/0, 1)
return calls, target[1], target[2], target[3], target[4], target[2.5], nil_ok, nan_ok
"#,
        ),
        [
            Value::Number(3.0),
            Value::Boolean(false),
            Value::Number(12.0),
            Value::Number(7.0),
            Value::Number(10.0),
            Value::Number(25.0),
            Value::Boolean(false),
            Value::Boolean(false),
        ]
    );
}

#[test]
fn constructor_list_fields_keep_nil_holes_in_array_storage() {
    let mut vm = Vm::new();
    execute(
        &mut vm,
        br#"
local function check()
    local controls = {false, '0', nil, 1}
    assert(#controls == 4 and controls[1] == false and controls[2] == '0')
    assert(controls[3] == nil and controls[4] == 1)
    local values = {[51] = 'kept', 1, nil, 3}
    assert(#values == 3 and values[2] == nil and values[51] == 'kept')
    local seen = {}
    for key, value in pairs(values) do seen[key] = value end
    assert(seen[1] == 1 and seen[3] == 3 and seen[51] == 'kept')
    local function tail() return nil, 5, nil end
    local expanded = {1, nil, 3, tail()}
    assert(expanded[4] == nil and expanded[5] == 5 and expanded[6] == nil)
end
check()
assert(loadstring(string.dump(check)))()
"#,
    );
}
fn get(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    stack.push(stack.value(2).unwrap())?;
    stack.get_table(1, false)?;
    stack.finish(1)
}
fn set(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    stack.push(stack.value(2).unwrap())?;
    stack.push(stack.value(3).unwrap())?;
    stack.set_table(1, false)?;
    stack.finish(0)
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    for (name, callback) in [
        (b"get".as_slice(), get as deadlib_lua::HostFunction),
        (b"set", set),
    ] {
        let function = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, function).unwrap();
    }
    vm
}

#[test]
fn table_creation_integer_keys_fields_and_raw_access_preserve_identity() {
    fn build(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let payload = stack.value(1).unwrap();
        stack.create_table(4, 3)?;
        stack.push(payload)?;
        stack.set_field(-2, b"payload\0bytes")?;
        stack.push(Value::Number(17.))?;
        stack.raw_set_integer(-2, -7)?;
        stack.raw_get_integer(-1, -7)?;
        assert_eq!(stack.value(-1), Some(Value::Number(17.)));
        stack.set_top(2)?;
        let key = stack.intern(b"key");
        stack.push(key)?;
        stack.push(payload)?;
        stack.set_table(2, true)?;
        stack.push(key)?;
        stack.get_table(2, true)?;
        assert_eq!(stack.value(-1), Some(payload));
        stack.set_top(2)?;
        stack.finish(1)
    }
    let mut vm = vm();
    let function = vm.new_host_function(build, &[]).unwrap();
    vm.set_global(b"build", function).unwrap();
    assert_eq!(
        execute(&mut vm, b"local p={};local t=build(p);collectgarbage();return t['payload\\0bytes']==p,t[-7],t.key==p"),
        [Value::Boolean(true), Value::Number(17.), Value::Boolean(true)]
    );
}

#[test]
fn indexed_metamethods_share_state_roots_and_child_coroutine_scheduling() {
    let mut vm = vm();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local values={};local object=setmetatable({},{
 __newindex=function(t,k,v)collectgarbage();values[k]=v end,
 __index=function(t,k)
  local child=coroutine.create(function()coroutine.yield(17);return values[k]end)
  local ok,n=coroutine.resume(child);assert(ok and n==17)
  local ok,v=coroutine.resume(child);assert(ok);collectgarbage();return v
 end})
local payload={};set(object,'key',payload)
local proxy=setmetatable({},{__index=object,__newindex=object})
local value=get(proxy,'key');set(proxy,'number',42)
return value==payload,get(proxy,'number'),rawget(object,'key')==nil
"#
        ),
        [
            Value::Boolean(true),
            Value::Number(42.),
            Value::Boolean(true)
        ]
    );
}

#[test]
fn table_faults_keep_native_inputs_and_mutable_open_lua_locals() {
    let mut vm = vm();
    assert_eq!(
        execute(&mut vm,br#"
local payload={};local escaped;local observed=false
local function fail(t,k,v)local retained={tag=17};escaped=function()return retained end;error(payload,0)end
local object=setmetatable({},{__newindex=fail})
local ok,e=xpcall(function()set(object,'key',payload)end,function(e)
 local native,lua=false,false
 for level=2,12 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==set then
   local _,k=debug.getlocal(level,4);local _,v=debug.getlocal(level,5)
   native=k=='key'and v==payload
  elseif info.func==fail then
   for index=1,8 do
    local name,value=debug.getlocal(level,index)
    if name=='retained' then debug.setlocal(level,index,{tag=42});lua=true end
   end
  end
 end
 collectgarbage();observed=native and lua;return e
end)
return ok,e==payload,observed,escaped().tag
"#),
        [Value::Boolean(false),Value::Boolean(true),Value::Boolean(true),Value::Number(42.)]
    );
}

#[test]
fn field_lookup_does_not_expose_its_local_key_as_a_native_stack_value() {
    fn field(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.get_field(1, b"field")?;
        stack.finish(1)
    }
    let mut vm = vm();
    let function = vm.new_host_function(field, &[]).unwrap();
    vm.set_global(b"field", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local inspected=false;local payload={}
local object=setmetatable({},{__index=function()error(payload,0)end})
local ok,e=xpcall(function()field(object)end,function(e)
 for level=2,12 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==field then inspected=debug.getlocal(level,2)==nil end
 end
 collectgarbage();return e
end)
return ok,e==payload,inspected
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
fn ignored_table_faults_remain_pending_and_reject_result_writes() {
    fn ignore(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let error = stack.get_field(1, b"field").unwrap_err();
        assert_eq!(stack.top(), 1);
        assert_eq!(stack.set_top(0).unwrap_err(), error);
        assert_eq!(stack.finish(0).unwrap_err(), error);
        assert_eq!(call.push(Value::Nil).unwrap_err(), error);
        Ok(())
    }
    let mut vm = vm();
    let function = vm.new_host_function(ignore, &[]).unwrap();
    vm.set_global(b"ignore", function).unwrap();
    assert_eq!(execute(&mut vm,b"local p={};local t=setmetatable({},{__index=function()error(p,0)end});local ok,e=pcall(ignore,t);return ok,e==p"),[Value::Boolean(false),Value::Boolean(true)]);
}

#[test]
fn metatable_access_bypasses_lua_protection_and_supports_shared_primitive_types() {
    fn change(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(stack.value(2).unwrap())?;
        stack.set_metatable(1)?;
        if !stack.get_metatable(1)? {
            stack.push(Value::Nil)?;
        }
        stack.finish(1)
    }
    let mut vm = vm();
    let function = vm.new_host_function(change, &[]).unwrap();
    vm.set_global(b"change", function).unwrap();
    assert_eq!(execute(&mut vm,b"local t={};local mt={__metatable='hidden'};local actual=change(t,mt);assert(getmetatable(t)=='hidden');change(t,nil);local primitive={};change(17,primitive);local same=debug.getmetatable(42)==primitive;change(17,nil);return actual==mt,getmetatable(t)==nil,same,debug.getmetatable(42)==nil"),[Value::Boolean(true);4]);
}

#[test]
fn traversal_and_stack_shifts_keep_values_rooted_and_release_removed_values() {
    fn count(call: &mut HostCall<'_>) -> Result<(), Error> {
        let collect = call.captures()[0];
        let mut stack = call.native_stack()?;
        let mut count = 0;
        stack.push(Value::Nil)?;
        while stack.next_table(1)? {
            count += 1;
            stack.set_top(stack.top() - 1)?;
        }
        stack.push(Value::Number(f64::from(count)))?;
        stack.insert(1)?;
        stack.remove(-1)?;
        stack.push(collect)?;
        stack.call(0, Some(0))?;
        stack.finish(1)
    }
    let mut vm = vm();
    let collect = vm.global(b"collectgarbage");
    let function = vm.new_host_function(count, &[collect]).unwrap();
    vm.set_global(b"count", function).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            b"return count({[1]=17,[7]=42,a={},b=false}),count({})"
        ),
        [Value::Number(4.), Value::Number(0.)]
    );
    assert_eq!(
        execute(
            &mut vm,
            br#"
local weak=setmetatable({},{__mode='v'})
local function argument()local t={value=42};weak[1]=t;return t end
local n=count(argument());return n,weak[1]==nil
"#
        ),
        [Value::Number(1.), Value::Boolean(true)]
    );
}

#[test]
fn table_api_resource_failures_clean_up_and_allow_a_new_entry() {
    let mut vm = vm();
    vm.set_instruction_limit(Some(200));
    let error = vm
        .execute(
            &compile(b"get(setmetatable({},{__index=function()while true do end end}),'key')")
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        execute(&mut vm, b"return get({key=42},'key')"),
        [Value::Number(42.)]
    );
}

#[test]
fn impossible_table_hints_and_failed_stack_operations_leave_existing_values_intact() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert!(stack.create_table(usize::MAX, 0).is_err());
        assert!(stack.create_table(0, usize::MAX).is_err());
        assert_eq!(stack.top(), 1);
        assert!(stack.remove(-10000).is_err());
        assert!(stack.insert(99).is_err());
        assert_eq!(stack.number(1)?, 42.);
        stack.finish(1)
    }
    let mut vm = vm();
    let function = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    assert_eq!(execute(&mut vm, b"return host(42)"), [Value::Number(42.)]);
}

#[test]
fn standalone_table_call_depth_accounts_for_its_outer_context_once() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let source = br#"
local depth=0
local object=setmetatable({},{__index=function(t,k)depth=depth+1;return get(t,k)end})
local ok,e=pcall(get,object,'key')
assert(not ok and e:find('C stack overflow',1,true));local ordinary=depth
depth=0
local ok,e=xpcall(function()get(object,'key')end,function(e)return get(object,'key')end)
assert(not ok and e=='error in error handling')
return ordinary,depth
"#;
            let mut embedded = vm();
            let embedded = execute(&mut embedded, source);
            let mut standalone = vm();
            standalone.begin_standalone().unwrap();
            let values = execute(&mut standalone, source);
            standalone.end_standalone().unwrap();
            assert_eq!(values, [Value::Number(196.), Value::Number(219.)]);
            assert_eq!(embedded, [Value::Number(197.), Value::Number(220.)]);
        })
        .unwrap()
        .join()
        .unwrap();
}
