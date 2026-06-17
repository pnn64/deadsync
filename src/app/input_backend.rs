use deadsync_input_native::{BackendHost, InputThreadPolicy};

use crate::config;

#[inline(always)]
pub(super) fn host() -> BackendHost {
    BackendHost::new(
        config::pad_index_for_uuid,
        |vendor, product| {
            deadsync_smx::native_smx_owns_device(vendor, product, config::get().smx_input)
        },
        deadlib_platform::host_time::now_nanos,
        deadlib_platform::host_time::instant_nanos,
        qpc_ticks_to_nanos,
        boost_input_thread,
    )
}

#[cfg(windows)]
#[inline(always)]
fn qpc_ticks_to_nanos(ticks: u64) -> Option<u64> {
    deadlib_platform::windows_rt::qpc_ticks_to_nanos(ticks)
}

#[cfg(not(windows))]
#[inline(always)]
const fn qpc_ticks_to_nanos(_ticks: u64) -> Option<u64> {
    None
}

#[cfg(windows)]
#[inline(always)]
fn boost_input_thread() -> InputThreadPolicy {
    let token = deadlib_platform::windows_rt::boost_current_thread(
        deadlib_platform::windows_rt::ThreadRole::Input,
    )
    .into_mmcss_token();
    InputThreadPolicy::new(token, restore_input_thread)
}

#[cfg(windows)]
#[inline(always)]
fn restore_input_thread(token: usize) {
    deadlib_platform::windows_rt::restore_thread_policy_token(token);
}

#[cfg(not(windows))]
#[inline(always)]
const fn boost_input_thread() -> InputThreadPolicy {
    InputThreadPolicy::none()
}
