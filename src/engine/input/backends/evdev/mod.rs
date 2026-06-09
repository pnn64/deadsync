pub(super) use super::{
    GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, emit_dir_edges, uuid_from_bytes,
};
#[cfg(target_os = "freebsd")]
pub(super) use deadsync_input::backend::devd::{DevdEvent, DevdWatch};
pub(super) use deadsync_input::backend::unix_time::{EventTimeSample, event_time};

#[inline(always)]
pub(super) fn receipt_time() -> EventTimeSample {
    deadsync_input::backend::unix_time::receipt_time(crate::engine::host_time::instant_nanos)
}

#[cfg(target_os = "freebsd")]
mod freebsd;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "freebsd")]
pub use freebsd::{
    keyboard_backend_active, run, run_pad_only, set_keyboard_capture_enabled,
    set_keyboard_window_focused,
};
#[cfg(target_os = "linux")]
pub use linux::{
    keyboard_backend_active, run, run_pad_only, set_keyboard_capture_enabled,
    set_keyboard_window_focused,
};
