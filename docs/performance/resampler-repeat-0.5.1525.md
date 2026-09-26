# Convert repeated output channels only once

Baseline: `6ce82aafc`.

`write_resampler_output` maps output channel `c` to input channel
`c % source_channels`. Its generic path previously rounded and converted that
floating-point sample to PCM separately for every output channel. For example,
stereo to eight channels converted each source sample four times.

The generic path now converts the first set of channels, then borrows those
PCM values directly from the current output frame to fill repeated channels.
It retains the same channel mapping, rounding, clipping, frame count and
shortest-input limit. No additional buffer, cache, allocation or unsafe code is
needed. Existing specialized mono and stereo paths are unchanged.

This applies to streaming music when sample-rate conversion or pitch-preserving
stretching produces planar samples for a wider output device. It also applies
to the shared resampled SFX conversion. It does not change device channel policy
or improve the ordinary stereo-to-stereo algorithm.

## Behavior checks

The new integration test compares production output against the frozen baseline:

- 4,032 cases compare the complete PCM vector and returned frame count exactly.
- Inputs have 0, 1, 2, 3, 6, 8 or 16 channels; outputs have 0, 1, 2, 3, 4, 6, 8,
  16 or 31 channels. This covers downmixing, matching counts and partial repeats.
- Empty inputs, unequal channel lengths, requests beyond the available input,
  lengths around stereo chunk boundaries, and retained output storage are covered.
- Samples include arbitrary float bit patterns, NaNs, infinities, signed zero,
  minimum normal values and half-step rounding boundaries.
- Allocation checks reuse each output allocation across growing and shrinking
  frame counts, including zero frames.

The existing decoder tests also cover arbitrary float conversion, mapping,
rounding, fade and seek behavior. All 34 decoder library tests and both new
integration tests passed in release mode before and after the edit. Debug
validation passed those 36 tests plus all 44 streaming-audio library tests.
Formatting, diff checks and performance Clippy checks passed; unrelated style
warnings remain.

## Benchmarks

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order for run 2. No builds or other tests from this pass ran during
measurement. Windows `QueryThreadCycleTime` supplies thread cycles.

Both variants use equivalent black-boxed function pointers in one executable
and reuse the same output storage. Seven timing batches follow warmup;
allocation accounting runs separately. Fixture construction and buffer growth
are excluded. Throughput counts output audio frames per second.

The conversion benchmark uses 8,192 calls per batch, normally 1,024 frames per
call (64 for `stereo_six64`). The combined benchmark includes the actual Rubato
44.1-to-48 kHz sinc resampling plus PCM conversion, with 256 calls of 256 output
frames per batch. It resets the resampler before each variant and uses the
existing production interpolation parameters and borrowed planar adapters.

### PCM conversion

| Source → output channels | Frames/call | Before ns/call | After ns/call | Median cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 → 1 | 1024 | 3940.0 | 3890.1 | 1.3% | -2.7 to 2.6% | 1.3% |
| 1 → 2 | 1024 | 4070.0 | 4156.3 | -0.6% | -3.7 to 1.7% | -0.5% |
| 2 → 2 | 1024 | 7463.3 | 7513.8 | -0.7% | -1.5 to 2.0% | -0.7% |
| 6 → 2 | 1024 | 10731.3 | 10850.9 | -0.9% | -1.3 to 0.2% | -0.9% |
| 6 → 6 | 1024 | 29843.9 | 29225.8 | 2.1% | 0.1 to 3.7% | 2.2% |
| 6 → 8 | 1024 | 39189.7 | 31129.2 | 20.6% | 18.9 to 21.3% | 25.9% |
| 2 → 6 | 64 | 1846.0 | 914.6 | 50.4% | 49.8 to 51.8% | 101.8% |
| 2 → 6 | 1024 | 29804.9 | 14669.3 | 50.5% | 50.2 to 52.8% | 102.1% |
| 2 → 8 | 1024 | 39184.1 | 16835.0 | 56.8% | 56.6 to 57.2% | 131.7% |
| 1 → 8 | 1024 | 38898.1 | 12854.7 | 66.7% | 66.6 to 67.9% | 200.8% |
| 3 → 8 | 1024 | 39405.6 | 19881.0 | 49.1% | 48.5 to 49.9% | 96.6% |
| 16 → 16 | 1024 | 78864.6 | 74715.9 | 4.3% | 4.2 to 5.5% | 4.7% |

### Resampling plus PCM conversion

| Source → output channels | Before ns/call | After ns/call | Median cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2 → 2 | 38334.0 | 38484.4 | -0.4% | -0.7 to 0.2% | -0.4% |
| 2 → 6 | 44327.7 | 40118.8 | 8.6% | 8.3 to 9.4% | 9.4% |
| 2 → 8 | 47168.8 | 41553.5 | 11.8% | 10.8 to 11.9% | 13.4% |
| 6 → 6 | 85382.0 | 83732.0 | 2.1% | 2.1 to 3.3% | 2.1% |
| 6 → 8 | 86285.9 | 82352.0 | 3.6% | 1.7 to 4.5% | 3.8% |

Times are medians across runs; percentages are medians of paired ratios, so
they need not equal ratios of displayed time medians. Negative reductions mean
a slower candidate. Every repeated-channel case improved in all three runs.
Matching generic layouts also avoid cycling the source iterator unnecessarily.
The specialized mono/stereo paths and six-to-two control vary slightly in both
directions; these do not establish a performance change.

Every measured operation used zero allocations, reallocations, frees, allocated
bytes and freed bytes before and after. The [raw CSV](resampler-repeat-0.5.1525.csv)
contains all 126 reported samples: 24 unchanged-code control samples from an
executable built before the edit, plus 102 final paired samples. The control
contains conversion cases only; the combined benchmark was added afterward.

These are CPU processing measurements, not callback latency, audible-quality
or whole-game CPU improvements. Both versions produce the same PCM samples.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-audio-decode --lib --test resampler_repeat
cargo test --locked -p deadlib-audio-decode --lib --test resampler_repeat
cargo test --locked -p deadlib-audio --lib
cargo clippy --locked -p deadlib-audio-decode --lib --test resampler_repeat -- -D clippy::perf
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed release integration executable directly:
# <executable> benchmark_resampler_repeat --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Local raw logs and the unchanged baseline executable are retained under
`target/resampler-repeat-pass`.
