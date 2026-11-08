//! UCRT ABI, pinned to the declarations in the Windows SDK's locale/ctype headers.
//! No Lua values cross this boundary. The recursive CRT locale lock protects
//! returned strings and lconv storage even against other CRT setlocale callers.
use std::{
    cmp::Ordering,
    ffi::{CStr, c_char},
    ptr,
};

unsafe extern "C" {
    fn _lock_locales();
    fn _unlock_locales();
    #[link_name = "setlocale"]
    fn native_setlocale(category: i32, name: *const c_char) -> *const c_char;
    fn localeconv() -> *const Numeric;
    fn _isctype(byte: i32, mask: i32) -> i32;
    fn tolower(byte: i32) -> i32;
    fn toupper(byte: i32) -> i32;
    fn strcoll(left: *const c_char, right: *const c_char) -> i32;
}
// Only the first field of the larger native lconv is read.
#[repr(C)]
struct Numeric {
    decimal: *const c_char,
}
struct Lock;
impl Lock {
    fn new() -> Self {
        // SAFETY: this UCRT call eagerly initializes locale APIs, then enters
        // its recursive critical section. Drop releases it on the same thread.
        unsafe { _lock_locales() };
        Self
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        // SAFETY: one successful acquisition by this thread owns this guard.
        unsafe { _unlock_locales() };
    }
}
pub(super) struct Cache {
    pub decimal: Option<Vec<u8>>,
    pub ctype: Option<([u32; 256], bool)>,
    pub collate_c: Option<bool>,
}
pub(super) struct Change {
    pub result: Vec<u8>,
    pub cache: Option<Cache>,
}

pub(super) fn set(name: Option<&[u8]>, category: usize) -> Option<Change> {
    assert!(category < 6);
    let name = name.map(|s| {
        let len = s.iter().position(|&b| b == 0).unwrap_or(s.len());
        let mut value = Vec::with_capacity(len + 1);
        value.extend_from_slice(&s[..len]);
        value.push(0);
        value
    });
    let _lock = Lock::new();
    // SAFETY: SDK category constants are exactly 0..=5 in Lua category order.
    // The optional buffer is NUL terminated and remains alive for this call.
    let result = unsafe {
        native_setlocale(
            category as i32,
            name.as_ref().map_or(ptr::null(), |s| s.as_ptr().cast()),
        )
    };
    if result.is_null() {
        return None;
    }
    // SAFETY: nonnull setlocale results are NUL terminated. The CRT locale
    // lock prevents replacement/freeing until this copy finishes.
    let result = unsafe { CStr::from_ptr(result) }.to_bytes().to_vec();
    let cache = name.map(|_| {
        // SAFETY: fixed valid categories query current CRT-owned NUL strings;
        // localeconv returns a valid lconv whose first field is decimal_point.
        // The same CRT lock keeps every pointer alive through these reads.
        let decimal = matches!(category, 0 | 4)
            .then(|| unsafe { CStr::from_ptr((*localeconv()).decimal).to_bytes().to_vec() });
        // SAFETY: the fixed valid category returns a CRT-owned NUL string,
        // stabilized by the same locale lock.
        let collate_c = matches!(category, 0 | 1).then(|| unsafe {
            CStr::from_ptr(native_setlocale(1, ptr::null())).to_bytes() == b"C"
        });
        let ctype = matches!(category, 0 | 2).then(|| {
            // SAFETY: the fixed valid category returns a CRT-owned NUL string,
            // stabilized by the same locale lock.
            let ctype_c =
                unsafe { CStr::from_ptr(native_setlocale(2, ptr::null())).to_bytes() == b"C" };
            let mut ctype = [0; 256];
            // C-locale mapping uses Rust ASCII operations; only the zero high-
            // byte masks need publishing when returning to that locale.
            if !ctype_c {
                for (byte, slot) in ctype.iter_mut().enumerate() {
                    // SAFETY: these functions accept unsigned-char values;
                    // this loop passes only 0..=255 and holds the locale lock.
                    *slot = unsafe {
                        (_isctype(byte as i32, 0x1ff) as u32) << 16
                            | ((toupper(byte as i32) as u8 as u32) << 8)
                            | tolower(byte as i32) as u8 as u32
                    };
                }
            }
            (ctype, ctype_c)
        });
        Cache {
            decimal,
            ctype,
            collate_c,
        }
    });
    Some(Change { result, cache })
}

pub(super) fn compare(left: &[u8], right: &[u8]) -> Ordering {
    let mut a = Vec::with_capacity(left.len() + 1);
    let mut b = Vec::with_capacity(right.len() + 1);
    a.extend_from_slice(left);
    a.push(0);
    b.extend_from_slice(right);
    b.push(0);
    let _lock = Lock::new();
    let (mut x, mut y) = (0, 0);
    loop {
        // SAFETY: each offset starts a NUL-terminated suffix of an owned
        // buffer. The CRT lock stabilizes collation state for this comparison.
        let result = unsafe { strcoll(a.as_ptr().add(x).cast(), b.as_ptr().add(y).cast()) };
        if result != 0 {
            return result.cmp(&0);
        }
        let n = a[x..].iter().position(|&c| c == 0).unwrap();
        // Lua 5.1 advances both suffixes by the left segment's byte length,
        // including when differently sized segments collate as equal.
        if n == right.len().saturating_sub(y) {
            return if n == left.len() - x {
                Ordering::Equal
            } else {
                Ordering::Greater
            };
        }
        if n == left.len() - x {
            return Ordering::Less;
        }
        x += n + 1;
        y += n + 1;
        if y > right.len() {
            return Ordering::Greater;
        }
    }
}
