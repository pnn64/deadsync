use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn protect(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let handler = (stack.value(1) != Some(Value::Nil)).then_some(1);
    let results = stack.number(3)?;
    stack.remove(3)?;
    let arguments = stack.top() - 2;
    let status = stack.protected_call(
        arguments,
        (results >= 0.).then_some(results as usize),
        handler,
    )?;
    stack.push(Value::Number(status as u8 as f64))?;
    stack.insert(2)?;
    let count = stack.top() - 1;
    stack.finish(count)
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    let function = vm.new_host_function(protect, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn protected_results_preserve_prefix_arity_nil_padding_and_callable_values() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local function f(a)return a,nil,false,42 end
      local status,a,b,c,d=host(nil,f,-1,17)
      assert(status==0 and a==17 and b==nil and c==false and d==42)
      assert(select('#',host(nil,f,0))==1)
      assert(select('#',host(false,f,7))==8)
      local status,a,b=host(nil,setmetatable({},{__call=function(t,a)return t,a end}),-1,99)
      assert(status==0 and type(a)=='table' and b==99)
      return true
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn handlers_inspect_and_mutate_fault_frames_before_their_cells_close() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local marker={};local escaped;local observed=false
      local function fail()
        local retained={tag=17};escaped=function()return retained end
        error(marker,0)
      end
      local function handler(e)
        assert(e==marker)
        for level=2,12 do
          local info=debug.getinfo(level,'f');if not info then break end
          if info.func==fail then
            for i=1,6 do
              local name,value=debug.getlocal(level,i)
              if name=='retained' then
                assert(value.tag==17);debug.setlocal(level,i,{tag=42});observed=true
              end
            end
          end
        end
        collectgarbage();return e
      end
      local status,e=host(handler,fail,0)
      assert(status==2 and e==marker and observed and escaped().tag==42)
      collectgarbage();return escaped().tag
    "#
        ),
        [Value::Number(42.)]
    );
}

#[test]
fn error_status_distinguishes_handler_failure_from_identical_diagnostic_bytes() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local function fail()error({},0)end
      local status,e=host(function()return 'error in error handling'end,fail,-1)
      assert(status==2 and e=='error in error handling')
      local status,e=host(false,fail,-1)
      assert(status==5 and e=='error in error handling')
      local status,e=host(setmetatable({},{__call=function()return 'wrong'end}),fail,-1)
      assert(status==5 and e=='error in error handling')
      local status,e=host(nil,false,-1)
      assert(status==2 and e=='attempt to call a boolean value')
      return host(nil,function()return 42 end,-1)
    "#
        ),
        [Value::Number(0.), Value::Number(42.)]
    );
}

#[test]
fn nested_native_calls_reader_failures_and_coroutines_share_protection() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local marker={};local seen=0
      local function handler(e)seen=seen+1;assert(e==marker);return 'handled'end
      local status,a,b=host(handler,function()
        return load(function()error(marker,0)end)
      end,-1)
      assert(status==0 and a==nil and b=='handled' and seen==1)
      local status,a,b=host(handler,function()
        return host(nil,function()error(marker,0)end,-1)
      end,-1)
      assert(status==0 and a==2 and b==marker and seen==1)
      local status,n=host(nil,function()
        local co=coroutine.create(function()coroutine.yield(17);return 42 end)
        local ok,n=coroutine.resume(co);assert(ok and n==17)
        local ok,n=coroutine.resume(co);assert(ok);return n
      end,-1)
      assert(status==0 and n==42)
      local co=coroutine.create(function()return host(nil,function()coroutine.yield()end,-1)end)
      local ok,status,e=coroutine.resume(co)
      return ok,status,e:find('yield across',1,true)~=nil
    "#
        ),
        [
            Value::Boolean(true),
            Value::Number(2.),
            Value::Boolean(true)
        ]
    );
}

#[test]
fn a_handler_can_fail_recursively_and_later_recover_without_poisoning_the_callback() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local marker={};local calls=0
      local status,e=host(function(e)
        calls=calls+1;collectgarbage();if calls<4 then error(marker,0)end
        return e
      end,function()error(marker,0)end,-1)
      return status,e==marker,calls,host(nil,function()return 42 end,1)
    "#
        ),
        [
            Value::Number(2.),
            Value::Boolean(true),
            Value::Number(4.),
            Value::Number(0.),
            Value::Number(42.)
        ]
    );
}

#[test]
fn instruction_limits_stay_fatal_even_when_protected_status_is_ignored() {
    fn ignore(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let _ = stack.protected_call(0, Some(0), None);
        Ok(())
    }
    let mut vm = vm();
    let ignore = vm.new_host_function(ignore, &[]).unwrap();
    vm.set_global(b"ignore", ignore).unwrap();
    vm.set_instruction_limit(Some(200));
    let e = vm
        .execute(&compile(b"return ignore(function()while true do end end)").unwrap())
        .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        run(&mut vm, b"return host(nil,function()return 42 end,1)"),
        [Value::Number(0.), Value::Number(42.)]
    );
}

#[test]
fn invalid_protected_call_inputs_preserve_the_existing_window() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert!(stack.protected_call(usize::MAX, None, None).is_err());
        assert!(stack.protected_call(1, None, None).is_err());
        assert!(stack.protected_call(0, Some(8001), None).is_err());
        assert!(stack.protected_call(0, None, Some(999)).is_err());
        assert!(stack.protected_call(0, None, Some(-10000)).is_err());
        assert_eq!(stack.top(), 1);
        stack.finish(1)
    }
    let mut vm = vm();
    let callback = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"check", callback).unwrap();
    assert_eq!(run(&mut vm, b"return check(42)"), [Value::Number(42.)]);
}

#[test]
fn handler_overflow_returns_its_distinct_status_and_allows_a_later_call() {
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let mut vm = vm();
            vm.begin_standalone().unwrap();
            assert_eq!(
                run(
                    &mut vm,
                    br#"
          local attempts=0
          local status,e=host(function(e)attempts=attempts+1;error(e,0)end,
            function()error({},0)end,-1)
          assert(status==5 and e=='error in error handling' and attempts>100)
          return host(nil,function()return 42 end,1)
        "#
                ),
                [Value::Number(0.), Value::Number(42.)]
            );
            vm.end_standalone().unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn error_function_references_its_mutable_stack_slot() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local calls=0;local replacement=function()return 'replacement'end
      local function handler(e)
        calls=calls+1
        if calls==1 then
          for level=2,12 do
            local info=debug.getinfo(level,'f');if not info then break end
            if info.func==host then debug.setlocal(level,1,replacement);break end
          end
          error(e,0)
        end
        return 'snapshot'
      end
      local status,e=host(handler,function()error({},0)end,-1)
      return status,e=='replacement',calls
    "#
        ),
        [Value::Number(2.), Value::Boolean(true), Value::Number(1.)]
    );
}
