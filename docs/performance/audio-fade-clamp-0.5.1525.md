# Remove the repeated clamp during audio fading

Baseline: `8721d6d94`.

`deadlib-audio-decode::resample::apply_fade_envelope` calculates and clamps the
packet's endpoint volumes, then interpolates between them for each audio frame.
It previously clamped each interpolated volume again. The interpolation is
already bounded, so the second clamp is redundant.

The change removes only that per-frame clamp. Endpoint calculation/clamping,
interpolation arithmetic, near-unity skipping, integer rounding, channel order,
and partial-frame handling are unchanged. No cache or allocation is added.
Music decoding invokes this function for packet fades; full-volume and silent
packets keep their existing early-return/fill paths.

## Why the volume remains bounded

The endpoints are finite floats in `[0, 1]`: they originate from finite integer
positions, a guarded nonzero denominator, and a clamp before conversion to f32.
The frame count is positive, and converting its smaller frame indices to f32
and dividing produces a finite `t` in `[0, 1]`. Very large indices can round
to the same float as the frame count, giving `t == 1`, which is also safe.

For decreasing endpoints, the rounded difference cannot be more negative than
the exactly representable negative start volume, so the fused interpolation
cannot go below zero. For increasing endpoints, the worst case is an end volume
of one. The rounded difference can put the exact sum at most `2^-25` above one;
the rounding midpoint is `1 + 2^-24`, so the fused result still rounds to at
most one. This reasoning depends on these unit
interval bounds and the existing fused multiply-add; it is not a general rule
for arbitrary interpolation endpoints.

## Behavior checks

The integration test compares the production function with a frozen copy of
the baseline. All 5,785 PCM packet cases match exactly, covering zero/mono/stereo/
multichannel output, empty and partial frames, fade-in/out and boundary-crossing
packets, extreme signed/unsigned frame positions, every i16 value, deterministic
random packets, and near-unity rounding.

A separate numeric check compares unclamped and clamped interpolation results
by float bits in 4,002,800 cases. It includes subnormals, signed zero, endpoints
near one, one million generated input triples, and frame counts around `2^24`
and `usize::MAX`. This detects volume differences that PCM rounding could hide.

The three active integration tests passed before and after the edit in release.
The final code also passed all 34 decoder library tests and the three integration
tests in debug, formatting, diff checks and performance Clippy checks. Existing
type-complexity warnings remain. The manual benchmarks are ignored by default.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6; variant
order reversed in run 2. No builds or other tests from this pass ran during
measurement. Both variants use equivalent black-boxed function pointers in
the same executable. Windows `QueryThreadCycleTime` supplies thread cycles;
allocation counting runs separately.

Each operation restores a PCM packet into an existing buffer and applies its
fade. The restoration copy is included equally; input creation is excluded.
Seven batches follow three warmups. Active-fade batches contain 8,192 operations;
the full-volume and silent controls use 262,144 because each operation takes
only tens of nanoseconds. Throughput counts audio frames per second.

| Case | Before ns/op | After ns/op | Median paired cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| mono256 | 1931.4 | 2005.8 | 6.7% | -9.0 to 8.1% | 7.2% |
| stereo256 | 3523.7 | 3102.9 | 8.8% | 8.8 to 12.0% | 9.7% |
| stereo4096 | 55938.1 | 49767.3 | 10.7% | 6.5 to 20.3% | 11.9% |
| surround256 | 8032.0 | 7527.4 | 7.0% | 6.5 to 10.1% | 7.5% |
| fade_in256 | 3594.8 | 3049.3 | 10.4% | 4.0 to 15.1% | 11.6% |
| cross_silence | 3490.2 | 3134.7 | 12.0% | 10.1 to 12.5% | 13.6% |
| unity | 43.8 | 43.6 | 0.2% | 0.2 to 3.5% | 0.3% |
| silence | 62.6 | 63.9 | -0.8% | -2.2 to 7.1% | -0.9% |

Times are medians across runs; percentages are medians of paired ratios, so
they need not equal ratios of displayed time medians. Negative reductions mean
a slower candidate. Stereo fade workloads improved in every final paired run.
Mono results are mixed; no consistent mono improvement is claimed. Full-volume
and silent paths have no algorithmic change and show small timing differences.

Exploratory runs used 8,192 operations even for the tiny controls, producing
large percentage swings there. An unchanged-code executable also showed those
swings. Increasing the control batch duration reduced the variation; the final
table uses only the revised batches. The [raw CSV](audio-fade-clamp-0.5.1525.csv)
retains both phases and the unchanged control, including absolute cycle counts,
timing ranges, iteration counts, throughput and allocation counts.

Every measured operation used zero allocations, reallocations, frees, allocated
bytes and freed bytes, before and after. These are synthetic decoder CPU timings;
they do not measure total game CPU, audio callback latency or audible quality.
Behavior validation compares the actual PCM output exactly.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-audio-decode --test fade_clamp
cargo test --locked -p deadlib-audio-decode --lib --test fade_clamp
cargo clippy --locked -p deadlib-audio-decode --lib --test fade_clamp -- -D clippy::perf
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed release executable directly:
# <executable> benchmark_fade_clamp --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Local final logs are in `target/fade-clamp-pass`. Exploratory logs are in
`target/fade-clamp-short-controls`.
