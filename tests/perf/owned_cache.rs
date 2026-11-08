use super::*;
use crate::owned_perf::compare;
use crate::perf::measure;
use std::hint::black_box;

mod before {
    use super::*;
    include!("owned_cache_original.rs");
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "deadsync-owned-cache-{}-{}",
            std::process::id(),
            TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn original(&self) -> PathBuf {
        self.0.join("original.json")
    }
    fn current(&self) -> PathBuf {
        self.0.join("current.json")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.original());
        let _ = fs::remove_file(self.current());
        let _ = fs::remove_dir(&self.0);
    }
}

fn payload(count: usize, samples: usize) -> CacheFile {
    let options = AnalysisOptions {
        revision: ANALYSIS_REVISION,
        fingerprint_ms: 10.0_f64.to_bits(),
        window_ms: 5.0_f64.to_bits(),
        step_ms: 1.0_f64.to_bits(),
        magic_offset_ms: 0.0_f64.to_bits(),
        kernel_target: 0,
        kernel_type: 0,
        full_spectrogram: false,
        confidence_percent: 80,
    };
    let mut entries = Vec::new();
    let mut plots = Vec::new();
    for id in 0..count {
        let path = PathBuf::from(format!("song-{id}.ssc"));
        entries.push(CacheEntry {
            simfile: SourceStamp {
                path: path.clone(),
                len: 12,
                modified_ns: 34,
                sha256: [id as u8; 32],
            },
            music: SourceStamp {
                path: PathBuf::from(format!("song-{id}.ogg")),
                len: 56,
                modified_ns: 78,
                sha256: [42; 32],
            },
            chart_ix: id,
            options,
            result: CachedResult {
                bias_ms: -3.0,
                confidence: 0.91,
                applied: false,
            },
        });
        plots.push(CachedPlotEntry {
            simfile_path: path,
            chart_ix: id,
            options,
            last_used_ns: (count - id) as u64,
            plot: CachedPlot {
                freq_rows: 1,
                digest_rows: 0,
                cols: samples,
                post_rows: 0,
                freq_domain: vec![std::f64::consts::PI; samples],
                beat_digest: Vec::new(),
                post_kernel: Vec::new(),
                times_ms: vec![std::f64::consts::PI; samples],
                convolution: vec![std::f64::consts::PI; samples],
                edge_discard: 1,
            },
        });
    }
    CacheFile {
        version: CACHE_VERSION,
        entries,
        plots,
        legacy_plot: None,
    }
}

#[test]
fn cache_writer_preserves_exact_bytes_eviction_order_and_estimates() {
    let scratch = Scratch::new();
    for (count, samples, retained) in [
        (0, 0, 0),
        (2, 256, 2),
        (1, 1_400_000, 0),
        (4, 350_000, 3),
        (8, 250_000, 4),
    ] {
        let input = payload(count, samples);
        assert!(input.plots.iter().all(|p| p.plot.is_complete()));
        assert!(input.plots.iter().map(|p| p.plot.byte_len()).sum::<usize>() <= MAX_PLOT_BYTES);
        before::write_cache_file(&scratch.original(), input.clone()).unwrap();
        write_cache_file(&scratch.current(), input).unwrap();
        let bytes = fs::read(scratch.current()).unwrap();
        assert_eq!(bytes, fs::read(scratch.original()).unwrap());
        assert!(bytes.len() as u64 <= MAX_CACHE_BYTES);
        let result: CacheFile = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.entries.len(), count);
        assert_eq!(result.plots.len(), retained);
        assert_eq!(
            result.plots.iter().map(|p| p.chart_ix).collect::<Vec<_>>(),
            (0..retained).collect::<Vec<_>>()
        );
        let (entries, plots) = load_entries(&scratch.current()).unwrap();
        assert_eq!(entries.len(), count);
        assert_eq!(plots.len(), retained);
        assert_eq!(fs::read_dir(&scratch.0).unwrap().count(), 2);
    }
}

#[test]
fn cache_retry_reuses_allocation_and_reduces_peak_memory() {
    let scratch = Scratch::new();
    let input = payload(4, 350_000);
    let (_, old) =
        measure(|| before::write_cache_file(&scratch.original(), input.clone()).unwrap());
    let (_, new) = measure(|| write_cache_file(&scratch.current(), input.clone()).unwrap());
    assert!(new.allocs < old.allocs, "{old:?} -> {new:?}");
    assert!(new.reallocs < old.reallocs, "{old:?} -> {new:?}");
    assert!(
        new.allocated_bytes < old.allocated_bytes,
        "{old:?} -> {new:?}"
    );
    assert!(
        new.peak_added_bytes < old.peak_added_bytes,
        "{old:?} -> {new:?}"
    );
}

#[test]
fn cached_analysis_is_taken_once_and_keeps_preparation_state() {
    let cached = CachedAnalysis {
        bias_ms: -2.0,
        confidence: 0.9,
        applied: false,
        plot: Some(CachedPlot::default()),
    };
    let entry = payload(1, 0).entries.pop().unwrap();
    let mut preparation = TargetPreparation {
        cached: Some(cached),
        prepared: Some(PreparedTarget {
            entry: entry.clone(),
        }),
    };
    let taken = preparation.take_cached_analysis().unwrap();
    assert_eq!(taken.bias_ms, -2.0);
    assert!(taken.plot.is_some());
    assert!(preparation.cached_analysis().is_none());
    assert!(preparation.take_cached_analysis().is_none());
    assert_eq!(preparation.into_prepared().unwrap().entry, entry);
}

#[test]
#[ignore = "paired release benchmark; writes generated cache files to TEMP"]
fn benchmark_owned_pipelines_cache() {
    let scratch = Scratch::new();
    let original = scratch.original();
    let current = scratch.current();
    for (label, count, samples) in [
        ("fits", 2, 256),
        ("drop-all", 1, 1_400_000),
        ("drop-one", 4, 350_000),
        ("drop-many", 8, 250_000),
    ] {
        let input = payload(count, samples);
        compare(
            &format!("cache/{label}"),
            || {
                before::write_cache_file(black_box(&original), black_box(input.clone())).unwrap();
            },
            || {
                write_cache_file(black_box(&current), black_box(input.clone())).unwrap();
            },
        );
    }
}
