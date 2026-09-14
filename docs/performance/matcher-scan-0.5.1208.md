# Matcher scanning - 0.5.1208

Baseline: `74650590ae323f6e7be0d9bccef869c61c422ab9` / 0.5.1207.
This pass follows `rust-performance.md`'s `M-HOTPATH` guidance: measure CPU
and allocation costs in shared search loops, and remove repeated work.
The shared matcher is called for song titles, transliterations, pack names
and settings candidates. Its ordinary scoring path was already allocation-free;
these changes primarily reduce CPU work without adding retained caches.

1. **ASCII scoring:** retain the original scalar scorer for labels of up to
   32 bytes. For longer labels, score a contiguous prefix with a simpler loop,
   then find each remaining character directly. Short gaps use a scalar byte search;
   spans of at least 32 bytes use `memchr2` to find either letter case. Only
   matched positions need gap and boundary scoring. The theme now declares
   `memchr 2.8.3`, which was already present in the workspace lockfile.
2. **Unicode scoring:** after consuming the query, count the remaining UTF-8
   scalars with `Chars::count` instead of decoding them through the scoring
   loop. This preserves the original total-character length penalty.
3. **Typo fallback:** reduce the remaining edit budget after finding a better
   word, and stop at distance one. Fallback runs only after whole-label
   subsequence matching fails, so an exact matching word is impossible at
   that point. Later words are still examined when they could improve a
   distance of two or more. This skips redundant folding and dynamic
   programming, including unnecessary long-Unicode scratch allocations.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), repository release profile (opt-level 3, LTO).
`matcher_scan_perf` compiles the production matcher alongside the complete
parent matcher, excluding its test module. The frozen baseline was checked
against the parent, allowing only its provenance comment, line endings and
trailing whitespace. Both sides call the complete `best_match_score` API,
including alias handling and typo fallback.

Three warmups precede seven timing samples per measurement. Five complete
runs alternate old-first/new-first order; tables report medians of the five
per-run medians. Builds and regression checks finish before these runs.
CPU cycles use Windows `QueryThreadCycleTime` for the calling thread.
Allocator counting is disabled during timing; one separate operation counts
allocations, reallocations, frees, requested bytes and freed bytes. Prepared
queries and input fixtures are outside timing and allocation tracking.
Inputs and outputs pass through black boxes.

Each single-candidate sample uses 10,000 iterations, or 2,000 when its label
exceeds 1,000 bytes. Each catalog sample uses 32 scans of 4,096 titles.
Throughput counts candidates, while catalog ns/op and cycles/op cover a
whole scan. These are synthetic matcher workloads, not end-to-end search
latency, frame rates or measured player-query frequencies. Sorting, rendering,
query preparation and title normalization are outside their scope.

The host is shared and background load is not controlled. Small timings vary
between runs; the [CSV](matcher-scan-0.5.1208.csv) includes all 440 measurements
and seven-sample wall-time ranges. Thread cycles are not retired instructions
or cache misses. Allocation bytes measure churn, not RSS, peak live memory
or allocator metadata. Results on other CPUs may differ.

## Results

The scattered catalog query uses **43.8% fewer cycles**.
Matching at the start of a 128-scalar Unicode title uses
**70.5% fewer cycles**. The eight-word typo case uses
**80.2% fewer cycles**. Ordinary matching remains at zero
allocations, reallocations and frees on both sides. The long Unicode typo
stress case reduces allocation/free calls from **16 to 1** and requested/freed
bytes from **8,192 to 512**, a **93.75% reduction in allocation churn**.

29 of 44 workloads have lower median thread cycles. The changes do not improve every input.
Control/scaling cases with higher cycle medians are listed explicitly below:

- `ascii_sparse_16`: 42.7 -> 46.8 ns/op; 10.9% more cycles.
- `ascii_dense_16`: 28.2 -> 33.9 ns/op; 20.2% more cycles.
- `ascii_prefix_31`: 36.3 -> 39.4 ns/op; 7.1% more cycles.
- `ascii_sparse_31`: 65.5 -> 67.1 ns/op; 2.5% more cycles.
- `ascii_dense_31`: 44.0 -> 43.9 ns/op; 0.5% more cycles.
- `ascii_dense_32`: 35.1 -> 40.2 ns/op; 14.2% more cycles.
- `ascii_prefix_64`: 30.1 -> 38.5 ns/op; 28.0% more cycles.
- `ascii_dense_64`: 38.1 -> 44.1 ns/op; 16.0% more cycles.
- `ascii_prefix_512`: 41.7 -> 46.6 ns/op; 11.8% more cycles.
- `ascii_dense_512`: 50.5 -> 48.8 ns/op; 1.1% more cycles.
- `unicode_prefix_8`: 66.1 -> 68.5 ns/op; 3.7% more cycles.
- `unicode_late_8`: 230.1 -> 234.9 ns/op; 2.0% more cycles.
- `unicode_late_32`: 725.2 -> 755.7 ns/op; 4.1% more cycles.
- `unicode_late_4096`: 76,158.6 -> 87,625.8 ns/op; 15.1% more cycles.

All measured cases are included here; negative cycle reductions mean a
regression. The large gains apply to skipped spans and redundant typo work,
while short/dense matches have little work to remove.

| Workload | Old ns/op | New ns/op | Thread cycles old/new | Cycle reduction | Throughput old/new (M candidates/s) |
|---|---:|---:|---:|---:|---:|
| `ascii_prefix_16` | 48.6 | 44.7 | 106.9 / 98.3 | 8.0% | 20.568 / 22.361 |
| `ascii_sparse_16` | 42.7 | 46.8 | 92.7 / 102.8 | -10.9% | 23.436 / 21.390 |
| `ascii_missing_16` | 88.1 | 75.1 | 193.4 / 164.7 | 14.8% | 11.355 / 13.310 |
| `ascii_dense_16` | 28.2 | 33.9 | 62.0 / 74.5 | -20.2% | 35.511 / 29.542 |
| `ascii_prefix_31` | 36.3 | 39.4 | 79.9 / 85.6 | -7.1% | 27.518 / 25.394 |
| `ascii_sparse_31` | 65.5 | 67.1 | 143.9 / 147.5 | -2.5% | 15.265 / 14.901 |
| `ascii_missing_31` | 121.6 | 113.1 | 266.5 / 239.9 | 10.0% | 8.226 / 8.839 |
| `ascii_dense_31` | 44.0 | 43.9 | 95.9 / 96.4 | -0.5% | 22.722 / 22.789 |
| `ascii_prefix_32` | 40.1 | 40.1 | 88.1 / 88.1 | 0.0% | 24.944 / 24.944 |
| `ascii_sparse_32` | 65.0 | 61.4 | 142.7 / 134.9 | 5.5% | 15.394 / 16.289 |
| `ascii_missing_32` | 138.9 | 117.4 | 305.1 / 257.7 | 15.5% | 7.198 / 8.521 |
| `ascii_dense_32` | 35.1 | 40.2 | 77.2 / 88.2 | -14.2% | 28.498 / 24.845 |
| `ascii_prefix_64` | 30.1 | 38.5 | 66.1 / 84.6 | -28.0% | 33.256 / 26.001 |
| `ascii_sparse_64` | 107.8 | 53.1 | 236.6 / 116.8 | 50.6% | 9.281 / 18.832 |
| `ascii_missing_64` | 259.3 | 133.4 | 568.6 / 286.7 | 49.6% | 3.857 / 7.494 |
| `ascii_dense_64` | 38.1 | 44.1 | 83.8 / 97.2 | -16.0% | 26.261 / 22.660 |
| `ascii_prefix_128` | 42.9 | 28.2 | 94.2 / 62.1 | 34.1% | 23.337 / 35.436 |
| `ascii_sparse_128` | 216.3 | 62.8 | 474.4 / 137.9 | 70.9% | 4.624 / 15.931 |
| `ascii_missing_128` | 434.5 | 216.7 | 953.1 / 475.0 | 50.2% | 2.301 / 4.615 |
| `ascii_dense_128` | 47.4 | 40.4 | 104.1 / 88.9 | 14.6% | 21.084 / 24.734 |
| `ascii_prefix_512` | 41.7 | 46.6 | 91.6 / 102.4 | -11.8% | 24.004 / 21.464 |
| `ascii_sparse_512` | 724.7 | 74.6 | 1,589.2 / 163.4 | 89.7% | 1.380 / 13.408 |
| `ascii_missing_512` | 1,858.5 | 683.7 | 4,075.7 / 1,499.8 | 63.2% | 0.538 / 1.463 |
| `ascii_dense_512` | 50.5 | 48.8 | 106.2 / 107.4 | -1.1% | 19.802 / 20.500 |
| `unicode_prefix_8` | 66.1 | 68.5 | 145.2 / 150.5 | -3.7% | 15.129 / 14.599 |
| `unicode_late_8` | 230.1 | 234.9 | 504.4 / 514.6 | -2.0% | 4.346 / 4.257 |
| `unicode_prefix_32` | 105.6 | 54.4 | 231.5 / 119.6 | 48.3% | 9.470 / 18.379 |
| `unicode_late_32` | 725.2 | 755.7 | 1,590.8 / 1,656.2 | -4.1% | 1.379 / 1.323 |
| `unicode_prefix_128` | 269.5 | 79.3 | 590.7 / 174.3 | 70.5% | 3.710 / 12.612 |
| `unicode_late_128` | 3,036.6 | 2,807.2 | 6,656.3 / 6,155.6 | 7.5% | 0.329 / 0.356 |
| `unicode_prefix_4096` | 7,958.8 | 825.4 | 17,431.7 / 1,812.5 | 89.6% | 0.126 / 1.212 |
| `unicode_late_4096` | 76,158.6 | 87,625.8 | 166,710.7 / 191,893.9 | -15.1% | 0.013 / 0.011 |
| `typo_words_2` | 209.1 | 135.9 | 458.7 / 298.0 | 35.0% | 4.783 / 7.360 |
| `typo_improving_2` | 454.0 | 423.9 | 995.8 / 929.9 | 6.6% | 2.203 / 2.359 |
| `typo_words_8` | 881.3 | 174.6 | 1,932.6 / 382.9 | 80.2% | 1.135 / 5.728 |
| `typo_improving_8` | 1,357.4 | 1,131.4 | 2,976.0 / 2,474.4 | 16.9% | 0.737 / 0.884 |
| `typo_words_32` | 2,665.9 | 221.0 | 5,846.0 / 485.0 | 91.7% | 0.375 / 4.524 |
| `typo_improving_32` | 4,818.5 | 4,053.5 | 10,566.3 / 8,876.3 | 16.0% | 0.208 / 0.247 |
| `typo_single_control` | 138.3 | 118.5 | 303.2 / 258.6 | 14.7% | 7.232 / 8.437 |
| `alias_control` | 56.8 | 55.2 | 124.3 / 120.9 | 2.7% | 17.599 / 18.106 |
| `typo_unicode_spill` | 71,020.8 | 33,817.7 | 155,722.5 / 74,149.4 | 52.4% | 0.014 / 0.030 |
| `catalog_sparse` | 286,946.9 | 162,859.4 | 629,093.9 / 353,642.0 | 43.8% | 14.274 / 25.151 |
| `catalog_prefix` | 155,143.8 | 138,443.8 | 340,032.9 / 303,664.5 | 10.7% | 26.401 / 29.586 |
| `catalog_missing` | 1,379,984.4 | 1,276,053.1 | 3,025,313.6 / 2,793,158.0 | 7.7% | 2.968 / 3.210 |

| Workload | Allocations old/new | Reallocations old/new | Frees old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|---:|
| All 43 ordinary/control workloads | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 |
| `typo_unicode_spill` | 16 / 1 | 0 / 0 | 16 / 1 | 8,192 / 512 |

The Unicode spill fixture has a 97-scalar query, beyond song search's
80-character input limit, and 16 nearly matching 97-scalar words. It is a
shared-matcher stress case, not an expected song-search allocation saving.
One scratch allocation remains; the change avoids repeatedly allocating it.
Scratch was freed between words before this change too, so the reduction in
cumulative allocated bytes does not imply the same reduction in peak memory.

### Fixtures

- `ascii_prefix_N`: query `speed`, label `Speed ` followed by padding to
  exactly N bytes. `ascii_dense_N`: query `aaaaaa` in N `a` bytes.
- `ascii_sparse_N`: query `spr`, label `S` + N/2 `a` bytes + `p` + N/2 `b`
  bytes + `R`; its actual length is `2 * floor(N / 2) + 3`, so the 16 and
  31 names describe 19- and 33-byte labels. `ascii_missing_N`: `xyz` in N `a`
  bytes, including the subsequent failed typo fallback.
- `unicode_prefix_N`: query U+65E5 in N repetitions of U+65E5.
  `unicode_late_N`: N repetitions of U+672C followed by U+65E5, so its match
  is at the end and the label has N+1 scalars. These cover both an early
  match with a suffix to count and a control with no suffix to skip.
- `typo_words_N`: `perspextive` against N space-separated `Perspective`
  words. `typo_improving_N`: `abcxefgy` against N copies of `abcuefgw`,
  followed by `abcxefgw`; the later word improves distance two to one.
- `typo_single_control`: `prespective` / `Perspective`. `alias_control`:
  `arrows` / `NoteSkin` with the alias `arrows`.
- `typo_unicode_spill`: 96 U+0430 scalars plus U+044F in the query; each
  word has 96 U+0430 scalars plus U+0431.
- Catalogs contain `Electronic Music Volume {i:04} - An Extended Remix`
  for i=0..4095. Queries are `emr`, `electronic`, and `zzzz`. Each scan
  consumes the scores in a checksum without sorting or creating results.
  The 512-byte and 4,096-scalar cases demonstrate scaling rather than
  typical title sizes.

## Behavior and validation

The comparison suite checks every ASCII preceding-byte/matched-byte pair,
short/long search transitions, prefix offsets, gaps and case boundaries.
Unicode suffix cases include Japanese, Korean, Cyrillic, Greek, emoji,
combining marks, whitespace and NUL, including scalar-length penalty edges.
Typo tests check later better words, exact hits, aliases, whitespace splits,
and query lengths around inline storage limits. Generated mixed catalogs
compare every score and stable ranking against the frozen parent. Allocation
tests require zero churn on ordinary paths and at most one scratch allocation
on the long Unicode typo case.

- Full theme suite: **1,279 passed**, 5 pre-existing ignored tests, serial.
- Focused matcher suite: **21 passed**, one manual benchmark ignored,
  in both debug and release modes.
- Existing Unicode suite: **5 passed**, one manual benchmark ignored;
  includes preparation/folding parity for every Unicode scalar.
- Existing ranking suite: **22 passed**, one manual benchmark ignored;
  includes exhaustive short-word and long-query scoring comparisons.
- `cargo check -p deadsync`: passed.
- Performance Clippy: passed.

Performance Clippy uses the existing `large_enum_variant` exception for the
unchanged `SimplyLoveRuntimeRequest` in `effects.rs`. No new code suppression
was added. Scoped rustfmt, the frozen-baseline/version audit, and
`git diff --check` pass. The version is bumped exactly once, from 0.5.1207 to
0.5.1208, in the workspace manifest and all three corresponding lock entries.

## Reproduce

```powershell
cargo test -p deadsync-theme-simply-love --lib -- --test-threads=1
cargo test -p deadsync-theme-simply-love --test matcher_scan_perf
cargo test -p deadsync-theme-simply-love --test search_ranking_perf --test fuzzy_unicode
cargo test -p deadsync-theme-simply-love --release --test matcher_scan_perf
cargo check -p deadsync
cargo clippy -p deadsync-theme-simply-love --lib --test matcher_scan_perf --no-deps -- -A clippy::all -D clippy::perf -A clippy::large_enum_variant
cargo test -p deadsync-theme-simply-love --release --test matcher_scan_perf benchmark_matcher_scan -- --ignored --test-threads=1 --nocapture
```

Repeat the benchmark command five times, setting `DEADSYNC_PERF_REVERSE=1`
for runs 2 and 4 and removing it for runs 1, 3 and 5. The benchmark uses the
existing allocator and timing harness in `tests/support/perf.rs`.
