# Native actor input snapshots and compiler preparation

Baseline: `29b77f770e9bd1d4d4fe9184ed0ad64cdaf00149`, workspace 0.5.2353. Commit version: 0.5.2354.

Three retained changes:

1. `SongLuaLiveScene::compile_actor_inputs` creates an independent, VM-free input snapshot. Real input packets drive the original callback bytecode's branches and authenticated `visible(boolean)` actions. The owner updates native overlay/tracked states and exposes ordered action observations. Captured actor aliases share an actual state index. Copied bound methods keep their captured target and returned actor identity, including subsequent chained calls.
2. Native compiler action contracts borrow parameter slices from immutable bindings during lowering. This removes one temporary parameter-vector allocation in each measured rule, plus 48 requested bytes. The compiled result retains no reference to those bindings.
3. Bytecode instruction words are decoded from the borrowed dump while lowering, removing the intermediate instruction vector. Programs retain owned operations/constants after the dump and VM are destroyed. This removes another allocation and 64/116/80 requested bytes in the three rules. No unsafe code was added.

Method identity comes from a private weak-key registry populated at native function creation. Names and C-function status alone are insufficient. Compilation inspects raw fields, authenticated lookup/assignment adapters, class inheritance and the bound method's actual target. Authored replacements and functional metamethods reject the snapshot. Weak keys avoid rooting replaced functions; opaque target pointers are identities only and are never dereferenced. Registration runs only in live loading, before which `begin_live_runtime` is established by the loader. Offline compilation does not build this registry.

The common input schema is reused by predicates and actor snapshots. Actor schemas, field names, class rules and state ownership remain in deadsync-song-lua; bytecode decoding, action contracts and execution remain in deadlib-lua-legacy. No theme or renderer implementation changed. The full register clear retained by the preceding pass is unchanged.

## Measurements

[Raw samples, source/binary hashes, staged compiler builds, exact traces and focused-test receipts](2026-10-24-lua-actor-input.json).

Rust 1.99.1, Windows MSVC static CRT, release, `--locked -j 2 --config profile.release.lto=false`. Timed workers run sequentially on logical CPU 2 with the same counted System allocator. Windows `QueryThreadCycleTime` supplies worker-thread cycles; allocation counts cover the process, including mlua's Rust-backed Lua allocator. Native malloc and resident memory are outside this counter. Unrelated machine activity is uncontrolled.

The primary individual-operation evidence uses six fresh process sets, balancing every compiler stage position and adjacency. Compiler rows have 108 retained samples per stage/rule; actor execution and snapshot preparation have 54 samples per variant. Initial three-process experiments are retained as exploratory evidence because CPU effects varied. Complete compilation uses three alternating process pairs and five samples per process, with a reversed first order in the final run.

### Actual input-to-visibility interaction

The fixture has two real Quads, an alias of one captured actor, chained setters, alternate branches and input-dependent handling. The same 60 packets cover press/repeat/release, optional players, repeated inputs, differing levels and NaN. Visibility persists across inputs that do not write that actor.

| Per input | Ordinary `SongLuaLiveScene::input` | Native input snapshot |
|---|---:|---:|
| Wall time | 111.455 us | 0.320 us |
| Worker cycles | 243,398.9 | 696.7 |
| Inputs/second | 8,972.2 | 3,128,691.9 |
| Allocations | 26.75 | 0 |
| Reallocations | 0 | 0 |
| Requested bytes | 1,955 | 0 |

This is **99.71% fewer cycles** and **348.7x throughput** for the isolated interaction. The Lua measurement includes its normal actor-state readback and output draining; the native owner already owns those states. It is not a measurement of full scene updates, timers or drawing callbacks.

Snapshot preparation takes 23.682 us / 51,885 cycles, with 61 allocations, 11 reallocations and 23,729 requested bytes. It is additional loading work. The native owned payload is **12,779 bytes**, including code, scratch space, overlay/tracked states and target identities, excluding allocator/Arc bookkeeping. The source scene reports **17,414,367 current Lua allocator bytes** after the 60-input validation trace. That figure includes pending garbage, has no forced collection, excludes Rust allocations and is not RSS or a minimum live-heap estimate.

In every primary process, a globally rooted Lua proxy's finalizer writes a marker when its loading VM closes. The marker is absent before dropping the scene, present afterward, and native inputs continue afterward. The focused differential separately records all Lua results/states, drops the scene, then checks every native result/state against that sequence. Runtime storage contains no Lua, Function or Table handle.

### Compiler preparation

One operation includes binding/upvalue preparation, dump, lowering, machine construction and destruction. Stages are original / borrowed action contracts / both changes.

| Rule | Cycles, three stages | Wall us, three stages | Allocations | Requested bytes | Final throughput gain |
|---|---|---|---|---|---:|
| Input branch | 11,827.7 / 10,751.1 / 11,361.8 | 5.408 / 4.912 / 5.198 | 29 / 28 / 27 | 3,924 / 3,876 / 3,812 | +4.03% |
| Aliased table cells | 16,219.8 / 16,360.7 / 15,596.0 | 7.432 / 7.477 / 7.118 | 35 / 34 / 33 | 6,741 / 6,693 / 6,577 | +4.40% |
| Scalar cell | 12,947.2 / 12,689.5 / 11,982.5 | 5.921 / 5.811 / 5.481 | 32 / 31 / 30 | 4,356 / 4,308 / 4,228 | +8.02% |

Reallocations remain 8 / 10 / 8 respectively. Final cycle reductions versus original are 3.94%, 3.85% and 7.45%. Across these equally weighted rules, geometric-mean cycle reductions are 3.50% for action contracts, another 1.65% for instruction borrowing, and 5.09% combined. These aggregates describe these fixtures, not an application-wide estimate.

Individual CPU effects are mixed: contracts are 0.87% slower on the aliased-cell rule; instruction borrowing is 5.68% slower than contracts alone on the input-branch rule. Both consistently remove their allocation. The changes are retained for the uniform allocation/byte reduction and aggregate CPU improvement; no universal per-callback speedup is claimed.

### Complete compilation

All five complete DTO snapshots match across 30 normal process runs. Source loading/execution, host creation, extraction and returned DTO preparation are timed. Warm input/context setup, snapshot formatting and final output destruction are excluded equally.

| Workload | Cycles before -> after | Compiles/s before -> after | Allocations before -> after | Reallocations | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|
| Offline state | 980,758,918 -> 948,144,080 | 2.227 -> 2.301 | 293,611 -> 293,612 | 17,055 -> 17,055 | 43,687,445 -> 43,687,483 |
| Live state | 841,534,466 -> 871,258,708 | 2.595 -> 2.504 | 271,793 -> 271,812 | 16,921 -> 16,921 | 42,950,350 -> 43,032,547 |
| Offline basic | 559,113,253 -> 553,086,226 | 3.907 -> 3.948 | 254,585 -> 254,584 | 13,078 -> 13,078 | 39,568,324 -> 39,568,250 |
| Live wrappers | 1,646,226,521 -> 1,640,143,300 | 1.327 -> 1.332 | 720,750 -> 720,771 | 30,076 -> 30,076 | 120,785,255 -> 120,990,332 |
| Live actors | 1,334,600,829 -> 1,381,440,592 | 1.637 -> 1.581 | 604,264 -> 604,285 | 26,054 -> 26,054 | 98,086,340 -> 98,291,417 |

Cycle changes range from 3.33% fewer to 3.53% more. There is **no general whole-compilation CPU gain**. Live identity metadata adds loading bytes even when a caller does not extract a native snapshot. In the isolated pre-gate/final comparison, restricting that metadata to live mode removes 15 allocations and 82,038 requested bytes from offline state compilation and reduces cycles 3.35%. The initial larger wrapper regression did not reproduce in the final comparison; all exploratory results remain in the evidence.

A final copied-setter regression checks the actual target assignment adapter, rather than relying on the receiver proof. Unknown target assignment handlers reject the snapshot, and the retained Lua path preserves their side effects.

The saved normal baseline executable embeds workspace 0.5.2352; the preceding committed 0.5.2353 differs only in release metadata and an unrelated EOF cleanup, as recorded in the previous report. The final verification includes the target-assignment check and API documentation clarification. Only the workspace patch metadata changes afterward.

## Validation and limits

- **30 focused checks pass:** 4 real actor-input differentials/rejection checks, 11 existing input checks, 13 generic program checks, 1 exact method identity/weak-rooting check, and 1 production renderer check. No Song `--lib` harness or full suite ran.
- The production playback/composition path produces **30 exact backend-visible frame comparisons**, including floating-point bits, and **30 byte-identical OpenGL pixel captures** in a hidden 64x48 window. A visible-content assertion prevents an empty-frame comparison. The GPU check is explicitly opted into, consistent with the repository's other graphics tests.
- Normal pre-change/post-change scene audits match on both actor-input and predicate fixtures: 12 matched frame pairs covering update/input/draw outputs and actor states.
- Rejection covers authored instance/class/bound-method replacements, functional lookup/assignment metamethods, mutable scalar state, unknown calls, event writes and globals. The generic decoder check covers both byte orders, both size_t widths, released dump storage, negative zero and NaN payload bits, alongside existing truncation/mutation tests.
- The six historical Song failures mentioned in the task were absent in the preceding baseline audit (831 passed, 0 failed, 41 ignored). This pass does not rerun, fix or suppress them.

The snapshot freezes current actor bindings and states. Subsequent live lifecycle activity is independent. **Default `SongLuaLiveScene::input` still uses Lua for actor callbacks.** This pass adds a reusable, independently executable input-to-actor unit; it does not automatically replace the full gameplay scene. Whole-scene migration still needs capture/dependency isolation proof, shared scalar-state and timer lowering, lifecycle support and runtime integration. Unsupported interactions keep the existing live fallback. No cross-theme or whole-minigame pixel matrix is claimed.

Reproduce individual measurements with `scripts/benchmark_actor_input.py` and preserved/staged binaries; use six pairs to balance all orders. Complete comparisons use `scripts/benchmark_song_lua_pipeline.py`. Build commands, source hashes and focused test output are in the evidence. New Rust sources are formatted with child-module traversal disabled; Python syntax, diff and release-version checks are recorded separately.
