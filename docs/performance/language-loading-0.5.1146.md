# Full language-loading performance, 0.5.1146

Baseline: `fd44d31f1` / 0.5.1145. This pass bumps the workspace patch version exactly once to 0.5.1146, including the matching Cargo.lock entries for deadsync, deadsync-theme-simply-love, and deadsync-version.

## Three changes

The local `rust-performance.md` guidance M-HOTPATH, M-MEM-REUSE, M-INITIAL-CAPACITY, and M-THROUGHPUT motivates avoiding temporary ownership, reserving known collection sizes, and copying unchanged text in chunks.

1. **Borrow INI strings during language preparation.** The configuration parser now shares its section-building implementation between owned `SimpleIni` maps and a new `BorrowedIniSections` result. Language loading uses borrowed names, keys, and values while the file text is alive. It no longer creates intermediate owned strings just to turn them into final `Box<str>` keys and `Arc<str>` translations. The borrowed parser still allocates map tables and a reusable section buffer; it does not allocate storage per string. Other configuration/profile users retain their owned API and the same parsing strategy.
2. **Reserve final translation maps before insertion.** The builder reserves the outer section count and each section's retained-value count. This removes repeated table growth and rehashing while building thousands of translations. Values equal to `@skip` are excluded from the reservation; fully skipped and empty sections remain present with an empty, unallocated entry table. The fully skipped case skips the insertion scan after counting.
3. **Copy unchanged escape-decoding spans in bulk.** The INI decoder copies long UTF-8 spans between backslashes instead of decoding and re-encoding each scalar. It retains the scalar decoder for short or closely spaced escape tails (less than 32 bytes before the next escape), avoiding repeated substring searches on dense escape sequences. Supported escapes, unknown escapes, trailing backslashes, output bytes, and the single output-string allocation are preserved.

Duplicate sections/keys are resolved before translation conversion. In particular, a final `@skip` suppresses an earlier visible value, a later visible value replaces `@skip`, and escaped whitespace around the literal text `@skip` is decoded only after the skip decision. Root and empty sections, case-sensitive keys, and all previous INI trimming/comment rules remain intact. Final maps own their keys and values independently of the file buffer.

## Validation

The frozen baseline copies the previous configuration parser and escape decoder. Its language converter copies the successful in-memory portion of `load_ini_to_map`, excluding file I/O. These bodies were audited against `fd44d31f1`; formatting is the only body change. Tests call the public configuration API, compile the private production language-map module unchanged, and exercise the actual public language bundle loader.

Five new tests cover 512 generated INI inputs, repeated/root/empty sections, duplicate keys and skip overrides, malformed headers, case sensitivity, Unicode whitespace, BOMs, NULs, and every property in all 11 bundled language files. Public bundle tests compare active and English fallback maps, locale strings, and missing-language behavior. The escape test compares every one of the 1,112,064 valid Unicode scalars following a backslash after a long unchanged prefix, all triples of 12 text/escape fragments, and boundaries around the 32-byte scalar/chunk decision. Pointer checks confirm borrowed strings refer to the original input. Allocation checks cap a two-property borrowed parse at three small allocations regardless of its long values, cap decoding at one input-sized allocation, and require zero churn for empty decoding.

- New integration tests: **5 passed in debug and 5 in release**; the manual benchmark is ignored in ordinary runs.
- Complete assets, configuration, and profile library suites: **571 tests passed** in debug.
- Previous INI-loading regression tests: **5 passed** in debug.
- Clippy with `-D clippy::perf` for assets, configuration, and the new integration test: passed; existing non-performance warnings remain.
- `cargo check --workspace --bins`, targeted Rust formatting, and `git diff --check`: passed.

## Measurement

Windows x64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (`88d9e12ae`), LLVM 22.1.8. Release profile: opt-level 3, LTO. Functions execute in the same binary through opaque function pointers with input/output black boxes. Windows `QueryThreadCycleTime` measures calling-thread cycles.

Three final runs occur after builds/checks finish, reversing old/new order in the middle run. Each run has seven timed batches after three warm-up operations; the tables take the median of the three reported medians for each metric. Parser/map/full-pipeline batch iterations are `(2_000_000 / input_bytes.max(1)).clamp(4, 2000)`; escape decoding uses 5,000 operations per batch. Inputs and pre-parsed maps are prepared outside timing. All returned maps and strings are dropped inside the measured operation.

The stage comparisons have deliberately different scopes:

- **Borrowed parse:** the frozen owned INI parser versus the new borrowed parser, including allocation/destruction. The representations differ in ownership; tests establish identical section/key/value content.
- **Map reservation:** the same pre-parsed borrowed maps and current decoder on both sides. The control applies the old grow-on-insert strategy to that borrowed representation; it is an adapted capacity-only control, not the frozen full language converter.
- **Full language:** frozen owned parsing plus the frozen language converter/decoder versus the complete new in-memory pipeline. This is the combined old/new comparison used for overall conclusions. Stage gains overlap and must not be added together.
- **Owned parser control:** old/new owned `SimpleIni` parsing of English, checking the shared generic implementation on the existing ownership path.
- **Escapes:** old/new string decoding, including output allocation and destruction.

Full-language fixtures use actual bundled English, Japanese, and pseudo files, plus empty, tiny, and fully skipped controls. Escape fixtures include long ASCII/Unicode strings, sparse escapes, dense escapes, and a long plain prefix followed by dense escapes. Allocation counting runs separately for one operation with thread-local counters. Tracking is disabled for timing while the same `System` allocator wrapper remains installed. Throughput counts source bytes/s; map-reservation throughput uses the originating file size as a normalization unit, even though parsing is outside that stage. Empty input has no byte throughput.

| Workload | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | MiB/s old -> new |
|---|---:|---:|---:|---:|
| Borrowed parse: empty | 22.4 -> 27.0 | 50.0 -> 59.9 | -19.8% | n/a |
| Map reservation: empty | 9.2 -> 11.7 | 21.1 -> 26.3 | -24.6% | n/a |
| Full language: empty | 34.0 -> 38.0 | 75.3 -> 84.3 | -12.0% | n/a |
| Borrowed parse: one property | 456.5 -> 285.8 | 1,000.6 -> 627.0 | 37.3% | 16.71 -> 26.69 |
| Map reservation: one property | 358.6 -> 383.4 | 776.7 -> 842.2 | -8.4% | 21.27 -> 19.90 |
| Full language: one property | 832.5 -> 637.1 | 1,808.5 -> 1,399.2 | 22.6% | 9.16 -> 11.97 |
| Borrowed parse: English | 903,835.3 -> 294,264.7 | 1,974,015.1 -> 645,058.9 | 67.3% | 117.99 -> 362.41 |
| Map reservation: English | 644,823.5 -> 558,382.4 | 1,402,462.9 -> 1,224,164.4 | 12.7% | 165.39 -> 190.99 |
| Full language: English | 1,194,188.2 -> 864,205.9 | 2,602,327.3 -> 1,887,893.8 | 27.5% | 89.30 -> 123.40 |
| Borrowed parse: Japanese | 36,371.2 -> 14,556.2 | 79,065.3 -> 31,929.8 | 59.6% | 147.88 -> 369.51 |
| Map reservation: Japanese | 30,570.3 -> 28,695.2 | 66,708.2 -> 62,218.3 | 6.7% | 175.95 -> 187.44 |
| Full language: Japanese | 62,191.0 -> 50,252.5 | 135,791.1 -> 109,584.4 | 19.3% | 86.49 -> 107.03 |
| Borrowed parse: pseudo | 714,841.7 -> 261,808.3 | 1,550,127.2 -> 573,279.1 | 63.0% | 216.68 -> 591.63 |
| Map reservation: pseudo | 676,808.3 -> 609,133.3 | 1,482,356.8 -> 1,326,474.9 | 10.5% | 228.86 -> 254.28 |
| Full language: pseudo | 1,269,008.3 -> 903,833.3 | 2,765,462.2 -> 1,972,097.8 | 28.7% | 122.06 -> 171.37 |
| Borrowed parse: 500 skipped values | 104,991.0 -> 33,432.7 | 229,918.5 -> 72,976.7 | 68.3% | 58.16 -> 182.65 |
| Map reservation: 500 skipped values | 3,853.8 -> 4,035.9 | 8,445.1 -> 8,624.5 | -2.1% | 1584.49 -> 1513.02 |
| Full language: 500 skipped values | 106,652.9 -> 38,749.4 | 233,103.4 -> 84,279.6 | 63.8% | 57.25 -> 157.59 |
| Owned parser control: English | 768,535.3 -> 786,382.4 | 1,665,540.2 -> 1,720,608.8 | -3.3% | 138.76 -> 135.61 |
| Escapes: empty | 11.2 -> 12.4 | 24.9 -> 27.4 | -10.0% | n/a |
| Escapes: a\nb | 70.6 -> 63.6 | 155.2 -> 139.8 | 9.9% | 54.03 -> 59.98 |
| Escapes: plain ASCII | 1,261.6 -> 175.8 | 2,740.4 -> 386.1 | 85.9% | 628.91 -> 4514.43 |
| Escapes: plain Unicode | 1,358.3 -> 179.2 | 2,915.9 -> 389.5 | 86.6% | 629.11 -> 4767.31 |
| Escapes: sparse escapes | 2,340.4 -> 286.4 | 5,087.0 -> 628.0 | 87.7% | 706.98 -> 5778.13 |
| Escapes: dense escapes | 1,405.4 -> 1,528.5 | 3,055.0 -> 3,351.9 | -9.7% | 694.87 -> 638.91 |
| Escapes: late dense escapes | 2,513.6 -> 1,640.9 | 5,478.2 -> 3,597.1 | 34.3% | 704.17 -> 1078.70 |

## Allocation churn

Allocated and freed byte totals match, and allocation counts were identical across all three runs. Requested bytes include all allocation/reallocation requests; these totals measure cumulative churn rather than peak live storage or RSS. Table growth can allocate/free replacement tables without using the allocator's `realloc` entry point, so both kinds of counters matter.

| Workload | Allocations / reallocations / frees old -> new | Allocated and freed bytes/op old -> new |
|---|---:|---:|
| Borrowed parse: empty | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Map reservation: empty | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Full language: empty | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Borrowed parse: one property | 5/0/5 -> 2/0/2 | 459 -> 360 |
| Map reservation: one property | 5/0/5 -> 5/0/5 | 386 -> 386 |
| Full language: one property | 8/0/8 -> 7/0/7 | 843 -> 746 |
| Borrowed parse: English | 4143/7/4143 -> 84/7/84 | 302,898 -> 152,048 |
| Map reservation: English | 4475/0/4475 -> 4280/0/4280 | 377,100 -> 272,336 |
| Full language: English | 6550/7/6550 -> 4364/7/4364 | 647,986 -> 424,384 |
| Borrowed parse: Japanese | 279/4/279 -> 22/4/22 | 22,777 -> 13,976 |
| Map reservation: Japanese | 295/0/295 -> 276/0/276 | 22,432 -> 16,268 |
| Full language: Japanese | 438/4/438 -> 298/4/298 | 43,040 -> 30,244 |
| Borrowed parse: pseudo | 4071/7/4071 -> 83/7/83 | 355,541 -> 146,752 |
| Map reservation: pseudo | 4399/0/4399 -> 4208/0/4208 | 446,755 -> 347,203 |
| Full language: pseudo | 6438/7/6438 -> 4291/7/4291 | 770,691 -> 493,955 |
| Borrowed parse: 500 skipped values | 1004/7/1004 -> 3/7/3 | 88,476 -> 66,660 |
| Map reservation: 500 skipped values | 2/0/2 -> 2/0/2 | 222 -> 222 |
| Full language: 500 skipped values | 1005/7/1005 -> 5/7/5 | 88,688 -> 66,882 |
| Owned parser control: English | 4143/7/4143 -> 4143/7/4143 | 302,898 -> 302,898 |
| Escapes: empty | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Escapes: a\nb | 1/0/1 -> 1/0/1 | 4 -> 4 |
| Escapes: plain ASCII | 1/0/1 -> 1/0/1 | 832 -> 832 |
| Escapes: plain Unicode | 1/0/1 -> 1/0/1 | 896 -> 896 |
| Escapes: sparse escapes | 1/0/1 -> 1/0/1 | 1,735 -> 1,735 |
| Escapes: dense escapes | 1/0/1 -> 1/0/1 | 1,024 -> 1,024 |
| Escapes: late dense escapes | 1/0/1 -> 1/0/1 | 1,856 -> 1,856 |

## Variation and limits

English full loading used **27.5% fewer median thread cycles** and **34.5% fewer requested allocation bytes**. Japanese full loading used **19.3% fewer median thread cycles** and **29.7% fewer requested allocation bytes**. Pseudo full loading used **28.7% fewer median thread cycles** and **35.9% fewer requested allocation bytes**.

Measured regressions/controls are retained:

- Borrowed parse: empty: 22.4 -> 27.0 ns/op, 19.8% more cycles.
- Map reservation: empty: 9.2 -> 11.7 ns/op, 24.6% more cycles.
- Full language: empty: 34.0 -> 38.0 ns/op, 12.0% more cycles.
- Map reservation: one property: 358.6 -> 383.4 ns/op, 8.4% more cycles.
- Map reservation: 500 skipped values: 3,853.8 -> 4,035.9 ns/op, 2.1% more cycles.
- Owned parser control: English: 768,535.3 -> 786,382.4 ns/op, 3.3% more cycles.
- Escapes: empty: 11.2 -> 12.4 ns/op, 10.0% more cycles.
- Escapes: dense escapes: 1,405.4 -> 1,528.5 ns/op, 9.7% more cycles.

These are operation-level measurements on one CPU/compiler configuration. Small controls are sensitive to dispatch and code layout, and the run ranges below show variation rather than confidence intervals. No universal speedup is claimed. File reading, directory enumeration, overall startup/menu latency, peak resident memory, and process RSS are not measured. Borrowed parsing still needs map tables and retains the source text until conversion finishes; final owned translations still allocate. The unchanged owned parser control must not be counted as a separate optimization.

| Workload | Range of run medians, ns/op old | Range of run medians, ns/op new |
|---|---:|---:|
| `parse_empty` | 21.7..23.9 | 26.2..28.9 |
| `reserve_empty` | 8.9..9.8 | 11.7..12.2 |
| `language_empty` | 33.0..34.0 | 37.0..39.6 |
| `parse_tiny` | 455.8..496.6 | 277.6..378.3 |
| `reserve_tiny` | 354.6..419.1 | 377.1..443.1 |
| `language_tiny` | 726.8..835.8 | 608.9..646.8 |
| `parse_english` | 883,647.1..922,594.1 | 277,382.4..372,017.6 |
| `reserve_english` | 578,294.1..651,658.8 | 554,241.2..604,694.1 |
| `language_english` | 1,155,435.3..1,246,700.0 | 863,264.7..887,864.7 |
| `parse_japanese` | 34,395.8..39,114.1 | 13,774.0..14,713.6 |
| `reserve_japanese` | 29,599.4..31,719.8 | 28,473.2..31,400.0 |
| `language_japanese` | 59,908.8..71,402.8 | 48,354.8..51,894.9 |
| `parse_pseudo` | 666,800.0..763,425.0 | 255,616.7..271,558.3 |
| `reserve_pseudo` | 672,308.3..697,100.0 | 601,608.3..633,808.3 |
| `language_pseudo` | 1,265,358.3..1,287,108.3 | 885,583.3..908,091.7 |
| `parse_skipped` | 102,332.4..109,232.4 | 33,101.3..33,637.2 |
| `reserve_skipped` | 3,572.1..4,716.7 | 3,451.3..4,043.3 |
| `language_skipped` | 105,326.0..115,701.9 | 37,244.2..44,009.6 |
| `owned_english` | 707,835.3..858,217.6 | 744,035.3..850,958.8 |
| `escape_empty` | 10.9..13.6 | 12.0..14.7 |
| `escape_tiny` | 69.9..82.2 | 62.4..71.5 |
| `escape_plain` | 1,168.5..1,344.4 | 155.7..185.1 |
| `escape_unicode` | 1,314.7..1,370.9 | 156.7..325.5 |
| `escape_sparse` | 2,326.4..2,544.5 | 273.2..365.6 |
| `escape_dense` | 1,285.6..1,427.1 | 1,426.1..1,564.7 |
| `escape_late_dense` | 2,426.2..2,605.4 | 1,576.7..1,665.5 |

## Reproduce

Run from the repository root with other builds and benchmarks idle:

```powershell
cargo test -p deadsync-assets --test language_loading -- --test-threads=1
cargo test -p deadsync-assets --release --test language_loading -- --test-threads=1
cargo test -p deadsync-config -p deadsync-assets -p deadsync-profile --lib -- --test-threads=1
cargo test -p deadsync-profile --test ini_loading -- --test-threads=1
cargo clippy -p deadsync-assets -p deadsync-config --lib --test language_loading -- -D clippy::perf
cargo check --workspace --bins

Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadsync-assets --release --test language_loading language_loading_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-assets --release --test language_loading language_loading_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-assets --release --test language_loading language_loading_bench -- --ignored --nocapture --test-threads=1
```

Take the median of the three reported medians for each metric. On non-Windows platforms the shared helper reports zero cycles when that counter is unavailable; zero must not be interpreted as a measured improvement.
