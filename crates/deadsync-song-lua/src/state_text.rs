use std::ffi::c_int;

use mlua::{FromLua, FromLuaMulti, Lua, LuaString, MultiValue, Value, ffi, state::RawLua};

/// A validated string handle, with String's getter coercions and errors.
pub(crate) enum StateText {
    Lua(LuaString),
    Coerced(String),
}

impl StateText {
    pub(crate) fn into_string(self, lua: &Lua) -> mlua::Result<LuaString> {
        match self {
            Self::Lua(text) => Ok(text),
            Self::Coerced(text) => lua.create_string(text),
        }
    }
}

impl FromLua for StateText {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        // Preserve String's safe conversion contract, including its error names.
        Ok(Self::Coerced(String::from_lua(value, lua)?))
    }

    unsafe fn from_stack(index: c_int, lua: &RawLua) -> mlua::Result<Self> {
        // SAFETY: FromLua supplies a live stack index. Inspecting its type does
        // not mutate the stack; the string branch is checked before borrowing.
        if unsafe { ffi::lua_type(lua.state(), index) } == ffi::LUA_TSTRING {
            // SAFETY: this live slot contains a Lua string. Validation finishes
            // before another Lua call or allocation can invalidate its bytes.
            unsafe { validate_stack_text(index, lua) }.map_err(|error| {
                mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "String".to_string(),
                    message: Some(error.to_string()),
                }
            })?;
            // SAFETY: mlua creates an owning handle to the same live string.
            return Ok(Self::Lua(unsafe { LuaString::from_stack(index, lua)? }));
        }
        // Numeric coercions and type errors use the original implementation.
        // SAFETY: the original index is still live and no borrowed bytes escape.
        Ok(Self::Coerced(unsafe { String::from_stack(index, lua)? }))
    }
}

/// String-only setter argument; invalid or absent input means the empty string.
pub(crate) struct StateTextArgs(pub(crate) Option<LuaString>);

impl FromLuaMulti for StateTextArgs {
    fn from_lua_multi(mut values: MultiValue, _lua: &Lua) -> mlua::Result<Self> {
        let mut value = values.pop_front();
        if matches!(value, Some(Value::Table(_))) {
            value = values.pop_front();
        }
        Ok(Self(match value {
            Some(Value::String(text)) if text.to_str().is_ok() => Some(text),
            _ => None,
        }))
    }

    unsafe fn from_stack_multi(nvals: c_int, lua: &RawLua) -> mlua::Result<Self> {
        if nvals == 0 {
            return Ok(Self(None));
        }
        // SAFETY: mlua supplies nvals live arguments. Keep their original
        // negative indices even when many ignored arguments follow the value.
        let offset = c_int::from(unsafe { ffi::lua_type(lua.state(), -nvals) } == ffi::LUA_TTABLE);
        if nvals <= offset {
            return Ok(Self(None));
        }
        let index = -nvals + offset;
        // SAFETY: index selects one of those live arguments. Only strings are
        // validated; numeric/boolean values must keep the setter's empty default.
        if unsafe { ffi::lua_type(lua.state(), index) } == ffi::LUA_TSTRING
            && unsafe { validate_stack_text(index, lua) }.is_ok()
        {
            // SAFETY: validation has ended and mlua returns an owning handle.
            return Ok(Self(Some(unsafe { LuaString::from_stack(index, lua)? })));
        }
        Ok(Self(None))
    }
}

/// Caller must provide a live stack slot whose type is LUA_TSTRING.
unsafe fn validate_stack_text(index: c_int, lua: &RawLua) -> Result<(), std::str::Utf8Error> {
    let mut len = 0;
    // SAFETY: a live string slot guarantees a non-null, readable pointer and
    // exact byte length. No Lua operation runs while this borrowed slice exists.
    let bytes = unsafe {
        let data = ffi::lua_tolstring(lua.state(), index, &mut len);
        std::slice::from_raw_parts(data.cast::<u8>(), len)
    };
    std::str::from_utf8(bytes).map(|_| ())
}
