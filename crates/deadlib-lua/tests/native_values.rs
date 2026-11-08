use deadlib_lua::{Error, ErrorKind, HostCall, HostFunction, Value, Vm, compile};

fn compare(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let equal = stack.equal(1, 2)?;
    stack.push(Value::Boolean(equal))?;
    stack.finish(1)
}
fn less(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let less = stack.less_than(1, 2)?;
    stack.push(Value::Boolean(less))?;
    stack.finish(1)
}
fn concat(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let count = stack.top();
    stack.concatenate(count)?;
    stack.finish(1)
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    for (name, callback) in [
        (b"eq".as_slice(), compare as HostFunction),
        (b"lt", less),
        (b"cat", concat),
    ] {
        let function = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, function).unwrap();
    }
    vm
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn nonmutating_scalar_conversions_keep_signed_zero_and_reject_booleans() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert_eq!(stack.to_number(1), Some(1.5));
        assert_eq!(stack.to_integer(1), Some(1));
        assert!(matches!(stack.value(1), Some(Value::String(_))));
        assert_eq!(stack.to_number(2).unwrap().to_bits(), (-0f64).to_bits());
        assert_eq!(stack.to_integer(3), Some(isize::MIN));
        assert!(stack.to_number(4).is_none());
        assert!(stack.to_integer(4).is_none());
        assert!(!stack.to_boolean(4));
        assert!(stack.to_boolean(2));
        assert!(!stack.to_boolean(999));
        assert!(stack.to_number(999).is_none());
        stack.push_integer(isize::MAX)?;
        assert_eq!(stack.to_number(-1), Some(isize::MAX as f64));
        stack.finish(1)
    }
    let mut vm = vm();
    let callback = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"convert", callback).unwrap();
    assert_eq!(
        run(&mut vm, b"return convert('1.5',-0,1/0,false)"),
        [Value::Number(isize::MAX as f64)]
    );
}

#[test]
fn comparison_does_not_retain_private_copies_after_callback_operands_are_cleared() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local weak=setmetatable({},{__mode='v'})
      local function equal(a,b)
        for level=2,12 do
          local info=debug.getinfo(level,'f');if not info then break end
          if info.func==eq then
            debug.setlocal(level,1,nil);debug.setlocal(level,2,nil);break
          end
        end
        a,b=nil,nil;collectgarbage();collectgarbage()
        return weak[1]==nil and weak[2]==nil
      end
      local mt={__eq=equal}
      local function make(i)local value=setmetatable({},mt);weak[i]=value;return value end
      return eq(make(1),make(2))
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn comparison_uses_common_metamethods_and_normalizes_their_results() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local calls=0
      local function equal(a,b)calls=calls+1;collectgarbage();return {}end
      local a,b=setmetatable({},{__eq=equal}),setmetatable({},{__eq=equal})
      assert(eq(a,b) and eq(a,a) and calls==1)
      assert(not eq(a,setmetatable({},{__eq=function()return true end})))
      local u,v=newproxy(true),newproxy(true)
      getmetatable(u).__eq=equal;getmetatable(v).__eq=equal;assert(eq(u,v))
      local function order()collectgarbage();return 'truthy'end
      setmetatable(a,{__lt=order});setmetatable(b,{__lt=order});assert(lt(a,b))
      assert(not eq(1,'1') and not eq(0/0,0/0) and eq(0,-0))
      assert(not eq() and not lt() and eq(nil,nil))
      local ok,e=pcall(lt,1,'1');assert(not ok and e=='attempt to compare number with string')
      return calls
    "#
        ),
        [Value::Number(2.)]
    );
}

#[test]
fn concatenation_preserves_zero_one_binary_and_numeric_operand_rules() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      assert(cat()=='' and select('#',cat())==1)
      local object={};assert(cat(object)==object and type(cat(17))=='number')
      assert(cat('a\0',17,'',2.5)=='a\000172.5')
      assert(cat(17,'')=='17' and cat('',-0)=='-0')
      return true
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn native_concat_updates_original_slots_and_top_before_later_callbacks() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local seen=false
      local a=setmetatable({},{__concat=function(a,b)
        for level=2,12 do
          local info=debug.getinfo(level,'f');if not info then break end
          if info.func==cat then
            local _,v=debug.getlocal(level,2);assert(v=='172.5')
            local _,last=debug.getlocal(level,3);assert(last=='2.5');seen=true
          end
        end
        collectgarbage();return 'result'
      end})
      assert(cat(a,17,2.5)=='result' and seen)
      return seen
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn concatenation_runs_right_to_left_and_retains_fault_frames_and_coercions() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local marker={};local retained;local inspected=false;local trace=''
      local function fail(a,b)
        local local_value={tag=17};retained=function()return local_value end
        trace=trace..a.name..':'..b;error(marker,0)
      end
      local a=setmetatable({name='left'},{__concat=fail})
      local ok,e=xpcall(function()return cat(a,17,2.5)end,function(e)
        for level=2,12 do
          local info=debug.getinfo(level,'f');if not info then break end
          if info.func==fail then
            for i=1,6 do local n,v=debug.getlocal(level,i)
              if n=='local_value' then debug.setlocal(level,i,{tag=42});inspected=true end
            end
          elseif info.func==cat then
            local _,v=debug.getlocal(level,2);assert(v=='172.5')
            local _,last=debug.getlocal(level,3);assert(last=='2.5')
          end
        end
        collectgarbage();return e
      end)
      return ok,e==marker,inspected,retained().tag,trace=='left:172.5'
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

#[test]
fn native_concat_keeps_original_operand_roots_until_the_operation_finishes() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local weak=setmetatable({},{__mode='v'});local observed=false
      local function right()
        local object=newproxy(true);weak[1]=object;return object
      end
      local middle=setmetatable({},{__concat=function()return 'middle'end})
      local left=setmetatable({},{__concat=function(a,b)
        assert(b=='middle');collectgarbage();collectgarbage()
        observed=weak[1]~=nil;return 'result'
      end})
      assert(cat(left,middle,right())=='result')
      collectgarbage();collectgarbage();return observed and weak[1]==nil
    "#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn metamethod_callbacks_can_resume_children_but_cannot_yield_the_native_boundary() {
    assert_eq!(
        run(
            &mut vm(),
            br#"
      local function method()
        local co=coroutine.create(function()coroutine.yield(17);return 'child'end)
        local ok,n=coroutine.resume(co);assert(ok and n==17)
        local ok,n=coroutine.resume(co);assert(ok);return n
      end
      assert(cat(setmetatable({},{__concat=method}),'value')=='child')
      local co=coroutine.create(function()
        return cat(setmetatable({},{__concat=function()coroutine.yield()end}),'value')
      end)
      local ok,e=coroutine.resume(co)
      return ok,e:find('yield across',1,true)~=nil
    "#
        ),
        [Value::Boolean(false), Value::Boolean(true)]
    );
}

#[test]
fn native_value_apis_reject_bad_counts_and_keep_instruction_limits_fatal() {
    fn invalid(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert!(stack.concatenate(usize::MAX).is_err());
        assert_eq!(stack.top(), 1);
        stack.finish(1)
    }
    let mut vm = vm();
    let invalid = vm.new_host_function(invalid, &[]).unwrap();
    vm.set_global(b"invalid", invalid).unwrap();
    assert_eq!(run(&mut vm, b"return invalid(42)"), [Value::Number(42.)]);
    vm.set_instruction_limit(Some(300));
    let e = vm
        .execute(
            &compile(
                b"return cat(setmetatable({},{__concat=function()while true do end end}),'value')",
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        run(&mut vm, b"return cat(17,25) == '1725'"),
        [Value::Boolean(true)]
    );
}
