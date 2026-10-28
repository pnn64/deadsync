# Reproducible performance measurements

Run from the workspace root:

```powershell
$env:DEADLIB_GIF_BENCH_ITERS = '40'
$env:DEADLIB_GIF_BENCH_SAMPLES = '5'
cargo bench -p deadlib-image-gif --bench pipeline
```

The deterministic harness validates both encoders and both decoders before
timing. It uses gif 0.14.2, gif-dispose 6.0.0 and weezl 0.1.12 only as development
dependencies. All decode comparisons use identical Rust gif-encoded input.
Fixtures: 512x512 solid, 16-color tiles, 256-color gradient, xorshift noise,
interlaced noise, and a 512x512 animation with 32 offset 128x128 frames,
transparency and Keep/Background/Previous disposal. Seed: 1327.

- `decode-retain` retains each frame's indexed allocation. The Rust gif reader
  frame is cloned because the reader owns/reuses it; native events already own
  their pixels. Native parsing additionally retains extension records.
- `decode-reuse` consumes all frames, recycling native pixels and using the
  Rust gif reader's reusable frame, without retaining an animation.
- `decode-render` additionally composites each frame onto a reusable transparent
  RGBA canvas, without copying the resulting canvas. It includes initialization.
- `composite` excludes decoding and includes canvas allocation/initialization
  and every graphic. Its throughput denominator is the sum of logical-screen
  RGBA sizes, even for partial frames. This is **not memory bandwidth**; use
  latency for the offset-animation comparison.
- `encode` starts from identical row-major indexed frames. It includes the row
  permutation needed by Rust gif's encoder for interlaced input. No quantization.
  Encoded sizes are reported for the corresponding encoder.
- `lzw-decode` times only the first frame's concatenated compressed payload,
  including a fresh decoder and output allocation. For interlaced input its
  output is still transmission-order indices. `indexed_bytes` describes the
  whole animation; LZW throughput uses only that first frame's pixel count.

Three warmups precede each operation. Reported values are medians across sample
groups; raw TSVs also include minimum and maximum group-average latencies.

## Environment and results (2026-10-26)

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz (22 cores, 44 logical processors),
Rust 1.99.1, Cargo bench profile: opt-level 3, LTO, debug info 1, static MSVC CRT.
The host is shared; CPU affinity, frequency and other workloads were not
controlled. The final confirmation ran after this task's validation and fuzz
processes finished. These are reproducible local workload comparisons, not a universal speed
claim. A controlled multi-machine corpus benchmark and allocation profiler
remain useful future work.

Confirmation (`benchmark-confirm.tsv`, 40 iterations, 5 sample groups):

| Fixture | Native indexed decode MiB/s | Rust gif MiB/s | Native decode+render ms | gif+gif-dispose ms | Native encode MiB/s | Rust gif encode MiB/s |
|---|---:|---:|---:|---:|---:|---:|
| solid | 1382 | 656 | 0.672 | 0.950 | 137 | 108 |
| tiles16 | 771 | 308 | 0.897 | 1.515 | 104 | 90 |
| gradient256 | 322 | 220 | 1.360 | 1.861 | 81 | 81 |
| noise256 | 101 | 71 | 3.053 | 4.251 | 48 | 29 |
| interlaced-noise | 101 | 71 | 3.076 | 4.542 | 44 | 27 |
| animation32 | 431 | 296 | 2.087 | 3.653 | 93 | 77 |

Native retained decode is 1.42–2.51x the Rust gif throughput on these fixtures;
decode+render is 1.37–1.75x. Encoding is approximately tied on the gradient and
faster on the other cases. Output sizes are identical on solid/tiles/gradient/
animation; native noise outputs are 43–50 bytes smaller (about 361 KiB each).
This does not establish superiority on every GIF, CPU, compiler or workload.

## Measured optimization decisions

Baseline code and validation are committed in `279a21db4`.
`benchmark-baseline.tsv` records 15 iterations x 3 sample groups.
Stage timings showed canvas initialization dominating static compositing.
Replacing per-pixel vector extension with a single zero initialization and a
bulk array fill approximately doubled that stage. The decoder keeps the
output-referencing phrase dictionary, which copies phrases directly from
decoded indices and avoids prefix-chain traversal and a reverse expansion stack.

`benchmark-word-refill-trial.tsv` tried checking/refilling a 32-bit word before
each LZW code. It regressed most LZW timings by about 10–20% and was discarded.
`benchmark-bulk-refill-trial.tsv` instead decodes all available codes after each
safe 32-bit refill, retaining byte fallback for small streaming fragments.
That approach is retained. One noisy gradient-stage sample in that trial was
inconsistent with full decode; the longer confirmation resolves it and shows
about 14–16% faster LZW on gradient/noise/interlaced noise than baseline.

The confirmation improves full decode+render by 1.29–1.95x over baseline across
the six cases. All corpus, malformed, giflib and animation parity checks were
rerun on the retained optimization, followed by another sanitizer fuzz smoke.
Flat buffers, bulk copies and compiler-vectorizable fills remain safe Rust.
No architecture-specific SIMD was justified by the measured stage bottlenecks.
The scalar implementation has no CPU feature requirements beyond Rust's target.

Further tuning opportunities include larger independent corpora, encoder
adaptive-clear/compression-effort choices, code-batch decoding and palette
expansion on other architectures. None changes indexed-data or animation
semantics, and future candidates must retain differential parity.

## CPU cycles and allocations (2026-10-28)

Windows x86-64, Intel Core i7-1250U, Rust 1.97.1 / LLVM 22.1.6, workspace
bench profile (opt-level 3, LTO, debug info 1, static MSVC CRT). The harness
pins its thread to logical CPU 2. Windows `QueryThreadCycleTime` measures
scheduled thread cycles, excluding time descheduled. These are OS-reported
cycles, not retired instructions; frequency, cache contention, and other host
activity remain uncontrolled. Other platforms currently report zero cycles.
Allocation counts use the repository's thread-local System allocator probe,
in a separate invocation outside the timed samples. Allocated bytes include
all allocations and replacement sizes on reallocation, not just live memory.

`pipeline` now also includes real 2/16-entry palettes and repeated plain-text
rendering after warming reusable buffers. Its existing assertions compare
indexed pixels and RGBA output against gif/gif-dispose before timing.
The text fixture independently checks that its all-foreground RGBA is white.

Save an executable after each build, then alternate before/after builds:

```powershell
cargo bench -p deadlib-image-gif --bench pipeline --no-run
# Copy the printed executable to before.exe / after.exe at the relevant revisions.
./crates/deadlib-image/gif/benches/paired.ps1 before.exe after.exe out.tsv composite tiles16-palette solid2-palette
```

Each comparison runs six pairs, alternates which variant runs first, and uses
40 iterations x 7 samples per invocation (100x iterations for text). Tables
report the upper median of the six invocation medians. The TSVs retain every
invocation and its sample min/max. Throughput ratios below use before/after
cycles; wall-clock throughput remains available in the raw data. The broad
`00-baseline.tsv` and `01-validation.tsv` runs used 80 x 9; changing host load
also moved unchanged operations, so only alternating comparisons support the
individual optimization claims.

Replacing the per-pixel early-exit predicate with a byte-maximum reduction
allows compiler SIMD on valid small-palette frames. The 256-color fast path
still skips scanning. Invalid indices return the same error, though validation
now scans the whole buffer before rejecting an invalid small-palette frame.

| Workload / operation | Before cycles | After cycles | CPU throughput | Allocations |
|---|---:|---:|---:|---:|
| tiles16-palette / decode-retain | 1,407,561 | 935,109 | 1.51x | 4 -> 4 |
| solid2-palette / decode-retain | 857,843 | 372,132 | 2.31x | 4 -> 4 |
| tiles16-palette / composite | 1,029,242 | 744,411 | 1.38x | 2 -> 2 |
| solid2-palette / composite | 1,628,508 | 1,185,984 | 1.37x | 2 -> 2 |

Raw comparisons: `validation/2026-10-28/01-paired-{decode,composite}.tsv`.
Production change: one validation predicate, removing the branch on each
pixel. All production code retains `forbid(unsafe_code)`.

Plain-text rendering now uses the compositor's existing global/external
palette directly. Removing the temporary local-palette Vec also removes its
reserve, copy, and drop on every graphic (12 requested bytes for a two-color
palette, up to 768 for a 256-color palette). The missing-palette check still
runs before invoking the caller's rasterizer. A regression test checks exact
colors, Previous disposal, and zero allocation churn on repeated global and
external palette rendering.

| Workload / operation | Before cycles | After cycles | CPU throughput | Allocations / bytes |
|---|---:|---:|---:|---:|
| text-small / render-text-reuse | 338 | 282 | 1.20x | 1 / 12 -> 0 / 0 |
| text-grid / render-text-reuse | 89,550 | 88,906 | effectively unchanged | 1 / 12 -> 0 / 0 |

Raw comparison: `validation/2026-10-28/02-paired-text.tsv`, immediately after
the validation change. Production change: delete the palette allocation/copy
and temporary local-palette field (10 lines removed, 3 added).
