use deadlib_lua::{Value, Vm, compile};

#[test]
fn immediate_name_fields_follow_the_actual_caller_and_builtin() {
    let program = compile(
        br#"
local builtin = debug.getinfo
local function read()
    return debug.getinfo(1, 'n').name, debug.getinfo(1, 'n').namewhat
end
local alias = read
for i = 1, 25 do
    local name, kind = alias()
    assert(name == 'alias' and kind == 'local')
end

local calls = 0
debug.getinfo = function(level, opts)
    assert(level == 1 and opts == 'n')
    return setmetatable({}, { __index = function(_, field)
        calls = calls + 1
        return field
    end })
end
local name, kind = alias()
assert(name == 'name' and kind == 'namewhat' and calls == 2)
debug.getinfo = builtin
name, kind = read()
assert(name == 'read' and kind == 'local')
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
fn immediate_name_fields_retain_observable_debug_calls() {
    let program = compile(
        br#"
local builtin = debug.getinfo
local function name_of_self() return debug.getinfo(1, 'n').name end
local events = 0
debug.sethook(function(event)
    if event == 'call' and builtin(2, 'f').func == builtin then
        events = events + 1
    end
end, 'c')
local name = name_of_self()
debug.sethook()
assert(name == 'name_of_self' and events == 1)
return 42
"#,
    )
    .unwrap();
    assert_eq!(Vm::new().execute(&program).unwrap(), [Value::Number(42.0)]);
}
