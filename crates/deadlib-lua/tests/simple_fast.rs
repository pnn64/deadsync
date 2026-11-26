use deadlib_lua::{Value, Vm, compile};

#[test]
fn numeric_vararg_relays_keep_defaults_results_and_dispatch_guards() {
    let program = compile(
        br#"
        local function target(first, ...)
            local second, third, fourth = ...
            return first + (second or 0) * 3, (third or 0) * 5 + (fourth or 0) * 7
        end
        local function relay(...) return target(...) end
        local function one(x) return relay(x) end
        local function check(x, y, z, w)
            local a, b = relay(x, y, z, w)
            return a, b
        end
        for i = 1, 4 do
            local a, b = relay(4)
            assert(a == 4 and b == 0)
            a, b = relay(4, 6)
            assert(a == 22 and b == 0)
            a, b = check(4, 6, 8, 10)
            assert(a == 22 and b == 110)
        end
        assert(one(4) == 4)
        local object = setmetatable({}, {__mul = function(_, weight)
            assert(weight == 3)
            return 7
        end})
        local a, b = check(4, object, false, nil)
        assert(a == 11 and b == 0)
        target = function(x, ...) return x - 1, 39 end
        a, b = check(4, 6, 8, 10)
        return a + b
        "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
    vm.set_instruction_limit(Some(u64::MAX));
    assert_eq!(vm.execute(&program).unwrap(), [Value::Number(42.0)]);
}

#[test]
fn branched_vararg_sums_recheck_arrays_targets_and_rounding() {
    let program = compile(
        br#"
        local function setup(values, selectors)
            local function target(a, ...)
                local b, c = ...
                return a + (b or 0) * 2, (c or 0) * -3
            end
            local function relay(...) return target(...) end
            local function scan(first, last, step, total)
                for i = first, last, step do
                    local x, selected = values[i], selectors[i]
                    local a, b
                    if selected == 1 then
                        a, b = relay(x, x - 4)
                    elseif selected == 2 then
                        a, b = relay(x, x + 5, x * 2)
                    else
                        a, b = relay(x)
                    end
                    total = total + a + b
                end
                return total
            end
            return scan, function(new) target = new end
        end
        local values, selectors = {}, {}
        for i = 1, 192 do
            values[i] = (i % 19 - 10) / 4
            selectors[i] = i % 3
        end
        local scan, retarget = setup(values, selectors)
        local a = scan(1, 192, 1, 0)
        local b = scan(1, 192, 1, 21)
        values[81] = 11
        local c = scan(1, 192, 1, 0)
        selectors[149] = 2
        local d = scan(1, 192, 1, 0)
        local e = scan(192, 1, -1, 1e16)
        retarget(function(a, ...)
            local b = ...
            return a - (b or 0), 3
        end)
        local f = scan(1, 192, 1, 0)
        local hits = 0
        values[5] = nil
        setmetatable(values, {__index = function(_, key)
            assert(key == 5)
            hits = hits + 1
            return 15
        end})
        local g = scan(1, 192, 1, 0)
        assert(hits == 1)
        return a, b, c, d, e, f, g
        "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    let optimized = vm.execute(&program).unwrap().to_vec();
    vm.set_instruction_limit(Some(u64::MAX));
    assert_eq!(vm.execute(&program).unwrap(), optimized);
}

#[test]
fn fast_table_lengths_match_observed_execution_and_ignore_table_len_metamethods() {
    let program = compile(
        br#"
local dense={1,2,3,4,5}
local sparse={[1]=1,[2]=2,[8]=8}
local empty=setmetatable({},{__len=function()error('table __len was called')end})
local total=0
for i=1,1000 do total=total+#dense end
assert(total==5000 and #empty==0 and #'hello'==5)
local ok=pcall(function()return #false end)
assert(not ok)
local ticks=0
debug.sethook(function(event) if event=='count' then ticks=ticks+1 end end,'',1)
local observed=#dense
debug.sethook()
assert(observed==5 and ticks>0)
return total,#dense,#sparse,#empty,#'hello'
"#,
    )
    .unwrap();
    let mut results = Vec::new();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(u64::MAX));
        }
        results.push(vm.execute(&program).unwrap().to_vec());
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0][0], Value::Number(5000.));
    assert_eq!(results[0][1], Value::Number(5.));
    assert_eq!(results[0][3], Value::Number(0.));
    assert_eq!(results[0][4], Value::Number(5.));
}

#[test]
fn primitive_equality_keeps_lua_identity_and_shared_eq_handlers() {
    let program = compile(
        br#"
local calls = 0
local function equal() calls = calls + 1; return 'truthy' end
local meta = {__eq = equal}
local a = setmetatable({}, meta)
local b = setmetatable({}, meta)
local u = newproxy(true)
local v = newproxy(u)
getmetatable(u).__eq = equal
local function check(left, right, expected)
  assert((left == right) == expected)
  assert((left ~= right) == (not expected))
end
check(nil, nil, true)
check(nil, false, false)
check(true, false, false)
check('a\0b', 'a\0b', true)
check('a', 'b', false)
check(0, -0, true)
check(0 / 0, 0 / 0, false)
check(a, a, true)
check(a, b, true)
check(u, u, true)
check(u, v, true)
assert(calls == 4)
local count = 0
for i = 1, 1000 do
  if nil ~= false and true == true and 'a' ~= 'b' then count = count + 1 end
end
return count, calls
"#,
    )
    .unwrap();
    let mut results = Vec::new();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(1_000_000));
        }
        results.push(vm.execute(&program).unwrap().to_vec());
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0], [Value::Number(1000.), Value::Number(4.)]);
}

#[test]
fn distinct_tables_observe_shared_eq_only_when_both_metatables_provide_it() {
    let program = compile(
        br#"
local calls = 0
local a, b = {}, {}
local function equal() calls = calls + 1; return true end
local meta = { __eq = equal }
assert(a ~= b and not (a == b))
setmetatable(a, meta)
assert(a ~= b and not (a == b))
setmetatable(b, {})
assert(a ~= b and not (a == b))
getmetatable(b).__eq = equal
assert(a == b and not (a ~= b))
assert(calls == 2)
getmetatable(b).__eq = function() return true end
assert(a ~= b and not (a == b))
setmetatable(b, nil)
assert(a ~= b and not (a == b))
return calls
"#,
    )
    .unwrap();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(1_000_000));
        }
        assert_eq!(vm.execute(&program).unwrap(), [Value::Number(2.)]);
    }
}

#[test]
fn scalar_global_overwrites_preserve_newindex_and_deletion() {
    let program = compile(
        br#"
local hits = 0
local object = {}
_G.__scalar_global_test = object
setmetatable(_G, {__newindex = function(table, key, value)
  hits = hits + 1
  rawset(table, key, value)
end})
for i = 1, 1000 do __scalar_global_test = i end
assert(__scalar_global_test == 1000 and hits == 0)
__scalar_global_test = nil
assert(hits == 0)
__scalar_global_test = 1001
assert(hits == 1 and __scalar_global_test == 1001)
setmetatable(_G, nil)
return __scalar_global_test, hits
"#,
    )
    .unwrap();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(100_000));
        }
        assert_eq!(
            vm.execute(&program).unwrap(),
            [Value::Number(1001.), Value::Number(1.)]
        );
    }
}

#[test]
fn scalar_table_overwrites_preserve_newindex_deletions_and_invalid_keys() {
    let program = compile(
        br#"
local hits = 0
local t = setmetatable({a = 0, b = 1}, {__newindex = function(table, key, value)
  hits = hits + 1
  rawset(table, key, value)
end})
for i = 1, 1000 do t.a = i; t.b = i + 1 end
assert(t.a == 1000 and t.b == 1001 and hits == 0)
t.a = nil
t.a = 123
t.c = 7
t.b = 'text'
t.b = 456
assert(not pcall(function() t[nil] = 1 end))
assert(not pcall(function() t[0/0] = 1 end))
return hits, t.a, t.b, t.c
"#,
    )
    .unwrap();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(100_000));
        }
        assert_eq!(
            vm.execute(&program).unwrap(),
            [
                Value::Number(2.),
                Value::Number(123.),
                Value::Number(456.),
                Value::Number(7.),
            ]
        );
    }
}

#[test]
fn scalar_array_overwrites_preserve_newindex_on_nil_slots() {
    let program = compile(
        br#"
local hits = 0
local t = setmetatable({10, 20, 30}, {__newindex = function(table, key, value)
  hits = hits + 1
  rawset(table, key, value)
end})
for i = 1, 1000 do t[1] = i; t[2] = i + 1 end
assert(hits == 0)
t[2] = nil
t[2] = 9
t[4] = 11
assert(hits == 2)
assert(not pcall(function() t[nil] = 1 end))
assert(not pcall(function() t[0/0] = 1 end))
setmetatable(t, nil)
local sum = 0
for i = 1, 5000 do
  t[1] = i; t[2] = i + 1; t[3] = i + 2; t[4] = i + 3
  sum = sum + t[1] + t[4]
end
return sum, hits, t[4]
"#,
    )
    .unwrap();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(2_000_000));
        }
        assert_eq!(
            vm.execute(&program).unwrap(),
            [
                Value::Number(25_020_000.),
                Value::Number(2.),
                Value::Number(5003.),
            ]
        );
    }
}

#[test]
fn fast_upvalue_reads_preserve_open_closed_and_suspended_thread_values() {
    let program = compile(
        br#"
local function parent()
  local factor = 2
  local function add(n)
    local sum = 0
    for i = 1, n do sum = sum + factor end
    return sum
  end
  local during = add(2000)
  factor = 3
  return during, add
end
local during, closed = parent()
assert(during == 4000 and closed(2000) == 6000)
local shared
local co = coroutine.create(function()
  local factor = 4
  shared = function(n)
    local sum = 0
    for i = 1, n do sum = sum + factor end
    return sum
  end
  coroutine.yield()
  factor = 5
end)
assert(coroutine.resume(co))
local suspended = shared(2000)
assert(coroutine.resume(co))
local finished = shared(2000)
assert(suspended == 8000 and finished == 10000)
return during, closed(2000), suspended, finished
"#,
    )
    .unwrap();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(1_000_000));
        }
        assert_eq!(
            vm.execute(&program).unwrap(),
            [
                Value::Number(4000.),
                Value::Number(6000.),
                Value::Number(8000.),
                Value::Number(10_000.),
            ]
        );
    }
}

#[test]
fn inherited_reads_match_observed_execution_for_table_and_function_handlers() {
    let program = compile(
        br#"
local calls = 0
local base = {shared = 'base', zero = 0, no = false}
local middle = setmetatable({own = 'middle'}, {__index = base})
local leaf = setmetatable({mine = 'leaf'}, {__index = middle})
local dynamic = setmetatable({}, {__index = function(_, key) calls = calls + 1; return key .. '!' end})
local below = setmetatable({}, {__index = dynamic})
local text = setmetatable({}, {__index = 'abc'})
local bare = setmetatable({}, {})
local deep = base
for _ = 1, 20 do deep = setmetatable({}, {__index = deep}) end
local out = {}
for i = 1, 300 do
  out[1], out[2], out[3] = leaf.mine, leaf.own, leaf.shared
  out[4], out[5], out[6] = leaf.zero, leaf.no, leaf.missing
  out[7], out[8] = below.late, text.len
  out[9], out[10] = bare.missing, deep.shared
  out[11] = leaf[i]
end
assert(calls == 300 and out[8] == string.len and rawget(leaf, 'shared') == nil)
-- A shadowing write is visible to the next inherited read.
middle.shared = 'shadow'
local shadowed = leaf.shared
middle.shared = nil
base[7] = 'seven'
local numeric = leaf[7]
-- Global reads resolve through a sandbox environment the same way.
local sandbox = setmetatable({}, {__index = _G})
local function globals() return tostring, missing_global end
setfenv(globals, sandbox)
local found, absent = globals()
local cycle = {}
setmetatable(cycle, {__index = cycle})
local ok, message = pcall(function() return cycle.anything end)
return out[1], out[2], out[3], out[4], out[5], out[6], out[7], out[9], out[10], out[11],
  shadowed, numeric, found == tostring, absent, ok, message
"#,
    )
    .unwrap();
    let mut results = Vec::new();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.set_instruction_limit(Some(u64::MAX));
        }
        let values = vm.execute(&program).unwrap().to_vec();
        let text = |vm: &Vm, value: Value| match value {
            Value::String(id) => String::from_utf8(vm.string_bytes(id).unwrap().to_vec()).unwrap(),
            other => format!("{other:?}"),
        };
        results.push(values.iter().map(|&value| text(&vm, value)).collect::<Vec<_>>());
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(
        results[0],
        [
            "leaf",
            "middle",
            "base",
            "Number(0.0)",
            "Boolean(false)",
            "Nil",
            "late!",
            "Nil",
            "base",
            "Nil",
            "shadow",
            "seven",
            "Boolean(true)",
            "Nil",
            "Boolean(false)",
            "(deadlib-lua):34: loop in gettable",
        ]
    );
}
