//! Exercise actual disk entries across independently launched Rust processes.
use deadlib_lua::{CacheStatus, ProgramCache, Value, Vm, compile_named};
use std::{fs, path::PathBuf, process::Command};

#[test]
fn cache_file_child() {
    let Some(directory) = std::env::var_os("DEADLIB_CACHE_CHILD_DIR") else {
        return;
    };
    let source = fs::read(std::env::var_os("DEADLIB_CACHE_CHILD_SOURCE").unwrap()).unwrap();
    let cache = ProgramCache::new(directory);
    let expected = std::env::var("DEADLIB_CACHE_CHILD_STATUS").unwrap();
    if expected == "Error" {
        assert_eq!(
            cache
                .compile_named(&source, b"@persistent.lua")
                .unwrap_err(),
            compile_named(&source, b"@persistent.lua").unwrap_err()
        );
    } else {
        let cached = cache.compile_named(&source, b"@persistent.lua").unwrap();
        assert_eq!(
            cached.status,
            if expected == "Stored" {
                CacheStatus::Stored
            } else {
                CacheStatus::Hit
            }
        );
        let mut vm = Vm::new();
        assert_eq!(vm.execute(&cached.program).unwrap(), &[Value::Number(42.)]);
        vm.collect_garbage().unwrap();
        assert_eq!(vm.execute(&cached.program).unwrap(), &[Value::Number(42.)]);
    }
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let resolved = self.0.canonicalize().unwrap();
        assert_eq!(
            resolved.parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        for entry in fs::read_dir(&resolved).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            fs::remove_file(entry.path()).unwrap();
        }
        fs::remove_dir(resolved).unwrap();
    }
}

#[test]
fn persisted_source_and_native_programs_reload_in_new_processes() {
    let directory = Directory(std::env::temp_dir().join(format!(
            "deadlib-lua-persist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )));
    fs::create_dir(&directory.0).unwrap();
    let source=b"local x=9;local function f(a)return x+a end;local co=coroutine.create(function()coroutine.yield(f(1));collectgarbage();return f(33)end);assert(coroutine.resume(co));local ok,n=coroutine.resume(co);assert(ok);return n";
    let lua = lua_reference::Lua::new();
    let native = lua
        .load(source.as_slice())
        .set_name("@persisted-native.lua")
        .into_function()
        .unwrap()
        .dump(false);
    for (filename, bytes) in [
        ("text.lua", source.as_slice()),
        ("native.luac", native.as_slice()),
        ("unary.lua", b"local a,b=2,3;local value=not(a>b);assert(value);collectgarbage();return value and 42 or 0".as_slice()),
        ("boolean.lua", b"local a,b,c=2,3,false;local value=not((a<b)or c);collectgarbage();return value and 0 or 42".as_slice()),
        ("predicate.lua", b"local keep;local n=0;repeat local x=n;keep=function()return x end;n=n+1 until n>=3 or false;while n<6 and true do n=n+1 end;if n==6 then collectgarbage();return keep()+40 else return 0 end".as_slice()),
        ("rk-window.lua", b"local function f(a)local x=a+3;return x end;local seen=false;debug.sethook(function()if debug.getinfo(2,'f').func==f then local n,v=debug.getlocal(2,2);if n=='(*temporary)'and v==nil then seen=true end;collectgarbage()end end,'',1);local n=f(2);debug.sethook();assert(n==5 and seen);return 42".as_slice()),
        ("stack-clearing.lua", b"local visits,cleared=0,false;local function f()collectgarbage();local x=string.sub('abc',1,2);return x end;debug.sethook(function()if debug.getinfo(2,'f').func==f then visits=visits+1;if visits==1 then debug.setlocal(2,4,17)end;if visits==3 then local _,v=debug.getlocal(2,4);cleared=v==nil end end end,'',1);assert(f()=='ab');debug.sethook();assert(cleared);return 42".as_slice()),
        ("ldexp-validation.lua", b"local ok,e=pcall(math.ldexp,nil,nil);assert(not ok and e==\"bad argument #2 to '?' (number expected, got nil)\");assert(math.ldexp('1.5','2')==6);return 42".as_slice()),
        ("invalid.lua", b"local =".as_slice()),
    ] {
        let path = directory.0.join(filename);
        fs::write(&path, bytes).unwrap();
        for expected in if filename == "invalid.lua" {
            ["Error", "Error"]
        } else {
            ["Stored", "Hit"]
        } {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "cache_file_child", "--nocapture"])
                .env("DEADLIB_CACHE_CHILD_DIR", &directory.0)
                .env("DEADLIB_CACHE_CHILD_SOURCE", &path)
                .env("DEADLIB_CACHE_CHILD_STATUS", expected)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "child {filename} {expected}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    assert_eq!(
        fs::read_dir(&directory.0)
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|s| s == "dlc"))
            .count(),
        8
    );
}
