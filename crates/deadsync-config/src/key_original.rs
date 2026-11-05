// Frozen from 381612308 for exact classification/record/caller comparisons.
use super::{MachinePreferredPlayStyle, ThemeFlag};
use ::null_or_die::KernelTarget;
const NORMALIZED_KEY_CAPACITY: usize = 32;
struct NormalizedAsciiKey {
    bytes: [u8; NORMALIZED_KEY_CAPACITY],
    len: u8,
}
impl NormalizedAsciiKey {
    fn parse(raw: &str) -> Option<Self> {
        let mut key = Self {
            bytes: [0; NORMALIZED_KEY_CAPACITY],
            len: 0,
        };
        for byte in raw.trim().bytes() {
            if !byte.is_ascii_alphanumeric() {
                continue;
            }
            let index = usize::from(key.len);
            if index == NORMALIZED_KEY_CAPACITY {
                return None;
            }
            key.bytes[index] = byte.to_ascii_lowercase();
            key.len += 1;
        }
        Some(key)
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}
pub fn key(raw: &str) -> Option<([u8; 32], u8)> {
    NormalizedAsciiKey::parse(raw).map(|key| (key.bytes, key.len))
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TwitchChannel {
    bytes: [u8; 25],
    len: u8,
}
impl TwitchChannel {
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
        if raw.len() > 25 || !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        let mut channel = Self::default();
        for (out, byte) in channel.bytes.iter_mut().zip(raw.bytes()) {
            *out = byte.to_ascii_lowercase();
        }
        channel.len = raw.len() as u8;
        Some(channel)
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }

    pub const fn is_empty(self) -> bool {
        self.len == 0
    }
}
pub fn target(raw: &str) -> Option<KernelTarget> {
    let mut key = raw
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| byte.to_ascii_lowercase());
    match key.next()? {
        b'd' if key.by_ref().eq(b"igest".iter().copied()) => Some(KernelTarget::Digest),
        b'a' if key.by_ref().eq(b"ccumulator".iter().copied()) => Some(KernelTarget::Accumulator),
        _ => None,
    }
}
pub fn theme(s: &str) -> Result<ThemeFlag, ()> {
    let Some(key) = NormalizedAsciiKey::parse(s) else {
        return Err(());
    };
    match key.as_str() {
        "simplylove" => Ok(ThemeFlag::SimplyLove),
        _ => Err(()),
    }
}
pub fn style(s: &str) -> Result<MachinePreferredPlayStyle, ()> {
    let Some(key) = NormalizedAsciiKey::parse(s) else {
        return Err(());
    };
    match key.as_str() {
        "single" | "1player" | "oneplayer" => Ok(MachinePreferredPlayStyle::Single),
        "versus" | "2player" | "2players" | "twoplayer" | "twoplayers" => {
            Ok(MachinePreferredPlayStyle::Versus)
        }
        "double" => Ok(MachinePreferredPlayStyle::Double),
        _ => Err(()),
    }
}
