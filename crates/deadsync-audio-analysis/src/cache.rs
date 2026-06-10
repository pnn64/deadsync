use crate::ReplayGainInfo;
use bincode::{Decode, Encode};
use std::hash::Hasher;
use std::path::Path;
use twox_hash::XxHash64;

const CACHE_MAGIC: u64 = 0x44535952_47414946; // "DSYRGAIF" - file cache.
const CACHE_VERSION: u32 = 1;

#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq)]
pub struct ReplayGainCacheEntry {
    pub path_hash: u64,
    pub mtime_unix_nanos: u64,
    pub lufs: f32,
    pub true_peak_linear: f32,
}

impl ReplayGainCacheEntry {
    #[inline]
    pub fn new(path_hash: u64, mtime_unix_nanos: u64, info: ReplayGainInfo) -> Self {
        Self {
            path_hash,
            mtime_unix_nanos,
            lufs: info.lufs,
            true_peak_linear: info.true_peak_linear,
        }
    }

    #[inline]
    pub fn info(self) -> ReplayGainInfo {
        ReplayGainInfo {
            lufs: self.lufs,
            true_peak_linear: self.true_peak_linear,
        }
    }
}

#[derive(Encode, Decode, Default, Clone, Debug, PartialEq)]
pub struct ReplayGainCacheFile {
    pub entries: Vec<ReplayGainCacheEntry>,
}

pub fn encode_replaygain_cache(payload: &ReplayGainCacheFile) -> Result<Vec<u8>, String> {
    let body =
        bincode::encode_to_vec(payload, bincode::config::standard()).map_err(|e| format!("{e}"))?;
    let mut out = Vec::with_capacity(12 + body.len());
    out.extend_from_slice(&CACHE_MAGIC.to_le_bytes());
    out.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_replaygain_cache(bytes: &[u8]) -> Option<ReplayGainCacheFile> {
    if bytes.len() < 12 {
        return None;
    }
    let magic = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    if magic != CACHE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    if version != CACHE_VERSION {
        return None;
    }
    let (payload, _) = bincode::decode_from_slice::<ReplayGainCacheFile, _>(
        &bytes[12..],
        bincode::config::standard(),
    )
    .ok()?;
    Some(payload)
}

#[inline]
pub fn replaygain_path_hash(path: &Path) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(path.as_os_str().to_string_lossy().as_bytes());
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<ReplayGainCacheEntry> {
        vec![
            ReplayGainCacheEntry {
                path_hash: 0x1111_1111_1111_1111,
                mtime_unix_nanos: 123_456_789_000,
                lufs: -22.5,
                true_peak_linear: 0.83,
            },
            ReplayGainCacheEntry {
                path_hash: 0xfeed_face_dead_beef,
                mtime_unix_nanos: 987_654_321_000,
                lufs: -9.7,
                true_peak_linear: 1.12,
            },
        ]
    }

    #[test]
    fn cache_file_roundtrip() {
        let payload = ReplayGainCacheFile {
            entries: sample_entries(),
        };
        let bytes = encode_replaygain_cache(&payload).expect("encode");
        let decoded = decode_replaygain_cache(&bytes).expect("decode");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn cache_file_rejects_bad_magic() {
        let payload = ReplayGainCacheFile {
            entries: sample_entries(),
        };
        let mut bytes = encode_replaygain_cache(&payload).expect("encode");
        bytes[0] ^= 0xff;
        assert!(decode_replaygain_cache(&bytes).is_none());
    }

    #[test]
    fn cache_file_rejects_bad_version() {
        let payload = ReplayGainCacheFile {
            entries: sample_entries(),
        };
        let mut bytes = encode_replaygain_cache(&payload).expect("encode");
        bytes[8] = bytes[8].wrapping_add(1);
        assert!(decode_replaygain_cache(&bytes).is_none());
    }

    #[test]
    fn cache_file_rejects_header_truncation() {
        let payload = ReplayGainCacheFile {
            entries: sample_entries(),
        };
        let bytes = encode_replaygain_cache(&payload).expect("encode");

        assert!(decode_replaygain_cache(&[]).is_none());
        assert!(decode_replaygain_cache(&bytes[..8]).is_none());
        assert!(decode_replaygain_cache(&bytes[..11]).is_none());
    }

    #[test]
    fn cache_entry_returns_replaygain_info() {
        let info = ReplayGainInfo {
            lufs: -18.25,
            true_peak_linear: 0.92,
        };
        let entry = ReplayGainCacheEntry::new(123, 456, info);

        assert_eq!(entry.info(), info);
        assert_eq!(entry.path_hash, 123);
        assert_eq!(entry.mtime_unix_nanos, 456);
    }
}
