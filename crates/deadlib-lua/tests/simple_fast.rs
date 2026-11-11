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
