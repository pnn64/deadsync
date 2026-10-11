# Course startup, endless updates and graph ownership

Pass: 2026-10-10. Branch: `perf/1872-direct-updates-20261010`.
Base: `6463950f77501f08fdc7752974c1c606ec74d3b9`, the latest committed main at worktree creation. Version: **0.5.1871 → 0.5.1872**, updated exactly once in Cargo.toml and Cargo.lock.

## Changes

1. Course startup consumes its owned stage plans. Song references and modifier strings move into runtime stages, and totals accumulate during that traversal. This removes per-stage modifier allocations, unnecessary reference-count traffic, and a second traversal.
2. Endless-course updates append valid stages directly to the destination while updating totals. The temporary stage vector and its growth/copying are removed. Rejected courses, mismatched paths and selections with no valid stages still return false and leave the course unchanged.
3. Course graphs share one immutable chart snapshot between players when their steps indices match. Different charts remain independent; missing charts remain absent. Shared snapshots retain ordinary Arc copy-on-write behavior. This removes duplicate chart metadata, strings and graph arrays.

All production changes are in `crates/deadsync-shell/src/course.rs`, outside Song Lua. No production dependency, cache or unsafe code was added.

## Paired release measurements

Each result is the median of four fresh-process medians. The same executable contains production functions and frozen starting implementations. Each process runs three warmups, calibration toward 25 ms (capped at 4,096 operations), and nine timed batches in alternating order. Input preparation happens before timing and allocation counting; results are consumed and dropped inside the measured operation. The timed input-batch vector traversal/destruction is common to both implementations.

Windows x86-64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz, 22 cores/44 logical processors; rustc 1.98.1 / LLVM 22.1.8. Both variants use release optimization with `profile.release.lto=false`. Benchmark processes are pinned to logical CPU 2 at AboveNormal priority. These are local function-throughput measurements, not frame-rate or CPU-cycle claims.

| Operation | Original ns/op | Current ns/op | Throughput | Four-process range | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| startup/4 | 1,895.22 | 1,542.08 | 1.229× | 1.180–1.282× | 5 → 1 | 0 → 0 | 440 → 288 |
| startup/32 | 12,510.52 | 9,622.60 | 1.300× | 1.256–1.337× | 33 → 1 | 0 → 0 | 3,542 → 2,304 |
| startup/all_invalid | 3,152.72 | 3,184.86 | 0.990× | 0.978–1.032× | 1 → 1 | 0 → 0 | 1,152 → 1,152 |
| endless/4 | 2,589.53 | 2,223.62 | 1.165× | 1.149–1.217× | 5 → 0 | 1 → 1 | 1,016 → 576 |
| endless/4_owned_control | 2,312.36 | 2,223.77 | 1.040× | 1.022–1.081× | 1 → 0 | 1 → 1 | 864 → 576 |
| endless/32 | 15,587.46 | 11,871.43 | 1.313× | 1.228–1.357× | 33 → 0 | 4 → 1 | 10,166 → 4,608 |
| endless/32_owned_control | 12,909.77 | 12,341.47 | 1.046× | 0.958–1.084× | 1 → 0 | 4 → 1 | 8,928 → 4,608 |
| graphs/same_4 | 14,027.86 | 7,316.45 | 1.917× | 1.855–1.981× | 138 → 70 | 0 → 0 | 39,344 → 19,736 |
| graphs/same_32 | 126,787.73 | 60,414.65 | 2.099× | 2.012–2.122× | 1,090 → 546 | 0 → 0 | 314,752 → 157,888 |
| graphs/different_32 | 128,096.70 | 125,778.91 | 1.018× | 0.987–1.084× | 1,090 → 1,090 | 0 → 0 | 314,752 → 314,752 |
| graphs/mixed_32 | 127,555.98 | 92,288.72 | 1.382× | 1.354–1.393× | 1,090 → 818 | 0 → 0 | 314,752 → 236,320 |
| graphs/missing_32 | 449.76 | 407.04 | 1.105× | 1.079–1.192× | 2 → 2 | 0 → 0 | 1,024 → 1,024 |

Startup and endless cases use synthetic four- and 32-stage plans with nonempty modifiers. Endless cases append one cycle to an existing course of the same length; the destination must grow. Owned input cloning is excluded. Graph fixtures contain 256 measure entries per chart; same, different, mixed and missing player selections exercise both sharing and fallback paths. Allocation counts are identical across all four processes.

The `owned_control` rows use an explicitly labeled intermediate reference: stage plans already move, but the original temporary-vector append algorithm remains. Those rows isolate the additional benefit of direct appending. All other rows compare against frozen implementations from the starting commit.

Invalid startup and different-chart graph cases are controls with unchanged allocation counts; small timing differences are interpreted with the recorded ranges. Requested bytes count allocation requests, including reallocations; they are not process RSS. Counters use a test-only scoped thread-local System allocator wrapper; the production allocator is unchanged.

## Behavioral validation

| Suite | Passed | Failed, unchanged from baseline | Ignored |
|---|---:|---:|---:|
| deadsync_shell-tests | 360 | 17 | 7 |
| deadsync_theme_simply_love-tests | 1263 | 1 | 3 |
| deadsync_profile-tests | 228 | 0 | 0 |
| deadsync_simfile-tests | 203 | 0 | 0 |
| semantic | 151 | 42 | 77 |
| actor | 30 | 1 | 0 |

Five new regression tests pass. They compare startup metadata, filtering, totals, modifier-buffer ownership, rejected/empty/invalid endless selections, stage order, saturated totals, mixed/missing/edit charts, and independent copy-on-write behavior. Startup has exactly one output-vector allocation; an endless append with sufficient destination capacity performs no allocations or reallocations.

All existing unit and ITGmania semantic/actor outcomes and failure diagnostics match the saved baseline. Only thread IDs and two audited nondeterministic reports are normalized: the order of the same three missing Sprite.Load aliases, and which of the same two missing non-local archives is reported first. Alias diagnostics were repeated three times per binary; missing-reference diagnostics ten times per binary. The reference/index/test files were checked against the starting Git blobs.

The baseline is not completely green. Of the 17 shell failures, 15 pass when rerun in isolated baseline/current processes; the same two underlying failures poison the shared test lock. Archive-index validation also rejects an existing capture with obsolete `continuous-bpm` timing. This pass introduces no observed behavioral regressions and does not claim full Song Lua compatibility.

Six native full-song ITGmania captures were run for both saved baseline and final executables, with identical archive selectors and temporary extraction directory:

| Capture | Comparisons per variant | Mismatches | Panics |
|---|---:|---:|---:|
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`0f2ef3f9`) | 363,873 | 0 | 0 |
| 319\|TECH SOUP\|[lv.P.Clark] Epidermis (`af2f887d`) | 363,873 | 0 | 0 |
| 280\|MODS\|[MASTER] Sharkmode (`b05379b7`) | 304,425 | 0 | 0 |
| Warp Zone (`ded0f7ff`) | 212,220 | 0 | 0 |
| Let Me Hear That (`2a77063d`) | 205,071 | 0 | 0 |
| 272\|MODS\|[lv.02] Riddle (`7ffacb89`) | 201,471 | 0 | 0 |

**1,650,933 native comparisons per variant, zero mismatches and zero panics.** These results cover the exercised captures; they do not cover unavailable archives or untested songs. Capture wall times are not used as performance measurements.

Baseline and final release builds completed without compiler warnings. Edited Rust is formatted, with unrelated formatting in the existing gameplay test file preserved. Frozen function bodies match the starting Git blobs. Source and executable SHA-256 manifests bind the tested changes to the committed files. `git diff --check` passes.

## Isolation and pending branches

The initial audit covered 87 unmerged local/remote-tracking perf refs and 57 distinct heads. The final audit covered 87 refs and 57 heads. No pending optimization changes course.rs. Main at final audit was `4327e89ddf816c47d5291e3f4dff79a09efed4b0`; this pass remains based on its pinned starting commit.

All source edits and pass scripts are in `D:/deadsync-perf-1872-direct-updates-20261010`. The original checkout and its uncommitted work were not edited. The commit is local and unmerged. It excludes deadsync-song.json.gz, rust-performance.md, optimize.sh and optimize.ps1.

Generated Cargo artifacts reuse `C:/GitHub/deadsync-perf-1872-data-churn-20261009/target/build` for disk capacity. Baseline executables were copied and hash-checked before source edits. Raw logs, SHA manifests and orchestration scripts remain ignored under this worktree’s target directory.

## Reproduction

Run from the separate worktree. Known baseline failures remain expected; `--no-fail-fast` runs all requested suites:

```powershell
cargo test --release --locked --config profile.release.lto=false --no-fail-fast -p deadsync-shell -p deadsync-theme-simply-love -p deadsync-profile -p deadsync-simfile --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --no-fail-fast -p deadsync --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-shell --lib benchmark_course_updates -- --ignored --nocapture --test-threads=1
```

Repeat the benchmark command in four fresh processes. The retained `target/run-bench.py` applies the recorded affinity/priority settings to the compiled executable. Frozen originals are compiled into the test binary, so no second checkout is needed for paired measurements. For the native captures:

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
