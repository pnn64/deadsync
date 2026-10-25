// Frozen production conversions from 3f3152be3, for differential checks.
pub fn encode_half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let magnitude = bits & 0x7fff_ffff;
    if magnitude >= 0x7f80_0000 {
        return sign | 0x7c00 | if magnitude == 0x7f80_0000 { 0 } else { 0x0200 };
    }
    let exponent = ((magnitude >> 23) & 0xff) as i32 - 127;
    if exponent > 15 {
        return sign | 0x7c00;
    }
    if exponent >= -14 {
        let rounded = magnitude + 0x0fff + ((magnitude >> 13) & 1);
        return sign | ((rounded >> 13) - 0x1c000) as u16;
    }
    if exponent < -25 {
        return sign;
    }
    let mantissa = (magnitude & 0x007f_ffff) | 0x0080_0000;
    let shift = (-exponent - 1) as u32;
    let rounded = mantissa + ((1 << (shift - 1)) - 1) + ((mantissa >> shift) & 1);
    sign | (rounded >> shift) as u16
}

pub fn decode_half(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 0x1f);
    let mantissa = u32::from(bits & 0x03ff);
    let magnitude = match exponent {
        0 if mantissa == 0 => 0,
        0 => {
            let shift = mantissa.leading_zeros() - 21;
            ((113 - shift) << 23) | (((mantissa << shift) & 0x03ff) << 13)
        }
        31 => 0x7f80_0000 | (mantissa << 13),
        _ => ((exponent + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(sign | magnitude)
}
