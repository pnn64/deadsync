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
        let name = b"=capture-limits";
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
fn assignments(count: usize) -> String {
    (1..=count).map(|i| format!("a{i}=1\n")).collect()
}
fn direct(count: usize, closed: bool) -> String {
    format!(
        "local {}\nlocal function f()\n{}{}",
        names(count),
        assignments(count),
        if closed { "end" } else { "" }
    )
}

#[test]
fn capture_boundaries_match_for_complete_and_unfinished_functions() {
    let mut oracle = Oracle::new();
    for count in [1, 58, 59, 60, 61, 62, 70] {
        for closed in [false, true] {
            oracle.check(&direct(count, closed), 0, 0);
        }
    }
    oracle.valid(&direct(60, true));
}

#[test]
fn descendant_captures_report_the_first_overflowing_ancestor() {
    let mut oracle = Oracle::new();
    for count in [8, 12, 59, 60, 61, 70] {
        let mut source = format!("function foo()\nlocal {},b\n", names(count));
        for i in 1..=count {
            source.push_str(&format!("function foo{i}()\na{i}=3\n"));
        }
        oracle.check(&source, 0, 0);
        // Deep successful code generation still uses the compiler's host stack.
        // Keep these debug-thread probes below that separate implementation limit.
        if count <= 12 || count > 60 {
            source.push_str(&"end\n".repeat(count + 1));
            oracle.check(&source, 0, 0);
        }
    }
}

#[test]
fn capture_checks_wait_for_the_next_lexical_token() {
    let mut oracle = Oracle::new();
    let prefix = format!(
        "local {}\nlocal function f()\n{}a61",
        names(61),
        assignments(60)
    );
    for tail in [
        "",
        "=",
        "\n=1",
        "\n'bad",
        "\n0xZ",
        "\n[==x",
        "\n--comment",
        "\n--[=[bad",
    ] {
        let source = format!("{prefix}{tail}");
        for width in [0, 1, 2, 3, 7, 31, 4096] {
            oracle.check(&source, width, 0);
        }
    }
}

#[test]
fn reader_failures_at_the_capture_boundary_keep_consumption_and_precedence() {
    let mut oracle = Oracle::new();
    let source = direct(61, false);
    for width in [1, 3, 7, 64] {
        let boundary = source.find("a61=1").unwrap();
        for fault in boundary / width..=source.len() / width + 3 {
            oracle.check(&source, width, fault);
        }
    }
}

#[test]
fn globals_fields_methods_and_record_keys_do_not_consume_captures() {
    let mut oracle = Oracle::new();
    let fields = (1..=70)
        .map(|i| format!("t.a{i}=t:a{i}({{a{i}=1}})\n"))
        .collect::<String>();
    oracle.valid(&format!(
        "local {}\nlocal t\nlocal function f()\n{fields}end",
        names(70)
    ));
    oracle.valid(&format!("local function f()\n{}end", assignments(70)));
}

#[test]
fn local_activation_and_shadowed_initializers_preserve_outer_bindings() {
    let mut oracle = Oracle::new();
    for count in [59, 60, 61] {
        let initializers = (1..=count)
            .map(|i| format!("do local a{i}=a{i} end\n"))
            .collect::<String>();
        oracle.check(
            &format!(
                "local {}\nlocal function f()\n{initializers}end",
                names(count)
            ),
            0,
            0,
        );
        let shadows = (1..=count)
            .map(|i| format!("do local a{i}=1;a{i}=2 end\n"))
            .collect::<String>();
        oracle.valid(&format!(
            "local {}\nlocal function f()\n{shadows}end",
            names(count)
        ));
    }
}

#[test]
fn loop_variables_parameters_method_self_and_vararg_arg_have_function_scope() {
    let mut oracle = Oracle::new();
    for count in [59, 60] {
        let decl = names(count);
        let refs = assignments(count);
        for (header, extra, footer) in [
            ("for i=1,2 do local function f()", "i=1", "end end"),
            (
                "for k,v in pairs({}) do local function f()",
                "k=1;v=2",
                "end end",
            ),
            (
                "local function outer(p) local function f()",
                "p=1",
                "end end",
            ),
            ("function t:m() local function f()", "self=1", "end end"),
            (
                "local function outer(...) local function f()",
                "arg=1",
                "end end",
            ),
            (
                "local function outer() local function f()",
                "f=1",
                "end end",
            ),
        ] {
            oracle.check(
                &format!("local {decl}\n{header}\n{refs}{extra}\n{footer}"),
                0,
                0,
            );
        }
    }
}

#[test]
fn repeat_body_locals_remain_visible_in_the_until_condition() {
    let mut oracle = Oracle::new();
    for count in [59, 60, 61] {
        let source = format!(
            "repeat local {} until (function()\n{}return true end)()",
            names(count),
            assignments(count)
        );
        oracle.check(&source, 0, 0);
        oracle.check(&source, 3, 0);
    }
}

#[test]
fn sequential_function_scopes_reuse_capture_storage_without_losing_parent_captures() {
    let mut oracle = Oracle::new();
    let mut source = format!("local {}\nlocal function outer()\na1=1\n", names(61));
    for _ in 0..100 {
        source.push_str(&format!(
            "do local function f()\n{}end end\n",
            assignments(60)
        ));
    }
    source.push_str("a60=1\nend");
    oracle.valid(&source);
    source.truncate(source.len() - 3);
    source.push_str("a61=1\n");
    oracle.check(&source, 37, 0);
}

#[test]
fn generated_lexical_layouts_preserve_capture_identity_and_error_order() {
    let mut oracle = Oracle::new();
    let mut seed = 0x51_ca_70_u32;
    for case in 0..256 {
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let mut source = format!("local {}\n", names(70));
        for level in 0..4 {
            source.push_str(&format!("local function f{level}(p)\n"));
            for _ in 0..next() % 6 {
                let index = next() % 70 + 1;
                source.push_str(&format!("local a{index}=p\n"));
            }
        }
        let mut order: Vec<_> = (1..=70).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, next() as usize % (i + 1));
        }
        for index in order.into_iter().take(58 + next() as usize % 13) {
            source.push_str(&format!("a{index}=a{index}\n"));
        }
        if case % 3 != 0 {
            source.push_str("end\nend\nend\nend\n");
        }
        oracle.check(&source, 0, 0);
        if case % 32 == 0 {
            oracle.check(&source, 7, 0);
        }
    }
}
