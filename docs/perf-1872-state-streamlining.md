# Gameplay state preparation performance pass

Branch: `perf/1872-state-streamlining-20261009`. Starting main: `5df9ca170778d7b6a910f0d2c038b551255d1fe3`.
DeadSync patch version: **0.5.1871 -> 0.5.1872**, in Cargo.toml and Cargo.lock.

## Changes

1. Pump hold event setup returns immediately when the existing sizing pass finds no eligible events. This removes the tap-row allocation and two subsequent note scans for those charts. Nonempty plans still use the original emission and sorting code.
2. Crossover cue construction sizes its allocation only when it emits the first cue. Empty outputs no longer pay for a separate transition-count scan, including widely spaced annotations. The separate emission helper and argument forwarding are removed; nonempty outputs keep the original reservation size.
3. Crossover lane selection intersects the column mask with the inner/outer lane mask and selects its lowest set bit. This removes the per-lane loop and preserves left/right pad ordering.

Production changes are limited to `crates/deadsync-gameplay/src/holds.rs` and `cues.rs`. No Song Lua source changed. The pending-branch audit covered 61 refs / 35 distinct heads; neither production file overlaps their pending diffs. `rust-performance.md` guided removal of redundant work and measurement; it and the other excluded files are absent from the commit.

## Regression and compatibility results

- **1112 affected/dependency unit tests passed**, 2 ignored test entries. Both new paired benchmark tests were run explicitly in eight independent processes per test suite.
- All original unit-test outcomes are preserved. New tests compare Pump plans, event order and score-row counts across player counts, fake holds, invalid tail times, clipped/reversed ranges, truncated caches and unjudgable notes; they check zero scratch allocations for empty plans.
- Crossover differential tests compare complete output and timing-callback arguments/order across inactive, active, bracket, mixed, spaced, empty-mask and reversed-beat inputs. All 256 masks x 2 inner/outer choices also match an independent ascending-lane oracle and the original loop.
- Fresh baseline and current runs of the Song Lua harness compared the same six recorded native ITGmania archives. All **1,650,933 comparisons** passed on both runs. This replays existing native captures; it does not claim new live captures or dedicated native Pump/crossover coverage.
- Focused semantic parity: **143 passed / 38 failed / 77 ignored**. Actor conformance: **30 passed / 1 failed**. The existing failures and their diagnostics are unchanged. Only nondeterministic actor-name diagnostic ordering and panic thread IDs are normalized by the comparator.

| Native capture | Comparisons | Before / after |
| --- | ---: | --- |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f98744`) | 363873 | pass / pass |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d212d`) | 363873 | pass / pass |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7d12b`) | 304425 | pass / pass |
| Warp Zone (`b38698ececd6`) | 212220 | pass / pass |
| Let Me Hear That (`0229b74d092e`) | 205071 | pass / pass |
| 272\|MODS\|[lv.02] Riddle (`920eb361ce68`) | 201471 | pass / pass |

## Paired benchmarks

Release `opt-level=3`, LTO disabled for both variants; rustc 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc, Intel Xeon E5-2696 v4 (22 cores / 44 logical CPUs). Each benchmark child process is pinned to logical CPU 2 with AboveNormal process priority; no other process priority or affinity is changed. Inputs are synthetic and prepared outside timing; two-player Pump cases reuse the same note range for each player. Output construction and destruction are timed. Pump operations are complete event-plan builds; crossover operations are complete cue builds; lane operations are **batches of 256 selections**.

Frozen starting implementations and current code run in the same binary with equivalent inlining annotations. Each process alternates execution order over 10 batches, discards warmup and reports medians of nine batches. The table shows medians over eight process runs and the range of paired per-process throughput ratios. Values above 1 are faster. These are routine-level throughput measurements, not whole-game frame-rate or hardware-cycle measurements.

An earlier timing attempt with large scheduling outliers is retained in `target/noisy-bench`. The final eight runs below were repeated after this pass's builds and native replays completed, using the stated process priority.

The test-only System allocator wrapper counts thread-local allocation churn outside timed batches. Its disabled TLS check remains present for both timed variants. No production allocator or dependencies changed.

| Case | Original ns/op | Current ns/op | Throughput | Eight-run range |
| --- | ---: | ---: | ---: | ---: |
| pump-taps-64-p1 | 571.14 | 98.95 | 5.772 | 5.132-6.368 |
| pump-taps-64-p2 | 1109.49 | 174.63 | 6.353 | 5.550-7.272 |
| pump-fake-holds-64-p1 | 265.21 | 83.56 | 3.174 | 2.727-3.571 |
| pump-fake-holds-64-p2 | 529.91 | 139.92 | 3.787 | 3.564-4.045 |
| pump-holds-control-64-p1 | 1909.08 | 2047.05 | 0.933 | 0.882-1.145 |
| pump-holds-control-64-p2 | 4747.71 | 4931.43 | 0.963 | 0.888-1.156 |
| pump-taps-4096-p1 | 55214.58 | 13694.35 | 4.032 | 3.733-4.517 |
| pump-taps-4096-p2 | 108412.50 | 27211.25 | 3.984 | 3.606-4.249 |
| pump-fake-holds-4096-p1 | 34943.12 | 11392.50 | 3.067 | 2.802-3.138 |
| pump-fake-holds-4096-p2 | 68062.50 | 22732.25 | 2.994 | 2.846-3.263 |
| pump-holds-control-4096-p1 | 125380.00 | 126440.00 | 0.992 | 0.954-1.108 |
| pump-holds-control-4096-p2 | 418367.50 | 419420.00 | 0.997 | 0.922-1.026 |
| pump-taps-32768-p1 | 434482.50 | 110270.00 | 3.940 | 3.758-3.970 |
| pump-taps-32768-p2 | 858677.50 | 218227.50 | 3.935 | 3.735-4.226 |
| pump-fake-holds-32768-p1 | 271015.00 | 89687.50 | 3.022 | 2.871-3.386 |
| pump-fake-holds-32768-p2 | 543600.00 | 176937.50 | 3.072 | 2.825-3.222 |
| crossover-inactive-0 | 7.75 | 7.96 | 0.972 | 0.918-1.189 |
| crossover-active-0 | 8.13 | 8.07 | 1.008 | 0.932-1.069 |
| crossover-bracket-only-0 | 8.91 | 8.06 | 1.105 | 0.892-1.264 |
| crossover-mixed-control-0 | 7.90 | 7.95 | 0.994 | 0.974-1.053 |
| crossover-spaced-control-0 | 7.74 | 7.74 | 1.000 | 0.840-1.361 |
| crossover-inactive-64 | 91.55 | 55.44 | 1.651 | 1.506-1.785 |
| crossover-active-64 | 184.62 | 118.64 | 1.556 | 1.416-1.726 |
| crossover-bracket-only-64 | 93.77 | 55.83 | 1.680 | 1.494-1.984 |
| crossover-mixed-control-64 | 393.55 | 405.08 | 0.972 | 0.865-1.186 |
| crossover-spaced-control-64 | 167.25 | 146.89 | 1.139 | 1.078-1.354 |
| crossover-inactive-4096 | 4336.98 | 2513.03 | 1.726 | 1.510-1.827 |
| crossover-active-4096 | 10530.28 | 6551.29 | 1.607 | 1.389-1.806 |
| crossover-bracket-only-4096 | 4303.15 | 2440.22 | 1.763 | 1.536-1.865 |
| crossover-mixed-control-4096 | 20472.50 | 20200.00 | 1.013 | 0.928-1.100 |
| crossover-spaced-control-4096 | 9632.50 | 8661.15 | 1.112 | 1.004-1.226 |
| crossover-inactive-32768 | 34848.33 | 19846.25 | 1.756 | 1.448-1.784 |
| crossover-active-32768 | 82618.33 | 51016.67 | 1.619 | 1.529-1.773 |
| crossover-bracket-only-32768 | 33868.06 | 20568.75 | 1.647 | 1.561-1.771 |
| crossover-mixed-control-32768 | 187575.00 | 190376.66 | 0.985 | 0.950-1.047 |
| crossover-spaced-control-32768 | 80193.33 | 66910.83 | 1.199 | 1.086-1.334 |
| lanes-empty-outerfalse | 256.99 | 251.14 | 1.023 | 0.867-1.093 |
| lanes-empty-outertrue | 257.74 | 249.23 | 1.034 | 0.863-1.093 |
| lanes-single-outerfalse | 537.38 | 327.88 | 1.639 | 1.527-1.896 |
| lanes-single-outertrue | 527.30 | 332.25 | 1.587 | 1.485-1.770 |
| lanes-all-masks-outerfalse | 715.15 | 331.55 | 2.157 | 1.967-2.251 |
| lanes-all-masks-outertrue | 674.43 | 352.12 | 1.915 | 1.701-2.221 |
| lanes-dense-outerfalse | 692.24 | 335.61 | 2.063 | 1.908-2.488 |
| lanes-dense-outertrue | 476.57 | 334.22 | 1.426 | 1.294-1.558 |
| lanes-inner-only-outerfalse | 504.68 | 337.71 | 1.494 | 1.305-1.694 |
| lanes-inner-only-outertrue | 1634.47 | 292.01 | 5.597 | 5.014-6.324 |
| lanes-outer-only-outerfalse | 1593.18 | 292.69 | 5.443 | 5.112-6.181 |
| lanes-outer-only-outertrue | 487.51 | 322.44 | 1.512 | 1.432-1.744 |

Cases with a lower median: `pump-holds-control-64-p1`: 0.933x (0.882-1.145); `pump-holds-control-64-p2`: 0.963x (0.888-1.156); `pump-holds-control-4096-p1`: 0.992x (0.954-1.108); `pump-holds-control-4096-p2`: 0.997x (0.922-1.026); `crossover-inactive-0`: 0.972x (0.918-1.189); `crossover-mixed-control-0`: 0.994x (0.974-1.053); `crossover-mixed-control-64`: 0.972x (0.865-1.186); `crossover-mixed-control-32768`: 0.985x (0.950-1.047)

Some control cases have lower pooled throughput medians, down to 0.933x (6.7% lower). Each of those ranges spans baseline performance across the eight runs. These results demonstrate the targeted gains, while leaving uncertainty about small control-case costs. No behavioral regressions were detected.

## Allocation and memory measurements

Requested bytes sum allocation/reallocation requests; peak bytes are live requested bytes within the operation. All eight runs produced identical allocation counts. Fixture allocations are excluded.

| Case | Original alloc / realloc | Current alloc / realloc | Original requested bytes | Current requested bytes | Original peak bytes | Current peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| pump-taps-64-p1 | 1 / 0 | 0 / 0 | 256 | 0 | 256 | 0 |
| pump-taps-64-p2 | 2 / 0 | 0 / 0 | 512 | 0 | 256 | 0 |
| pump-fake-holds-64-p1 | 1 / 0 | 0 / 0 | 256 | 0 | 256 | 0 |
| pump-fake-holds-64-p2 | 2 / 0 | 0 / 0 | 512 | 0 | 256 | 0 |
| pump-holds-control-64-p1 | 2 / 0 | 2 / 0 | 832 | 832 | 832 | 832 |
| pump-holds-control-64-p2 | 3 / 0 | 3 / 0 | 1664 | 1664 | 1408 | 1408 |
| pump-taps-4096-p1 | 1 / 0 | 0 / 0 | 16384 | 0 | 16384 | 0 |
| pump-taps-4096-p2 | 2 / 0 | 0 / 0 | 32768 | 0 | 16384 | 0 |
| pump-fake-holds-4096-p1 | 1 / 0 | 0 / 0 | 16384 | 0 | 16384 | 0 |
| pump-fake-holds-4096-p2 | 2 / 0 | 0 / 0 | 32768 | 0 | 16384 | 0 |
| pump-holds-control-4096-p1 | 2 / 0 | 2 / 0 | 51088 | 51088 | 51088 | 51088 |
| pump-holds-control-4096-p2 | 3 / 0 | 3 / 0 | 102176 | 102176 | 85792 | 85792 |
| pump-taps-32768-p1 | 1 / 0 | 0 / 0 | 131072 | 0 | 131072 | 0 |
| pump-taps-32768-p2 | 2 / 0 | 0 / 0 | 262144 | 0 | 131072 | 0 |
| pump-fake-holds-32768-p1 | 1 / 0 | 0 / 0 | 131072 | 0 | 131072 | 0 |
| pump-fake-holds-32768-p2 | 2 / 0 | 0 / 0 | 262144 | 0 | 131072 | 0 |

## Reproduction and evidence

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-gameplay -p deadsync-rules -p deadsync-simfile --lib
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-gameplay -p deadsync-rules -p deadsync-simfile --lib benchmark_state -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync -p deadsync-simfile --lib --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance --test full_song_lua --no-run
```

The focused compatibility targets have known failures, so compare baseline/current logs rather than expecting those two targets to exit successfully. Run the custom full-song executable with each archive selector recorded in `target/current-fullsong-selectors.json`.

Ignored local evidence: `target/pending-audit.json`, `baseline-*.log`, `current-*.log`, `final-1.log` through `final-4.log` and `confirmation-1.log` through `confirmation-4.log`, `benchmark-results.json`, `compatibility-comparison.json` and `source-hashes.json`. `run-local.py`, `run-compat.py`, `run-bench.py`, `analyze-bench.py`, `compare-results.py` and `verify-change.py` reproduce the orchestration in this worktree. Builds reuse the ignored Cargo cache under the earlier data-churn worktree; source changes and the commit stay in this pass's worktree.

The original checkout was not edited. This branch is committed without merging or pushing.
