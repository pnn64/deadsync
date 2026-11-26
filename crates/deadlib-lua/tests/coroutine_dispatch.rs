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
        local calls = 0
        local replacement = function() calls = calls + 1 end
        local ok, first = resume(co)
        assert(ok and first == 1)
        local ok, second = resume(co)
        assert(ok and second == 2)
        local ok, third = resume(co)
        assert(ok and third == 3)
        yield = replacement
        local ok, answer = resume(co)
        assert(ok and calls == 2 and coroutine.status(co) == 'dead')
        return answer
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn individual_resumes_sync_the_parked_numeric_loop_before_debug_calls() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = 1, 50 do yield(i) end
            return 42
        end)
        local count, sum = 0, 0
        while count < 12 do
            count = count + 1
            local ok, value = resume(co)
            assert(ok)
            sum = sum + value
        end
        assert(sum == 78)
        local control, index = debug.getlocal(co, 1, 1)
        local name, visible = debug.getlocal(co, 1, 4)
        assert(control == '(for index)' and index == 12 and name == 'i' and visible == 12)
        assert(debug.setlocal(co, 1, 1, 49) == control)
        local ok, last = resume(co)
        assert(ok and last == 50)
        local ok, result = resume(co)
        assert(ok and coroutine.status(co) == 'dead')
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn interleaved_resumes_sync_both_threads_on_loop_exit_and_fallback() {
    let program = compile(
        br#"
        local check = assert
        local yield, resume = coroutine.yield, coroutine.resume
        local function producer(first, last, step)
            return coroutine.create(function()
                for i = first, last, step do yield(i) end
                return last + step
            end)
        end
        local function consume(resume, a, b, limit)
            local count, sum = 0, 0
            while count < limit do
                count = count + 1
                local ok, x = resume(a)
                assert(ok)
                local ok, y = resume(b)
                assert(ok and x == count and y == -count)
                sum = sum + x - y
            end
            return sum
        end

        local a, b = producer(1, 23, 1), producer(-1, -40, -1)
        check(consume(resume, a, b, 23) == 552)
        local control, x = debug.getlocal(a, 1, 1)
        local name, y = debug.getlocal(b, 1, 4)
        check(control == '(for index)' and x == 23 and name == 'i' and y == -23)
        local ok, last = resume(a)
        check(ok and last == 24 and coroutine.status(a) == 'dead')
        local ok, next_y = resume(b)
        check(ok and next_y == -24)

        a, b = producer(1, 23, 1), producer(-1, -40, -1)
        local ok, message = pcall(consume, resume, a, b, 26)
        check(not ok and type(message) == 'string' and coroutine.status(a) == 'dead')
        local _, parked = debug.getlocal(b, 1, 4)
        check(parked == -24)

        a, b = producer(1, 23, 1), producer(-1, -40, -1)
        local calls = 0
        assert = function(ok)
            calls = calls + 1
            local _, current = debug.getlocal(a, 1, 4)
            check(ok and current == math.floor((calls + 1) / 2))
        end
        check(consume(resume, a, b, 23) == 552 and calls == 46)
        assert = check
        return 42
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn interleaved_resumes_publish_each_thread_before_an_assertion_error() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local function producer(first, last, step)
            return coroutine.create(function()
                for i = first, last, step do yield(i) end
            end)
        end
        local left, right = producer(1, 30, 1), producer(-1, -30, -1)
        parked_left, parked_right = left, right
        local count, sum = 0, 0
        while count < 30 do
            count = count + 1
            local ok, x = resume(left)
            assert(ok)
            local allowed = count < 18
            assert(allowed)
            local ok, y = resume(right)
            assert(ok and x == count and y == -count)
            sum = sum + x - y
        end
        return sum
    "#,
    )
    .unwrap();
    let inspect = compile(
        b"local _, x = debug.getlocal(parked_left, 1, 4)
          local _, y = debug.getlocal(parked_right, 1, 4)
          return x, y",
    )
    .unwrap();
    let run = |limit| {
        let mut vm = Vm::new();
        vm.set_instruction_limit(limit);
        let error = vm.execute(&program).unwrap_err();
        assert_eq!(
            vm.execute(&inspect).unwrap(),
            [Value::Number(18.0), Value::Number(-17.0)]
        );
        (error.kind(), error.line(), error.message_bytes().to_vec())
    };
    assert_eq!(run(None), run(Some(10_000)));
}

#[test]
fn repeating_payload_resumes_update_the_frame_at_a_different_yield_site() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = 1, 20 do yield(i * 3 + 1, -i) end
            yield('last', 'payload')
            return 42
        end)
        local count, sum = 0, 0
        while count < 20 do
            count = count + 1
            local ok, first, second = resume(co)
            assert(ok)
            sum = sum + first + second
        end
        local ok, first, second = resume(co)
        assert(sum == 440 and ok and first == 'last' and second == 'payload')
        assert(debug.getinfo(co, 0, 'f').func == yield)
        local ok, result = resume(co)
        assert(ok and coroutine.status(co) == 'dead')
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn repeated_payload_consumer_falls_back_for_a_replaced_assertion() {
    let program = compile(
        br#"
        local check = assert
        local yield, resume = coroutine.yield, coroutine.resume
        local function producer()
            for i = 1, 17 do yield(41 - i, (i + 12) / 2) end
            yield("9", "4")
            return 42
        end
        local function consume(co, limit)
            local count, sum = 0, 0
            while count < limit do
                count = count + 1
                local ok, first, second = resume(co)
                assert(ok)
                sum = sum + first - second
            end
            return sum
        end
        local co = coroutine.create(producer)
        check(consume(co, 18) == 370.5)
        local ok, result = resume(co)
        check(ok and result == 42 and coroutine.status(co) == "dead")

        local observed = coroutine.create(producer)
        local calls = 0
        assert = function(ok)
            calls = calls + 1
            local _, index = debug.getlocal(observed, 1, 1)
            check(ok and index == calls)
        end
        check(consume(observed, 17) == 365.5 and calls == 17)
        assert = check
        local ok, first, second = resume(observed)
        check(ok and first == "9" and second == "4")
        local finished, result = resume(observed)
        check(finished and coroutine.status(observed) == "dead")
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn linked_resume_additions_preserve_rounding_and_the_next_yield() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local function producer()
            for i = 1, 120 do yield(1e16, -1e16, i / 8) end
            yield('9', '4', '1')
            return 42
        end
        local function consume(co)
            local count, sum = 0, 0
            while count < 121 do
                count = count + 1
                local ok, a, b, c = resume(co)
                assert(ok)
                sum = sum + a + b + c
            end
            return sum
        end
        local fast = coroutine.create(producer)
        local sum = consume(fast)
        assert(debug.getinfo(fast, 0, 'f').func == yield)
        local ok, result = resume(fast)
        assert(ok and result == 42 and coroutine.status(fast) == 'dead')

        local hooked = coroutine.create(producer)
        local calls = 0
        debug.sethook(function() calls = calls + 1 end, '', 1)
        local reference = consume(hooked)
        debug.sethook()
        assert(sum == reference and calls > 121)
        local ok, result = resume(hooked)
        assert(ok and coroutine.status(hooked) == 'dead')
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(50_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn numeric_payload_preserves_signed_zero_and_rechecks_the_captured_yield() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = -2, 3 do yield(i * 3 + 1, -i) end
            return 42
        end)
        local calls = 0
        local replacement = function(first, second)
            assert(first == (calls + 1) * 3 + 1 and second == -(calls + 1))
            calls = calls + 1
        end
        local i, sum = -2, 0
        while i < 1 do
            local ok, first, second = resume(co)
            assert(ok and first == i * 3 + 1 and second == -i)
            if i == 0 then assert(1 / second == -math.huge) end
            sum = sum + first + second
            i = i + 1
        end
        assert(sum == -3)
        yield = replacement
        local ok, result = resume(co)
        assert(ok and calls == 3 and coroutine.status(co) == 'dead')
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn numeric_payload_preserves_operand_order_across_three_values() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = -2, 4 do yield(9 - i, (i + 3) / 2, 8 / i) end
            return 42
        end)
        local i = -2
        while i <= 4 do
            local ok, a, b, c = resume(co)
            assert(ok and a == 9 - i and b == (i + 3) / 2 and c == 8 / i)
            i = i + 1
        end
        local ok, result = resume(co)
        assert(ok and coroutine.status(co) == 'dead')
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn projected_numeric_payload_syncs_before_debug_reads_and_writes() {
    let program = compile(
        br#"
        local yield, resume = coroutine.yield, coroutine.resume
        local co = coroutine.create(function()
            for i = 1, 64 do yield(i * 3 + 1, -i, (i + 12) / 2) end
            return 42
        end)
        local count, total = 0, 0
        while count < 12 do
            count = count + 1
            local ok, a, b, c = resume(co)
            assert(ok)
            total = total + a + b + c
        end
        assert(total == 279)
        local control, index = debug.getlocal(co, 1, 1)
        local name, visible = debug.getlocal(co, 1, 4)
        assert(control == '(for index)' and index == 12 and name == 'i' and visible == 12)
        assert(debug.setlocal(co, 1, 1, 63) == control)
        local ok, a, b, c = resume(co)
        assert(ok and a == 193 and b == -64 and c == 38)
        local ok, result = resume(co)
        assert(ok and coroutine.status(co) == 'dead')
        return result
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(10_000));
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
