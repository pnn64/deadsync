use deadlib_lua::{Error, HostCall, Value, Vm, compile};

fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let body = stack.builtin_body(1).expect("builtin body");
    stack.remove(1)?;
    let count = stack.call_builtin_body(body)?;
    if count < 0 {
        let top = stack.top();
        return stack.yield_results(top);
    }
    stack.push_integer(count)?;
    let top = stack.top();
    stack.finish(top)
}
fn setup() -> Vm {
    let mut vm = Vm::new();
    let body = vm.new_host_function(invoke, &[]).unwrap();
    vm.set_global(b"body", body).unwrap();
    let copy = vm.new_host_function(copy, &[]).unwrap();
    vm.set_global(b"copy", copy).unwrap();
    vm
}
fn copied(call: &mut HostCall<'_>) -> Result<(), Error> {
    let body = *call
        .function_data::<deadlib_lua::NativeBuiltinBody>()
        .unwrap();
    let mut stack = call.native_stack()?;
    let count = stack.call_builtin_body(body)?;
    if count < 0 {
        let top = stack.top();
        return stack.yield_results(top);
    }
    stack.finish(count as usize)
}
fn copy(call: &mut HostCall<'_>) -> Result<(), Error> {
    let plain = call.arguments().get(1) == Some(&Value::Boolean(true));
    let mut stack = call.native_stack()?;
    let body = stack.builtin_body(1).unwrap();
    let mut count = 0;
    while !plain && stack.get_upvalue(1, count + 1)?.is_some() {
        count += 1;
    }
    stack.push_closure(copied, count, body)?;
    if !plain {
        stack.get_environment(1)?;
        stack.set_environment(-2)?;
    }
    stack.finish(1)
}

#[test]
fn coroutine_bodies_reconstruct_the_complete_c_stack_before_selecting_results() {
    let mut vm = setup();
    vm.execute(
        &compile(
            br#"
local fn=function()return 17 end
local a,b,c,co,n=body(coroutine.create,fn,'extra',nil)
assert(a==fn and b=='extra'and c==nil and type(co)=='thread'and n==1)
local a,b,c,status,n=body(coroutine.status,co,'extra',nil)
assert(a==co and b=='extra'and c==nil and status=='suspended'and n==1)
local a,b,current,result,n=body(coroutine.running,'extra',nil)
assert(a=='extra'and b==nil and type(current)=='thread'and result==nil and n==1)
local a,b,c,wrap,n=body(coroutine.wrap,fn,'extra',nil)
assert(a==fn and b=='extra'and c==nil and type(wrap)=='function'and n==1 and wrap()==17)
local child=coroutine.create(function()
  local a,b,current,n=body(coroutine.running,'extra',nil)
  assert(a=='extra'and b==nil and current==coroutine.running()and n==1)
end)
assert(coroutine.resume(child))
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn a_body_code_does_not_retain_the_original_closures_environment() {
    let mut vm = setup();
    vm.execute(
        &compile(
            br#"
local freed=false;local p=newproxy(true);getmetatable(p).__gc=function()freed=true end
debug.setfenv(tostring,{p=p});p=nil
tostring=copy(tostring,true)
local value=setmetatable({},{__tostring=function()collectgarbage();return 'ok'end})
assert(tostring(value)=='ok'and freed)
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn hook_bodies_preserve_the_interrupted_lua_function_and_its_environment() {
    fn hook(call: &mut HostCall<'_>) -> Result<(), Error> {
        let event = call.hook_event().unwrap();
        let target = call.captures()[0];
        let mode = call.captures()[1];
        let env = call.captures()[2];
        let mut s = call.native_stack()?;
        let top = s.top();
        s.get_info(event.frame, b"f")?;
        let matches = s.value(-1) == Some(target);
        s.set_top(top)?;
        if !matches {
            return Ok(());
        }
        s.set_hook(s.thread_id(), deadlib_lua::NativeHook::None, 0, 0)?;
        let saved: Vec<_> = (1..=top).map(|i| s.value(i as isize).unwrap()).collect();
        let code = if mode == Value::Number(0.) {
            32
        } else {
            s.set_top(1)?;
            s.set(1, Value::Number(0.))?;
            22
        };
        assert_eq!(
            s.call_builtin_body(
                deadlib_lua::NativeBuiltinBody::from_implementation_code(code).unwrap()
            )?,
            1
        );
        assert_eq!(
            s.value(-1),
            Some(if code == 32 { Value::Number(17.) } else { env })
        );
        s.set_top(top)?;
        for (i, value) in saved.into_iter().enumerate() {
            s.set(i as isize + 1, value)?;
        }
        s.set(1, Value::Number(29.))?;
        Ok(())
    }
    fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        s.get_environment(1)?;
        s.push_closure(hook, 3, ())?;
        s.set_hook(
            s.thread_id(),
            deadlib_lua::NativeHook::Host(s.value(-1).unwrap()),
            1,
            0,
        )?;
        s.finish(0)
    }
    let mut vm = setup();
    let install = vm.new_host_function(install, &[]).unwrap();
    vm.set_global(b"install_body", install).unwrap();
    vm.execute(
        &compile(
            br#"
local fn=setfenv(function(x)return x end,{})
for mode=0,1 do install_body(fn,mode);assert(fn(-17)==29)end
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn file_close_alias_checks_its_implicit_nil_default_argument() {
    let mut vm = setup();
    vm.execute(
        &compile(
            br#"
local ok,a,b=pcall(getmetatable(io.stdout).__index.close)
assert(not ok and a:match('FILE%* expected, got nil')and b==nil)
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn copied_bodies_use_their_own_upvalues_environments_and_coroutine_continuations() {
    let mut vm = setup();
    vm.execute(&compile(br#"
local gm=copy(string.gmatch('a1b2','%d'));assert(gm()=='1'and gm()=='2'and gm()==nil)
local wrap=copy(coroutine.wrap(function(x)coroutine.yield(x);return 29 end));assert(wrap(17)==17 and wrap()==29)
local co=coroutine.create(function(x)coroutine.yield(x);return 42 end)
local resume=copy(coroutine.resume);local ok,x=resume(co,17);assert(ok and x==17)
ok,x=resume(co);assert(ok and x==42)
local p=copy(newproxy);local a=p(true);assert(getmetatable(p(a))==getmetatable(a))
local file=assert(io.tmpfile());file:write('hello\n');file:seek('set',0)
local lines=copy(file:lines());assert(lines()=='hello'and lines()==nil);file:close()
local output=copy(io.output);local env={};debug.setfenv(output,env)
local value={};env[2]=value;assert(output()==value)
local caller=copy(getfenv);local fn,env=debug.getupvalue(caller,1)
assert(caller(0)==_G)
"#).unwrap()).unwrap();
}

#[test]
fn child_instruction_abort_leaves_no_body_context_and_vm_remains_reusable() {
    let mut vm = setup();
    vm.set_instruction_limit(Some(2000));
    let program=compile(b"local resume=copy(coroutine.resume);local co=coroutine.create(function()while true do end end);return resume(co)").unwrap();
    assert!(vm.execute(&program).is_err());
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"return copy(math.abs)(-17)").unwrap())
            .unwrap(),
        &[Value::Number(17.)]
    );
}

#[test]
fn direct_bodies_keep_complete_stacks_and_result_counts() {
    let mut vm = setup();
    assert_eq!(
        vm.execute(&compile(b"return body(math.abs,-17)").unwrap())
            .unwrap(),
        &[Value::Number(-17.), Value::Number(17.), Value::Number(1.)]
    );
    assert_eq!(
        vm.execute(&compile(b"return body(pcall,function()return 17,nil end)").unwrap())
            .unwrap(),
        &[
            Value::Boolean(true),
            Value::Number(17.),
            Value::Nil,
            Value::Number(3.)
        ]
    );
    vm.execute(&compile(br#"
local token={};local status,result,n=body(xpcall,function()error(token,0)end,function(e)assert(e==token);return 29 end)
assert(status==false and result==29 and n==2)
local reader=(function()local n=0;return function()n=n+1;if n==1 then return 'return 17'end end end)()
local a,b,c,d,f,count=body(load,reader,'=direct',token)
assert(a==reader and b=='=direct' and c=='return 17'and type(d)=='table'and f()==17 and count==1)
"#).unwrap()).unwrap();
}

#[test]
fn direct_bodies_preserve_caller_identity_without_extra_hook_events() {
    let mut vm = setup();
    vm.execute(
        &compile(
            br#"
local events={}
debug.sethook(function(e)if debug.getinfo(2,'f').func==body then events[#events+1]=e end end,'cr')
local function check()assert(debug.getinfo(2,'f').func==body);collectgarbage();return 17 end
local ok,v,count=body(pcall,check)
debug.sethook();assert(ok and v==17 and count==2);assert(table.concat(events,',')=='call,return')
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn direct_yield_returns_through_rust_before_thread_suspension() {
    let mut vm = setup();
    vm.execute(
        &compile(
            br#"
local co=coroutine.create(function()return body(coroutine.yield,17,29)end)
local ok,a,b=coroutine.resume(co);assert(ok and a==17 and b==29)
ok,a,b=coroutine.resume(co,42,57);assert(ok and a==42 and b==57 and coroutine.status(co)=='dead')
"#,
        )
        .unwrap(),
    )
    .unwrap();
}
