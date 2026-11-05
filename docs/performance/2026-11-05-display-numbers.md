# Display numbers and ordered transform flags — 2026-11-05

Baseline: `3a510ae07e9f34a19c6280ec6705fe25781914fd`. Three measured routines in `deadsync-notefield/src/display_mods.rs`:

1. `push_i16_decimal` writes two decimal digits per division by 100 using a 200-byte digit-pair table, then writes the one/two leading digits. Sign handling, `i16::MIN`, the single final digit-slice append, and String capacities are preserved. The five-byte digit buffer remains the same size.
2. `push_trimmed_float2` handles values exactly equal to an `i16` directly. It writes the decimal integer, appends `.00` to preserve the original transient capacity growth, then truncates those three bytes. Negative zero retains its sign. Every other value uses the original formatting/trimming body, including fractions, larger integers, infinities and NaNs. Exhausting all signed 16-bit integers plus negative zero covers every value accepted by the new exact-equality branch.
3. `push_transform_parts` maps the three input bytes to a 21-bit ordered selection mask using compile-time tables, then visits only set bits. It removes the intermediate 21-entry bool/name array and repeated disabled-option checks. Names and ordering match the old loop, and the original all-zero/ignored-hold-bit early exit remains. The tables ignore high hold bits exactly as before.

The three production bodies change by **+82/−32 lines**, net +50, excluding the test registration and one test-only clock visibility change. Runtime work removes per-digit divisions, general float formatting on the bounded integer path, and a fixed 21-position option traversal. Read-only lookup payloads total **3,272 bytes**: 200 decimal bytes plus three 256-entry `u32` masks (3,072 bytes). This excludes existing/new name-pointer storage and is not a total binary-size measurement. No unsafe, dependency, public API, struct-layout or mutable cache change is introduced. Stack digit buffers and read-only tables are outside heap counters.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package unit-test executables use release opt-level 3, fat LTO and static CRT. Five fresh processes each execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. Windows `QueryThreadCycleTime` supplies thread cycles. Cycle columns below are independent process medians; paired throughput is the median of five within-process ratios. Identical-source baseline controls expose timing/code-generation bias. Our builds and benchmark processes are sequential; unrelated user activity is not controlled.

The counted-System allocator remains installed with counting disabled during CPU timing, retaining its inactive TLS check. Fixture construction stays outside timing. Integer fixtures each format six values; float/transform fixtures append one result to a reused 4,096-byte String. These use 20,000 repetitions per fixture. Complete text fixtures use 10,000 repetitions, materializing and destroying the owned String inside timing. Whole-text cases cover integer/fractional speed with sparse or mixed options. Mixed cases also include an integer-valued average-error-bar intensity, decimal percentages and transform flags. Table data are hot after warmup. Cold-cache behavior, UI frame time and application-wide speedups are not measured.

| Workload | Frozen cycles/fixture | Current cycles/fixture | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `float/fraction` | 482.89 | 484.90 | 0.997× | 0.998× |
| `float/infinity` | 81.53 | 83.96 | 0.972× | 1.029× |
| `float/nan` | 81.16 | 84.78 | 0.956× | 1.000× |
| `float/negative-zero` | 106.10 | 40.18 | 2.641× | 1.000× |
| `float/one` | 419.79 | 39.42 | 10.633× | 1.008× |
| `float/outside` | 547.17 | 546.57 | 1.001× | 1.011× |
| `float/speed` | 444.49 | 45.32 | 9.743× | 1.012× |
| `float/zero` | 96.12 | 39.41 | 2.437× | 0.998× |
| `integer/large` | 194.14 | 166.37 | 1.169× | 0.935× |
| `integer/small` | 104.08 | 105.66 | 0.985× | 1.001× |
| `integer/three` | 162.98 | 150.50 | 1.083× | 0.969× |
| `integer/two` | 129.53 | 119.03 | 1.088× | 0.971× |
| `text/basic-fraction` | 949.16 | 967.84 | 0.981× | 1.018× |
| `text/basic-integer` | 879.82 | 452.96 | 1.959× | 0.994× |
| `text/mixed-fraction` | 2652.09 | 2149.89 | 1.232× | 1.009× |
| `text/mixed-integer` | 2574.05 | 1585.23 | 1.621× | 1.015× |
| `transform/dense` | 241.32 | 219.76 | 1.098× | 0.984× |
| `transform/empty` | 10.51 | 6.37 | 1.650× | 1.065× |
| `transform/ignored` | 10.51 | 6.37 | 1.650× | 1.065× |
| `transform/mixed` | 152.97 | 110.29 | 1.388× | 0.986× |
| `transform/single` | 84.36 | 16.55 | 5.102× | 1.003× |
| `transform/sparse` | 102.74 | 40.79 | 2.519× | 1.003× |

Owned text allocation counters (old → new, identical in all five processes):

| Workload | Allocations | Reallocations | Frees | Cumulative requested bytes | Peak live requested bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| `text/basic-fraction` | 1 → 1 | 0 → 0 | 1 → 1 | 64 → 64 | 64 → 64 |
| `text/basic-integer` | 1 → 1 | 0 → 0 | 1 → 1 | 64 → 64 | 64 → 64 |
| `text/mixed-fraction` | 1 → 1 | 2 → 2 | 1 → 1 | 448 → 448 | 256 → 256 |
| `text/mixed-integer` | 1 → 1 | 2 → 2 | 1 → 1 | 448 → 448 | 256 → 256 |

Observed raw paired medians below 0.970× (all costs retained in the evidence):

- `float/nan`: 81.16 → 84.78 cycles, 0.956×; control 1.000×.

Process ranges (minimum–maximum):

- `float/fraction`: 0.996×–1.005×; control 0.978×–1.009×.
- `float/infinity`: 0.970×–0.982×; control 0.996×–1.071×.
- `float/nan`: 0.950×–0.987×; control 0.964×–1.006×.
- `float/negative-zero`: 2.631×–3.201×; control 0.996×–1.053×.
- `float/one`: 10.505×–11.144×; control 0.974×–1.017×.
- `float/outside`: 0.996×–1.002×; control 1.010×–1.041×.
- `float/speed`: 9.453×–9.880×; control 0.999×–1.035×.
- `float/zero`: 2.426×–2.449×; control 0.972×–1.031×.
- `integer/large`: 1.166×–1.230×; control 0.909×–0.952×.
- `integer/small`: 0.983×–0.991×; control 0.976×–1.037×.
- `integer/three`: 1.078×–1.125×; control 0.871×–1.074×.
- `integer/two`: 1.088×–1.110×; control 0.942×–0.985×.
- `text/basic-fraction`: 0.926×–1.004×; control 0.995×–1.027×.
- `text/basic-integer`: 1.903×–2.011×; control 0.964×–1.029×.
- `text/mixed-fraction`: 1.212×–1.244×; control 0.986×–1.024×.
- `text/mixed-integer`: 1.548×–1.633×; control 0.964×–1.025×.
- `transform/dense`: 1.084×–1.137×; control 0.868×–1.023×.
- `transform/empty`: 1.644×–1.653×; control 1.032×–1.113×.
- `transform/ignored`: 1.641×–1.653×; control 1.065×–1.208×.
- `transform/mixed`: 1.375×–1.576×; control 0.963×–1.021×.
- `transform/single`: 5.067×–5.357×; control 0.964×–1.010×.
- `transform/sparse`: 2.498×–2.572×; control 0.961×–1.112×.

Small/no-work and fallback differences near the controls are inconclusive. Integer-valued float formatting and sparse transform selection are the intended fast paths; fractions and larger values still pay the branch check before the original formatter. The fixed lookup payload trades arithmetic/conditional work for data reads. Direct integer-pair results are per batch of six values; they must not be read as cycles per individual digit. No control-ratio division is presented as a causal improvement.

## Correctness and allocation evidence

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **513 unit tests** with identical ordinary test-name/status sets. The package has no integration targets. Dependencies retain their cached release profiles in the assertion-enabled gate. The complete notefield package includes its existing geometry, noteskin, model, HUD and modifier regression tests; required shipped PNGs and **859 external media fixture files** are archived and hash-verified unchanged across all three gates.
- Every `i16` value is compared for exact decimal output and capacity. Integral float formatting also compares every `i16` value converted exactly to `f32`, all prefix lengths for capacities 0–79 over signed/digit-width samples, 65,536 deterministic raw float words, signed zeros, fractions, out-of-range integers and non-finite values. Values passing the fast branch are exactly the enumerated integral values plus negative zero; the remaining body is source-verified unchanged.
- All **2,097,152 valid transform-mask combinations** (256 insert × 256 remove × 32 low hold masks) compare exact text/order/capacity. Additional cases cover high ignored hold bits, preexisting output and capacity boundaries. Whole-text cases compare owned output bytes/capacities across option profiles and representative speed/non-finite values. Existing modifier tests remain included.
- A **15,317,248-byte output trace** matches byte-for-byte across all three profiles, SHA-256 `8a6fa9e8a9c70da9e7a1533ed6a005201580ee3c50348e692d95eef3fca00abb`. It records all signed decimal/integral-float outputs, every seventeenth valid transform combination, bounded prefix/capacity float cases and complete text profiles. Length and capacity accompany every string. This stated cohort supplements the wider ordinary assertions; it is not the full cross-product of every possible float and prefix.
  - `display` part: 15,317,240 bytes, SHA-256 `35cbb93f3a68f4f578cad36373dbf06f9a1f12f7053b4bc7e796b35d198c3e16`.
- All **440 raw rows** are checked against process logs. Six allocation counters are zero in direct reused-buffer integer/float/transform cases. Complete owned text materialization retains the same allocation/reallocation/free/requested-byte/peak counts between variants and across processes. No heap reduction is claimed. Counters exclude read-only tables, allocator metadata, total RSS and stack frames.
- Scoped Clippy and production-only `cargo check --offline --locked --lib --release` pass. The latter's Cargo artifact explicitly retains opt-level 3 with assertions/overflow checks disabled. Clippy allows `dead_code` for unused public constants copied into the private frozen reference; other diagnostics remain recorded. The source audit verifies unchanged production signatures, exact three-body scope, the frozen module and test-only clock visibility. All **42 package inputs**, compiler/lock/profiles, source/dependency/media archives, test sets, binary/log identities, trace and every timing/allocation field are verified. The standalone exploratory source/binary/log cohort is separately hash-verified.

## Reproduction

`scripts/benchmark_display_numbers.py` builds the actual notefield package in the existing six-member scratch workspace and snapshots the Git dependency closure plus exact checkout overlays. It preserves cache mtimes, uses compiler/test temporary storage on C:, installs the package's required media fixtures, and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; the lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_DISPLAY_NUMBERS_ARCHIVE = 'C:\deadsync-display-numbers-20261105'
python scripts/benchmark_display_numbers.py baseline --baseline 3a510ae07e9f34a19c6280ec6705fe25781914fd --runs 5 --reference baseline
python scripts/benchmark_display_numbers.py final --runs 5 --reference baseline
python scripts/benchmark_display_numbers.py debug --debug --reference baseline
```

Run stages sequentially with the archived compiler/lock/workspace inputs. Baseline restores the original production module plus only the new test registration; references, adapters and clock visibility are common to all three gates. Manifests retain exact sources, flags, profiles, lock, binaries/logs and media identities. Each measured TSV has 220 rows; checks JSON retains 440 verified rows and all process/control ranges. Standalone trials isolate each candidate before the formal package run; final package evidence above is authoritative.
