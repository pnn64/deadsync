use deadlib_lua::{ErrorKind, HostCall, Value, Vm, compile};

#[test]
fn host_userdata_obeys_the_budget_and_releases_storage_after_collection() {
    fn allocate(call: &mut HostCall<'_>) -> Result<(), deadlib_lua::Error> {
        let Value::Number(size) = call.arguments()[0] else {
            unreachable!()
        };
        let value = call.try_new_userdata(size as usize)?;
        call.push(value)
    }
    let mut vm = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    let allocate = vm.new_host_function(allocate, &[]).unwrap();
    vm.set_global(b"allocate", allocate).unwrap();
    let before = vm.memory_usage().unwrap().bytes;
    let value = vm.call(allocate, &[Value::Number(65536.0)]).unwrap()[0];
    let Value::Userdata(id) = value else {
        panic!("uploaded storage")
    };
    let retained = vm.pin(value).unwrap();
    assert_eq!(vm.userdata_bytes(id).unwrap(), vec![0; 65536]);
    assert!(vm.memory_usage().unwrap().bytes >= before + 65536);
    assert_eq!(
        vm.call(allocate, &[Value::Number(262144.0)])
            .unwrap_err()
            .kind(),
        ErrorKind::Memory
    );
    vm.collect_garbage().unwrap();
    assert_eq!(vm.userdata_bytes(id).unwrap().len(), 65536);
    let retained_bytes = vm.memory_usage().unwrap().bytes;
    vm.unpin(retained).unwrap();
    let type_function = vm.global(b"type");
    vm.call(type_function, &[Value::Nil]).unwrap();
    vm.collect_garbage().unwrap();
    assert!(vm.userdata_bytes(id).is_none());
    assert!(vm.memory_usage().unwrap().bytes + 65536 <= retained_bytes);
    let Value::Userdata(id) = vm.call(allocate, &[Value::Number(216.0)]).unwrap()[0] else {
        panic!("recovery after rejected upload")
    };
    assert_eq!(vm.userdata_bytes(id).unwrap().len(), 216);
    let usage = vm.memory_usage().unwrap();
    assert!(usage.peak_bytes <= usage.limit_bytes);
}

#[test]
fn loaded_bytecode_is_charged_once_and_released_with_its_binding() {
    let source = format!("local a=0; {} return a", "a=a+1;".repeat(20000));
    let program = compile(source.as_bytes()).unwrap();
    let mut small = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    assert_eq!(
        small.try_load_program(&program).unwrap_err().kind(),
        ErrorKind::Memory
    );
    assert_eq!(small.heap_stats().programs, 0);

    let mut vm = Vm::try_new_with_memory_limit(4 * 1024 * 1024).unwrap();
    let first = vm.try_load_program(&program).unwrap();
    let loaded = vm.memory_usage().unwrap().bytes;
    assert_eq!(vm.try_load_program(&program).unwrap(), first);
    assert_eq!(vm.memory_usage().unwrap().bytes, loaded);
    drop(program);
    // Loading retains the entry in result registers; its charge must survive
    // until those registers are replaced, even after the host drops Program.
    assert_eq!(vm.collect_garbage().unwrap().programs, 1);
    let type_function = vm.global(b"type");
    vm.call(type_function, &[Value::Nil]).unwrap();
    assert_eq!(vm.collect_garbage().unwrap().programs, 0);
    assert!(vm.memory_usage().unwrap().bytes < loaded);
}

#[test]
fn initialization_must_fit_the_budget() {
    for limit in [0, 1, 256] {
        let error = match Vm::try_new_with_memory_limit(limit) {
            Err(error) => error,
            Ok(_) => panic!("insufficient budget"),
        };
        assert_eq!(error.kind(), ErrorKind::Memory);
        assert_eq!(error.value(), None);
    }
}

#[test]
fn compilation_workspace_and_lua_loadstring_share_the_vm_ceiling() {
    let source = format!("local a=0; {} return a", "a=a+1;".repeat(20000));
    let mut vm = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    assert_eq!(
        vm.try_load_source(source.as_bytes(), b"=large source")
            .unwrap_err()
            .kind(),
        ErrorKind::Memory
    );
    assert_eq!(vm.heap_stats().programs, 0);
    let function = vm.try_load_source(b"return 42", b"=small source").unwrap();
    assert_eq!(
        vm.call(Value::Function(function), &[]).unwrap(),
        &[Value::Number(42.0)]
    );

    let source_value = vm.try_intern(source.as_bytes()).unwrap();
    vm.set_global(b"source", source_value).unwrap();
    let program = compile(b"local f,e=loadstring(source); return f,e").unwrap();
    let values = vm.execute(&program).unwrap().to_vec();
    assert_eq!(values[0], Value::Nil);
    let Value::String(message) = values[1] else {
        panic!("loadstring memory diagnostic")
    };
    assert_eq!(
        vm.string_bytes(message),
        Some(b"not enough memory".as_slice())
    );
    let usage = vm.memory_usage().unwrap();
    assert!(usage.peak_bytes <= usage.limit_bytes);
}

#[test]
fn streaming_load_budget_survives_reader_reentry_and_is_released_on_failure() {
    let mut vm = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    let program = compile(
        br#"
        local reads = 0
        local f,e = load(function()
            reads = reads + 1
            if reads == 1 then
                assert(assert(loadstring('return 42'))() == 42)
                collectgarbage('collect')
                return 'local a=0;'
            end
            if reads < 20000 then return 'a=a+1;' end
            return 'return a'
        end)
        return f,e,reads
    "#,
    )
    .unwrap();
    let values = vm.execute(&program).unwrap().to_vec();
    assert_eq!(values[0], Value::Nil);
    let Value::String(message) = values[1] else {
        panic!("load memory diagnostic")
    };
    assert_eq!(
        vm.string_bytes(message),
        Some(b"not enough memory".as_slice())
    );
    let Value::Number(reads) = values[2] else {
        panic!("reader count")
    };
    assert!(reads > 1.0 && reads < 20000.0);
    vm.collect_garbage().unwrap();
    let function = vm
        .try_load_source(b"return 42", b"=after reader failure")
        .unwrap();
    assert_eq!(
        vm.call(Value::Function(function), &[]).unwrap(),
        &[Value::Number(42.0)]
    );
    assert!(vm.memory_usage().unwrap().peak_bytes <= 256 * 1024);
}

#[test]
fn loadfile_source_storage_is_bounded_and_failure_closes_the_job() {
    struct File(std::path::PathBuf);
    impl Drop for File {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let path =
        File(std::env::temp_dir().join(format!("deadlua-memory-limit-{}.lua", std::process::id())));
    // A comment exercises source growth independently of compiler instruction
    // storage. The loader must reject before buffering the entire input.
    std::fs::write(&path.0, format!("--{}\nreturn 42", "x".repeat(1024 * 1024))).unwrap();
    let mut vm = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    let name = vm.try_intern(path.0.to_str().unwrap().as_bytes()).unwrap();
    vm.set_global(b"path", name).unwrap();
    let program = compile(b"local f,e=loadfile(path); return f,e").unwrap();
    let values = vm.execute(&program).unwrap().to_vec();
    assert_eq!(values[0], Value::Nil);
    let Value::String(message) = values[1] else {
        panic!("loadfile memory diagnostic")
    };
    assert_eq!(
        vm.string_bytes(message),
        Some(b"not enough memory".as_slice())
    );
    vm.collect_garbage().unwrap();
    std::fs::write(&path.0, b"return 42").unwrap();
    let function = vm
        .load_file(Some(path.0.to_str().unwrap().as_bytes()))
        .unwrap();
    assert_eq!(
        vm.call(Value::Function(function), &[]).unwrap(),
        &[Value::Number(42.0)]
    );
    assert!(vm.memory_usage().unwrap().peak_bytes <= 256 * 1024);
}

#[test]
fn oversized_string_is_rejected_before_growth_and_the_vm_recovers() {
    let mut vm = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    let program = compile(b"return string.rep('x', 1024 * 1024)").unwrap();
    let error = vm.execute(&program).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Memory);
    let usage = vm.memory_usage().unwrap();
    assert!(usage.bytes <= usage.peak_bytes);
    assert!(usage.peak_bytes <= usage.limit_bytes);
    vm.collect_garbage().unwrap();
    assert_eq!(
        vm.execute(&compile(b"return 6 * 7").unwrap()).unwrap(),
        &[Value::Number(42.0)]
    );
}

#[test]
fn dump_workspace_cannot_bypass_the_remaining_vm_budget() {
    let limit = 512 * 1024;
    let mut vm = Vm::try_new_with_memory_limit(limit).unwrap();
    let source = format!(
        "return function() local a=0; {} return a end",
        "a=a+1;".repeat(2000)
    );
    let program = compile(source.as_bytes()).unwrap();
    let function = vm.execute(&program).unwrap()[0];
    vm.set_global(b"f", function).unwrap();
    let dump = compile(b"return string.dump(f)").unwrap();
    let caller = vm.try_load_program(&dump).unwrap();
    let pinned = vm.pin(Value::Function(caller)).unwrap();
    let remaining = limit - vm.memory_usage().unwrap().bytes;
    let padding = vm.try_intern(&vec![b'x'; remaining - 32 * 1024]).unwrap();
    vm.set_global(b"padding", padding).unwrap();
    assert_eq!(
        vm.call(Value::Function(caller), &[]).unwrap_err().kind(),
        ErrorKind::Memory
    );
    vm.unpin(pinned).unwrap();
    vm.set_global(b"padding", Value::Nil).unwrap();
    vm.collect_garbage().unwrap();
    let small = vm
        .try_load_source(b"return 42", b"=after dump failure")
        .unwrap();
    assert_eq!(
        vm.call(Value::Function(small), &[]).unwrap(),
        &[Value::Number(42.0)]
    );
    assert!(vm.memory_usage().unwrap().peak_bytes <= limit);
}

#[test]
fn table_budget_failure_does_not_affect_another_state() {
    let mut bounded = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    let growth = compile(b"local t={} for i=1,100000 do t[i]={i} end return t").unwrap();
    assert_eq!(
        bounded.execute(&growth).unwrap_err().kind(),
        ErrorKind::Memory
    );
    assert!(bounded.memory_usage().unwrap().peak_bytes <= 256 * 1024);
    let mut other = Vm::try_new_with_memory_limit(512 * 1024).unwrap();
    assert_eq!(
        other
            .execute(&compile(b"return #string.rep('a', 50000)").unwrap())
            .unwrap(),
        &[Value::Number(50000.0)]
    );
    assert!(Vm::new().memory_usage().is_none());
}

#[test]
fn bounded_rust_binding_storage_keeps_its_native_alignment() {
    #[repr(align(64))]
    struct Aligned(u8);
    fn callback(call: &mut HostCall<'_>) -> Result<(), deadlib_lua::Error> {
        let data = call.function_data::<Aligned>().unwrap();
        assert_eq!((std::ptr::from_ref(data) as usize) % 64, 0);
        let value = data.0;
        call.push(Value::Number(f64::from(value)))
    }
    let mut vm = Vm::try_new_with_memory_limit(256 * 1024).unwrap();
    let function = vm
        .new_host_function_with_data(callback, &[], Aligned(42))
        .unwrap();
    assert_eq!(vm.call(function, &[]).unwrap(), &[Value::Number(42.0)]);
}
