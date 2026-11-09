//! Existing offline network, fixed crypto and theme-info INI host behavior.
use super::*;

#[derive(Clone, Copy)]
enum Operation {
    Crypto,
    Network,
    Ini,
    Response,
    Socket,
    False,
    True,
    Owner,
    Zeros(usize),
    Uuid,
    Effect(Option<bool>),
    ReadIni,
    Request,
    Query,
}
pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    for (name, op) in [
        ("CRYPTMAN", Operation::Crypto),
        ("NETWORK", Operation::Network),
        ("IniFile", Operation::Ini),
    ] {
        let function = vm.new_host_function_with_data(dispatch, &[globals], op)?;
        let root = vm.pin(function)?;
        let result = (|| {
            let value = vm.call(function, &[])?[0];
            bind(vm, name, value)
        })();
        vm.unpin(root)?;
        result?;
    }
    Ok(())
}
fn method(call: &mut HostCall<'_>, table: TableId, name: &str, op: Operation) -> Result<(), Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.with_roots(&[key], |call| {
        let mut captures = vec![call.captures()[0]];
        if matches!(op, Operation::Owner) {
            captures.push(Value::Table(table));
        }
        let function = call.new_host_function_with_data(dispatch, &captures, op)?;
        call.raw_table_set(table, key, function)
    })
}
fn child(call: &mut HostCall<'_>, table: TableId, name: &str, value: Value) -> Result<(), Error> {
    call.with_roots(&[value], |call| set(table, name, value, call))
}
fn effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    super::super::note_side_effect(call, call.captures()[0])
}
fn construct(call: &mut HostCall<'_>, op: Operation) -> Result<Value, Error> {
    Ok(Value::Table(object(call, |call, table| {
        match op {
            Operation::Crypto => {
                for (name, len) in [
                    ("SHA1File", 20),
                    ("SHA1String", 20),
                    ("SHA256File", 32),
                    ("SHA256String", 32),
                ] {
                    method(call, table, name, Operation::Zeros(len))?;
                }
                method(call, table, "GenerateRandomUUID", Operation::Uuid)?;
                method(
                    call,
                    table,
                    "SignFileToFile",
                    Operation::Effect(Some(false)),
                )?;
            }
            Operation::Network => {
                method(call, table, "IsUrlAllowed", Operation::False)?;
                method(call, table, "HttpRequest", Operation::Request)?;
                method(call, table, "WebSocket", Operation::Socket)?;
                method(call, table, "EncodeQueryParameters", Operation::Query)?;
            }
            Operation::Ini => {
                method(call, table, "ReadFile", Operation::ReadIni)?;
                method(call, table, "WriteFile", Operation::Effect(Some(true)))?;
            }
            Operation::Response => {
                set(table, "status", 0, call)?;
                set(table, "code", 0, call)?;
                set(table, "body", "", call)?;
                set(table, "error", "offline", call)?;
                let headers = Value::Table(call.try_new_table()?);
                child(call, table, "headers", headers)?;
                method(call, table, "IsFinished", Operation::True)?;
                method(call, table, "GetResponse", Operation::Owner)?;
                method(call, table, "Cancel", Operation::Effect(None))?;
            }
            Operation::Socket => {
                set(table, "is_open", false, call)?;
                method(call, table, "IsOpen", Operation::False)?;
                method(call, table, "Send", Operation::Effect(None))?;
                method(call, table, "Close", Operation::Effect(None))?;
            }
            _ => unreachable!(),
        }
        Ok(())
    })?))
}
fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let op = *call.function_data::<Operation>().unwrap();
    let value = match op {
        Operation::Crypto
        | Operation::Network
        | Operation::Ini
        | Operation::Response
        | Operation::Socket => construct(call, op)?,
        Operation::False => Value::Boolean(false),
        Operation::True => Value::Boolean(true),
        Operation::Owner => call.captures()[1],
        Operation::Zeros(len) => call.try_intern(&vec![0; len])?,
        Operation::Uuid => call.try_intern(b"00000000-0000-4000-8000-000000000000")?,
        Operation::Effect(result) => {
            effect(call)?;
            let Some(result) = result else {
                return Ok(());
            };
            Value::Boolean(result)
        }
        Operation::Request => {
            effect(call)?;
            construct(call, Operation::Response)?
        }
        Operation::ReadIni => {
            let index = usize::from(matches!(arg(call, 0), Value::Table(_)));
            let path = text(call, arg(call, index)).unwrap_or_default();
            Value::Table(object(call, |call, table| {
                if path.ends_with("ThemeInfo.ini") {
                    let info = object(call, |call, info| {
                        set(info, "DisplayName", crate::SONG_LUA_THEME_NAME, call)?;
                        set(info, "Version", crate::SONG_LUA_PRODUCT_VERSION, call)?;
                        set(info, "Author", "DeadSync song-Lua compat", call)
                    })?;
                    child(call, table, "ThemeInfo", Value::Table(info))?;
                }
                Ok(())
            })?)
        }
        Operation::Query => {
            let index = usize::from(matches!(arg(call, 0), Value::Table(_)));
            let mut parts = Vec::new();
            if let Value::Table(table) = arg(call, index) {
                for (key, value) in rows(call, table)? {
                    if let Some(key) = value_text(call, key, true)? {
                        let value = value_text(call, value, true)?.unwrap_or_default();
                        parts.push(format!(
                            "{}={}",
                            crate::url_encode_component(&key),
                            crate::url_encode_component(&value)
                        ));
                    }
                }
            }
            parts.sort_unstable();
            call.try_intern(parts.join("&").as_bytes())?
        }
    };
    call.push(value)
}
