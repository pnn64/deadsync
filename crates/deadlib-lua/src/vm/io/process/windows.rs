//! Windows process/pipe ownership. No infallible Rust process builder is used.
use crate::{Error, memory::fallible::VecExt};
use std::{
    ffi::c_void,
    fs::File,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
};
type Handle = *mut c_void;
#[repr(C)]
struct Security {
    size: u32,
    descriptor: *mut c_void,
    inherit: i32,
}
#[repr(C)]
#[derive(Default)]
struct Startup {
    size: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    window: [u32; 8],
    show: u16,
    reserved_len: u16,
    reserved_bytes: *mut u8,
    standard: [Handle; 3],
}
#[repr(C)]
struct StartupEx {
    base: Startup,
    attributes: *mut c_void,
}
#[repr(C)]
#[derive(Default)]
struct Process {
    process: Handle,
    thread: Handle,
    process_id: u32,
    thread_id: u32,
}
const _: () = {
    assert!(size_of::<Startup>() == if size_of::<usize>() == 8 { 104 } else { 68 });
    assert!(size_of::<StartupEx>() == if size_of::<usize>() == 8 { 112 } else { 72 });
};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreatePipe(
        read: *mut Handle,
        write: *mut Handle,
        security: *const Security,
        size: u32,
    ) -> i32;
    fn GetStdHandle(kind: u32) -> Handle;
    fn GetCurrentProcess() -> Handle;
    fn DuplicateHandle(
        process: Handle,
        source: Handle,
        target_process: Handle,
        target: *mut Handle,
        access: u32,
        inherit: i32,
        options: u32,
    ) -> i32;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attribute: usize,
        value: *mut c_void,
        size: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
    fn CreateProcessW(
        application: *const u16,
        command: *mut u16,
        process_security: *const c_void,
        thread_security: *const c_void,
        inherit: i32,
        flags: u32,
        environment: *const c_void,
        directory: *const u16,
        startup: *const Startup,
        result: *mut Process,
    ) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(handle: Handle, code: *mut u32) -> i32;
    fn GetFileAttributesW(path: *const u16) -> u32;
}

#[derive(Debug)]
pub(super) struct Child {
    handle: OwnedHandle,
    status: Option<i32>,
}
impl Child {
    pub(super) fn wait(&mut self) -> io::Result<i32> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        let mut code = 0;
        // SAFETY: the child process handle remains owned through both calls.
        if unsafe { WaitForSingleObject(self.handle.as_raw_handle(), u32::MAX) } != 0
            || unsafe { GetExitCodeProcess(self.handle.as_raw_handle(), &mut code) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        self.status = Some(code as i32);
        Ok(code as i32)
    }
}

struct Attributes(Vec<usize>);
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: successful initialization precedes constructing this owner.
        unsafe {
            DeleteProcThreadAttributeList(self.0.as_mut_ptr().cast());
        }
    }
}
fn error() -> i32 {
    super::super::errno(io::Error::last_os_error())
}

fn executable(name: &[u16]) -> Result<Option<Vec<u16>>, Error> {
    let mut path = Vec::new();
    path.try_extend_from_slice(name)?;
    path.try_push(0)?;
    // SAFETY: path is terminated; attributes are queried without opening a file.
    if unsafe { GetFileAttributesW(path.as_ptr()) } != u32::MAX {
        return Ok(Some(path));
    }
    let Some(search) = crate::vm::os::environment::wide_value(&b"PATH\0".map(u16::from))? else {
        return Ok(None);
    };
    // Quoted PATH components may contain semicolons, as with Windows split_paths.
    let mut start = 0;
    let mut quoted = false;
    for end in 0..=search.len() {
        if end < search.len() && search[end] == 34 {
            quoted = !quoted;
        }
        if end != search.len() && (search[end] != 59 || quoted) {
            continue;
        }
        path.clear();
        path.try_extend(search[start..end].iter().copied().filter(|&u| u != 34))?;
        start = end + 1;
        if path.is_empty() {
            continue;
        }
        if !matches!(path.last(), Some(47 | 92)) {
            path.try_push(92)?;
        }
        path.try_extend_from_slice(name)?;
        path.try_push(0)?;
        if unsafe { GetFileAttributesW(path.as_ptr()) } != u32::MAX {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub(super) fn open(command: &[u8], reading: bool) -> Result<Result<(File, Child), i32>, Error> {
    let mut shell = match crate::vm::os::environment::wide_value(&b"COMSPEC\0".map(u16::from))? {
        Some(shell) => shell,
        None => {
            let mut shell = Vec::new();
            shell.try_extend("cmd.exe".encode_utf16())?;
            shell
        }
    };
    if shell.is_empty() {
        return Ok(Err(22));
    }
    let Some(application) = executable(&shell)? else {
        return Ok(Err(2));
    };
    // The shell receives the command verbatim after /c, matching the CRT. An
    // explicit application path avoids treating command text as an executable.
    shell.try_extend(" /c ".encode_utf16())?;
    for chunk in command.utf8_chunks() {
        shell.try_extend(chunk.valid().encode_utf16())?;
        if !chunk.invalid().is_empty() {
            shell.try_push(0xfffd)?;
        }
    }
    shell.try_push(0)?;

    let mut size = 0;
    // SAFETY: null storage queries the required size for one handle-list attribute.
    unsafe {
        InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut size);
    }
    if size == 0 {
        return Ok(Err(error()));
    }
    let mut storage = Vec::<usize>::new();
    storage.try_resize(size.div_ceil(size_of::<usize>()), 0)?;
    // All fallible Rust storage is prepared before creating handles or a child.
    Ok(spawn(&application, &mut shell, storage, reading))
}
fn duplicate(handle: Handle) -> io::Result<OwnedHandle> {
    let mut result = ptr::null_mut();
    // SAFETY: source is a borrowed live handle; the duplicate is uniquely owned.
    let process = unsafe { GetCurrentProcess() };
    if unsafe { DuplicateHandle(process, handle, process, &mut result, 0, 1, 2) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(result) })
}
fn spawn(
    application: &[u16],
    command: &mut [u16],
    mut storage: Vec<usize>,
    reading: bool,
) -> Result<(File, Child), i32> {
    let (mut read, mut write) = (ptr::null_mut(), ptr::null_mut());
    // SAFETY: null security creates noninheritable endpoints; only a duplicate
    // of the child's selected end is subsequently made inheritable.
    if unsafe { CreatePipe(&mut read, &mut write, ptr::null(), 0) } == 0 {
        return Err(error());
    }
    let (read, write) = unsafe {
        (
            OwnedHandle::from_raw_handle(read),
            OwnedHandle::from_raw_handle(write),
        )
    };
    let (parent, endpoint) = if reading {
        (read, write)
    } else {
        (write, read)
    };
    let endpoint = duplicate(endpoint.as_raw_handle()).map_err(super::super::errno)?;
    let mut inherited: [Option<OwnedHandle>; 3] = [None, None, None];
    let mut standard = [ptr::null_mut(); 3];
    let selected = usize::from(reading);
    standard[selected] = endpoint.as_raw_handle();
    for kind in 0..3 {
        if kind == selected {
            continue;
        }
        // SAFETY: standard handles are borrowed. Absent standard streams remain
        // absent; duplicates avoid changing the host's inheritance flags.
        let handle = unsafe { GetStdHandle(0u32.wrapping_sub(10 + kind as u32)) };
        if !handle.is_null() && handle != (-1isize) as Handle {
            let owned = duplicate(handle).map_err(super::super::errno)?;
            standard[kind] = owned.as_raw_handle();
            inherited[kind] = Some(owned);
        }
    }
    let mut handles = [ptr::null_mut(); 3];
    let mut count = 0;
    for handle in standard {
        if !handle.is_null() {
            handles[count] = handle;
            count += 1;
        }
    }
    let mut size = storage.len() * size_of::<usize>();
    if unsafe { InitializeProcThreadAttributeList(storage.as_mut_ptr().cast(), 1, 0, &mut size) }
        == 0
    {
        return Err(error());
    }
    // The handle array stays live until this later-declared owner is destroyed.
    let mut attributes = Attributes(storage);
    let attribute_ptr = attributes.0.as_mut_ptr().cast();
    // SAFETY: HANDLE_LIST refers to count valid inheritable handles, all owned
    // through CreateProcess. It excludes unrelated host and other pipe handles.
    if unsafe {
        UpdateProcThreadAttribute(
            attribute_ptr,
            0,
            0x20002,
            handles.as_mut_ptr().cast(),
            count * size_of::<Handle>(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(error());
    }
    let mut startup = StartupEx {
        base: Startup {
            size: size_of::<StartupEx>() as u32,
            standard,
            ..Startup::default()
        },
        attributes: attribute_ptr,
    };
    startup.base.window[7] = 0x100; // STARTF_USESTDHANDLES
    let mut result = Process::default();
    // SAFETY: all terminated strings, ABI records and handles live for the call.
    // EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW keeps pipes noninteractive.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            0x08080000,
            ptr::null(),
            ptr::null(),
            &startup.base,
            &mut result,
        )
    } == 0
    {
        return Err(error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(result.process) };
    drop(unsafe { OwnedHandle::from_raw_handle(result.thread) });
    Ok((
        File::from(parent),
        Child {
            handle: process,
            status: None,
        },
    ))
}
