# Profile loading and player-option input

Pass: 2026-10-10. Branch: `perf/1872-profile-dataflow-20261010`.
Base: `37b94e1416b49bfea3d1875e12f719e147bb0ab3`, the latest committed main when the separate worktree was created. Version: **0.5.1871 → 0.5.1872**, changed exactly once in Cargo.toml and Cargo.lock.

## Changes

1. Player-option input compares the current options, palette and device identifiers by reference with the saved pre-change snapshot. It no longer clones the entire current snapshot just to compare it. Bitmask input also stops copying an identifier its projection cannot change, and validates the selected bit before taking the necessary pre-change options snapshot. Persistence, audio, visibility and mirrored selections retain their original behavior.
2. The profile picker uses the existing borrowed INI parser, looks up the selected section once, and constructs display values only once. It removes owned copies of all INI names/values, eagerly cloned defaults that were immediately overwritten, and a redundant profile-ID copy. File reading, catalog scanning, score counts, legacy settings and fallback rules are preserved.
3. Visual-option loading assigns the four owned graphics/noteskin fields only after successful parsing. Missing or invalid values retain their existing buffers instead of cloning and immediately dropping them. Getter ordering, legacy aliases, optional component resets and noteskin migration still run as before.

No production cache, helper, dependency, or unsafe code was added. All three changes are outside Song Lua. The extra code is differential tests, frozen originals, and allocation/benchmark instrumentation.

## Paired release measurements

Each row is the median of four fresh-process medians. Both implementations run in the same executable, with three warmups, calibration toward 25 ms (capped at one million iterations), and nine timed batches in alternating order. Outputs are consumed; owned results are dropped. Each test binary gets four fresh processes (12 processes total), pinned to logical CPU 2 at AboveNormal priority. Allocation counting runs separately from timing.

Windows x86-64 MSVC, Intel Xeon E5-2696 v4 @ 2.20 GHz (22 cores/44 logical processors), rustc 1.98.1 / LLVM 22.1.8. Release optimization uses `profile.release.lto=false` for both variants. These are local throughput measurements, not CPU-cycle or end-to-end frame-rate claims.

| Operation | Original ns/op | Current ns/op | Throughput | Four-process range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| picker/sparse | 692,608.46 | 691,655.60 | 1.001× | 0.977–1.003× | 86 → 77 | 9 → 9 | 10,244 → 9,994 |
| picker/typical | 4,937,400.00 | 4,761,175.00 | 1.037× | 1.014–1.039× | 1813 → 365 | 106 → 106 | 247,108 → 207,188 |
| picker/large | 11,892,600.00 | 9,740,516.66 | 1.221× | 1.155–1.262× | 13725 → 685 | 243 → 243 | 1,705,704 → 1,361,122 |
| input/numeric | 1,302.53 | 916.25 | 1.422× | 1.375–1.430× | 16 → 11 | 0 → 0 | 892 → 800 |
| input/no_change | 988.37 | 577.83 | 1.711× | 1.696–1.732× | 10 → 5 | 0 → 0 | 184 → 92 |
| input/perspective | 1,629.02 | 1,101.26 | 1.479× | 1.339–1.548× | 16 → 11 | 0 → 0 | 892 → 800 |
| input/bitmask | 1,533.78 | 1,090.33 | 1.407× | 1.352–1.556× | 16 → 10 | 0 → 0 | 892 → 790 |
| visual/missing | 331.40 | 55.83 | 5.936× | 5.707–6.034× | 4 → 0 | 0 → 0 | 82 → 0 |
| visual/partial | 529.80 | 349.50 | 1.516× | 1.481–1.549× | 6 → 3 | 0 → 0 | 87 → 8 |
| visual/invalid | 779.84 | 530.23 | 1.471× | 1.344–1.613× | 8 → 4 | 0 → 0 | 225 → 143 |
| visual/complete | 881.32 | 857.50 | 1.028× | 0.987–1.034× | 8 → 8 | 0 → 0 | 97 → 97 |

Picker measurements include complete catalog scanning and real file I/O in an isolated temporary profile directory. Sparse has one profile; typical has eight profiles with 80 unrelated fields each; large has 16 profiles with 400 unrelated fields each. Setup, profile creation and cleanup are excluded. Files are warm after calibration. Requested-byte medians may vary slightly with per-process temporary path lengths. Allocation/reallocation counts are checked across processes.

Input measurements use initialized versus-mode state and include queued effects and their destruction. Numeric and perspective choices wrap; the no-change case keeps the current numeric value. The visual complete case supplies all four string fields and is a control with no expected allocation savings. Missing, partial and invalid cases exercise the removed fallback copies. These costs apply during input and loading, not every rendered frame.

Allocation figures use a test-only System allocator wrapper with scoped thread-local counters. The production allocator is unchanged. Requested bytes count allocation requests including reallocations; they are not process RSS. Peak deltas from the raw logs are deliberately not presented as live-memory peaks for in-place updates, since those operations can free buffers allocated before measurement.

The sparse picker is near parity (1.001× aggregate, 0.977–1.003× across processes). The complete visual-options control is also near parity (1.028× aggregate, 0.987–1.034× range), with unchanged allocation counts. No speedup is claimed for these controls. All 11 aggregate medians improved or stayed at parity, and no allocation/reallocation count increased.

## Behavioral validation

| Suite | Passed | Failed, unchanged from baseline | Ignored |
|---|---:|---:|---:|
| deadsync_shell-tests | 356 | 17 | 7 |
| deadsync_theme_simply_love-tests | 1265 | 1 | 4 |
| deadsync_profile-tests | 230 | 0 | 1 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 151 | 41 | 77 |
| actor | 30 | 1 | 0 |

Five new regression tests pass. They compare complete picker catalogs for legacy, duplicate and malformed values; compare visual-option results and exact getter call order; assert zero allocations and retained buffers for missing visual fields; and compare option input state, choices, visibility-related masks, and queued effects with the frozen originals.

All pre-existing unit and ITGmania semantic/actor outcomes and failure diagnostics match the baseline. Diagnostic comparison normalizes only thread IDs and two separately audited unordered reports: the same three missing Sprite.Load aliases, and whichever of the same two missing non-local archives is reported first. The alias check was repeated three times per binary; the missing-reference check ten times per binary, with source/index/reference files verified unchanged.

Of the 17 shell suite failures, 15 pass in isolated baseline and current processes; two underlying failures remain unchanged. The suite failures poison the shared session-test lock. This pass does not claim a completely green baseline or full Song Lua compatibility.

Six native ITGmania full-song captures were run with the saved baseline and updated executables, using identical archive selectors and the same temporary extraction directory:

| Capture | Comparisons per variant | Mismatches | Panics |
|---|---:|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | 363,873 | 0 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | 363,873 | 0 | 0 |
| 280\|MODS\|[MASTER] Sharkmode | 304,425 | 0 | 0 |
| Warp Zone | 212,220 | 0 | 0 |
| Let Me Hear That | 205,071 | 0 | 0 |
| 272\|MODS\|[lv.02] Riddle | 201,471 | 0 | 0 |

**1,650,933 native comparisons per variant, zero mismatches and zero panics.** These captures cover the exercised songs; they do not prove compatibility for untested songs or unavailable archives. Capture execution times were not used as performance measurements.

Both release builds completed without compiler warnings. Edited Rust was formatted; pre-existing formatting in the large profile/test files was preserved outside the edited function or appended test wiring. Frozen reference bodies were checked against the starting Git blobs. Source and executable SHA-256 manifests bind the tested changes to the commit. `git diff --check` passes.

## Isolation and overlap audit

The initial audit found 86 unmerged local/remote-tracking perf refs and 56 distinct heads. The final audit found 86 refs and 56 heads. The only selected production-file overlap is the pending profile-persistence branch (`18a3560c3`); its changes affect other functions, not visual-option loading. Player-option change detection and shell profile-picker loading have no pending production-file overlap.
The original checkout was used only for reads and worktree creation; its uncommitted work was not edited. Main at final audit was `6463950f77501f08fdc7752974c1c606ec74d3b9`. The branch is based on the pinned starting commit, committed locally, and left unmerged.

All source changes and pass scripts are in `D:/deadsync-perf-1872-profile-dataflow-20261010`. Generated Cargo artifacts reuse `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` for disk capacity. Baseline executables were copied and hash-checked before source edits. Raw logs, manifests and orchestration scripts are ignored under the worktree’s `target/` directory. The commit excludes deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1.

## Reproduction

Run the unit and compatibility suites from the worktree. The known baseline failures are expected; `--no-fail-fast` allows all requested suites to run:

```powershell
cargo test --release --locked --config profile.release.lto=false --no-fail-fast -p deadsync-shell -p deadsync-theme-simply-love -p deadsync-profile -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --no-fail-fast -p deadsync --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-shell -p deadsync-theme-simply-love -p deadsync-profile --lib benchmark_profile_dataflow -- --ignored --nocapture --test-threads=1
```

The paired benchmarks contain the frozen original implementations; rebuilding another checkout is unnecessary for those comparisons. For the native captures:

```powershell
$archives = @(
    "0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst",
    "af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst",
    "b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst",
    "ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst",
    "2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst",
    "7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst"
)
foreach ($archive in $archives) {
    cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- $archive
}
```
