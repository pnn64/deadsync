//! Standalone C exports. Every public Lua entry is a frame-free tail jump into
//! C, so C error jumps never cross Rust forwarding frames.
#![allow(unsafe_code)]
#![cfg(all(
    any(windows, target_os = "linux"),
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")
))]

#[unsafe(no_mangle)]
unsafe extern "C" fn deadlua_engine_new_state(
    anchor: *const std::ffi::c_void,
) -> *mut std::ffi::c_void {
    // SAFETY: only this image's C default-state constructor calls this hook.
    unsafe { deadlib_lua::c_api_owned_state(anchor) }
}

#[unsafe(no_mangle)]
extern "C" fn deadlua_engine_abi_version() -> u64 {
    deadlib_lua::c_api_version()
}

/// Private cleanup diagnostics; the public Lua ABI remains in Lua's headers.
#[unsafe(no_mangle)]
unsafe extern "C" fn deadlua_engine_statistics(output: *mut usize, count: usize) -> usize {
    let stats = deadlib_lua::NativeCAbiLoader::statistics();
    let values = [
        stats.states_created,
        stats.states_destroyed,
        stats.groups_created,
        stats.groups_destroyed,
        stats.operations,
        stats.returned,
        stats.libraries_opened,
        stats.libraries_closed,
    ];
    if count >= values.len() && !output.is_null() {
        // SAFETY: the private diagnostic caller lends count writable elements.
        unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), output, values.len()) };
    }
    values.len()
}

include!(concat!(env!("OUT_DIR"), "/exports.rs"));
