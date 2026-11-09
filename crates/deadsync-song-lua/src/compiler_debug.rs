//! The existing song compiler's public debug table, separate from VM primitives.
use super::{Error, HostCall, Value, Vm};

#[derive(Clone, Copy)]
enum Method {
    Info,
    Upvalue,
    Traceback,
}
pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let table = vm.try_new_table()?;
    let root = vm.pin(Value::Table(table))?;
    let result = (|| {
        for (name, method) in [
            ("getinfo", Method::Info),
            ("getupvalue", Method::Upvalue),
            ("traceback", Method::Traceback),
        ] {
            let function = vm.new_host_function_with_data(callback, &[globals], method)?;
            super::put(vm, table, name, function)?;
        }
        vm.set_global(b"debug", Value::Table(table))
    })();
    vm.unpin(root)?;
    result
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    match *call.function_data::<Method>().unwrap() {
        Method::Info => info(call),
        Method::Upvalue => upvalue(call),
        Method::Traceback => {
            let first = call.arguments().first().copied().unwrap_or(Value::Nil);
            let value = match first {
                Value::String(id)
                    if std::str::from_utf8(call.string_bytes(id).unwrap()).is_ok() =>
                {
                    first
                }
                _ => call.try_intern(b"")?,
            };
            call.push(value)
        }
    }
}
fn info(call: &mut HostCall<'_>) -> Result<(), Error> {
    let supplied = !call.arguments().is_empty();
    let value = super::field(call, call.captures()[0], "__songlua_current_script_path")?;
    let source = if value == Value::Nil {
        "=[songlua]".to_owned()
    } else {
        let id = call
            .coerce_string(value)?
            .ok_or_else(|| call.error(b"script path: string expected"))?;
        let text = std::str::from_utf8(call.string_bytes(id).unwrap())
            .map_err(|error| call.error(error.to_string().as_bytes()))?;
        format!("@{text}")
    };
    let source = call.try_intern(source.as_bytes())?;
    call.with_roots(&[source], |call| {
        let result = call.try_new_table()?;
        call.with_roots(&[Value::Table(result)], |call| {
            for key in ["source", "short_src"] {
                super::set(call, Value::Table(result), key, source)?;
            }
            let what = call.try_intern(b"Lua")?;
            super::set(call, Value::Table(result), "what", what)?;
            for key in ["currentline", "linedefined", "lastlinedefined"] {
                super::set(call, Value::Table(result), key, Value::Number(0.))?;
            }
            if supplied {
                let empty = call.try_intern(b"")?;
                super::set(call, Value::Table(result), "namewhat", empty)?;
            }
            call.push(Value::Table(result))
        })
    })
}
fn upvalue(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let index = match (stack.value(1), stack.value(2)) {
        (Some(Value::Function(_) | Value::Native(_)), Some(Value::Number(number))) => {
            (number as i64) as i32
        }
        _ => 0,
    };
    if index > 0
        && let Some(name) = stack.get_upvalue(1, index as usize)?
    {
        let name = name.strip_suffix(&[0]).unwrap_or(name).to_vec();
        let value = stack.value(-1).unwrap();
        stack.push_bytes(&name)?;
        stack.push(value)?;
    } else {
        stack.push(Value::Nil)?;
        stack.push(Value::Nil)?;
    }
    stack.finish(2)
}
