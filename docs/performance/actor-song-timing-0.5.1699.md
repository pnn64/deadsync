# Actor presence, timing ownership and easing performance 0.5.1699

Parent: `19f0502ca` (0.5.1698). Date: 2026-10-03.

This pass follows `rust-performance.md`'s guidance to remove redundant work,
reuse owned memory and measure hot paths before adding machinery:

1. Check visual actors directly instead of collecting every descendant index
   into a temporary vector. Nonvisual actors need no ancestor walk, and a
   matching visual descendant ends the search immediately.
2. Move the song summary's global timing `Arc` into the existing owning cache
   conversion. The previous clone guaranteed that `Arc::try_unwrap` failed,
   forcing copies of the timing vectors after chart construction had finished.
   Externally shared timing still uses the existing copying fallback.
3. Apply runtime overlay eases to the current state by mutable reference.
   The old helper received and returned the entire state by value. Removing
   that round trip preserves ease ordering, cutoffs, interpolation, sprite
   animation epochs and subsequent update/reapplication behavior.

No dependencies, unsafe code or public API changes were added.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores/44 logical processors),
Rust/Cargo 1.98.1, release opt-level 3 with full LTO. Builds and validation
finish before six serial benchmark rounds alternate original-first and
current-first. Each round reports seven-sample medians and ranges.
`QueryThreadCycleTime` measures the calling thread's CPU cycles. Other host
activity is not isolated.

The three original function bodies are frozen from the parent and verified
modulo whitespace and formatting. Both variants use the same unchanged
visual classification, interpolation and song-building helpers. They run in
the same release test binaries. A thread-local System allocator wrapper counts
heap traffic in a separate operation after timing.

The [raw CSV](actor-song-timing-0.5.1699.csv) records 156 measurements across
13 paired workloads: time, cycles, throughput, allocation/reallocation/free
calls, requested bytes and freed bytes. Allocation call counts and paired byte
savings are stable across rounds; song byte totals vary slightly with temporary
paths. Requested bytes measure allocation traffic rather than peak RSS. The
tables show medians of paired cycle reductions and
throughput ratios; elapsed times are separate six-round medians. Their ratios
can differ. Negative reductions indicate more cycles.

These synthetic cases measure the changed operations, not whole-song loading
time, frame latency or gameplay FPS.

## Visual presence

One operation queries a 512-actor tree. Flat actors all have the root as their
parent; nested actors form a chain. The visual actor is absent, near the root,
or at the final index. Lua tables and actor fixtures are built outside timing.
Throughput counts queries per second. Allocation calls include reallocations.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocation/growth calls/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `visual/flat/absent` | 3125.8 -> 524.0 | 84.0% (81.7..85.5) | 6.24x | 8 -> 0 | 8160 -> 0 |
| `visual/flat/early` | 1963.5 -> 4.6 | 99.7% (99.7..99.7) | 428.94x | 8 -> 0 | 8160 -> 0 |
| `visual/flat/late` | 3157.4 -> 477.9 | 85.0% (78.8..85.6) | 6.64x | 8 -> 0 | 8160 -> 0 |
| `visual/nested/absent` | 521440.9 -> 487.2 | 99.9% (99.9..99.9) | 1039.44x | 8 -> 0 | 8160 -> 0 |
| `visual/nested/early` | 516207.4 -> 4.3 | 99.999% (99.999..99.999) | 117361.97x | 8 -> 0 | 8160 -> 0 |
| `visual/nested/late` | 520744.5 -> 3792.1 | 99.3% (99.1..99.3) | 139.41x | 8 -> 0 | 8160 -> 0 |

All six cases remove one allocation, seven growth reallocations and one free,
including 8,160 requested/freed bytes per query. Flat absent/late queries use
84.0-85.0% fewer median cycles; nested absent/late queries use 99.3-99.9%
fewer. The early-match cases stop after inspecting the first visual child.

## Global timing ownership

One operation builds and destroys a complete serializable song with one chart,
a Lua foreground change and 32 or 4096 stops. RSSP analysis and fixture setup
are outside measurement. Filesystem asset discovery, song construction and
result destruction are included. Equal per-operation clock overhead is also
included. Throughput counts songs per second.

Inputs own buffers allocated during setup and consumed during construction,
so free counts can exceed fresh allocations. Both variants consume equivalent
inputs; the CSV records their complete allocation and destruction traffic.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocation/growth calls/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `song/timing=32` | 899445.3 -> 893387.5 | 1.2% (-5.0..3.4) | 1.01x | 77 -> 75 | 8523..8590 -> 8259..8326 |
| `song/timing=4096` | 1009040.7 -> 963993.8 | 4.1% (0.7..10.8) | 1.04x | 77 -> 75 | 73547..73614 -> 40771..40838 |

Both sizes remove exactly two allocations and two frees without changing
reallocations: 264 bytes with 32 stops and 32,776 bytes with 4096 stops.
Temporary path names differ between benchmark processes; absolute song byte
totals span 67 bytes, while paired savings are identical in every round. The
large fixture reduces median cycles by 4.1% and allocation traffic by about
44.5%. The 32-stop control has no consistent timing improvement: its cycle
change ranges from -5.0% to +3.4%, while its allocation saving is exact.

## In-place easing

One operation advances 1024 overlay states in an existing buffer. Workloads
cover a missing range, a dormant future ease, active sparse/dense deltas and a
completed ease. Identical input/output black-box boundaries prevent dead-code
elimination without forcing extra copies in either variant. Throughput counts
actors per second. Both implementations allocate nothing.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocation/growth calls/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `ease/missing` | 50103.5 -> 13064.7 | 73.5% (72.8..75.0) | 3.78x | 0 -> 0 | 0 -> 0 |
| `ease/dormant` | 50353.3 -> 17362.7 | 64.7% (62.5..65.9) | 2.84x | 0 -> 0 | 0 -> 0 |
| `ease/sparse` | 127375.6 -> 90312.7 | 29.1% (24.5..30.9) | 1.41x | 0 -> 0 | 0 -> 0 |
| `ease/dense` | 182173.8 -> 143217.0 | 20.7% (19.6..22.2) | 1.26x | 0 -> 0 | 0 -> 0 |
| `ease/completed` | 72173.9 -> 55419.1 | 23.0% (19.3..27.1) | 1.30x | 0 -> 0 | 0 -> 0 |

The overlay state is 536 bytes on this target. Removing its by-value round
trip reduces median cycles by 20.7-73.5% across all five cases, with positive
savings in every round. Active sparse/dense easing improves throughput by
1.41x/1.26x while retaining zero allocation traffic.

## Behavioral validation

- Library tests: 227 simfile and 771 song-Lua tests pass in debug and release.
- Playback integration: 164 tests pass in debug and release, including the new
  differential easing test. Playback runs in its existing asset-backed integration binary,
  since the library unit-test build excludes that module.
- New actor tests compare roots, branches, forward and out-of-range parents,
  deep trees and every visual position against the original. A 512-actor
  allocation assertion protects the zero-allocation path.
- New song tests compare complete bincode output for Lua/non-Lua songs,
  empty/nonempty timing and uniquely owned/externally shared sources. Retained
  shared data remains unchanged. The full builder has a reduced-churn assertion.
- New easing tests compare every debug-rendered state field, including NaNs,
  at start/end/sustain boundaries, zero/negative durations, cutoffs, missing and
  empty ranges, multiple eases and stretch rectangles.
- Root actor conformance: all 31 tests pass. Song-wrapper tests: 6 pass and
  `recurring_commands_use_song_easing_and_continuous_clock` fails its existing
  unsupported-perframe count assertion (1 versus 0). The same failure reproduces
  using the original cached dependency build from immediately after the parent.
- Architecture boundaries: 141 pass and 12 fail. All 12 reproduce against a
  source snapshot of the parent. Two Windows CRLF-sensitive string checks
  reproduce after encoding their unchanged field source to match the checkout;
  the remainder are existing source/API expectations. No new architecture
  failures were introduced.
- Clippy for both affected libraries completes with existing warnings.
  New test files pass targeted rustfmt checks, and `git diff --check` passes.

The 12 pre-existing architecture failures are:

```text
audio_control_and_hot_sfx_are_application_owned
audio_machinery_is_engine_owned_and_playback_policy_is_game_owned
canonical_notefield_keeps_internal_composition_helpers_crate_private
canonical_notefield_public_symbols_match_allowlist
column_countdowns_stay_on_the_prepared_hud_path
gameplay_config_and_profile_runtime_is_shell_prepared
generic_runtime_requests_stay_backend_neutral
measure_quads_stay_on_the_direct_field_path
noteskin_slot_contract_stays_renderer_neutral_and_asset_backed
options_runtime_state_and_persistence_are_shell_owned
select_music_session_runtime_is_shell_prepared
simply_love_notefield_uses_canonical_composition_boundaries
```

## Reproduce

```powershell
cargo test -p deadsync-song-lua -p deadsync-simfile --lib
cargo test -p deadsync-song-lua --test playback
cargo test --release -p deadsync-song-lua -p deadsync-simfile --lib
cargo test --release -p deadsync-song-lua --test playback
cargo clippy -p deadsync-song-lua -p deadsync-simfile --lib
cargo test --release -p deadsync-song-lua --lib actor_preparation_perf::actor_preparation_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib song::song_timing_ownership_perf::song_timing_ownership_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --test playback playback::ease_playback_perf::ease_playback_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run benchmarks serially after builds finish. Repeat six rounds, setting
`$env:DEADSYNC_BENCH_NEW_FIRST = '1'` for even rounds and removing that variable
for odd rounds. Ignored benchmarks do not run during ordinary regression tests.
