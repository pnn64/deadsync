use deadlib_lua::{ErrorKind, Value, compile};
use deadsync_song_lua::{SongLuaCompileContext, rust_engine::RustSongRuntime};
use std::{fs, path::Path};

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    String(Vec<u8>),
}

struct Pair {
    directory: tempfile::TempDir,
    rust: RustSongRuntime,
}

impl Pair {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        reference_test::register_path(directory.path());

        let context = SongLuaCompileContext::new(directory.path(), "Rust loader regression");
        let rust = RustSongRuntime::new(&context).unwrap();
        Self { directory, rust }
    }

    fn write(&self, path: &str, source: &str) {
        let path = self.directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }

    fn compare(&mut self, source: &str) -> Vec<Scalar> {
        let values = self
            .rust
            .vm_mut()
            .execute(&compile(source.as_bytes()).unwrap())
            .unwrap()
            .to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|value| match value {
                Value::Nil => Scalar::Nil,
                Value::Boolean(value) => Scalar::Boolean(value),
                Value::Number(value) => Scalar::Number(value.to_bits()),
                Value::String(value) => {
                    Scalar::String(self.rust.vm_mut().string_bytes(value).unwrap().to_vec())
                }
                value => panic!("non-scalar Rust observation {value:?}"),
            })
            .collect();
        reference_test::assert_debug("song/tests/rust_loader.rs::observations", &actual);
        actual
    }
}

#[test]
fn nested_files_keep_directories_paths_arguments_and_all_return_values() {
    let mut pair = Pair::new();
    pair.write("child.lua", "return 'wrong directory'");
    pair.write(
        "nested/child.lua",
        r#"
        local a, b = ...
        collectgarbage()
        return a + b, nil, __songlua_script_dir, __songlua_current_script_path, nil
    "#,
    );
    pair.write("nested/main.lua", r#"
        local dir, path, env = __songlua_script_dir, __songlua_current_script_path, __songlua_current_chunk_env
        local f, err = loadfile('child.lua')
        assert(f and err == nil and select('#', loadfile('child.lua')) == 2)
        local function observe(...) return select('#', ...), ... end
        local n, sum, empty, childDir, childPath, last = observe(f(...))
        assert(dir == __songlua_script_dir and path == __songlua_current_script_path and env == __songlua_current_chunk_env)
        return n, sum, empty, childDir == dir, childPath:match('child.lua$') ~= nil, last
    "#);
    pair.compare(
        r#"
        local f = assert(loadfile('nested/main.lua'))
        local n, sum, empty, dir, path, last = f(7, 8)
        return n, sum, empty, dir, path, last,
            __songlua_script_dir, __songlua_current_script_path, __songlua_current_chunk_env
    "#,
    );
}

#[test]
fn environment_retargeting_inheritance_and_compile_globals_match_legacy() {
    let mut pair = Pair::new();
    pair.write(
        "child.lua",
        "value = value + 1; mods = {value}; return value, _G.value",
    );
    pair.write(
        "main.lua",
        r#"
        local child = assert(loadfile('child.lua'))
        local before = value
        local result = child()
        return before, result, value, mods[1], _G.mods[1], _G.value
    "#,
    );
    pair.compare(
        r#"
        value = 3
        local env = {value = 20}
        local f = assert(loadfile('main.lua'))
        assert(setfenv(f, env) == f)
        local a,b,c,d,e,g = f()
        env.value = 40
        local a2,b2 = f()
        return a,b,c,d,e,g, a2,b2, env.value, _G.mods[1]
    "#,
    );
    pair.write(
        "numeric.lua",
        "local result = setfenv(1, replacement); private = private + 2; return result, private",
    );
    pair.compare(
        r#"
        replacement = {private = 9}
        local f = assert(loadfile('numeric.lua'))
        local result, privateValue = f()
        return result, privateValue, replacement.private, private
    "#,
    );
}

#[test]
fn initial_xero_environment_and_dofile_first_result_match_legacy() {
    let mut pair = Pair::new();
    pair.write(
        "template/std.lua",
        "answer = answer + 1; mods = {answer}; return answer, 'discarded', nil",
    );
    pair.compare(
        r#"
        answer = 1; xero = {answer = 10}
        local a = dofile('template/std.lua')
        return a, select('#', dofile('template/std.lua')), answer, xero.answer, mods[1]
    "#,
    );
    // loadfile inherits the caller's environment instead of the entry xero policy.
    pair.compare("return loadfile('template/std.lua')(), answer, xero.answer");
}

#[test]
fn cmd_preprocessing_loadstring_and_captured_primitives_match_legacy() {
    let mut pair = Pair::new();
    pair.write(
        "command.lua",
        r#"
        local number = ...
        local command = cmd(x, number; y, (function() return 12 end)())
        local actor = {x = function(self, n) self.a = n; return self end,
                       y = function(self, n) self.b = n; return self end}
        command(actor)
        return actor.a, actor.b, 'cmd(x, 99)', [[cmd(y, 42)]]
    "#,
    );
    pair.compare("return assert(loadfile('command.lua'))(7)");
    pair.compare(
        r#"
        local f = loadstring('return cmd(x, 14)', '@dynamic.lua')
        local actor = {x = function(self, value) self.value = value end}
        f()(actor)
        return actor.value, select('#', loadstring('return 1')), (pcall(loadstring, 'local ='))
    "#,
    );
    pair.compare(
        r#"
        local original = loadfile
        loadstring = function() error('replacement must not compile files') end
        setmetatable = function() error('replacement must not build loader proxies') end
        getfenv = function() error('replacement must not inspect loader callers') end
        return original('command.lua')(23)
    "#,
    );
}

#[test]
fn missing_invalid_and_failed_nested_files_restore_context_and_leave_session_usable() {
    let mut pair = Pair::new();
    pair.write("bad.lua", "local =");
    pair.write(
        "nested/fail.lua",
        "collectgarbage(); error({tag = 'failure'})",
    );
    pair.write("nested/main.lua", r#"
        local dir,path,env = __songlua_script_dir,__songlua_current_script_path,__songlua_current_chunk_env
        local ok = pcall(assert(loadfile('fail.lua')))
        return ok, dir == __songlua_script_dir, path == __songlua_current_script_path, env == __songlua_current_chunk_env
    "#);
    pair.compare(
        r#"
        local a,b = loadfile('missing.lua')
        local c,d = loadfile('bad.lua')
        local ok = pcall(loadfile, false)
        return a == nil, type(b), c == nil, type(d), ok, loadfile('nested/main.lua')()
    "#,
    );
    pair.compare(
        "return __songlua_script_dir, __songlua_current_script_path, __songlua_current_chunk_env",
    );
    pair.write("bad.lua", "return 42, nil");
    pair.compare("return loadfile('bad.lua')()");
}

#[test]
fn environment_metamethods_can_collect_reenter_and_replace_the_target() {
    let mut pair = Pair::new();
    pair.write(
        "probe.lua",
        "output = input + 1; mods = {output}; return output, fallback",
    );
    pair.compare(
        r#"
        fallback = 77
        local storage = {input = 9}
        local env = setmetatable({}, {
            __index = function(_, key) collectgarbage(); return storage[key] end,
            __newindex = function(_, key, value) collectgarbage(); storage[key] = value end,
        })
        local f = loadfile('probe.lua'); setfenv(f, env)
        local a,b = f()
        setfenv(f, {input = 30})
        local c,d = f()
        return a,b,c,d,storage.output,storage.mods[1],mods[1]
    "#,
    );
}

#[test]
fn rust_entry_arguments_and_returned_objects_survive_context_cleanup_collection() {
    let mut pair = Pair::new();
    pair.write("object.lua", "collectgarbage(); local value = ...; return {value = value}, nil, function() return value end, nil");
    let vm = pair.rust.vm_mut();
    vm.execute(
        &compile(
            br#"
        local g = _G
        setmetatable(g, {__index = function(_, key)
            if key == '__songlua_script_dir' then collectgarbage() end
        end})
    "#,
        )
        .unwrap(),
    )
    .unwrap();
    let object = Value::Table(vm.try_new_table().unwrap());
    let values = pair
        .rust
        .execute_file(Path::new("object.lua"), &[object])
        .unwrap()
        .to_vec();
    assert_eq!(values.len(), 4);
    assert_eq!(values[1], Value::Nil);
    assert_eq!(values[3], Value::Nil);
    let roots: Vec<_> = values
        .iter()
        .map(|&value| pair.rust.vm_mut().pin(value).unwrap())
        .collect();
    pair.rust
        .vm_mut()
        .execute(&compile(b"collectgarbage()").unwrap())
        .unwrap();
    assert_eq!(pair.rust.vm_mut().call(values[2], &[]).unwrap(), &[object]);
    for root in roots {
        pair.rust.vm_mut().unpin(root).unwrap();
    }
}

#[test]
fn instruction_limit_and_coroutine_yield_do_not_leak_file_context() {
    let mut pair = Pair::new();
    pair.write("yield.lua", "coroutine.yield('forbidden')");
    pair.compare(r#"
        local co = coroutine.create(function() return loadfile('yield.lua')() end)
        local ok = coroutine.resume(co)
        return ok, coroutine.status(co), __songlua_script_dir, __songlua_current_script_path, __songlua_current_chunk_env
    "#);
    pair.write("loop.lua", "while true do end");
    pair.rust.vm_mut().set_instruction_limit(Some(2000));
    let error = pair
        .rust
        .execute_file(Path::new("loop.lua"), &[])
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    pair.rust.vm_mut().set_instruction_limit(None);
    pair.compare(
        "return __songlua_script_dir, __songlua_current_script_path, __songlua_current_chunk_env",
    );
}

#[test]
fn unreachable_loaded_functions_are_collected_while_saved_functions_remain_usable() {
    let mut pair = Pair::new();
    pair.write("constant.lua", "return 42");
    pair.compare(
        r#"
        local weak = setmetatable({}, {__mode = 'v'})
        local saved = assert(loadfile('constant.lua'))
        for i = 1, 48 do weak[i] = assert(loadfile('constant.lua')) end
        collectgarbage(); collectgarbage()
        local alive = 0
        for _ in pairs(weak) do alive = alive + 1 end
        return alive, saved()
    "#,
    );
}

#[test]
fn scoped_context_and_error_values_survive_gc_during_restoration() {
    let mut pair = Pair::new();
    pair.write(
        "gc.lua",
        "local a = ...; local result = {argument = a}; weak[4] = result; return result, nil",
    );
    pair.write(
        "error.lua",
        "local err = {tag = 72}; weak[4] = err; error(err)",
    );
    pair.rust.vm_mut().execute(&compile(br#"
        weak = setmetatable({}, {__mode = 'v'})
        local storage = {}
        local keys = {'__songlua_script_dir','__songlua_current_script_path','__songlua_current_chunk_env'}
        for i,key in ipairs(keys) do local t = {}; weak[i] = t; storage[key] = t end
        setmetatable(_G, {
            __index = function(_,key) collectgarbage(); return storage[key] end,
            __newindex = function(_,key,value) storage[key] = value; collectgarbage() end,
        })
    "#).unwrap()).unwrap();
    let value = pair
        .rust
        .execute_file(
            &pair.directory.path().join("gc.lua"),
            &[Value::Number(37.0)],
        )
        .unwrap()[0];
    let root = pair.rust.vm_mut().pin(value).unwrap();
    assert_eq!(
        pair.rust
            .vm_mut()
            .execute(
                &compile(
                    br#"
        return weak[1] == __songlua_script_dir, weak[2] == __songlua_current_script_path,
            weak[3] == __songlua_current_chunk_env, weak[4].argument
    "#
                )
                .unwrap()
            )
            .unwrap(),
        &[
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Number(37.0)
        ]
    );
    pair.rust.vm_mut().unpin(root).unwrap();
    let error = pair
        .rust
        .execute_file(&pair.directory.path().join("error.lua"), &[])
        .unwrap_err();
    let value = error.value().unwrap();
    let root = pair.rust.vm_mut().pin(value).unwrap();
    pair.rust.vm_mut().set_global(b"caught", value).unwrap();
    assert_eq!(
        pair.rust
            .vm_mut()
            .execute(
                &compile(
                    br#"
        collectgarbage()
        return caught == weak[4], caught.tag, weak[1] == __songlua_script_dir,
            weak[2] == __songlua_current_script_path, weak[3] == __songlua_current_chunk_env
    "#
                )
                .unwrap()
            )
            .unwrap(),
        &[
            Value::Boolean(true),
            Value::Number(72.0),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true)
        ]
    );
    pair.rust.vm_mut().unpin(root).unwrap();
}

#[test]
fn path_precedence_unicode_names_and_directory_easing_fallback_match_legacy() {
    let mut pair = Pair::new();
    pair.write("dir/été.lua", "return 7");
    pair.write("fallback.lua", "return 19");
    fs::create_dir(pair.directory.path().join("easing.lua")).unwrap();
    pair.compare(r#"
        ease = {marker = 87}
        return loadfile('dir/été.lua')(), loadfile('easing.lua')() == ease, dofile('easing.lua').marker
    "#);
    pair.write("dir/main.lua", "return loadfile('fallback.lua')()");
    pair.compare("return loadfile('dir/main.lua')()");
    let absolute = pair
        .directory
        .path()
        .join("fallback.lua")
        .to_string_lossy()
        .replace('\\', "/");
    pair.compare(&format!("return loadfile([[  {absolute}  ]])()"));
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
