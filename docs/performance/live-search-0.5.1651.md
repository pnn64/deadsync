# Live search filtering, rank ties and row formatting - 0.5.1651

Parent: `4391f6452` (0.5.1650). Date: 2026-10-01.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY, M-AVOID-INDIRECTION and M-THROUGHPUT guidance to the
active Select Music typeahead overlay:

1. Reuse display-BPM filter tiers across keystrokes. The immutable search index
   owns a lazy side array of thread-safe slots, independent of chart type and
   difficulty. Only songs passing those chart filters initialize their slot.
   A first filter-only query keeps the existing short path without building the
   array; a query with text builds it as needed. Later queries reuse populated
   slots, including cached missing BPMs. Clones share an initialized array;
   catalog reloads build a fresh index. Chart filtering and fuzzy scores retain
   their existing behavior.
2. Reuse the library's measured ASCII byte comparator for ranking ties. Equal
   bytes avoid folding, and long equal prefixes compare in 16-byte blocks.
   The same descending score, ASCII case-insensitive title/name ordering and
   stable ties still select the same nine rows. Non-ASCII UTF-8 bytes keep their
   original byte ordering.
3. Reuse one formatting buffer across visible BPM and difficulty labels.
   Formatting APIs replace the caller's previous contents; existing owning APIs
   remain available. Each label is copied into its required owning `Arc<str>`
   before the next format. Typical nine-row requests replace 18 temporary
   strings with one 32-byte buffer. No buffer is allocated for zero results.

There are no new dependencies or unsafe production changes. The cache is local
to an immutable index, with no global invalidation policy. The overlay still
returns an owning vector and owning labels; this does not make the whole search
allocation-free. Warm cached tier reads and formatting into sufficient existing
capacity have zero heap churn in the scoped regression checks.

## Measurement method

The [raw CSV](live-search-0.5.1651.csv) records 126 rows: 21 workloads, two
implementations and three independent serial runs, old-first/new-first/old-first.
Each timing/cycle value is the median of seven batches, with each batch range
retained in the CSV. Allocation counting runs separately for one complete
operation, including destruction of returned rows. Allocation counters must
agree across all three runs for each workload/implementation.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz (22 cores / 44
logical processors), Rust 1.98.1, LLVM 22.1.8. The repository release profile
uses opt-level 3 and full LTO, with test unwinding. Processes run serially on
logical processor 6 (affinity mask 64), with `--test-threads=1`, after compiler
processes finish. CPU cycles use Windows `QueryThreadCycleTime` on the calling
thread. Throughput units are catalog entries/s for full scans and visible rows/s
for short filter-only and formatting requests.

Synthetic fixtures, indexes and queries are built outside measurement. The
parent search builders, rank comparators, filter predicate and formatters are
frozen in `tests/live_search/baseline.rs`. A source audit verifies all ten
function bodies and compiler attributes against the parent. The baseline BPM
label call is redirected to the frozen formatter so the new formatter cannot
contaminate it. Both paths share unchanged parsing, fuzzy scoring, index
fixtures and stack-based top-nine storage. Measurements cover the actual
production builders, rather than just standalone replacement helpers.

## Ranking

Each corpus has 4,096 entries in reverse catalog order. `short` uses short
names, `prefix` shares a long identical prefix, `case` mixes the case of that
prefix, `unicode` uses accented/Japanese text, and `diverse` changes early bytes.
Pack ranking uses an empty query to isolate tie comparison (64 iterations/batch).
Song ranking scores `album` or `cafe`, including result formatting (32/batch).

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `live_pack_short` | 564,984.4 -> 407,703.1 | 1,238,226.9 -> 893,639.4 | 22.6% to 27.8% | 29.1% to 38.6% | 1/0/1 -> 1/0/1 | 864 -> 864 |
| `live_pack_prefix` | 2,527,856.2 -> 488,385.9 | 5,539,199.1 -> 1,069,942.5 | 77.8% to 80.7% | 350.3% to 417.6% | 1/0/1 -> 1/0/1 | 864 -> 864 |
| `live_pack_case` | 2,210,259.4 -> 621,748.4 | 4,844,220.9 -> 1,362,597.7 | 71.9% to 76.6% | 255.5% to 327.0% | 1/0/1 -> 1/0/1 | 864 -> 864 |
| `live_pack_unicode` | 1,217,592.2 -> 643,325.0 | 2,667,929.9 -> 1,410,212.0 | 41.9% to 47.1% | 72.1% to 89.3% | 1/0/1 -> 1/0/1 | 864 -> 864 |
| `live_pack_diverse` | 72,043.8 -> 60,117.2 | 158,050.3 -> 131,840.6 | 16.6% to 25.7% | 19.8% to 34.7% | 1/0/1 -> 1/0/1 | 864 -> 864 |

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `live_song_short` | 1,040,106.2 -> 906,068.8 | 2,278,979.3 -> 1,985,713.7 | 11.7% to 14.6% | 13.2% to 17.1% | 47/0/47 -> 30/0/30 | 2264 -> 1837 |
| `live_song_prefix` | 2,702,284.4 -> 894,540.6 | 5,921,238.8 -> 1,959,785.2 | 65.5% to 66.9% | 190.0% to 202.1% | 47/0/47 -> 30/0/30 | 2264 -> 1837 |
| `live_song_case` | 2,578,059.4 -> 1,166,990.6 | 5,648,990.2 -> 2,556,633.0 | 54.7% to 61.5% | 120.9% to 159.7% | 47/0/47 -> 30/0/30 | 2264 -> 1837 |
| `live_song_unicode` | 1,626,331.2 -> 1,083,800.0 | 3,563,472.7 -> 2,375,093.0 | 29.9% to 37.2% | 42.8% to 59.4% | 47/0/47 -> 30/0/30 | 2263 -> 1836 |
| `live_song_diverse` | 527,825.0 -> 493,612.5 | 1,156,867.9 -> 1,081,579.4 | 6.5% to 11.3% | 6.9% to 12.8% | 47/0/47 -> 30/0/30 | 2264 -> 1837 |

## Visible-row formatting

Empty queries return the first one or nine rows (1,024 iterations/batch). The
4,096-song corpus also returns only nine rows. Fixtures have range BPM labels,
subtitles and all five standard difficulties. Required output ownership stays
unchanged.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `live_rows_1` | 1,331.7 -> 1,507.5 | 2,921.0 -> 3,306.6 | -13.2% to 8.8% | -11.7% to 9.4% | 6/0/6 -> 5/0/5 | 251 -> 232 |
| `live_rows_9` | 11,469.7 -> 11,245.4 | 25,145.2 -> 24,650.0 | -11.5% to 7.0% | -10.3% to 7.6% | 46/0/46 -> 29/0/29 | 2259 -> 1832 |
| `live_rows_4096` | 11,184.8 -> 10,303.3 | 24,514.1 -> 22,595.0 | 7.8% to 14.5% | 8.6% to 17.0% | 46/0/46 -> 29/0/29 | 2259 -> 1832 |

## BPM filters

All BPM corpora have 4,096 songs with `119.5:180.5` display tags. `hit` searches
`[120] album`, `miss` searches `[200] album`, `difficulty` searches
`[12][120] album`, and `no_text` searches `[120]`, stopping after nine hits.
Each batch uses 64 iterations. Warm requests prepopulate applicable slots.
Cold requests clone a fixture and clear its cache outside timing, then measure
its first request. Fixture destruction is outside cold measurement; the newly
retained cache bytes therefore appear in allocated bytes but not freed bytes.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `live_bpm_warm_hit` | 1,491,195.3 -> 933,956.2 | 3,266,050.2 -> 2,046,031.6 | 36.3% to 42.0% | 56.9% to 72.6% | 47/0/47 -> 30/0/30 | 2270 -> 1843 |
| `live_bpm_warm_miss` | 591,482.8 -> 375,267.2 | 1,296,048.0 -> 822,157.8 | 36.6% to 49.7% | 57.6% to 98.6% | 1/0/1 -> 1/0/1 | 11 -> 11 |
| `live_bpm_warm_no_text` | 13,670.3 -> 11,878.1 | 29,961.8 -> 26,120.5 | 4.2% to 14.9% | 4.7% to 17.6% | 47/0/47 -> 30/0/30 | 2264 -> 1837 |
| `live_bpm_warm_difficulty` | 1,795,467.2 -> 1,303,128.1 | 3,931,680.6 -> 2,854,354.0 | 20.8% to 27.4% | 27.2% to 37.8% | 47/0/47 -> 30/0/30 | 2274 -> 1847 |

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `live_bpm_cold_hit` | 1,534,143.8 -> 1,428,982.8 | 3,366,173.2 -> 3,135,921.0 | -0.1% to 6.8% | -0.1% to 7.4% | 47/0/47 -> 31/0/30 | 2270 -> 67395 |
| `live_bpm_cold_miss` | 661,635.9 -> 740,009.4 | 1,456,711.8 -> 1,626,848.2 | -24.4% to -11.7% | -19.6% to -10.6% | 1/0/1 -> 2/0/1 | 11 -> 65563 |
| `live_bpm_cold_no_text` | 18,820.3 -> 18,379.7 | 44,030.3 -> 43,179.8 | 1.9% to 8.9% | 2.4% to 10.9% | 47/0/47 -> 30/0/30 | 2264 -> 1837 |
| `live_bpm_cold_difficulty` | 1,612,154.7 -> 1,580,603.1 | 3,536,296.0 -> 3,466,776.1 | -1.8% to 5.7% | -1.7% to 6.1% | 47/0/47 -> 31/0/30 | 2274 -> 67399 |

## Tradeoffs and validation

All five pack-ranking corpora use 16.6% to 80.7% fewer CPU cycles, and all
five full song-ranking corpora use 6.5% to 66.9% fewer. Warm full BPM scans use
20.8% to 49.7% fewer cycles, with 27.2% to 98.6% more entries/s across the hit,
miss and difficulty cases. Ranking comparison itself was already allocation-free;
pack requests retain their one required output-vector allocation.

Nine-row formatting removes 17 allocations/frees and 427 requested/freed bytes
per request: 46/0/46 -> 29/0/29, and 2,259 -> 1,832 bytes. Nonempty song searches
also allocate their parsed query, giving 47 -> 30 allocations. Normal measured
formatting has no reallocations in either implementation. The 4,096-song
formatting control uses 7.8% to 14.5% fewer cycles, but the one-song and nine-song
controls have mixed CPU reductions (-13.2% to +8.8%, and -11.5% to +7.0%). Their
allocation reductions are deterministic; these runs do not establish a CPU
improvement for every small catalog. Longer labels or maximum u32 meters can
grow the scratch buffer; no new input or output length limit is introduced.
For a degenerate row with no BPM label and only an edit chart, the 32-byte scratch
can request more bytes than the parent's one-byte temporary difficulty string.

The lazy cache adds 24 bytes to the index and no bytes to individual song entries.
On the measured x64 target, its populated side array requests `16 * song_count
+ 16` bytes (slots plus Arc header): 65,552 bytes for 4,096 songs, in one allocation.
That storage persists until the last owning index clone is dropped. No BPM query,
no qualifying chart, or a first filter-only query does not allocate the array.
Only populated slots avoid parsing later; difficulty/type changes may populate
previously unused slots. Warm cache reads add no per-request allocations. Cold
here means an empty index cache, not cold hardware caches or a fresh process.

The first full BPM miss uses 11.7% to 24.4% more cycles and one extra allocation.
Cold hits are approximately flat to 6.8% better, and cold difficulty queries range
from 1.8% worse to 5.7% better. In these paired measurements one subsequent warm
miss saves more cycles than the first miss's extra cost. This is a CPU versus
retained-memory tradeoff for repeated typeahead, not a claim that cache memory or
first-request latency is lower. Filter-only first requests avoid that cache cost,
reduce allocations, and use 1.9% to 8.9% fewer measured cycles. The CSV records
both allocated and freed bytes so retained storage remains visible.

Validation:

- Release chart unit suite: 32 passed.
- Release simfile unit suite: 224 passed, 9 manual benchmarks ignored.
- Release theme unit suite: 1,146 passed, 3 ignored, one existing Cyber noteskin
  asset guard failed. All six new behavior/allocation tests passed in that run.
  The untouched test requires logical height / model height > 1.5; both are
  60.162445 with the current asset. It fails identically in isolation with the
  new binary and the pre-existing 2026-09-28 release binary, confirming this
  pass did not introduce it. The test is retained without weakening the guard.
- Complete result equivalence covers empty/small/large catalogs, scores, stable
  order, all labels, Arc identities, Unicode, typo and malformed/filter queries,
  chart-type case, edits, missing charts/BPMs, BPM rounding/tag edge cases and
  reloads. Cache checks cover lazy creation, partial slot population, clone
  sharing, concurrent searches and allocation-free warmed reads. Formatting
  checks cover nonfinite/extreme values, rate thresholds, reused contents and
  maximum meters against the frozen parent.
- All three release benchmark rounds passed; all 126 CSV rows were parsed and
  their allocation counters agree across the three runs.
- `cargo check --workspace --offline --locked` passed.
- Performance lint checks passed for the three affected libraries with
  `-D clippy::perf -A clippy::large_enum_variant`; the exception covers existing
  large enums, as in the previous pass. Other existing non-performance warnings
  remain. Modified Rust files pass `rustfmt --check`; `git diff --check` passes.
- Cargo.toml advances exactly 0.5.1650 -> 0.5.1651. Cargo.lock updates only the
  three packages inheriting the workspace version.

To reproduce behavior checks and the manual benchmark:

```powershell
cargo test -p deadsync-theme-simply-love -p deadsync-chart -p deadsync-simfile --lib --release --offline --locked
$env:DEADSYNC_PERF_ORDER = 'old-first' # alternate with 'new-first'
cargo test -p deadsync-theme-simply-love --lib --release --offline --locked benchmark_live_search -- --ignored --nocapture --test-threads=1
```

For comparable numbers, run the compiled benchmark binary serially with the
same CPU affinity, with no compiler process running. Timings are diagnostic,
not CI pass/fail thresholds. Deterministic result and allocation checks are
normal tests. These synthetic measurements do not establish whole-application
frame-rate or RSS improvements.
