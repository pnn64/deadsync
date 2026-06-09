#[cfg(target_os = "freebsd")]
pub(super) use super::devd::{DevdEvent, DevdWatch};
pub(super) use super::unix_time::{EventTimeSample, event_time};
pub(super) use super::{BackendHost, GpSystemEvent, PadBackend, emit_dir_edges, uuid_from_bytes};
pub(super) use crate::{PadCode, PadEvent, PadId};

#[inline(always)]
pub(super) fn receipt_time(host: BackendHost) -> EventTimeSample {
    super::unix_time::receipt_time(|instant| host.instant_nanos(instant))
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
