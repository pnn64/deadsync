# Safe Rust performance kernels - 2026-10-25

Three independent changes preserve scoring and parsed values while removing repeated arithmetic, heap structures, and redundant pixel passes. `deadlib-lua` and `deadlib-lua-legacy` are outside this change.

## Changes

1. **Golf scoring:** replace the logarithmic curve, two floating-point floors, and intermediate arithmetic with a 104-byte lookup. The public functions still take absolute timing errors and apply the exact 103.5 ms cutoff before indexing. Every integer bucket, adjacent floating-point boundaries, exceptional values, and 100,000 arbitrary f32 bit patterns are compared against the original calculation.
2. **Golf metadata:** borrow section names from the input and replace each song's difficulty hash map and owned difficulty strings with seven inline f64 values and a presence bitmap. Slot zero is the default. Set-bit traversal examines only overrides that exist. This removes section copies, inner hash tables, difficulty key allocations, and duplicate insertion branches. The inline record remains 64 bytes on this x86_64 host, including the default and bitmap. Defaults, duplicate overrides, case folding, invalid values, colon-containing folder names, and signed zero are checked against the original parser.
3. **Hidden RGB cleanup:** when the first visibility scan finds no visible pixel, clear complete RGBA pixels with a slice fill and return. This eliminates the reverse visibility scan and scalar RGB clearing for fully transparent images. Visible images use the original pixel loop. Byte-exact checks include every alpha value, matching/different edge colors, empty images, and surplus/partial backing-buffer pixels.

All production changes use safe Rust. The benchmark-only Windows FFI has documented handle, lifetime, and pointer requirements. Scoring and image cleanup remain allocation-free.

## Method

Windows x86_64, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Release optimization level 3, LTO disabled equally for both variants, repository static MSVC CRT flags. The benchmark worker is pinned to logical processor 2. Windows `QueryThreadCycleTime` supplies thread CPU cycles; `Instant` supplies elapsed throughput measurements. Inputs and fixtures are prepared outside timing. Function pointers are opaque to the optimizer for scoring and pixel cleanup.

Baseline production sources came from `241a16adc`. The final benchmark tests were compiled first with those production sources and then with the optimized production sources, using identical Cargo options and package selection. Both executable sets were saved. The three files were restored to their optimized contents immediately after the baseline build, including on failure. No benchmark run overlapped a build started by this task.

Four fresh process runs per version alternate before/after process order. Each process alternates a frozen-original control and its actual production function across twelve batches, discarding the first batch. The table reports the median of the four production-column medians; each process median uses eleven batches. This avoids attributing differences between a copied test implementation and a production crate to the optimization. The range is the minimum/maximum ratio of corresponding process runs, not a confidence interval. The committed single-binary benchmarks retain the frozen originals for convenient paired reproduction.

A scoring operation is **1,024 timing errors**: `accurate` spans 0.25 to 23.25 ms, `mixed` spans 0.25 to 103.25 ms, and `late` spans 44.25 to 103.25 ms. A metadata parse operation builds and drops one pack; its label gives song count and overrides per song. A metadata lookup operation contains **1,024 queries**, cycling through all six difficulties. Image operations clean one warmed image in place, without cloning it. `kind=0` is fully transparent, `1` opaque, `2` an alpha ramp with sparse zero alpha, and `3` a checkerboard of zero/full alpha. Image sizes are square pixel dimensions.

These are operation-level results for optional golf features and texture preparation, not whole-game FPS claims. Small image controls include regressions: 64x64 visible-image throughput falls about 2 to 11%, equivalent to roughly 0.03 to 0.19 us/image on this host. Larger controls vary across processes; no general speedup is claimed for visible textures. The retained image optimization targets the consistently 2.9 to 3.3x faster fully transparent case. Raw per-process results and the full control range are included below and in the TSV.

## CPU cycles and throughput

| Workload | Before cycles/op | After cycles/op | Throughput | Process ratio range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `alpha/512/kind=0` | 1099450.27 | 382125.34 | 2.877x | 2.537-3.253x | 502374.50 | 174627.50 |
| `alpha/512/kind=1` | 210536.72 | 180107.43 | 1.169x | 0.991-1.273x | 96114.00 | 82189.00 |
| `alpha/512/kind=2` | 221686.22 | 216937.33 | 1.022x | 0.746-1.188x | 101097.50 | 99007.00 |
| `alpha/512/kind=3` | 363351.53 | 334939.43 | 1.085x | 0.923-1.279x | 165951.00 | 153030.00 |
| `alpha/64/kind=0` | 18220.81 | 5483.52 | 3.323x | 3.223-4.082x | 8319.26 | 2506.75 |
| `alpha/64/kind=1` | 3020.43 | 3089.49 | 0.978x | 0.890-1.099x | 1379.18 | 1408.95 |
| `alpha/64/kind=2` | 3187.11 | 3597.36 | 0.886x | 0.813-0.968x | 1455.30 | 1640.59 |
| `alpha/64/kind=3` | 6093.48 | 6480.19 | 0.940x | 0.886-0.991x | 2780.73 | 2959.66 |
| `golf/accurate/1024` | 16852.44 | 9795.96 | 1.720x | 1.397-1.877x | 7690.85 | 4475.62 |
| `golf/late/1024` | 48001.31 | 10396.40 | 4.617x | 4.433-4.869x | 21900.20 | 4742.12 |
| `golf/mixed/1024` | 35066.01 | 9695.92 | 3.617x | 3.419-3.745x | 16000.95 | 4424.40 |
| `pars/get/1/6/1024` | 131170.79 | 111502.49 | 1.176x | 1.055-1.271x | 59891.70 | 50866.00 |
| `pars/get/128/0/1024` | 85553.86 | 80769.64 | 1.059x | 0.949-1.296x | 39055.60 | 36889.60 |
| `pars/get/128/1/1024` | 97450.10 | 97487.63 | 1.000x | 0.902-1.160x | 44507.60 | 44506.10 |
| `pars/get/128/6/1024` | 132492.40 | 124112.77 | 1.068x | 0.907-1.120x | 60451.90 | 56679.60 |
| `pars/parse/1/6` | 6848.32 | 3433.75 | 1.994x | 1.562-2.311x | 3124.52 | 1570.05 |
| `pars/parse/128/0` | 122465.64 | 111759.51 | 1.096x | 0.983-1.387x | 55878.00 | 51003.50 |
| `pars/parse/128/1` | 299770.05 | 150080.93 | 1.997x | 1.798-2.511x | 137084.50 | 68470.50 |
| `pars/parse/128/6` | 1037385.53 | 532377.49 | 1.949x | 1.748-2.049x | 474192.50 | 243017.50 |

## Allocation measurements

Allocation probes run separately from timing and keep the returned map alive. Counts include internal temporary allocation/free activity; output destruction is outside the probe. Bytes are cumulative requested bytes, and peak bytes include the retained output plus live temporary storage. There are no reallocations in these cases.

| Songs / overrides per song | Allocations before -> after | Allocated bytes before -> after | Peak live bytes before -> after |
| --- | ---: | ---: | ---: |
| 1 / 6 | 23 -> 8 | 960 -> 414 | 839 -> 384 |
| 128 / 0 | 263 -> 135 | 47152 -> 46238 | 35010 -> 35002 |
| 128 / 1 | 775 -> 263 | 70100 -> 47152 | 52482 -> 35002 |
| 128 / 6 | 2823 -> 903 | 122632 -> 51722 | 70290 -> 35002 |

For 128 songs with six overrides each, allocations decrease **68.0%**, allocated bytes **57.8%**, and peak live bytes **50.2%**. Default-only packs also allocate less, with essentially unchanged peak storage on this host. The regular test suite asserts allocation reductions, independently of CPU timing.

## Validation and reproduction

The final release suites passed **1,364 tests** across `deadsync-core`, `deadsync-rules`, `deadsync-chart`, `deadsync-gameplay`, `deadsync-simfile`, and `deadlib-assets`; 30 benchmark tests remained ignored in the ordinary run. The three new benchmarks were then run explicitly. Baseline and optimized executables passed their behavior checks on every measured process run; the forward-looking allocation-reduction assertion is skipped for the baseline executable. Clippy completed successfully with warnings elsewhere in the repository, and no warnings identified these changed kernels or new test files.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-core -p deadsync-rules -p deadsync-chart -p deadsync-gameplay -p deadsync-simfile -p deadlib-assets --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-rules -p deadsync-chart -p deadlib-assets --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadsync-rules --test golf_kernel -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadsync-chart --test golf_pars_kernel -- --ignored --nocapture --test-threads=1
cargo test --release --locked --config profile.release.lto=false -p deadlib-assets --test hidden_alpha_kernel -- --ignored --nocapture --test-threads=1
```

[Measurement summary, allocation counters, and source hashes](2026-10-25-safe-kernels.json) | [Raw per-process production measurements](2026-10-25-safe-kernels.tsv).

Local build, suite, lint, and process logs are under `target/kernel-final*`, with archived before/after test executables in `target/kernel-final/before` and `target/kernel-final/after`.
