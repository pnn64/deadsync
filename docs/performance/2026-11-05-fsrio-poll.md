# FSRIO polling kernels — 2026-11-05

Baseline: `d38d22996e042a6c29ca9101fc3dd9946adf6a47`. Three safe optimizations in `deadsync-input-fsr/src/fsrio.rs`:

1. Build button views from the same valid-button mask, then group the twelve sensors in one pass. A sixteen-entry stack map points each valid HID button ID at its ascending output slot. This replaces rescanning all twelve sensors separately for each button and removes the intermediate mapped-button collection and per-button helper. Sensor order, all raw values, exact normalized float bits, maxima, threshold activation rules, labels, curves, capacities and output order remain unchanged. IDs 0–15 are supported, with at most twelve nonempty groups because there are twelve sensors. Negative and out-of-range mappings remain ignored.
2. Select a requested group directly from the mask: reject ranks outside its population count, clear the lowest set bit for each preceding rank, and pass the selected ID to the unchanged sensor-index filter. This removes construction of a temporary index collection. Arbitrarily large ranks, including `usize::MAX`, return empty without an unbounded loop. This selector is used when routing threshold edits to firmware sensor indices.
3. Decode sensor values with a fixed twelve-element `array::from_fn` after the existing payload guard. This removes the unused button-bits decode, the running byte offset and twelve fallible slice-to-array conversions. The existing ID-prefixed/raw ambiguity, minimum lengths, trailing-byte handling, little-endian interpretation and `Option` result policy are unchanged.

The production diff adds **42 lines and removes 44**, excluding the equivalent stock-mapping test update and test registrations. Two private helpers are removed and one shared mask helper replaces their common mask-building prefix. There is no new unsafe, dependency, public API, mutable cache or persistent state. The local sixteen-`usize` slot map occupies **128 source-level stack bytes on x86-64**; this is not a measured total stack-frame size. All returned collections retain their existing inline capacities (16 buttons, 12 sensors/indices). No heap allocation is removed because these kernels were already allocation-free.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz; exact rustc/lockfile identities are recorded in the manifests. Actual Cargo package unit-test executables use release opt-level 3, fat LTO and static CRT. Five fresh processes per measured stage run twelve alternating frozen/current rounds on CPU 2, discard the first and retain the median of eleven. Windows `QueryThreadCycleTime` provides CPU cycles. Our builds and CPU benchmarks execute sequentially; unrelated user activity is not controlled. Result materialization and destruction stay inside timing; fixture construction and allocation-counter setup stay outside it. The counted-System allocator remains installed with counting disabled during CPU samples, retaining its inactive TLS check.

Each fixture is a complete private production helper call through the actual package module. The frozen view and group bodies use their own original helpers, while the current bodies use the shared mask; their mechanisms do not call one another. The parser is independent. A baseline executable holds original production unchanged beside the frozen references, exposing code-layout and timing bias. Cycle columns are independent medians across five processes; paired throughput is the median of the five within-process ratios. All process ratios, nanoseconds and six allocation counters are retained in TSV/checks data.

`views/n` constructs one complete inline button collection with 0, 1, 4, 8 or 12 distinct mappings (12 sensors total), 10,000 repetitions per round. `group/n/rank` selects sensor indices for ranks 0, 7, 11, 12 and maximum `usize` using 20,000 repetitions. View fixtures use raw sensor values through 1,001 and thresholds through 770; normalization still clamps at 850 through the existing table. Packet fixtures use 0/25/26/27/64 bytes and 200,000 repetitions; first byte 1 deliberately exercises the existing ID/raw fallback rule. All 256 first bytes and lengths 0–79 are covered separately by correctness tests.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `group/0/0` | 45.02 | 39.09 | 1.164× | 0.947× |
| `group/0/11` | 45.56 | 37.52 | 1.174× | 0.959× |
| `group/0/12` | 47.24 | 38.79 | 1.184× | 0.956× |
| `group/0/18446744073709551615` | 45.33 | 37.50 | 1.176× | 0.961× |
| `group/0/7` | 44.09 | 37.52 | 1.175× | 0.962× |
| `group/1/0` | 82.53 | 74.90 | 1.170× | 1.049× |
| `group/1/11` | 43.36 | 37.49 | 1.157× | 0.936× |
| `group/1/12` | 42.29 | 36.58 | 1.156× | 0.924× |
| `group/1/18446744073709551615` | 43.69 | 37.51 | 1.165× | 0.925× |
| `group/1/7` | 41.98 | 36.31 | 1.156× | 0.926× |
| `group/12/0` | 148.61 | 67.68 | 2.196× | 0.987× |
| `group/12/11` | 78.55 | 63.67 | 1.229× | 0.850× |
| `group/12/12` | 54.56 | 36.65 | 1.489× | 0.840× |
| `group/12/18446744073709551615` | 54.37 | 36.70 | 1.484× | 0.835× |
| `group/12/7` | 99.88 | 64.38 | 1.514× | 0.973× |
| `group/4/0` | 121.97 | 66.35 | 1.835× | 0.972× |
| `group/4/11` | 45.91 | 36.33 | 1.246× | 0.883× |
| `group/4/12` | 49.82 | 39.47 | 1.248× | 0.877× |
| `group/4/18446744073709551615` | 47.14 | 38.41 | 1.235× | 0.892× |
| `group/4/7` | 49.83 | 38.77 | 1.240× | 0.888× |
| `group/8/0` | 129.70 | 62.18 | 2.063× | 0.945× |
| `group/8/11` | 50.64 | 36.31 | 1.388× | 0.826× |
| `group/8/12` | 53.24 | 39.47 | 1.390× | 0.835× |
| `group/8/18446744073709551615` | 52.20 | 37.50 | 1.401× | 0.829× |
| `group/8/7` | 96.05 | 66.12 | 1.453× | 0.973× |
| `packet/0` | 4.76 | 4.29 | 1.000× | 0.999× |
| `packet/25` | 4.47 | 4.42 | 1.000× | 1.001× |
| `packet/26` | 16.42 | 8.10 | 2.112× | 0.994× |
| `packet/27` | 15.92 | 7.41 | 2.164× | 0.995× |
| `packet/64` | 17.37 | 7.99 | 2.182× | 1.009× |
| `views/0` | 863.60 | 751.79 | 1.166× | 1.003× |
| `views/1` | 1046.64 | 974.38 | 1.129× | 1.004× |
| `views/12` | 1843.93 | 1327.62 | 1.451× | 0.995× |
| `views/4` | 1333.31 | 1061.70 | 1.268× | 0.960× |
| `views/8` | 1568.11 | 1211.00 | 1.332× | 0.981× |

Selected results and controls:

- `views/1`: **1.129×**; control **1.004×**.
- `views/4`: **1.268×**; control **0.960×**.
- `views/8`: **1.332×**; control **0.981×**.
- `views/12`: **1.451×**; control **0.995×**.
- `group/4/0`: **1.835×**; control **0.972×**.
- `group/8/0`: **2.063×**; control **0.945×**.
- `group/12/0`: **2.196×**; control **0.987×**.
- `group/8/7`: **1.453×**; control **0.973×**.
- `packet/26`: **2.112×**; control **0.994×**.
- `packet/27`: **2.164×**; control **0.995×**.
- `packet/64`: **2.182×**; control **1.009×**.

No raw paired median is below 0.970× in this fixture set; this does not establish improvement for unmeasured inputs or for whole-device polling. Empty/short packet rejection remains at parity. Group-selection controls vary substantially, **0.826×–1.049×**, especially for absent/late ranks. The report does not assign every raw ratio solely to the algorithm or treat controls as exact multiplicative corrections. The clearest group result is rank 0 with twelve mappings (**2.196×**, control **0.987×**). View controls are 0.960×–1.004× and packet controls 0.994×–1.009×. Examples needing their controls include `group/12/11` (1.229×, control 0.850×) and `group/8/11` (1.388×, control 0.826×). Every process range remains in checks JSON.

These are synthetic in-memory helper workloads. Returned inline collections are large enough that materialization/moves can dominate a view call; the benchmark includes them for both variants. USB enumeration, feature reads/writes, device transport timing, pending-save behavior and real hardware latency are not measured or changed. There is no claim of an equivalent end-to-end speedup.

## Allocation and correctness evidence

Every one of **700 raw rows** has zero in all six allocation counters: allocations, reallocations, frees, allocated/freed bytes and peak bytes. A normal test also enforces no allocation churn across view construction, rank selection and parsing. The slot map uses stack memory and is not included in these counters; no RSS or total stack-frame claim is made.

- Baseline, optimized release and package opt-level 0 with debug assertions/overflow checks each pass **24 unit tests**. The package has no integration test targets. All existing protocol/mock/SMX tests run alongside three new differential tests. Two timing/trace tests are ignored in ordinary gates and run explicitly. The package-debug gate keeps dependencies in their cached release builds.
- Views and selection cover all 256 mapping-byte values at each of twelve positions, 4,096 deterministic mixed mappings, negative/out-of-range IDs, sparse/high IDs through 15, duplicates, absent and arbitrarily large ranks, exact ordered sensor indices, all 65,536 raw values and reversed-bit thresholds, and every observable view field. Normalized floats are compared by their bit patterns. The former helper-only eight-button test now verifies the same ordered HID labels through actual button views.
- Parser checks cover all first-byte values at lengths 0–79, and all 65,536 starting sensor values across all twelve little-endian slots. The unchanged prefix guard preserves 26-byte raw fallback and 27-byte ID-prefixed selection, including the ambiguous first-byte-1 case.
- The **10,107,493-byte trace** matches exactly in all three profiles: SHA-256 `a74ea0a1a4326ed40c1f2ebe3175db23c2d084e211aab28f9f573f7d098b6bb7`. It records 8,192 deterministic mapping/view/group cases, complete labels/order/capacities/activation/curve/normalized float bits and packet results for all 65,536 starting values at four lengths. The trace is a stated deterministic cohort, not an exhaustive cross-product of all mappings and readings.
- Scoped Clippy passes with zero changed-scope diagnostics; production-only `cargo check --offline --locked --lib --release` passes. The audit verifies exact production/shared-mask scope, removed helpers, the equivalent normal-test update, test-only registrations and every frozen function body against Git. All **8 package input files**, compiler/lock/profile/source/archive/binary/log identities, ordinary test-name sets, traces and all timing/allocation fields are verified. No production source changed after measurement.

## Reproduction and exploratory work

`scripts/benchmark_fsrio_poll.py` builds the actual Windows backend package from its Git dependency closure and exact checkout overlays. It uses the existing minimal scratch workspace plus the FSRIO member, locked offline resolution and one final write per overlay to preserve mtimes. Compiler temporary files and the new snapshots are on C:. The shared resolved lock adds only the existing `deadsync-input-fsr` 0.1.0 package to that scratch graph, with no dependency-version additions/removals. Repository Cargo manifests and lockfile are unchanged. FSRIO's pure tests require no external media fixtures or attached HID device.

```powershell
$env:DEADSYNC_FSRIO_ARCHIVE = 'C:\deadsync-fsr-poll-20261105'
python scripts/benchmark_fsrio_poll.py baseline --baseline d38d22996e042a6c29ca9101fc3dd9946adf6a47 --runs 5 --reference baseline
python scripts/benchmark_fsrio_poll.py debug --debug --reference baseline
python scripts/benchmark_fsrio_poll.py final --runs 5 --reference baseline
```

Run stages sequentially with the archived compiler/lock/workspace inputs. The baseline overlays the pre-change FSRIO module plus only the new child-test registration; the library's new allocator/clock registrations are test-only. Manifests retain source/dependency archive hashes, exact overlays, compiler/flags/profile/lock, commands, executable/log hashes and trace digests. The two TSVs retain all per-process cycles/nanoseconds/counters; checks JSON retains paired ranges, source/prototype identities, scoped diagnostics and full validation test sets.

A separate standalone exploratory executable includes the real input-view types and frozen helpers. It passed exhaustive mapping/packet/float checks and three benchmark processes. It also measured a promising SMX normalization-table candidate; that candidate is **not part of this commit** and is reserved for fresh evaluation in a later batch. Exploratory source, executable and log hashes remain separate from the final Cargo measurements. An initial test-source extraction prefix error was corrected before any trial executable or measurements existed.
