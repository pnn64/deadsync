#[cfg(target_os = "freebsd")]
pub(super) use super::devd::{DevdEvent, DevdWatch};
pub(super) use super::{
    GpSystemEvent, PadBackend, PadCode, PadEvent, PadId, emit_dir_edges, uuid_from_bytes,
};

#[cfg(target_os = "freebsd")]
mod freebsd;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "freebsd")]
pub use freebsd::run;
#[cfg(target_os = "linux")]
pub use linux::run;
