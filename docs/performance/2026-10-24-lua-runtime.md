# Native register, traversal and predicate execution

Baseline source: `c846a687e148949e1ae6e46500bee866a0e97673` (0.5.2360). Result: 0.5.2361. Preserved baseline executables embed 0.5.2359, with prior receipts matching every production source hash. New measured binaries embed 0.5.2360; the final bump changes metadata only. Raw cycles, throughput, allocations, reallocations, requested bytes, retained payload, source/binary hashes and test receipts are in the adjacent JSON.

## Changes and ownership

1. The generic compiler records whether any verified register read can include nil. Those callbacks retain full register clearing. When every read excludes nil, the interpreter skips clearing registers that its verified paths assign before reading. Type joins preserve NIL for implicit-entry-nil paths, including joins with explicitly assigned values; unknown joins still reject. This conservative rule also retains clearing for explicitly assigned nils and nullable input reads. The flag adds eight bytes per unique compiled program.
2. Native update traversal no longer clears the complete delta buffer before its Enter jobs. The frozen plan rejects duplicate slots and requires each parent/slot to exist before child/callback references. Every Enter overwrites both delta fields, including paused/hibernating paths. Rate, rounding, wake behavior and traversal order remain unchanged.
3. The interpreter applies a private generic result projection at its return points. New Machine/Group `run_truthy` methods return a boolean directly, avoiding intermediate Value/Datum result returns. Ordinary `run` preserves its existing value API. DeadSync uses the predicate API for closed screen inputs and actor input/update callbacks. Action order, static results and error propagation use the same executor.

Compiler analysis, register policy and predicate execution stay in deadlib-lua-legacy. Frozen traversal and gameplay callers stay in deadsync-song-lua. Noteskin/theme policy did not move. No new production unsafe code was added. The new standalone SDK benchmark uses the existing Windows thread-affinity pattern.

## Methods and prior regression check

Release, locked dependencies, LTO disabled, two build jobs; measured workers pinned to logical CPU 2. Measurements run serially after builds finish. Each gameplay step has three alternating before/after process pairs per fixture, ten samples of 1,536 input/update pairs per variant, first discarded; preparation has ten batches of 128, first discarded, after the same complete runtime history. Native preparation includes schema construction, compilation, group preparation and destruction.

A rotated repeat of the four previous saved binaries produced timer medians of baseline: 1,066 cycles, names: 1,020 cycles, signatures: 1,004 cycles, cached: 1,050 cycles. The earlier roughly 9% timer regression did not reproduce as a stable stage effect; no deterministic cause is claimed. All repeats still matched authored behavior and destroyed the loading VM.

The shared metrics helper was accidentally reformatted when rustfmt followed the new example module. It was restored to its exact previous hash, and final benchmark executables were rebuilt before measurement.

## Individual native execution changes

| Change / fixture | Cycles before → after | Change | Pairs/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| Register clearing / timed-input | 1,120 → 980 | -12.5% | 1,928,195 → 2,210,708 | 0 → 0 | 0 → 0 | 0 → 0 |
| Register clearing / state-machine-input | 949 → 961 | +1.3% | 2,304,230 → 2,281,640 | 0 → 0 | 0 → 0 | 0 → 0 |
| Register clearing / movement-input | 1,137 → 1,128 | -0.8% | 1,912,589 → 1,942,583 | 0 → 0 | 0 → 0 | 0 → 0 |
| Register clearing / update-only-input | 577 → 572 | -0.8% | 3,805,748 → 3,826,607 | 0 → 0 | 0 → 0 | 0 → 0 |
| Delta clearing / timed-input | 1,049 → 1,036 | -1.2% | 2,087,524 → 2,119,790 | 0 → 0 | 0 → 0 | 0 → 0 |
| Delta clearing / state-machine-input | 982 → 965 | -1.7% | 2,237,110 → 2,232,883 | 0 → 0 | 0 → 0 | 0 → 0 |
| Delta clearing / movement-input | 1,142 → 1,133 | -0.7% | 1,920,480 → 1,934,022 | 0 → 0 | 0 → 0 | 0 → 0 |
| Delta clearing / update-only-input | 556 → 528 | -4.9% | 3,950,617 → 4,135,703 | 0 → 0 | 0 → 0 | 0 → 0 |
| Direct truthiness / timed-input | 1,028 → 1,038 | +1.0% | 2,128,013 → 2,112,211 | 0 → 0 | 0 → 0 | 0 → 0 |
| Direct truthiness / state-machine-input | 938 → 957 | +2.1% | 2,339,680 → 2,288,098 | 0 → 0 | 0 → 0 | 0 → 0 |
| Direct truthiness / movement-input | 1,150 → 1,130 | -1.7% | 1,906,180 → 1,943,074 | 0 → 0 | 0 → 0 | 0 → 0 |
| Direct truthiness / update-only-input | 524 → 528 | +0.9% | 4,173,913 → 4,134,590 | 0 → 0 | 0 → 0 | 0 → 0 |

## Individual preparation changes

The runtime changes target execution. Preparation timing can move independently; unchanged allocation counts do not establish a preparation speedup. The new program flag increases requested preparation bytes and retained native payload.

| Change / fixture | Cycles before → after | Change | Preparations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| Register clearing / timed-input | 122,184 → 111,871 | -8.4% | 17,936 → 19,508 | 93 → 93 | 29 → 29 | 32,137 → 32,161 |
| Register clearing / state-machine-input | 141,421 → 141,809 | +0.3% | 15,361 → 15,408 | 161 → 161 | 33 → 33 | 46,228 → 46,252 |
| Register clearing / movement-input | 121,399 → 121,920 | +0.4% | 18,015 → 17,704 | 95 → 95 | 32 → 32 | 37,284 → 37,308 |
| Register clearing / update-only-input | 152,005 → 149,977 | -1.3% | 14,307 → 14,509 | 50 → 50 | 21 → 21 | 43,390 → 43,398 |
| Delta clearing / timed-input | 113,327 → 118,993 | +5.0% | 19,254 → 18,352 | 93 → 93 | 29 → 29 | 32,161 → 32,161 |
| Delta clearing / state-machine-input | 140,240 → 148,593 | +6.0% | 15,542 → 14,700 | 161 → 161 | 33 → 33 | 46,252 → 46,252 |
| Delta clearing / movement-input | 120,581 → 128,149 | +6.3% | 17,943 → 16,994 | 95 → 95 | 32 → 32 | 37,308 → 37,308 |
| Delta clearing / update-only-input | 152,237 → 154,897 | +1.7% | 14,339 → 14,084 | 50 → 50 | 21 → 21 | 43,398 → 43,398 |
| Direct truthiness / timed-input | 119,326 → 120,336 | +0.8% | 18,283 → 18,042 | 93 → 93 | 29 → 29 | 32,161 → 32,161 |
| Direct truthiness / state-machine-input | 157,543 → 145,616 | -7.6% | 13,849 → 14,889 | 161 → 161 | 33 → 33 | 46,252 → 46,252 |
| Direct truthiness / movement-input | 126,405 → 120,319 | -4.8% | 17,217 → 18,150 | 95 → 95 | 32 → 32 | 37,308 → 37,308 |
| Direct truthiness / update-only-input | 152,823 → 153,160 | +0.2% | 14,258 → 14,248 | 50 → 50 | 21 → 21 | 43,398 → 43,398 |

## Direct truthiness microbenchmark

A rejected first implementation returned a raw Value before computing truthiness. It improved paired SDK results but regressed update-only gameplay by about 6%; those receipts and complete third-stage results are retained in JSON. The retained implementation projects the result inside the interpreter return path.

Three processes, 14 alternating paired samples per fixture/API, first discarded, 1,048,576 calls per sample. These rows compare ordinary value-returning and predicate APIs within the final binary; the staged gameplay comparison separately measures the complete implementation change. Each paired digest is equal, numeric zero and empty strings retain Lua truthiness, and all timed calls outlive the loading VM.

| Fixture / API | Cycles before → after | Change | Calls/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| nullable_scalars / Machine | 53.59 → 42.58 | -20.6% | 40,789,354 → 51,360,753 | 0 → 0 | 0 → 0 | 0 → 0 |
| nullable_scalars / Group | 52.03 → 43.56 | -16.3% | 42,064,691 → 50,161,260 | 0 → 0 | 0 → 0 | 0 → 0 |
| bytes / Machine | 49.25 → 37.34 | -24.2% | 44,374,966 → 58,406,078 | 0 → 0 | 0 → 0 | 0 → 0 |
| bytes / Group | 46.70 → 40.41 | -13.5% | 46,788,243 → 54,090,738 | 0 → 0 | 0 → 0 | 0 → 0 |

The direct predicate API improves the paired SDK workloads. Its staged whole-gameplay differences are mixed and mostly flat; the microbenchmark result is not a universal gameplay speedup claim.

## Combined change

| Fixture / operation | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| timed-input / native pair | 1,044 → 1,023 | -2.0% | 2,097,501 → 2,141,065 | 0 → 0 | 0 → 0 | 0 → 0 |
| timed-input / preparation | 116,079 → 114,910 | -1.0% | 18,878 → 19,070 | 93 → 93 | 29 → 29 | 32,137 → 32,161 |
| state-machine-input / native pair | 962 → 971 | +0.9% | 2,277,580 → 2,258,159 | 0 → 0 | 0 → 0 | 0 → 0 |
| state-machine-input / preparation | 142,984 → 143,243 | +0.2% | 15,288 → 15,233 | 161 → 161 | 33 → 33 | 46,228 → 46,252 |
| movement-input / native pair | 1,141 → 1,239 | +8.6% | 1,919,520 → 1,768,770 | 0 → 0 | 0 → 0 | 0 → 0 |
| movement-input / preparation | 122,153 → 124,112 | +1.6% | 17,921 → 17,628 | 95 → 95 | 32 → 32 | 37,284 → 37,308 |
| update-only-input / native pair | 577 → 553 | -4.1% | 3,809,524 → 3,950,617 | 0 → 0 | 0 → 0 | 0 → 0 |
| update-only-input / preparation | 153,720 → 150,469 | -2.1% | 14,164 → 14,504 | 50 → 50 | 21 → 21 | 43,390 → 43,398 |

The combined movement fixture measured +8.6% native cycles. This regression remains an explicit tradeoff alongside the individual clear-removal and paired predicate gains; its cause is not established.

timed-input: native payload 14,883 → **14,907 bytes** (+24 bytes), excluding allocator/Arc bookkeeping.

state-machine-input: native payload 17,164 → **17,188 bytes** (+24 bytes), excluding allocator/Arc bookkeeping.

movement-input: native payload 16,815 → **16,839 bytes** (+24 bytes), excluding allocator/Arc bookkeeping.

update-only-input: native payload 21,731 → **21,739 bytes** (+8 bytes), excluding allocator/Arc bookkeeping.

## Complete compilation

Each step and the combined change uses six identical workloads, three alternating process pairs and five timed compiles per process: 144 processes, 720 timed compiles. Every returned DTO snapshot matches exactly, including across step comparisons. Full per-step metrics are in JSON. The combined comparison follows.

| Workload | Cycles before → after | Change | Compiles/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 421,144,989 → 420,996,608 | -0.0% | 5.17 → 5.17 | 254,584 → 254,584 | 13,078 → 13,078 | 39,579,938 → 39,579,938 |
| live_movement | 463,938,050 → 455,167,715 | -1.9% | 4.69 → 4.78 | 251,965 → 251,965 | 14,444 → 14,444 | 40,570,198 → 40,570,198 |
| live_timed_inputs | 464,466,393 → 455,105,369 | -2.0% | 4.68 → 4.78 | 251,967 → 251,967 | 14,439 → 14,439 | 40,567,861 → 40,567,861 |
| offline_state_boundaries | 790,256,648 → 779,259,682 | -1.4% | 2.76 → 2.79 | 293,920 → 293,920 | 17,119 → 17,119 | 43,729,513 → 43,729,513 |
| live_state_boundaries | 697,666,710 → 686,062,831 | -1.7% | 3.12 → 3.17 | 273,282 → 273,282 | 16,985 → 16,985 | 43,139,513 → 43,139,513 |
| live_update_only | 485,880,151 → 474,707,391 | -2.3% | 4.48 → 4.58 | 270,414 → 270,414 | 14,971 → 14,971 | 43,672,918 → 43,672,918 |

## Validation and limits

53 focused checks passed. SDK coverage includes implicit nils, nullable joins, register reuse after failed reads, grouped scratch state, primitive predicate value types and static objects, exact input errors and loading-VM destruction. Actor/input checks retain copied-setter identity, numeric bits, shared cells, hibernation/rates, alternate/repeated input and fallback coverage.

The OpenGL differential compares 180 RenderFrame structures and 180 pixel buffers across six authored fixtures. Four ordinary Scene fixtures match six update/input/draw frames each. All 96 staged gameplay processes compare Lua handling, actor states and scalar bits, then prove Lua VM finalization before detached replay. Ordered native action traces and trace hashes also match across every stage; the focused actor tests check authored expected action sequences. The new update-only fixture has an authored timer plus 16 empty ActorFrame branches; its elapsed-time decisions remain runtime computations.

This pass improves the existing compiled input/update/movement subset and the automatic closed-input predicate path. It does not expand the accepted authored language or prove whole-scene ownership isolation. Default Scene still retains Lua; queued commands, effect-owned poses, loops/general calls and existing unsupported cases retain fallback. The native program flag costs eight bytes per unique program. There is no cross-theme matrix. The full Song --lib harness and full suite were not run; the historical six failures were not re-audited or changed. Small per-fixture cycle movements should be read with the observed process variation, not as universal speedup claims.
