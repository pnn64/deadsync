# Rules timing conversions, 2026-11-04

Three independent changes in `deadsync-rules`, based on `9b94bc272`. Work is
isolated on `perf/non-lua-kernels-20261104` at
`C:\GitHub\deadsync-perf-nonlua-20261104`. Lua and the original worktree's
existing edits are outside this batch.

## Changes and behavior

1. `timing_ns_delta_seconds` computes the exact integer magnitude with
   `i64::abs_diff`, converts its `u64` result once, then restores the sign.
   This removes the signed `i128` subtraction/conversion from speed-modifier
   interpolation and cached song-position timing. Every possible difference
   fits `u64`; sign restoration preserves symmetric floating-point rounding.
   The multiplication by `1.0e-9` and final `f32` conversion are unchanged.
2. `judgment_time_error_music_ns_from_ms` uses the existing
   `normalized_song_rate` and removes a redundant floating-point clamp.
   `(i64::MIN + 1) as f64` is exactly `-2^63`; `i64::MAX as f64` rounds to
   `2^63`. Rust's saturating float-to-integer cast already implements these
   bounds. NaNs still become zero, infinities still saturate, and rounding
   remains ties away from zero.
3. `TimingProfileNs::from_profile_scaled` validates and widens its shared rate
   once, then converts all five tap windows and the optional FA+/mine windows
   directly. This removes the nested helper, repeated rate validation and
   seven redundant clamps using the same bounds argument. Multiplication order,
   rounding, optional-window behavior and invalid-rate fallback are unchanged.

All production changes use safe Rust. The two production files have a net
reduction of two lines, including comments and the test module declaration;
the execution savings come from removing conversions, clamps and repeated
validation rather than shortening names or hiding work behind new APIs.

## Measurements

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1; optimized release builds with LTO and
the repository's static MSVC runtime setting. The small build workspace contains
byte-identical core/rules sources and their existing tests, with `log 0.4.34`.
This avoids building unrelated workspace members and fits the machine's limited
local storage. Build/executable/source archives are on
`D:\deadsync-timing-conversions-20261104`.

Each workload processes 256 conversions. The existing benchmark helper pins
the thread to CPU 2, uses Windows `QueryThreadCycleTime`, alternates original
and current implementations for 12 rounds, discards the warmup round, and takes
the median of 11 samples. Fixtures and allocation accounting run outside timing.
The table contains the median of five final process runs. Throughput is the
ratio of original/current CPU cycles; these are kernel measurements, not a
claim about whole-game frame rate.

| Workload | Before cycles/256 | After cycles/256 | CPU throughput | Fewer cycles |
| --- | ---: | ---: | ---: | ---: |
| Timestamp delta, gameplay | 9,208.02 | 2,837.04 | 3.246x | 69.2% |
| Timestamp delta, full integer range | 6,688.39 | 2,131.12 | 3.138x | 68.1% |
| Judgment conversion, gameplay | 6,590.05 | 4,843.93 | 1.360x | 26.5% |
| Judgment conversion, extreme floats/rates | 4,963.77 | 4,599.62 | 1.079x | 7.3% |
| Profile construction, gameplay | 39,693.28 | 34,259.12 | 1.159x | 13.7% |
| Profile construction, extreme floats/rates | 34,381.38 | 28,842.96 | 1.192x | 16.1% |

Both variants perform zero allocations, reallocations, frees or allocated bytes
in every workload, asserted with the existing thread-local counting allocator.
Final medians improve all six workloads. Individual paired runs are noisier:
the extreme-judgment throughput range is 0.941–1.243x and the gameplay-profile
range is 0.999–1.422x. The earlier three-run `profile` stage also improves all
six workload medians, with gameplay ratios of 2.914x, 1.163x and 1.124x for
delta, judgment and profile respectively. Treat the smaller gains as ranges
subject to machine load rather than precise universal percentages.

The sequential records preserve benchmarks before and after every change:

- `baseline`: unchanged production conversions, three runs.
- `delta`: timestamp change only, three runs.
- `judgment`: timestamp and judgment changes, three runs.
- `profile`: all three changes, three runs.
- `final`: all three changes with formatted changed lines, five runs.

Each stage's adjacent TSV contains cycle and elapsed-nanosecond measurements;
its manifest records source and binary SHA-256 hashes, compiler details and
build flags. CPU clock/load changed between stages, so compare the original and
current measurements paired within a stage rather than absolute numbers across
different stages. Unchanged profile code in the `judgment` stage measured
0.930–0.944x; the retained profile change improves both workloads in the final
binary. No production candidate was discarded in this batch.

## Correctness and reproduction

All 150 core/rules unit and integration tests pass in both release and debug,
with zero failures. Existing ignored performance tests remain ignored except
for this batch's release benchmark. Added differential checks compare against
frozen production conversions from the base commit:

- Cartesian products of integer extrema and values around every integer power
  of two, plus 200,000 random timestamp pairs, compared bit for bit as `f32`.
- Cartesian products of floating-point special values, half-integer rounding
  boundaries, adjacent representable values and saturation boundaries, plus
  300,000 random offset/rate bit-pattern pairs. Judgment outputs and every
  profile field match, including absent/present FA+ windows.

Clippy completes successfully for all rules targets. It reports existing
warnings elsewhere in the crate; none refer to the added files or changed
conversion lines. Rustfmt passes for judgment and the two added Rust files.
The changed timing code follows rustfmt; the full existing `timing.rs` has
unrelated formatting differences. `git diff --check` passes.

```powershell
$env:DEADSYNC_TIMING_ARCHIVE = 'D:\deadsync-timing-conversions-20261104'
python scripts/benchmark_timing_conversions.py final --runs 5
python scripts/benchmark_timing_conversions.py debug --debug
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo clippy --offline --manifest-path D:\deadsync-timing-conversions-20261104\workspace\Cargo.toml -p deadsync-rules --all-targets --no-deps
```

Omit `DEADSYNC_TIMING_ARCHIVE` to use `target/timing-conversions` locally.
The runner archives the exact tested sources, test output, executables and
per-run benchmark output. The `final` and `debug` manifests also preserve the
dependency lockfile, test totals and correctness-log hashes.
