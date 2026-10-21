//! Checked unused callback inputs, retaining the bound receiver inline.
use mlua::{FromLua, FromLuaMulti, Lua, MultiValue, Value};
use std::os::raw::c_int;

pub(crate) enum IgnoredArgs {
    Empty,
    One { _value: Value },
    Many { _values: MultiValue },
}

impl FromLuaMulti for IgnoredArgs {
    fn from_lua_multi(mut values: MultiValue, _: &Lua) -> mlua::Result<Self> {
        Ok(match values.len() {
            0 => Self::Empty,
            1 => Self::One {
                _value: values.pop_front().unwrap(),
            },
            _ => Self::Many { _values: values },
        })
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: FromLuaMulti's caller supplies nvals live argument slots.
        // Keep the same checked conversions and callback input lifetime. Larger
        // lists use mlua's original implementation, including conversion order.
        Ok(match nvals {
            0 => Self::Empty,
            1 => Self::One {
                _value: unsafe { Value::from_stack(-1, lua)? },
            },
            _ => Self::Many {
                _values: unsafe { MultiValue::from_stack_multi(nvals, lua)? },
            },
        })
    }
}

#[cfg(test)]
#[path = "../tests/perf/ignored_core.rs"]
pub(crate) mod perf;
