//! Normalization for saved DeadSync preference values.

// All accepted punctuation-insensitive preference aliases fit in this stack
// buffer. A longer normalized key cannot match any supported alias.
const NORMALIZED_KEY_CAPACITY: usize = 32;

/// A case-insensitive ASCII key with punctuation and whitespace removed.
pub struct NormalizedAsciiKey {
    bytes: [u8; NORMALIZED_KEY_CAPACITY],
    len: u8,
}

impl NormalizedAsciiKey {
    /// Normalize a saved preference without allocating.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
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
        let mut key = Self {
            bytes: [0; NORMALIZED_KEY_CAPACITY],
            len: 0,
        };
        let mut len = 0usize;
        for byte in raw.bytes() {
            let byte = MAP[usize::from(byte)];
            if byte == 0 {
                continue;
            }
            let index = len;
            if index == NORMALIZED_KEY_CAPACITY {
                return None;
            }
            key.bytes[index] = byte;
            len += 1;
        }
        key.len = len as u8;
        Some(key)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_normalized_keys_strip_punctuation_and_non_ascii() {
        for (raw, expected) in [
            ("Simply Love", "simplylove"),
            ("  POST-kernel_fingerprint ", "postkernelfingerprint"),
            ("1-player", "1player"),
            ("sïmply_love", "smplylove"),
            ("___", ""),
            ("S----------------------------RPG10", "srpg10"),
        ] {
            let key = NormalizedAsciiKey::parse(raw).expect("valid-size normalized key");
            assert_eq!(key.as_str(), expected, "input {raw:?}");
        }

        let too_long = "a".repeat(NORMALIZED_KEY_CAPACITY + 1);
        assert!(NormalizedAsciiKey::parse(&too_long).is_none());
    }
}
