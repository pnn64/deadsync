const AXIS_ZERO_CUTOFF_BITS: u64 = 0x3f00000000000000;
const F64_INFINITY_BITS: u64 = 0x7ff0000000000000;
pub fn scale_axis(v: f64) -> i16 {
    let bits = v.to_bits();
    let magnitude = bits & 0x7fff_ffff_ffff_ffff;
    let keep = (AXIS_ZERO_CUTOFF_BITS..F64_INFINITY_BITS).contains(&magnitude);
    let v = f64::from_bits(bits & u64::from(keep).wrapping_neg());
    // The i16 cast already saturates the upper bound at 32767.
    let v = v.max(-1.0);
    (v * 32767.0) as i16
}
pub fn scale_trigger(v: f64) -> i16 {
    // Unsigned bit ordering also excludes every negative input.
    let bits = v.to_bits();
    let keep = (AXIS_ZERO_CUTOFF_BITS..F64_INFINITY_BITS).contains(&bits);
    let v = f64::from_bits(bits & u64::from(keep).wrapping_neg());
    (v * 32767.0) as i16
}