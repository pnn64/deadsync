use deadlib_lua::{ErrorKind, Value, Vm, compile};

fn check(source: &[u8], expected: &[Value]) {
    let program = compile(source).expect("compile assignment test");
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&program).expect("execute assignment test"),
        expected
    );
}

#[test]
fn single_targets_cover_locals_globals_indices_and_function_definitions() {
    check(
        br#"
        local x = 1
        x = 4
        assigned_global = x + 2
        local t = {n = 7}
        t.n = assigned_global
        function t:increase(value)
            self.n = self.n + value
            return self.n
        end
        function assigned_function(value)
            return value * 2
        end
        return x, t:increase(3), assigned_function(5), assigned_global
        "#,
        &[
            Value::Number(4.0),
            Value::Number(9.0),
            Value::Number(10.0),
            Value::Number(6.0),
        ],
    );
}

#[test]
fn multi_target_swaps_and_rhs_adjustments_preserve_order() {
    check(
        br#"
        local a, b, c = 1, 2, 3
        a, b, c = b, c, a
        assert(a == 2 and b == 3 and c == 1)

        local calls = {}
        local function mark(n)
            calls[#calls + 1] = n
            return n
        end
        a, b, c = mark(5), mark(6), mark(7)
        assert(a == 5 and b == 6 and c == 7)
        assert(table.concat(calls, ',') == '5,6,7')

        local function triple() return 21, 22, 23 end
        a, b, c = triple()
        assert(a == 21 and b == 22 and c == 23)
        a, b, c = 99, triple()
        assert(a == 99 and b == 21 and c == 22)
        a, b, c = triple(), 88
        assert(a == 21 and b == 88 and c == nil)
        return a, b, c
        "#,
        &[Value::Number(21.0), Value::Number(88.0), Value::Nil],
    );
}

#[test]
fn indexed_targets_preserve_original_table_and_key_during_aliasing() {
    check(
        br#"
        local values = {[1] = 11, [2] = 22}
        local key = 1
        values[key], key = 33, 2
        assert(values[1] == 33 and values[2] == 22 and key == 2)

        local original = {value = 5}
        local replacement = {value = 6}
        local saved = original
        original, original.value = replacement, 77
        assert(original == replacement and saved.value == 77)
        assert(replacement.value == 6)

        local index = 1
        local first = {[1] = 1, [2] = 2}
        first[index], index, first[2] = 101, 2, 202
        assert(first[1] == 101 and first[2] == 202 and index == 2)
        return values[1], saved.value, first[1], first[2]
        "#,
        &[
            Value::Number(33.0),
            Value::Number(77.0),
            Value::Number(101.0),
            Value::Number(202.0),
        ],
    );
}

#[test]
fn assignment_errors_remain_catchable_and_do_not_corrupt_next_assignment() {
    for source in [
        b"a, 1 = 2, 3".as_slice(),
        b"a, b = 1, ".as_slice(),
        b"function t.() end".as_slice(),
    ] {
        let error = compile(source).expect_err("invalid assignment must not compile");
        assert_eq!(error.kind(), ErrorKind::Syntax, "{source:?}");
    }

    check(
        br#"
        local table_value = {}
        local key = 1
        local ok_nil = pcall(function() table_value[nil] = 8 end)
        local ok_nan = pcall(function() table_value[0/0] = 9 end)
        local ok_index = pcall(function() local bad = nil; bad[1] = 4 end)
        table_value[key], key = 55, 2
        assert(not ok_nil and not ok_nan and not ok_index)
        return table_value[1], key
        "#,
        &[Value::Number(55.0), Value::Number(2.0)],
    );
}
