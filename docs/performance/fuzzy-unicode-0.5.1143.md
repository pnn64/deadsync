# Unicode fuzzy-search performance, 0.5.1143

Baseline: `6785ebda8` / 0.5.1142. This pass bumps the workspace patch version exactly once to 0.5.1143, including the matching Cargo.lock entries.

## Three changes

The local `rust-performance.md` guidance M-HOTPATH, M-MEM-REUSE, and M-THROUGHPUT motivates removing temporary ownership and repeated per-candidate work from the shared song/settings search engine.

1. **Delay title-folding allocation until text changes.** `fold_diacritics` used to build and discard a string for every unchanged non-ASCII label. It now locates the first scalar that changes, copies the unchanged prefix only when needed, and folds the remainder into one input-sized buffer. ASCII scalars bypass Unicode decomposition. Non-Latin titles, kana, Hangul, and emoji retain their original spelling and borrowed return value, with no temporary allocation. Settings search folds labels during ranking; song search folds them when building its index.
2. **Fold queries directly into their final character storage.** `prepare_query` and `query_chars` compose the existing diacritic fold, whitespace removal, and one-scalar case fold without an intermediate UTF-8 string. Prepared queries retain the same 32-character inline buffer and long-query spill behavior. The scalar helper is also shared by ghost-prefix matching, preserving its original-character split counts.
3. **Prune redundant typo comparisons before materializing candidates.** The fallback counts Unicode scalars before building its case-folded buffer, rejecting impossible length differences without allocating for long titles. It also skips the duplicate whole-label comparison introduced by `split_whitespace` for one-word labels. Accepted words still use the same bounded Levenshtein computation and threshold; aliases, ranking scores, and early exact-match handling remain unchanged.

Public APIs and search results are preserved. No dependencies or allocator configuration changed. Owned rewritten titles and long prepared queries still require storage; this pass removes unnecessary intermediate work rather than promising zero allocations for every input.

## Validation

The new integration test calls the production public API directly. Its frozen baseline copies the previous fuzzy module without its old test module; formatting is the only baseline code change.

Five regression/allocation tests cover all **1,112,064 valid Unicode scalars**, 512 generated mixed strings, NFC/NFD forms, combining marks, Unicode whitespace, NUL, kana/dakuten, Hangul, case-fold edge cases, empty queries, inline/spill boundaries, long queries, ranking with aliases, original-text prefix splits, and typo length thresholds. Allocation checks enforce zero churn for unchanged non-Latin titles, short accented/non-Latin prepared queries, and impossible long Unicode candidates.

- New tests: **5 passed in debug**, **5 passed in release**; the manual benchmark is ignored in ordinary test runs.
- Existing tests matching `fuzzy`: **16 passed**; matching `search`: **43 passed**, covering **58 distinct existing tests**.
- `cargo check --workspace --bins`: passed.
- Clippy with `-D clippy::perf -A clippy::large_enum_variant`: passed. The exception covers the pre-existing large `SimplyLoveRuntimeRequest` enum in `src/effects.rs`; no source-level suppression was added. Existing warning classes remain.
- Targeted Rust formatting and `git diff --check`: passed.

## Measurement

Windows x64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (`88d9e12ae`), LLVM 22.1.8. Release profile: opt-level 3, LTO. Old and new functions execute in the same test binary via opaque function pointers and input/output black boxes; fixtures and query preparation for scoring are outside the measured region. Returned values are dropped inside it. `QueryThreadCycleTime` measures calling-thread cycles.

The tables use medians from three runs, reversing old/new order in the middle run. Each run reports seven timing batches after warm-up, with 20,000 operations per batch for folding/query preparation and 5,000 for matching. Allocation counts come from a separate operation. Counting is disabled during timed batches, while the same `System` allocator wrapper remains installed for both implementations. Final timing runs occur after builds/checks finish.

Folding throughput counts original Unicode scalars per second. Query preparation and matching throughput count operations per second. Matching includes the public API's subsequence attempt and typo fallback, not just an isolated rejection helper.

| Workload | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | Million units/s old -> new |
|---|---:|---:|---:|---:|
| ASCII title (unchanged control) | 11.3 -> 12.5 | 24.9 -> 27.6 | -10.8% | 1501.767 -> 1356.744 |
| Japanese title, unchanged by folding | 244.3 -> 127.5 | 535.8 -> 278.8 | 48.0% | 98.220 -> 188.294 |
| ASCII title with emoji, unchanged by folding | 404.3 -> 148.1 | 879.7 -> 324.3 | 63.1% | 358.614 -> 979.035 |
| Short title with multiple accents | 140.4 -> 132.8 | 308.0 -> 291.3 | 5.4% | 149.583 -> 158.097 |
| Long title with a late accent | 374.9 -> 225.1 | 817.4 -> 493.9 | 39.6% | 309.387 -> 515.269 |
| Short ASCII query | 86.3 -> 87.5 | 189.1 -> 191.8 | -1.4% | 11.591 -> 11.435 |
| Short NFC accented query | 198.2 -> 124.6 | 433.7 -> 273.4 | 37.0% | 5.045 -> 8.023 |
| Short NFD accented query | 179.2 -> 91.7 | 392.4 -> 201.2 | 48.7% | 5.581 -> 10.902 |
| Short Japanese query | 272.8 -> 162.9 | 598.3 -> 357.4 | 40.3% | 3.665 -> 6.139 |
| Accented query spilling beyond inline capacity | 1,309.1 -> 1,118.2 | 2,867.8 -> 2,452.2 | 14.5% | 0.764 -> 0.894 |
| One-word ASCII typo | 753.9 -> 400.6 | 1,653.6 -> 879.3 | 46.8% | 1.326 -> 2.496 |
| One-word Cyrillic typo | 595.9 -> 435.1 | 1,307.6 -> 954.4 | 27.0% | 1.678 -> 2.298 |
| Unrelated 24-character Unicode title | 1,684.1 -> 633.9 | 3,690.6 -> 1,390.2 | 62.3% | 0.594 -> 1.578 |
| Unrelated 300-character Unicode word | 21,085.4 -> 7,840.3 | 46,156.7 -> 17,175.9 | 62.8% | 0.047 -> 0.128 |
| Unrelated Unicode title with 40 words | 18,480.2 -> 6,925.8 | 40,221.4 -> 15,174.1 | 62.3% | 0.054 -> 0.144 |

## Allocation churn

Counts include intermediate allocations, reallocations, and frees. Requested bytes include all allocation/reallocation requests; allocated and freed byte totals match. The unchanged result's borrowed/owned status is checked against the baseline.

| Workload | Allocations / reallocations / frees old -> new | Allocated and freed bytes/op old -> new |
|---|---:|---:|
| ASCII title (unchanged control) | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Japanese title, unchanged by folding | 1/0/1 -> 0/0/0 | 72 -> 0 |
| ASCII title with emoji, unchanged by folding | 1/0/1 -> 0/0/0 | 148 -> 0 |
| Short title with multiple accents | 1/0/1 -> 1/0/1 | 25 -> 25 |
| Long title with a late accent | 1/0/1 -> 1/0/1 | 117 -> 117 |
| Short ASCII query | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Short NFC accented query | 1/0/1 -> 0/0/0 | 12 -> 0 |
| Short NFD accented query | 1/0/1 -> 0/0/0 | 15 -> 0 |
| Short Japanese query | 1/0/1 -> 0/0/0 | 15 -> 0 |
| Accented query spilling beyond inline capacity | 2/1/2 -> 1/1/1 | 888 -> 768 |
| One-word ASCII typo | 0/0/0 -> 0/0/0 | 0 -> 0 |
| One-word Cyrillic typo | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Unrelated 24-character Unicode title | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Unrelated 300-character Unicode word | 2/2/2 -> 0/0/0 | 6,144 -> 0 |
| Unrelated Unicode title with 40 words | 1/1/1 -> 0/0/0 | 3,072 -> 0 |

## Variation and limits

The targeted Unicode cases reduced median cycles, including a 5.4% gain for the short accented title and 48.0-63.1% for unchanged Japanese/mixed titles. Short Unicode queries improved by 37.0-48.7%; the spilling query improved by 14.5%. Typo cases improved by 27.0-62.8%, including the one-word ASCII case.

The small ASCII controls were slower in the final medians: title folding measured 11.3 -> 12.5 ns/op, and query preparation measured 86.3 -> 87.5 ns/op. Both remain allocation-free. Their run ranges overlap, but these results are retained as measured regressions rather than counted as gains. Code layout and timing variation matter at this scale; no universal speedup is claimed. Allocation counts were identical across all three runs.

| Workload | Range of run medians, ns/op old | Range of run medians, ns/op new |
|---|---:|---:|
| `fold_ascii` | 11.3..11.8 | 11.7..12.5 |
| `fold_japanese` | 230.3..267.9 | 118.3..132.0 |
| `fold_mixed` | 385.5..404.5 | 138.0..151.0 |
| `fold_accents` | 137.5..151.2 | 128.3..139.6 |
| `fold_late` | 372.5..387.0 | 219.5..226.9 |
| `query_ascii` | 85.1..95.8 | 79.2..87.6 |
| `query_accents` | 184.9..206.8 | 108.0..125.5 |
| `query_nfd` | 160.6..179.4 | 89.2..94.9 |
| `query_japanese` | 261.5..279.1 | 162.6..168.7 |
| `query_spill` | 1,241.4..1,323.0 | 1,067.8..1,154.9 |
| `match_ascii_typo` | 751.8..814.9 | 376.2..441.0 |
| `match_unicode_typo` | 559.0..629.3 | 359.0..451.7 |
| `match_unicode_reject` | 1,668.4..1,720.7 | 590.6..663.8 |
| `match_unicode_long` | 20,799.4..21,538.3 | 7,435.2..8,278.7 |
| `match_unicode_words` | 18,281.8..18,510.0 | 6,804.6..7,692.2 |

These are synthetic operation-level comparisons. They quantify per-input CPU time and allocation churn; no process-RSS, peak-memory, or end-to-end frame-rate change is measured. Catalog composition and real input lengths affect the aggregate benefit. The same production helper already participates in existing song and settings search tests.

## Reproduce

Run from the repository root with other builds and benchmarks idle:

```powershell
cargo test -p deadsync-theme-simply-love --test fuzzy_unicode -- --test-threads=1
cargo test -p deadsync-theme-simply-love --release --test fuzzy_unicode -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib fuzzy -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib search -- --test-threads=1
cargo clippy -p deadsync-theme-simply-love --lib --test fuzzy_unicode -- -D clippy::perf -A clippy::large_enum_variant
cargo check --workspace --bins

Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadsync-theme-simply-love --release --test fuzzy_unicode fuzzy_unicode_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-theme-simply-love --release --test fuzzy_unicode fuzzy_unicode_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-theme-simply-love --release --test fuzzy_unicode fuzzy_unicode_bench -- --ignored --nocapture --test-threads=1
```

Take the median of the three reported medians for each timing metric. Cycle counts are available on Windows; the shared helper prints zero when unavailable on other platforms, which must not be interpreted as a measured improvement.
