use deadsync_input_native::{BackendHost, InputThreadPolicy};

#[inline(always)]
pub(super) fn host() -> BackendHost {
    BackendHost::new(
        crate::config::pad_index_for_uuid,
        crate::engine::smx::native_smx_owns_device,
        crate::engine::host_time::now_nanos,
        crate::engine::host_time::instant_nanos,
        qpc_ticks_to_nanos,
        boost_input_thread,
    )
}

#[cfg(windows)]
#[inline(always)]
fn qpc_ticks_to_nanos(ticks: u64) -> Option<u64> {
    crate::engine::windows_rt::qpc_ticks_to_nanos(ticks)
}

#[cfg(not(windows))]
#[inline(always)]
const fn qpc_ticks_to_nanos(_ticks: u64) -> Option<u64> {
    None
}

#[cfg(windows)]
#[inline(always)]
fn boost_input_thread() -> InputThreadPolicy {
    let token = crate::engine::windows_rt::boost_current_thread(
        crate::engine::windows_rt::ThreadRole::Input,
    )
    .into_mmcss_token();
    InputThreadPolicy::new(token, restore_input_thread)
}

#[cfg(windows)]
#[inline(always)]
fn restore_input_thread(token: usize) {
    crate::engine::windows_rt::restore_thread_policy_token(token);
}

#[cfg(not(windows))]
#[inline(always)]
const fn boost_input_thread() -> InputThreadPolicy {
    InputThreadPolicy::none()
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(super) use deadsync_input_native::evdev;
#[cfg(target_os = "freebsd")]
pub(super) use deadsync_input_native::hidraw;
#[cfg(target_os = "macos")]
pub(super) use deadsync_input_native::iohid;
#[cfg(windows)]
pub(super) use deadsync_input_native::w32_raw_input;
#[cfg(all(windows, not(target_vendor = "win7")))]
pub(super) use deadsync_input_native::wgi;
