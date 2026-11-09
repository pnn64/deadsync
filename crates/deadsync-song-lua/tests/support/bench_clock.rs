//! Existing optional Windows benchmark clock and affinity helpers.
#[cfg(windows)]
pub(crate) fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut value = 0;
    // SAFETY: the pseudo-handle refers to this live thread and the output
    // pointer is valid for the synchronous Windows call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
        0
    );
    value
}

#[cfg(windows)]
pub(crate) fn pin_cpu2() -> impl Drop {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
    }
    struct Affinity(usize);
    impl Drop for Affinity {
        fn drop(&mut self) {
            // SAFETY: restore the affinity returned for this same live thread.
            unsafe {
                SetThreadAffinityMask(GetCurrentThread(), self.0);
            }
        }
    }
    // SAFETY: CPU 2 is present on the measurement host. Windows rejects an
    // unavailable processor rather than accepting an invalid mask.
    let previous = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) };
    assert_ne!(previous, 0, "benchmark requires logical CPU 2");
    Affinity(previous)
}
