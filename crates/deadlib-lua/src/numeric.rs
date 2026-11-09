//! Allocation-free runtime numeric conversion, including localized input.

#[derive(Clone)]
struct Normalized<'a> {
    bytes: &'a [u8],
    point: &'a [u8],
}
impl Iterator for Normalized<'_> {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        let &first = self.bytes.first()?;
        if self.bytes.starts_with(self.point) {
            self.bytes = &self.bytes[self.point.len()..];
            Some(b'.')
        } else {
            self.bytes = &self.bytes[1..];
            Some(first)
        }
    }
}

pub(crate) fn localized_number(bytes: &[u8], point: &[u8]) -> Option<f64> {
    assert!(!point.is_empty());
    let mut normalized = Normalized { bytes, point }.peekable();
    let negative = normalized.peek() == Some(&b'-');
    if matches!(normalized.peek(), Some(b'-' | b'+')) {
        normalized.next();
    }
    let mut prefix = normalized.clone();
    if prefix.next() == Some(b'0') && matches!(prefix.next(), Some(b'x' | b'X')) {
        return hex_number(prefix, negative);
    }
    decimal_number(normalized, negative)
}

// A textual exponent may be arbitrarily long. Saturation here cannot hide a
// finite result: input lengths fit usize, far below i128's exponent magnitude.
fn exponent(bytes: impl Iterator<Item = u8>) -> Option<i128> {
    let mut bytes = bytes.peekable();
    let negative = bytes.peek() == Some(&b'-');
    if matches!(bytes.peek(), Some(b'-' | b'+')) {
        bytes.next();
    }
    let mut any = false;
    let mut value = 0i128;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        any = true;
        value = value
            .saturating_mul(10)
            .saturating_add(i128::from(byte - b'0'));
    }
    any.then_some(if negative { -value } else { value })
}

fn decimal_number(mut bytes: impl Iterator<Item = u8>, negative: bool) -> Option<f64> {
    // Every binary64 rounding midpoint has at most 768 significant decimal
    // digits: its denominator divides 2^1075 and its significand has at most
    // 54 bits. Keep 800 plus a sticky digit. Replacing a nonzero discarded
    // suffix by that digit preserves which side of every midpoint it occupies.
    // The complete input is still scanned and validated; this is not a limit
    // on input length. Rust's core f64 parser performs the final rounding.
    // The pinned UCRT keeps only 768 digits and discards the remaining tail
    // (including its sticky information). Keep that observable profile rule.
    const WINDOWS: bool = cfg!(all(windows, target_env = "msvc"));
    const KEPT: usize = if WINDOWS { 768 } else { 800 };
    let mut text = [0u8; KEPT + 8];
    let mut len = 0;
    let mut dot = false;
    let mut any = false;
    let mut sticky = false;
    let mut fractional = 0i128;
    let mut skipped = 0i128;
    let mut power = 0i128;
    while let Some(byte) = bytes.next() {
        if byte == b'.' && !dot {
            dot = true;
        } else if byte.is_ascii_digit() {
            any = true;
            fractional += i128::from(dot);
            if len == 0 && byte == b'0' {
                continue;
            }
            if len < KEPT {
                text[len] = byte;
                len += 1;
            } else {
                skipped += 1;
                sticky |= byte != b'0';
            }
        } else if matches!(byte, b'e' | b'E') {
            power = exponent(bytes)?;
            break;
        } else {
            return None;
        }
    }
    if !any {
        return None;
    }
    if len == 0 {
        return Some(if negative { -0.0 } else { 0.0 });
    }
    if WINDOWS && power.abs() > 5200 {
        let value = if power > 0 { f64::INFINITY } else { 0.0 };
        return Some(if negative { -value } else { value });
    }
    power = power.saturating_sub(fractional).saturating_add(skipped);
    if sticky && !WINDOWS {
        text[len] = b'1';
        len += 1;
        power = power.saturating_sub(1);
    }
    // With at most 801 mantissa digits these exponent clamps are safely past
    // binary64 overflow and underflow, and need only four decimal digits.
    let power = power.clamp(-2000, 2000) as i32;
    text[len] = b'e';
    len += 1;
    if power < 0 {
        text[len] = b'-';
        len += 1;
    }
    let mut digits = power.unsigned_abs();
    let start = len;
    loop {
        text[len] = b'0' + (digits % 10) as u8;
        len += 1;
        digits /= 10;
        if digits == 0 {
            break;
        }
    }
    text[start..len].reverse();
    let value: f64 = std::str::from_utf8(&text[..len]).ok()?.parse().ok()?;
    Some(if negative { -value } else { value })
}

pub(crate) fn hex_number(mut bytes: impl Iterator<Item = u8>, negative: bool) -> Option<f64> {
    let mut bits = 0u64;
    let mut kept = 0;
    let mut skipped = 0i128;
    let mut fractional = 0i128;
    let mut dot = false;
    let mut any = false;
    let mut sticky = false;
    let mut power = 0i128;
    let mut prefix = 0u64;
    while let Some(byte) = bytes.next() {
        if matches!(byte, b'p' | b'P') {
            power = exponent(bytes)?;
            break;
        }
        if byte == b'.' {
            if dot {
                return None;
            }
            dot = true;
            continue;
        }
        if dot {
            fractional += 4;
        }
        let digit = (byte as char).to_digit(16)?;
        any = true;
        if prefix <= 0x001f_ffff_ffff_ffff {
            prefix = (prefix << 4) | u64::from(digit);
        }
        if kept == 0 && digit == 0 {
            continue;
        }
        if kept < 16 {
            bits = (bits << 4) | u64::from(digit);
            kept += 1;
        } else {
            skipped += 4;
            if !cfg!(all(windows, target_env = "msvc")) || skipped <= (768 - 16) * 4 {
                sticky |= digit != 0;
            }
        }
    }
    if !any {
        return None;
    }
    let sign = u64::from(negative) << 63;
    if bits == 0 {
        return Some(f64::from_bits(sign));
    }
    if cfg!(all(windows, target_env = "msvc")) && power.abs() > 5200 {
        return Some(f64::from_bits(
            sign | if power > 0 { 0x7ff0_0000_0000_0000 } else { 0 },
        ));
    }
    let exponent = power.saturating_sub(fractional).saturating_add(skipped);
    let length = 64 - bits.leading_zeros();
    let top = exponent.saturating_add(i128::from(length) - 1);
    if top > 1023 {
        return Some(f64::from_bits(sign | 0x7ff0_0000_0000_0000));
    }
    let shift = if top < -1022 {
        (-1074i128).saturating_sub(exponent)
    } else {
        i128::from(length) - 53
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
            u64::from((64 - prefix.leading_zeros()).saturating_sub(53)) << 52
        } else {
            rounded
        }
    } else {
        let carry = u64::from(rounded == 1 << 53);
        let mantissa = rounded >> carry;
        let exp = (top + 1023 + i128::from(carry)) as u64;
        if exp >= 2047 {
            0x7ff0_0000_0000_0000
        } else {
            (exp << 52) | (mantissa & 0x000f_ffff_ffff_ffff)
        }
    };
    Some(f64::from_bits(sign | value))
}

#[cfg(test)]
mod tests {
    use super::localized_number;

    #[test]
    fn localized_stream_accepts_multibyte_and_long_separators() {
        let long = format!("1.{}1", "0".repeat(5000));
        for point in [b"::".as_slice(), b"\xd9\xab", b"\xff\xfe", &[b'~'; 32]] {
            for (source, expected) in [
                ("1.25", Some(1.25f64)),
                ("-0.0", Some(-0.0)),
                ("+0x1.8p+1", Some(3.0)),
                (long.as_str(), Some(1.0)),
                ("1.2.3", None),
                ("0x1.2p1x", None),
            ] {
                let mut bytes = Vec::new();
                for byte in source.bytes() {
                    if byte == b'.' {
                        bytes.extend_from_slice(point);
                    } else {
                        bytes.push(byte);
                    }
                }
                assert_eq!(
                    localized_number(&bytes, point).map(f64::to_bits),
                    expected.map(f64::to_bits)
                );
            }
        }
    }
}
