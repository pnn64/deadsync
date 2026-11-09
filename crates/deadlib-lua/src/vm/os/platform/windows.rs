//! Windows OS ABI. No Lua values or VM storage cross this boundary.
use crate::{Error, memory::fallible::VecExt};
use std::{ffi::c_void, ptr};

type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct StartupInfo {
    size: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    // The eight DWORDs from dwX through dwFlags, in Windows ABI order.
    window: [u32; 8],
    show: u16,
    reserved_len: u16,
    reserved_bytes: *mut u8,
    standard: [Handle; 3],
}
#[repr(C)]
#[derive(Default)]
struct ProcessInfo {
    process: Handle,
    thread: Handle,
    process_id: u32,
    thread_id: u32,
}
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(std::mem::size_of::<StartupInfo>() == 104);
    assert!(std::mem::offset_of!(StartupInfo, standard) == 80);
    assert!(std::mem::size_of::<ProcessInfo>() == 24);
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_attributes: *const c_void,
        thread_attributes: *const c_void,
        inherit: i32,
        flags: u32,
        environment: *const c_void,
        directory: *const u16,
        startup: *const StartupInfo,
        result: *mut ProcessInfo,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(handle: Handle, code: *mut u32) -> i32;
}

unsafe extern "C" {
    #[link_name = "clock"]
    fn crt_clock() -> i32;
    fn _time64(result: *mut i64) -> i64;
    fn _mktime64(date: *mut super::Calendar) -> i64;
    fn _gmtime64(time: *const i64) -> *mut super::Calendar;
    fn _localtime64(time: *const i64) -> *mut super::Calendar;
    fn strftime(
        output: *mut std::ffi::c_char,
        size: usize,
        format: *const std::ffi::c_char,
        date: *const super::Calendar,
    ) -> usize;
}

const _: () = assert!(std::mem::size_of::<super::Calendar>() == 36);

pub(super) fn now() -> Option<i64> {
    // SAFETY: a null optional output requests a scalar CRT time result.
    let time = unsafe { _time64(ptr::null_mut()) };
    (time != -1).then_some(time)
}
pub(super) fn make_time(mut date: super::Calendar) -> Option<i64> {
    // SAFETY: Calendar is the nine-int Windows tm layout, fully initialized and
    // owned for the synchronous conversion. No Lua table memory is passed.
    let time = unsafe { _mktime64(&mut date) };
    (time != -1).then_some(time)
}
pub(super) fn date_parts(time: i64, utc: bool) -> Option<super::Calendar> {
    // SAFETY: input points to a live i64. The CRT result is thread-local tm
    // storage; copy it immediately before any other native or Lua callback.
    let date = unsafe {
        if utc {
            _gmtime64(&time)
        } else {
            _localtime64(&time)
        }
    };
    if date.is_null() {
        None
    } else {
        Some(unsafe { *date })
    }
}
pub(super) fn format_date(
    time: i64,
    utc: bool,
    format: &[u8],
    output: &mut impl crate::memory::bytes::ByteOutput,
) -> Result<bool, crate::Error> {
    let Some(date) = date_parts(time, utc) else {
        return Ok(false);
    };
    super::super::calendar::format_with(format, output, |spec, bytes| {
        // SAFETY: owned tm, a three-byte NUL-terminated conversion specifier and
        // a writable 200-byte buffer stay alive for the native formatter call.
        unsafe {
            strftime(
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                spec.as_ptr().cast(),
                &date,
            )
        }
    })?;
    Ok(true)
}

pub(in crate::vm::os) fn clock() -> f64 {
    // SAFETY: clock takes no arguments or pointers and uses CRT state
    // initialized before Rust entry. MSVC clock_t is signed 32-bit milliseconds.
    f64::from(unsafe { crt_clock() }) / 1000.0
}

struct OwnedHandle(Handle);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: constructed only from a successful CreateProcessW result;
        // each returned handle has exactly one owner and is closed once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub(in crate::vm::os) fn spawn(
    application: &[u16],
    name: &[u16],
    command: &[u8],
) -> Result<Result<i32, i32>, Error> {
    debug_assert_eq!(application.last(), Some(&0));
    let mut line = Vec::new();
    // The CRT's system call joins argv verbatim; normal Rust argument quoting
    // changes the shell's command line and treats .cmd files specially.
    line.try_extend_from_slice(name)?;
    line.try_extend([
        u16::from(b' '),
        u16::from(b'/'),
        u16::from(b'c'),
        u16::from(b' '),
    ])?;
    for chunk in command.utf8_chunks() {
        line.try_extend(chunk.valid().encode_utf16())?;
        if !chunk.invalid().is_empty() {
            line.try_push(0xfffd)?;
        }
    }
    line.try_push(0)?;
    let startup = StartupInfo {
        size: std::mem::size_of::<StartupInfo>() as u32,
        ..StartupInfo::default()
    };
    let mut result = ProcessInfo::default();
    // SAFETY: both strings are owned, NUL-terminated buffers alive for the call;
    // line is writable. ABI records have the Windows layout. Null optional
    // arguments request inherited environment/current directory and default
    // attributes. No pointer escapes into Rust or aliases VM storage.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            line.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            0,
            ptr::null(),
            ptr::null(),
            &startup,
            &mut result,
        )
    };
    if created == 0 {
        return Ok(Err(
            super::super::io::errno(std::io::Error::last_os_error()),
        ));
    }
    let process = OwnedHandle(result.process);
    let thread = OwnedHandle(result.thread);
    drop(thread);
    // SAFETY: the process handle is owned and live; INFINITE waits for process
    // termination. The output pointer below references a writable u32.
    if unsafe { WaitForSingleObject(process.0, u32::MAX) } == u32::MAX {
        return Ok(Err(
            super::super::io::errno(std::io::Error::last_os_error()),
        ));
    }
    let mut code = 0;
    // SAFETY: the live owned process handle grants query access, and code is a
    // valid writable output. The RAII owner closes the handle on every return.
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return Ok(Err(
            super::super::io::errno(std::io::Error::last_os_error()),
        ));
    }
    Ok(Ok(code as i32))
}
