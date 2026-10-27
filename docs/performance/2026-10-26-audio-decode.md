# Fixed-stride PCM conversion and direct channel appends

Three measured optimizations in `deadlib-audio-decode`, relative to its production
code at `02183e7c1`. No production unsafe, dependencies, public APIs, arithmetic
changes, or compiler flags were introduced. Measurements concern these kernels;
they do not establish an equivalent end-to-end decoder speedup.

## Changes

1. **Stereo packets to mono:** specialize the existing slice-copy helper with
   `as_chunks::<2>()`. The compiler can load/deinterleave fixed stereo frames
   instead of advancing dynamic chunks. Packet dispatch, wider-channel copying,
   output initialization, and the existing append API stay as before.
2. **Append repeated mono/stereo channels:** resize the appended region once and
   fill it with the existing `repeat_mono_stereo` helper. This bypasses nested
   repeat/tail loops and their per-frame `Vec::extend_from_slice` capacity checks.
   The existing prefix is preserved; odd output widths still end with the first
   source channel. Wider source layouts retain their existing fallback.
3. **PCM to planar floats:** use fixed frame strides for 3, 4, 6 and 8 channels.
   A private const-generic helper exposes fixed-stride reads and packed conversion
   to LLVM. Mono/stereo and other channel counts keep their existing paths. Each
   conversion still evaluates `f32::from(sample) / 32768.0` in the same order.

The production diff adds 38 lines and removes four. Fixed-stride specialization
adds source code to eliminate dynamic iterator work; the append path reuses a
helper rather than introducing another conversion layer or temporary buffer.
A standalone optimized assembly check of the fixed three-channel code contains
eight-frame batches with SSE2 `cvtdq2ps`, `mulps` and `movups` instructions.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, 22 cores / 44 logical CPUs;
rustc 1.99.1 (`58cf8f9c3`, LLVM 23.1.1). Cargo release test builds use opt-level 3,
full LTO, the static MSVC runtime and the default target CPU.

Each candidate was benchmarked before editing and after building it. Final
comparisons rebuild four production stages with identical final test sources,
dependency sources and configuration:

| Stage | Production changes |
| --- | --- |
| `before-common` | Original decoder code |
| `mono-slices` | Only the fixed stereo-to-mono slice loop |
| `repeat-slices` | Also the direct repeated-channel append |
| `after-slices` | Also the fixed-stride planar conversion |

The runner verifies archived executable hashes and rotates stage order over six
process runs. The existing kernel harness pins its thread to logical CPU 2,
reads Windows `QueryThreadCycleTime`, warms up once, and reports the median of
eleven measured batches. The table takes the median of six process results.
Throughput is before cycles divided by after cycles. Input preparation and
allocation counting are outside the cycle measurements; all executed conversion
work, including safe output initialization, remains inside them.

Each change is compared with its immediate predecessor stage:

| Kernel / layout | Frames | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: | ---: |
| Packet stereo to mono, address phase 0 | 64 | 111.54 | 62.59 | 1.782x |
| Packet stereo to mono, address phase 0 | 4096 | 4504.47 | 2918.49 | 1.543x |
| Append mono to 3 channels | 4096 | 101774.16 | 22040.78 | 4.618x |
| Append mono to 6 channels | 4096 | 188283.58 | 31204.23 | 6.034x |
| Append mono to 8 channels | 4096 | 247847.89 | 26877.96 | 9.221x |
| Append stereo to 3 channels | 4096 | 65788.35 | 19844.43 | 3.315x |
| Append stereo to 6 channels | 4096 | 99167.93 | 24285.92 | 4.083x |
| Append stereo to 8 channels | 4096 | 130156.31 | 28488.22 | 4.569x |
| Planar, 3 channels | 4096 | 33433.78 | 10416.08 | 3.210x |
| Planar, 4 channels | 4096 | 44958.82 | 15230.95 | 2.952x |
| Planar, 6 channels | 4096 | 66428.56 | 24326.57 | 2.731x |
| Planar, 8 channels | 4096 | 90011.18 | 34851.93 | 2.583x |

Across all targeted layouts and sizes, median throughput improves 1.543-1.872x
for packet mono extraction, 2.876-9.221x for repeated appends, and 2.583-3.210x
for planar conversion. Every targeted workload improves in every process pair.
For example, 4096-frame stereo-to-six-channel append pairs span 3.545-4.281x;
six-channel planar pairs span 2.581-2.915x.

All **3,360 allocation probes have zero allocations, reallocations, frees and
allocated bytes**. The raw data also records frames per cycle and nanoseconds.

Controls fluctuate and include slower medians. Comparing original to final,
64-frame six-channel copying at address phase 128 measures 55.21 -> 64.30 cycles
(0.859x; process ratios 0.573-1.289x). Short mono-to-stereo output at phase 0 is
39.19 -> 42.50 cycles (0.922x; 0.792-1.013x). At 4096 frames, unchanged stereo
planar conversion is 4772.78 -> 4904.57 cycles (0.973x; 0.664-1.188x). These
controls have no consistent per-run benefit; no speedup is claimed for them.
The retained target gains are substantially larger than this observed variation.

Discarded trials include widened-bias PCM rounding, dynamic frame iterators
without fixed strides, and routing packet mono output through the append API.
The rounding trial slowed long stereo conversion, dynamic iterators brought
little benefit, and dispatch changes slowed common controls. Trial executables,
sources and logs remain under `target/audio-decode-kernels`; the committed TSV
contains only the final four stages.

## Correctness and checks

- All 65,536 i16 values are checked against independent scalar expectations for
  planar conversion, including fallback channel counts 5, 7 and 9.
- Append checks cover every i16 value, zero through nine input/output channels,
  empty and partial frames, preserved prefixes, output pointers and zero
  allocator churn with sufficient capacity.
- Differential queue checks cover varying packet sizes, consumed prefixes,
  compaction and growth for 1, 2, 3, 4, 5, 6, 8 and 9 channels at three initial
  capacities.
- All **53 tests pass in each of the four archived release stages**, with 13
  opt-in benchmarks ignored in normal runs. The final debug suite also passes
  all 53 tests. Existing MP3, Ogg, WAV, rounding and fade checks remain covered.
- Clippy completes successfully with existing duplicate-module and type-complexity
  warnings outside the changed implementation. Scoped rustfmt and whitespace
  checks pass.

## Reproduction and evidence

Use isolated checkouts with the final tests, shared benchmark support, dependency
sources, lockfile and configuration held constant. For `before-common`, replace
only `src/resample.rs` with that file from `02183e7c1`. Apply the three changes
above one at a time to produce the intermediate and final stages. Build and
archive each stage before changing its production source:

```powershell
cargo test -p deadlib-audio-decode --release --no-run --message-format=json > target/decode-build.jsonl
python scripts/benchmark_audio_decode.py --capture before-common --build-log target/decode-build.jsonl
```

Repeat capture with the other three stage names, then run:

```powershell
python scripts/benchmark_audio_decode.py --stage before-common --stage mono-slices --stage repeat-slices --stage after-slices --target mapped --target append --target planar --target resampler --runs 6 --output docs/performance/2026-10-26-audio-decode.tsv
cargo test -p deadlib-audio-decode -- --test-threads=1
cargo clippy -p deadlib-audio-decode --all-targets
```

[All measurements](2026-10-26-audio-decode.tsv),
[source/configuration and executable hashes](2026-10-26-audio-decode.manifest.json),
[archived release results](../testing/2026-10-26-audio-decode-release.json), and
[final checks](../testing/2026-10-26-audio-decode-checks.json) record the evidence.
Raw process logs and archived binaries are in `target/audio-decode-kernels`.
