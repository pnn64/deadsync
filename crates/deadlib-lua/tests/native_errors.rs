use deadlib_lua::{Value, Vm, compile_named};
use lua_reference::Lua;

struct Oracle {
    vm: Vm,
    lua: Lua,
}
impl Oracle {
    fn new() -> Self {
        // SAFETY: fixed developer cases use only the official standard libraries.
        Self {
            vm: Vm::new(),
            lua: unsafe { Lua::unsafe_new() },
        }
    }
    fn check(&mut self, body: &str) {
        let source =
            format!("local ok,e=pcall(function() {body} end);return ok,not ok and e or nil");
        let program = compile_named(source.as_bytes(), b"=native-errors").unwrap();
        let actual = self.vm.execute(&program).unwrap().to_vec();
        let actual = match actual.as_slice() {
            [Value::Boolean(ok), Value::Nil] => (*ok, None),
            [Value::Boolean(ok), Value::String(id)] => {
                (*ok, Some(self.vm.string_bytes(*id).unwrap().to_vec()))
            }
            other => panic!("unexpected results for {body}: {other:?}"),
        };
        let (ok, error): (bool, Option<lua_reference::LuaString>) = self
            .lua
            .load(source.as_bytes())
            .set_name("=native-errors")
            .eval()
            .unwrap();
        let expected = (ok, error.map(|e| e.as_bytes().to_vec()));
        assert_eq!(
            actual,
            expected,
            "{body}\nRust: {:?}\nC: {:?}",
            actual.1.as_deref().map(String::from_utf8_lossy),
            expected.1.as_deref().map(String::from_utf8_lossy)
        );
    }
    fn cases(&mut self, cases: &[&str]) {
        for body in cases {
            self.check(body);
        }
    }
}

#[test]
fn native_names_aliases_methods_tail_calls_and_binary_field_names() {
    Oracle::new().cases(&[
        "math.sin(false)",
        "return math.sin('a')",
        "local f=math.sin;f(nil)",
        "f=math.sin;f(nil)",
        "local t={x=math.sin};t.x(false)",
        "local t={x=math.sin};t:x(3)",
        "local t={x=math.pow};t.x(2,false)",
        "return ('x'):sub(false)",
        "return ('x'):sub()",
        "local ok,e=pcall(math.sin,false);error(e,0)",
        "local f=math.sin;return(function()return f(false)end)()",
        "return (math.sin or math.cos)(false)",
        "return (math.sin and math.cos or math.pow)(false)",
        "local t={['\\255']=math.sin};t['\\255'](false)",
        "local t={['x\\000ignored']=math.sin};t['x\\000ignored'](false)",
    ]);
}

#[test]
fn generated_missing_nil_and_type_checks_across_native_libraries() {
    let mut oracle = Oracle::new();
    for function in [
        "math.sin",
        "math.pow",
        "string.sub",
        "string.rep",
        "string.byte",
        "string.char",
        "string.format",
        "string.find",
        "table.concat",
        "table.insert",
        "table.remove",
        "table.sort",
        "table.foreach",
        "table.foreachi",
        "debug.getinfo",
        "debug.getlocal",
        "debug.setlocal",
        "debug.getupvalue",
        "debug.setupvalue",
        "debug.sethook",
        "coroutine.create",
        "coroutine.wrap",
        "coroutine.status",
        "coroutine.resume",
        "rawget",
        "rawset",
        "next",
        "pairs",
        "ipairs",
        "unpack",
        "setmetatable",
        "getmetatable",
        "type",
        "tonumber",
        "select",
        "load",
        "loadstring",
        "newproxy",
        "io.open",
        "io.type",
    ] {
        for argument in [
            "",
            "nil",
            "false",
            "{}",
            "function()end",
            "math.sin",
            "coroutine.create(function()end)",
        ] {
            oracle.check(&format!("return {function}({argument})"));
        }
    }
}

#[test]
fn validation_order_options_and_argument_domains_match_reference() {
    Oracle::new().cases(&[
        "rawget(false)",
        "rawset(false)",
        "rawget({},nil)",
        "rawset({},nil)",
        "setmetatable(false)",
        "setmetatable({})",
        "debug.setmetatable()",
        "debug.setmetatable({})",
        "setmetatable(setmetatable({},{__metatable=true}),false)",
        "tonumber({},99)",
        "tonumber('a',99)",
        "math.random(0)",
        "math.random(3,2)",
        "string.char(256)",
        "string.format('%d',false)",
        "string.format('%d')",
        "table.sort({},false)",
        "table.foreach({},false)",
        "table.foreachi({},false)",
        "debug.getinfo(false,'?')",
        "debug.getinfo(1,'?')",
        "debug.getupvalue(false,false)",
        "debug.setupvalue(false,false)",
        "debug.sethook(false,'c')",
        "debug.sethook(function()end,'c',false)",
        "load(false,{})",
        "load(false)",
        "newproxy({})",
        "collectgarbage('invalid')",
        "collectgarbage('invalid\\255\\000tail')",
        "select(0)",
        "select(false)",
    ]);
}

#[test]
fn file_userdata_types_options_and_format_arguments_keep_native_context() {
    Oracle::new().cases(&[
        "getmetatable(io.stdin).__gc()","getmetatable(io.stdin).__tostring(false)",
        "io.close(false)","io.input(false)","io.output(false)","io.read(false)","io.read('bad')","io.read('*bad')",
        "local f=assert(io.tmpfile());local ok,e=pcall(function()f:read(false)end);f:close();error(e,0)",
        "local f=assert(io.tmpfile());local ok,e=pcall(function()f:seek('invalid')end);f:close();error(e,0)",
        "local f=assert(io.tmpfile());local ok,e=pcall(function()f:seek('set',false)end);f:close();error(e,0)",
        "local f=assert(io.tmpfile());local ok,e=pcall(function()f:setvbuf('invalid')end);f:close();error(e,0)",
    ]);
}

#[test]
fn environment_and_stack_argument_errors_preserve_validation_order() {
    Oracle::new().cases(&[
        "getfenv(-1)",
        "getfenv(999999)",
        "setfenv(-1,{})",
        "setfenv(999999,{})",
        "getfenv(false)",
        "setfenv(false,false)",
        "setfenv(false,{})",
        "debug.setfenv(false,nil)",
        "error('x',false)",
        "assert(false,{})",
        "select(-3,1)",
        "debug.setlocal(999999,nil)",
        "debug.getlocal(999999,nil)",
        "debug.getupvalue({},false)",
    ]);
}

#[test]
fn native_error_context_survives_dispatch_callbacks_and_hook_argument_mutation() {
    Oracle::new().cases(&[
        "local t={foreach=table.foreach};t:foreach(false)",
        "local t=setmetatable({},{__call=math.sin});return t()",
        "local t={f=setmetatable({},{__call=math.pow})};return t:f(1)",
        "local co=coroutine.create(function()return math.sin(false)end);local ok,e=coroutine.resume(co);error(e,0)",
        "local f=math.sin;debug.sethook(function()if debug.getinfo(2,'f').func==f then debug.setlocal(2,1,false)end end,'c');local ok,e=pcall(function()return f(1)end);debug.sethook();error(e,0)",
    ]);
}

#[test]
fn failed_named_file_operations_keep_the_caller_and_path_bytes() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/lua-native-error-fixtures");
    std::fs::create_dir_all(&base).unwrap();
    let directory = loop {
        let path = base.join(format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create fixture: {error}"),
        }
    };
    let missing = directory
        .join("missing.lua")
        .to_string_lossy()
        .replace('\\', "/");
    let nested = directory
        .join("absent/output.lua")
        .to_string_lossy()
        .replace('\\', "/");
    let mut oracle = Oracle::new();
    oracle.check(&format!("io.input({missing:?})"));
    oracle.check(&format!("io.output({nested:?})"));
    oracle.check(&format!("io.lines({missing:?})"));
    oracle.check(&format!("local f=io.lines;f({missing:?})"));
    std::fs::remove_dir(directory).unwrap();
}
