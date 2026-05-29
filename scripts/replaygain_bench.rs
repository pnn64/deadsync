//! ReplayGain analyzer timing helper.
//!
//! Run with: `cargo run --bin replaygain_bench -- <audio-file>...`

use deadsync::engine::audio::replaygain::compute_loudness_public;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("Usage: cargo run --bin replaygain_bench -- <audio-file>...");
        return ExitCode::from(2);
    }

    let mut failed = false;
    for path in paths {
        let start = Instant::now();
        match compute_loudness_public(Path::new(&path)) {
            Ok(info) => println!(
                "{path}\tlufs={:.3}\ttrue_peak={:.6}\telapsed_ms={:.3}",
                info.lufs,
                info.true_peak_linear,
                start.elapsed().as_secs_f64() * 1000.0
            ),
            Err(err) => {
                eprintln!("{path}: {err}");
                failed = true;
            }
        }
    }

    ExitCode::from(u8::from(failed))
}
