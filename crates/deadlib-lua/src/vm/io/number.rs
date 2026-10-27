//! Stream scanning preserves the native runtime's partial-token consumption.
use super::{ERRNO, parse_number, stream::Stream};

fn lower(byte: u8) -> u8 {
    if cfg!(all(target_os = "linux", target_env = "gnu")) {
        crate::locale::lower(byte)
    } else {
        byte.to_ascii_lowercase()
    }
}
fn word(file: &mut Stream, bytes: &mut Vec<u8>, text: &[u8]) -> Result<bool, i32> {
    for &expected in text {
        let byte = file.peek()?;
        if byte.map(lower) != Some(expected) {
            // glibc consumes the mismatching character of a failed inf/nan
            // word. The UCRT leaves that character unread.
            if cfg!(all(target_os = "linux", target_env = "gnu")) && byte.is_some() {
                file.get()?;
            }
            return Ok(false);
        }
        bytes.push(file.get()?.unwrap());
    }
    Ok(true)
}
pub(super) fn read_number(file: &mut Stream, bytes: &mut Vec<u8>) -> Result<Option<f64>, i32> {
    file.flush()?;
    while file.peek()?.is_some_and(crate::locale::is_space) {
        file.get()?;
    }
    if file.peek()?.is_some_and(|b| matches!(b, b'+' | b'-')) {
        bytes.push(file.get()?.unwrap());
    }
    let sign = usize::from(!bytes.is_empty());
    let negative = bytes.first() == Some(&b'-');
    match file.peek()?.map(lower) {
        Some(b'i') => {
            if !word(file, bytes, b"inf")? {
                return Ok(None);
            }
            if file.peek()?.is_some_and(|b| lower(b) == b'i') && !word(file, bytes, b"inity")? {
                return Ok(None);
            }
            // scanf matches the word with LC_CTYPE, then converts its ASCII
            // numeric prefix. A Turkish dotted I can match but not convert.
            if cfg!(all(target_os = "linux", target_env = "gnu"))
                && !bytes[sign..sign + 3].eq_ignore_ascii_case(b"inf")
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
                let tail = file.peek()?;
                if tail != Some(b')') {
                    if cfg!(all(target_os = "linux", target_env = "gnu")) && tail.is_some() {
                        file.get()?;
                    }
                    return Ok(None);
                }
                if !cfg!(all(target_os = "linux", target_env = "gnu"))
                    && bytes[start..].eq_ignore_ascii_case(b"ind")
                {
                    bits = 0xfff8_0000_0000_0000;
                }
                if !cfg!(all(target_os = "linux", target_env = "gnu"))
                    && bytes[start..].eq_ignore_ascii_case(b"snan")
                {
                    bits = 0x7ff0_0000_0000_0001;
                }
                bytes.push(file.get()?.unwrap());
            }
            if cfg!(all(target_os = "linux", target_env = "gnu"))
                && !bytes[sign..sign + 3].eq_ignore_ascii_case(b"nan")
            {
                return Ok(None);
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
    let point = crate::locale::decimal();
    let has_point = file.peek()? == point.first().copied();
    if has_point {
        bytes.push(file.get()?.unwrap());
        for &byte in &point[1..] {
            if file.peek()? != Some(byte) {
                return Ok(None);
            }
            bytes.push(file.get()?.unwrap());
        }
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
        // glibc accepts a hexadecimal prefix followed only by its decimal
        // point as zero. A bare 0x still fails, and no exponent is consumed.
        if cfg!(all(target_os = "linux", target_env = "gnu")) && hex && has_point {
            return Ok(Some(if negative { -0.0 } else { 0.0 }));
        }
        return Ok(None);
    }
    let exponent_start = bytes.len();
    let mut exponent_digits = 0;
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
            exponent_digits += 1;
        }
    }
    // glibc scanf consumes a started exponent even if it has no digits, but
    // converts the preceding mantissa. Keep the consumed bytes in the buffer.
    let value = if cfg!(all(target_os = "linux", target_env = "gnu")) && exponent_digits == 0 {
        parse_number(&bytes[..exponent_start])
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
