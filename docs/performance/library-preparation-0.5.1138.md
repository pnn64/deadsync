# Library grouping and crossover preparation, 0.5.1138

Baseline: `5aab5d460` (0.5.1137). This pass applies the local Rust guide's
M-HOTPATH and M-INITIAL-CAPACITY guidance to music selection and crossover
preparation. The operations return owned collections, so nonempty results still
need allocations; the changes remove repeated work and unnecessary buffer growth.

## Changes

1. BPM grouping caches parsed BPM keys once per song. Compact 32-bit source
   positions keep the scratch buffer at eight bytes per song and act as the
   final tie-breaker, preserving stable song identity. The sorted keys are reused
   to size and fill BPM groups. Inputs exceeding the index range retain the
   original implementation; ordinary small and untagged libraries use the cache.
2. Title and artist grouping count alphabetic buckets before filling them, retaining
   a one-byte bucket index per song so each prefix is classified once. Each
   output bucket starts at its final size, avoiding repeated allocations and
   retaining no unused capacity. Comparators and stable ordering are unchanged.
3. Crossover preparation merges ordered note heads with sorted hold tails. Only
   tails need temporary storage, and output buffers use the distinct row count.
   The original cell sorting remains available for unordered inputs and charts
   shorter than 64 notes. Source-note order preserves collision precedence and
   the beat chosen for each row.

The grouping functions feed the music selection screen. Crossover rows feed both
in-game crossover cues and evaluation. No public signatures or dependencies change.

## Measurement

Machine: Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`.

Both previous implementations and the production implementations run in the same
release test executables, with full LTO. References are frozen from the baseline
commit. Fixtures are constructed outside the measurement. Grouping measurements
include copying the consumed Arc vector and dropping the result; crossover
measurements include the complete row-building operation and dropping its outputs.

The existing benchmark helper takes seven timing batches after warm-up. Three
independent invocations alternate old/new order, with reverse order in the middle
invocation. Tables report medians of the three invocation medians. Windows
QueryThreadCycleTime measures calling-thread cycles. Allocation counting runs
separately from timing and includes frees and reallocations. Requested bytes
include allocation/reallocation traffic; they are not process RSS or peak memory.
These are operation benchmarks, not a whole-game FPS measurement.

## Results

All arrows below show baseline -> new. Timing uses microseconds per complete
operation; throughput uses millions of songs or input notes per second.

| Case | us/op | Thread cycles/op | M items/s |
| --- | ---: | ---: | ---: |
| bpm small | 21.86 -> 6.20 | 47,590 -> 13,540 | 1.464 -> 5.158 |
| bpm medium | 3,539.22 -> 685.57 | 7,745,582 -> 1,502,224 | 0.579 -> 2.987 |
| bpm large | 18,240.41 -> 3,214.89 | 39,923,792 -> 7,030,372 | 0.449 -> 2.548 |
| bpm untagged | 1,238.59 -> 535.30 | 2,712,951 -> 1,172,185 | 1.653 -> 3.826 |
| bpm sparse tag | 1,241.38 -> 537.91 | 2,715,973 -> 1,178,921 | 1.650 -> 3.807 |
| bpm skewed | 3,663.55 -> 683.12 | 8,024,629 -> 1,496,321 | 0.559 -> 2.998 |
| title small | 4.68 -> 4.56 | 10,257 -> 9,998 | 6.842 -> 7.014 |
| title medium | 798.53 -> 794.38 | 1,745,207 -> 1,740,964 | 2.565 -> 2.578 |
| title large | 4,393.81 -> 4,329.80 | 9,619,028 -> 9,478,000 | 1.864 -> 1.892 |
| title untagged | 811.81 -> 806.16 | 1,779,360 -> 1,754,707 | 2.523 -> 2.540 |
| title sparse tag | 823.99 -> 787.72 | 1,792,464 -> 1,726,296 | 2.485 -> 2.600 |
| title skewed | 1,328.67 -> 1,341.77 | 2,911,294 -> 2,940,696 | 1.541 -> 1.526 |
| artist small | 5.97 -> 5.84 | 13,098 -> 12,809 | 5.357 -> 5.477 |
| artist medium | 835.38 -> 821.74 | 1,828,970 -> 1,797,366 | 2.452 -> 2.492 |
| artist large | 4,319.30 -> 4,249.99 | 9,450,672 -> 9,307,918 | 1.897 -> 1.928 |
| artist untagged | 836.38 -> 848.22 | 1,831,998 -> 1,844,832 | 2.449 -> 2.414 |
| artist sparse tag | 912.51 -> 850.27 | 1,998,647 -> 1,862,293 | 2.244 -> 2.409 |
| artist skewed | 1,564.77 -> 1,538.27 | 3,427,482 -> 3,349,501 | 1.309 -> 1.331 |
| crossover small | 0.92 -> 0.86 | 1,983 -> 1,849 | 34.686 -> 37.313 |
| crossover taps | 152.29 -> 44.01 | 332,328 -> 96,288 | 26.896 -> 93.061 |
| crossover chords | 62.14 -> 38.95 | 135,514 -> 85,398 | 65.921 -> 105.160 |
| crossover mixed | 186.91 -> 164.03 | 408,411 -> 359,596 | 21.915 -> 24.972 |
| crossover holds | 352.98 -> 299.03 | 773,273 -> 652,357 | 11.604 -> 13.698 |
| crossover large | 1,046.85 -> 686.82 | 2,291,798 -> 1,488,092 | 15.651 -> 23.855 |
| crossover unordered | 200.35 -> 195.87 | 438,990 -> 429,308 | 20.445 -> 20.912 |

`small` means 32 items, library `medium` 2,048 songs and `large` 8,192.
The untagged, sparse-tag and skewed libraries have 2,048 songs; sparse-tag
has one explicit BPM range. Crossover fixtures use 4,096 notes except `small`
(32) and `large` (16,384). Taps have one note per row, chords have four,
mixed/large have two with every seventh note a hold, and holds have every
note as a hold. Unordered reverses the mixed fixture. See fixture source
for exact keys, duplicate distribution and hold-tail positions.

| Case | Allocations + reallocations/op | Requested bytes/op |
| --- | ---: | ---: |
| bpm small | 4 + 3 -> 5 + 0 | 960 -> 864 |
| bpm medium | 32 + 129 -> 32 + 0 | 74,656 -> 50,544 |
| bpm large | 32 + 204 -> 32 + 0 | 362,400 -> 198,000 |
| bpm untagged | 32 + 129 -> 32 + 0 | 74,656 -> 50,544 |
| bpm sparse tag | 32 + 129 -> 32 + 0 | 74,656 -> 50,544 |
| bpm skewed | 32 + 129 -> 32 + 0 | 74,656 -> 50,544 |
| title small | 11 + 2 -> 12 + 0 | 1,104 -> 976 |
| title medium | 30 + 118 -> 31 + 0 | 53,696 -> 36,160 |
| title large | 32 + 174 -> 33 + 0 | 226,056 -> 153,224 |
| title skewed | 5 + 15 -> 6 + 0 | 68,392 -> 50,024 |
| artist small | 9 + 3 -> 10 + 0 | 1,072 -> 880 |
| artist medium | 30 + 122 -> 31 + 0 | 60,864 -> 36,160 |
| artist large | 32 + 178 -> 33 + 0 | 254,496 -> 152,992 |
| artist skewed | 4 + 9 -> 5 + 0 | 65,552 -> 51,248 |
| crossover small | 4 + 0 -> 4 + 0 | 2,640 -> 2,640 |
| crossover taps | 4 + 0 -> 3 + 0 | 327,680 -> 65,536 |
| crossover chords | 4 + 0 -> 3 + 0 | 327,680 -> 16,384 |
| crossover mixed | 4 + 0 -> 4 + 0 | 337,056 -> 51,568 |
| crossover holds | 4 + 0 -> 4 + 0 | 393,216 -> 164,048 |
| crossover large | 4 + 0 -> 4 + 0 | 1,348,176 -> 206,016 |
| crossover unordered | 4 + 0 -> 4 + 0 | 337,056 -> 337,056 |

Title/artist allocation counts for untagged and sparse-tag fixtures match
their medium fixture. Every measured allocation is freed; freed-byte traffic
equals requested-byte traffic. The extra one-byte-per-song alpha index buffer
adds one allocation while eliminating all bucket reallocations.

BPM grouping saves about 57-82% of median cycles across the fixtures. The
2,048/8,192-song tagged libraries deliver 5.2/5.7 times the throughput and
request 32/45% fewer bytes. Even the 32-song fixture removes vector growth and
requests 10% fewer bytes. Compact cached positions avoid a larger usize index
buffer; the cached path still needs temporary memory, so this is not a claim
about measured peak RSS.

Title/artist grouping primarily improves allocation churn. Distributed medium
and large libraries request about 32-41% fewer bytes, with all 118-178 bucket
reallocations removed. The extra bucket-index allocation is included in these
numbers. Their median cycle improvements are only about 0-2%; the skewed title
and untagged artist controls instead use about 1% more cycles. Timing varies
between runs, so a broad CPU speedup is not claimed for this change.

Ordered crossover fixtures use about 12-71% fewer cycles and request 58-95%
fewer bytes. Tap-only output uses three allocations instead of four. The
16,384-note mixed chart improves throughput from 15.7 to 23.9 million notes/s.
Tap-only timing had substantial variation: the three old/new operation medians
were 153.9/44.0, 84.9/89.1, and 152.3/43.9 us. The reverse-order invocation used
3.5% more cycles, despite the same 80% byte reduction; the 71% overall median
is not a guaranteed per-run speedup. The large mixed fixture saved 34-43% of
paired cycle counts in all three invocations. Small and unordered inputs use
the original sorting path; their small timing differences are not treated as
an independent optimization gain.

## Validation

- Baseline: 761 gameplay and 185 simfile unit tests passed.
- Six new behavior/allocation tests compare group identity and order, row cells,
  row indices, and exact floating-point beat bits against the prior algorithms.
- Grouping cases cover empty/tiny/large lists, equal keys in distinct Arcs,
  permutations, transliteration, whitespace/Unicode prefixes, tagged/untagged
  BPMs, malformed tags and NaNs, and skewed/distributed alpha buckets.
- Crossover cases cover 0/4/8/10/16 lanes, cutoff boundaries, clipped/reversed
  ranges, ordered/unordered inputs, chords, every note type, fake flags,
  overlapping/backward/zero-length hold tails, collisions, signed zero and NaNs.
- Allocation budgets reject any reallocation in optimized grouping or ordered
  crossover construction. Ordered tap charts of at least 64 notes need only
  three output allocations; hold charts add one exact tail allocation.
  Filtered-out large charts allocate
  nothing. Alpha buckets retain exactly their final length.
- Final unit suites: 764 gameplay and 188 simfile tests passed in both debug
  and release, zero failures; five manual benchmarks are ignored.
- `cargo check --workspace --bins` passed.
- All-target Clippy with `-D clippy::perf` passed for both changed crates.
  Existing unrelated warnings remain; none refer to the changed routines or
  new benchmark modules.
- Changed production sections and the new benchmark files pass Rustfmt;
  unrelated existing formatting is preserved.
- Frozen reference bodies were checked against `5aab5d460`.
- Cargo.toml and Cargo.lock increment the patch exactly once:
  0.5.1137 -> 0.5.1138.

## Reproduction

```powershell
cargo test -p deadsync-gameplay -p deadsync-simfile --lib -- --test-threads=1
cargo test --release -p deadsync-gameplay -p deadsync-simfile --lib -- --test-threads=1
cargo test --release -p deadsync-gameplay --lib crossover_rows_bench -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-simfile --lib library_sort_bench -- --ignored --nocapture --test-threads=1
cargo check --workspace --bins
cargo clippy -p deadsync-gameplay -p deadsync-simfile --all-targets -- -D clippy::perf
```

Run the two manual benchmarks three times, setting `DEADSYNC_PERF_REVERSE=1` only
for the middle invocation. Do not run other builds/tests during timing. Raw logs,
comparison script, and machine-readable results are retained locally in the
ignored `target/library-perf/` directory.
