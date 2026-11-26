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
fn dense_growth_preserves_native_rehash_shape_and_scalar_values() {
    let program = compile(
        br#"
        local order = 0
        local function fill(array, first, last, value)
            for i = first, last do array[i] = value end
        end
        for _, n in ipairs({2, 3, 5, 17, 32, 33, 1023}) do
            local array = {}
            fill(array, 1, n, -1 / math.huge)
            assert(#array == n and 1/array[1] == -math.huge and 1/array[n] == -math.huge)
            fill(array, n+1, n+4, false)
            array[2*n+7] = 1
            array.alpha, array.beta = 1, 2
            for key in pairs(array) do
                local code = type(key) == 'number' and key or (key == 'alpha' and 3 or 5)
                order = (order * 31 + code) % 1000000007
            end
            assert(array[n+1] == false and array[n+4] == false)
        end
        local sparse = {}
        sparse[2] = 42
        fill(sparse, 3, 64, true)
        assert(sparse[1] == nil and sparse[2] == 42 and sparse[64] == true)
        return order, #sparse
        "#,
    )
    .unwrap();
    for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
        let optimized = vm.execute(&program).unwrap().to_vec();
        vm.set_instruction_limit(Some(500_000));
        assert_eq!(vm.execute(&program).unwrap(), optimized);
    }
}

#[test]
fn temporary_dense_fills_preserve_reads_and_escaping_tables() {
    let program = compile(
        br#"
        local function summarize(n, value)
            local score, last
            do
                local array = {}
                for i = 1, n do array[i] = value end
                score, last = #array + array[3], array[n]
            end
            return score, last
        end
        local function retain(n, value)
            local array = {}
            for i = 1, n do array[i] = value end
            return #array + array[3], array
        end
        local score, last = summarize(103, 7.25)
        assert(score == 110.25 and last == 7.25)
        score, last = summarize(97, -1/math.huge)
        assert(score == 97 and 1/last == -math.huge)
        local _, first = retain(103, 7.25)
        local _, second = retain(103, 7.25)
        first[103] = 59
        assert(#first == 103 and first[103] == 59 and second[103] == 7.25)

        local array = {}
        local alias = array
        for i = 1, 103 do array[i] = 42 end
        return #array == 103 and #alias == 103 and alias[103] == 42
        "#,
    )
    .unwrap();
    for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
        vm.set_instruction_limit(Some(100_000));
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
    }
}

#[test]
fn inline_array_dispatch_rechecks_identities_and_preserves_lua_equality() {
    let program = compile(
        br#"
        local function setup()
            local first, second, third = function() end, function() end, function() end
            local choices = {first, second, third}
            local entries, left, right = {}, {}, {}
            for i = 1, 192 do
                entries[i] = choices[(i-1)%3 + 1]
                left[i], right[i] = i + 0.25, i%5
            end
            local function tally(start, stop, step, rounds, sum)
                for r = 1, rounds do
                    for i = start, stop, step do
                        local f, x, y = entries[i], left[i], right[i]
                        if f == choices[1] then sum = sum + (x+y)
                        elseif f == choices[2] then sum = sum + (x-y)
                        else sum = sum + (2*x+y) end
                    end
                end
                return sum
            end
            return tally, entries, choices
        end
        local function equality_pair()
            local hits = 0
            local meta = {__eq = function() hits = hits+1; return true end}
            return setmetatable({}, meta), setmetatable({}, meta), function() return hits end
        end

        local tally, entries, choices = setup()
        local before = tally(1, 192, 1, 4, 0)
        assert(tally(192, 1, -1, 4, 0) == before)
        assert(tally(1, 192, 1, 4, 0) == before)
        entries[37] = choices[2]
        local changed = tally(1, 192, 1, 4, 0)
        assert(changed == before - 16)
        entries[68] = choices[3]
        local changed_again = tally(1, 192, 1, 4, 0)
        assert(changed_again == changed + 297)
        choices[1] = choices[3]
        local rebound = tally(1, 192, 1, 4, 0)
        assert(rebound ~= changed_again)
        local rounded = tally(1, 192, 1, 4, 9007199254740992)

        local one, two, hits = equality_pair()
        entries[1], choices[1] = one, two
        local with_equality = tally(1, 192, 1, 3, 0)
        assert(hits() == 3)
        return before, changed, changed_again, rebound, rounded, with_equality
        "#,
    )
    .unwrap();
    for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
        let optimized = vm.execute(&program).unwrap().to_vec();
        vm.set_instruction_limit(Some(500_000));
        assert_eq!(vm.execute(&program).unwrap(), optimized);
    }
}

#[test]
fn inline_dispatch_does_not_reuse_blocks_for_data_dependent_keys() {
    check(
        br#"
        local function setup()
            local keys, values = {}, {}
            for i = 1, 128 do keys[i], values[i] = 96, 1 end
            return function()
                local total = 0
                for round = 1, 2 do
                    for i = 1, 128 do
                        local v = values[keys[i]]
                        if v == 0 then total = total + 2
                        else total = total + v end
                    end
                end
                return total
            end, values
        end
        local tally, values = setup()
        assert(tally() == 256)
        values[96] = 3
        return tally() == 768
        "#,
    );
}

#[test]
fn dense_overwrites_preserve_native_array_extent_and_order() {
    let program = compile(
        br#"
        local array = {false, nil, false, nil, true, true, nil, nil}
        for i = 2, 7 do array[i] = i end
        for i = 4, 6 do array[i] = false end
        local keys = {}
        for key in pairs(array) do keys[#keys + 1] = key end
        assert(table.concat(keys, ',') == '1,2,3,4,5,6,7')
        assert(array[1] == false and array[2] == 2 and array[3] == 3)
        assert(array[4] == false and array[5] == false and array[6] == false)
        assert(array[7] == 7 and array[8] == nil)

        -- Writing beyond the reserved array can move keys between parts.
        for i = 8, 12 do array[i] = i end
        local order = 0
        for key in pairs(array) do order = order * 13 + key end
        return order, #array
        "#,
    )
    .unwrap();
    let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
    let actual = vm.execute(&program).unwrap().to_vec();
    vm.set_instruction_limit(Some(100_000));
    assert_eq!(vm.execute(&program).unwrap(), actual);
    assert_eq!(actual[1], Value::Number(12.0));
}

#[test]
fn copied_scalar_list_prefixes_keep_later_fields_and_tables_independent() {
    let fields = (1..=100)
        .map(|index| if index % 2 == 0 { "false" } else { "17" })
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"
        local function last(value) return value, true end
        local function make(value)
            return {{ {fields}, last(value) }}
        end
        local first, second = make(11), make(22)
        first[51] = 5
        for i = 1, 100 do
            local expected = i % 2 ~= 0 and 17 or false
            assert(second[i] == expected)
            assert(first[i] == (i == 51 and 5 or expected))
        end
        return #first == 102 and #second == 102
            and first[101] == 11 and second[101] == 22 and second[102] == true
        "#,
    );
    let program = compile(source.as_bytes()).unwrap();
    for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
        vm.set_instruction_limit(Some(100_000));
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
    }
}

#[test]
fn fully_overwritten_literals_preserve_shape_and_copy_on_write() {
    let fields = (1..=128)
        .map(|index| if index % 2 == 0 { "false" } else { "17" })
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"
        local function forward()
            local array = {{ {fields} }}
            for i = 1, 128 do array[i] = i end
            return array
        end
        local function reverse()
            local array = {{ {fields} }}
            for i = 128, 1, -1 do array[i] = i end
            return array
        end
        local first, second, third = forward(), forward(), reverse()
        local expected = 1
        for key, value in pairs(third) do
            assert(key == expected and value == expected)
            expected = expected + 1
        end
        assert(expected == 129)
        local object = {{ value = 42 }}
        rawset(first, 1, object)
        rawset(first, 128, nil)
        table.insert(first, 17)
        collectgarbage('collect')
        assert(first[1] == object and first[1].value == 42 and first[128] == 17)
        assert(second[1] == 1 and second[128] == 128)
        return #first == 128 and #second == 128 and #third == 128
        "#,
    );
    let program = compile(source.as_bytes()).unwrap();
    for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
        vm.set_instruction_limit(Some(100_000));
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
    }
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

#[test]
fn dense_substring_stores_preserve_bytes_and_observable_fallbacks() {
    let program = compile(
        br#"
        local function setup(source)
            local sub, slices = string.sub, {}
            local function fill(lower, upper)
                for i = lower, upper do
                    local first = 6000 - 3 * i
                    slices[i] = sub(source, first, first + i)
                end
            end
            return fill, slices, function(replacement) sub = replacement end
        end
        local parts = {}
        for i = 1, 1100 do parts[i] = string.format("%04d\000\255", i) end
        local source = table.concat(parts)
        local fill, slices, replace = setup(source)
        parts = nil
        collectgarbage("collect")
        fill(1, 650)
        fill(300, 800)
        collectgarbage("collect")
        assert(#slices == 800)
        for i = 1, 800 do
            local first = 6000 - 3 * i
            assert(slices[i] == string.sub(source, first, first + i))
        end
        local calls = 0
        replace(function() calls = calls + 1; return "replacement" end)
        fill(1, 50)
        assert(calls == 50 and slices[50] == "replacement")
        replace(string.sub)
        local keys = 0
        setmetatable(slices, {__newindex = function(_, key, value)
            local first = 6000 - 3 * key
            assert(value == string.sub(source, first, first + key))
            keys = keys + key
        end})
        fill(801, 810)
        return calls == 50 and keys == 8055 and #slices == 800
        "#,
    )
    .unwrap();
    for mut vm in [Vm::new(), Vm::try_new_with_lua51_table_order().unwrap()] {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
        vm.set_instruction_limit(Some(500_000));
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
    }
}

#[test]
fn substring_length_reduction_preserves_bytes_and_rebound_calls() {
    check(
        br#"
        local function setup()
            local source = string.rep("ab\000\255", 64)
            local sub = string.sub
            return function(count)
                local total, last = 0
                for i = 1, count do
                    local first = (i - 1) * 4 + 1
                    local piece = sub(source, first, first + i - 1)
                    total = total + #piece
                    last = piece
                end
                return total, last
            end, function(replacement) sub = replacement end
        end
        local sum, replace = setup()
        local total, last = sum(8)
        assert(total == 36 and last == "ab\000\255ab\000\255")
        replace(function(_, start, last) return string.rep("x", last) end)
        total, last = sum(8)
        return total == 148 and last == string.rep("x", 36)
        "#,
    );
}

#[test]
fn substring_lengths_follow_clipping_relative_indices_and_default_ends() {
    check(
        br#"
        local function setup()
            local source = "abcdef"
            local sub = string.sub
            return function()
                local clipped, omitted, explicit = 0, 0, 0
                for i = 1, 8 do
                    local piece = sub(source, i - 10, i + 2)
                    clipped = clipped + #piece
                end
                for i = 1, 6 do
                    omitted = omitted + #sub(source, i)
                end
                for i = 6, 1, -1 do
                    explicit = explicit + #sub(source, -i, nil)
                end
                return clipped, omitted, explicit
            end
        end
        local sum = setup()
        local clipped, omitted, explicit = sum()
        return clipped == 32 and omitted == 21 and explicit == 21
        "#,
    );
}

#[test]
fn periodic_substring_lengths_preserve_signed_remainders_and_the_last_iteration() {
    check(
        br#"
        local function setup()
            local source = string.rep("abc\000\255d", 100)
            local sub = string.sub
            return function(first, last, step, width)
                local total, piece = 0, ""
                for i = first, last, step do
                    local start = 200 + i * 3
                    piece = sub(source, start, start + 20 + (i * 2 + 1) % width)
                    total = total + #piece
                end
                return total, piece
            end, source
        end
        local sum, source = setup()
        local total, piece = sum(-31, 54, 2, 6)
        assert(total == 1034 and piece == string.sub(source, 359, 384))
        total, piece = sum(72, -12, -3, -7)
        assert(total == 523 and piece == string.sub(source, 164, 182))
        total, piece = sum(3, 6, 1, -7)
        assert(total == 75 and piece == string.sub(source, 218, 237))
        total, piece = sum(-9, 49, 4, 0)
        return total == 0 and piece == ""
        "#,
    );
}

#[test]
fn periodic_substring_lengths_follow_clipping_and_combined_remainders() {
    check(
        br#"
        local function setup()
            local source = string.rep("abc", 100)
            local sub = string.sub
            return function()
                local clipped, combined = 0, 0
                for i = 1, 146 do
                    local start = (i - 1) * 2 + 1
                    clipped = clipped + #sub(source, start, start + i % 10 + 15)
                end
                for i = 123, 1, -1 do
                    local start = (i - 1) * 2 + 1
                    combined = combined + #sub(source, start, start + i % 12 + i % 5)
                end
                return clipped, combined
            end
        end
        local sum = setup()
        local clipped, combined = sum()
        return clipped == 2956 and combined == 1035
        "#,
    );
}
