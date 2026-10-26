use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn install(vm: &mut Vm, function: fn(&mut HostCall<'_>) -> Result<(), Error>) {
    let function = vm.new_host_function(function, &[]).unwrap();
    vm.set_global(b"host", function).unwrap();
}
fn execute(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn mutable_stack_indexes_growth_coercion_and_selected_results() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        assert_eq!(stack.top(), 2);
        assert_eq!(stack.value(0), None);
        assert_eq!(stack.value(isize::MIN), None);
        assert_eq!(stack.value(3), None);
        stack.set(1, Value::Number(99.))?;
        stack.set_top(4)?;
        assert_eq!(stack.value(-1), Some(Value::Nil));
        stack.push(stack.value(2).unwrap())?;
        stack.set_top(3)?;
        let text = stack.intern(b"7.5");
        stack.push(text)?;
        assert_eq!(stack.number(-1)?, 7.5);
        stack.set(-2, Value::Boolean(false))?;
        stack.finish(4)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    let result = execute(&mut vm, b"return host(1,2)");
    assert_eq!(
        &result[..3],
        &[Value::Number(99.), Value::Number(2.), Value::Boolean(false)]
    );
    let Value::String(id) = result[3] else {
        panic!()
    };
    assert_eq!(vm.string_bytes(id), Some(b"7.5".as_slice()));
}

#[test]
fn native_temporaries_outgrow_captures_and_survive_collection_without_upvalues() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let collect = stack.value(1).unwrap();
        assert!(stack.check_stack(512));
        assert_eq!(stack.top(), 1);
        for i in 0..300 {
            let value = stack.intern(format!("temporary:{i}\0").as_bytes());
            stack.push(value)?;
        }
        stack.push(collect)?;
        stack.call(0, Some(0))?;
        stack.finish(300)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local values={host(function()collectgarbage()end)}
assert(#values==300)
for i=1,300 do assert(values[i]=='temporary:'..(i-1)..'\0')end
return debug.getinfo(host,'u').nups,#values
"#
        ),
        [Value::Number(0.), Value::Number(300.)]
    );
}

#[test]
fn popped_native_temporaries_stop_rooting_objects_before_the_next_callback() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let create = stack.value(1).unwrap();
        let collect = stack.value(2).unwrap();
        stack.push(create)?;
        stack.call(0, Some(1))?;
        assert!(matches!(stack.value(-1), Some(Value::Table(_))));
        stack.set_top(2)?;
        stack.push(collect)?;
        stack.call(0, Some(1))?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local weak=setmetatable({},{__mode='v'})
return host(function()local value={};weak[1]=value;return value end,
function()collectgarbage();return weak[1]==nil end)
"#
        ),
        [Value::Boolean(true)]
    );
}

#[test]
fn stack_call_result_counts_pad_nil_and_keep_a_live_prefix() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let function = stack.value(1).unwrap();
        stack.push(Value::Number(42.))?;
        stack.push(function)?;
        assert_eq!(stack.call(0, Some(4))?, 4);
        assert_eq!(stack.value(-1), Some(Value::Nil));
        stack.finish(5)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(
            &mut vm,
            b"return host(function()collectgarbage();return 1,nil,false end)"
        ),
        [
            Value::Number(42.),
            Value::Number(1.),
            Value::Nil,
            Value::Boolean(false),
            Value::Nil
        ]
    );
}

#[test]
fn native_return_hooks_mutate_the_selected_result_slots_in_place() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let result = stack.number(1)? + stack.number(2)?;
        stack.push(Value::Number(result))?;
        stack.push(Value::Number(stack.top() as f64))?;
        stack.finish(2)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local seen=false
debug.sethook(function(event)
if event=='return'and debug.getinfo(2,'f').func==host then
local _,a=debug.getlocal(2,1);local _,b=debug.getlocal(2,2)
local _,c=debug.getlocal(2,3);local _,d=debug.getlocal(2,4)
assert(a=='17.5'and b==25 and c==42.5 and d==3)
debug.setlocal(2,3,99);collectgarbage();seen=true
end end,'r')
local a,b=host('17.5',25);debug.sethook();return a,b,seen
"#
        ),
        [Value::Number(99.), Value::Number(3.), Value::Boolean(true)]
    );
}

#[test]
fn direct_stack_errors_preserve_fault_locals_identity_and_later_calls() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let stack = call.native_stack()?;
        Err(stack.raise(stack.value(1).unwrap_or(Value::Nil)))
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local payload={tag=17};local inspected=false
local ok,e=xpcall(function()host(payload)end,function(e)
local _,value=debug.getlocal(2,1);assert(value==payload)
collectgarbage();inspected=true;return e end)
local a,b=pcall(host,nil)
return ok,e==payload,inspected,a,b
"#
        ),
        [
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(false),
            Value::Nil
        ]
    );
}

#[test]
fn stack_capacity_checks_and_failed_operations_preserve_the_top() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.set_top(8000)?;
        assert!(!stack.check_stack(1));
        assert!(!stack.check_stack(usize::MAX));
        assert!(stack.push(Value::Nil).is_err());
        assert!(stack.set_top(8001).is_err());
        assert_eq!(stack.top(), 8000);
        stack.set_top(0)?;
        assert!(stack.set(-1, Value::Nil).is_err());
        assert!(stack.call(0, None).is_err());
        assert_eq!(stack.top(), 0);
        stack.push(Value::Boolean(true))?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(execute(&mut vm, b"return host(17)"), [Value::Boolean(true)]);
}

#[test]
fn resource_abort_from_a_nested_stack_call_remains_fatal_when_ignored() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.push(stack.value(1).unwrap())?;
        let _ = stack.call(0, None);
        stack.finish(0)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    vm.set_instruction_limit(Some(100));
    let error = vm
        .execute(&compile(b"host(function()while true do end end)").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        execute(&mut vm, b"return select('#',host(function()return 42 end))"),
        [Value::Number(0.)]
    );
}

#[test]
fn discarded_original_arguments_are_not_kept_as_duplicate_roots() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let collect = stack.value(2).unwrap();
        stack.set_top(0)?;
        stack.push(collect)?;
        stack.call(0, Some(1))?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(
            &mut vm,
            br#"
local weak=setmetatable({},{__mode='v'})
local function argument()local value={};weak[1]=value;return value end
local gone=host(argument(),function()collectgarbage();return weak[1]==nil end)
local named={tag=17};local captured=function()return named.tag end
host(named,function()collectgarbage();return true end)
return gone,named.tag,captured()
"#
        ),
        [Value::Boolean(true), Value::Number(17.), Value::Number(17.)]
    );
}

#[test]
fn an_empty_finished_stack_cannot_reopen_collected_original_arguments() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        stack.set_top(0)?;
        drop(stack);
        assert!(call.arguments().is_empty());
        assert!(call.native_stack().is_err());
        assert!(call.number(0).is_err());
        Ok(())
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    assert_eq!(
        execute(&mut vm, b"return select('#',host({}))"),
        [Value::Number(0.)]
    );
}

#[test]
fn native_windows_preserve_sort_jobs_iterator_state_and_metamethod_continuations() {
    fn less(call: &mut HostCall<'_>) -> Result<(), Error> {
        let collect = call.captures()[0];
        let mut stack = call.native_stack()?;
        let less = stack.number(1)? < stack.number(2)?;
        stack.push(collect)?;
        stack.call(0, Some(0))?;
        stack.push(Value::Boolean(less))?;
        stack.finish(1)
    }
    fn iterator(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Table(table) = call.arguments()[0] else {
            panic!()
        };
        let index = match call.arguments()[1] {
            Value::Nil => 1.,
            Value::Number(n) => n + 1.,
            _ => panic!(),
        };
        let value = call.raw_table_get(table, Value::Number(index))?;
        let mut stack = call.native_stack()?;
        if value == Value::Nil {
            return stack.finish(0);
        }
        stack.push(Value::Number(index))?;
        stack.push(value)?;
        stack.finish(2)
    }
    fn index(call: &mut HostCall<'_>) -> Result<(), Error> {
        call.native_stack()?.finish(1)
    }
    let mut vm = Vm::new();
    let collect = vm.global(b"collectgarbage");
    let compare = vm.new_host_function(less, &[collect]).unwrap();
    let iterator = vm.new_host_function(iterator, &[]).unwrap();
    let index = vm.new_host_function(index, &[]).unwrap();
    vm.set_global(b"compare", compare).unwrap();
    vm.set_global(b"iterate", iterator).unwrap();
    vm.set_global(b"index", index).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            br#"
local values={4,2,3,1};table.sort(values,compare)
local total=0;for i,v in iterate,{10,20,30}do total=total+i+v end
local object=setmetatable({},{__index=index})
return table.concat(values,',')=='1,2,3,4',total,object.answer=='answer'
"#
        ),
        [
            Value::Boolean(true),
            Value::Number(66.),
            Value::Boolean(true)
        ]
    );
}
