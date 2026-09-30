# Library comparison, one-edit search and compiled actor lookup - 0.5.1650

Parent: `68ff515a3` (0.5.1649). Date: 2026-10-01.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance to three existing paths:

1. Library sorting skips ASCII folding for equal bytes and compares identical
   16-byte blocks in bulk for long names. A small scalar path handles short
   names and early differences; its wrappers are forced to inline after control
   runs exposed per-call overhead. A separate bulk helper limits inline code size.
   Comparison still orders ASCII-lowercased bytes, preserving non-ASCII UTF-8
   byte order, transliteration choice, secondary keys, grouping and stable ties.
2. Fuzzy search handles an edit budget of one with a direct substitution,
   insertion or deletion check after the existing equal-end trimming. It avoids
   initializing and evaluating a dynamic-programming row. A separate helper
   keeps its loops out of the general matcher's inline code. Larger edit budgets
   retain the original bounded algorithm, scores, thresholds and Unicode rules.
3. Compiled noteskin lookup builds its query in one reusable 256-byte inline
   buffer. It truncates the same buffer between specificity fallbacks. Typical
   borrowed lookups and misses have zero heap churn; owning hits retain their
   required actor-declaration clone. Oversized keys reserve their maximum query
   size once and can spill once. Root selection, filename/path handling,
   case-insensitive matching, duplicates and fallback priority are unchanged.

There are no new dependencies, public API changes, global caches or unsafe
production changes. Comparison and prepared short-query scoring were already
allocation-free: their benefit is fewer CPU cycles. The noteskin buffer adds
inline stack storage while removing temporary heap keys. This does not make
the entire application or arbitrary long inputs allocation-free.

## Measurements

The [raw CSV](library-search-0.5.1650.csv) records 174 rows: 29 workloads, two
implementations and three independent serial runs. Run order is old-first,
new-first, old-first. Each timing and cycle value is the median of seven batches;
the CSV retains each batch range. Percentage ranges below cover all three
paired runs, with absolute values from run 1. Allocation tracking runs separately
for one complete operation, including destruction of owning results.

Inputs and prepared queries are constructed outside the measurements. Sort
fixture cloning is outside timing and allocation accounting; the consumed
input vector and grouped output are freed inside the measured operation.
Consequently, sort freed-byte counts include the input allocation made during
setup. Shared song data remains owned by the fixture. Allocation counters agree
across all three runs for every workload and implementation.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz (22 cores / 44
logical processors), Rust 1.98.1 with LLVM 22.1.8. Tests use the repository's
release profile, opt-level 3 and full LTO, with test unwinding. Benchmark
processes run serially on logical processor 6 (affinity mask 64), after compiler
processes finish, using `--test-threads=1`. CPU cycles come from Windows
`QueryThreadCycleTime` for the calling thread rather than elapsed TSC ticks.

Frozen parent implementations live beside each harness. A source audit verifies
their bodies against the parent revision and preserves the comparator's original
compiler attributes. Unchanged grouping helpers are shared by both sort paths.

### Library comparison and sorting

The synthetic library has 4,096 shuffled songs, repeated titles, secondary
subtitle/path keys and equal BPM tags to exercise title tiebreakers. `short`
uses short names; `prefix` uses a long shared pack prefix; `diverse` changes
early bytes of long names; `case` mixes upper/lowercase shared prefixes.
`cmp` measures 4,095 adjacent comparisons per operation (64 iterations per
batch); `title` and `bpm` measure full grouping/sorting (32 iterations per batch).
Throughput units are comparisons/s for `cmp` and songs/s for the sorts.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `library_cmp_short` | 225,756.2 -> 172,579.7 | 494,163.1 -> 378,476.3 | 23.4% to 26.5% | 30.5% to 31.9% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `library_title_short` | 2,617,215.6 -> 2,034,371.9 | 5,737,681.9 -> 4,460,966.9 | 21.4% to 25.6% | 27.0% to 34.3% | 4/0/5 -> 4/0/5 | 69680 -> 69680 |
| `library_bpm_short` | 3,017,675.0 -> 2,512,009.4 | 6,616,168.8 -> 5,507,323.6 | 16.8% to 18.3% | 20.1% to 22.3% | 3/0/4 -> 3/0/4 | 65584 -> 65584 |
| `library_cmp_prefix` | 474,754.7 -> 221,559.4 | 1,040,474.6 -> 485,650.6 | 52.4% to 53.8% | 110.0% to 117.0% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `library_title_prefix` | 5,508,568.8 -> 2,394,556.2 | 12,074,194.2 -> 5,249,280.8 | 55.7% to 56.5% | 125.2% to 130.0% | 4/0/5 -> 4/0/5 | 69680 -> 69680 |
| `library_bpm_prefix` | 5,833,653.1 -> 2,851,984.4 | 12,786,718.5 -> 6,252,965.2 | 51.1% to 52.0% | 104.5% to 110.7% | 3/0/4 -> 3/0/4 | 65584 -> 65584 |
| `library_cmp_diverse` | 100,995.3 -> 105,868.8 | 221,341.7 -> 232,025.2 | -7.3% to -3.9% | -8.2% to -3.8% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `library_title_diverse` | 1,864,484.4 -> 1,099,181.2 | 4,087,247.7 -> 2,412,833.2 | 40.2% to 41.0% | 67.2% to 69.6% | 28/0/29 -> 28/0/29 | 38112 -> 38112 |
| `library_bpm_diverse` | 2,885,196.9 -> 2,061,262.5 | 6,326,559.3 -> 4,517,501.9 | 27.8% to 28.6% | 38.6% to 40.0% | 3/0/4 -> 3/0/4 | 65584 -> 65584 |
| `library_cmp_case` | 480,976.6 -> 355,675.0 | 1,053,236.5 -> 779,173.5 | 25.1% to 26.8% | 33.5% to 36.8% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `library_title_case` | 5,598,859.4 -> 3,980,968.8 | 12,261,290.4 -> 8,721,715.7 | 25.1% to 28.9% | 32.0% to 40.6% | 4/0/5 -> 4/0/5 | 69680 -> 69680 |
| `library_bpm_case` | 5,941,700.0 -> 4,382,809.4 | 13,017,406.3 -> 9,593,727.7 | 24.3% to 26.6% | 32.2% to 36.5% | 3/0/4 -> 3/0/4 | 65584 -> 65584 |

### One-edit search

Each operation scores all 4,096 synthetic labels (64 iterations per batch).
`spxed` and `glxw` are short misspellings, `zzzz` is an unrelated short query,
and `spe` exercises the subsequence/typo mixture. `perspextive` is a longer
query control that retains the general algorithm and candidate length guards.
Throughput is labels/s. Both implementations have zero allocation,
reallocation, free and requested/freed-byte churn for every measured query.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `matcher_spxed` | 1,039,081.2 -> 466,076.6 | 2,276,959.2 -> 1,020,520.7 | 45.5% to 56.1% | 84.0% to 125.3% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `matcher_glxw` | 1,095,465.6 -> 469,770.3 | 2,401,237.4 -> 1,029,647.1 | 50.7% to 57.8% | 102.5% to 138.0% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `matcher_zzzz` | 1,165,229.7 -> 457,837.5 | 2,552,099.0 -> 1,002,837.2 | 58.3% to 60.7% | 137.7% to 157.2% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `matcher_spe` | 863,407.8 -> 395,989.1 | 1,891,819.1 -> 867,909.9 | 49.5% to 54.1% | 98.4% to 118.0% | 0/0/0 -> 0/0/0 | 0 -> 0 |
| `matcher_perspextive` | 855,512.5 -> 865,354.7 | 1,875,243.4 -> 1,896,195.3 | -5.5% to 3.7% | -5.3% to 4.1% | 0/0/0 -> 0/0/0 | 0 -> 0 |

### Compiled actor lookup

Each operation performs one owning lookup (4,096 iterations per batch).
Manifest sizes in workload names count unrelated files; one final target entry
is also present, so actual sizes are 17, 257 and 1,025. `specific`, `color` and
`base` hit the corresponding fallback; `miss` traverses all fallback tiers.
Throughput is lookups/s. Hit declarations have a sprite and texture string;
their two required clone allocations account for the remaining 148 bytes.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, three runs | Throughput gain, three runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `actor_16_specific` | 1,063.6 -> 802.3 | 2,333.0 -> 1,755.8 | 2.1% to 25.8% | 2.2% to 36.6% | 4/1/4 -> 2/0/2 | 244 -> 148 |
| `actor_16_color` | 1,298.7 -> 832.7 | 2,848.0 -> 1,826.0 | 35.9% to 44.0% | 56.0% to 78.6% | 5/2/5 -> 2/0/2 | 316 -> 148 |
| `actor_16_base` | 1,558.6 -> 870.3 | 3,417.7 -> 1,906.4 | 44.0% to 45.1% | 76.7% to 83.3% | 6/3/6 -> 2/0/2 | 388 -> 148 |
| `actor_16_miss` | 1,337.7 -> 704.8 | 2,932.1 -> 1,545.6 | 47.3% to 48.8% | 89.8% to 91.7% | 4/3/4 -> 0/0/0 | 240 -> 0 |
| `actor_256_specific` | 1,168.2 -> 980.5 | 2,562.1 -> 2,150.8 | 14.1% to 37.5% | 13.4% to 61.4% | 4/1/4 -> 2/0/2 | 244 -> 148 |
| `actor_256_color` | 1,587.1 -> 1,190.5 | 3,477.2 -> 2,610.5 | 24.9% to 48.8% | 33.3% to 97.5% | 5/2/5 -> 2/0/2 | 316 -> 148 |
| `actor_256_base` | 2,016.5 -> 1,474.8 | 4,419.5 -> 3,233.0 | 19.7% to 26.8% | 27.9% to 36.7% | 6/3/6 -> 2/0/2 | 388 -> 148 |
| `actor_256_miss` | 1,786.1 -> 1,306.2 | 3,909.1 -> 2,863.8 | 26.7% to 58.3% | 36.7% to 142.8% | 4/3/4 -> 0/0/0 | 240 -> 0 |
| `actor_1024_specific` | 2,110.3 -> 1,734.6 | 4,624.5 -> 3,804.1 | 12.3% to 17.7% | 12.5% to 21.7% | 4/1/4 -> 2/0/2 | 244 -> 148 |
| `actor_1024_color` | 3,287.8 -> 2,634.7 | 7,206.1 -> 5,775.2 | 19.9% to 26.5% | 24.8% to 39.6% | 5/2/5 -> 2/0/2 | 316 -> 148 |
| `actor_1024_base` | 5,314.2 -> 4,342.3 | 11,647.5 -> 9,521.2 | 10.5% to 18.3% | 12.1% to 22.4% | 6/3/6 -> 2/0/2 | 388 -> 148 |
| `actor_1024_miss` | 5,006.7 -> 4,072.7 | 10,976.5 -> 8,927.9 | 16.8% to 20.2% | 20.9% to 25.0% | 4/3/4 -> 0/0/0 | 240 -> 0 |

Allocated and freed bytes are equal for these lookup operations, including
reallocations and owning-result destruction. Specific/color/base hits remove
2/3/4 temporary allocations and 1/2/3 reallocations respectively. Misses remove
all 4 allocations and 3 reallocations. Borrowed lookup has zero churn in the
behavior tests. Key-size tests at 255, 256, 257, 512 and 1,024 bytes verify zero
temporary allocations through 256 bytes and at most one exact-size spill above
that limit, without changing lookup results.

## Controls and limits

Negative percentages mean higher CPU cost or lower throughput. Isolated
`library_cmp_diverse` comparisons use 3.9% to 7.3% more CPU cycles in every run.
The full title/BPM sorts on that same diverse corpus use 40.2% to 41.0% and
27.8% to 28.6% fewer cycles respectively. This small comparison overhead is a
tradeoff for the larger gains in actual sorting operations. The longer-query
`matcher_perspextive` control has mixed CPU changes (-5.5% to +3.7% reduction).
No uniform improvement is claimed for these controls; all samples remain in
the CSV.

These are synthetic library/search/manifest operations, not a whole-game
frame-rate or end-to-end loading benchmark. Requested/freed bytes measure
allocator churn, not peak RSS or retained memory. Instruction counts and cache
misses are not measured. Required sort output buffers and actor clones remain;
long strings can still exceed inline capacities. The results quantify these
specific paths and include controls rather than implying a universal speedup.

## Behavior validation

- Noteskin library tests: 293 passed, 10 ignored; new lookup harness: 3 passed,
  1 manual benchmark ignored, in both debug and release. Tests cover every
  variant availability mask, lookup ordering, case-insensitive duplicates,
  overlapping roots, nested filenames, Unicode/delimiter-containing keys,
  malformed root selection, borrowed pointer identity and spill boundaries.
- Simfile library tests: 224 passed, 9 ignored, in debug and release. New
  comparator tests cover every byte position around 16-byte and 32-byte
  boundaries, ASCII case, NUL, multibyte Unicode and unequal lengths, plus
  4,096 generated Unicode pairs. Grouped sort parity checks stable `Arc`
  identity, transliteration, groups and title/BPM tiebreakers against the parent.
- Matcher harnesses: `one_edit` 17 passed, `matcher_scan_perf` 21 passed and
  `search_ranking_perf` 22 passed; each ignores one manual benchmark. The
  one-edit harness also passes in release. Tests compare parent/current scores
  exhaustively over short words, every edit position, aliases and Unicode.
  Existing exhaustive bounded-distance tests compare with full Levenshtein
  distance at budgets 0 through 6, including the new fast path.
- `cargo check --workspace --locked --offline`, formatting and diff checks pass.
  Clippy's performance checks pass with `--no-deps` and the pre-existing
  `large_enum_variant` lint allowed. The unqualified check is blocked by
  unchanged large enums in `deadlib-present/src/actors.rs` and Simply Love's
  `effects.rs`; this pass does not change their representations.
- The manifest/lockfile audit verifies exactly `0.5.1649 -> 0.5.1650`, with
  only the three workspace-version package entries changed in `Cargo.lock`.

## Reproduce

```powershell
cargo test -p deadsync-noteskin --lib --test compiled_lookup --locked --offline
cargo test -p deadsync-simfile --lib --locked --offline
cargo test -p deadsync-theme-simply-love --test one_edit --test matcher_scan_perf --test search_ranking_perf --locked --offline
cargo check --workspace --locked --offline
cargo clippy -p deadsync-simfile -p deadsync-noteskin -p deadsync-theme-simply-love --lib --test one_edit --test compiled_lookup --locked --offline --no-deps -- -D clippy::perf -A clippy::large_enum_variant

# Compile before measuring, then run these commands serially on an idle core.
cargo test -p deadsync-simfile --lib --release --locked --offline --no-run
cargo test -p deadsync-theme-simply-love --test one_edit --release --locked --offline --no-run
cargo test -p deadsync-noteskin --test compiled_lookup --release --locked --offline --no-run
(Get-Process -Id $PID).ProcessorAffinity = [IntPtr]64
cargo test -p deadsync-simfile --lib --release --locked --offline benchmark_library_compare -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-theme-simply-love --test one_edit --release --locked --offline benchmark_one_edit -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-noteskin --test compiled_lookup --release --locked --offline benchmark_compiled_lookup -- --ignored --nocapture --test-threads=1
# Repeat all three benchmarks with $env:DEADSYNC_PERF_REVERSE = '1',
# then repeat without that environment variable for the third paired run.
```
