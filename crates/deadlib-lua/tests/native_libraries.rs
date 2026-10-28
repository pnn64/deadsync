use deadlib_lua::{
    Error, HostCall, NativeLibrary, NativeLibraryLoader, NativeSymbol, Value, Vm, compile,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Default)]
struct Events(Mutex<Vec<String>>);
#[derive(Debug)]
struct Loader(Arc<Events>);
#[derive(Debug)]
struct Library {
    events: Arc<Events>,
    handle: AtomicUsize,
}
impl NativeLibrary for Library {
    fn handle(&self) -> usize {
        self.handle.load(Ordering::SeqCst)
    }
    fn close(&self) {
        if self.handle.swap(0, Ordering::SeqCst) != 0 {
            self.events.0.lock().unwrap().push("close".into());
        }
    }
    fn symbol(self: Arc<Self>, name: &[u8]) -> Result<NativeSymbol, Box<[u8]>> {
        self.events
            .0
            .lock()
            .unwrap()
            .push(format!("symbol:{}", String::from_utf8_lossy(name)));
        if name == b"missing" || name == b"luaopen_root_missing" {
            return Err(b"missing symbol".as_slice().into());
        }
        Ok(NativeSymbol::with_data(function, (self, name.to_vec())))
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        self.close();
    }
}
impl NativeLibraryLoader for Loader {
    fn open(&self, path: &[u8]) -> Result<Arc<dyn NativeLibrary>, Box<[u8]>> {
        self.0
            .0
            .lock()
            .unwrap()
            .push(format!("open:{}", String::from_utf8_lossy(path)));
        if path == b"absent" {
            return Err(b"cannot open".as_slice().into());
        }
        Ok(Arc::new(Library {
            events: self.0.clone(),
            handle: AtomicUsize::new(17),
        }))
    }
}
fn function(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (owner, symbol) = call.function_data::<(Arc<Library>, Vec<u8>)>().unwrap();
    assert_ne!(owner.handle(), 0);
    let symbol = symbol.clone();
    let arg = call.arguments().first().copied().unwrap_or(Value::Nil);
    let mut stack = call.native_stack()?;
    if symbol == b"identity" {
        stack.push(arg)?;
    } else {
        let v = stack.intern(&symbol);
        stack.push(v)?;
    }
    stack.finish(1)
}
fn setup() -> (Vm, Arc<Events>) {
    let mut vm = Vm::new();
    let events = Arc::new(Events::default());
    vm.set_native_library_loader(Some(Arc::new(Loader(events.clone()))));
    (vm, events)
}
fn execute(vm: &mut Vm, source: &[u8]) {
    vm.execute(&compile(source).unwrap()).unwrap();
}

#[test]
fn loadlib_caches_handles_retries_failures_and_reopens_explicitly_closed_userdata() {
    let (mut vm, events) = setup();
    execute(&mut vm,br#"
local registry=debug.getregistry()
local f=assert(package.loadlib('library','identity'));assert(f(29)==29)
local again=assert(package.loadlib('library','identity'));assert(f~=again)
assert(debug.getfenv(f)==debug.getfenv(package.loadlib))
local ud=registry['LOADLIB: library'];assert(type(ud)=='userdata');library_userdata=ud
assert(getmetatable(ud)==registry._LOADLIB)
local value,why,where=package.loadlib('library','missing');assert(value==nil and why=='missing symbol'and where=='init')
value,why,where=package.loadlib('absent','identity');assert(value==nil and why=='cannot open'and where=='open')
value,why,where=package.loadlib('absent','identity');assert(value==nil and where=='open')
registry._LOADLIB.__gc(ud);registry._LOADLIB.__gc(ud)
local next=assert(package.loadlib('library','identity'));assert(next(42)==42 and registry['LOADLIB: library']==ud)
assert(package.loadlib('nul\0ignored','identity\0ignored')(57)==57)
"#);
    let Value::Userdata(ud) = vm.global(b"library_userdata") else {
        panic!("library userdata")
    };
    assert_eq!(vm.userdata_bytes(ud).unwrap(), 17usize.to_ne_bytes());
    let log = events.0.lock().unwrap().clone();
    assert_eq!(log.iter().filter(|s| *s == "open:library").count(), 2);
    assert_eq!(log.iter().filter(|s| *s == "open:absent").count(), 2);
    assert_eq!(log.iter().filter(|s| *s == "close").count(), 1);
    drop(vm);
    assert_eq!(
        events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| *s == "close")
            .count(),
        3
    );
}

#[test]
fn registry_callbacks_share_the_loader_frame_and_errors_leave_it_reusable() {
    let (mut vm, events) = setup();
    execute(
        &mut vm,
        br#"
local registry=debug.getregistry();local backing={};local tag={};local out={}
setmetatable(registry,{__index=function(t,k)
 if k:sub(1,9)=='LOADLIB: 'then
  assert(debug.getinfo(2,'f').func==package.loadlib)
  local name,value=debug.getlocal(2,3);assert(name and value==k)
  collectgarbage();return backing[k]
 end
end,__newindex=function(t,k,v)
 assert(debug.getinfo(2,'f').func==package.loadlib)
 if k=='LOADLIB: failing'then error(tag,0)end
 local _,ud=debug.getlocal(2,3);local _,key=debug.getlocal(2,4);local _,copy=debug.getlocal(2,5)
 assert(ud==v and key==k and copy==v);backing[k]=v;collectgarbage()
end})
local ok,e=pcall(package.loadlib,'failing','identity');assert(not ok and e==tag)
local f=assert(package.loadlib('good','identity'));assert(f(17)==17)
assert(package.loadlib('good','identity')(29)==29)
setmetatable(registry,nil)
"#,
    );
    assert_eq!(
        events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| *s == "open:good")
            .count(),
        1
    );
}

#[test]
fn c_searchers_use_symbol_names_croot_fallback_and_the_loader_environment() {
    let (mut vm, events) = setup();
    let path =
        std::env::temp_dir().join(format!("deadlua-native-library-{}.bin", std::process::id()));
    std::fs::write(&path, b"fixture").unwrap();
    let filename = vm.intern(path.to_string_lossy().as_bytes());
    vm.set_global(b"library_file", filename).unwrap();
    execute(&mut vm,br#"
package.path='';package.cpath=library_file
local f=assert(package.loaders[3]('prefix-mod.name'))
assert(debug.getfenv(f)==debug.getfenv(package.loaders[3]))
assert(f()=='luaopen_mod_name')
assert(require('prefix-mod.name')=='luaopen_mod_name')
assert(require('prefix-mod.name')=='luaopen_mod_name')
assert(package.loaders[4]('simple')==nil)
local root=assert(package.loaders[4]('root.leaf'));assert(root()=='luaopen_root_leaf')
local missing=package.loaders[4]('root.missing');assert(missing:match("no module 'root.missing' in file '",1,true))
"#);
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with("open:"))
            .count(),
        1
    );
}

#[test]
fn disabling_the_resolver_preserves_cached_libraries_and_rejects_new_opens() {
    let (mut vm, events) = setup();
    execute(
        &mut vm,
        b"assert(package.loadlib('cached','identity')(17)==17)",
    );
    vm.set_native_library_loader(None);
    execute(
        &mut vm,
        br#"
assert(package.loadlib('cached','identity')(29)==29)
local value,why,where=package.loadlib('new','identity')
assert(value==nil and why=='dynamic library loader is disabled'and where=='open')
"#,
    );
    assert_eq!(
        events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with("open:"))
            .count(),
        1
    );
}

#[test]
fn reentrant_and_aborted_registry_callbacks_leave_no_stale_library_jobs() {
    let (mut vm, events) = setup();
    execute(
        &mut vm,
        br#"
local r=debug.getregistry()
setmetatable(r,{__index=function(t,k)
 if k=='LOADLIB: outer'then
  local co=coroutine.create(function()return package.loadlib('child','identity')end)
  local ok,fn=coroutine.resume(co);assert(ok and fn(17)==17)
  assert(package.loadlib('inner','identity')(29)==29)
 end
end})
assert(package.loadlib('outer','identity')(42)==42)
setmetatable(r,{__index=function()while true do end end})
"#,
    );
    vm.set_instruction_limit(Some(100));
    let error = vm
        .execute(&compile(b"return package.loadlib('abort','identity')").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    vm.set_instruction_limit(Some(100_000));
    execute(&mut vm,b"setmetatable(debug.getregistry(),nil);assert(package.loadlib('after','identity')(57)==57)");
    assert_eq!(
        events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.starts_with("open:"))
            .count(),
        4
    );
}

#[test]
fn return_hooks_modify_the_real_loadlib_result_and_collected_cache_entries_close() {
    let (mut vm, events) = setup();
    execute(&mut vm,br#"
local registry=debug.getregistry();local seen=false
debug.sethook(function(e)
 if e=='return'and debug.getinfo(2,'f').func==package.loadlib then
  local n=0;for i=1,20 do if not debug.getlocal(2,i)then break end;n=i end
  local _,ud=debug.getlocal(2,3);assert(type(ud)=='userdata');assert(n==5) -- four C slots plus the Lua hook function
  assert(debug.setlocal(2,4,29));seen=true
 end
end,'r')
assert(package.loadlib('collect','identity')==29);debug.sethook();assert(seen)
registry['LOADLIB: collect']=nil;collectgarbage();collectgarbage()
"#);
    assert_eq!(
        events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|s| *s == "close")
            .count(),
        1
    );
}
