//! Persistent allocation rejection across public Rust VM construction.
use deadlib_lua::{Error, ErrorKind, Value, Vm, compile};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::probe;

fn rejected_construction(create: fn() -> Result<Vm, Error>, libraries: bool) {
    let (control, stats) = probe(0, create);
    let mut vm = control.unwrap();
    assert_eq!(vm.global(b"_VERSION") != Value::Nil, libraries);
    drop(vm);
    assert!(stats.attempts > 20);
    eprintln!("VM_CREATION_REJECTION_POINTS {}", stats.attempts);
    for nth in 1..=stats.attempts {
        eprintln!("reject from {nth}");
        let (result, failed) = probe(nth, create);
        let error = result.expect_err("constructor must report rejected allocation");
        assert!(failed.rejected > 0);
        assert_eq!(
            failed.allocated_bytes, failed.freed_bytes,
            "partial VM leaked at rejection {nth}"
        );
        assert_eq!(error.kind(), ErrorKind::Memory);
        assert_eq!(error.message_bytes(), b"not enough memory");
        assert_eq!(
            error.value(),
            None,
            "failed VM must not leak an object handle"
        );
        assert_eq!(error.line(), 0);
        let mut retry = create().unwrap();
        let program = compile(b"local a={answer=42}; return a.answer").unwrap();
        assert_eq!(retry.execute(&program).unwrap(), &[Value::Number(42.)]);
    }
}

#[test]
fn bare_state_recovers_from_every_initialization_allocation_failure() {
    rejected_construction(Vm::try_new_bare, false);
}

#[test]
fn libraries_recover_from_every_initialization_allocation_failure() {
    rejected_construction(Vm::try_new, true);
}

#[test]
fn creation_with_package_environment() {
    if std::env::var_os("DEADLIB_VM_CREATION_CHILD").is_some() {
        #[cfg(windows)]
        let defaults = [
            ".\\?.lua;!\\lua\\?.lua;!\\lua\\?\\init.lua;!\\?.lua;!\\?\\init.lua",
            "",
        ];
        #[cfg(not(windows))]
        let defaults = [
            "./?.lua;/usr/local/share/lua/5.1/?.lua;/usr/local/share/lua/5.1/?/init.lua;/usr/local/lib/lua/5.1/?.lua;/usr/local/lib/lua/5.1/?/init.lua",
            "",
        ];
        let mut vm = Vm::try_new().unwrap();
        let program = compile(b"return package.path, package.cpath").unwrap();
        let values = vm.execute(&program).unwrap().to_vec();
        for ((variable, default), value) in ["LUA_PATH", "LUA_CPATH"]
            .into_iter()
            .zip(defaults)
            .zip(values)
        {
            // LUA_CPATH is deliberately ignored: this engine supports Rust and
            // Lua preload functions and Lua files, but cannot load C modules.
            let expected = std::env::var_os(variable)
                .filter(|_| variable == "LUA_PATH")
                .map_or_else(
                    || default.to_owned(),
                    |s| {
                        s.to_string_lossy()
                            .replace(";;", ";\x01;")
                            .replace('\x01', default)
                    },
                );
            #[cfg(windows)]
            let expected = expected.replace(
                '!',
                &std::env::current_exe()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_string_lossy(),
            );
            let Value::String(id) = value else {
                panic!("expected package path")
            };
            assert_eq!(vm.string_bytes(id), Some(expected.as_bytes()));
        }
        drop(vm);
        rejected_construction(Vm::try_new, true);
        return;
    }
    let long = format!(
        "{};!/?;;\u{3bb}/\u{1f600}/?;\x01;end",
        "abcdefghij/".repeat(60)
    );
    let cases = vec![None, Some(std::ffi::OsString::from("")), Some(long.into())];
    #[cfg(unix)]
    let cases = {
        use std::os::unix::ffi::OsStringExt;
        let mut cases = cases;
        cases.push(Some(std::ffi::OsString::from_vec(
            b"invalid-\xff;;?/\xe2\x82".to_vec(),
        )));
        cases
    };
    #[cfg(windows)]
    let cases = {
        use std::os::windows::ffi::OsStringExt;
        let mut cases = cases;
        cases.push(Some(std::ffi::OsString::from_wide(&[
            105, 0xd800, 59, 59, 63,
        ])));
        cases
    };
    for value in cases {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "creation_with_package_environment",
                "--nocapture",
            ])
            .env("DEADLIB_VM_CREATION_CHILD", "1");
        for name in ["LUA_PATH", "LUA_CPATH"] {
            if let Some(value) = &value {
                command.env(name, value);
            } else {
                command.env_remove(name);
            }
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "child {:?}: {}\n{}",
            value,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for line in String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|s| s.starts_with("VM_CREATION_REJECTION_POINTS"))
        {
            eprintln!("environment {:?}: {line}", value.as_ref().map(|s| s.len()));
        }
    }
}

#[test]
fn fallible_state_has_working_standard_libraries_and_gc() {
    let mut vm = Vm::try_new().unwrap();
    let program = compile(br#"
        assert(_VERSION == 'Lua 5.1' and _G._G == _G)
        for _,name in ipairs{'math','string','table','coroutine','debug','io','os','package'} do
            assert(type(_G[name]) == 'table' and package.loaded[name] == _G[name])
        end
        assert(io.type(io.stdin)=='file' and io.type(io.stdout)=='file' and io.type(io.stderr)=='file')
        local co=coroutine.create(function() coroutine.yield(string.upper('ok')); return math.floor(4.9) end)
        local ok,s=coroutine.resume(co); assert(ok and s=='OK')
        collectgarbage('collect')
        local ok,n=coroutine.resume(co); assert(ok and n==4)
        return table.concat({'a','b'}, ':'), type(debug.getinfo(1)), type(os.clock())
    "#).unwrap();
    let values = vm.execute(&program).unwrap().to_vec();
    for (value, expected) in values
        .into_iter()
        .zip([b"a:b".as_slice(), b"table", b"number"])
    {
        let Value::String(id) = value else {
            panic!("expected string")
        };
        assert_eq!(vm.string_bytes(id), Some(expected));
    }
}
