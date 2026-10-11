# UI data pipelines — 2026-10-10

Branch: `perf/1872-ui-pipelines-20261010`. Base: `5989af816660f2b98556641d208fb571f8dcae00` (latest committed main at worktree creation). Version: **0.5.1871 → 0.5.1872**, updated in Cargo.toml and Cargo.lock. No merge or push.

Audited 76 unmerged local/remote perf refs (50 distinct commits). None overlap this pass’s four production files. No Song Lua source or fixture changes.

## Changes

1. Chart preview selection compares difficulty names with `eq_ignore_ascii_case`. Removes the lowercase String created for each candidate, while preserving the original singles/doubles priority, first-match behavior, and last-chart fallback.
2. Profile import summaries retain the existing localized `Arc<str>` values. Removes the `Arc → String → Arc` round trips for summary lines and changes the ratio helper to return its existing shared result directly. All statuses, colors, labels, counts, and optional notes remain the same.
3. Option navigation borrows cached row layouts and copies only the coordinate/index it needs. Deletes the owned-layout wrapper and removes four Arc increments/decrements per layout access. The borrow ends before state mutation or cache invalidation. This changes both vertical navigation helpers and horizontal choice navigation.

## Paired release benchmarks

Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.98.1. Release with LTO disabled; baseline/current use the same combined feature graph. Seven frozen baseline function bodies were verified against the base commit. Four fresh processes pinned to logical CPU 2 at AboveNormal priority; three warm calls, calibration to roughly 25 ms, and nine alternating timed batches per variant. Values are medians across process medians; ranges show the four paired ratios. No builds or compatibility tests from this pass ran during formal timings.

Inputs and outputs pass through black_box; output destruction is included. A scoped System allocator counts allocation/reallocation calls and requested bytes separately from timing. Both timed variants use that allocator with counting disabled. These are operation-level measurements, not whole-frame or end-to-end application speedups.

| Workload | Original ns | Current ns | Throughput | Paired range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| preview/1-single | 66.83 | 11.70 | 5.712× | 5.327–6.144× | 1 → 0 | 0 → 0 | 9 → 0 |
| preview/12-single | 439.90 | 16.11 | 27.314× | 27.268–29.074× | 7 → 0 | 0 → 0 | 33 → 0 |
| preview/12-double | 752.25 | 27.45 | 27.405× | 25.072–27.417× | 12 → 0 | 0 → 0 | 53 → 0 |
| preview/64-fallback | 3,835.25 | 100.27 | 38.249× | 36.139–39.295× | 64 → 0 | 0 → 0 | 256 → 0 |
| preview/128-single | 3,985.70 | 80.47 | 49.533× | 45.789–50.532× | 65 → 0 | 0 → 0 | 265 → 0 |
| summary/empty | 2,875.56 | 1,468.74 | 1.958× | 1.945–1.980× | 20 → 2 | 2 → 2 | 1,635 → 1,160 |
| summary/all | 4,616.23 | 2,966.59 | 1.556× | 1.507–1.585× | 35 → 15 | 2 → 2 | 1,890 → 1,297 |
| summary/partial | 4,193.14 | 2,617.89 | 1.602× | 1.514–1.618× | 30 → 12 | 2 → 2 | 1,753 → 1,268 |
| layout/sync-visual | 65.76 | 13.71 | 4.795× | 4.731–5.136× | 0 → 0 | 0 → 0 | 0 → 0 |
| layout/apply-visual | 69.22 | 13.74 | 5.038× | 4.918–5.156× | 0 → 0 | 0 → 0 | 0 → 0 |
| layout/sync-color | 72.03 | 13.59 | 5.300× | 4.887–5.607× | 0 → 0 | 0 → 0 | 0 → 0 |
| layout/apply-color | 67.59 | 13.38 | 5.054× | 5.014–5.094× | 0 → 0 | 0 → 0 | 0 → 0 |
| layout/sync-volume | 65.97 | 13.29 | 4.966× | 4.752–5.139× | 0 → 0 | 0 → 0 | 0 → 0 |
| layout/apply-volume | 67.94 | 13.82 | 4.915× | 4.854–5.189× | 0 → 0 | 0 → 0 | 0 → 0 |

Preview workloads place a mixed-case top difficulty last, or exercise the fallback with no top difficulty. Summary workloads cover empty, fully imported, and partially imported sections. Layout workloads exercise actual sync/apply navigation on warmed visual-style, color, and volume rows; both baseline and current allocate nothing there, and the gain comes from eliminating shared-owner operations. The layout cache’s construction, layout math, and rendered actors are unchanged.

An initial benchmark attempt stopped because its preferred-color row was hidden by the default settings. The fixture now explicitly enables that row, and the reported measurements come from four complete runs after rebuilding and retesting the final sources. The initial log is retained separately.

## Regression and ITGmania compatibility

8 new regression tests passed. Across the four suites below, 1,650 tests passed; 43 pre-existing failures matched baseline outcomes and normalized diagnostics.

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| deadsync_theme_simply_love-tests | 1271 | 1 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 146 | 41 | 77 |
| actor | 30 | 1 | 0 |

Regression coverage includes every ASCII case permutation of Challenge/Expert, generated chart lists with Unicode and invalid labels, selection priorities, allocation-free preview scans, all 128 combinations of import-summary flags, zero/large/maximum counts, opaque profile names, exact line/status/color equality, every option submenu, out-of-range indices, cache misses and invalidation, retained geometry identity, and allocation-free warmed navigation. Existing choice-input tests cover the horizontal caller.

The existing theme failure is `screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height`; the actor failure is `geometry::lua_align_matches_native`. Semantic failures include unavailable archive checks and existing Lua parity failures. The suites are not fully green. The comparison checks each existing test outcome and failure diagnostic; only thread IDs and separately re-proven unordered diagnostics are normalized.

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

Both variants use this build command:

```text
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Run the theme test executable with `benchmark_ui_pipelines --ignored --nocapture --test-threads=1` for paired benchmarks. The original implementations and allocation tracker are test-only. Theme/simfile unit suites and semantic/actor tests ran with --test-threads=1. The root library was compiled, not executed. Six recorded native captures ran through full_song_lua in each variant. All 848 noteskin assets were verified against Git before the baseline build.

Ignored target/ contains build/suite logs, saved and hashed baseline executables, source/binary hashes, four benchmark logs, benchmark-summary.json, compatibility-comparison.json, and pending-branch audits. Both variants use a shared generated Cargo cache and the same local temporary extraction directory. Source edits and the commit are confined to the new worktree.

Excluded from the commit: deadsync-song.json.gz, rust-performance.md, optimize.sh, and optimize.ps1. This pass did not write the original checkout; its main branch and uncommitted work may advance independently.
