# FSRIO masks and SMX view reductions — 2026-11-05

Baseline: `08f38d23412c77c4101d103f0b89b2fa5eb658a5`. Three separately measured routines in `deadsync-input-fsr`:

1. Map each signed sensor mapping byte to its valid bit through a 256-entry `u16` table, then OR the twelve entries. Entries 0–15 hold their matching bit; all other bytes, including the unsigned representations of negative `i8`s, hold zero. This replaces per-entry validity checks and shifts. Group selection now filters directly into its output collection and removes the separate sensor-index helper. The selected bit always denotes ID 0–15, so one unsigned-byte equality also rejects every invalid/negative mapping. This direct path is required by the measured caller results: a lookup-only prototype slowed valid group calls and was withdrawn.
2. In `fsr_button_view`, calculate raw-value and threshold maxima while constructing each of the four sensor records. This removes two subsequent scans over the constructed sensor collection. Sensor order, labels, enable/active flags, normalization and all public fields remain unchanged.
3. In `load_cell_button_view`, calculate the raw-value maximum during construction. This independently removes its subsequent sensor scan. Press/release fields and panel-driven activation remain unchanged. These two builders do not call one another or a newly shared reducer; each has its own paired benchmark and differential test.

Production code grows by **7 lines**, excluding test registrations, while removing one private helper and three aggregate traversals. The integer lookup contains **512 bytes of read-only table entries**; these bytes are not counted as heap allocations or a measured total image/stack delta. No floating-point lookup table, unsafe, dependency, public API, mutable cache or general-curve change is retained. The general `ValueCurve::normalize` source is hash-verified unchanged. All sensor values reaching these SMX normalization calls remain finite `u16` inputs to the same linear 250-scale curve.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package unit-test executables use release opt-level 3, fat LTO and static CRT; manifests record the exact compiler and locked dependency graph. Five fresh processes per benchmark namespace execute twelve alternating frozen/current rounds on CPU 2, discard the first and retain the median of eleven. Windows `QueryThreadCycleTime` supplies CPU cycles. The mask and SMX benchmark tests execute sequentially in separate processes (ten per measured stage). Our builds and benchmarks do not overlap; unrelated user activity is not controlled.

The baseline compiles unchanged production beside frozen references. Controls expose code-generation context and timing bias. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios, so it need not equal the quotient of the two independent medians. All process ratios, nanoseconds and six allocation counters are retained. The counted-System allocator stays installed with counting disabled during timing, retaining its inactive TLS check. Result materialization/destruction is timed. Immutable fixture setup stays outside timing; the consumed four-`SensorReading` input array is reconstructed identically inside each timed FSR-builder call. Its common preparation cost is included in both variants.

Mask fixtures cover no valid IDs, one repeated ID, eight valid IDs, twelve distinct high IDs and mixed invalid/valid bytes. Direct masks use 200,000 repetitions, complete view callers 10,000, and rank-zero group callers 20,000. SMX builders use 20,000 repetitions for raw seeds 100/250/500/65535. Four input values and thresholds vary/wrap across positions; firmware indices, optional labels, enable flags and panel activation retain the tested policies. These are synthetic steady-state in-memory workloads; the small integer table is hot after warmup. Cold-cache behavior, USB/HID latency and whole-device/application speedups are not measured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `group/eight` | 68.58 | 44.88 | 1.456× | 0.996× |
| `group/high` | 65.42 | 37.41 | 1.762× | 1.004× |
| `group/mixed` | 65.14 | 38.64 | 1.713× | 0.996× |
| `group/none` | 40.98 | 23.75 | 1.725× | 1.000× |
| `group/one` | 70.89 | 48.20 | 1.472× | 1.006× |
| `mask/eight` | 24.43 | 13.20 | 1.803× | 1.024× |
| `mask/high` | 23.05 | 13.61 | 1.801× | 0.986× |
| `mask/mixed` | 24.66 | 13.42 | 1.833× | 1.017× |
| `mask/none` | 25.29 | 13.89 | 1.825× | 1.004× |
| `mask/one` | 24.47 | 13.84 | 1.795× | 0.996× |
| `smx/fsr/100` | 111.65 | 58.84 | 2.018× | 1.013× |
| `smx/fsr/250` | 116.58 | 63.59 | 2.021× | 0.997× |
| `smx/fsr/500` | 109.80 | 58.56 | 2.020× | 1.016× |
| `smx/fsr/65535` | 112.92 | 57.03 | 1.980× | 1.000× |
| `smx/load/100` | 125.16 | 57.60 | 2.308× | 1.005× |
| `smx/load/250` | 131.97 | 56.93 | 2.309× | 1.006× |
| `smx/load/500` | 124.91 | 57.74 | 2.306× | 0.977× |
| `smx/load/65535` | 124.94 | 57.61 | 2.259× | 0.997× |
| `views/eight` | 1234.84 | 1268.75 | 1.009× | 1.016× |
| `views/high` | 1344.11 | 1336.93 | 1.023× | 0.969× |
| `views/mixed` | 1093.26 | 1107.07 | 1.015× | 0.970× |
| `views/none` | 821.94 | 816.91 | 1.009× | 0.999× |
| `views/one` | 1076.65 | 1063.39 | 1.001× | 1.015× |

Selected gains and controls:

- `mask/eight`: **1.803×**; control **1.024×**.
- `mask/mixed`: **1.833×**; control **1.017×**.
- `group/eight`: **1.456×**; control **0.996×**.
- `group/high`: **1.762×**; control **1.004×**.
- `smx/fsr/100`: **2.018×**; control **1.013×**.
- `smx/fsr/65535`: **1.980×**; control **1.000×**.
- `smx/load/100`: **2.308×**; control **1.005×**.
- `smx/load/65535`: **2.259×**; control **0.997×**.

No raw paired median is below 0.970× in this cohort; this does not prove absence of costs outside the measured inputs. Complete FSRIO view calls stay near parity (1.001×–1.023×), with controls 0.969×–1.016×, so those small differences are inconclusive. The mask and complete group results provide clearer evidence for that path; it is not presented as another large view-construction speedup. SMX gains are from the two independently changed builders. Every process range is retained. The lookup adds 512 table bytes and may trade arithmetic/branches for cache reads in workloads not represented here.

## Exactness and allocation evidence

All **460 raw rows** have zero in all six allocation counters (allocations, reallocations, frees, allocated/freed requested bytes, peak bytes). These routines were already allocation-free; no heap reduction is claimed. Normal tests also enforce no allocation churn in both SMX builders. Read-only table storage and stack temporaries are outside heap counters.

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **27 unit tests**. The package has no integration targets. Existing protocol/mock/SMX and prior FSRIO differential tests remain included. Six timing/trace tests are ignored in ordinary gates; the two new benchmark tests and two new trace tests run explicitly. Dependencies remain cached release builds during package-debug validation.
- Integer masks cover all 256 byte representations at each of twelve positions plus 16,384 deterministic mixed configurations. Full view fields and group ranks 0–16/maximum `usize` compare against frozen pre-change bodies. Existing dense/high-bit/whole-raw-range tests also run. The 16-bit selected-ID bound justifies filtering unsigned mapping bytes with one equality.
- Each SMX builder compares every observable field and normalized float bit over all 65,536 raw seeds, per-position wrapping values and rotated thresholds, reversed firmware indices, mixed `None`/empty/Unicode labels, both enable/panel states and arbitrary press/release words. The FSR and load-cell paths have separate normal tests. The general curve source is unchanged; uncommon curves and NaNs are outside these new finite-input paths.
- A **28,508,456-byte combined output trace** matches exactly in all three profiles: SHA-256 `33803265610566dda5837bde6f1aadeb371f1cc61b010fc4df2a9f61805e8785`. It concatenates length-prefixed mask/group and complete SMX outputs. Individual part identities are retained:
  - `mask`: 9,699,608 bytes, SHA-256 `f048da635cdf65a6816bafc7aba66d54a2f749a4ca0ea9dbeee4afbbe2db692e`.
  - `smx`: 18,808,832 bytes, SHA-256 `cc237d0808f23bd38e72381cb2e1e6971ea23d118a3c89fb2cb52416e4b79fef`.
- The mask trace covers 32,768 deterministic mappings and all stated group ranks; the SMX trace covers both complete builders at every raw seed, including labels/order/capacities/curve/activation/normalized float bits. This is the stated cohort, not the full cross-product of all possible readings and labels.
- Scoped Clippy has zero changed-scope diagnostics; production-only `cargo check --offline --locked --lib --release` passes. Audits verify exact mask/group and two-builder scope, the removed helper, test registrations and every frozen body against Git; all **12 package inputs**, source/dependency archives, compiler/lock/profiles, ordinary test-name sets, binaries/logs, both trace parts and every timing/counter field are verified. An extra source check confirms no new production unsafe and the unchanged general curve. Shared exploratory support is verified against its prior archived hash.

## Reproduction and withdrawn experiments

`scripts/benchmark_fsr_builders.py` builds the actual package in the existing four-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves dependency mtimes with one final write per overlay, uses compiler temporary storage on C:, and keeps offline locked resolution. Repository Cargo files and dependency versions are unchanged. The pure helper tests do not instantiate hardware monitors or require attached devices/media fixtures.

```powershell
$env:DEADSYNC_FSR_BUILDERS_ARCHIVE = 'C:\deadsync-fsr-tables-20261105'
python scripts/benchmark_fsr_builders.py baseline --baseline 08f38d23412c77c4101d103f0b89b2fa5eb658a5 --runs 5 --reference baseline
python scripts/benchmark_fsr_builders.py debug --debug --reference baseline
python scripts/benchmark_fsr_builders.py final --runs 5 --reference baseline
```

Run stages sequentially with the archived compiler/lock/workspace inputs. The baseline overlays the two original modules plus only the new private test-module registrations. Manifests retain exact sources/flags/profiles/lock, executable/log identities and trace-part hashes. TSVs retain all 230 rows per measured stage; checks JSON retains 460 verified rows, paired ranges, complete validation test sets, scoped diagnostics and prototype hashes.

Two standalone trials are archived separately. An SMX normalization-table-only variant regressed FSR construction (about 0.89×–0.92× in the first trial) and gave little load-cell benefit in that context, despite earlier encouraging exploratory results; it is fully withdrawn. Aggregate fusion remained strong with the original normalization and was selected. The first integer-table prototype improved the isolated mask but regressed valid group callers (about 0.51×–0.55× in one process). The refined direct selected-ID filtering removed that caller regression before any formal Cargo measurements. The final retained source contains no extra sensor-index helper or floating-point lookup table. No exploratory result substitutes for the final package measurements above.
