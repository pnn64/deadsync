use crate::ReplayGainInfo;
use bincode::{Decode, Encode};
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::{Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;
use twox_hash::XxHash64;

const CACHE_MAGIC: u64 = 0x44535952_47414946; // "DSYRGAIF" - file cache.
/// Current on-disk cache layout. Version 2 added the per-entry
/// `content_hash`; version 1 files are still decoded and migrated (see
/// [`decode_replaygain_cache`]) so an upgrade never discards existing gains.
const CACHE_VERSION: u32 = 2;
const CACHE_VERSION_V1: u32 = 1;

/// Sentinel stored in `content_hash` when the hash is not yet known — i.e. an
/// entry migrated from a v1 cache file. Such entries are validated by mtime
/// only until the hash is backfilled on the first fresh check.
pub const CONTENT_HASH_UNKNOWN: u64 = 0;

#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq)]
pub struct ReplayGainCacheEntry {
    pub path_hash: u64,
    pub mtime_unix_nanos: u64,
    /// xxhash64 of the raw source-file bytes. Lets us tell "same audio, new
    /// timestamp" from a genuinely edited file, so a timestamp-only change
    /// doesn't force a re-analysis. `CONTENT_HASH_UNKNOWN` for entries migrated
    /// from v1.
    pub content_hash: u64,
    pub lufs: f32,
    pub true_peak_linear: f32,
}

/// Legacy v1 cache entry (no `content_hash`). Only used to decode and migrate
/// pre-existing cache files written by older builds.
#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq)]
struct ReplayGainCacheEntryV1 {
    path_hash: u64,
    mtime_unix_nanos: u64,
    lufs: f32,
    true_peak_linear: f32,
}

#[derive(Encode, Decode, Default, Clone, Debug, PartialEq)]
struct ReplayGainCacheFileV1 {
    entries: Vec<ReplayGainCacheEntryV1>,
}

impl From<ReplayGainCacheEntryV1> for ReplayGainCacheEntry {
    fn from(v1: ReplayGainCacheEntryV1) -> Self {
        Self {
            path_hash: v1.path_hash,
            mtime_unix_nanos: v1.mtime_unix_nanos,
            content_hash: CONTENT_HASH_UNKNOWN,
            lufs: v1.lufs,
            true_peak_linear: v1.true_peak_linear,
        }
    }
}

/// Result of checking a cache entry against the current file on disk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CacheFreshness {
    /// Entry is valid as-is (mtime matched and the content hash is known).
    Fresh(ReplayGainInfo),
    /// Entry's gain is still valid but its bookkeeping was updated — either the
    /// mtime moved while the content hash matched, or a migrated v1 entry had
    /// its hash backfilled. The caller should persist the returned entry.
    Refreshed(ReplayGainCacheEntry),
    /// File content changed (or is unreadable); the entry must be re-analyzed.
    Stale,
}

impl ReplayGainCacheEntry {
    #[inline]
    pub fn new(
        path_hash: u64,
        mtime_unix_nanos: u64,
        content_hash: u64,
        info: ReplayGainInfo,
    ) -> Self {
        Self {
            path_hash,
            mtime_unix_nanos,
            content_hash,
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
    let config = bincode::config::standard();
    match version {
        CACHE_VERSION => {
            let (payload, _) =
                bincode::decode_from_slice::<ReplayGainCacheFile, _>(&bytes[12..], config).ok()?;
            Some(payload)
        }
        CACHE_VERSION_V1 => {
            // Migrate legacy entries: keep their gains, mark the content hash
            // unknown so the first fresh check backfills it.
            let (legacy, _) =
                bincode::decode_from_slice::<ReplayGainCacheFileV1, _>(&bytes[12..], config).ok()?;
            Some(ReplayGainCacheFile {
                entries: legacy.entries.into_iter().map(Into::into).collect(),
            })
        }
        _ => None,
    }
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

/// xxhash64 of the raw bytes of the source file, streamed in chunks so large
/// songs are not buffered whole. `None` if the file can't be read. This is the
/// content fingerprint used to validate a cache entry when the mtime changed.
pub fn replaygain_content_hash(path: &Path) -> Option<u64> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = XxHash64::with_seed(0);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        hasher.write(&buf[..read]);
    }
    Some(hasher.finish())
}

pub fn replaygain_cache_entry_for_path(path: &Path, info: ReplayGainInfo) -> ReplayGainCacheEntry {
    ReplayGainCacheEntry::new(
        replaygain_path_hash(path),
        replaygain_source_mtime_unix_nanos(path).unwrap_or(0),
        replaygain_content_hash(path).unwrap_or(CONTENT_HASH_UNKNOWN),
        info,
    )
}

/// Validate a cache entry against the file currently on disk.
///
/// Fast path: when the stored mtime still matches, the entry is reused without
/// touching the file (unless its content hash is unknown, in which case it is
/// backfilled). When the mtime moved, the file is hashed and the gain is reused
/// if the content is unchanged, refreshing the stored mtime so the hash is only
/// computed once. A differing hash (or unreadable file) is reported [`Stale`].
///
/// [`Stale`]: CacheFreshness::Stale
pub fn replaygain_cache_check(entry: ReplayGainCacheEntry, path: &Path) -> CacheFreshness {
    let Some(current_mtime) = replaygain_source_mtime_unix_nanos(path) else {
        return CacheFreshness::Stale;
    };
    if entry.mtime_unix_nanos == current_mtime {
        if entry.content_hash != CONTENT_HASH_UNKNOWN {
            return CacheFreshness::Fresh(entry.info());
        }
        // Migrated v1 entry whose mtime still matches: backfill the content
        // hash so a later timestamp-only change can be validated.
        return match replaygain_content_hash(path) {
            Some(hash) => CacheFreshness::Refreshed(ReplayGainCacheEntry {
                content_hash: hash,
                ..entry
            }),
            None => CacheFreshness::Fresh(entry.info()),
        };
    }

    let Some(current_hash) = replaygain_content_hash(path) else {
        return CacheFreshness::Stale;
    };
    if entry.content_hash != CONTENT_HASH_UNKNOWN && entry.content_hash == current_hash {
        CacheFreshness::Refreshed(ReplayGainCacheEntry {
            mtime_unix_nanos: current_mtime,
            ..entry
        })
    } else {
        CacheFreshness::Stale
    }
}

/// Convenience wrapper over [`replaygain_cache_check`] that collapses the
/// freshness result to the gain info (discarding the "needs persist" signal).
pub fn replaygain_cache_info_if_fresh(
    entry: ReplayGainCacheEntry,
    path: &Path,
) -> Option<ReplayGainInfo> {
    match replaygain_cache_check(entry, path) {
        CacheFreshness::Fresh(info) => Some(info),
        CacheFreshness::Refreshed(refreshed) => Some(refreshed.info()),
        CacheFreshness::Stale => None,
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
                content_hash: 0xabcd_0000_0000_0001,
                lufs: -22.5,
                true_peak_linear: 0.83,
            },
            ReplayGainCacheEntry {
                path_hash: 0xfeed_face_dead_beef,
                mtime_unix_nanos: 987_654_321_000,
                content_hash: 0xabcd_0000_0000_0002,
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
        let entry = ReplayGainCacheEntry::new(123, 456, 789, info);

        assert_eq!(entry.info(), info);
        assert_eq!(entry.path_hash, 123);
        assert_eq!(entry.mtime_unix_nanos, 456);
        assert_eq!(entry.content_hash, 789);
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

    #[test]
    fn cache_reused_when_content_unchanged_after_mtime_change() {
        let path = unique_temp_file("samecontent");
        fs::write(&path, b"loop-track-bytes").expect("write file");
        let info = ReplayGainInfo {
            lufs: -12.0,
            true_peak_linear: 0.7,
        };
        let entry = replaygain_cache_entry_for_path(&path, info);
        assert!(matches!(
            replaygain_cache_check(entry, &path),
            CacheFreshness::Fresh(_)
        ));

        std::thread::sleep(Duration::from_millis(1100));
        // Rewrite identical bytes so the content is unchanged but the mtime
        // moves.
        fs::write(&path, b"loop-track-bytes").expect("rewrite file");
        let new_mtime = replaygain_source_mtime_unix_nanos(&path).expect("new mtime");
        if new_mtime == entry.mtime_unix_nanos {
            let _ = fs::remove_file(&path);
            return;
        }

        match replaygain_cache_check(entry, &path) {
            CacheFreshness::Refreshed(updated) => {
                assert_eq!(updated.mtime_unix_nanos, new_mtime);
                assert_eq!(updated.content_hash, entry.content_hash);
                assert_eq!(updated.info(), info);
            }
            other => panic!("expected Refreshed, got {other:?}"),
        }
        assert!(replaygain_cache_info_if_fresh(entry, &path).is_some());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn v1_entry_backfills_content_hash_when_mtime_matches() {
        let path = unique_temp_file("backfill");
        fs::write(&path, b"abc").expect("write file");
        let mtime = replaygain_source_mtime_unix_nanos(&path).expect("mtime");
        let entry = ReplayGainCacheEntry {
            path_hash: replaygain_path_hash(&path),
            mtime_unix_nanos: mtime,
            content_hash: CONTENT_HASH_UNKNOWN,
            lufs: -10.0,
            true_peak_linear: 0.4,
        };

        match replaygain_cache_check(entry, &path) {
            CacheFreshness::Refreshed(updated) => {
                assert_ne!(updated.content_hash, CONTENT_HASH_UNKNOWN);
                assert_eq!(
                    updated.content_hash,
                    replaygain_content_hash(&path).expect("hash")
                );
            }
            other => panic!("expected Refreshed, got {other:?}"),
        }

        let _ = fs::remove_file(&path);
    }

    fn encode_v1_cache(file: &ReplayGainCacheFileV1) -> Vec<u8> {
        let body =
            bincode::encode_to_vec(file, bincode::config::standard()).expect("encode v1 body");
        let mut out = Vec::with_capacity(12 + body.len());
        out.extend_from_slice(&CACHE_MAGIC.to_le_bytes());
        out.extend_from_slice(&CACHE_VERSION_V1.to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn decode_migrates_v1_entries() {
        let v1 = ReplayGainCacheFileV1 {
            entries: vec![ReplayGainCacheEntryV1 {
                path_hash: 7,
                mtime_unix_nanos: 11,
                lufs: -14.0,
                true_peak_linear: 0.5,
            }],
        };
        let bytes = encode_v1_cache(&v1);

        let decoded = decode_replaygain_cache(&bytes).expect("decode v1");
        assert_eq!(decoded.entries.len(), 1);
        let entry = decoded.entries[0];
        assert_eq!(entry.path_hash, 7);
        assert_eq!(entry.mtime_unix_nanos, 11);
        assert_eq!(entry.content_hash, CONTENT_HASH_UNKNOWN);
        assert_eq!(entry.lufs, -14.0);
        assert_eq!(entry.true_peak_linear, 0.5);
    }
}
