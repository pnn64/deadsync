# Rules runtime work, 2026-11-04

Three retained optimizations in `deadsync-rules`, starting at `a4d53398c` on
isolated branch `perf/non-lua-kernels-20261104`. Lua crates and the main
worktree's existing edits are outside this batch.

## Changes and behavior

1. `recompute_player_totals` caches the carried-hold count until the earliest
   active end row. Rows before that boundary skip the 20-lane scan. A new hold
   updates the count and boundary; expiration rebuilds both in the existing
   array. End rows remain inclusive, `usize::MAX` remains an inactive sentinel,
   and an overwritten earliest end can cause one extra scan without changing
   results. Existing saturating/wrapping totals and column fallbacks are intact.
2. `LiveTimingStats` maintains the recent maximum when recording a judgment.
   It rescans the ring only when an outgoing maximum is not replaced by the
   incoming value. `live_timing_stats_snapshot` reads the cache instead of
   scanning 64 errors on each query. Miss handling, sums, FMA order and window
   eviction stay unchanged. Incoming NaNs cannot replace an evicted maximum.
   A manual `Debug` implementation preserves the original diagnostic fields
   and ordering, excluding the derived cache.
3. `ScrollSpeedSetting::pixels_per_second` validates positive finite BPMs with
   one integer interval test instead of separate finiteness and floating-point
   comparisons. Positive finite `f32` patterns are `1..0x7f80_0000`; subtracting
   one with wrapping and comparing to `0x7f7f_ffff` accepts exactly that set.
   Division/multiplication order and zero fallback are unchanged.

All production code remains safe Rust. The main execution removed is repeated
lane traversal and snapshot maximum reduction. The additional source maintains
those caches and preserves the existing diagnostic view. Across the three
production files, the diff adds 70 and removes 14 lines, including four lines
registering the test-only module.

## Final measurements and tradeoff

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1, release optimization with LTO and
the repository's static MSVC runtime setting. The runner builds exact
core/rules sources in a small workspace; its lockfile records `log 0.4.34`.

Each process pins its benchmark thread to CPU 2, alternates original/current
implementations for 12 rounds, discards the warmup, and reports the median of
11 `QueryThreadCycleTime` samples. Final values below are medians of five
process runs. CPU throughput is before/after cycles. Fixture generation and
allocation counting are outside timing; elapsed nanoseconds are also retained
in the TSVs.

One totals operation recomputes a 4,096-row chart. Other operations contain
256 calls or judgments. Final timed batches are five times longer than the
initial probes to reduce clock/load noise.

| Workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Totals, tap chart | 177,439.84 | 68,948.46 | 2.574x |
| Totals, mixed holds | 180,962.83 | 76,853.09 | 2.355x |
| Totals, wide periodic holds | 191,349.13 | 96,323.18 | 1.987x |
| Totals, sustained 20-lane holds | 159,086.58 | 75,956.66 | 2.094x |
| Snapshot, empty | 2,436.19 | 2,234.77 | 1.090x |
| Snapshot, partial window | 10,681.40 | 6,510.19 | 1.641x |
| Snapshot, full steady-state window | 22,156.68 | 6,500.62 | 3.408x |
| Snapshot, nonfinite errors | 22,219.11 | 6,491.67 | 3.423x |
| Record only, no snapshots | 5,119.62 | 6,352.99 | 0.806x |
| Record plus one snapshot | 26,359.76 | 12,970.91 | 2.032x |
| Record plus four snapshots | 89,575.76 | 32,741.94 | 2.736x |
| Scroll speed, CMod | 3,856.97 | 2,853.81 | 1.352x |
| Scroll speed, mixed mods | 4,252.20 | 2,741.29 | 1.551x |
| Scroll speed, invalid values | 4,104.83 | 2,437.24 | 1.684x |

All fourteen workloads have zero allocations, reallocations, frees and byte
churn in both variants, asserted by the existing thread-local allocator.
All five final runs improve the thirteen read/combined/totals/scroll workloads.
Tap-chart ratios range from 2.525–2.596x, full snapshots from 3.335–3.408x,
and CMod calculation from 1.346–1.417x.

The maximum cache deliberately moves work to judgment updates. **Record-only
CPU cost rises 24.1%, about 4.8 cycles per judgment in this workload.** This
cost also applies when the live timing display is disabled. With one snapshot
per update, the measured combined path improves 2.032x; four snapshots improve
2.736x. The theme queries the snapshot for its enabled live-timing display.
These measurements describe the relevant kernels and query ratios, not a
whole-game frame-rate result.

The cache increases `LiveTimingStats` from 304 to 312 bytes on this target.
Default diagnostic output matches the pre-batch control exactly, and ordinary
and alternate Debug formatting are compared after judgment traces.

## Before/after evidence and rejected candidates

Adjacent TSVs/manifests and the archived sources preserve every measured stage:

- `baseline`: unchanged production kernels.
- `hold-mask`: first active-lane bitmap candidate. `dense-control` exposed a
  sustained-hold regression to 0.768x, so it was discarded.
- `hold-boundary`: retained earliest-expiration/count cache, improving all four
  totals workloads instead of scanning active bits on every row.
- `snapshot-bits`: rejected integer maximum reduction, only 0.382x on full
  windows. `snapshot-before` records the unchanged reduction with the expanded
  record/read harness and steady-state ring buffers.
- `snapshot-cache`: retained maximum cache, with record-only cost recorded.
- `scroll`: rejected parser rewrite; common parsing measured 0.978x and padded
  parsing 1.043x. The parser source was restored.
- `snapshot-lanes`: four independent maxima improved full reads about 1.10x,
  but empty reads regressed to 0.765x. `snapshot-compare` regressed full reads
  to 0.625x. Both were discarded in favor of the measured maximum-cache path.
- `scroll-bits`: retained positive-finite integer guard.
- `control`: exact pre-batch production sources with the final harness and
  longer timed batches; all fourteen ratios lie between 0.995x and 1.008x.
- `final`: all three retained changes, final longer batches, five runs.

Initial probes include the discarded parser and snapshot experiments. The final
harness retains totals, snapshot reads, record/read combinations and scroll
calculations. Compare paired ratios within a process: absolute cycles vary
between stages. An initially noisy control was repeated with longer batches;
the final control above calibrates the final harness.

## Correctness and reproduction

All 153 core/rules unit and integration tests pass in release and debug.
The broader current core/rules/chart suites also pass all 206 tests in both
profiles; `rules-runtime-compatibility.json` records their source/log hashes
and commands, with logs in the archive's `compatibility` directory.
New differential checks cover inclusive hold tails, replacing heads at a tail,
missing hold data, sentinel/past end rows, 20-lane dense holds, out-of-domain
columns, partial ranges and empty ranges. Snapshot tests feed 256,000 updates
through both record implementations, including misses, all window lengths,
wraparound, expired maxima, subnormals, signed zeros, NaNs and infinities.
Scroll calculations cover every mod, special-value combinations and 100,000
random four-value input tuples.

Non-NaN floating results are compared bit for bit. Arithmetic NaNs are compared
by classification: even the unchanged control produced differing NaN payloads,
which [Rust explicitly permits](https://doc.rust-lang.org/std/primitive.f32.html#nan-bit-patterns).
The cached maximum remains non-NaN and its exact bits are checked. Diagnostic
formatting is also checked, including the original default view in the control.

Changed Rust code follows rustfmt, Python compilation and `git diff --check`
pass, and Clippy completes with pre-existing warnings outside the changed code.
The full existing `timing.rs` has unrelated formatting differences.

Archives, exact tested sources, logs and executables are at
`D:\deadsync-rules-runtime-20261104`. Manifests record source, compiler, binary
and correctness-log hashes, the build flags, dependency lockfile and test total.

```powershell
$env:DEADSYNC_RUNTIME_ARCHIVE = 'D:\deadsync-rules-runtime-20261104'
python scripts/benchmark_timing_conversions.py control --runtime --baseline a4d53398c
python scripts/benchmark_timing_conversions.py final --runtime --runs 5
python scripts/benchmark_timing_conversions.py debug --runtime --debug
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo clippy --offline --manifest-path D:\deadsync-rules-runtime-20261104\workspace\Cargo.toml -p deadsync-rules --all-targets --no-deps
```

The baseline option changes only the build copy and appends the test-module
registration to the baseline timing source. Without the archive environment
variable, the runner uses `target/rules-runtime`.
