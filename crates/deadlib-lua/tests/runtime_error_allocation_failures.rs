use deadlib_lua::{ErrorKind, Value, Vm, compile_named};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{begin, reject_from_now, statistics};
use std::cell::Cell;

thread_local! {
    static DELIVERY_START: Cell<usize> = const { Cell::new(0) };
}

#[derive(Debug)]
enum Outcome {
    Error(ErrorKind, Option<Value>),
    Returned(Value, Value),
}

fn execute(vm: &mut Vm, program: &deadlib_lua::Program) -> Outcome {
    match vm.execute(program) {
        Ok(values) => Outcome::Returned(
            values.first().copied().unwrap_or(Value::Nil),
            values.get(1).copied().unwrap_or(Value::Nil),
        ),
        Err(error) => Outcome::Error(error.kind(), error.value()),
    }
}

fn run_case(source: &[u8], protected: bool, nth: usize) -> usize {
    let guard = begin(0);
    let attempts;
    {
        let program = compile_named(source, b"@runtime-errors\xff.lua").unwrap();
        let retry = compile_named(b"return 42", b"=retry").unwrap();
        let mut vm = Vm::try_new().unwrap();
        let callback = vm
            .new_host_function(|call| Err(call.error(b"host failure\xff")), &[])
            .unwrap();
        vm.set_global(b"hostfail", callback).unwrap();
        vm.try_load_program(&program).unwrap();
        vm.try_load_program(&retry).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let outcome = execute(&mut vm, &program);
        attempts = statistics().attempts - start;
        reject_from_now(0);
        if nth == 0 {
            match outcome {
                Outcome::Error(ErrorKind::Runtime, _) => assert!(!protected),
                Outcome::Returned(Value::Boolean(false), Value::String(_)) => assert!(protected),
                _ => panic!("unexpected healthy outcome: {outcome:?}"),
            }
        } else {
            assert!(statistics().rejected > 0);
            let value = match outcome {
                Outcome::Error(ErrorKind::Memory, Some(value)) => value,
                Outcome::Returned(Value::Boolean(false), value) if protected => value,
                _ => panic!("unexpected rejection outcome: {outcome:?}"),
            };
            let Value::String(id) = value else {
                panic!("memory diagnostic must be a string")
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
        }
        assert_eq!(vm.execute(&retry).unwrap(), &[Value::Number(42.)]);
    }
    let stats = statistics();
    drop(guard);
    assert_eq!(
        stats.allocated_bytes, stats.freed_bytes,
        "rejection {nth} leaked runtime/error state"
    );
    attempts
}

#[test]
fn runtime_diagnostics_and_protected_delivery_recover_at_every_allocation() {
    let cases: &[(&[u8],bool)]=&[
        (b"local x=nil; return x+1",false),
        (b"return missing()",false),
        (b"local t={}; return t.missing[1]",false),
        (b"local x={}; return x .. 'text'",false),
        (b"local x=nil; local function f() return x+1 end; return f()",false),
        (b"return math.abs(nil)",false),
        (b"return string.byte({},1)",false),
        (b"return pcall(function() local x=nil; return x+1 end)",true),
        (b"return pcall(math.abs,nil)",true),
        (b"return xpcall(function() local x=nil; return x+1 end,function(e) return e end)",true),
        (b"assert(false)",false),
        (b"assert(false,42)",false),
        (b"assert(false,'binary\\255\\0tail')",false),
        (b"error('binary\\255\\0tail')",false),
        (b"error('message',0)",false),
        (b"return pcall(error,'message')",true),
        (b"return pcall(assert,false,'message')",true),
        (b"return pcall(function() error('binary\\255') end)",true),
        (b"local co=coroutine.create(function() local x=nil; return x+1 end); return coroutine.resume(co)",true),
        (b"local co=coroutine.create(function() error('thread message') end); return coroutine.resume(co)",true),
        (b"local f=coroutine.wrap(function() error('wrapped message') end); return pcall(f)",true),
        (b"return hostfail()",false),
        (b"return pcall(hostfail)",true),
        (b"local co=coroutine.create(hostfail); return coroutine.resume(co)",false),
        (b"local co=coroutine.create(function() return hostfail() end); return coroutine.resume(co)",true),
        (b"return pcall(coroutine.yield,1)",true),
        (b"local co=coroutine.create(function() end); coroutine.resume(co); return coroutine.resume(co,1,2,3)",true),
        (b"local f=coroutine.wrap(function() end); f(); return pcall(f)",true),
    ];
    // Warm process-wide stdio/locale setup before accounting owned engine state.
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let mut rejected = 0;
    for (case, &(source, protected)) in cases.iter().enumerate() {
        let count = run_case(source, protected, 0);
        for nth in 1..=count {
            eprintln!("runtime error allocation case={case} nth={nth}");
            run_case(source, protected, nth);
            rejected += 1;
        }
    }
    eprintln!("runtime errors recovered {rejected} allocation failures");
}

fn child_error_case(nth: usize) -> usize {
    let guard = begin(0);
    let attempts;
    {
        let program = compile_named(
            br#"
                child = coroutine.create(function()
                    local n = 40
                    retained = function() n = n + 1; return n end
                    arm()
                    local missing = nil
                    return missing + 1
                end)
                return coroutine.resume(child)
            "#,
            b"=child-error",
        )
        .unwrap();
        let inspect = compile_named(
            br#"return coroutine.status(child) == 'dead', retained(), retained()"#,
            b"=inspect",
        )
        .unwrap();
        let mut vm = Vm::try_new().unwrap();
        let arm = vm
            .new_host_function(
                |call| {
                    let Value::Number(nth) = call.captures()[0] else {
                        unreachable!()
                    };
                    DELIVERY_START.set(statistics().attempts);
                    reject_from_now(nth as usize);
                    Ok(())
                },
                &[Value::Number(nth as f64)],
            )
            .unwrap();
        vm.set_global(b"arm", arm).unwrap();
        vm.try_load_program(&program).unwrap();
        vm.try_load_program(&inspect).unwrap();
        let outcome = execute(&mut vm, &program);
        attempts = statistics().attempts - DELIVERY_START.get();
        reject_from_now(0);
        if nth == 0 {
            assert!(matches!(
                outcome,
                Outcome::Returned(Value::Boolean(false), Value::String(_))
            ));
        } else {
            assert!(statistics().rejected > 0);
            let value = match outcome {
                Outcome::Error(ErrorKind::Memory, Some(value))
                | Outcome::Returned(Value::Boolean(false), value) => value,
                _ => panic!("unexpected child failure outcome: {outcome:?}"),
            };
            let Value::String(id) = value else {
                panic!("memory error must be a string")
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
        }
        vm.collect_garbage().unwrap();
        assert_eq!(
            vm.execute(&inspect).unwrap(),
            &[Value::Boolean(true), Value::Number(41.), Value::Number(42.)]
        );
    }
    let stats = statistics();
    drop(guard);
    assert_eq!(stats.allocated_bytes, stats.freed_bytes);
    attempts
}

#[test]
fn rejected_child_error_delivery_preserves_retained_upvalues() {
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let count = child_error_case(0);
    assert!(count > 0);
    for nth in 1..=count {
        eprintln!("child error delivery rejection={nth}");
        child_error_case(nth);
    }
    eprintln!("child error delivery recovered {count} allocation failures");
}

#[test]
fn coroutine_argument_return_and_yield_growth_remain_recoverable() {
    allocation_probe::probe(0, || drop(Vm::try_new().unwrap()));
    let values = (1..=100)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let cases = [
        format!(
            "local co=coroutine.create(function(...) return ... end); return pcall(function() local ok,e=coroutine.resume(co,{values}); if not ok then error(e,0) end; error('after return',0) end)"
        ),
        format!(
            "local co=coroutine.create(function() return {values} end); return pcall(function() local ok,e=coroutine.resume(co); if not ok then error(e,0) end; error('after return',0) end)"
        ),
        format!(
            "local co=coroutine.create(function() coroutine.yield({values}); return 1 end); return pcall(function() local ok,e=coroutine.resume(co); if not ok then error(e,0) end; local ok,e=coroutine.resume(co,{values}); if not ok then error(e,0) end; error('after resume',0) end)"
        ),
    ];
    let mut total = 0;
    for (index, source) in cases.iter().enumerate() {
        let count = run_case(source.as_bytes(), true, 0);
        for nth in 1..=count {
            eprintln!("coroutine transfer case={index} rejection={nth}");
            run_case(source.as_bytes(), true, nth);
        }
        total += count;
    }
    eprintln!("coroutine transfers recovered {total} allocation failures");
}
