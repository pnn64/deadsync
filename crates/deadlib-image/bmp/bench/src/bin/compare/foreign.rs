use libloading::Library;
use std::{ffi::CStr, os::raw::c_char, path::Path};

#[repr(C)]
struct Output {
    data: *mut u8,
    size: usize,
    w: u32,
    h: u32,
    owner: *mut std::ffi::c_void,
    release: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    error: [c_char; 160],
}

type Decode = unsafe extern "C" fn(u32, *const u8, usize, *mut Output);
type Encode = unsafe extern "C" fn(u32, *const u8, u32, u32, u32, *mut Output);

pub struct Bridge {
    _library: Library,
    decode: Decode,
    encode: Encode,
}

pub struct Buffer<'a>(Output, std::marker::PhantomData<&'a Bridge>);

impl Bridge {
    pub fn load(path: &Path) -> Self {
        // The benchmark DLLs implement the ABI in contenders/bridge.h.
        unsafe {
            let lib = Library::new(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let init = lib
                .get::<unsafe extern "C" fn() -> i32>(b"init_bmp\0")
                .unwrap();
            assert_ne!(init(), 0, "initialization failed for {}", path.display());
            Self {
                decode: *lib.get(b"decode_bmp\0").unwrap(),
                encode: *lib.get(b"encode_bmp\0").unwrap(),
                _library: lib,
            }
        }
    }

    pub fn decode(&self, codec: u32, data: &[u8], length: usize) -> Result<Buffer<'_>, String> {
        let mut out = Buffer::empty();
        assert!(length <= data.len());
        // The buffer and input stay valid throughout the synchronous operation.
        unsafe { (self.decode)(codec, data.as_ptr(), length, &mut out.0) };
        out.checked()
    }

    pub fn encode(
        &self,
        codec: u32,
        raw: &[u8],
        w: u32,
        h: u32,
        n: u32,
    ) -> Result<Buffer<'_>, String> {
        let mut out = Buffer::empty();
        assert_eq!(raw.len(), w as usize * h as usize * n as usize);
        // The bridge borrows this packed pixel buffer only during the call.
        unsafe { (self.encode)(codec, raw.as_ptr(), w, h, n, &mut out.0) };
        out.checked()
    }
}

impl Buffer<'_> {
    fn empty() -> Self {
        Self(
            Output {
                data: std::ptr::null_mut(),
                size: 0,
                w: 0,
                h: 0,
                owner: std::ptr::null_mut(),
                release: None,
                error: [0; 160],
            },
            std::marker::PhantomData,
        )
    }
    fn checked(self) -> Result<Self, String> {
        if self.0.error[0] != 0 {
            // The bridge produces NUL-terminated diagnostics within error.
            Err(unsafe { CStr::from_ptr(self.0.error.as_ptr()) }
                .to_string_lossy()
                .into_owned())
        } else if self.0.data.is_null() || self.0.size == 0 {
            Err("empty codec result".to_owned())
        } else {
            Ok(self)
        }
    }
    pub fn bytes(&self) -> &[u8] {
        // A checked buffer is non-null and stays owned until Drop.
        unsafe { std::slice::from_raw_parts(self.0.data, self.0.size) }
    }
    pub fn shape(&self) -> (u32, u32) {
        (self.0.w, self.0.h)
    }
}

impl Drop for Buffer<'_> {
    fn drop(&mut self) {
        if let Some(release) = self.0.release {
            // Free with the producing codec's allocator while its DLL is loaded.
            unsafe { release(self.0.owner) };
        }
    }
}
