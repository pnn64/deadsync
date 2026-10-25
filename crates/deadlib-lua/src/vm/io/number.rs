//! Stream scanning keeps the first nonnumeric byte available to the next read.
use super::{ERRNO, parse_number, stream::Stream};

fn word(file: &mut Stream, bytes: &mut Vec<u8>, text: &[u8]) -> Result<bool, i32> {
    for &expected in text {
        if file.peek()?.map(|b| b.to_ascii_lowercase()) != Some(expected) {
            return Ok(false);
        }
        bytes.push(file.get()?.unwrap());
    }
    Ok(true)
}
pub(super) fn read_number(file: &mut Stream, bytes: &mut Vec<u8>) -> Result<Option<f64>, i32> {
    file.flush()?;
    while file
        .peek()?
        .is_some_and(|b| matches!(b, b'\t'..=b'\r' | b' '))
    {
        file.get()?;
    }
    if file.peek()?.is_some_and(|b| matches!(b, b'+' | b'-')) {
        bytes.push(file.get()?.unwrap());
    }
    let sign = usize::from(!bytes.is_empty());
    let negative = bytes.first() == Some(&b'-');
    match file.peek()?.map(|b| b.to_ascii_lowercase()) {
        Some(b'i') => {
            if !word(file, bytes, b"inf")? {
                return Ok(None);
            }
            if file.peek()?.is_some_and(|b| b.eq_ignore_ascii_case(&b'i'))
                && !word(file, bytes, b"inity")?
            {
                return Ok(None);
            }
            return Ok(Some(if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            }));
        }
        Some(b'n') => {
            if !word(file, bytes, b"nan")? {
                return Ok(None);
            }
            let mut bits = 0x7ff8_0000_0000_0000u64;
            if file.peek()? == Some(b'(') {
                bytes.push(file.get()?.unwrap());
                let start = bytes.len();
                while file
                    .peek()?
                    .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    bytes.push(file.get()?.unwrap());
                }
                if file.peek()? != Some(b')') {
                    return Ok(None);
                }
                if bytes[start..].eq_ignore_ascii_case(b"ind") {
                    bits = 0xfff8_0000_0000_0000;
                }
                if bytes[start..].eq_ignore_ascii_case(b"snan") {
                    bits = 0x7ff0_0000_0000_0001;
                }
                bytes.push(file.get()?.unwrap());
            }
            if negative {
                bits |= 1 << 63;
            }
            return Ok(Some(f64::from_bits(bits)));
        }
        _ => {}
    }
    let mut digits = 0;
    while file.peek()?.is_some_and(|b| b.is_ascii_digit()) {
        bytes.push(file.get()?.unwrap());
        digits += 1;
    }
    let hex = digits == 1
        && bytes[sign] == b'0'
        && file.peek()?.is_some_and(|b| b.eq_ignore_ascii_case(&b'x'));
    if hex {
        bytes.push(file.get()?.unwrap());
        digits = 0;
        while file.peek()?.is_some_and(|b| b.is_ascii_hexdigit()) {
            bytes.push(file.get()?.unwrap());
            digits += 1;
        }
    }
    if file.peek()? == Some(b'.') {
        bytes.push(file.get()?.unwrap());
        while file.peek()?.is_some_and(|b| {
            if hex {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        }) {
            bytes.push(file.get()?.unwrap());
            digits += 1;
        }
    }
    if digits == 0 {
        return Ok(None);
    }
    if file.peek()?.is_some_and(|b| {
        if hex {
            matches!(b, b'p' | b'P')
        } else {
            matches!(b, b'e' | b'E')
        }
    }) {
        bytes.push(file.get()?.unwrap());
        if file.peek()?.is_some_and(|b| matches!(b, b'+' | b'-')) {
            bytes.push(file.get()?.unwrap());
        }
        while file.peek()?.is_some_and(|b| b.is_ascii_digit()) {
            bytes.push(file.get()?.unwrap());
        }
    }
    let value = if hex {
        hex_number(&bytes[sign + 2..], negative)
    } else {
        parse_number(bytes)
    };
    if value.is_some_and(|n| {
        n.is_infinite()
            || (n == 0.0
                && bytes[sign..]
                    .iter()
                    .take_while(|&&b| !matches!(b, b'e' | b'E' | b'p' | b'P'))
                    .any(|b| matches!(b, b'1'..=b'9')))
    }) {
        ERRNO.set(34);
    }
    Ok(value)
}

fn hex_number(bytes: &[u8], negative: bool) -> Option<f64> {
    let (digits, exponent) = if let Some(at) = bytes.iter().position(|b| matches!(b, b'p' | b'P')) {
        let exp = std::str::from_utf8(&bytes[at + 1..]).ok()?;
        if exp.is_empty() || matches!(exp, "+" | "-") {
            return None;
        }
        let parsed = exp.parse::<i32>().unwrap_or(if exp.starts_with('-') {
            i32::MIN
        } else {
            i32::MAX
        });
        (&bytes[..at], parsed)
    } else {
        (bytes, 0)
    };
    let mut bits = 0u64;
    let mut kept = 0;
    let mut skipped = 0i32;
    let mut fractional = 0i32;
    let mut dot = false;
    let mut sticky = false;
    for &byte in digits {
        if byte == b'.' {
            dot = true;
            continue;
        }
        if dot {
            fractional = fractional.saturating_add(4);
        }
        let digit = (byte as char).to_digit(16)?;
        if kept == 0 && digit == 0 {
            continue;
        }
        if kept < 16 {
            bits = (bits << 4) | u64::from(digit);
            kept += 1;
        } else {
            skipped = skipped.saturating_add(4);
            sticky |= digit != 0;
        }
    }
    let sign = u64::from(negative) << 63;
    if bits == 0 {
        return Some(f64::from_bits(sign));
    }
    let exponent = i64::from(exponent) - i64::from(fractional) + i64::from(skipped);
    let length = 64 - bits.leading_zeros();
    let top = exponent + i64::from(length) - 1;
    if top > 1023 {
        return Some(f64::from_bits(sign | 0x7ff0_0000_0000_0000));
    }
    let shift = if top < -1022 {
        -1074 - exponent
    } else {
        i64::from(length) - 53
    };
    let rounded = if shift <= 0 {
        bits << (-shift as u32)
    } else if shift > 64 {
        0
    } else {
        let half = 1u64 << (shift - 1);
        let low = bits & (half - 1);
        let high = if shift == 64 { 0 } else { bits >> shift };
        high + u64::from(bits & half != 0 && (low != 0 || sticky || high & 1 != 0))
    };
    let value = if top < -1022 {
        if cfg!(all(windows, target_env = "msvc")) && rounded == 1 << 52 {
            // The pinned UCRT scanner's subnormal carry uses the width of its
            // bounded hexadecimal prefix. Preserve that observable CRT quirk.
            let mut prefix = 0u64;
            for &byte in digits {
                if byte != b'.' {
                    prefix = (prefix << 4) | u64::from((byte as char).to_digit(16)?);
                    if prefix > 0x001f_ffff_ffff_ffff {
                        break;
                    }
                }
            }
            u64::from((64 - prefix.leading_zeros()).saturating_sub(53)) << 52
        } else {
            rounded
        }
    } else {
        let carry = u64::from(rounded == 1 << 53);
        let mantissa = rounded >> carry;
        let exp = (top + 1023 + carry as i64) as u64;
        if exp >= 2047 {
            0x7ff0_0000_0000_0000
        } else {
            (exp << 52) | (mantissa & 0x000f_ffff_ffff_ffff)
        }
    };
    Some(f64::from_bits(sign | value))
}
