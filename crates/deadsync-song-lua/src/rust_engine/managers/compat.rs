//! Existing song-host convenience bindings, separate from the generic engine.
use super::fields::{array, object, set};
use super::*;
mod basic;
mod calendar;
mod colors;
mod enums;
mod files;
mod json;
mod misc;
mod offline;
mod tables;
mod versions;

pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    basic::install(vm)?;
    enums::install(vm, context)?;
    colors::install(vm)?;
    files::install(vm, context)?;
    versions::install(vm)?;
    json::install(vm)?;
    offline::install(vm)?;
    tables::install(vm)?;
    misc::install(vm, context)?;
    calendar::install(vm, context)?;
    let print = deadlib_lua::compile_named(
        include_bytes!("compat/native_print.lua"),
        b"@_fallback/Scripts/02 Utilities.lua",
    )?;
    vm.execute(&print).map(|_| ())
}

fn arg(call: &HostCall<'_>, index: usize) -> Value {
    call.arguments().get(index).copied().unwrap_or(Value::Nil)
}

/// Exercise the base stdlib helper stage separately from late theme overrides.
#[cfg(feature = "test-support")]
pub(in crate::rust_engine) fn install_base_table_helpers(vm: &mut Vm) -> Result<(), Error> {
    tables::install(vm)
}

fn text(call: &HostCall<'_>, value: Value) -> Option<String> {
    super::catalog::text(call, value)
}

fn float(call: &HostCall<'_>, value: Value) -> Option<f32> {
    super::catalog::float(call, value)
}

fn integer(call: &HostCall<'_>, index: usize) -> Result<i64, Error> {
    let value = call.number(index)?.trunc();
    if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
        return Err(call.error(b"integer out of range"));
    }
    Ok(value as i64)
}

fn bind(vm: &mut Vm, name: &str, value: Value) -> Result<(), Error> {
    let root = vm.pin(value)?;
    let result = vm.set_global(name.as_bytes(), value);
    vm.unpin(root)?;
    result
}

fn missing(vm: &mut Vm, name: &str) -> Result<bool, Error> {
    Ok(vm.get_global(name.as_bytes())? == Value::Nil)
}

fn constant_global(vm: &mut Vm, name: &str, value: Value) -> Result<(), Error> {
    let function = vm.new_host_function(super::constant, &[value])?;
    bind(vm, name, function)
}

fn rows(call: &mut HostCall<'_>, table: TableId) -> Result<Vec<(Value, Value)>, Error> {
    // A native stack owns its activation's results. Use a separate activation
    // for each scan, and retain every collected key/value on that stack while
    // growing it, including entries of weak tables.
    let scan = call.new_host_function(
        |call| {
            let table = call.captures()[0];
            let mut stack = call.native_stack()?;
            stack.push(table)?;
            let table = stack.top() as isize;
            stack.push(Value::Nil)?;
            while stack.next_table(table)? {
                let key = stack.value(-2).unwrap();
                stack.push(key)?;
            }
            let count = stack.top() - 1;
            stack.finish(count)
        },
        &[Value::Table(table)],
    )?;
    Ok(call
        .call(scan, &[])?
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect())
}

fn native_integer(value: f64) -> Option<i64> {
    if (i64::MIN as f64..-(i64::MIN as f64)).contains(&value) {
        let integer = value as i64;
        (value.to_bits() == (integer as f64).to_bits()).then_some(integer)
    } else {
        None
    }
}

fn value_text(call: &HostCall<'_>, value: Value, lossy: bool) -> Result<Option<String>, Error> {
    Ok(match value {
        Value::String(id) => {
            let bytes = call.string_bytes(id).expect("rooted text");
            Some(if lossy {
                String::from_utf8_lossy(bytes).into_owned()
            } else {
                std::str::from_utf8(bytes)
                    .map_err(|error| call.error(error.to_string().as_bytes()))?
                    .to_owned()
            })
        }
        Value::Number(value) => {
            Some(native_integer(value).map_or_else(|| value.to_string(), |v| v.to_string()))
        }
        Value::Boolean(value) => Some(value.to_string()),
        _ => None,
    })
}

pub(in crate::rust_engine) fn clone_value(
    call: &mut HostCall<'_>,
    value: Value,
    after: &impl Fn(&mut HostCall<'_>, TableId, TableId) -> Result<(), Error>,
) -> Result<Value, Error> {
    tables::clone_value(call, value, after)
}

#[cfg(feature = "test-support")]
pub(in crate::rust_engine) fn install_calendar_for_date(
    vm: &mut Vm,
    context: &crate::SongLuaCompileContext,
    date: crate::SongLuaDateGlobals,
) -> Result<(), Error> {
    calendar::install_for_date(vm, context, date)
}
