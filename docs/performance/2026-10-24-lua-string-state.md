# Native finite string state

Baseline: `eddfeba8f43cdeaea91d839d1976fbee75820895`, workspace 0.5.2356. Commit version: 0.5.2357.

Three retained changes:

1. Generic native byte cells support authored finite state transitions across input and update callbacks. Compiled literals and copies from other byte cells remain owned after Lua closes. A Group normalizes independently compiled constant pools into one shared immutable arena, rewriting stored constant IDs while preserving cell identities. External Programs/Machines retain their original pools, and repeated Program Arcs share rewritten code. Boolean/number groups bypass arena normalization. Game bindings retain the existing immutable byte-field specialization first and retry rejected units with mutable byte cells.
2. The Game binder transfers captured scalar-field keys from the consumed schema into final runtime metadata after lowering. This removes a key clone per cell and the temporary metadata vector's growth/shrink work while preserving cell order.
3. The timing binder collects non-rendered traversal identities during the lookup it already performs. Capture validation checks these cached integers instead of repeatedly locking Lua and reading every traversal Table's identity. Known rendered actors retain their opaque actor binding; unrendered timing tables still cannot become mutable scalar stores through an alias.

Generic cells, compiler verification and constant arenas belong to deadlib-lua-legacy. Actor inventories, fields, update order and method compatibility stay in deadsync-song-lua. No engine unsafe code or theme changes were added. Existing numeric operations and register clearing remain unchanged.

## Measurement

[Raw samples, exact traces/DTOs, stage sources, build hashes and test receipts](2026-10-24-lua-string-state.json).

Windows MSVC static CRT, Rust 1.99.1, release, `--locked -j 2 --config profile.release.lto=false`. Workers run sequentially on logical CPU 2 after builds finish. QueryThreadCycleTime supplies worker cycles; counted System allocations include Rust/mlua allocations and exclude native malloc. Other system activity is uncontrolled. Values are per-operation medians; throughput comes from measured wall time. Preparation and compilation include destruction; runtime rows reuse their initialized owners.

There are three process sets. Numeric before/after runtime order alternates; stage preparation order rotates feature/keys/cached, keys/cached/feature, cached/feature/keys. Each retains nine samples after warmup (27 per row). Native and ordinary scene runtime order also alternates within each process. Feature includes finite string support, keys additionally transfers schema keys, and cached additionally caches traversal identities.

## Input, finite status, timer and action

The authored fixture shares mode/previous/decoy byte fields and numeric deadlines between a real input callback, a child update and a parent update. Right arms or cancels into cooldown, Left resets to idle, and elapsed time expires armed/cooldown states. Input level changes the deadline. A static parent rate of 1.25, child rate of 0.5 and initial hibernation exercise actual scheduling. Copy-before-overwrite and an equal-valued independent decoy check byte-cell identity. Native execution uses the authored bytecode lowering; no fixed timeline or song-specific Rust rewrite is supplied.

| String-state input/update pair | Worker cycles | Wall us | Pairs/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| Ordinary scene | 1,374,650.77 | 631.31 | 1,584.01 | 119.50 / 0.00 / 7,080.00 |
| Native callback unit | 1,023.05 | 0.47 | 2,139,275.77 | 0.00 / 0.00 / 0.00 |

Native execution uses 99.93% fewer cycles and 1,350.5x throughput in this workload. Ordinary scene timing includes host work, readback and output draining, so this is not an isolated interpreter comparison. Native retained payload is **17,164 bytes**; the loading scene reports 17,669,081 current Lua allocator bytes after validation. The scopes differ: native payload excludes allocator/Arc bookkeeping; Lua bytes include pending garbage without forced collection and exclude Rust payload. Neither is RSS.

The baseline could not compile this string-state unit. The migration comparison is authored Lua against the native representation. Existing numeric native execution is checked separately:

| Numeric input/update pair | Worker cycles | Wall us | Pairs/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| baseline native | 1,057.06 | 0.48 | 2,072,315.16 | 0.00 / 0.00 / 0.00 |
| cached native | 1,102.22 | 0.50 | 1,989,895.06 | 0.00 / 0.00 / 0.00 |

The existing numeric native path measured 4.3% more cycles (45.2 cycles per pair) and remains allocation-free. No numeric runtime speedup is claimed, and this measurement does not isolate the cause of the regression.

## Preparation and removed work

Each preparation includes capture inspection, dump/lowering, schema work, Group construction and destruction. Dense workloads install 16 or 32 actual child update callbacks sharing one numeric state table plus an input callback. Their structure exercises capture validation without adding Lua behavior to native Rust.

| Fixture / stage | Worker cycles | Wall us | Units/s | Allocations / reallocations / requested bytes |
|---|---:|---:|---:|---:|
| numeric / feature | 102,431.05 | 47.04 | 21,259.99 | 119.00 / 30.00 / 31,781.00 |
| numeric / keys | 102,923.21 | 47.11 | 21,225.09 | 114.00 / 28.00 / 31,458.00 |
| numeric / cached | 102,820.32 | 47.14 | 21,211.37 | 114.00 / 28.00 / 31,458.00 |
| strings / feature | 135,028.52 | 62.17 | 16,084.65 | 198.00 / 43.00 / 49,913.00 |
| strings / keys | 140,989.31 | 64.80 | 15,432.84 | 192.00 / 41.00 / 49,554.00 |
| strings / cached | 131,955.52 | 60.45 | 16,543.02 | 192.00 / 41.00 / 49,554.00 |
| dense16 / feature | 385,568.88 | 176.82 | 5,655.38 | 339.00 / 121.00 / 89,998.00 |
| dense16 / keys | 382,293.55 | 175.38 | 5,701.81 | 336.00 / 120.00 / 89,888.00 |
| dense16 / cached | 362,471.66 | 166.59 | 6,002.63 | 336.00 / 120.00 / 89,888.00 |
| dense32 / feature | 750,942.08 | 344.46 | 2,903.11 | 612.00 / 221.00 / 169,566.00 |
| dense32 / keys | 751,134.16 | 344.43 | 2,903.31 | 609.00 / 220.00 / 169,456.00 |
| dense32 / cached | 677,745.70 | 310.44 | 3,221.22 | 609.00 / 220.00 / 169,456.00 |

Key transfer removes 3–6 allocations, 1–2 reallocations and 110–359 requested bytes per unit across these fixtures. Its CPU effects are mixed, including a slower string-state preparation row. Caching traversal identities uses 5.2% fewer cycles for 16 update callbacks and 9.8% fewer for 32, without additional allocations in these fixtures. The cached list stays inline through eight non-rendered identities and can allocate for larger inventories.

## Complete compilation

Five complete loader/DTO workloads use three alternating process pairs with five retained samples: 30 processes and 150 measured compilations. Every full DTO snapshot matches exactly. Loading, source execution, extraction and DTO preparation are included; warm setup and snapshot formatting are excluded equally. Native unit extraction is additional loading work measured above; default loading still retains Lua.

| Workload | Cycles before -> after | Wall us before -> after | Compiles/s before -> after | Allocations before -> after | Reallocations before -> after | Requested bytes before -> after |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 492,374,272.00 -> 529,847,094.00 | 226,761.10 -> 243,917.40 | 4.41 -> 4.10 | 254,584 -> 254,584 | 13,078 -> 13,078 | 39,568,256 -> 39,568,256 |
| live_string_states | 516,926,670.00 -> 506,347,424.00 | 237,760.70 -> 233,022.70 | 4.21 -> 4.29 | 250,514 -> 250,551 | 14,381 -> 14,380 | 40,462,678 -> 40,464,456 |
| live_timed_inputs | 521,873,978.00 -> 532,842,614.00 | 240,370.00 -> 246,136.80 | 4.16 -> 4.06 | 250,511 -> 250,548 | 14,376 -> 14,375 | 40,459,611 -> 40,461,389 |
| offline_state_boundaries | 887,657,562.00 -> 892,580,297.00 | 408,034.30 -> 410,439.40 | 2.45 -> 2.44 | 293,920 -> 293,920 | 17,119 -> 17,119 | 43,717,383 -> 43,717,383 |
| live_state_boundaries | 741,268,188.00 -> 726,051,776.00 | 341,637.10 -> 334,049.00 | 2.93 -> 2.99 | 272,058 -> 272,120 | 16,986 -> 16,985 | 43,059,340 -> 43,062,447 |

No general whole-compilation gain is inferred from these loading cases. The JSON retains every sample, throughput and allocation value, including regressions.

## Validation and limits

- **42 focused checks pass:** 18 SDK compiler checks, 11 actor input/update checks, 11 existing input checks, one method identity/weak-rooting check and one production renderer check. No full suite or Song `--lib` harness ran.
- The new gameplay differential compares 120 inputs and 120 updates against authored Lua: handling, ordered visibility actions, all actor state and byte/numeric scalar state. Native replay follows Scene destruction. SDK checks exercise binary NUL/FF/empty strings, distinct constant-pool orders, byte-valued action returns, branch joins, copy-before-overwrite, repeated/external Program sharing, invalid initial indices and transient/type-change rejection.
- Every interactive benchmark process checks 96 input/update pairs against Lua, proves loading VM destruction with a globally rooted finalizer and replays detached execution. Exact traces match across all measured stages and processes; state strings in the evidence are interned losslessly and round-trip checked.
- Production playback/composition yields **120 exact render frames and 120 byte-identical OpenGL captures** across actor-input, shared-state, timed and new string-state fixtures. Native replay/rendering occurs after the Lua reference Scene is destroyed; visible-content assertions exclude empty comparisons.
- Baseline/final ordinary Scene audits match on 12 update/input/draw frame pairs across numeric and string-state fixtures. Method identity, copied-setter targeting, metamethod guards and unrendered traversal-alias rejection remain covered by focused regressions.
- The six historical Song failures were absent at the earlier audit (831 passed, 0 failed, 41 ignored); they were not rerun or fixed here.

Finite byte cells accept compiled constants and other byte cells. Persisting a borrowed input string or changing a cell's type rejects compilation. The compatibility retry promotes all captured byte fields only after immutable specialization fails; this adds preparation work and mixed mutable-state/constant-field-name schemas can still reject. Unsupported bodies keep their live fallback.

The new native unit is an independent snapshot. Default Scene input/update dispatch, host/player updates, queued command timers, effects and dynamic lifecycle changes still use Lua. Whole-scene escape analysis and integration remain outstanding. Frame validation and later host activity are outside the snapshot. No theme code changed and no cross-theme matrix was run; render equality here uses identical assets and rendering conditions.

Stage binaries embed 0.5.2356. Saved baseline normal binaries embed 0.5.2355 from the preceding pass, whose final commit changed release metadata to 0.5.2356. The 0.5.2357 bump is metadata only. Build receipts, immutable source hashes and staged source variants are retained with the evidence.
