use deadlib_lua::{ErrorKind, Value, Vm, compile};

#[test]
fn scalar_sort_callbacks_preserve_parameters_results_and_metamethods() {
    let source = br#"
local comparators = {
 function(a,b) return a<b end,
 function(a,b) return b>a end,
 function(a,b) return not(a>=b) end,
}
for _,less in ipairs(comparators) do
 local t={9,2,7,1,8,3,6,4,5}
 table.sort(t,less)
 for i=1,9 do assert(t[i]==i) end
end
for _,less in ipairs{
 function(a,b,c) return c end,
 function(a,b) return nil end,
 function(a,b) return false end,
 function(a,b) end,
} do
 local t={false,false,false,false}
 table.sort(t,less)
 assert(#t==4)
 for i=1,4 do assert(t[i]==false) end
end
local hits=0
local mt={__eq=function(a,b) hits=hits+1; return true end}
local t={};for i=1,12 do t[i]=setmetatable({},mt) end
local function unequal(a,b) return not(a==b) end
table.sort(t,unequal)
assert(hits>0 and #t==12)
return 42
"#;
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        [Value::Number(42.)]
    );
}

#[test]
fn invalid_sort_comparator_preserves_its_last_executed_line() {
    let program = compile(b"local t={};for i=1,100 do t[i]=i end\nlocal function less(a,b)\n return true\nend\ntable.sort(t,less)").unwrap();
    let mut vm = Vm::new();
    let error = vm.execute(&program).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Runtime);
    assert_eq!(error.line(), 3);
    assert!(
        error
            .message_bytes()
            .ends_with(b"invalid order function for sorting")
    );
    assert_eq!(
        vm.execute(&compile(b"return 42").unwrap()).unwrap(),
        [Value::Number(42.)]
    );
}

#[test]
fn sort_hooks_keep_comparator_frames_and_balanced_call_returns() {
    let source = br#"
local function less(a,b) return a<b end
local calls,returns=0,0
local t={8,3,7,2,6,1,5,4}
debug.sethook(function(event)
 if debug.getinfo(2,'f').func==less then
  if event=='call' then
   calls=calls+1
   local name,a=debug.getlocal(2,1)
   local name2,b=debug.getlocal(2,2)
   assert(name=='a' and name2=='b' and type(a)=='number' and type(b)=='number')
  elseif event=='return' then returns=returns+1 end
 end
end,'cr')
table.sort(t,less)
debug.sethook()
assert(calls>0 and calls==returns)
for i=1,8 do assert(t[i]==i) end
return 42
"#;
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        [Value::Number(42.)]
    );
}

#[test]
fn sort_budgets_match_dispatch_with_a_nonfiring_count_hook() {
    let hook =
        compile(b"debug.sethook(function() error('unexpected hook') end, '', 2147483647)").unwrap();
    for source in [
        b"local t={3,1,2};table.sort(t);return t[1],t[2],t[3]".as_slice(),
        b"local t={false,false,false};table.sort(t,function(a,b)return nil end);return #t"
            .as_slice(),
        b"local t={false,false,false};table.sort(t,function(a,b)return b end);return #t".as_slice(),
    ] {
        let program = compile(source).unwrap();
        for limit in 0..180 {
            let mut ordinary = Vm::new();
            let mut hooked = Vm::new();
            hooked.execute(&hook).unwrap();
            ordinary.set_instruction_limit(Some(limit));
            hooked.set_instruction_limit(Some(limit));
            match (
                ordinary.execute(&program).map(<[Value]>::to_vec),
                hooked.execute(&program).map(<[Value]>::to_vec),
            ) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "limit {limit}"),
                (Err(a), Err(b)) => {
                    assert_eq!(a.kind(), b.kind(), "limit {limit}");
                    assert_eq!(a.line(), b.line(), "limit {limit}");
                    assert_eq!(a.message_bytes(), b.message_bytes(), "limit {limit}");
                }
                other => panic!("sort budget differs at {limit}: {other:?}"),
            }
        }
    }
}

#[test]
fn numeric_sort_callbacks_keep_their_instruction_limit_boundary() {
    let program =
        compile(b"local t={3,1,2};table.sort(t,function(a,b)return a<b end);return t[1],t[2],t[3]")
            .unwrap();
    let hook =
        compile(b"debug.sethook(function() error('unexpected hook') end, '', 2147483647)").unwrap();
    // Established by ordinary frame dispatch: count hooks prevent comparison /
    // boolean fusion, so its two extra dispatches have their own budget charges.
    for (count_hook, needed) in [(false, 21), (true, 23)] {
        for limit in [needed - 1, needed] {
            let mut vm = Vm::new();
            if count_hook {
                vm.execute(&hook).unwrap();
            }
            vm.set_instruction_limit(Some(limit));
            let result = vm.execute(&program);
            if limit < needed {
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Limit);
            } else {
                assert_eq!(
                    result.unwrap(),
                    [Value::Number(1.), Value::Number(2.), Value::Number(3.)]
                );
            }
        }
    }
}

#[test]
fn completed_scalar_sort_lines_do_not_override_later_errors() {
    let mut vm = Vm::new();
    let program =
        compile(b"table.sort({3,1,2},function(a,b)\nreturn a<b\nend)\nerror('later')").unwrap();
    let error = vm.execute(&program).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Runtime);
    assert_eq!(error.line(), 4);
    let error = vm
        .execute(&compile(b"error('next entry')").unwrap())
        .unwrap_err();
    assert_eq!(error.line(), 1);
}

#[test]
fn numeric_sort_preserves_duplicates_and_fields_outside_the_sequence() {
    let source = br#"
local t={5,2,5,-3,0,4};t.extra='kept';t[100]='tail'
table.sort(t)
assert(table.concat(t,',')=='-3,0,2,4,5,5' and t.extra=='kept' and t[100]=='tail')
local a={};for i=1,4096 do a[i]=(i*37)%401-200 end
table.sort(a)
for i=2,#a do assert(a[i-1]<=a[i]) end
assert(#a==4096)
return 42
"#;
    let mut vm = Vm::new();
    assert_eq!(
        vm.execute(&compile(source).unwrap()).unwrap(),
        [Value::Number(42.)]
    );
}

#[test]
fn special_numeric_sort_values_retain_ordinary_dispatch_permutations() {
    let hook =
        compile(b"debug.sethook(function()error('unexpected hook')end,'',2147483647)").unwrap();
    for source in [
        b"local p=0;local n=-p;local t={p,n,1,-1,p,n};table.sort(t);return unpack(t)".as_slice(),
        b"local z=0;local n=z/z;local t={n,1,2,n,-1};table.sort(t);return unpack(t)".as_slice(),
        b"local z=0;local p=1/z;local t={p,3,-p,0,p};table.sort(t);return unpack(t)".as_slice(),
    ] {
        let program = compile(source).unwrap();
        let mut plain = Vm::new();
        let mut observed = Vm::new();
        observed.execute(&hook).unwrap();
        let bits = |values: &[Value]| {
            values
                .iter()
                .map(|value| match value {
                    Value::Number(n) => n.to_bits(),
                    _ => panic!("expected numbers"),
                })
                .collect::<Vec<_>>()
        };
        let plain = bits(plain.execute(&program).unwrap());
        let observed = bits(observed.execute(&program).unwrap());
        assert_eq!(plain, observed);
    }
}

#[test]
fn numeric_global_updates_match_observed_sort_dispatch() {
    let program = compile(
        br#"
local out={}
for _,less in ipairs{
 function(a,b)counter=counter+3;return a<b end,
 function(a,b)counter=counter-0.25;return b>a end,
 function(a,b)counter=counter*1.125;return not(a>=b)end,
}do
 counter=1
 local t={9,2,7,1,8,3,6,4,5};table.sort(t,less)
 for i=1,9 do assert(t[i]==i) end
 out[#out+1]=counter
end
local env=setmetatable({counter=2},{__newindex=function()error('existing numeric field')end})
local function less(a,b)counter=counter+1;return a<b end
setfenv(less,env);table.sort({4,3,2,1},less)
out[#out+1]=env.counter
return unpack(out)
"#,
    )
    .unwrap();
    let hook =
        compile(b"debug.sethook(function()error('unexpected hook')end,'',2147483647)").unwrap();
    let mut plain = Vm::new();
    let mut observed = Vm::new();
    observed.execute(&hook).unwrap();
    assert_eq!(
        plain.execute(&program).unwrap(),
        observed.execute(&program).unwrap()
    );
}

#[test]
fn numeric_global_update_fallbacks_preserve_metamethod_effects() {
    let program = compile(
        br#"
local reads,writes=0,0
local env=setmetatable({}, {__index=function()reads=reads+1;return 10 end,
 __newindex=function(_,_,n)writes=writes+1;assert(n==11)end})
local function less(a,b)counter=counter+1;return a<b end
setfenv(less,env);table.sort({9,3,8,2,7,1,6,4,5},less)
assert(reads==writes and reads>0)
counter='0';table.sort({4,3,2,1},function(a,b)counter=counter+1;return a<b end)
local coerced=counter
local additions=0
counter=setmetatable({},{__add=function(_,n)additions=additions+1;return 10+n end})
table.sort({4,3,2,1},function(a,b)counter=counter+1;return a<b end)
assert(additions==1);local added=counter
counter=0;local calls=0
local mt={__lt=function(a,b)calls=calls+1;assert(counter==calls);return a.n<b.n end}
local t={};for i=1,8 do t[i]=setmetatable({n=9-i},mt)end
table.sort(t,function(a,b)counter=counter+1;return a<b end)
assert(counter==calls)
return reads,writes,coerced,additions,added,counter,calls
"#,
    )
    .unwrap();
    let hook =
        compile(b"debug.sethook(function()error('unexpected hook')end,'',2147483647)").unwrap();
    let mut plain = Vm::new();
    let mut observed = Vm::new();
    observed.execute(&hook).unwrap();
    assert_eq!(
        plain.execute(&program).unwrap(),
        observed.execute(&program).unwrap()
    );
}

#[test]
fn numeric_global_update_sort_errors_keep_completed_writes_and_return_lines() {
    let program=compile(b"counter=0;local t={};for i=1,100 do t[i]=i end\nlocal function less(a,b)\n counter=counter+1\n return true\nend\ntable.sort(t,less)").unwrap();
    let hook =
        compile(b"debug.sethook(function()error('unexpected hook')end,'',2147483647)").unwrap();
    for observed in [false, true] {
        let mut vm = Vm::new();
        if observed {
            vm.execute(&hook).unwrap();
        }
        let error = vm.execute(&program).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Runtime);
        assert_eq!(error.line(), 4);
        assert!(
            error
                .message_bytes()
                .ends_with(b"invalid order function for sorting")
        );
        assert_eq!(vm.global(b"counter"), Value::Number(102.));
    }
}

#[test]
fn numeric_global_updates_keep_budget_side_effects_in_order() {
    let program=compile(b"counter=0;local t={1,2,3};table.sort(t,function(a,b)counter=counter+1;return nil end);return counter").unwrap();
    let hook =
        compile(b"debug.sethook(function()error('unexpected hook')end,'',2147483647)").unwrap();
    for limit in 0..120 {
        let mut plain = Vm::new();
        let mut observed = Vm::new();
        observed.execute(&hook).unwrap();
        plain.set_instruction_limit(Some(limit));
        observed.set_instruction_limit(Some(limit));
        let a = plain
            .execute(&program)
            .map(<[Value]>::to_vec)
            .map_err(|e| (e.kind(), e.line(), e.message_bytes().to_vec()));
        let b = observed
            .execute(&program)
            .map(<[Value]>::to_vec)
            .map_err(|e| (e.kind(), e.line(), e.message_bytes().to_vec()));
        assert_eq!(a, b, "limit {limit}");
        assert_eq!(
            plain.global(b"counter"),
            observed.global(b"counter"),
            "limit {limit}"
        );
    }
}

#[test]
fn direct_scalar_partitions_match_observed_counts_and_partial_results() {
    let hook =
        compile(b"debug.sethook(function()error('unexpected hook')end,'',2147483647)").unwrap();
    let bits = |values: &[Value]| {
        values
            .iter()
            .map(|value| match value {
                Value::Number(number) => (0, number.to_bits()),
                Value::Boolean(boolean) => (1, u64::from(*boolean)),
                Value::Nil => (2, 0),
                _ => panic!("primitive sort result"),
            })
            .collect::<Vec<_>>()
    };
    for array in [
        "",
        "1",
        "9,2,7,1,8,3,6,4,5",
        "3,1,3,1,3,1,0,-0.0,0,-0.0",
        "1/0,-1/0,0/0,3,0/0,-2,0",
        "false,false,false,false,false,false,false",
        "nil,false,nil,3,nil,2,false,1",
    ] {
        for update in [
            "counter=counter+1",
            "counter=1+counter",
            "counter=counter-0.25",
            "counter=counter*1.01",
            "counter=counter+counter",
        ] {
            for predicate in [
                "a<b", "b>a", "b<a", "a>b", "nil", "false", "true", "counter",
            ] {
                let source = format!(
                    "counter=1.25;local t={{{array}}};local n=select('#',{array});\
                     local ok=pcall(table.sort,t,function(a,b){update};return {predicate} end);\
                     return ok,counter,unpack(t,1,n)"
                );
                // select('#',) would be invalid for the empty constructor.
                let source = source.replace("select('#',)", "0");
                let program = compile(source.as_bytes()).unwrap();
                let mut plain = Vm::new();
                let mut observed = Vm::new();
                observed.execute(&hook).unwrap();
                assert_eq!(
                    bits(plain.execute(&program).unwrap()),
                    bits(observed.execute(&program).unwrap()),
                    "{array}; {update}; {predicate}"
                );
            }
        }
    }
}
