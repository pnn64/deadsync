use deadlib_lua::{Value, Vm, compile};

#[test]
fn fixed_parameter_returns_preserve_open_arity_and_object_identity() {
    let program = compile(
        br#"
local function project(first, second) return second end
local function arguments(x) return 17, x, 29 end
local marker = {}
for i = 1, 20 do
  assert(project(false, marker) == marker)
  assert(project('missing') == nil)
  assert(select('#', project(arguments(marker))) == 1)
  assert(project(arguments(marker)) == marker)
  local v, extra = project(arguments(marker))
  assert(v == marker and extra == nil)
  project(false, marker)
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
fn fixed_parameter_returns_preserve_call_hooks_that_write_arguments() {
    let program = compile(
        br#"
local function project(x) return x end
local function hook(event)
    if event == 'call' and debug.getinfo(2, 'f').func == project then
        assert(debug.setlocal(2, 1, 42) == 'x')
    end
end
local warm = project(1)
assert(warm == 1)
debug.sethook(hook, 'c')
local result = project(23)
debug.sethook()
return result
"#,
    )
    .unwrap();
    assert_eq!(Vm::new().execute(&program).unwrap(), [Value::Number(42.0)]);
}
