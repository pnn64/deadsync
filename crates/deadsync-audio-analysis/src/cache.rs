use crate::ReplayGainInfo;
use bincode::{Decode, Encode};
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::Path;
use std::time::UNIX_EPOCH;
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

impl ReplayGainCacheFile {
    pub fn from_entries<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = ReplayGainCacheEntry>,
    {
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_by_key(|entry| entry.path_hash);
        Self { entries }
    }

    pub fn into_entry_map(self) -> HashMap<u64, ReplayGainCacheEntry> {
        let mut map = HashMap::with_capacity(self.entries.len());
        for entry in self.entries {
            map.insert(entry.path_hash, entry);
        }
        map
    }
}

pub fn read_replaygain_cache_file(path: &Path) -> Option<HashMap<u64, ReplayGainCacheEntry>> {
    let bytes = fs::read(path).ok()?;
    Some(decode_replaygain_cache(&bytes)?.into_entry_map())
}

pub fn write_replaygain_cache_file(
    path: &Path,
    payload: &ReplayGainCacheFile,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = encode_replaygain_cache(payload)
        .map_err(|e| std::io::Error::other(format!("encode failed: {e}")))?;
    let tmp = path.with_extension("bin.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all().ok();
    }
    fs::rename(&tmp, path)
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

pub fn replaygain_source_mtime_unix_nanos(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(UNIX_EPOCH).ok()?;
    Some(
        dur.as_secs()
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::from(dur.subsec_nanos())),
    )
}

pub fn replaygain_cache_entry_for_path(path: &Path, info: ReplayGainInfo) -> ReplayGainCacheEntry {
    ReplayGainCacheEntry::new(
        replaygain_path_hash(path),
        replaygain_source_mtime_unix_nanos(path).unwrap_or(0),
        info,
    )
}

pub fn replaygain_cache_info_if_fresh(
    entry: ReplayGainCacheEntry,
    path: &Path,
) -> Option<ReplayGainInfo> {
    let current_mtime = replaygain_source_mtime_unix_nanos(path)?;
    if entry.mtime_unix_nanos == current_mtime {
        Some(entry.info())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime};

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

    fn unique_temp_file(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("deadsync-replaygain-{tag}-{pid}-{stamp}-{id}.tmp"))
    }

    #[test]
    fn cache_info_invalidates_on_mtime_change() {
        let path = unique_temp_file("mtime");
        fs::write(&path, b"alpha").expect("write file");
        let entry = replaygain_cache_entry_for_path(
            &path,
            ReplayGainInfo {
                lufs: -16.0,
                true_peak_linear: 0.9,
            },
        );

        assert!(replaygain_cache_info_if_fresh(entry, &path).is_some());

        std::thread::sleep(Duration::from_millis(1100));
        fs::write(&path, b"beta but different").expect("rewrite file");
        let new_mtime = replaygain_source_mtime_unix_nanos(&path).expect("new mtime");
        if new_mtime == entry.mtime_unix_nanos {
            let _ = fs::remove_file(&path);
            return;
        }
        assert!(replaygain_cache_info_if_fresh(entry, &path).is_none());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn cache_file_entry_map_roundtrip() {
        let file = ReplayGainCacheFile::from_entries(sample_entries());
        let map = file.clone().into_entry_map();

        assert_eq!(map.len(), 2);
        assert_eq!(map[&file.entries[0].path_hash], file.entries[0]);
        assert_eq!(map[&file.entries[1].path_hash], file.entries[1]);
    }
}
