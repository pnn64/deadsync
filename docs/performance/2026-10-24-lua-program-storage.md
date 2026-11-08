# Native program preparation and initial byte storage

Baseline: `33a6cc5f9e21b17eaa2762cce4bdb44969840e73`, workspace 0.5.2357. Commit version: 0.5.2358.

Three retained changes:

1. Byte-arena normalization rewrites exclusively owned Code Arcs in place with safe `Arc::get_mut`. External/shared Programs still receive independent code before remapping, and repeated original Arcs retain the existing rewrite cache. This removes deallocation/reallocation of each uniquely owned Code allocation.
2. Normalization reuses its consumed Vec of Programs. Each original entry is replaced in place, including repeated Program aliases, and the same allocation becomes the Group's boxed program list. This removes the temporary result Vec and its allocation, element copies and separate destruction. Overcapacity may still require shrinking when converting to a boxed slice.
3. During mutable-byte schema promotion, the Song binder reuses the last initial byte entry when its contents match. Generic lowering reuses the last compiled initial byte value when the schema references that same entry again. Each mutable cell keeps its own index and writes; only immutable bytes are shared. Index comparison keeps distinct-vocabulary preparation linear and avoids repeatedly comparing byte contents in every compiled callback.

Generic code ownership, normalization and initial-cell lowering remain in deadlib-lua-legacy. Captured table schemas and promotion remain in deadsync-song-lua. No engine unsafe code or theme implementation changed. The interpreter source is unchanged from the baseline. These changes improve the existing VM-free representation; they do not install it into default Scene ownership.

## Method

[Raw samples, exact traces and DTOs, staged sources, source/binary hashes and focused-test receipts](2026-10-24-lua-program-storage.json).

Windows MSVC static CRT, Rust 1.99.1, release, `--locked -j 2 --config profile.release.lto=false`. Timed workers run sequentially on logical CPU 2 after builds finish. QueryThreadCycleTime supplies worker cycles. The counted System allocator includes Rust/mlua allocations and excludes native malloc. Other system activity is uncontrolled. Values are per-operation medians; throughput uses measured wall time. Runtime reuses initialized owners; preparation includes destruction.

SDK stages are baseline, arcs (exclusive Code reuse), vectors (plus consumed Vec reuse) and literals (plus initial byte reuse). Four process-order rotations each retain nine samples after warmup, giving 36 samples per row. Runtime operations are one callback; compile/group operations are three-callback units. Repeated-byte fixtures reference one initial byte entry from 3/16/64 distinct cells; distinct64 gives every cell a different 18-byte value. This declares shared immutable data, not aliased mutable cells.

## Generic runtime

| Fixture / stage | Cycles/callback | Wall us | Callbacks/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| numeric / baseline | 329.35 | 0.15 | 6,607,017.74 | 0.00 / 0.00 / 0.00 |
| numeric / arcs | 300.26 | 0.14 | 7,250,346.18 | 0.00 / 0.00 / 0.00 |
| numeric / vectors | 271.64 | 0.12 | 8,040,404.87 | 0.00 / 0.00 / 0.00 |
| numeric / literals | 274.25 | 0.13 | 7,942,522.07 | 0.00 / 0.00 / 0.00 |
| bytes3 / baseline | 299.80 | 0.14 | 7,261,888.27 | 0.00 / 0.00 / 0.00 |
| bytes3 / arcs | 326.49 | 0.15 | 6,685,220.86 | 0.00 / 0.00 / 0.00 |
| bytes3 / vectors | 278.52 | 0.13 | 7,821,053.93 | 0.00 / 0.00 / 0.00 |
| bytes3 / literals | 326.47 | 0.15 | 6,677,523.71 | 0.00 / 0.00 / 0.00 |

All stages produce identical 96-input state/results and identical measured execution digests. The interpreter is unchanged; runtime measurements check for cumulative effects without claiming an execution optimization.

## Preparation

Unique grouping consumes precompiled Programs with no external owners. Shared grouping retains external Programs. Compilation and the input batch allocation are outside the grouping timer; grouping, normalization and destruction are inside. Complete compile_group includes dump, schema reuse, lowering, group construction and destruction.

| Fixture / operation / stage | Cycles/unit | Wall us | Units/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| bytes3 / group_unique / baseline | 11,112.19 | 5.11 | 195,613.97 | 24.00 / 1.00 / 1,852.00 |
| bytes3 / group_unique / arcs | 12,635.83 | 5.90 | 169,564.50 | 21.00 / 1.00 / 1,564.00 |
| bytes3 / group_unique / vectors | 10,968.57 | 5.00 | 199,890.68 | 20.00 / 1.00 / 1,540.00 |
| bytes3 / group_unique / literals | 11,411.43 | 5.21 | 192,105.66 | 20.00 / 1.00 / 1,492.00 |
| bytes3 / group_shared / baseline | 8,520.20 | 3.89 | 257,376.97 | 33.00 / 1.00 / 4,924.00 |
| bytes3 / group_shared / arcs | 10,526.14 | 4.83 | 207,086.23 | 33.00 / 1.00 / 4,924.00 |
| bytes3 / group_shared / vectors | 8,704.97 | 3.97 | 251,968.50 | 32.00 / 1.00 / 4,900.00 |
| bytes3 / group_shared / literals | 9,151.69 | 4.21 | 237,311.70 | 32.00 / 1.00 / 4,852.00 |
| numeric / compile_group / baseline | 26,211.39 | 12.00 | 83,319.77 | 49.00 / 21.00 / 8,840.00 |
| numeric / compile_group / arcs | 25,951.59 | 11.83 | 84,530.30 | 49.00 / 21.00 / 8,840.00 |
| numeric / compile_group / vectors | 22,942.04 | 10.46 | 95,640.15 | 49.00 / 21.00 / 8,840.00 |
| numeric / compile_group / literals | 22,099.20 | 10.17 | 98,344.28 | 49.00 / 21.00 / 8,840.00 |
| bytes3 / compile_group / baseline | 31,239.31 | 14.25 | 70,160.05 | 87.00 / 24.00 / 12,064.00 |
| bytes3 / compile_group / arcs | 38,194.71 | 17.60 | 56,811.88 | 84.00 / 24.00 / 11,776.00 |
| bytes3 / compile_group / vectors | 32,816.96 | 14.97 | 66,797.13 | 83.00 / 24.00 / 11,752.00 |
| bytes3 / compile_group / literals | 33,634.08 | 15.50 | 64,525.89 | 77.00 / 20.00 / 11,008.00 |
| bytes16 / compile_group / baseline | 41,330.31 | 18.91 | 52,880.54 | 126.00 / 29.00 / 16,036.00 |
| bytes16 / compile_group / arcs | 49,268.32 | 22.69 | 44,068.79 | 123.00 / 29.00 / 15,748.00 |
| bytes16 / compile_group / vectors | 48,248.84 | 22.25 | 44,945.40 | 122.00 / 29.00 / 15,724.00 |
| bytes16 / compile_group / literals | 31,975.84 | 14.84 | 67,382.61 | 77.00 / 20.00 / 11,840.00 |
| bytes64 / compile_group / baseline | 80,921.77 | 37.10 | 26,957.58 | 270.00 / 35.00 / 32,356.00 |
| bytes64 / compile_group / arcs | 87,638.80 | 39.98 | 25,011.97 | 267.00 / 35.00 / 32,068.00 |
| bytes64 / compile_group / vectors | 90,402.28 | 41.74 | 23,958.37 | 266.00 / 35.00 / 32,044.00 |
| bytes64 / compile_group / literals | 37,143.51 | 17.12 | 58,418.15 | 77.00 / 20.00 / 14,912.00 |
| distinct64 / compile_group / baseline | 130,633.36 | 59.81 | 16,718.37 | 337.00 / 39.00 / 46,708.00 |
| distinct64 / compile_group / arcs | 108,427.86 | 50.05 | 19,981.58 | 334.00 / 39.00 / 46,420.00 |
| distinct64 / compile_group / vectors | 125,167.30 | 57.17 | 17,492.67 | 333.00 / 39.00 / 46,396.00 |
| distinct64 / compile_group / literals | 110,561.98 | 50.88 | 19,654.66 | 333.00 / 39.00 / 46,396.00 |

Exclusive Code reuse removes three allocations and 288 requested bytes for the three-program unit. Shared/external Code follows its original clone policy. Vec reuse removes one allocation and 24 requested bytes in both ownership cases. Initial-byte reuse removes another 6/45/189 allocations and 744/3,884/17,132 requested bytes in the 3/16/64 repeated-byte compile units. Distinct values receive no storage merging benefit, and their measured CPU cost is retained above.

| SDK fixture | Independent-machine payload bytes baseline -> final | Group payload bytes baseline -> final |
|---|---:|---:|
| numeric | 3,513 -> 3,513 | 3,305 -> 3,305 |
| bytes3 | 4,189 -> 4,069 | 3,648 -> 3,648 |
| bytes16 | 6,217 -> 5,317 | 4,480 -> 4,480 |
| bytes64 | 13,705 -> 9,925 | 7,552 -> 7,552 |
| distinct64 | 16,393 -> 16,393 | 9,728 -> 9,728 |

Payload excludes allocator/Arc bookkeeping. The old Group already deduplicated shared immutable bytes, so initial-cell reuse chiefly removes preparation copies and reduces independent Program/Machine payload. No Lua VM is included in these payload counts.

## Actual Song input/update units

Three alternating baseline/final process pairs retain nine samples per process (27 per row). Numeric and finite-string authored fixtures keep actual player input, shared state, rates, hibernation and elapsed-time branches. Every process compares 96 inputs and 96 updates against Lua, proves VM closure using a globally rooted finalizer, then replays detached native execution. All traces compare exactly across processes and variants.

| Fixture / operation / variant | Cycles | Wall us | Operations/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| timed-input / native pair / baseline | 1,089.50 | 0.50 | 1,990,926.77 | 0.00 / 0.00 / 0.00 |
| timed-input / native pair / normal | 1,000.18 | 0.46 | 2,182,438.19 | 0.00 / 0.00 / 0.00 |
| timed-input / ordinary Scene pair / baseline | 1,406,002.40 | 645.51 | 1,549.15 | 119.50 / 0.00 / 7,064.00 |
| timed-input / ordinary Scene pair / normal | 1,361,735.13 | 625.33 | 1,599.16 | 119.50 / 0.00 / 7,064.00 |
| timed-input / prepare unit / baseline | 102,004.04 | 47.32 | 21,133.97 | 114.00 / 28.00 / 31,458.00 |
| timed-input / prepare unit / normal | 96,862.95 | 44.28 | 22,585.71 | 114.00 / 28.00 / 31,458.00 |
| state-machine-input / native pair / baseline | 930.45 | 0.42 | 2,361,623.62 | 0.00 / 0.00 / 0.00 |
| state-machine-input / native pair / normal | 1,070.92 | 0.49 | 2,051,008.15 | 0.00 / 0.00 / 0.00 |
| state-machine-input / ordinary Scene pair / baseline | 1,360,320.39 | 624.27 | 1,601.86 | 119.50 / 0.00 / 7,080.00 |
| state-machine-input / ordinary Scene pair / normal | 1,375,324.54 | 631.42 | 1,583.73 | 119.50 / 0.00 / 7,080.00 |
| state-machine-input / prepare unit / baseline | 129,914.83 | 59.37 | 16,843.44 | 192.00 / 41.00 / 49,554.00 |
| state-machine-input / prepare unit / normal | 127,165.95 | 58.10 | 17,211.94 | 182.00 / 38.00 / 48,562.00 |

timed-input native execution measured -8.2% cycles. state-machine-input native execution measured +15.1% cycles. The interpreter source is unchanged, and the cause of these mixed runtime effects is not isolated. In particular, the slower string-state row is a limitation of the cumulative result; no general gameplay execution gain is claimed.

Ordinary Scene runtime includes host work, output draining and state readback; it is not an isolated interpreter comparison. Native execution allocates no engine storage.

| Fixture | Native payload bytes baseline -> final | Current Lua allocator bytes baseline -> final |
|---|---:|---:|
| timed-input | 14,883 -> 14,883 | 17,680,843 -> 17,668,667 |
| state-machine-input | 17,164 -> 17,164 | 17,681,275 -> 17,669,099 |

Native payload excludes allocator/Arc bookkeeping. Lua bytes include pending garbage and exclude Rust payload, so the scopes are not equivalent and neither is RSS. Every run is recorded in the JSON.

## Complete compilation

Five loader/DTO cases use three alternating process pairs and five samples: 30 processes and 150 measured compilations. Every complete DTO snapshot matches exactly. Source loading/execution, extraction and DTO preparation are timed; warm setup and snapshot formatting are excluded equally. Native unit extraction is additional loading work measured separately above.

| Workload | Cycles before -> after | Compiles/s before -> after | Allocations before -> after | Reallocations before -> after | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 418,601,868.00 -> 417,125,509.00 | 5.20 -> 5.22 | 254,585 -> 254,585 | 13,078 -> 13,078 | 39,568,330 -> 39,568,330 |
| live_string_states | 440,796,831.00 -> 444,043,672.00 | 4.93 -> 4.90 | 250,551 -> 250,551 | 14,380 -> 14,380 | 40,464,474 -> 40,464,474 |
| live_timed_inputs | 442,547,784.00 -> 442,568,638.00 | 4.92 -> 4.91 | 250,548 -> 250,548 | 14,375 -> 14,375 | 40,461,407 -> 40,461,407 |
| offline_state_boundaries | 784,754,873.00 -> 783,834,060.00 | 2.78 -> 2.78 | 293,920 -> 293,917 | 17,119 -> 17,119 | 43,717,401 -> 43,717,271 |
| live_state_boundaries | 685,062,569.00 -> 681,279,948.00 | 3.18 -> 3.20 | 272,100 -> 272,120 | 16,985 -> 16,985 | 43,061,422 -> 43,062,465 |

Complete-compilation wall times, throughput, cycles and every allocation sample are retained in the JSON. No general whole-loading improvement is assumed from local gains.

The trial containing a direct-comparison interpreter change demonstrated no actual Song execution gain: cumulative numeric/string cycles increased about 1.0%/1.9% despite isolated microbenchmark gains. That interpreter change was discarded. These cumulative measurements do not isolate its causal cost; its summary, source and build receipts are preserved in the JSON.

## Validation and scope

- **43 focused checks pass:** 19 generic compiler checks, 11 actor input/update checks, 11 existing input checks, one method identity/weak-rooting check and one production renderer check. No full suite or Song `--lib` harness ran.
- The new regression exercises 3/16/17/64 independent cells sharing initial bytes, checks storage reduction against distinct values, mutates one cell and verifies the others remain unchanged after loading VM destruction. Existing differential tests cover numeric bits, signed-zero/NaN equality, binary/NUL/FF/empty bytes, branch joins, distinct constant-pool orders, transient/type-change rejection, external Machines and duplicate Program Arcs.
- Authored numeric and string-state gameplay differentials compare handling, ordered writes, state transitions and actor/scalar state, then replay after Scene destruction. Production playback/composition matches on **120 render frames and 120 byte-identical OpenGL captures**, with visible-content assertions and detached native replay.
- Baseline/final ordinary Scene audits match on 12 update/input/draw frame pairs. All interned evidence traces round-trip exactly. Source/binary hashes and build settings are recorded for each stage.
- The six historical Song failures were absent at the earlier audit (831 passed, 0 failed, 41 ignored). They were not rerun or fixed in this pass.

The existing native callback units remain independent snapshots. Default Scene ownership, host/player updates, queued timers, effects and dynamic lifecycle work still retain Lua. This pass migrates no new behavior category and does not prove whole-scene isolation. Finite byte writes remain limited to compiled literals or other byte cells. No theme implementation changed and no cross-theme matrix ran; render equality here uses identical assets and conditions.

Saved normal baseline binaries embed 0.5.2356 from the preceding pass, which then bumped metadata to 0.5.2357. SDK and current normal binaries embed 0.5.2357. The final 0.5.2358 bump is metadata only.
