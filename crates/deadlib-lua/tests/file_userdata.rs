use deadlib_lua::{Vm, compile};

#[test]
fn a_zero_file_payload_is_closed_without_requiring_a_native_stream_backend() {
    let mut vm = Vm::new();
    let value = vm.new_userdata(std::mem::size_of::<usize>());
    vm.set_global(b"file", value).unwrap();
    vm.execute(
        &compile(
            br#"
assert(io.type(file)==nil)
debug.setmetatable(file,debug.getregistry()['FILE*'])
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
