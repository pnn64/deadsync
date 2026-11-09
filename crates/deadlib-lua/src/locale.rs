//! Lua's shared locale state, with Rust fast paths for the initial C locale.
use crate::Error;
#[cfg(not(any(
    all(windows, target_env = "msvc"),
    all(target_os = "linux", target_env = "gnu")
)))]
use crate::ErrorKind;
use std::{
    cmp::Ordering,
    sync::{
        Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering},
    },
};

static DECIMAL: Mutex<Decimal> = Mutex::new(Decimal::Dot);
static DECIMAL_DOT: AtomicBool = AtomicBool::new(true);
static COLLATE_C: AtomicBool = AtomicBool::new(true);
static REVISION: AtomicU64 = AtomicU64::new(0);
// Publish the class/case table and its C-locale flag as one snapshot.
struct Ctype {
    values: [u32; 256],
    is_c: bool,
}
static CTYPE: RwLock<Ctype> = RwLock::new(Ctype {
    values: [0; 256],
    is_c: true,
});

#[cfg(all(windows, target_env = "msvc"))]
#[allow(unsafe_code)]
pub(crate) mod windows;
#[cfg(all(windows, target_env = "msvc"))]
use windows as native;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(unsafe_code)]
pub(crate) mod posix;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use posix as native;

pub(crate) fn set(name: Option<&[u8]>, category: usize) -> Result<Option<Vec<u8>>, Error> {
    #[cfg(any(
        all(windows, target_env = "msvc"),
        all(target_os = "linux", target_env = "gnu")
    ))]
    {
        let mut decimal = DECIMAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(change) = native::set(name, category)? else {
            return Ok(None);
        };
        if let Some(cache) = change.cache {
            if let Some(point) = cache.decimal {
                *decimal = point;
                DECIMAL_DOT.store(&**decimal == b".", AtomicOrdering::Release);
            }
            if let Some((ctype, ctype_c)) = cache.ctype {
                *CTYPE
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Ctype {
                    values: ctype,
                    is_c: ctype_c,
                };
                REVISION.fetch_add(1, AtomicOrdering::Release);
            }
            if let Some(collate_c) = cache.collate_c {
                COLLATE_C.store(collate_c, AtomicOrdering::Release);
            }
        }
        // Windows may have applied the native selection before copying its
        // returned name fails. Publish its allocation-free cache first, so a
        // memory error never leaves the numeric/ctype rules on the old locale.
        change.result.map(Some)
    }
    #[cfg(not(any(
        all(windows, target_env = "msvc"),
        all(target_os = "linux", target_env = "gnu")
    )))]
    {
        let _ = (name, category);
        Err(Error::new(
            ErrorKind::Unsupported,
            0,
            "native locales are not implemented on this platform",
        ))
    }
}

pub(crate) fn revision() -> u64 {
    REVISION.load(AtomicOrdering::Acquire)
}
#[derive(Clone)]
pub(crate) enum Decimal {
    Dot,
    Inline {
        bytes: [u8; 16],
        len: usize,
    },
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    Shared(crate::memory::Shared<Vec<u8>>),
}
impl Decimal {
    fn inline(value: &[u8]) -> Option<Self> {
        if value == b"." {
            return Some(Self::Dot);
        }
        let mut bytes = [0; 16];
        bytes.get_mut(..value.len())?.copy_from_slice(value);
        Some(Self::Inline {
            bytes,
            len: value.len(),
        })
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    fn try_new(value: &[u8]) -> Result<Self, Error> {
        use crate::memory::{Memory, Shared, fallible::SliceExt};
        match Self::inline(value) {
            Some(value) => Ok(value),
            None => Ok(Self::Shared(Shared::try_new_in(
                value.try_to_vec()?,
                Memory::default(),
            )?)),
        }
    }
}
impl std::ops::Deref for Decimal {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Dot => b".",
            Self::Inline { bytes, len } => &bytes[..*len],
            #[cfg(all(target_os = "linux", target_env = "gnu"))]
            Self::Shared(bytes) => bytes,
        }
    }
}
pub(crate) fn decimal() -> Decimal {
    if DECIMAL_DOT.load(AtomicOrdering::Acquire) {
        Decimal::Dot
    } else {
        DECIMAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
pub(crate) fn try_localize(bytes: &mut Vec<u8>) -> Result<(), Error> {
    if !DECIMAL_DOT.load(AtomicOrdering::Acquire)
        && let Some(at) = bytes.iter().position(|&b| b == b'.')
    {
        let point = decimal();
        bytes
            .try_reserve(point.len().saturating_sub(1))
            .map_err(|_| Error::memory())?;
        bytes.splice(at..at + 1, point.iter().copied());
    }
    Ok(())
}
pub(crate) fn try_localize_owned(bytes: &mut crate::memory::Buffer<u8>) -> Result<(), Error> {
    if !DECIMAL_DOT.load(AtomicOrdering::Acquire)
        && let Some(at) = bytes.iter().position(|&b| b == b'.')
    {
        // Snapshot before allocation: a custom callback may enter a distinct
        // state and use numeric formatting without holding this process lock.
        let point = decimal();
        let len = bytes.len();
        let next = len
            .checked_sub(1)
            .and_then(|len| len.checked_add(point.len()))
            .ok_or_else(Error::memory)?;
        bytes.try_reserve(next.saturating_sub(len))?;
        if next > len {
            bytes.resize(next, 0);
        }
        bytes.copy_within(at + 1..len, at + point.len());
        bytes.truncate(next);
        bytes[at..at + point.len()].copy_from_slice(&point);
    }
    Ok(())
}
fn mask(byte: u8) -> u32 {
    CTYPE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .values[usize::from(byte)]
        >> 16
}
pub(crate) fn is_alpha(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || (byte >= 128 && mask(byte) & 0x103 != 0)
}
pub(crate) fn is_alnum(byte: u8) -> bool {
    is_digit(byte) || is_alpha(byte)
}
pub(crate) fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit() || (byte >= 128 && mask(byte) & 4 != 0)
}
pub(crate) fn is_space(byte: u8) -> bool {
    matches!(byte, b'\t'..=b'\r' | b' ') || (byte >= 128 && mask(byte) & 8 != 0)
}
pub(crate) fn class(byte: u8, class: u8) -> bool {
    if byte >= 128 {
        let bits = mask(byte);
        let hit = match class.to_ascii_lowercase() {
            b'a' => bits & 0x103 != 0,
            b'c' => bits & 32 != 0,
            b'd' => bits & 4 != 0,
            b'l' => bits & 2 != 0,
            b'p' => bits & 16 != 0,
            b's' => bits & 8 != 0,
            b'u' => bits & 1 != 0,
            b'w' => bits & 0x107 != 0,
            b'x' => bits & 128 != 0,
            b'z' => false,
            _ => return byte == class,
        };
        return hit != class.is_ascii_uppercase();
    }
    let hit = match class.to_ascii_lowercase() {
        b'a' => byte.is_ascii_alphabetic(),
        b'c' => byte.is_ascii_control(),
        b'd' => byte.is_ascii_digit(),
        b'l' => byte.is_ascii_lowercase(),
        b'p' => byte.is_ascii_punctuation(),
        b's' => is_space(byte),
        b'u' => byte.is_ascii_uppercase(),
        b'w' => byte.is_ascii_alphanumeric(),
        b'x' => byte.is_ascii_hexdigit(),
        b'z' => byte == 0,
        _ => return byte == class,
    };
    hit != class.is_ascii_uppercase()
}
pub(crate) fn map_case(source: &[u8], upper: bool, output: &mut crate::memory::Buffer<u8>) {
    let ctype = CTYPE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if ctype.is_c {
        for byte in source.iter().map(|b| {
            if upper {
                b.to_ascii_uppercase()
            } else {
                b.to_ascii_lowercase()
            }
        }) {
            output.push(byte);
        }
    } else {
        let shift = if upper { 8 } else { 0 };
        for &byte in source {
            output.push((ctype.values[usize::from(byte)] >> shift) as u8);
        }
    }
}
pub(crate) fn lower(byte: u8) -> u8 {
    let ctype = CTYPE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if ctype.is_c {
        byte.to_ascii_lowercase()
    } else {
        ctype.values[usize::from(byte)] as u8
    }
}
pub(crate) fn compare(left: &[u8], right: &[u8]) -> Result<Ordering, Error> {
    if COLLATE_C.load(AtomicOrdering::Acquire) {
        return Ok(left.cmp(right));
    }
    #[cfg(any(
        all(windows, target_env = "msvc"),
        all(target_os = "linux", target_env = "gnu")
    ))]
    {
        native::compare(left, right)
    }
    #[cfg(not(any(
        all(windows, target_env = "msvc"),
        all(target_os = "linux", target_env = "gnu")
    )))]
    {
        Ok(left.cmp(right))
    }
}

fn terminated(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let mut result = crate::memory::fallible::with_capacity(
        bytes.len().checked_add(1).ok_or_else(Error::memory)?,
    )?;
    result.extend_from_slice(bytes);
    result.push(0);
    Ok(result)
}

pub(crate) fn parse_number(bytes: &[u8]) -> Option<f64> {
    let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
    let start = bytes
        .iter()
        .position(|&b| !is_space(b))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| !is_space(b))
        .map_or(start, |i| i + 1);
    let bytes = &bytes[start..end];
    if DECIMAL_DOT.load(AtomicOrdering::Acquire) {
        return parse_normalized(bytes);
    }
    if bytes.contains(&b'.') {
        return None;
    }
    let point = decimal();
    if point.is_empty() || !bytes.windows(point.len()).any(|window| window == &*point) {
        return parse_normalized(bytes);
    }
    crate::numeric::localized_number(bytes, &point)
}
fn parse_normalized(bytes: &[u8]) -> Option<f64> {
    let (negative, unsigned) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if unsigned.starts_with(b"0x") || unsigned.starts_with(b"0X") {
        let hex = &unsigned[2..];
        return crate::numeric::hex_number(hex.iter().copied(), negative);
    }
    // UCRT's runtime converter truncates its decimal significand after 768
    // digits and clamps the textual exponent before adjusting for the radix
    // position. Short inputs cannot expose either difference from core's parser.
    if cfg!(all(windows, target_env = "msvc")) && bytes.len() > 768 {
        return crate::numeric::localized_number(bytes, b".");
    }
    crate::lexer::parse_number(bytes)
}
