# Indexed debounce heap and gamepad formatting — 2026-11-05

Baseline: `bb638e5f97753a9154ac7ea583c63ed2df0060b5`. Three separately measured routines in `deadsync-input`:

1. `sift_up` caches the moving slot and its `(deadline, slot)` key. It moves parents into the open position and writes the moving slot once at its final position. This replaces repeated pairwise swaps and duplicate metadata updates. A no-movement call still writes nothing, and root calls return before loading a key.
2. `sift_down` independently uses the same movement strategy while selecting the smaller child with the original deadline/slot ordering. The moving key is loaded once; each chosen child moves up once. Leaf/no-movement calls retain their early exit. The now-unused swap helper is removed; generic removal and heap repair remain unchanged.
3. `write_gamepad_code_binding_token` emits the eight uppercase code digits through safe nibble extraction into an eight-byte stack array, avoiding general integer formatting for that fixed-width field. The header, code, closing bracket, device formatting and UUID append order retain the previous capacity-growth behavior. Decimal-device formatting and the existing UUID character writes remain unchanged.

Production changes are **+70/−17 lines** (net +53), including the removed private swap helper and excluding test registrations/visibility. For a movement of `k > 0` levels, the old code writes two heap entries and two slot indices per level; the new code writes `k + 1` of each in total. This is a source-level write count, not a measured hardware-store or cache-traffic counter. Struct layout and collection capacities are unchanged. No production unsafe, dependency, public API or persistent cache is added. The formatter's eight-byte source-level stack array is outside heap counters; total stack/binary size is not measured. One test-only visibility line reuses the existing CPU-clock helper and allocator.

## Measurement method and scope

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes run twelve alternating frozen/current rounds pinned to CPU 2; the first round is discarded and the median of eleven retained. Windows `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns below are independent process medians; throughput is the median of five within-process ratios. All raw nanoseconds, cycles and six allocation counters are retained. Identical-source baseline controls expose code-generation/context bias. Our builds/benchmarks run sequentially; unrelated user activity is not controlled.

Fixture setup is outside timing. The counted-System allocator remains installed with counting disabled during CPU measurements, retaining its inactive TLS check. Heap/event fixtures reuse prepared vectors. Owned token results are materialized and destroyed inside timing; append cases reuse a 4,096-byte String. No hardware, input backend or device thread is instantiated.

**Units are cycles per whole fixture**, not per heap comparison or physical input:

- `heap/up/N`: update all N slots to successively earlier deadlines, exercising upward movement. `heap/down/N`: postpone the root N times. These paths isolate their respective changed routine during timing.
- `heap/drain/N`: remove all roots, then refill in reverse slot order. `heap/cancel/N`: cancel even slots followed by odd slots, then refill. These caller fixtures include both removal and refill work and can benefit from both changed movement routines. Sizes are 1/4/8/32/128/512. Repetitions are `max(50000 / N, 128)`; counters remain bounded below the fixture's one-second decreasing-deadline range.
- `event/N`: N presses, N delayed releases, release draining and expired-state cleanup per cycle at a 20 ms debounce window. Sizes are 1/4/8/32/128, with `max(10000 / N, 64)` repetitions. The clock callback returns prepared timestamps, logging is disabled, and emitted edges are consumed with a black box. This includes real debounce state transitions, callbacks and heap maintenance, but excludes hardware delivery and OS-clock cost.
- `token/*` and `append/*`: code zero/max, maximum device ID, UUID, and combined device/UUID fixtures, with 20,000 owned or 30,000 reused-buffer repetitions. All code fields remain eight uppercase hex digits.

These are in-memory kernel/caller measurements. No real-device latency, gameplay throughput or whole-application speedup is inferred.

| Workload | Frozen cycles/fixture | Current cycles/fixture | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `append/device` | 155.35 | 115.33 | 1.341× | 1.000× |
| `append/full` | 217.70 | 198.46 | 1.100× | 1.016× |
| `append/max` | 73.57 | 41.41 | 1.677× | 0.965× |
| `append/uuid` | 151.13 | 109.34 | 1.402× | 1.018× |
| `append/zero` | 102.21 | 39.98 | 2.612× | 0.956× |
| `event/1` | 390.75 | 405.31 | 0.945× | 0.909× |
| `event/128` | 63412.99 | 63942.05 | 1.004× | 0.913× |
| `event/32` | 13499.95 | 13609.71 | 0.989× | 0.920× |
| `event/4` | 1427.98 | 1535.27 | 0.938× | 0.900× |
| `event/8` | 2961.85 | 3181.70 | 0.931× | 0.886× |
| `heap/cancel/1` | 27.58 | 30.38 | 0.908× | 1.025× |
| `heap/cancel/128` | 12946.56 | 10803.34 | 1.207× | 0.998× |
| `heap/cancel/32` | 2264.14 | 1980.70 | 1.145× | 0.978× |
| `heap/cancel/4` | 139.83 | 149.45 | 0.936× | 0.934× |
| `heap/cancel/512` | 72373.26 | 59307.88 | 1.235× | 1.006× |
| `heap/cancel/8` | 361.38 | 369.11 | 0.976× | 1.030× |
| `heap/down/1` | 14.94 | 16.32 | 0.925× | 1.055× |
| `heap/down/128` | 18243.83 | 17113.12 | 1.074× | 0.970× |
| `heap/down/32` | 2910.55 | 2654.52 | 1.092× | 1.001× |
| `heap/down/4` | 104.87 | 100.79 | 1.040× | 1.009× |
| `heap/down/512` | 107796.80 | 94494.76 | 1.147× | 0.995× |
| `heap/down/8` | 293.99 | 269.37 | 1.087× | 0.959× |
| `heap/drain/1` | 27.80 | 30.75 | 0.928× | 1.013× |
| `heap/drain/128` | 17118.75 | 13306.20 | 1.284× | 1.007× |
| `heap/drain/32` | 2865.73 | 2309.25 | 1.243× | 0.993× |
| `heap/drain/4` | 148.36 | 142.68 | 1.042× | 0.987× |
| `heap/drain/512` | 111315.66 | 100544.71 | 1.108× | 1.006× |
| `heap/drain/8` | 438.02 | 382.11 | 1.129× | 0.964× |
| `heap/up/1` | 15.16 | 15.16 | 1.003× | 1.051× |
| `heap/up/128` | 8478.89 | 5891.04 | 1.434× | 0.968× |
| `heap/up/32` | 1538.61 | 1150.62 | 1.337× | 0.996× |
| `heap/up/4` | 102.25 | 90.28 | 1.131× | 1.016× |
| `heap/up/512` | 43493.58 | 29620.49 | 1.462× | 1.020× |
| `heap/up/8` | 252.30 | 210.12 | 1.196× | 0.991× |
| `token/device` | 353.45 | 299.47 | 1.182× | 1.001× |
| `token/full` | 407.94 | 376.32 | 1.088× | 0.977× |
| `token/max` | 207.66 | 180.88 | 1.166× | 1.065× |
| `token/uuid` | 284.95 | 246.07 | 1.178× | 0.993× |
| `token/zero` | 246.27 | 171.81 | 1.429× | 1.063× |

The complete event-cycle controls already penalize the unchanged actual implementation relative to the frozen copy. The optimized raw event ratios also stay near/below parity. These comparisons do **not establish an end-to-end debounce-cycle speedup**; the direct movement kernels and heap drain/cancel results provide the narrower evidence for retaining those routines. Code-generation context limits attribution, particularly for small/no-work fixtures. The report does not divide by the control ratio and present that as a causal improvement.

Observed raw paired medians below 0.970×:

- `event/1`: 390.75 → 405.31 cycles (0.945×); control 0.909×.
- `event/4`: 1427.98 → 1535.27 cycles (0.938×); control 0.900×.
- `event/8`: 2961.85 → 3181.70 cycles (0.931×); control 0.886×.
- `heap/cancel/1`: 27.58 → 30.38 cycles (0.908×); control 1.025×.
- `heap/cancel/4`: 139.83 → 149.45 cycles (0.936×); control 0.934×.
- `heap/down/1`: 14.94 → 16.32 cycles (0.925×); control 1.055×.
- `heap/drain/1`: 27.80 → 30.75 cycles (0.928×); control 1.013×.

All process ranges (minimum–maximum):

- `append/device`: 1.298×–1.376×; control 0.980×–1.141×.
- `append/full`: 1.079×–1.124×; control 0.969×–1.115×.
- `append/max`: 1.361×–1.796×; control 0.940×–1.035×.
- `append/uuid`: 1.188×–1.436×; control 0.968×–1.076×.
- `append/zero`: 1.934×–2.619×; control 0.924×–0.984×.
- `event/1`: 0.914×–0.981×; control 0.872×–1.002×.
- `event/128`: 0.936×–1.017×; control 0.865×–0.956×.
- `event/32`: 0.969×–0.994×; control 0.902×–0.966×.
- `event/4`: 0.919×–0.963×; control 0.887×–0.986×.
- `event/8`: 0.902×–0.982×; control 0.857×–0.919×.
- `heap/cancel/1`: 0.905×–0.930×; control 0.934×–1.038×.
- `heap/cancel/128`: 1.150×–1.218×; control 0.938×–1.002×.
- `heap/cancel/32`: 1.125×–1.166×; control 0.928×–0.986×.
- `heap/cancel/4`: 0.787×–1.060×; control 0.803×–1.013×.
- `heap/cancel/512`: 1.209×–1.306×; control 0.965×–1.015×.
- `heap/cancel/8`: 0.947×–1.032×; control 0.836×–1.097×.
- `heap/down/1`: 0.860×–0.992×; control 1.000×–1.093×.
- `heap/down/128`: 1.056×–1.088×; control 0.966×–1.067×.
- `heap/down/32`: 1.076×–1.108×; control 0.942×–1.012×.
- `heap/down/4`: 1.038×–1.085×; control 0.955×–1.060×.
- `heap/down/512`: 1.120×–1.169×; control 0.972×–1.028×.
- `heap/down/8`: 1.046×–1.112×; control 0.931×–0.989×.
- `heap/drain/1`: 0.852×–0.932×; control 0.932×–1.097×.
- `heap/drain/128`: 1.268×–1.328×; control 0.937×–1.093×.
- `heap/drain/32`: 1.213×–1.261×; control 0.968×–1.037×.
- `heap/drain/4`: 1.037×–1.045×; control 0.930×–1.284×.
- `heap/drain/512`: 1.084×–1.146×; control 0.993×–1.047×.
- `heap/drain/8`: 1.089×–1.451×; control 0.921×–1.032×.
- `heap/up/1`: 1.000×–1.024×; control 1.044×–1.055×.
- `heap/up/128`: 1.349×–1.458×; control 0.912×–1.022×.
- `heap/up/32`: 1.321×–1.340×; control 0.921×–1.023×.
- `heap/up/4`: 1.099×–1.133×; control 0.953×–1.046×.
- `heap/up/512`: 1.388×–1.481×; control 0.938×–1.035×.
- `heap/up/8`: 1.189×–1.215×; control 0.857×–1.069×.
- `token/device`: 1.144×–1.192×; control 0.962×–1.022×.
- `token/full`: 1.082×–1.108×; control 0.937×–1.008×.
- `token/max`: 1.145×–1.212×; control 1.017×–1.105×.
- `token/uuid`: 1.124×–1.217×; control 0.973×–1.037×.
- `token/zero`: 1.324×–1.446×; control 0.997×–1.078×.

## Correctness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **81 unit tests and 26 ordinary integration tests**. All four integration targets are built; `input_pipeline_timing` is an ignored-only manual timing target in normal gates. That manual integration is separately executed successfully once for each archived release binary; its wall-clock outputs are supplemental, not CPU-cycle speedup evidence. Dependencies retain cached release profiles in the assertion-enabled gate. Ordinary test-name/status sets match in all three builds.
- The new heap test performs 8,192 deterministic mixed updates at each of ten sizes (1–512), including equal deadlines, cancellations, unchanged values and repeated root removal. It compares the exact heap array, every deadline and slot index, epochs, lengths and capacities after every operation. The three changed routines preserve the original `(deadline, slot)` ordering. Existing public-input differential tests additionally retain exact edges, clock-call counts, caught clock panics, logs and private heap layout.
- Complete event-cycle tests compare every emitted edge field and normalized timestamp plus cleanup state/capacities over 128 cycles at each of five sizes. These use actual public debounce operations and the frozen full module; they complement the existing mixed/chatter/window-change tests.
- Formatting tests compare every low-16-bit code plus high-bit/max samples, device/UUID combinations, exact bytes and owned String capacity. Append checks span input capacities 0–159, every prefix length within each capacity, eight code samples, five device settings and three UUID settings. They preserve existing String growth, rather than silently changing capacity through a single combined write. Existing binding round-trip tests remain included.
- A **19,051,016-byte output trace** matches byte-for-byte across all three gates: SHA-256 `f3786ae6221fcea774145e090c5d7ccb931de53de5cac2b3df9c90349f101b75`. It samples every seventeenth mixed-heap operation, includes all stated complete event cycles, and records formatter bytes/capacities across its bounded code/device/UUID/capacity cohort. Instants are encoded relative to a common per-process base, never as nondeterministic absolute clock values. Part identity:
  - `input`: 19,051,008 bytes, SHA-256 `237ba1c2f9912e2db8313e9cef0c963e7a53d9d4114d80509c5bcef2bf23ca11`.
- All **780 raw rows** match their process logs. **All six heap/event/append allocation counters remain zero**. Owned token cases retain one allocation/free with unchanged requested/peak bytes and zero reallocations; no allocation reduction is claimed. Every workload's six counters are identical between variants and stable across the five processes.
- Scoped Clippy and production-only `cargo check --offline --locked --lib --release` pass. Production check artifacts explicitly retain opt-level 3 with assertions/overflow checks disabled. Clippy allows `too_many_arguments` for the copied legacy full-module reference; scope verification confirms that all three changed production signatures are unchanged and have at most two parameters. Other out-of-scope messages remain recorded. Audits verify all **21 package inputs**, exact source scope and removed helper, frozen Git bodies, dependency/source archives, compiler/lock/profiles, test sets, binary/log hashes, trace and every raw timing/counter field.

## Reproduction and rejected removal variant

`scripts/benchmark_input_heap.py` builds the actual package with its four integration targets in the existing six-member scratch workspace (simfile, notefield, profile, input-fsr, config, input). It snapshots the Git dependency closure and exact checkout overlays, preserves dependency mtimes, uses C: compiler/test temporary storage, and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged. The resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_INPUT_HEAP_ARCHIVE = 'C:\deadsync-debounce-heap-20261105'
python scripts/benchmark_input_heap.py baseline --baseline bb638e5f97753a9154ac7ea583c63ed2df0060b5 --runs 5 --reference baseline
python scripts/benchmark_input_heap.py final --runs 5 --reference baseline
python scripts/benchmark_input_heap.py debug --debug --reference baseline
```

Run stages sequentially using the archived compiler/lock/workspace inputs. Baseline restores the two original production modules and adds only the new test registration; reference/fixture files and test-only clock visibility are common to every gate. Manifests retain source/flags/profile/lock, binaries/logs and trace identities. Each measured TSV has 390 rows; checks JSON retains all 780 verified rows and process/control ranges.

The first standalone trial also rewrote removal to avoid an intermediate slot-index write and remove the generic repair helper. It passed heap-layout differential tests but regressed several removal cohorts, including mixed cancellation, so it is fully withdrawn. Generic removal/repair remain unchanged in the retained source. A refined standalone trial verified the two movement routines plus safe code formatting before formal Cargo measurements. Both exploratory source/binary/log cohorts remain archived; neither substitutes for the final package results above.
