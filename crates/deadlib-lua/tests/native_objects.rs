use deadlib_lua::{Error, HostCall, HostFunction, Value, Vm, compile};

fn vm(callback: HostFunction) -> Vm {
    let mut vm = Vm::new();
    let function = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn aligned_payloads_of_every_small_size_survive_nested_collection_and_mutation() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let mut pointers = Vec::new();
        for size in 0..66 {
            stack.new_userdata(size)?;
            let bytes = stack.userdata_bytes_mut(-1)?.unwrap();
            assert_eq!(bytes.len(), size);
            assert!(bytes.iter().all(|&byte| byte == 0));
            let pointer = bytes.as_ptr() as usize;
            assert_eq!(pointer % std::mem::align_of::<f64>(), 0);
            assert_eq!(pointer % std::mem::align_of::<usize>(), 0);
            assert!(!pointers.contains(&pointer));
            pointers.push(pointer);
            for (i, byte) in bytes.iter_mut().enumerate() {
                *byte = (size + i) as u8;
            }
        }
        stack.push(stack.value(1).unwrap())?;
        stack.call_unprotected(0, Some(0))?;
        for (size, pointer) in pointers.into_iter().enumerate() {
            let bytes = stack.userdata_bytes(size as isize + 2).unwrap();
            assert_eq!(bytes.as_ptr() as usize, pointer);
            assert_eq!(stack.object_len(size as isize + 2)?, size);
            let bytes = stack.userdata_bytes(size as isize + 2).unwrap();
            for (i, &byte) in bytes.iter().enumerate() {
                assert_eq!(byte, (size + i) as u8);
            }
        }
        stack.push(Value::Boolean(true))?;
        stack.finish(1)
    }
    assert_eq!(
        run(
            &mut vm(callback),
            br#"return host(function()
          local t={};for i=1,3000 do t[i]=newproxy(true)end;collectgarbage()
        end)"#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn payload_address_survives_moving_the_vm_and_releasing_its_last_root() {
    let mut engine = Vm::new();
    let value = engine.new_userdata(31);
    let Value::Userdata(id) = value else { panic!() };
    let root = engine.pin(value).unwrap();
    let bytes = engine.userdata_bytes_mut(id).unwrap();
    bytes[30] = 255;
    let pointer = bytes.as_ptr() as usize;
    let mut moved = Box::new(engine);
    moved.collect_garbage().unwrap();
    assert_eq!(moved.userdata_bytes(id).unwrap().as_ptr() as usize, pointer);
    assert_eq!(moved.userdata_bytes(id).unwrap()[30], 255);
    moved.unpin(root).unwrap();
    moved.collect_garbage().unwrap();
    assert!(moved.userdata_bytes(id).is_none());
}

#[test]
fn impossible_userdata_allocation_and_wrong_types_leave_the_stack_intact() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert!(stack.new_userdata(usize::MAX).is_err());
        assert!(stack.new_userdata(isize::MAX as usize).is_err());
        assert_eq!(stack.top(), 1);
        assert!(stack.userdata_bytes(1).is_none());
        assert!(stack.userdata_bytes_mut(1)?.is_none());
        stack.set_top(8000)?;
        assert!(stack.new_userdata(1).is_err());
        assert_eq!(stack.top(), 8000);
        stack.set_top(1)?;
        stack.finish(1)
    }
    assert_eq!(
        run(&mut vm(callback), b"return host(42)"),
        [Value::Number(42.)]
    );
}

#[test]
fn binary_and_numeric_strings_have_a_separate_trailing_nul() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let logical = stack.to_bytes(1)?.unwrap().to_vec();
        let bytes = stack.to_c_bytes(1)?.unwrap();
        assert_eq!(&bytes[..bytes.len() - 1], logical);
        assert_eq!(bytes.last(), Some(&0));
        assert!(matches!(stack.value(1), Some(Value::String(_))));
        assert_eq!(stack.object_len(1)?, logical.len());
        stack.finish(1)
    }
    let mut vm = vm(callback);
    assert_eq!(
        run(
            &mut vm,
            b"return host(17)== '17',host('a\\0b\\0')=='a\\0b\\0',host('')==''"
        ),
        [Value::Boolean(true); 3]
    );
}

#[test]
fn raw_lengths_do_not_invoke_metamethods_and_convert_only_numbers() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let length = stack.object_len(1)?;
        let value = stack.value(1).unwrap_or(Value::Nil);
        stack.push(Value::Number(length as f64))?;
        stack.push(value)?;
        assert!(stack.to_bytes(999)?.is_none());
        assert!(stack.to_c_bytes(-10000)?.is_none());
        stack.finish(2)
    }
    assert_eq!(
        run(
            &mut vm(callback),
            br#"
      local t=setmetatable({1,2,3},{__len=function()error('called')end})
      local n,v=host(t);assert(n==3 and v==t)
      local n,v=host(1.5);assert(n==3 and v=='1.5')
      local n,v=host(false);assert(n==0 and v==false)
      local n,v=host();return n,v
    "#
        ),
        [Value::Number(0.), Value::Nil]
    );
}

#[test]
fn userdata_environments_trace_cycles_and_native_creation_inherits_its_environment() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.new_userdata(3)?;
        stack.get_environment(-1)?;
        stack.push(stack.value(1).unwrap())?;
        assert!(stack.set_environment(-3)?);
        stack.finish(2)
    }
    let mut vm = vm(callback);
    assert_eq!(
        run(
            &mut vm,
            br#"
      local env={tag={}}
      local u,original=host(env);assert(original==_G)
      assert(debug.getfenv(u)==env);env.cycle=u
      local weak=setmetatable({u,env,env.tag},{__mode='v'})
      u=nil;env=nil;collectgarbage();collectgarbage()
      return weak[1]==nil,weak[2]==nil,weak[3]==nil
    "#
        ),
        [Value::Boolean(true); 3]
    );
}

#[test]
fn pending_unprotected_fault_prevents_object_api_mutation() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.new_userdata(3)?;
        stack.push(stack.value(1).unwrap())?;
        assert!(stack.call_unprotected(0, Some(0)).is_err());
        assert!(stack.new_userdata(3).is_err());
        assert!(stack.userdata_bytes_mut(2).is_err());
        assert!(stack.get_environment(2).is_err());
        assert!(stack.set_environment(2).is_err());
        assert!(stack.to_bytes(2).is_err());
        assert!(stack.to_c_bytes(2).is_err());
        assert!(stack.object_len(2).is_err());
        Ok(())
    }
    assert_eq!(
        run(
            &mut vm(callback),
            br#"
      local e={};local ok,actual=pcall(host,function()error(e,0)end)
      return ok,actual==e
    "#
        ),
        [Value::Boolean(false), Value::Boolean(true)]
    );
}

#[test]
fn unsupported_environment_setters_consume_the_table_and_getters_push_nil() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.create_table(0, 0)?;
        assert!(!stack.set_environment(1)?);
        assert_eq!(stack.top(), 1);
        stack.create_table(0, 0)?;
        assert!(!stack.set_environment(-2)?);
        assert_eq!(stack.top(), 1);
        stack.push(Value::Nil)?;
        assert!(stack.set_environment(1).is_err());
        assert_eq!(stack.top(), 2);
        stack.get_environment(1)?;
        assert_eq!(stack.value(-1), Some(Value::Nil));
        stack.get_environment(999)?;
        assert_eq!(stack.value(-1), Some(Value::Nil));
        stack.finish(1)
    }
    assert_eq!(run(&mut vm(callback), b"return host(42)"), [Value::Nil]);
}
