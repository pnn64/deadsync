use libloading::Library;
use std::{ffi::CStr, path::Path};

#[repr(C)]
pub struct Output {
    data: *mut u8,
    size: usize,
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    pub precision: u32,
    error: [std::ffi::c_char; 256],
}

#[repr(C)]
pub struct Raster {
    pub data: *const u8,
    pub width: u32,
    pub height: u32,
    pub channels: u32,
    pub precision: u32,
    pub quality: u32,
    pub horizontal: u32,
    pub vertical: u32,
    pub flags: u32,
}

type Decode = unsafe extern "C" fn(*const u8, usize, i32, *mut Output);
type Encode = unsafe extern "C" fn(*const Raster, *mut Output);
type Release = unsafe extern "C" fn(*mut Output);

pub struct Bridge {
    _library: Library,
    decode: Decode,
    encode: Encode,
    release: Release,
    release_sjpeg: Option<Release>,
}

pub struct Buffer<'a> {
    pub output: Output,
    release: Release,
    _bridge: &'a Bridge,
}

impl Bridge {
    pub fn load(path: &Path) -> Self {
        // The DLLs are built from the pinned benchmark sources and use bridge.h.
        unsafe {
            let library = Library::new(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            Self {
                decode: *library.get(b"decode_jpeg\0").unwrap(),
                encode: *library.get(b"encode_jpeg\0").unwrap(),
                release: *library.get(b"release_output\0").unwrap(),
                release_sjpeg: library.get(b"release_sjpeg\0").ok().map(|symbol| *symbol),
                _library: library,
            }
        }
    }

    fn buffer(&self, release: Release) -> Buffer<'_> {
        Buffer {
            output: Output {
                data: std::ptr::null_mut(),
                size: 0,
                width: 0,
                height: 0,
                channels: 0,
                precision: 0,
                error: [0; 256],
            },
            release,
            _bridge: self,
        }
    }

    pub fn decode(&self, input: &[u8], mode: i32) -> Result<Buffer<'_>, String> {
        let mut buffer = self.buffer(self.release);
        // The bridge borrows the input during this synchronous call.
        unsafe { (self.decode)(input.as_ptr(), input.len(), mode, &mut buffer.output) };
        buffer.check()
    }

    pub fn encode(&self, input: &Raster, sjpeg: bool) -> Result<Buffer<'_>, String> {
        let release = if sjpeg {
            self.release_sjpeg.ok_or("sjpeg release API unavailable")?
        } else {
            self.release
        };
        let mut buffer = self.buffer(release);
        // The caller keeps the owned raster alive through this call.
        unsafe { (self.encode)(input, &mut buffer.output) };
        buffer.check()
    }
}

impl Buffer<'_> {
    fn check(self) -> Result<Self, String> {
        if self.output.error[0] != 0 {
            // Diagnostics are NUL-terminated in initialized output storage.
            let diagnostic = unsafe { CStr::from_ptr(self.output.error.as_ptr()) };
            Err(diagnostic.to_string_lossy().into_owned())
        } else if self.output.data.is_null() || self.output.size == 0 {
            Err("empty bridge result".to_owned())
        } else {
            Ok(self)
        }
    }

    pub fn bytes(&self) -> &[u8] {
        // The bridge owns this allocation until Drop.
        unsafe { std::slice::from_raw_parts(self.output.data, self.output.size) }
    }
}

impl Drop for Buffer<'_> {
    fn drop(&mut self) {
        // Deallocate in the same library that allocated the buffer.
        unsafe { (self.release)(&mut self.output) };
    }
}
