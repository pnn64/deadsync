# Nanosecond conversions — 2026-10-26

Three safe Rust optimizations in `deadsync-core::song_time`, following the hot-path and CPU-throughput guidance in `rust-performance.md`. The helpers serve gameplay timestamp conversion, display-clock rates, and captured-input clock extrapolation. The production diff changes three existing functions: 28 lines added and three removed, including comments. No production function, type, dependency, public API, unsafe block, or CPU feature requirement is added. Lua and unrelated worktree changes are excluded.

## Work removed from common paths

1. **Seconds to nanoseconds:** replace general f64 rounding with a signed half bias and a saturating integer conversion. Finite f32 seconds times 1,000,000,000 have at most 45 significant bits: the multiplier is `2^9 × 1,953,125`. This leaves enough precision that biasing a sub-half value cannot round it up to one. Values at or above `2^52` are already integral f64 values and skip the bias. The existing invalid-input sentinel and saturation remain.
2. **Rate normalization:** replace separate finite/positive checks with one unsigned interval check on the float bits. Positive finite values occupy `1..0x7f80_0000`; subtracting one with wrapping arithmetic allows a single upper-bound comparison. Negative numbers, either zero, infinities, and all NaNs still return exactly 1.0. Valid rates retain their original bits.
3. **Host-delta scaling:** convert inputs through i64 when they fit, retaining the i128 conversion for larger inputs. Scaled magnitudes below half return zero. Magnitudes in `[0.5, 2^52)` use the signed half bias and an i64 conversion before widening to i128; larger magnitudes are already integral and use the original saturating i128 cast. This removes general rounding and wide conversions from ordinary captured-input deltas. The sub-half guard matters here: `(2^53 - 1) × 2^-54` is just below half and must remain zero.

These changes simplify executed operations while making the numeric domains explicit. The host-delta fallback preserves the full i128 input/output contract.

## Isolated measurements

Each row compares consecutive source stages: original → seconds conversion → rate check → host-delta conversion. All builds use the repository's **default release profile, including LTO**. Results are median thread CPU cycles per fixture from eight fresh processes per version for the first two stages and ten for the third.

| Change / workload | Before cycles | After cycles | CPU throughput | Paired ratio range |
| --- | ---: | ---: | ---: | ---: |
| Seconds conversion | 40,785.85 | 32,434.97 | 1.257x | 1.046–1.346x |
| Seconds change, scaled timestamp caller | 52,271.18 | 41,461.90 | 1.261x | 1.207–1.475x |
| Seconds change, timestamp-offset caller | 49,553.22 | 40,211.30 | 1.232x | 1.175–1.503x |
| Rate check, valid inputs | 17,285.07 | 7,666.59 | 2.255x | 2.016–2.478x |
| Rate check, mixed inputs | 17,513.36 | 9,785.31 | 1.790x | 1.594–2.030x |
| Rate change, scaled timestamp caller | 39,373.36 | 36,021.04 | 1.093x | 1.077–1.143x |
| Complete host-delta scaling | 160,952.76 | 90,255.65 | 1.783x | 1.419–1.863x |

Raw data, including unchanged controls: [seconds](2026-10-26-nanosecond-seconds.tsv), [rates](2026-10-26-nanosecond-rates.tsv), [host deltas](2026-10-26-nanosecond-deltas.tsv). Paired ranges are minimum/maximum ratios of corresponding process runs, not confidence intervals. An input-conversion-only trial improved host deltas less; the retained change also removes ordinary output rounding/conversion work.

## All three changes together

The [combined comparison](2026-10-26-nanosecond-final.tsv) uses sixteen fresh processes per version, comparing all three original functions with all three final functions.

| Workload | Before cycles/fixture | After cycles/fixture | CPU throughput | Before → after ns/fixture |
| --- | ---: | ---: | ---: | ---: |
| Seconds conversion | 37,499.93 | 30,788.17 | 1.218x | 17,096.75 → 14,161.25 |
| Broad float-pattern seconds | 21,604.28 | 23,811.90 | 0.907x | 9,877.25 → 10,926.50 |
| Valid rates | 16,395.56 | 7,444.90 | 2.202x | 7,622.50 → 3,515.75 |
| Mixed rates | 16,232.03 | 10,090.42 | 1.609x | 7,400.50 → 4,632.00 |
| Host-delta scaling | 158,123.41 | 90,269.92 | 1.752x | 72,173.00 → 41,195.25 |
| Scaled timestamp caller | 49,485.73 | 35,934.35 | 1.377x | 22,649.75 → 16,381.00 |
| Timestamp-offset caller | 47,441.08 | 38,284.09 | 1.239x | 21,638.00 → 17,615.25 |

The broad float-pattern seconds batch regresses by about **10% in CPU cycles**, including in the isolated seconds comparison. It contains every exponent class and many values far outside gameplay timestamps. This is an explicit cost of the added magnitude handling; the change is retained for its gains on typical timestamps and both measured callers. Large host-delta fallback throughput is not separately measured. These are local helper/caller results, not whole-game FPS or input-latency measurements. No allocation is removed: all probes report zero allocations, reallocations, frees, requested bytes, and peak bytes before and after.

## Method and verification

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Measured archives use `--release --locked`, the static MSVC runtime from `.cargo/config.toml`, the baseline CPU target, and default LTO. The existing helper pins the benchmark thread to logical processor 2, reads `QueryThreadCycleTime`, discards one warmup sample, and takes the median of eleven samples. Each sample times 200 fixture operations and divides by 200. The driver alternates before/after process order; tables use medians of process medians. Fixture creation, allocation probes, and compilation are outside CPU timing. No compilation started by this task overlaps reported comparisons; other activity on the shared machine is uncontrolled.

Most workloads contain 4,096 calls per operation. Typical seconds range from -600 to just below 3,600, generated in millisecond increments before f32 rounding. Valid rates range from 0.25 to 1.7421875 in steps of 1/128; mixed rates replace one in eight with zero, negative zero, -1, infinity, or NaN. Host deltas range from -1,000,000 to just below 39,000,000 nanoseconds. The broad seconds batch contains 3,072 bit patterns: both signs, every exponent, and six mantissa patterns. All inputs/results pass through `black_box`. The four TSVs retain all **588 process/workload records**, executable SHA-256 values, elapsed times, and allocation counts.

Frozen references are the production helpers from `3ba0ad8e6`. Three new checks verify:

- Seconds and rate results across all float exponent/sign classes, two million generated float-bit inputs, and adjacent f32 values around 100,000 nanosecond half ties, `2^52` nanoseconds, and i64 saturation.
- Host-delta results for i128 limits, neighbors of every positive/negative power of two through bit 126, all 3,072 rate patterns, one million generated i128/rate combinations, and the sub-half counterexample above. This covers both conversion paths, tie rounding, and saturation.
- Scaled timestamp and saturating timestamp-offset callers for 200,000 generated inputs, including invalid timestamps and maximum timestamps.

Validation passes: **1,018 release tests** across core, chart, rules, and gameplay (including doctests; 17 ignored), **15 core debug tests** (two ignored), and all three new checks in the measured default-LTO after archive. Core Clippy passes for all targets without warnings. All changed/new Rust files pass `rustfmt --check`. Logs and archived executables are under `target/nanosecond-kernels/`.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-core -p deadsync-chart -p deadsync-rules -p deadsync-gameplay
cargo test --locked -p deadsync-core
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-core --all-targets
cargo test --release --locked -p deadsync-core --test time_conversion --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 16 --test-filter benchmark_time_conversion --log-dir target/nanosecond-kernels/runs --before target/nanosecond-kernels/before/core.exe --after target/nanosecond-kernels/deltas-round/core.exe --output docs/performance/2026-10-26-nanosecond-final.tsv
```

To reconstruct the archives, keep the committed harness and reference module. Restore `song_time_ns_from_seconds`, `normalized_song_rate`, and `scaled_song_delta_ns` from `3ba0ad8e6` for `before`; enable only seconds conversion for `seconds`, add rate normalization for `rates`, and add the complete host-delta change for `deltas-round`. Build each stage with the default-release command above and save Cargo's reported `time_conversion` executable before changing the next function. Compare consecutive archives with the same driver using `--runs 8`, `--runs 8`, and `--runs 10` respectively.
