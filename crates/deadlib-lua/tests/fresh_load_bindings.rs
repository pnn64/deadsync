use deadlib_lua::{Value, Vm, compile};

#[test]
fn dynamically_compiled_chunks_are_distinct_and_survive_collection() {
    let mut vm = Vm::new();
    let program = compile(
        br#"
local first = assert(loadstring('return 18'))
local second = assert(loadstring('return 18'))
assert(first ~= second and first() == 18 and second() == 18)
for i = 1, 300 do
    local source = 'return ' .. i
    local chunk = assert(loadstring(source))
    assert(chunk() == i)
    if i % 50 == 0 then collectgarbage() end
end
return 42
"#,
    )
    .unwrap();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn reloading_the_same_host_program_preserves_entry_identity() {
    let mut vm = Vm::new();
    let program = compile(b"return 93").unwrap();
    let first = vm.load_program(&program);
    let second = vm.load_program(&program);
    assert_eq!(first, second);
    assert_eq!(
        vm.call(Value::Function(first), &[]).unwrap(),
        [Value::Number(93.0)]
    );
}
