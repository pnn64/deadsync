//! POSIX time conversion; native tm timezone pointers never leave this module.
use super::Calendar;
use std::{
    ffi::{c_char, c_long},
    ptr,
};
#[cfg(target_env = "musl")]
type Time = i64;
#[cfg(not(target_env = "musl"))]
type Time = c_long;
pub(super) const fn time_is_32_bit() -> bool {
    std::mem::size_of::<Time>() == 4
}
#[repr(C)]
#[derive(Default)]
struct NativeDate {
    parts: Calendar,
    offset: c_long,
    zone: *const c_char,
}
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<NativeDate>() == 56);
    assert!(std::mem::offset_of!(NativeDate, offset) == 40);
    assert!(std::mem::offset_of!(NativeDate, zone) == 48);
};
#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(std::mem::size_of::<NativeDate>() == 44);
    assert!(std::mem::offset_of!(NativeDate, offset) == 36);
    assert!(std::mem::offset_of!(NativeDate, zone) == 40);
};
unsafe extern "C" {
    #[cfg_attr(
        all(target_env = "musl", target_pointer_width = "32"),
        link_name = "__time64"
    )]
    fn time(out: *mut Time) -> Time;
    #[cfg_attr(
        all(target_env = "musl", target_pointer_width = "32"),
        link_name = "__mktime64"
    )]
    fn mktime(date: *mut NativeDate) -> Time;
    #[cfg_attr(
        all(target_env = "musl", target_pointer_width = "32"),
        link_name = "__gmtime64_r"
    )]
    fn gmtime_r(time: *const Time, out: *mut NativeDate) -> *mut NativeDate;
    #[cfg_attr(
        all(target_env = "musl", target_pointer_width = "32"),
        link_name = "__localtime64_r"
    )]
    fn localtime_r(time: *const Time, out: *mut NativeDate) -> *mut NativeDate;
    fn strftime(
        out: *mut c_char,
        size: usize,
        format: *const c_char,
        date: *const NativeDate,
    ) -> usize;
}
fn converted(value: i64, utc: bool) -> Option<NativeDate> {
    let value = value as Time;
    let mut date = NativeDate::default();
    // SAFETY: platform time_t and tm records are owned, initialized and live.
    // Reentrant conversion fills only the provided native record.
    let out = unsafe {
        if utc {
            gmtime_r(&value, &mut date)
        } else {
            localtime_r(&value, &mut date)
        }
    };
    (!out.is_null()).then_some(date)
}
pub(super) fn now() -> Option<i64> {
    // SAFETY: the optional output pointer is null; only the scalar is returned.
    let value = unsafe { time(ptr::null_mut()) };
    (value != -1).then_some(value as i64)
}
pub(super) fn make_time(parts: Calendar) -> Option<i64> {
    let mut date = NativeDate {
        parts,
        ..NativeDate::default()
    };
    // SAFETY: the owned native tm has all input fields initialized; outputs stay
    // inside this module and no borrowed Lua storage is exposed to C.
    let value = unsafe { mktime(&mut date) };
    (value != -1).then_some(value as i64)
}
pub(super) fn date_parts(value: i64, utc: bool) -> Option<Calendar> {
    converted(value, utc).map(|date| date.parts)
}
pub(super) fn format_date(value: i64, utc: bool, format: &[u8], output: &mut Vec<u8>) -> bool {
    let Some(date) = converted(value, utc) else {
        return false;
    };
    super::super::calendar::format_with(format, output, |spec, bytes| {
        // SAFETY: the freshly converted native tm and its platform timezone
        // context remain inside this call. Both byte buffers have valid bounds
        // and the conversion string is NUL-terminated. No Lua callback occurs.
        unsafe {
            strftime(
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                spec.as_ptr().cast(),
                &date,
            )
        }
    });
    true
}
