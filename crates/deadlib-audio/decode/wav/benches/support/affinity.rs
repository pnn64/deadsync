// Validation-only thread placement; does not control frequency or other processes.
#[cfg(windows)]
pub fn pin_thread() -> usize {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn GetProcessAffinityMask(
            p: *mut core::ffi::c_void,
            process: *mut usize,
            system: *mut usize,
        ) -> i32;
        fn SetThreadAffinityMask(t: *mut core::ffi::c_void, mask: usize) -> usize;
    }
    let (mut process, mut system) = (0usize, 0usize);
    // SAFETY: pseudo-handles are valid, both outputs are writable, and the
    // selected CPU belongs to the process mask in its current processor group.
    unsafe {
        assert_ne!(
            GetProcessAffinityMask(GetCurrentProcess(), &mut process, &mut system),
            0
        );
        assert_ne!(process, 0);
        let bit = 1usize << (usize::BITS - 1 - process.leading_zeros());
        assert_ne!(SetThreadAffinityMask(GetCurrentThread(), bit), 0);
        bit
    }
}
#[cfg(not(windows))]
pub fn pin_thread() -> usize {
    0
}
