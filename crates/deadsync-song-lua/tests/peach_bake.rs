//! Full-chart Baked measurements using the production library feature set.
//! Run with `cargo test -p deadsync-song-lua --test peach_bake -- --ignored --nocapture`.
//! Set `DEADSYNC_PEACH_FINGERPRINT=1` to print the captured-output fingerprints.

#[path = "../../../tests/support/allocations.rs"]
#[expect(
    dead_code,
    reason = "shared allocator probe provides additional assertions"
)]
mod allocations;
#[path = "support/peach_bake.rs"]
mod peach_bake;
#[path = "support/thread_cycles.rs"]
mod thread_cycles;

#[test]
#[ignore = "full Peach Summer Splash bake benchmark; run alone with --nocapture"]
fn benchmark_peach_bake() {
    peach_bake::run();
}
