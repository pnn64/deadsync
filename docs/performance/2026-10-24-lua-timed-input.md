# Native input/update callback units

Baseline: `48719519aae85d4ec866483cfd5f7f8e1a0314bd`, workspace 0.5.2355. Commit version: 0.5.2356.

Three retained changes:

1. `SongLuaLiveScene::compile_actor_interactions` compiles screen input callbacks and the authored root's installed update callbacks into one independent native unit. They share actual captured scalar-table cells and native actor state. Real inputs arm/cancel a deadline; real elapsed time drives the original callback's countdown, branches and visibility actions. No song-specific rewrite or fixed timeline is substituted. The artifact owns no Lua, Function or Table.
2. `deadlib-lua::program::Compiler` reuses an immutable borrowed schema across callbacks while binding arguments/captures separately. It checks field uniqueness once per schema. Schemas with at least 32 fields use one sorted field index, replacing quadratic duplicate checks and repeated linear lookup; smaller schemas retain allocation-free contiguous scans. Code and literal data remain owned by each returned Program. The Song binder uses the reusable compiler for both input and update entries.
3. Input frames retain formatted buffers only for uncommon indices and select common static names directly from the input. This removes the nested optional name representation and its repeated tag handling. None remains nil, P1/P2 and controllers 1/2 bytes are unchanged, and other indices keep the original inline formatting and release overflow behavior. All 13 fields match a frozen preceding implementation.

Native timing copies the loading scene's already-built update traversal without running authored lookup code. Parent/child/wrapper order, f32 delta conversions, static rates and hibernation overshoot follow that plan. The synthetic loader root participates in timing without needing render state. Callback return values cannot stop later update callbacks. Traversal tables outside the render inventory cannot also enter mutable scalar-cell storage through captures. Active queues, effects, pending callback changes, recurring commands, note columns and foreground overrides reject this unit. Default scene input/update dispatch still uses Lua. The snapshot is independent of later lifecycle activity and is not an isolation proof for a whole scene.

Generic indexing/compilation remains in deadlib-lua. Actor fields, method authentication, host traversal and timing rules remain in deadsync-song-lua. No theme implementation changed and no engine unsafe code was added. The changes remove repeated field validation/search, schema work, optional-name handling and Lua dispatch/readback for the migrated callback unit.

## Measurements

[Raw samples, source/binary hashes, staged source, traces and test receipts](2026-10-24-lua-timed-input.json).

Windows MSVC static CRT, Rust 1.99.1, release, `--locked -j 2 --config profile.release.lto=false`. All timed workers run sequentially on logical CPU 2 after compilation. QueryThreadCycleTime supplies worker cycles; the counted System allocator covers process Rust allocations, including mlua, but excludes native malloc. Other system activity is uncontrolled.

Three process sets alternate SDK before/after order and actor before/after order. SDK/frame pairs also alternate inside each process and retain 13 samples after warmup. Timer and actor preparation retain 9 samples per process. Medians are per operation. The SDK operation includes dump, schema/lowering, machine construction, one native call and destruction; schema creation is included and amortized only over the stated callback count.

## Input-to-countdown-to-action unit

The fixture has a parent rate of 1.25, a child rate of 0.5, initial hibernation, shared deadline/order/tick cells, and one Quad. Repeated actual Right inputs restart its deadline; Left cancels it. Child updates expire it before the parent callback runs. Loading and compilation may use Lua; subsequent input/update execution uses native storage only.

| One input/update pair | Worker cycles | Wall us | Pairs/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| Ordinary scene | 1,736,055.23 | 803.15 | 1,245.09 | 119.50 / 0.00 / 7,064.00 |
| Native callback unit | 1,309.85 | 0.60 | 1,673,567.23 | 0.00 / 0.00 / 0.00 |

The native unit uses 99.92% fewer cycles and 1,344.13x throughput in this workload. Ordinary scene timing includes host updates, state readback and output draining; it is not a measurement of only the Lua function interpreter. Native owned payload is **14,859 bytes**. The loading scene reports 17,668,643 current Lua allocator bytes after validation, including pending garbage without forced collection. These scopes differ and neither is RSS.

| One native unit preparation | Worker cycles | Wall us | Units/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| Input/update unit | 133,629.20 | 61.21 | 16,336.12 | 119.00 / 30.00 / 31,757.00 |

Each timer process compares 96 inputs and 96 updates against the actual scene, including every authored actor state and captured scalar's bits. A globally rooted Lua finalizer proves that a separate loading VM closes before detached native replay continues with the same results. Original host/player lifecycle behavior is outside this callback unit. Frame validation remains the caller/host's responsibility.

## Reusable compiler

Original uses the preceding compiler. Indexed uses the final ordinary compile API and builds its index per callback. Prepared reuses the final Compiler across the stated callback group. All stages compile and run the same callback, with equal digests. Unused schema fields model a larger host binding surface. No frequency-weighted whole-application aggregate is claimed.

| Fields / callbacks | Cycles original / indexed / prepared | Wall us original / indexed / prepared | Allocations original / indexed / prepared | Reallocations original / indexed / prepared | Requested bytes original / indexed / prepared |
|---|---:|---:|---:|---:|---:|
| 16 / 1 | 8,690.81 / 9,637.41 / 9,129.84 | 4.04 / 4.44 / 4.13 | 17.00 / 17.00 / 17.00 | 6.00 / 6.00 / 6.00 | 2,524.00 / 2,524.00 / 2,524.00 |
| 16 / 16 | 8,166.51 / 8,134.36 / 8,064.48 | 3.79 / 3.73 / 3.73 | 17.00 / 17.00 / 17.00 | 6.00 / 6.00 / 6.00 | 2,524.00 / 2,524.00 / 2,524.00 |
| 16 / 64 | 8,518.16 / 8,454.39 / 8,103.60 | 3.93 / 3.92 / 3.75 | 17.00 / 17.00 / 17.00 | 6.00 / 6.00 / 6.00 | 2,524.00 / 2,524.00 / 2,524.00 |
| 64 / 1 | 11,221.94 / 9,822.62 / 9,808.91 | 5.09 / 4.48 / 4.43 | 17.00 / 18.00 / 18.00 | 6.00 / 6.00 / 6.00 | 3,676.00 / 4,188.00 / 4,188.00 |
| 64 / 16 | 11,269.95 / 9,075.81 / 8,282.27 | 5.21 / 4.21 / 3.79 | 17.00 / 18.00 / 17.06 | 6.00 / 6.00 / 6.00 | 3,676.00 / 4,188.00 / 3,708.00 |
| 64 / 64 | 10,858.93 / 9,248.15 / 8,421.17 | 5.04 / 4.27 / 3.91 | 17.00 / 18.00 / 17.02 | 6.00 / 6.00 / 6.00 | 3,676.00 / 4,188.00 / 3,684.00 |
| 256 / 1 | 45,024.94 / 12,175.38 / 12,038.22 | 21.42 / 5.52 / 5.46 | 17.00 / 18.00 / 18.00 | 6.00 / 6.00 / 6.00 | 8,284.00 / 10,332.00 / 10,332.00 |
| 256 / 16 | 43,323.38 / 12,049.35 / 9,127.69 | 20.02 / 5.52 / 4.19 | 17.00 / 18.00 / 17.06 | 6.00 / 6.00 / 6.00 | 8,284.00 / 10,332.00 / 8,412.00 |
| 256 / 64 | 41,411.87 / 11,689.13 / 9,365.19 | 19.15 / 5.39 / 4.31 | 17.00 / 18.00 / 17.02 | 6.00 / 6.00 / 6.00 | 8,284.00 / 10,332.00 / 8,316.00 |

Prepared compilation uses 12.6â€“78.9% fewer cycles in the 64/256-field cases. The single-callback, 16-field case uses 5.1% more cycles. Throughput for every stage is retained in the JSON evidence. Extra index storage can add an allocation for a single large-schema callback; reuse amortizes it. Small-schema and single-callback CPU effects are reported without assuming a universal gain.

## Actual actor snapshot preparation

The same fixtures with 1/16/64 callbacks are compiled before and after all changes. Every trace matches exactly; these measurements isolate preparation by skipping the longer Lua runtime loop. The ordinary snapshot still covers input only.

| Callbacks | Prepare cycles before -> after | Wall us before -> after | Snapshots/s before -> after | Allocations before -> after | Reallocations before -> after | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 54,804.69 -> 53,525.41 | 25.49 -> 24.73 | 39,230.11 -> 40,433.40 | 58 -> 58 | 10 -> 10 | 22,314 -> 22,314 |
| 16 | 328,341.12 -> 316,121.16 | 151.27 -> 145.45 | 6,610.89 -> 6,875.14 | 433 -> 433 | 149 -> 149 | 153,985 -> 153,985 |
| 64 | 1,181,978.38 -> 1,142,528.40 | 545.70 -> 527.14 | 1,832.51 -> 1,897.03 | 1,633 -> 1,633 | 583 -> 583 | 575,233 -> 575,233 |

## Input frame representation

All six cases improve worker cycles by 8.6â€“45.8%. All cases allocate zero before and after. Each case uses the same optional index for controller and player. The frozen preceding frame and final frame produce identical 13-field values.

| Index | Cycles before -> after | Wall ns before -> after | Frames/s before -> after |
|---|---:|---:|---:|
| None | 55.03 -> 38.22 | 25.63 -> 17.74 | 39,014,168.35 -> 56,365,356.50 |
| Some(0) | 70.02 -> 38.28 | 32.21 -> 17.45 | 31,043,531.81 -> 57,306,750.61 |
| Some(1) | 70.05 -> 37.94 | 32.57 -> 17.33 | 30,701,770.82 -> 57,690,140.85 |
| Some(2) | 148.39 -> 109.74 | 67.71 -> 50.27 | 14,768,676.06 -> 19,891,340.64 |
| Some(18446744073709551614) | 193.78 -> 177.11 | 90.33 -> 81.61 | 11,070,083.28 -> 12,253,384.19 |
| Some(18446744073709551615) | 150.47 -> 120.62 | 69.44 -> 55.39 | 14,400,984.44 -> 18,052,999.83 |

## Complete compilation

Five complete loader/DTO workloads use three alternating process pairs and five samples per process: 30 processes and 150 measured compilations. Every complete DTO snapshot matches exactly. Warm setup/snapshot formatting are excluded equally; loading, source execution, extraction and DTO preparation are included. Native callback-unit extraction is additional loading work, measured separately above.

| Workload | Cycles before -> after | Compiles/s before -> after | Allocations before -> after | Reallocations before -> after | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 515,559,413.00 -> 520,170,224.00 | 4.20 -> 4.16 | 254,584 -> 254,585 | 13,078 -> 13,078 | 39,568,250 -> 39,568,306 |
| live_timed_inputs | 563,240,308.00 -> 540,759,766.00 | 3.84 -> 3.99 | 250,394 -> 250,394 | 14,343 -> 14,343 | 40,444,940 -> 40,444,940 |
| live_shared_inputs | 546,218,497.00 -> 543,445,792.00 | 3.96 -> 3.98 | 250,459 -> 250,459 | 14,342 -> 14,342 | 40,448,674 -> 40,448,674 |
| offline_state_boundaries | 954,429,694.00 -> 961,836,497.00 | 2.27 -> 2.25 | 293,920 -> 293,920 | 17,119 -> 17,119 | 43,717,377 -> 43,717,377 |
| live_state_boundaries | 834,209,110.00 -> 811,039,124.00 | 2.59 -> 2.66 | 272,023 -> 272,123 | 16,985 -> 16,985 | 43,057,365 -> 43,062,568 |

No general whole-compilation speedup is inferred from these loading fixtures. Default loading does not automatically extract the new unit. Allocation distributions and CPU changes are preserved even where effects are mixed.

## Validation and limits

- **38 focused checks pass:** 16 generic compiler checks, 9 real actor-input/update checks, 11 existing input checks, 1 method identity/weak-rooting check and 1 renderer check. No full suite or Song `--lib` harness ran.
- The timer differential covers **540 input/update steps** across three legal positive rate/hibernation profiles, including non-exact f32 rates, signed zero, repeated branches, NaN input levels and deadline overshoot. It compares scalar bits, actor states, handling and ordered writes, then replays after scene destruction. Unsupported queues, effects, globals and callback changes reject the unit.
- Production playback/composition produces **90 exact render-frame comparisons and 90 byte-identical OpenGL captures** across input, shared-state and timed fixtures. Native replay/rendering occurs after the reference scene is destroyed. Visible-content assertions exclude empty-frame comparisons.
- A live traversal-alias regression inserts an unrendered scalar table, builds the actual update plan, removes preparation-only callbacks and confirms that input-only copying remains available while the timing unit rejects the mutable traversal capture.
- Baseline/final ordinary scene audits match on **12 update/input/draw frame pairs** across timed/shared fixtures.
- Compiler regression checks cover small/indexed schema boundaries, binary/NUL field names, duplicate errors, captures, negative-zero/NaN bits, and execution after compiler, bindings, dump and loading VM are released.
- The six historical Song failures were absent in the earlier audit (831 passed, 0 failed, 41 ignored). They were not rerun, fixed or suppressed here.

Top-level scene validation rejects negative frame deltas, and ActorFrame loading rejects nonpositive rates. Differential inputs respect those existing host contracts. The native unit receives the actual parent delta; host validation and host side effects remain outside this snapshot.

The default gameplay scene still retains Lua. Remaining work includes whole-scene capture isolation, runtime integration, queued-command/tween timer lowering, dynamic lifecycle/asset behavior and a cross-theme gameplay matrix. This pass compiles authored input-driven countdown callbacks as a reusable unit; it does not claim all timers or a whole minigame are VM-free.

The SDK baseline restores the exact preceding program module/lowering; only its benchmark's prepared-API branch is replaced with the old ordinary call. The final binary includes both final ordinary/indexed and reusable paths. Saved normal Game baselines embed 2354; their production code matches the preceding 2355 commit except release metadata. Final measured binaries embed 2355; the 2356 bump is metadata only. Source/binary hashes and the staged source are in the evidence.

Reproduce with `scripts/benchmark_timed_input.py` and `scripts/benchmark_song_lua_pipeline.py --suite timed_inputs`. Build commands are in the receipts. Trace string values are interned losslessly: each `{"string":n}` indexes `trace_strings`.
