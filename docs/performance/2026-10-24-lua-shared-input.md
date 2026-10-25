# Shared native input state and callback preparation

Baseline commit: `94dd65ca1a2d20935300a9c724572267a30bf5af`, workspace 0.5.2354. Commit version: 0.5.2355.

Three retained optimizations:

1. A generic `deadlib-lua-legacy::program::Group` owns one explicit scalar-cell store and one reusable register buffer for multiple programs. It supports shared captured state across authored callbacks, without Lua handles. The Song binder identifies actual captured raw tables and byte keys, preserving aliases while keeping distinct equal tables separate. Boolean/number fields become typed cells; byte strings remain immutable literals. Scalar upvalues, nested or unsupported values, unknown writes, type changes and metatables reject the snapshot before any callback body runs.
2. The Song binder gathers one complete schema, reuses input fields and actor action contracts, and proves each captured actor method once per snapshot. Per-callback upvalues still bind independently. Actor identities, method authentication, copied-setter targets and assignment behavior stay in deadsync-song-lua. Generic storage and execution stay in deadlib-lua-legacy.
3. Native input frames borrow static P1/P2 and controller 1/2 names. Other indices retain the original inline formatting and release overflow behavior. Absent values remain nil. All thirteen bound values are checked against the frozen preceding implementation.

The interpreter dispatch loop is shared by Machine and Group; register clearing remains in every call. No engine unsafe code or theme implementation changed. The gains come from removing repeated buffers, schema construction, method inspection and common-name formatting. Moving the interpreter loop alone is not counted as a gain.

## Measurement method

[Raw samples, hashes, exact trace data, staged source and build/test receipts](2026-10-24-lua-shared-input.json).

Windows MSVC static CRT, Rust 1.99.1, release, `--locked -j 2 --config profile.release.lto=false`. Every timed worker is pinned to logical CPU 2; workers and compilation run sequentially. QueryThreadCycleTime measures worker cycles. The counted System allocator includes mlua's Rust-backed Lua allocation; native malloc is excluded. Background system activity is uncontrolled.

Individual measurements use three process sets. Group/frame pairs alternate before/after inside each process, discarding one warmup and retaining thirteen samples. Actual scene input and preparation retain nine samples per process. Before/after actor processes alternate across sets. The schema control uses the same Group, enum code and benchmark source in both builds; only schema preparation changes. It uses three alternating process pairs, with nine retained samples each. Values below are medians per operation, not an application-wide estimate.

## Shared-state interaction

Two real Quads are controlled by two original Lua callbacks sharing a held flag and a numeric accumulator. Another captured table starts with the same flag value but must remain independent. Actual input packets toggle state, accumulate input levels, branch on thresholds/releases, emit visibility writes and stop dispatch through Lua truthiness. This is reusable lowering of the authored callbacks, without a hand-written gameplay implementation or fixed action timeline.

| One input | Worker cycles | Wall us | Inputs/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| Lua scene | 230,492.04 | 106.41 | 9,397.80 | 18.75 / 0.00 / 1,635.00 |
| Native snapshot | 685.22 | 0.31 | 3,196,936.27 | 0.00 / 0.00 / 0.00 |

This is 99.70% fewer cycles and 340.18x throughput for this input interaction. The Lua path includes normal state readback/output draining; the native owner already owns its states. Native payload is **13,796 bytes**. The source scene reports 17,419,417 current Lua allocator bytes after validation, including pending garbage without forced collection. These are different accounting scopes, not RSS.

| One snapshot preparation | Worker cycles | Wall us | Snapshots/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| Shared state | 70,507.52 | 32.49 | 30,779.59 | 93.00 / 17.00 / 27,570.00 |

Every primary actor process compares 60 identical Lua/native input packets, states and handled results, then proves actual loading-VM destruction with a globally rooted Lua finalizer before continuing detached native calls. The focused stateful test compares scalar bits, complete actor/tracked states, handling and ordered writes across 90 inputs, then destroys the scene before native replay. Its button/type sequences vary independently; finite threshold crossings precede NaN inputs. The generic group differential also records the actual Lua host-action sequence across both callbacks and continues after its VM-drop flag fires.

## Shared scratch in isolation

One immutable pure-input program is shared in both variants. Independent machines each allocate register storage; Group allocates one buffer. Code payload is counted once in both retained-memory figures. Preparation includes construction and destruction; execution is per callback.

| Callbacks | Prepare cycles before -> after | Allocations before -> after | Bytes before -> after | Retained payload before -> after | Execute cycles before -> after |
|---|---:|---:|---:|---:|---:|
| 1 | 369.98 -> 377.91 | 2 -> 2 | 72 -> 40 | 759 -> 751 | 157.55 -> 140.19 |
| 16 | 3,560.66 -> 750.24 | 17 -> 2 | 1,152 -> 160 | 1839 -> 871 | 138.10 -> 147.93 |
| 64 | 15,115.49 -> 2,464.87 | 65 -> 2 | 4,608 -> 544 | 5295 -> 1255 | 141.36 -> 141.21 |
| 256 | 60,141.93 -> 7,754.52 | 257 -> 2 | 18,432 -> 2,080 | 19119 -> 2791 | 153.56 -> 140.15 |

Execution allocates zero in both variants. Single-callback construction is slightly slower; execution effects are mixed, including a regression at sixteen callbacks. The consistent benefit is lower construction allocation/bytes and retained storage as groups grow. Full throughput and wall-time metrics are in the evidence.

## Schema reuse in isolation

Both variants already use Group and static common names. The control retains the preceding per-callback schema and method proofs; the final binder reuses them. Every control/final trace matches authored Lua and every other process exactly.

| Callbacks | Prepare cycles before -> after | Wall us before -> after | Snapshots/s before -> after | Allocations before -> after | Reallocations before -> after | Bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 47,367.42 -> 46,021.27 | 21.94 -> 21.39 | 45,571.06 -> 46,747.74 | 57 -> 58 | 11 -> 10 | 22,353 -> 22,345 |
| 16 | 654,096.30 -> 292,063.63 | 301.47 -> 134.31 | 3,317.06 -> 7,445.28 | 807 -> 433 | 162 -> 149 | 210,728 -> 154,481 |
| 64 | 2,365,813.91 -> 1,055,491.48 | 1,089.48 -> 486.61 | 917.87 -> 2,055.05 | 3,207 -> 1,633 | 644 -> 583 | 813,608 -> 577,217 |

Across all retained changes, the ordinary actor-only snapshots show these combined results. The 16/64-callback native runtime regressions are retained alongside the much lower preparation costs. The one-callback snapshot adds one allocation and 16 retained bytes.

| Callbacks | Native input cycles before -> after | Prepare cycles before -> after | Native payload bytes before -> after |
|---|---:|---:|---:|
| 1 | 507.16 -> 473.75 | 45,558.25 -> 46,585.45 | 12,272 -> 12,288 |
| 16 | 7,378.89 -> 7,540.02 | 669,274.38 -> 291,568.02 | 59,672 -> 58,248 |
| 64 | 30,148.73 -> 30,498.52 | 2,578,824.94 -> 1,085,456.70 | 211,352 -> 205,320 |


## Input enum names

Each case supplies the same optional index for controller and player. All cases allocate zero before and after. The common cases improve, while absent/uncommon cases regress. This fast path is retained for the normal P1/P2/controller 1/2 inputs; no frequency-weighted aggregate gain is claimed.

| Index | Cycles before -> after | Wall ns before -> after | Frames/s before -> after |
|---|---:|---:|---:|
| None | 36.18 -> 50.65 | 16.61 -> 23.19 | 60,196,564.71 -> 43,112,953.10 |
| Some(0) | 111.35 -> 63.72 | 50.99 -> 29.29 | 19,613,335.73 -> 34,136,889.26 |
| Some(1) | 105.97 -> 64.02 | 50.60 -> 29.36 | 19,764,762.65 -> 34,060,599.76 |
| Some(2) | 106.33 -> 137.88 | 48.72 -> 63.39 | 20,527,469.77 -> 15,775,461.57 |
| Some(18446744073709551614) | 157.38 -> 188.80 | 72.59 -> 86.16 | 13,775,302.15 -> 11,606,687.45 |
| Some(18446744073709551615) | 103.56 -> 142.58 | 47.30 -> 65.19 | 21,142,009.16 -> 15,339,028.67 |

## Complete compilation

Five full loader/DTO workloads use three alternating process pairs and five retained samples per process (30 processes, 150 measured compilations). Every complete DTO snapshot matches exactly. Source loading/execution, extraction and DTO preparation are timed; warm setup and snapshot formatting are excluded equally. Native input-snapshot extraction is additional work, measured above.

| Workload | Worker cycles before -> after | Compiles/s before -> after | Allocations before -> after | Reallocations before -> after | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 492,123,171.00 -> 481,867,910.00 | 4.41 -> 4.49 | 254,584 -> 254,585 | 13,078 -> 13,078 | 39,568,256 -> 39,568,312 |
| live_actor_inputs | 505,210,416.00 -> 500,272,765.00 | 4.29 -> 4.33 | 250,257 -> 250,257 | 14,292 -> 14,292 | 40,425,688 -> 40,425,688 |
| live_shared_inputs | 513,887,263.00 -> 502,216,006.00 | 4.21 -> 4.31 | 250,305 -> 250,305 | 14,310 -> 14,310 | 40,433,485 -> 40,433,485 |
| offline_state_boundaries | 907,690,669.00 -> 896,292,686.00 | 2.39 -> 2.42 | 293,766 -> 293,766 | 17,087 -> 17,087 | 43,702,436 -> 43,702,436 |
| live_state_boundaries | 799,882,587.00 -> 786,860,967.00 | 2.71 -> 2.75 | 271,966 -> 271,969 | 16,953 -> 16,953 | 43,047,500 -> 43,047,627 |

No general complete-compilation speedup is claimed. These loading workloads do not automatically extract the new actor snapshot. Lua-side allocation counts can vary slightly with table iteration and VM layout; their full sample distributions are preserved.

## Validation and remaining work

- **34 focused checks pass:** 15 generic program checks, 6 real actor-input checks, 11 existing input checks, 1 method-identity/weak-rooting check, and 1 production renderer check. No full suite or Song `--lib` harness ran.
- Two fixtures produce **60 exact render-frame comparisons and 60 byte-identical OpenGL captures**, using the production playback/composition path and a visible-content assertion.
- Baseline/final normal scene audits match on both fixtures across **12 update/input/draw frame pairs**, including actor states and drained outputs.
- Group tests preserve negative-zero and NaN payload layouts, reject mismatched types/bits, check invalid indices and scratch isolation across different register counts. Actor rejection retains the prior method/metamethod protections and adds unsupported state fields and typed-write failures.
- The six historical Song failures were absent in the preceding audit (831 passed, 0 failed, 41 ignored). They were not rerun, fixed or suppressed in this pass.

The new owner is an **independent input snapshot**. Later lifecycle activity in its source scene is independent. Default actor callback dispatch still uses Lua. This does not establish whole-scene capture isolation, timer/lifecycle lowering, minigame migration, runtime integration or a cross-theme pixel matrix. Unsupported interactions retain their live fallback. No theme code changed.

The saved Game baseline uses the preceding actor binder/frame with the already extracted common interpreter; Group is unused there. The generic comparison intentionally uses one identical dispatch loop for both storage designs. The schema control source and exact build hashes are preserved. The optional benchmark `prepare` mode was added after primary actor timings and affects measurement scheduling only. The frozen frame helper and added generic tests are formatted after timing; their measured sources are archived as well. Production Rust source is unchanged after final measurements. Measured binaries embed 0.5.2354; the final patch bump is metadata only.

Reproduce with `scripts/benchmark_shared_input.py`, `scripts/benchmark_shared_input_schema.py`, and `scripts/benchmark_song_lua_pipeline.py --suite shared_inputs`. Build commands and staged control source are in the JSON evidence. Trace strings are interned losslessly: each `{"string": n}` value indexes `trace_strings`.
