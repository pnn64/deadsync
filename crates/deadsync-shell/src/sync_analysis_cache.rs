use null_or_die::{BiasCfg, BiasKernel, KernelTarget};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const CACHE_VERSION: u32 = 1;
/// Bump whenever a null-or-die update can change bias estimates without an
/// accompanying input or option change.
const ANALYSIS_REVISION: u32 = 1;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 100_000;
const MAX_CACHED_PLOTS: usize = 8;
const MAX_PLOT_BYTES: usize = 48 * 1024 * 1024;
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
    pub(crate) const fn new(cfg: &BiasCfg, confidence_percent: u8) -> Self {
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
pub(crate) struct CachedPlot {
    #[serde(default)]
    pub(crate) freq_rows: usize,
    #[serde(default)]
    pub(crate) digest_rows: usize,
    #[serde(default)]
    pub(crate) cols: usize,
    #[serde(default)]
    pub(crate) post_rows: usize,
    #[serde(default)]
    pub(crate) freq_domain: Vec<f64>,
    #[serde(default)]
    pub(crate) beat_digest: Vec<f64>,
    #[serde(default)]
    pub(crate) post_kernel: Vec<f64>,
    pub(crate) times_ms: Vec<f64>,
    pub(crate) convolution: Vec<f64>,
    pub(crate) edge_discard: usize,
}

impl CachedPlot {
    fn is_complete(&self) -> bool {
        self.cols > 0
            && self.times_ms.len() == self.cols
            && self.convolution.len() == self.cols
            && self.freq_rows.checked_mul(self.cols) == Some(self.freq_domain.len())
            && self.digest_rows.checked_mul(self.cols) == Some(self.beat_digest.len())
            && self.post_rows.checked_mul(self.cols) == Some(self.post_kernel.len())
    }

    fn byte_len(&self) -> usize {
        [
            self.freq_domain.len(),
            self.beat_digest.len(),
            self.post_kernel.len(),
            self.convolution.len(),
            self.times_ms.len(),
        ]
        .into_iter()
        .fold(0usize, |total, len| {
            total.saturating_add(len.saturating_mul(std::mem::size_of::<f64>()))
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct CachedPlotEntry {
    simfile_path: PathBuf,
    chart_ix: usize,
    options: AnalysisOptions,
    #[serde(default)]
    last_used_ns: u64,
    #[serde(alias = "curve")]
    plot: CachedPlot,
}

#[derive(Clone, Debug)]
pub(crate) struct CachedAnalysis {
    pub(crate) bias_ms: f64,
    pub(crate) confidence: f64,
    pub(crate) applied: bool,
    pub(crate) plot: Option<CachedPlot>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct CacheEntry {
    simfile: SourceStamp,
    music: SourceStamp,
    chart_ix: usize,
    options: AnalysisOptions,
    result: CachedResult,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CacheFile {
    version: u32,
    entries: Vec<CacheEntry>,
    #[serde(default)]
    plots: Vec<CachedPlotEntry>,
    #[serde(default, alias = "plot", alias = "curve", skip_serializing)]
    legacy_plot: Option<CachedPlotEntry>,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<PathBuf, CacheEntry>,
    plots: HashMap<PathBuf, CachedPlotEntry>,
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
/// recently used single-chart analyses retain up to eight complete visual
/// plots within a 48 MiB raw-data budget; all other entries retain just the
/// estimate and validation stamps. Least-recently-used plots are evicted on a
/// menu worker. The disk snapshot is capped at 64 MiB and drops oldest plots
/// before the estimate index rather than making the whole cache unreadable.
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

    pub(crate) const fn cached_analysis(&self) -> Option<&CachedAnalysis> {
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
    plot: Option<CachedPlot>,
}

impl CompletedTarget {
    pub(crate) const fn new(prepared: PreparedTarget, bias_ms: f64, confidence: f64) -> Self {
        Self {
            prepared,
            bias_ms,
            confidence,
            plot: None,
        }
    }

    pub(crate) const fn with_plot(
        prepared: PreparedTarget,
        bias_ms: f64,
        confidence: f64,
        plot: CachedPlot,
    ) -> Self {
        Self {
            prepared,
            bias_ms,
            confidence,
            plot: Some(plot),
        }
    }
}

impl Cache {
    pub(crate) fn load(path: PathBuf) -> Self {
        let (entries, plots) = load_entries(&path).unwrap_or_default();
        if !entries.is_empty() {
            log::info!("Loaded {} null-or-die sync cache entries.", entries.len());
        }
        Self {
            path,
            state: Mutex::new(CacheState {
                entries,
                plots,
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
        require_plot: bool,
    ) -> TargetPreparation {
        let Ok(simfile_path) = canonical_path(simfile_path) else {
            return analyze_target(None);
        };
        let Ok(music_path) = canonical_path(music_path) else {
            return analyze_target(None);
        };
        let cached = self.state.lock().ok().and_then(|state| {
            state.entries.get(&simfile_path).cloned().map(|entry| {
                let plot = state
                    .plots
                    .get(&simfile_path)
                    .filter(|plot| {
                        plot.simfile_path == simfile_path
                            && plot.chart_ix == chart_ix
                            && plot.options == options
                    })
                    .map(|plot| plot.plot.clone());
                (entry, plot)
            })
        });

        if let Some((mut entry, plot)) = cached
            && entry.chart_ix == chart_ix
            && entry.options == options
            && entry.music.path == music_path
            && (!require_plot || plot.as_ref().is_some_and(CachedPlot::is_complete))
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
                    plot,
                };
                if changed {
                    self.replace_entry(&simfile_path, entry);
                }
                if require_plot {
                    self.touch_plot(&simfile_path);
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
                plot,
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
            if !insert_entry(&mut state, prepared.entry) {
                continue;
            }
            if let Some(plot) = plot {
                state.plots.insert(
                    path.clone(),
                    CachedPlotEntry {
                        simfile_path: path,
                        chart_ix,
                        options,
                        last_used_ns: now_ns(),
                        plot,
                    },
                );
                trim_plots(&mut state.plots);
                mark_changed(&mut state);
            } else if state.plots.remove(&path).is_some() {
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
                clear_plot(&mut state, &path);
                mark_changed(&mut state);
                continue;
            }
            let Some(stamp) = SourceStamp::capture(&path) else {
                state.entries.remove(&path);
                clear_plot(&mut state, &path);
                mark_changed(&mut state);
                continue;
            };
            entry.simfile = stamp;
            entry.result.applied = true;
            state.entries.insert(path.clone(), entry);
            clear_plot(&mut state, &path);
            mark_changed(&mut state);
        }
    }

    pub(crate) fn flush(&self) {
        let Ok(_flush) = self.flush_lock.lock() else {
            return;
        };
        let (generation, entries, plots) = {
            let Ok(state) = self.state.lock() else {
                return;
            };
            if state.generation == state.persisted_generation {
                return;
            }
            let mut entries = state.entries.values().cloned().collect::<Vec<_>>();
            entries.sort_by(|a, b| a.simfile.path.cmp(&b.simfile.path));
            let mut plots = state.plots.values().cloned().collect::<Vec<_>>();
            plots.sort_by_key(|plot| std::cmp::Reverse(plot.last_used_ns));
            (state.generation, entries, plots)
        };
        let payload = CacheFile {
            version: CACHE_VERSION,
            entries,
            plots,
            legacy_plot: None,
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

    fn replace_entry(&self, path: &Path, entry: CacheEntry) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        state.entries.insert(path.to_path_buf(), entry);
        mark_changed(&mut state);
    }

    fn touch_plot(&self, path: &Path) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        let Some(plot) = state.plots.get_mut(path) else {
            return;
        };
        plot.last_used_ns = now_ns();
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

fn insert_entry(state: &mut CacheState, entry: CacheEntry) -> bool {
    let path = entry.simfile.path.clone();
    if !state.entries.contains_key(&path) && state.entries.len() >= MAX_CACHE_ENTRIES {
        if !state.warned_full {
            log::warn!(
                "Null-or-die sync cache reached its {}-entry limit; new results will not be cached.",
                MAX_CACHE_ENTRIES
            );
            state.warned_full = true;
        }
        return false;
    }
    state.entries.insert(path, entry);
    mark_changed(state);
    true
}

const fn analyze_target(prepared: Option<PreparedTarget>) -> TargetPreparation {
    TargetPreparation {
        cached: None,
        prepared,
    }
}

fn clear_plot(state: &mut CacheState, path: &Path) {
    state.plots.remove(path);
}

fn trim_plots(plots: &mut HashMap<PathBuf, CachedPlotEntry>) {
    while plots.len() > MAX_CACHED_PLOTS
        || plots
            .values()
            .map(|entry| entry.plot.byte_len())
            .fold(0usize, usize::saturating_add)
            > MAX_PLOT_BYTES
    {
        let Some(path) = plots
            .iter()
            .min_by(|(path_a, a), (path_b, b)| {
                (a.last_used_ns, path_a.as_path()).cmp(&(b.last_used_ns, path_b.as_path()))
            })
            .map(|(path, _)| path.clone())
        else {
            break;
        };
        plots.remove(&path);
    }
}

const fn mark_changed(state: &mut CacheState) {
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

fn load_entries(
    path: &Path,
) -> Option<(
    HashMap<PathBuf, CacheEntry>,
    HashMap<PathBuf, CachedPlotEntry>,
)> {
    if fs::metadata(path).ok()?.len() > MAX_CACHE_BYTES {
        log::warn!(
            "Ignoring oversized null-or-die sync cache '{}'.",
            path.display()
        );
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mut file: CacheFile = serde_json::from_slice(&bytes).ok()?;
    if file.version != CACHE_VERSION || file.entries.len() > MAX_CACHE_ENTRIES {
        return None;
    }
    let mut entries = HashMap::with_capacity(file.entries.len());
    for entry in file.entries {
        entries.insert(entry.simfile.path.clone(), entry);
    }
    if let Some(plot) = file.legacy_plot.take() {
        file.plots.push(plot);
    }
    let mut plots = HashMap::with_capacity(file.plots.len());
    for plot in file.plots {
        if entries.contains_key(&plot.simfile_path) {
            plots.insert(plot.simfile_path.clone(), plot);
        }
    }
    trim_plots(&mut plots);
    Some((entries, plots))
}

fn write_cache_file(path: &Path, payload: &CacheFile) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cache path has no parent directory".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut payload = payload.clone();
    let mut bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    while bytes.len() as u64 > MAX_CACHE_BYTES && !payload.plots.is_empty() {
        payload.plots.pop();
        log::warn!(
            "Null-or-die cached visuals exceeded {} MiB; dropping the oldest plot.",
            MAX_CACHE_BYTES / (1024 * 1024)
        );
        bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    }
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(format!(
            "serialized cache exceeds the {} MiB limit",
            MAX_CACHE_BYTES / (1024 * 1024)
        ));
    }
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

fn now_ns() -> u64 {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    duration
        .as_secs()
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::from(duration.subsec_nanos()))
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
        let prepared = cache
            .prepare(simfile, music, 2, options(), false)
            .into_prepared();
        let Some(prepared) = prepared else {
            panic!("uncached target should be prepared");
        };
        cache.record_completed(vec![CompletedTarget::new(prepared, -3.0, 0.91)]);
    }

    fn complete_with_plot(cache: &Cache, simfile: &Path, music: &Path) {
        let prepared = cache
            .prepare(simfile, music, 2, options(), true)
            .into_prepared();
        let Some(prepared) = prepared else {
            panic!("uncached target should be prepared");
        };
        cache.record_completed(vec![CompletedTarget::with_plot(
            prepared,
            -3.0,
            0.91,
            CachedPlot {
                freq_rows: 1,
                digest_rows: 1,
                cols: 3,
                post_rows: 1,
                freq_domain: vec![0.2, 0.4, 0.6],
                beat_digest: vec![0.3, 0.5, 0.7],
                post_kernel: vec![0.4, 0.6, 0.8],
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
        assert!(
            loaded
                .prepare(&simfile, &music, 2, options(), false)
                .is_cached()
        );
        let visual = loaded.prepare(&simfile, &music, 2, options(), true);
        assert!(!visual.is_cached());
        assert!(visual.into_prepared().is_some());
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
                .prepare(&simfile, &music, 2, changed_options, false)
                .is_cached()
        );

        fs::write(&simfile, b"#OFFSET:0.125; changed").expect("change simfile");
        assert!(
            !cache
                .prepare(&simfile, &music, 2, options(), false)
                .is_cached()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn single_chart_result_round_trips_with_complete_visuals() {
        let root = temp_dir("single-chart");
        fs::create_dir_all(&root).expect("create temp dir");
        let simfile = root.join("song.ssc");
        let music = root.join("song.ogg");
        let cache_path = root.join("cache.json");
        fs::write(&simfile, b"#OFFSET:0.000;").expect("write simfile");
        fs::write(&music, b"audio").expect("write music");

        let cache = Cache::load(cache_path.clone());
        complete_with_plot(&cache, &simfile, &music);
        cache.flush();

        let loaded = Cache::load(cache_path);
        let prepared = loaded.prepare(&simfile, &music, 2, options(), true);
        let cached = prepared
            .cached_analysis()
            .expect("completed single-chart result should be cached");
        assert_eq!(cached.bias_ms, -3.0);
        assert_eq!(cached.confidence, 0.91);
        assert!(!cached.applied);
        let plot = cached
            .plot
            .as_ref()
            .expect("cached visuals should be present");
        assert_eq!(plot.freq_domain, [0.2, 0.4, 0.6]);
        assert_eq!(plot.beat_digest, [0.3, 0.5, 0.7]);
        assert_eq!(plot.post_kernel, [0.4, 0.6, 0.8]);
        assert_eq!(plot.convolution, [0.1, 0.8, 0.2]);
        assert_eq!(plot.edge_discard, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn complete_visuals_are_retained_for_multiple_single_charts() {
        let root = temp_dir("multiple-visuals");
        fs::create_dir_all(&root).expect("create temp dir");
        let cache_path = root.join("cache.json");
        let simfile_a = root.join("a.ssc");
        let music_a = root.join("a.ogg");
        let simfile_b = root.join("b.ssc");
        let music_b = root.join("b.ogg");
        fs::write(&simfile_a, b"#OFFSET:0.000;").expect("write first simfile");
        fs::write(&music_a, b"audio a").expect("write first music");
        fs::write(&simfile_b, b"#OFFSET:0.000;").expect("write second simfile");
        fs::write(&music_b, b"audio b").expect("write second music");

        let cache = Cache::load(cache_path.clone());
        complete_with_plot(&cache, &simfile_a, &music_a);
        complete_with_plot(&cache, &simfile_b, &music_b);
        cache.flush();

        let loaded = Cache::load(cache_path);
        assert!(
            loaded
                .prepare(&simfile_a, &music_a, 2, options(), true)
                .is_cached()
        );
        assert!(
            loaded
                .prepare(&simfile_b, &music_b, 2, options(), true)
                .is_cached()
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
        let prepared = cache.prepare(&simfile, &music, 2, options(), false);
        assert!(prepared.is_cached());
        assert!(
            prepared
                .cached_analysis()
                .is_some_and(|cached| cached.applied)
        );
        let _ = fs::remove_dir_all(root);
    }
}
