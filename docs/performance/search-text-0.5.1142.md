# Song-search text performance, 0.5.1142

Baseline: `453c5a21a` / 0.5.1141. This pass bumps the workspace patch version exactly once to 0.5.1142, with matching lockfile entries.

Three optimizations follow `rust-performance.md` guidance M-HOTPATH (measure repeated CPU/memory work), M-MEM-REUSE (borrow existing storage), and M-INITIAL-CAPACITY (size buffers for their known bounds):

1. **Normalize surviving title spans directly into one output.** Removing difficulty parentheses previously built an intermediate string, a vector of whitespace-separated words, and a joined string. The new helper writes surviving spans into a single input-sized string while carrying pending whitespace across removed groups. It retains the borrowed fast path for titles without parentheses and preserves whitespace/group rules. This runs during search-index construction and result-label generation.
2. **Assemble completion filters in bounded stack storage.** The old code allocated a temporary string for every candidate `[###]` token, built a full output, then collected another string to enforce the character limit. The new scanner copies valid ASCII tokens into a 320-byte stack buffer and appends the permitted Unicode prefix of the label. Only the final, exactly sized string is allocated. Invalid/nested tokens, leading zeros, token order, trailing spaces, and truncation inside a long token retain their old behavior. Ghost completion and completion acceptance use the same public API as before.
3. **Fit text using borrowed UTF-8 prefixes.** Binary-search probes no longer allocate character vectors or candidate strings. The helper borrows the font map once and measures the full text once. It preserves the old binary-search decisions, including negative advances, absent fonts, and nonfinite values. The caller still creates the same owned `TextContent` string for the actor; this final allocation is included in all fitting benchmarks.

The algorithms live in a small production `song_search/text.rs` module so the integration test can compile them unchanged without adding public benchmark APIs. The public completion function is re-exported at its original path.

## Measurement

Windows x64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (`88d9e12ae`), LLVM 22.1.8. Release profile uses `opt-level=3` and LTO. Old function bodies are frozen from the baseline commit, with test visibility and formatting changes only. Both versions execute in the same binary through opaque function pointers with input/output black boxes.

Results are medians of three runs, each with seven timing batches after warm-up. The middle run reverses old/new ordering. Cleanup and completion use 2,000 operations per batch; fitting uses 1,000. Allocation counters measure a separate operation, so tracking is excluded from timing. Windows `QueryThreadCycleTime` counts calling-thread cycles. Inputs and synthetic font/fallback fixtures are constructed outside measurements; returned values are dropped inside the measured operation. All build/check activity finishes before final timing runs.

Throughput units are source Unicode characters for cleanup/fitting, and completed queries for completion assembly. Fitting includes the final owned string required by the UI. These are synthetic operation-level benchmarks, not an end-to-end FPS or process-RSS measurement.

| Workload | ns/op, old -> new | Thread cycles/op, old -> new | Cycles saved | Million units/s, old -> new |
|---|---:|---:|---:|---:|
| Plain title (unchanged fast path) | 18.1 -> 16.9 | 40.5 -> 38.0 | 6.2% | 718.232 -> 766.962 |
| Annotated title, 43 bytes | 425.2 -> 225.3 | 934.2 -> 495.6 | 46.9% | 101.117 -> 190.814 |
| Unicode title with repeated groups, 256 bytes | 1,694.8 -> 1,071.2 | 3,713.5 -> 2,350.0 | 36.7% | 113.284 -> 179.247 |
| Completion without filters | 236.4 -> 120.3 | 513.8 -> 264.8 | 48.5% | 4.230 -> 8.309 |
| Completion with four numeric filters | 1,516.2 -> 158.9 | 3,309.9 -> 349.4 | 89.4% | 0.660 -> 6.293 |
| Completion with Unicode label, capped at 80 characters | 1,123.2 -> 456.0 | 2,435.1 -> 1,001.6 | 58.9% | 0.890 -> 2.193 |
| Completion with malformed tokens | 587.2 -> 161.3 | 1,287.9 -> 354.9 | 72.4% | 1.703 -> 6.198 |
| Short text already within its width budget | 158.9 -> 100.0 | 350.1 -> 221.0 | 36.9% | 56.639 -> 90.000 |
| Clipped ASCII text, 57 characters | 2,413.4 -> 1,200.4 | 5,295.4 -> 2,617.8 | 50.6% | 23.618 -> 47.484 |
| Clipped Unicode text | 7,149.1 -> 3,658.2 | 15,542.1 -> 8,011.5 | 48.5% | 11.750 -> 22.962 |
| Clipped 900-character text (stress) | 19,396.3 -> 11,830.4 | 42,117.0 -> 25,700.6 | 39.0% | 49.494 -> 81.147 |

## Allocation churn

Counters delegate to `System` in the test executable; production allocator configuration is unchanged. Bytes include every allocation and reallocation request, not just the final payload. Allocated and freed byte totals match in every case. Allocation/free counts also match; reallocations are listed separately.

| Workload | Allocations / reallocations / frees, old -> new | Allocated and freed bytes/op, old -> new |
|---|---:|---:|
| Plain title (unchanged fast path) | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Annotated title, 43 bytes | 3/0/3 -> 1/0/1 | 129 -> 43 |
| Unicode title with repeated groups, 256 bytes | 3/3/3 -> 1/0/1 | 1,400 -> 256 |
| Completion without filters | 2/1/2 -> 1/0/1 | 33 -> 9 |
| Completion with four numeric filters | 6/10/6 -> 1/0/1 | 263 -> 49 |
| Completion with Unicode label, capped at 80 characters | 4/6/4 -> 1/0/1 | 1,413 -> 195 |
| Completion with malformed tokens | 7/2/7 -> 1/0/1 | 51 -> 14 |
| Short text already within its width budget | 1/0/1 -> 1/0/1 | 9 -> 9 |
| Clipped ASCII text, 57 characters | 8/2/8 -> 1/0/1 | 587 -> 25 |
| Clipped Unicode text | 9/12/9 -> 1/0/1 | 1,692 -> 34 |
| Clipped 900-character text (stress) | 12/2/12 -> 1/0/1 | 7,971 -> 49 |

Fitting probes have zero heap churn; the table includes ownership conversion at the real caller. Plain titles remain borrowed and allocate nothing. Rewritten titles keep capacity equal to their input size, which can retain more spare capacity than the old tightly joined final string, while eliminating the temporary string/vector churn. The search index converts these temporary strings to `Arc<str>`, so this spare capacity is not retained in the stored titles. Completion uses bounded stack scratch rather than eliminating its required owned output.

## Behavior and checks

Five new tests compare against frozen old functions. They cover 1,024 generated title/query combinations, Unicode whitespace and combining marks, malformed/nested groups and filters, all-annotation titles, long tokens, zero-prefixed filters, truncation at the 80-character limit, multibyte characters at every clipping boundary, fallback/missing glyphs and fonts, negative advances, NaN/infinite widths and zoom, and borrowed-slice identity. Allocation assertions enforce one output allocation for rewrites/completions and zero fitting-probe churn. The existing search tests also exercise index construction, ranking, completion acceptance, filters, and Unicode ghost completions.

Validation completed:

- New integration tests: **5 passed** in debug and **5 passed** in release; manual benchmark ignored during ordinary tests.
- Existing theme tests matching `song_search`: **34 passed**, covering the real public completion API and search UI behavior.
- `cargo check --workspace --bins`: passed.
- Clippy with `-D clippy::perf` initially failed on the existing `SimplyLoveRuntimeRequest` large-enum lint in `src/effects.rs:925`. Rerunning with `-A clippy::large_enum_variant` passed; no source-level suppression was added. Other existing warning classes remain.
- Targeted `rustfmt --check` for the new Rust files and `git diff --check`: passed.

## Variation and limits

Every changed workload improved in all three runs, including reversed ordering. These small benchmarks still show scheduling, cache, and code-layout variation. The unchanged plain-title control measured 18.1 -> 16.9 ns/op across medians; the implementations tie exactly in two individual runs, so that apparent gain is not attributed to a code improvement. The short already-fitting case also varies, so use the ranges below alongside the summary medians. The allocation counts were identical across runs.

| Workload | Range of run medians, ns/op old | Range of run medians, ns/op new |
|---|---:|---:|
| `clean_plain` | 16.9..23.8 | 14.3..18.1 |
| `clean_annotated` | 378.6..442.9 | 224.6..231.2 |
| `clean_unicode` | 1,688.0..1,783.5 | 1,046.1..1,080.3 |
| `completion_plain` | 227.3..238.8 | 91.3..125.8 |
| `completion_filters` | 1,452.2..1,626.2 | 139.7..161.8 |
| `completion_unicode` | 1,073.6..1,129.2 | 414.9..504.7 |
| `completion_invalid` | 585.5..643.3 | 156.0..162.1 |
| `fit_short` | 153.7..201.1 | 97.2..136.6 |
| `fit_clipped` | 2,402.8..2,419.6 | 1,118.3..1,405.6 |
| `fit_unicode` | 6,554.9..7,589.3 | 3,551.8..4,144.8 |
| `fit_long` | 19,185.2..20,192.1 | 11,091.3..12,259.0 |

No process RSS, peak-memory, cache-miss, or end-to-end frame-rate improvement is claimed. The measured memory gains are reduced allocation requests and bytes churned by these helpers. Synthetic font data permits deterministic CPU comparisons without loading a renderer; real title lengths, font metrics, allocator choice, and catalog composition affect the absolute impact.

## Reproduce

Run from the repository root, with other builds and benchmarks idle. The old/new implementations and shared measurement harness are committed with this report.

```powershell
cargo test -p deadsync-theme-simply-love --test search_text -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib song_search -- --test-threads=1
cargo test -p deadsync-theme-simply-love --release --test search_text -- --test-threads=1
cargo clippy -p deadsync-theme-simply-love --lib --test search_text -- -D clippy::perf -A clippy::large_enum_variant
cargo check --workspace --bins

Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadsync-theme-simply-love --release --test search_text search_text_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-theme-simply-love --release --test search_text search_text_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-theme-simply-love --release --test search_text search_text_bench -- --ignored --nocapture --test-threads=1
```

Take the median of the three reported medians for each metric. Cycle counts are available on Windows; the helper prints zero for unavailable cycle counts on other platforms. They must not be interpreted as measured improvements there.
