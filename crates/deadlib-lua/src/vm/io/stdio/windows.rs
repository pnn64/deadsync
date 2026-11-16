//! Borrowed Windows standard handles with fixed storage for console encodings.
//! ReadConsoleW/WriteConsoleW use UTF-16; redirected files and pipes use bytes.
//! https://learn.microsoft.com/en-us/windows/console/readconsole
//! https://learn.microsoft.com/en-us/windows/console/writeconsole
use super::codec::{Input, Output};
use std::{ffi::c_void, io, ptr, sync::Mutex};

type Handle = *mut c_void;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(kind: u32) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn GetConsoleOutputCP() -> u32;
    fn GetFileType(handle: Handle) -> u32;
    fn GetFileInformationByHandleEx(
        handle: Handle,
        class: i32,
        output: *mut c_void,
        size: u32,
    ) -> i32;
    fn ReadFile(
        handle: Handle,
        output: *mut c_void,
        size: u32,
        count: *mut u32,
        overlap: *mut c_void,
    ) -> i32;
    fn WriteFile(
        handle: Handle,
        input: *const c_void,
        size: u32,
        count: *mut u32,
        overlap: *mut c_void,
    ) -> i32;
    fn ReadConsoleW(
        handle: Handle,
        output: *mut c_void,
        size: u32,
        count: *mut u32,
        control: *const ReadControl,
    ) -> i32;
    fn WriteConsoleW(
        handle: Handle,
        input: *const u16,
        size: u32,
        count: *mut u32,
        reserved: *mut c_void,
    ) -> i32;
    fn SetLastError(code: u32);
    fn GetLastError() -> u32;
}
#[repr(C)]
struct ReadControl {
    size: u32,
    initial: u32,
    wake: u32,
    state: u32,
}
#[repr(C)]
struct PipeName {
    bytes: u32,
    name: [u16; 260],
}

fn handle(kind: usize) -> io::Result<Handle> {
    // SAFETY: only the three documented standard-handle selectors are passed.
    // The process retains ownership; reacquiring follows SetStdHandle changes.
    let value = unsafe { GetStdHandle(0xfffffff6 - kind as u32) };
    if value.is_null() || value == (-1isize) as Handle {
        Err(io::Error::from_raw_os_error(6))
    } else {
        Ok(value)
    }
}
fn console(handle: Handle) -> bool {
    let mut mode = 0;
    // SAFETY: the borrowed handle is queried with a writable scalar output.
    unsafe { GetConsoleMode(handle, &mut mode) != 0 }
}
pub(in crate::vm::io) fn is_terminal(kind: usize) -> bool {
    let Ok(handle) = handle(kind) else {
        return false;
    };
    if console(handle) {
        return true;
    }
    // Preserve MSYS/Cygwin pseudo-terminal detection without a lossy String
    // allocation: their identifying portions are entirely ASCII UTF-16 units.
    if unsafe { GetFileType(handle) } != 3 {
        return false;
    }
    let mut name = PipeName {
        bytes: 0,
        name: [0; 260],
    };
    // SAFETY: repr(C) FILE_NAME_INFO with initialized, bounded inline storage.
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            2,
            (&raw mut name).cast(),
            size_of::<PipeName>() as u32,
        )
    } == 0
    {
        return false;
    }
    let Some(name) = name.name.get(..name.bytes as usize / 2) else {
        return false;
    };
    let name = name.rsplit(|&unit| unit == 92).next().unwrap_or(name);
    (name.starts_with(&b"msys-".map(u16::from)) || name.starts_with(&b"cygwin-".map(u16::from)))
        && name.windows(4).any(|part| part == b"-pty".map(u16::from))
}

static INPUT: Mutex<Input> = Mutex::new(Input::new());

fn read_console(handle: Handle, units: &mut [u16]) -> io::Result<usize> {
    let control = ReadControl {
        size: size_of::<ReadControl>() as u32,
        initial: 0,
        wake: 1 << 26,
        state: 0,
    };
    loop {
        let mut count = 0;
        // SAFETY: initialized UTF-16 buffer, writable count and a correctly
        // laid-out control record live for this synchronous console read.
        let success = unsafe {
            SetLastError(0);
            ReadConsoleW(
                handle,
                units.as_mut_ptr().cast(),
                units.len() as u32,
                &mut count,
                &control,
            )
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        // Ctrl-C can report success with no data and this error code.
        if count == 0 && unsafe { GetLastError() } == 995 {
            continue;
        }
        return Ok(count as usize);
    }
}

pub(in crate::vm::io) fn read(output: &mut [u8]) -> io::Result<usize> {
    let handle = handle(0)?;
    if console(handle) {
        return INPUT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .read_with(output, |units| read_console(handle, units));
    }
    let mut count = 0;
    // SAFETY: borrowed synchronous handle and exclusively writable byte slice;
    // the request is bounded by both its storage and Windows' DWORD count.
    let success = unsafe {
        ReadFile(
            handle,
            output.as_mut_ptr().cast(),
            output.len().min(u32::MAX as usize) as u32,
            &mut count,
            ptr::null_mut(),
        )
    };
    if success == 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(38 | 109) => Ok(0),
            Some(234) => Ok(count as usize),
            _ => Err(error),
        };
    }
    Ok(count as usize)
}

static OUTPUT: [Mutex<Output>; 2] = [Mutex::new(Output::new()), Mutex::new(Output::new())];

pub(in crate::vm::io) fn write(kind: usize, bytes: &[u8]) -> io::Result<usize> {
    if bytes.is_empty() {
        return Ok(0);
    }
    let handle = handle(kind)?;
    // UTF-8 consoles accept bytes directly, just like redirected pipes/files.
    if !console(handle) || unsafe { GetConsoleOutputCP() } == 65001 {
        let mut count = 0;
        // SAFETY: input stays live through a bounded synchronous write. The
        // process owns the handle and this function never closes it.
        if unsafe {
            WriteFile(
                handle,
                bytes.as_ptr().cast(),
                bytes.len().min(u32::MAX as usize) as u32,
                &mut count,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        return Ok(count as usize);
    }
    OUTPUT[kind - 1]
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .write(bytes, |text| {
            let mut units = [0u16; 4096];
            let mut end = 0;
            for unit in text.encode_utf16() {
                units[end] = unit;
                end += 1;
            }
            let mut start = 0;
            while start < end {
                let mut count = 0;
                // SAFETY: only initialized units are supplied. Partial writes are
                // completed, including a split surrogate pair, before returning.
                if unsafe {
                    WriteConsoleW(
                        handle,
                        units[start..end].as_ptr(),
                        (end - start) as u32,
                        &mut count,
                        ptr::null_mut(),
                    )
                } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                if count == 0 {
                    return Err(io::ErrorKind::WriteZero.into());
                }
                start += count as usize;
            }
            Ok(())
        })
}
