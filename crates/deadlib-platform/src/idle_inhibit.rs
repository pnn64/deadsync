//! Platform idle-display inhibition.
//!
//! This is process/window policy: while `DeadSync` owns the display, the desktop
//! should not blank or power it down just because input is arriving through a
//! raw device path.

#[cfg(all(unix, not(target_os = "macos")))]
mod unix;

#[derive(Default)]
pub struct IdleInhibitor {
    #[cfg(all(unix, not(target_os = "macos")))]
    _inner: Option<unix::IdleInhibitor>,
}

impl IdleInhibitor {
    #[must_use]
    // The Unix implementation performs runtime I/O and cannot be `const`.
    #[allow(clippy::missing_const_for_fn)]
    pub fn acquire() -> Self {
        Self {
            #[cfg(all(unix, not(target_os = "macos")))]
            _inner: unix::IdleInhibitor::acquire(),
        }
    }
}
