use crate::{FromLua, Function, Lua, Result, Table, Value, state::RawLua};
use std::slice;

/// Raw values suitable for conservative load-time inspection.
///
/// Strings are copied as bytes and numeric subtypes/bits are preserved. Tables
/// and functions remain owning Lua handles. Other types are classified without
/// decoding them: in particular, inspecting userdata does not resume a caught
/// Rust panic. This differs deliberately from checked conversion to [`Value`].
/// Hosts can reject unsupported captures or fields without executing a dormant
/// error branch. These values are for loading; compiled artifacts should retain
/// only Rust-owned literals and host identities.
#[derive(Debug)]
pub enum InspectionValue {
    Nil,
    Boolean(bool),
    Integer(crate::Integer),
    Number(crate::Number),
    Bytes(Box<[u8]>),
    Table(Table),
    Function(Function),
    Other(&'static str),
}

impl FromLua for InspectionValue {
    fn from_lua(value: Value, _: &Lua) -> Result<Self> {
        Ok(match value {
            Value::Nil => Self::Nil,
            Value::Boolean(v) => Self::Boolean(v),
            Value::Integer(v) => Self::Integer(v),
            Value::Number(v) => Self::Number(v),
            Value::String(v) => Self::Bytes(v.as_bytes().as_ref().into()),
            Value::Table(v) => Self::Table(v),
            Value::Function(v) => Self::Function(v),
            v => Self::Other(v.type_name()),
        })
    }

    unsafe fn from_stack(index: std::os::raw::c_int, lua: &RawLua) -> Result<Self> {
        let state = lua.state();
        // SAFETY: FromLua's caller roots this stack slot under the VM lock.
        // Only actual strings reach tolstring, so it cannot allocate or coerce.
        // Their bytes are copied before returning. Checked handle conversion is
        // restricted to tables/functions, never private wrapped-failure userdata.
        unsafe {
            Ok(match ffi::lua_type(state, index) {
                ffi::LUA_TNIL => Self::Nil,
                ffi::LUA_TBOOLEAN => Self::Boolean(ffi::lua_toboolean(state, index) != 0),
                ffi::LUA_TNUMBER if ffi::lua_isinteger(state, index) != 0 => {
                    Self::Integer(ffi::lua_tointeger(state, index))
                }
                ffi::LUA_TNUMBER => Self::Number(ffi::lua_tonumber(state, index)),
                ffi::LUA_TSTRING => {
                    let mut len = 0;
                    let data = ffi::lua_tolstring(state, index, &mut len);
                    Self::Bytes(slice::from_raw_parts(data.cast::<u8>(), len).into())
                }
                ffi::LUA_TTABLE => Self::Table(Table::from_stack(index, lua)?),
                ffi::LUA_TFUNCTION => Self::Function(Function::from_stack(index, lua)?),
                ffi::LUA_TUSERDATA => Self::Other("userdata"),
                ffi::LUA_TLIGHTUSERDATA => Self::Other("lightuserdata"),
                ffi::LUA_TTHREAD => Self::Other("thread"),
                _ => Self::Other("other"),
            })
        }
    }
}
