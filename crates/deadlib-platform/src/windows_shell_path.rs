use std::{
    ffi::{OsString, c_void},
    os::windows::ffi::OsStringExt,
    path::PathBuf,
};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{Error, PWSTR, Result, s, w};

type CoTaskMemFree = unsafe extern "system" fn(*const c_void);

pub(crate) struct ComTaskAllocator {
    module: HMODULE,
    free: CoTaskMemFree,
}

impl ComTaskAllocator {
    pub(crate) fn new() -> Result<Self> {
        // SAFETY: Ole32 is a system Known DLL. Look up the export on its own
        // handle so the linker cannot bind the call to combase.dll on Win7.
        let module = unsafe { LoadLibraryW(w!("ole32.dll")) }?;
        let Some(free) = (unsafe { GetProcAddress(module, s!("CoTaskMemFree")) }) else {
            let error = Error::from_thread();
            let _ = unsafe { FreeLibrary(module) };
            return Err(error);
        };
        // SAFETY: this is Ole32's CoTaskMemFree export with the Win32 ABI.
        let free = unsafe { std::mem::transmute::<_, CoTaskMemFree>(free) };
        Ok(Self { module, free })
    }

    /// `ptr` must be null or owned by the caller and allocated by the COM task
    /// allocator.
    pub(crate) unsafe fn free(&self, ptr: PWSTR) {
        // SAFETY: the caller guarantees allocator ownership.
        unsafe { (self.free)(ptr.0.cast()) };
    }
}

impl Drop for ComTaskAllocator {
    fn drop(&mut self) {
        // SAFETY: this balances the successful LoadLibraryW call.
        let _ = unsafe { FreeLibrary(self.module) };
    }
}

pub(crate) struct ShellPath<'a> {
    ptr: PWSTR,
    allocator: &'a ComTaskAllocator,
}

impl<'a> ShellPath<'a> {
    /// `ptr` must be null or a NUL-terminated UTF-16 string owned by the caller
    /// and allocated by the COM task allocator.
    pub(crate) unsafe fn from_raw(ptr: PWSTR, allocator: &'a ComTaskAllocator) -> Self {
        Self { ptr, allocator }
    }

    pub(crate) fn into_path_buf(self) -> Option<PathBuf> {
        if self.ptr.is_null() {
            return None;
        }
        // SAFETY: the constructor guarantees a NUL-terminated string. The
        // owned OsString preserves unpaired surrogates and outlives the buffer.
        let path = unsafe { self.ptr.as_wide() };
        Some(PathBuf::from(OsString::from_wide(path)))
    }
}

impl Drop for ShellPath<'_> {
    fn drop(&mut self) {
        // SAFETY: the constructor transfers COM allocation ownership. The
        // allocator also accepts null pointers.
        unsafe { self.allocator.free(self.ptr) };
    }
}
