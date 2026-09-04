use std::time::Instant;

use deadsync_input::{PadDir, PadEvent, PadId};

#[cfg(any(target_os = "linux", target_os = "freebsd", test))]
mod deferred_sample;
#[cfg(target_os = "freebsd")]
pub mod devd;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod evdev;
#[cfg(any(target_os = "freebsd", test))]
mod hid_report_cache;
#[cfg(target_os = "freebsd")]
pub mod hidraw;
#[cfg(target_os = "macos")]
pub mod iohid;
#[cfg(any(target_os = "macos", test))]
mod iohid_filter;
#[cfg(any(target_os = "linux", target_os = "freebsd", test))]
mod poll_registration;
#[cfg(any(unix, test))]
mod unix_time;
#[cfg(windows)]
pub mod w32_raw_input;
#[cfg(all(windows, not(target_vendor = "win7")))]
pub mod wgi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadBackend {
    #[cfg(windows)]
    WindowsRawInput,
    #[cfg(windows)]
    WindowsWgi,
    #[cfg(target_os = "linux")]
    LinuxEvdev,
    #[cfg(target_os = "freebsd")]
    FreeBsdHidraw,
    #[cfg(target_os = "freebsd")]
    FreeBsdEvdev,
    #[cfg(target_os = "macos")]
    MacOsIohid,
    /// `StepManiaX` pad via the `RustManiaX` SDK (all platforms).
    Smx,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowsPadBackend {
    /// Choose the default Windows backend (currently Raw Input).
    Auto,
    #[default]
    RawInput,
    Wgi,
}

impl WindowsPadBackend {
    #[inline(always)]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::RawInput => "RawInput",
            Self::Wgi => "WGI",
        }
    }
}

impl std::fmt::Display for WindowsPadBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for WindowsPadBackend {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        if s.eq_ignore_ascii_case("rawinput")
            || s.eq_ignore_ascii_case("raw_input")
            || s.eq_ignore_ascii_case("raw")
        {
            return Ok(Self::RawInput);
        }
        if s.eq_ignore_ascii_case("wgi")
            || s.eq_ignore_ascii_case("windowsgaminginput")
            || s.eq_ignore_ascii_case("gaminginput")
        {
            return Ok(Self::Wgi);
        }
        Err(())
    }
}

/// Input backends that persist a stable pad order. SMX is intentionally
/// excluded because it has its own serial-based assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PadOrderBackend {
    RawInput,
    Wgi,
    IoHid,
    Hidraw,
    LinuxEvdev,
    FreeBsdEvdev,
}

pub const PAD_ORDER_BACKENDS: [PadOrderBackend; 6] = [
    PadOrderBackend::RawInput,
    PadOrderBackend::Wgi,
    PadOrderBackend::IoHid,
    PadOrderBackend::Hidraw,
    PadOrderBackend::LinuxEvdev,
    PadOrderBackend::FreeBsdEvdev,
];

#[derive(Clone, Copy)]
pub struct BackendHost {
    pad_index_for_uuid: fn(PadOrderBackend, [u8; 16]) -> u32,
    native_smx_owns_device: fn(Option<u16>, Option<u16>) -> bool,
    now_nanos: fn() -> u64,
    instant_nanos: fn(Instant) -> u64,
    qpc_ticks_to_nanos: fn(u64) -> Option<u64>,
    boost_input_thread: fn() -> InputThreadPolicy,
}

impl BackendHost {
    #[inline(always)]
    pub const fn new(
        pad_index_for_uuid: fn(PadOrderBackend, [u8; 16]) -> u32,
        native_smx_owns_device: fn(Option<u16>, Option<u16>) -> bool,
        now_nanos: fn() -> u64,
        instant_nanos: fn(Instant) -> u64,
        qpc_ticks_to_nanos: fn(u64) -> Option<u64>,
        boost_input_thread: fn() -> InputThreadPolicy,
    ) -> Self {
        Self {
            pad_index_for_uuid,
            native_smx_owns_device,
            now_nanos,
            instant_nanos,
            qpc_ticks_to_nanos,
            boost_input_thread,
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn pad_id_for_uuid(self, backend: PadOrderBackend, uuid: [u8; 16]) -> PadId {
        PadId((self.pad_index_for_uuid)(backend, uuid))
    }

    #[inline(always)]
    #[must_use]
    pub fn native_smx_owns_device(self, vendor: Option<u16>, product: Option<u16>) -> bool {
        (self.native_smx_owns_device)(vendor, product)
    }

    #[inline(always)]
    #[must_use]
    pub fn now_nanos(self) -> u64 {
        (self.now_nanos)()
    }

    #[inline(always)]
    #[must_use]
    pub fn instant_nanos(self, at: Instant) -> u64 {
        (self.instant_nanos)(at)
    }

    #[inline(always)]
    #[must_use]
    pub fn sample_time(self) -> (Instant, u64) {
        let timestamp = Instant::now();
        // Audio snapshots use the authoritative host clock; `instant_nanos` may
        // have a different epoch (notably process-relative `Instant` vs QPC on Windows).
        (timestamp, self.now_nanos())
    }

    #[inline(always)]
    #[must_use]
    pub fn qpc_ticks_to_nanos(self, ticks: u64) -> Option<u64> {
        (self.qpc_ticks_to_nanos)(ticks)
    }

    #[inline(always)]
    #[must_use]
    pub fn boost_input_thread(self) -> InputThreadPolicy {
        (self.boost_input_thread)()
    }
}

#[inline(always)]
pub fn backend_host(
    pad_index_for_uuid: fn(PadOrderBackend, [u8; 16]) -> u32,
    native_smx_owns_device: fn(Option<u16>, Option<u16>) -> bool,
) -> BackendHost {
    BackendHost::new(
        pad_index_for_uuid,
        native_smx_owns_device,
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

pub struct InputThreadPolicy {
    token: usize,
    restore: fn(usize),
}

impl InputThreadPolicy {
    #[inline(always)]
    #[must_use]
    pub const fn none() -> Self {
        Self {
            token: 0,
            restore: noop_input_thread_policy,
        }
    }

    #[inline(always)]
    pub const fn new(token: usize, restore: fn(usize)) -> Self {
        Self { token, restore }
    }
}

impl Drop for InputThreadPolicy {
    #[inline(always)]
    fn drop(&mut self) {
        if self.token != 0 {
            (self.restore)(self.token);
        }
    }
}

#[inline(always)]
#[must_use]
pub fn uuid_from_bytes(bytes: &[u8]) -> [u8; 16] {
    // Deterministic, fast, and tiny: two FNV-1a 64-bit passes with different offsets.
    const OFF0: u64 = 0xcbf2_9ce4_8422_2325;
    const OFF1: u64 = 0xaf63_dc4c_8601_ec8c;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    #[inline(always)]
    fn fnv64(mut h: u64, bytes: &[u8]) -> u64 {
        let mut i = 0;
        while i < bytes.len() {
            h ^= u64::from(bytes[i]);
            h = h.wrapping_mul(PRIME);
            i += 1;
        }
        h
    }

    let a = fnv64(OFF0, bytes);
    let b = fnv64(OFF1, bytes);
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&a.to_le_bytes());
    out[8..].copy_from_slice(&b.to_le_bytes());
    out
}

#[inline(always)]
const fn noop_input_thread_policy(_token: usize) {}

#[derive(Clone, Debug)]
pub enum GpSystemEvent {
    Connected {
        name: String,
        id: PadId,
        vendor_id: Option<u16>,
        product_id: Option<u16>,
        backend: PadBackend,
        /// True when this connection is part of startup enumeration.
        initial: bool,
    },
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    Disconnected {
        name: String,
        id: PadId,
        backend: PadBackend,
        /// True when this disconnect is part of startup enumeration.
        initial: bool,
    },
    StartupComplete,
}

const DIRS: [PadDir; 4] = [PadDir::Up, PadDir::Down, PadDir::Left, PadDir::Right];

#[inline(always)]
pub fn emit_dir_edges(
    emit_pad: &mut impl FnMut(PadEvent),
    id: PadId,
    dir_state: &mut [bool; 4],
    timestamp: Instant,
    host_nanos: u64,
    want: [bool; 4],
) {
    for i in 0..DIRS.len() {
        if dir_state[i] == want[i] {
            continue;
        }
        dir_state[i] = want[i];
        emit_pad(PadEvent::Dir {
            id,
            timestamp,
            host_nanos,
            dir: DIRS[i],
            pressed: want[i],
        });
    }
}

#[inline(always)]
pub fn emit_hat_axis_edges(
    emit_pad: &mut impl FnMut(PadEvent),
    id: PadId,
    dir_state: &mut [bool; 4],
    timestamp: Instant,
    host_nanos: u64,
    horizontal: bool,
    value: i32,
) {
    let first = [0, 2][usize::from(horizontal)];
    let want = [value < 0, value > 0];
    for (offset, &pressed) in want.iter().enumerate() {
        let index = first + offset;
        if dir_state[index] == pressed {
            continue;
        }
        dir_state[index] = pressed;
        emit_pad(PadEvent::Dir {
            id,
            timestamp,
            host_nanos,
            dir: DIRS[index],
            pressed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_time_sample_uses_authoritative_host_clock() {
        fn pad_idx(_: PadOrderBackend, _: [u8; 16]) -> u32 {
            0
        }
        fn smx_owns(_: Option<u16>, _: Option<u16>) -> bool {
            false
        }
        fn now() -> u64 {
            9_876_543_210
        }
        fn instant_nanos(_: Instant) -> u64 {
            123
        }
        fn qpc(_: u64) -> Option<u64> {
            None
        }
        fn boost() -> InputThreadPolicy {
            InputThreadPolicy::none()
        }

        let host = BackendHost::new(pad_idx, smx_owns, now, instant_nanos, qpc, boost);
        let before = Instant::now();
        let (timestamp, host_nanos) = host.sample_time();
        let after = Instant::now();

        assert!((before..=after).contains(&timestamp));
        assert_eq!(host_nanos, now());
        assert_ne!(host_nanos, instant_nanos(timestamp));
    }

    #[test]
    fn emit_dir_edges_updates_only_changed_dirs() {
        let mut events = Vec::new();
        let timestamp = Instant::now();
        let mut dir_state = [false; 4];

        emit_dir_edges(
            &mut |event| events.push(event),
            PadId(7),
            &mut dir_state,
            timestamp,
            42,
            [true, false, true, false],
        );
        assert_eq!(dir_state, [true, false, true, false]);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[0],
            PadEvent::Dir {
                id: PadId(7),
                timestamp: ts,
                host_nanos: 42,
                dir: PadDir::Up,
                pressed: true,
            } if ts == timestamp
        ));
        assert!(matches!(
            events[1],
            PadEvent::Dir {
                id: PadId(7),
                timestamp: ts,
                host_nanos: 42,
                dir: PadDir::Left,
                pressed: true,
            } if ts == timestamp
        ));

        events.clear();
        emit_dir_edges(
            &mut |event| events.push(event),
            PadId(7),
            &mut dir_state,
            timestamp,
            42,
            [true, false, true, false],
        );
        assert!(events.is_empty());
    }

    #[test]
    fn hat_axis_edges_match_full_direction_scan() {
        let timestamp = Instant::now();
        let mut full_state = [false; 4];
        let mut axis_state = [false; 4];
        let mut full_events = Vec::new();
        let mut axis_events = Vec::new();
        let mut x = 0;
        let mut y = 0;

        for (horizontal, value) in [
            (true, -1),
            (false, -1),
            (true, 1),
            (false, 1),
            (true, 0),
            (false, 0),
            (false, 0),
            (true, 0),
        ] {
            if horizontal {
                x = value;
            } else {
                y = value;
            }
            emit_dir_edges(
                &mut |event| {
                    let PadEvent::Dir {
                        id,
                        timestamp,
                        host_nanos,
                        dir,
                        pressed,
                    } = event
                    else {
                        panic!("direction helper emitted a non-direction event");
                    };
                    full_events.push((id, timestamp, host_nanos, dir, pressed));
                },
                PadId(9),
                &mut full_state,
                timestamp,
                77,
                [y < 0, y > 0, x < 0, x > 0],
            );
            emit_hat_axis_edges(
                &mut |event| {
                    let PadEvent::Dir {
                        id,
                        timestamp,
                        host_nanos,
                        dir,
                        pressed,
                    } = event
                    else {
                        panic!("hat helper emitted a non-direction event");
                    };
                    axis_events.push((id, timestamp, host_nanos, dir, pressed));
                },
                PadId(9),
                &mut axis_state,
                timestamp,
                77,
                horizontal,
                value,
            );
        }

        assert_eq!(axis_state, full_state);
        assert_eq!(axis_events, full_events);
    }
}
