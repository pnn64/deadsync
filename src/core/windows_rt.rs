use log::{debug, warn};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Performance;
use windows::Win32::System::Threading::{
    self, AVRT_PRIORITY, AVRT_PRIORITY_CRITICAL, AVRT_PRIORITY_HIGH, AVRT_PRIORITY_NORMAL,
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY, THREAD_PRIORITY_ABOVE_NORMAL,
    THREAD_PRIORITY_HIGHEST,
};
use windows::core::w;

static QPC_FREQ_HZ: std::sync::LazyLock<Option<u64>> = std::sync::LazyLock::new(qpc_freq_hz);

#[derive(Clone, Copy)]
pub enum ThreadRole {
    Main,
    AudioRender,
    AudioDecode,
    Input,
}

struct ThreadProfile {
    label: &'static str,
    task_name: &'static str,
    mmcss_task: windows::core::PCWSTR,
    mmcss_priority: AVRT_PRIORITY,
    fallback_priority: THREAD_PRIORITY,
    fallback_label: &'static str,
}

impl ThreadRole {
    fn profile(self) -> ThreadProfile {
        match self {
            Self::Main => ThreadProfile {
                label: "main",
                task_name: "Games",
                mmcss_task: w!("Games"),
                mmcss_priority: AVRT_PRIORITY_HIGH,
                fallback_priority: THREAD_PRIORITY_HIGHEST,
                fallback_label: "THREAD_PRIORITY_HIGHEST",
            },
            Self::AudioRender => ThreadProfile {
                label: "audio_render",
                task_name: "Pro Audio",
                mmcss_task: w!("Pro Audio"),
                mmcss_priority: AVRT_PRIORITY_CRITICAL,
                fallback_priority: THREAD_PRIORITY_HIGHEST,
                fallback_label: "THREAD_PRIORITY_HIGHEST",
            },
            Self::AudioDecode => ThreadProfile {
                label: "audio_decode",
                task_name: "Audio",
                mmcss_task: w!("Audio"),
                mmcss_priority: AVRT_PRIORITY_NORMAL,
                fallback_priority: THREAD_PRIORITY_ABOVE_NORMAL,
                fallback_label: "THREAD_PRIORITY_ABOVE_NORMAL",
            },
            Self::Input => ThreadProfile {
                label: "input",
                task_name: "Games",
                mmcss_task: w!("Games"),
                mmcss_priority: AVRT_PRIORITY_HIGH,
                fallback_priority: THREAD_PRIORITY_HIGHEST,
                fallback_label: "THREAD_PRIORITY_HIGHEST",
            },
        }
    }
}

pub struct ThreadPolicyGuard {
    mmcss_handle: Option<HANDLE>,
}

impl Drop for ThreadPolicyGuard {
    fn drop(&mut self) {
        let Some(handle) = self.mmcss_handle.take() else {
            return;
        };
        unsafe {
            if let Err(e) = Threading::AvRevertMmThreadCharacteristics(handle) {
                warn!("Failed to leave MMCSS thread class: {e}");
            }
        }
    }
}

#[inline(always)]
fn apply_fallback_priority(profile: ThreadProfile) {
    unsafe {
        if let Err(e) = SetThreadPriority(GetCurrentThread(), profile.fallback_priority) {
            warn!(
                "Failed to set Windows thread priority for {} to {}: {e}",
                profile.label, profile.fallback_label
            );
        } else {
            debug!(
                "Applied Windows fallback thread priority for {}: {}",
                profile.label, profile.fallback_label
            );
        }
    }
}

pub fn boost_current_thread(role: ThreadRole) -> ThreadPolicyGuard {
    let profile = role.profile();
    let mut task_index = 0u32;
    let mmcss =
        unsafe { Threading::AvSetMmThreadCharacteristicsW(profile.mmcss_task, &mut task_index) };
    match mmcss {
        Ok(handle) => {
            unsafe {
                if let Err(e) = Threading::AvSetMmThreadPriority(handle, profile.mmcss_priority) {
                    warn!(
                        "Failed to set MMCSS priority for {} (task '{}'): {e}",
                        profile.label, profile.task_name
                    );
                } else {
                    debug!(
                        "Applied MMCSS thread policy for {} (task='{}', task_index={})",
                        profile.label, profile.task_name, task_index
                    );
                }
            }
            ThreadPolicyGuard {
                mmcss_handle: Some(handle),
            }
        }
        Err(e) => {
            warn!(
                "Failed to enter MMCSS for {} (task '{}'): {e}. Falling back to {}.",
                profile.label, profile.task_name, profile.fallback_label
            );
            apply_fallback_priority(profile);
            ThreadPolicyGuard { mmcss_handle: None }
        }
    }
}

#[inline(always)]
fn qpc_freq_hz() -> Option<u64> {
    unsafe {
        let mut hz = 0i64;
        Performance::QueryPerformanceFrequency(&mut hz).ok()?;
        u64::try_from(hz).ok().filter(|hz| *hz > 0)
    }
}

#[inline(always)]
pub(crate) fn qpc_ticks_to_nanos(ticks: u64) -> Option<u64> {
    let hz = (*QPC_FREQ_HZ)?;
    ((u128::from(ticks) * 1_000_000_000u128) / u128::from(hz))
        .try_into()
        .ok()
}

#[inline(always)]
pub(crate) fn current_qpc_nanos() -> Option<u64> {
    unsafe {
        let mut ticks = 0i64;
        Performance::QueryPerformanceCounter(&mut ticks).ok()?;
        qpc_ticks_to_nanos(u64::try_from(ticks).ok()?)
    }
}

#[inline(always)]
pub(crate) fn current_host_nanos() -> u64 {
    current_qpc_nanos().unwrap_or(0)
}
