# Authored native movement and scoped string inspection

Baseline: `02bb26dce978e0b0b4ca1291f97e74a34a62f482`, workspace 0.5.2358. Commit version: 0.5.2359.

Three retained changes:

1. Native actor interactions compile authenticated `x(number)` and `y(number)` writes alongside visibility, real input, shared state and update timers. Setters narrow f64 to f32 exactly like the existing Lua property setters, including signed zero, NaN and infinity. Copied methods preserve their actual axis, captured target and return object, so method chains continue on the correct actor. Position writes reject active pose queues/effects and the loader's method-probe scope. Weak function-key metadata holds only the operation and an opaque actor identity, never a rooted actor.
2. Generic `LuaString::inspect_bytes` inspects immutable bytes using a caller's existing Lua context. It validates main-state identity and holds that context's lock while the borrowed string keeps its auxiliary reference rooted. The higher-ranked callback cannot return the byte borrow. It avoids cloning a Lua state/string reference and avoids stack pushes, stack checks and restoration. The Game method-name classifier now calls this safe API instead of manipulating the VM stack.
3. Actor binding resolves the three supported setters together. Receiver metatable checks, actor-kind decoding, class-chain validation, cycle detection and bound-method-table access are shared. Each method still keeps its own selection, and copied forwards resolve the actual forward name. Consecutive x/y targets also reuse the same pure position-eligibility result. This removes repeated locks, temporary handles and string copies during compilation.

Generic byte inspection stays in deadlib-lua-legacy. Actor method identities, schemas, float narrowing and pose compatibility stay in deadsync-song-lua. No theme implementation changed. The duplicated x/y Lua setter factories were replaced with the existing common scalar factory while preserving their probe names and behavior. The SDK factors its existing string-data read into one private helper; the Game classifier's unsafe block is removed.

## Measurement

[Raw samples, exact traces/DTOs, stage sources, source/binary hashes and focused-test receipts](2026-10-24-lua-movement.json).

Windows MSVC static CRT, Rust 1.99.1, release, `--locked -j 2 --config profile.release.lto=false`. Workers run sequentially on logical CPU 2 after builds finish. QueryThreadCycleTime supplies worker cycles. Counted System allocations include Rust/mlua allocations and exclude native malloc. Other activity is uncontrolled. Values are per-operation medians; throughput comes from measured wall time. Runtime reuses initialized owners, while preparation includes destruction.

Feature is movement support with separate lookups and the original classifier. Safe adds scoped string inspection. Batched adds shared method resolution and eligibility caching. Preparation uses three rotations of feature/safe/batched order, retaining nine samples per process (27 per row). Runtime baseline/final process order alternates across three pairs. The string classifier alternates before/after order within each process, retaining 13 samples across three process sets (39 per variant/key).

## Movement driven by input and elapsed time

The authored fixture resets x/y from actual input level and direction, arms/cancels a deadline, and advances y from a child update while active. It shares numeric state with a parent update, uses rates of 1.25/0.5 and initial hibernation, and emits ordered x/y/visibility writes. No song-specific Rust rewrite or fixed action timeline is provided. The preceding native binder could not lower these position calls.

| Movement input/update pair | Worker cycles | Wall us | Pairs/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| Ordinary Scene | 1,451,598.23 | 663.93 | 1,506.19 | 123.50 / 0.00 / 7,230.00 |
| Native unit | 1,141.37 | 0.53 | 1,899,814.47 | 0.00 / 0.00 / 0.00 |

The native unit uses 99.92% fewer cycles and 1,261.3x throughput in this workload. Ordinary Scene timing includes host work, output draining and state readback, so this is not an isolated interpreter comparison.

Native payload is **16,815 bytes**; current loading-VM allocator bytes are 17,730,730. Native payload excludes allocator/Arc bookkeeping; Lua bytes include pending garbage and exclude Rust payload. The scopes differ and neither is RSS.

## Scoped method-name classification

| Key | Cycles before -> after | Wall ns before -> after | Keys/s before -> after | Allocations / reallocations / bytes before -> after |
|---|---:|---:|---:|---:|
| b'x' | 82.14 -> 57.31 | 38.13 -> 26.07 | 26,222,791.29 -> 38,361,039.57 | 0/0/0 -> 0/0/0 |
| b'visible' | 83.97 -> 59.63 | 38.36 -> 27.12 | 26,068,416.87 -> 36,867,686.77 | 0/0/0 -> 0/0/0 |
| b'__songlua_private' | 80.33 -> 59.23 | 36.65 -> 26.94 | 27,283,930.06 -> 37,118,260.08 | 0/0/0 -> 0/0/0 |
| b'OnCommand' | 97.21 -> 64.55 | 44.24 -> 29.46 | 22,601,738.17 -> 33,949,440.53 | 0/0/0 -> 0/0/0 |
| b'SetUpdateFunction' | 97.12 -> 68.51 | 44.29 -> 31.23 | 22,579,933.85 -> 32,025,019.55 | 0/0/0 -> 0/0/0 |
| b'SetDrawFunction' | 100.85 -> 71.61 | 45.91 -> 32.58 | 21,781,441.11 -> 30,693,143.50 | 0/0/0 -> 0/0/0 |
| b'GetDrawFunction' | 101.28 -> 74.25 | 45.96 -> 33.85 | 21,758,300.13 -> 29,542,012.26 | 0/0/0 -> 0/0/0 |
| b'OnCommand\x00suffix' | 92.51 -> 61.33 | 42.10 -> 28.23 | 23,751,812.12 -> 35,424,864.86 | 0/0/0 -> 0/0/0 |
| b'longlonglonglonglonglonglong' (1024 bytes) | 175.99 -> 149.77 | 80.44 -> 68.30 | 12,431,899.23 -> 14,641,644.33 | 0/0/0 -> 0/0/0 |
| b'\xff\x00' | 703.47 -> 674.09 | 320.73 -> 307.67 | 3,117,911.24 -> 3,250,277.73 | 2/1/82 -> 2/1/82 |

Flags and exact UTF-8 errors match the frozen original, including embedded NUL, private/command filters, frame callbacks and long keys. The ordinary owned as_bytes view was measured and rejected because it was slower; its results/source are retained in the JSON.

## Native unit preparation

Preparation includes capture inspection, method authentication, schema construction, function dump/lowering, Group construction and destruction.

| Fixture / stage | Worker cycles | Wall us | Units/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| timed-input / feature | 164,450.09 | 75.18 | 13,301.19 | 132.00 / 29.00 / 32,104.00 |
| timed-input / safe | 163,187.96 | 74.59 | 13,406.51 | 132.00 / 29.00 / 32,104.00 |
| timed-input / batched | 124,343.32 | 56.78 | 17,612.17 | 126.00 / 29.00 / 32,056.00 |
| state-machine-input / feature | 195,150.94 | 89.10 | 11,223.84 | 200.00 / 39.00 / 49,352.00 |
| state-machine-input / safe | 194,020.84 | 88.78 | 11,263.64 | 200.00 / 39.00 / 49,352.00 |
| state-machine-input / batched | 154,454.25 | 70.71 | 14,142.24 | 194.00 / 39.00 / 49,304.00 |
| movement-input / feature | 173,293.52 | 79.05 | 12,650.85 | 134.00 / 32.00 / 37,251.00 |
| movement-input / safe | 176,687.21 | 80.74 | 12,384.98 | 134.00 / 32.00 / 37,251.00 |
| movement-input / batched | 134,776.43 | 61.47 | 16,266.98 | 128.00 / 32.00 / 37,203.00 |

## Existing native execution

| Fixture / stage | Worker cycles | Wall us | Pairs/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| timed-input / baseline | 1,027.19 | 0.47 | 2,139,275.77 | 0.00 / 0.00 / 0.00 |
| timed-input / batched | 1,078.35 | 0.49 | 2,028,794.08 | 0.00 / 0.00 / 0.00 |
| state-machine-input / baseline | 1,048.48 | 0.48 | 2,085,539.71 | 0.00 / 0.00 / 0.00 |
| state-machine-input / batched | 1,037.19 | 0.47 | 2,109,310.63 | 0.00 / 0.00 / 0.00 |

The wider actor schema and weak x/y metadata are additional preparation/loading work. The next comparison uses the preparation samples after identical full runtime sequences in the baseline/final processes, separate from the preparation-only stage experiment above.

| Existing unit preparation / stage | Worker cycles | Wall us | Units/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| timed-input / baseline | 101,721.10 | 46.40 | 21,553.18 | 114.00 / 28.00 / 31,458.00 |
| timed-input / batched | 139,180.16 | 63.49 | 15,750.36 | 126.00 / 29.00 / 32,056.00 |
| state-machine-input / baseline | 135,458.94 | 61.93 | 16,146.73 | 182.00 / 38.00 / 48,562.00 |
| state-machine-input / batched | 158,112.02 | 72.35 | 13,822.30 | 194.00 / 39.00 / 49,304.00 |

These existing units add 12 preparation allocations. Numeric-timer and string-state preparation measured +36.8% and +16.7% cycles respectively versus the preceding binder, despite the reduction from the initial movement feature stage. No universal preparation gain is claimed.

## Complete compilation

Five loader/DTO cases use three alternating process pairs and five samples: 30 processes and 150 measured compilations. Every full DTO snapshot matches exactly. Loading, source execution, extraction and DTO preparation are timed; warm setup/snapshot formatting are excluded equally. Native unit extraction is additional loading work, measured separately above.

| Workload | Cycles before -> after | Compiles/s before -> after | Allocations before -> after | Reallocations before -> after | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|
| offline_basic | 418,882,604.00 -> 452,741,794.00 | 5.22 -> 4.83 | 254,584 -> 254,584 | 13,078 -> 13,078 | 39,568,232 -> 39,579,944 |
| live_movement | 450,231,155.00 -> 458,981,077.00 | 4.85 -> 4.76 | 250,603 -> 251,826 | 14,412 -> 14,412 | 40,474,289 -> 40,554,061 |
| live_timed_inputs | 480,336,900.00 -> 472,782,373.00 | 4.55 -> 4.61 | 250,605 -> 251,828 | 14,407 -> 14,407 | 40,471,952 -> 40,551,724 |
| offline_state_boundaries | 790,210,974.00 -> 776,336,826.00 | 2.77 -> 2.82 | 293,920 -> 293,920 | 17,119 -> 17,119 | 43,717,359 -> 43,729,519 |
| live_state_boundaries | 703,569,055.00 -> 710,281,593.00 | 3.10 -> 3.08 | 272,023 -> 273,282 | 16,985 -> 16,985 | 43,057,347 -> 43,139,519 |

No general whole-loading gain is inferred from local improvements; complete wall times and every sample remain in the JSON.

## Validation and safety

- **62 focused checks pass:** 19 generic compiler tests, 11 string tests, 15 actor interaction tests, 11 existing input tests, two method-identity tests, two method-name/stack tests, one production renderer check and one compile-fail doctest. No full suite or Song `--lib` harness ran.
- The new authored movement differential covers 120 inputs and 120 updates, comparing handling, ordered x/y/visibility actions, all other actor fields, exact x/y bits and scalar-cell bits, then replays after Scene destruction. A separate receiver test uses ActorFrame update callbacks to distinguish traversal slots from actor inventory IDs.
- Copied bound and forward setters preserve actual targets, axes and returned actor identities. Numeric checks include signed zero, NaN payloads, infinities and f32::MAX. Replaced methods, custom assignment, coercions, missing/nil arguments, getters, active queues/effects and unrecognized calls retain fallback.
- Each interactive benchmark process compares 96 input/update pairs against Lua, proves VM destruction with a globally rooted finalizer and replays detached native execution. Exact traces match across stages/processes and interned evidence round-trips losslessly.
- Production playback/composition matches **150 render frames and 150 byte-identical OpenGL captures** across visibility, shared state, numeric timers, string state and movement. Native replay/rendering occurs after the Lua reference Scene is destroyed; visible-content assertions exclude empty comparisons. Baseline/final ordinary Scene audits match 18 update/input/draw frame pairs.
- Scoped inspection retains the caller's borrowed Lua context and lock, checks matching main-state identity, and borrows a string whose auxiliary ValueRef remains rooted. Strings are immutable/nonmoving under the existing byte-view contract. The callback's higher-ranked lifetime prevents a byte borrow from escaping. Tests cover reentrant allocation/GC, nested inspection, dropping another string alias, Rust unwinding, foreign-context rejection and subsequent VM use. The private raw string read is the existing operation factored from to_slice; no Game-side raw stack access remains in name classification.
- Only Lua 5.1 vendored/send was exercised; no cross-VM-version or Miri run is claimed. The six historical Song failures were absent at the earlier audit (831 passed, 0 failed, 41 ignored); they were not rerun or fixed here.

The returned interaction is an independent native snapshot. Default Scene ownership, host/player work, queues, effects, drawing and dynamic lifecycle changes still retain Lua. This is broader VM-free authored behavior, not a whole-scene escape or ownership proof. Only authenticated single-number x/y calls and boolean visibility calls are lowered; unsupported units keep their live fallback. No theme implementation changed and no cross-theme matrix ran; rendering comparisons use identical assets and conditions.

Saved normal baselines embed 0.5.2357 from the preceding pass, which then bumped metadata to 0.5.2358. New stage binaries embed 0.5.2358. The final 0.5.2359 bump is metadata only. Source/binary hashes, stage sources, standalone probe source and build receipts are included with the evidence.
