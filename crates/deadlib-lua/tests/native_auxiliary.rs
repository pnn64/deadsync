use deadlib_lua::{Error, HostCall, Value, Vm, compile_named};

fn where_at(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let level = stack.number(1)? as isize;
    stack.push_where(level)?;
    stack.finish(1)
}
fn argument(call: &mut HostCall<'_>) -> Result<(), Error> {
    let stack = call.native_stack()?;
    Err(stack.argument_error(1, b"tag expected"))
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    for (name, callback) in [
        (
            b"where_at".as_slice(),
            where_at as deadlib_lua::HostFunction,
        ),
        (b"argument", argument),
    ] {
        let f = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, f).unwrap();
    }
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile_named(source, b"@native-aux.lua").unwrap())
        .unwrap()
        .to_vec()
}

#[test]
fn where_prefixes_use_visible_levels_and_ignore_c_or_missing_frames() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      assert(where_at(0)=='' and where_at(-1)=='' and where_at(999)=='')
      assert(where_at(1)=='native-aux.lua:3: ')
      local function callback()
        assert(where_at(1)=='native-aux.lua:5: ')
        assert(where_at(2)=='')
      end
      assert(pcall(callback))
      return true
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn argument_errors_keep_method_names_self_adjustment_and_source_locations() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local t={checked=argument}
      local ok,e=pcall(function()t:checked()end)
      assert(not ok and e:find("calling 'checked' on bad self (tag expected)",1,true))
      local ok,e=pcall(t.checked)
      assert(not ok and e=="bad argument #1 to '?' (tag expected)")
      return true
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn primitive_format_concatenation_does_not_dispatch_metamethods() {
    fn format(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let count = stack.top();
        stack.concatenate_primitives(count)?;
        stack.finish(1)
    }
    let mut vm = vm();
    let f = vm.new_host_function(format, &[]).unwrap();
    vm.set_global(b"format", f).unwrap();
    assert_eq!(
        run(
            &mut vm,
            br#"
      assert(format('a',17,'\0',2.5)=='a17\0002.5')
      local called=false
      local ok,e=pcall(format,setmetatable({},{__concat=function()called=true end}),'value')
      assert(not ok and not called)
      return format()=='' and type(format(17))=='number'
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn auxiliary_helpers_preserve_full_stacks_and_pending_unprotected_errors() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert!(stack.concatenate_primitives(2).is_err());
        assert_eq!(stack.top(), 1);
        stack.set_top(8000)?;
        assert!(stack.push_where(1).is_err());
        assert_eq!(stack.top(), 8000);
        stack.set_top(1)?;
        assert!(stack.call_unprotected(0, Some(0)).is_err());
        assert!(stack.push_where(1).is_err());
        assert!(stack.concatenate_primitives(0).is_err());
        Ok(())
    }
    let mut vm = vm();
    let f = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"callback", f).unwrap();
    assert_eq!(
        run(
            &mut vm,
            br#"local e={};local ok,a=pcall(callback,function()error(e,0)end);return ok,a==e"#
        ),
        [Value::Boolean(false), Value::Boolean(true)]
    );
}
