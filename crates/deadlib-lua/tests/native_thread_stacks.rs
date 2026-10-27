use deadlib_lua::{Error, GcOperation, HostCall, NativeThreadStatus, Value, Vm, compile};

fn api(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    match stack.to_integer(1).unwrap() {
        0 => {
            stack.new_thread()?;
            stack.finish(1)
        }
        1 => {
            let to = stack.to_thread(2).unwrap();
            let count = stack.top() - 2;
            let from = stack.thread_id();
            stack.move_thread_values(from, to, count)?;
            stack.finish(0)
        }
        2 => {
            let co = stack.to_thread(2).unwrap();
            let values = {
                let s = stack.thread_stack(co)?;
                (1..=s.top())
                    .map(|i| s.value(i as isize).unwrap())
                    .collect::<Vec<_>>()
            };
            stack.push_integer(values.len() as isize)?;
            for v in &values {
                stack.push(*v)?;
            }
            stack.finish(values.len() + 1)
        }
        3 => {
            let from = stack.to_thread(2).unwrap();
            let to = stack.to_thread(3).unwrap();
            let n = stack.to_integer(4).unwrap() as usize;
            stack.move_thread_values(from, to, n)?;
            stack.finish(0)
        }
        4 => {
            let co = stack.to_thread(2).unwrap();
            let top = stack.to_integer(3).unwrap() as usize;
            stack.thread_stack(co)?.set_top(top)?;
            stack.finish(0)
        }
        5 => {
            let co = stack.to_thread(2).unwrap();
            let n = stack.thread_stack(co)?.top();
            stack.push_integer(n as isize)?;
            stack.finish(1)
        }
        6 => {
            let co = stack.to_thread(2).unwrap();
            let count = stack.to_integer(3).unwrap() as usize;
            let status = stack.resume_thread(co, count)?;
            stack.push_integer(status as isize)?;
            stack.finish(1)
        }
        7 => {
            let from = stack.to_thread(2).unwrap();
            let to = stack.to_thread(3).unwrap();
            let before = [stack.value(1), stack.value(2), stack.value(3)];
            stack.copy_thread_call_level(from, to)?;
            assert_eq!([stack.value(1), stack.value(2), stack.value(3)], before);
            stack.finish(0)
        }
        8 => {
            let thread = stack.to_thread(2).unwrap();
            let level = stack.thread_call_level(thread).unwrap();
            stack.push_integer(level as isize)?;
            stack.finish(1)
        }
        9 => {
            let thread = stack.to_thread(2).unwrap();
            let status = stack.thread_status(thread).unwrap();
            stack.push_integer(status as isize)?;
            stack.finish(1)
        }
        _ => unreachable!(),
    }
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    let f = vm.new_host_function(api, &[]).unwrap();
    vm.set_global(b"api", f).unwrap();
    fn yield_values(call: &mut HostCall<'_>) -> Result<(), Error> {
        let stack = call.native_stack()?;
        let count = stack.top();
        stack.yield_results(count)
    }
    let f = vm.new_host_function(yield_values, &[]).unwrap();
    vm.set_global(b"native_yield", f).unwrap();
    fn current(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push_thread()?;
        stack.finish(1)
    }
    fn nested(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let arguments = stack.top() - 1;
        stack.call_unprotected(arguments, None)?;
        let count = stack.top();
        stack.finish(count)
    }
    fn collect(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.gc(GcOperation::Collect)?;
        stack.finish(0)
    }
    fn reader(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let zero = stack.to_thread(1).unwrap();
        let function = stack.value(2).unwrap();
        let reset = stack.to_integer(3).unwrap() as usize;
        let mut visits = 0;
        let status = stack.load_reader(Some(b"=level-reader"), |stack| {
            visits += 1;
            if visits == reset {
                stack.copy_thread_call_level(zero, stack.thread_id())?;
            }
            stack.push(function)?;
            stack.push_integer(visits as isize)?;
            stack.call_unprotected(1, Some(1))?;
            Ok(stack.to_bytes(-1)?.map(<[u8]>::to_vec))
        })?;
        stack.push_integer(status as isize)?;
        stack.insert(-2)?;
        stack.push_integer(visits as isize)?;
        stack.finish(3)
    }
    for (name, callback) in [
        (
            b"current_thread".as_slice(),
            current as fn(&mut HostCall<'_>) -> Result<(), Error>,
        ),
        (b"native_call", nested),
        (b"native_collect", collect),
        (b"native_reader", reader),
    ] {
        let f = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, f).unwrap();
    }
    vm
}

#[test]
fn stock_reader_loads_wrap_the_c_call_count_and_restore_parser_failures() {
    let mut vm = vm();
    run(&mut vm,br#"
for _,mode in ipairs{'value','empty','syntax','error','invalid'}do
 local co=api(0);api(1,co,function()
  local me=current_thread();local zero,keep=api(0),api(0);api(7,me,keep)
  pcall(function()api(7,zero,me)end);assert(api(8,me)==65535)
  local trace={};local n=0
  local f,e=load(function()
   n=n+1;trace[n]=api(8,me)
   if mode=='empty'then return nil elseif mode=='error'then error('bad',0)
   elseif mode=='invalid'then return {} elseif n==1 then return mode=='syntax'and'local x = )'or'return 42'end
  end,'=wrapped-reader')
  assert(api(8,me)==65535);api(7,keep,me)
  assert(trace[1]==0)
  if mode=='value'or mode=='empty'then assert(type(f)=='function'and e==nil and n==2)
  else assert(f==nil and type(e)=='string')end
  if mode=='value'then assert(f()==42 and trace[2]==1)end
  return 42
 end)
 assert(api(6,co,0)==0 and api(8,co)==0)
end
"#);
}

#[test]
fn stock_readers_execute_before_syntax_limits_in_the_c_error_reserve() {
    std::thread::Builder::new().stack_size(32*1024*1024).spawn(||{
        let mut vm=vm();run(&mut vm,br#"
local exact=api(0);local capture=api(0)
local function nest(n,f)if n==0 then return f()end;return native_call(function()return nest(n-1,f)end)end
api(1,capture,function()local me=current_thread();nest(198,function()api(7,me,exact)end)end)
assert(api(6,capture,0)==0 and api(8,exact)==199)
for _,mode in ipairs{'value','empty','error','invalid'}do
 local co=api(0);api(1,co,function()
  local me=current_thread();assert(api(8,me)==200);local n=0
  local f,e=load(function()
   n=n+1;assert(api(8,me)==201)
   if mode=='empty'then return nil elseif mode=='error'then error('bad',0)
   elseif mode=='invalid'then return {} else return 'return 42'end
  end,'=reserve-reader')
  assert(f==nil and type(e)=='string'and api(8,me)==200)
  assert(n==(mode=='empty'and 2 or 1));return 42
 end)
 api(7,exact,co);assert(api(6,co,0)==0 and api(8,co)==199)
end
local co=api(0);api(1,co,function()
 local me=current_thread();local visits=0
 local ok,e=pcall(function()
  api(7,exact,me)
  for _ in function()visits=visits+1;return 1 end do break end
 end)
 assert(not ok and visits==0 and api(8,me)==1 and e:find('C stack overflow',1,true))
 return 42
end)
assert(api(6,co,0)==0)
"#);
    }).unwrap().join().unwrap();
}

#[test]
fn copied_call_levels_preserve_stacks_and_reach_active_and_normal_threads() {
    let mut vm = vm();
    run(&mut vm, br#"
local main=current_thread();assert(api(8,main)==1)
local zero,save,co=api(0),api(0),api(0);api(7,main,save)
local object={};api(1,co,object,false,nil);api(7,main,co)
local n,a,b,c=api(2,co);assert(n==3 and a==object and b==false and c==nil and api(8,co)==1)
api(4,co,0);api(1,co,function()return api(8,current_thread())end)
assert(api(6,co,0)==0 and api(8,co)==1);local n,v=api(2,co);assert(n==1 and v==2)
local parent=api(0);api(1,parent,function()
 assert(api(8,main)==1);api(7,zero,main);assert(api(8,main)==0);return 42
end)
assert(api(6,parent,0)==0 and api(8,main)==0);api(7,save,main);assert(api(8,main)==1)
local active=api(0);api(1,active,function()
 assert(api(8,current_thread())==1);api(7,zero,current_thread());assert(api(8,current_thread())==0);return 42
end)
assert(api(6,active,0)==0 and api(8,active)==65535)
assert(api(6,active,0)==2 and api(9,active)==0 and api(8,active)==65535)
"#);
}

#[test]
fn protected_call_level_changes_restore_on_failure_and_wrap_on_success() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local zero,save,co=api(0),api(0),api(0)
api(1,co,function()
 local current=current_thread();api(7,current,save)
 local ok,e=pcall(function()api(7,zero,current);error('bad',0)end)
 assert(not ok and e=='bad' and api(8,current)==1)
 ok,e=pcall(function()api(7,zero,current);return 42 end)
 assert(ok and e==42 and api(8,current)==65535)
 local n=native_call(function()return api(8,current)end)
 assert(n==0 and api(8,current)==65535);api(7,save,current)
 collectgarbage();local seen={}
 do local p=newproxy(true);getmetatable(p).__gc=function()seen[#seen+1]=api(8,current)end end
 native_collect();assert(seen[1]==2 and api(8,current)==1)
 do local p=newproxy(true);getmetatable(p).__gc=function()seen[#seen+1]=api(8,current)end end
 collectgarbage();assert(seen[2]==2 and api(8,current)==1)
 local hooks=0;local busy=false
 debug.sethook(function(event)
  if not busy then busy=true;assert(api(8,current)>=2);hooks=hooks+1;busy=false end
 end,'cr')
 assert(pcall(function()return tostring(setmetatable({},{__tostring=function()return 'ok'end}))end))
 debug.sethook(nil);assert(hooks>0);return 42
end)
assert(api(6,co,0)==0 and api(8,co)==0)
"#,
    );
    run(
        &mut vm,
        b"zero=api(0);saved=api(0);api(7,current_thread(),saved);api(7,zero,current_thread())",
    );
    assert_eq!(
        run(&mut vm, b"return api(8,current_thread())"),
        vec![Value::Number(0.)]
    );
    let error = vm
        .execute(
            &compile(
                b"local function f()api(7,saved,current_thread());error('bad',0)end;return f()",
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.message_bytes(), b"bad");
    assert_eq!(
        run(&mut vm, b"return api(8,current_thread())"),
        vec![Value::Number(0.)]
    );
    run(&mut vm, b"api(7,saved,current_thread())");
}

#[test]
fn copied_levels_preserve_the_resume_limit_and_error_handler_status() {
    std::thread::Builder::new().stack_size(32*1024*1024).spawn(|| {
        let mut vm = vm();
        run(&mut vm, br#"
local main=current_thread();local exact,high,co=api(0),api(0),api(0)
local function nest(n,f)if n==0 then return f()end;return native_call(function()return nest(n-1,f)end)end
api(1,exact,function()return api(8,current_thread())end)
api(1,high,function()return api(8,current_thread())end)
api(1,co,function()
 local current=current_thread()
 nest(198,function()api(7,current,exact)end);assert(api(8,exact)==199)
 local ok,e=xpcall(function()nest(500,function()end)end,function(e)api(7,current,high);return e end)
 assert(not ok and type(e)=='string' and api(8,current)==1 and api(8,high)==201)
 return 42
end)
assert(api(6,co,0)==0 and api(8,co)==0)
assert(api(6,exact,0)==0 and api(8,exact)==199);assert(select(2,api(2,exact))==200)
assert(api(6,high,0)==2 and api(9,high)==0 and api(8,high)==201)
assert(select(2,api(2,high))=='C stack overflow')
local bad=api(0);api(1,bad,function()
 local zero=api(0);local chunks={'return ','(','(','17',')',')'}
 native_reader(zero,function(i)return chunks[i]end,4)
 print('forces a native call at the wrapped level')
end)
assert(api(6,bad,0)==5 and api(9,bad)==5 and api(8,bad)==65534)
local elevated=api(0);native_call(function()api(7,current_thread(),elevated)end)
assert(api(8,elevated)==2)
for _,yield in ipairs{native_yield,coroutine.yield}do
 local co=api(0);api(1,co,function()api(7,elevated,current_thread());return yield(17)end)
 assert(api(6,co,0)==2 and api(9,co)==2 and api(8,co)==1)
end
"#);
    }).unwrap().join().unwrap();
}

#[test]
fn native_parser_level_changes_cover_text_binary_and_protected_recovery() {
    let mut vm = vm();
    let lua = lua_reference::Lua::new();
    let binary = lua
        .load("return function(a)return function(b)return a,b end end")
        .into_function()
        .unwrap()
        .dump(false);
    let bytes = vm.intern(&binary);
    vm.set_global(b"wire", bytes).unwrap();
    run(&mut vm, br#"
local co=api(0);api(1,co,function()
 local zero=api(0);local current=current_thread()
 local status,f,n=native_reader(zero,function(i)return ({'return ','(','(','17',')',')'})[i]end,2)
 assert(status==0 and f()==17 and n==7 and api(8,current)==65535)
end)
assert(api(6,co,0)==0 and api(8,co)==65534)
for reset=0,3 do
 local co=api(0);api(1,co,function()
  local zero=api(0);local current=current_thread()
  local status,e,n=native_reader(zero,function(i)return ({'local',' x =',' )'})[i]end,reset)
  assert(status==3 and type(e)=='string' and n==4 and api(8,current)==1)
  status,e,n=native_reader(zero,function(i)if i==2 then api(7,zero,current);error('bad',0)end;return 'return 'end,0)
  assert(status==2 and e=='bad' and api(8,current)==1);return 42
 end)
 assert(api(6,co,0)==0 and api(8,co)==0)
end
for _,size in ipairs{1,7,4096}do
 local co=api(0);api(1,co,function()
  local zero=api(0);local current=current_thread()
  local status,f,n=native_reader(zero,function(i)local s=wire:sub((i-1)*size+1,i*size);if s~=''then return s end end,1)
  assert(status==0 and type(f)=='function' and api(8,current)==0);return 42
 end)
 assert(api(6,co,0)==0 and api(8,co)==65535)
end
"#);
}

#[test]
fn copying_collected_thread_call_levels_rejects_without_mutation() {
    use std::sync::{Arc, Mutex};
    let saved = Arc::new(Mutex::new(None::<deadlib_lua::ThreadId>));
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let saved = call
            .function_data::<Arc<Mutex<Option<deadlib_lua::ThreadId>>>>()
            .unwrap()
            .clone();
        let mut stack = call.native_stack()?;
        if let Some(thread) = stack.to_thread(1) {
            *saved.lock().unwrap() = Some(thread);
        } else {
            let stale = saved.lock().unwrap().unwrap();
            let current = stack.thread_id();
            let before = stack.thread_call_level(current);
            assert_eq!(stack.thread_call_level(stale), None);
            assert!(stack.copy_thread_call_level(stale, current).is_err());
            assert!(stack.copy_thread_call_level(current, stale).is_err());
            assert_eq!(stack.thread_call_level(current), before);
            assert_eq!(stack.top(), 0);
        }
        stack.finish(0)
    }
    let mut vm = vm();
    let f = vm
        .new_host_function_with_data(callback, &[], saved)
        .unwrap();
    vm.set_global(b"save_level_thread", f).unwrap();
    run(
        &mut vm,
        b"save_level_thread(api(0));collectgarbage();save_level_thread()",
    );
}

#[test]
fn native_resume_keeps_values_and_interoperates_with_lua_resume() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local co=api(0);local marker={}
api(1,co,'prefix',function(a)local b,c=native_yield(a,nil,false);return b,c end,marker)
assert(api(6,co,1)==1)
local n,a,b,c=api(2,co);assert(n==3 and a==marker and b==nil and c==false)
api(1,co,17,25);assert(api(6,co,2)==0)
local n,prefix,a,b=api(2,co);assert(n==3 and prefix=='prefix' and a==17 and b==25)
api(4,co,0);api(1,co,tonumber,'42');assert(api(6,co,1)==0)
assert(select(2,api(2,co))==42)
co=coroutine.create(function()local v=native_yield(17);return v end)
assert(select(2,coroutine.resume(co))==17);api(1,co,42);assert(api(6,co,1)==0)
co=coroutine.create(function()local v=coroutine.yield(17);return v end)
assert(api(6,co,0)==1);assert(select(2,coroutine.resume(co,42))==42)
for _,yield in ipairs{native_yield,coroutine.yield}do
 co=coroutine.create(function()return yield('old',nil,false)end)
 assert(api(6,co,0)==1)
 local name,a=debug.getlocal(co,0,1);local name3,c=debug.getlocal(co,0,3)
 assert(name=='(*temporary)' and a=='old' and name3==name and c==false and debug.getlocal(co,0,4)==nil)
 assert(debug.setlocal(co,0,1,'changed')==name and debug.setlocal(co,0,4,42)==name)
 local n,a,b,c=api(2,co);assert(n==3 and a=='changed' and b==nil and c==false)
 api(4,co,0);assert(debug.getlocal(co,0,1)==nil and debug.setlocal(co,0,1,42)==name)
 assert(api(6,co,0)==0 and api(5,co)==0)
 co=coroutine.create(function()return yield('old')end)
 local seen=0
 debug.sethook(co,function(event)
  if event=='return' and debug.getinfo(2,'f').func==yield then
   local n1,a=debug.getlocal(2,1);local n2,b=debug.getlocal(2,2)
   local n3,c=debug.getlocal(2,3);local n4,d=debug.getlocal(2,4)
   assert(n1=='(*temporary)' and n2==n1 and n3==n1 and n4==n1)
   assert(a=='old' and b==17 and c==25 and type(d)=='table' and debug.getlocal(2,5)==nil)
   debug.setlocal(2,2,19);collectgarbage();seen=seen+1
  end
 end,'r')
 assert(api(6,co,0)==1);api(1,co,17,25);assert(api(6,co,2)==0)
 local n,a,b=api(2,co);assert(n==2 and a==19 and b==25 and seen==1)
 debug.sethook(co,nil)
end
"#,
    );
}

#[test]
fn native_resume_retains_error_status_and_rejects_yield_boundaries() {
    let mut vm = vm();
    run(&mut vm, br#"
local message='attempt to yield across metamethod/C-call boundary'
local ok,e=pcall(native_yield,17);assert(not ok and e==message)
local co=coroutine.create(function()return pcall(native_yield,17)end)
assert(api(6,co,0)==0);local n,ok,e=api(2,co);assert(n==2 and not ok and e==message)
co=coroutine.create(function()error(17)end)
assert(api(6,co,0)==2);local n,a,b,c=api(2,co);assert(n==3 and a==17 and type(b)=='string' and b==c)
assert(api(6,co,0)==2);local n,e=api(2,co);assert(n==1 and e=='cannot resume non-suspended coroutine')
"#);
}

#[test]
fn yielding_callbacks_return_through_rust_and_are_not_reentered_on_resume() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Returned(Arc<[AtomicUsize; 2]>);
    impl Drop for Returned {
        fn drop(&mut self) {
            self.0[1].fetch_add(1, Ordering::Relaxed);
        }
    }
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let state = call
            .function_data::<Arc<[AtomicUsize; 2]>>()
            .unwrap()
            .clone();
        state[0].fetch_add(1, Ordering::Relaxed);
        let _returned = Returned(state);
        let stack = call.native_stack()?;
        let count = stack.top();
        stack.yield_results(count)
    }
    let mut vm = vm();
    let state = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
    let f = vm
        .new_host_function_with_data(callback, &[], state.clone())
        .unwrap();
    vm.set_global(b"yield_once", f).unwrap();
    run(
        &mut vm,
        br#"
co=coroutine.create(function()local a=yield_once(17);local b=yield_once(a);return b end)
assert(api(6,co,0)==1)
"#,
    );
    assert_eq!(state[0].load(Ordering::Relaxed), 1);
    assert_eq!(state[1].load(Ordering::Relaxed), 1);
    run(
        &mut vm,
        br#"
api(1,co,29);assert(api(6,co,1)==1)
api(1,co,42);assert(api(6,co,1)==0)
local n,v=api(2,co);assert(n==1 and v==42)
"#,
    );
    assert_eq!(state[0].load(Ordering::Relaxed), 2);
    assert_eq!(state[1].load(Ordering::Relaxed), 2);
}

#[test]
fn native_resume_limits_escape_handlers_and_close_child_cells() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
co=coroutine.create(function()
 local x=42;escaped=function()return x end
 while true do x=x end
end)
"#,
    );
    vm.set_instruction_limit(Some(100));
    let error = vm
        .execute(&compile(b"return pcall(function()return api(6,co,0)end)").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    vm.set_instruction_limit(None);
    run(
        &mut vm,
        b"assert(escaped()==42 and coroutine.status(co)=='dead');collectgarbage()",
    );
    assert_eq!(run(&mut vm, b"return 42"), vec![Value::Number(42.)]);
}

#[test]
fn native_resume_validates_argument_counts_before_touching_stacks() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let entry = stack.value(1).unwrap();
        let co = stack.new_thread()?;
        stack.thread_stack(co)?.push(entry)?;
        for count in [1, 2, usize::MAX] {
            assert!(stack.resume_thread(co, count).is_err());
            let child = stack.thread_stack(co)?;
            assert_eq!(child.top(), 1);
            assert_eq!(child.value(1), Some(entry));
        }
        assert_eq!(stack.resume_thread(co, 0)?, NativeThreadStatus::Ok);
        let child = stack.thread_stack(co)?;
        assert_eq!(child.top(), 1);
        assert_eq!(child.value(1), Some(Value::Number(42.)));
        assert_eq!(stack.top(), 2);
        stack.finish(0)
    }
    let mut vm = vm();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"validate_resume", f).unwrap();
    run(&mut vm, b"validate_resume(function()return 42 end)");
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn native_empty_threads_accept_entries_prefixes_and_reuse_after_completion() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local co=api(0);assert(coroutine.status(co)=='dead' and api(5,co)==0)
local f=function(a,b)collectgarbage();return a+b end
api(1,co,'prefix',f);local ok,prefix,result=coroutine.resume(co,17,25)
assert(ok and prefix=='prefix' and result==42 and api(5,co)==0)
api(1,co,f);ok,result=coroutine.resume(co,21,21);assert(ok and result==42)
api(1,co,tonumber);ok,result=coroutine.resume(co,'42');assert(ok and result==42)
"#,
    );
}

#[test]
fn inactive_stack_moves_preserve_nil_order_identity_and_shared_heap_roots() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local a,b=api(0),api(0);local object={};local f=function()return 42 end
api(1,a,nil,false,42,'a\0b',object,f,a);api(3,a,b,5)
local n,x,y,z,fn,co=api(2,b);assert(n==5 and x==42 and y=='a\0b' and z==object and fn==f and co==a)
api(3,b,a,0);assert(api(5,a)==2 and api(5,b)==5);api(3,a,a,999999)
local weak=setmetatable({},{__mode='v'})
do local v={};weak[1]=v;api(1,b,v)end
collectgarbage();assert(weak[1]);api(4,b,0);collectgarbage();assert(weak[1]==nil)
local cycle=api(0);weak[2]=cycle;api(1,cycle,cycle);cycle=nil;collectgarbage();assert(weak[2]==nil)
"#,
    );
}

#[test]
fn yielded_stacks_accept_staged_values_and_normal_parents_expose_auxresume_window() {
    let mut vm = vm();
    fn current(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        s.push_thread()?;
        s.finish(1)
    }
    let f = vm.new_host_function(current, &[]).unwrap();
    vm.set_global(b"current", f).unwrap();
    run(
        &mut vm,
        br#"
local main=current()
local co=api(0)
api(1,co,function(a)
 local n,parent=api(2,main);assert(n==1 and parent==coroutine.running())
 local b,c=coroutine.yield(a,nil,false);return b,c
end)
local ok,a,b,c=coroutine.resume(co,17);assert(ok and a==17 and b==nil and c==false and api(5,co)==0)
api(1,co,91,92);collectgarbage();local n,x,y=api(2,co);assert(n==2 and x==91 and y==92)
ok,a,b=coroutine.resume(co,18,19);assert(ok and a==18 and b==19 and api(5,co)==0)
"#,
    );
}

#[test]
fn inactive_primitives_coerce_slots_and_keep_thread_global_pseudo_indexes() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let co = s.new_thread()?;
        let globals = s.value(-10002).unwrap();
        let registry = s.value(-10000).unwrap();
        let replacement = s.intern(b"different");
        {
            let mut t = s.thread_stack(co)?;
            assert_eq!(t.value(-10002), Some(globals));
            assert_eq!(t.value(-10000), Some(registry));
            t.push(Value::Number(17.))?;
            t.push(Value::Nil)?;
            t.push(Value::Boolean(false))?;
            t.push(replacement)?;
            t.insert(1)?;
            t.remove(2)?;
            t.set(2, Value::Number(42.))?;
            assert_eq!(t.to_number(-2), Some(42.));
            assert_eq!(t.to_c_bytes(-2)?, Some(b"42\0".as_slice()));
            assert_eq!(t.object_len(1)?, 9);
            assert_eq!(t.top(), 3);
            assert!(t.to_pointer(1).is_none());
            assert!(!t.push_thread()?);
            assert_eq!(t.to_thread(-1), Some(co));
        }
        s.gc(GcOperation::Collect)?;
        {
            let mut t = s.thread_stack(co)?;
            assert_eq!(t.top(), 4);
            assert_eq!(t.value(2).unwrap().type_name(), "string");
            t.set_top(0)?;
        }
        s.finish(1)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    run(
        &mut vm,
        b"local co=host();assert(type(co)=='thread' and coroutine.status(co)=='dead')",
    );
}

#[test]
fn rejected_moves_and_stale_thread_handles_leave_live_stacks_unchanged() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let a = s.new_thread()?;
        let b = s.new_thread()?;
        {
            let mut t = s.thread_stack(a)?;
            t.push(Value::Number(17.))?;
        }
        assert!(s.move_thread_values(a, b, 2).is_err());
        assert_eq!(s.thread_stack(a)?.top(), 1);
        assert_eq!(s.thread_stack(b)?.top(), 0);
        s.thread_stack(b)?.set_top(8000)?;
        assert!(s.move_thread_values(a, b, 1).is_err());
        assert_eq!(s.thread_stack(a)?.value(1), Some(Value::Number(17.)));
        s.move_thread_values(b, b, usize::MAX)?;
        s.thread_stack(b)?.set_top(0)?;
        s.set_top(0)?;
        s.gc(GcOperation::Collect)?;
        assert!(s.thread_stack(a).is_err());
        assert!(s.thread_stack(b).is_err());
        let current = s.thread_id();
        assert!(s.thread_stack(current).is_err());
        s.finish(0)
    }
    let mut vm = Vm::new();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", f).unwrap();
    run(&mut vm, b"host()");
}

#[test]
fn failed_stack_windows_retain_diagnostics_and_pop_without_resetting_error_status() {
    let mut vm = vm();
    run(
        &mut vm,
        br#"
local co=coroutine.create(function()return nil+17 end)
assert(not coroutine.resume(co));local n,a,b,raw,located=api(2,co)
assert(n==4 and a==nil and b==nil and type(raw)=='string' and type(located)=='string')
api(4,co,0);assert(api(5,co)==0 and coroutine.status(co)=='dead')
api(1,co,function()return 42 end);assert(not coroutine.resume(co))
"#,
    );
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let s = call.native_stack()?;
        let co = s.to_thread(1).unwrap();
        assert_eq!(s.thread_status(co), Some(NativeThreadStatus::RuntimeError));
        s.finish(0)
    }
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"check_status", f).unwrap();
    run(&mut vm,b"local co=coroutine.create(function()error(17)end);assert(not coroutine.resume(co));check_status(co)");
}
