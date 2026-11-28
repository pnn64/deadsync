//! Hardware cycles for the benchmark thread on Linux x86-64.
use std::{
    ffi::{c_int, c_long, c_ulong, c_void},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    sync::OnceLock,
};

const PERF_EVENT_OPEN: c_long = 298;
const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
const PERF_FLAG_FD_CLOEXEC: c_ulong = 1 << 3;
const EXCLUDE_KERNEL_AND_HYPERVISOR: u64 = (1 << 5) | (1 << 6);

// Linux perf_event_attr version 0. Later fields are optional for this event.
#[repr(C)]
struct Event {
    kind: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    breakpoint_type: u32,
    config1: u64,
}
const _: () = assert!(size_of::<Event>() == 64);

unsafe extern "C" {
    fn syscall(number: c_long, ...) -> c_long;
    fn read(fd: c_int, buffer: *mut c_void, bytes: usize) -> isize;
}

fn open() -> Option<OwnedFd> {
    let event = Event {
        kind: PERF_TYPE_HARDWARE,
        size: size_of::<Event>() as u32,
        config: PERF_COUNT_HW_CPU_CYCLES,
        sample_period: 0,
        sample_type: 0,
        read_format: 0,
        flags: EXCLUDE_KERNEL_AND_HYPERVISOR,
        wakeup_events: 0,
        breakpoint_type: 0,
        config1: 0,
    };
    // SAFETY: Linux reads the versioned event descriptor during this call.
    // Measure the current thread on any CPU with a standalone event.
    let fd = unsafe {
        syscall(
            PERF_EVENT_OPEN,
            &raw const event,
            0,
            -1,
            -1,
            PERF_FLAG_FD_CLOEXEC,
        )
    };
    let fd = c_int::try_from(fd).ok().filter(|fd| *fd >= 0)?;
    // SAFETY: A successful perf_event_open returns a new owned descriptor.
    Some(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub(super) fn cycles() -> Option<u64> {
    // The driver reads both snapshots from the same benchmark thread.
    static COUNTER: OnceLock<Option<OwnedFd>> = OnceLock::new();
    let counter = COUNTER.get_or_init(open).as_ref()?;
    let mut cycles = 0u64;
    // SAFETY: The counter is open and cycles is writable for one u64.
    let bytes = unsafe {
        read(
            counter.as_raw_fd(),
            (&raw mut cycles).cast(),
            size_of::<u64>(),
        )
    };
    (bytes == size_of::<u64>() as isize).then_some(cycles)
}
