use deadlib_lua::{Value, Vm, compile};

#[test]
fn resume_preserves_the_parked_yield_frame_and_deoptimizes_at_return() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = 1, 30 do yield(i, -i) end
            return 42
        end)
        for i = 1, 30 do
            local ok, a, b = resume(co)
            assert(ok and a == i and b == -i)
            assert(coroutine.status(co) == 'suspended')
            assert(debug.getinfo(co, 0, 'f').func == yield)
        end
        local ok, answer = resume(co)
        assert(ok and coroutine.status(co) == 'dead')
        return answer
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn resume_falls_back_when_the_captured_yield_is_replaced() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = 1, 5 do yield(i) end
            return 42
        end)
        local ok, first = resume(co)
        assert(ok and first == 1)
        local ok, second = resume(co)
        assert(ok and second == 2)
        local calls = 0
        yield = function() calls = calls + 1 end
        local ok, answer = resume(co)
        assert(ok and calls == 3 and coroutine.status(co) == 'dead')
        return answer
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn batching_an_exact_sum_preserves_the_next_yield() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = 98, 0, -2 do yield(i) end
        end)
        local sum = 0
        for i = 1, 30 do
            local ok, n = resume(co)
            assert(ok)
            sum = sum + n
        end
        assert(sum == 2070 and coroutine.status(co) == 'suspended')
        assert(debug.getinfo(co, 0, 'f').func == yield)
        local ok, next_value = resume(co)
        assert(ok and next_value == 38)
        return 42
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}
