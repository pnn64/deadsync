//! Platform operations with a safe boundary around native OS state.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;
#[cfg(windows)]
pub(super) use windows::{clock, rename, spawn};

#[cfg(not(windows))]
pub(super) fn rename(from: &std::path::Path, to: &std::path::Path) -> Result<(), i32> {
    std::fs::rename(from, to).map_err(super::io::errno)
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[allow(unsafe_code)]
mod posix_clock {
    #[cfg(target_vendor = "apple")]
    type Clock = std::ffi::c_ulong;
    #[cfg(not(target_vendor = "apple"))]
    type Clock = std::ffi::c_long;
    unsafe extern "C" {
        #[link_name = "clock"]
        fn native_clock() -> Clock;
    }
    pub(in crate::vm::os) fn clock() -> f64 {
        // SAFETY: the platform clock() takes no arguments or pointers. These
        // targets use the clock_t widths above and one million ticks per second.
        unsafe { native_clock() as f64 / 1_000_000.0 }
    }
}
#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
pub(super) use posix_clock::clock;
