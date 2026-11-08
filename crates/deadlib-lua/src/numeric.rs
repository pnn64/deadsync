//! Shared, correctly rounded binary64 conversion for native hexadecimal input.
pub(crate) fn hex_number(bytes: &[u8], negative: bool) -> Option<f64> {
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
