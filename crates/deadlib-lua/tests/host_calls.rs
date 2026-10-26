use deadlib_lua::{Value, Vm, compile};

#[test]
fn chunks_receive_host_arguments_and_preserve_results() {
    let mut vm = Vm::new();
    let text = vm.intern(b"a\0b");
    let program = compile(b"local a,b,c=...;return select('#',...),a,b,c").unwrap();
    assert_eq!(
        vm.execute_with_args(&program, &[text, Value::Nil, Value::Number(3.)])
            .unwrap(),
        &[Value::Number(3.), text, Value::Nil, Value::Number(3.)]
    );
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[Value::Number(0.), Value::Nil, Value::Nil, Value::Nil]
    );
}

#[test]
fn host_calls_reuse_lua_closures_and_builtin_results() {
    let mut vm = Vm::new();
    let program = compile(b"local n=4;return function(x,...)n=n+x;return n,... end").unwrap();
    let function = vm.execute(&program).unwrap()[0];
    let root = vm.pin(function).unwrap();
    assert_eq!(
        vm.call(function, &[Value::Number(2.), Value::Nil]).unwrap(),
        &[Value::Number(6.), Value::Nil]
    );
    vm.collect_garbage().unwrap();
    assert_eq!(
        vm.call(function, &[Value::Number(3.)]).unwrap(),
        &[Value::Number(9.)]
    );
    let select = vm.global(b"select");
    let count = vm.intern(b"#");
    assert_eq!(
        vm.call(select, &[count, Value::Nil, Value::Boolean(false)])
            .unwrap(),
        &[Value::Number(2.)]
    );
    vm.unpin(root).unwrap();
}

#[test]
fn host_tables_use_raw_writes_and_callable_metatables() {
    let mut vm = Vm::new();
    let table = vm.new_table();
    vm.raw_table_set(table, Value::Number(-2.), Value::Number(7.))
        .unwrap();
    vm.set_global(b"host", Value::Table(table)).unwrap();
    vm.execute(&compile(b"setmetatable(host,{__newindex=function()error('raw')end,__call=function(t,x)return t[-2]+x end})").unwrap()).unwrap();
    vm.raw_table_set(table, Value::Number(-2.), Value::Number(8.))
        .unwrap();
    assert_eq!(
        vm.call(Value::Table(table), &[Value::Number(2.)]).unwrap(),
        &[Value::Number(10.)]
    );
    assert!(vm.raw_table_set(table, Value::Nil, Value::Nil).is_err());
    assert!(
        vm.raw_table_set(table, Value::Number(f64::NAN), Value::Nil)
            .is_err()
    );
}

#[test]
fn host_calls_reject_foreign_handles_and_recover_after_errors() {
    let mut vm = Vm::new();
    let mut other = Vm::new();
    let foreign = other.intern(b"foreign");
    let select = vm.global(b"select");
    assert!(vm.call(select, &[foreign]).is_err());
    let table = other.new_table();
    assert!(
        vm.raw_table_set(table, Value::Number(1.), Value::Nil)
            .is_err()
    );
    let program = compile(b"error({tag=7})").unwrap();
    let error = vm.execute_with_args(&program, &[Value::Nil]).unwrap_err();
    let Value::Table(_) = error.value().unwrap() else {
        panic!("error identity")
    };
    assert!(vm.call(Value::Nil, &[]).is_err());
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn immediate_builtin_results_remain_roots_after_arguments_are_discarded() {
    let mut vm = Vm::new();
    let outer = vm.execute(&compile(b"return {{tag=17}}").unwrap()).unwrap()[0];
    let unpack = vm.global(b"unpack");
    let inner = vm.call(unpack, &[outer]).unwrap()[0];
    vm.collect_garbage().unwrap();
    vm.set_global(b"saved", inner).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return saved.tag").unwrap()).unwrap(),
        &[Value::Number(17.)]
    );
    assert!(vm.call(unpack, &[outer]).is_err());
}

#[test]
fn host_file_loading_keeps_the_native_loader_after_global_mutation() {
    let path = std::env::temp_dir().join(format!("deadlib-host-{}.lua", std::process::id()));
    std::fs::write(
        &path,
        b"#!header\nlocal a,b=...;return a,b,debug.getinfo(1,'S').source",
    )
    .unwrap();
    let mut vm = Vm::new();
    vm.execute(&compile(b"loadfile=nil;dofile=nil;collectgarbage('collect')").unwrap())
        .unwrap();
    let function = vm
        .load_file(Some(path.to_str().unwrap().as_bytes()))
        .unwrap();
    let results = vm
        .call(Value::Function(function), &[Value::Number(2.), Value::Nil])
        .unwrap()
        .to_vec();
    assert_eq!(&results[..2], &[Value::Number(2.), Value::Nil]);
    let Value::String(source) = results[2] else {
        panic!("source name")
    };
    assert_eq!(
        vm.string_bytes(source).unwrap(),
        format!("@{}", path.display()).as_bytes()
    );
    std::fs::remove_file(&path).unwrap();
    let error = vm
        .load_file(Some(path.to_str().unwrap().as_bytes()))
        .unwrap_err();
    assert!(error.message().starts_with("cannot open "));
}

#[test]
fn native_chunk_parameters_and_old_varargs_use_the_shared_host_entry() {
    let lua = lua_reference::Lua::new();
    let function: lua_reference::Function = lua
        .load("return function(a,...)return a,arg.n,arg[1],arg[2]end")
        .eval()
        .unwrap();
    let program = compile(&function.dump(false)).unwrap();
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute_with_args(
            &program,
            &[Value::Number(9.), Value::Nil, Value::Number(7.)]
        )
        .unwrap(),
        &[
            Value::Number(9.),
            Value::Number(2.),
            Value::Nil,
            Value::Number(7.)
        ]
    );
}

#[test]
fn host_calls_restore_state_after_instruction_abort() {
    let mut vm = Vm::new();
    let function = vm
        .execute(&compile(b"return function()while true do end end").unwrap())
        .unwrap()[0];
    vm.set_instruction_limit(Some(100));
    let error = vm.call(function, &[]).unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute_with_args(&compile(b"return ...").unwrap(), &[Value::Number(42.)])
            .unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn host_global_writes_deliver_lua_newindex_and_recover_from_its_errors() {
    let mut vm = Vm::new();
    vm.execute(&compile(b"seen={};setmetatable(_G,{__newindex=function(t,k,v)seen[#seen+1]=k;if k=='bad' then error('bad write')end;rawset(t,k,v)end})").unwrap()).unwrap();
    vm.set_global(b"good", Value::Number(7.)).unwrap();
    assert!(vm.set_global(b"bad", Value::Nil).is_err());
    let values = vm
        .execute(&compile(b"return good,seen[1],seen[2],bad").unwrap())
        .unwrap()
        .to_vec();
    assert_eq!(values[0], Value::Number(7.));
    assert_eq!(values[3], Value::Nil);
    for (&value, expected) in values[1..3].iter().zip([b"good".as_slice(), b"bad"]) {
        let Value::String(id) = value else {
            panic!("callback key")
        };
        assert_eq!(vm.string_bytes(id).unwrap(), expected);
    }
}

#[test]
fn loaded_functions_follow_lua_ownership_after_the_program_is_dropped() {
    let mut vm = Vm::new();
    let function = {
        let program = compile(b"local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end;setfenv(1,{p=p})").unwrap();
        vm.load_program(&program)
    };
    vm.call(Value::Function(function), &[]).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.global(b"finalized"), Value::Boolean(true));
    assert!(vm.call(Value::Function(function), &[]).is_err());
}

#[test]
fn global_lookup_callbacks_keep_previous_results_alive_and_recover_after_errors() {
    for standalone in [false, true] {
        let mut vm = Vm::new();
        if standalone {
            vm.begin_standalone().unwrap();
        }
        vm.execute(&compile(b"setmetatable(_G,{__index=function(t,k)collectgarbage();if k=='bad' then error('lookup failed')end;return 17 end})").unwrap()).unwrap();
        assert_eq!(vm.global(b"missing"), Value::Nil);
        let table = vm.execute(&compile(b"return {tag=7}").unwrap()).unwrap()[0];
        let root = (!standalone).then(|| vm.pin(table).unwrap());
        assert_eq!(vm.get_global(b"missing").unwrap(), Value::Number(17.));
        vm.set_global(b"saved", table).unwrap();
        if let Some(root) = root {
            vm.unpin(root).unwrap();
        }
        assert_eq!(
            vm.execute(&compile(b"return saved.tag").unwrap()).unwrap(),
            &[Value::Number(7.)]
        );
        let error = vm.get_global(b"bad").unwrap_err();
        assert!(error.message().contains("lookup failed"));
        assert_eq!(
            vm.execute(&compile(b"return 42").unwrap()).unwrap(),
            &[Value::Number(42.)]
        );
        vm.end_standalone().unwrap();
    }
}
