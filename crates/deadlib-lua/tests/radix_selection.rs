use deadlib_lua::{Value, Vm, compile};

#[test]
fn binary_radix_selection_preserves_fractional_and_large_quotients() {
    let program = compile(
        br#"
local function choose(source, number)
    local width = source.width
    local index = math.fmod(math.floor(number / state), width) + 1
    state = state * width
    return source[index]
end
local source = {11, 22, width = 2}
for again = 1, 30 do
    state = 1
    local first, second = choose(source, 5.5), choose(source, 5.5)
    assert(first == 22 and second == 11 and state == 4)

    state = 2147483648
    local high, next = choose(source, 4294967295.75), choose(source, 4294967295.75)
    assert(high == 22 and next == 11 and state == 8589934592)

    state = 1
    assert(choose(source, -0.0) == 11 and state == 2)
    assert(choose(source, 4294967296.0) == 11 and state == 4)
end
return 42
"#,
    )
    .unwrap();
    for instructions in [None, Some(u64::MAX)] {
        let mut vm = Vm::new();
        vm.set_instruction_limit(instructions);
        assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    }
}

#[test]
fn selection_observes_environment_changes_and_lookup_order() {
    let program = compile(
        br#"
local function choose(index, source)
    local width = source.width
    local key = calc.remainder(calc.round(index / stride), width) + 0.5
    stride = width * stride
    return source[key]
end

local env = { calc = {round = math.floor, remainder = math.fmod}, stride = 4 }
setfenv(choose, env)
local source = { width = 3, [0.5] = 11, [1.5] = 19, [2.5] = 42 }
for i = 1, 30 do
    env.stride = 4
    assert(choose(9, source) == 42 and env.stride == 12)
end

env.choose = choose
local function read_between(source)
    return choose(9, source), stride, choose(9, source), stride
end
setfenv(read_between, env)
env.stride = 4
local first, middle, second, last = read_between(source)
assert(first == 42 and middle == 12 and second == 11 and last == 36)

local function discard_first_result(source)
    choose(9, source)
    return stride, choose(9, source), stride
end
setfenv(discard_first_result, env)
env.stride = 4
local before, chosen, after = discard_first_result(source)
assert(before == 12 and chosen == 11 and after == 36 and env.stride == 36)

local function alias(index, source)
    local width = source.stride
    local key = calc.remainder(calc.round(index / stride), width) + 0.5
    stride = width * stride
    return source[key]
end
setfenv(alias, env)
env.stride, env[0.5], env[2.5] = 2, 21, 55
local first, second = alias(9, env), alias(9, env)
assert(first == 21 and second == 55 and env.stride == 16)

local hits = 0
local proxy = setmetatable({ width = 3 }, {
    __index = function(_, key)
        assert(key == 2.5 and env.stride == 12)
        hits = hits + 1
        return 42
    end,
})
env.stride = 4
assert(choose(9, proxy) == 42 and hits == 1 and env.stride == 12)

local follower = setmetatable({ width = 3 }, {
    __index = function(_, key)
        assert(key == 0.5 and env.stride == 36)
        hits = hits + 1
        return 17
    end,
})
env.stride = 4
local first, second = choose(9, source), choose(9, follower)
assert(first == 42 and second == 17 and hits == 2 and env.stride == 36)

local rounded = 0
env.calc.round = function(x)
    assert(x == 2.25 and env.stride == 4)
    rounded = rounded + 1
    return 1
end
env.stride = 4
assert(choose(9, source) == 19 and rounded == 1 and env.stride == 12)
return 42
"#,
    )
    .unwrap();
    for instructions in [None, Some(u64::MAX)] {
        let mut vm = Vm::new();
        vm.set_instruction_limit(instructions);
        assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    }
}
