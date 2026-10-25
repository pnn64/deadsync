//! Platform operations with a safe boundary around native OS state.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Calendar {
    pub fields: [i32; 9],
}

pub(super) fn timestamp(number: f64) -> i64 {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    if posix_calendar::time_is_32_bit() {
        return i64::from(super::debug::integer(number));
    }
    super::io::integer(number)
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
#[allow(unsafe_code)]
mod posix_calendar;

pub(super) fn now() -> Result<Option<i64>, super::Error> {
    #[cfg(windows)]
    {
        Ok(windows::now())
    }
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        Ok(posix_calendar::now())
    }
    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple"
    )))]
    {
        Err(super::Error::new(
            super::ErrorKind::Unsupported,
            0,
            "calendar time is unsupported on this platform",
        ))
    }
}
pub(super) fn make_time(date: Calendar) -> Result<Option<i64>, super::Error> {
    #[cfg(windows)]
    {
        Ok(windows::make_time(date))
    }
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        Ok(posix_calendar::make_time(date))
    }
    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple"
    )))]
    {
        let _ = date;
        Err(super::Error::new(
            super::ErrorKind::Unsupported,
            0,
            "calendar time is unsupported on this platform",
        ))
    }
}
pub(super) fn date_parts(time: i64, utc: bool) -> Result<Option<Calendar>, super::Error> {
    #[cfg(windows)]
    {
        Ok(windows::date_parts(time, utc))
    }
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        Ok(posix_calendar::date_parts(time, utc))
    }
    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple"
    )))]
    {
        let _ = (time, utc);
        Err(super::Error::new(
            super::ErrorKind::Unsupported,
            0,
            "calendar time is unsupported on this platform",
        ))
    }
}
pub(super) fn format_date(
    time: i64,
    utc: bool,
    format: &[u8],
    output: &mut Vec<u8>,
) -> Result<bool, super::Error> {
    #[cfg(windows)]
    {
        Ok(windows::format_date(time, utc, format, output))
    }
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        Ok(posix_calendar::format_date(time, utc, format, output))
    }
    #[cfg(not(any(
        windows,
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple"
    )))]
    {
        let _ = (time, utc, format, output);
        Err(super::Error::new(
            super::ErrorKind::Unsupported,
            0,
            "calendar time is unsupported on this platform",
        ))
    }
}
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
