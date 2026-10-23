//! Private identities for native actor methods used by the callback compiler.
//! Weak keys preserve collection when a method is replaced or its actor dies.
use mlua::{Function, LightUserData, Lua, Table, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Lookup,
    Assign,
    VisibleForward,
    VisibleBound(usize),
}

struct Methods(Table);

pub(crate) fn register(lua: &Lua, function: &Function, method: Method) -> mlua::Result<()> {
    let table = lua
        .app_data_ref::<Methods>()
        .map(|methods| methods.0.clone());
    let table = match table {
        Some(table) => table,
        None => {
            let table = lua.create_table()?;
            let mt = lua.create_table_with_capacity(0, 1)?;
            mt.raw_set("__mode", "k")?;
            table.set_metatable(Some(mt))?;
            lua.set_app_data(Methods(table.clone()));
            table
        }
    };
    let value = match method {
        Method::Lookup => Value::Boolean(false),
        Method::Assign => Value::Boolean(true),
        Method::VisibleForward => Value::Integer(1),
        // This pointer is an opaque Lua identity, never dereferenced. A light
        // userdata value does not root its actor or the weak function key.
        Method::VisibleBound(actor) => {
            Value::LightUserData(LightUserData(actor as *mut std::ffi::c_void))
        }
    };
    table.raw_set(function, value)
}

pub(crate) fn identity(lua: &Lua, function: &Function) -> mlua::Result<Option<Method>> {
    let table = lua
        .app_data_ref::<Methods>()
        .map(|methods| methods.0.clone());
    let Some(table) = table else {
        return Ok(None);
    };
    Ok(match table.raw_get::<Value>(function)? {
        Value::Boolean(false) => Some(Method::Lookup),
        Value::Boolean(true) => Some(Method::Assign),
        Value::Integer(1) => Some(Method::VisibleForward),
        Value::LightUserData(actor) => Some(Method::VisibleBound(actor.0 as usize)),
        _ => None,
    })
}
