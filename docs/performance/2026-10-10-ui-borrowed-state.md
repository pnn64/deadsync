# Menu ownership and leaderboard data flow

Date: 2026-10-10

Branch: `perf/1872-ui-borrowed-state-20261010`

Starting main: `33b5a318211e2b9ab11a02976c97facc0bbbbe7d`

Version: **0.5.1871 → 0.5.1872**, exactly one patch increment in Cargo.toml and all three workspace-version entries in Cargo.lock.

## Changes

1. **Menu entries:** consume the temporary category lists and move their items into the final wheel. Removes the eight-argument slice adapter and six repeated copy loops. Reserve the final entry count once. The first available expanded category still wins; absent and empty categories remain distinct, and all labels and ordering are preserved. Both production callers already discarded the temporary lists after rebuilding the menu.
2. **Leaderboard snapshots:** move the incoming machine pane into retained state before applying an online snapshot. Removes a deep copy of the machine records followed by destruction of the original. Borrow the selected pane's name after loading/error returns, and remove a second machine-pane fallback that could never run. Loading/error behavior, stale-chart rejection, pane identity and fallback selection remain unchanged.
3. **Leaderboard rendering:** retain the local-self-adjusted slice for the duration of rendering and select borrowed row references using the existing prioritization API. Removes the temporary owned-entry helper and the extra copies of names, tags, and dates. Loading, error, and disabled panes skip row preparation. The local-self transformation, ordering, formatting and highlights are unchanged.

All production changes are outside Song Lua. The pending audit covered **71 refs / 45 distinct unmerged heads**. Only `select_music.rs` overlaps a pending production file: `perf/1872-ui-dataflows-20261009` changes top-grade grouping and lobby status text; this pass only transfers ownership at the two menu rebuild calls. No pending leaderboard or menu-construction optimization is duplicated.

## Validation

Fresh release baseline and modified builds used the same combined package graph, `--locked`, and `profile.release.lto=false`. Dependencies were built in the existing cache at `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build`, outside the original checkout. Temporary files and evidence are in this worktree's ignored `target/`. All 848 tracked noteskin asset blobs were verified against Git before baseline execution.

| Suite | Passed | Baseline failures | Ignored | New passing tests |
| --- | --- | --- | --- | --- |
| deadsync_theme_simply_love-tests | 1269 | 1 | 6 | 6 |
| deadsync_simfile-tests | 203 | 0 | 0 | 0 |
| semantic | 144 | 40 | 77 | 0 |
| actor | 30 | 1 | 0 | 0 |

Individual test outcomes and failure diagnostics match the fresh baseline. Thread IDs are normalized; the alias-line order and first-missing-archive diagnostics are normalized only after fresh unchanged-binary reruns proved their nondeterministic ordering. No baseline failures were silently excluded.

Six new regression tests cover all 64 category expansion masks crossed with optional-category presence and empty/populated lists, moved-string identity and allocations, snapshot transition sequences and pane selection, and complete rendered leaderboard actor trees for both fonts, one/two players, local-self overrides, unavailable states, and out-of-range selections. Six frozen original function bodies were mechanically checked against the starting commit.

**1,650,933 full-song comparisons passed, zero failed**, independently on both revisions, using recorded ITGmania captures rather than a live ITGmania run.

| Capture | Archive | Comparisons |
| --- | --- | --- |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | 0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst | 363,873 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis | af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst | 363,873 |
| 280\|MODS\|[MASTER] Sharkmode | b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst | 304,425 |
| Warp Zone | ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst | 212,220 |
| Let Me Hear That | 2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst | 205,071 |
| 272\|MODS\|[lv.02] Riddle | 7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst | 201,471 |

The failing tests and their exact diagnostics remain in `target/baseline-*.log` and `target/current-*.log`; `target/compatibility-comparison.json` records their equality. These cover pre-existing fixture availability and native semantic/geometry discrepancies; they are not a fully green compatibility suite.

## Paired benchmarks

Windows x86-64, Intel(R) Xeon(R) CPU E5-2696 v4 @ 2.20GHz (44 logical CPUs), release optimization with LTO disabled. Each pair invokes the actual modified function and a frozen original in the same executable. Four fresh processes run sequentially after our builds/tests finish, pinned to logical CPU 2 (mask 4) at AboveNormal priority. Each variant gets three warm-up calls, calibration toward 25 ms batches (maximum 1,000,000 calls), and nine alternating timed batches. The table reports medians of four process medians; ranges are the four paired original/current ratios.

Menu fixtures use the production playlist/pad-profile constructors and actual static sort list. Both pipelines include the same cloning of an immutable input list into newly owned temporary lists and all destruction, representing the ownership passed by production callers. This common setup makes the timing conservative for the entry builder alone. Snapshot timing includes identical view materialization and applies repeated updates to separate equivalent retained states. Rendering uses the production `INCLUDE_ICONS=false` instantiation and its 73/118-actor initial capacities, including construction/destruction of the output vector; both variants borrow the same immutable overlay. Conversion to a retained Arc and steady cache-hit frames are outside this renderer measurement.

After the initial full test run, the renderer benchmark was corrected to this production instantiation/capacity and actor-tree comparisons were extended to both icon variants, before collecting any timing samples. The entire theme suite was rerun after rebuilding. SHA-256 hashes confirmed that the simfile, semantic, actor and full-song executables were unchanged, preserving their completed results. The initial theme log and executable/source hashes remain in `target/initial-*`.

A separate scoped System-allocator sample uses the same closures to count allocation calls, reallocations, cumulative requested bytes, and the high-water mark of additional live requested bytes above the sample's starting allocation balance. This last measure excludes existing input/state memory and allocator internals; it is not peak RSS. Menu/render samples construct and destroy their outputs within the sample. The existing full-pane local-self copy is retained through rendering, so the peak measurement checks this longer lifetime as well as allocation churn. These are path throughput measurements, not whole-game frame-rate claims. The 1,024-playlist and 64-row snapshot cases are stress cases; empty menus/rendering, stale charts and local-only updates are controls.

| Case | Original ns/op | Current ns/op | Speedup | Paired range |
| --- | --- | --- | --- | --- |
| render/empty | 4080.28 | 3934.87 | 1.037× | 0.970–1.374× |
| render/single-13 | 19465.79 | 15928.06 | 1.222× | 1.163–1.330× |
| render/single-64 | 19420.33 | 15981.54 | 1.215× | 1.182–1.244× |
| render/double-13 | 33127.67 | 28627.76 | 1.157× | 1.146–1.199× |
| render/loading-64 | 8712.29 | 4044.34 | 2.154× | 2.042–2.201× |
| render/error-64 | 8724.86 | 4014.70 | 2.173× | 2.031–2.199× |
| render/disabled-64 | 8931.91 | 4045.59 | 2.208× | 2.152–2.280× |
| render/local-self-64 | 37661.85 | 34208.56 | 1.101× | 1.023–1.186× |
| snapshots/ready-13 | 15949.36 | 12244.91 | 1.303× | 1.250–1.336× |
| snapshots/ready-64 | 74955.67 | 57305.21 | 1.308× | 1.232–1.338× |
| snapshots/loading-13 | 6305.88 | 2954.83 | 2.134× | 1.995–2.244× |
| snapshots/error-13 | 6291.18 | 2935.28 | 2.143× | 1.944–2.213× |
| snapshots/local-only-13 | 5467.90 | 5566.64 | 0.982× | 0.975–1.016× |
| snapshots/stale-13 | 2947.12 | 2996.00 | 0.984× | 0.979–1.020× |
| snapshots/empty-ready | 791.88 | 622.14 | 1.273× | 1.247–1.297× |
| menu/collapsed-static | 1304.31 | 1064.49 | 1.225× | 1.216–1.263× |
| menu/sorts-static | 1845.35 | 1167.03 | 1.581× | 1.515–1.602× |
| menu/profile-static | 1938.47 | 1356.00 | 1.430× | 1.389–1.450× |
| menu/pad-32 | 10842.76 | 6202.89 | 1.748× | 1.661–1.803× |
| menu/playlists-100 | 29115.97 | 15801.95 | 1.843× | 1.673–1.902× |
| menu/playlists-1024 | 308578.29 | 161845.60 | 1.907× | 1.833–1.964× |
| menu/empty-expanded | 992.63 | 1022.98 | 0.970× | 0.954–0.984× |

| Case | Allocations | Reallocations | Requested bytes | Peak additional live bytes |
| --- | --- | --- | --- | --- |
| render/empty | 4 → 4 | 0 → 0 | 33917 → 33917 | 33917 → 33917 |
| render/single-13 | 96 → 56 | 0 → 0 | 36330 → 34650 | 36202 → 34650 |
| render/single-64 | 96 → 56 | 0 → 0 | 36318 → 34632 | 36214 → 34632 |
| render/double-13 | 190 → 110 | 0 → 0 | 59643 → 56283 | 57407 → 55855 |
| render/loading-64 | 45 → 4 | 0 → 0 | 35709 → 33919 | 35687 → 33919 |
| render/error-64 | 45 → 4 | 0 → 0 | 35707 → 33917 | 35687 → 33917 |
| render/disabled-64 | 45 → 4 | 0 → 0 | 35706 → 33916 | 35687 → 33916 |
| render/local-self-64 | 289 → 249 | 0 → 0 | 44626 → 42941 | 43995 → 42941 |
| snapshots/ready-13 | 209 → 167 | 0 → 0 | 8724 → 7019 | 7029 → 5324 |
| snapshots/ready-64 | 974 → 779 | 0 → 0 | 41874 → 33539 | 33549 → 25214 |
| snapshots/loading-13 | 84 → 42 | 0 → 0 | 3413 → 1708 | 3403 → 1708 |
| snapshots/error-13 | 86 → 44 | 0 → 0 | 3437 → 1732 | 3403 → 1708 |
| snapshots/local-only-13 | 83 → 83 | 0 → 0 | 3403 → 3403 | 3403 → 3403 |
| snapshots/stale-13 | 41 → 41 | 0 → 0 | 1691 → 1691 | 1691 → 1691 |
| snapshots/empty-ready | 9 → 7 | 0 → 0 | 324 → 299 | 309 → 284 |
| menu/collapsed-static | 4 → 4 | 1 → 0 | 3136 → 2696 | 2784 → 2696 |
| menu/sorts-static | 4 → 4 | 3 → 0 | 4632 → 3136 | 3488 → 3136 |
| menu/profile-static | 5 → 5 | 3 → 0 | 5272 → 3512 | 4128 → 3512 |
| menu/pad-32 | 69 → 37 | 5 → 0 | 16260 → 7854 | 10892 → 7854 |
| menu/playlists-100 | 205 → 105 | 6 → 0 | 37944 → 21768 | 26944 → 21768 |
| menu/playlists-1024 | 2053 → 1029 | 10 → 0 | 501528 → 202872 | 321568 → 202872 |
| menu/empty-expanded | 4 → 4 | 0 → 0 | 2168 → 2168 | 2168 → 2168 |

## Controls and limits

The synthetic empty-expanded playlist case is **992.63 → 1,022.98 ns** (about **30 ns / 3.1% slower**), with unchanged allocations and peak additional bytes. This edge case supplies `Some(Vec::new())`; the production menu builder uses `None` for an empty playlist library. It remains in the results and is a measured latency tradeoff, not a claimed improvement. Local-only and stale-chart snapshot controls have slightly slower aggregate medians but paired ranges cross parity; empty-render timing also crosses parity. The targeted populated menu, snapshot-update and renderer cases improve in all four paired runs.

Every measured case has equal or lower allocation calls, reallocations, requested bytes and peak additional live bytes. The local-self renderer's longer buffer lifetime still lowers the measured peak from **43,995 to 42,941 bytes**. There are no behavioral regressions in the covered tests, but the baseline compatibility failures remain unresolved.

## Reproduction and evidence

```powershell
$env:CARGO_TARGET_DIR = "C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build"
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run --message-format=json
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib borrowed_state -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-theme-simply-love --lib benchmark_ui_borrowed_state -- --ignored --nocapture --test-threads=1
```

The combined build's `target/*-binaries.json` manifests identify the exact executables used. The runner executes the theme and simfile libraries, semantic parity, actor conformance, and the six full-song archive selectors directly. The root library is compiled. Benchmark pinning is recorded in `target/run-bench.py`; `final-1.log` through `final-4.log` retain raw output. `benchmark-summary.json`, source/binary hashes, pending-ref audit, baseline/current diagnostics, and the original-checkout snapshots remain local under `target/`.

```text
rustc 1.98.1 (48a229cea 2026-09-01)
binary: rustc
commit-hash: 48a229ceaefd4985c50990b14116b6d856af0985
commit-date: 2026-09-01
host: x86_64-pc-windows-msvc
release: 1.98.1
LLVM version: 22.1.8
```

The original checkout advanced independently during the pass. Its final audit records HEAD `58e888a2404a7e7b68e7028b8da444c2784b1032`; its uncommitted work was left untouched. This perf branch remains based on the starting main commit above and is not merged. The four excluded files are absent from the commit file list.
