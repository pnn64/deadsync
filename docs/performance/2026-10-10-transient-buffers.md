# Transient UI buffers — 2026-10-10

Branch: `perf/1872-transient-buffers-20261010`. Base: `b73e9144cf8b2ac4a63a38addd3140c1aff4f8c8` (committed main at worktree creation). Version: **0.5.1871 → 0.5.1872**, updated in Cargo.toml and Cargo.lock. No merge or push.

Audited 75 unmerged local/remote perf refs (49 distinct commits). No pending changes overlap the production files used by this pass. No Song Lua source or fixture changes.

## Changes

1. Profile selection keeps the localized played-song count as `Arc<str>` through to the retained choice. Removes an `Arc → String → Arc` round trip and two copies/allocations per count.
2. Player-options palette updates convert borrowed choices directly into their existing inline/shared actor representation. Removes the cloned input vector and each cloned string. Owned-input callers still consume their iterator one item at a time; selection and layout invalidation are unchanged.
3. SRPG shop queues store immutable item IDs in one set per existing shop slot. Removes the composite-key formatting helper and every temporary `shop:item` string used during queue lookup. Catalog counts, item status and download deduplication keep their shop isolation. `Box<str>` drops the unused capacity word from each stored key.

Tradeoff: the fixed overlay struct grows from 248 to 392 bytes because it holds four set handles. Empty sets allocate no tables. Stored keys lose their shop prefix and use smaller handles. Each table grows independently, so retained table capacity depends on how queued items are distributed among shops. The insertion benchmark measures allocator savings for a populated single-shop queue; the scan benchmark measures the removal of transient allocations.

## Paired release benchmarks

Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.98.1. Release with LTO disabled and the same combined feature graph for both variants. Ten frozen original function bodies were verified against the base commit. Four fresh processes pinned to logical CPU 2 at AboveNormal priority; three warm calls, calibration to roughly 25 ms, then nine alternating timed batches per variant. Values are medians across process medians; ranges are the four paired ratios. No builds or compatibility tests from this pass ran during formal timings.

Outputs pass through black_box and timings include amortized destruction. Actor vectors are preallocated and reused; queue-insertion batches retain hash-table capacity. A scoped System allocator measures allocation/reallocation calls and requested bytes separately from timing. Both timed variants use the same allocator with counting disabled. These are operation-level results.

| Workload | Original ns | Current ns | Throughput | Paired range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| shop/1-false-false | 210.59 | 4.03 | 52.254× | 46.954–54.163× | 1 → 0 | 1 → 0 | 24 → 0 |
| shop/64-false-false | 13,251.85 | 128.97 | 102.751× | 101.711–103.800× | 64 → 0 | 64 → 0 | 1,536 → 0 |
| shop/1024-false-true | 257,231.29 | 35,291.48 | 7.289× | 7.141–7.516× | 1,024 → 0 | 1,024 → 0 | 24,600 → 0 |
| shop/1024-true-true | 166,999.71 | 24,978.92 | 6.686× | 6.398–7.028× | 638 → 0 | 638 → 0 | 15,326 → 0 |
| catalog/1024 | 348,050.59 | 60,913.49 | 5.714× | 5.613–5.847× | 1,304 → 22 | 1,286 → 4 | 31,514 → 718 |
| enqueue/128 | 54,389.40 | 34,480.24 | 1.577× | 1.552–1.597× | 386 → 386 | 133 → 5 | 19,610 → 18,220 |
| profile/0 | 564.44 | 388.61 | 1.452× | 1.419–1.528× | 4 → 2 | 0 → 0 | 79 → 33 |
| profile/1 | 537.99 | 379.29 | 1.418× | 1.355–1.507× | 4 → 2 | 0 → 0 | 78 → 33 |
| profile/999 | 518.60 | 352.26 | 1.472× | 1.433–1.527× | 4 → 2 | 0 → 0 | 83 → 35 |
| profile/4294967295 | 528.25 | 375.62 | 1.406× | 1.326–1.577× | 4 → 2 | 0 → 0 | 113 → 50 |
| palette/1-short | 135.30 | 65.78 | 2.057× | 1.909–2.134× | 2 → 1 | 0 → 0 | 33 → 24 |
| palette/16-short | 1,300.38 | 216.05 | 6.019× | 5.931–6.110× | 17 → 1 | 0 → 0 | 534 → 384 |
| palette/128-short | 10,704.80 | 1,377.55 | 7.771× | 7.640–7.987× | 129 → 1 | 0 → 0 | 4,370 → 3,072 |
| palette/32-long | 4,714.42 | 2,538.13 | 1.857× | 1.821–1.897× | 65 → 33 | 0 → 0 | 5,238 → 3,328 |
| owned-control/32-long | 4,687.12 | 4,637.10 | 1.011× | 0.992–1.044× | 65 → 65 | 0 → 0 | 5,238 → 5,238 |

Profile measurements include the original caller’s conversion into retained Arc storage. Palette measurements include the original input-vector/string copies. The owned-control case keeps those owned inputs in both variants and checks the unchanged path. Shop labels encode item count, mixed availability, and prequeued entries. Scans include the complete ready-count predicate; catalog/1024 measures complete catalog actor construction; enqueue/128 measures the actual bulk-download result and queue mutation. Retained presentation cache hits are outside these setup/rebuild measurements.

## Regression and ITGmania compatibility

9 new regression tests passed. Across the four suites below, 1,651 tests passed and 42 pre-existing failures matched baseline outcomes and normalized diagnostics.

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| deadsync_theme_simply_love-tests | 1272 | 1 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 146 | 40 | 77 |
| actor | 30 | 1 | 0 |

Regression coverage includes singular/plural and maximum counts, inline text boundaries, Unicode, owned-input allocation parity, both players’ palette selections, layout invalidation, opaque and repeated item IDs across all four shops, repeated downloads, availability filters, messages, and complete catalog Actor output. The fixed state-size cost is asserted. Thread IDs and separately re-proven unordered diagnostics are the only normalized output. The compatibility suite still has the baseline failures shown above.

The existing theme failure is `screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height`; the actor failure is `geometry::lua_align_matches_native`. The semantic suite has 40 existing failures, including unavailable archive checks and Lua parity failures. These suites are not fully green; the comparison verifies that this pass introduces no additional failures or diagnostic changes.

| Recorded ITGmania capture | Comparisons | Failures |
|---|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `0f2ef3f98744` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `af2f887d212d` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode — `b05379b7d12b` | 304,425 | 0 |
| Warp Zone — `ded0f7ff1951` | 212,220 | 0 |
| Let Me Hear That — `2a77063dd2ab` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle — `7ffacb89fd95` | 201,471 | 0 |

Total: **1,650,933 comparisons per variant**, with zero failures in both variants.

## Reproduction and evidence

Both variants use the same build command:

```text
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Run the theme test executable with `benchmark_transient_buffers --ignored --nocapture --test-threads=1` for paired benchmarks. Original implementations and allocator support are test-only. Simfile/theme unit tests and semantic/actor tests ran with --test-threads=1. The root library was compiled, not executed. Six recorded native captures ran through full_song_lua in each variant. All 848 noteskin assets were verified against Git before the baseline build.

Ignored target/ holds build and suite logs, saved and hashed baseline executables, source/binary hashes, four benchmark logs, benchmark-summary.json, compatibility-comparison.json and pending-branch audits. The pass uses a shared generated Cargo cache and the same local temporary extraction directory for both variants. Source edits and the commit are confined to the new D: worktree.

Excluded from the commit: deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1. The original checkout was not written by this pass; its main branch and uncommitted work may advance independently.
