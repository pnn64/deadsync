//! Pure Rust formatting for Lua 5.1's supported C-style conversion specifiers.
use crate::{Error, ErrorKind};

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
            if let Some(c @ b'0'..=b'9') = bytes.get(*pos) {
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
                if let Some(c @ b'0'..=b'9') = bytes.get(*pos) {
                    n = n * 10 + usize::from(*c - b'0');
                    *pos += 1;
                } else {
                    break;
                }
            }
            s.precision = Some(n);
        }
        if bytes.get(*pos).is_some_and(u8::is_ascii_digit) {
            return Err(fail("invalid format (width or precision too long)"));
        }
        s.conversion = bytes.get(*pos).copied().unwrap_or(0);
        *pos += 1;
        Ok(s)
    }
    fn pad(&self, mut bytes: Vec<u8>, prefix: usize, zero: bool) -> Vec<u8> {
        let extra = self.width.saturating_sub(bytes.len());
        if extra == 0 {
            return bytes;
        }
        if self.left {
            bytes.resize(bytes.len() + extra, b' ');
            return bytes;
        }
        let mut out = Vec::with_capacity(bytes.len() + extra);
        if zero {
            out.extend_from_slice(&bytes[..prefix]);
            out.resize(prefix + extra, b'0');
            out.extend_from_slice(&bytes[prefix..]);
        } else {
            out.resize(extra, b' ');
            out.extend_from_slice(&bytes);
        }
        out
    }
    pub fn string(&self, bytes: &[u8]) -> Vec<u8> {
        if self.precision.is_none() && bytes.len() >= 100 {
            return bytes.to_vec();
        }
        let len = bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(bytes.len())
            .min(self.precision.unwrap_or(usize::MAX));
        self.pad(bytes[..len].to_vec(), 0, false)
    }
    pub fn character(&self, n: f64) -> Vec<u8> {
        let bytes = self.pad(vec![cast_i32(n) as u8], 0, false);
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        bytes[..len].to_vec()
    }
    pub fn integer(&self, n: f64) -> Vec<u8> {
        // Stock 5.1 uses C long here: 32 bits on Windows, 64 on LP64 systems.
        let signed = matches!(self.conversion, b'd' | b'i');
        let (magnitude, negative) = if signed {
            let number = if cfg!(windows) {
                i64::from(cast_i32(n))
            } else {
                cast_i64(n)
            };
            (number.unsigned_abs(), number < 0)
        } else {
            let value = if cfg!(windows) {
                cast_i64(n) as u32 as u64
            } else if n >= 9223372036854775808.0 {
                (cast_i64(n - 9223372036854775808.0) as u64) | (1u64 << 63)
            } else {
                cast_i64(n) as u64
            };
            (value, false)
        };
        let mut digits = match self.conversion {
            b'o' => format!("{magnitude:o}"),
            b'x' => format!("{magnitude:x}"),
            b'X' => format!("{magnitude:X}"),
            _ => magnitude.to_string(),
        }
        .into_bytes();
        if magnitude == 0 && self.precision == Some(0) {
            digits.clear();
        }
        let leading = self.precision.unwrap_or(0).saturating_sub(digits.len());
        let mut bytes = Vec::with_capacity(leading + digits.len() + 3);
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
        bytes.extend_from_slice(&digits);
        self.pad(bytes, prefix, self.zero && self.precision.is_none())
    }
    pub fn float(&self, n: f64) -> Vec<u8> {
        let negative = n.is_sign_negative();
        let value = n.abs();
        let precision = self.precision.unwrap_or(6);
        let upper = matches!(self.conversion, b'E' | b'G');
        let mut body = if value.is_nan() {
            nan_body(n).to_string()
        } else if value.is_infinite() {
            "inf".to_string()
        } else if self.conversion == b'f' {
            let mut text = format!("{value:.precision$}");
            if precision == 0 && self.alternate {
                text.push('.');
            }
            text
        } else {
            let general = matches!(self.conversion, b'g' | b'G');
            let digits = if general {
                precision.max(1)
            } else {
                precision + 1
            };
            let scientific = format!("{value:.p$e}", p = digits - 1);
            let (mantissa, exponent) = scientific
                .split_once('e')
                .expect("scientific format includes exponent");
            let exponent: i32 = exponent.parse().expect("formatted exponent is an integer");
            if !general || exponent < -4 || exponent >= digits as i32 {
                let mut mantissa = if general && !self.alternate {
                    mantissa
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                } else {
                    mantissa.to_string()
                };
                if self.alternate && !mantissa.contains('.') {
                    mantissa.push('.');
                }
                format!(
                    "{mantissa}e{}{e:02}",
                    if exponent < 0 { '-' } else { '+' },
                    e = exponent.unsigned_abs()
                )
            } else {
                let p = (digits as i32 - 1 - exponent).max(0) as usize;
                let fixed = format!("{value:.p$}");
                if !self.alternate && fixed.contains('.') {
                    fixed
                        .trim_end_matches('0')
                        .trim_end_matches('.')
                        .to_string()
                } else if self.alternate && !fixed.contains('.') {
                    fixed + "."
                } else {
                    fixed
                }
            }
        };
        if upper {
            body.make_ascii_uppercase();
        }
        let mut bytes = Vec::with_capacity(body.len() + 1);
        if negative {
            bytes.push(b'-');
        } else if self.plus {
            bytes.push(b'+');
        } else if self.space {
            bytes.push(b' ');
        }
        let prefix = bytes.len();
        bytes.extend_from_slice(body.as_bytes());
        self.pad(bytes, prefix, self.zero && value.is_finite())
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
fn fail(message: &str) -> Error {
    Error::new(ErrorKind::Runtime, 0, message)
}

pub(crate) fn quote(bytes: &[u8], out: &mut Vec<u8>) {
    out.push(b'"');
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
    out.push(b'"');
}
