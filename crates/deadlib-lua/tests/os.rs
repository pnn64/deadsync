use deadlib_lua::{Value, Vm, compile};

#[cfg(target_vendor = "apple")]
type ClockTicks = std::ffi::c_ulong;
#[cfg(not(target_vendor = "apple"))]
type ClockTicks = std::ffi::c_long;
unsafe extern "C" {
    #[link_name = "clock"]
    fn system_clock_ticks() -> ClockTicks;
}

fn system_clock() -> f64 {
    let divisor = if cfg!(windows) { 1000.0 } else { 1_000_000.0 };
    // SAFETY: the system clock has no parameters or borrowed memory. Reading it
    // independently checks the Lua builtin's scale, epoch and result placement.
    unsafe { system_clock_ticks() as f64 / divisor }
}

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

fn observe(source: &[u8]) -> Vec<Scalar> {
    let mut vm = Vm::new();
    let program = compile(source).unwrap();
    return vm
        .execute(&program)
        .unwrap()
        .to_vec()
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Boolean(b),
            Value::Number(n) => Scalar::Number(n.to_bits()),
            Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            v => panic!("expected a scalar, got {v:?}"),
        })
        .collect();
}

fn compare(source: &[u8]) {
    let actual = observe(source);
    reference_test::assert_debug("tests/os.rs::observe::expected", &actual);
}

#[test]
fn time_differences_convert_inputs_before_subtracting_and_accept_default_start() {
    compare(
        br#"
        local numbers={0,-0,1,-1,.9,-.9,1.9,-1.9,2147483648,4503599627370496,
          9007199254740991,9007199254740992,9007199254740994,
          9223372036854774784,9223372036854775808,-9223372036854775808,
          0/0,math.huge,-math.huge,'3.9',' 42 ','0x20'}
        local out={}
        for _,a in ipairs(numbers) do
          out[#out+1]=os.difftime(a);out[#out+1]=os.difftime(a,nil)
          for _,b in ipairs(numbers) do out[#out+1]=os.difftime(a,b) end
        end
        return unpack(out)
    "#,
    );
}

#[test]
fn os_argument_errors_preserve_names_validation_order_and_missing_values() {
    compare(
        br#"
        local out={}
        local function save(f,...)local ok,why=pcall(f,...);out[#out+1]=ok;out[#out+1]=why end
        save(os.difftime);save(os.difftime,nil);save(os.difftime,false,{})
        save(os.difftime,1,{});save(os.difftime,{},false);save(os.difftime,'bad')
        save(os.getenv);save(os.getenv,nil);save(os.getenv,{});save(os.getenv,false)
        save(os.remove);save(os.remove,nil);save(os.remove,{});save(os.remove,false)
        local t={get=os.getenv,remove=os.remove,diff=os.difftime}
        save(function()return t:get()end);save(function()return t:remove()end)
        save(function()return t:diff(1)end)
        return unpack(out)
    "#,
    );
}

#[test]
fn process_argument_errors_and_aliases_use_lua51_types_and_native_environments() {
    compare(
        br#"
        local out={}
        local function save(f,...)
          local ok,why=pcall(f,...);out[#out+1]=ok;out[#out+1]=why
        end
        save(os.execute,false);save(os.execute,{});save(os.exit,false);save(os.exit,{})
        local t={execute=os.execute,exit=os.exit}
        save(function()return t:execute()end);save(function()return t:exit()end)
        local env={};debug.setfenv(os.execute,env)
        collectgarbage();out[#out+1]=getfenv(os.execute)==env
        return unpack(out)
    "#,
    );
}

#[test]
fn environment_names_use_c_string_boundaries_numeric_coercion_and_missing_nil() {
    let actual = observe(
        br#"
        return os.getenv('DEADSYNC_LUA_UNSET_7F189225')==nil,
          os.getenv('')==nil,os.getenv('=')==nil,os.getenv('PATH=ignored')==nil,
          os.getenv(1234567890123)==nil,
          os.getenv('PATH')==os.getenv('PATH\0ignored'),
          os.getenv('PATH'),os.getenv('TEMP'),os.getenv('TMP'),os.getenv('COMSPEC'),
          select('#',os.getenv('DEADSYNC_LUA_UNSET_7F189225'))
    "#,
    );
    let mut expected: Vec<_> = (0..6).map(|_| Scalar::Boolean(true)).collect();
    for name in ["PATH", "TEMP", "TMP", "COMSPEC"] {
        expected.push(match std::env::var_os(name) {
            Some(value) => Scalar::Bytes(value.to_string_lossy().as_bytes().to_vec()),
            None => Scalar::Nil,
        });
    }
    expected.push(Scalar::Number(1f64.to_bits()));
    assert_eq!(actual, expected);
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lua-os-fixtures");
        std::fs::create_dir_all(&base).unwrap();
        loop {
            let path = base.join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    reference_test::register_path(&path);
                    return Self(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned fixture: {error}"),
            }
        }
    }
    fn source(&self, source: &str) -> Vec<u8> {
        source
            .replace(
                "@PATH@",
                &format!("{:?}", self.0.join("file.dat").to_string_lossy()),
            )
            .replace("@DIR@", &format!("{:?}", self.0.to_string_lossy()))
            .into_bytes()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let result = std::fs::remove_dir_all(&self.0);
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}

#[test]
fn clock_returns_one_number_and_ignores_arguments_environments_and_method_self() {
    compare(
        br#"
        local clock=os.clock;local t={clock=clock};local env={};debug.setfenv(clock,env)
        local a=clock();local b=t:clock(false,{},function()end)
        return type(a),type(b),b>=a,select('#',clock()),getfenv(clock)==env,
          package.loaded.os==os,debug.getinfo(clock).what
    "#,
    );
}

#[test]
fn clock_readings_are_bracketed_by_the_official_clock_in_the_same_process() {
    let program = compile(b"return os.clock(false,nil,42)").unwrap();
    let mut vm = Vm::new();
    let mut same_tick = 0;
    for _ in 0..512 {
        let before = system_clock();
        let Value::Number(actual) = vm.execute(&program).unwrap()[0] else {
            panic!("clock result")
        };
        let after = system_clock();
        assert!(
            before <= actual && actual <= after,
            "reference bracket {before}..={after}, Rust {actual}"
        );
        if before.to_bits() == after.to_bits() {
            assert_eq!(actual.to_bits(), before.to_bits());
            same_tick += 1;
        }
    }
    if cfg!(windows) {
        assert!(
            same_tick > 0,
            "at least one exact stable millisecond tick comparison"
        );
    }
}

#[test]
#[cfg(windows)]
fn clock_epoch_precedes_vm_creation_and_is_shared_with_other_os_threads() {
    let start = system_clock();
    std::thread::sleep(std::time::Duration::from_millis(30));
    let before = system_clock();
    assert!(
        before - start >= 0.020,
        "the MSVC clock includes idle wall time"
    );
    let actual = std::thread::spawn(|| {
        let mut vm = Vm::new();
        let p = compile(b"return os.clock()").unwrap();
        let Value::Number(n) = vm.execute(&p).unwrap()[0] else {
            panic!("clock result")
        };
        n
    })
    .join()
    .unwrap();
    let after = system_clock();
    assert!(
        before <= actual && actual <= after,
        "clock epoch must not restart with a VM or thread"
    );
}

#[test]
fn rename_argument_errors_validate_from_before_to_and_keep_method_names() {
    compare(
        br#"
        local out={}
        local function save(...)local ok,e=pcall(os.rename,...);out[#out+1]=ok;out[#out+1]=e end
        save();save(nil);save(false,{});save({},false);save('from');save('from',nil)
        save('from',false);save(42,false);save(function()end,'to')
        local t={rename=os.rename};local ok,e=pcall(function()return t:rename('to')end)
        out[#out+1]=ok;out[#out+1]=e;return unpack(out)
    "#,
    );
}

#[test]
fn rename_preserves_data_result_counts_nul_boundaries_and_full_failure_tuples() {
    let fixture = Fixture::new();
    compare(&fixture.source(r#"
        local path=@PATH@;local other=path..'.other';local out={}
        local function save(...)out[#out+1]=select('#',...);for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end
        local f=assert(io.open(path,'wb'));f:write('before\0after');f:close()
        save(os.rename(path..'\0ignored',other..'\0ignored'));save(os.rename(path,other))
        f=assert(io.open(other,'rb'));out[#out+1]=f:read('*a');f:close()
        save(os.rename(other,other));save(os.rename(other,path));save(os.rename('',other))
        save(os.rename(path,other..'/missing'));save(os.rename(path..'.missing',other))
        local function attempt(target)
          local function finish(...)
            save(...)
            if (...)then
              local f=assert(io.open(target,'rb'));assert(f:read('*a')=='before\0after');f:close()
              assert(os.rename(target,path))
            end
          end
          finish(os.rename(path,target))
        end
        attempt(path..'*');attempt(path..'?')
        save(os.rename(path,path..string.rep('x',300)))
        save(os.rename(path..string.rep('x',300),other))
        assert(os.remove(path));return unpack(out)
    "#));
}

#[test]
fn rename_preserves_existing_destinations_and_open_handle_behavior() {
    let fixture = Fixture::new();
    compare(&fixture.source(r#"
        local path=@PATH@;local other=path..'.other';local out={}
        local function save(...)out[#out+1]=select('#',...);for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end
        local a=assert(io.open(path,'wb'));a:write('source');a:close()
        local b=assert(io.open(other,'wb'));b:write('target');b:close()
        save(os.rename(path,other));b=assert(io.open(other,'rb'));out[#out+1]=b:read('*a');b:close()
        a=io.open(path,'rb');if a then
          save(os.rename(path,path..'.held'));a:close()
          os.rename(path..'.held',path)
        end
        os.remove(path);assert(os.remove(other));return unpack(out)
    "#));
}

#[test]
fn rename_directories_and_case_changes_match_the_reference() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join("folder")).unwrap();
    compare(&fixture.source(r#"
        local folder=@DIR@..'/folder';local renamed=folder..'-other';local path=@PATH@;local out={}
        local function save(...)out[#out+1]=select('#',...);for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end
        save(os.rename(folder,renamed));save(os.rename(folder,renamed));save(os.rename(renamed,folder))
        local f=assert(io.open(path,'wb'));f:write('case');f:close()
        local upper=string.sub(path,1,-9)..'FILE.dat'
        save(os.rename(path,upper));f=assert(io.open(upper,'rb'));out[#out+1]=f:read('*a');f:close()
        save(os.rename(upper,path));assert(os.remove(path));return unpack(out)
    "#));
    assert!(fixture.0.join("folder").is_dir());
}

#[test]
#[cfg(windows)]
fn rename_and_exclusive_creation_cannot_overwrite_each_others_winner() {
    use std::io::Write;
    use std::sync::{Arc, Barrier};
    let fixture = Fixture::new();
    let from = fixture.0.join("source");
    let to = fixture.0.join("destination");
    let program = compile(b"return os.rename(from_path,to_path)").unwrap();
    let mut vm = Vm::new();
    let name = vm.intern(from.to_str().unwrap().as_bytes());
    vm.set_global(b"from_path", name).unwrap();
    let name = vm.intern(to.to_str().unwrap().as_bytes());
    vm.set_global(b"to_path", name).unwrap();
    for _round in 0..2 {
        for _ in 0..64 {
            std::fs::write(&from, b"source").unwrap();
            let gate = Arc::new(Barrier::new(2));
            let worker_gate = gate.clone();
            let target = to.clone();
            let worker = std::thread::spawn(move || {
                worker_gate.wait();
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)
                    .and_then(|mut f| f.write_all(b"target"))
            });
            gate.wait();
            let renamed = vm.execute(&program).unwrap()[0] == Value::Boolean(true);
            let created = worker.join().unwrap();
            match (renamed, created) {
                (true, Err(error)) => {
                    assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
                    assert_eq!(std::fs::read(&to).unwrap(), b"source");
                }
                (false, Ok(())) => assert_eq!(std::fs::read(&to).unwrap(), b"target"),
                result => panic!("rename/create race: {result:?}"),
            }
            let _ = std::fs::remove_file(&from);
            std::fs::remove_file(&to).unwrap();
        }
    }
}

#[test]
fn rename_keeps_complete_names_when_native_paths_need_more_than_stack_storage() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join("p".repeat(80))).unwrap();
    compare(&fixture.source(
        r#"
        local path=@DIR@..'/'..string.rep('p',80)..'/file.dat';local other=path..'.other'
        assert(#path>=128);local f=assert(io.open(path,'wb'));f:write('long path');f:close()
        local count=select('#',os.rename(path,other));f=assert(io.open(other,'rb'))
        local bytes=f:read('*a');f:close();assert(os.rename(other,path));assert(os.remove(path))
        return count,bytes
    "#,
    ));
}

#[test]
fn remove_preserves_success_count_c_string_paths_and_failure_tuples() {
    let fixture = Fixture::new();
    compare(&fixture.source(r#"
        local path=@PATH@;local out={}
        local function save(...)out[#out+1]=select('#',...);for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end
        local f=assert(io.open(path,'wb'));f:write('retained');f:close()
        save(os.remove(path..'\0ignored'));save(os.remove(path));save(os.remove(path..'.missing'))
        save(os.remove(''));save(os.remove('\0ignored'))
        return unpack(out)
    "#));
}

#[test]
fn windows_remove_does_not_remove_directories_or_open_files() {
    if !cfg!(windows) {
        return;
    }
    let fixture = Fixture::new();
    compare(&fixture.source(
        r#"
        local path=@PATH@;local dir=@DIR@
        local f=assert(io.open(path,'w+b'));f:write('retained');f:flush()
        local a,b,c=os.remove(path);f:seek('set',0);local bytes=f:read('*a');f:close()
        assert(os.remove(path));local d,e,g=os.remove(dir)
        return a,b,c,bytes,d,e,g
    "#,
    ));
}

#[test]
fn removal_reports_platform_errors_for_invalid_and_missing_components() {
    let fixture = Fixture::new();
    compare(&fixture.source(
        r#"
        local path=@PATH@;local out={}
        local paths={path..'.missing/leaf',path..'/leaf',path..'/',path..'*',path..'?',path..':' }
        for _,p in ipairs(paths) do
          local f=assert(io.open(path,'wb'));assert(f:write('original'));assert(f:close())
          local a,b,c=os.remove(p);out[#out+1]=a==nil;out[#out+1]=b or false;out[#out+1]=c or false
          f=io.open(path,'rb');out[#out+1]=f~=nil
          if f then
            out[#out+1]=f:read('*a');assert(f:close());assert(os.remove(path))
          end
        end
        return unpack(out)
    "#,
    ));
}

#[test]
fn temporary_names_are_unique_and_round_trip_through_files_and_gc() {
    compare(
        format!(
            r#"
        local names={{}};local out={{}}
        for i=1,16 do
          local path=os.tmpname(false,{{}});assert(type(path)=='string' and #path>0)
          assert(not names[path]);names[path]=true
          local f=io.open(path,'rb');out[#out+1]=(f~=nil)=={}
          if f then f:close()end
          f=assert(io.open(path,'wb'));assert(f:write('value\0'..i));assert(f:close())
          collectgarbage();f=assert(io.open(path,'rb'));out[#out+1]=f:read('*a')=='value\0'..i
          assert(f:close());out[#out+1]=os.remove(path)
        end
        return unpack(out)
    "#,
            if cfg!(windows) { "false" } else { "true" }
        )
        .as_bytes(),
    );
}

#[cfg(unix)]
#[test]
fn temporary_names_load_as_extensionless_modules() {
    compare(
        br#"
        local name=os.tmpname()
        local file=assert(io.open(name,'wb'))
        assert(file:write('return {value=42}'));assert(file:close())
        local old=package.path;package.path='?'
        local ok,value=pcall(require,name)
        package.path=old;package.loaded[name]=nil
        assert(os.remove(name))
        assert(ok,value)
        return value.value
    "#,
    );
}

#[test]
fn os_module_identity_and_native_environments_survive_registry_mutations() {
    compare(
        br#"
        local os=os;assert(require('os')==os and package.loaded.os==os)
        local f=os.getenv;local env={};debug.setfenv(f,env);collectgarbage()
        local same=getfenv(f)==env;local old=os.remove;os.remove=nil
        collectgarbage();local ok,why=pcall(old,false)
        return same,ok,why,type(f(1234567890123)),debug.getinfo(f).what
    "#,
    );
}

#[allow(dead_code)]
#[path = "../src/reference_test.rs"]
mod reference_test;
