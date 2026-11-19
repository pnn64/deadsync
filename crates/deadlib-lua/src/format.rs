//! Pure Rust formatting for Lua 5.1's supported C-style conversion specifiers.
use crate::{
    Error,
    memory::{Buffer, Memory},
};
use std::fmt::Write as _;

#[derive(Default, Debug)]
pub(crate) struct Spec {
    pub conversion: u8,
    pub width: usize,
    pub precision: Option<usize>,
    pub left: bool,
    pub plus: bool,
    pub space: bool,
    pub alternate: bool,
    pub zero: bool,
    // UCRT printf emits an unrecognized high-byte digit and the following
    // format suffix literally. Lua's scanner still recognizes it as a digit.
    literal: [u8; 6],
    literal_len: u8,
}

impl Spec {
    pub fn parse(bytes: &[u8], pos: &mut usize) -> Result<Self, Error> {
        let mut s = Self::default();
        let start = *pos;
        while let Some(&c) = bytes.get(*pos) {
            match c {
                b'-' => s.left = true,
                b'+' => s.plus = true,
                b' ' => s.space = true,
                b'#' => s.alternate = true,
                b'0' => s.zero = true,
                _ => break,
            }
            *pos += 1;
        }
        if *pos - start >= 6 {
            return Err(fail("invalid format (repeated flags)"));
        }
        for _ in 0..2 {
            if let Some(c) = bytes.get(*pos).filter(|&&c| crate::locale::is_digit(c)) {
                s.width = s.width * 10 + usize::from(*c - b'0');
                *pos += 1;
            } else {
                break;
            }
        }
        if bytes.get(*pos) == Some(&b'.') {
            *pos += 1;
            let mut n = 0;
            for _ in 0..2 {
                if let Some(c) = bytes.get(*pos).filter(|&&c| crate::locale::is_digit(c)) {
                    n = n * 10 + usize::from(*c - b'0');
                    *pos += 1;
                } else {
                    break;
                }
            }
            s.precision = Some(n);
        }
        if bytes.get(*pos).is_some_and(|&c| crate::locale::is_digit(c)) {
            return Err(fail("invalid format (width or precision too long)"));
        }
        s.conversion = bytes.get(*pos).copied().unwrap_or(0);
        *pos += 1;
        if let Some(first) = bytes[start..(*pos).min(bytes.len())]
            .iter()
            .position(|&b| b >= 128)
        {
            let suffix = &bytes[start + first..(*pos).min(bytes.len())];
            s.literal[..suffix.len()].copy_from_slice(suffix);
            s.literal_len = suffix.len() as u8;
        }
        Ok(s)
    }
    fn literal(&self, integer: bool, memory: &mut Scratch) -> Result<Buffer<u8>, Error> {
        let bytes = &self.literal[..usize::from(self.literal_len)];
        if integer {
            let mut value = buffer(memory, bytes.len() + 1)?;
            value.extend_from_slice(&bytes[..bytes.len() - 1]);
            value.push(b'l');
            value.push(self.conversion);
            Ok(value)
        } else {
            copy(memory, bytes)
        }
    }
    fn pad(
        &self,
        mut bytes: Buffer<u8>,
        prefix: usize,
        zero: bool,
        memory: &mut Scratch,
    ) -> Result<Buffer<u8>, Error> {
        let extra = self.width.saturating_sub(bytes.len());
        if extra == 0 {
            return Ok(bytes);
        }
        if self.left {
            memory.reserve(&mut bytes, extra)?;
            bytes.resize(bytes.len() + extra, b' ');
            return Ok(bytes);
        }
        let mut out = buffer(memory, bytes.len() + extra)?;
        if zero {
            out.extend_from_slice(&bytes[..prefix]);
            out.resize(prefix + extra, b'0');
            out.extend_from_slice(&bytes[prefix..]);
        } else {
            out.resize(extra, b' ');
            out.extend_from_slice(&bytes);
        }
        Ok(out)
    }
    /// The bytes Lua 5.1's unmodified `%s` would copy, without a temporary buffer.
    /// Short inputs use C-string termination; inputs of 100 bytes or more are
    /// copied in full when no precision is specified, including embedded NULs.
    pub fn plain_string_bytes<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        if self.conversion != b's'
            || self.width != 0
            || self.precision.is_some()
            || self.left
            || self.plus
            || self.space
            || self.alternate
            || self.zero
            || self.literal_len != 0
        {
            return None;
        }
        if bytes.len() >= 100 {
            return Some(bytes);
        }
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Some(&bytes[..len])
    }
    pub fn string(&self, bytes: &[u8], memory: &mut Scratch) -> Result<Buffer<u8>, Error> {
        if self.precision.is_none() && bytes.len() >= 100 {
            return copy(memory, bytes);
        }
        if self.literal_len != 0 {
            return self.literal(false, memory);
        }
        let limit = bytes.len().min(self.precision.unwrap_or(usize::MAX));
        let len = bytes[..limit].iter().position(|&b| b == 0).unwrap_or(limit);
        self.pad(copy(memory, &bytes[..len])?, 0, false, memory)
    }
    pub fn character(&self, n: f64, memory: &mut Scratch) -> Result<Buffer<u8>, Error> {
        if self.literal_len != 0 {
            return self.literal(false, memory);
        }
        let bytes = self.pad(copy(memory, &[cast_i32(n) as u8])?, 0, false, memory)?;
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let mut bytes = bytes;
        bytes.truncate(len);
        Ok(bytes)
    }
    pub fn integer(&self, n: f64, memory: &mut Scratch) -> Result<Buffer<u8>, Error> {
        if self.literal_len != 0 {
            return self.literal(true, memory);
        }
        // Stock 5.1 uses C long here, including 32-bit long on i686.
        let signed = matches!(self.conversion, b'd' | b'i');
        let (magnitude, negative) = if signed {
            let number = if std::mem::size_of::<std::ffi::c_long>() == 4 {
                i64::from(cast_i32(n))
            } else {
                cast_i64(n)
            };
            (number.unsigned_abs(), number < 0)
        } else {
            let value = if std::mem::size_of::<std::ffi::c_ulong>() == 4 {
                cast_i64(n) as u32 as u64
            } else if n >= 9223372036854775808.0 {
                (cast_i64(n - 9223372036854775808.0) as u64) | (1u64 << 63)
            } else {
                cast_i64(n) as u64
            };
            (value, false)
        };
        const LOWER: &[u8; 16] = b"0123456789abcdef";
        const UPPER: &[u8; 16] = b"0123456789ABCDEF";
        // Octal needs the most space: 22 digits for a u64.
        let mut storage = [0; 22];
        let digits = if magnitude == 0 && self.precision == Some(0) {
            &[]
        } else {
            match self.conversion {
                b'o' => integer_digits::<8>(magnitude, LOWER, &mut storage),
                b'x' => integer_digits::<16>(magnitude, LOWER, &mut storage),
                b'X' => integer_digits::<16>(magnitude, UPPER, &mut storage),
                _ => integer_digits::<10>(magnitude, LOWER, &mut storage),
            }
        };
        let leading = self.precision.unwrap_or(0).saturating_sub(digits.len());
        let mut bytes = buffer(memory, leading + digits.len() + 3)?;
        if negative {
            bytes.push(b'-');
        } else if signed && self.plus {
            bytes.push(b'+');
        } else if signed && self.space {
            bytes.push(b' ');
        }
        if self.alternate {
            if matches!(self.conversion, b'x' | b'X') && magnitude != 0 {
                bytes.extend_from_slice(if self.conversion == b'x' {
                    b"0x"
                } else {
                    b"0X"
                });
            }
            if self.conversion == b'o' && leading == 0 && digits.first() != Some(&b'0') {
                bytes.push(b'0');
            }
        }
        let prefix = bytes.len();
        bytes.resize(prefix + leading, b'0');
        bytes.extend_from_slice(digits);
        self.pad(bytes, prefix, self.zero && self.precision.is_none(), memory)
    }
    pub fn float(&self, n: f64, memory: &mut Scratch) -> Result<Buffer<u8>, Error> {
        if self.literal_len != 0 {
            return self.literal(false, memory);
        }
        let negative = n.is_sign_negative();
        let value = n.abs();
        let precision = self.precision.unwrap_or(6);
        let upper = matches!(self.conversion, b'E' | b'G');
        let mut body = if value.is_nan() {
            Text::copy(nan_body(n))
        } else if value.is_infinite() {
            Text::copy("inf")
        } else if self.conversion == b'f' {
            let mut text = Text::format(format_args!("{value:.precision$}"));
            if precision == 0 && self.alternate {
                text.push(b'.');
            }
            text
        } else {
            self.scientific(value, precision)
        };
        if upper {
            body.make_ascii_uppercase();
        }
        let mut bytes = buffer(memory, body.len + 1)?;
        if negative {
            bytes.push(b'-');
        } else if self.plus {
            bytes.push(b'+');
        } else if self.space {
            bytes.push(b' ');
        }
        let prefix = bytes.len();
        bytes.extend_from_slice(body.as_bytes());
        let capacity = bytes.capacity();
        let result = crate::locale::try_localize_owned(&mut bytes);
        memory.allocated = memory.allocated.saturating_add(bytes.capacity() - capacity);
        result?;
        self.pad(bytes, prefix, self.zero && value.is_finite(), memory)
    }

    fn scientific(&self, value: f64, precision: usize) -> Text {
        let general = matches!(self.conversion, b'g' | b'G');
        let digits = if general {
            precision.max(1)
        } else {
            precision + 1
        };
        let mut text = Text::format(format_args!("{value:.p$e}", p = digits - 1));
        let marker = text
            .as_bytes()
            .iter()
            .rposition(|&byte| byte == b'e')
            .expect("scientific format includes exponent");
        let (sign, exponent) = match &text.as_bytes()[marker + 1..] {
            [sign @ (b'-' | b'+'), exponent @ ..] => (*sign, exponent),
            exponent => (b'+', exponent),
        };
        if general {
            let magnitude = exponent
                .iter()
                .fold(0, |n, &byte| n * 10 + i32::from(byte - b'0'));
            let exponent = if sign == b'-' { -magnitude } else { magnitude };
            if (-4..digits as i32).contains(&exponent) {
                let p = (digits as i32 - 1 - exponent) as usize;
                let mut fixed = Text::format(format_args!("{value:.p$}"));
                if p == 0 && self.alternate {
                    fixed.push(b'.');
                } else if p != 0 && !self.alternate {
                    fixed.len = trimmed_fraction(fixed.as_bytes());
                }
                return fixed;
            }
        }

        // Keep the rounded mantissa and exponent digits in the original buffer.
        let exponent_len = exponent.len();
        let mantissa_len = if general && !self.alternate {
            trimmed_fraction(&text.bytes[..marker])
        } else {
            marker
        };
        let point = self.alternate && digits == 1;
        let suffix = mantissa_len + usize::from(point);
        let leading_zero = exponent_len == 1;
        let exponent_start = suffix + 2 + usize::from(leading_zero);
        text.bytes
            .copy_within(text.len - exponent_len..text.len, exponent_start);
        if point {
            text.bytes[mantissa_len] = b'.';
        }
        text.bytes[suffix..suffix + 2].copy_from_slice(&[b'e', sign]);
        if leading_zero {
            text.bytes[suffix + 2] = b'0';
        }
        text.len = exponent_start + exponent_len;
        text
    }
}

fn trimmed_fraction(bytes: &[u8]) -> usize {
    let end = bytes
        .iter()
        .rposition(|&byte| byte != b'0')
        .map_or(0, |i| i + 1);
    end - usize::from(bytes[..end].last() == Some(&b'.'))
}

#[cfg(test)]
#[test]
fn scientific_suffix_preserves_rounded_digits_and_exponent_width() {
    for (conversion, alternate, precision, value, expected) in [
        (b'e', true, 0, 0.0, "0.e+00"),
        (b'e', false, 6, 0.00125, "1.250000e-03"),
        (b'e', false, 6, f64::from_bits(1), "4.940656e-324"),
        (b'g', false, 6, 999999.5, "1e+06"),
        (b'g', true, 1, 1e101, "1.e+101"),
        (b'g', false, 5, 0.0001, "0.0001"),
        (b'g', true, 6, 100000., "100000."),
    ] {
        let spec = Spec {
            conversion,
            alternate,
            ..Spec::default()
        };
        assert_eq!(
            spec.scientific(value, precision).as_bytes(),
            expected.as_bytes()
        );
    }
}

fn integer_digits<'a, const RADIX: u64>(
    mut magnitude: u64,
    alphabet: &'static [u8; 16],
    storage: &'a mut [u8; 22],
) -> &'a [u8] {
    for (start, digit) in storage.iter_mut().enumerate().rev() {
        *digit = alphabet[(magnitude % RADIX) as usize];
        magnitude /= RADIX;
        if magnitude == 0 {
            return &storage[start..];
        }
    }
    unreachable!("u64 fits in 22 digits for the supported radices")
}

#[cfg(test)]
#[test]
fn integer_digits_cover_each_radix_and_full_width() {
    let mut storage = [0; 22];
    let lower = b"0123456789abcdef";
    let upper = b"0123456789ABCDEF";
    for n in [0, 7, 8, 9, 10, 15, 16, 99, 100, 1 << 63, u64::MAX] {
        assert_eq!(
            integer_digits::<8>(n, lower, &mut storage),
            format!("{n:o}").as_bytes()
        );
        assert_eq!(
            integer_digits::<10>(n, lower, &mut storage),
            n.to_string().as_bytes()
        );
        assert_eq!(
            integer_digits::<16>(n, lower, &mut storage),
            format!("{n:x}").as_bytes()
        );
        assert_eq!(
            integer_digits::<16>(n, upper, &mut storage),
            format!("{n:X}").as_bytes()
        );
    }
}

pub(crate) fn nan_body(n: f64) -> &'static str {
    if cfg!(windows) && n.to_bits() == 0xfff8000000000000 {
        "nan(ind)"
    } else if cfg!(windows) && n.to_bits() & (1u64 << 51) == 0 {
        "nan(snan)"
    } else {
        "nan"
    }
}
fn cast_i32(n: f64) -> i32 {
    if !(-2147483648.0..2147483648.0).contains(&n) {
        i32::MIN
    } else {
        n as i32
    }
}
fn cast_i64(n: f64) -> i64 {
    if !n.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&n) {
        i64::MIN
    } else {
        n as i64
    }
}
fn fail(message: &'static str) -> Error {
    Error::runtime_static(message)
}

pub(crate) fn quote(bytes: &[u8], out: &mut Buffer<u8>, memory: &mut Scratch) -> Result<(), Error> {
    let unescaped = bytes.len().checked_add(2).ok_or_else(Error::memory)?;
    // A 4 KiB block adds at most 12 KiB, which fits in the narrow sum.
    let size = bytes
        .chunks(4096)
        .try_fold(unescaped, |size, block| {
            let extra: u16 = block
                .iter()
                .map(|&byte| match byte {
                    0 => 3,
                    b'"' | b'\\' | b'\n' | b'\r' => 1,
                    _ => 0,
                })
                .sum();
            size.checked_add(usize::from(extra))
        })
        .ok_or_else(Error::memory)?;
    memory.reserve(out, size)?;
    out.push(b'"');
    if size == unescaped {
        out.extend_from_slice(bytes);
    } else {
        for &c in bytes {
            match c {
                b'"' | b'\\' | b'\n' => {
                    out.push(b'\\');
                    out.push(c);
                }
                b'\r' => out.extend_from_slice(b"\\r"),
                0 => out.extend_from_slice(b"\\000"),
                _ => out.push(c),
            }
        }
    }
    out.push(b'"');
    Ok(())
}

pub(crate) fn buffer(memory: &mut Scratch, capacity: usize) -> Result<Buffer<u8>, Error> {
    let mut bytes = Buffer::new_in(memory.memory.clone());
    bytes.try_reserve_exact(capacity)?;
    memory.allocated = memory.allocated.saturating_add(bytes.capacity());
    Ok(bytes)
}
fn copy(memory: &mut Scratch, source: &[u8]) -> Result<Buffer<u8>, Error> {
    let mut bytes = buffer(memory, source.len())?;
    bytes.extend_from_slice(source);
    Ok(bytes)
}

// Two ASCII precision digits bound every numeric conversion to at most 410
// bytes (309 integer digits, decimal point and 99 fractional digits, plus sign).
// The non-ASCII locale-digit path is emitted literally before formatting.
struct Text {
    bytes: [u8; 512],
    len: usize,
}
impl Text {
    fn copy(text: &str) -> Self {
        let mut value = Self {
            bytes: [0; 512],
            len: 0,
        };
        value.write_str(text).expect("bounded numeric text");
        value
    }
    fn format(args: std::fmt::Arguments<'_>) -> Self {
        let mut value = Self::copy("");
        value.write_fmt(args).expect("bounded numeric format");
        value
    }
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    fn push(&mut self, byte: u8) {
        self.bytes[self.len] = byte;
        self.len += 1;
    }
    fn make_ascii_uppercase(&mut self) {
        self.bytes[..self.len].make_ascii_uppercase();
    }
}
impl std::fmt::Write for Text {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(std::fmt::Error)?;
        let target = self.bytes.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        target.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

pub(crate) struct Scratch {
    memory: Memory,
    pub(crate) allocated: usize,
}
impl Scratch {
    pub(crate) fn new(memory: Memory) -> Self {
        Self {
            memory,
            allocated: 0,
        }
    }
    pub(crate) fn reserve(
        &mut self,
        bytes: &mut Buffer<u8>,
        additional: usize,
    ) -> Result<(), Error> {
        let capacity = bytes.capacity();
        let result = bytes.try_reserve(additional);
        self.allocated = self.allocated.saturating_add(bytes.capacity() - capacity);
        result
    }
}
