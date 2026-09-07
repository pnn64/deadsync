//! Audio output backends, realtime mixing, and buffered stream decoding.

mod launch;

pub use deadlib_audio_core::*;
pub use launch::LinuxAudioBackend;
#[cfg(target_os = "linux")]
pub use launch::available_linux_backends;
pub use launch::{InitConfig, OutputPlan, prepare_output};

pub mod stream;

#[cfg(test)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
