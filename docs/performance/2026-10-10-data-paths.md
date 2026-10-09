# Query editing, pack selection, and help truncation - 2026-10-10

Starting main: `b4d4a8b49c21b96b3be66cf05cdfda37b74f9b2f` (0.5.1871). Branch: `perf/1872-data-paths-20261010`. This pass increments the workspace patch version to **0.5.1872**, updating Cargo.toml and all three workspace-version entries in Cargo.lock.

## Changes

1. **Delete the last search word in the existing UTF-8 buffer.** Replace the String -> Vec<char> -> String round trip with two borrowed suffix trims and String::truncate. It scans only the deleted suffix, preserves Unicode whitespace behavior, and retains capacity for subsequent input. Ordinary typed queries are capped at 80 Unicode scalars. Retaining that existing allocation until the query is replaced/dropped is intentional; the long-prefix benchmark is a stress case beyond normal typed input.
2. **Prune pack selections without copying pack keys.** Share the same small retention operation between a pack-list refresh and a locale refresh. Skip its membership set entirely when selection is empty (the all-packs convention); otherwise collect borrowed keys. Pack sorting, deduplication, case handling, selection semantics, picker summary/cursor updates, and cache invalidation retain their original behavior.
3. **Truncate help text directly into its final-size string.** Remove the temporary vector of line slices and subsequent string growth. Count the kept lines, allocate exactly the joined text plus newline/ellipsis, and write those lines directly. Ellipsis-only output constructs its Arc directly. CRLF, blank lines, bullet markers, zero/one-line limits, and declared line counts are preserved.

Production changes are confined to those functions and the shared pack-retention helper. Song Lua implementation and compatibility fixtures are unchanged.

## Pending-branch audit and isolation

Audited 68 unmerged local/remote perf references (42 distinct heads) against the starting commit. No pending branch changes song_search.rs, score_import.rs, or options/update.rs. The previous `perf/1872-summary-resources-20261009` branch changes `fit_help_blocks` in options/render.rs; its `truncate_help_block` body is byte-identical to the starting main. This pass changes only that latter function. Common test-only allocator support repeats the compatible measurement extension used by earlier passes.

All source changes and the commit belong to `C:/GitHub/deadsync-perf-1872-data-paths-20261010`. This pass did not edit, check out, reset, stash, or clean the original checkout. Main continued to advance independently during this run; this branch stays based on the recorded starting commit. An existing ignored Cargo build cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` was reused; all new source, fixture extraction, temporary files, and evidence are in this pass's worktree. The four prohibited filenames are absent from the commit. No merge or push was performed.

## Paired release benchmarks

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.98.1 (48a229cea, LLVM 22.1.8). Release opt-level 3, LTO disabled for both implementations. Four fresh theme-test processes, each pinned to logical CPU 2 (affinity mask 4), AboveNormal priority, after this pass's builds and compatibility tests completed. Each process alternates original/current batches, discards its calibration sample, and reports the median of nine timed samples. The table uses the median of the four process medians; ranges are the four paired original/current ratios. These are function/workflow timings, not whole-game FPS claims.

Frozen originals are copied from the starting commit with only function names changed, verified against git before commit. Benchmarks call those originals and the production replacements in the same binary. Query timing includes restoring input via clear/push_str in both variants; retained capacity is part of the edit/retype workload. Help timing includes the same input Arc clone and output drop. Pack timing covers complete refresh functions, including rebuilding/sorting options and updating UI state, with input setup outside timing. Both variants use identical fixtures and validate outputs.

Allocation counts come from separate scoped System-allocator measurements; byte counts sum requested allocation/reallocation sizes, not peak RSS or allocator usable size. Counting is disabled during timing, although the test allocator's TLS check remains. All four processes must agree on allocation data.

| Workload | Original -> current ns/op | Throughput | Paired range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| `delete/empty` | 9.79 -> 5.91 | 1.656x | 1.649-1.687x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `delete/one-word` | 351.15 -> 20.79 | 16.891x | 15.597-18.448x | 2 -> 0 | 2 -> 0 | 121 -> 0 |
| `delete/ascii` | 487.74 -> 18.48 | 26.400x | 24.343-27.089x | 2 -> 0 | 3 -> 0 | 244 -> 0 |
| `delete/unicode` | 490.35 -> 23.09 | 21.241x | 17.822-22.290x | 2 -> 0 | 3 -> 0 | 180 -> 0 |
| `delete/80-chars` | 631.12 -> 22.13 | 28.519x | 24.790-30.249x | 2 -> 0 | 3 -> 0 | 813 -> 0 |
| `delete/long-prefix` | 19588.92 -> 145.45 | 134.678x | 91.192-179.513x | 2 -> 0 | 3 -> 0 | 43032 -> 0 |
| `truncate/empty` | 179.14 -> 98.75 | 1.814x | 1.552-2.138x | 2 -> 1 | 0 -> 0 | 32 -> 24 |
| `truncate/ellipsis` | 195.23 -> 103.70 | 1.883x | 1.845-1.965x | 2 -> 1 | 0 -> 0 | 32 -> 24 |
| `truncate/bullet-ellipsis` | 200.86 -> 102.68 | 1.956x | 1.809-2.061x | 2 -> 1 | 0 -> 0 | 32 -> 24 |
| `truncate/three-lines` | 428.14 -> 316.78 | 1.352x | 1.181-1.438x | 3 -> 2 | 1 -> 0 | 213 -> 91 |
| `truncate/crlf` | 426.85 -> 305.02 | 1.399x | 1.371-1.427x | 3 -> 2 | 1 -> 0 | 175 -> 73 |
| `truncate/long-tail` | 1354.12 -> 1104.71 | 1.226x | 1.154-1.324x | 3 -> 2 | 4 -> 0 | 3105 -> 1087 |
| `packs/refresh/all/0` | 66.81 -> 56.30 | 1.187x | 1.059-1.621x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `packs/refresh/selected/0` | 71.14 -> 56.11 | 1.268x | 1.122-1.309x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `packs/locale/all/0` | 1379.90 -> 1334.39 | 1.034x | 0.976-1.098x | 12 -> 12 | 0 -> 0 | 267 -> 267 |
| `packs/locale/selected/0` | 1323.82 -> 1323.41 | 1.000x | 0.978-1.015x | 12 -> 12 | 0 -> 0 | 267 -> 267 |
| `packs/refresh/all/32` | 34086.33 -> 30175.75 | 1.130x | 1.096-1.191x | 292 -> 259 | 64 -> 64 | 14208 -> 11952 |
| `packs/refresh/selected/32` | 36367.24 -> 32938.71 | 1.104x | 1.062-1.121x | 292 -> 260 | 64 -> 64 | 14208 -> 13056 |
| `packs/locale/all/32` | 47142.04 -> 42232.96 | 1.116x | 1.065-1.150x | 402 -> 369 | 64 -> 64 | 20859 -> 18603 |
| `packs/locale/selected/32` | 45572.06 -> 43496.51 | 1.048x | 0.999-1.085x | 402 -> 370 | 64 -> 64 | 20859 -> 19707 |
| `packs/refresh/all/256` | 272577.78 -> 245085.00 | 1.112x | 1.072-1.149x | 2309 -> 2052 | 512 -> 512 | 125728 -> 107792 |
| `packs/refresh/selected/256` | 286766.66 -> 266494.76 | 1.076x | 1.048-1.106x | 2309 -> 2053 | 512 -> 512 | 125728 -> 116512 |
| `packs/locale/all/256` | 365917.86 -> 328360.42 | 1.114x | 1.101-1.131x | 3092 -> 2835 | 512 -> 512 | 189243 -> 171307 |
| `packs/locale/selected/256` | 377071.67 -> 343977.69 | 1.096x | 1.049-1.160x | 3092 -> 2836 | 512 -> 512 | 189243 -> 180027 |
| `packs/refresh/all/4096` | 5411000.00 -> 4622450.00 | 1.171x | 1.123-1.223x | 36869 -> 32772 | 8192 -> 8192 | 2011168 -> 1724432 |
| `packs/refresh/selected/4096` | 5663950.00 -> 5074225.00 | 1.116x | 1.062-1.169x | 36869 -> 32773 | 8192 -> 8192 | 2011168 -> 1863712 |
| `packs/locale/all/4096` | 7103750.00 -> 6360500.00 | 1.117x | 1.104-1.152x | 49172 -> 45075 | 8192 -> 8192 | 3023163 -> 2736427 |
| `packs/locale/selected/4096` | 7275150.00 -> 6585150.00 | 1.105x | 1.086-1.120x | 49172 -> 45076 | 8192 -> 8192 | 3023163 -> 2875707 |

Across all 28 workloads, median timing improved or was effectively unchanged. ASCII word deletion measured 487.74 -> 18.48 ns (26.40x) with 2 -> 0 allocations and 3 -> 0 reallocations per edit/retype cycle. The 4,096-pack default-selection refresh measured 5.411 -> 4.622 ms (1.171x), eliminating 4,097 allocations and 286,736 requested bytes. Three-line help measured 428.14 -> 316.78 ns (1.352x), with 3 -> 2 allocations, 1 -> 0 reallocations, and 213 -> 91 requested bytes.

The two zero-pack locale controls had paired timing ranges crossing 1.0 and unchanged allocations; their medians were 1.034x and 1.000x. One of four 32-pack selected-locale pairs was effectively tied (0.99894x), with a 1.048x median and 32 fewer allocations. All other nonempty workload pairs improved. No consistent throughput regression was observed; the full table retains the controls and stress-case spread.

## Regression and compatibility checks

Fresh baseline and current binaries were built and executed in this worktree using the same release/LTO settings. The baseline was completed and its executables archived before any production edits. Each existing test outcome and normalized failure diagnostic was compared, rather than only comparing failure counts. Normalization is limited to runtime thread IDs, ordering of known unordered missing-actor diagnostic lines, and the first missing-archive filename in the consolidated-reference check. The latter is nondeterministic: three reruns of each saved binary alternated between the same two missing non-local archives (`c0846148...` and `d6b0ff8c...`). The complete missing-file set and byte-identical reference index, reference map, and test source were verified. That test remains FAILED in both runs; no other diagnostic text is discarded.

| Suite | Current passed | Existing failed | Ignored | New passing tests |
|---|---:|---:|---:|---:|
| deadsync_theme_simply_love-tests | 1269 | 1 | 6 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 | 0 |
| semantic | 144 | 39 | 77 | 0 |
| actor | 30 | 1 | 0 | 0 |

The new tests compare Unicode deletion over combinations of whitespace, combining marks, CJK, emoji, controls, and zero-width characters; verify that deletion neither allocates nor frees; compare help truncation across CRLF/blank-line/bullet/limit combinations; assert help allocation reductions; exercise removed/duplicate/mixed-case pack groups and active picker controls; and measure per-key allocation removal in both full refresh paths.

The baseline is not fully green. Semantic/actor failures include unavailable external song corpora and reference archives, older clock captures, and pre-existing native mismatches. The theme's existing Cyber model-height guard also fails with logical and model heights both 60.162445. Every one of these failures remains unchanged; none was suppressed or marked ignored by this pass.

Six recorded ITGmania full-song captures pass on both implementations:

| Capture | Archive | Comparisons |
|---|---|---:|
| 319 / TECH SOUP / [lv.P.Clark] Epidermis | `0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst` | 363873 |
| 319 / TECH SOUP / [lv.P.Clark] Epidermis | `af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst` | 363873 |
| 280 / MODS / [MASTER] Sharkmode | `b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst` | 304425 |
| Warp Zone | `ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst` | 212220 |
| Let Me Hear That | `2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst` | 205071 |
| 272 / MODS / [lv.02] Riddle | `7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst` | 201471 |

These exercise the Song Lua compatibility harness against recorded native ITGmania references; no live ITGmania process was launched. Native trace comparisons are complemented by the focused Rust differential tests for the changed UI behavior.

## Reproduction and evidence

Build commands used (run once on the recorded baseline and once on the branch):

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-theme-simply-love --lib --no-run --message-format=json
```

The actual executable paths were taken from Cargo's JSON artifacts. Theme, simfile, semantic and actor binaries were run with `--test-threads=1`; the full-song binary was run once per archive selector above. Root library/bin targets were built to align dependency features, but their unit tests were not included in the reported suite counts.

Run the committed paired benchmarks:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-theme-simply-love --lib benchmark_data_paths -- --ignored --nocapture --test-threads=1
```

Local ignored evidence: `target/baseline-*.log`, `target/current-*.log`, `target/final-{1,2,3,4}.log`, `target/benchmark-summary.json`, `target/compatibility-comparison.json`, `target/pending-audit.json`, `target/missing-reference-audit.json`, `target/baseline-saved-binaries.json`, and `target/verified-source-hashes.json`. Formatting, diff whitespace, exact +1 version changes, frozen-original equality, and absence of unrelated production changes were checked before committing.

Production source SHA-256:

- `crates/deadsync-theme-simply-love/src/screens/components/select_music/select_music_menu/song_search.rs`: `7faa83b148139f91a707a4ac742f43dc37f8ea13716c862f26176b8d1221c26e`
- `crates/deadsync-theme-simply-love/src/screens/options/score_import.rs`: `4821711fb4726347ca91d48eb577d57d059af57df2c65841ffd759ab1ffb005e`
- `crates/deadsync-theme-simply-love/src/screens/options/update.rs`: `151646b019c283db0e8e202efd7e3d5962a5477972c433fcbdfd73a4cf12432c`
- `crates/deadsync-theme-simply-love/src/screens/options/render.rs`: `09a3076cbbf570ae8dad84d5ffe4b6cdb583ed7ea2096ee4895753dcf7453690`
