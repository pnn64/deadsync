# Callback ownership, result context and shared code

Baseline: `a5761fc7bb8ee7c50a1a048654451ed8fbca8354` (0.5.2361). Result: 0.5.2362. All new stage executables were built at 0.5.2361 with the same benchmark harness; the final bump changes metadata only. Adjacent JSON contains raw CPU cycles, throughput, allocation/reallocation counts, requested bytes, retained payloads, source/binary hashes, native end states and validation receipts.

## Retained changes

1. DeadSync keeps the rooted input snapshot and iterates caller-owned update functions by reference. It no longer clones/appends updates into a combined function vector. The existing function-pointer input order, traversal update order, per-callback bindings and retry order remain identical.
2. The generic interpreter passes its existing code/input references into the result projection. Ordinary value returns use the zero-capture `datum` function item rather than a closure carrying duplicate references. Predicate projections remain zero-capture. This removes intermediate context storage without changing the public APIs.
3. Generic groups of at least eight callbacks intern identical immutable code. Fingerprints select candidates, then complete operations, parameters, initial cells, register metadata and string-pool equality must match. Hash collisions can leave code unshared but cannot merge unequal code. Callback slots and executions remain separate, and mutable state/cell indices are not merged. Shared byte arenas are fingerprinted once.

Generic result conversion and immutable code sharing remain in deadlib-lua-legacy. Actor schemas, callback ordering, capture ownership and rendering fixtures remain in deadsync-song-lua. No noteskin/theme policy moved. No production unsafe code was added. This pass improves the existing native subset; it does not expand the accepted authored language or whole-scene ownership.

## Measurement and rejected layout

Release, locked dependencies, LTO disabled, two build jobs, logical CPU 2 affinity. Timing runs are serial and separate from compilation. Allocation metrics cover Rust GlobalAlloc across all process threads, including mlua; native malloc is excluded. A new `timed_input REPORT FIXTURE native ROUNDS` mode first performs the existing 96-input/96-update authored Lua differential and preparation measurement. It then drops the initial Scene/artifact, proves finalization of a separately loaded VM, replays its authored trace, and times only the detached native owner.

Each native sample has 393,216 real input/update pairs (49,152 for 16-callback fanout), ten samples with the first discarded, three alternating process pairs per fixture/stage. The end handling digest, complete actor states and scalar bits match across every stage/process. The old short paired Lua/native mode remains available. Preparation still follows the initial 96-step validation history and uses ten batches of 128, first discarded; it is not mixed with the long detached native timing.

A trial storing register count as u8 saved eight bytes per program but measured slower long native execution: timer +5.9%, movement +3.5%, callback fanout +8.8%. It was rejected and both SDK files restored to their exact baseline hashes before the retained changes. Trial binaries, receipts and measurements are retained in JSON.

A global flat binding-vector/range trial passed behavior checks and saved allocations, but timer preparation measured roughly 37-39% slower. A subsequent inline-capture trial reduced allocations but measured about 16% slower combined fanout execution; it was also rejected. The retained third change shares identical immutable code while preserving every callback execution. The rejected trial data and binaries are preserved in JSON.

The retained sharing step reduced the dense fanout unit payload by about 74% and combined native cycles by about 12%. Its preparation cost rose about 16%, with one additional allocation for the fingerprint index. Other runtime/loading results remain mixed; the tables preserve those regressions as explicit tradeoffs.

## Individual native and preparation measurements


### Native input/update

| Change / fixture | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| Borrow update callbacks / timed-input | 1,093 → 1,077 | -1.4% | 2,006,065 → 2,035,329 | 0 → 0 | 0 → 0 | 0 → 0 |
| Borrow update callbacks / state-machine-input | 1,073 → 1,046 | -2.5% | 2,042,948 → 2,091,770 | 0 → 0 | 0 → 0 | 0 → 0 |
| Borrow update callbacks / movement-input | 1,330 → 1,198 | -9.9% | 1,648,175 → 1,828,969 | 0 → 0 | 0 → 0 | 0 → 0 |
| Borrow update callbacks / update-only-input | 600 → 654 | +8.9% | 3,653,402 → 3,352,096 | 0 → 0 | 0 → 0 | 0 → 0 |
| Borrow update callbacks / fanout-input | 4,924 → 4,802 | -2.5% | 445,274 → 456,312 | 0 → 0 | 0 → 0 | 0 → 0 |
| Result conversion context / timed-input | 1,038 → 1,116 | +7.5% | 2,113,036 → 1,965,225 | 0 → 0 | 0 → 0 | 0 → 0 |
| Result conversion context / state-machine-input | 1,027 → 996 | -3.0% | 2,134,301 → 2,198,979 | 0 → 0 | 0 → 0 | 0 → 0 |
| Result conversion context / movement-input | 1,235 → 1,281 | +3.7% | 1,775,804 → 1,712,122 | 0 → 0 | 0 → 0 | 0 → 0 |
| Result conversion context / update-only-input | 595 → 566 | -4.9% | 3,683,515 → 3,875,126 | 0 → 0 | 0 → 0 | 0 → 0 |
| Result conversion context / fanout-input | 5,064 → 5,026 | -0.7% | 432,146 → 436,119 | 0 → 0 | 0 → 0 | 0 → 0 |
| Share immutable code / timed-input | 1,192 → 1,277 | +7.1% | 1,835,479 → 1,715,220 | 0 → 0 | 0 → 0 | 0 → 0 |
| Share immutable code / state-machine-input | 1,082 → 1,071 | -1.0% | 2,021,087 → 2,044,633 | 0 → 0 | 0 → 0 | 0 → 0 |
| Share immutable code / movement-input | 1,303 → 1,324 | +1.6% | 1,679,501 → 1,651,977 | 0 → 0 | 0 → 0 | 0 → 0 |
| Share immutable code / update-only-input | 604 → 597 | -1.2% | 3,623,403 → 3,667,030 | 0 → 0 | 0 → 0 | 0 → 0 |
| Share immutable code / fanout-input | 5,901 → 5,486 | -7.0% | 371,298 → 398,677 | 0 → 0 | 0 → 0 | 0 → 0 |

### Preparation

| Change / fixture | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| Borrow update callbacks / timed-input | 123,138 → 120,752 | -1.9% | 17,704 → 18,202 | 93 → 91 | 29 → 28 | 32,161 → 32,033 |
| Borrow update callbacks / state-machine-input | 174,933 → 159,228 | -9.0% | 12,500 → 13,758 | 161 → 159 | 33 → 32 | 46,252 → 46,124 |
| Borrow update callbacks / movement-input | 133,866 → 150,512 | +12.4% | 16,376 → 14,566 | 95 → 93 | 32 → 31 | 37,308 → 37,180 |
| Borrow update callbacks / update-only-input | 166,175 → 159,739 | -3.9% | 13,137 → 13,676 | 50 → 48 | 21 → 21 | 43,398 → 43,286 |
| Borrow update callbacks / fanout-input | 275,815 → 242,270 | -12.2% | 7,928 → 9,034 | 395 → 395 | 103 → 103 | 106,566 → 106,566 |
| Result conversion context / timed-input | 123,157 → 121,634 | -1.2% | 17,791 → 18,006 | 91 → 91 | 28 → 28 | 32,033 → 32,033 |
| Result conversion context / state-machine-input | 162,238 → 172,602 | +6.4% | 13,539 → 12,693 | 159 → 159 | 32 → 32 | 46,124 → 46,124 |
| Result conversion context / movement-input | 130,114 → 131,618 | +1.2% | 16,849 → 16,581 | 93 → 93 | 31 → 31 | 37,180 → 37,180 |
| Result conversion context / update-only-input | 158,642 → 161,110 | +1.6% | 13,800 → 13,606 | 48 → 48 | 21 → 21 | 43,286 → 43,286 |
| Result conversion context / fanout-input | 231,693 → 274,138 | +18.3% | 9,438 → 7,980 | 395 → 395 | 103 → 103 | 106,566 → 106,566 |
| Share immutable code / timed-input | 132,863 → 145,935 | +9.8% | 16,445 → 15,006 | 91 → 91 | 28 → 28 | 32,033 → 32,033 |
| Share immutable code / state-machine-input | 179,254 → 168,681 | -5.9% | 12,217 → 12,990 | 159 → 159 | 32 → 32 | 46,124 → 46,124 |
| Share immutable code / movement-input | 155,540 → 155,595 | +0.0% | 14,078 → 14,078 | 93 → 93 | 31 → 31 | 37,180 → 37,180 |
| Share immutable code / update-only-input | 173,041 → 181,573 | +4.9% | 12,616 → 12,069 | 48 → 48 | 21 → 21 | 43,286 → 43,286 |
| Share immutable code / fanout-input | 247,997 → 287,051 | +15.7% | 8,826 → 7,634 | 395 → 396 | 103 → 103 | 106,566 → 107,126 |

## Result-conversion microbenchmark

Frozen baseline and new SDK binaries use identical example/helper sources and build flags. Three alternating process pairs; each has 14 alternating ordinary/predicate samples per fixture/API, first discarded, 1,048,576 calls per sample. These comparisons measure implementation before/after, rather than comparing two APIs only inside the final binary. All paired digests are exact and all timed calls outlive the loading VM.

| Change / fixture | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| nullable_scalars / Machine / value | 52.76 → 48.58 | -7.9% | 41,563,482 → 45,144,897 | 0 → 0 | 0 → 0 | 0 → 0 |
| nullable_scalars / Machine / predicate | 41.47 → 40.81 | -1.6% | 52,881,465 → 53,743,363 | 0 → 0 | 0 → 0 | 0 → 0 |
| nullable_scalars / Group / value | 50.49 → 48.80 | -3.4% | 43,404,019 → 44,932,488 | 0 → 0 | 0 → 0 | 0 → 0 |
| nullable_scalars / Group / predicate | 43.99 → 43.30 | -1.6% | 49,853,612 → 50,652,175 | 0 → 0 | 0 → 0 | 0 → 0 |
| bytes / Machine / value | 51.46 → 45.72 | -11.1% | 42,611,010 → 47,954,852 | 0 → 0 | 0 → 0 | 0 → 0 |
| bytes / Machine / predicate | 39.50 → 39.32 | -0.5% | 55,516,635 → 55,770,573 | 0 → 0 | 0 → 0 | 0 → 0 |
| bytes / Group / value | 48.93 → 47.76 | -2.4% | 44,818,410 → 45,911,642 | 0 → 0 | 0 → 0 | 0 → 0 |
| bytes / Group / predicate | 42.49 → 42.53 | +0.1% | 51,606,959 → 51,560,774 | 0 → 0 | 0 → 0 | 0 → 0 |

## Callback-count sensitivity

The authored fanout source is also measured with 1, 4, 8, 16 and 24 callbacks, three alternating process pairs and preparation-only timing. Each process retains authored differential and VM-destruction checks. This measures the sharing threshold, repeated callback bodies, constructor overhead and requested-byte tradeoffs.

| Change / fixture | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| 1 callbacks | 61,064 → 57,447 | -5.9% | 35,633 → 38,166 | 50 → 50 | 10 → 10 | 20,198 → 20,198 |
| 4 callbacks | 95,465 → 92,001 | -3.6% | 22,933 → 23,829 | 119 → 119 | 29 → 29 | 36,390 → 36,390 |
| 8 callbacks | 154,094 → 158,199 | +2.7% | 14,198 → 13,794 | 211 → 212 | 54 → 54 | 58,022 → 58,310 |
| 16 callbacks | 243,227 → 257,597 | +5.9% | 8,987 → 8,482 | 395 → 396 | 103 → 103 | 101,526 → 102,086 |
| 24 callbacks | 341,268 → 364,247 | +6.7% | 6,411 → 6,000 | 579 → 580 | 151 → 151 | 144,654 → 145,214 |

## Combined change

| Change / fixture | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| timed-input / native pair | 1,204 → 1,220 | +1.3% | 1,817,913 → 1,795,493 | 0 → 0 | 0 → 0 | 0 → 0 |
| timed-input / preparation | 135,896 → 126,986 | -6.6% | 16,111 → 17,242 | 93 → 91 | 29 → 28 | 32,161 → 32,033 |
| state-machine-input / native pair | 1,062 → 1,000 | -5.9% | 2,062,580 → 2,193,034 | 0 → 0 | 0 → 0 | 0 → 0 |
| state-machine-input / preparation | 170,046 → 173,230 | +1.9% | 12,881 → 12,644 | 161 → 159 | 33 → 32 | 46,252 → 46,124 |
| movement-input / native pair | 1,245 → 1,235 | -0.9% | 1,757,663 → 1,773,171 | 0 → 0 | 0 → 0 | 0 → 0 |
| movement-input / preparation | 131,306 → 130,849 | -0.3% | 16,613 → 16,739 | 95 → 93 | 32 → 31 | 37,308 → 37,180 |
| update-only-input / native pair | 614 → 649 | +5.7% | 3,560,930 → 3,375,508 | 0 → 0 | 0 → 0 | 0 → 0 |
| update-only-input / preparation | 174,403 → 151,314 | -13.2% | 12,569 → 14,473 | 50 → 48 | 21 → 21 | 43,398 → 43,286 |
| fanout-input / native pair | 5,828 → 5,108 | -12.4% | 376,007 → 429,117 | 0 → 0 | 0 → 0 | 0 → 0 |
| fanout-input / preparation | 278,040 → 323,804 | +16.5% | 7,869 → 6,761 | 395 → 396 | 103 → 103 | 106,566 → 107,126 |

Runtime allocation/reallocation counts remain zero. Per-fixture cycle differences are reported independently; the structural gains do not establish a universal gameplay speedup.

timed-input: native payload 14,907 to **14,907 bytes** (+0.0%), excluding allocator/Arc bookkeeping.
state-machine-input: native payload 17,188 to **17,188 bytes** (+0.0%), excluding allocator/Arc bookkeeping.
movement-input: native payload 16,839 to **16,839 bytes** (+0.0%), excluding allocator/Arc bookkeeping.
update-only-input: native payload 21,739 to **21,739 bytes** (+0.0%), excluding allocator/Arc bookkeeping.
fanout-input: native payload 42,972 to **11,322 bytes** (-73.7%), excluding allocator/Arc bookkeeping.

## Complete compilation

Every retained step and the combined change uses seven workloads, three alternating process pairs and five timed compiles per process: 168 processes and 840 timed compiles. Every returned DTO snapshot matches exactly, also across step comparisons. Full individual metrics are in JSON; the combined comparison follows.

| Change / fixture | Cycles before → after | Change | Operations/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 431,213,677 → 414,128,453 | -4.0% | 5.07 → 5.28 | 254,585 → 254,585 | 13,078 → 13,078 | 39,579,976 → 39,579,976 |
| live_movement | 462,263,489 → 461,270,254 | -0.2% | 4.73 → 4.74 | 252,095 → 252,113 | 14,476 → 14,476 | 40,584,299 → 40,585,277 |
| live_timed_inputs | 462,458,179 → 461,493,043 | -0.2% | 4.73 → 4.74 | 252,097 → 252,115 | 14,471 → 14,471 | 40,581,962 → 40,582,940 |
| offline_state_boundaries | 778,569,127 → 804,472,762 | +3.3% | 2.81 → 2.72 | 293,920 → 293,920 | 17,119 → 17,119 | 43,729,495 → 43,729,495 |
| live_state_boundaries | 695,899,948 → 692,869,092 | -0.4% | 3.14 → 3.15 | 273,258 → 273,282 | 16,985 → 16,985 | 43,138,263 → 43,139,495 |
| live_update_only | 486,494,097 → 483,115,324 | -0.7% | 4.49 → 4.52 | 270,544 → 270,562 | 15,003 → 15,003 | 43,687,019 → 43,687,997 |
| live_fanout | 467,227,920 → 468,458,872 | +0.3% | 4.68 → 4.67 | 250,753 → 250,771 | 14,475 → 14,475 | 40,399,935 → 40,400,913 |

## Validation and limits

57 focused checks passed. The new mixed-capture test covers zero/one/two/three captured objects, shared aliases, ordered position/visibility writes, scalar bits and execution after Scene destruction. Its first version incorrectly assumed registration order; the corrected test derives the actual function-pointer dispatch order from a scalar log written by authored Lua. New SDK checks verify repeated shared-state execution, independent cells and machine state, signed zero/NaN payloads, binary constants, object IDs and mutable byte-arena indices after VM destruction. Existing SDK/game tests retain nil, error, malformed-chunk, receiver identity, rate/hibernation and fallback coverage.

The production OpenGL differential compares 210 RenderFrame structures and 210 pixel buffers across seven authored fixtures. Five ordinary Scene fixtures match six update/input/draw frames each. All 120 retained-stage gameplay processes compare initial authored handling/states/scalar bits and ordered native traces, prove VM finalization before timing, and match complete long-run end states. The native rules still evaluate every actual input/delta; no action timeline is precomputed.

The default Scene still retains Lua. Independent compiled snapshots are not whole-scene ownership proof. General loops/calls, effect-owned movement, queued behavior and other unsupported cases retain fallback; there is no cross-theme matrix. The full Song --lib harness/full suite were not run, and the historical six failures were not re-audited. Allocation/requested-byte gains and regressions are separate from native cycle results.
