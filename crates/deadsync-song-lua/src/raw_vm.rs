//! Locked, stack-restoring access for read-only raw VM batches.
use mlua::{Lua, Result};
use mlua::ffi;

struct RestoreStack {
    state: *mut ffi::lua_State,
    top: std::os::raw::c_int,
}

impl Drop for RestoreStack {
    fn drop(&mut self) {
        // SAFETY: the guard cannot outlive exec_raw_lua's VM lock. Lua 5.1
        // stack truncation neither allocates nor invokes authored callbacks.
        unsafe { ffi::lua_settop(self.state, self.top); }
    }
}

/// Avoid allocating a temporary protected C closure for raw reads.
///
/// SAFETY: `slots` must cover all working space. `read` may only push existing
/// rooted values, perform raw reads and use mlua's checked value conversions.
/// It must never call authored code or an allocating/throwing raw Lua API.
/// Writes and metamethod calls must use protected APIs outside this scope.
pub(crate) unsafe fn with_read_stack<R>(
    lua: &Lua,
    slots: std::os::raw::c_int,
    read: impl FnOnce(&mlua::state::RawLua, std::os::raw::c_int) -> Result<R>,
) -> Result<R> {
    lua.exec_raw_lua(|raw| {
        let state = raw.state();
        // SAFETY: same check used by mlua's raw_get; the pinned Lua 5.1 build
        // grows its stack under luaD_rawrunprotected and returns zero on error.
        // The guard restores the stack on Rust errors and unwinding as well.
        unsafe {
            if ffi::lua_checkstack(state, slots) == 0 { return Err(mlua::Error::StackError); }
            let top = ffi::lua_gettop(state);
            let _restore = RestoreStack { state, top };
            read(raw, top)
        }
    })
}


