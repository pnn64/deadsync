use deadlib_lua::{Error, ErrorKind, GcOperation, HostCall, Value, Vm, compile};
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "deadlua-native-file-{}-{}.lua",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
    fn bind(&self, vm: &mut Vm) {
        let path = vm.intern(self.0.to_str().unwrap().as_bytes());
        vm.set_global(b"path", path).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn loader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let path = stack.to_bytes(1)?.map(<[u8]>::to_vec);
    let count = stack.top();
    let status = stack.load_file(path.as_deref())?;
    assert_eq!(stack.top(), count + 1);
    stack.push_integer(status as isize)?;
    stack.insert(-2)?;
    stack.finish(2)
}
fn collecting_loader(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let path = stack.to_bytes(1)?.unwrap().to_vec();
    // Warm the parser/child frames while collection is stopped. Restart's
    // next allocation will then be the filename, inside the intended test phase.
    stack.gc(GcOperation::Stop)?;
    stack.load_buffer(b"", None)?;
    stack.set_top(stack.top() - 1)?;
    stack.gc(GcOperation::SetStepMultiplier(0))?;
    stack.gc(GcOperation::Restart)?;
    let status = stack.load_file(Some(&path))?;
    assert_eq!(stack.top(), 2);
    stack.push_integer(status as isize)?;
    stack.insert(-2)?;
    stack.finish(2)
}
fn install(vm: &mut Vm, callback: fn(&mut HostCall<'_>) -> Result<(), Error>) {
    let value = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", value).unwrap();
}
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}

#[test]
fn text_headers_long_inputs_and_source_names_share_the_native_loader() {
    for source in [
        b"return 42".to_vec(),
        b"#! /lua\r\nreturn 42".to_vec(),
        [b"#!".as_slice(), &vec![b'h'; 40_000], b"\nreturn 42"].concat(),
        [b"--long\n".repeat(2000), b"return 42".to_vec()].concat(),
    ] {
        let file = Fixture::new(&source);
        let mut vm = Vm::new();
        install(&mut vm, loader);
        file.bind(&mut vm);
        run(
            &mut vm,
            br#"
loadfile=function()error('must not invoke the global loader')end
local status,f=host(path..'\0ignored')
assert(status==0 and f()==42 and debug.getinfo(f,'S').source=='@'..path)
"#,
        );
        std::fs::remove_file(&file.0).unwrap();
    }
}

#[test]
fn binary_files_reopen_raw_and_initialize_imported_capture_cells() {
    let reference = lua_reference::Lua::new();
    let f: lua_reference::Function = reference
        .load("local a=17;return function()return a,42 end")
        .eval()
        .unwrap();
    let binary = f.dump(false);
    for prefix in [b"".as_slice(), b"#!/lua\r\n"] {
        let file = Fixture::new(&[prefix, &binary].concat());
        let mut vm = Vm::new();
        install(&mut vm, loader);
        file.bind(&mut vm);
        run(
            &mut vm,
            b"local s,f=host(path);local a,b=f();assert(s==0 and a==nil and b==42)",
        );
        std::fs::remove_file(&file.0).unwrap();
    }
}

#[test]
fn file_and_syntax_statuses_preserve_prefixes_and_skip_error_handlers() {
    let file = Fixture::new(b"#!/lua\nlocal =");
    let mut vm = Vm::new();
    install(&mut vm, loader);
    file.bind(&mut vm);
    run(
        &mut vm,
        br#"
local called=false
local ok,s,e=xpcall(function()return host(path)end,function()called=true end)
assert(ok and s==3 and not called and e:find(':2:',1,true))
ok,s,e=xpcall(function()return host(path..'.missing')end,function()called=true end)
assert(ok and s==6 and not called and e:find('cannot open ',1,true)==1)
assert(select(1,host(''))==6)
"#,
    );
    std::fs::remove_file(&file.0).unwrap();
}

#[test]
fn active_files_survive_nested_finalizer_collection_and_native_loads() {
    let file = Fixture::new(b"#!/lua\nreturn 42");
    let mut vm = Vm::new();
    install(&mut vm, collecting_loader);
    file.bind(&mut vm);
    run(
        &mut vm,
        br#"
collectgarbage('stop')
local finalized=false
do local p=newproxy(true);getmetatable(p).__gc=function()
 assert(debug.getinfo(2,'f').func==host)
 assert(select(2,debug.getlocal(2,1))==path)
 assert(select(2,debug.getlocal(2,2))=='@'..path)
 collectgarbage()
 local co=coroutine.create(function()local s,f=host(path);assert(s==0 and f()==42)end)
 assert(coroutine.resume(co));finalized=true
end end
local s,f=host(path);assert(s==0 and f()==42 and finalized)
"#,
    );
    std::fs::remove_file(&file.0).unwrap();
}

#[test]
fn finalizer_failures_return_statuses_and_quotas_close_files_before_escaping() {
    let file = Fixture::new(b"#!/lua\nreturn 42");
    let mut vm = Vm::new();
    install(&mut vm, collecting_loader);
    file.bind(&mut vm);
    run(&mut vm,br#"
collectgarbage('stop')
local marker,replacement={},{}
do local p=newproxy(true);getmetatable(p).__gc=function()error(marker)end end
local ok,s,e=xpcall(function()return host(path)end,function(e)assert(e==marker);return replacement end)
assert(ok and s==2 and e==replacement)
"#);
    std::fs::remove_file(&file.0).unwrap();
    std::fs::write(&file.0, b"#!/lua\nreturn 42").unwrap();
    vm.collect_garbage().unwrap();
    vm.set_instruction_limit(Some(1000));
    let error=vm.execute(&compile(b"collectgarbage('stop');do local p=newproxy(true);getmetatable(p).__gc=function()while true do end end end;host(path)").unwrap()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    std::fs::remove_file(&file.0).unwrap();
    vm.set_instruction_limit(None);
    assert_eq!(run(&mut vm, b"return 42"), [Value::Number(42.)]);
    vm.collect_garbage().unwrap();
}

#[test]
fn current_coroutine_globals_bind_the_compiled_file() {
    let file = Fixture::new(b"return answer");
    let mut vm = Vm::new();
    install(&mut vm, loader);
    file.bind(&mut vm);
    run(
        &mut vm,
        br#"
local co=coroutine.create(function()
 local old=getfenv(0);local env=setmetatable({answer=42},{__index=old})
 setfenv(0,env);local s,f=host(path);setfenv(0,old)
 assert(s==0 and getfenv(f)==env and f()==42)
end)
assert(coroutine.resume(co))
"#,
    );
}

#[test]
fn source_name_push_collection_precedes_open_and_is_unprotected() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let path = stack.to_bytes(1)?.unwrap().to_vec();
        stack.gc(GcOperation::SetStepMultiplier(0))?;
        stack.gc(GcOperation::Restart)?;
        stack.intern(b"pressure before the native filename push");
        let error = stack.load_file(Some(&path)).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Runtime);
        // Returning success cannot suppress the pending unprotected failure.
        Ok(())
    }
    let file = Fixture::new(b"return 42");
    let mut vm = Vm::new();
    install(&mut vm, callback);
    file.bind(&mut vm);
    run(
        &mut vm,
        br#"
collectgarbage('stop')
local marker={}
do local p=newproxy(true);getmetatable(p).__gc=function()
 assert(debug.getinfo(2,'f').func==host)
 assert(debug.getlocal(2,2)==nil)
 assert(os.remove(path));error(marker)
end end
local ok,e=pcall(host,path);assert(not ok and e==marker)
"#,
    );
    assert!(!file.0.exists());
}

#[test]
fn empty_path_errors_match_official_file_library_diagnostics() {
    let reference = lua_reference::Lua::new();
    let expected: lua_reference::LuaString = reference
        .load("local f,e=loadfile('');assert(f==nil);return e")
        .eval()
        .unwrap();
    let mut vm = Vm::new();
    install(&mut vm, loader);
    let values = run(&mut vm, b"local s,e=host('');assert(s==6);return e");
    let Value::String(id) = values[0] else {
        panic!("file error must be a string")
    };
    assert_eq!(vm.string_bytes(id).unwrap(), expected.as_bytes().as_ref());
    let values = run(&mut vm, b"local f,e=io.open('');assert(f==nil);return e");
    let expected: lua_reference::LuaString = reference
        .load("local f,e=io.open('');assert(f==nil);return e")
        .eval()
        .unwrap();
    let Value::String(id) = values[0] else {
        panic!("I/O error must be a string")
    };
    assert_eq!(vm.string_bytes(id).unwrap(), expected.as_bytes().as_ref());
}

#[test]
fn selected_file_capacity_failures_restore_both_stacks_and_release_named_files() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let path = s.to_bytes(1)?.unwrap().to_vec();
        let owner = s.thread_id();
        let child = s.new_thread()?;
        for _ in 0..7999 {
            s.thread_stack(child)?.push(Value::Number(17.))?;
        }
        assert!(s.load_file_thread(child, Some(&path)).is_err());
        assert_eq!(s.thread_id(), owner);
        assert_eq!(s.top(), 2);
        assert_eq!(s.thread_stack(child)?.top(), 7999);
        assert_eq!(s.thread_stack(child)?.value(7999), Some(Value::Number(17.)));
        s.thread_stack(child)?.set_top(0)?;
        let status = s.load_file_thread(child, Some(&path))?;
        assert_eq!(status, deadlib_lua::NativeLoadStatus::Ok);
        assert_eq!(s.thread_stack(child)?.top(), 1);
        s.move_thread_values(child, owner, 1)?;
        s.call(0, Some(1))?;
        assert_eq!(s.value(-1), Some(Value::Number(42.)));
        assert_eq!(s.load_file_thread(owner, Some(&path))?, status);
        s.call(0, Some(1))?;
        s.finish(2)
    }
    let file = Fixture::new(b"return 42");
    let mut vm = Vm::new();
    install(&mut vm, callback);
    file.bind(&mut vm);
    assert_eq!(run(&mut vm, b"return host(path)"), [Value::Number(42.); 2]);
    std::fs::remove_file(&file.0).unwrap();
    run(&mut vm, b"collectgarbage();assert(17+29==46)");
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn read_errors_override_protected_finalizer_errors_and_release_directory_streams() {
    let file = Fixture::new(b"");
    let dir = file.0.with_extension("directory");
    std::fs::create_dir(&dir).unwrap();
    let mut vm = Vm::new();
    install(&mut vm, collecting_loader);
    let path = vm.intern(dir.to_str().unwrap().as_bytes());
    vm.set_global(b"path", path).unwrap();
    run(
        &mut vm,
        br#"
collectgarbage('stop')
local marker={};local handled=false
do local p=newproxy(true);getmetatable(p).__gc=function()error(marker)end end
local ok,s,e=xpcall(function()return host(path)end,function(e)handled=e==marker;return e end)
assert(ok and s==6 and handled and e:find('cannot read ',1,true)==1)
"#,
    );
    std::fs::remove_dir(&dir).unwrap();
}
