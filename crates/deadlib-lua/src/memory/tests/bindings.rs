use super::*;
use crate::{NativeDebugFrame, Program, compile};

fn fixed_memory(vm: &Vm, error: Error) {
    assert_eq!(error.kind(), ErrorKind::Memory);
    assert_eq!(error.line(), 0);
    let Some(Value::String(id)) = error.value() else {
        panic!("fixed memory value");
    };
    assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
}

fn imported_captures() -> Program {
    let program =
        compile(b"local alpha,beta=17,25;return function()return alpha,beta end").unwrap();
    let mut vm = Vm::new_bare();
    let function = vm.execute(&program).unwrap()[0];
    let root = vm.pin(function).unwrap();
    let mut bytes = Vec::new();
    vm.with_native_stack(|stack| {
        stack.push(function)?;
        assert_eq!(
            stack.dump(|_, block| {
                bytes.extend_from_slice(block);
                Ok(0)
            })?,
            0
        );
        Ok(())
    })
    .unwrap();
    vm.unpin(root).unwrap();
    compile(&bytes).unwrap()
}

#[test]
fn program_binding_rejects_each_owned_allocation_without_publishing_partial_objects() {
    fn run(program: &Program, reject_at: usize, expected: &[Value]) -> usize {
        let ledger = Ledger::default();
        let endpoint = Endpoint {
            ledger: &ledger,
            tag: 1,
        };
        // SAFETY: endpoint and ledger outlive the VM and its entire allocation domain.
        let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
        let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
        vm.with_native_stack(|s| s.push(Value::Number(73.)))
            .unwrap();
        let start = ledger.attempts.get();
        ledger
            .reject_at
            .set(if reject_at == 0 { 0 } else { start + reject_at });
        let result = vm.try_load_program(program);
        let points = ledger.attempts.get() - start;
        if reject_at != 0 {
            fixed_memory(&vm, result.unwrap_err());
            let stats = vm.heap_stats();
            assert_eq!((stats.programs, stats.functions, stats.upvalues), (0, 0, 0));
            // Failure retains the caller's previous result stack, even while
            // every subsequent domain allocation remains rejected.
            vm.with_native_stack(|s| {
                assert_eq!(s.value(-1), Some(Value::Number(73.)));
                Ok(())
            })
            .unwrap();
        } else {
            result.unwrap();
        }
        ledger.reject_at.set(0);
        let function = vm.try_load_program(program).unwrap();
        assert_eq!(vm.call(Value::Function(function), &[]).unwrap(), expected);
        assert_eq!(vm.try_load_program(program).unwrap(), function);
        drop(vm);
        drop(memory);
        assert!(
            ledger.blocks.borrow().is_empty(),
            "binding failure {reject_at}"
        );
        points
    }
    let source = compile(b"local first='owned first';local second='owned second';local f=function()return first,second end;return 42").unwrap();
    let native = imported_captures();
    let mut total = 0;
    for (program, expected) in [
        (&source, &[Value::Number(42.)][..]),
        (&native, &[Value::Nil, Value::Nil][..]),
    ] {
        let points = run(program, 0, expected);
        assert!(points >= 12);
        for nth in 1..=points {
            run(program, nth, expected);
        }
        total += points;
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn lua_capture_failures_unwind_open_cells_and_allow_another_call() {
    fn run(program: &Program, reject_at: usize) -> usize {
        let ledger = Ledger::default();
        let endpoint = Endpoint {
            ledger: &ledger,
            tag: 1,
        };
        // SAFETY: all owners are dropped before their endpoint and ledger.
        let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
        let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
        let factory = vm.execute(program).unwrap()[0];
        let root = vm.pin(factory).unwrap();
        let start = ledger.attempts.get();
        ledger
            .reject_at
            .set(if reject_at == 0 { 0 } else { start + reject_at });
        let result = vm.call(factory, &[]);
        let points = ledger.attempts.get() - start;
        if reject_at != 0 {
            let error = result.unwrap_err();
            fixed_memory(&vm, error);
        } else {
            result.unwrap();
        }
        ledger.reject_at.set(0);
        let function = vm.call(factory, &[]).unwrap()[0];
        assert_eq!(vm.call(function, &[]).unwrap(), &[Value::Number(42.)]);
        vm.unpin(root).unwrap();
        drop(vm);
        drop(memory);
        assert!(
            ledger.blocks.borrow().is_empty(),
            "Lua capture failure {reject_at}"
        );
        points
    }
    let program = compile(
        b"return function()local a,b,c,d,e=2,4,6,10,20;return function()return a+b+c+d+e end end",
    )
    .unwrap();
    let total = run(&program, 0);
    assert!(total >= 6);
    for nth in 1..=total {
        run(&program, nth);
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn debug_name_storage_uses_the_domain_and_recovers_after_each_failed_allocation() {
    fn run(program: &Program, reject_at: usize) -> usize {
        let ledger = Ledger::default();
        let endpoint = Endpoint {
            ledger: &ledger,
            tag: 1,
        };
        // SAFETY: the endpoint remains live through VM teardown.
        let memory = unsafe { Memory::custom(endpoint.allocator()) }.unwrap();
        let mut vm = Vm::try_bare_in(memory.clone()).unwrap();
        let function = Value::Function(vm.try_load_program(program).unwrap());
        let root = vm.pin(function).unwrap();
        let inspect = |vm: &mut Vm| {
            vm.with_native_stack(|s| {
                s.push(function)?;
                let record = NativeDebugFrame::from_index(s.thread_id(), 0);
                let info = s.get_info(record, b">S")?;
                assert!(info.valid && info.source.is_some());
                Ok(())
            })
        };
        let start = ledger.attempts.get();
        ledger
            .reject_at
            .set(if reject_at == 0 { 0 } else { start + reject_at });
        let result = inspect(&mut vm);
        let points = ledger.attempts.get() - start;
        if reject_at != 0 {
            fixed_memory(&vm, result.unwrap_err());
        } else {
            result.unwrap();
        }
        ledger.reject_at.set(0);
        inspect(&mut vm).unwrap();
        vm.unpin(root).unwrap();
        drop(vm);
        drop(memory);
        assert!(
            ledger.blocks.borrow().is_empty(),
            "debug string failure {reject_at}"
        );
        points
    }
    let program = crate::compile_named(b"return 42", b"@allocator-owned-debug-source.lua").unwrap();
    let total = run(&program, 0);
    assert!(total >= 3);
    for nth in 1..=total {
        run(&program, nth);
    }
    eprintln!("CUSTOM_OPERATION_REJECTION_POINTS {total}");
}

#[test]
fn shared_compiled_program_keeps_mutable_bindings_in_independent_domains_after_replacement() {
    let a = Ledger::default();
    let b = Ledger::default();
    let first = Endpoint { ledger: &a, tag: 1 };
    let replacement = Endpoint { ledger: &a, tag: 2 };
    let independent = Endpoint { ledger: &b, tag: 1 };
    // SAFETY: all endpoints remain live; replacement accepts exactly A's heap.
    let memory_a = unsafe { Memory::custom(first.allocator()) }.unwrap();
    let memory_b = unsafe { Memory::custom(independent.allocator()) }.unwrap();
    let program =
        compile(b"local counter=0;return function(delta)counter=counter+delta;return counter end")
            .unwrap();
    let mut vm_a = Vm::try_bare_in(memory_a.clone()).unwrap();
    let mut vm_b = Vm::try_bare_in(memory_b.clone()).unwrap();
    let function_a = vm_a.execute(&program).unwrap()[0];
    let function_b = vm_b.execute(&program).unwrap()[0];
    let root_a = vm_a.pin(function_a).unwrap();
    let root_b = vm_b.pin(function_b).unwrap();
    drop(program);
    let changed = a.calls.borrow().len();
    // SAFETY: both callbacks accept all existing blocks and exact prior sizes.
    unsafe {
        memory_a.replace(replacement.allocator());
    }
    for delta in [1., 2.] {
        assert_eq!(
            vm_a.call(function_a, &[Value::Number(delta)]).unwrap(),
            &[Value::Number(if delta == 1. { 1. } else { 3. })]
        );
        assert_eq!(
            vm_b.call(function_b, &[Value::Number(delta * 10.)])
                .unwrap(),
            &[Value::Number(if delta == 1. { 10. } else { 30. })]
        );
        vm_a.collect_garbage().unwrap();
        vm_b.collect_garbage().unwrap();
    }
    vm_a.unpin(root_a).unwrap();
    vm_b.unpin(root_b).unwrap();
    drop(vm_a);
    drop(vm_b);
    drop(memory_a);
    drop(memory_b);
    assert!(
        a.calls.borrow()[changed..]
            .iter()
            .all(|&(tag, _, _)| tag == 2)
    );
    assert!(a.blocks.borrow().is_empty() && b.blocks.borrow().is_empty());
}
