use deadlib_lua::{Value, Vm, compile};

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
