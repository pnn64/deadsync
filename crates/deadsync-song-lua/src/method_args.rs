use std::ffi::c_int;

use mlua::{FromLua, FromLuaMulti, Lua, MultiValue, Value, state::RawLua};

/// The argument prefix used by small numeric methods, including explicit nils.
pub(crate) struct MethodArgs<const N: usize> {
    values: [Value; N],
    len: usize,
}

impl<const N: usize> MethodArgs<N> {
    pub(crate) fn front(&self) -> Option<&Value> {
        (self.len > 0).then(|| &self.values[0])
    }

    pub(crate) fn take_method_arg(&mut self, index: usize) -> Option<Value> {
        let offset = usize::from(matches!(self.front(), Some(Value::Table(_))));
        let index = offset + index;
        (index < self.len).then(|| std::mem::replace(&mut self.values[index], Value::Nil))
    }
}

impl<const N: usize> FromLuaMulti for MethodArgs<N> {
    fn from_lua_multi(mut values: MultiValue, _lua: &Lua) -> mlua::Result<Self> {
        let len = values.len().min(N);
        Ok(Self {
            values: std::array::from_fn(|_| values.pop_front().unwrap_or(Value::Nil)),
            len,
        })
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &RawLua) -> mlua::Result<Self> {
        let len = (nvals as usize).min(N);
        let mut values = std::array::from_fn(|_| Value::Nil);
        for (index, value) in values[..len].iter_mut().enumerate() {
            // SAFETY: mlua supplies nvals live stack values. Read their original
            // negative indices, including when more than N arguments are given.
            // mlua creates owning handles; no borrowed stack value escapes.
            *value = unsafe { Value::from_stack(-nvals + index as c_int, lua)? };
        }
        Ok(Self { values, len })
    }
}
