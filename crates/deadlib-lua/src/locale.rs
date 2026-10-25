//! Lua's shared locale state, with Rust fast paths for the initial C locale.
use crate::Error;
#[cfg(not(all(windows, target_env = "msvc")))]
use crate::ErrorKind;
use std::{
    borrow::Cow,
    cmp::Ordering,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering as AtomicOrdering},
    },
};

static DECIMAL: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(b".".to_vec()));
static DECIMAL_DOT: AtomicBool = AtomicBool::new(true);
static CTYPE_C: AtomicBool = AtomicBool::new(true);
static COLLATE_C: AtomicBool = AtomicBool::new(true);
static REVISION: AtomicU64 = AtomicU64::new(0);
// Low/upper mapped bytes and the platform character-class mask in one word.
static CTYPE: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

#[cfg(all(windows, target_env = "msvc"))]
#[allow(unsafe_code)]
mod windows;

pub(crate) fn set(name: Option<&[u8]>, category: usize) -> Result<Option<Vec<u8>>, Error> {
    #[cfg(all(windows, target_env = "msvc"))]
    {
        let mut decimal = DECIMAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(change) = windows::set(name, category) else {
            return Ok(None);
        };
        if let Some(cache) = change.cache {
            if let Some(point) = cache.decimal {
                *decimal = point;
                DECIMAL_DOT.store(decimal.as_slice() == b".", AtomicOrdering::Release);
            }
            if let Some((ctype, ctype_c)) = cache.ctype {
                for (slot, value) in CTYPE.iter().zip(ctype) {
                    slot.store(value, AtomicOrdering::Relaxed);
                }
                CTYPE_C.store(ctype_c, AtomicOrdering::Release);
                REVISION.fetch_add(1, AtomicOrdering::Release);
            }
            if let Some(collate_c) = cache.collate_c {
                COLLATE_C.store(collate_c, AtomicOrdering::Release);
            }
        }
        Ok(Some(change.result))
    }
    #[cfg(not(all(windows, target_env = "msvc")))]
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
pub(crate) fn decimal() -> Cow<'static, [u8]> {
    if DECIMAL_DOT.load(AtomicOrdering::Acquire) {
        Cow::Borrowed(b".")
    } else {
        Cow::Owned(
            DECIMAL
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone(),
        )
    }
}
pub(crate) fn localize(bytes: &mut Vec<u8>) {
    if !DECIMAL_DOT.load(AtomicOrdering::Acquire)
        && let Some(at) = bytes.iter().position(|&b| b == b'.')
    {
        let point = decimal();
        bytes.splice(at..at + 1, point.iter().copied());
    }
}
fn mask(byte: u8) -> u32 {
    CTYPE[usize::from(byte)].load(AtomicOrdering::Relaxed) >> 16
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
pub(crate) fn map_case(source: &[u8], upper: bool, output: &mut Vec<u8>) {
    if CTYPE_C.load(AtomicOrdering::Acquire) {
        output.extend(source.iter().map(|b| {
            if upper {
                b.to_ascii_uppercase()
            } else {
                b.to_ascii_lowercase()
            }
        }));
    } else {
        let shift = if upper { 8 } else { 0 };
        output.extend(
            source
                .iter()
                .map(|&b| (CTYPE[usize::from(b)].load(AtomicOrdering::Relaxed) >> shift) as u8),
        );
    }
}
pub(crate) fn compare(left: &[u8], right: &[u8]) -> Ordering {
    if COLLATE_C.load(AtomicOrdering::Acquire) {
        return left.cmp(right);
    }
    #[cfg(all(windows, target_env = "msvc"))]
    {
        windows::compare(left, right)
    }
    #[cfg(not(all(windows, target_env = "msvc")))]
    {
        left.cmp(right)
    }
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
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at..].starts_with(&point) {
            normalized.push(b'.');
            at += point.len();
        } else {
            normalized.push(bytes[at]);
            at += 1;
        }
    }
    parse_normalized(&normalized)
}
fn parse_normalized(bytes: &[u8]) -> Option<f64> {
    let (negative, unsigned) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if unsigned.starts_with(b"0x") || unsigned.starts_with(b"0X") {
        let hex = &unsigned[2..];
        let end = hex
            .iter()
            .position(|b| matches!(b, b'p' | b'P'))
            .unwrap_or(hex.len());
        let digits = &hex[..end];
        if !digits.iter().any(u8::is_ascii_hexdigit)
            || digits.iter().filter(|&&b| b == b'.').count() > 1
            || digits.iter().any(|b| !b.is_ascii_hexdigit() && *b != b'.')
        {
            return None;
        }
        return crate::numeric::hex_number(hex, negative);
    }
    crate::lexer::parse_number(bytes)
}
