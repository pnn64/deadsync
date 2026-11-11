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
