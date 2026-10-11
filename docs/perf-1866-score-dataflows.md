# Score dataflow performance

Branch: `perf/1866-score-dataflows-20261008`.
Base: committed main `6b440b74ab4cbd95a6af7fd116009ebda0a5c682`.
Workspace patch version: **0.5.1865 -> 0.5.1866**, in Cargo.toml and all three workspace-version entries in Cargo.lock.

## Changes

1. **Borrow leaderboard keys for cached requests.** Gameplay scoreboxes, evaluation and music selection can return ready data or a cooling-down error without constructing an owned cache key or fetch context. Requests that are already in flight defer unused username/profile strings too. Lookup and fetch planning share the existing cache lock and perform one cache probe; refresh, cooldown, queue and invalidation rules are unchanged.
2. **Remove the one-use score-import callback container.** `ScoreImportRuntime` had no callers other than the two application entry points that constructed it immediately before calling it. Those entry points now invoke the existing generic functions with direct callbacks. This removes six heap-allocated trait objects, their reference-count operations and virtual dispatch, plus four temporary strings per import or grade request. The application entry points and generic callback APIs remain; the unused runtime type is removed.
3. **Move ArrowCloud user IDs into the user context.** Reuse the existing in-place trimming helper on owned API strings instead of cloning every retained ID. Empty IDs, Unicode whitespace, case sensitivity and duplicate elimination keep their original behavior.

The 17 existing unmerged perf branches were reviewed. These paths are distinct from the pending leaderboard-completion, local import preparation/resolution, and SRPG catalog changes. All work is in this separate worktree; nothing is merged into main.

## Paired release benchmarks

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.1 / LLVM 22.1.8. Release optimizations with LTO disabled for both versions. Original implementations are frozen from the base commit in test-only modules and audited against that commit. There are no new dependencies.

Each measurement alternates original/current order for ten pairs, discards the first pair, and reports the median of nine samples per implementation. Cache cases use 50,000 iterations per sample; import cases 4,096; user-context cases 2,048 (256 for 512 rivals). Consumed inputs are prepared outside timing; result/input destruction is timed. The forced-refresh control includes identical in-flight cleanup. No builds or compatibility harnesses from this pass ran concurrently with benchmarks.

| Case | Original ns/op | Current ns/op | Throughput | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| leaderboard ready-hit | 552.30 | 100.17 | 5.51x | 6 -> 0 | 81 -> 0 |
| leaderboard error-cooldown | 551.34 | 103.46 | 5.33x | 6 -> 0 | 81 -> 0 |
| leaderboard in-flight | 622.89 | 386.86 | 1.61x | 6 -> 3 | 81 -> 45 |
| leaderboard refresh-control | 1033.46 | 974.75 | 1.06x | 9 -> 9 | 126 -> 126 |
| user-context rivals=0 padded=false | 132.91 | 79.49 | 1.67x | 1 -> 0 | 18 -> 0 |
| user-context rivals=0 padded=true | 150.93 | 112.21 | 1.35x | 1 -> 0 | 18 -> 0 |
| user-context rivals=8 padded=false | 2108.98 | 1558.59 | 1.35x | 12 -> 3 | 918 -> 748 |
| user-context rivals=8 padded=true | 2352.93 | 1995.02 | 1.18x | 12 -> 3 | 918 -> 748 |
| user-context rivals=64 padded=false | 15449.95 | 11241.75 | 1.37x | 71 -> 6 | 7,630 -> 6,396 |
| user-context rivals=64 padded=true | 16675.88 | 13659.13 | 1.22x | 71 -> 6 | 7,630 -> 6,396 |
| user-context rivals=512 padded=false | 121382.42 | 89009.38 | 1.36x | 522 -> 9 | 60,990 -> 51,244 |
| user-context rivals=512 padded=true | 136923.05 | 105876.56 | 1.29x | 522 -> 9 | 60,990 -> 51,244 |
| score-import valid=false | 3194.12 | 2335.50 | 1.37x | 12 -> 2 | 278 -> 116 |
| score-import valid=true | 3336.13 | 2210.06 | 1.51x | 11 -> 1 | 260 -> 98 |
| score-grade | 2680.32 | 1745.17 | 1.54x | 12 -> 2 | 244 -> 82 |

Allocations and requested bytes are measured separately with a thread-local counter around the system allocator. These cases made no reallocations. Bytes describe new allocation requests during the operation, not RSS or retained memory. Timing uses the same instrumented allocator with counting disabled. Moved strings retain their existing capacity, including spare space left by trimming; no shrink allocation is added.

These are focused local-operation measurements, not whole-game frame-rate or network-latency claims. Import benchmarks use empty selections or validation errors and include the full application entry-point setup. New import regressions stay offline; they do not exercise a successful live score-service transaction. Unchanged generic score-import tests cover result dispatch, progress, and request errors.

## Regression and ITGmania compatibility results

| Suite | Passed | Failed | Ignored | Added cases |
|---|---:|---:|---:|---:|
| deadsync_score-tests | 239 | 1 | 1 | 3 |
| deadsync_online-tests | 359 | 0 | 3 | 7 |
| semantic | 105 | 68 | 74 | 0 |
| actor | 30 | 1 | 0 | 0 |

All original test outcomes and failure diagnostics are unchanged; diagnostic comparison normalizes only panic thread IDs. Seven new regression/allocation tests pass. Three ignored paired benchmark tests were explicitly run and passed. The other ignored online test downloads a live pack.

The new cache test compares 216 combinations of cached state, cooldown, in-flight request size, refresh flag and requested row count against the original, including resulting cache/queue state and fetch payloads. Other new cases cover empty/inactive requests, import credential validation and empty-selection progress, grade validation, and user-ID trimming/deduplication.

The score crate retains its baseline `lua_submit_allowlist_requires_known_hash` failure for `f95bc209c6f2cbfe`. Native semantic parity retains 68 baseline failures; actor conformance retains `geometry::lua_align_matches_native`. The 278 native test outcomes are identical before and after.

Two Epidermis archives and one Sharkmode archive were run through the full-song ITGmania harness both before and after. All three stop at the same existing dependency-hash guard for `common/common/Fallback Receptor.lua`, before comparisons:

- Actual: `52ffa6df0701b426d6af887f9c91cd61895d8b32f349f0cd7ddfa48bae41e74d`
- Expected: `96623726284f5ae0c5b12e05e0ae841eae40e100341a20042db68c9b80b52c74`

No full-song parity pass is claimed. Song Lua implementation, fixtures, expected results and dependency guards were not changed.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-score -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-score -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

Full-song selectors (one per `cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive>`):

- `b40903481944c3395db4c848c671e27a383d2cbea51c74b029b7dea9dbccf16a.tar.zst`
- `a867a12d25c984a49c2dd29fa06d47f88afe01841ee9d410dbf1332800797336.tar.zst`
- `f7a2d5e8aa6a48c8709ab3e189b1e54cf282029b6109fbc13ca8794bd3125249.tar.zst`

Raw run logs, benchmark logs, build-artifact manifests and `compatibility-comparison.json` are retained under this worktree's ignored `target/` directory. The baseline/current builds used two Cargo jobs and the existing shared build cache. Explicit changed-file rustfmt checks, `git diff --check`, the exact-base/version/original-source audit and the commit file allowlist audit pass.

The commit excludes `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and `optimize.ps1`.
