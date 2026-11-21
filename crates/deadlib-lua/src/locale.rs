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
        Mutex, RwLock, RwLockReadGuard,
        atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering},
    },
};

static DECIMAL: Mutex<Decimal> = Mutex::new(Decimal::Dot);
static DECIMAL_DOT: AtomicBool = AtomicBool::new(true);
static COLLATE_C: AtomicBool = AtomicBool::new(true);
static REVISION: AtomicU64 = AtomicU64::new(0);
static CTYPE_C: AtomicBool = AtomicBool::new(true);
static CTYPE: RwLock<[u32; 256]> = RwLock::new([0; 256]);

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
                let mut values = CTYPE
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *values = ctype;
                CTYPE_C.store(ctype_c, AtomicOrdering::Release);
                drop(values);
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
    try_localize_owned_from(bytes, 0)
}
pub(crate) fn try_localize_owned_from(
    bytes: &mut crate::memory::Buffer<u8>,
    start: usize,
) -> Result<(), Error> {
    if !DECIMAL_DOT.load(AtomicOrdering::Acquire)
        && let Some(offset) = bytes[start..].iter().position(|&b| b == b'.')
    {
        let at = start + offset;
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
fn ctype() -> Option<RwLockReadGuard<'static, [u32; 256]>> {
    if CTYPE_C.load(AtomicOrdering::Acquire) {
        return None;
    }
    let values = CTYPE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A locale change can publish C before this reader acquires the lock.
    (!CTYPE_C.load(AtomicOrdering::Relaxed)).then_some(values)
}
fn mask(byte: u8) -> u32 {
    ctype().map_or(0, |values| values[usize::from(byte)] >> 16)
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
pub(crate) fn ascii_space(byte: u8) -> bool {
    (byte == b' ') | (byte.wrapping_sub(b'\t') <= b'\r' - b'\t')
}
pub(crate) fn is_space(byte: u8) -> bool {
    ascii_space(byte) || (byte >= 128 && mask(byte) & 8 != 0)
}
pub(crate) fn is_control(byte: u8) -> bool {
    byte.is_ascii_control() || (byte >= 128 && mask(byte) & 32 != 0)
}

pub(crate) const fn ascii_class_bits(class: u8) -> [u64; 2] {
    let mut bits = [0; 2];
    let mut byte = 0u8;
    while byte < 128 {
        let hit = match class {
            b'a' => byte.is_ascii_alphabetic(),
            b'c' => byte.is_ascii_control(),
            b'd' => byte.is_ascii_digit(),
            b'l' => byte.is_ascii_lowercase(),
            b'p' => byte.is_ascii_punctuation(),
            b's' => matches!(byte, b'\t'..=b'\r' | b' '),
            b'u' => byte.is_ascii_uppercase(),
            b'w' => byte.is_ascii_alphanumeric(),
            b'x' => byte.is_ascii_hexdigit(),
            b'z' => byte == 0,
            _ => unreachable!(),
        };
        if hit {
            bits[(byte / 64) as usize] |= 1u64 << (byte % 64);
        }
        byte += 1;
    }
    bits
}

pub(crate) fn add_class_bits(set: &mut [u64; 4], code: u8) {
    let ([lo, hi], flags) = match code.to_ascii_lowercase() {
        b'a' => (const { ascii_class_bits(b'a') }, 0x103),
        b'c' => (const { ascii_class_bits(b'c') }, 32),
        b'd' => (const { ascii_class_bits(b'd') }, 4),
        b'l' => (const { ascii_class_bits(b'l') }, 2),
        b'p' => (const { ascii_class_bits(b'p') }, 16),
        b's' => (const { ascii_class_bits(b's') }, 8),
        b'u' => (const { ascii_class_bits(b'u') }, 1),
        b'w' => (const { ascii_class_bits(b'w') }, 0x107),
        b'x' => (const { ascii_class_bits(b'x') }, 128),
        b'z' => (const { ascii_class_bits(b'z') }, 0),
        _ => {
            set[usize::from(code / 64)] |= 1u64 << (code % 64);
            return;
        }
    };
    let mut bits = [lo, hi, 0, 0];
    if flags != 0
        && let Some(ctype) = ctype()
    {
        for (word, values) in bits[2..].iter_mut().zip(ctype[128..].as_chunks::<64>().0) {
            for (bit, &value) in values.iter().enumerate() {
                *word |= u64::from(value & (flags << 16) != 0) << bit;
            }
        }
    }
    let complement = 0u64.wrapping_sub(u64::from(code.is_ascii_uppercase()));
    for (word, members) in set.iter_mut().zip(bits) {
        *word |= members ^ complement;
    }
}
pub(crate) fn map_case(bytes: &mut [u8], upper: bool) {
    if let Some(ctype) = ctype() {
        let shift = if upper { 8 } else { 0 };
        for byte in bytes {
            *byte = (ctype[usize::from(*byte)] >> shift) as u8;
        }
    } else if upper {
        bytes.make_ascii_uppercase();
    } else {
        bytes.make_ascii_lowercase();
    }
}
pub(crate) fn lower(byte: u8) -> u8 {
    ctype().map_or_else(
        || byte.to_ascii_lowercase(),
        |values| values[usize::from(byte)] as u8,
    )
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
    let nul = if bytes.len() < 32 {
        bytes.iter().position(|&byte| byte == 0)
    } else {
        crate::pattern::find_byte(bytes, 0)
    };
    let bytes = &bytes[..nul.unwrap_or(bytes.len())];
    let ascii_prefix = if bytes.len() >= 128 && ascii_space(bytes[0]) {
        crate::pattern::scan_boundary(bytes, |byte| !ascii_space(byte))
    } else {
        0
    };
    let bytes = &bytes[ascii_prefix..];
    let start = bytes
        .iter()
        .position(|&b| !is_space(b))
        .unwrap_or(bytes.len());
    let bytes = &bytes[start..];
    let end = if bytes.len() >= 64 && bytes.last().is_some_and(|&byte| ascii_space(byte)) {
        trailing_space_end(bytes)
    } else {
        bytes
            .iter()
            .rposition(|&byte| !is_space(byte))
            .map_or(0, |i| i + 1)
    };
    let bytes = &bytes[..end];
    if DECIMAL_DOT.load(AtomicOrdering::Acquire) {
        return parse_normalized(bytes);
    }
    if bytes.contains(&b'.') {
        return None;
    }
    let point = decimal();
    let has_point = match &*point {
        [] => false,
        [byte] => crate::pattern::find_byte(bytes, *byte).is_some(),
        point => bytes.windows(point.len()).any(|window| window == point),
    };
    if !has_point {
        return parse_normalized(bytes);
    }
    crate::numeric::localized_number(bytes, &point)
}

fn trailing_space_end(bytes: &[u8]) -> usize {
    let (head, blocks) = bytes.as_rchunks::<32>();
    for (index, block) in blocks.iter().enumerate().rev() {
        // ASCII-only whitespace needs no locale-sensitive classification.
        if block
            .iter()
            .fold(false, |found, &byte| found | !ascii_space(byte))
            && let Some(offset) = block.iter().rposition(|&byte| !is_space(byte))
        {
            return head.len() + index * 32 + offset + 1;
        }
    }
    head.iter()
        .rposition(|&byte| !is_space(byte))
        .map_or(0, |i| i + 1)
}

fn parse_normalized(bytes: &[u8]) -> Option<f64> {
    let (negative, unsigned) = match bytes.first() {
        Some(b'-') => (true, &bytes[1..]),
        Some(b'+') => (false, &bytes[1..]),
        _ => (false, bytes),
    };
    if let [b'0', b'x' | b'X', hex @ ..] = unsigned {
        let hex = if hex.len() >= 64 && hex[0] == b'0' {
            // Leading integer zeros leave both the significand and radix
            // position unchanged. Keep one so the digit check still applies.
            &hex[crate::pattern::byte_run(hex, b'0') - 1..]
        } else {
            hex
        };
        return crate::numeric::hex_number(hex.iter().copied(), negative);
    }
    if let Some(integer) = crate::lexer::short_decimal_integer(unsigned) {
        return Some(if negative { -integer } else { integer });
    }
    // UCRT's runtime converter truncates its decimal significand after 768
    // digits and clamps the textual exponent before adjusting for the radix
    // position. Short inputs cannot expose either difference from core's parser.
    if cfg!(all(windows, target_env = "msvc")) && bytes.len() > 768 {
        return crate::numeric::decimal_number(unsigned.iter().copied(), negative);
    }
    std::str::from_utf8(bytes).ok()?.parse().ok()
}
