//! Author existing compiler fixtures with the Rust interpreter's ordinary API.
//! Compiled programs own bytecode and bindings, without retaining the authoring VM.
#![allow(dead_code)]
use deadlib_lua::{HostCall, Value as LuaValue, Vm, compile_named};
pub use deadsync_song_lua::rust_engine::baked_program::*;

fn retain(vm: &mut Vm, values: &[LuaValue]) {
    for &value in values {
        if !matches!(
            value,
            LuaValue::Nil | LuaValue::Boolean(_) | LuaValue::Number(_)
        ) {
            // Fixture handles have the loading VM's lifetime. Explicit VM
            // shutdown, rather than this authoring helper, tests detachment.
            vm.pin(value).unwrap();
        }
    }
}

pub fn eval(vm: &mut Vm, source: impl AsRef<[u8]>) -> Vec<LuaValue> {
    let code = compile_named(source.as_ref(), b"=baked-program fixture").unwrap();
    let values = vm.execute(&code).unwrap().to_vec();
    retain(vm, &values);
    values
}

pub fn load(vm: &mut Vm, source: impl AsRef<[u8]>) -> LuaValue {
    let loader = vm.get_global(b"loadstring").unwrap();
    let source = string(vm, source);
    let name = string(vm, b"=baked-program fixture");
    let values = call(vm, loader, &[source, name]);
    let [value @ LuaValue::Function(_)] = *values.as_slice() else {
        panic!("fixture function load: {values:?}")
    };
    value
}

pub fn as_number(value: LuaValue) -> f64 {
    let LuaValue::Number(value) = value else {
        panic!("fixture number expected: {value:?}")
    };
    value
}

pub fn as_bool(value: LuaValue) -> bool {
    let LuaValue::Boolean(value) = value else {
        panic!("fixture Boolean expected: {value:?}")
    };
    value
}

pub fn call(vm: &mut Vm, function: LuaValue, args: &[LuaValue]) -> Vec<LuaValue> {
    let values = vm.call(function, args).unwrap().to_vec();
    retain(vm, &values);
    values
}

pub fn call_one(vm: &mut Vm, function: LuaValue, args: &[LuaValue]) -> LuaValue {
    call(vm, function, args)
        .first()
        .copied()
        .unwrap_or(LuaValue::Nil)
}

pub fn call_number(vm: &mut Vm, function: LuaValue, args: &[LuaValue]) -> f64 {
    as_number(call_one(vm, function, args))
}

pub fn call_bool(vm: &mut Vm, function: LuaValue, args: &[LuaValue]) -> bool {
    as_bool(call_one(vm, function, args))
}

pub fn call_bytes(vm: &mut Vm, function: LuaValue, args: &[LuaValue]) -> Vec<u8> {
    let value = call_one(vm, function, args);
    bytes(vm, value)
}

pub fn table(vm: &mut Vm) -> LuaValue {
    let value = LuaValue::Table(vm.try_new_table().unwrap());
    retain(vm, &[value]);
    value
}

pub fn string(vm: &mut Vm, bytes: impl AsRef<[u8]>) -> LuaValue {
    let value = vm.try_intern(bytes.as_ref()).unwrap();
    retain(vm, &[value]);
    value
}

pub fn bytes(vm: &Vm, value: LuaValue) -> Vec<u8> {
    let LuaValue::String(id) = value else {
        panic!("fixture string expected: {value:?}")
    };
    vm.string_bytes(id).unwrap().to_vec()
}

pub fn set(vm: &mut Vm, owner: LuaValue, name: &[u8], value: LuaValue) {
    let LuaValue::Table(owner) = owner else {
        panic!("fixture table expected")
    };
    let name = vm.try_intern(name).unwrap();
    vm.raw_table_set(owner, name, value).unwrap();
}

pub fn set_string(vm: &mut Vm, owner: LuaValue, name: &[u8], bytes: impl AsRef<[u8]>) {
    let value = string(vm, bytes);
    set(vm, owner, name, value);
}

pub fn set_metatable(vm: &mut Vm, owner: LuaValue, metatable: LuaValue) {
    let setter = vm.get_global(b"setmetatable").unwrap();
    call(vm, setter, &[owner, metatable]);
}

pub fn set_environment(vm: &mut Vm, function: LuaValue, environment: LuaValue) -> bool {
    if !matches!(function, LuaValue::Function(_)) {
        return false;
    }
    let setter = vm.get_global(b"setfenv").unwrap();
    call(vm, setter, &[function, environment]);
    true
}

pub fn get(vm: &mut Vm, owner: LuaValue, name: &[u8]) -> LuaValue {
    let LuaValue::Table(owner) = owner else {
        panic!("fixture table expected")
    };
    let name = vm.try_intern(name).unwrap();
    let value = vm.raw_table_get(owner, name).unwrap();
    retain(vm, &[value]);
    value
}

pub fn get_number(vm: &mut Vm, owner: LuaValue, name: &[u8]) -> f64 {
    as_number(get(vm, owner, name))
}

pub fn get_bool(vm: &mut Vm, owner: LuaValue, name: &[u8]) -> bool {
    as_bool(get(vm, owner, name))
}

pub fn get_bytes(vm: &mut Vm, owner: LuaValue, name: &[u8]) -> Vec<u8> {
    let value = get(vm, owner, name);
    bytes(vm, value)
}

pub fn entries(vm: &mut Vm, table: LuaValue) -> Vec<(LuaValue, LuaValue)> {
    let next = vm.get_global(b"next").unwrap();
    let mut key = LuaValue::Nil;
    let mut out = Vec::new();
    loop {
        let values = call(vm, next, &[table, key]);
        match values.as_slice() {
            [] | [LuaValue::Nil] => break,
            &[next, value] => {
                key = next;
                out.push((next, value));
            }
            other => panic!("fixture next result: {other:?}"),
        }
    }
    out
}

pub fn upvalues(vm: &mut Vm, function: LuaValue) -> Vec<(Vec<u8>, LuaValue)> {
    let debug = vm.get_global(b"debug").unwrap();
    let getter = get(vm, debug, b"getupvalue");
    let mut out = Vec::new();
    for index in 1.. {
        let values = call(vm, getter, &[function, LuaValue::Number(index as f64)]);
        match values.as_slice() {
            [] => break,
            &[name @ LuaValue::String(_), value] => out.push((bytes(vm, name), value)),
            other => panic!("fixture upvalue result: {other:?}"),
        }
    }
    out
}

pub fn shutdown_marker<T: std::any::Any + Send + Sync>(vm: &mut Vm, marker: T) {
    let function = vm
        .new_host_function_with_data(|_: &mut HostCall<'_>| Ok(()), &[], marker)
        .unwrap();
    vm.set_global(b"fixture_shutdown_marker", function).unwrap();
}

pub fn dump(vm: &mut Vm, function: LuaValue) -> Result<Vec<u8>, CompileError> {
    if !matches!(function, LuaValue::Function(_)) {
        return Err(CompileError::Unsupported {
            pc: 0,
            reason: "native closure",
        });
    }
    Ok(raw_dump(vm, function).expect("fixture function dump"))
}

pub fn raw_dump(vm: &mut Vm, function: LuaValue) -> Result<Vec<u8>, deadlib_lua::Error> {
    let string = vm.get_global(b"string")?;
    let dump = get(vm, string, b"dump");
    let values = vm.call(dump, &[function])?;
    let [LuaValue::String(id)] = *values else {
        panic!("fixture function dump")
    };
    Ok(vm.string_bytes(id).unwrap().to_vec())
}

pub fn compile(
    vm: &mut Vm,
    function: LuaValue,
    bindings: &Bindings,
) -> Result<Program, CompileError> {
    compile_bytecode(&dump(vm, function)?, bindings)
}

pub trait FixtureCompiler {
    fn compile(
        &self,
        vm: &mut Vm,
        function: LuaValue,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError>;
}
impl FixtureCompiler for Compiler<'_> {
    fn compile(
        &self,
        vm: &mut Vm,
        function: LuaValue,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        self.compile_bytecode(&dump(vm, function)?, arguments, upvalues)
    }
}
pub trait FixtureBatch {
    fn compile(
        &mut self,
        vm: &mut Vm,
        function: LuaValue,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError>;
}
impl FixtureBatch for BatchCompiler<'_, '_> {
    fn compile(
        &mut self,
        vm: &mut Vm,
        function: LuaValue,
        arguments: &[Binding],
        upvalues: &[Binding],
    ) -> Result<Program, CompileError> {
        self.compile_bytecode(&dump(vm, function)?, arguments, upvalues)
    }
}
