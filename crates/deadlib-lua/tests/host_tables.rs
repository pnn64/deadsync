use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn get(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.table_get(call.arguments()[0], call.arguments()[1])?;
    call.push(value)
}
fn peek_get(call: &mut HostCall<'_>) -> Result<(), Error> {
    let target = call.arguments()[0];
    let key = call.arguments()[1];
    match call.peek_table_get(target, key)? {
        Some(value) => call.push(value),
        None => {
            let value = call.table_get(target, key)?;
            call.push(value)
        }
    }
}
fn set(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.table_set(
        call.arguments()[0],
        call.arguments()[1],
        call.arguments()[2],
    )
}
fn vm() -> Vm {
    let mut vm = Vm::new();
    for (name, callback) in [
        (b"host_get".as_slice(), get as deadlib_lua::HostFunction),
        (b"host_peek".as_slice(), peek_get),
        (b"host_set", set),
    ] {
        let function = vm.new_host_function(callback, &[]).unwrap();
        vm.set_global(name, function).unwrap();
    }
    vm
}

#[test]
fn peek_table_fields_preserves_inheritance_and_defers_dynamic_handlers() {
    let mut vm = vm();
    let program = compile(
        br#"
        local base = {name = 'base', absent = false}
        local child = setmetatable({}, {__index = setmetatable({}, {__index = base})})
        assert(host_peek(child, 'name') == child.name)
        assert(host_peek(child, 'absent') == false)
        assert(host_peek(child, 'missing') == nil)
        base.name = 'changed'
        assert(host_peek(child, 'name') == 'changed')

        local calls = 0
        local dynamic = setmetatable({}, {__index = function(_, key)
            calls = calls + 1
            collectgarbage('collect')
            return { key = key }
        end})
        local result = host_peek(setmetatable({}, {__index = dynamic}), 'fresh')
        collectgarbage('collect')
        assert(result.key == 'fresh' and calls == 1)

        local deep = base
        for _ = 1, 20 do deep = setmetatable({}, {__index = deep}) end
        assert(host_peek(deep, 'name') == 'changed')
        assert(host_peek('text', 'sub') == string.sub)
        return true
    "#,
    )
    .unwrap();
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
}

#[test]
fn protected_fields_follow_metatable_chains_and_keep_operands_alive_during_collection() {
    let source = br#"
        local target, writes = {}, {}
        local proxy = setmetatable({}, {__index = setmetatable({}, {
            __index = function(_, key) collectgarbage(); return {key = key} end
        }), __newindex = function(_, key, value)
            collectgarbage(); writes[#writes+1] = key; target[key] = value
        end})
        local key, value = {}, {}
        SET(proxy, key, value)
        local result = GET(proxy, key)
        assert(result.key == key and target[key] == value and writes[1] == key)
        SET(target, 'raw', 42)
        assert(GET(target, 'raw') == 42 and GET(target, 'missing') == nil)
        assert(GET('abc', 'sub') == string.sub)
        return true
    "#;
    let mut vm = vm();
    let source = std::str::from_utf8(source).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                source
                    .replace("SET", "host_set")
                    .replace("GET", "host_get")
                    .as_bytes()
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true)]
    );
    // Exercise the same written invariants through Lua indexing and through
    // the Rust host table API above.
    let mut language = Vm::new();
    language
        .execute(
            &compile(b"function GET(t,k) return t[k] end; function SET(t,k,v) t[k]=v end").unwrap(),
        )
        .unwrap();
    assert_eq!(
        language
            .execute(&compile(source.as_bytes()).unwrap())
            .unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn ordinary_field_reads_preserve_results_and_observe_later_table_changes() {
    fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
        let target = call.arguments()[0];
        let key = call.arguments()[1];
        call.push(Value::Number(17.))?;
        let value = call.table_get(target, key)?;
        call.push(value)?;
        // A later nested call must preserve the results already appended.
        call.call(call.arguments()[2], &[target, key])?;
        let changed = call.table_get(target, key)?;
        call.push(changed)?;
        call.push(call.arguments()[3])
    }
    let mut vm = vm();
    let function = vm.new_host_function(read, &[]).unwrap();
    vm.set_global(b"read", function).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                br#"
            local marker = {}
            local t = setmetatable({x = false}, {
                __index = function(_, key) collectgarbage(); return key end
            })
            local function remove(t, key) t[key] = nil; collectgarbage() end
            local a,b,c,d = read(t, 'x', remove, marker)
            assert(a == 17 and b == false and c == 'x' and d == marker)
            setmetatable(t, nil)
            a,b,c,d = read(t, 'x', function(t,k) t[k] = 42 end, marker)
            assert(a == 17 and b == nil and c == 42 and d == marker)
            return true
        "#
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn rust_can_handle_exact_metamethod_errors_and_continue_using_the_callback() {
    fn recover(call: &mut HostCall<'_>) -> Result<(), Error> {
        let object = call.arguments()[0];
        let key = call.try_intern(b"missing")?;
        let error = call.table_get(object, key).unwrap_err();
        call.push(error.value().expect("original Lua error"))?;
        let error = call.table_set(object, key, Value::Number(7.)).unwrap_err();
        call.push(error.value().expect("original Lua error"))?;
        let key = call.try_intern(b"present")?;
        let value = call.table_get(object, key)?;
        call.push(value)
    }
    let mut vm = vm();
    let host = vm.new_host_function(recover, &[]).unwrap();
    vm.set_global(b"recover", host).unwrap();
    let program = compile(
        br#"
        local marker = {}
        local t = setmetatable({present=42}, {
            __index=function() collectgarbage(); error(marker) end,
            __newindex=function() collectgarbage(); error(marker) end
        })
        local a,b,c = recover(t)
        return a == marker and b == marker and c == 42
    "#,
    )
    .unwrap();
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
}

#[test]
fn ignoring_a_field_instruction_limit_still_aborts_the_outer_entry() {
    fn ignore(call: &mut HostCall<'_>) -> Result<(), Error> {
        let key = call.try_intern(b"missing")?;
        let _ = call.table_get(call.arguments()[0], key);
        // The primitive fast path must still reject an already-aborted call.
        assert_eq!(
            call.table_get(call.arguments()[1], key).unwrap_err().kind(),
            ErrorKind::Limit
        );
        Ok(())
    }
    let mut vm = vm();
    let function = vm.new_host_function(ignore, &[]).unwrap();
    vm.set_global(b"ignore", function).unwrap();
    vm.set_instruction_limit(Some(100));
    let program = compile(
        b"ignore(setmetatable({}, {__index=function() while true do end end}), {}); return 42",
    )
    .unwrap();
    assert_eq!(vm.execute(&program).unwrap_err().kind(), ErrorKind::Limit);
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        &[Value::Number(42.)]
    );
}

#[test]
fn raw_reads_reject_collected_handles_and_do_not_invoke_metamethods() {
    let mut vm = vm();
    let table = vm
        .execute(
            &compile(b"return setmetatable({7,8}, {__index=function() error('unexpected') end})")
                .unwrap(),
        )
        .unwrap()[0];
    let Value::Table(id) = table else {
        panic!("table")
    };
    assert_eq!(vm.raw_table_len(id).unwrap(), 2);
    assert_eq!(
        vm.raw_table_get(id, Value::Number(1.)).unwrap(),
        Value::Number(7.)
    );
    assert_eq!(vm.raw_table_get(id, Value::Number(9.)).unwrap(), Value::Nil);
    vm.execute(&compile(b"return").unwrap()).unwrap();
    vm.collect_garbage().unwrap();
    assert!(vm.raw_table_get(id, Value::Number(1.)).is_err());
    assert!(vm.raw_table_len(id).is_err());
}

#[test]
fn ordinary_field_reads_reject_collected_tables_and_keys() {
    fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
        let stale = *call.function_data::<[Value; 2]>().unwrap();
        for (target, key) in [(stale[0], Value::Nil), (call.arguments()[0], stale[1])] {
            let error = call.table_get(target, key).unwrap_err();
            assert!(error.message().contains("invalid or collected"));
        }
        call.push(Value::Boolean(true))
    }
    let mut vm = Vm::new();
    let stale = [Value::Table(vm.new_table()), vm.intern(b"collected key")];
    vm.collect_garbage().unwrap();
    // Opaque data deliberately does not trace these handles.
    let function = vm.new_host_function_with_data(read, &[], stale).unwrap();
    vm.set_global(b"read", function).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return read({})").unwrap()).unwrap(),
        &[Value::Boolean(true)]
    );
}

#[test]
fn rust_created_callbacks_keep_captures_and_coerce_strings_without_metamethods() {
    fn factory(call: &mut HostCall<'_>) -> Result<(), Error> {
        fn captured(call: &mut HostCall<'_>) -> Result<(), Error> {
            call.push(call.captures()[0])
        }
        let id = call.coerce_string(call.arguments()[0])?.unwrap();
        let function = call.new_host_function(captured, &[Value::String(id)])?;
        call.push(function)
    }
    let mut vm = vm();
    let function = vm.new_host_function(factory, &[]).unwrap();
    vm.set_global(b"factory", function).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                br#"
        local a,b = factory(1.234567890123456),factory('a\000b')
        collectgarbage()
        return a()==tostring(1.234567890123456), b()=='a\000b'
    "#
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true), Value::Boolean(true)]
    );
    let table = vm
        .execute(
            &compile(b"return setmetatable({}, {__tostring=function() error('unexpected') end})")
                .unwrap(),
        )
        .unwrap()[0];
    assert_eq!(vm.coerce_string(table).unwrap(), None);
}

#[test]
fn field_metamethods_cannot_suspend_the_rust_callback_stack() {
    let mut vm = vm();
    let program = compile(
        br#"
        local t = setmetatable({}, {
            __index=function() coroutine.yield('get') end,
            __newindex=function() coroutine.yield('set') end
        })
        for _, f in ipairs({function() host_get(t,'x') end, function() host_set(t,'x',1) end}) do
            local thread = coroutine.create(f)
            local ok, err = coroutine.resume(thread)
            assert(not ok and type(err)=='string')
            assert(coroutine.status(thread)=='dead')
        end
        return true
    "#,
    )
    .unwrap();
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
}

#[test]
fn inherited_fields_resolve_through_table_handlers_like_lua_indexing() {
    let source = br#"
        local base = {shared = 'base', no = false}
        local leaf = setmetatable({mine = 1}, {__index = setmetatable({own = 2}, {__index = base})})
        local deep = base
        for _ = 1, 20 do deep = setmetatable({}, {__index = deep}) end
        local calls = 0
        local dynamic = setmetatable({}, {__index = setmetatable({}, {
            __index = function(_, key) calls = calls + 1; return key end
        })})
        local cycle = {}
        setmetatable(cycle, {__index = cycle})
        for _, object in ipairs{leaf, deep, dynamic, setmetatable({}, {})} do
            for _, key in ipairs{'mine', 'own', 'shared', 'no', 'missing', 7} do
                local expected = object[key]
                local actual = host_get(object, key)
                assert(actual == expected, tostring(key))
            end
        end
        assert(calls == 12)
        local ok, message = pcall(host_get, cycle, 'anything')
        assert(not ok and message == 'loop in gettable', message)
        -- The result stays rooted in the callback's scratch slot across a collection.
        base.fresh = {}
        collectgarbage()
        return host_get(leaf, 'fresh') == base.fresh
    "#;
    let mut vm = vm();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        &[Value::Boolean(true)]
    );
}

// Walks a table with `raw_table_next`, returning how many entries it holds and
// the sum of its numeric values, plus whether the actual metatable is the
// second argument.
fn walk(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(target) = call.arguments()[0] else {
        return Err(call.error(b"table expected"));
    };
    let (mut count, mut sum, mut key) = (0.0, 0.0, Value::Nil);
    while let Some((next, value)) = call.raw_table_next(target, key)? {
        count += 1.0;
        if let Value::Number(n) = value {
            sum += n;
        }
        key = next;
    }
    let metatable = call.raw_metatable(target)?.map(Value::Table);
    call.push(Value::Number(count))?;
    call.push(Value::Number(sum))?;
    call.push(Value::Boolean(
        metatable.unwrap_or(Value::Nil) == call.arguments()[1],
    ))
}

#[test]
fn raw_traversal_and_metatable_reads_bypass_metamethods() {
    let mut vm = vm();
    let function = vm.new_host_function(walk, &[]).unwrap();
    vm.set_global(b"host_walk", function).unwrap();
    let program = compile(
        br#"
        local count, sum, same = host_walk({}, nil)
        assert(count == 0 and sum == 0 and same)

        -- Array and hash parts are both visited, each entry once.
        local mixed = {10, 20, 30, name = 'x', [2.5] = 4, [true] = 5}
        local expected = 0
        for _ in next, mixed do expected = expected + 1 end
        count, sum, same = host_walk(mixed, nil)
        assert(count == expected and sum == 69 and same)

        -- Neither `__pairs`-like handlers nor `__index` contribute entries, and
        -- a `__metatable` field does not hide the actual metatable.
        local mt = {__index = {inherited = 1}, __metatable = 'locked'}
        local guarded = setmetatable({own = 7}, mt)
        assert(getmetatable(guarded) == 'locked')
        count, sum, same = host_walk(guarded, mt)
        assert(count == 1 and sum == 7 and same)
        count, sum, same = host_walk(guarded, nil)
        assert(not same)
        "#,
    )
    .unwrap();
    vm.execute(&program).unwrap();
}

fn stale_next(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(target) = call.arguments()[0] else {
        return Err(call.error(b"table expected"));
    };
    let absent = call.intern(b"absent");
    let error = call.raw_table_next(target, absent).unwrap_err();
    let message = call.intern(error.to_string().as_bytes());
    call.push(message)
}

#[test]
fn raw_traversal_rejects_a_key_the_table_does_not_hold() {
    let mut vm = vm();
    let function = vm.new_host_function(stale_next, &[]).unwrap();
    vm.set_global(b"host_stale_next", function).unwrap();
    let program = compile(b"return host_stale_next({present = 1})").unwrap();
    let Value::String(id) = vm.execute(&program).unwrap()[0] else {
        panic!("error message")
    };
    let message = String::from_utf8(vm.string_bytes(id).unwrap().to_vec()).unwrap();
    assert!(message.contains("invalid key to 'next'"), "{message}");
}

fn static_field(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Table(target) = call.arguments()[0] else {
        return Err(call.error(b"table expected"));
    };
    let value = call.raw_table_get_static(target, b"field")?;
    call.push(value)
}

#[test]
fn static_name_reads_are_raw_and_agree_with_ordinary_raw_reads() {
    let mut vm = vm();
    let function = vm.new_host_function(static_field, &[]).unwrap();
    vm.set_global(b"static_field", function).unwrap();
    let program = compile(
        br#"
        assert(static_field({}) == nil)
        assert(static_field({field = 7}) == 7)
        assert(static_field({field = false}) == false)
        -- Inherited fields and handlers are not consulted.
        local calls = 0
        local inherited = setmetatable({}, {__index = function() calls = calls + 1 return 1 end})
        assert(static_field(inherited) == nil and calls == 0)
        assert(static_field(setmetatable({}, {__index = {field = 3}})) == nil)
        -- Many other fields share the table, and the field is added last.
        local crowded = {}
        for i = 1, 200 do crowded['field' .. i] = i end
        assert(static_field(crowded) == nil)
        crowded.field = 'found'
        assert(static_field(crowded) == 'found' and rawget(crowded, 'field') == 'found')
        crowded.field = nil
        assert(static_field(crowded) == nil)
        "#,
    )
    .unwrap();
    vm.execute(&program).unwrap();
}
