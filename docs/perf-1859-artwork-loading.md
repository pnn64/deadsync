# Artwork loading performance, 0.5.1859

Branch: `perf/1859-runtime-reuse-20261008`.
Starting commit: `1139884c188ec79f4133f63d7f3f4fa4bffa6f89` (main, 0.5.1858).
Cargo.toml and the three workspace package entries in Cargo.lock advance to 0.5.1859.

## Changes

- Bound texture decode and artwork prewarm workers by the number of eight-job batches. Keep the decode worker for a single batch so decoding can still overlap uploads. On this machine, 8/32/129 texture jobs use 1/4/17 workers instead of 8/32/44.
- Check cache freshness and read the validated payload through one file handle, removing a redundant filesystem metadata lookup and the separate validation wrapper. Preserve stale-cache handling, invalid-cache removal, exact length checks, and the fallback cleanup policy when opening fails.
- Hash normalized path components directly and construct the cache filename in one step. This removes the normalized path string, intermediate stem, and intermediate directory allocation while preserving cache keys.

All optimization code changes are in deadlib-assets. No Song Lua source changes are included.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.1; Rust reports 44 available workers. Release optimization with LTO disabled. Both implementations run in the same executable with the same compiler options and allocator. Frozen originals match the starting commit, ignoring formatting and their `_original` suffix.

Each measurement alternates original/current order across ten sample pairs, discards the first pair, and reports the median of nine samples per implementation. Texture inputs are prepared before timing. Each pair uses the same warm local files on C:. Values are per entire batch for texture jobs, and per cache operation otherwise. These are targeted benchmarks, not whole-game startup or frame-rate measurements.

| Workload | Original µs/op | Current µs/op | Throughput |
| --- | ---: | ---: | ---: |
| texture decode/1 PNGs | 222.96 | 221.63 | 1.01x |
| texture decode/8 PNGs | 1630.17 | 1559.01 | 1.05x |
| texture decode/32 PNGs | 4719.09 | 3086.26 | 1.53x |
| texture decode/129 PNGs | 13070.44 | 12382.82 | 1.06x |
| cache handle/1x1/validate=false | 193.85 | 144.63 | 1.34x |
| cache handle/1x1/validate=true | 194.12 | 145.23 | 1.34x |
| cache handle/256x80/validate=false | 206.76 | 157.84 | 1.31x |
| cache handle/256x80/validate=true | 203.16 | 153.69 | 1.32x |
| cache handle/1024x512/validate=false | 1410.38 | 1361.20 | 1.04x |
| cache handle/1024x512/validate=true | 767.58 | 719.37 | 1.07x |
| canonical cache key | 183.29 | 183.47 | 1.00x |

Allocation counts are measured separately from timing. Texture counts cover the caller thread only; worker allocations and OS thread-stack reservations are excluded. Key counts cover the complete cache-key operation. Allocated bytes include allocation and reallocation requests, rather than peak live memory.

| Operation | Allocations | Reallocations | Allocated bytes |
| --- | ---: | ---: | ---: |
| texture decode/1 caller-thread | 10 → 10 | 1 → 1 | 95197 → 95197 |
| texture decode/8 caller-thread | 26 → 5 | 0 → 0 | 1256 → 192 |
| texture decode/32 caller-thread | 98 → 14 | 0 → 0 | 4904 → 648 |
| texture decode/129 caller-thread | 134 → 53 | 0 → 0 | 6728 → 2624 |
| cache key | 8 → 5 | 5 → 4 | 1560 → 958 |

## Regression checks

All 73 starting asset tests still pass. Three new differential tests pass (76 total), covering batch boundaries, alpha/grayscale output, missing textures, cancellation, Unicode/path separator keys, source timestamp cases, corrupt/truncated/oversized headers and payloads, dimension overflow, zero-width images, missing files and directories, and invalid-cache deletion. Three ignored paired benchmark tests were run separately and passed. Formatting and whitespace checks pass.

ITGmania compatibility outcomes and failure diagnostics are identical before and after, after normalizing thread IDs:

| Suite | Passed | Existing failures | Ignored |
| --- | ---: | ---: | ---: |
| semantic | 112 | 61 | 74 |
| actor | 30 | 1 | 0 |

Both Epidermis archive variants and the selected Sharkmode archive hit the same pre-existing `common/common/Fallback Receptor.lua` dependency hash guard before any full-song comparisons. This blocks full-song parity verification; the unchanged outcomes do not establish complete compatibility. No new failures were observed in the checks that ran.

The shared D: build cache became unavailable during preliminary work; those timings are not used above. The default fat-LTO test link then failed with `Allocation failed` on local storage. Final tests and paired benchmarks therefore use `profile.release.lto=false`. No compiler/profile change is committed.

## Reproduce

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib -- --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --lib benchmark_ -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false --test song_lua_itgmania_semantic_parity --test itgmania_actor_conformance -- --test-threads=1
```

Run each archive with the `full_song_lua` test executable and its filename from `tests/fixtures/full_song_lua/index.json`. The workspace's ignored `target/` directory contains the raw baseline/current logs and the machine-readable compatibility comparison for this pass.
