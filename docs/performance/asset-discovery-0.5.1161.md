# Asset discovery performance — 0.5.1161

Baseline: `071bd24d7` (`0.5.1160`), measured 2026-09-12. This pass increments the workspace patch exactly once to `0.5.1161`, including the three shared-version package entries in Cargo.lock.

## Three changes

The local `rust-performance.md` guidance on measuring hot paths, avoiding transient copies, reusing storage and improving throughput motivated these changes:

1. **Borrow already normalized asset tags.** The private ITG-compatible path normalizer returns `Cow<str>`. Simple filenames and normalized slash-separated paths borrow their original bytes; paths requiring backslash conversion, dot/parent collapse or duplicate-separator removal retain the old normalizer. `resolve_song_path_like_itg` consumes the borrowed view when building its final path, removing a temporary string allocation. Public APIs and the separate public slash-only normalizer are unchanged.
2. **Reuse path storage and entry types during recursive discovery.** `list_song_dir_rel_entries` keeps one mutable full-path buffer per visited directory, pushing and popping sibling filenames. It uses directory-entry file types for ordinary files/directories and follows links or retries missing type information using `fs::metadata`. Only discovered subdirectories receive owned path copies. Returned strings, slash normalization, traversal behavior and longest-first ordering match the previous implementation on the tested fixtures.
3. **Use enumerated regular-file metadata for song cache hashes.** `get_song_directory_hash` avoids a fresh path allocation and metadata lookup for ordinary files. Directories, links and unavailable metadata retain the original path lookup. The directory fallback is essential: tests found enumeration metadata reported a directory length 4,096 bytes smaller than the old query on this filesystem. Hash inputs still use modification time rounded down to Unix seconds plus byte length, with the original wrapping sum, canonical directory hash and resource-fork exclusion.

The active callers include artwork/media path resolution, background/foreground/Lua discovery in `changes.rs`, song cache writing, and cache freshness validation. No persistent filesystem metadata cache or on-disk format change is introduced. Rust documents that `DirEntry::metadata` needs no additional system call on Windows, while Unix generally performs a metadata query; neither entry metadata nor file type follows symlinks. These platform differences motivate the guarded fast paths. [Rust DirEntry documentation](https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.metadata).

## Behavior and project checks

- 211 unit tests passed in both debug and release; seven manual benchmark tests are ignored by ordinary runs (two added here).
- Seven new tests compare more than 1,000 generated normalization strings at every UTF-8 prefix, borrowed/no-churn normalized cases, direct and case-insensitive path resolution, parent paths, missing paths, invalid input, recursive ordering, resource forks, non-UTF-8 filenames, filesystem edits, timestamp rounding including pre-epoch times, and hash error kinds against the frozen old functions.
- Valid directory junctions and a dangling junction exercise the target-following fallback on Windows. Creating file symlinks returns Windows privilege error 1314 in this environment; file-symlink-specific assertions are conditional on successful fixture creation and were not exercised here. On Unix, the same fixture helper creates native file and directory symlinks. Windows junction fixture creation uses PowerShell with paths supplied through environment variables, outside all measured work.
- `cargo clippy -p deadsync-simfile --all-targets --locked -- -D clippy::perf` and root `cargo check --all-targets --locked` passed. Non-performance style warnings remain. Targeted rustfmt and diff checks passed, as did exact version/lockfile, frozen-baseline and benchmark-source audits.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (88d9e12ae, LLVM 22.1.8). Release opt-level 3, full LTO. Six baseline function bodies are frozen from the parent commit, with imports, visibility and formatting adapted. Old and new implementations execute in the same test binary. Build and other Cargo activity finish before benchmarks begin.

Run `cargo test -p deadsync-simfile --lib --release --locked asset_discovery_bench -- --ignored --test-threads=1 --nocapture` three times; set `DEADSYNC_PERF_REVERSE=1` for the middle run. Each operation receives three warmups and seven timing batches. Tables report the median of the three per-run medians. Normalization uses 16,384 iterations/batch, resolution 128, and recursive listing/cache hashing 32. Fixture creation, symlinks/junctions, input cloning and cleanup occur outside measurement. Returned output allocation and destruction are included. Directory queries themselves, including enumeration, are inside measurement.

Timing runs disable allocation counting. The common `tests/support/perf.rs` helper measures Rust allocation/reallocation/free counts and requested/freed bytes in a separate single-operation sample. Thread cycles come from Windows `QueryThreadCycleTime` for the calling thread. They are not retired-instruction counters, system-wide CPU totals or kernel-service profiling. Requested byte churn is not peak live memory or process RSS. These are warmed filesystem measurements, not cold-disk or end-to-end library-load measurements; no system-call count was directly recorded. Unique PID/time fixture paths can have slightly different prefix lengths between runs, affecting absolute byte totals; each old/new pair shares exactly the same path.

Normalization fixtures include empty/simple, Unicode, a 266-byte nested path, redundant components, backslashes and leading parents. Resolution fixtures cover existing, nested, collapsed, missing and differently cased paths. Recursive fixtures contain 0, 8, 128 or 512 files in one directory, or 32 files at each of five levels plus four directories (164 entries). Every seventh filename is Unicode. Hash fixtures enumerate 0, 8, 128 or 512 regular files, or 32 files plus a subdirectory, extra target file and directory junction (35 entries here). Hashing remains nonrecursive. Empty controls count one operation as one unit.

## CPU and elapsed time

Positive cycle savings mean improvement on this machine and workload.

| Scenario | Old µs/op | New µs/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| hash_empty | 249.1562 | 257.3719 | 543,372.2 | 563,415.3 | -3.69% |
| hash_small | 558.3406 | 192.2938 | 1,211,859.5 | 420,630.6 | 65.29% |
| hash_flat128 | 5,849.7125 | 293.7719 | 12,736,494.3 | 642,997.9 | 94.95% |
| hash_flat512 | 23,357.1531 | 607.4406 | 51,103,372.5 | 1,319,771.2 | 97.42% |
| hash_mixed | 1,955.8469 | 340.4688 | 4,234,292.2 | 735,345.6 | 82.63% |
| normalize_empty | 0.0170 | 0.0103 | 37.4 | 21.8 | 41.71% |
| normalize_plain | 0.0740 | 0.0209 | 162.3 | 45.9 | 71.72% |
| normalize_nested | 0.1313 | 0.0731 | 288.1 | 160.4 | 44.32% |
| normalize_long | 0.6741 | 0.6474 | 1,475.9 | 1,419.2 | 3.84% |
| normalize_collapse | 0.1262 | 0.1703 | 276.8 | 373.4 | -34.90% |
| normalize_backslash | 0.1179 | 0.1373 | 258.5 | 301.0 | -16.44% |
| normalize_parents | 0.1255 | 0.1497 | 275.2 | 328.4 | -19.33% |
| resolve_plain | 44.9641 | 45.3438 | 98,416.6 | 99,390.6 | -0.99% |
| resolve_nested | 45.3258 | 45.4383 | 99,354.6 | 99,587.8 | -0.23% |
| resolve_collapse | 43.1422 | 43.6320 | 94,493.0 | 95,528.8 | -1.10% |
| resolve_missing | 134.5406 | 142.4984 | 294,438.7 | 311,377.9 | -5.75% |
| resolve_case | 51.4172 | 51.7336 | 112,709.8 | 112,840.1 | -0.12% |
| entries_empty | 139.1031 | 133.1750 | 304,858.1 | 291,653.8 | 4.33% |
| entries_small | 817.3438 | 108.3500 | 1,787,498.2 | 237,656.8 | 86.70% |
| entries_flat128 | 12,410.6469 | 397.0375 | 27,159,201.3 | 869,528.7 | 96.80% |
| entries_flat512 | 48,596.5062 | 1,463.8125 | 106,353,216.8 | 3,202,168.8 | 96.99% |
| entries_nested | 14,559.0125 | 831.9625 | 31,871,838.8 | 1,822,645.7 | 94.28% |

## Churn and throughput

A/R/F means allocation, reallocation and free call counts. Requested and freed bytes include reallocation traffic; these are cumulative operation totals. The borrow-only normalizer cases have no returned allocation to free.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s | Unit |
|---|---:|---:|---:|---:|---:|---:|---|
| hash_empty | 8/5/8 | 8/5/8 | 1,613/1,613 | 1,613/1,613 | 4,013.5 | 3,885.4 | operations/s |
| hash_small | 56/49/56 | 16/7/16 | 11,797/11,797 | 1,787/1,787 | 14,328.2 | 41,603.0 | entries/s |
| hash_flat128 | 776/683/776 | 136/24/136 | 163,933/163,933 | 4,098/4,098 | 21,881.4 | 435,712.2 | entries/s |
| hash_flat512 | 3080/2713/3080 | 520/79/520 | 650,797/650,797 | 11,507/11,507 | 21,920.5 | 842,880.7 | entries/s |
| hash_mixed | 218/190/218 | 53/20/53 | 45,876/45,876 | 4,696/4,696 | 17,895.1 | 102,799.4 | entries/s |
| normalize_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 58,787,226.4 | 97,292,161.5 | operations/s |
| normalize_plain | 1/0/1 | 0/0/0 | 10/10 | 0/0 | 13,505,894.0 | 47,906,432.7 | operations/s |
| normalize_nested | 1/0/1 | 0/0/0 | 35/35 | 0/0 | 7,617,276.5 | 13,676,126.9 | operations/s |
| normalize_long | 1/0/1 | 0/0/0 | 266/266 | 0/0 | 1,483,399.6 | 1,544,698.6 | operations/s |
| normalize_collapse | 1/0/1 | 1/0/1 | 22/22 | 22/22 | 7,924,163.3 | 5,872,401.4 | operations/s |
| normalize_backslash | 1/0/1 | 1/0/1 | 35/35 | 35/35 | 8,480,331.3 | 7,283,072.5 | operations/s |
| normalize_parents | 1/0/1 | 1/0/1 | 24/24 | 24/24 | 7,971,197.8 | 6,681,074.9 | operations/s |
| resolve_plain | 5/5/5 | 4/5/4 | 1,236/1,236 | 1,226/1,226 | 22,240.0 | 22,053.8 | operations/s |
| resolve_nested | 5/6/5 | 4/6/4 | 2,174/2,174 | 2,150/2,150 | 22,062.5 | 22,007.9 | operations/s |
| resolve_collapse | 5/5/5 | 5/5/5 | 1,247/1,247 | 1,247/1,247 | 23,179.2 | 22,918.9 | operations/s |
| resolve_missing | 18/15/18 | 17/15/17 | 3,928/3,928 | 3,917/3,917 | 7,432.7 | 7,017.6 | operations/s |
| resolve_case | 5/6/5 | 4/6/4 | 2,174/2,174 | 2,150/2,150 | 19,448.7 | 19,329.8 | operations/s |
| entries_empty | 8/5/8 | 8/5/8 | 1,476/1,476 | 1,476/1,476 | 7,188.9 | 7,508.9 | operations/s |
| entries_small | 65/48/65 | 41/48/41 | 13,772/13,772 | 9,428/9,428 | 9,787.8 | 73,834.8 | entries/s |
| entries_flat128 | 905/669/905 | 521/669/521 | 199,288/199,288 | 129,836/129,836 | 10,313.7 | 322,387.7 | entries/s |
| entries_flat512 | 3594/2646/3594 | 2058/2646/2058 | 805,244/805,244 | 527,444/527,444 | 10,535.7 | 349,771.6 | entries/s |
| entries_nested | 1177/1075/1177 | 693/1075/693 | 414,960/414,960 | 316,068/316,068 | 11,264.5 | 197,124.3 | entries/s |

## Tradeoffs and limits

- `hash_empty`: 3.69% more thread cycles; elapsed 249156.2 → 257371.9 ns/op. This is a control/fallback case; filesystem timing is variable.
- `normalize_collapse`: 34.90% more thread cycles; elapsed 126.2 → 170.3 ns/op. The compatibility normalizer now has a preliminary borrowed-path check.
- `normalize_backslash`: 16.44% more thread cycles; elapsed 117.9 → 137.3 ns/op. The compatibility normalizer now has a preliminary borrowed-path check.
- `normalize_parents`: 19.33% more thread cycles; elapsed 125.5 → 149.7 ns/op. The compatibility normalizer now has a preliminary borrowed-path check.
- `resolve_plain`: 0.99% more thread cycles; elapsed 44964.1 → 45343.8 ns/op. This is a control/fallback case; filesystem timing is variable.
- `resolve_nested`: 0.23% more thread cycles; elapsed 45325.8 → 45438.3 ns/op. This is a control/fallback case; filesystem timing is variable.
- `resolve_collapse`: 1.10% more thread cycles; elapsed 43142.2 → 43632.0 ns/op. This is a control/fallback case; filesystem timing is variable.
- `resolve_missing`: 5.75% more thread cycles; elapsed 134540.6 → 142498.4 ns/op. This is a control/fallback case; filesystem timing is variable.
- `resolve_case`: 0.12% more thread cycles; elapsed 51417.2 → 51733.6 ns/op. This is a control/fallback case; filesystem timing is variable.

Per-round cycle savings for representative paths (paired within each run):

- `normalize_plain`: 71.72%, 70.95%, 73.57%.
- `entries_flat512`: 96.99%, 96.93%, 96.63%.
- `hash_flat512`: 97.73%, 97.42%, 96.90%.

Borrowing is limited to already normalized tags; rewritten tags still allocate. Final resolved paths and discovered entry strings remain owned allocations, and output/stack vectors still grow when necessary. Directory and link hashing deliberately retains metadata queries. Gains from enumerated metadata are platform-dependent; Unix may perform an additional lookup for links/directories before the fallback. Enumeration metadata is a per-scan snapshot, so concurrent filesystem changes or permission changes can race with discovery; neither implementation provides an atomic filesystem snapshot. The tested equivalence applies to stable accessible fixtures, including explicit edits between calls. File-symlink fixtures were unavailable on this Windows account, as noted above.

Raw runs, per-run medians, source hashes, commands and logs remain in ignored `target/asset-discovery-perf/`. Committed tests include all fixtures and frozen baselines needed to reproduce the results. All four user-excluded files remain outside the commit.
