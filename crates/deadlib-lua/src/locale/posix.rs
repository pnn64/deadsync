//! Linux/glibc locale ABI from locale.h, langinfo.h and ctype.h.
//! Owned locale objects avoid process-global setlocale's thread-safety hazard:
//! https://sourceware.org/glibc/manual/latest/html_node/Setting-the-Locale.html
//! Engine VMs share one selection. Native calls borrow it under CURRENT's lock;
//! no locale pointer, C string or native ownership escapes this module's guards.
use std::{
    cmp::Ordering,
    ffi::{CStr, c_char, c_void},
    ptr::{self, NonNull},
    sync::{LazyLock, Mutex, MutexGuard},
};

const ALL: usize = 6;
const ALL_MASK: i32 = 0x1fbf; // glibc categories 0..=12, excluding LC_ALL.
const CATEGORIES: [usize; 6] = [ALL, 3, 0, 4, 1, 2];
const NAMES: [&[u8]; 13] = [
    b"LC_CTYPE",
    b"LC_NUMERIC",
    b"LC_TIME",
    b"LC_COLLATE",
    b"LC_MONETARY",
    b"LC_MESSAGES",
    b"LC_ALL",
    b"LC_PAPER",
    b"LC_NAME",
    b"LC_ADDRESS",
    b"LC_TELEPHONE",
    b"LC_MEASUREMENT",
    b"LC_IDENTIFICATION",
];

unsafe extern "C" {
    fn newlocale(mask: i32, name: *const c_char, base: *mut c_void) -> *mut c_void;
    fn duplocale(locale: *mut c_void) -> *mut c_void;
    fn freelocale(locale: *mut c_void);
    fn nl_langinfo_l(item: i32, locale: *mut c_void) -> *const c_char;
    fn isalpha_l(byte: i32, locale: *mut c_void) -> i32;
    fn isupper_l(byte: i32, locale: *mut c_void) -> i32;
    fn islower_l(byte: i32, locale: *mut c_void) -> i32;
    fn isdigit_l(byte: i32, locale: *mut c_void) -> i32;
    fn isspace_l(byte: i32, locale: *mut c_void) -> i32;
    fn ispunct_l(byte: i32, locale: *mut c_void) -> i32;
    fn iscntrl_l(byte: i32, locale: *mut c_void) -> i32;
    fn isxdigit_l(byte: i32, locale: *mut c_void) -> i32;
    fn tolower_l(byte: i32, locale: *mut c_void) -> i32;
    fn toupper_l(byte: i32, locale: *mut c_void) -> i32;
    fn strcoll_l(left: *const c_char, right: *const c_char, locale: *mut c_void) -> i32;
    fn strerror_l(code: i32, locale: *mut c_void) -> *const c_char;
}

struct Locale(NonNull<c_void>);
// SAFETY: these owned locale_t objects may be read/freed on any thread. They
// are never installed in a thread or mutated after publication; CURRENT's
// mutex keeps native users alive and excludes replacement until they finish.
unsafe impl Send for Locale {}
impl Drop for Locale {
    fn drop(&mut self) {
        // SAFETY: one successful newlocale/duplocale owns this reference. No
        // native user survives the surrounding guard or ownership transfer.
        unsafe { freelocale(self.0.as_ptr()) };
    }
}
impl Locale {
    fn initial() -> Self {
        // SAFETY: valid glibc mask, terminated static name and null base. The
        // mandatory C locale uses glibc's built-in immutable locale object.
        let handle = unsafe { newlocale(ALL_MASK, c"C".as_ptr(), ptr::null_mut()) };
        Self(NonNull::new(handle).expect("glibc provides the C locale"))
    }
    fn changed(&self, name: &[u8], category: usize) -> Option<Self> {
        let mut text = name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())].to_vec();
        text.push(0);
        // SAFETY: self owns a live locale, locked against replacement. The
        // duplicate is private; newlocale may consume only that duplicate.
        let candidate = Self(NonNull::new(unsafe { duplocale(self.0.as_ptr()) })?);
        let mask = if category == ALL {
            ALL_MASK
        } else {
            1 << category
        };
        // SAFETY: the mask and string are valid; candidate owns its base.
        // Failure leaves that base valid; success transfers its ownership.
        let handle =
            NonNull::new(unsafe { newlocale(mask, text.as_ptr().cast(), candidate.0.as_ptr()) })?;
        std::mem::forget(candidate);
        Some(Self(handle))
    }
    fn text(&self, item: i32) -> Vec<u8> {
        // SAFETY: all callers pass glibc-defined items. Their terminated
        // strings belong to this live locale; copy before releasing its guard.
        unsafe { CStr::from_ptr(nl_langinfo_l(item, self.0.as_ptr())) }
            .to_bytes()
            .to_vec()
    }
}

struct State {
    locale: Locale,
    names: [Vec<u8>; 13],
}
impl State {
    fn new(locale: Locale) -> Self {
        let names = std::array::from_fn(|i| {
            if i == ALL {
                Vec::new()
            } else {
                locale.text(((i as i32) << 16) | 0xffff)
            }
        });
        Self { locale, names }
    }
    fn name(&self, category: usize) -> Vec<u8> {
        if category != ALL {
            return self.names[category].clone();
        }
        if self
            .names
            .iter()
            .enumerate()
            .all(|(i, name)| i == ALL || name == &self.names[0])
        {
            return self.names[0].clone();
        }
        let mut text = Vec::new();
        for (i, name) in self.names.iter().enumerate() {
            if i == ALL {
                continue;
            }
            if !text.is_empty() {
                text.push(b';');
            }
            text.extend_from_slice(NAMES[i]);
            text.push(b'=');
            text.extend_from_slice(name);
        }
        text
    }
}
static CURRENT: LazyLock<Mutex<State>> =
    LazyLock::new(|| Mutex::new(State::new(Locale::initial())));

pub(crate) struct Guard(MutexGuard<'static, State>);
impl Guard {
    /// Native calls must finish before this guard is dropped. The pointer is
    /// immutable and must never be passed as a mutable newlocale base.
    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.0.locale.0.as_ptr()
    }
}
pub(crate) fn current() -> Guard {
    Guard(
        CURRENT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
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
    let native_category = CATEGORIES[category];
    let mut state = current();
    let cache = if let Some(name) = name {
        let next = State::new(state.0.locale.changed(name, native_category)?);
        let decimal = matches!(category, 0 | 4).then(|| next.locale.text(1 << 16));
        let collate_c = matches!(category, 0 | 1).then(|| next.names[3] == b"C");
        let ctype = matches!(category, 0 | 2).then(|| {
            let is_c = next.names[0] == b"C";
            let mut values = [0; 256];
            if !is_c {
                for (byte, slot) in values.iter_mut().enumerate() {
                    let locale = next.locale.0.as_ptr();
                    let byte = byte as i32;
                    // SAFETY: each input is 0..=255, never a negative char.
                    // next owns a live immutable locale throughout the calls.
                    *slot = unsafe {
                        let flags = u32::from(isupper_l(byte, locale) != 0)
                            | (u32::from(islower_l(byte, locale) != 0) << 1)
                            | (u32::from(isdigit_l(byte, locale) != 0) << 2)
                            | (u32::from(isspace_l(byte, locale) != 0) << 3)
                            | (u32::from(ispunct_l(byte, locale) != 0) << 4)
                            | (u32::from(iscntrl_l(byte, locale) != 0) << 5)
                            | (u32::from(isxdigit_l(byte, locale) != 0) << 7)
                            | (u32::from(isalpha_l(byte, locale) != 0) << 8);
                        (flags << 16)
                            | ((toupper_l(byte, locale) as u8 as u32) << 8)
                            | tolower_l(byte, locale) as u8 as u32
                    };
                }
            }
            (values, is_c)
        });
        *state.0 = next;
        Some(Cache {
            decimal,
            ctype,
            collate_c,
        })
    } else {
        None
    };
    Some(Change {
        result: state.0.name(native_category),
        cache,
    })
}

pub(super) fn compare(left: &[u8], right: &[u8]) -> Ordering {
    let mut a = left.to_vec();
    a.push(0);
    let mut b = right.to_vec();
    b.push(0);
    let locale = current();
    let (mut x, mut y) = (0, 0);
    loop {
        // SAFETY: each suffix is terminated inside an owned buffer. The guard
        // pins this immutable locale for every comparison of binary segments.
        let result = unsafe {
            strcoll_l(
                a.as_ptr().add(x).cast(),
                b.as_ptr().add(y).cast(),
                locale.as_ptr(),
            )
        };
        if result != 0 {
            return result.cmp(&0);
        }
        let length = a[x..].iter().position(|&c| c == 0).unwrap();
        if length == right.len().saturating_sub(y) {
            return if length == left.len() - x {
                Ordering::Equal
            } else {
                Ordering::Greater
            };
        }
        if length == left.len() - x {
            return Ordering::Less;
        }
        x += length + 1;
        y += length + 1;
        if y > right.len() {
            return Ordering::Greater;
        }
    }
}
pub(crate) fn error_message(code: i32) -> Vec<u8> {
    let locale = current();
    // SAFETY: strerror_l accepts any errno and a live owned locale. Its
    // locale-owned/thread-local string is copied before another native call.
    let text = unsafe { strerror_l(code, locale.as_ptr()) };
    if text.is_null() {
        return b"Unknown error".to_vec();
    }
    // SAFETY: a nonnull strerror_l result is NUL-terminated and remains valid
    // through this copy; no other call occurs on the same native thread.
    unsafe { CStr::from_ptr(text) }.to_bytes().to_vec()
}
