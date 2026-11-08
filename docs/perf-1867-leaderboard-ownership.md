# Leaderboard and unlock ownership performance

Branch: `perf/1867-leaderboard-ownership-20261008`.
Base: `99a4e81feb74ff3c5dffae8e102c9d71c8c40a8e` (committed main at worktree creation).
Version: **0.5.1866 -> 0.5.1867**, in Cargo.toml and all three workspace-version Cargo.lock packages.

## Changes

1. Leaderboard completion callbacks borrow profile IDs, API keys, chart hashes, and usernames. The worker no longer clones strings for callbacks that immediately borrowed them. Completion remains ordered ITL, SRPG, imported score, then queued refresh. A private completion function separates these effects from thread creation for offline regression testing; required stored ownership remains in the cache APIs.
2. ArrowCloud pagination borrows target IDs into `HashSet<&str>` from its existing user context. This removes a clone per identity and reduces table entry size. Trimming, case-sensitive identity matching, deduplication, per-page removal, and early completion remain unchanged.
3. Submission unlock plans borrow folder slices and trimmed URLs from the API response. Generated labels remain owned. The download queue still copies what its worker retains; borrowed planning data never crosses that lifetime boundary.

These scopes were checked against the 18 existing unmerged perf branches. They do not repeat the pending request-cache, user-context decoding, import, or download-catalog changes. Song Lua production code and native fixtures are unchanged.

## Benchmarks

Release, `profile.release.lto=false`, rustc 1.98.1 / LLVM 22.1.8, x86_64-pc-windows-msvc, Intel Xeon E5-2696 v4 at 2.20 GHz. Ten alternating original/current pairs, first pair discarded, median of the remaining nine. Consumed completion inputs are prepared outside timing; output destruction is included. The thread-local System allocator counter is disabled during timing. Allocation measurements are separate and count newly requested bytes, not RSS or retained memory.

Frozen originals are checked against the base commit. The completion reference returns the original queued request instead of spawning a thread, allowing identical offline callback and request comparisons. Timing covers local completion dispatch, target-set construction/pagination, and unlock planning; it does not measure network requests, disk writes, downloads, or frame rate. No agent-owned build or compatibility process ran during the final timing run.

`targets-N` and `pagination-N` contain N rivals plus one self ID. Pagination removes half the rivals from a prepared page. Unlock fixtures contain the indicated quest/folder counts for both SRPG and ITL; every third URL is blank. Empty completion and zero-quest cases are controls.

| Case | Original ns/op | Current ns/op | Throughput | Allocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| targets-0 | 197.98 | 124.02 | 1.60x | 2 -> 1 | 136 -> 84 |
| pagination-0 | 191.66 | 124.38 | 1.54x | 2 -> 1 | 136 -> 84 |
| targets-8 | 1126.64 | 413.32 | 2.73x | 10 -> 1 | 588 -> 288 |
| pagination-8 | 1473.80 | 776.20 | 1.90x | 10 -> 1 | 588 -> 288 |
| targets-64 | 8362.08 | 2592.04 | 3.23x | 66 -> 1 | 4452 -> 2192 |
| pagination-64 | 11030.06 | 5124.78 | 2.15x | 66 -> 1 | 4452 -> 2192 |
| targets-512 | 64827.96 | 20547.28 | 3.16x | 514 -> 1 | 35364 -> 17424 |
| pagination-512 | 89894.66 | 43891.20 | 2.05x | 514 -> 1 | 35364 -> 17424 |
| unlocks-0x0-downloads-false | 24.14 | 11.23 | 2.15x | 0 -> 0 | 0 -> 0 |
| unlocks-0x0-downloads-true | 27.58 | 15.40 | 1.79x | 0 -> 0 | 0 -> 0 |
| unlocks-2x2-downloads-false | 519.80 | 80.10 | 6.49x | 7 -> 1 | 184 -> 32 |
| unlocks-2x2-downloads-true | 1348.94 | 790.80 | 1.71x | 14 -> 6 | 512 -> 274 |
| unlocks-32x8-downloads-false | 22052.94 | 89.34 | 246.84x | 289 -> 1 | 9648 -> 512 |
| unlocks-32x8-downloads-true | 36396.92 | 10704.92 | 3.40x | 416 -> 86 | 16592 -> 5622 |
| completion-all-queued | 1042.01 | 399.82 | 2.61x | 9 -> 0 | 159 -> 0 |
| completion-all | 918.93 | 291.76 | 3.15x | 9 -> 0 | 159 -> 0 |
| completion-itl | 431.07 | 221.38 | 1.95x | 3 -> 0 | 59 -> 0 |
| completion-empty | 176.74 | 174.37 | 1.01x | 0 -> 0 | 0 -> 0 |

All measured cases have zero reallocations. The final empty-completion control is level with the original. An intermediate closure-based queued-request rewrite was removed after that control slowed down; the retained conditional matches the original control flow.

## Behavioral and compatibility validation

**599 passing local crate tests**, including **8 new regression tests**. Three ignored benchmark tests were also explicitly executed. New tests cover 512 completion combinations, callback order and payloads, queued refresh contents, borrowed ID identity and deduplication, Unicode whitespace and case-sensitive pagination, 192 unlock-gating/label combinations, allocation counts, and real offline queue deduplication after the source response is dropped.

| Suite | Baseline cases | Current passed / failed / ignored | Unchanged failure diagnostics |
| --- | ---: | ---: | ---: |
| deadsync_online-tests | 355 | 362 / 0 / 4 | 0 |
| deadsync_score-tests | 238 | 237 / 1 / 0 | 1 |
| semantic | 247 | 105 / 68 / 74 | 68 |
| actor | 31 | 30 / 1 / 0 | 1 |

Every existing local and native test outcome matches its baseline. Failure comparisons normalize only panic thread IDs. The score allowlist assertion and 69 native parity failures already occur at the starting commit; this pass does not repair them. Native builds encountered system-memory exhaustion in unchanged simfile and shell code. Compilation resumed from cached dependencies with one Cargo job after memory recovered. Completed unit tests and timing measurements preceded those build failures.

The full-song harness was run before and after against:

- 319|TECH SOUP|[lv.P.Clark] Epidermis: `1c0225a7d2fc61cdd507b7884bef6a80a9a1fc5fecf5c313e2c5c18fa0466857.tar.zst`
- 319|TECH SOUP|[lv.P.Clark] Epidermis: `a84ff944882c91f701e6b0ea4064c25949f0eee87705e843ea5b209b89eaa83a.tar.zst`
- 280|MODS|[MASTER] Sharkmode: `c0a1741ca9d19cd8b8baf1292402b010423732e0eebad4d371fadb8d6430e85a.tar.zst`

All three runs have the same existing pre-comparison guard failure:
`noteskin dependency changed: common/common/Fallback Receptor.lua`. Actual `"52ffa6df0701b426d6af887f9c91cd61895d8b32f349f0cd7ddfa48bae41e74d"`; expected `"96623726284f5ae0c5b12e05e0ae841eae40e100341a20042db68c9b80b52c74"`.
They perform no song comparisons, so full-song parity is **blocked by the unchanged fixture guard**, not certified by this pass. No goldens or fixture hashes were changed. No live online service transaction was performed.

## Reproduce

```powershell
$env:CARGO_BUILD_JOBS = '1'
cargo test --release --locked --config profile.release.lto=false -p deadsync-online -p deadsync-score --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
```

Run `cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- <archive>` for each selector above. Raw build, unit-test, compatibility, benchmark, and comparison logs remain under this worktree's ignored `target/` directory.
