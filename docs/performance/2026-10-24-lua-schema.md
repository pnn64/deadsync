# Borrowed native schemas and retry bytecode reuse

Baseline source: `819cd9189eddfca017bfdb1f9579636f404847ac` (0.5.2359). Result: 0.5.2360. Preserved baseline executables embed 0.5.2358; their prior build receipts match every baseline production source hash. New measured executables embed 0.5.2359. The final version bump changes metadata only. See the adjacent JSON for raw rows, throughput, allocation/reallocation counts, requested bytes, source/binary hashes, test receipts and exact replay evidence.

## Changes and ownership

1. `deadlib-lua-legacy::program::Field::name` uses `Cow<'static, [u8]>`. Static input and actor method names borrow their bytes; inspected state keys remain owned and transfer into scalar metadata without another allocation. Binary keys and duplicate-field checks retain byte semantics. The wider key descriptor increases schema-vector requested bytes.
2. `Action::parameters` also permits static borrowed slices. The generic `Parameter::Receiver` constraint checks the identity of the object defining the action field; it is independent of the static return object. DeadSync shares visibility/numeric setter signatures across actors, including copied setters.
3. `program::dump` exposes the existing checked closure-dump operation. DeadSync lazily retains each demanded callback dump when captured byte fields can require a mutable-cell retry, then reuses those bytes. Callbacks after an earlier rejection remain unread. Numeric-only schemas allocate no cache.

Generic storage, receiver checking and reusable dump support belong to deadlib-lua-legacy. Actor fields, tags, movement eligibility and schema retry policy stay in deadsync-song-lua. No noteskin or theme policy moved. No new production unsafe code was added. Struct-literal schema callers now use `.into()` for owned names/signatures; repository examples were updated.

## Individual preparation changes

Normal production-library builds, release, locked dependencies, LTO disabled, two build jobs; all measured workers pinned to logical CPU 2. Each step has three alternating before/after process pairs per fixture, ten preparation batches of 128 operations, first batch discarded. Measures include schema construction, lowering, native group preparation and destruction. These preparation-only measurements follow identical 96-input/96-update validation sequences.

| Change | Fixture | Cycles before → after | Cycle change | Ops/s before → after | Allocations | Reallocations | Requested bytes |
|---|---|---:|---:|---:|---:|---:|---:|
| Borrowed field keys | timed-input | 121,850 → 124,287 | +2.0% | 17,945 → 17,443 | 126 → 102 | 29 → 29 | 32,056 → 32,281 |
| Borrowed field keys | state-machine-input | 150,261 → 149,846 | -0.3% | 14,457 → 14,588 | 194 → 170 | 39 → 39 | 49,304 → 49,529 |
| Borrowed field keys | movement-input | 131,532 → 131,403 | -0.1% | 16,592 → 16,616 | 128 → 104 | 32 → 32 | 37,203 → 37,428 |
| Static action signatures | timed-input | 125,976 → 123,493 | -2.0% | 17,301 → 17,632 | 102 → 93 | 29 → 29 | 32,281 → 32,137 |
| Static action signatures | state-machine-input | 158,001 → 151,738 | -4.0% | 13,821 → 14,357 | 170 → 161 | 39 → 39 | 49,529 → 49,385 |
| Static action signatures | movement-input | 142,912 → 128,747 | -9.9% | 15,226 → 17,012 | 104 → 95 | 32 → 32 | 37,428 → 37,284 |
| Reused retry dumps | timed-input | 118,940 → 116,681 | -1.9% | 18,279 → 18,681 | 93 → 93 | 29 → 29 | 32,137 → 32,137 |
| Reused retry dumps | state-machine-input | 146,775 → 142,555 | -2.9% | 14,858 → 15,328 | 161 → 161 | 39 → 33 | 49,385 → 46,228 |
| Reused retry dumps | movement-input | 124,297 → 125,070 | +0.6% | 17,425 → 17,426 | 95 → 95 | 32 → 32 | 37,284 → 37,284 |

Borrowed keys remove 24 allocations per native preparation, with roughly unchanged cycles and 225 additional requested bytes. Static signatures remove another nine allocations and 144 bytes. Retry reuse removes six reallocations and 3,157 bytes in the writable string-state fixture; the number of allocation calls is unchanged because the cache offsets one removed dump allocation. The numeric and movement fixtures do not use the cache. Small cycle differences on those unaffected paths are not evidence of a cache speedup.

## Combined runtime and preparation

Three alternating baseline/final process pairs per fixture. Runtime has ten samples of 1,536 input/update pairs per variant, first discarded. Preparation follows the complete runtime sequence and therefore has a different live state/GC history from the preparation-only measurements above. Runtime and retained memory are regression checks; the interpreter is unchanged.

| Fixture / operation | Cycles before → after | Cycle change | Ops/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| timed-input / native input/update pair | 978 → 1,070 | +9.3% | 2,238,414 → 2,050,461 | 0 → 0 | 0 → 0 | 0 → 0 |
| timed-input / preparation | 120,181 → 113,948 | -5.2% | 18,098 → 19,138 | 126 → 93 | 29 → 29 | 32,056 → 32,137 |
| state-machine-input / native input/update pair | 991 → 997 | +0.6% | 2,215,811 → 2,196,796 | 0 → 0 | 0 → 0 | 0 → 0 |
| state-machine-input / preparation | 148,780 → 141,271 | -5.0% | 14,614 → 15,385 | 194 → 161 | 39 → 33 | 49,304 → 46,228 |
| movement-input / native input/update pair | 1,181 → 1,141 | -3.3% | 1,857,765 → 1,924,571 | 0 → 0 | 0 → 0 | 0 → 0 |
| movement-input / preparation | 128,085 → 119,353 | -6.8% | 17,028 → 18,285 | 128 → 95 | 32 → 32 | 37,203 → 37,284 |

The timer fixture has a measured native-runtime regression of about 9.3% (roughly 40 ns per pair); string-state runtime is about 0.6% slower and movement about 3.3% faster. These binaries have identical runtime semantics and unchanged interpreter source, but this pass does not establish the cause of the timing differences. Preparation allocation savings are retained with that explicit runtime tradeoff.

timed-input: unchanged native payload **14,883 bytes**. Allocator/Arc bookkeeping is excluded. Lua allocator readings and their scope are retained in JSON.

state-machine-input: unchanged native payload **17,164 bytes**. Allocator/Arc bookkeeping is excluded. Lua allocator readings and their scope are retained in JSON.

movement-input: unchanged native payload **16,815 bytes**. Allocator/Arc bookkeeping is excluded. Lua allocator readings and their scope are retained in JSON.

## Complete compilation

Each of the three steps and the combined change has five identical workloads, three alternating process pairs, five timed samples per process: 120 processes and 600 timed compiles total. Every returned DTO snapshot matched exactly. Full per-step metrics are in JSON. The combined comparison is below; small whole-load cycle movements do not establish a general compilation speedup.

| Workload | Cycles before → after | Cycle change | Compiles/s before → after | Allocations | Reallocations | Requested bytes |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 422,531,572 → 411,496,208 | -2.6% | 5.15 → 5.29 | 254,584 → 254,584 | 13,078 → 13,078 | 39,579,932 → 39,579,932 |
| live_movement | 455,018,244 → 450,151,263 | -1.1% | 4.78 → 4.84 | 251,826 → 251,808 | 14,412 → 14,412 | 40,554,049 → 40,553,934 |
| live_timed_inputs | 454,544,324 → 453,764,887 | -0.2% | 4.79 → 4.80 | 251,828 → 251,810 | 14,407 → 14,407 | 40,551,712 → 40,551,597 |
| offline_state_boundaries | 788,940,070 → 786,020,947 | -0.4% | 2.76 → 2.77 | 293,920 → 293,920 | 17,119 → 17,119 | 43,729,507 → 43,729,507 |
| live_state_boundaries | 699,771,262 → 691,004,872 | -1.3% | 3.11 → 3.15 | 273,279 → 273,279 | 16,985 → 16,985 | 43,139,380 → 43,139,380 |

## Validation and limits

52 focused checks passed. The new SDK checks cover borrowed/owned signatures, distinct receiver/return identities, incorrect receivers, signed zero, native-closure rejection, dump reuse after VM destruction, readonly-schema rejection and mutable-byte-cell rebinding. Existing actor/input checks cover shared state, alternate/repeated inputs, timers, movement, copied setters, numeric bits and unsupported fallbacks.

The OpenGL differential compares 150 RenderFrame structures and 150 pixel buffers across five authored fixtures. Three ordinary Scene fixtures also match six update/input/draw frames each. Every preparation-stage pair and all 18 runtime processes compare exact authored traces; each proves loading-VM finalization before detached native replay. Four SDK examples using the schema API also pass targeted cargo check. Source hashes are frozen during builds and matched again after measurement.

This pass improves preparation of the existing VM-free input/update/movement subset; it adds no authored behavior to that subset. Default Scene still retains Lua, and the independent compiled snapshot is not proof of whole-scene ownership isolation. Unsupported getters, general loops/calls, queued/effect-owned movement and other existing fallbacks remain. There is no cross-theme matrix. The full Song `--lib` harness and full suite were not run. The historical six Song failures were not re-audited or changed. Borrowed names trade descriptor size for fewer allocations; temporary dump retention can increase preparation peak live bytes, which was not measured, while final native payload stays unchanged.
