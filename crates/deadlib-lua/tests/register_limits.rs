use deadlib_lua::{Value, Vm, compile};
use lua_reference::Lua;

const WRAPPER: &[u8] = br#"local calls=0;local at=0
local ok,f,e=pcall(function()
 if width==0 then return loadstring(source,name) end
 return load(function()
  calls=calls+1
  if calls==fault then error('reader stopped',0) end
  if at>=#source then return nil end
  local s=string.sub(source,at+1,at+width);at=at+width;return s
 end,name)
end)
if not ok then e=f;f=nil end
return ok,f~=nil,e,calls"#;

struct Oracle {
    vm: Vm,
    lua: Lua,
    wrapper: deadlib_lua::Program,
}
impl Oracle {
    fn new() -> Self {
        Self {
            vm: Vm::new(),
            // SAFETY: fixed developer text exercises the protected official loader.
            lua: unsafe { Lua::unsafe_new() },
            wrapper: compile(WRAPPER).unwrap(),
        }
    }
    fn check(&mut self, source: &str, width: usize, fault: usize) {
        let name = b"=register-budget";
        for (key, bytes) in [("source", source.as_bytes()), ("name", name.as_slice())] {
            let value = self.vm.intern(bytes);
            self.vm.set_global(key.as_bytes(), value).unwrap();
            self.lua
                .globals()
                .set(key, self.lua.create_string(bytes).unwrap())
                .unwrap();
        }
        for (key, number) in [("width", width), ("fault", fault)] {
            self.vm
                .set_global(key.as_bytes(), Value::Number(number as f64))
                .unwrap();
            self.lua.globals().set(key, number).unwrap();
        }
        let values = self.vm.execute(&self.wrapper).unwrap().to_vec();
        let [
            Value::Boolean(ok),
            Value::Boolean(loaded),
            error,
            Value::Number(calls),
        ] = values.as_slice()
        else {
            panic!("unexpected loader results: {values:?}");
        };
        let error = match error {
            Value::Nil => None,
            Value::String(id) => Some(self.vm.string_bytes(*id).unwrap().to_vec()),
            other => panic!("unexpected diagnostic: {other:?}"),
        };
        let actual = (*ok, *loaded, error, *calls as usize);
        let (ok, loaded, error, calls): (bool, bool, Option<lua_reference::LuaString>, usize) =
            self.lua.load(WRAPPER).eval().unwrap();
        let expected = (ok, loaded, error.map(|s| s.as_bytes().to_vec()), calls);
        assert_eq!(
            actual, expected,
            "width={width}, fault={fault}, source={source}"
        );
    }
    fn valid(&mut self, source: &str) {
        self.lua.load(source).into_function().expect(source);
        self.check(source, 0, 0);
    }
}
fn names(count: usize) -> String {
    (1..=count)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn repeated(value: &str, count: usize) -> String {
    std::iter::repeat_n(value, count)
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn calls_returns_and_constructor_boundaries_match_the_reference() {
    let mut oracle = Oracle::new();
    for locals in [0, 1, 20, 100, 199, 200] {
        let prefix = if locals == 0 {
            String::new()
        } else {
            format!("local {};", names(locals))
        };
        for count in 245usize.saturating_sub(locals)..=251 - locals {
            let args = repeated("1", count);
            for body in [
                format!("return f({args})"),
                format!("f({args})"),
                format!("return t:m({args})"),
                format!("return {args}"),
            ] {
                oracle.check(&format!("{prefix}{body}"), 0, 0);
            }
        }
        for count in [47, 48, 49, 50, 51, 99, 100, 101, 249, 250] {
            oracle.check(
                &format!("{prefix}return {{{}}}", repeated("1", count)),
                0,
                0,
            );
        }
    }
}

#[test]
fn logical_expression_lifetimes_match_at_the_register_boundary() {
    let mut oracle = Oracle::new();
    for expr in [
        "a",
        "f()",
        "(f())",
        "...",
        "(...)",
        "{}",
        "{1}",
        "{key=1}",
        "{[f()]=g()}",
        "a+b",
        "f()+g()",
        "-a",
        "#a",
        "not a",
        "not not a",
        "a<b",
        "a==f()",
        "a and b",
        "a or b",
        "a and b or c",
        "a<b and c",
        "not(a and b)",
        "a..b..c",
        "a..(b..c)",
        "(a..b)..c",
        "a[b]",
        "f()[g()]",
        "a.b",
        "a:m(1)",
        "function()return 1 end",
        "{f()}",
        "{1,f()}",
        "f(...)",
        "a:m(...)",
        "1+2",
        "0/0",
        "1/0",
        "0%0",
        "2^3",
        "0^(-1)",
    ] {
        for count in 243..=249 {
            oracle.check(&format!("return f({},{expr})", repeated("1", count)), 0, 0);
        }
    }
}

#[test]
fn register_checks_keep_error_precedence_and_reader_consumption() {
    let mut oracle = Oracle::new();
    for tail in [
        ")", ") ", ")\n'bad", ",1)", ",0xZ", ",f()", ",...)", ",{1})", ",a..b)",
    ] {
        let source = format!("return f({}{}", repeated("1", 249), tail);
        for width in [0, 1, 2, 3, 7, 64, 4096] {
            oracle.check(&source, width, 0);
        }
    }
    let source = format!("return f({},1)\n'bad", repeated("1", 249));
    for width in [1, 3, 7, 64] {
        for fault in 490 / width..=source.len() / width + 3 {
            oracle.check(&source, width, fault);
        }
    }
}

#[test]
fn constant_pools_and_function_scopes_have_independent_resource_limits() {
    let mut oracle = Oracle::new();
    for pool in [0, 254, 255, 256, 300] {
        let prefix = format!(
            "local pad={{{}}};",
            (1..=pool)
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        for expr in [
            "a[1]",
            "a['marker']",
            "a[true]",
            "a[nil]",
            "{[false]=1}",
            "a:m()",
            "a+1",
            "1+a",
        ] {
            for count in 242..=248 {
                oracle.check(
                    &format!("{prefix}return f({},{expr})", repeated("1", count)),
                    0,
                    0,
                );
            }
        }
    }
    let body = format!("return f({})", repeated("1", 248));
    oracle.valid(&format!(
        "local {};local f=function(){body} end;return f",
        names(199)
    ));
}

#[test]
fn assignments_loop_headers_and_fixed_multiple_results_match() {
    let mut oracle = Oracle::new();
    for count in [1, 2, 49, 100, 198, 199, 200] {
        let targets = names(count);
        for args in [47, 48, 49, 100, 247, 248, 249] {
            let args = repeated("1", args);
            for body in [
                format!("local {targets}=f({args})"),
                format!("local {targets};{targets}=f({args})"),
                format!("for a in {args} do end"),
            ] {
                oracle.check(&body, 0, 0);
            }
        }
    }
    for count in 240..=250 {
        oracle.check(
            &format!("for a,b,c in {} do end", repeated("1", count)),
            0,
            0,
        );
    }
}

#[test]
fn constructor_lookahead_and_expression_discharge_preserve_reader_order() {
    let mut oracle = Oracle::new();
    let locals = format!("local {};", names(200));
    let pending = repeated("1", 48);
    for tail in ["a=1}", "a\n'bad", "[1]=1}", "0xZ}", "...}", "f()}"] {
        let source = format!("{locals}return {{{pending},{tail}");
        for width in [0, 1, 2, 7, 31] {
            oracle.check(&source, width, 0);
        }
    }
    let args = repeated("1", 247);
    for tail in [
        "a[1])", "a[f()])", "a.b)", "a:m())", "(f()))", "a+f())", "a..b)", "a and b)", "not a)",
        "{...})",
    ] {
        for suffix in ["", "\n'bad", "\n0xZ"] {
            let source = format!("return f({args},{tail}{suffix}");
            for width in [0, 1, 3, 9] {
                oracle.check(&source, width, 0);
            }
        }
    }
}

#[test]
fn conditions_conflicts_and_nested_expressions_share_the_logical_stack() {
    let mut oracle = Oracle::new();
    for count in [190, 198, 199, 200] {
        let prefix = format!("local {};", names(count));
        for expr in [
            "a1",
            "f()",
            "a1 and f()",
            "a1 or f()",
            "not (a1 and f())",
            "a1<f()",
            "a1..a2..f()",
        ] {
            for body in [
                format!("if {expr} then end"),
                format!("while {expr} do break end"),
                format!("repeat until {expr}"),
            ] {
                oracle.check(&format!("{prefix}{body}"), 0, 0);
            }
        }
        for args in [40, 45, 46, 47, 48, 49] {
            oracle.check(
                &format!("{prefix}a1[a2],a2,a1=f({})", repeated("1", args)),
                0,
                0,
            );
        }
    }
    fn expression(seed: &mut u32, depth: u8) -> String {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let choice = *seed;
        if depth == 0 {
            return ["a", "1", "f()", "...", "nil", "false"][(choice as usize) % 6].into();
        }
        let left = expression(seed, depth - 1);
        match choice % 10 {
            0 => format!("({left}+{})", expression(seed, depth - 1)),
            1 => format!("({left} and {})", expression(seed, depth - 1)),
            2 => format!("({left} or {})", expression(seed, depth - 1)),
            3 => format!("({left}..{})", expression(seed, depth - 1)),
            4 => format!("({left}<{})", expression(seed, depth - 1)),
            5 => format!("not({left})"),
            6 => format!("{{{left}}}"),
            7 => format!("a[{left}]"),
            8 => format!("f({left})"),
            _ => format!("a:m({left})"),
        }
    }
    let mut seed = 0x251c0de;
    for _ in 0..256 {
        let expr = expression(&mut seed, 3);
        for count in 241..=249 {
            oracle.check(&format!("return f({},{expr})", repeated("1", count)), 0, 0);
        }
    }
}

#[test]
fn local_stores_contribute_to_reference_constant_pool_boundaries() {
    let mut oracle = Oracle::new();
    for pool in [250, 254, 255, 256, 300] {
        let prefix = format!(
            "local x;{}local {};",
            (1..=pool).map(|n| format!("x={n};")).collect::<String>(),
            names(199)
        );
        for expr in ["a1[1]", "a1[true]", "{[1]=1}", "a1:m()"] {
            for count in 44..=49 {
                let source = format!("{prefix}return f({},{expr})", repeated("1", count));
                for width in [0, 7, 128] {
                    oracle.check(&source, width, 0);
                }
            }
        }
    }
}
