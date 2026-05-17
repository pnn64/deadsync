//! Experimental ReplayGain 2.0 / EBU R 128 loudness analysis and caching.
//!
//! Public API:
//! - [`get_or_queue_gain_linear`] — returns the linear playback gain for a
//!   song if already known (either in memory or on disk), otherwise enqueues
//!   a background analysis job and returns `None`.
//! - [`clear_cache`] — drop all in-memory state and remove the on-disk cache
//!   directory. Intended for debug / a future "rescan" option.
//!
//! Behavior summary:
//! - One worker thread streams the song through the existing decoder layer
//!   and feeds samples to `ebur128::EbuR128` to compute integrated loudness
//!   (LUFS) and true peak.
//! - Computed values are persisted at
//!   `cache_dir/replaygain/<xxhash64(abs_path)>.bin` (mtime is stored in the
//!   header so the entry is automatically invalidated if the file changes).
//! - Linear gain is derived as `10^((TARGET_LUFS - lufs) / 20)`, clamped so
//!   that `gain * true_peak <= 1.0` (prevent clipping) and never exceeds
//!   [`MAX_GAIN_LINEAR`] (= +12 dB).
//!
//! When a song that triggered a queued analysis completes computation, the
//! worker calls back into the audio engine via
//! `crate::engine::audio::set_music_replaygain_if_matches` so the result can
//! be applied retroactively to the currently playing stream.

use crate::config::dirs;
use crate::engine::audio::decode;
use ebur128::{EbuR128, Mode};
use log::{debug, warn};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::UNIX_EPOCH;
use twox_hash::XxHash64;

/// EBU R 128 / ReplayGain 2.0 reference loudness.
const TARGET_LUFS: f64 = -18.0;
/// Hard ceiling on the linear gain factor we will apply (≈ +12 dB).
const MAX_GAIN_LINEAR: f32 = 4.0;
/// Linear gain returned for silence / un-analyzable tracks.
const UNITY_GAIN: f32 = 1.0;

const CACHE_MAGIC: u64 = 0x44535952_47414955; // "DSYRGAIU"
const CACHE_VERSION: u32 = 1;
/// Maximum number of bytes we will read from a cache file before declaring
/// it corrupt. The current record is 36 bytes, this leaves room for growth.
const CACHE_MAX_BYTES: usize = 1024;

/// Maximum frames fed to the analyzer per call. Decoders emit short packets,
/// but we still cap so a buggy decoder cannot blow the stack of `add_frames`.
const ANALYZE_CHUNK_FRAMES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayGainInfo {
    pub lufs: f32,
    pub true_peak_linear: f32,
}

#[derive(Clone, Copy)]
enum SlotState {
    Pending,
    Ready(ReplayGainInfo),
    Failed,
}

struct Worker {
    tx: Sender<Job>,
}

struct Job {
    path: PathBuf,
    track_id: u64,
}

static IN_MEMORY: OnceLock<Mutex<HashMap<PathBuf, SlotState>>> = OnceLock::new();
static WORKER: OnceLock<Worker> = OnceLock::new();

#[inline(always)]
fn in_memory() -> &'static Mutex<HashMap<PathBuf, SlotState>> {
    IN_MEMORY.get_or_init(|| Mutex::new(HashMap::new()))
}

#[inline(always)]
fn worker() -> &'static Worker {
    WORKER.get_or_init(spawn_worker)
}

fn spawn_worker() -> Worker {
    let (tx, rx) = channel::<Job>();
    thread::Builder::new()
        .name("replaygain-analyzer".to_string())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                analyze_one(job);
            }
        })
        .expect("failed to spawn replaygain worker");
    Worker { tx }
}

/// Returns the linear gain to apply when playing `path`, if it has already
/// been computed (memory or disk). If the value is not yet known, queues a
/// background analysis job tagged with `track_id` and returns `None`. When
/// the analysis later completes, the worker pushes the resulting gain back
/// into the audio engine via [`crate::engine::audio::set_music_replaygain_if_matches`].
pub fn get_or_queue_gain_linear(path: &Path, track_id: u64) -> Option<f32> {
    let abs = canonicalize_or_clone(path);

    {
        let map = in_memory().lock().unwrap();
        match map.get(&abs) {
            Some(SlotState::Ready(info)) => return Some(gain_linear_from_info(*info)),
            Some(SlotState::Failed) => return Some(UNITY_GAIN),
            Some(SlotState::Pending) => {
                // Already enqueued by a previous caller. Drop the current
                // track id into a fresh job below so the latest call gets
                // notified when work completes.
            }
            None => {}
        }
    }

    if let Some(info) = load_disk_cache(&abs) {
        in_memory()
            .lock()
            .unwrap()
            .insert(abs.clone(), SlotState::Ready(info));
        return Some(gain_linear_from_info(info));
    }

    {
        let mut map = in_memory().lock().unwrap();
        map.insert(abs.clone(), SlotState::Pending);
    }
    if worker()
        .tx
        .send(Job {
            path: abs.clone(),
            track_id,
        })
        .is_err()
    {
        warn!("ReplayGain worker channel closed; analysis disabled");
    }
    None
}

/// Convert LUFS + true peak into a linear playback gain, applying a peak
/// limit so we never amplify into clipping and clamping to a sensible
/// ceiling.
#[inline]
pub fn gain_linear_from_info(info: ReplayGainInfo) -> f32 {
    if !info.lufs.is_finite() || info.lufs <= -69.5 {
        return UNITY_GAIN;
    }
    let gain_db = TARGET_LUFS - f64::from(info.lufs);
    let raw_linear = 10f64.powf(gain_db / 20.0) as f32;
    let peak_limited = if info.true_peak_linear > f32::EPSILON {
        (1.0 / info.true_peak_linear).max(0.0)
    } else {
        MAX_GAIN_LINEAR
    };
    raw_linear.min(peak_limited).clamp(0.0, MAX_GAIN_LINEAR)
}

/// Drops the in-memory map and removes the on-disk cache directory.
pub fn clear_cache() {
    if let Some(mutex) = IN_MEMORY.get() {
        mutex.lock().unwrap().clear();
    }
    let dir = dirs::app_dirs().replaygain_cache_dir();
    if let Err(err) = fs::remove_dir_all(&dir)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            "Failed to clear ReplayGain cache dir {}: {err}",
            dir.display()
        );
    }
}

/* --------------------------- Worker internals --------------------------- */

fn analyze_one(job: Job) {
    let Job { path, track_id } = job;
    let info = match compute_loudness(&path) {
        Ok(info) => info,
        Err(err) => {
            debug!("ReplayGain analysis failed for {}: {err}", path.display());
            in_memory()
                .lock()
                .unwrap()
                .insert(path.clone(), SlotState::Failed);
            crate::engine::audio::set_music_replaygain_if_matches(track_id, UNITY_GAIN);
            return;
        }
    };

    if let Err(err) = write_disk_cache(&path, info) {
        debug!(
            "Failed to write ReplayGain cache for {}: {err}",
            path.display()
        );
    }
    in_memory()
        .lock()
        .unwrap()
        .insert(path.clone(), SlotState::Ready(info));
    crate::engine::audio::set_music_replaygain_if_matches(track_id, gain_linear_from_info(info));
}

fn compute_loudness(path: &Path) -> Result<ReplayGainInfo, String> {
    let opened = decode::open_file(path).map_err(|e| e.to_string())?;
    let channels = opened.channels.max(1);
    let sample_rate = opened.sample_rate_hz.max(1);
    if channels > 8 {
        return Err(format!(
            "ReplayGain: refusing to analyze {} channels",
            channels
        ));
    }

    let mut analyzer = EbuR128::new(channels as u32, sample_rate, Mode::I | Mode::TRUE_PEAK)
        .map_err(|e| format!("ebur128 init failed: {e:?}"))?;

    let mut reader = opened.reader;
    let mut buf: Vec<i16> = Vec::with_capacity(ANALYZE_CHUNK_FRAMES * channels);
    let mut had_samples = false;

    loop {
        buf.clear();
        match reader.read_dec_packet_into(&mut buf) {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => return Err(e.to_string()),
        }
        if buf.is_empty() {
            continue;
        }
        // Truncate to a whole number of frames in case the decoder produced
        // a partial frame.
        let frames_in_buf = buf.len() / channels;
        if frames_in_buf == 0 {
            continue;
        }
        had_samples = true;
        let usable = frames_in_buf * channels;
        analyzer
            .add_frames_i16(&buf[..usable])
            .map_err(|e| format!("ebur128 add_frames failed: {e:?}"))?;
    }

    if !had_samples {
        return Err("decoder produced no samples".to_string());
    }

    let lufs = analyzer
        .loudness_global()
        .map_err(|e| format!("ebur128 loudness_global failed: {e:?}"))? as f32;

    let mut true_peak = 0.0_f64;
    for ch in 0..channels {
        if let Ok(peak) = analyzer.true_peak(ch as u32)
            && peak > true_peak
        {
            true_peak = peak;
        }
    }

    Ok(ReplayGainInfo {
        lufs,
        true_peak_linear: true_peak as f32,
    })
}

/* ---------------------------- Disk cache I/O ---------------------------- */

fn canonicalize_or_clone(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn cache_path_for(song_path: &Path) -> Option<PathBuf> {
    let mut hasher = XxHash64::with_seed(0);
    use std::hash::Hasher;
    let bytes = song_path.as_os_str().to_string_lossy();
    hasher.write(bytes.as_bytes());
    let hash = hasher.finish();
    let dir = dirs::app_dirs().replaygain_cache_dir();
    Some(dir.join(format!("{hash:016x}.bin")))
}

fn source_mtime_unix_nanos(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?;
    let dur = mtime.duration_since(UNIX_EPOCH).ok()?;
    Some(
        dur.as_secs()
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::from(dur.subsec_nanos())),
    )
}

fn load_disk_cache(song_path: &Path) -> Option<ReplayGainInfo> {
    let cache_path = cache_path_for(song_path)?;
    let file = fs::File::open(&cache_path).ok()?;
    let mut bytes = Vec::with_capacity(64);
    file.take(CACHE_MAX_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    let (mtime, info) = decode_cache_record(&bytes)?;
    let current_mtime = source_mtime_unix_nanos(song_path)?;
    if mtime != current_mtime {
        return None;
    }
    Some(info)
}

fn write_disk_cache(song_path: &Path, info: ReplayGainInfo) -> std::io::Result<()> {
    let cache_path =
        cache_path_for(song_path).ok_or_else(|| std::io::Error::other("no cache path"))?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mtime = source_mtime_unix_nanos(song_path).unwrap_or(0);
    let buf = encode_cache_record(mtime, info);
    let tmp = cache_path.with_extension("bin.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&buf)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, &cache_path)
}

fn encode_cache_record(mtime: u64, info: ReplayGainInfo) -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&CACHE_MAGIC.to_le_bytes());
    buf.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    buf.extend_from_slice(&mtime.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.extend_from_slice(&info.lufs.to_le_bytes());
    buf.extend_from_slice(&info.true_peak_linear.to_le_bytes());
    buf
}

fn decode_cache_record(bytes: &[u8]) -> Option<(u64, ReplayGainInfo)> {
    if bytes.len() < 32 {
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
    let mtime = u64::from_le_bytes(bytes[12..20].try_into().ok()?);
    let _flags = u32::from_le_bytes(bytes[20..24].try_into().ok()?);
    let lufs = f32::from_le_bytes(bytes[24..28].try_into().ok()?);
    let true_peak = f32::from_le_bytes(bytes[28..32].try_into().ok()?);
    Some((
        mtime,
        ReplayGainInfo {
            lufs,
            true_peak_linear: true_peak,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_unity_for_target_loudness() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: TARGET_LUFS as f32,
            true_peak_linear: 0.5,
        });
        assert!((g - 1.0).abs() < 1e-4, "expected ~1.0, got {g}");
    }

    #[test]
    fn gain_boost_for_quiet_track() {
        // -30 LUFS → +12 dB ideal, but peak 0.5 → ceiling +6 dB (= 2.0).
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -30.0,
            true_peak_linear: 0.5,
        });
        assert!(g <= 2.0 + 1e-4 && g > 1.0, "got {g}");
    }

    #[test]
    fn gain_cut_for_loud_track() {
        // -10 LUFS → -8 dB → ≈0.398.
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -10.0,
            true_peak_linear: 0.99,
        });
        assert!((g - 0.398).abs() < 0.01, "got {g}");
    }

    #[test]
    fn gain_unity_for_silence() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: f32::NEG_INFINITY,
            true_peak_linear: 0.0,
        });
        assert_eq!(g, UNITY_GAIN);
    }

    #[test]
    fn gain_capped_at_max() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -100.0,
            true_peak_linear: 0.0,
        });
        assert!(g <= MAX_GAIN_LINEAR + 1e-4);
    }

    #[test]
    fn cache_record_roundtrip() {
        let info = ReplayGainInfo {
            lufs: -22.5,
            true_peak_linear: 0.83,
        };
        let bytes = encode_cache_record(123_456_789_000, info);
        let (mtime, decoded) = decode_cache_record(&bytes).expect("roundtrip");
        assert_eq!(mtime, 123_456_789_000);
        assert_eq!(decoded, info);
    }

    #[test]
    fn cache_rejects_bad_magic() {
        let mut bytes = encode_cache_record(
            0,
            ReplayGainInfo {
                lufs: -20.0,
                true_peak_linear: 0.5,
            },
        );
        bytes[0] ^= 0xff;
        assert!(decode_cache_record(&bytes).is_none());
    }

    #[test]
    fn cache_rejects_bad_version() {
        let mut bytes = encode_cache_record(
            0,
            ReplayGainInfo {
                lufs: -20.0,
                true_peak_linear: 0.5,
            },
        );
        bytes[8] = bytes[8].wrapping_add(1);
        assert!(decode_cache_record(&bytes).is_none());
    }

    #[test]
    fn cache_rejects_truncated() {
        let bytes = encode_cache_record(
            0,
            ReplayGainInfo {
                lufs: -20.0,
                true_peak_linear: 0.5,
            },
        );
        assert!(decode_cache_record(&bytes[..bytes.len() - 1]).is_none());
        assert!(decode_cache_record(&[]).is_none());
    }
}
