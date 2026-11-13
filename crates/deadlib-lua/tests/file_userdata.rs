use deadlib_lua::{Vm, compile};

#[test]
fn files_require_owned_rust_streams_and_retain_closed_file_behavior() {
    let mut vm = Vm::new();
    let value = vm.new_userdata(std::mem::size_of::<usize>());
    vm.set_global(b"file", value).unwrap();
    vm.execute(
        &compile(
            br#"
assert(io.type(file)==nil)
debug.setmetatable(file,debug.getregistry()['FILE*'])
-- A raw userdata payload is no longer interpreted as a C FILE**. Copying
-- the metatable must not manufacture ownership of a Rust stream.
assert(io.type(file)==nil)
local ok,err=pcall(file.read,file)
assert(not ok and err:find('FILE* expected',1,true))
debug.setmetatable(file,nil)
file=assert(io.tmpfile())
assert(io.type(file)=='file')
assert(file:close())
assert(io.type(file)=='closed file'and tostring(file)=='file (closed)')
for _,name in ipairs{'close','read','write','flush','seek','setvbuf','lines'}do
  local ok,err=pcall(file[name],file)
  assert(not ok and err:find('attempt to use a closed file',1,true))
end
assert(select('#',getmetatable(file).__gc(file))==0)
file=nil;collectgarbage();collectgarbage()
"#,
        )
        .unwrap(),
    )
    .unwrap();
}
