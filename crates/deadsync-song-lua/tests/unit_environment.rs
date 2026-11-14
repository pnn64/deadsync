//! Original environment lookup comparison using the Rust loader.
use deadlib_lua::{Value, Vm};
use deadsync_song_lua::{SongLuaCompileContext, SongRuntime};
#[path = "support/baked_program.rs"]
mod api;

fn read(vm: &mut Vm, reader: Value, owner: Value, key: &str) -> Result<Value, String> {
    let key = api::string(vm, key);
    match vm.call(reader, &[owner, key]) {
        Ok(values) => {
            let value = values[0];
            vm.pin(value).unwrap();
            Ok(value)
        }
        Err(error) => {
            let text = error
                .value()
                .and_then(|value| match value {
                    Value::String(id) => vm
                        .string_bytes(id)
                        .map(|s| String::from_utf8_lossy(s).into_owned()),
                    _ => None,
                })
                .unwrap_or_else(|| error.to_string());
            // Native type diagnostics have no authored Lua source location.
            // Keep the cause; authored error values are compared in full.
            Err(if text.contains("environment target: table expected") {
                "environment target: table expected".into()
            } else {
                text
            })
        }
    }
}
fn global_number(vm: &mut Vm, name: &[u8]) -> i32 {
    api::as_number(vm.get_global(name).unwrap()) as i32
}

#[test]
fn chunk_environment_reads_preserve_rebinding_metamethods_false_and_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("environment.lua");
    std::fs::write(&path, b"return __songlua_current_chunk_env").unwrap();
    let mut host = SongRuntime::new(&SongLuaCompileContext::new(
        dir.path(),
        "environment lookup unit",
    ))
    .unwrap();
    let current = host.execute_file(&path, &[]).unwrap()[0];
    host.vm_mut().pin(current).unwrap();
    let vm = host.vm_mut();
    let target = api::eval(
        vm,
        "return {present=42,disabled=false,object={},['function']=function() return 42 end}",
    )[0];
    let fallback = Value::Number(17.);
    vm.set_global(b"fallback", fallback).unwrap();
    api::set(vm, current, b"__songlua_env_target", target);
    let factory = api::eval(
        vm,
        r#"return function(target)
        local proxy={__songlua_env_target=target}
        return setmetatable(proxy,{__index=function(_,key)
            local target=proxy.__songlua_env_target
            if type(target)~='table' then error('environment target: table expected',0) end
            local value=target[key]
            if value~=nil then return value end
            return _G[key]
        end})
    end"#,
    )[0];
    let original = api::call_one(vm, factory, &[target]);
    let reader = api::eval(vm, "return function(owner,key) return owner[key] end")[0];
    for key in [
        "present", "disabled", "fallback", "missing", "object", "function",
    ] {
        assert_eq!(
            read(vm, reader, original, key).unwrap(),
            read(vm, reader, current, key).unwrap()
        );
    }
    let replacement = api::eval(vm, "return {present=84}")[0];
    for proxy in [original, current] {
        api::set(vm, proxy, b"__songlua_env_target", replacement);
    }
    assert_eq!(
        api::as_number(read(vm, reader, current, "present").unwrap()) as i32,
        84
    );
    let metamethod = api::eval(vm, ORIGINAL_ENV_METAMETHOD)[0];
    let mt = api::table(vm);
    api::set(vm, mt, b"__index", metamethod);
    api::set_metatable(vm, replacement, mt);
    for key in ["disabled", "fallback", "missing", "fault"] {
        vm.set_global(b"LOOKUPS", Value::Number(0.)).unwrap();
        let expected = read(vm, reader, original, key);
        let lookups = global_number(vm, b"LOOKUPS");
        vm.set_global(b"LOOKUPS", Value::Number(0.)).unwrap();
        let actual = read(vm, reader, current, key);
        assert_eq!(actual, expected, "lookup {key}");
        assert_eq!(global_number(vm, b"LOOKUPS"), lookups);
    }
    let globals = vm.get_global(b"_G").unwrap();
    api::set_metatable(vm, globals, mt);
    for proxy in [original, current] {
        api::set(vm, proxy, b"__songlua_env_target", globals);
        vm.set_global(b"LOOKUPS", Value::Number(0.)).unwrap();
        assert_eq!(read(vm, reader, proxy, "missing").unwrap(), Value::Nil);
        assert_eq!(global_number(vm, b"LOOKUPS"), 2);
        api::set(vm, proxy, b"__songlua_env_target", Value::Boolean(false));
    }
    assert_eq!(
        read(vm, reader, original, "present").unwrap_err(),
        read(vm, reader, current, "present").unwrap_err()
    );
}

const ORIGINAL_ENV_METAMETHOD: &str = r#"return function(_, key)
            LOOKUPS = LOOKUPS + 1
            if key == 'disabled' then return false end
            if key == 'fault' then error('environment read fault') end
        end"#;

// Global reads made by a chunk's own code, through plain tables, inherited
// tables, a function handler and a broken target.
const ENVIRONMENT_READS: &str = r#"
local rawset, setmetatable, pcall, type, tostring, table = rawset, setmetatable, pcall, type, tostring, table
local out = {}
local function note(label, value)
    out[#out + 1] = label .. '=' .. type(value) .. ':' .. tostring(value)
end
local function failure(label, read)
    out[#out + 1] = label .. '=' .. tostring((pcall(read)))
end
local proxy = __songlua_current_chunk_env
note('builtin', math.pi); note('missing', missing_name); note('boolean key', proxy[true])
assigned = 5; switched = false
note('assigned', assigned); note('switched', switched)
local target = {present = 42, disabled = false, math = 'shadow'}
rawset(proxy, '__songlua_env_target', target)
note('present', present); note('disabled', disabled); note('shadowed', math)
note('fallback', string.len('four')); note('absent', assigned)
setmetatable(target, {__index = {inherited = 7, disabled = true}})
note('inherited', inherited); note('own before inherited', disabled)
note('still fallback', string.len('four'))
local calls = 0
setmetatable(target, {__index = function(_, key)
    calls = calls + 1
    if key == 'computed' then return 'made' end
    if key == 'fault' then error('fault') end
end})
note('computed', computed); note('through function', string.len('four')); note('calls', calls)
failure('fault', function() return fault end)
rawset(proxy, '__songlua_env_target', false)
failure('no target', function() return present end)
return table.concat(out, '\n')
"#;

fn environment_reads(limit: bool) -> String {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reads.lua");
    std::fs::write(&path, ENVIRONMENT_READS).unwrap();
    let mut host = SongRuntime::new(&SongLuaCompileContext::new(
        dir.path(),
        "environment read resolution",
    ))
    .unwrap();
    if limit {
        // An instruction limit sends every read to the handler.
        host.vm_mut().set_instruction_limit(Some(u64::MAX));
    }
    let Value::String(text) = host.execute_file(&path, &[]).unwrap()[0] else {
        panic!("environment observations")
    };
    String::from_utf8(host.vm_mut().string_bytes(text).unwrap().to_vec()).unwrap()
}

#[test]
fn inline_environment_reads_match_the_handler() {
    let inline = environment_reads(false);
    let handler = environment_reads(true);
    assert_eq!(inline, handler);
    // Spot checks keep the comparison from passing on two empty or broken runs.
    for expected in [
        "missing=nil:nil",
        "boolean key=nil:nil",
        "assigned=number:5",
        "switched=boolean:false",
        "present=number:42",
        "disabled=boolean:false",
        "shadowed=string:shadow",
        "fallback=number:4",
        "inherited=number:7",
        "own before inherited=boolean:false",
        "still fallback=number:4",
        "computed=string:made",
        "through function=number:4",
        "calls=number:2",
        "fault=false",
        "no target=false",
    ] {
        assert!(
            inline.lines().any(|line| line == expected),
            "missing {expected}\n{inline}"
        );
    }
}
