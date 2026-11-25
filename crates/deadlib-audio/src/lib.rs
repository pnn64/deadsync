//! Audio output backends, realtime mixing, and buffered stream decoding.

mod launch;

pub use deadlib_audio_core::*;
#[cfg(target_os = "linux")]
pub use launch::available_linux_backends;
pub use launch::{InitConfig, OutputPlan, prepare_output};

pub mod stream;

/// Sample the host clock used by the active audio callback with its timing window.
#[must_use]
pub fn callback_clock_snapshot_now() -> (std::time::Instant, u64, CallbackClockSource, CallbackClockWindow) {
    load_callback_clock_snapshot_now(|valid_at, source| match source {
        CallbackClockSource::Instant => Some(deadlib_platform::host_time::instant_nanos(valid_at)),
        #[cfg(windows)]
        CallbackClockSource::Qpc => deadlib_platform::windows_rt::current_qpc_nanos(),
    })
}
