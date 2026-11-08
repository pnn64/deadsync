# Performance pass: GIF buffers and SRPG ownership

Branch: `perf/1862-buffer-streamlining-20261008`

Starting main: `2b71d944fcc02b31b1b94c1619da8c0c32035ff6`

Version: **0.5.1861 → 0.5.1862**, exactly one patch increment in `Cargo.toml` and all three workspace-version entries in `Cargo.lock`. No third-party dependency changes.

The pass uses a separate worktree. Main advanced independently during execution; this branch keeps the starting main commit as its base. All 15 unmerged `perf/` branches were inspected; none changes either selected production file. Song Lua and simfile code are unchanged. The branch is not merged, and the original checkout was not edited. None of `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, or `optimize.ps1` is included.

## Changes

1. **Stream SMX GIF frames into LED samples.** Remove `DecodedGif` and its retained collection of RGBA images. Read markers, duration and LED samples before releasing each decoded frame. Keep the original frame-error filtering, marker precedence, delay snapping and output pixels. Shrink the final sample vectors so longer-lived animations do not retain spare capacity.
2. **Move SRPG catalog strings out of parsed JSON.** Consume display fields through mutable rows after caching sort keys. Reuse clean strings, preserve HTML cleanup and censorship, and avoid allocating a replacement effect string when it contains no separator. Sorting and case-insensitive response-key precedence are unchanged.
3. **Use copy-on-write purchase snapshots.** Mutate a uniquely owned snapshot in place. If the UI still owns the old snapshot, copy once; share the purchasing snapshot with the worker instead of making a second deep copy. Preserve generation publication, ready/session checks, stale-result handling, ordering and error rollback. Rollback remains copy-on-write so existing readers are unchanged.

## Measurements

Windows x86-64 MSVC; Intel Xeon E5-2696 v4 (22 cores, 44 logical processors); rustc 1.98.1, LLVM 22.1.8. All measurements use optimized release builds with `profile.release.lto=false`. The initial six-job native baseline build exhausted memory. It succeeded with one job; changed builds used two jobs. Build parallelism is not a runtime benchmark setting. Fat-LTO performance was not measured.

Each benchmark links the original and current implementations into the same executable. The original functions were extracted from the starting commit and checked after formatting; the purchase reference only adapts the existing lock-block's returns for direct invocation. Ten alternating pairs are run, the warmup pair is discarded, and the median of nine samples is reported. Inputs consumed by purchase setup are prepared outside the timer. Output destruction is timed; no network requests or file I/O are timed. Final benchmarks run after this pass's builds and compatibility tests finish. GIF samples use 256 iterations per variant; catalog and purchase samples use 32. Earlier 16-iteration short pad-GIF samples varied from 0.93x to 0.99x, so their CPU result was rechecked with longer samples. Other user workloads were not stopped; elapsed-time ratios are indicative, while the allocation and heap counts were stable across runs.

The test allocator forwards to `System` and counts thread-local allocations/reallocations. Peak bytes are the maximum tracked live allocation payload, excluding allocator bookkeeping and transient storage inside `realloc`; they are not RSS. Requested-byte totals count each reallocation's new size and do not mean retained memory. Timings include the common disabled allocation-counter wrapper.

| Workload | Original µs/op | Current µs/op | Throughput ratio |
| --- | ---: | ---: | ---: |
| GIF (23, 24) frames=32 | 375.42 | 372.52 | 1.01× |
| GIF (23, 24) frames=257 | 3097.99 | 3099.01 | 1.00× |
| GIF (7, 8) frames=257 | 531.82 | 521.79 | 1.02× |
| Catalog rows=1000 html=false | 2908.52 | 2559.67 | 1.14× |
| Catalog rows=1000 html=true | 2357.95 | 2312.05 | 1.02× |
| Purchase rows=32 shared=false | 25.45 | 3.76 | 6.76× |
| Purchase rows=32 shared=true | 25.13 | 13.57 | 1.85× |
| Purchase rows=1000 shared=false | 817.68 | 113.62 | 7.20× |
| Purchase rows=1000 shared=true | 843.17 | 465.23 | 1.81× |

The GIF result is primarily a memory improvement. Short pad-GIF timings are approximately unchanged; do not infer an overall game-frame-rate gain from these loading benchmarks. Purchase timings cover local setup and disposal, not server response time.

| Allocation/memory measurement | Original | Current |
| --- | ---: | ---: |
| Pad GIF, 32 frames: peak live bytes | 113,374 | 65,246 (42.5% less) |
| Pad GIF, 257 frames: peak live bytes | 763,459 | 391,166 (48.8% less) |
| Panel GIF, 257 frames: peak live bytes | 118,350 | 78,926 (33.3% less) |
| Plain catalog, 1,000 rows: allocations | 10,007 | 6,007 (4,000 fewer) |
| HTML catalog, 1,000 rows: allocations | 10,007 | 8,007 (2,000 fewer) |
| Shared purchase, 1,000 items: allocations | 8,009 | 4,006 (50.0% less) |
| Shared purchase, 1,000 items: requested bytes | 430,009 | 215,045 (50.0% less) |
| Unique purchase, 1,000 items: allocations | 8,009 | 2 |
| Unique purchase, 1,000 items: requested bytes | 430,009 | 38 |

**Tradeoff:** streaming does not know the final GIF frame count in advance, so sample vectors grow during decoding. The 257-frame pad case changes from 14 to 80 reallocations and from 1,023,729 to 1,672,457 total requested bytes, while reducing peak live memory by 372,293 bytes. For 32 pad frames, reallocations are 6 → 30 and requested bytes 160,014 → 176,514. For 257 panel frames, they are 14 → 16 and 209,129 → 245,857. Final retained pad-animation bytes fall from 175,523 to 174,503 in the 257-frame case. This is a peak-memory optimization, not an allocation-churn win. Catalog reallocations remain unchanged (2,008 plain / 3,008 HTML); purchase setup performs no reallocations in either implementation.

## Regression checks

| Suite | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| deadsync_smx-tests | 104 | 0 | 1 |
| deadsync_online-tests | 255 | 0 | 3 |
| semantic | 105 | 68 | 74 |
| actor | 30 | 1 | 0 |

All 353 originally passing affected-package tests still pass. Six additional checks pass, making 359 passing package tests. The three paired benchmarks also pass when explicitly run. The existing live download test stays ignored.

The new checks compare all 34 bundled SMX GIFs; generated 16/25-LED canvases, invalid dimensions, marker ordering, absent markers, delay snapping and malformed headers; catalog sorting, aliases, duplicate-case key precedence, mixed JSON types, short rows, HTML, censorship and errors; and purchase generation wraparound, session/phase gates, unique ownership, retained UI snapshots and copy-on-write rollback. Allocation checks enforce the saved catalog/purchase churn and at least a 40% peak-heap reduction for the long pad-GIF case.

The native ITGmania comparison runs the Song Lua semantic parity and actor conformance harnesses. All 278 outcomes and all 69 existing failure diagnostics are unchanged, with only panic thread IDs normalized. These suites are integration guards; SMX LED and SRPG shop behavior are checked directly against their original implementations rather than represented as ITGmania harness coverage.

Both Epidermis archives and the indexed Sharkmode archive were attempted on baseline and current. All three stop before comparisons at the same existing noteskin dependency guard: `common/common/Fallback Receptor.lua`, actual hash `52ffa6df0701b426d6af887f9c91cd61895d8b32f349f0cd7ddfa48bae41e74d`, expected hash `96623726284f5ae0c5b12e05e0ae841eae40e100341a20042db68c9b80b52c74`. Full-song parity therefore remains unverified. No golden traces, fixtures or guards were changed.

**Existing GIF limitation:** a truncation sweep found inputs for which the original `image` frame iterator repeatedly returns errors and the original `filter_map` loop never finishes. A bounded preflight allowed comparison of two terminating truncated prefixes and skipped 91 repeated-error prefixes. The production error-filter behavior is preserved; this pass does not claim arbitrary-truncation robustness.

Formatting and `git diff --check` pass. The source-reference, exact-base, version, lockfile, excluded-file and unmerged-branch audits pass.

## Reproduce

From this worktree, use a reused Cargo target directory if disk space is limited. Build the two package test binaries with the same profile, and run the ignored paired benchmarks serially while builds and compatibility runs are idle:

```powershell
$env:CARGO_BUILD_JOBS = '2'
$env:CARGO_TARGET_DIR = 'C:/GitHub/deadsync-perf-1859-runtime-reuse-20261008/target/build'
cargo test --release --locked --config profile.release.lto=false -p deadsync-smx -p deadsync-online --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-smx -p deadsync-online --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test itgmania_actor_conformance -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- b40903481944c3395db4c848c671e27a383d2cbea51c74b029b7dea9dbccf16a.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- a867a12d25c984a49c2dd29fa06d47f88afe01841ee9d410dbf1332800797336.tar.zst
cargo test --release --locked --config profile.release.lto=false --test full_song_lua -- db60b3a89af8957a5e4fb1be2ec3157d09c436a564938536ec91b8ab064afa45.tar.zst
```

Machine-readable before/after outcomes and local logs remain under this worktree's ignored `target/` directory. The benchmark implementations and regression tests are committed.
