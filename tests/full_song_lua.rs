//! A custom harness accepts an archive filename as Cargo's test filter, and
//! writes progress directly without requiring --ignored or --nocapture.
#[path = "support/paths.rs"]
mod paths;
#[path = "support/song_lua_metrics.rs"]
mod metrics;

// Share the production comparators with the focused regression test target.
// Its #[test] functions are omitted when compiled without the libtest harness.
#[allow(dead_code, unused_imports)]
#[path = "song_lua_itgmania_semantic_parity.rs"]
mod semantics;

fn main() -> std::process::ExitCode {
    let metrics_path = std::env::var_os("DEADSYNC_SONG_LUA_METRICS");
    let measurement = metrics_path.as_ref().map(|_| metrics::Measurement::start());
    let result = semantics::whole_song_archives::run_cli(std::env::args().skip(1).collect());
    if let (Some(path), Some(measurement)) = (metrics_path, measurement) {
        std::fs::write(path, serde_json::to_vec_pretty(&measurement.finish()).unwrap())
            .expect("write opt-in allocation metrics");
    }
    result
}
