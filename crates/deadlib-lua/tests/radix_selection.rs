use deadlib_lua::{Value, Vm, compile};

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
