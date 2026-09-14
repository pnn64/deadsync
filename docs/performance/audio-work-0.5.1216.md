# Audio mapping and mixing performance, 0.5.1216

Baseline: `a907beb1d` / 0.5.1215. This pass applies the supplied guide's
M-HOTPATH and M-THROUGHPUT recommendations to audio clock lookup, assist-tick
scheduling and SFX mixing. These paths already avoid heap churn once their
storage is initialized. The changes reduce repeated arithmetic without adding
retained memory, allocations, dependencies or unsafe production code.

## Three optimizations

1. **Defer forward-map extrapolation.** `PlaybackPosMap::search` previously
   calculated music positions whenever a nearer endpoint was found, including
   candidates superseded later in the scan. It now remembers the nearest
   segment and endpoint, then extrapolates only if no segment contains the
   query. The endpoint's two fused operations retain their original order.
   This lookup supplies the music clock through `lookup_music_position` in
   `deadsync-audio-stream/src/music_map.rs`.
2. **Divide only for the selected inverse-map segment.** `PlaybackPosMap::invert`
   previously converted seconds to a stream frame for every valid candidate.
   It now divides only for a containing segment or the final nearest segment.
   The assist-tick scheduler uses this conversion to place sounds on the stream.
   Both map changes preserve scan order, first-winner ties, half-open intervals,
   overlapping/out-of-order segments and extrapolation behavior.
3. **Simplify muted SFX arithmetic.** `mix_sfx_samples` now handles positive and
   negative zero gain without normalization or fused multiply-add. Multiplying
   the signed sample by zero and adding that zero still preserves signed-zero
   output and NaN propagation. The complete mixer continues advancing cursors,
   applying scheduled onsets, checking generations and reporting mixed SFX.
   Unity and other audible gains retain their previous arithmetic.

Map lookup remains linear in retained segments; insertion, coalescing and the
80,000-frame backlog limit are unchanged. On this target each old/new map object
is 40 bytes and each segment is 32 bytes. The 256-segment fixture has the same
256-slot deque capacity (8,192 payload bytes), with 65,536 retained frames.
Mixer buffer sizes and active-voice storage are unchanged. There is no claimed
memory-footprint reduction or new cache.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`, release with full LTO.
The old map and mixer are frozen from the baseline commit under
`crates/deadlib-audio-core/tests/audio_work/`, preserving inline attributes.
The harness includes the actual production modules and compares both versions
in the same executable. No target-CPU flags or production allocator changes
were made.

Five independent invocations alternate old/new order (new first on runs 2 and
4). Each invocation uses `tests/support/perf.rs`: three warmups, seven timed
batches, and one separately allocation-counted operation. Tables report medians
of the five invocation medians. Windows `QueryThreadCycleTime` measures calling
thread CPU cycles; all measured operations are synchronous. Throughput derives
from elapsed time. Timings and cycles include the small loop/measurement costs.
The tests assert behavior and allocation budgets, not machine-dependent timing
thresholds. No benchmark ran concurrently with a build or test suite.

- Map operations contain 32 queries. Fixtures have 1, 8, 64 or 256 contiguous
  256-frame segments with alternating rates that prevent coalescing. `tail`
  queries lie inside the final segment; `after` queries start at its end;
  `first` queries hit the first segment of a 256-segment map. Inverse queries
  use the corresponding music seconds. There are 4,096 operations per batch
  for 1/8 segments and 512 for 64/256 segments. Storage construction is excluded.
- Sample operations zero-fill a destination and mix 64, 1,024 or 8,192 samples.
  Both zero signs, unity gain and half gain are measured. There are 8,192
  operations per batch for 64 samples and 2,048 for the larger buffers.
- Complete mixer operations reset active cursors and zero-fill 1,024 output
  samples, then call the actual `mix_active_sfx` with 1, 8 or 32 active muted
  voices. Each voice uses a shared 2,048-sample source, so it remains active.
  Unity/half-gain controls use 8 voices. There are 512 operations per batch.
  Fixtures, sample Arcs and controls are constructed outside measurement.

## Results

Times/cycles are per operation. Throughput units are map queries or interleaved
samples; full-mixer throughput counts output samples multiplied by voice count.
These are computational throughput figures, not audio device sample rates.
Negative "fewer cycles" values indicate higher measured CPU use.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `map/forward/1-tail` | 278.8 -> 285.3 | 612.8 -> 629.8 | -2.8% | 114.774 -> 112.162 |
| `map/inverse/1-tail` | 406.7 -> 392.4 | 892.3 -> 859.1 | 3.7% | 78.679 -> 81.548 |
| `map/forward/1-after` | 465.0 -> 480.2 | 1,020.2 -> 1,053.9 | -3.3% | 68.815 -> 66.646 |
| `map/inverse/1-after` | 515.8 -> 491.2 | 1,131.3 -> 1,077.5 | 4.8% | 62.037 -> 65.145 |
| `map/forward/8-tail` | 2,081.9 -> 1,054.8 | 4,563.1 -> 2,313.6 | 49.3% | 15.371 -> 30.339 |
| `map/inverse/8-tail` | 2,386.6 -> 1,989.2 | 5,234.3 -> 4,361.0 | 16.7% | 13.408 -> 16.087 |
| `map/forward/8-after` | 2,201.6 -> 1,204.5 | 4,825.8 -> 2,638.0 | 45.3% | 14.535 -> 26.568 |
| `map/inverse/8-after` | 2,506.8 -> 2,052.4 | 5,494.4 -> 4,499.2 | 18.1% | 12.765 -> 15.591 |
| `map/forward/64-tail` | 16,093.4 -> 7,569.1 | 35,278.2 -> 16,577.0 | 53.0% | 1.988 -> 4.228 |
| `map/inverse/64-tail` | 18,443.6 -> 14,985.2 | 40,428.3 -> 32,842.7 | 18.8% | 1.735 -> 2.135 |
| `map/forward/64-after` | 16,275.8 -> 7,542.6 | 35,674.8 -> 16,544.4 | 53.6% | 1.966 -> 4.243 |
| `map/inverse/64-after` | 18,934.6 -> 14,869.7 | 41,532.7 -> 32,612.5 | 21.5% | 1.690 -> 2.152 |
| `map/forward/256-tail` | 64,157.2 -> 27,985.4 | 140,618.0 -> 61,355.4 | 56.4% | 0.499 -> 1.143 |
| `map/inverse/256-tail` | 73,205.5 -> 58,243.9 | 160,443.3 -> 127,683.8 | 20.4% | 0.437 -> 0.549 |
| `map/forward/256-after` | 63,963.7 -> 28,055.1 | 140,241.6 -> 61,490.9 | 56.2% | 0.500 -> 1.141 |
| `map/inverse/256-after` | 73,826.0 -> 58,220.5 | 161,834.9 -> 127,644.4 | 21.1% | 0.433 -> 0.550 |
| `map/forward/256-first` | 273.8 -> 273.8 | 604.1 -> 604.1 | 0.0% | 116.862 -> 116.862 |
| `map/inverse/256-first` | 386.3 -> 388.3 | 851.0 -> 857.4 | -0.8% | 82.831 -> 82.414 |
| `samples/muted-64` | 181.5 -> 26.1 | 398.1 -> 56.9 | 85.7% | 352.581 -> 2451.089 |
| `samples/negative-zero-64` | 180.7 -> 25.2 | 394.7 -> 55.6 | 85.9% | 354.273 -> 2536.468 |
| `samples/unity-64` | 25.7 -> 25.5 | 56.7 -> 56.1 | 1.1% | 2487.135 -> 2512.161 |
| `samples/half-64` | 178.2 -> 183.2 | 391.0 -> 401.5 | -2.7% | 359.052 -> 349.316 |
| `samples/muted-1024` | 2,711.7 -> 340.2 | 5,941.7 -> 739.6 | 87.6% | 377.627 -> 3009.690 |
| `samples/negative-zero-1024` | 2,736.0 -> 338.8 | 6,000.5 -> 744.6 | 87.6% | 374.264 -> 3022.268 |
| `samples/unity-1024` | 343.2 -> 335.1 | 754.3 -> 736.3 | 2.4% | 2983.571 -> 3056.182 |
| `samples/half-1024` | 2,711.7 -> 2,712.3 | 5,925.0 -> 5,947.3 | -0.4% | 377.620 -> 377.539 |
| `samples/muted-8192` | 22,206.4 -> 2,992.2 | 48,665.6 -> 6,557.0 | 86.5% | 368.903 -> 2737.796 |
| `samples/negative-zero-8192` | 22,333.9 -> 3,082.6 | 48,961.3 -> 6,734.4 | 86.2% | 366.797 -> 2657.482 |
| `samples/unity-8192` | 3,008.5 -> 3,042.2 | 6,594.0 -> 6,665.9 | -1.1% | 2722.955 -> 2692.799 |
| `samples/half-8192` | 22,308.6 -> 22,251.9 | 48,906.6 -> 48,776.5 | 0.3% | 367.212 -> 368.148 |
| `mixer/muted-1-voices` | 2,742.6 -> 353.7 | 6,023.4 -> 779.8 | 87.1% | 373.371 -> 2895.019 |
| `mixer/muted-8-voices` | 21,318.4 -> 2,162.9 | 46,672.9 -> 4,751.0 | 89.8% | 384.270 -> 3787.524 |
| `mixer/muted-32-voices` | 86,126.4 -> 8,391.4 | 188,818.9 -> 18,407.1 | 90.3% | 380.464 -> 3904.947 |
| `mixer/unity-8-voices` | 2,146.3 -> 2,205.3 | 4,706.4 -> 4,844.0 | -2.9% | 3816.820 -> 3714.732 |
| `mixer/half-8-voices` | 21,441.8 -> 21,801.4 | 47,033.0 -> 47,794.4 | -1.6% | 382.058 -> 375.756 |

Forward lookup across 8-256 segments uses **45.3-56.4% fewer cycles** and inverse
lookup uses **16.7-21.5% fewer cycles**. Complete muted mixing uses **87.1-90.3%
fewer cycles**, with throughput increasing roughly 7.8-10.3 times. The helper
loop alone uses 85.7-87.6% fewer cycles for either sign of zero gain.

The short/early-hit and audible controls show small mixed differences: forward
one-segment lookup uses 2.8-3.3% more cycles (about 0.2-0.5 ns extra per query),
the early inverse hit uses 0.8% more, and complete unity/half-gain mixing uses
2.9%/1.6% more. These controls do not establish an improvement. The benefit is
avoiding arithmetic across multiple map candidates and muted samples. A map
coalesced to one segment does not obtain the longer-map gains.

**All 350 old/new measurements report zero allocations, reallocations, frees,
requested bytes and freed bytes per operation.** Allocation counters describe
heap traffic after fixture setup, not process RSS. Storage equality and warmed
zero-churn behavior also have assertions. The results do not measure audio I/O,
callback deadlines, game FPS or whole-process memory, and CPU/compiler changes
can change the size of the arithmetic savings.

## Behavioral validation and reproduction

Eight new regression tests compare against the frozen old implementation.
Map coverage includes empty/nonfinite inputs, exact endpoints, ties, gaps,
overlaps, reversed order, positive/negative/zero rates, overflowing floating
endpoints, extreme stream offsets, 2,500 randomized inserts with eviction/clear,
wrapped deques and coalescing. Finite results are bit-identical; NaNs are
compared by classification.

Mixing tests exercise all 65,536 `i16` values with both zero signs and audible
controls, signed-zero/subnormal/finite/infinite/NaN destinations, varied and
mismatched slice lengths, scheduling offsets, multiple channel counts, cursor
progress and generation invalidation. The full mixer compares output bits and
retained state. The focused target also runs the 21 tests included from the
production modules (29 passed, one manual benchmark ignored).

Validation passed in debug and release for the focused target. The audio-core,
audio-stream and gameplay library suites passed 843 tests (8 existing manual
benchmarks ignored). The locked application check, performance-focused Clippy
check and formatting checks also passed.

```powershell
cargo test -p deadlib-audio-core --test audio_work --locked -- --test-threads=1
cargo test -p deadlib-audio-core --release --test audio_work --locked -- --test-threads=1
cargo test -p deadlib-audio-core -p deadsync-audio-stream -p deadsync-gameplay --lib --locked -- --test-threads=1
cargo check -p deadsync --locked
cargo clippy -p deadlib-audio-core --lib --test audio_work --locked --no-deps -- -A clippy::all -W clippy::perf
cargo test -p deadlib-audio-core --release --test audio_work --locked -- --ignored --exact benchmark_audio_work --nocapture --test-threads=1
```

Repeat the benchmark five times, setting `$env:DEADSYNC_PERF_REVERSE = '1'` on
runs 2 and 4 and removing that environment variable on runs 1, 3 and 5. The
directly executable test binary gives the same harness without Cargo startup.
The [raw CSV](audio-work-0.5.1216.csv) contains all 350 measurements, including
each invocation's elapsed range and heap counters.
