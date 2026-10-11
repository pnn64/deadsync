# Chart metadata performance pass

Branch: `perf/1872-core-dataflows-20261009`. Starting committed main: `bac581834e31dbddc74a9b02a3de545136e9b67d`.
Patch version: **0.5.1871 -> 0.5.1872**, with Cargo.toml and Cargo.lock updated.

## Changes

1. Full-title construction concatenates the title, separator and subtitle into exactly sized storage. This removes general formatting and its intermediate buffer growth while preserving transliteration fallback and whitespace.
2. Standard difficulty lookup dispatches on the fixed ASCII name shapes instead of scanning the name list. It folds letter case directly and rejects unrelated leading bytes before examining the rest of longer names.
3. Edit-index collection waits for the first matching Edit chart before reserving output storage. Songs without matching edits return an empty vector without allocating, while nonempty results retain the same capacity, filtering, sort keys and stable tie order.

Production changes are confined to `crates/deadsync-chart/src/song.rs`. These functions serve chart parsing, chart selection, the music wheel and UI titles. Edit-index collection is called when Select Music rebuilds its edit cache and the music wheel constructs meter entries. No Song Lua source changed. The initial and final audits covered 62 unmerged perf refs / 36 distinct heads, with no pending diff in the production file. The pass follows `rust-performance.md` by removing formatting work and redundant comparisons before specializing the remaining fixed lookups.

## Behavioral validation

- **1152 unit tests passed** across gameplay, chart, rules, core and simfile. The one ignored benchmark entry was run explicitly in 4 independent processes.
- Title tests cover 1,250 combinations of empty strings, whitespace, Unicode and transliterated fields, plus allocation checks at several string lengths.
- Difficulty tests cover every ASCII case permutation of accepted names, every single-position ASCII-byte substitution, unknown values, Unicode and embedded NULs. They compare with the frozen starting implementation and independently assert the canonical indices.
- Edit-index tests cover 240 combinations of chart counts, edit placement and chart types, plus an explicit stable-tie assertion. They compare exact indices and allocation counts with the starting implementation, including Unicode descriptions and wrong chart types.
- Fresh baseline/current Song Lua harness runs replayed six recorded native ITGmania captures. All **1,650,933 comparisons passed** on both runs.
- Focused semantic parity: **143 passed / 38 failed / 77 ignored**. Actor conformance: **30 passed / 1 failed**. All existing failure names and diagnostics match. The comparison normalizes only panic thread IDs and known unordered missing-actor diagnostic lines.

This is replay of existing native captures, supplemented by differential metadata tests. No new live ITGmania capture or exhaustive native coverage of every metadata spelling is claimed.

| Native capture | Comparisons | Baseline / current |
| --- | ---: | --- |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f98744`) | 363873 | pass / pass |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d`) | 363873 | pass / pass |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12b`) | 304425 | pass / pass |
| Warp Zone (`b38698ececd6`) | 212220 | pass / pass |
| Let Me Hear That (`0229b74d092e`) | 205071 | pass / pass |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce68`) | 201471 | pass / pass |

## Paired benchmarks

Release `opt-level=3`, LTO disabled for both implementations. Rustc 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc; Xeon E5-2696 v4, 22 cores / 44 logical CPUs, Ultimate Performance power plan. Each benchmark child is pinned to logical CPU 2 with AboveNormal priority. No other process priority or affinity is changed.

The same binary contains the frozen starting bodies and current implementations. The old title and edit-index methods have explicit `SongData` receiver parameters. Black-boxed function pointers prevent either implementation from being inlined into the timing loop. Fixtures are constructed outside timing; title allocation and destruction are timed. Difficulty timings represent **batches of 256 queries**; title and edit-index timings represent **one complete construction and destruction** of the output.

Each process alternates variant order over 10 paired rounds, discards the first round, and reports each variant's median from the remaining nine. Warmup calibrates toward five milliseconds per batch, capped at five million iterations. The table reports the median of 4 independent process medians and retains the range of paired process ratios. Ratios above 1 indicate higher throughput. These are function microbenchmarks; whole-game frame rates were not measured.

| Case | Original ns/op | Current ns/op | Throughput | 4-run range |
| --- | ---: | ---: | ---: | --- |
| title-empty-translitfalse | 10.35 | 7.29 | 1.419x | 1.356-1.465 |
| title-empty-translittrue | 15.64 | 13.30 | 1.175x | 1.116-1.212 |
| title-title-only-translitfalse | 72.52 | 67.41 | 1.076x | 0.995-1.110 |
| title-title-only-translittrue | 81.16 | 75.66 | 1.073x | 0.908-1.102 |
| title-ascii-translitfalse | 314.46 | 85.44 | 3.681x | 3.486-3.938 |
| title-ascii-translittrue | 246.74 | 97.97 | 2.518x | 2.221-2.749 |
| title-unicode-translitfalse | 341.35 | 89.71 | 3.805x | 3.448-3.931 |
| title-unicode-translittrue | 257.48 | 102.21 | 2.519x | 2.314-2.611 |
| title-blank-subtitle-translitfalse | 73.74 | 76.00 | 0.970x | 0.944-1.008 |
| title-blank-subtitle-translittrue | 88.17 | 94.28 | 0.935x | 0.904-1.042 |
| title-long-translitfalse | 484.58 | 186.00 | 2.605x | 2.435-2.788 |
| title-long-translittrue | 480.74 | 202.00 | 2.380x | 2.284-2.485 |
| title-empty-title-translitfalse | 225.54 | 84.27 | 2.677x | 2.615-2.865 |
| title-empty-title-translittrue | 132.88 | 93.58 | 1.420x | 1.415-1.464 |
| title-translit-fallback-translitfalse | 221.13 | 88.53 | 2.498x | 2.395-2.633 |
| title-translit-fallback-translittrue | 242.66 | 99.97 | 2.427x | 2.320-2.595 |
| difficulty-beginner | 2515.66 | 1543.78 | 1.630x | 1.550-1.713 |
| difficulty-easy | 1278.48 | 1045.88 | 1.222x | 1.160-1.467 |
| difficulty-medium | 1675.74 | 1430.57 | 1.171x | 1.137-1.212 |
| difficulty-hard | 1558.58 | 1079.41 | 1.444x | 1.360-1.543 |
| difficulty-challenge | 2665.52 | 1856.57 | 1.436x | 1.385-1.464 |
| difficulty-empty | 765.10 | 845.71 | 0.905x | 0.876-0.932 |
| difficulty-unknown | 1091.00 | 991.88 | 1.100x | 1.060-1.180 |
| difficulty-unicode | 1108.57 | 986.69 | 1.124x | 1.018-1.176 |
| difficulty-uppercase | 2830.64 | 2267.47 | 1.248x | 1.236-1.272 |
| difficulty-mixed | 1591.09 | 1239.57 | 1.284x | 1.194-1.369 |
| edits-empty | 12.66 | 8.34 | 1.517x | 1.463-1.568 |
| edits-standard-5 | 180.57 | 116.92 | 1.544x | 1.498-1.594 |
| edits-standard-10 | 308.02 | 235.67 | 1.307x | 1.271-1.309 |
| edits-wrong-type-10 | 117.97 | 36.30 | 3.249x | 2.740-3.257 |
| edits-one-last-6 | 215.03 | 218.49 | 0.984x | 0.877-1.018 |
| edits-one-first-6 | 206.71 | 206.09 | 1.003x | 0.972-1.041 |
| edits-sparse-64 | 1650.37 | 1701.03 | 0.970x | 0.958-1.005 |
| edits-mixed-16 | 615.69 | 624.65 | 0.986x | 0.944-1.025 |
| edits-all-5 | 237.65 | 227.18 | 1.046x | 0.998-1.098 |
| edits-all-32 | 3881.95 | 3951.55 | 0.982x | 0.901-1.076 |
| edits-all-128 | 35168.00 | 35487.50 | 0.991x | 0.944-1.039 |
| edits-unicode-ties-32 | 12641.62 | 12825.30 | 0.986x | 0.976-1.030 |

Empty difficulty lookup was slower in all four final runs: 10.5% more time, or approximately 0.31 ns per query. Blank-subtitle and several nonempty Edit controls also had lower pooled medians; their paired run ranges crossed 1.0. The complete results are retained above and do not establish a universal speedup on every input. Allocation counts did not increase in the measured cases. No behavioral regressions were detected.

## Allocation measurements

The test-only System allocator wrapper records thread-local allocation requests outside timed batches. Its disabled TLS check remains in both timed variants. Fixture allocations are excluded. Requested bytes sum allocation/reallocation requests; peak bytes are live requested bytes within the operation and exclude allocator bookkeeping. All four runs produced identical counts. No production allocator or dependency changed.

| Case | Old alloc / realloc | New alloc / realloc | Old requested bytes | New requested bytes | Old peak bytes | New peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| title-empty-translitfalse | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| title-empty-translittrue | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| title-title-only-translitfalse | 1 / 0 | 1 / 0 | 7 | 7 | 7 | 7 |
| title-title-only-translittrue | 1 / 0 | 1 / 0 | 9 | 9 | 9 | 9 |
| title-ascii-translitfalse | 1 / 2 | 1 / 0 | 98 | 29 | 56 | 29 |
| title-ascii-translittrue | 1 / 1 | 1 / 0 | 28 | 20 | 20 | 20 |
| title-unicode-translitfalse | 1 / 2 | 1 / 0 | 63 | 19 | 36 | 19 |
| title-unicode-translittrue | 1 / 1 | 1 / 0 | 28 | 20 | 20 | 20 |
| title-blank-subtitle-translitfalse | 1 / 0 | 1 / 0 | 4 | 4 | 4 | 4 |
| title-blank-subtitle-translittrue | 1 / 0 | 1 / 0 | 6 | 6 | 6 | 6 |
| title-long-translitfalse | 1 / 1 | 1 / 0 | 12288 | 4225 | 8192 | 4225 |
| title-long-translittrue | 1 / 1 | 1 / 0 | 12288 | 4225 | 8192 | 4225 |
| title-empty-title-translitfalse | 1 / 1 | 1 / 0 | 24 | 9 | 16 | 9 |
| title-empty-title-translittrue | 1 / 0 | 1 / 0 | 8 | 4 | 8 | 4 |
| title-translit-fallback-translitfalse | 1 / 1 | 1 / 0 | 24 | 14 | 16 | 14 |
| title-translit-fallback-translittrue | 1 / 1 | 1 / 0 | 33 | 25 | 25 | 25 |
| edits-empty | 0 / 0 | 0 / 0 | 0 | 0 | 0 | 0 |
| edits-standard-5 | 1 / 0 | 0 / 0 | 40 | 0 | 40 | 0 |
| edits-standard-10 | 1 / 0 | 0 / 0 | 80 | 0 | 80 | 0 |
| edits-wrong-type-10 | 1 / 0 | 0 / 0 | 80 | 0 | 80 | 0 |
| edits-one-last-6 | 1 / 0 | 1 / 0 | 48 | 48 | 48 | 48 |
| edits-one-first-6 | 1 / 0 | 1 / 0 | 48 | 48 | 48 | 48 |
| edits-sparse-64 | 1 / 0 | 1 / 0 | 512 | 512 | 512 | 512 |
| edits-mixed-16 | 1 / 0 | 1 / 0 | 128 | 128 | 128 | 128 |
| edits-all-5 | 1 / 0 | 1 / 0 | 40 | 40 | 40 | 40 |
| edits-all-32 | 1 / 0 | 1 / 0 | 256 | 256 | 256 | 256 |
| edits-all-128 | 1 / 0 | 1 / 0 | 1024 | 1024 | 1024 | 1024 |
| edits-unicode-ties-32 | 1 / 0 | 1 / 0 | 256 | 256 | 256 | 256 |

## Reproduction and local evidence

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-gameplay -p deadsync-chart -p deadsync-rules -p deadsync-core --lib
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-chart --lib benchmark_metadata -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run
```

The two focused compatibility suites have pre-existing failures, so compare baseline/current logs. Run the custom full-song executable using the six selectors in `target/current-fullsong-selectors.json`.

Ignored evidence in this worktree includes `target/baseline-*.log`, `target/current-*.log`, `target/final-1.log` through `target/final-4.log`, `target/benchmark-results.json`, `target/compatibility-comparison.json`, `target/source-hashes.json` and the pending-branch audits. The local `run-local.py`, `run-compat.py`, `run-bench.py`, `analyze-bench.py`, `compare-results.py` and `verify-change.py` scripts live in `target`. Builds reuse the ignored Cargo cache under the earlier data-churn worktree; all source edits and the commit belong to this worktree.

The original checkout and concurrent user edits are untouched. The pass is committed on its perf branch without merging or pushing. None of `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh` or `optimize.ps1` is included in the commit.
