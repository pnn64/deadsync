# Initials and diagnostic text performance — 2026-10-10

Branch: `perf/1872-string-pipelines-20261010`. Base: `e001ee23bfe8df93aec868d6294eb6de895883df` (committed main at worktree creation). Version: **0.5.1871 → 0.5.1872** in Cargo.toml and Cargo.lock. No merge or push.

Audited 74 unmerged local/remote perf refs (48 distinct commits). None changes Initials or Test Input; pending browser changes affect other functions. No Song Lua source or fixture changes.

## Changes

1. Borrow leaderboard entries and chart hashes during initials high-score setup. The temporary cache owns only consumed-score flags; final display rows still own their text. Removes whole-table copies of names, dates and machine tags while preserving reverse-stage matching independently for each player.
2. Format the Test Input event-rate summary directly into its final string. Removes the two temporary frequency strings and their helper, preserving the 1,000 Hz display cap.
3. Format the content-browser reload summary directly. Removes the temporary vector, component strings and join, preserving zero-count fallback, singular/plural wording and all actor properties.

## Paired release benchmarks

Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.98.1. Release with LTO disabled and identical feature graph/compiler flags for both variants. Frozen original function bodies are verified byte-for-byte against the base commit. Four fresh processes pinned to logical CPU 2 at AboveNormal priority, three warm calls, calibration to roughly 25 ms, then nine alternating timed batches per variant. Values are medians across the four process medians; ranges describe the four paired ratios. No builds or compatibility tests from this pass ran during these measurements.

Timed outputs pass through black_box and include amortized destruction. Reload actor vectors are preallocated and reused. The scoped System allocator counts allocations, reallocations and requested bytes separately; counting is disabled for timings, with the same instrumented allocator in both variants. These are operation-level throughput and allocation results, not end-to-end frame-rate claims.

| Workload | Original ns | Current ns | Throughput | Paired range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| input/0-0 | 622.46 | 209.44 | 2.972× | 2.948–3.103× | 3 → 1 | 2 → 0 | 72 → 40 |
| input/250-1000 | 662.99 | 187.42 | 3.537× | 3.444–3.555× | 3 → 1 | 2 → 0 | 72 → 40 |
| input/1000-1001 | 615.59 | 187.09 | 3.290× | 3.274–3.376× | 3 → 1 | 2 → 0 | 75 → 40 |
| input/4294967295-4294967295 | 602.56 | 215.07 | 2.802× | 2.755–2.924× | 3 → 1 | 2 → 0 | 78 → 40 |
| reload/0-0 | 1,322.17 | 1,239.85 | 1.066× | 1.008–1.083× | 7 → 6 | 0 → 0 | 224 → 176 |
| reload/1-0 | 1,803.62 | 1,503.96 | 1.199× | 1.186–1.226× | 9 → 6 | 1 → 1 | 246 → 184 |
| reload/0-1 | 1,736.86 | 1,423.64 | 1.220× | 1.205–1.245× | 9 → 6 | 1 → 1 | 244 → 182 |
| reload/1-1 | 1,906.33 | 1,633.35 | 1.167× | 1.106–1.189× | 10 → 6 | 1 → 2 | 292 → 216 |
| reload/12-12 | 2,623.91 | 2,318.30 | 1.132× | 1.095–1.160× | 10 → 6 | 1 → 2 | 308 → 216 |
| initials/empty-table | 3,116.47 | 3,026.43 | 1.030× | 1.012–1.034× | 29 → 28 | 0 → 0 | 1,023 → 1,000 |
| initials/five | 8,620.97 | 7,692.10 | 1.121× | 1.113–1.156× | 56 → 39 | 5 → 5 | 1,758 → 1,165 |
| initials/hundred | 34,794.64 | 9,223.15 | 3.773× | 3.677–3.954× | 341 → 39 | 5 → 5 | 12,683 → 1,260 |
| initials/thousand | 346,879.08 | 7,630.20 | 45.461× | 39.740–48.746× | 3041 → 39 | 5 → 5 | 116,183 → 2,160 |
| initials/repeated | 108,178.69 | 82,480.53 | 1.312× | 1.274–1.314× | 737 → 435 | 60 → 60 | 25,619 → 14,020 |
| initials/distinct | 418,068.93 | 84,766.83 | 4.932× | 4.902–5.011× | 4070 → 446 | 60 → 60 | 152,198 → 15,120 |

Mixed pack/song reload summaries perform one extra reallocation (1 → 2), while total allocation/reallocation calls fall from 11 to 8 and requested bytes fall from 292/308 to 216. The complete dialog is still faster in every measured process.

Initials cases measure complete construction of one player’s high-score lists: empty/5/100/1,000-row tables for one stage, plus twelve stages sharing one chart or using distinct charts with 100 rows each. Fixtures include machine tags. The production initials runtime requests the full local leaderboard (usize::MAX), so table-copy costs scale with stored history even though only five rows are displayed. Input cases span zero, normal, boundary and saturated readings. Reload cases measure the complete dialog composition with zero, singular and plural pack/song counts. Retained presentation cache hits are outside these setup/rebuild measurements.

## Regression and ITGmania compatibility

8 new regression tests passed. Across the four suites below, 1,649 tests passed and 42 existing failures matched the baseline. Every pre-existing individual outcome and normalized failure diagnostic was compared.

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| deadsync_theme_simply_love-tests | 1271 | 1 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 145 | 40 | 77 |
| actor | 30 | 1 | 0 |

The unchanged non-semantic failures are `cyber_model_tap_scale_uses_model_height_not_logical_height` and `geometry::lua_align_matches_native`. The semantic suite retains its 40 baseline failures, including two missing non-local archive references. This is a baseline-matched result, not a claim that the entire compatibility suite is green.

Regressions cover both players, repeated and missing charts, duplicate score consumption in reverse stage order, Unicode names, unusual ranks/scores, all frequency boundaries around the display cap, and reload-dialog actor output at two viewport sizes. Resource checks assert that cached score tables are borrowed and that transient string/vector allocations disappear. Only thread IDs and separately re-proven unordered diagnostics are normalized.

| Recorded ITGmania capture | Comparisons | Failures |
|---|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `0f2ef3f98744` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `af2f887d212d` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode — `b05379b7d12b` | 304,425 | 0 |
| Warp Zone — `ded0f7ff1951` | 212,220 | 0 |
| Let Me Hear That — `2a77063dd2ab` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle — `7ffacb89fd95` | 201,471 | 0 |

Total: **1,650,933 comparisons per variant**, with zero failures in both the baseline and current implementation.

Both variants use the same combined build graph:

```text
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Simfile/theme unit tests and semantic/actor compatibility tests ran with --test-threads=1. The root library was compiled, not executed. Six recorded ITGmania captures were exercised by full_song_lua in both variants. All 848 tracked noteskin assets were verified against Git before the baseline build. Baseline executables were copied and hashed before editing production sources.

## Reproduction and evidence

Run paired benchmarks in the combined feature graph above using the theme test executable with `benchmark_string_pipelines --ignored --nocapture --test-threads=1`. Original implementations and allocator support are test-only.

Ignored target/ contains per-variant build and suite logs, saved baseline executables and hashes, source hashes, six full-song outputs per variant, four benchmark logs, benchmark-summary.json, compatibility-comparison.json and pending-branch audits. A shared generated Cargo cache and local temporary extraction directory on C: avoid duplicate dependencies and network extraction overhead. Both completed compatibility runs use the same temporary directory. Unused generated dependency artifacts were relocated into this worktree after excluding every artifact in the baseline/current build manifests; per-file SHA-256 hashes and paths are recorded in target/relocated-cache.json and target/relocated-cache-recovery.jsonl. A build and native run interrupted by C: disk exhaustion are preserved separately and excluded from final results; completed native runs were restarted from the saved baseline and final binaries. Source edits and the commit are confined to the new D: worktree.

Excluded from the commit: deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1. The original checkout was not written by this pass; its main branch and uncommitted Song Lua work may advance independently.
