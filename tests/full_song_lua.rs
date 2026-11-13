//! A custom harness accepts an archive filename as Cargo's test filter, and
//! writes progress directly without requiring --ignored or --nocapture.
#[path = "support/paths.rs"]
mod paths;

// Share the production comparators with the focused regression test target.
// Its #[test] functions are omitted when compiled without the libtest harness.
#[allow(dead_code, unused_imports)]
#[path = "song_lua_itgmania_semantic_parity.rs"]
mod semantics;

fn main() -> std::process::ExitCode {
    semantics::whole_song_archives::run_cli(std::env::args().skip(1).collect())
}
