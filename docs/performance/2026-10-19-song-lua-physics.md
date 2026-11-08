# Song Lua compiled simulation queries

This pass starts from `20586889b`, following the [draw conversion pass](2026-10-19-song-lua-conversion.md). It applies three changes under M-HOTPATH, M-INITIAL-CAPACITY, M-MEM-REUSE, M-BOX-DST and M-THROUGHPUT in [rust-performance.md](../../rust-performance.md):

1. For scenes with at least 64 entities and fewer than half carrying a ball, build an ordered ball-index slice at initialization. Collision, respawn, checkpoint and goal systems visit those entities directly instead of repeatedly filtering the complete entity array.
2. In that sparse mode, build an ordered physics-index slice for collision-flag resets and both physics integration half-steps. Other component systems keep their original traversal.
3. In sparse mode, track whether a collision response set a collision flag and use that event to gate the existing final sound/RNG operation, replacing a full post-simulation flag scan.

Small and dense-ball scenes retain the original contiguous traversals and final flag scan through constant specialization, with no query-buffer allocation. Both variants use the current scene representation; the frozen baseline ignores its new cache fields. Lowering fixes component membership before initialization; runtime systems only update component contents. Reinitialization refreshes the traversal mode and query slices. Author order, per-collider snapshots, double-precision arithmetic, input thresholds and system order remain unchanged. No new unsafe code is introduced. The shared rotation, rendering, sound and containment helpers are unchanged from the preceding commit.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-physics.json) retain six alternating-order rounds and 54 samples per variant/workload. Tables show medians before → after. These results combine all three changes and do not assign separate speedups to each.

| Case | CPU cycles/unit | Allocations/unit | Requested bytes/unit | Units/second |
| --- | ---: | ---: | ---: | ---: |
| Eight boxes, two balls | 1,942.92 → 1,942.92 | 0.0000 → 0.0000 | 0.00 → 0.00 | 1,125,770 → 1,115,225 |
| 1,000 background entities, 32 rotated boxes, two balls | 169,313.37 → 42,524.70 | 0.0000 → 0.0000 | 0.00 → 0.00 | 12,878 → 51,559 |
| 32 rotated boxes, 128 balls | 249,132.50 → 259,102.59 | 0.0000 → 0.0000 | 0.00 → 0.00 | 8,796 → 8,444 |
| 1,000 background entities, 32 unrotated boxes, two balls | 159,413.58 → 38,234.16 | 0.0000 → 0.0000 | 0.00 → 0.00 | 13,751 → 57,243 |
| 2,000 static entities | 115,787.98 → 65,825.99 | 0.0000 → 0.0000 | 0.00 → 0.00 | 18,918 → 33,280 |
| 128 background entities, eight boxes, two contacting balls | 6,422.95 → 3,422.83 | 0.0234 → 0.0234 | 2.38 → 2.38 | 340,697 → 637,927 |

A tick is one complete compiled simulation update with empty render bindings. It includes input/movement, two integration/collision half-steps, respawn/follow/trigger systems and audio gating. Fixtures use zero through 128 balls and zero through 2,000 background entities. Render projection and Lua execution are outside these workloads, so the rates are not gameplay FPS or whole-song speedups. The contact workload retains ordinary sound paths and RNG selection. Its sound output vector is reused and cleared per tick; fixture setup and final output/scene destruction are excluded.

Sparse cases run 4.00–4.16× as fast and use 74.9–76.0% fewer CPU cycles. Static and contact throughput rises 75.9% and 87.2%. Small-scene throughput is 0.9% lower in aggregate and varies in sign across rounds. Dense-ball throughput is 4.0% lower; its per-round changes range from -21.4% to +5.9%. Sparse, static and contact cases improve in every round. All tick allocation/reallocation counts and requested-byte traffic are unchanged.

| Case | CPU cycles/unit | Allocations/unit | Requested bytes/unit | Units/second |
| --- | ---: | ---: | ---: | ---: |
| Initialize empty scene | 13.17 → 23.93 | 0.0000 → 0.0000 | 0.00 → 0.00 | 144,927,536 → 86,206,897 |
| Initialize sparse scene (two balls/physics bodies) | 1,673.14 → 11,496.42 | 0.0000 → 2.0000 | 0.00 → 32.00 | 1,307,190 → 190,598 |
| Initialize dense scene (128 balls/physics bodies) | 192.17 → 793.16 | 0.0000 → 0.0000 | 0.00 → 0.00 | 11,293,055 → 2,755,580 |

Setup units repeat initialization on an already allocated entity array; after the first call they also include releasing the preceding query slices. Each result is observed to prevent redundant-reset elimination. A nonempty cached query adds one allocation, with exactly `sizeof(usize) * component_count` bytes. On this 64-bit host the sparse case adds two allocations totaling 32 bytes, while small/dense cases allocate no query buffers. Empty queries allocate nothing. Cache headers and the traversal-mode flag additionally enlarge the scene representation; outer scene storage is excluded from these isolated measurements. Sparse reinitialization rises from 0.77 to 5.25 microseconds. These are explicit setup costs in exchange for reusing the indices for the scene lifetime.

## Behavior and validation

Four new release checks pass against frozen initialization/tick code from `20586889b`. They compare authored entity order, input activation across frame 60, moves/spins, follow targets, respawn, moving/hybrid ball-collider entities, checkpoints, multiple goal matches, collision flag resets, sound ordering and complete RNG state. Numeric cases include signed zero, very small/large finite values, infinities and NaNs. Position/velocity finite values compare exact bits; NaNs compare classification. Query construction checks its authored order, dense-mode bypass, exact allocation traffic and refresh after component edits followed by reinitialization.

The default-feature release library suite has 697 passes, 13 failures and five ignored performance benchmarks. All 13 failures match the preceding commit's normalized diagnostics, whose retained log hash matches its committed receipt. All four new physics checks pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records each failure and diagnostic hash, source/executable hashes and the build-receipt hash. Frozen methods were checked against `git show 20586889b` after receiver adapters; shared helpers were checked unchanged. Native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction and scope

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 20586889b --test-filter compiled_scene::runtime::perf::physics::benchmark_physics --out docs/performance/2026-10-19-song-lua-physics.json
```

Run the executable with `--test-threads=1` for the full library suite. Each round uses one warmup plus nine retained samples per variant/workload, pinned to logical CPU 2. Tick samples execute 32–256 updates and setup samples repeat initialization 1,000 times. Both variants run in one executable with the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled.

QueryThreadCycleTime records scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Timing/cycles include accounting overhead. Requested bytes describe allocation traffic, not RSS. Builds and other benchmarks do not overlap measurements. Small/dense cases provide coverage of the query bookkeeping cost; gains on sparse scenes must not be generalized to all levels.
