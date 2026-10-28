//! Native loader ownership; one C API image serves a VM's foreign calls.
use super::*;

#[derive(Debug)]
pub(super) struct Library(pub(super) usize);
impl Library {
    pub(super) fn try_open(path: &[u8]) -> Result<Self, Box<[u8]>> {
        platform::open(path, false).map(Self)
    }
    pub(super) fn shim(path: &std::path::Path) -> Result<Self, Box<[u8]>> {
        platform::shim(path).map(Self)
    }
    pub(super) fn try_symbol(&self, name: &[u8]) -> Result<usize, Box<[u8]>> {
        platform::symbol(self.0, name)
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: this owner holds exactly one successful native loader reference.
        unsafe {
            platform::close(self.0);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryA(name: *const c_char) -> *mut c_void;
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetModuleHandleA(name: *const c_char) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> c_int;
        fn FormatMessageA(
            flags: u32,
            source: *const c_void,
            message: u32,
            language: u32,
            buffer: *mut c_char,
            size: u32,
            args: *const c_void,
        ) -> u32;
    }
    fn error() -> Box<[u8]> {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
        let mut bytes = [0u8; 128];
        // SAFETY: the system formatter writes at most the buffer capacity.
        let n = unsafe {
            FormatMessageA(
                0x1200,
                std::ptr::null(),
                code,
                0,
                bytes.as_mut_ptr().cast(),
                128,
                std::ptr::null(),
            )
        };
        if n == 0 {
            format!("system error {code}\n").into_bytes().into()
        } else {
            bytes[..n as usize].into()
        }
    }
    pub(super) fn open(path: &[u8], shim: bool) -> Result<usize, Box<[u8]>> {
        let path =
            CString::new(path).map_err(|_| Box::<[u8]>::from(b"NUL in library path".as_slice()))?;
        if shim {
            // SAFETY: the literal is terminated; this query borrows the image.
            let existing = unsafe { GetModuleHandleA(c"lua51.dll".as_ptr()) };
            if !existing.is_null() {
                let version = symbol(existing as usize, b"boundary_abi_version").ok();
                if !version.is_some_and(super::super::matching_version) {
                    return Err(
                        b"lua51.dll is already provided by another Lua implementation"
                            .as_slice()
                            .into(),
                    );
                }
                // SAFETY: load by basename adds a reference to the validated image.
                let handle = unsafe { LoadLibraryA(c"lua51.dll".as_ptr()) };
                return if handle.is_null() {
                    Err(error())
                } else {
                    Ok(handle as usize)
                };
            }
        }
        // SAFETY: the path is terminated. Native modules execute with Lua 5.1's trust model.
        let handle = unsafe { LoadLibraryA(path.as_ptr()) };
        if handle.is_null() {
            Err(error())
        } else {
            Ok(handle as usize)
        }
    }
    pub(super) fn shim(path: &std::path::Path) -> Result<usize, Box<[u8]>> {
        use std::os::windows::ffi::OsStrExt;
        // SAFETY: the basename is terminated and the result is only borrowed.
        let existing = unsafe { GetModuleHandleA(c"lua51.dll".as_ptr()) };
        if !existing.is_null() {
            if !symbol(existing as usize, b"boundary_abi_version")
                .ok()
                .is_some_and(super::super::matching_version)
            {
                return Err(
                    b"lua51.dll is already provided by another Lua implementation"
                        .as_slice()
                        .into(),
                );
            }
            return open(b"lua51.dll", false);
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: the cache path is native UTF-16, terminated and build-verified.
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if handle.is_null() {
            Err(error())
        } else {
            Ok(handle as usize)
        }
    }
    pub(super) fn symbol(handle: usize, name: &[u8]) -> Result<usize, Box<[u8]>> {
        let name =
            CString::new(name).map_err(|_| Box::<[u8]>::from(b"NUL in symbol name".as_slice()))?;
        // SAFETY: the owner retains the module and the symbol is terminated.
        let p = unsafe { GetProcAddress(handle as *mut c_void, name.as_ptr()) };
        if p.is_null() {
            Err(error())
        } else {
            Ok(p as usize)
        }
    }
    pub(super) unsafe fn close(handle: usize) {
        // SAFETY: delegated one-reference ownership contract.
        unsafe {
            FreeLibrary(handle as *mut c_void);
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    #[link(name = "dl")]
    unsafe extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    fn error() -> Box<[u8]> {
        // SAFETY: dlerror lends its terminated message until the next loader call.
        let p = unsafe { dlerror() };
        if p.is_null() {
            b"dynamic library loader error".as_slice().into()
        } else {
            unsafe { CStr::from_ptr(p) }.to_bytes().into()
        }
    }
    pub(super) fn open(path: &[u8], shim: bool) -> Result<usize, Box<[u8]>> {
        let path =
            CString::new(path).map_err(|_| Box::<[u8]>::from(b"NUL in library path".as_slice()))?;
        if shim {
            // SAFETY: a null handle addresses the process's global loader scope.
            let existing = unsafe { dlsym(std::ptr::null_mut(), c"lua_gettop".as_ptr()) };
            if !existing.is_null() {
                let version =
                    unsafe { dlsym(std::ptr::null_mut(), c"boundary_abi_version".as_ptr()) };
                if version.is_null() || !super::super::matching_version(version as usize) {
                    return Err(b"the process already exports another Lua C API"
                        .as_slice()
                        .into());
                }
            }
        }
        // SAFETY: valid loader flags, with global scope only for this C API image.
        let handle = unsafe { dlopen(path.as_ptr(), 2 | if shim { 0x100 } else { 0 }) };
        if handle.is_null() {
            Err(error())
        } else {
            Ok(handle as usize)
        }
    }
    pub(super) fn shim(path: &std::path::Path) -> Result<usize, Box<[u8]>> {
        use std::os::unix::ffi::OsStrExt;
        open(path.as_os_str().as_bytes(), true)
    }
    pub(super) fn symbol(handle: usize, name: &[u8]) -> Result<usize, Box<[u8]>> {
        let name =
            CString::new(name).map_err(|_| Box::<[u8]>::from(b"NUL in symbol name".as_slice()))?;
        // SAFETY: a live loader reference and a terminated symbol name.
        let p = unsafe { dlsym(handle as *mut c_void, name.as_ptr()) };
        if p.is_null() {
            Err(error())
        } else {
            Ok(p as usize)
        }
    }
    pub(super) unsafe fn close(handle: usize) {
        // SAFETY: delegated one-reference ownership contract.
        unsafe {
            dlclose(handle as *mut c_void);
        }
    }
}
