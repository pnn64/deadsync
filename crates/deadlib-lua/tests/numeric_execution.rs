use deadlib_lua::{Value, Vm, compile};

fn check(source: &[u8]) {
    check_with_limit(source, 100_000);
}

fn check_with_limit(source: &[u8], limit: u64) {
    let program = compile(source).unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
    vm.set_instruction_limit(Some(limit));
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
}

#[test]
fn numeric_sums_preserve_loop_carried_values_and_rounding() {
    check(
        br#"
        local sum = 0
        local offset = 3
        for i = 2, 200, 3 do sum = sum + (i + offset) * 2 end
        assert(sum == 13936)
        local negative = 0
        for i = -2, -200, -3 do negative = negative + i - 1 end
        assert(negative == -6834)

        local doubled = 0
        for i = 1, 8 do doubled = doubled + (doubled + i) end
        assert(doubled == 502)

        local large = 9007199254740992
        local rounded = 0
        for i = 1, 4 do rounded = rounded + ((i + large) - large) end
        assert(rounded == 10)

        local fractional = 0
        for i = 1, 8 do fractional = fractional + (i + 0.25) end
        return fractional == 38
        "#,
    );
}

#[test]
fn numeric_leaf_calls_preserve_results_and_observable_fallbacks() {
    check(
        br#"
        local function add(x, y) return x + y end
        local function scale(x) return x * 2 end
        local function negate(x) return -x end
        local sum = 0
        for i = 1, 80 do sum = sum + add(i, 3) + scale(i) end
        assert(sum == 9960)
        local number, absent = add(16, 26)
        assert(number == 42 and absent == nil)
        local negative_zero = negate(0)
        assert(1 / negative_zero == -math.huge)
        assert(add('40', 2) == 42)
        local good, bad = pcall(add, 1)
        assert(not good and type(bad) == 'string')

        local calls, returns = 0, 0
        debug.sethook(function(event)
            if debug.getinfo(2, 'f').func == add then
                if event == 'call' then calls = calls + 1 end
                if event == 'return' then returns = returns + 1 end
            end
        end, 'cr')
        local hooked = 0
        for i = 1, 21 do hooked = hooked + add(i, 2) end
        debug.sethook()
        assert(hooked == 273 and calls == 21 and returns == 21)

        local special = setmetatable({}, {__add = function(_, y)
            assert(debug.getinfo(2, 'f').func == add)
            return y + 1
        end})
        return add(special, 41) == 42
        "#,
    );
}

#[test]
fn two_result_numeric_leaf_preserves_discarded_operations_and_hooks() {
    check(
        br#"
        local function pair(a, b) return a + 1, -b end
        local first, second, absent = pair(41, 2)
        assert(first == 42 and second == -2 and absent == nil)
        first, second = pair(41, 0)
        assert(first == 42 and 1 / second == -math.huge)

        local negations = 0
        local special = setmetatable({}, {__unm = function()
            assert(debug.getinfo(2, 'f').func == pair)
            negations = negations + 1
            return 10
        end})
        first = pair(41, special)
        assert(first == 42 and negations == 1)
        assert(not pcall(function() local discarded = pair(41) end))

        local calls, returns = 0, 0
        debug.sethook(function(event)
            if debug.getinfo(2, 'f').func == pair then
                if event == 'call' then calls = calls + 1 end
                if event == 'return' then returns = returns + 1 end
            end
        end, 'cr')
        first, second = pair(41, -21)
        debug.sethook()
        return first == 42 and second == 21 and calls == 1 and returns == 1
        "#,
    );
}

#[test]
fn two_result_numeric_reduction_preserves_both_values_and_rounding() {
    check(
        br#"
        local function offsets(a, b) return a + 1, b + 2 end
        local total = 0
        for i = 1, 120 do
            local a, b = offsets(i, 6)
            total = total + a + b
        end
        assert(total == 8340)

        local function identity(a, b) return a, b end
        total = 0
        for i = 120, 1, -1 do
            local a, b = identity(i, 2 * i)
            total = total + a + b
        end
        assert(total == 21780)

        local large = 9007199254740992
        total = 0
        for i = 1, 4 do
            local a, b = offsets(large, i)
            total = total + (a - large) + b
        end
        assert(total == 18)
        return true
        "#,
    );
}

#[test]
fn forwarded_calls_follow_upvalue_changes_and_pad_results() {
    check(
        br#"
        local target = function(first, ...) return ..., first end
        local function forward(...) return target("prefix", ...) end
        for i = 1, 24 do
            if i == 12 then
                target = function(first, ...) return first, ... end
            end
            local a, b, c, d = forward(i, nil, "suffix")
            if i < 12 then
                assert(a == i and b == "prefix" and c == nil and d == nil)
            else
                assert(a == "prefix" and b == i and c == nil and d == "suffix")
            end
        end
        local a, b = forward()
        assert(a == "prefix" and b == nil)
        assert(select('#', forward(nil, nil)) == 3)

        local calls = 0
        debug.sethook(function()
            if debug.getinfo(2, "f").func == forward then calls = calls + 1 end
        end, "c")
        a, b = forward(42)
        debug.sethook()
        return a == "prefix" and b == 42 and calls == 1
        "#,
    );
}

#[test]
fn forwarded_numeric_loops_check_nil_guards_and_live_targets() {
    check(
        br#"
        local target = function(first, ...) return ..., first end
        local function forward(...) return target(8, ...) end
        local total = 0
        for i = 1, 120 do
            local a, b, c = forward(i, 6, 7)
            if c ~= nil then error("extra result") end
            total = total + a + b
        end
        assert(total == 8220)

        target = function(first, ...) return first, ... end
        total = 0
        local extra = 0
        for i = 120, 1, -1 do
            local a, b, c = forward(i, 6)
            if c ~= nil then extra = extra + c end
            total = total + a + b
        end
        assert(total == 8220 and extra == 720)

        target = function(first, ...) return ..., first end
        local replacement = function(first, ...) return ..., 17 end
        total = 0
        for i = 1, 120 do
            target = replacement
            local a, b, c = forward(i)
            if c ~= nil then error("extra result") end
            total = total + a + b
        end
        return total == 9300
        "#,
    );
}

#[test]
fn dense_array_calls_preserve_callees_rounding_and_loop_carried_arguments() {
    check(
        br#"
        local plus = function(x, y) return x + y end
        local multi = function(x, y) return 2 * x - y end
        local functions, x, y = {}, {}, {}
        for i = 1, 32 do
            functions[i] = i % 2 == 0 and plus or multi
            x[i], y[i] = i, 0.25
        end
        local result = 9007199254740992
        local expected = result
        for i = 32, 1, -1 do
            local term = i % 2 == 0 and i + 0.25 or 2 * i - 0.25
            expected = expected + term
        end
        for i = 32, 1, -1 do result = result + functions[i](x[i], y[i]) end
        assert(result == expected)

        local visits = 0
        functions[7] = function(a, b)
            visits = visits + 1
            return a + b
        end
        result = 0
        for i = 1, 8 do result = result + functions[i](x[i], result) end
        assert(visits == 1)
        functions[7] = multi
        expected = 0
        for i = 1, 8 do expected = expected + (i % 2 == 0 and i + expected or 2 * i - expected) end
        result = 0
        for i = 1, 8 do result = result + functions[i](x[i], result) end
        return result == expected
        "#,
    );
}

#[test]
fn dense_array_call_cache_rechecks_arguments_and_callees_after_writes() {
    check(
        br#"
        local function setup()
            local functions, x, y = {}, {}, {}
            local plus = function(a, b) return a + b end
            local minus = function(a, b) return a - b end
            local min = math.min
            for i = 1, 96 do
                functions[i] = i % 2 == 0 and minus or plus
                x[i], y[i] = i, 0.5
            end
            return function(total)
                for i = 1, 96 do total = total + functions[i](x[i], y[i]) end
                return total
            end, function()
                local total = 0
                for i = 1, 96 do total = total + min(x[i], 1000) end
                return total
            end, x, functions
        end
        local sum, clamp, x, functions = setup()
        assert(sum(0) == 4656 and sum(0) == 4656)
        x[9] = 33
        assert(clamp() == 4680 and clamp() == 4680)
        assert(sum(0) == 4680 and sum(0) == 4680)
        functions[1] = function(a, b) return 2 * a + b end
        assert(sum(0) == 4681 and sum(0) == 4681)

        local large = 9007199254740992
        local expected = large
        for i = 1, 96 do
            local term = x[i] + (i % 2 == 0 and -0.5 or 0.5)
            if i == 1 then term = term + 1 end
            expected = expected + term
        end
        return sum(large) == expected
        "#,
    );
}

#[test]
fn dense_array_pair_calls_preserve_order_and_recheck_cached_inputs() {
    check(
        br#"
        local function setup()
            local first, second = {}, {}
            local adjust = function(a, b) return a + 1, b + 2 end
            for i = 1, 96 do first[i], second[i] = i, i * 2 end
            return function(total)
                for i = 96, 1, -1 do
                    local a, b = adjust(first[i], second[i])
                    total = total + b + a
                end
                return total
            end, function(f) adjust = f end, first, second
        end
        local sum, replace, first, second = setup()
        local function expected(total, left, right)
            for i = 96, 1, -1 do
                total = total + (second[i] + right) + (first[i] + left)
            end
            return total
        end
        assert(sum(0) == 14256 and sum(0) == 14256)
        assert(sum(9007199254740992) == expected(9007199254740992, 1, 2))
        first[31], second[77] = 106, 171
        assert(sum(0) == expected(0, 1, 2) and sum(0) == 14348)

        replace(function(a, b) return a + 4, b - 3 end)
        assert(sum(0) == expected(0, 4, -3) and sum(0) == 14156)
        local visits = 0
        replace(function(a, b)
            visits = visits + 1
            return a + 4, b - 3
        end)
        assert(sum(0) == 14156 and visits == 96)

        replace(function(a, b) return a + 1, b + 2 end)
        first[31] = nil
        setmetatable(first, {__index = function(_, i) return i end})
        return sum(0) == 14273
        "#,
    );
}

#[test]
fn two_result_calls_recheck_changed_callee_array_slots() {
    check(
        br#"
        local function setup()
            local targets, first, second = {}, {}, {}
            local identity = function(a, b) return a, b end
            for i = 1, 96 do targets[i], first[i], second[i] = identity, i, i * 2 end
            return function()
                local total = 0
                for i = 1, 96 do
                    local a, b = targets[i](first[i], second[i])
                    total = total + a + b
                end
                return total
            end, targets
        end
        local sum, targets = setup()
        assert(sum() == 13968 and sum() == 13968)
        targets[9] = function(a, b) return b + 3, a - 2 end
        return sum() == 13969 and sum() == 13969
        "#,
    );
}

#[test]
fn paired_array_sums_repair_changed_blocks_and_preserve_rounding() {
    check_with_limit(
        br#"
        local function setup()
            local first, second = {}, {}
            local adjust = function(a, b) return a + 1, b + 2 end
            for i = 1, 8192 do first[i], second[i] = i % 37, i % 29 end
            local function sum(total, lower, upper, step)
                for i = lower, upper, step do
                    local a, b = adjust(first[i], second[i])
                    total = total + a + b
                end
                return total
            end
            local function check_sum(total, lower, upper, step)
                local expected = total
                for i = lower, upper, step do
                    expected = expected + (first[i] + 1) + (second[i] + 2)
                end
                assert(sum(total, lower, upper, step) == expected)
            end
            return check_sum, first, second
        end
        local check_sum, first, second = setup()
        check_sum(0, 34, 8192, 1)
        first[34], second[199] = 53, 61
        check_sum(0, 34, 8192, 1)
        first[96], second[1] = 23, 29
        check_sum(0, 34, 8192, 1)
        first[35], first[101] = 43, 83
        check_sum(0, 34, 8192, 1)
        first[8150] = 15
        check_sum(0, 34, 8192, 1)

        check_sum(0, 8192, 34, -1)
        first[34], second[97] = 107, 127
        check_sum(0, 8192, 34, -1)
        second[96] = 9007199254740991
        check_sum(0, 8192, 34, -1)
        second[96] = 5
        check_sum(9007199254740992, 8192, 34, -1)
        return true
        "#,
        4_000_000,
    );
}
