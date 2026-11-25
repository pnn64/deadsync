use deadlib_lua::{Value, Vm, compile};

#[test]
fn batched_overwrites_keep_the_field_version_and_final_values() {
    let program = compile(
        br#"
        local t = { a = 0, b = 0 }
        for i = 31, 1, -2 do
            t.a = i * 2
            t.b = 10 - i
            t.a = i + 33
        end
        assert(t.a == 34 and t.b == 9)
        return t
    "#,
    )
    .unwrap();
    for create in [Vm::try_new, Vm::try_new_with_lua51_table_order] {
        let mut vm = create().unwrap();
        let [Value::Table(table)] = *vm.execute(&program).unwrap() else {
            panic!("expected table")
        };
        let batched = vm.raw_fields_version(table).unwrap();
        vm.set_instruction_limit(Some(10_000));
        let [Value::Table(table)] = *vm.execute(&program).unwrap() else {
            panic!("expected table")
        };
        assert_eq!(batched, vm.raw_fields_version(table).unwrap());
    }
}

#[test]
fn field_hints_track_replacements_and_inheritance() {
    let program = compile(
        br#"
        local t = setmetatable({ x = 1, spare = 2 }, { __index = { x = 3 } })
        local function get() return t.x end
        local function set(v) t.x = v end
        assert(get() == 1)
        for i = 1, 20 do set(i); assert(get() == i) end
        t.x = nil
        assert(get() == 3)
        local parent = { spare = 1, x = 4 }
        setmetatable(t, { __index = parent })
        assert(get() == 4)
        parent.x = nil
        assert(get() == nil)
        for i = 1, 100 do parent['key' .. i] = i end
        parent.x = 5
        assert(get() == 5)
        local calls = 0
        setmetatable(t, { __index = function() calls = calls + 1; return calls end })
        local sum = 0
        for i = 1, 10 do sum = sum + t.x end
        assert(sum == 55 and calls == 10)
        return 42
    "#,
    )
    .unwrap();
    for create in [Vm::try_new, Vm::try_new_with_lua51_table_order] {
        let mut vm = create().unwrap();
        assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    }
}
