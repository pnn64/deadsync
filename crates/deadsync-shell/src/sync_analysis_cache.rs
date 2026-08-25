use null_or_die::{BiasCfg, BiasKernel, KernelTarget};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

const CACHE_VERSION: u32 = 1;
/// Bump whenever a null-or-die update can change bias estimates without an
/// accompanying input or option change.
const ANALYSIS_REVISION: u32 = 1;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 100_000;
static TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct SourceStamp {
    path: PathBuf,
    len: u64,
    modified_ns: u64,
    sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct AnalysisOptions {
    revision: u32,
    fingerprint_ms: u64,
    window_ms: u64,
    step_ms: u64,
    magic_offset_ms: u64,
    kernel_target: u8,
    kernel_type: u8,
    full_spectrogram: bool,
    confidence_percent: u8,
}

impl AnalysisOptions {
    pub(crate) fn new(cfg: &BiasCfg, confidence_percent: u8) -> Self {
        Self {
            revision: ANALYSIS_REVISION,
            fingerprint_ms: cfg.fingerprint_ms.to_bits(),
            window_ms: cfg.window_ms.to_bits(),
            step_ms: cfg.step_ms.to_bits(),
            magic_offset_ms: cfg.magic_offset_ms.to_bits(),
            kernel_target: match cfg.kernel_target {
                KernelTarget::Digest => 0,
                KernelTarget::Accumulator => 1,
            },
            kernel_type: match cfg.kernel_type {
                BiasKernel::Rising => 0,
                BiasKernel::Loudest => 1,
            },
            full_spectrogram: cfg._full_spectrogram,
            confidence_percent,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
struct CachedResult {
    bias_ms: f64,
    confidence: f64,
    applied: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct CachedCurve {
    pub(crate) times_ms: Vec<f64>,
    pub(crate) convolution: Vec<f64>,
    pub(crate) edge_discard: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct CachedCurveEntry {
    simfile_path: PathBuf,
    chart_ix: usize,
    options: AnalysisOptions,
    curve: CachedCurve,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedAnalysis {
    pub(crate) bias_ms: f64,
    pub(crate) confidence: f64,
    pub(crate) applied: bool,
    pub(crate) curve: Option<CachedCurve>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct CacheEntry {
    simfile: SourceStamp,
    music: SourceStamp,
    chart_ix: usize,
    options: AnalysisOptions,
    result: CachedResult,
}

#[derive(Debug, Deserialize, Serialize)]
struct CacheFile {
    version: u32,
    entries: Vec<CacheEntry>,
    #[serde(default)]
    curve: Option<CachedCurveEntry>,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<PathBuf, CacheEntry>,
    curve: Option<CachedCurveEntry>,
    generation: u64,
    persisted_generation: u64,
    warned_full: bool,
}

/// Persistent sync-analysis cache.
///
/// Owner: the shell's sync-analysis service. Worker threads share it through
/// this mutex; no gameplay path reads it. Its application-session state warms
/// from one bounded disk file when the service is constructed. A cache miss
/// hashes the simfile and resolved audio before null-or-die analysis. Entries
/// are never evicted during a session: insertion saturates at 100,000 entries,
/// and corrupt, oversized, or version-mismatched files start empty. Completed
/// analyses and confirmed offset saves flush a snapshot outside the state lock.
/// Destruction occurs with the service at shutdown. Logs expose loads,
/// saturation, and write failures; per-target worst-case work is two streamed
/// source hashes on a menu worker, never on a gameplay frame. Only the most
/// recently completed single-chart analysis retains its compact convolution
/// curve; all other entries retain just the estimate and validation stamps.
pub(crate) struct Cache {
    path: PathBuf,
    state: Mutex<CacheState>,
    flush_lock: Mutex<()>,
}

pub(crate) struct TargetPreparation {
    cached: Option<CachedAnalysis>,
    prepared: Option<PreparedTarget>,
}

impl TargetPreparation {
    pub(crate) const fn is_cached(&self) -> bool {
        self.cached.is_some()
    }

    pub(crate) fn cached_analysis(&self) -> Option<&CachedAnalysis> {
        self.cached.as_ref()
    }

    pub(crate) fn into_prepared(self) -> Option<PreparedTarget> {
        self.prepared
    }
}

pub(crate) struct PreparedTarget {
    entry: CacheEntry,
}

pub(crate) struct CompletedTarget {
    prepared: PreparedTarget,
    bias_ms: f64,
    confidence: f64,
    curve: Option<CachedCurve>,
}

impl CompletedTarget {
    pub(crate) fn new(prepared: PreparedTarget, bias_ms: f64, confidence: f64) -> Self {
        Self {
            prepared,
            bias_ms,
            confidence,
            curve: None,
        }
    }

    pub(crate) fn with_curve(
        prepared: PreparedTarget,
        bias_ms: f64,
        confidence: f64,
        curve: CachedCurve,
    ) -> Self {
        Self {
            prepared,
            bias_ms,
            confidence,
            curve: Some(curve),
        }
    }
}

impl Cache {
    pub(crate) fn load(path: PathBuf) -> Self {
        let (entries, curve) = load_entries(&path).unwrap_or_default();
        if !entries.is_empty() {
            log::info!("Loaded {} null-or-die sync cache entries.", entries.len());
        }
        Self {
            path,
            state: Mutex::new(CacheState {
                entries,
                curve,
                ..CacheState::default()
            }),
            flush_lock: Mutex::new(()),
        }
    }

    pub(crate) fn prepare(
        &self,
        simfile_path: &Path,
        music_path: &Path,
        chart_ix: usize,
        options: AnalysisOptions,
    ) -> TargetPreparation {
        let Ok(simfile_path) = canonical_path(simfile_path) else {
            return analyze_target(None);
        };
        let Ok(music_path) = canonical_path(music_path) else {
            return analyze_target(None);
        };
        let cached = self.state.lock().ok().and_then(|state| {
            state.entries.get(&simfile_path).cloned().map(|entry| {
                let curve = state
                    .curve
                    .as_ref()
                    .filter(|curve| {
                        curve.simfile_path == simfile_path
                            && curve.chart_ix == chart_ix
                            && curve.options == options
                    })
                    .map(|curve| curve.curve.clone());
                (entry, curve)
            })
        });

        if let Some((mut entry, curve)) = cached
            && entry.chart_ix == chart_ix
            && entry.options == options
            && entry.music.path == music_path
        {
            // Simfiles are small and define chart timing, so always hash them.
            // Audio uses metadata as its steady-state fast path and hashes only
            // after a timestamp or length change.
            let simfile = refresh_stamp(&entry.simfile, true);
            let music = refresh_stamp(&entry.music, false);
            if let (Some(simfile), Some(music)) = (simfile, music) {
                let changed = simfile != entry.simfile || music != entry.music;
                entry.simfile = simfile;
                entry.music = music;
                let cached = CachedAnalysis {
                    bias_ms: entry.result.bias_ms,
                    confidence: entry.result.confidence,
                    applied: entry.result.applied,
                    curve,
                };
                if changed {
                    self.replace_entry(simfile_path, entry);
                }
                return TargetPreparation {
                    cached: Some(cached),
                    prepared: None,
                };
            }
        }

        let prepared = SourceStamp::capture(&simfile_path)
            .and_then(|simfile| {
                SourceStamp::capture(&music_path).map(|music| CacheEntry {
                    simfile,
                    music,
                    chart_ix,
                    options,
                    result: CachedResult {
                        bias_ms: 0.0,
                        confidence: 0.0,
                        applied: false,
                    },
                })
            })
            .map(|entry| PreparedTarget { entry });
        analyze_target(prepared)
    }

    pub(crate) fn record_completed(&self, completed: Vec<CompletedTarget>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        for completed in completed {
            let CompletedTarget {
                mut prepared,
                bias_ms,
                confidence,
                curve,
            } = completed;
            if !bias_ms.is_finite()
                || !confidence.is_finite()
                || !prepared.entry.sources_unchanged()
            {
                continue;
            }
            prepared.entry.result = CachedResult {
                bias_ms,
                confidence,
                applied: false,
            };
            let path = prepared.entry.simfile.path.clone();
            let chart_ix = prepared.entry.chart_ix;
            let options = prepared.entry.options;
            insert_entry(&mut state, prepared.entry);
            if let Some(curve) = curve {
                state.curve = Some(CachedCurveEntry {
                    simfile_path: path,
                    chart_ix,
                    options,
                    curve,
                });
                mark_changed(&mut state);
            } else if state
                .curve
                .as_ref()
                .is_some_and(|curve| curve.simfile_path == path)
            {
                state.curve = None;
                mark_changed(&mut state);
            }
        }
    }

    pub(crate) fn refresh_applied<'a>(&self, changes: impl IntoIterator<Item = (&'a Path, f32)>) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        for (path, delta_seconds) in changes {
            let Ok(path) = canonical_path(path) else {
                continue;
            };
            let Some(mut entry) = state.entries.get(&path).cloned() else {
                continue;
            };
            if (quantized_delta(entry.result.bias_ms) - delta_seconds).abs() > 0.000_1 {
                state.entries.remove(&path);
                clear_curve(&mut state, &path);
                mark_changed(&mut state);
                continue;
            }
            let Some(stamp) = SourceStamp::capture(&path) else {
                state.entries.remove(&path);
                clear_curve(&mut state, &path);
                mark_changed(&mut state);
                continue;
            };
            entry.simfile = stamp;
            entry.result.applied = true;
            state.entries.insert(path.clone(), entry);
            clear_curve(&mut state, &path);
            mark_changed(&mut state);
        }
    }

    pub(crate) fn flush(&self) {
        let Ok(_flush) = self.flush_lock.lock() else {
            return;
        };
        let (generation, entries, curve) = {
            let Ok(state) = self.state.lock() else {
                return;
            };
            if state.generation == state.persisted_generation {
                return;
            }
            let mut entries = state.entries.values().cloned().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.simfile.path.cmp(&b.simfile.path));
            (state.generation, entries, state.curve.clone())
        };
        let payload = CacheFile {
            version: CACHE_VERSION,
            entries,
            curve,
        };
        if let Err(error) = write_cache_file(&self.path, &payload) {
            log::warn!(
                "Failed to write null-or-die sync cache '{}': {error}",
                self.path.display()
            );
            return;
        }
        if let Ok(mut state) = self.state.lock()
            && state.generation == generation
        {
            state.persisted_generation = generation;
        }
    }

    fn replace_entry(&self, path: PathBuf, entry: CacheEntry) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.entries.insert(path, entry);
        mark_changed(&mut state);
    }
}

impl CacheEntry {
    fn sources_unchanged(&self) -> bool {
        metadata_matches(&self.simfile) && metadata_matches(&self.music)
    }
}

impl SourceStamp {
    fn capture(path: &Path) -> Option<Self> {
        let path = canonical_path(path).ok()?;
        let (len, modified_ns) = source_metadata(&path)?;
        Some(Self {
            sha256: file_sha256(&path)?,
            path,
            len,
            modified_ns,
        })
    }
}

fn insert_entry(state: &mut CacheState, entry: CacheEntry) {
    let path = entry.simfile.path.clone();
    if !state.entries.contains_key(&path) && state.entries.len() >= MAX_CACHE_ENTRIES {
        if !state.warned_full {
            log::warn!(
                "Null-or-die sync cache reached its {}-entry limit; new results will not be cached.",
                MAX_CACHE_ENTRIES
            );
            state.warned_full = true;
        }
        return;
    }
    state.entries.insert(path, entry);
    mark_changed(state);
}

const fn analyze_target(prepared: Option<PreparedTarget>) -> TargetPreparation {
    TargetPreparation {
        cached: None,
        prepared,
    }
}

fn clear_curve(state: &mut CacheState, path: &Path) {
    if state
        .curve
        .as_ref()
        .is_some_and(|curve| curve.simfile_path == path)
    {
        state.curve = None;
    }
}

fn mark_changed(state: &mut CacheState) {
    state.generation = state.generation.wrapping_add(1);
}

fn refresh_stamp(stored: &SourceStamp, always_hash: bool) -> Option<SourceStamp> {
    let (len, modified_ns) = source_metadata(&stored.path)?;
    if !always_hash && len == stored.len && modified_ns == stored.modified_ns {
        return Some(stored.clone());
    }
    let sha256 = file_sha256(&stored.path)?;
    (sha256 == stored.sha256).then(|| SourceStamp {
        path: stored.path.clone(),
        len,
        modified_ns,
        sha256,
    })
}

fn metadata_matches(stamp: &SourceStamp) -> bool {
    source_metadata(&stamp.path)
        .is_some_and(|(len, modified_ns)| len == stamp.len && modified_ns == stamp.modified_ns)
}

fn source_metadata(path: &Path) -> Option<(u64, u64)> {
    let metadata = fs::metadata(path).ok()?;
    let duration = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    let modified_ns = duration
        .as_secs()
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::from(duration.subsec_nanos()));
    Some((metadata.len(), modified_ns))
}

fn canonical_path(path: &Path) -> std::io::Result<PathBuf> {
    fs::canonicalize(path)
}

fn file_sha256(path: &Path) -> Option<[u8; 32]> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hasher.finalize().into())
}

fn quantized_delta(bias_ms: f64) -> f32 {
    let delta = -(bias_ms as f32) * 0.001;
    (delta / 0.001).round() * 0.001
}

fn load_entries(path: &Path) -> Option<(HashMap<PathBuf, CacheEntry>, Option<CachedCurveEntry>)> {
    if fs::metadata(path).ok()?.len() > MAX_CACHE_BYTES {
        log::warn!(
            "Ignoring oversized null-or-die sync cache '{}'.",
            path.display()
        );
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if file.version != CACHE_VERSION || file.entries.len() > MAX_CACHE_ENTRIES {
        return None;
    }
    let mut entries = HashMap::with_capacity(file.entries.len());
    for entry in file.entries {
        entries.insert(entry.simfile.path.clone(), entry);
    }
    Some((entries, file.curve))
}

fn write_cache_file(path: &Path, payload: &CacheFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cache path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
    let id = TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".null-or-die-sync-{}-{id}.tmp", std::process::id()));
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    if fs::rename(&temp, path).is_err() {
        let _ = fs::remove_file(path);
        if let Err(error) = fs::rename(&temp, path) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "deadsync-sync-cache-{tag}-{}-{id}",
            std::process::id()
        ))
    }

    fn options() -> AnalysisOptions {
        AnalysisOptions {
            revision: ANALYSIS_REVISION,
            fingerprint_ms: 10.0_f64.to_bits(),
            window_ms: 5.0_f64.to_bits(),
            step_ms: 1.0_f64.to_bits(),
            magic_offset_ms: 0.0_f64.to_bits(),
            kernel_target: 0,
            kernel_type: 0,
            full_spectrogram: false,
            confidence_percent: 80,
        }
    }

    fn complete(cache: &Cache, simfile: &Path, music: &Path) {
        let prepared = cache.prepare(simfile, music, 2, options()).into_prepared();
        let Some(prepared) = prepared else {
            panic!("uncached target should be prepared");
        };
        cache.record_completed(vec![CompletedTarget::new(prepared, -3.0, 0.91)]);
    }

    fn complete_with_curve(cache: &Cache, simfile: &Path, music: &Path) {
        let prepared = cache.prepare(simfile, music, 2, options()).into_prepared();
        let Some(prepared) = prepared else {
            panic!("uncached target should be prepared");
        };
        cache.record_completed(vec![CompletedTarget::with_curve(
            prepared,
            -3.0,
            0.91,
            CachedCurve {
                times_ms: vec![-1.0, 0.0, 1.0],
                convolution: vec![0.1, 0.8, 0.2],
                edge_discard: 1,
            },
        )]);
    }

    #[test]
    fn completed_target_round_trips_as_cache_hit() {
        let root = temp_dir("roundtrip");
        fs::create_dir_all(&root).expect("create temp dir");
        let simfile = root.join("song.ssc");
        let music = root.join("song.ogg");
        let cache_path = root.join("cache.json");
        fs::write(&simfile, b"#OFFSET:0.000;").expect("write simfile");
        fs::write(&music, b"audio").expect("write music");

        let cache = Cache::load(cache_path.clone());
        complete(&cache, &simfile, &music);
        cache.flush();

        let loaded = Cache::load(cache_path);
        assert!(loaded.prepare(&simfile, &music, 2, options()).is_cached());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn content_and_option_changes_invalidate_entry() {
        let root = temp_dir("invalidate");
        fs::create_dir_all(&root).expect("create temp dir");
        let simfile = root.join("song.ssc");
        let music = root.join("song.ogg");
        fs::write(&simfile, b"#OFFSET:0.000;").expect("write simfile");
        fs::write(&music, b"audio").expect("write music");
        let cache = Cache::load(root.join("cache.json"));
        complete(&cache, &simfile, &music);

        let mut changed_options = options();
        changed_options.confidence_percent = 90;
        assert!(
            !cache
                .prepare(&simfile, &music, 2, changed_options)
                .is_cached()
        );

        fs::write(&simfile, b"#OFFSET:0.125; changed").expect("change simfile");
        assert!(!cache.prepare(&simfile, &music, 2, options()).is_cached());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn single_chart_result_round_trips_with_compact_curve() {
        let root = temp_dir("single-chart");
        fs::create_dir_all(&root).expect("create temp dir");
        let simfile = root.join("song.ssc");
        let music = root.join("song.ogg");
        let cache_path = root.join("cache.json");
        fs::write(&simfile, b"#OFFSET:0.000;").expect("write simfile");
        fs::write(&music, b"audio").expect("write music");

        let cache = Cache::load(cache_path.clone());
        complete_with_curve(&cache, &simfile, &music);
        cache.flush();

        let loaded = Cache::load(cache_path);
        let prepared = loaded.prepare(&simfile, &music, 2, options());
        let cached = prepared
            .cached_analysis()
            .expect("completed single-chart result should be cached");
        assert_eq!(cached.bias_ms, -3.0);
        assert_eq!(cached.confidence, 0.91);
        assert!(!cached.applied);
        assert_eq!(
            cached.curve.as_ref().map(|curve| curve.edge_discard),
            Some(1)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn applied_offset_refreshes_self_authored_simfile_change() {
        let root = temp_dir("applied");
        fs::create_dir_all(&root).expect("create temp dir");
        let simfile = root.join("song.ssc");
        let music = root.join("song.ogg");
        fs::write(&simfile, b"#OFFSET:0.000;").expect("write simfile");
        fs::write(&music, b"audio").expect("write music");
        let cache_path = root.join("cache.json");
        let cache = Cache::load(cache_path.clone());
        complete(&cache, &simfile, &music);

        fs::write(&simfile, b"#OFFSET:0.003;").expect("apply offset");
        cache.refresh_applied([(simfile.as_path(), 0.003)]);
        cache.flush();

        let cache = Cache::load(cache_path);
        let prepared = cache.prepare(&simfile, &music, 2, options());
        assert!(prepared.is_cached());
        assert!(
            prepared
                .cached_analysis()
                .is_some_and(|cached| cached.applied)
        );
        let _ = fs::remove_dir_all(root);
    }
}
