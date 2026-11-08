use deadlib_lua::{Program, Value, Vm, compile};
use lua_reference::Lua;

const WRAPPER: &[u8]=br#"local f,e;local calls=0
if width==0 then f,e=loadstring(source,name)
elseif width<0 then f,e=loadfile(source)
else local at=0;f,e=load(function()calls=calls+1;if at>=#source then return nil end;local part=string.sub(source,at+1,at+width);at=at+width;return part end,name)end
return f~=nil,e,calls"#;

struct Oracle {
    vm: Vm,
    lua: Lua,
    wrapper: Program,
}
impl Oracle {
    fn new() -> Self {
        // SAFETY: fixed developer inputs use the unmodified official loader.
        let lua = unsafe { Lua::unsafe_new() };
        Self {
            vm: Vm::new(),
            lua,
            wrapper: compile(WRAPPER).unwrap(),
        }
    }
    fn check(&mut self, source: &[u8], name: &[u8]) {
        self.check_mode(source, name, 0);
    }
    fn check_mode(&mut self, source: &[u8], name: &[u8], width: i64) {
        self.vm
            .set_global(b"width", Value::Number(width as f64))
            .unwrap();
        self.lua.globals().set("width", width).unwrap();
        let source_value = self.vm.intern(source);
        let name_value = self.vm.intern(name);
        self.vm.set_global(b"source", source_value).unwrap();
        self.vm.set_global(b"name", name_value).unwrap();
        let values = self.vm.execute(&self.wrapper).unwrap().to_vec();
        let actual = match values.as_slice() {
            [Value::Boolean(ok), Value::Nil, Value::Number(calls)] => (*ok, None, *calls as i64),
            [Value::Boolean(ok), Value::String(id), Value::Number(calls)] => (
                *ok,
                Some(self.vm.string_bytes(*id).unwrap().to_vec()),
                *calls as i64,
            ),
            v => panic!("loader results: {v:?}"),
        };
        self.lua
            .globals()
            .set("source", self.lua.create_string(source).unwrap())
            .unwrap();
        self.lua
            .globals()
            .set("name", self.lua.create_string(name).unwrap())
            .unwrap();
        let (ok, error, calls): (bool, Option<lua_reference::LuaString>, i64) =
            self.lua.load(WRAPPER).eval().unwrap();
        let expected = (ok, error.map(|s| s.as_bytes().to_vec()), calls);
        if actual != expected {
            panic!(
                "source={source:?}, name={name:?}, width={width}, calls={}/{}\nRust: {:?}\nC: {:?}",
                actual.2,
                expected.2,
                actual.1.as_deref().map(String::from_utf8_lossy),
                expected.1.as_deref().map(String::from_utf8_lossy)
            );
        }
    }
    fn cases(&mut self, cases: &[&[u8]]) {
        for source in cases {
            self.check(source, b"=syntax");
        }
    }
}

#[test]
fn expected_tokens_and_expression_statement_errors_match_reference() {
    Oracle::new().cases(&[
        b"local",
        b"local = 1",
        b"local a,=1",
        b"local function ()end",
        b"function f(a,)end",
        b"function f(,)end",
        b"function f(...,a)end",
        b"a",
        b"a + b",
        b"(a)=3",
        b"a,(b)=3",
        b"f()=1",
        b"f(),x=1",
        b"a=f()=1",
        b"return ; ;",
        b";",
        b"if true end",
        b"while true end",
        b"for a do end",
        b"for i=1 do end",
        b"for a,b=1,2 do end",
        b"a:",
        b"a:x",
        b"a.x=",
        b"return )",
        b"return function()return ...end",
        b"break label",
    ]);
}

#[test]
fn closing_diagnostics_retain_opening_symbols_and_lines() {
    Oracle::new().cases(&[
        b"do",
        b"do\n",
        b"while true do\n",
        b"for i=1,3 do\n",
        b"for x in f do\n",
        b"if true then\n",
        b"if true then\nelse\n",
        b"repeat\n",
        b"function f()\n",
        b"local function f()\n",
        b"local function f\n()\n",
        b"return (1\n",
        b"f(1\n",
        b"local a={4\n\n",
        b"return t[1\n",
        b"return function(\na\n",
        b"return {a=1\n",
        b"if true then\nelseif false then\n",
    ]);
}

#[test]
fn quoted_strings_preserve_decoded_bytes_and_lexical_failure_context() {
    Oracle::new().cases(&[
        b"return '",
        b"return 'a",
        b"return 'a\n",
        b"return 'a\r\n",
        b"return 'a\\n\n",
        b"return 'a\\000b\n",
        b"return 'a\\256'",
        b"return '\\999'",
        b"return 'x\\25a' 17",
        b"return 'a\\n\\t' 'b'",
        b"local 'hello'",
        b"local 'a\\000b'",
        b"return 'a\\\r\nb' 17",
        b"return '\xff\n",
        b"local '\xff'",
    ]);
}

#[test]
fn long_string_delimiters_comments_and_newlines_match_reference() {
    Oracle::new().cases(&[
        b"return [=x",
        b"return [==x",
        b"return [[a",
        b"--[[x",
        b"return [[\n",
        b"return [=[a\r\nb]=] 17",
        b"local [=[\na\nb]=]",
        b"return [[a[[b]]c]]",
        b"--[[a[[b]]c]]",
        b"return [=[a[=[b]=]",
        b"return [[a\0b]] 17",
        b"local [[a\0b]]",
    ]);
}

#[test]
fn malformed_numbers_and_nearby_numeral_lexemes_match_reference() {
    Oracle::new().cases(&[
        b"return 1e",
        b"return 1e+",
        b"return 0x",
        b"return 123name",
        b"return .1e-",
        b"return 1..2",
        b"return 1.2.3",
        b"return 0x1G",
        b"local 17",
        b"return 1 2.000",
        b"return 1e+2 0xff",
        b"return 1E-2 0Xff",
        b"return 1e+\n",
        b"return 1e+\xff",
        b"return 1e\0abc",
        b"return 0x1p+1",
    ]);
}

#[test]
fn arbitrary_byte_tokens_preserve_control_labels_and_non_utf8_messages() {
    let mut oracle = Oracle::new();
    for byte in 0..=255u8 {
        for prefix in [b"return ".as_slice(), b"local a=1; ", b"local "] {
            let mut source = prefix.to_vec();
            source.push(byte);
            source.extend_from_slice(b" !");
            oracle.check(&source, b"=bytes");
        }
    }
}

#[test]
fn names_and_lookahead_keep_the_active_scanner_buffer() {
    Oracle::new().cases(&[
        b"return {a b}",
        b"return {a if}",
        b"return {a 17}",
        b"return {a 'b'}",
        b"return {a [=[x]=]}",
        b"return {a .}",
        b"return {a ..}",
        b"return {a [=x}",
        b"return {a=1 b=2}",
        b"return {a=1 true}",
        b"return {a=1\nlongname}",
        b"return 1\n[=[long\nstring]=]",
        b"function a(a)\nend end",
    ]);
}

#[test]
fn source_names_keep_lexer_truncation_and_binary_prefixes() {
    let mut oracle = Oracle::new();
    for length in [0, 1, 40, 60, 70, 79, 80, 81, 120] {
        for prefix in [b"".as_slice(), b"=", b"@"] {
            let mut name = prefix.to_vec();
            name.extend(std::iter::repeat_n(b'x', length));
            oracle.check(b"return '\xff\n", &name);
        }
    }
    oracle.check(b"return )", b"@file\0ignored");
    oracle.check(b"return )", b"=\xff");
}

#[test]
fn host_diagnostics_retain_raw_bytes_with_readable_utf8_display() {
    let error = compile(b"return \xff").unwrap_err();
    assert!(error.message_bytes().ends_with(b"'\xff'"));
    assert!(error.message().contains('\u{fffd}'));
    assert_eq!(error.line(), 1);
}

#[test]
fn generated_truncations_and_token_mutations_keep_full_diagnostics() {
    let mut oracle = Oracle::new();
    let programs=[
  b"local function f(a,...) if a then return {x=a,[a]=f(a-1,...)} else return 'x' end end;return f(3)".as_slice(),
  b"local t={};for k,v in pairs(t)do while v do repeat t[k]=v until true;break end end;return t.x",
  b"local x,y=1,2;function t:f(a) x,y=y,x;do local z=(a+x)*y;return z end end;return t:f{x=3}",
 ];
    for program in programs {
        for end in 0..=program.len() {
            oracle.check(&program[..end], b"=generated");
        }
        for at in 0..program.len() {
            let mut removed = program.to_vec();
            removed.remove(at);
            oracle.check(&removed, b"=generated");
            for byte in b"=,;(){}[]'\n\x00\xff" {
                let mut source = program.to_vec();
                source.insert(at, *byte);
                oracle.check(&source, b"=generated");
            }
        }
    }
}

#[test]
fn source_local_and_assignment_limits_identify_the_owning_function() {
    let mut oracle = Oracle::new();
    for count in [198, 199, 200, 201, 205] {
        let names = (0..count)
            .map(|n| format!("a{n}"))
            .collect::<Vec<_>>()
            .join(",");
        for prefix in ["", "return function()\n", "local function f()\n"] {
            let suffix = if prefix.is_empty() { "" } else { "\nend" };
            oracle.check(
                format!("{prefix}local {names}{suffix}").as_bytes(),
                b"=limits",
            );
        }
        oracle.check(format!("function f({names})end").as_bytes(), b"=limits");
        oracle.check(format!("function f({names},...)end").as_bytes(), b"=limits");
        oracle.check(
            format!("function t:f({names},...)end").as_bytes(),
            b"=limits",
        );
        oracle.check(format!("{names}=1").as_bytes(), b"=limits");
    }
}

#[test]
fn reader_chunk_boundaries_keep_diagnostic_bytes_and_request_counts() {
    let mut oracle = Oracle::new();
    for source in [
        b"local a={4\n\n".as_slice(),
        b"return '\xff\n",
        b"return 1e+!",
        b"return [[a[[b",
        b"return [=[a\n",
        b"local a=1; a(f\n(3))",
        b"return 'a\\256'",
        b"return {a b}",
        b"function f(a,.\0..)end",
        b"return 1\0abc",
        b"return (1\n",
        b"local 'a\\000b'",
    ] {
        for width in 1..=9 {
            oracle.check_mode(source, b"=reader", width);
        }
    }
    let names = (0..201)
        .map(|n| format!("a{n}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!("local {names}");
    for width in [1, 7, 29, 1000] {
        oracle.check_mode(source.as_bytes(), b"=reader", width);
    }
}

#[test]
fn long_token_word_scans_preserve_byte_lanes_and_diagnostic_lines() {
    let mut oracle = Oracle::new();
    for equals in [b"".as_slice(), b"="] {
        for comment in [false, true] {
            for byte in 0..=255 {
                for offset in 0..8 {
                    let mut source = if comment {
                        b"--[".to_vec()
                    } else {
                        b"return [".to_vec()
                    };
                    source.extend_from_slice(equals);
                    source.push(b'[');
                    source.extend(std::iter::repeat_n(b'x', 16 + offset));
                    source.push(byte);
                    source.extend_from_slice(b"y]\nzzzzzzzzzzzzzzzz]");
                    source.extend_from_slice(equals);
                    source.extend_from_slice(b"] @");
                    oracle.check(&source, b"=word-scan");
                }
            }
        }
    }
}

#[test]
fn file_diagnostics_preserve_binary_bytes_headers_and_text_line_numbers() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lua-syntax-fixtures");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!(
        "{}-{}.lua",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut oracle = Oracle::new();
    for source in [
        b"#!/usr/bin/lua\r\nlocal a={4\r\n\r\n".as_slice(),
        b"return '\xff\n",
        b"#header\nreturn 1e+",
        b"return [=[a\r\n",
        b"return 'a\\256'",
        b"return {a\n b}",
    ] {
        std::fs::write(&path, source).unwrap();
        oracle.check_mode(path.to_string_lossy().as_bytes(), b"unused", -1);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn embedded_nul_scanner_quirks_preserve_executable_values() {
    let mut vm = Vm::new();
    let lua = unsafe { Lua::unsafe_new() };
    let wrapper = compile(b"return assert(loadstring(source))()").unwrap();
    for source in [
        b"return 1\0abc".as_slice(),
        b"return 1\0+abc",
        b"return 1\0\0abc",
        b"return .\0.",
    ] {
        let value = vm.intern(source);
        vm.set_global(b"source", value).unwrap();
        let actual = vm.execute(&wrapper).unwrap().to_vec();
        lua.globals()
            .set("source", lua.create_string(source).unwrap())
            .unwrap();
        let expected: lua_reference::MultiValue = lua
            .load("return assert(loadstring(source))()")
            .eval()
            .unwrap();
        let expected: Vec<_> = expected
            .into_iter()
            .map(|v| match v {
                lua_reference::Value::Number(n) => Value::Number(n),
                lua_reference::Value::Integer(n) => Value::Number(n as f64),
                _ => panic!("scalar result"),
            })
            .collect();
        assert_eq!(actual, expected, "{source:?}");
    }
}

#[test]
fn reader_runtime_errors_precede_pending_lexical_faults() {
    let mut vm = Vm::new();
    let lua = unsafe { Lua::unsafe_new() };
    for prefix in ["return [[a[[", "return 1e", "return @", "return 'a\\256"] {
        let source = format!(
            r#"return pcall(function()local tag={{}};local calls=0;local f,e=load(function()calls=calls+1;if calls==1 then return {prefix:?} end;error(tag,0)end,'=pending');return f==nil,e==tag,calls end)"#
        );
        let values = vm
            .execute(&compile(source.as_bytes()).unwrap())
            .unwrap()
            .to_vec();
        let expected: (bool, bool, bool, i64) = lua.load(source.as_bytes()).eval().unwrap();
        assert_eq!(
            values,
            vec![
                Value::Boolean(expected.0),
                Value::Boolean(expected.1),
                Value::Boolean(expected.2),
                Value::Number(expected.3 as f64)
            ],
            "{prefix:?}"
        );
    }
}
