# INI-loading performance, 0.5.1145

Baseline: `d53d34a2e` / 0.5.1144. This pass bumps the workspace patch version exactly once to 0.5.1145, including the three matching Cargo.lock package entries.

## Three changes

The local `rust-performance.md` guidance M-HOTPATH, M-INITIAL-CAPACITY, M-MEM-REUSE, and M-THROUGHPUT motivates reducing temporary ownership, repeated map growth, and work unrelated to the requested value.

1. **Reuse borrowed section storage in the configuration INI loader.** Previously, `SimpleIni::load_str` accumulated owned strings and property vectors for every section before building the final maps. It now parses one section at a time, keeps the first property inline, and reuses one vector of borrowed property pairs across sections. Each final section map reserves its required capacity before insertion. Owned strings are created only when inserting into the final map. This removes the outer staging vector and repeated per-section staging allocations while keeping one parse of each line.
2. **Remove repeated section-name copies and lookups from profile parsing.** `ProfileIni` now delegates to the common parser. The old parser cloned the current section name and looked up the outer map for every property. The shared loader holds a mutable reference to the section while inserting its values. The private profile map also adopts the configuration parser's existing `FxHashMap` instead of `std::collections::HashMap`; no dependency is added. Returned property strings, missing-section checks, and profile-loading APIs keep their previous behavior. Profile benchmark gains include both the shared parsing strategy and this map implementation change; they are not additive with configuration gains.
3. **Read language names without constructing translation maps.** The language picker needs only `[Meta] NativeName` from each language file. A borrowed `ini_value` scan now finds that value without allocating keys, values, or maps for unrelated translations. It scans the whole input to preserve last-key-wins behavior across repeated sections. After the file has been read, lookup itself allocates nothing; the language boundary copies just the returned name or fallback into its required `String`.

The two loading paths and single-value lookup share the same line parser. Keys and sections remain case-sensitive; repeated sections merge, later keys replace earlier values, empty sections remain observable, root properties are supported, and Unicode trimming/comment/malformed-line behavior is preserved. Full INI loading still owns its resulting maps and strings. The language file read and returned name still require storage.

## Validation

The frozen baseline contains the previous `SimpleIni` and `ProfileIni` methods. Bodies were audited against `d53d34a2e`: only formatting and profile visibility differ. Configuration map aliases refer to the identical production alias types, preserving the original hasher. The test compiles the actual private profile wrapper unchanged and calls the public configuration API directly.

Five new integration tests cover root/empty/repeated sections, duplicate and empty values, malformed headers, embedded equals signs, comments, case sensitivity, Unicode whitespace/text, BOMs, NUL, 512 generated inputs, reload removal of stale properties, and every parsed property in all 11 bundled language files. Selective lookup is compared against full parsing, including absent keys and late overrides. Allocation checks enforce no churn for borrowed lookup and a one-allocation/name-sized upper bound for the owned English name.

An additional asset test exercises the real file-reading language-name helper against all bundled files and checks missing-file, invalid-UTF-8, missing-metadata, empty-name, and duplicate-name behavior.

- New integration tests: **5 passed in debug and 5 in release**; the manual benchmark is ignored in ordinary runs.
- Complete configuration and profile library suites: **230 configuration tests and 215 profile tests passed** in debug.
- Asset language tests: **5 passed**, including the new file-boundary test.
- Clippy with `-D clippy::perf` for configuration, profile, assets, and the new integration test: passed; existing non-performance warnings remain.
- `cargo check --workspace --bins`, targeted Rust formatting, and `git diff --check`: passed.

## Measurement

Windows x64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (`88d9e12ae`), LLVM 22.1.8. Release profile: opt-level 3, LTO. Old and new execute in the same binary through opaque function pointers and input/output black boxes. Calling-thread cycles come from `QueryThreadCycleTime`.

The tables take the median of three final run medians, with old/new order reversed in the middle run. Each run uses seven timing batches after three warm-up operations. Batch iterations are `(2_000_000 / input_bytes.max(1)).clamp(4, 2000)`. Fixtures are prepared outside timing. Parser timing includes allocating, populating, and dropping the resulting maps. Name timing includes lookup, creating the returned `String`, and dropping it. File reads, directory enumeration, logging, and the language UI are outside timing; the name benchmark reproduces the successful in-memory part of the actual file helper.

Fixtures include an empty control, a one-property configuration, 640 generated settings in eight sections, the actual bundled English and Japanese files, 256 repetitions of a section with a duplicate key, and absent/repeated metadata. English/Japanese profile rows are grammar/size stress controls for the profile parser, not claims that profile loading consumes translation files.

Allocation counts are measured in a separate single operation using thread-local counters. Counting is disabled during timing, while the same `System` allocator wrapper remains installed for both versions. Allocated and freed totals match and were stable across all three runs. Requested bytes include all allocation and reallocation requests; they are cumulative churn rather than live memory or RSS. Throughput counts source bytes/s; no byte throughput is reported for empty input.

| Workload | ns/op old -> new | Thread cycles/op old -> new | Cycles saved | MiB/s old -> new |
|---|---:|---:|---:|---:|
| Config: empty control | 42.6 -> 26.9 | 94.4 -> 59.6 | 36.9% | n/a |
| Profile: empty control | 24.6 -> 26.1 | 54.7 -> 58.1 | -6.2% | n/a |
| Config: one property | 587.8 -> 460.1 | 1,288.5 -> 1,008.9 | 21.7% | 32.45 -> 41.45 |
| Profile: one property | 660.5 -> 575.3 | 1,448.8 -> 1,257.8 | 13.2% | 28.88 -> 33.15 |
| Config: 640 settings | 159,871.1 -> 154,104.0 | 348,152.0 -> 335,980.8 | 3.5% | 80.03 -> 83.03 |
| Profile: 640 settings | 251,763.8 -> 143,579.9 | 547,715.9 -> 313,303.1 | 42.8% | 50.82 -> 89.11 |
| Config: bundled English | 847,352.9 -> 807,741.2 | 1,852,076.4 -> 1,761,268.1 | 4.9% | 125.86 -> 132.03 |
| Profile: bundled English | 1,081,594.1 -> 707,558.8 | 2,356,642.5 -> 1,551,129.0 | 34.2% | 98.60 -> 150.72 |
| Config: bundled Japanese | 37,523.7 -> 35,723.2 | 81,716.6 -> 77,840.0 | 4.7% | 143.34 -> 150.57 |
| Profile: bundled Japanese | 61,721.5 -> 35,928.0 | 135,294.5 -> 77,847.5 | 42.5% | 87.15 -> 149.71 |
| Config: duplicate-heavy | 163,162.7 -> 136,964.7 | 357,143.1 -> 299,926.7 | 16.0% | 46.39 -> 55.26 |
| Profile: duplicate-heavy | 223,788.5 -> 133,408.3 | 490,344.7 -> 291,148.4 | 40.6% | 33.82 -> 56.73 |
| Name: bundled English | 719,117.6 -> 217,935.3 | 1,552,846.3 -> 476,921.9 | 69.3% | 148.30 -> 489.34 |
| Name: bundled Japanese | 38,273.7 -> 9,559.9 | 83,223.4 -> 20,698.7 | 75.1% | 140.53 -> 562.63 |
| Name: missing metadata | 156,322.8 -> 34,463.8 | 339,939.2 -> 75,571.4 | 77.8% | 81.85 -> 371.24 |
| Name: last override | 1,523.2 -> 305.1 | 3,341.1 -> 665.3 | 80.1% | 42.58 -> 212.55 |

## Allocation churn

| Workload | Allocations / reallocations / frees old -> new | Allocated and freed bytes/op old -> new |
|---|---:|---:|
| Config: empty control | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Profile: empty control | 0/0/0 -> 0/0/0 | 0 -> 0 |
| Config: one property | 7/0/7 -> 5/0/5 | 855 -> 471 |
| Profile: one property | 7/0/7 -> 5/0/5 | 549 -> 471 |
| Config: 640 settings | 1306/41/1306 -> 1300/5/1300 | 160,688 -> 72,124 |
| Profile: 640 settings | 1987/0/1987 -> 1300/5/1300 | 122,828 -> 72,124 |
| Config: bundled English | 4215/174/4215 -> 4143/7/4143 | 527,302 -> 302,898 |
| Profile: bundled English | 6400/0/6400 -> 4143/7/4143 | 446,571 -> 302,898 |
| Config: bundled Japanese | 293/19/293 -> 279/4/279 | 36,365 -> 22,777 |
| Profile: bundled Japanese | 431/0/431 -> 279/4/279 | 28,289 -> 22,777 |
| Config: duplicate-heavy | 1539/6/1539 -> 1283/0/1283 | 109,092 -> 6,728 |
| Profile: duplicate-heavy | 2050/0/2050 -> 1283/0/1283 | 12,040 -> 6,728 |
| Name: bundled English | 4216/174/4216 -> 1/0/1 | 527,309 -> 7 |
| Name: bundled Japanese | 294/19/294 -> 1/0/1 | 36,374 -> 9 |
| Name: missing metadata | 1307/41/1307 -> 1/0/1 | 160,696 -> 8 |
| Name: last override | 17/0/17 -> 1/0/1 | 1,494 -> 4 |

The borrowed staging vector can grow to accommodate the largest section, so full parsing still has some reallocations. The previous profile parser had no vector reallocations but performed more string/map allocations. The table reports these separately instead of treating one counter as a complete memory result.

## Variation and limits

Parsing 640 settings through the configuration loader saved **3.5%** median thread cycles. Parsing the English configuration saved **4.9%** median thread cycles. Parsing 640 settings through the profile wrapper saved **42.8%** median thread cycles. Reading the English native name from loaded text saved **69.3%** median thread cycles.

Configuration timing gains are modest and the settings/English run ranges overlap; both cases were slower in the reversed-order run. Their allocation-byte reductions were stable (55.1% for 640 settings and 42.6% for English). The ranges below show run-to-run variation, not confidence intervals.

Measured control/regression cases are retained: Profile: empty control measured 24.6 -> 26.1 ns/op (6.2% more cycles). No universal speedup is claimed.

| Workload | Range of run medians, ns/op old | Range of run medians, ns/op new |
|---|---:|---:|
| `config_empty` | 32.6..47.6 | 26.0..28.6 |
| `profile_empty` | 24.1..25.3 | 26.0..27.7 |
| `config_tiny` | 571.8..599.0 | 456.6..545.5 |
| `profile_tiny` | 601.5..766.4 | 469.6..592.1 |
| `config_profile` | 153,594.0..160,889.9 | 146,098.7..160,180.5 |
| `profile_profile` | 250,827.5..253,340.3 | 143,152.3..172,002.7 |
| `config_english` | 737,552.9..982,105.9 | 705,100.0..908,382.4 |
| `profile_english` | 1,066,052.9..1,215,205.9 | 673,382.4..720,929.4 |
| `config_japanese` | 37,199.4..38,504.8 | 35,467.2..36,301.4 |
| `profile_japanese` | 60,581.1..69,078.2 | 35,188.7..42,410.2 |
| `config_duplicates` | 162,552.8..165,087.7 | 130,606.3..138,806.7 |
| `profile_duplicates` | 216,425.4..245,956.7 | 128,866.7..134,888.5 |
| `name_english` | 707,047.1..738,741.2 | 211,041.2..227,729.4 |
| `name_japanese` | 37,486.7..43,903.1 | 9,422.9..9,938.4 |
| `name_absent` | 153,820.8..158,644.3 | 33,855.0..37,321.5 |
| `name_repeated` | 1,459.2..1,548.5 | 295.2..328.4 |

These comparisons measure in-memory operations on one CPU/compiler configuration. They do not measure disk I/O, end-to-end startup or menu latency, peak resident memory, or process RSS. Section sizes, repeated keys, and file size influence aggregate benefits. Zero-allocation lookup does not imply zero-allocation file loading or zero allocation for a fully owned INI result.

## Reproduce

Run from the repository root with other builds and benchmarks idle:

```powershell
cargo test -p deadsync-profile --test ini_loading -- --test-threads=1
cargo test -p deadsync-profile --release --test ini_loading -- --test-threads=1
cargo test -p deadsync-config -p deadsync-profile --lib -- --test-threads=1
cargo test -p deadsync-assets --lib language -- --test-threads=1
cargo clippy -p deadsync-profile --lib --test ini_loading -- -D clippy::perf
cargo clippy -p deadsync-assets -p deadsync-config --lib -- -D clippy::perf
cargo check --workspace --bins

Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadsync-profile --release --test ini_loading ini_loading_bench -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-profile --release --test ini_loading ini_loading_bench -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-profile --release --test ini_loading ini_loading_bench -- --ignored --nocapture --test-threads=1
```

Take the median of the three reported medians for each metric. The shared helper reports zero cycles on non-Windows platforms where the counter is unavailable; that value must not be interpreted as a measured improvement.
