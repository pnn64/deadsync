# View buffers — 2026-10-10

Branch: `perf/1872-view-buffers-20261010`. Base: `a9e3f71fd45f02cfe9e38d500e0cfd60f7172f5b` (latest committed main at worktree creation). Version: **0.5.1871 → 0.5.1872**, updated in Cargo.toml and Cargo.lock. No merge or push.

Initially audited 77 unmerged local/remote perf refs; the final audit covered 81 refs (51 distinct commits). None overlap the five production files changed here. Song Lua source and fixtures are unchanged.

## Changes

1. Library reload and ReplayGain progress overlays copy short detail text directly into the existing inline actor payload. Removes the reload-detail cloning helper. Text over 14 UTF-8 bytes follows the original owned-String path; event handling, progress, ETA, cache invalidation, and layout are unchanged.
2. Pack catalog names, metadata labels/values, and status text use the same inline conversion. Removes transient heap buffers for short visible text when rebuilding catalog actors. Longer text retains the original owned representation. Existing overlay caching remains intact.
3. Playlist libraries use the existing ASCII-insensitive comparator with stable sorting. Removes both allocated lowercase keys per playlist and their cached-key vector. Owner/name ordering, non-ASCII byte ordering, and the input order of tied entries remain unchanged.

## Paired release benchmarks

Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.98.1. Release with LTO disabled and the same combined Cargo feature graph for both variants. Eight frozen function bodies were verified against the base commit. Four fresh processes pinned to logical CPU 2 at AboveNormal priority; three warm calls, calibration to roughly 25 ms, and nine alternating batches per variant. Table values are medians across process medians; ranges show the four paired ratios. No builds or compatibility tests from this pass ran during formal timing.

Inputs and outputs pass through black_box; output destruction is included. Actor output vectors are preallocated and reused for both variants. The scoped System allocator counts allocation calls, reallocations, requested bytes, and additional live bytes separately from timing. Counting is disabled during timings in both variants.

These are complete actor-builder and playlist-library operations, not whole-frame FPS or application-wide speedups. Progress fixtures hold elapsed time at zero so both variants format the same speed label. Catalog results represent tree rebuilds, not hits in the existing retained cache. Long progress labels are fallback controls: they intentionally retain the original allocation behavior.

Playlist fixtures contain 16, 128, or 1,000 menu entries with mixed owners and deterministic shuffled names. Their song lookup and playlist bodies are empty to isolate menu construction and sorting; song resolution is unchanged. The long-key fixture uses a shared 380-byte prefix. Catalog fixtures contain one or seven visible packs, with short metadata and either short names or long Unicode names.

| Workload | Original ns | Current ns | Throughput ratio | Paired range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| replaygain/short | 1,996.61 | 1,768.02 | 1.129× | 1.001–1.154× | 8 → 6 | 1 → 1 | 1,997 → 1,975 |
| replaygain/long | 2,515.72 | 2,440.49 | 1.031× | 0.997–1.091× | 8 → 8 | 1 → 1 | 5,775 → 5,775 |
| catalog/1-short | 4,968.45 | 4,451.87 | 1.116× | 1.070–1.289× | 27 → 14 | 2 → 2 | 251 → 177 |
| catalog/7-short | 12,567.25 | 11,709.36 | 1.073× | 1.035–1.087× | 63 → 44 | 14 → 14 | 803 → 693 |
| catalog/7-long | 200,952.90 | 195,712.34 | 1.027× | 0.988–1.039× | 63 → 52 | 126 → 126 | 53,171 → 53,109 |
| reload/short | 1,898.03 | 1,769.62 | 1.073× | 1.057–1.088× | 6 → 4 | 1 → 1 | 1,976 → 1,954 |
| reload/long | 2,363.81 | 2,253.42 | 1.049× | 0.989–1.122× | 6 → 6 | 1 → 1 | 5,754 → 5,754 |
| playlists/16-short | 10,609.83 | 8,340.22 | 1.272× | 1.252–1.299× | 98 → 65 | 10 → 10 | 3,654 → 2,264 |
| playlists/128-short | 74,773.02 | 68,378.46 | 1.094× | 1.057–1.167× | 770 → 514 | 85 → 85 | 29,386 → 29,523 |
| playlists/1000-short | 852,912.06 | 793,589.02 | 1.075× | 1.026–1.119× | 6,002 → 4,002 | 666 → 666 | 230,372 → 231,432 |
| playlists/128-long | 120,728.68 | 96,302.90 | 1.254× | 1.082–1.302× | 770 → 514 | 85 → 85 | 125,386 → 77,523 |

Playlist tradeoff: stable sorting uses a larger temporary element buffer than sorting cached keys. In the 128- and 1,000-playlist short-key workloads this increases requested bytes slightly, while removing two allocation calls per entry. The exact byte deltas are shown above; this pass does not claim lower memory use for every playlist workload. Long-key workloads avoid copying the long sort keys.

The long-text fallback progress cases and long-name catalog case have paired timing ranges crossing 1.0, so their timing differences are inconclusive. Long-name catalog construction still removes 11 allocation calls for short metadata fields. The short progress/catalog cases and all playlist cases improve in every paired process median.

## Regression and native compatibility

8 new regression tests passed. Across the four suites below, **1,650 tests passed**. All **43 pre-existing failures** matched the baseline outcomes and normalized diagnostics; no new failures remained.

| Suite | Passed | Failed | Ignored |
|---|---:|---:|---:|
| deadsync_theme_simply_love-tests | 1271 | 1 | 7 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 146 | 41 | 77 |
| actor | 30 | 1 | 0 |

Actor regressions compare exact text and every other actor field, normalizing only the intended text-storage variant. Cases include every library phase, completion/cancellation combinations, unknown/zero/overflowing progress, Unicode, embedded NUL/newlines, long fallback labels, catalog selection boundaries, all five installation phases, and empty catalogs/status text. Allocation checks cover both inline and fallback cases. Playlist tests verify Unicode/ASCII ordering and stable ties by original identity.

The existing theme failure is `screens::components::gameplay::notefield::tests::cyber_model_tap_scale_uses_model_height_not_logical_height`; the actor failure is `geometry::lua_align_matches_native`. Semantic failures include missing archive checks and existing Lua parity differences. The suites are not fully green. Failure comparisons normalize thread IDs and separately re-proven unordered diagnostics only.

| Recorded ITGmania capture | Comparisons | Failures |
|---|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `0f2ef3f98744` | 363,873 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis — `af2f887d212d` | 363,873 | 0 |
| 280\|MODS\|[MASTER] Sharkmode — `b05379b7d12b` | 304,425 | 0 |
| Warp Zone — `ded0f7ff1951` | 212,220 | 0 |
| Let Me Hear That — `2a77063dd2ab` | 205,071 | 0 |
| 272\|MODS\|[lv.02] Riddle — `7ffacb89fd95` | 201,471 | 0 |

Total: **1,650,933 comparisons per variant**, zero failures in both variants.

## Reproduction and evidence

Both variants use:

```text
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
```

Run the resulting theme test executable with `benchmark_view_buffers --ignored --nocapture --test-threads=1` for all paired benchmarks. The frozen originals and allocator tracker are test-only. Theme/simfile unit suites and semantic/actor tests used --test-threads=1. The root library was compiled, not executed. Six recorded native captures ran through full_song_lua in each variant. All 848 noteskin assets were verified against Git before the baseline build.

Ignored target/ retains build/suite logs, saved and hashed baseline executables, source/binary manifests, exploratory logs, four final benchmark logs, benchmark-summary.json, compatibility-comparison.json, and branch audits. Both variants use a shared generated Cargo cache and the same local compatibility-extraction directory.

All source edits and the commit are confined to this worktree. This pass did not write the original checkout; its main branch and uncommitted work may advance independently. Excluded from the commit: deadsync-song.json.gz, rust-performance.md, optimize.sh, optimize.ps1.
