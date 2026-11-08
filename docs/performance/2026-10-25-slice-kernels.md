# PCM appends, float conversion and YUV normalization

Three independent optimizations, measured against production code at
`528b835f4cd1be1c389e56bc1c51a3d7b858db9c`. Both Lua crates are excluded.
No production `unsafe`, dependencies, compiler flags or public APIs were added.

## Execution changes

1. **Append mono PCM directly.** `append_channel_mapped_i16` selects the first
   sample of each complete frame in one iterator extension. It bypasses the
   generic repeat/tail loops and their per-frame `extend_from_slice` calls.
   Existing output prefixes, incomplete input frames and allocation reuse are
   preserved. This is the complete-clip append path used by audio loading.
2. **Round mono/stereo PCM without per-sample `round` calls.** After the unchanged
   `sample * 32767`, a saturating i16 cast gives the whole part. Subtracting that
   exact integer leaves the fraction; comparisons at +/-0.5 correct half ties
   away from zero, followed by integer saturation. This permits vectorization
   in the existing slice loops. The dynamic multichannel fallback retains
   `round`, where the alternative was slower.
3. **Normalize YUV bytes once per upload.** Two 256-entry stack tables replace
   repeated luma/chroma normalization FMAs. They use the original normalized
   bytes and ordered fused operations, preserving output rounding. Images below
   1,024 pixels use direct normalization. There is no retained cache or heap
   allocation; prepared tables occupy about 2 KiB of stack per upload. The
   pixel helper no longer takes normalization levels or performs normalization.

These changes remove executed work rather than reduce source length: the two
production files have 42 added and 16 removed lines, including comments and the
small-image path. No new named production functions or architectural layers
were introduced.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.99.1
(`58cf8f9c3`, LLVM 23.1.1). Default release settings: opt-level 3, full LTO,
static MSVC runtime, baseline target CPU. Identical test sources and Cargo
arguments were used for all archived executables:

```powershell
cargo test --release -p deadlib-audio-decode --test slice_kernels -p deadlib-render-backend-software --test texture_pipeline --no-run --message-format=json
```

Three stages were built: unchanged production (`before`), only the mono append
change (`mono`), and all retained changes (`after`). Every change was benchmarked
before implementation as well as after. The final comparisons below use those
three rebuilt stages with identical final benchmark sources.

`tests/support/kernel_bench.rs` pins the thread to logical CPU 2 and reads
Windows `QueryThreadCycleTime`. Each process reports the median of eleven
measured batches following a warmup; four process runs alternate before/after
order. Tables report medians of those four process medians. CPU throughput is
before cycles divided by after cycles. Paired ranges are observed min/max run
ratios, not confidence intervals. These are scheduled thread-cycle counters,
not retired-instruction PMU measurements or whole-game FPS results.

Inputs and capacities are prepared outside timing. The mono benchmark includes
truncating an existing two-sample output prefix. Audio function pointers,
channel counts and inputs are opaque to the optimizer. The YUV benchmark
updates a retained image. Allocation probes run separately from cycle timing;
all **632 probes** record zero allocations, reallocations, frees and churn bytes
before and after. Resident input/output buffers are outside those probes.

| Change / workload | Before cycles/op | After cycles/op | CPU throughput | Paired range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Mono append, 2 -> 1, 64 frames | 446.35 | 115.73 | 3.857x | 3.515-3.995x | 203.44 | 52.67 |
| Mono append, 2 -> 1, 256 frames | 1872.69 | 323.49 | 5.789x | 4.368-6.660x | 853.79 | 147.22 |
| Mono append, 2 -> 1, 4096 frames | 23526.53 | 4315.33 | 5.452x | 5.263-5.616x | 10724.18 | 1963.53 |
| Mono append, 6 -> 1, 4096 frames | 23811.26 | 4494.81 | 5.298x | 5.187-6.710x | 10863.73 | 2048.97 |
| Float conversion, mono, 256 frames | 1491.62 | 805.59 | 1.852x | 1.702-2.273x | 679.64 | 367.03 |
| Float conversion, mono, 4096 frames | 26037.74 | 12345.52 | 2.109x | 1.902-2.317x | 11868.44 | 5629.10 |
| Float conversion, stereo, 4096 frames | 55521.81 | 33040.60 | 1.680x | 1.663-1.859x | 25340.37 | 15070.69 |
| YUV, 32x32 | 33270.56 | 31283.58 | 1.064x | 1.023-1.116x | 15173.41 | 14266.08 |
| YUV, 1920x1080 | 66659625.00 | 59248154.00 | 1.125x | 1.090-1.200x | 30459125.00 | 27056525.00 |

Mono rows compare `before` with `mono`; float rows compare `mono` with `after`;
YUV rows compare `before` with `after`. Mono medians improve 3.43-5.79x across
the nine targeted workloads; mono/stereo float conversion improves 1.51-2.11x
across six workloads. The 1080p YUV gain is about 3.4 ms of CPU work per upload.

## Controls and limits

All workloads, including unchanged and slower controls, are retained in the
three TSV files. Unchanged controls vary; the mono-stage 2 -> 6 append at
4,096 frames measures 73092.60 -> 79709.54 cycles (0.917x), and its unchanged
256-frame mono fade measures 3488.26 -> 4031.52 cycles (0.865x). The round-stage
4,096-frame mono -> stereo append control measures 1116.84 -> 1253.57 cycles
(0.891x). These comparisons do not establish improvements to those paths.
Copy controls do not constrain relative 4 KiB buffer alignment, so their
placement-sensitive results should not be interpreted as copy-kernel gains.

YUV 640x360 has a 1.046x median with a 0.911-1.278x paired range; its gain is
less repeatable than 1080p. The 2x2 small-image path costs about ten extra
cycles (179.18 -> 189.68, 0.945x); it avoids the unconditional-table trial's
roughly tenfold tiny-image regression. Results describe this compiler/target,
not every architecture or feature-enabled build.

Discarded trials: replacing the endian decoder's unsafe per-element writes
with a safe iterator loop after one checked type cast showed no consistent
gain; a YUV slice/zip rewrite also lacked a repeatable useful gain. Applying
fraction correction to the dynamic multichannel loop was slower and was
reverted. None of those production changes were retained.

## Correctness

- **170 release tests pass**, with 28 performance tests ignored in ordinary
  runs, across `deadlib-audio-decode` and `deadlib-render-backend-software`.
  Four focused debug tests pass. Clippy completes successfully with existing
  warnings; it emits none for the changed kernels or the new audio test file.
  Commands:

  ```powershell
  cargo test --release -p deadlib-audio-decode -p deadlib-render-backend-software
  cargo test -p deadlib-audio-decode --test slice_kernels
  cargo test -p deadlib-render-backend-software --test texture_pipeline yuv_
  cargo clippy -p deadlib-audio-decode -p deadlib-render-backend-software --lib --tests
  ```

- The float differential check tests **1,330,777 inputs**: five adjacent floats
  around every i16 half-rounding boundary, both signs of every exponent with
  representative mantissas/NaN kinds, and one million seeded arbitrary bit
  patterns. Existing layout tests cover zero through eight channels, short
  sources, output tails, saturation, NaN and infinity.
- For finite in-range scaled samples, i16-to-f32 is exact, and subtracting the
  truncated whole part is exact. The fractional comparisons therefore implement
  the same half-away rounding. Out-of-range values can only be corrected farther
  out and saturate to the original bound. NaN casts to zero and both comparisons
  are false. The integer intermediate is bounded by [-32769, 32768].
- Mono append checks preserve prefixes, partial frames and buffer pointers
  across channel strides, with the complete i16 range in the source. Existing
  tests cover empty inputs and zero through nine input/output channels.
- YUV checks compare every output byte against the ordered fused reference,
  including changing levels/coefficients, infinities, NaNs and dimensions on
  both sides of the 1,024-pixel preparation threshold. Retained image updates
  preserve allocation pointers and have zero allocation churn.

## Reproduction and raw data

Use separate baseline and modified checkouts. Copy the final `slice_kernels.rs`
and `texture_pipeline.rs` test sources to the baseline checkout before building;
for the intermediate stage apply only the four-line mono append branch. Archive
the executables reported by Cargo's JSON output under `before`, `mono`, `after`.
Every measurement row includes its executable's SHA-256 hash.

```powershell
python scripts/benchmark_slice_kernels.py --before target/slice-kernels/evidence/before/slice_kernels.exe --after target/slice-kernels/evidence/mono/slice_kernels.exe --output docs/performance/2026-10-25-slice-kernels-mono.tsv
python scripts/benchmark_slice_kernels.py --before target/slice-kernels/evidence/mono/slice_kernels.exe --after target/slice-kernels/evidence/after/slice_kernels.exe --output docs/performance/2026-10-25-slice-kernels-round.tsv
python scripts/benchmark_slice_kernels.py --before target/slice-kernels/evidence/before/texture_pipeline.exe --after target/slice-kernels/evidence/after/texture_pipeline.exe --filter benchmark_yuv_conversion --output docs/performance/2026-10-25-slice-kernels-yuv.tsv
```

Raw records: [mono](2026-10-25-slice-kernels-mono.tsv),
[rounding](2026-10-25-slice-kernels-round.tsv),
[YUV](2026-10-25-slice-kernels-yuv.tsv).
