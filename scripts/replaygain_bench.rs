//! Standalone benchmark for the ReplayGain analyzer.
//!
//! Walks a directory, finds supported audio files, runs the same loudness
//! analysis the in-game worker uses, and prints per-file timings plus a
//! summary. Use this to size prewarm strategies (neighbor-window, pack-open,
//! song-scan) against your actual library.
//!
//! Usage:
//!   cargo run --profile local --bin replaygain_bench -- <dir> [--limit N] [--write-cache]
//!
//! `--write-cache` routes through the public `prewarm_paths` + `flush_now`
//! API instead of calling the analyzer directly, exercising the disk-cache
//! write path end-to-end.

use deadsync::engine::audio::replaygain::{
    Priority, compute_loudness_public, flush_now, prewarm_paths,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: replaygain_bench <dir> [--limit N] [--write-cache]");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[0]);
    let limit: Option<usize> = args
        .windows(2)
        .find(|w| w[0] == "--limit")
        .and_then(|w| w[1].parse().ok());
    let write_cache = args.iter().any(|a| a == "--write-cache");

    let mut files: Vec<PathBuf> = Vec::new();
    walk(&root, &mut files);
    files.sort();
    if let Some(n) = limit {
        files.truncate(n);
    }

    if files.is_empty() {
        eprintln!("no audio files found under {}", root.display());
        std::process::exit(1);
    }

    if write_cache {
        println!(
            "Routing {} files through the disk cache (prewarm_paths + flush_now)",
            files.len()
        );
        let t = Instant::now();
        // Use Foreground priority so the worker pool serves these
        // immediately, not as background prewarm.
        prewarm_paths(files.iter().cloned(), Priority::Foreground);
        // Wait long enough for the worker pool (2 threads) to drain,
        // sized off the measured ~400 ms/song analyze cost with 100% buffer.
        let drain_ms = (files.len() as u64 * 400) + 2000;
        println!("Waiting up to {} ms for worker pool to drain...", drain_ms);
        std::thread::sleep(std::time::Duration::from_millis(drain_ms));
        flush_now();
        println!(
            "Wall time (incl. drain + flush): {:.1} s",
            t.elapsed().as_secs_f64()
        );
        return;
    }

    println!("Analyzing {} files under {}", files.len(), root.display());
    println!("{:>9} {:>7} {:>7}  {}", "time_ms", "LUFS", "peak", "path");

    let mut total_ms = 0.0f64;
    let mut ok = 0usize;
    let mut fail = 0usize;
    let mut min_ms = f64::INFINITY;
    let mut max_ms = 0.0f64;
    let mut samples_ms: Vec<f64> = Vec::with_capacity(files.len());

    let bench_start = Instant::now();
    for path in &files {
        let t = Instant::now();
        let result = compute_loudness_public(path);
        let elapsed_ms = t.elapsed().as_secs_f64() * 1000.0;
        total_ms += elapsed_ms;
        match result {
            Ok(info) => {
                ok += 1;
                min_ms = min_ms.min(elapsed_ms);
                max_ms = max_ms.max(elapsed_ms);
                samples_ms.push(elapsed_ms);
                let rel = path.strip_prefix(&root).unwrap_or(path);
                println!(
                    "{:>9.1} {:>7.2} {:>7.3}  {}",
                    elapsed_ms,
                    info.lufs,
                    info.true_peak_linear,
                    rel.display()
                );
            }
            Err(e) => {
                fail += 1;
                let rel = path.strip_prefix(&root).unwrap_or(path);
                println!("{:>9.1}    ERR    -    {}  ({e})", elapsed_ms, rel.display());
            }
        }
    }
    let wall_ms = bench_start.elapsed().as_secs_f64() * 1000.0;

    samples_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| {
        if samples_ms.is_empty() {
            0.0
        } else {
            let idx = ((p / 100.0) * (samples_ms.len() as f64 - 1.0)).round() as usize;
            samples_ms[idx.min(samples_ms.len() - 1)]
        }
    };
    let mean = if ok > 0 { total_ms / ok as f64 } else { 0.0 };

    println!();
    println!("--- summary ---");
    println!("files:        {}  ({} ok, {} failed)", files.len(), ok, fail);
    println!("wall time:    {:>9.1} ms", wall_ms);
    println!("sum analyze:  {:>9.1} ms", total_ms);
    if ok > 0 {
        println!("per file mean:{:>9.1} ms", mean);
        println!("           min:{:>8.1} ms", min_ms);
        println!("           p50:{:>8.1} ms", pct(50.0));
        println!("           p90:{:>8.1} ms", pct(90.0));
        println!("           p99:{:>8.1} ms", pct(99.0));
        println!("           max:{:>8.1} ms", max_ms);
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if is_audio(&path) {
            out.push(path);
        }
    }
}

fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("ogg" | "mp3" | "flac" | "wav" | "opus")
    )
}
