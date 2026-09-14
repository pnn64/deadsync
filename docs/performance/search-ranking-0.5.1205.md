# Search ranking and visible text - 0.5.1205

Baseline: `5c0e0263b` / 0.5.1204. This pass applies the unnecessary-copy,
allocation-reuse, and throughput guidance in `rust-performance.md`
(`M-HOTPATH`, `M-MEM-REUSE`, `M-INITIAL-CAPACITY`, and `M-THROUGHPUT`) to
three operations used by settings and song search.

1. **Settings candidate labels:** cleaning returns a borrowed slice while
   scoring. Rejected candidates no longer create a temporary label string.
   Successful unchanged labels share the row's existing `Arc<str>`; cleaned
   labels allocate their final shared string directly. Component-choice labels
   also skip the intermediate `String` before becoming shared text.
2. **Result display variants:** normal/focused labels are created only when
   displayed, then shared with subsequent actors. Short labels concatenate
   through 128 bytes of inline scratch and allocate only the final `Arc<str>`.
   Longer labels reserve exact scratch capacity without growing it. Results
   outside the visible window no longer eagerly format both variants. Query
   changes replace the result list and its cells; focus changes and component
   scrolling initialize each needed variant independently.
3. **Typo scoring:** bounded Levenshtein matching trims equal prefixes and
   suffixes before building its working row. It stops when every cell in the
   active band already exceeds the allowed edit cost. The threshold, Unicode
   folding, subsequence scores, alias penalties, and final ranking rules are
   unchanged. This reduces work per candidate across the entire song catalog
   and the settings search, including rejected candidates. Long near-matches
   can keep the reduced working row on the stack.

These are targeted search measurements. They do not measure full UI frames,
whole-library ranking latency, game FPS, or total application CPU usage.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), repository release profile with opt-level 3 and LTO. The shared
allocator tracker forwards to `System`. Counting is disabled during timing;
a separate operation records allocation, reallocation, free, requested-byte,
and freed-byte churn. Input construction and query preparation are outside
timing. Outputs are consumed through black boxes and dropped inside timing.

The integration test compiles the exact production matcher and search helper
modules. The frozen parent matcher differs only by its provenance comment and
removal of its test module. Label ownership comparisons share the current
matcher between old and new paths to isolate copying costs; their frozen
parent logic substitutes prebuilt inputs for state/row access. Display
comparisons use the parent's eager formatting expressions. Typo comparisons
run the complete old/new public scoring operation, including the initial
subsequence attempt, word splitting, and any fallback.

Each measurement has three warmups and seven timing samples. Five complete
runs alternate old/new order, after builds and checks finish. Tables show
medians of five per-run medians. CPU cycles come from Windows
`QueryThreadCycleTime` for the calling thread; these workloads are single
threaded. The [CSV](search-ranking-0.5.1205.csv) contains all 250 measurements,
including sample ranges and every churn metric.

Label fixtures cover hits, misses, aliases, empty queries, templated names,
and accented text. One operation processes one candidate and drops its result.
Display fixtures construct state for 8, 128, or 1,024 labels, read the visible
variants, and destroy that state. The ordinary viewport contains eight rows
with one focused row. Additional cases visit every row, materialize both
variants of every row, and use 240-byte labels that exceed inline capacity.
The 60-frame case constructs 128 rows, reads an eight-row window 60 times while
cycling focus, and destroys the state. The warm control reads eight already
prepared rows, with the same label/display-state layout for both versions.

Typo fixtures cover front/middle/end substitutions, a transposition, unrelated
words, Cyrillic text, multiword labels, and a 128-character ASCII near-match.
The last case exercises the public matcher's long-input behavior; it exceeds
song search's 80-character UI limit and is a scaling case, not a typical query.
Prefix and alias matches serve as unchanged scoring controls.

Throughput counts candidates for label/scoring operations, result slots for
display preparation, rows for warm reads, and text-preparation frames for the
60-frame lifecycle. Allocation bytes count requests, including reallocations;
they are not RSS or allocator overhead. Thread cycles do not measure retired
instructions or cache misses.

## Results

| Workload | Old ns/op | New ns/op | CPU cycles old/new | Fewer cycles | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| Label / hit | 266.2 | 129.2 | 583.6 / 283.3 | 51.5% | 3.757 / 7.740 |
| Label / miss | 539.1 | 417.0 | 1,181.8 / 912.7 | 22.8% | 1.855 / 2.398 |
| Label / trimmed | 305.8 | 240.7 | 669.6 / 527.8 | 21.2% | 3.270 / 4.155 |
| Label / alias | 310.9 | 153.9 | 681.7 / 337.5 | 50.5% | 3.216 / 6.496 |
| Label / accented | 366.6 | 252.3 | 803.7 / 553.0 | 31.2% | 2.728 / 3.963 |
| Label / empty query | 215.4 | 92.7 | 472.4 / 203.1 | 57.0% | 4.642 / 10.787 |
| Display / 8 rows, 8 visible | 4,716.8 | 1,448.4 | 10,359.4 / 3,184.5 | 69.3% | 1.696 / 5.523 |
| Display / 128 rows, 8 visible | 71,603.9 | 1,707.8 | 156,967.4 / 3,753.8 | 97.6% | 1.788 / 74.950 |
| Display / 1,024 rows, 8 visible | 647,752.3 | 4,196.9 | 1,418,816.2 / 9,218.1 | 99.4% | 1.581 / 243.991 |
| Display / all 128 rows | 68,286.7 | 16,665.2 | 149,722.2 / 36,525.3 | 75.6% | 1.874 / 7.681 |
| Display / both variants, all 128 rows | 74,671.9 | 35,334.8 | 163,705.8 / 77,450.9 | 52.7% | 1.714 / 3.622 |
| Display / long labels, 8 visible | 73,198.8 | 1,830.5 | 160,184.4 / 3,993.9 | 97.5% | 1.749 / 69.927 |
| Display / long labels, both variants of all rows | 78,157.8 | 61,012.1 | 171,336.0 / 133,768.1 | 21.9% | 1.638 / 2.098 |
| Display / 60-frame lifecycle | 74,654.7 | 9,382.8 | 163,660.4 / 20,562.7 | 87.4% | 0.804 / 6.395 |
| Display / 8 warm reads | 105.4 | 118.5 | 230.9 / 259.9 | -12.6% | 75.936 / 67.501 |
| Typo / front | 450.6 | 129.5 | 988.3 / 284.5 | 71.2% | 2.219 / 7.720 |
| Typo / middle | 451.4 | 137.5 | 990.4 / 301.6 | 69.5% | 2.215 / 7.271 |
| Typo / end | 461.6 | 134.2 | 1,012.7 / 294.3 | 70.9% | 2.166 / 7.454 |
| Typo / transposed | 452.6 | 147.8 | 992.8 / 324.3 | 67.3% | 2.210 / 6.764 |
| Typo / unrelated | 327.8 | 208.2 | 718.9 / 456.7 | 36.5% | 3.050 / 4.803 |
| Typo / Cyrillic | 671.4 | 557.1 | 1,472.1 / 1,222.0 | 17.0% | 1.490 / 1.795 |
| Typo / multiword | 472.6 | 171.4 | 1,035.8 / 376.1 | 63.7% | 2.116 / 5.833 |
| Typo / 128-character near-match | 38,817.1 | 909.3 | 85,097.8 / 1,994.1 | 97.7% | 0.026 / 1.100 |
| Prefix scoring / control | 29.5 | 28.4 | 65.0 / 62.5 | control | 33.875 / 35.186 |
| Alias scoring / control | 65.2 | 46.4 | 143.1 / 102.0 | control | 15.337 / 21.547 |

The unchanged prefix and alias controls are reported without attributing their
timing differences to an optimization. In particular, the alias control varies
substantially despite following the same algorithm, so small absolute timing
differences should not be treated as portable guarantees.

Warm reads retain zero allocation/free churn but cost 13.1 ns more per eight
rows in this run (105.4 -> 118.5 ns, 230.9 -> 259.9 thread cycles). Lazy access
adds a cell check. The measured 60-frame lifecycle includes setup, focus
changes, repeated reads, and destruction, and uses 87.4% fewer cycles overall.
The benefit depends on result-list size and how long a query remains unchanged;
this is not a claim that every individual cached read is faster.

| Workload | Allocation/free calls old/new | Reallocations old/new | Requested/freed bytes old/new |
|---|---:|---:|---:|
| Label / hit | 2 / 0 | 0 / 0 | 41 / 0 |
| Label / miss | 1 / 0 | 0 / 0 | 9 / 0 |
| Label / trimmed | 2 / 1 | 0 / 0 | 42 / 32 |
| Label / alias | 2 / 0 | 0 / 0 | 32 / 0 |
| Label / accented | 3 / 1 | 0 / 0 | 50 / 9 |
| Label / empty query | 2 / 0 | 0 / 0 | 41 / 0 |
| Display / 8 rows, 8 visible | 33 / 9 | 16 / 0 | 1,360 / 576 |
| Display / 128 rows, 8 visible | 513 / 9 | 256 / 0 | 21,760 / 4,416 |
| Display / 1,024 rows, 8 visible | 4,097 / 9 | 2,048 / 0 | 174,080 / 33,088 |
| Display / all 128 rows | 513 / 129 | 256 / 0 | 21,760 / 9,216 |
| Display / both variants, all 128 rows | 513 / 257 | 256 / 0 | 21,760 / 14,336 |
| Display / long labels, 8 visible | 513 / 17 | 256 / 0 | 135,424 / 8,146 |
| Display / long labels, both variants of all rows | 513 / 513 | 256 / 0 | 135,424 / 133,888 |
| Display / 60-frame lifecycle | 513 / 17 | 256 / 0 | 21,760 / 4,736 |
| Display / 8 warm reads | 0 / 0 | 0 / 0 | 0 / 0 |
| Typo / 128-character near-match | 1 / 0 | 0 / 0 | 1,032 / 0 |

Frees equal allocations and freed bytes equal requested bytes in every
measurement. Every new implementation has zero reallocations in these
fixtures. The CSV keeps allocation/free counts and byte totals separately.
Ordinary short typo comparisons remain allocation-free; the long ASCII
near-match removes the old 1,032-byte working-row allocation.

## Memory and behavior

Display state remains 32 inline bytes per result on this Windows target, the
same size as the previous two `Arc<str>` fields. Unvisited variants own no
strings. Once displayed, a variant retains its final shared text until its
result list is replaced or the overlay closes. At most the same two variants
per result are retained. The 128-byte scratch buffer exists only during initial
formatting, with a heap fallback for longer text. No persistent scratch cache
or global state is added.

Unchanged setting labels share their source allocation. Trimmed labels still
need one final allocation, and diacritic folding can still allocate a temporary
string for accented candidates. Long queries and Unicode candidate buffers
may still spill; typo pruning does not make arbitrary input allocation-free.
Existing public search APIs and serialized data remain compatible. No external
dependency versions or unsafe code are added.

- Theme library: 1,277 tests passed; five existing manual tests ignored. The
  new overlay test checks exact emitted row text while scrolling and wrapping
  a 32-choice component, typing/filtering, backspacing, and reopening. Existing
  tests cover visibility, duplicate rows, selection, aliases, completion, and
  search input handling.
- Search integration: 22 tests passed in debug and release; one manual
  benchmark ignored. These include six new regressions for cleaning, shared
  identity, display-cache behavior, inline/spill boundaries, stable ranking,
  Unicode edits, long-input allocation budgets, and parent score equivalence.
  All 65,025 pairs of binary-alphabet words through length seven preserve scores.
- An independent distance check compares the optimized bounded solver against
  `strsim::levenshtein` for 27,783 word-pair/threshold combinations, covering
  empty inputs, unequal lengths, and thresholds zero through six.
- `cargo check -p deadsync`, scoped rustfmt, and `git diff --check` passed.
  Performance Clippy passed with the existing unrelated
  `SimplyLoveRuntimeRequest` large-enum diagnostic allowed. The theme test build
  retains the existing unused `song_lua_overlay_camera_state` warning.
- Workspace version: exactly 0.5.1204 -> 0.5.1205. Cargo.lock updates the same
  three workspace-version packages; external dependencies are unchanged.

Reproduce from the repository root:

```powershell
cargo test -p deadsync-theme-simply-love --lib --test search_ranking_perf -- --test-threads=1
cargo test --release -p deadsync-theme-simply-love --test search_ranking_perf -- --test-threads=1
cargo check -p deadsync
cargo clippy -p deadsync-theme-simply-love --lib --test search_ranking_perf --no-deps -- -A clippy::all -D clippy::perf -A clippy::large_enum_variant
cargo test --release -p deadsync-theme-simply-love --test search_ranking_perf benchmark_search_ranking -- --ignored --nocapture --test-threads=1
```

Repeat the final command five times, setting `DEADSYNC_PERF_REVERSE=1` for runs
two and four and removing it for the others. Finish builds before timing.
Other platforms report zero for the unavailable Windows thread-cycle metric.
