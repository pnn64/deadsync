//! Minimal Windows process ABI. No Lua values or VM storage cross this boundary.
use std::{
    ffi::{OsStr, c_void},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr,
};

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

pub(super) fn spawn(path: &Path, name: &OsStr, command: &[u8]) -> Result<i32, i32> {
    let mut application = Vec::with_capacity(path.as_os_str().len() + 1);
    application.extend(path.as_os_str().encode_wide());
    application.push(0);
    let mut line = Vec::with_capacity(name.len() + command.len() + 5);
    // The CRT's system call joins argv verbatim; normal Rust argument quoting
    // changes the shell's command line and treats .cmd files specially.
    line.extend(name.encode_wide());
    line.extend([
        u16::from(b' '),
        u16::from(b'/'),
        u16::from(b'c'),
        u16::from(b' '),
    ]);
    line.extend(String::from_utf8_lossy(command).encode_utf16());
    line.push(0);
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
        return Err(super::super::io::errno(std::io::Error::last_os_error()));
    }
    let process = OwnedHandle(result.process);
    let thread = OwnedHandle(result.thread);
    drop(thread);
    // SAFETY: the process handle is owned and live; INFINITE waits for process
    // termination. The output pointer below references a writable u32.
    if unsafe { WaitForSingleObject(process.0, u32::MAX) } == u32::MAX {
        return Err(super::super::io::errno(std::io::Error::last_os_error()));
    }
    let mut code = 0;
    // SAFETY: the live owned process handle grants query access, and code is a
    // valid writable output. The RAII owner closes the handle on every return.
    if unsafe { GetExitCodeProcess(process.0, &mut code) } == 0 {
        return Err(super::super::io::errno(std::io::Error::last_os_error()));
    }
    Ok(code as i32)
}
