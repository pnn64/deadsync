# Lua frame capture - 0.5.1627

Parent: `8b3c45af9` (0.5.1626). Date: 2026-09-30.

This pass removes recurring directory and vertex-color ownership work and speeds
up internal actor lookup. It applies the local guide's M-HOTPATH, M-MEM-REUSE,
M-FAST-HASHER, and M-THROUGHPUT recommendations:

1. Actor callbacks retain the existing immutable Lua directory string while
   scoping `__songlua_script_dir`. They avoid copying it into a Rust `String`
   and then creating another Lua string. This works for long paths as well as
   short ones, without caching a potentially stale actor field.
2. Overlay update capture uses `FxHashMap` for actor pointers, which are keys
   assigned by the runtime. The compiler builds this map directly from its actor
   iterator.
3. Existing tracks compare vertex colors by borrowing the state's inline array
   and the last sample's owned array. Current and message snapshots are created
   only when the track changes. An unchanged color track previously created and
   freed two `Arc` allocations on every frame. Scalar comparisons also avoid
   constructing an unused current snapshot.

The private directory conversion checks a live, type-checked Lua stack string
and validates UTF-8 before retaining mlua's owning handle. No borrowed pointer
escapes the conversion. Numeric coercions and conversion errors preserve the
former `String` behavior. Global metatable operations, nested scope restoration,
callback mutation and restoration-error precedence remain observable at the same
points. There are no new dependencies or changes to the crate's exported API.

## Measurements

All 136 measurement rows, including both execution orders, timing ranges,
throughput, frees and byte counts, are in [the CSV](lua-frame-capture-0.5.1627.csv).
Absolute values below use run 1; CPU ranges cover all four runs.

| Workload | CPU cycles/op old -> new | CPU reduction across runs | Allocations/reallocations old -> new | Requested bytes old -> new |
| --- | ---: | ---: | ---: | ---: |
| 64 callbacks, 35-byte directory | 105,434 -> 92,705 | 5.3-12.5% | 64/0 -> 0/0 | 2,240 -> 0 |
| 64 callbacks, 4107-byte directory | 522,971 -> 110,436 | 78.5-79.1% | 136/0 -> 0/0 | 527,808 -> 0 |
| 128 actors, 8192 captured writes | 806,097 -> 601,262 | 25.4-29.3% | 0/0 -> 0/0 | 0 -> 0 |
| 32 unchanged color tracks, 64 frames | 653,872 -> 71,459 | 88.8-89.9% | 4096/0 -> 0/0 | 327,680 -> 0 |
| 128 unchanged color tracks, 64 frames | 2,720,433 -> 359,687 | 85.4-86.8% | 16384/0 -> 0/0 | 1,310,720 -> 0 |

Every measured new directory batch has zero allocations, reallocations, frees
and requested/freed bytes. Short 11- and 35-byte directories use 5.3-13.6% fewer
cycles and gain 5.7-15.7% throughput. A 107-byte directory uses 20.5-33.2% fewer
cycles and gains 25.8-49.6% throughput. The 4107-byte stress case gains
366.2-379.5% throughput. Long-path baseline counts include Lua GC work; freed
bytes can exceed bytes requested in the separately counted operation because
GC also collects garbage from earlier iterations.

Actor capture uses 25.0-33.2% fewer cycles and gains 33.4-49.8% throughput across
all four actor counts. Both maps already have zero warm allocation churn.
With 128 actors, run 1 increases throughput from 22.28 to 29.80 million captured
writes per second. These measurements cover capture after map creation.

Unchanged color tracks eliminate all 128, 4096 or 16384 temporary allocations
and frees per batch with 1, 32 or 128 tracks. They use 72.9-89.9% fewer cycles.
With 32 tracks, throughput increases 791.8-890.7% across the runs, and run 1
reduces batch time from 298.80 to 32.58 microseconds. With 128 tracks, throughput
increases 587.1-656.8%. Scalar controls also stay allocation-free and use
3.5-22.1% fewer cycles across the measured sizes.

Colors that change every frame still require owning output samples. The control
retains identical allocations, reallocations, frees and requested/freed bytes:
33/4 allocation/reallocation calls and 4,736 requested bytes with one actor;
1025/131 calls and 147,200 requested bytes with 32 actors. It shows no consistent
CPU improvement: the one-actor case ranges from 8.2% more cycles to 5.5% fewer,
and the 32-actor case ranges from 5.1% more to 7.9% fewer. Throughput ranges from
7.3% lower to 5.2% higher and from 4.7% lower to 8.8% higher, respectively.
These timing regressions are included in the CSV. The reliable color gain
applies to comparisons that avoid producing a new sample.

## Method and reproduction

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.1. Both implementations
run in the same release test executable with optimization level 3 and fat LTO.
The scoped System allocator counts calling-thread
allocations, reallocations, frees, and requested/freed bytes. QueryThreadCycleTime
counts calling-thread CPU cycles. Timing and allocation accounting are separate.
Byte totals describe allocator churn, not peak resident memory or process RSS.

Each row reports the median of seven timing samples after warmup and a separate
allocation-counted operation. Four runs alternate old-first and new-first.
Directory batches execute 64 real Lua callbacks, each checking its scoped path
and parameter and updating a counter. Directory payloads of 0, 24, 96, 512 and
4096 bytes follow an 11-byte `Songs/Pack/` prefix. Normal Lua GC stays on.

Actor capture batches write four targets per actor for 16 frames with 1, 32, 128
or 512 actors, clearing touched buffers after each frame. Map creation is outside
measurement. The loop includes pointer lookup, first-touch tracking, replacement
of captured values, and reuse of existing vectors. Track batches run 64 frames
with 1, 32 or 128 existing color or scalar tracks. State buffers and output tracks
already exist, and scratch storage is warmed before timing.

The changing-color control emits new samples on every one of 16 frames with
1 or 32 actors. Each operation has a fresh fixture; setup, warmup and fixture
destruction are outside measurement. Allocations for output growth are included;
frees associated with fixture destruction are excluded. Per-operation clock
overhead is included equally in both variants. The other workloads include
destruction of temporary values inside the measured batch.

The test-only baselines freeze the parent's capture struct, its complete method
implementation, callback helper and complete track-capture helper. All four items
were checked against the parent, allowing visibility and formatting only. Shared
helpers are unchanged in the measured paths. These benchmarks measure the three
operations and track synchronization; they do not measure complete compilation,
whole-game frame rate, cache misses or peak memory.

```powershell
cargo test -p deadsync-song-lua --lib
cargo test -p deadsync-song-lua --release --lib
cargo test -p deadsync-song-lua --release --lib frame_capture -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib frame_capture -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

## Behavior validation

Eight new regression tests cover directory coercions, missing and blank values,
Unicode whitespace, Unicode/NUL and invalid UTF-8, long paths, callback field
mutation, nested callbacks with GC and partial errors, global metatable ordering
and restoration-error precedence. Capture tests compare missing actors,
first-touch order, repeated writes, scheduled updates and stateful broadcasts.
Track tests cover every target, clears, signed zero, NaN payloads and repeated
NaNs, color changes and captured-target precedence. Allocation assertions require
zero warm directory, actor-capture and steady-track churn and reduced color churn.

- Debug and release library suites each report 644 passed, 4 failed, 59 ignored.
- All eight new tests pass in both profiles. Both manual benchmark tests pass
  in all four release runs.
- Parent `8b3c45af9` recorded 636 passed, the same 4 failed, and 57 ignored in
  both profiles. See [the prior report](lua-update-dispatch-0.5.1626.md).
- An automated log comparison confirms identical failure assertions:
  `compile_song_lua_extracts_actorproxy_targets`,
  `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`, and
  `compile_song_lua_runs_cmd_queuecommand_builders` expect different initial
  proxy visibility; `compile_song_lua_supports_notefield_column_api` expects
  `-96:-125` while both versions produce `-96:-135`.
- Clippy completes with the existing warnings and four additional warnings in
  frozen parent bodies (argument counts and an existing optional-value check).
  It reports no warnings in the new production adapter or comparison code, or
  in the new test fixtures.
- Formatting, frozen-item audit, locked metadata, version/lock consistency and
  `git diff --check` pass.

The workspace version advances exactly once, 0.5.1626 -> 0.5.1627. Cargo.lock
updates the three packages that inherit it. The four excluded files are not
part of the commit.
