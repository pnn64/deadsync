# Direct scan kernels - 2026-10-25

Three measured changes use safe Rust and preserve returned values, text ownership, malformed-input handling, and backing-buffer semantics. `deadlib-lua` and `deadlib-lua-legacy` are excluded.

1. **Text markers:** replace the nested scalar delimiter loop and match with `memchr2`, use `str::find` for the first ampersand, and continue directly from an ampersand already found by the delimiter search. This removes nine net lines from the production scanner and its repeated delimiter branches. Marker resolution and allocation policy are unchanged. The text builder's existing plain-text guard remains: removing it slowed short labels in an experiment.
2. **Ten-lane stream densities:** replace the first eight byte comparisons in row validation with an endian-independent XOR/subtract/AND zero-byte test. The last two bytes retain the standard scan. Four-, five-, and eight-lane charts retain their existing validation. Density counting, caps, comments, whitespace, malformed-line fallbacks, and run traversal are unchanged. The word load uses safe slice conversion; wrapping subtraction is intentional.
3. **Software texture opacity:** AND four 128-bit words before testing their alpha bytes, reducing loop branches from one per four pixels to one per sixteen pixels. An immediate first-pixel check preserves the common transparent-image exit. Complete backing-buffer pixels and partial trailing bytes retain the original interpretation. There is no new unsafe code or CPU feature requirement.

Packed fixed-point image resizing, a scalar alpha reduction, and alternative step classifiers did not demonstrate worthwhile gains and were discarded. Their production code is absent from the change.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Both builds use release optimization level 3, identical package selection, LTO disabled, and the repository's single static-MSVC-CRT flag. Windows `QueryThreadCycleTime` measures thread CPU cycles; `Instant` supplies secondary elapsed-time results. The benchmark thread is pinned to logical processor 2. Fixture construction and allocation probes stay outside timing; consuming and dropping returned output stay inside each operation. Density and marker binaries use the same System allocator wrapper, with accounting disabled during timing.

The baseline algorithms come from `55ca4c72e`. An isolated checkout combines those production algorithms with the final test harness and the same manifests/lockfile as the optimized build. Both executable sets are archived separately. Fresh compilation of the optimized kernels and distinct executable hashes were checked, avoiding reuse of the baseline's shared Cargo artifacts. The baseline and optimized executables both passed the new exact-output tests before measurement.

Four fresh-process runs per version alternate before/after order. Each process discards one warmup batch and reports the median of eleven batches. The table reports the median of four **production-function** measurements for each version. Density and opacity also alternate a frozen-original control and production within each process; only the production column is used below. Marker measurements call the public production API directly. No build started by this task overlapped these final runs.

The throughput column is before/after thread cycles, not a wall-clock ratio. The range spans the four corresponding process ratios; it is not a confidence interval. These are warmed kernel measurements, not whole-game FPS claims.

- A density operation builds and drops the result for 128 measures of 64 rows, plus a trailing empty measure. `kind=0` has no steps, `1` has steps on one quarter of rows, and `2` has a step on every row; the step column rotates and note bytes cycle through `1`, `2`, and `4`. The label gives lane count and input bytes.
- An opacity operation scans one square RGBA image. `kind=0` is opaque; `1`, `2`, and `3` place a non-opaque pixel first, halfway through, or last. RGB is nonzero. The label gives pixel dimensions.
- A marker operation scans the named string and drops its result; `text-build` also constructs and drops the real text actor. Labels give exact UTF-8 bytes. The dense string repeats valid aliases, numeric markers, and unknown markers; sparse text places one marker after 2,048 ASCII bytes. Alias initialization occurs before timing.

## CPU cycles and throughput

The retained targets improve consistently across all four paired process runs: ten-lane density throughput is **1.38-1.52x**, opaque scans are **1.52x at 512x512** and **2.80x at 64x64**, and a sparse-marker text actor improves **2.33x**. Direct long-plain-text scanning improves **5.51x**. Four-, five-, and eight-lane results are controls, with no claimed algorithmic speedup.

Small controls are included rather than hidden: direct 14-byte plain-text scanning regresses about 2.9 cycles / 1.3 ns, and dense text actor construction regresses about 281 cycles / 0.127 us (4.2%). The builder's unchanged short-plain-text path has no retained algorithmic change. The text optimization targets long scans; it does not guarantee faster short strings.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Process ratio range | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `density/10/kind=0/bytes=90368` | 208030.58 | 150635.73 | 1.381x | 1.186-1.629x | 95071.50 | 68757.00 |
| `density/10/kind=1/bytes=90368` | 222849.02 | 146571.12 | 1.520x | 1.274-1.761x | 101703.25 | 66843.50 |
| `density/10/kind=2/bytes=90368` | 190862.39 | 127670.52 | 1.495x | 1.289-1.747x | 87083.50 | 58245.25 |
| `density/4/kind=0/bytes=41216` | 84190.32 | 77767.75 | 1.083x | 0.987-1.215x | 38423.25 | 35467.25 |
| `density/4/kind=1/bytes=41216` | 86005.04 | 82195.62 | 1.046x | 0.900-1.227x | 39227.50 | 37496.75 |
| `density/4/kind=2/bytes=41216` | 89423.76 | 83138.36 | 1.076x | 0.879-1.265x | 40911.50 | 37936.00 |
| `density/5/kind=0/bytes=49408` | 115855.39 | 100545.82 | 1.152x | 0.931-1.307x | 52859.50 | 45823.00 |
| `density/5/kind=1/bytes=49408` | 111637.70 | 107882.60 | 1.035x | 0.974-1.267x | 50980.50 | 49215.25 |
| `density/5/kind=2/bytes=49408` | 102415.42 | 100314.80 | 1.021x | 0.876-1.332x | 46707.50 | 45751.50 |
| `density/8/kind=0/bytes=73984` | 137481.08 | 129446.29 | 1.062x | 0.949-1.140x | 62710.00 | 59049.00 |
| `density/8/kind=1/bytes=73984` | 136275.48 | 122186.87 | 1.115x | 1.017-1.231x | 62201.00 | 55729.25 |
| `density/8/kind=2/bytes=73984` | 117466.52 | 105479.08 | 1.114x | 0.978-1.257x | 53580.25 | 48123.00 |
| `markers/alias/bytes=13` | 319.24 | 321.06 | 0.994x | 0.929-1.053x | 145.66 | 146.34 |
| `markers/dense/bytes=624` | 6722.11 | 6643.27 | 1.012x | 0.946-1.076x | 3066.53 | 3030.89 |
| `markers/numeric/bytes=27` | 384.18 | 393.14 | 0.977x | 0.772-0.999x | 175.18 | 179.41 |
| `markers/plain-long/bytes=704` | 844.37 | 153.34 | 5.506x | 4.301-5.914x | 385.17 | 69.89 |
| `markers/plain-short/bytes=14` | 27.20 | 30.12 | 0.903x | 0.792-1.091x | 12.36 | 13.69 |
| `markers/sparse/bytes=2056` | 2716.66 | 768.06 | 3.537x | 3.038-3.680x | 1239.84 | 350.39 |
| `markers/unknown/bytes=2050` | 6082.80 | 2530.80 | 2.404x | 2.136-2.432x | 2775.22 | 1154.26 |
| `markers/unterminated/bytes=2049` | 3276.68 | 122.11 | 26.834x | 21.455-28.886x | 1494.98 | 55.68 |
| `opacity/1920/kind=0` | 1763090.95 | 1148417.42 | 1.535x | 1.502-1.601x | 804679.75 | 524564.00 |
| `opacity/1920/kind=1` | 8.03 | 7.13 | 1.125x | 0.914-1.212x | 3.67 | 3.25 |
| `opacity/1920/kind=2` | 921040.12 | 577978.64 | 1.594x | 1.473-1.695x | 420435.00 | 264100.00 |
| `opacity/1920/kind=3` | 1848667.96 | 1150409.92 | 1.607x | 1.442-1.701x | 843981.75 | 525274.75 |
| `opacity/512/kind=0` | 121401.05 | 79748.20 | 1.522x | 1.457-1.716x | 55369.00 | 36383.50 |
| `opacity/512/kind=1` | 7.77 | 7.09 | 1.096x | 1.028-1.198x | 3.54 | 3.23 |
| `opacity/512/kind=2` | 63343.85 | 40115.27 | 1.579x | 1.526-1.636x | 28886.75 | 18312.00 |
| `opacity/512/kind=3` | 136077.93 | 80549.91 | 1.689x | 1.458-1.756x | 62077.25 | 36761.00 |
| `opacity/64/kind=0` | 1882.35 | 671.47 | 2.803x | 2.139-3.141x | 859.94 | 306.07 |
| `opacity/64/kind=1` | 8.06 | 7.21 | 1.119x | 0.923-1.481x | 3.67 | 3.29 |
| `opacity/64/kind=2` | 963.21 | 375.48 | 2.565x | 2.264-2.945x | 438.95 | 171.35 |
| `opacity/64/kind=3` | 1797.31 | 664.08 | 2.706x | 2.374-3.195x | 819.75 | 302.93 |
| `text-build/alias/bytes=13` | 406.03 | 379.17 | 1.071x | 0.874-1.096x | 185.26 | 172.93 |
| `text-build/dense/bytes=624` | 6651.24 | 6932.12 | 0.959x | 0.916-1.038x | 3036.00 | 3162.76 |
| `text-build/numeric/bytes=27` | 500.78 | 481.05 | 1.041x | 0.905-1.892x | 228.28 | 219.42 |
| `text-build/plain-long/bytes=704` | 228.30 | 196.25 | 1.163x | 0.931-1.180x | 104.06 | 89.44 |
| `text-build/plain-short/bytes=14` | 83.56 | 68.94 | 1.212x | 0.930-1.327x | 38.09 | 31.37 |
| `text-build/sparse/bytes=2056` | 3034.63 | 1304.16 | 2.327x | 2.035-2.440x | 1383.79 | 595.40 |
| `text-build/unknown/bytes=2050` | 6227.04 | 3569.47 | 1.745x | 1.562-1.828x | 2841.20 | 1628.27 |
| `text-build/unterminated/bytes=2049` | 3182.21 | 201.47 | 15.795x | 14.838-16.757x | 1460.55 | 91.83 |

[Raw production measurements for all 320 per-process measurements](2026-10-25-direct-scans.tsv) are retained for independent recalculation.

## Allocations and correctness

Allocation probes retain output through the end of counting; output destruction is excluded. Counts and requested bytes are unchanged. The ten-lane density operation allocates once for its returned vector (4,112 bytes); the normal lane controls request 4,128 / 4,120 / 4,112 bytes for 4 / 5 / 8 lanes. Text without a valid replacement allocates zero times. The alias, numeric, sparse, and dense fixtures allocate one output string requesting 13 / 27 / 2,056 / 624 bytes respectively. Both direct scanning and actor construction have these counts, with no reallocations. Opacity scanning has no allocation calls, and a scoped allocator test verifies no churn in the production scan for opaque and early/middle/late transparent images.

The new differential checks cover every byte in every lane position, 6,000 deterministic malformed chart inputs, all alpha values, complete/partial backing-buffer tails, and 4,913 combinations of aliases, numeric overflow, invalid Unicode codepoints, repeated delimiters, UTF-8, NUL, and newline. Text checks compare both contents and borrowed/owned results against the original. Existing stream tests additionally exercise density caps and run progress; existing renderer tests verify pixel behavior.

The final release run passed **1,951 tests** in 27 test binaries across eight packages; 53 optional benchmark tests remained ignored. The benchmarks were run explicitly from their archived binaries. Clippy completed successfully with existing warnings elsewhere in the repository; no diagnostics identified the changed kernels or their new test files. Targeted formatting and `git diff --check` also passed.

## Reproduction

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-core -p deadsync-rules -p deadsync-chart -p deadsync-gameplay -p deadsync-simfile -p deadlib-assets -p deadlib-render-backend-software -p deadlib-present --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-rules -p deadlib-present -p deadlib-render-backend-software --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadsync-rules -p deadlib-assets -p deadlib-render-backend-software -p deadlib-present --lib --tests --no-run
```

Run the density and opacity benchmarks serially by invoking their release library test executable with `benchmark_density_kernel` or `benchmark_opacity_kernel`, then `--ignored --nocapture --test-threads=1`. Invoke the `marker_kernel` integration executable with `benchmark_marker_kernel` and the same flags.

For comparisons between revisions, archive the two library test executables as `deadsync_rules.exe` / `deadlib_render_backend_software.exe` and the marker integration executable as `marker_kernel.exe` in separate directories, compiling with identical options and fixtures. Use separate Cargo target directories, or invalidate source timestamps in both checkouts when sharing one; Cargo can otherwise reuse relocated workspace artifacts. Then run:

```powershell
python scripts/benchmark_direct_kernels.py --before target/direct-kernels/final-before --after target/direct-kernels/final-after --output docs/performance/2026-10-25-direct-scans.tsv --runs 4
```

Local build/suite/lint logs, archived executables, and per-process output are under `target/direct-kernels`. The script reports every workload and retains allocation probe output in its process logs.
