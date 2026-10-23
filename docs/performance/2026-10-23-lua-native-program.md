# Native Lua callback compiler foundation, 2026-10-23

Baseline: `2f50bc13a`. Workspace patch: `0.5.2351` → `0.5.2352`.
This pass adds a reusable Lua 5.1 callback compiler and measures three improvements. It does **not** migrate production Song input dispatch. The VM-free input/action results below are controlled library interactions; actor binding, scene ownership and pixel equivalence remain integration work.

## Changes and ownership

1. `deadlib-lua::program` lowers callbacks into owned native instructions, direct field/cell slots and host action tags. Runtime inputs remain new data on every invocation. Branches, short-circuit expressions, ordered actions, aliased table fields and mutable scalar captures execute after the loading VM has been destroyed. Reusable register storage and borrowed action arguments eliminate runtime Lua calls, locks, handles, argument collections and event-table allocations. The library has no DeadSync actor names, input field names or theme policy.
2. `Function::for_each_upvalue_as` permits direct typed inspection under the existing batch lock/root/stack guard. `InspectionValue` copies string bytes directly into owned storage instead of allocating a temporary Lua reference handle and then copying. It preserves ordinary numeric bits and classifies unsupported values without decoding dormant caught-panic userdata. Tables/functions retain owning handles during inspection. The existing `for_each_upvalue` retains checked `Value` conversion semantics.
3. The native compiler passes register-type state to a single successor by ownership. It clones only when a control-flow fork needs another state; joins merge into existing storage. This removes 12–25 temporary vector allocations per measured callback compared with the initial copying implementation. Both compiler binaries use the same inspection API and harness; their recorded source manifests differ only in `program/compile.rs`.

The parser, compiler and native execution engine use safe Rust. The new inspection FFI follows deadlib's existing `FromLua` contract: the caller holds the VM lock and roots the stack value, actual strings alone reach `lua_tolstring`, and bytes are copied before return. Table/function conversion uses existing checked handle creation. The visitor excludes private native closure cells and restores its stack on errors/unwinding. No game-side unsafe code or theme code changes.

The compiler accepts finite forward control flow, fixed-arity bound actions, primitive comparisons/truthiness, numeric arithmetic and explicit fixed-type boolean/number cells. It rejects loops, nested closures, varargs, dynamic field keys, coercions, object comparisons, general Lua calls and unsupported instructions before running any callback body. Dump bytes are bounds-checked in Rust and are never loaded as untrusted Lua bytecode. The runtime returns the first result, matching an input handler's single-result contract.

Bindings are a host contract, not inferred proof. Hosts must establish actual object and method identity, absence of relevant metamethods, exact method receivers, private mutable state and stable action effects/results. Equal capture names or values do not establish shared-cell identity. Aliasing is explicit within a machine; independent machines do not share mutable cells. Actions cannot indirectly change inputs or native cells. The controlled factory in the benchmark establishes its schema; it is not an automatic Song binder.

## Measurements

[Raw samples, source/binary hashes, build receipts, checks and complete Song snapshots](2026-10-23-lua-native-program.json).

Rust 1.99.1, Windows MSVC static CRT, release, `--locked -j 2 --config profile.release.lto=false`. All workers run sequentially on logical CPU 2. Both variants use the same counted System allocator. Allocation counts include Rust and mlua's Rust-backed Lua allocator; native malloc is excluded. Bytes are requested allocation/reallocation bytes, not resident memory. Unrelated machine activity is not controlled.

Three fresh process pairs alternate binary order. Each process discards a warmup pair and retains nine samples per variant. The execution and inspection comparisons below use the optimized binary's 27 samples per variant. Each execution operation is **eight ordered input events**, including alternate buttons, press/repeat/release and stateful branches. Both paths emit equivalent host actions. Lua's prepared-event baseline already has its event tables; its new-event baseline creates three fields per event. Native inputs borrow a stack frame in both cases.

| Operation | Cycles before → after | Operations/s before → after | Allocations before → after | Requested bytes before → after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| Stateless rule, prepared events | 5,072.51 → 1,599.09 | 431,630 → 1,376,344 | 0 → 0 | 0 → 0 | +218.87% |
| Stateless rule, new events | 21,157.75 → 1,526.21 | 103,317 → 1,443,067 | 32 → 0 | 2,752 → 0 | +1296.73% |
| Aliased table state, prepared events | 5,778.16 → 2,648.58 | 380,160 → 826,606 | 0 → 0 | 0 → 0 | +117.44% |
| Aliased table state, new events | 22,427.59 → 2,784.91 | 96,473 → 789,636 | 32 → 0 | 2,752 → 0 | +718.51% |
| Scalar state, prepared events | 5,057.93 → 1,910.34 | 434,193 → 1,152,115 | 0 → 0 | 0 → 0 | +165.35% |
| Scalar state, new events | 21,467.27 → 1,915.48 | 102,090 → 1,149,529 | 32 → 0 | 2,752 → 0 | +1026.00% |
| Inspect eight owned strings | 4,739.13 → 1,587.95 | 461,324 → 1,369,487 | 16 → 8 | 160 → 32 | +196.86% |

Compiler-state comparisons pool the two identical native-lowering measurements in each process, yielding 54 samples per binary. They include schema/capture inspection, dump creation, lowering, machine preparation and destruction, starting with an already loaded callback. They compare the copying implementation with ownership transfer, not Lua execution with native execution.

| Native lowering operation | Cycles before → after | Operations/s before → after | Allocations before → after | Requested bytes before → after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| Stateless rule | 10,454.01 → 8,632.74 | 209,206 → 254,053 | 41 → 29 | 4,692 → 3,924 | +21.44% |
| Aliased table state | 16,635.70 → 12,431.01 | 131,412 → 175,944 | 60 → 35 | 8,341 → 6,741 | +33.89% |
| Scalar state | 12,306.36 → 9,835.06 | 177,821 → 221,389 | 48 → 32 | 5,380 → 4,356 | +24.50% |

## Cold preparation and retained memory

Cold preparation includes VM creation, source execution and returned artifact preparation. Lua returns a retained VM/callback; native additionally validates/binds/compiles the callback and destroys its loading VM before returning a machine. Batches retain 32 artifacts until timing ends, excluding final artifact destruction equally. Each variant has 27 samples.

| Rule | Cycles Lua → native | Preparations/s Lua → native | Allocations Lua → native | Requested bytes Lua → native | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| Stateless | 304,206.41 → 375,022.66 | 7,197 → 5,785 | 604 → 636 | 48,719 → 52,774 | -19.63% |
| Aliased table state | 306,518.03 → 360,172.03 | 7,092 → 6,073 | 607 → 645 | 51,080 → 57,952 | -14.37% |
| Scalar state | 305,523.41 → 364,431.72 | 7,167 → 5,971 | 605 → 640 | 49,635 → 54,122 | -16.69% |

Native execution trades additional loading work for lower execution cost and VM-free retained state. Ownership transfer alone changes complete native preparation throughput by +0.79%, +2.81% and -2.81%, respectively; a general cold-compilation CPU gain is not established.

| Rule | Retained Lua allocator bytes, lower bound | Native owned payload bytes |
| --- | ---: | ---: |
| Stateless | 30,391 | 1,220 |
| Aliased table state | 30,886 | 2,016 |
| Scalar state | 30,508 | 1,476 |

These are different accounting boundaries, not resident-memory measurements. Lua uses `used_memory()` after collection and excludes Rust-side handles/host closures and prepared event tables. Native counts the machine, code, constants, cells and scratch payload, excluding allocator bookkeeping and Arc counters. Native has no retained Lua handle or pointer. A loading-VM drop marker is asserted before every native execution comparison.

## Complete Song compilation and validation

Normal-library offline/live state-boundary fixtures compare with the preceding committed pass. The first two process pairs use three samples per case/variant; a reversed-order confirmation uses three pairs of five samples. All **104 complete outputs**, including warm operations, match exactly. The confirmed throughput changes are -1.70% offline and -0.91% live; the initial changes were -6.51% and -3.05%. Whole Song compilation has no demonstrated CPU gain. Both runs' samples, including the slower initial result, are retained in the JSON report.

| Confirmed Song workload | Cycles before → after | Compilations/s before → after | Allocations before → after | Requested bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| offline_state_boundaries | 769,750,721 ? 783,034,212 | 2.84 ? 2.79 | 293,304 ? 293,301 | 43,656,023 ? 43,655,917 |
| live_state_boundaries | 688,087,281 ? 694,518,625 | 3.18 ? 3.15 | 271,485 ? 271,444 | 42,918,817 ? 42,916,882 |

Focused checks: 20 pass on Lua 5.1 and 10 on Lua 5.4. Coverage includes authored-Lua differential actions/state/results, aliased table fields, distinct equal-valued captures, repeated/alternate inputs, method receiver identity, method chaining/tail calls, binary strings, nil/truthiness, signed zero and NaN payloads, numeric operators/branches, independent machines, malformed/truncated dump rejection, input contracts, unsupported-body rejection without prefix side effects, reentrant GC/rooting, private native closure exclusion, and dormant panic/stack behavior. The final Lua 5.1 checks include the ownership-transfer compiler. Lua 5.4 covers the inspection/visitor API; the native compiler is Lua 5.1-only.

`native_scene_audit` supplies a headless trace through the existing Song live scene with a real authored Quad input callback. It records update/input/draw output and actor states across six combo/life/input frames. It has no sampled unrelated tracked-pose changes. This is ownership reconnaissance, not a native-vs-Lua gameplay or rendering comparison.

Targeted formatting, Python syntax, diff and version checks pass. Formatting checks skip unchanged child modules; the shared benchmark helper has pre-existing formatting differences. No full Song/noteskin test suite or Song `--lib` harness was run. The six historical Song failures were absent in the preceding baseline audit; this pass does not rerun, fix or suppress them. Miri was not run on Lua FFI.

## Remaining integration

Production Song input/state/timers still use the live Lua scene. No gameplay VM or theme dependency is removed by this checkpoint. There is no pixel, theme-switch, judgment/lifecycle or chart-corpus equivalence claim. The controlled library interactions demonstrate reusable lowering and VM-free execution; they are not reported as migrated gameplay.

The next integration must authenticate actual actor methods and object identities, prove capture privacy/shared-cell identity and stable bindings, preserve callback/event ordering and mutation, connect host action tags to the existing gameplay actor state owner, and release each migrated interaction's Lua roots. Remaining authored callbacks must observe the same state. VM-free interaction tests then need identical actual Song inputs, actor-state transitions and rendering. Theme policy remains outside deadlib's compiler.

## Reproduction

```powershell
cargo test -p deadlib-lua --no-default-features --features lua51,vendored,send --test inspection --test program --test upvalues --release --locked -j 2 --config profile.release.lto=false
cargo test -p deadlib-lua --no-default-features --features lua54,vendored,send --test inspection --test upvalues --release --locked -j 2 --config profile.release.lto=false
cargo build -p deadlib-lua --no-default-features --features lua51,vendored,send --example compiled_program --release --locked -j 2 --config profile.release.lto=false
cargo build -p deadsync-song-lua --example native_scene_audit --example compile_pipeline --release --locked -j 2 --config profile.release.lto=false
python scripts/benchmark_native_lua.py --before-flow <copying-compiler.exe> --after-flow <ownership-compiler.exe> --out <paired> --pairs 3
python scripts/benchmark_song_lua_pipeline.py --suite state_boundaries --before <previous-pass.exe> --after <current.exe> --out <song> --pairs 3 --samples 5 --reverse-first
native_scene_audit.exe crates/deadsync-song-lua/tests/fixtures/native-input.lua <audit.json>
```

The JSON contains the copying compiler source for reproduction. Build/preserve that binary, restore the optimized compiler and build/preserve the second binary using identical settings. The harness itself provides the Lua/native and original/owned-inspection comparisons. Production code was unchanged during measurements. Only host-contract documentation and release metadata changed afterward; the report records those hash differences explicitly.
