use std::ffi::{CStr, c_char, c_void};

#[repr(C)]
#[derive(Default)]
pub struct Buffer {
    data: *mut u8,
    size: usize,
    pub width: u32,
    pub height: u32,
    error: *const c_char,
    owner: *mut c_void,
    release: Option<unsafe extern "C" fn(*mut c_void)>,
}

#[repr(C)]
pub struct Raster<'a> {
    pixels: *const u8,
    row_bytes: usize,
    width: u32,
    height: u32,
    color: u8,
    depth: u8,
    interlaced: u8,
    palette: *const u8,
    palette_len: usize,
    lifetime: std::marker::PhantomData<&'a [u8]>,
}

unsafe extern "C" {
    fn codec_release(buffer: *mut Buffer);
    fn codec_decode(codec: i32, data: *const u8, size: usize, depth: i32, output: *mut Buffer);
    fn codec_encode(codec: i32, input: *const Raster<'_>, level: i32, output: *mut Buffer);
}

impl<'a> Raster<'a> {
    pub fn new(image: &'a deadlib_image_png::Png, packed: &'a [u8]) -> Self {
        Self {
            pixels: packed.as_ptr(),
            row_bytes: packed.len() / image.image.height as usize,
            width: image.image.width,
            height: image.image.height,
            color: image.image.color_type as u8,
            depth: image.image.bit_depth,
            interlaced: u8::from(image.interlaced),
            palette: image.palette.as_ptr().cast(),
            palette_len: image.palette.len(),
            lifetime: std::marker::PhantomData,
        }
    }
}

impl Buffer {
    pub fn bytes(&self) -> &[u8] {
        assert!(!self.data.is_null());
        // The DLL owns this allocation and keeps it live until Drop.
        unsafe { std::slice::from_raw_parts(self.data, self.size) }
    }

    fn checked(self) -> Result<Self, String> {
        if self.error.is_null() {
            Ok(self)
        } else {
            // The wrapper returns a NUL-terminated diagnostic from the codec.
            Err(unsafe { CStr::from_ptr(self.error) }
                .to_string_lossy()
                .into_owned())
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        // Keep allocation and deallocation within the same CRT.
        unsafe { codec_release(self) };
    }
}

pub fn decode(codec: i32, input: &[u8], depth: u8) -> Result<Buffer, String> {
    let mut result = Buffer::default();
    // Input is borrowed for the synchronous call and the wrapper sets all output fields.
    unsafe {
        codec_decode(
            codec,
            input.as_ptr(),
            input.len(),
            depth.into(),
            &mut result,
        )
    };
    result.checked()
}

pub fn encode(codec: i32, input: &Raster<'_>, level: i32) -> Result<Buffer, String> {
    let mut result = Buffer::default();
    unsafe { codec_encode(codec, input, level, &mut result) };
    result.checked()
}
