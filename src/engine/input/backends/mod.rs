#[cfg(windows)]
pub(super) use deadsync_input::backend::RawKeyboardEvent;
pub(super) use deadsync_input::backend::{
    GpSystemEvent, PadBackend, emit_dir_edges, uuid_from_bytes,
};
pub(super) use deadsync_input::{PadCode, PadEvent, PadId};

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(super) mod evdev;
#[cfg(target_os = "freebsd")]
pub(super) mod hidraw;
#[cfg(target_os = "macos")]
pub(super) mod iohid;
#[cfg(windows)]
pub(super) mod w32_raw_input;
#[cfg(all(windows, not(target_vendor = "win7")))]
pub(super) mod wgi;
