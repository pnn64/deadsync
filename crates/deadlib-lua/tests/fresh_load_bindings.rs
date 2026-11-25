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

#[test]
fn reused_source_keeps_distinct_environments_names_and_weak_lifetimes() {
    let program = compile(
        br#"
        local text = 'return answer'
        local first = assert(loadstring(text, '=first'))
        local second = assert(loadstring(text, '=first'))
        local third = assert(loadstring(text, '=other'))
        setfenv(first, { answer = 17 })
        setfenv(second, { answer = 25 })
        assert(first ~= second and first() + second() == 42)
        assert(debug.getinfo(second, 'S').source == '=first')
        assert(debug.getinfo(third, 'S').source == '=other')
        local weak = setmetatable({ first, second, third }, { __mode = 'v' })
        first, second, third = nil, nil, nil
        collectgarbage('collect')
        assert(weak[1] == nil and weak[2] == nil and weak[3] == nil)
        local again = assert(loadstring(text, '=first'))
        setfenv(again, { answer = 42 })
        return again()
    "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}
