use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

fn observe(source: &[u8], reference: bool) -> Vec<Scalar> {
    if !reference {
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
    // SAFETY: fixed local compatibility probes use the development oracle's
    // standard libraries; they never load third-party native modules.
    let lua = unsafe { Lua::unsafe_new() };
    let values: MultiValue = lua.load(source).set_name("=(deadlib-lua)").eval().unwrap();
    values
        .into_iter()
        .map(|v| match v {
            Ref::Nil => Scalar::Nil,
            Ref::Boolean(b) => Scalar::Boolean(b),
            Ref::Number(n) => Scalar::Number(n.to_bits()),
            Ref::Integer(n) => Scalar::Number((n as f64).to_bits()),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            v => panic!("expected a reference scalar, got {v:?}"),
        })
        .collect()
}

fn compare(source: &[u8]) {
    let actual = observe(source, false);
    let expected = observe(source, true);
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        if let (Scalar::Bytes(actual), Scalar::Bytes(expected)) = (actual, expected) {
            assert_eq!(
                String::from_utf8_lossy(actual),
                String::from_utf8_lossy(expected),
                "observation {index}"
            );
        }
        assert_eq!(actual, expected, "observation {index}");
    }
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
    compare(
        br#"
        return os.getenv('DEADSYNC_LUA_UNSET_7F189225')==nil,
          os.getenv('')==nil,os.getenv('=')==nil,os.getenv('PATH=ignored')==nil,
          os.getenv(1234567890123)==nil,
          os.getenv('PATH')==os.getenv('PATH\0ignored'),
          os.getenv('PATH'),os.getenv('TEMP'),os.getenv('TMP'),os.getenv('COMSPEC'),
          select('#',os.getenv('DEADSYNC_LUA_UNSET_7F189225'))
    "#,
    );
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/lua-os-fixtures")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
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
        let _ = std::fs::remove_file(self.0.join("file.dat"));
        std::fs::remove_dir(&self.0).unwrap();
    }
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
        local f=assert(io.open(path,'wb'));f:close()
        local paths={path..'.missing/leaf',path..'/leaf',path..'/',path..'*',path..'?',path..':' }
        for _,p in ipairs(paths) do
          local a,b,c=os.remove(p);out[#out+1]=a==nil;out[#out+1]=b;out[#out+1]=c
        end
        assert(os.remove(path));return unpack(out)
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
