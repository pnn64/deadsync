#[cfg(windows)]
pub(super) use super::RawKeyboardEvent;
pub(super) use super::{
    GpSystemEvent, PadBackend, PadCode, PadDir, PadEvent, PadId, uuid_from_bytes,
};

#[cfg(target_os = "linux")]
pub(super) mod linux_evdev;
#[cfg(target_os = "macos")]
pub(super) mod macos_iohid;
#[cfg(windows)]
pub(super) mod windows_raw_input;
#[cfg(windows)]
pub(super) mod windows_wgi;
