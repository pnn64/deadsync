use deadlib_lua::{Error, HostCall, Value, Vm, compile};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[test]
fn c_modules_have_a_permanent_deterministic_unsupported_result() {
    let mut vm = Vm::new();
    vm.execute(
        &compile(
            br#"
assert(#package.loaders==4 and package.cpath=='')
assert(debug.getregistry()._LOADLIB==nil)
local message='C modules are not supported by deadlib-lua'
local function check(...)
 assert(select('#',...)==3)
 local value,reason,status=...
 assert(value==nil and reason==message and status=='absent')
end
for _,path in ipairs{'missing.so','module.dll','',string.rep('x',20000),'nul\0ignored'}do
 check(package.loadlib(path,'luaopen_test'))
 check(package.loadlib(path,'*'))
end
check(package.loadlib(17,29))
assert(not pcall(package.loadlib,{},'symbol'))
assert(not pcall(package.loadlib,'file',false))
package.cpath=nil
setmetatable(package,{__index=function(_,k)error('unexpected package lookup: '..k)end})
for _,name in ipairs{'x','tree.leaf','-tree.leaf','nul\0name'}do
 assert(package.loaders[3](name)=='\n\t'..message)
 assert(package.loaders[4](name)=='\n\t'..message)
end
setmetatable(package,nil)
package.path=''
local ok,reason=pcall(require,'missing_c_module')
assert(not ok and reason:find(message,1,true))
assert(package.loaded.missing_c_module==nil)
package.preload.available=function()return 42 end
assert(require('available')==42)
collectgarbage()
check(package.loadlib('module.so','luaopen_test'))
assert(debug.getregistry()._LOADLIB==nil)
"#,
        )
        .unwrap(),
    )
    .unwrap();
}

struct Owner(Arc<AtomicUsize>);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn rust_preload_bindings_keep_captures_and_owned_data_until_collection() {
    fn module(call: &mut HostCall<'_>) -> Result<(), Error> {
        assert_eq!(
            call.function_data::<Owner>()
                .unwrap()
                .0
                .load(Ordering::SeqCst),
            0
        );
        assert_eq!(call.arguments().len(), 1);
        call.call(call.captures()[1], &[])?;
        call.push(call.captures()[0])
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let mut vm = Vm::new();
    let value = vm.new_table();
    let collect = vm.global(b"collectgarbage");
    let function = vm
        .new_host_function_with_data(
            module,
            &[Value::Table(value), collect],
            Owner(drops.clone()),
        )
        .unwrap();
    vm.set_global(b"rust_module", function).unwrap();
    vm.execute(
        &compile(
            br#"
package.preload.rust_owned=rust_module
local first=require('rust_owned')
assert(type(first)=='table' and require('rust_owned')==first)
package.loaded.rust_owned=nil
assert(require('rust_owned')==first)
collectgarbage()
"#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    vm.execute(&compile(b"rust_module=nil;package.preload.rust_owned=nil;package.loaded.rust_owned=nil;collectgarbage();collectgarbage()").unwrap()).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(vm);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
