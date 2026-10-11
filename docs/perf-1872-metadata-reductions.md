# Performance pass: archive and catalog metadata

Branch: `perf/1872-metadata-reductions-20261009`. Starting committed main: `38890f14b70cc7aee1db6078a2ebd872e577195f`. Version: **0.5.1871 -> 0.5.1872**, in Cargo.toml and all three matching workspace entries in Cargo.lock.

Read rust-performance.md and reviewed 24 existing unmerged perf branches before selecting this work. The pending archive work in `perf/1870-data-hotpaths-20261009` changes `merge_ranges`; the pending SMO work in `perf/1872-catalog-parsing-20261009` changes extension handling and other functions outside `resolve_song`. Those functions remain unchanged here. No reviewed pending branch changes itgdb.rs. The new test module filenames do not collide with the reviewed pending branches. Work is isolated in a new worktree; the original checkout continued to receive independent user work and was left untouched by this pass. Main advanced during validation; this branch remains based on the starting commit above. No merge or push was performed.

## Changes

1. **Borrow cached simfile metadata during song matching.** The only production caller already holds the cache lock throughout matching. Returning references from its callback removes cloning five owned Strings on every cached metadata lookup. Matching rules and callback order/count are unchanged; the result contains only a folder index or match status, so no reference escapes. Folder-name matching and missing cache entries remain covered as controls.
2. **Group and classify archive entries in one pass.** Reuse the path iterator that identifies the root and song folder to find the filename and distinguish direct children from nested files. Remove the second traversal, repeated path splitting, and full component count. Keep folder/entry/audio order, case-insensitive grouping, AppleDouble filtering, and first-direct-SSC-over-SM selection. Allocations and retained vector capacities remain the same.
3. **Normalize ASCII catalog names directly.** Skip Unicode character decoding, classification, and lowercase expansion for ASCII input. The original Unicode loop remains intact for non-ASCII names, including per-character Greek sigma casing and the combining dot produced by capital dotted I. Output capacity and allocation churn remain the same. This function joins names across catalog collections in the content browser as well as checking dedicated doubles packs.

All production changes are outside Song Lua. Original implementations and benchmark support compile only for tests. Original functions are checked against the starting commit, and matching bodies are checked to differ only in the callback's borrowed return type. Existing matching tests retain their inputs and assertions, with metadata fixtures held in locals to supply references.

## Benchmark method

Windows x86-64/MSVC, Intel Xeon E5-2696 v4 at 2.20 GHz, `rustc 1.98.1 (48a229cea 2026-09-01)`, release optimization with LTO disabled. The online crate's test binary compares production functions with frozen starting implementations using the same inputs and production data types. This pass runs no compilation or compatibility job during timing; unrelated work on the shared workstation is left alone.

Each case uses ten alternating original/current sample pairs, discards the warm-up pair, and reports the median of nine samples per implementation. Inputs are prepared outside timing, inputs/results are black-boxed, and destruction of returned values is included. Fast batches calibrate to approximately 2 ms. These are function-level wall-time measurements, not whole-application FPS or CPU-cycle measurements.

Archive cases span empty input, flat packs, and nested effect files. Matching uses the same HashMap cache access on both sides; only the original clones metadata. Normalization includes ASCII, Unicode, punctuation-only and empty controls, a long ASCII prefix followed by Unicode, and 1,200/9,000-name ASCII catalog batches.

## Timing results

| Case | Original ns/op | Current ns/op | Throughput ratio |
|---|---:|---:|---:|
| normalize-empty | 14.02 | 15.45 | 0.91x |
| normalize-lower | 474.61 | 120.63 | 3.93x |
| normalize-ascii | 842.54 | 178.77 | 4.71x |
| normalize-punctuation | 121.48 | 93.43 | 1.30x |
| normalize-greek | 1,160.00 | 1,114.63 | 1.04x |
| normalize-japanese | 743.26 | 721.89 | 1.03x |
| normalize-mixed | 732.36 | 726.68 | 1.01x |
| normalize-late-unicode | 68,271.00 | 69,637.00 | 0.98x |
| normalize-catalog-1200 | 788,430.00 | 166,885.00 | 4.72x |
| normalize-catalog-9000 | 6,188,990.00 | 1,324,050.00 | 4.67x |
| group-0x0-flat | 21.23 | 20.58 | 1.03x |
| group-1x4-flat | 3,285.00 | 3,008.48 | 1.09x |
| group-24x4-flat | 67,630.00 | 62,747.50 | 1.08x |
| group-200x12-flat | 1,796,755.00 | 1,593,750.00 | 1.13x |
| group-1200x8-flat | 6,685,575.00 | 6,165,150.00 | 1.08x |
| group-200x32-deep | 4,387,380.00 | 3,941,495.00 | 1.11x |
| match-0-Song-no-artist | 227.95 | 227.30 | 1.00x |
| match-24-Folder-12-no-artist | 5,878.95 | 5,958.75 | 0.99x |
| match-24-Catalog-Title-12-no-artist | 33,218.33 | 23,960.00 | 1.39x |
| match-200-Catalog-Title-112-no-artist | 296,730.00 | 219,675.00 | 1.35x |
| match-1200-Catalog-Title-112-no-artist | 1,846,935.00 | 1,430,725.00 | 1.29x |
| match-200-Roman-Title-112-no-artist | 311,295.00 | 233,820.00 | 1.33x |
| match-200-Missing-Song-no-artist | 553,040.00 | 402,630.00 | 1.37x |
| match-200-Catalog-Title-112-artist | 289,145.00 | 216,080.00 | 1.34x |
| match-200-Missing-Song-artist | 793,220.00 | 588,705.00 | 1.35x |

All measured cases are shown, including controls. Cases with a higher current median: `normalize-empty` +1.43 ns (10.20%); `normalize-late-unicode` +1366.00 ns (2.00%); `match-24-Folder-12-no-artist` +79.80 ns (1.36%).

Three fixed additional runs of the same final binary checked those initially slower controls, without changing code or choosing a preferred result:

| Repeat | Control | Original ns/op | Current ns/op | Current time change |
|---:|---|---:|---:|---:|
| 1 | normalize-empty | 16.95 | 14.76 | -12.92% |
| 1 | normalize-late-unicode | 67,821.00 | 67,958.00 | +0.20% |
| 1 | match-24-Folder-12-no-artist | 5,478.16 | 5,175.00 | -5.53% |
| 2 | normalize-empty | 16.16 | 14.59 | -9.72% |
| 2 | normalize-late-unicode | 68,923.00 | 68,580.00 | -0.50% |
| 2 | match-24-Folder-12-no-artist | 5,570.28 | 5,866.47 | +5.32% |
| 3 | normalize-empty | 14.15 | 17.62 | +24.52% |
| 3 | normalize-late-unicode | 62,099.00 | 63,245.00 | +1.85% |
| 3 | match-24-Folder-12-no-artist | 5,799.17 | 5,852.11 | +0.91% |

The controls vary in both directions across the fixed repeats. Empty normalization differs by a few nanoseconds; the long-Unicode control still performs an added ASCII check, so these results do not establish identical throughput for every Unicode input. The intended normalization gain is on ASCII catalog names. Grouping improves each nonempty case in the primary run; metadata matching saves exactly five allocations per lookup when all five cached strings are nonempty, independent of timing variance.

## Allocation churn

Thread-local counters wrap the System allocator. Counting is enabled only for separate allocation checks; the same wrapper remains installed during timing, so timings are instrumented microbenchmarks. Counts cover each call until its result is returned, excluding fixture construction and the later destruction of retained results. Allocated bytes include realloc requests; this is allocation traffic, not peak RSS. Grouping and normalization also assert unchanged retained capacities.

Cells below are **allocations / reallocations / frees / requested bytes / freed bytes** per call. Matching fixtures populate all five metadata strings; empty metadata strings would naturally save fewer allocations.

| Case | Original | Current |
|---|---:|---:|
| normalize-empty | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| normalize-lower | 1 / 0 / 0 / 22 / 0 | 1 / 0 / 0 / 22 / 0 |
| normalize-ascii | 1 / 0 / 0 / 46 / 0 | 1 / 0 / 0 / 46 / 0 |
| normalize-punctuation | 1 / 0 / 0 / 18 / 0 | 1 / 0 / 0 / 18 / 0 |
| normalize-greek | 1 / 0 / 0 / 30 / 0 | 1 / 0 / 0 / 30 / 0 |
| normalize-japanese | 1 / 0 / 0 / 32 / 0 | 1 / 0 / 0 / 32 / 0 |
| normalize-mixed | 1 / 0 / 0 / 39 / 0 | 1 / 0 / 0 / 39 / 0 |
| normalize-late-unicode | 1 / 0 / 0 / 4,402 / 0 | 1 / 0 / 0 / 4,402 / 0 |
| group-0x0-flat | 0 / 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 / 0 |
| group-1x4-flat | 17 / 4 / 13 / 786 / 364 | 17 / 4 / 13 / 786 / 364 |
| group-24x4-flat | 365 / 99 / 292 / 14,258 / 9,748 | 365 / 99 / 292 / 14,258 / 9,748 |
| group-200x12-flat | 7,808 / 2,806 / 7,207 / 247,350 / 191,332 | 7,808 / 2,806 / 7,207 / 247,350 / 191,332 |
| group-1200x8-flat | 32,411 / 10,809 / 28,810 / 1,196,902 / 891,788 | 32,411 / 10,809 / 28,810 / 1,196,902 / 891,788 |
| group-200x32-deep | 19,808 / 7,006 / 19,207 / 520,350 / 438,732 | 19,808 / 7,006 / 19,207 / 520,350 / 438,732 |
| match-0-Song-no-artist | 1 / 0 / 1 / 8 / 8 | 1 / 0 / 1 / 8 / 8 |
| match-24-Folder-12-no-artist | 27 / 0 / 27 / 808 / 808 | 27 / 0 / 27 / 808 / 808 |
| match-24-Catalog-Title-12-no-artist | 194 / 48 / 194 / 3,448 / 3,448 | 74 / 48 / 74 / 1,952 / 1,952 |
| match-200-Catalog-Title-112-no-artist | 1,602 / 500 / 1,602 / 30,792 / 30,792 | 602 / 500 / 602 / 17,632 / 17,632 |
| match-1200-Catalog-Title-112-no-artist | 9,602 / 3,500 / 9,602 / 195,592 / 195,592 | 3,602 / 3,500 / 3,602 / 113,632 / 113,632 |
| match-200-Roman-Title-112-no-artist | 1,603 / 501 / 1,603 / 30,816 / 30,816 | 603 / 501 / 603 / 17,656 / 17,656 |
| match-200-Missing-Song-no-artist | 3,002 / 901 / 3,002 / 53,544 / 53,544 | 1,002 / 901 / 1,002 / 27,224 / 27,224 |
| match-200-Catalog-Title-112-artist | 1,609 / 500 / 1,609 / 30,876 / 30,876 | 604 / 500 / 604 / 17,648 / 17,648 |
| match-200-Missing-Song-artist | 4,403 / 1,301 / 4,403 / 76,312 / 76,312 | 1,403 / 1,301 / 1,403 / 36,832 / 36,832 |

## Regression checks

- Online release tests: **363 passed, 0 failed, 4 ignored**. All 355 baseline outcomes are preserved. Nine new regression tests pass; three new ignored benchmark tests pass when run explicitly.
- Matching checks compare results and exact callback traces for empty/ambiguous matches, folder/title/transliteration rules, missing cache entries, artist selection, and unchanged cached data. Allocation checks prove removal of five string allocations per populated cached-metadata lookup.
- Grouping checks compare every folder name, entry index, chosen simfile, audio index and retained capacity against the original, including 40 generated irregular-path trials (10,140 entries in total), Unicode/case variants, repeated/mixed separators, directories, AppleDouble files, and nested charts/audio.
- Normalization compares all 16,384 two-byte ASCII inputs, sampled valid Unicode scalars through U+10FFFF, casing-expansion examples, dedicated-pack membership, output capacities and allocation churn.
- Edited Rust files pass rustfmt checks. Git diff checks, frozen-original audits, pending-branch scope checks, and the exact version-change audit pass.

## Song Lua / ITGmania compatibility

Fresh baseline and current binaries use this pass's starting main and worktree, including the committed native model-draw observations. Both run the same semantic/actor suites and six available full-song archives. Song Lua production code is unchanged.

| Suite | Passed | Failed | Ignored | Baseline/current comparison |
|---|---:|---:|---:|---|
| semantic | 143 | 37 | 76 | All 256 outcomes and 37 failure diagnostics unchanged |
| actor | 30 | 1 | 0 | All 31 outcomes and 1 failure diagnostics unchanged |

The native suites are **not fully green**: their 38 pre-existing failures remain. Failure comparisons normalize panic thread IDs and the ordering of existing unordered `Sprite.Load` diagnostic lines; all other diagnostic content and ordering must match.

| Archive | Passing comparisons before and after |
|---|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst`) | 363,873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst`) | 363,873 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst`) | 304,425 |
| Warp Zone (`b38698ececd60bdf311a15de205f43d0f3466eac6d41296b9abf469485ebab4d.tar.zst`) | 212,220 |
| Let Me Hear That (`0229b74d092e8f04278d64b7c994877a0a3bcc52e9d7ddb00d6a98f1f5244cd0.tar.zst`) | 205,071 |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce688e70b4364258337b30c9e4bf7d337f2c83a30e59215eb838735d.tar.zst`) | 201,471 |

All six selected archives pass **1,650,933 comparisons** before and after. The runner reports two other local-only archives unavailable; this is not a claim of coverage for the entire archive corpus.

## Reproduction

From this worktree in PowerShell:

```powershell
$env:CARGO_BUILD_JOBS = '2'
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
# Run separately for each exact archive filename listed above (no libtest flags):
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- '0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst'
```

Raw build/test/benchmark logs, baseline/current archive selectors, and the outcome comparison are retained under this worktree's ignored `target/` directory. Builds reused the existing shared release cache at `C:/GitHub/deadsync-perf-1859-runtime-reuse-20261008/target/build`; temporary files stayed within this worktree. No excluded input or automation file is included in the commit.
