# Color output growth and metadata lookups

Three safe Rust changes reserve color-key output before writing, index larger background lists by beat, and resolve pad-config defaults in one pass. The runtime baseline is `434ea4029`. No changes are made to the excluded Lua engine crates, shared benchmark helpers, dependencies, manifests, or release settings.

Isolated CPU throughput improves **1.900×** for the RGBA output batch, **1.344×** for 1,024 distinct background records, and **1.804×** for missing defaults across 128 profiles. Each of these gains holds in every retained process pair.

The production diff is **56 inserted / 19 removed lines**, plus a four-line unit-test registration. No production unsafe code is added or removed. The index adds a temporary allocation for larger background lists; this is a CPU/memory tradeoff, recorded below.

## Changes and compatibility

1. `normalized_background_lua_color` reserves its existing final capacity up front: eight bytes for opaque RGB and sixteen for RGBA. Formatting, byte quantization, UTF-8 validation, uppercase digits, alpha omission, and output capacities remain identical. RGBA output no longer grows after writing `#`; RGB already needed no growth and is a control.
2. Background replacement keeps a linear scan while the output has fewer than 32 slots. NaNs are appended directly because they never compare equal to an existing beat. The next numeric change at or above 32 slots lazily builds a map from normalized beat bits to vector positions. Both zero signs share a key; every other non-NaN value has its own key. Existing slots are overwritten in place, preserving first-slot order, last-authored contents, signed-zero payloads, fallback selection, and final sorting. The map stores positions, so vector growth cannot invalidate it. At initialization, at most 32 numeric slots exist; later NaN slots require no index entries. The map grows with distinct numeric beats and is destroyed before returning the result.
3. `pad_config::resolve` consumes one compatible-profile iterator. It returns the first serial match. When it encounters the first compatible global default, it checks the remaining iterator for a serial match before using that global fallback. This preserves serial priority, the first matching reference, exact backend/serial comparisons, and pad-type wildcard behavior while eliminating the fallback scan from the beginning.

## Isolated measurements

Throughput is before/after thread CPU cycles, so values above 1 are faster. Each table reports medians across alternating process pairs. The paired range is the minimum and maximum ratio for matching run numbers, not a confidence interval. A color operation normalizes **256 fields**; a background operation resolves **one tag with the stated record count**; a profile operation performs **64 lookups**.

### Reserve color output only

Eight process pairs; [all 512 records](2026-10-26-lookup-growth-reserve.tsv):

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `color/rgb` | 72,928.88 → 70,833.35 | 33,300.00 → 32,281.25 | 1.030× | 0.891–1.181× |
| `color/rgba` | 134,107.64 → 70,589.82 | 61,131.25 → 32,160.94 | 1.900× | 1.675–2.082× |
| `color/caret` | 217,339.32 → 140,017.00 | 99,067.19 → 63,818.75 | 1.552× | 1.414–1.637× |
| `color/mixed` | 218,261.88 → 137,955.77 | 99,648.43 → 62,879.68 | 1.582× | 1.443–1.717× |
| `color/unicode` | 265,255.47 → 196,599.98 | 120,934.38 → 89,614.07 | 1.349× | 1.238–1.556× |
| `color/invalid` | 28,658.47 → 31,539.39 | 13,034.38 → 14,343.75 | 0.909× | 0.834–1.328× |

RGB and invalid inputs retain their original allocation paths. RGBA, caret, mixed-delimiter, and Unicode-whitespace fixtures produce nonopaque keys and exercise the removed growth step. Unchanged background-resolution controls remain in the raw data.

The RGBA batch removes all 255 growth reallocations (one of its 256 alpha values is opaque RGB). Requested output bytes fall from 6,128 to 4,088, with final capacities and peak live requested bytes unchanged. The fixed nonopaque decimal batches remove 256 reallocations each.

### Beat index added after output reservation

Eight process pairs; [all 512 records](2026-10-26-lookup-growth-beats.tsv):

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `beats/unique0` | 229,171.69 → 217,565.62 | 104,596.88 → 99,290.62 | 1.053× | 0.912–1.128× |
| `beats/few0` | 228,876.75 → 227,943.90 | 104,681.25 → 104,040.62 | 1.004× | 0.622–1.110× |
| `beats/reverse0` | 225,940.97 → 219,568.57 | 103,109.38 → 100,209.38 | 1.029× | 0.682–1.096× |
| `beats/unique16` | 272,728.75 → 261,657.68 | 124,450.00 → 119,396.88 | 1.042× | 0.775–1.192× |
| `beats/few16` | 261,781.18 → 254,414.22 | 123,437.50 → 116,337.50 | 1.029× | 0.906–1.148× |
| `beats/reverse16` | 277,009.03 → 262,686.62 | 126,396.88 → 120,437.50 | 1.055× | 0.922–1.170× |
| `beats/unique32` | 301,476.41 → 292,970.72 | 137,525.00 → 134,134.38 | 1.029× | 0.941–1.174× |
| `beats/few32` | 299,027.59 → 279,807.66 | 136,425.00 → 127,643.75 | 1.069× | 0.867–1.166× |
| `beats/reverse32` | 308,157.47 → 295,591.06 | 140,600.00 → 134,975.00 | 1.043× | 0.965–1.093× |
| `beats/unique33` | 299,082.47 → 308,541.56 | 141,303.12 → 140,962.50 | 0.969× | 0.844–1.129× |
| `beats/few33` | 285,789.00 → 288,381.84 | 130,684.38 → 131,665.62 | 0.991× | 0.839–1.205× |
| `beats/reverse33` | 300,811.00 → 312,437.66 | 137,453.12 → 142,515.62 | 0.963× | 0.808–1.088× |
| `beats/unique64` | 359,739.91 → 365,330.31 | 166,515.62 → 166,640.62 | 0.985× | 0.852–1.140× |
| `beats/few64` | 322,870.78 → 334,847.25 | 147,265.62 → 153,246.88 | 0.964× | 0.902–1.154× |
| `beats/reverse64` | 365,501.82 → 363,450.84 | 167,387.50 → 166,037.50 | 1.006× | 0.916–1.050× |
| `beats/unique256` | 945,770.88 → 935,042.50 | 431,687.50 → 426,100.00 | 1.011× | 0.863–1.191× |
| `beats/few256` | 671,615.00 → 684,620.38 | 307,575.00 → 312,612.50 | 0.981× | 0.881–1.145× |
| `beats/reverse256` | 931,338.50 → 939,926.38 | 426,125.00 → 428,512.50 | 0.991× | 0.823–1.144× |
| `beats/unique1024` | 3,619,829.38 → 2,693,320.12 | 1,654,950.00 → 1,232,312.50 | 1.344× | 1.196–1.509× |
| `beats/few1024` | 2,094,715.88 → 2,087,993.75 | 962,025.00 → 953,337.50 | 1.003× | 0.901–1.167× |
| `beats/reverse1024` | 3,642,547.75 → 2,715,379.75 | 1,663,100.00 → 1,239,012.50 | 1.341× | 1.248–1.420× |
| `beats/duplicates64` | 350,075.12 → 371,284.25 | 160,300.00 → 172,262.50 | 0.943× | 0.850–1.122× |
| `beats/duplicates256` | 688,955.56 → 720,742.00 | 314,475.00 → 335,937.50 | 0.956× | 0.828–1.015× |
| `beats/duplicates1024` | 2,004,885.69 → 2,119,107.87 | 935,881.25 → 1,016,093.75 | 0.946× | 0.870–1.053× |
| `beats/nonfinite512` | 1,330,019.13 → 1,376,265.06 | 607,300.00 → 628,756.25 | 0.966× | 0.841–1.088× |
| `beats/nan512` | 1,619,045.69 → 1,525,017.38 | 786,150.00 → 701,218.75 | 1.062× | 0.972–1.153× |

`unique` uses increasing beats; `reverse` uses decreasing beats; `few` repeatedly updates three beats; `duplicates` repeatedly updates 37 beats. `nonfinite` mixes positive/negative NaNs, infinities, and both zero signs; `nan` consists entirely of NaNs. The background fixture includes rates, effects, transitions, and both color fields, with a random-background target. Results include parsing, directory enumeration, replacement, sorting, owned output, and destruction.

The 1,024-distinct-record workload is the demonstrated indexing win. The 256-record median is only 1.011×, and the 37-beat repetition median is 0.946×; their paired ranges cross 1. The latter still pays for the temporary map. No gain is claimed for these cases. At 1,024 distinct records, peak live requested bytes increase from 348,332 to 383,164 (**10.0%**), with six extra table allocations and 68,640 extra cumulative requested bytes. Small and duplicate-heavy cases remain visible in both the timing and allocation tables.

### One-pass pad-config lookup

Twelve process pairs; [all 432 records](2026-10-26-lookup-growth-profiles.tsv):

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `resolve/serialfirst1` | 3,002.69 → 2,997.55 | 1,357.03 → 1,354.69 | 1.002× | 0.769–1.374× |
| `resolve/seriallast1` | 2,901.52 → 2,982.12 | 1,310.94 → 1,378.12 | 0.973× | 0.777–1.140× |
| `resolve/globalfirst1` | 3,213.62 → 2,452.23 | 1,453.12 → 1,107.82 | 1.310× | 1.058–1.636× |
| `resolve/globallast1` | 3,134.74 → 2,472.81 | 1,439.85 → 1,120.31 | 1.268× | 1.119–1.728× |
| `resolve/missing1` | 3,184.47 → 2,479.67 | 1,441.40 → 1,120.31 | 1.284× | 1.155–1.762× |
| `resolve/serialfirst16` | 2,810.62 → 2,899.81 | 1,269.54 → 1,310.16 | 0.969× | 0.775–1.312× |
| `resolve/seriallast16` | 23,165.83 → 21,910.56 | 10,555.47 → 9,981.25 | 1.057× | 0.833–1.261× |
| `resolve/globalfirst16` | 24,160.43 → 21,975.72 | 11,005.47 → 10,016.41 | 1.099× | 0.931–1.312× |
| `resolve/globallast16` | 42,077.11 → 23,013.19 | 19,168.75 → 10,474.22 | 1.828× | 1.526–2.046× |
| `resolve/missing16` | 41,677.57 → 22,440.45 | 18,989.85 → 10,214.07 | 1.857× | 1.590–2.093× |
| `resolve/serialfirst128` | 2,723.17 → 3,000.98 | 1,230.47 → 1,356.25 | 0.907× | 0.712–1.200× |
| `resolve/seriallast128` | 175,972.14 → 171,757.04 | 80,663.28 → 83,270.31 | 1.025× | 0.933–1.151× |
| `resolve/globalfirst128` | 179,732.76 → 167,397.89 | 82,951.56 → 76,328.90 | 1.074× | 0.953–1.185× |
| `resolve/globallast128` | 311,046.92 → 176,939.29 | 144,804.68 → 81,175.00 | 1.758× | 1.601–1.971× |
| `resolve/missing128` | 313,432.28 → 173,765.12 | 144,238.28 → 79,258.59 | 1.804× | 1.672–2.155× |
| `resolve/empty` | 1,563.94 → 1,936.05 | 702.34 → 872.65 | 0.808× | 0.610–0.905× |
| `resolve/incompatible` | 18,194.49 → 9,424.78 | 8,278.91 → 4,322.66 | 1.930× | 1.738–2.405× |
| `resolve/dense` | 487,627.83 → 306,185.34 | 224,104.69 → 139,700.00 | 1.593× | 1.374–1.781× |

Profile counts are 1, 16, and 128. Ordinary profiles contain two unrelated serial associations. Serial/global matches appear at either end; `missing` has no match or global default. Additional controls use an empty list, 128 incompatible backends, and 128 profiles with sixteen serial associations each. All profile lookup workloads allocate zero bytes before and after.

Late-global and missing-default cases benefit from eliminating the repeated compatibility scan. At 128 profiles, first-global and last-serial medians are 1.074× and 1.025× with ranges crossing 1; no improvement is established for those controls.

## Combined simfile baseline versus final

Twelve process pairs; [all 768 records](2026-10-26-lookup-growth-simfile-final.tsv):

| Workload | Cycles/op before → after | ns/op before → after | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| `color/rgb` | 75,093.01 → 74,057.25 | 34,378.12 → 33,771.87 | 1.014× | 0.893–1.426× |
| `color/rgba` | 135,249.72 → 72,294.39 | 61,651.56 → 32,940.62 | 1.871× | 1.621–2.387× |
| `color/caret` | 221,365.77 → 143,203.19 | 100,934.37 → 65,385.94 | 1.546× | 1.231–1.975× |
| `color/mixed` | 223,060.02 → 146,300.17 | 101,684.37 → 66,812.50 | 1.525× | 1.282–2.060× |
| `color/unicode` | 277,701.80 → 199,360.85 | 128,345.32 → 90,931.25 | 1.393× | 1.134–1.885× |
| `color/invalid` | 30,771.17 → 26,470.32 | 13,993.75 → 12,037.50 | 1.162× | 0.830–1.392× |
| `beats/unique0` | 223,272.65 → 220,117.31 | 101,890.62 → 100,340.62 | 1.014× | 0.845–1.250× |
| `beats/few0` | 228,725.88 → 223,588.18 | 104,143.75 → 102,184.38 | 1.023× | 0.845–1.287× |
| `beats/reverse0` | 221,482.35 → 221,797.94 | 101,603.12 → 101,240.62 | 0.999× | 0.824–1.247× |
| `beats/unique16` | 266,973.72 → 260,285.85 | 121,803.12 → 120,396.88 | 1.026× | 0.824–1.229× |
| `beats/few16` | 251,176.60 → 260,306.43 | 114,634.38 → 119,034.38 | 0.965× | 0.833–1.209× |
| `beats/reverse16` | 259,963.47 → 271,926.16 | 118,615.62 → 124,068.75 | 0.956× | 0.764–1.170× |
| `beats/unique32` | 302,025.16 → 293,203.94 | 137,784.38 → 134,209.38 | 1.030× | 0.933–1.220× |
| `beats/few32` | 294,130.00 → 271,761.59 | 134,181.25 → 124,000.00 | 1.082× | 0.841–1.294× |
| `beats/reverse32` | 308,747.32 → 306,271.06 | 142,728.12 → 139,762.50 | 1.008× | 0.833–1.203× |
| `beats/unique33` | 305,372.53 → 293,642.94 | 139,512.50 → 133,937.50 | 1.040× | 0.875–1.215× |
| `beats/few33` | 283,388.25 → 274,765.97 | 129,278.12 → 125,609.38 | 1.031× | 0.877–1.152× |
| `beats/reverse33` | 308,692.43 → 309,337.29 | 141,625.00 → 141,100.00 | 0.998× | 0.831–1.161× |
| `beats/unique64` | 364,870.75 → 345,019.71 | 166,509.38 → 157,368.75 | 1.058× | 0.901–1.276× |
| `beats/few64` | 338,496.47 → 334,140.75 | 154,378.12 → 152,412.50 | 1.013× | 0.882–1.234× |
| `beats/reverse64` | 370,173.04 → 365,837.91 | 169,234.38 → 167,100.00 | 1.012× | 0.846–1.232× |
| `beats/unique256` | 972,988.50 → 994,527.12 | 444,075.00 → 453,837.50 | 0.978× | 0.839–1.278× |
| `beats/few256` | 681,437.88 → 681,739.62 | 311,625.00 → 320,962.50 | 1.000× | 0.876–1.309× |
| `beats/reverse256` | 944,316.38 → 922,640.62 | 432,012.50 → 422,362.50 | 1.023× | 0.859–1.299× |
| `beats/unique1024` | 3,783,549.00 → 2,700,563.25 | 1,727,162.50 → 1,251,362.50 | 1.401× | 1.179–1.692× |
| `beats/few1024` | 2,030,265.38 → 2,037,014.75 | 940,825.00 → 929,750.00 | 0.997× | 0.771–1.220× |
| `beats/reverse1024` | 3,808,023.50 → 2,661,135.62 | 1,748,275.00 → 1,215,875.00 | 1.431× | 1.076–1.707× |
| `beats/duplicates64` | 374,274.94 → 369,226.43 | 170,606.25 → 168,331.25 | 1.014× | 0.823–1.144× |
| `beats/duplicates256` | 736,161.81 → 697,667.06 | 336,087.50 → 317,137.50 | 1.055× | 0.803–1.226× |
| `beats/duplicates1024` | 2,233,549.56 → 2,102,823.69 | 1,025,568.75 → 961,225.00 | 1.062× | 0.803–1.251× |
| `beats/nonfinite512` | 1,475,492.69 → 1,409,011.63 | 675,387.50 → 644,556.25 | 1.047× | 0.873–1.240× |
| `beats/nan512` | 1,840,507.50 → 1,584,995.88 | 840,812.50 → 736,993.75 | 1.161× | 0.943–1.351× |

Color output and background merging are separate production paths. The combined archive checks both changes against the same original executable; it does not imply a multiplicative speedup or an application-wide gain. Small or unchanged controls may move with executable layout and shared-machine conditions. Ranges crossing 1 do not establish a gain.

## Allocation effects

One separate untimed operation includes owned output creation and destruction. The existing System wrappers count allocations only on the current thread, with tracking disabled during timed repetitions. Requested bytes are cumulative allocation/reallocation requests; peak live bytes exclude allocator bookkeeping and native allocations outside Rust's allocator. All counters are identical across every process for a workload/version.

Color reservation:

| Workload | Allocations before → after | Reallocations before → after | Requested bytes before → after | Peak live bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| `color/rgb` | 256 → 256 | 0 → 0 | 2,048 → 2,048 | 8 → 8 |
| `color/rgba` | 256 → 256 | 255 → 0 | 6,128 → 4,088 | 16 → 16 |
| `color/caret` | 256 → 256 | 256 → 0 | 6,144 → 4,096 | 16 → 16 |
| `color/mixed` | 256 → 256 | 256 → 0 | 6,144 → 4,096 | 16 → 16 |
| `color/unicode` | 512 → 512 | 256 → 0 | 11,520 → 9,472 | 21 → 21 |
| `color/invalid` | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |

Beat indexing:

| Workload | Allocations before → after | Reallocations before → after | Requested bytes before → after | Peak live bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| `beats/unique32` | 72 → 72 | 4 → 4 | 18,731 → 18,731 | 13,444 → 13,444 |
| `beats/unique33` | 74 → 75 | 5 → 5 | 28,993 → 30,097 | 18,586 → 19,690 |
| `beats/unique64` | 136 → 138 | 5 → 5 | 32,179 → 35,475 | 21,772 → 23,964 |
| `beats/unique256` | 520 → 524 | 7 → 7 | 128,211 → 144,595 | 87,084 → 95,804 |
| `beats/unique1024` | 2,056 → 2,062 | 9 → 9 | 512,339 → 580,979 | 348,332 → 383,164 |
| `beats/few1024` | 2,055 → 2,055 | 1 → 1 | 22,099 → 22,099 | 722 → 722 |
| `beats/duplicates1024` | 2,056 → 2,057 | 5 → 5 | 48,979 → 50,083 | 18,667 → 19,771 |
| `beats/nonfinite512` | 1,032 → 1,033 | 7 → 7 | 120,411 → 121,515 | 72,378 → 73,482 |
| `beats/nan512` | 1,032 → 1,032 | 8 → 8 | 256,251 → 256,251 | 174,164 → 174,164 |

The beat map adds storage and rehashing allocations once initialized. Small lists and repeated three-beat lists keep the original vector strategy. NaNs do not need hash entries. The threshold limits setup costs on small outputs; the data includes both sides of that boundary. Allocation improvements are claimed for color output, and CPU improvements for indexed large lists.

## Method and limits

- Windows x86-64 MSVC; Intel Xeon E5-2696 v4 at 2.20 GHz, 44 logical processors. Rust 1.99.1 (`58cf8f9c3`), LLVM 23.1.1. Measured test builds use the unchanged release settings: optimization level 3, LTO enabled, and static CRT. No target-CPU, allocator, manifest, feature, or dependency changes are used.
- Three simfile unit-test archives (`before`, `reserve`, `after`) and two profile integration archives (`profile-before`, `profile-after`) use one fixed harness per package. Adjacent simfile stages isolate reservation and indexing; the combined comparison reuses the first/last archives. The profile comparison calls the public API directly.
- The shared helper pins the benchmark thread to logical processor 2. Each process discards one warm sample and selects the median of 11 measured samples by `QueryThreadCycleTime`, with the corresponding `Instant` duration. Tables take medians across processes. The outer driver alternates process order. No compilation from this pass overlaps retained sampling; external machine activity is uncontrolled.
- Inputs and fixtures are constructed outside timing and allocation probes. `black_box` protects inputs and outputs. Owned results are constructed and destroyed inside the measured operation. Timing retains the existing counted System wrapper with tracking disabled.
- Color batches contain 256 strings. RGBA varies alpha through all 256 values, so one result is opaque RGB; RGB varies red. Decimal fixtures use caret or mixed separators, with a Unicode-whitespace control. Samples repeat color batches 32 times.
- Background operations call the production resolver from already-extracted raw tag values. They include UTF-8 decoding, unescaping, field parsing, enumeration of a warmed empty directory, merging, sorting, and output destruction. They do not decode media files. A fixture uses fixed-width process/timestamp fields to keep path length and directory-allocation bytes constant across processes. Counts 0–64 repeat 16 times per sample; counts 256/1,024 repeat four times. The 37-beat duplicate and nonfinite fixtures repeat eight times.
- Profile operations perform 64 calls per batch, repeated 64 times per sample. Lookup references are consumed without allocating or cloning results. Fixture construction, names, and serial-list allocation are excluded.

CPU-cycle gains here are scoped to these release test executables and inputs. They do not measure application startup, frame rate, device I/O, or end-to-end song loading. Directory enumeration remains inside background timings, and its CPU/elapsed-time variation is visible in controls. The map's allocation and peak-memory costs are part of the measurements.

Local source snapshots, builds, process logs, statistics, and audits are under `target/lookup-growth-20261026-434ea4029/`. The retained TSVs include full executable SHA-256 digests:

| Archive | SHA-256 |
| --- | --- |
| `before` | `0a2910dee62c20df1e31731a56d6f012a79f6eade7e1db47f68903dc9a383eb7` |
| `reserve` | `cff922c4657cf468d911fd69c0729aadab1da626769c2d893cbc06b4c9a863be` |
| `after` | `9063ac5321684de490fbcf69cc6ba1464f9e36ca0d2f6818657c3aea0013fbfc` |
| `profile-before` | `2235d77513a9875bc73b96e36f0554c95d3e1ce1b6ef3a09673e626fdaa35df8` |
| `profile-after` | `f3caa1f12143f67f0772a65818743f44cd050b6276a535bfa5e85de6159a34c3` |

## Correctness and validation

The [simfile harness](../../crates/deadsync-simfile/src/changes/lookup_growth.rs) compares the production resolver and color writer with [frozen baseline functions](../../crates/deadsync-simfile/src/changes/lookup_growth_original.rs). Its three ordinary tests pass in every measured simfile archive. They cover sizes 0, 1, 16, 31, 32, 33, 64, 128, and 512; forward/reverse beats; three- and 37-beat repetition; both zero signs; infinities; positive/negative and repeated NaNs; both fallback modes; movie/still selection with out-of-order negative beats and replacements; every byte in every RGBA channel; output capacities; and malformed Unicode panic payloads. Every returned background field is compared, with exact bit comparisons for all floats.

The [profile harness](../../crates/deadsync-profile/tests/pad_lookup_growth.rs) compares public lookups with the [frozen original lookup](../../crates/deadsync-profile/tests/support/pad_lookup_original.rs). Both ordinary tests pass in both profile archives. Deterministic generated profiles cover empty lists, multiple global and serial defaults, exact backend case/Unicode/empty names, pad-type wildcard combinations, absent/empty serials, and reference identity. Explicit checks require a later serial match to beat an earlier global default and the first matching serial to win.

The source audit reconstructs both complete runtime files from all five archived stages, verifies frozen references against `434ea4029` after rustfmt normalization, checks fixed harness/helper hashes, matches final runtime bytes to the measured snapshots, and validates all **2,224 retained records** against archive digests and stable allocation counters.

Final checks:

- Release tests for `deadsync-simfile` and `deadsync-profile`: **520 passed, 0 failed, 26 ignored**.
- Debug tests for the same crates: **520 passed, 0 failed, 26 ignored**.
- Scoped rustfmt checks the four edited functions and all four new test/reference files.
- Scoped Clippy with `--no-deps --all-targets` completes successfully. It reports existing style/dead-code diagnostics and repeated loading of shared test helpers. There are no diagnostics in the edited function bodies or new test/reference files; the existing `ptr_arg` warning in `sort_foreground_changes` remains. Full diagnostics are retained in the validation log.

Broad release tests and Clippy use `--config profile.release.lto=false` to reduce link time. The measured archives retain default LTO-enabled release settings. Ignored tests are opt-in benchmarks; both new benchmark tests are explicitly run during retained sampling.

## Reproduction

Keep the new harness/reference files and simfile test-module registration fixed. Build the original runtime versions from `434ea4029` into the two baseline archives, then apply color reservation, beat indexing, and profile lookup edits separately. Obtain executable paths from Cargo's JSON `compiler-artifact` output, archive each binary, and run its ordinary differential checks before sampling.

```powershell
cargo test --release --locked -p deadsync-simfile --lib --no-run --message-format=json
cargo test --release --locked -p deadsync-profile --test pad_lookup_growth --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_lookup_growth --log-dir target/lookup-growth-20261026-434ea4029/runs --before target/lookup-growth-20261026-434ea4029/before/bench.exe --after target/lookup-growth-20261026-434ea4029/reserve/bench.exe --output docs/performance/2026-10-26-lookup-growth-reserve.tsv
python scripts/benchmark_sampling.py --runs 8 --test-filter benchmark_lookup_growth --log-dir target/lookup-growth-20261026-434ea4029/runs --before target/lookup-growth-20261026-434ea4029/reserve/bench.exe --after target/lookup-growth-20261026-434ea4029/after/bench.exe --output docs/performance/2026-10-26-lookup-growth-beats.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_lookup_growth --log-dir target/lookup-growth-20261026-434ea4029/runs --before target/lookup-growth-20261026-434ea4029/before/bench.exe --after target/lookup-growth-20261026-434ea4029/after/bench.exe --output docs/performance/2026-10-26-lookup-growth-simfile-final.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_profile_lookup_growth --log-dir target/lookup-growth-20261026-434ea4029/runs --before target/lookup-growth-20261026-434ea4029/profile-before/bench.exe --after target/lookup-growth-20261026-434ea4029/profile-after/bench.exe --output docs/performance/2026-10-26-lookup-growth-profiles.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-simfile -p deadsync-profile -- --test-threads=1
cargo clippy --no-deps --release --locked --config profile.release.lto=false -p deadsync-simfile -p deadsync-profile --all-targets
```

Use fresh destinations to preserve earlier measurements. Compute medians per version/workload from the TSVs, throughput from the ratio of cycle medians, and paired extrema from matching run numbers.
