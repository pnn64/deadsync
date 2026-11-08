//! Measures the same Lua preparation entry point used by the gameplay loader.
#[path = "../tests/support/paths.rs"]
mod paths;
#[path = "../tests/support/song_lua_metrics.rs"]
mod metrics;

use sha2::{Digest, Sha256};
use std::{fmt::Write as _, path::Path, sync::Arc};

struct Fingerprint(Sha256);
impl std::fmt::Write for Fingerprint {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.update(text.as_bytes());
        Ok(())
    }
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert!(args.len() >= 6 && args[0] == "--compile" && args[2] == "--report"
        && args[4] == "--iterations",
        "usage: song_lua_performance --compile SIMFILE --report JSON --iterations N [--renderer NAME] [--chart-index N]");
    let iterations = args[5].parse::<usize>().expect("positive iteration count");
    assert!(iterations > 0);
    let mut renderer = "null";
    let mut chart_index = 0;
    let options = args[6..].chunks_exact(2);
    assert!(options.remainder().is_empty(), "benchmark options require values");
    for option in options {
        match option[0].as_str() {
            "--renderer" => renderer = &option[1],
            "--chart-index" => chart_index = option[1].parse::<usize>().expect("chart index"),
            _ => panic!("unknown benchmark option {}", option[0]),
        }
    }
    paths::init();
    let simfile = Path::new(&args[1]).canonicalize().expect("benchmark simfile");
    let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0)
        .expect("benchmark song");
    let payload = Arc::new(deadsync_simfile::app_runtime::load_gameplay_charts(&song, &[chart_index], 0.0)
        .expect("benchmark notes").remove(0));
    let chart = Arc::new(song.charts[chart_index].clone());
    let charts = [chart.clone(), chart];
    let profiles = std::array::from_fn(|_| deadsync_profile::Profile::default());
    let scroll = profiles.each_ref().map(|p| p.scroll_speed);
    let session = deadsync_gameplay::GameplaySession::default();
    let config = deadsync_gameplay::GameplayConfig::default();
    let mut samples = Vec::with_capacity(iterations);
    let mut fingerprints = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let measurement = metrics::Measurement::start();
        let prepared = deadsync_shell::gameplay_entry::prepare_song_lua(
            &song, &charts, [&payload.timing; 2], &profiles, &scroll, 1.0,
            deadsync_gameplay::GameplayViewport::new(640.0, 480.0), (640, 480),
            4.0 / 3.0, 1.0, &session, &config, renderer, 0);
        samples.push(measurement.finish());
        let mut fingerprint = Fingerprint(Sha256::new());
        let mut count = 0;
        let mut live_scenes = 0;
        for compiled in prepared.compiled_layers() {
            live_scenes += usize::from(compiled.live_scene.is_some());
            write!(&mut fingerprint, "{compiled:?}").unwrap();
            count += 1;
        }
        assert!(count > 0, "benchmark requires prepared Lua layers");
        assert_eq!(live_scenes, 1, "production preparation owns one shared Lua VM");
        let mut hash = String::with_capacity(64);
        for byte in fingerprint.0.finalize() { write!(&mut hash, "{byte:02x}").unwrap(); }
        fingerprints.push(hash);
    }
    assert!(fingerprints.windows(2).all(|p| p[0] == p[1]), "compiler output changed between iterations");
    let report = serde_json::json!({"simfile":simfile,"iterations":iterations,
        "renderer":renderer,"chart_index":chart_index,
        "samples":samples,"compiled_sha256":fingerprints[0],
        "measurement":"production prepare_song_lua, including context/timing; parsing initial song/notes and hashing compiled output excluded"});
    std::fs::write(&args[3], serde_json::to_vec_pretty(&report).unwrap()).expect("write compiler metrics");
    println!("{report}");
}
