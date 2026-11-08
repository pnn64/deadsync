use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

struct Fixture {
    path: std::path::PathBuf,
    files: Vec<std::path::PathBuf>,
}
impl Fixture {
    fn new(files: &[(&str, &[u8])]) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/lua-package-fixtures")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        let mut paths = Vec::new();
        for (name, bytes) in files {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            paths.push(path);
        }
        Self {
            path: root,
            files: paths,
        }
    }
    fn script(&self, source: &str) -> Vec<u8> {
        source
            .replace(
                "@PATH@",
                &format!("{:?}", self.path.join("?.lua").to_string_lossy()),
            )
            .replace("@DIR@", &format!("{:?}", self.path.to_string_lossy()))
            .into_bytes()
    }
    fn compare(&self, source: &str) {
        compare(&self.script(source))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = std::fs::remove_file(file);
        }
        for file in &self.files {
            for parent in file.ancestors().skip(1).take_while(|p| *p != self.path) {
                let _ = std::fs::remove_dir(parent);
            }
        }
        let _ = std::fs::remove_dir(&self.path);
    }
}

#[test]
fn package_namespace_uses_shared_registry_and_native_environments() {
    compare(br#"
  return type(require),type(module),type(package.loadlib),type(package.seeall),#package.loaders,
   package.loaded==debug.getregistry()._LOADED,package.loaded.package==package,
   package.loaded._G==_G,require('math')==math,require('string')==string,require('package')==package,
   debug.getfenv(require)==package,debug.getfenv(module)==package,debug.getfenv(package.loaders[1])==package,
   getfenv(require)==_G,package.config,package.path,package.cpath
 "#);
}

#[test]
fn preloaded_modules_cache_truthy_results_and_retry_false_values() {
    compare(br#"
  local n=0;local object={};package.preload.x=function(name,...)assert(name=='x' and select('#',...)==0);n=n+1;return object,'ignored'end
  local a,b=require('x'),require('x');package.loaded.x=false;local c=require('x');package.loaded.x=nil;local d=require('x')
  package.preload.y=function()n=n+1;return false end;local e,f=require('y'),require('y')
  package.preload.z=function()end;local g=require('z');package.preload.n=function()package.loaded.n=23 end
  package.preload.p=function()package.loaded.p=nil end;package.preload.t=type
  return a==object,b==object,c==object,d==object,n,e,f,g,require('n'),require('p'),require('t'),select('#',require('x'))
 "#);
}

#[test]
fn recursive_and_failed_imports_keep_a_light_userdata_sentinel() {
    compare(br#"
  local tag={};local calls=0;package.preload.x=function()calls=calls+1;error(tag,0)end
  local ok,e=pcall(require,'x');local sentinel=package.loaded.x;local retry,why=pcall(require,'x')
  package.preload.y=function()return require('y')end;local loop,msg=pcall(require,'y')
  local weak=setmetatable({sentinel},{__mode='v'});collectgarbage();package.loaded.x=nil;package.preload.x=function()return 37 end
  return ok,e==tag,type(sentinel),debug.getmetatable(sentinel)==nil,debug.getfenv(sentinel)==nil,
   weak[1]==sentinel,retry,string.find(why,"loop or previous error loading module 'x'",1,true)~=nil,
   loop,string.find(msg,"loop or previous error loading module 'y'",1,true)~=nil,calls,require('x')
 "#);
}

#[test]
fn loaders_keep_order_snapshot_callable_values_and_error_accumulation() {
    compare(br#"
  local trace={};local old=package.loaders
  package.loaders={function(n)trace[#trace+1]='a'..n;return '\nfirst'end,
   setmetatable({},{__call=function(self,n)trace[#trace+1]='b'..n;package.loaders={};return 23 end}),
   function(n)trace[#trace+1]='c'..n;return false end}
  local ok,e=pcall(require,'x');package.loaders=old
  return ok,string.match(e,"module 'x' not found:(.*)"),table.concat(trace,','),package.loaded.x==nil
 "#);
    compare(br#"
  local calls=0;package.loaders=setmetatable({[1]=function()return nil end},{__index=function()calls=calls+1;error('unexpected')end})
  local ok,e=pcall(require,123);return ok,string.find(e,"module '123' not found:",1,true)~=nil,calls
 "#);
}

#[test]
fn lua_file_search_loads_named_chunks_and_passes_only_the_module_name() {
    let files=Fixture::new(&[("foo.lua",b"local name=...;return {name=name,n=select('#',...),source=debug.getinfo(1,'S').source}"),("bar.lua",b"local name=...;package.loaded[name]={value=37}"),("foo.foo.lua",b"return 41")]);
    files.compare(r#"
  package.path=@PATH@;package.cpath='';local a=require('foo');local b=require('foo');local c=require('bar')
  package.path=@DIR@..'/?.?.lua';package.loaded.foo=nil;local d=require('foo')
  return a==b,a.name,a.n,string.sub(a.source,1,1),string.find(a.source,'foo.lua',1,true)~=nil,c.value,d
 "#);
}

#[test]
fn file_search_errors_and_searcher_results_keep_messages_and_arity() {
    let files = Fixture::new(&[("bad.lua", b"return )")]);
    files.compare(
        r#"
  package.path=';;'..@PATH@..';;'..@DIR@..'/missing/?.lua;';package.cpath=''
  local s=package.loaders[2]('absent');local ok,e=pcall(require,'bad')
  local empty=select('#',package.loaders[4]('plain'))
  return s,ok,string.find(e,"error loading module 'bad' from file '",1,true)~=nil,
   string.find(e,'bad.lua:1:',1,true)~=nil,package.loaded.bad==nil,empty
 "#,
    );
}

#[test]
fn module_creates_dotted_globals_and_applies_options_after_switching_environment() {
    compare(br#"
  local original=_G;local mod=module;local see=package.seeall;local get=getfenv;local typ=type;local select=select
  local trace={};local function option(m)trace[#trace+1]=m._NAME;m.answer=37;assert(getfenv(3)==m)end
  local function f()local n=select('#',mod('one.two',see,option));return n,_M,_NAME,_PACKAGE,answer,_G==original end
  local n,m,name,prefix,value,global=f()
  return n,m==one.two,name,prefix,value,global,get(f)==m,package.loaded['one.two']==m,#trace,trace[1]
 "#);
    compare(
        br#"
  local original=_G;local f=function()module('isolated');return _M,_NAME,_PACKAGE,_G,print end
  local m,name,prefix,g,p=f();return m==isolated,name,prefix,g,p,package.loaded.isolated==m
 "#,
    );
}

#[test]
fn module_reuses_loaded_tables_preserves_initialization_and_reports_conflicts() {
    compare(br#"
  local m={_NAME=false,_PACKAGE='kept',_M=17};package.loaded.x=m
  local f=function()module('x');return _NAME,_PACKAGE,_M end;local a,b,c=f()
  local ok,e=pcall(module,'created');local exists=type(created)
  local g=function()module('clash.child')end;clash=false;local collision,why=pcall(g)
  return a,b,c,x==nil,getfenv(f)==m,ok,exists,string.find(e,"'module' not called from a Lua function",1,true)~=nil,
   collision,string.find(why,"name conflict for module 'clash.child'",1,true)~=nil
 "#);
}

#[test]
fn module_seeall_reuses_metatables_and_current_thread_globals() {
    compare(
        br#"
  local mt={};local t=setmetatable({},mt);local n=select('#',package.seeall(t));local original=_G
  local changed=setmetatable({x=37},{__index=original});setfenv(0,changed);package.seeall(t)
  return n,getmetatable(t)==mt,mt.__index==changed,t.x,pcall(package.seeall,17)
 "#,
    );
}

#[test]
fn registry_and_loader_metatables_preserve_callbacks_and_original_cache_table() {
    compare(br#"
  local loaded=package.loaded;local trace={};local tag={}
  setmetatable(loaded,{__index=function(t,k)trace[#trace+1]='get:'..k;if k=='cached' then return tag end end,
   __newindex=function(t,k,v)trace[#trace+1]='set:'..k..':'..type(v);collectgarbage();rawset(t,k,v)end})
  package.preload.x=function()package.loaded={x='different'};debug.getregistry()._LOADED={x='registry'};return 37 end
  local cached=require('cached')==tag;local x=require('x');local again=require('x')
  return cached,x,again,loaded.x,package.loaded.x,table.concat(trace,',')
 "#);
    compare(br#"
  local seen={};local p=setmetatable({},{__index=function(t,k)seen[#seen+1]=k;return function(n)collectgarbage();return n..'!' end end})
  package.preload=p;local a=require('abc');local old=require;debug.setfenv(old,{loaders={function(n)return function()return 17 end end}})
  return a,seen[1],old('different')
 "#);
}

#[test]
fn traditional_modules_load_from_files_with_shared_globals_and_identity() {
    let files = Fixture::new(&[
        (
            "tree/leaf.lua",
            b"module(...,package.seeall);count=(count or 0)+1;function answer()return count+36 end",
        ),
        (
            "tree/other.lua",
            b"local leaf=require('tree.leaf');return {leaf=leaf}",
        ),
    ]);
    files.compare(r#"
  package.path=@PATH@;package.cpath='';local a=require('tree.leaf');local b=require('tree.other')
  return a==tree.leaf,a==package.loaded['tree.leaf'],b.leaf==a,a._NAME,a._PACKAGE,a._M==a,a.answer(),a.count
 "#);
}

#[test]
fn module_initialization_and_options_follow_metamethod_side_effects() {
    compare(br#"
  local events={};local m=setmetatable({},{__index=function(t,k)events[#events+1]='get:'..k;return nil end,
   __newindex=function(t,k,v)events[#events+1]='set:'..k;collectgarbage();rawset(t,k,v)end})
  package.loaded.special=m;local f=function()module('special',function(t)events[#events+1]='option';assert(getfenv(3)==t)end)end
  f();return m._M==m,m._NAME,m._PACKAGE,table.concat(events,','),getfenv(f)==m
 "#);
    compare(br#"
  local original=_G;local events={};local mt=setmetatable({},{__newindex=function(t,k,v)events[#events+1]=k;rawset(t,k,v)end})
  local t=setmetatable({},mt);package.seeall(t);return mt.__index==original,events[1]
 "#);
}

#[test]
fn package_errors_keep_byte_names_nul_prefixes_and_argument_diagnostics() {
    compare(
        br#"
  local name='a\255';package.preload[name]=function()error('tag',0)end
  pcall(require,name);local ok,e=pcall(require,name)
  package.preload.x=function(n)return n end;local a=require('x\0ignored')
  local b,c=pcall(require,false);local d,f=pcall(module);local g,h=pcall(package.loadlib,'x',false)
  return ok,string.find(e,name,1,true)~=nil,a,b,c,d,f,g,h
 "#,
    );
}

#[test]
fn nested_imports_hooks_and_yield_boundaries_keep_faulting_stacks() {
    compare(br#"
  local events={};package.preload.inner=function(n)return n end
  package.preload.outer=function(n)collectgarbage();return require('inner')..n end
  debug.sethook(function(e)local f=debug.getinfo(2,'f').func;if f==require then events[#events+1]=e end end,'cr')
  local value=require('outer');debug.sethook()
  package.preload.yielding=function()return coroutine.yield('x')end
  local co=coroutine.create(function()return require('yielding')end);local ok,e=coroutine.resume(co)
  return value,table.concat(events,','),ok,string.find(e,'attempt to yield across metamethod/C-call boundary',1,true)~=nil,coroutine.status(co),type(package.loaded.yielding)
 "#);
}

#[test]
fn import_callbacks_expose_native_argument_slots() {
    compare(
        br#"
  local seen={};local function inspect(n)
    for i=1,5 do local k,v=debug.getlocal(2,i);seen[#seen+1]=tostring(k)..':'..type(v)end
    return function(name)
      for i=1,5 do local k,v=debug.getlocal(2,i);seen[#seen+1]=tostring(k)..':'..type(v)end
      return name
    end
  end
  package.loaders={inspect};local value=require('x',newproxy())
  return value,table.concat(seen,',')
 "#,
    );
}

#[test]
fn module_option_callbacks_expose_retained_native_values() {
    compare(
        br#"
  local out={};local function option(m)
    for i=1,6 do local k,v=debug.getlocal(2,i);out[#out+1]=tostring(k)..':'..type(v)end
  end
  local function f()module('slots',option)end;f();return table.concat(out,',')
 "#,
    );
}

#[test]
fn light_userdata_keeps_unmanaged_keys_weak_values_and_its_own_metatable() {
    compare(br#"
  local pointer=host_pointer;local weak=setmetatable({pointer},{__mode='kv'});local t={[pointer]=17}
  local mt={__index={answer=37},__eq=function()error('must not compare')end,__lt=function()error('must not order')end}
  debug.setmetatable(pointer,mt);local full=newproxy(true);debug.setmetatable(full,mt)
  collectgarbage();local eq=pointer==full;local ok,e=pcall(function()return pointer<full end)
  return type(pointer),pointer==host_pointer,weak[1]==pointer,t[pointer],pointer.answer,eq,ok,
    string.find(e,'attempt to compare',1,true)~=nil,debug.getfenv(pointer)==nil,tostring(pointer),
    type(host_null_pointer),not not host_null_pointer,pointer==host_null_pointer,host_null_pointer.answer,tostring(host_null_pointer)
 "#);
}

#[test]
fn deep_imports_use_reference_native_limits_and_preserve_error_handlers() {
    compare(br#"
  local depth=0;local function make(name)
    depth=depth+1;if depth<300 then return require('depth'..depth)end;return 0
  end
  package.preload=setmetatable({},{__index=function()return make end})
  local ok,e=pcall(require,'start');return ok,type(e),string.find(e,'C stack overflow',1,true)~=nil,depth
 "#);
    compare(br#"
  local tag={};package.preload.x=function()error(tag,0)end;local count=0
  local ok,e=xpcall(function()return require('x')end,function(e)count=count+1;assert(e==tag);collectgarbage();return 'handled'end)
  return ok,e,count,type(package.loaded.x)
 "#);
}

#[test]
fn module_options_preserve_errors_callable_tables_and_execution_order() {
    compare(br#"
  local tag={};local calls=0;local mod=module;local env
  local option=setmetatable({},{__call=function(self,m)calls=calls+1;env=m;collectgarbage();m.x=17 end})
  local function f()mod('options',option,function()calls=calls+1;error(tag,0)end,function()calls=999 end)end
  local ok,e=pcall(f);return ok,e==tag,calls,env==getfenv(f),options.x,options._NAME
 "#);
}

#[test]
fn c_searchers_report_missing_paths_and_native_open_status() {
    let files = Fixture::new(&[]);
    files.compare(r#"
  package.path='';package.cpath=@DIR@..'/?.missing';local out={}
  for _,name in ipairs({'x','a.b','-a.b'})do
    out[#out+1]=package.loaders[3](name);out[#out+1]=package.loaders[4](name) or 'none'
  end
  local ok,e=pcall(require,'a.b');return table.concat(out,'|'),ok,string.match(e,"module 'a.b' not found:(.*)")
 "#);
    let source = compile(
        b"local f,e,where=package.loadlib('anything','luaopen_anything');return f,type(e),where",
    )
    .unwrap();
    let mut vm = Vm::new();
    let results = vm.execute(&source).unwrap().to_vec();
    assert_eq!(results[0], Value::Nil);
    let Value::String(where_id) = results[2] else {
        panic!("native loader status")
    };
    let expected = if cfg!(all(
        feature = "native-c-api",
        any(windows, target_os = "linux")
    )) {
        b"open".as_slice()
    } else {
        b"absent".as_slice()
    };
    assert_eq!(vm.string_bytes(where_id).unwrap(), expected);
}

#[test]
fn import_quota_aborts_recover_and_keep_initialized_sentinels() {
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(100));
    let program =
        compile(b"package.preload.x=function()while true do end end;return require('x')").unwrap();
    assert_eq!(
        vm.execute(&program).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(None);
    let values=vm.execute(&compile(b"local ok,e=pcall(require,'x');return ok,type(package.loaded.x),coroutine.running()").unwrap()).unwrap().to_vec();
    assert_eq!(values[0], Value::Boolean(false));
    assert_eq!(values[2], Value::Nil);
    let Value::String(kind) = values[1] else {
        panic!("sentinel type")
    };
    assert_eq!(vm.string_bytes(kind).unwrap(), b"userdata");
    let program=compile(b"for i=1,250 do local n='transient'..i;package.preload[n]=function()return {}end;assert(type(require(n))=='table');package.loaded[n]=nil;package.preload[n]=nil end;return 42").unwrap();
    for _ in 0..8 {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(42.)]);
        let stats = vm.collect_garbage().unwrap();
        assert!(stats.programs < 10);
        assert!(stats.reserved_bytes < 2_000_000, "{}", stats.reserved_bytes);
    }
}

#[test]
fn file_import_abort_releases_reader_records_and_executable_handles() {
    // Stay in the file reader beyond the quota with either native buffer size.
    let body = format!("--[=[{}]=]\nreturn 42", "x".repeat(2 * 1024 * 1024));
    let files = Fixture::new(&[("long.lua", body.as_bytes())]);
    let program = compile(&files.script("package.path=@PATH@;return require('long')")).unwrap();
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(45));
    assert_eq!(
        vm.execute(&program).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    assert_eq!(vm.heap_stats().files, 3);
    std::fs::remove_file(files.path.join("long.lua")).unwrap();
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"return package.loaded.long,coroutine.running()").unwrap())
            .unwrap(),
        &[Value::Nil, Value::Nil]
    );
}

#[test]
fn module_path_environment_expansion_is_read_at_state_creation() {
    if let Ok(mode) = std::env::var("DEADLIB_LUA_PACKAGE_PATH_CHILD") {
        let source = b"return package.path,package.cpath,package.config";
        println!("OBS\t{:?}", observe(source, mode == "reference"));
        return;
    }
    let mut values = Vec::new();
    for engine in ["rust", "reference"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "module_path_environment_expansion_is_read_at_state_creation",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("DEADLIB_LUA_PACKAGE_PATH_CHILD", engine)
            .env("LUA_PATH", "before;;after;!;\u{1};tail")
            .env("LUA_CPATH", ";;x;;")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let at = stdout.find("OBS\t").unwrap();
        values.push(stdout[at..].lines().next().unwrap().to_owned());
    }
    assert_eq!(values[0], values[1]);
}

#[test]
fn unmanaged_identities_cross_vms_with_independent_metatables_and_gc_roots() {
    let mut one = Vm::new();
    let mut two = Vm::new();
    let pointer = Value::LightUserdata(0x1abc);
    one.set_global(b"p", pointer).unwrap();
    two.set_global(b"p", pointer).unwrap();
    one.execute(&compile(b"debug.setmetatable(p,{__index={answer=37}});weak=setmetatable({p},{__mode='v'});t={[p]=42};return nil").unwrap()).unwrap();
    one.collect_garbage().unwrap();
    assert_eq!(
        one.execute(&compile(b"return p.answer,t[p],weak[1]==p").unwrap())
            .unwrap(),
        &[Value::Number(37.), Value::Number(42.), Value::Boolean(true)]
    );
    assert_eq!(
        two.execute(&compile(b"return getmetatable(p),type(p)=='userdata'").unwrap())
            .unwrap(),
        &[Value::Nil, Value::Boolean(true)]
    );
    assert_eq!(one.heap_stats().userdata, 3);
}

#[test]
fn callback_local_mutation_does_not_alias_captured_names_or_module_copies() {
    compare(br#"
      package.loaders={function()debug.setlocal(2,5,{});return nil end}
      local ok,e=pcall(require,'missing');return ok,string.match(e,"module 'missing' not found:(.*)")
    "#);
    compare(
        br#"
  package.loaders={function(name)
    debug.setlocal(2,1,'changed');return function(n)return n end
  end}
  local value=require('original');return value,package.loaded.original,package.loaded.changed
 "#,
    );
    for slot in [5, 6] {
        compare(format!(r#"
   local received;local original;local replacement={{replaced=true}}
   local function first(m)original=m;debug.setlocal(2,{slot},replacement);collectgarbage()end
   local function second(m)received=m end
   local function f()module('mutopts',first,second)end;f()
   return received==original,received==replacement,getfenv(f)==original,package.loaded.mutopts==original
  "#).as_bytes());
    }
    compare(
        br#"
      local original;local name;local m=setmetatable({},{__newindex=function(t,k,v)
        if k=='_M' then debug.setlocal(2,1,'changed')end
        rawset(t,k,v)
      end});package.loaded.original=m
      local function f()module('original\0suffix',function(t)
        local key,value=debug.getlocal(2,1);name=value;original=t
      end)end;f()
      return name,m._NAME,m==original,package.loaded.original==m,package.loaded.changed
    "#,
    );
}

#[test]
fn package_return_hooks_keep_library_temporaries_and_writable_result_suffixes() {
    compare(br#"
local function pack(...)return {n=select('#',...),...}end
local function scalar(v)
 local t=type(v)
 if t=='string'or t=='number'or t=='boolean'or t=='nil'then return t..':'..tostring(v)end
 if t=='function'then local d=debug.getinfo(v,'Su');return t..':'..d.what..':'..d.nups end
 return t
end
local out={};local token={}
local function observe(fn,args)
 for mode=0,2 do
  local co=coroutine.create(function()return fn(unpack(args,1,args.n))end)
  debug.sethook(co,function(e)
   if e=='return'and debug.getinfo(2,'f').func==fn then
    local n=0
    for i=1,40 do local name,v=debug.getlocal(2,i);if not name then break end
     n=i;out[#out+1]=name..'='..scalar(v)
    end
    out[#out+1]='top='..n
    if mode~=0 then assert(debug.setlocal(2,mode==1 and 1 or n-1,'hook-token'))end
    collectgarbage()
   end
  end,'r')
  local r=pack(coroutine.resume(co))
  for i=1,r.n do out[#out+1]='result='..scalar(r[i])end
 end
end
package.loaded.cached=17;observe(require,pack('cached',token))
for _,value in ipairs{17,false}do
 package.preload.lookup=function()return value end
 for mode=1,3 do package.loaded.lookup=nil;observe(require,pack('lookup',token))end
end
observe(package.loaders[1],pack('missing',token))
observe(package.loaders[1],pack('lookup',token))
observe(module,pack('mod_return',function()end))
observe(package.seeall,pack({},token))
package.path='absent-?.lua';observe(package.loaders[2],pack('mod.missing',token))
package.cpath='absent-?.dll';observe(package.loaders[3],pack('mod.missing',token))
observe(package.loaders[4],pack('mod.missing',token));observe(package.loaders[4],pack('plain',token))
return table.concat(out,'|')
"#);
}

#[test]
fn import_metamethods_see_and_mutate_the_actual_live_c_window() {
    compare(
        br#"
local out={};local marker={};local backing={};local loaded={}
local function snapshot(event)
 local row={event}
 for i=1,20 do local k,v=debug.getlocal(3,i);if not k then break end
  row[#row+1]=type(v)..(v==marker and ':marker'or '')
 end
 out[#out+1]=table.concat(row,',')
end
setmetatable(loaded,{
 __index=function(t,k)snapshot('get');collectgarbage();return backing[k]end,
 __newindex=function(t,k,v)snapshot('set:'..type(v));collectgarbage();backing[k]=v end
})
debug.getregistry()._LOADED=loaded;package.loaded=loaded
package.loaders={function()snapshot('loader');assert(debug.setlocal(2,3,marker));return function()
 snapshot('module');return nil
end end}
local result=require('live')
return result,table.concat(out,'|')
"#,
    );
}

#[test]
fn consumed_import_results_stop_retaining_userdata_before_the_final_cache_lookup() {
    compare(
        br#"
local count=0;local finalized=false;local loaded={}
setmetatable(loaded,{
 __index=function()
  count=count+1
  if count==1 then return nil end
  collectgarbage();collectgarbage();return 17
 end,
 __newindex=function()end
})
debug.getregistry()._LOADED=loaded
package.preload.lifetime=function()
 local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end;return p
end
local result=require('lifetime')
return result,finalized,count
"#,
    );
}

#[test]
fn package_field_callbacks_expose_consumed_values_and_preserve_captured_names() {
    compare(
        br#"
local out={};local tag={};local old_preload=package.preload
local function scalar(v)
 if type(v)=='string'then return 'string:'..v end
 return type(v)
end
local function snapshot(event)
 local row={event}
 for i=1,20 do local key,value=debug.getlocal(3,i);if not key then break end
  row[#row+1]=scalar(value)
 end
 out[#out+1]=table.concat(row,',')
end
package.preload=setmetatable({},{__index=function()snapshot('preload');return 17 end})
assert(package.loaders[1]('lookup',tag)==17);package.preload=old_preload
local m=setmetatable({},{__index=function()snapshot('get-name')end,
 __newindex=function(t,k,v)snapshot('set-'..k);rawset(t,k,v)end})
package.loaded.module_live=m
local function f()module('module_live',function()snapshot('option')end)end
f()
local old_path,old_cpath=package.path,package.cpath
package.path=nil;package.cpath=nil
setmetatable(package,{__index=function(t,k)
 snapshot(k)
 assert(debug.setlocal(2,1,'changed-argument'))
 assert(debug.setlocal(2,3,'changed-name'))
 if k=='cpath'then assert(debug.setlocal(2,4,'changed-translation'))end
 return 'absent-?.missing'
end})
out[#out+1]=package.loaders[2]('original.name',tag)
out[#out+1]=package.loaders[4]('original.name',tag)
package.path=old_path;package.cpath=old_cpath;setmetatable(package,nil)
return table.concat(out,'|')
"#,
    );
}

#[test]
fn replaced_module_options_release_their_original_argument_roots() {
    compare(
        br#"
local finalized=false;local observed=false;local first
do
 local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end
 first=function()assert(p);assert(debug.setlocal(2,2,nil));first=nil end
end
local function second()collectgarbage();collectgarbage();observed=finalized end
local function f()module('option_lifetime',first,second)end
f();return observed,finalized
"#,
    );
}

#[test]
fn module_initialization_pops_the_name_lookup_before_options_collect() {
    compare(
        br#"
local finalized=false;local observed=false
local m=setmetatable({},{__index=function(t,k)
 if k=='_NAME'then
  local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end;return p
 end
end})
package.loaded.pop_name=m
local function f()module('pop_name',function()
 collectgarbage();collectgarbage();observed=finalized
end)end
f();return observed,finalized,getfenv(f)==m
"#,
    );
}

#[test]
fn seeall_metamethods_keep_the_original_metatable_and_pending_globals_on_the_stack() {
    compare(
        br#"
local target={};local old={};local replacement={};local seen={}
setmetatable(target,old)
setmetatable(old,{__newindex=function(t,k,v)
 for i=1,8 do local name,value=debug.getlocal(2,i);if not name then break end
  seen[#seen+1]=type(value)..':'..tostring(value==old)..':'..tostring(value==_G)
 end
 debug.setmetatable(target,replacement);collectgarbage()
end})
local retained
debug.sethook(function(e)
 if e=='return'and debug.getinfo(2,'f').func==package.seeall then
  local _,v=debug.getlocal(2,3);retained=v==old
 end
end,'r')
package.seeall(target,17);debug.sethook()
return table.concat(seen,'|'),retained,getmetatable(target)==replacement
"#,
    );
}

fn observe(source: &[u8], reference: bool) -> Vec<Scalar> {
    if !reference {
        let mut vm = Vm::new();
        vm.set_instruction_limit(Some(10_000_000));
        vm.set_output_capture(true);
        vm.set_global(b"host_pointer", Value::LightUserdata(0x1abc))
            .unwrap();
        vm.set_global(b"host_null_pointer", Value::LightUserdata(0))
            .unwrap();
        let program = compile(source).unwrap();
        let values = vm
            .execute(&program)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(source)))
            .to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => Scalar::Number(n.to_bits()),
                Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
                v => panic!(
                    "return scalar observations, got {v:?}: {}",
                    String::from_utf8_lossy(source)
                ),
            })
            .collect();
        return actual;
    }
    // SAFETY: fixed local programs need the development oracle's debug library.
    let lua = unsafe { Lua::unsafe_new() };
    lua.globals()
        .set(
            "host_pointer",
            lua_reference::LightUserData(std::ptr::without_provenance_mut(0x1abc)),
        )
        .unwrap();
    lua.globals()
        .set(
            "host_null_pointer",
            lua_reference::LightUserData(std::ptr::null_mut()),
        )
        .unwrap();
    // Use a plain protected entry with no host error-handler value below it.
    // The normal mlua eval wrapper leaves that C function on the base stack,
    // which Lua 5.1 can expose through negative/lost-tail local queries.
    let reference: MultiValue = lua.exec_raw_lua(|raw| unsafe {
        use lua_reference::{FromLuaMulti, ffi};
        let state = raw.state();
        let start = ffi::lua_gettop(state);
        let mut status = ffi::luaL_loadbuffer(
            state,
            source.as_ptr().cast(),
            source.len(),
            c"=(deadlib-lua)".as_ptr(),
        );
        if status == 0 {
            status = ffi::lua_pcall(state, 0, ffi::LUA_MULTRET, 0);
        }
        if status != 0 {
            let mut len = 0;
            let text = ffi::lua_tolstring(state, -1, &mut len);
            let message = if text.is_null() {
                "non-string error".into()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(text.cast(), len)).into_owned()
            };
            ffi::lua_settop(state, start);
            panic!("reference: {message}: {}", String::from_utf8_lossy(source));
        }
        assert_ne!(
            ffi::lua_checkstack(state, 32),
            0,
            "oracle result conversion stack"
        );
        let values = MultiValue::from_stack_multi(ffi::lua_gettop(state) - start, raw).unwrap();
        ffi::lua_settop(state, start);
        values
    });
    let expected: Vec<_> = reference
        .into_iter()
        .map(|v| match v {
            Ref::Nil => Scalar::Nil,
            Ref::Boolean(b) => Scalar::Boolean(b),
            Ref::Number(n) => Scalar::Number(n.to_bits()),
            Ref::Integer(n) => Scalar::Number((n as f64).to_bits()),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            _ => panic!("return reference scalar observations"),
        })
        .collect();
    expected
}

fn compare(source: &[u8]) {
    let actual = observe(source, false);
    let expected = observe(source, true);
    if actual != expected {
        for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
            if a != b && !matches!((a, b), (Scalar::Bytes(_), Scalar::Bytes(_))) {
                panic!("observation {index}: rust={a:?}, reference={b:?}");
            }
            if let (Scalar::Bytes(a), Scalar::Bytes(b)) = (a, b)
                && a != b
            {
                let at = a
                    .iter()
                    .zip(b)
                    .position(|(a, b)| a != b)
                    .unwrap_or(a.len().min(b.len()));
                panic!(
                    "observation {index}, bytes differ at {at}: rust={:?}, reference={:?}: {}",
                    String::from_utf8_lossy(&a[at.saturating_sub(40)..(at + 80).min(a.len())]),
                    String::from_utf8_lossy(&b[at.saturating_sub(40)..(at + 80).min(b.len())]),
                    String::from_utf8_lossy(source)
                );
            }
        }
        assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
    }
}
