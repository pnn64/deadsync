use deadlib_lua::{Error, HostCall, IndexView, TableId, Value, Vm, compile};

/// Counts handler calls, so tests can tell which reads the resolver answered.
struct Calls(usize);

// `captures()[0]` is the class table. The handler adds two members a plain
// table read cannot produce: a computed value and the indexed table itself.
fn index(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.host_state_mut::<Calls>().unwrap().0 += 1;
    let owner = call.arguments()[0];
    let key = call.arguments()[1];
    let (computed, itself) = (call.intern(b"computed"), call.intern(b"owner"));
    if key == computed {
        let value = call.intern(b"from handler");
        return call.push(value);
    }
    if key == itself {
        return call.push(owner);
    }
    let Value::Table(class) = call.captures()[0] else {
        return Err(call.error(b"class table expected"));
    };
    let value = call.raw_table_get(class, key)?;
    call.push(value)
}

// Answers what the handler answers without a call: plain class fields and the
// owner. Absent fields and the computed member are left to the handler.
fn resolve(view: &IndexView<'_>, owner: TableId, key: Value) -> Option<Value> {
    let Value::String(name) = key else {
        return None;
    };
    if view.string_bytes(name)? == b"owner" {
        return Some(Value::Table(owner));
    }
    let Value::Table(class) = view.captures()[0] else {
        return None;
    };
    match view.raw_get(class, key) {
        Value::Nil => None,
        value => Some(value),
    }
}

fn vm() -> Vm {
    let mut vm = Vm::new();
    vm.set_host_state(Box::new(Calls(0)));
    let class = vm
        .execute(&compile(b"class = {answer = 42, shared = {}, no = false}; return class").unwrap())
        .unwrap()[0];
    let handler = vm.new_host_function(index, &[class]).unwrap();
    vm.set_index_resolver(handler, resolve).unwrap();
    vm.set_global(b"handler", handler).unwrap();
    vm
}

fn calls(vm: &mut Vm) -> usize {
    std::mem::take(&mut vm.host_state_mut::<Calls>().unwrap().0)
}

const READS: &[u8] = br#"
local object = setmetatable({own = 1}, {__index = handler})
local child = setmetatable({}, {__index = object})
local out = {}
for round = 1, 50 do
  out[1], out[2], out[3] = object.own, object.answer, object.no
  out[4], out[5], out[6] = object.computed, object.missing, object[round]
  out[7], out[8] = object.owner == object, child.owner == object
  out[9], out[10] = child.answer, child.computed
  collectgarbage('step')
  out[11] = object.shared == class.shared
end
return out[1], out[2], out[3], out[4], out[5], out[6], out[7], out[8], out[9], out[10], out[11]
"#;

fn observe(vm: &mut Vm) -> Vec<String> {
    let values = vm.execute(&compile(READS).unwrap()).unwrap().to_vec();
    values
        .into_iter()
        .map(|value| match value {
            Value::String(id) => String::from_utf8(vm.string_bytes(id).unwrap().to_vec()).unwrap(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_resolver_answers_plain_reads_and_the_handler_answers_the_rest() {
    let mut vm = vm();
    let results = observe(&mut vm);
    assert_eq!(
        results,
        [
            "Number(1.0)",
            "Number(42.0)",
            "Boolean(false)",
            "from handler",
            "Nil",
            "Nil",
            "Boolean(true)",
            "Boolean(true)",
            "Number(42.0)",
            "from handler",
            "Boolean(true)",
        ]
    );
    // Per round the handler runs only for: computed, missing, the numeric key
    // and the inherited computed member.
    assert_eq!(calls(&mut vm), 50 * 4);
}

#[test]
fn an_instruction_limit_or_hook_sends_every_read_to_the_handler_with_equal_results() {
    let mut fast = vm();
    let expected = observe(&mut fast);
    calls(&mut fast);

    let mut limited = vm();
    limited.set_instruction_limit(Some(u64::MAX));
    assert_eq!(observe(&mut limited), expected);
    // Every read that misses the object's own fields reaches the handler: ten per round.
    assert_eq!(calls(&mut limited), 50 * 10);

    let mut hooked = vm();
    hooked
        .execute(&compile(b"debug.sethook(function() end, 'c')").unwrap())
        .unwrap();
    assert_eq!(observe(&mut hooked), expected);
    assert_eq!(calls(&mut hooked), 50 * 10);
    hooked
        .execute(&compile(b"debug.sethook()").unwrap())
        .unwrap();
    assert_eq!(observe(&mut hooked), expected);
    assert_eq!(calls(&mut hooked), 50 * 4);
}

fn host_get(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (target, key) = (call.arguments()[0], call.arguments()[1]);
    let peeked = call.peek_table_get(target, key)?;
    let value = call.table_get(target, key)?;
    call.push(value)?;
    call.push(Value::Boolean(peeked.is_some()))?;
    call.push(Value::Boolean(peeked.is_none_or(|peeked| peeked == value)))
}

#[test]
fn host_reads_use_the_resolver_and_agree_with_lua_indexing() {
    let mut vm = vm();
    let get = vm.new_host_function(host_get, &[]).unwrap();
    vm.set_global(b"host_get", get).unwrap();
    let program = compile(
        br#"
        local object = setmetatable({own = 1}, {__index = handler})
        local seen = {}
        for _, key in ipairs{'own', 'answer', 'no', 'computed', 'missing', 'owner'} do
          local value, resolved, same = host_get(object, key)
          assert(value == object[key] and same, key)
          seen[#seen + 1] = tostring(resolved)
        end
        return table.concat(seen, ',')
        "#,
    )
    .unwrap();
    let Value::String(id) = vm.execute(&program).unwrap()[0] else {
        panic!("resolution flags")
    };
    // The computed and missing members need the handler; peeking reports that.
    assert_eq!(
        vm.string_bytes(id).unwrap(),
        b"true,true,true,false,false,true"
    );
}

#[test]
fn only_host_functions_accept_a_resolver() {
    let mut vm = vm();
    let lua = vm
        .execute(&compile(b"return function() end").unwrap())
        .unwrap()[0];
    let builtin = vm.get_global(b"print").unwrap();
    for value in [lua, builtin, Value::Nil, Value::Number(1.0)] {
        let error = vm.set_index_resolver(value, resolve).unwrap_err();
        assert!(
            error.to_string().contains("host function expected"),
            "{error}"
        );
    }
}

// A class whose own metatable routes back to the handler must not recurse
// inside a resolver: the view only follows table handlers.
fn through_class(view: &IndexView<'_>, _: TableId, key: Value) -> Option<Value> {
    let Value::Table(class) = view.captures()[0] else {
        return None;
    };
    view.get(class, key)
}

#[test]
fn a_view_defers_function_handlers_instead_of_resolving_them() {
    let mut vm = Vm::new();
    vm.set_host_state(Box::new(Calls(0)));
    let class = vm
        .execute(&compile(b"class = {answer = 42}; return class").unwrap())
        .unwrap()[0];
    let handler = vm.new_host_function(index, &[class]).unwrap();
    vm.set_index_resolver(handler, through_class).unwrap();
    vm.set_global(b"handler", handler).unwrap();
    let program = compile(
        br#"
        setmetatable(class, {__index = handler})
        local object = setmetatable({}, {__index = handler})
        return object.answer, object.missing, object.computed
        "#,
    )
    .unwrap();
    let values = vm.execute(&program).unwrap().to_vec();
    assert_eq!(values[0], Value::Number(42.0));
    assert_eq!(values[1], Value::Nil);
    assert!(matches!(values[2], Value::String(_)));
    // `answer` resolved inline; the other two each called the handler once.
    assert_eq!(calls(&mut vm), 2);
}

// A handler made inside a host callback gets its resolver there.
fn make_handler(call: &mut HostCall<'_>) -> Result<(), Error> {
    let class = call.arguments()[0];
    let handler = call.new_host_function(index, &[class])?;
    call.set_index_resolver(handler, resolve)?;
    // Only a host function can take one.
    assert!(call.set_index_resolver(class, resolve).is_err());
    call.push(handler)
}

#[test]
fn a_host_callback_can_give_its_handler_a_resolver() {
    let mut vm = Vm::new();
    vm.set_host_state(Box::new(Calls(0)));
    let factory = vm.new_host_function(make_handler, &[]).unwrap();
    vm.set_global(b"make_handler", factory).unwrap();
    let program = compile(
        br#"
        local object = setmetatable({}, {__index = make_handler({answer = 42})})
        local answer, computed
        for round = 1, 50 do answer, computed = object.answer, object.computed end
        return answer, computed == 'from handler'
        "#,
    )
    .unwrap();
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[Value::Number(42.0), Value::Boolean(true)]
    );
    // The class field never reached the handler; the computed member always did.
    assert_eq!(calls(&mut vm), 50);
}

#[test]
fn global_reads_through_an_environment_handler_use_the_resolver() {
    let mut vm = vm();
    let program = compile(
        br#"
        local environment = setmetatable({own = 1}, {__index = handler})
        local function read() return own, answer, no, computed == 'from handler', missing end
        setfenv(read, environment)
        local a, b, c, d, e
        for round = 1, 50 do a, b, c, d, e = read() end
        return a, b, c, d, e
        "#,
    )
    .unwrap();
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[
            Value::Number(1.0),
            Value::Number(42.0),
            Value::Boolean(false),
            Value::Boolean(true),
            Value::Nil
        ]
    );
    // Per round the handler runs for the computed and the missing global only.
    assert_eq!(calls(&mut vm), 50 * 2);
}
