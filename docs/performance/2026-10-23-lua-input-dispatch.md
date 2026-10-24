# Compiled input predicates and event boundaries, 2026-10-23

Baseline: `5f6cf6677`. Workspace patch: `0.5.2352` → `0.5.2353`.

Song input dispatch now uses the generic native compiler for eligible input-only predicates. These bodies process new runtime inputs and can stop later callbacks without running authored Lua. Actor actions, captured state, timers and the surrounding scene still retain Lua fallback. This is a production dispatch step, not a VM-free actor/minigame migration.

## Changes and ownership

1. `SongLuaCompiledInput` binds the actual screen input schema to `deadlib-lua-legacy::program`. Qualifying callbacks have no captures, global access, calls or event writes. Input reads, local variables, primitive expressions and branches compile to a Rust-owned machine. The screen evaluates a native prefix in its existing function-pointer order. A truthy result stops dispatch; a false prefix continues into Lua. Once Lua runs, every remaining callback uses its shared mutable event, including callbacks that also have compiled representations. Dynamic registration, deferred removal, duplicate identity and weak-reference rooting retain their existing behavior. Reentrant dispatch and debug hooks use the Lua path. Normal gameplay input remains independent of callback return values.
2. `Table::raw_set_many` writes pairs sequentially under one VM lock and table root. Each key/value conversion and write finishes before requesting the next pair. Fallback event construction uses two batches for its fresh parent/device tables. It keeps the original insertion order, nil writes, capacities, coercions and string formatting order. No intermediate value collection or new game-side unsafe access is introduced.
3. The ordered callback registry uses a sorted contiguous vector. The usual next callback advances by index. It first verifies that the prior identity still occupies its previous position; after additions, nested removals or screen replacement, a partition search repairs the position. This removes repeated BTreeMap range searches while retaining actual function-pointer order and changes made during dispatch. Registration/removal uses binary search and retains the original deferred-removal rules.

A focused differential check exposed a conservative rejection of `return nil`: Lua 5.1 can omit entry `LOADNIL`. Register typing now starts non-parameters as nil, matching the existing full runtime clear and vendored Lua semantics. The compiler accepts literal nil and default-nil locals, including alternating assignments and a large register bank. The previous fallback had preserved behavior for rejected callbacks. The proposed selective register initializer is **not shipped**: large early-return bodies improved, but small-body gains did not reproduce. Both rejected experiments remain in the evidence.

Generic batching, hook inspection and register typing live in deadlib. Input field names, player/controller enum strings, callback ordering and fallback policy live in deadsync-song-lua. No theme or rendering policy changes. Actor schemas are not inferred from method names, and mutable captures are not inferred from equal values.

The new unsafe code is limited to deadlib's existing FFI abstractions. Batch writes root the table, reserve the same stack space as a single raw write, keep iterator/conversion ownership outside protected Lua calls, and retain the table on both direct/protected paths. Invalid keys, allocation errors, reentrant conversions and unwinding restore the caller's stack while preserving completed writes. `Lua::has_hook` reads the current thread's hook pointer under its VM lock without touching the stack.

## Measurement

[Samples, source/binary hashes, receipts, focused checks, raw traversal and complete output traces](2026-10-23-lua-input-dispatch.json).

Rust 1.99.1, Windows MSVC static CRT, release, `--locked -j 2 --config profile.release.lto=false`. Workers run sequentially on logical CPU 2. Both variants use the same counted System allocator. Counts cover Rust and mlua's Rust-backed Lua allocator; native malloc is excluded. Bytes are requested allocation/reallocation bytes, not resident memory. Unrelated machine activity is uncontrolled.

Three fresh process pairs alternate order. Each worker discards a warmup pair and retains nine samples per variant. The dispatch/field comparisons use 27 samples per variant. Runtime results below are normalized to **one input event**, including optional player/controller values and press/repeat/release branches. The frozen dispatcher is the baseline source; its field-only helper copies the original construction verbatim. The current side compiles the production input module. Both use identical surrounding library/build settings.

| Workload | Cycles before → after | Operations/s before → after | Allocations before → after | Requested bytes before → after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| event_fields | 8,336.71 → 6,795.28 | 262,474.93 → 321,860.76 | 11.08 → 11.08 | 1,360.31 → 1,360.31 | +22.63% |
| dispatch_0_closed | 8,680.54 → 165.05 | 251,968.50 → 13,509,234.83 | 11.07 → 0.00 | 1,359.70 → 0.00 | +5261.48% |
| dispatch_1_closed | 9,267.88 → 357.54 | 235,088.85 → 6,183,574.88 | 12.07 → 0.00 | 1,376.08 → 0.00 | +2530.31% |
| dispatch_8_closed | 13,920.68 → 1,103.07 | 157,499.69 → 1,993,769.47 | 12.06 → 0.00 | 1,375.49 → 0.00 | +1165.89% |
| dispatch_64_closed | 53,381.37 → 8,291.70 | 40,950.83 → 264,572.14 | 12.03 → 0.00 | 1,374.14 → 0.00 | +546.07% |
| dispatch_8_mixed | 16,814.04 → 12,625.54 | 130,412.63 → 173,641.73 | 12.05 → 12.05 | 1,375.33 → 1,375.33 | +33.15% |

Mixed dispatch is sensitive to the loaded function-pointer order: the native prefix ends at the first Lua-only callback. The benchmark reports its measured result, not a general gain for all mixtures.

Registry rows isolate the storage/cursor change. Both sides already have the same native-prefix support and batched field construction; a frozen BTreeMap dispatcher compares with the production vector dispatcher. These callbacks read a global and remain Lua-only, so both sides execute all callbacks and construct the same event.

| Workload | Cycles before → after | Operations/s before → after | Allocations before → after | Requested bytes before → after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| registry_8_lua | 12,995.52 → 13,361.63 | 168,150.02 → 163,771.87 | 12.07 → 12.07 | 1,375.81 → 1,375.81 | -2.60% |
| registry_64_lua | 48,440.91 → 43,364.11 | 45,262.87 → 50,389.73 | 12.04 → 12.04 | 1,374.49 → 1,374.49 | +11.33% |
| registry_512_lua | 367,199.49 → 327,809.54 | 5,964.67 → 6,669.34 | 12.00 → 12.00 | 1,373.00 → 1,373.00 | +11.81% |

Eight Lua callbacks are 2.60% slower in the isolated registry comparison; 64 and 512 callbacks improve by 11.33% and 11.81% throughput. Vector insertion/removal shifts entries, trading loading-time work for contiguous dispatch. These samples do not establish a win for every registry size.

## Preparation and memory

Complete standalone preparation includes Lua creation, screen input API installation, source execution, callback registration and compilation where applicable. Each sample retains 16 prepared VMs/registries until timing ends; final destruction is excluded equally. There are 27 samples per variant. Additional loading work and retained native code are deliberate costs of this partial migration.

| Workload | Cycles before → after | Operations/s before → after | Allocations before → after | Requested bytes before → after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| prepare_0_closed | 223,451.00 → 227,168.75 | 9,786.53 → 9,636.81 | 534.00 → 534.00 | 41,852.00 → 41,852.00 | -1.53% |
| prepare_1_closed | 277,475.44 → 299,013.88 | 7,902.02 → 7,326.68 | 584.00 → 617.00 | 45,676.00 → 49,282.00 | -7.28% |
| prepare_8_closed | 505,631.94 → 644,397.12 | 4,327.60 → 3,393.43 | 814.00 → 1,062.00 | 62,937.00 → 90,673.00 | -21.59% |
| prepare_64_closed | 2,129,918.19 → 2,993,499.81 | 1,025.65 → 730.41 | 2,623.75 → 4,599.00 | 213,275.00 → 436,493.00 | -28.79% |
| prepare_8_mixed | 574,033.62 → 726,270.62 | 3,817.70 → 3,018.58 | 848.00 → 1,124.00 | 65,816.00 → 97,488.00 | -20.93% |

Preparing one compiled callback is 7.28% slower than the Lua-only baseline; 64 callbacks are 28.79% slower and request more allocations/bytes. Whole Song compilation is mixed (+0.67% offline, -0.71% live), without a general compiler speedup.

The detached predicate retains 822 native payload bytes. Its loaded Lua reference uses at least 30,016 Lua allocator bytes. All three workers verify loading-VM destruction before executing the detached predicate.

The standalone memory comparison uses Lua `used_memory()` after collection and the native artifact's owned code/constants/cells/register payload. These are different accounting boundaries. Lua excludes Rust-side handle/host allocation overhead; native excludes allocator bookkeeping and Arc counters. They are not resident-memory measurements. A VM drop marker is checked before evaluating the detached artifact against new inputs.

The production registry **retains its scene VM and original functions**, preserving Lua identity, weak references and later fallback. Native code is extra retained payload in that owner. No production gameplay retained-memory reduction or whole-scene VM release is claimed.

## Complete compilation and validation

Two fresh process pairs use three measured compilations per case/variant and one warm operation. All 32 complete compiled outputs match exactly across offline/live state-boundary fixtures.

| Workload | Cycles before → after | Operations/s before → after | Allocations before → after | Requested bytes before → after | Throughput |
| --- | ---: | ---: | ---: | ---: | ---: |
| offline_state_boundaries | 808,158,176.50 → 802,945,708.00 | 2.70 → 2.72 | 293,609.00 → 293,611.00 | 43,687,371.00 → 43,687,445.00 | +0.67% |
| live_state_boundaries | 713,547,087.50 → 719,127,658.50 | 3.06 → 3.04 | 271,752.00 → 271,793.00 | 42,948,336.00 → 42,950,350.00 | -0.71% |

Normal-library Song scene traces for the authored `closed-input.lua` fixture compare with the preceding executable. Each trace processes six actual frame/input/update/draw steps, including press/repeat/release, combo and life changes. Handled results, overlay states, tracked-pose observations and drained outputs match exactly. This checks the public live scene route; it is not a pixel, theme-switch, judgment/lifecycle or chart-corpus comparison.

Focused release checks pass: five batch/hook checks on Lua 5.1, five on Lua 5.4, twelve native program checks, and eleven Song input checks. Song uses the named `compiled_input` integration target against its normal library, without building the giant Song `--lib` harness. Checks cover unsupported-body fallback, native-vs-authored Lua truthiness, all bound input field families, repeated and alternate branches, nullable enum strings and large indices, VM destruction, entry nils, input errors, shared event mutation, ordered stopping before Lua actions, dynamic removal, reentrancy, hooks and weak function roots.

Each field-boundary worker also compares all parent/child raw traversal, float bits and nil-key `next` behavior for sixteen input packets. The packets cover all four optional player/controller combinations, NaN payloads and signed zero. The native compiler's existing numeric, alias/cell, method-receiver and malformed-dump checks remain in the focused program target.

No full Song/noteskin suite was run. The six historical Song failures were absent in the preceding baseline audit; this pass does not rerun, fix or suppress them. Miri and a whole-app FPS/pixel test were not run. An uncached dev-profile shader check was stopped; the focused validation uses the cached release profile. Targeted formatting, Python syntax, diff and version checks are recorded separately.

## Remaining work

This pass lowers input predicate bodies and connects them to real dispatch. It does not lower positive actor commands, captured/shared mutable state, timers or minigame logic. Those callbacks continue through Lua. The scene also reads/publishes actor state and runs its remaining callbacks through Lua.

A VM-free actor interaction still needs authenticated native method bindings and an actor/state owner whose changes remain visible to every remaining observer. That integration must preserve shared captures, event mutation, lifecycle/timing order and actual rendering, then release the migrated interaction's Lua roots. The long-term goal remains active.

## Reproduction

```powershell
cargo test -p deadlib-lua-legacy --no-default-features --features lua51,vendored,send --test raw_set_many --test program --release --locked -j 2 --config profile.release.lto=false
cargo test -p deadlib-lua-legacy --no-default-features --features lua54,vendored,send --test raw_set_many --release --locked -j 2 --config profile.release.lto=false
cargo test -p deadsync-song-lua --test compiled_input --release --locked -j 2 --config profile.release.lto=false
cargo build -p deadlib-lua-legacy --no-default-features --features lua51,vendored,send --example compiled_program --release --locked -j 2 --config profile.release.lto=false
cargo build -p deadsync-song-lua --example input_dispatch --example native_scene_audit --example compile_pipeline --release --locked -j 2 --config profile.release.lto=false
python scripts/benchmark_input_dispatch.py --dispatch <input_dispatch.exe> --audit-before <baseline-audit.exe> --audit-after <current-audit.exe> --out <paired> --pairs 3
python scripts/benchmark_song_lua_pipeline.py --suite state_boundaries --before <baseline-pipeline.exe> --after <current-pipeline.exe> --out <complete> --pairs 2 --samples 3
```

The evidence contains the frozen dispatchers, rejected initializer measurements and matching binary receipts. Preserve baseline/candidate executables before rebuilding. Runtime source stays unchanged during measurement; the patch version bump follows measurement and changes release metadata only. One redundant final blank line in the frozen tree fixture was removed afterward; its recorded hash difference is whitespace-only.
