# Performance pass: copies, permutations and song-match keys

Branch: `perf/1872-copy-reductions-20261009`. Starting committed main: `625783f6153b00c9932743825483dc7dcd28bb5b`. Version: **0.5.1871 -> 0.5.1872**, updated in Cargo.toml and all three matching workspace entries in Cargo.lock.

Read rust-performance.md and inspected 25 distinct unmerged perf branch heads before choosing these targets. The pending response pass changes banner eviction (`overflow`); this pass changes failure-set mutation only. The pending archive passes change `merge_ranges`, `group_folders`, and metadata ownership in `settle`/`match_song`; those functions remain unchanged here. No reviewed pending branch changes downloads.rs. New test filenames avoid collisions with those branches. The original checkout and its independent work are left untouched; this branch remains anchored to the starting commit. No merge or push was performed.

## Changes

1. **Banner failure snapshots:** use `Arc::make_mut` when inserting or removing a failed ID. A uniquely owned set no longer requires a full HashSet clone and a new Arc allocation for every update. Held readers keep their original snapshot, and a batch needs at most one copy for a reader held across the batch. Missing removals and duplicate insertions retain their guards, while retry counts, deadlines, slot updates and publication rules stay unchanged. The test mutex visibility change matches the pending response pass and adds no production behavior.
2. **Download-folder ordering:** use the source indices already present in the sorted key records to follow permutation cycles directly. Remove three byte-encoding/decoding helpers, the inverse-map construction, and zeroing/swapping of the key byte buffer. Zero- and one-path inputs return immediately. Stable case-insensitive filename ordering and existing PathBuf buffers are preserved. The key buffer also no longer reserves space for a usize permutation when filenames are shorter than that representation.
3. **Song-match keys:** for ASCII input, count the retained letters/digits and fill one exactly sized lowercase String. This skips Unicode lowercase expansion and repeated String growth without retaining excess capacity. Punctuation-only input keeps the original trimmed lowercase fallback. Non-ASCII input retains the original per-character lowercase-then-filter loop, including its behavior for Greek sigma and dotted capital I. This is separate from the pending optimization that borrows cached metadata during matching.

All production changes are outside Song Lua. Original implementations are frozen from the starting commit and compiled only for tests. Audits verify the originals, unchanged functions, exact retry mutation scope, and the unchanged Unicode fallback.

## Benchmark method

Windows x86-64/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, `rustc 1.98.1 (48a229cea 2026-09-01)`, release optimization with LTO disabled. These are function-level wall-time benchmarks, not application FPS or CPU-cycle measurements. This pass runs no build or native compatibility job during measurements; unrelated workstation activity is left alone.

Every case alternates original/current implementations for ten sample pairs, discards the warm-up pair, and reports the median of nine samples per implementation. Inputs/results are black-boxed. Key and matcher batches calibrate fast cases to approximately 2 ms. Sorting clones input paths outside timing and includes destruction of the returned paths on both sides.

Banner cases call the actual production/original mutation functions. They restore the singleton, its map capacities/hash seeds, and the requested reader ownership outside each measured call. Both variants include the same timer, dispatch and mutex overhead. A sample contains 500 single-update calls or 100 multi-update calls; ns/op means time for the stated update count. Reader drops and fixture destruction occur outside timing. The existing banner test mutex serializes these tests with the rest of the banner suite.

The recorded complete matching cases used the same metadata fixture and owned callback on both sides; only key normalization differed. After merging main's borrowed-metadata optimization, the benchmark uses borrowed callbacks on both sides; the baseline matcher is adapted to that API while retaining its original key normalization and matching rules. The tables below record the original owned-callback runs, rather than new measurements of the combined implementation.

## Timing results

| Case | Original ns/op | Current ns/op | Throughput ratio |
|---|---:|---:|---:|
| failure-0-unique-insert-32 | 20,623.00 | 5,462.00 | 3.78x |
| failure-0-shared-insert-1 | 259.60 | 275.00 | 0.94x |
| failure-24-shared-insert-1 | 289.60 | 299.40 | 0.97x |
| failure-512-unique-insert-1 | 801.40 | 188.40 | 4.25x |
| failure-512-shared-insert-1 | 831.00 | 797.40 | 1.04x |
| failure-512-weak-insert-1 | 851.60 | 256.60 | 3.32x |
| failure-512-unique-insert-32 | 17,967.00 | 3,741.00 | 4.80x |
| failure-512-shared-insert-32 | 16,723.00 | 4,260.00 | 3.93x |
| failure-512-shared-duplicate-1 | 168.00 | 166.20 | 1.01x |
| failure-512-unique-remove-32 | 14,735.00 | 1,953.00 | 7.54x |
| failure-512-shared-remove-1 | 720.60 | 709.80 | 1.02x |
| failure-4096-shared-remove-32 | 134,171.00 | 9,560.00 | 14.03x |
| paths-0-sorted | 16.12 | 9.64 | 1.67x |
| paths-1-sorted | 418.96 | 72.56 | 5.77x |
| paths-24-reverse | 5,725.25 | 5,616.92 | 1.02x |
| paths-200-reverse | 44,764.44 | 44,054.00 | 1.02x |
| paths-1200-shuffled | 458,460.00 | 440,600.00 | 1.04x |
| paths-1200-sorted | 258,240.00 | 260,820.00 | 0.99x |
| paths-1200-short | 283,600.00 | 281,690.00 | 1.01x |
| paths-200-unicode | 43,292.00 | 43,842.00 | 0.99x |
| paths-4096-reverse | 974,160.00 | 959,480.00 | 1.02x |
| key-empty | 28.82 | 22.00 | 1.31x |
| key-lower | 389.94 | 124.62 | 3.13x |
| key-ascii | 771.63 | 178.42 | 4.32x |
| key-punctuation | 357.56 | 123.55 | 2.89x |
| key-greek | 1,540.86 | 1,578.93 | 0.98x |
| key-japanese | 1,067.16 | 1,092.72 | 0.98x |
| key-mixed | 868.65 | 885.13 | 0.98x |
| key-late-unicode | 66,358.00 | 67,812.00 | 0.98x |
| key-match-0-ascii-Song | 224.07 | 164.01 | 1.37x |
| key-match-24-ascii-Folder-12 | 5,778.53 | 3,292.60 | 1.76x |
| key-match-200-ascii-Catalog-Title-112 | 307,940.00 | 169,125.00 | 1.82x |
| key-match-1200-ascii-Catalog-Title-112 | 1,942,440.00 | 1,024,880.00 | 1.90x |
| key-match-200-ascii-Missing-Song | 821,860.00 | 471,480.00 | 1.74x |
| key-match-200-unicode-東京-Title-112 | 344,310.00 | 294,100.00 | 1.17x |

All primary cases are shown, including controls. Cases with a higher current median: `failure-0-shared-insert-1` +15.40 ns (5.93%); `failure-24-shared-insert-1` +9.80 ns (3.38%); `paths-1200-sorted` +2580.00 ns (1.00%); `paths-200-unicode` +550.00 ns (1.27%); `key-greek` +38.07 ns (2.47%); `key-japanese` +25.56 ns (2.40%); `key-mixed` +16.48 ns (1.90%); `key-late-unicode` +1454.00 ns (2.19%).

Three fixed repeats of the same binary checked the initially slower cases; the primary table remains the first run.

| Repeat | Case | Original ns/op | Current ns/op | Current time change |
|---:|---|---:|---:|---:|
| 1 | failure-0-shared-insert-1 | 254.00 | 267.80 | +5.43% |
| 1 | failure-24-shared-insert-1 | 282.20 | 273.00 | -3.26% |
| 1 | paths-1200-sorted | 214,740.00 | 214,070.00 | -0.31% |
| 1 | paths-200-unicode | 37,261.67 | 35,646.67 | -4.33% |
| 1 | key-greek | 1,296.88 | 1,312.00 | +1.17% |
| 1 | key-japanese | 1,031.61 | 981.26 | -4.88% |
| 1 | key-mixed | 803.19 | 781.67 | -2.68% |
| 1 | key-late-unicode | 61,892.00 | 58,448.00 | -5.56% |
| 2 | failure-0-shared-insert-1 | 269.40 | 270.80 | +0.52% |
| 2 | failure-24-shared-insert-1 | 310.60 | 309.00 | -0.52% |
| 2 | paths-1200-sorted | 249,440.00 | 242,290.00 | -2.87% |
| 2 | paths-200-unicode | 43,680.00 | 41,990.91 | -3.87% |
| 2 | key-greek | 1,521.23 | 1,541.21 | +1.31% |
| 2 | key-japanese | 1,071.53 | 1,094.84 | +2.18% |
| 2 | key-mixed | 869.70 | 881.57 | +1.36% |
| 2 | key-late-unicode | 65,986.00 | 67,845.00 | +2.82% |
| 3 | failure-0-shared-insert-1 | 252.80 | 271.00 | +7.20% |
| 3 | failure-24-shared-insert-1 | 282.80 | 299.40 | +5.87% |
| 3 | paths-1200-sorted | 260,090.00 | 261,910.00 | +0.70% |
| 3 | paths-200-unicode | 43,282.00 | 42,862.00 | -0.97% |
| 3 | key-greek | 1,541.31 | 1,575.50 | +2.22% |
| 3 | key-japanese | 1,109.74 | 1,074.77 | -3.15% |
| 3 | key-mixed | 895.85 | 846.57 | -5.50% |
| 3 | key-late-unicode | 66,360.00 | 65,063.00 | -1.95% |

The empty shared banner insertion is consistently slower: +1.40 to +18.20 ns in the repeats (+0.52% to +7.20%). `Arc::make_mut` must check ownership even when a held snapshot still requires the original allocation work. Greek-only keys are also consistently slower, by +15.12 to +34.19 ns (+1.17% to +2.22%); the ASCII dispatch adds work before the unchanged Unicode fallback. These are measured tradeoffs, not claimed improvements. The 24-entry shared insertion, sorted-path control, and other Unicode keys vary in both directions across runs. The 200-path Unicode sort is faster in all three repeats after a slower primary run. There is no claim of a speedup on every input or ownership pattern.

The changes are retained for the demonstrated removal of per-update table copies with unique ownership, amortized copies across held-snapshot batches, smaller temporary sorting storage, and faster complete matching workloads with no normalization changes. Behavioral comparisons remain exact. The primary table is the first complete measurement, not a selected repeat.

The representative gains remain in all three repeats: the 512-entry unique 32-insertion batch is 4.24-4.84x faster, the 200-song ASCII title match is 1.75-1.81x faster, the 1,200-path shuffled sort is 1.01-1.06x faster, and the 1,200-short-name sort is 1.03-1.07x faster. All 35 allocation measurements are identical across the primary run and three repeats.

## Allocation traffic and retained storage

Thread-local counters wrap the System allocator and are enabled only for separate allocation checks. The wrapper remains installed during timing, so timings are instrumented microbenchmarks. Counts exclude fixture preparation and later destruction of retained outputs, and include allocation requests made by realloc. This is allocation traffic, not peak RSS.

Cells are **allocations / reallocations / frees / requested bytes / freed bytes** per operation. A banner operation applies the update count in its label. Sorting measures only the in-place sort, excluding input construction/destruction.

| Case | Original | Current |
|---|---:|---:|
| failure-0-unique-insert-32 | 68 / 0 / 67 / 10,904 / 10,312 | 5 / 0 / 4 / 1,196 / 604 |
| failure-0-shared-insert-1 | 2 / 0 / 0 / 116 / 0 | 2 / 0 / 0 / 116 / 0 |
| failure-24-shared-insert-1 | 2 / 0 / 0 / 368 / 0 | 2 / 0 / 0 / 368 / 0 |
| failure-512-unique-insert-1 | 2 / 0 / 2 / 9,296 / 9,296 | 0 / 0 / 0 / 0 / 0 |
| failure-512-shared-insert-1 | 2 / 0 / 0 / 9,296 / 0 | 2 / 0 / 0 / 9,296 / 0 |
| failure-512-weak-insert-1 | 2 / 0 / 1 / 9,296 / 9,232 | 1 / 0 / 0 / 64 / 0 |
| failure-512-unique-insert-32 | 64 / 0 / 64 / 297,472 / 297,472 | 0 / 0 / 0 / 0 / 0 |
| failure-512-shared-insert-32 | 64 / 0 / 62 / 297,472 / 288,176 | 2 / 0 / 0 / 9,296 / 0 |
| failure-512-shared-duplicate-1 | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| failure-512-unique-remove-32 | 64 / 0 / 64 / 297,472 / 297,472 | 0 / 0 / 0 / 0 / 0 |
| failure-512-shared-remove-1 | 2 / 0 / 0 / 9,296 / 0 | 2 / 0 / 0 / 9,296 / 0 |
| failure-4096-shared-remove-32 | 64 / 0 / 62 / 2,361,856 / 2,288,048 | 2 / 0 / 0 / 73,808 / 0 |
| paths-0-sorted | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| paths-1-sorted | 2 / 0 / 2 / 42 / 42 | 0 / 0 / 0 / 0 / 0 |
| paths-24-reverse | 2 / 0 / 2 / 1,008 / 1,008 | 2 / 0 / 2 / 1,008 / 1,008 |
| paths-200-reverse | 2 / 0 / 2 / 8,400 / 8,400 | 2 / 0 / 2 / 8,400 / 8,400 |
| paths-1200-shuffled | 2 / 0 / 2 / 50,400 / 50,400 | 2 / 0 / 2 / 50,400 / 50,400 |
| paths-1200-sorted | 2 / 0 / 2 / 50,400 / 50,400 | 2 / 0 / 2 / 50,400 / 50,400 |
| paths-1200-short | 2 / 0 / 2 / 28,800 / 28,800 | 2 / 0 / 2 / 22,800 / 22,800 |
| paths-200-unicode | 2 / 0 / 2 / 8,000 / 8,000 | 2 / 0 / 2 / 8,000 / 8,000 |
| paths-4096-reverse | 2 / 0 / 2 / 172,032 / 172,032 | 2 / 0 / 2 / 172,032 / 172,032 |
| key-empty | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| key-lower | 1 / 1 / 0 / 24 / 8 | 1 / 0 / 0 / 15 / 0 |
| key-ascii | 1 / 2 / 0 / 56 / 24 | 1 / 0 / 0 / 25 / 0 |
| key-punctuation | 1 / 0 / 0 / 16 / 0 | 1 / 0 / 0 / 16 / 0 |
| key-greek | 1 / 2 / 0 / 56 / 24 | 1 / 2 / 0 / 56 / 24 |
| key-japanese | 1 / 2 / 0 / 56 / 24 | 1 / 2 / 0 / 56 / 24 |
| key-mixed | 1 / 2 / 0 / 56 / 24 | 1 / 2 / 0 / 56 / 24 |
| key-late-unicode | 1 / 9 / 0 / 8,184 / 4,088 | 1 / 9 / 0 / 8,184 / 4,088 |
| key-match-0-ascii-Song | 1 / 0 / 1 / 8 / 8 | 1 / 0 / 1 / 4 / 4 |
| key-match-24-ascii-Folder-12 | 27 / 0 / 27 / 808 / 808 | 27 / 0 / 27 / 798 / 798 |
| key-match-200-ascii-Catalog-Title-112 | 1,602 / 500 / 1,602 / 30,792 / 30,792 | 1,602 / 0 / 1,602 / 25,064 / 25,064 |
| key-match-1200-ascii-Catalog-Title-112 | 9,602 / 3,500 / 9,602 / 195,592 / 195,592 | 9,602 / 0 / 9,602 / 155,464 / 155,464 |
| key-match-200-ascii-Missing-Song | 4,403 / 1,301 / 4,403 / 76,312 / 76,312 | 4,403 / 0 / 4,403 / 62,128 / 62,128 |
| key-match-200-unicode-東京-Title-112 | 1,602 / 600 / 1,602 / 32,192 / 32,192 | 1,602 / 401 / 1,602 / 29,893 / 29,893 |

| Returned key capacity (bytes) | Original | Current |
|---|---:|---:|
| key-empty | 0 | 0 |
| key-lower | 16 | 15 |
| key-ascii | 32 | 25 |
| key-punctuation | 16 | 16 |
| key-greek | 32 | 32 |
| key-japanese | 32 | 32 |
| key-mixed | 32 | 32 |
| key-late-unicode | 4,096 | 4,096 |

## Regression checks

- Online release tests: **365 passed, 0 failed, 4 ignored**. All 355 baseline outcomes are preserved. Eleven new regression tests pass; three new ignored benchmark tests pass when run explicitly.
- Banner checks compare unique, shared, and weak snapshot ownership; no-op Arc identity; HashSet contents/iteration order; unrelated runtime fields; retry count saturation, deadline ranges and publication rules; and allocation traffic for insertion/removal batches.
- Sort checks cover all 5,914 permutations through seven paths, stable filename ties across different parents, Unicode, invalid OS strings, unusual/empty paths, and retained PathBuf pointers. An independent stable comparator also verifies the result. The existing oversized-key fallback remains unchanged.
- Key checks compare all 16,384 two-byte ASCII inputs and sampled valid Unicode scalars through U+10FFFF. They verify exact ASCII output capacity, no reallocations, punctuation fallback, lowercase/filter order, and complete matching results, callback traces and requested metadata order.
- Edited Rust files pass rustfmt checks. Frozen-original, version, pending-branch scope, file allowlist and Git whitespace audits pass.

## Song Lua / ITGmania compatibility

Fresh baseline and current builds run the same native semantic/actor suites and six selected full-song archives from this pass's starting main. Song Lua production code is unchanged.

| Suite | Passed | Failed | Ignored | Baseline/current comparison |
|---|---:|---:|---:|---|
| semantic | 143 | 37 | 77 | All 257 outcomes and 37 failure diagnostics unchanged |
| actor | 30 | 1 | 0 | All 31 outcomes and 1 failure diagnostics unchanged |

The native suites are **not fully green**: their 38 pre-existing failures remain. Diagnostic comparisons normalize panic thread IDs and ordering of the known unordered `Sprite.Load` lines; all other content and ordering must match.

| Archive | Passing comparisons before and after |
|---|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`) | 363,873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`) | 363,873 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`) | 304,425 |
| Warp Zone (`b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst`) | 212,220 |
| Let Me Hear That (`0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst`) | 205,071 |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`) | 201,471 |

All six selected archives pass **1,650,933 comparisons** before and after. Coverage is limited to these selected archives; unavailable local-only fixtures are not claimed as tested.

## Reproduction

From this worktree in PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = '4'
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
# Run separately for each archive filename above (no libtest flags):
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- '0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst'
```

Raw baseline/current logs, selectors and comparisons remain under this worktree's ignored `target/` directory. Builds reused `C:/GitHub/deadsync-perf-1859-runtime-reuse-20261008/target/build`; temporary files stayed within this worktree. The initial native baseline used two build jobs, and subsequent builds used four; test execution remained serial. No excluded input or automation file is included in the commit.
