// Frozen from e79ff01306d8a3cc5af795f6db2e69e7df8b1919 for exact integer/enum result comparisons.
use super::{BiasKernel, CoinMode, KernelTarget};
#[inline(always)]
#[must_use]
pub fn seconds_to_us_u32(seconds: f32) -> u32 {
    let micros = (seconds * 1_000_000.0).max(0.0);
    if micros > u32::MAX as f32 {
        u32::MAX
    } else {
        micros as u32
    }
}
#[inline(always)]
#[must_use]
pub fn stutter_severity(frame_seconds: f32, expected_seconds: f32) -> u8 {
    if expected_seconds <= 0.0 {
        return 0;
    }
    let thresholds = [expected_seconds * 2.0, expected_seconds * 4.0, 0.1];
    let mut severity: u8 = 0;
    while usize::from(severity) < thresholds.len()
        && frame_seconds > thresholds[usize::from(severity)]
    {
        severity = severity.saturating_add(1);
    }
    severity
}
pub fn parse_null_or_die_kernel_type(raw: &str) -> Option<BiasKernel> {
    let mut key = raw
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase());
    match key.next()? {
        b'r' if key.by_ref().eq(b"ising".iter().copied()) => Some(BiasKernel::Rising),
        b'l' if key.by_ref().eq(b"oudest".iter().copied()) => Some(BiasKernel::Loudest),
        _ => None,
    }
}
pub fn parse_null_or_die_kernel_target(raw: &str) -> Option<KernelTarget> {
    const MAP: [u8; 256] = {
        let mut map = [0u8; 256];
        let mut byte = 0usize;
        while byte < 256 {
            let value = byte as u8;
            if value.is_ascii_alphanumeric() {
                map[byte] = value.to_ascii_lowercase();
            }
            byte += 1;
        }
        map
    };
    let mut key = raw.bytes().filter_map(|byte| {
        let mapped = MAP[usize::from(byte)];
        (mapped != 0).then_some(mapped)
    });
    match key.next()? {
        b'd' if key.by_ref().eq(b"igest".iter().copied()) => Some(KernelTarget::Digest),
        b'a' if key.by_ref().eq(b"ccumulator".iter().copied()) => Some(KernelTarget::Accumulator),
        _ => None,
    }
}
pub fn coin(value: &str) -> Result<CoinMode, ()> {
    match value.trim() {
        value if value.eq_ignore_ascii_case("pay") => Ok(CoinMode::Pay),
        value if value.eq_ignore_ascii_case("free") => Ok(CoinMode::Free),
        value if value.eq_ignore_ascii_case("home") => Ok(CoinMode::Home),
        _ => Err(()),
    }
}
