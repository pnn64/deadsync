use deadlib_lua::{Error, Value, Vm, compile, compile_named};

fn error_string(vm: &Vm, error: &Error) -> Vec<u8> {
    let Value::String(id) = error.value().unwrap() else {
        panic!("string error")
    };
    vm.string_bytes(id).unwrap().to_vec()
}

#[test]
fn standalone_errors_include_the_faulting_stack_before_cleanup() {
    let mut vm = Vm::new();
    vm.begin_standalone().unwrap();
    let program = compile_named(b"error('bad')", b"=standalone-test").unwrap();
    let error = vm.execute(&program).unwrap_err();
    assert!(vm.standalone_active());
    assert_eq!(error_string(&vm, &error), b"standalone-test:1: bad\nstack traceback:\n\t[C]: in function 'error'\n\tstandalone-test:1: in main chunk\n\t[C]: ?");
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.)]
    );
    vm.end_standalone().unwrap();
    assert!(!vm.standalone_active());
}

#[test]
fn message_handlers_inspect_and_mutate_live_faulting_locals() {
    let mut vm = Vm::new();
    vm.begin_standalone().unwrap();
    vm.execute(&compile(b"debug.traceback=function(e,l)local n,v=debug.getlocal(4,1);debug.setlocal(4,1,9);collectgarbage();return n..':'..v..':'..l end").unwrap()).unwrap();
    let error = vm
        .execute(
            &compile(b"function f()local x=7;hold=function()return x end;error('bad')end;f()")
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error_string(&vm, &error), b"x:7:2");
    assert_eq!(
        vm.execute(&compile(b"return hold()").unwrap()).unwrap(),
        &[Value::Number(9.)]
    );
    vm.end_standalone().unwrap();
}

#[test]
fn the_parked_main_frame_keeps_its_identity_and_returns_once() {
    let mut vm = Vm::new();
    vm.set_output_capture(true);
    vm.begin_standalone().unwrap();
    let value = vm
        .execute(&compile(b"return debug.getinfo(2,'f').func").unwrap())
        .unwrap()[0];
    vm.set_global(b"main_function", value).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(
        vm.execute(&compile(b"return debug.getinfo(2,'f').func==main_function").unwrap())
            .unwrap(),
        &[Value::Boolean(true)]
    );
    vm.execute(&compile(b"debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==main_function then print('main return')end end,'cr')").unwrap()).unwrap();
    vm.end_standalone().unwrap();
    assert_eq!(vm.output().unwrap(), b"main return\n");
}

#[test]
fn nil_handler_results_and_instruction_abort_preserve_reusable_state() {
    let mut vm = Vm::new();
    vm.begin_standalone().unwrap();
    vm.execute(&compile(b"debug.traceback=function()end").unwrap())
        .unwrap();
    let error = vm.execute(&compile(b"error('bad')").unwrap()).unwrap_err();
    assert_eq!(error.value(), Some(Value::Nil));
    vm.set_instruction_limit(Some(30));
    let error = vm
        .execute(&compile(b"while true do end").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"return 7").unwrap()).unwrap(),
        &[Value::Number(7.)]
    );
    vm.end_standalone().unwrap();
}

#[test]
fn failed_calls_collect_before_return_and_keep_the_error_value_rooted() {
    let mut vm = Vm::new();
    vm.begin_standalone().unwrap();
    vm.execute(&compile(b"local weak=setmetatable({},{__mode='v'});debug.traceback=function()local result={};weak[1]=result;local p=newproxy(true);getmetatable(p).__gc=function()assert(weak[1]~=nil);collected=true end;return result end").unwrap()).unwrap();
    let error = vm.execute(&compile(b"error('bad')").unwrap()).unwrap_err();
    assert_eq!(vm.global(b"collected"), Value::Boolean(true));
    vm.set_global(b"error_value", error.value().unwrap())
        .unwrap();
    vm.end_standalone().unwrap();
}

#[test]
fn successive_entries_reclaim_unretained_message_handler_closures() {
    let mut vm = Vm::new();
    vm.begin_standalone().unwrap();
    let program = compile(b"return 7").unwrap();
    for _ in 0..4_000 {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(7.)]);
    }
    let stats = vm.heap_stats();
    assert!(stats.collections > 0);
    assert!(stats.native_closures < 2_000, "{stats:?}");
    vm.collect_garbage().unwrap();
    assert_eq!(vm.heap_stats().native_closures, 1);
    vm.end_standalone().unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.heap_stats().native_closures, 0);
}
