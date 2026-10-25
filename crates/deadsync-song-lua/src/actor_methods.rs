//! Private identities for native actor methods used by the callback compiler.
//! Weak keys preserve collection when a method is replaced or its actor dies.
use mlua::{Function, LightUserData, Lua, Table, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setter {
    Visible,
    X,
    Y,
}
impl Setter {
    pub(crate) const ALL: [Self; 3] = [Self::Visible, Self::X, Self::Y];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::X => "x",
            Self::Y => "y",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Method {
    Lookup,
    Assign,
    Forward(Setter),
    Bound(Setter, usize),
}

#[derive(Clone, Copy)]
struct Bound(Setter, usize);
impl mlua::UserData for Bound {}

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
        Method::Forward(setter) => Value::Integer(match setter {
            Setter::Visible => 1,
            Setter::X => 2,
            Setter::Y => 3,
        }),
        // This pointer is an opaque Lua identity, never dereferenced. A light
        // userdata value does not root its actor or the weak function key.
        Method::Bound(Setter::Visible, actor) => {
            Value::LightUserData(LightUserData(actor as *mut std::ffi::c_void))
        }
        Method::Bound(setter, actor) => Value::UserData(lua.create_userdata(Bound(setter, actor))?),
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
        Value::Integer(tag @ 1..=3) => Some(Method::Forward(match tag {
            1 => Setter::Visible,
            2 => Setter::X,
            _ => Setter::Y,
        })),
        Value::LightUserData(actor) => Some(Method::Bound(Setter::Visible, actor.0 as usize)),
        Value::UserData(data) if data.is::<Bound>() => {
            let Bound(setter, actor) = *data.borrow::<Bound>()?;
            Some(Method::Bound(setter, actor))
        }
        _ => None,
    })
}
