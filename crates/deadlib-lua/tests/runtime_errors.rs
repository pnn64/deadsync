use deadlib_lua::{Value, Vm, compile_named};
use lua_reference::Lua;

struct Oracle {
    vm: Vm,
    lua: Lua,
}
impl Oracle {
    fn new() -> Self {
        // SAFETY: fixed developer inputs use only standard runtime operations.
        Self {
            vm: Vm::new(),
            lua: unsafe { Lua::unsafe_new() },
        }
    }
    fn check(&mut self, body: &str) {
        let source = format!("return pcall(function() {body} end)");
        let program = compile_named(source.as_bytes(), b"=runtime-errors").unwrap();
        let actual = self.vm.execute(&program).unwrap().to_vec();
        let error = match actual.as_slice() {
            [Value::Boolean(false), Value::String(id)] => {
                self.vm.string_bytes(*id).unwrap().to_vec()
            }
            value => panic!("expected a string failure for {body}: {value:?}"),
        };
        let (ok, expected): (bool, lua_reference::LuaString) = self
            .lua
            .load(source.as_bytes())
            .set_name("=runtime-errors")
            .eval()
            .unwrap();
        assert!(!ok, "reference succeeded: {body}");
        assert_eq!(
            error,
            expected.as_bytes().as_ref(),
            "{body}\nRust: {}\nC: {}",
            String::from_utf8_lossy(&error),
            String::from_utf8_lossy(expected.as_bytes().as_ref())
        );
    }
    fn cases(&mut self, bodies: &[&str]) {
        for body in bodies {
            self.check(body);
        }
    }
}

#[test]
fn destination_reuse_preserves_error_names_and_operand_types() {
    Oracle::new().cases(&[
        "local a=1;local function f()return false end;a=-f()",
        "local a=1;local function f()return false end;a=(f()+1)+2",
        "local t={a=false};return t.a+1",
        "local t={a={b=false}};return t.a.b+1",
        "local t={a={}};return t.a.b[1]",
        "local a=1;local function f()a=false;return 1 end;return a+f()",
        "local t={a=false};local key='a';return t[key]+1",
        "local t={a=false};return #t.a",
        "local a={};local function f()return false end;a=f().x",
        "local t={a=false};return t.a[1]",
    ]);
}

#[test]
fn call_target_and_constructor_reuse_preserve_chained_failure_context() {
    Oracle::new().cases(&[
        "local function f()return false end;return f()()",
        "local obj={};function obj:m()return false end;return obj:m():m()",
        "local obj={};function obj:m()return 42 end;return obj:m()(false)",
        "local function f()return math.sin end;return f()(false)",
        "local a={};local function f()return false end;a={key=f().missing}",
        "local function f()return 1 end;return f().x()",
    ]);
}

#[test]
fn call_failures_identify_globals_locals_fields_methods_and_upvalues() {
    Oracle::new().cases(&[
        "a=1; bbbb=2; a=math.sin(3)+bbbb(3)",
        "a=1; local a,bbbb=2,3; a=math.sin(1) and bbbb(3)",
        "a={}; do local a=1 end a:bbbb(3)",
        "local a={}; a.bbbb(3)",
        "a={13}; local bbbb=1; a[bbbb](3)",
        "local bbbb=1; (function() bbbb() end)()",
        "local f=1; local g=f; g()",
        "local f=1; local function g() return f() end; g()",
        "return (nil)()",
        "return ({})()",
        "local t={};t['\\255\\000suffix']()",
        "local t={};t['\\255']()",
    ]);
}

#[test]
fn indexing_failures_keep_original_operands_and_discard_redirected_names() {
    Oracle::new().cases(&[
        "aaa=nil;return aaa.bbb:ddd(9)",
        "local aaa={bbb=1}; return aaa.bbb:ddd(9)",
        "local aaa={bbb={}};return aaa.bbb:ddd(9)",
        "local x=nil;return x.y",
        "local x=nil;x.y=1",
        "x=nil;x.y=1",
        "local x=nil;(function() return x.y end)()",
        "local x=nil;(function() x.y=1 end)()",
        "local x=setmetatable({},{__index=false});return x.y",
        "local x=setmetatable({},{__newindex=false});x.y=1",
        "return ({a=false}).a.b",
        "({a=false}).a.b=1",
        "local key='a';return ({a=false})[key].b",
        "return ({a=false})[('a')].b",
        "return ({[3]=false})[3].b",
        "return ({['\\255']=false})['\\255'].b",
        "local a,b=nil,3;a,b.x=1,2",
    ]);
}

#[test]
fn arithmetic_length_and_concatenation_select_the_failing_operand() {
    Oracle::new().cases(&[
        "local a,b,c;(function() a=b+1 end)()",
        "b=1;local aaa='a';return aaa+b",
        "aaa={};return 3/aaa",
        "aaa='2';b=nil;return aaa*b",
        "aaa={};return -aaa",
        "aaa={};return (aaa or aaa)+(aaa and aaa)",
        "aaa={};return (aaa or aaa)()",
        "local a=false;return #a",
        "a=false;return #a",
        "local a=false;(function()return #a end)()",
        "local t={a=false};return #t.a",
        "local a=false;return a..'x'",
        "local a=false;return 'x'..a",
        "local a=false;return 'x'..3 ..a",
        "return print..'a'",
        "return nil+false",
        "return 'wrong'+nil",
        "local t={['\\255']=false};return t['\\255']+1",
        "local t={a=false};return t.a+function()end",
        "local x=false;local function f()return 3 end;return x+f()",
    ]);
}

#[test]
fn generated_operand_types_and_origin_combinations_match_reference() {
    let mut oracle = Oracle::new();
    for value in [
        "nil",
        "false",
        "{}",
        "function()end",
        "coroutine.create(function()end)",
        "'wrong'",
    ] {
        for (setup, operand) in [
            (format!("local target={value};"), "target"),
            (format!("target={value};"), "target"),
            (
                format!("local target={value};return (function() "),
                "target",
            ),
            (format!("local t={{field={value}}};"), "t.field"),
            (
                format!("local k='field';local t={{field={value}}};"),
                "t[k]",
            ),
        ] {
            let suffix = if setup.ends_with("(function() ") {
                "end)()"
            } else {
                ""
            };
            for expression in [
                format!("{operand}+1"),
                format!("1+{operand}"),
                format!("-{operand}"),
                format!("#{operand}"),
                format!("{operand}..'a'"),
                format!("'a'..{operand}"),
            ] {
                if (value == "{}" && expression.starts_with('#'))
                    || (value == "'wrong'"
                        && (expression.starts_with('#') || expression.contains("..")))
                {
                    continue;
                }
                oracle.check(&format!("{setup}return {expression} {suffix}"));
            }
        }
    }
}

#[test]
fn comparisons_keep_the_reference_type_pair_wording() {
    let mut oracle = Oracle::new();
    for left in [
        "nil",
        "false",
        "{}",
        "function()end",
        "coroutine.create(function()end)",
        "'x'",
        "1",
    ] {
        for right in [
            "nil",
            "true",
            "{}",
            "function()end",
            "coroutine.create(function()end)",
            "'y'",
            "2",
        ] {
            if (left == "'x'" && right == "'y'") || (left == "1" && right == "2") {
                continue;
            }
            for op in ["<", "<=", ">", ">="] {
                oracle.check(&format!("local a,b={left},{right};return a{op}b"));
            }
        }
    }
}

#[test]
fn short_circuit_results_and_temporary_calls_keep_reference_provenance() {
    let mut oracle = Oracle::new();
    for value in ["nil", "false", "{}", "'wrong'"] {
        for expression in [
            "target or target",
            "target and target",
            "target and target or target",
            "target or target and target",
            "(target or target) and target",
            "(target)",
            "(target or target)",
            "true and target",
            "false or target",
            "(false and target) or target",
            "target and false or target",
        ] {
            for setup in [format!("target={value};"), format!("local target={value};")] {
                for body in [
                    format!("return ({expression})+1"),
                    format!("return ({expression})()"),
                ] {
                    oracle.check(&format!("{setup}{body}"));
                }
            }
        }
    }
    oracle.cases(&[
        "local function f()return false end;return f()+1",
        "local function f()return false end;return #f()",
        "local function f()return false end;return f().x",
        "local function f()return false end;return f()()",
        "local function f()return false end;return f()..'x'",
    ]);
}

#[test]
fn nested_callbacks_loops_tail_calls_and_scopes_preserve_error_frames() {
    Oracle::new().cases(&[
        "local missing;local t=setmetatable({},{__index=function()return missing.x end});return t.x",
        "local missing;local t=setmetatable({},{__newindex=function()return missing.x end});t.x=1",
        "local target=false;local function f() return target+1 end;local function g()return f() end;return g()",
        "local target=false;for i=1,3 do if i==2 then return target+1 end end",
        "local target=false;repeat local q=3;return target+q until true",
        "local target=false;do local target=3 end;return target+1",
        "local target=3;do local target=false;return target+1 end",
        "local target=false;return setmetatable({},{__add=function()return target+1 end})+1",
    ]);
}

#[test]
fn generated_logical_trees_preserve_symbolic_names_across_distinct_operands() {
    let mut oracle = Oracle::new();
    for (a, b, c) in [
        ("nil", "false", "{}"),
        ("false", "{}", "nil"),
        ("{}", "false", "{}"),
        ("'wrong'", "'wrong'", "'wrong'"),
        ("nil", "nil", "nil"),
        ("false", "false", "false"),
        ("{}", "{}", "{}"),
        ("'wrong'", "false", "'wrong'"),
    ] {
        for first in ["and", "or"] {
            for second in ["and", "or"] {
                for expression in [
                    format!("(a {first} b) {second} c"),
                    format!("a {first} (b {second} c)"),
                ] {
                    for prefix in [
                        format!("a,b,c={a},{b},{c};"),
                        format!("local a,b,c={a},{b},{c};"),
                        format!("local a,b,c={a},{b},{c};return (function() "),
                    ] {
                        let suffix = if prefix.ends_with("(function() ") {
                            "end)()"
                        } else {
                            ""
                        };
                        for operation in [format!("({expression})+1"), format!("({expression})()")]
                        {
                            oracle.check(&format!("{prefix}return {operation} {suffix}"));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn protected_handlers_coroutines_and_nonfunction_metamethods_keep_messages() {
    Oracle::new().cases(&[
        "local ok,e=xpcall(function()local target=false;return target+1 end,function(e)collectgarbage();return e end);error(e,0)",
        "local co=coroutine.create(function()local target=false;return target+1 end);local ok,e=coroutine.resume(co);error(e,0)",
        "return coroutine.wrap(function()local target=false;return target+1 end)()",
        "local target=setmetatable({},{__add=false});return target+1",
        "local target=setmetatable({},{__concat=false});return target..'x'",
        "local target=setmetatable({},{__call=false});return target()",
        "local mt={__index={}};setmetatable(mt.__index,{__index=false});local target=setmetatable({},mt);return target.x",
        "local mt={__newindex={}};setmetatable(mt.__newindex,{__newindex=false});local target=setmetatable({},mt);target.x=1",
    ]);
}

#[test]
fn host_runtime_diagnostics_keep_raw_names_with_readable_display() {
    let program = compile_named(
        b"local t={['\\255']=false};return t['\\255']+1",
        b"=runtime-errors",
    )
    .unwrap();
    let mut vm = Vm::new();
    let error = vm.execute(&program).unwrap_err();
    assert_eq!(
        error.message_bytes(),
        b"attempt to perform arithmetic on field '\xff' (a boolean value)"
    );
    assert!(error.message().contains('\u{fffd}'));
    assert_eq!(error.line(), 1);
}
