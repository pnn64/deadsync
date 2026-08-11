#[cfg(target_os = "freebsd")]
mod freebsd_pcm;
mod launch;
#[cfg(target_os = "linux")]
mod linux_alsa;
#[cfg(target_os = "linux")]
mod linux_jack;
#[cfg(target_os = "linux")]
#[cfg(has_pipewire_audio)]
mod linux_pipewire;
#[cfg(target_os = "linux")]
mod linux_pulse;
#[cfg(target_os = "macos")]
mod macos_coreaudio;
mod telemetry;
#[cfg(windows)]
mod windows_wasapi;

#[cfg(target_os = "linux")]
pub use launch::available_linux_backends;
pub use launch::{OutputPlan, prepare_output};
