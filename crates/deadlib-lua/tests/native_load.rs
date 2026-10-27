use deadlib_lua::{Error, ErrorKind, HostCall, NativeLoadStatus, Value, Vm, compile};

fn install(vm: &mut Vm, callback: fn(&mut HostCall<'_>) -> Result<(), Error>) {
    let function = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn fixed_buffers_and_fragmented_native_readers_share_stack_and_errors() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let source = stack.to_bytes(1)?.unwrap().to_vec();
        let before = stack.top();
        let mut at = 0;
        let status = stack.load_reader(Some(b"=native reader"), |stack| {
            assert_eq!(stack.top(), before + if at == 0 { 0 } else { 2 });
            if at != 0 {
                assert!(matches!(stack.value(-2), Some(Value::Table(_))));
                assert!(matches!(stack.value(-1), Some(Value::ParserPrototype(_))));
                assert_eq!(stack.to_pointer(-1), None);
            }
            if at == source.len() {
                return Ok(None);
            }
            let byte = source[at];
            at += 1;
            Ok(Some(vec![byte]))
        })?;
        assert_eq!(stack.top(), before + 1);
        stack.push_integer(status as isize)?;
        stack.insert(-2)?;
        stack.finish(2)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,b"local s,f=host('return 17+25');assert(s==0 and f()==42);s,f=host('local =');assert(s==3 and type(f)=='string');local s,f=host('return 7');assert(s==0 and f()==7)");
}

#[test]
fn reader_callbacks_reenter_collect_and_change_original_caller_slots() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let reader = stack.value(1).unwrap();
        let status = stack.load_reader(None, |stack| {
            stack.push(reader)?;
            stack.call_unprotected(0, Some(1))?;
            let bytes = stack.to_bytes(-1)?.map(<[u8]>::to_vec);
            stack.set_top(stack.top() - 1)?;
            Ok(bytes)
        })?;
        assert_eq!(status, NativeLoadStatus::Ok);
        assert_eq!(stack.value(2), Some(Value::Number(91.)));
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(
        &mut vm,
        br#"
local i=0
local f=host(function()
 i=i+1
 assert(debug.getinfo(2,'f').func==host)
 assert(debug.setlocal(2,2,91))
 collectgarbage()
 local co=coroutine.create(function()coroutine.yield(17)end)
 assert(coroutine.resume(co))
 return ({'return ','42'})[i]
end,17)
assert(f()==42)
"#,
    );
}

#[test]
fn protected_reader_errors_inherit_handlers_then_return_status_and_recover() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let reader = stack.value(1).unwrap();
        let status = stack.load_reader(Some(b"=reader"), |stack| {
            stack.push(reader)?;
            stack.call_unprotected(0, Some(1))?;
            Ok(None)
        })?;
        assert_eq!(status, NativeLoadStatus::RuntimeError);
        assert_eq!(stack.top(), 2);
        assert_eq!(stack.load_buffer(b"return 42", None)?, NativeLoadStatus::Ok);
        stack.call_unprotected(0, Some(1))?;
        stack.finish(2)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,b"local marker,replaced={},{};local seen=false;local ok,value,number=xpcall(function()return host(function()error(marker)end)end,function(e)seen=e==marker;return replaced end);assert(ok and value==replaced and number==42 and seen)");
}

#[test]
fn initial_empty_probe_is_nonsticky_and_nested_native_loaders_are_independent() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let mut calls = 0;
        let status = stack.load_reader(None, |stack| {
            calls += 1;
            if calls == 1 {
                assert_eq!(stack.load_buffer(b"return 17", None)?, NativeLoadStatus::Ok);
                stack.call_unprotected(0, Some(1))?;
                assert_eq!(stack.value(-1), Some(Value::Number(17.)));
                stack.set_top(stack.top() - 1)?;
                Ok(None)
            } else if calls == 2 {
                Ok(Some(b"return 42".to_vec()))
            } else {
                Ok(None)
            }
        })?;
        assert_eq!(status, NativeLoadStatus::Ok);
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm, b"assert(host()()==42)");
}

#[test]
fn reader_instruction_limits_escape_and_the_vm_recovers() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let reader = stack.value(1).unwrap();
        stack.load_reader(None, |stack| {
            stack.push(reader)?;
            stack.call_unprotected(0, Some(1))?;
            Ok(None)
        })?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    vm.set_instruction_limit(Some(1000));
    let error = vm
        .execute(&compile(b"host(function()while true do end end)").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(run(&mut vm, b"return 42"), [Value::Number(42.)]);
}

#[test]
fn imported_binary_upvalues_start_nil_for_every_reader_block_size() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let bytes = stack.to_bytes(1)?.unwrap().to_vec();
        let size = stack.to_integer(2).unwrap() as usize;
        let mut parts = bytes.chunks(size);
        let status = stack.load_reader(Some(b"=native binary"), |_| {
            Ok(parts.next().map(<[u8]>::to_vec))
        })?;
        assert_eq!(status, NativeLoadStatus::Ok);
        stack.finish(1)
    }
    let reference = lua_reference::Lua::new();
    let function: lua_reference::Function = reference
        .load("local captured=17;return function()return captured,42 end")
        .eval()
        .unwrap();
    let bytes = function.dump(false);
    let mut vm = Vm::new();
    install(&mut vm, callback);
    let source = vm.intern(&bytes);
    vm.set_global(b"binary", source).unwrap();
    run(&mut vm,b"for size=1,#binary do local f=host(binary,size);local a,b=f();assert(a==nil and b==42)end");
}

#[test]
fn syntax_errors_bypass_handlers_and_reader_failures_cannot_be_ignored() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let reader = stack.value(1).unwrap();
        let status = stack.load_reader(Some(b"=native status"), |stack| {
            stack.push(reader)?;
            // A C-style error must survive an embedding that ignores Result.
            let _ = stack.call_unprotected(0, Some(1));
            let bytes = stack.to_bytes(-1)?.map(<[u8]>::to_vec);
            stack.set_top(stack.top() - 1)?;
            Ok(bytes)
        })?;
        stack.push_integer(status as isize)?;
        stack.insert(-2)?;
        stack.finish(2)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm, br#"
local marker={}
local ok,status,value=xpcall(function()return host(function()error(marker)end)end,
 function(e)assert(e==marker);return 42 end)
assert(ok and status==2 and value==42)
local n=0
ok,status,value=xpcall(function()return host(function()n=n+1;return n==1 and 'local ='or nil end)end,
 function()error('syntax must not invoke handler')end)
assert(ok and status==3 and value=="native status:1: '<name>' expected near '='")
ok,status,value=xpcall(function()return host(function()error(marker)end)end,function()error(17)end)
assert(ok and status==5 and value=='error in error handling')
assert(select(2,host(function()return nil end))~=nil)
"#);
}

#[test]
fn native_load_counts_the_host_call_in_the_syntax_depth_budget() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let source = stack.to_bytes(1)?.unwrap().to_vec();
        let status = stack.load_buffer(&source, None)?;
        stack.push_integer(status as isize)?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    vm.begin_standalone().unwrap();
    run(&mut vm,b"for depth=195,200 do local s='return '..string.rep('(',depth)..'42'..string.rep(')',depth);assert(host(s)==(depth<=196 and 0 or 3))end");
    vm.end_standalone().unwrap();
}

fn observing_reader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let bytes = stack.to_bytes(1)?.unwrap().to_vec();
    let observer = stack.value(2).unwrap();
    let width = stack.to_integer(3).unwrap() as usize;
    let mut blocks = bytes.chunks(width);
    let mut calls = 0;
    let status = stack.load_reader(Some(b"=observed roots"), |stack| {
        calls += 1;
        let roots: Vec<_> = (4..=stack.top())
            .map(|i| stack.value(i as isize).unwrap())
            .collect();
        stack.push(observer)?;
        stack.push_integer(calls)?;
        for &value in &roots {
            stack.push(value)?;
        }
        stack.call_unprotected(roots.len() + 1, Some(0))?;
        Ok(blocks.next().map(<[u8]>::to_vec))
    })?;
    assert_eq!(status, NativeLoadStatus::Ok);
    stack.finish(1)
}

#[test]
fn native_parser_prototypes_follow_closure_subtrees_and_tables_are_temporary() {
    let mut vm = Vm::new();
    install(&mut vm, observing_reader);
    run(
        &mut vm,
        br#"
local ids,weak={},setmetatable({},{__mode='v'})
local tables=setmetatable({},{__mode='k'})
local nested=false
local entry=host('return function()return 17 end,function()return 42 end',function(n,...)
 local roots={...}
 assert(#roots%2==0)
 for i=1,#roots,2 do
  assert(type(roots[i])=='table' and type(roots[i+1])=='proto')
  tables[roots[i]]=true
  if not ids[roots[i+1]] then local id=#weak+1;ids[roots[i+1]]=id;weak[id]=roots[i+1]end
 end
 if n==2 and not nested then
  nested=true
  assert(host('return 9',function()collectgarbage()end,1)()==9)
 end
 collectgarbage()
end,1)
assert(nested and #weak==3)
ids=nil;collectgarbage();assert(next(tables)==nil and weak[1] and weak[2] and weak[3])
local a,b=entry();assert(a()==17 and b()==42)
entry=nil;b=nil;collectgarbage();assert(weak[1]==nil and weak[2] and weak[3]==nil)
a=nil;collectgarbage();assert(next(weak)==nil)
"#,
    );
    vm.collect_garbage().unwrap();
    assert_eq!(vm.heap_stats().parser_prototypes, 0);
}

#[test]
fn binary_reader_prototypes_survive_fragmentation_and_collections() {
    let reference = lua_reference::Lua::new();
    let f: lua_reference::Function = reference
        .load("return function(a)return function(b)return a+b end end")
        .into_function()
        .unwrap();
    let mut vm = Vm::new();
    install(&mut vm, observing_reader);
    let binary = vm.intern(&f.dump(false));
    vm.set_global(b"binary", binary).unwrap();
    run(
        &mut vm,
        br#"
for width=1,32 do
 local seen=false
 local f=host(binary,function(n,...)
  for i=1,select('#',...)do assert(type(select(i,...))=='proto');seen=true end
  collectgarbage()
 end,width)
 assert(seen and f()(17)(25)==42)
end
"#,
    );
    vm.collect_garbage().unwrap();
    assert_eq!(vm.heap_stats().parser_prototypes, 0);
}

#[test]
fn parsed_function_order_survives_literal_branches_and_inline_siblings() {
    let mut vm = Vm::new();
    install(&mut vm, observing_reader);
    run(
        &mut vm,
        br#"
local sources={
 'return false and function()return 17 end or function()return 42 end',
 'if true then return function()return 42 end else return function()return 17 end end',
 'local t={function()return 42 end,function()return 17 end};return t[1]',
 'local function a()return function()return 17 end end;return function()return 42 end',
}

for _,source in ipairs(sources)do
 local entry=host(source,function()collectgarbage()end,1)
 assert(entry()()==42)
end
"#,
    );
}

#[test]
fn parser_root_stack_overflow_restores_the_loader_scope() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.set_top(7999)?;
        assert_eq!(
            stack.load_buffer(b"return 17", None)?,
            NativeLoadStatus::RuntimeError
        );
        assert_eq!(stack.top(), 8000);
        assert!(
            stack
                .to_bytes(-1)?
                .unwrap()
                .ends_with(b"native stack overflow")
        );
        stack.set_top(0)?;
        assert_eq!(stack.load_buffer(b"return 42", None)?, NativeLoadStatus::Ok);
        stack.call_unprotected(0, Some(1))?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(run(&mut vm, b"return host()"), [Value::Number(42.)]);
    vm.collect_garbage().unwrap();
    assert_eq!(vm.heap_stats().parser_prototypes, 0);
}
