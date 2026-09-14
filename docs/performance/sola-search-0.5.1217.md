# SOLA search performance, 0.5.1217

Baseline: `d78b945c0` / 0.5.1216. This pass follows the supplied guide's
M-HOTPATH and M-THROUGHPUT recommendations: benchmark the decoder's correlation
search, stop work that cannot improve its result, and share work when its cost
is lower than recomputation. SOLA powers pitch-preserving music-rate changes
on the decoder worker through `MusicStages`.

## Three optimizations

1. **Stop mono correlation at the first perfect match.** The previous search
   stopped for a perfect first candidate, but continued scanning if a later
   candidate scored zero. Absolute-difference scores cannot be negative. The
   search now returns as soon as an improving candidate reaches zero, preserving
   the earliest winner and the inclusive final-candidate behavior.
2. **Stop stereo correlation once both channels have perfect matches.** Each
   channel still chooses its own offset. An improving zero score ends the
   search only when the other channel already has its earliest zero score.
   Channel activity uses the current best score directly, removing redundant
   checks against whether the initial candidate was perfect.
3. **Share correlation for identical stereo channels.** After the existing
   perfect-first exit, equal first scores allow a comparison of both search
   slices and both reference slices. If they match, one mono search supplies
   both offsets. Different first scores bypass these comparisons. Silence
   retains its existing immediate exit, and NaNs stay on the independent
   channel path. Signed-zero differences have identical absolute-difference
   scores. The shared path repeats the initial scalar score before continuing;
   it introduces no cached state or invalidation rules.

The score accumulation order, match metric, window/tolerance sizes, rate
accumulator, input compaction, crossfade arithmetic and EOF behavior are
unchanged. The object layout and retained buffers are unchanged. These paths
already had zero heap churn once warmed; this pass targets CPU work, with no
new memory, dependency, production allocator or unsafe-code changes.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`, release with full LTO.
No additional target-CPU flags were used. The entire old SOLA implementation,
including inline attributes, is frozen in
`crates/deadlib-audio/tests/perf/sola_work_baseline.rs`. The new harness is a
test module of the production stretcher, so it calls the actual private
functions and exercises complete old/new stretcher instances in one executable.

Five independent invocations alternate variant order (new first on runs 2 and
4). `tests/support/perf.rs` performs three warmups, seven timed batches and a
separately allocation-counted operation. Results are medians of the five
invocation medians. Windows `QueryThreadCycleTime` reports the synchronous
calling thread's CPU cycles. Throughput derives from wall time; timings include
loop overhead. No build or other test suite ran alongside the benchmarks.

Search fixtures contain 360 reference samples and 720 search samples, allowing
offsets 0 through 360. Mono exact matches lie at 0, 12, 180 or 360. Independent
stereo uses the same left positions and right positions 7, 19, 187 or 360.
`no-exact` uses distinct deterministic input. `dual-mono` uses equal samples in
separate buffers, with exact offsets 0/12 or no exact match. First-hit cases
use 4,096 operations per batch, early hits 1,024, and longer searches 128.
Each search operation produces one mono offset or one stereo offset pair.

Complete stream operations reset a warmed stretcher at 48 kHz and rate 1.2,
push 12,000 input frames, finish, and drain through 1,024-frame pulls. Fixtures
use one/two channels, periodic or aperiodic signals, plus stereo copies of
mono input. Periodic channels have periods 61 and 63 frames. There are 64
operations per batch. Input generation, initialization and output capacity
allocation occur outside measurement. Throughput counts input samples, not
device output rate.

## Results

Times and cycles are per operation. Throughput is millions of searches/s or
input samples/s. Negative "fewer cycles" values indicate greater measured CPU
use. The [raw CSV](sola-search-0.5.1217.csv) retains all invocation ranges and
heap counters.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `search/mono/first` | 373.3 -> 374.5 | 819.0 -> 817.2 | 0.2% | 2.679 -> 2.670 |
| `search/stereo/first` | 4,411.1 -> 3,370.7 | 9,639.2 -> 7,357.2 | 23.7% | 0.227 -> 0.297 |
| `search/mono/early` | 7,323.2 -> 5,117.3 | 15,994.1 -> 11,224.3 | 29.8% | 0.137 -> 0.195 |
| `search/stereo/early` | 12,221.5 -> 9,736.5 | 26,791.6 -> 21,244.6 | 20.7% | 0.082 -> 0.103 |
| `search/mono/middle` | 73,057.8 -> 71,796.1 | 160,056.7 -> 157,357.5 | 1.7% | 0.014 -> 0.014 |
| `search/stereo/middle` | 101,969.5 -> 100,335.2 | 223,428.7 -> 219,841.3 | 1.6% | 0.010 -> 0.010 |
| `search/mono/last` | 137,437.5 -> 137,801.6 | 301,310.0 -> 302,109.2 | -0.3% | 0.007 -> 0.007 |
| `search/stereo/last` | 201,191.4 -> 192,404.7 | 438,787.4 -> 418,722.0 | 4.6% | 0.005 -> 0.005 |
| `search/mono/no-exact` | 140,328.9 -> 142,208.6 | 306,770.1 -> 311,796.3 | -1.6% | 0.007 -> 0.007 |
| `search/stereo/no-exact` | 195,879.7 -> 195,111.7 | 429,141.4 -> 427,766.0 | 0.3% | 0.005 -> 0.005 |
| `dual-mono/first` | 571.7 -> 560.7 | 1,254.0 -> 1,227.4 | 2.1% | 1.749 -> 1.784 |
| `dual-mono/early` | 9,194.7 -> 6,722.6 | 20,080.8 -> 14,657.6 | 27.0% | 0.109 -> 0.149 |
| `dual-mono/no-exact` | 189,133.6 -> 144,518.0 | 413,867.3 -> 314,809.3 | 23.9% | 0.005 -> 0.007 |
| `stream/periodic-1ch` | 68,437.5 -> 56,742.2 | 149,078.2 -> 124,442.8 | 16.5% | 175.342 -> 211.483 |
| `stream/aperiodic-1ch` | 62,020.3 -> 61,807.8 | 135,033.7 -> 135,376.6 | -0.3% | 193.485 -> 194.150 |
| `stream/periodic-2ch` | 134,056.2 -> 123,212.5 | 293,752.8 -> 268,589.1 | 8.6% | 179.029 -> 194.785 |
| `stream/aperiodic-2ch` | 142,431.2 -> 134,940.6 | 312,187.3 -> 295,800.2 | 5.2% | 168.502 -> 177.856 |
| `stream/dual-periodic-2ch` | 104,159.4 -> 87,567.2 | 227,021.3 -> 191,908.2 | 15.5% | 230.416 -> 274.075 |
| `stream/dual-aperiodic-2ch` | 112,254.7 -> 92,853.1 | 245,689.1 -> 203,459.3 | 17.2% | 213.800 -> 258.473 |

The early mono fixture uses **29.8% fewer cycles**; independent stereo with
near-start exact hits uses **20.7-23.7% fewer**. Identical-channel stereo without
an exact match uses **23.9% fewer cycles**, isolating the benefit of sharing its
search. With an early perfect match, the combined identical-channel and early
exit changes save **27.0%**.

Complete periodic mono/stereo operations use **16.5%/8.6% fewer cycles**.
Identical-channel stream operations save **15.5-17.2%**, and independent
aperiodic stereo saves **5.2%**. Mono aperiodic streaming is essentially flat
(**0.3% more cycles**). The mono final-candidate and no-exact controls measure
**0.3%/1.6% more cycles**, while middle matches improve only 1.6-1.7%. These
controls do not establish a meaningful speedup; perfect matches found late
leave little remaining work to avoid.

**All 190 old/new measurements report zero allocations, reallocations, frees,
requested bytes and freed bytes per warmed operation.** These counters measure
heap traffic after fixture setup, not process RSS. There is no memory-footprint
reduction claim.

These synthetic fixtures isolate correlation behavior and complete in-memory
stretch operations. They do not establish game-FPS gains, callback deadline
improvements or whole-process memory savings. Gains depend on input periodicity,
match location, channel similarity, compiler and CPU. Worst-case search remains
linear in candidate count times reference length.

## Validation and reproduction

Six new tests compare the frozen implementation with production code:

- Exact match positions, first-winner ties, empty/short references and the final
  candidate; independent stereo hits at different offsets.
- 700 deterministic randomized search cases with NaNs, infinities, maximum
  floats and subnormals, plus identical and nearly identical stereo buffers.
- Signed-zero, NaN and infinite samples across the identical-channel decision.
- Complete output and per-pull frame counts, buffered source frames and trailing
  rate across 8/44.1/48 kHz, 1/2/6 channels, packet boundaries, zero-size pulls,
  rate changes, resets, independent/identical channels and EOF draining.
- Allocation-free mono, independent stereo and identical-channel searches.

Finite output samples are bit-identical; NaNs are compared by classification.
Old/new stretcher object sizes are asserted equal. The six comparisons pass in
debug and release. Broader audio/stream validation passes 47 tests, including
the decoder integration test for cuts, fades, channels and timing. Four manual
benchmarks/captures are ignored in the normal suite. The locked application
check, performance-focused Clippy and formatting checks pass. Clippy reports
only an existing unused helper in the shared benchmark support module.

```powershell
cargo test -p deadlib-audio -p deadsync-audio-stream --locked -- --test-threads=1
cargo test -p deadlib-audio --release --lib --locked sola_work -- --test-threads=1
cargo check -p deadsync --locked
cargo clippy -p deadlib-audio --lib --tests --locked --no-deps -- -A clippy::all -W clippy::perf
cargo test -p deadlib-audio --release --lib --locked benchmark_sola_work -- --ignored --nocapture --test-threads=1
```

Repeat the benchmark five times, setting `$env:DEADSYNC_PERF_REVERSE = '1'` for
runs 2 and 4 and removing it for runs 1, 3 and 5. The release test executable can
also be called directly with the exact filter
`stream::stretch::sola_work::benchmark_sola_work`.
