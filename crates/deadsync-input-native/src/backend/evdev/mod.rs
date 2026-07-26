use super::deferred_sample::DeferredSample;
#[cfg(target_os = "freebsd")]
pub(super) use super::devd::{DevdEvent, DevdWatch};
use super::unix_time::{self, EventTimeSample};
pub(super) use super::{
    BackendHost, GpSystemEvent, PadBackend, PadOrderBackend, emit_dir_edges, uuid_from_bytes,
};
pub(super) use deadsync_input::{PadCode, PadEvent, PadId};

pub(super) struct ReceiptTime {
    sample: DeferredSample<EventTimeSample>,
}

impl ReceiptTime {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self {
            sample: DeferredSample::new(),
        }
    }

    #[inline(always)]
    pub(super) fn event_time(
        &mut self,
        host: BackendHost,
        sec: i64,
        usec: i64,
    ) -> (std::time::Instant, u64) {
        let sample = self
            .sample
            .get_or_init(|| unix_time::receipt_time(|instant| host.instant_nanos(instant)));
        unix_time::event_time(sample, sec, usec)
    }
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
