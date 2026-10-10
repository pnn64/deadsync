# Course loading and deterministic selection

Pass: 2026-10-10. Branch: `perf/1872-borrowed-lookups-20261010`.
Base: `9cd643329b1f3dfef20a4f340ac4678987af9616`, the latest committed main at worktree creation.
Version: **0.5.1871 -> 0.5.1872**, changed exactly once in Cargo.toml and Cargo.lock.

## Changes

1. Remove the discarded preliminary course-resolution pass in `select_course::build_init_data`. Each rating already resolves its own stages and supplies the displayed duration/BPM metadata. A default rating always exists: explicit nonnegative meters retain their ratings even without charts; otherwise Medium is included and retained. Random courses with at most one explicit meter include all difficulties, including Medium. The deleted preliminary stage resolutions, repeat-key vector, duration accumulation and BPM scanning have no retained side effects. Random seed creation and course flags remain.
2. Resolve fixed-song entries directly. The general candidate struct/vector and separate heap chart-index buffer are unnecessary once a lookup has selected one song. A SmallVec stores up to eight chart indices inline and reserves once for larger songs. Chart filters, order, difficulty shifts, shared song ownership and stage metadata remain identical. Empty charts return immediately. The existing smallvec dependency is reused.
3. Skip song/path hashing and song-key construction when selection has only one possibility. Stateless seeded hashing modulo one always returns zero; there is no mutable RNG state to advance. Multi-candidate random/ranked paths retain the original formulas and their existing cached song keys.

All production changes are outside Song Lua, in `crates/deadsync-simfile/src/course.rs` and `crates/deadsync-theme-simply-love/src/screens/select_course.rs`. No production dependency, cache or unsafe code was added.

## Paired release measurements

Each result combines four fresh-process medians. Each process runs three warmups, calibration toward 25 ms (capped at 4,096 operations), and nine measured batches in alternating order. Original and current implementations run in the same executable. Input preparation is outside timing/allocation counting; result consumption and dropping are inside. Input-batch vector traversal/destruction is common to both variants.

Windows x86-64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz, 44 logical processors; rustc 1.98.1 / LLVM 22.1.8. Release optimization with `profile.release.lto=false`; processes pinned to logical CPU 2 at AboveNormal priority. Formal measurements ran after baseline compatibility work and before current compatibility work, without this pass's builds or native harness running concurrently. These are local function measurements, not frame-rate or CPU-cycle claims.

| Operation | Original ns/op | Current ns/op | Throughput | Four-process range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| resolve/fixed_0 | 223.53 | 216.05 | 1.035x | 0.985-1.074x | 2 -> 2 | 0 -> 0 | 9 -> 9 |
| fixed_buffers/0 | 16.62 | 13.17 | 1.262x | 1.174-1.269x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| resolve/fixed_1 | 785.18 | 358.69 | 2.189x | 2.112-2.534x | 6 -> 3 | 0 -> 0 | 112 -> 32 |
| fixed_buffers/1 | 286.87 | 131.84 | 2.176x | 2.028-2.277x | 3 -> 1 | 0 -> 0 | 87 -> 23 |
| resolve/fixed_4 | 960.65 | 705.74 | 1.361x | 1.231-1.374x | 6 -> 4 | 0 -> 0 | 136 -> 48 |
| fixed_buffers/4 | 617.56 | 501.47 | 1.232x | 1.038-1.461x | 4 -> 2 | 0 -> 0 | 127 -> 39 |
| resolve/fixed_64 | 2,669.39 | 2,586.36 | 1.032x | 1.022-1.038x | 6 -> 5 | 0 -> 0 | 616 -> 560 |
| fixed_buffers/64 | 2,364.09 | 2,275.56 | 1.039x | 0.990-1.088x | 4 -> 3 | 0 -> 0 | 607 -> 551 |
| chart_pick/1 | 240.91 | 3.25 | 74.240x | 69.438-78.969x | 1 -> 0 | 0 -> 0 | 16 -> 0 |
| chart_pick/4 | 238.39 | 250.91 | 0.950x | 0.891-1.040x | 1 -> 1 | 0 -> 0 | 16 -> 16 |
| resolve/random_control | 1,053.88 | 1,036.71 | 1.017x | 0.900-1.154x | 4 -> 4 | 0 -> 0 | 302 -> 302 |
| init/fixed_0 | 34,576.40 | 35,220.45 | 0.982x | 0.969-1.011x | 49 -> 49 | 2 -> 2 | 5,597 -> 5,597 |
| init/fixed_4 | 52,492.05 | 51,240.87 | 1.024x | 1.015-1.046x | 152 -> 135 | 3 -> 3 | 8,433 -> 8,185 |
| init/fixed_32 | 140,896.93 | 119,424.02 | 1.180x | 1.172-1.222x | 740 -> 611 | 26 -> 26 | 27,099 -> 25,115 |
| init/random_4 | 114,086.32 | 111,179.21 | 1.026x | 0.981-1.053x | 529 -> 512 | 6 -> 6 | 17,790 -> 17,322 |

`resolve/fixed_N` compares the frozen original resolver and original pick functions with production, using N matching charts and nonempty modifiers. This measures the combined direct-resolution and singleton changes. The random control uses three candidate songs with four charts each, where no singleton shortcut applies.

`fixed_buffers/N` is an explicitly labeled intermediate control that retains the original two candidate buffers but uses the current singleton shortcuts. It pops the sole candidate directly, making this a conservative comparison of container removal. The regression with empty modifiers proves those two buffer allocations disappear (2 -> 0); benchmark modifiers remain nonempty.

`chart_pick/1` isolates key construction and hashing removed for singleton chart choice. Its large ratio applies only to that small helper. The four-chart row exercises unchanged multi-choice hashing.

`init/fixed_N` and `init/random_4` compare the frozen original initialization body with production. Both call the current resolver to isolate removal of the preliminary pass. They use one song/one chart, so time-derived seeds cannot change the selected result, and include ordinary banner-path filesystem probes under `C:/deadsync-perf-fixtures/course.crs`. Timing reflects that shared filesystem cost as well as CPU work. These fixtures are synthetic and do not establish whole-library startup speedups.

Allocation counts are identical across all four processes. Requested bytes sum allocation requests, including reallocations; they are not process RSS. The counters use a test-only scoped thread-local System allocator wrapper, leaving the production allocator unchanged. Both timed variants use that same wrapper with counting disabled; these timings are specific to this test allocator and host. The four-chart helper has 0.950x median throughput (0.891-1.040x across processes), and empty initialization has 0.982x (0.969-1.011x), with unchanged allocations. These small control paths show mixed timing results, so no speedup is claimed for them. Complete four-chart fixed resolution improves 1.361x. The data establishes allocation reductions and the targeted whole-operation gains; it does not establish that every unchanged-work control is faster.

## Behavioral validation

| Suite | Passed | Failed, unchanged from baseline | Ignored |
|---|---:|---:|---:|
| deadsync_shell-tests | 355 | 17 | 6 |
| deadsync_theme_simply_love-tests | 1265 | 1 | 4 |
| deadsync_simfile-tests | 208 | 0 | 1 |
| deadsync_profile-tests | 228 | 0 | 0 |
| semantic | 153 | 42 | 77 |
| actor | 30 | 1 | 0 |

Seven new regression tests pass, and both ignored benchmark tests pass when invoked explicitly. Fixed resolution is compared against frozen originals over 0/1/8/9/64 charts, filters, difficulty shifts, seeds, case/whitespace variants, missing groups, empty note data, course types and metadata including NaN payload bits. Random, ranked and filtered selection preserve seeded choices and repeat rules. Singleton tests check empty/ASCII/Unicode paths and index boundaries. Initialization compares 360 combinations of song-selection kinds, stage counts, explicit/missing/negative meters and available/missing songs, including full rating metadata and wheel output. Allocation assertions cover all three changes.

All existing unit and ITGmania semantic/actor outcomes and failure diagnostics match saved baseline. Only thread IDs and two audited nondeterministic reports are normalized: the order of the same three missing Sprite.Load aliases, and which of the same two missing non-local archives is reported first. Alias diagnostics were repeated three times per binary; missing-reference diagnostics ten times per binary. Reference/index/test files were checked against the starting Git blobs.

The baseline is not fully green. Of 17 shell failures, 15 pass in isolated baseline/current processes; the same two underlying failures poison the shared test lock. Archive-index validation also rejects an existing capture with obsolete `continuous-bpm` timing. This pass introduces no observed behavioral regressions and does not claim complete Song Lua compatibility.

Six native full-song ITGmania captures ran for both saved baseline and final executables, using identical archive selectors and temporary extraction directory:

| Capture and archive prefix | Comparisons per variant | Mismatches | Panics |
|---|---:|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f9`) | 363,873 | 0 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d`) | 363,873 | 0 | 0 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7`) | 304,425 | 0 | 0 |
| Warp Zone (`ded0f7ff`) | 212,220 | 0 | 0 |
| Let Me Hear That (`2a77063d`) | 205,071 | 0 | 0 |
| 272\|MODS\|[lv.02] Riddle (`7ffacb89`) | 201,471 | 0 | 0 |

**1,650,933 native comparisons per variant, zero mismatches and zero panics.** These cover the exercised captures, not unavailable archives or untested songs. Capture wall times are not performance measurements.

Baseline and final release builds completed without compiler warnings. Edited Rust is formatted; `git diff --check` passes. Frozen function bodies match starting Git blobs. Source/executable SHA-256 manifests bind the tested changes to the committed files.

## Isolation and pending branches

The initial audit covered 88 unmerged local/remote-tracking perf refs and 58 distinct heads. The final audit covered 88 refs and 58 heads. No pending branch changes the selected functions. `perf/1872-metadata-ownership-20261009` shares simfile's course.rs but only changes progress-label ownership in a different function; its change does not duplicate this work. Main at final audit was `4e59125883b7cf5331779a0a95627fea47930fd9`; this pass remains based on its pinned starting commit.

All source edits and pass scripts are in `D:/deadsync-perf-1872-borrowed-lookups-20261010`. The original checkout and its uncommitted work were not edited. The commit is local and unmerged. It excludes deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1.

Generated Cargo artifacts reuse `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` for disk capacity. Baseline executables were copied and hash-checked before source edits. Raw logs, SHA manifests and orchestration scripts remain ignored under this worktree's target directory.

## Reproduction

Build and run affected units plus native semantic/actor coverage from the branch:

```powershell
cargo test --release --locked --config profile.release.lto=false --no-fail-fast -p deadsync -p deadsync-shell -p deadsync-profile -p deadsync-simfile -p deadsync-theme-simply-love --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile -p deadsync-theme-simply-love --lib benchmark_course_loading -- --ignored --nocapture --test-threads=1
```

Repeat the benchmark in four fresh processes, with the affinity and priority above. The ignored harness prints timing and allocation rows. The unit/semantic command returns nonzero for the documented baseline failures; compare with the starting commit under identical fixture and environment conditions.

Run each full-song capture separately:

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 0f2ef3f987449e6218525fba450d1db1201c9f733d4956061d3b36529b922305.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- af2f887d212d8ab702b772cea9c953500254ce01a6b78b423276fca9cf5a8306.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- b05379b7d12bfb02149e03f166be5d496dc3e7687dcc042f0bf525cc46aa918b.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- ded0f7ff19513e30119e0d6f65b57a661f1683872fc9ffced27737db06c49993.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 2a77063dd2ab8c43bbdbb9e101be7fcf73156302e21e9771c88797218317d327.tar.zst
cargo test --release --locked --config profile.release.lto=false -p deadsync --test full_song_lua -- 7ffacb89fd95ab788758e273ba878c30380c4622406f1bb22d8c51292ca78d83.tar.zst
```

The pass ran executables directly after the equivalent `--no-run --message-format=json` build so Cargo would not replace saved baseline binaries. Both variants used the same generated extraction directory specified in `target/native-temp.json`. `target/compare-results.py` compares individual outcomes and failure diagnostics; `target/bench-summary.json` records all four process samples and allocation counts.
