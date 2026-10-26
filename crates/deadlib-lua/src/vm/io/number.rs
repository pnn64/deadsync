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
    while file.peek()?.is_some_and(crate::locale::is_space) {
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
    let point = crate::locale::decimal();
    if file.peek()? == point.first().copied() {
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
    let value = parse_number(bytes);
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
