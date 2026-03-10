pub mod cpal;
#[cfg(target_os = "freebsd")]
pub mod freebsd_pcm;
#[cfg(target_os = "linux")]
pub mod linux_alsa;
#[cfg(target_os = "linux")]
#[cfg(has_jack_audio)]
pub mod linux_jack;
#[cfg(target_os = "linux")]
#[cfg(has_pulse_audio)]
pub mod linux_pulse;
#[cfg(target_os = "macos")]
pub mod macos_coreaudio;
#[cfg(windows)]
pub mod windows_wasapi;
