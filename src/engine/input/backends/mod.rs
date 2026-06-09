#[cfg(windows)]
pub(super) use deadsync_input::backend::RawKeyboardEvent;
pub(super) use deadsync_input::backend::{
    BackendHost, GpSystemEvent, PadBackend, emit_dir_edges, uuid_from_bytes,
};
pub(super) use deadsync_input::{PadCode, PadEvent, PadId};

#[inline(always)]
pub(super) fn host() -> BackendHost {
    BackendHost::new(
        crate::config::pad_index_for_uuid,
        crate::engine::smx::native_smx_owns_device,
        crate::engine::host_time::now_nanos,
        crate::engine::host_time::instant_nanos,
        qpc_ticks_to_nanos,
    )
}

#[cfg(windows)]
#[inline(always)]
fn qpc_ticks_to_nanos(ticks: u64) -> Option<u64> {
    crate::engine::windows_rt::qpc_ticks_to_nanos(ticks)
}

#[cfg(not(windows))]
#[inline(always)]
const fn qpc_ticks_to_nanos(_ticks: u64) -> Option<u64> {
    None
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(super) use deadsync_input::backend::evdev;
#[cfg(target_os = "freebsd")]
pub(super) mod hidraw;
#[cfg(target_os = "macos")]
pub(super) mod iohid;
#[cfg(windows)]
pub(super) mod w32_raw_input;
#[cfg(all(windows, not(target_vendor = "win7")))]
pub(super) mod wgi;
