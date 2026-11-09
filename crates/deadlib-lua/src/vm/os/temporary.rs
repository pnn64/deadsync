//! Temporary names retain cleanup ownership until their Lua string is ready.
use crate::{
    Error,
    memory::fallible::{self, VecExt},
};

pub(super) struct Name {
    // NUL termination also lets Drop unlink without another allocation.
    bytes: Vec<u8>,
    #[cfg(unix)]
    remove_on_drop: bool,
}
impl Name {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes[..self.bytes.len() - 1]
    }
    pub(super) fn publish(self) {
        #[cfg(unix)]
        {
            let mut name = self;
            name.remove_on_drop = false;
        }
    }
}
#[cfg(unix)]
impl Drop for Name {
    fn drop(&mut self) {
        if !self.remove_on_drop {
            return;
        }
        {
            unsafe extern "C" {
                fn unlink(path: *const std::ffi::c_char) -> i32;
            }
            // SAFETY: this guard owns the freshly created, unpublished path.
            // Storage remains terminated even while allocation is rejected.
            unsafe {
                unlink(self.bytes.as_ptr().cast());
            }
        }
    }
}
fn failure(code: i32) -> Error {
    if code == 12 {
        Error::memory()
    } else {
        Error::runtime_static("unable to generate a unique filename")
    }
}

#[cfg(unix)]
pub(super) fn create() -> Result<Name, Error> {
    use std::{ffi::c_char, os::fd::FromRawFd};
    unsafe extern "C" {
        #[cfg(target_os = "linux")]
        fn mkostemp(path: *mut c_char, flags: i32) -> i32;
        #[cfg(not(target_os = "linux"))]
        fn mkstemp(path: *mut c_char) -> i32;
    }
    // Keep the POSIX name extensionless: require(os.tmpname()) with LUA_PATH=?
    // must not translate an artificial extension into a module separator.
    let mut bytes = fallible::format(format_args!(
        "/tmp/deadsync-lua-{}-XXXXXX",
        std::process::id()
    ))?
    .into_bytes();
    bytes.try_push(0)?;
    // SAFETY: a writable terminated template ends in six X bytes. The native
    // operation creates one mode-0600 file exclusively; Linux also uses CLOEXEC.
    let descriptor = unsafe {
        #[cfg(target_os = "linux")]
        {
            mkostemp(bytes.as_mut_ptr().cast(), 0x80000)
        }
        #[cfg(not(target_os = "linux"))]
        {
            mkstemp(bytes.as_mut_ptr().cast())
        }
    };
    if descriptor < 0 {
        return Err(failure(super::super::io::errno(
            std::io::Error::last_os_error(),
        )));
    }
    // SAFETY: the fresh descriptor has one owner and is immediately closed.
    drop(unsafe { std::fs::File::from_raw_fd(descriptor) });
    Ok(Name {
        bytes,
        remove_on_drop: true,
    })
}

#[cfg(windows)]
pub(super) fn create() -> Result<Name, Error> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileAttributesW(path: *const u16) -> u32;
    }
    for _ in 0..64 {
        let mut path = super::super::io::input::temporary_directory()?.map_err(failure)?;
        let nonce = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let suffix = fallible::format(format_args!(
            "deadsync-lua-{}-{stamp}-{nonce}.tmp",
            std::process::id()
        ))?;
        path.try_extend(suffix.encode_utf16())?;
        let mut bytes = Vec::new();
        for character in char::decode_utf16(path.iter().copied()) {
            let mut buffer = [0; 4];
            bytes.try_extend_from_slice(
                character
                    .unwrap_or(char::REPLACEMENT_CHARACTER)
                    .encode_utf8(&mut buffer)
                    .as_bytes(),
            )?;
        }
        bytes.try_push(0)?;
        path.try_push(0)?;
        let path = super::super::io::input::windows_absolute(path)?.map_err(failure)?;
        // SAFETY: initialized terminated UTF-16 path, borrowed for this query.
        if unsafe { GetFileAttributesW(path.as_ptr()) } != u32::MAX {
            continue;
        }
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(2 | 3)) {
            // Windows tmpnam returns an unused name without creating a file.
            return Ok(Name { bytes });
        }
        return Err(failure(super::super::io::errno(error)));
    }
    Err(failure(17))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn create() -> Result<Name, Error> {
    Err(failure(22))
}
