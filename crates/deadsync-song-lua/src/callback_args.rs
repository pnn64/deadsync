//! Checked callback values with inline storage for short lists.
use mlua::{FromLua, FromLuaMulti, IntoLuaMulti, Lua, MultiValue, Value};
use std::os::raw::c_int;

pub(crate) use mlua::IgnoredArgs;

/// Tuple decoding with MultiValue's original unwrapped stack-error policy.
/// Numeric callbacks retain the consumed prefix inline and check every extra
/// input through the tuple's final MultiValue field.
pub(crate) struct CheckedPrefix<T>(pub(crate) T);

pub(crate) type NumericArgs = CheckedPrefix<(Value, Value, Value, MultiValue)>;

impl<T: FromLuaMulti> FromLuaMulti for CheckedPrefix<T> {
    fn from_lua_multi(values: MultiValue, lua: &Lua) -> mlua::Result<Self> {
        T::from_lua_multi(values, lua).map(Self)
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: forward the unchanged live argument range to mlua's decoder.
        unsafe { T::from_stack_multi(nvals, lua).map(Self) }
    }
}

pub(crate) enum ForwardArgs {
    Empty,
    One(Value),
    Two([Value; 2]),
    Three([Value; 3]),
    Many(MultiValue),
}

impl ForwardArgs {
    pub(crate) fn front(&self) -> Option<&Value> {
        match self {
            Self::Empty => None,
            Self::One(value) => Some(value),
            Self::Two(values) => Some(&values[0]),
            Self::Three(values) => Some(&values[0]),
            Self::Many(values) => values.front(),
        }
    }
}

impl FromLuaMulti for ForwardArgs {
    fn from_lua_multi(mut values: MultiValue, _: &Lua) -> mlua::Result<Self> {
        Ok(match values.len() {
            0 => Self::Empty,
            1 => Self::One(values.pop_front().unwrap()),
            2 => Self::Two(std::array::from_fn(|_| values.pop_front().unwrap())),
            3 => Self::Three(std::array::from_fn(|_| values.pop_front().unwrap())),
            _ => Self::Many(values),
        })
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: the caller supplies nvals live slots. Use the same checked
        // left-to-right Value conversions, retaining every input and exact arity.
        unsafe {
            Ok(match nvals {
                0 => Self::Empty,
                1 => Self::One(Value::from_stack(-1, lua)?),
                2 => Self::Two([Value::from_stack(-2, lua)?, Value::from_stack(-1, lua)?]),
                3 => Self::Three([
                    Value::from_stack(-3, lua)?,
                    Value::from_stack(-2, lua)?,
                    Value::from_stack(-1, lua)?,
                ]),
                _ => Self::Many(MultiValue::from_stack_multi(nvals, lua)?),
            })
        }
    }
}

impl IntoLuaMulti for ForwardArgs {
    fn into_lua_multi(self, _: &Lua) -> mlua::Result<MultiValue> {
        Ok(match self {
            Self::Empty => MultiValue::new(),
            Self::One(value) => MultiValue::from_vec(vec![value]),
            Self::Two(values) => MultiValue::from_vec(Vec::from(values)),
            Self::Three(values) => MultiValue::from_vec(Vec::from(values)),
            Self::Many(values) => values,
        })
    }

    unsafe fn push_into_stack_multi(self, lua: &mlua::state::RawLua) -> mlua::Result<c_int> {
        // SAFETY: delegate stack reservation and writes to mlua. The trailing
        // unit preserves MultiValue's n+1 reservation, without adding a value.
        // Borrow each value so all handles survive until every push finishes.
        unsafe {
            match self {
                Self::Empty => ((),).push_into_stack_multi(lua),
                Self::One(value) => (&value, ()).push_into_stack_multi(lua),
                Self::Two(values) => (&values[0], &values[1], ()).push_into_stack_multi(lua),
                Self::Three(values) => {
                    (&values[0], &values[1], &values[2], ()).push_into_stack_multi(lua)
                }
                Self::Many(values) => values.push_into_stack_multi(lua),
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/perf/ignored_core.rs"]
pub(crate) mod perf;
