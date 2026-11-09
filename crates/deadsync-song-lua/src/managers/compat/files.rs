//! Existing lua.ReadFile/WriteFile host contracts; writes remain virtual.
use super::*;

pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let initialize =
        vm.new_host_function_with_data(initialize, &[globals], context.song_dir.clone())?;
    vm.call(initialize, &[]).map(|_| ())
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    let directory = call.function_data::<std::path::PathBuf>().unwrap().clone();
    let lua = object(call, |call, out| {
        let read = call.new_host_function_with_data(read, &[], directory)?;
        call.with_roots(&[read], |call| set(out, "ReadFile", read, call))?;
        let write = call.new_host_function(write, &[Value::Table(globals)])?;
        call.with_roots(&[write], |call| set(out, "WriteFile", write, call))
    })?;
    call.with_roots(&[Value::Table(lua)], |call| set(globals, "lua", lua, call))
}

fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let index = usize::from(matches!(arg(call, 0), Value::Table(_)));
    let Some(path) = text(call, arg(call, index)) else {
        return call.push(Value::Nil);
    };
    let directory = call.function_data::<std::path::PathBuf>().unwrap();
    let path = crate::resolve_compat_path(directory, &path);
    let value = match std::fs::read_to_string(path) {
        Ok(text) => call.try_intern(text.as_bytes())?,
        Err(_) => Value::Nil,
    };
    call.push(value)
}

fn write(call: &mut HostCall<'_>) -> Result<(), Error> {
    super::super::note_side_effect(call, call.captures()[0])?;
    call.push(Value::Boolean(true))
}
