# Software byte writes, half-float quantization and contiguous copies — 2026-11-04

Baseline: `ecfd04050aa748c724b182cf6835924ca7baa9da`. Three safe private-path changes:

1. Quantize nonnegative binary16 values below one through a **15 KiB** byte table. Larger encodings retain explicit saturation/zero handling. This removes the per-channel integer multiply, variable shift and rounding bias from the capture loop.
2. Copy equal-stride depth and half-float rows as one contiguous span. A shared private helper preserves the original complete-row truncation, unequal strides and backing padding. Byte color conversion and viewport policy retain their behavior.
3. Return from depth-only byte writes after preserving the destination bounds check. NoEffect retains destination color/alpha, so unpacking, input clamping, blending and repacking are redundant. Rasterization still updates depth. The existing opaque-alpha fast path remains first.

The one static occupies **15,360 bytes (15 KiB)**, requires no runtime initialization and adds no heap allocations. Its application cache/RSS impact is unmeasured. No dependencies, public fields, public parameters or unsafe production code are added. Quantization work moves into compile-time table construction, and the two repeated row loops share one implementation. The scalar half decoder is restored exactly to the Git baseline.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.99.1 (58cf8f9c3 2026-10-13). Real Cargo-built package/integration binaries use release opt-level 3, fat LTO and the static CRT. Five fresh processes per benchmark family execute twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Function arguments and helper function pointers are black-boxed. Existing development builds may still run; our formal CPU benchmarks run sequentially.

Cycle columns are independent process medians. Paired throughput is the median of the five within-process ratios. The control column runs identical production sources and therefore exposes compiler-context and call-site bias as well as timing variation. Raw TSVs also retain nanoseconds and every allocation field. Small raw ratios must be assessed against their specific controls.

Helper fixtures process 1,024 values for 2,000 repetitions. Blend/writer fixtures process 255/256 colors for 2,000 repetitions. Writer buffers evolve identically on both sides, including saturation and underflow after repeated blends. Row fixtures repeat 1,000 times. Capture fixtures time the complete copy_target_pixels call after rasterization and before presentation, with PRESERVE_ALPHA=true; correctness checks cover both policies. These scale from 4 to 4,000 repetitions by image size. Depth-raster fixtures run the real textured triangle rasterizer, nearest sampling, depth test/write and byte writer, including depth-buffer reset; iteration counts are 1,000 at 16² and 122 at 64². Allocations, fixture construction, file output and correctness comparisons remain outside timing.

`copy/size/float/depth/full` identifies the backing size, color format, depth flag and full-width viewport. `rows/width/height` measures the shared copy kernel. `half-byte` modes are 0=arbitrary bits, 1=positive normal values, 2=positive zero, 3=one, 4=negative encodings. `unpack`, `pack`, `blend` and `half-write` are unchanged-code controls after the decoder candidate was removed.

Depth-only byte writes measure **8.962×** paired throughput (control **1.002×**). Complete depth-only raster cases measure **1.094×–1.096×** (controls **0.965×–0.973×**). The eight large floating-point capture cases measure **2.110×–2.261×** (controls **0.993×–1.008×**). Full-width byte/depth capture at 16² measures **1.183×**, against its **0.990×** control. The 4×256 row-copy fixture measures **5.072×**, against **0.942×** control. These support the retained paths; they do not establish a full-application speedup.

| Workload | Frozen reference cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `blend/Add` | 6036.69 | 6043.16 | 1.012× | 0.984× |
| `blend/Alpha` | 6162.79 | 6337.73 | 0.983× | 1.005× |
| `blend/Multiply` | 4990.99 | 5091.19 | 1.001× | 1.012× |
| `blend/NoEffect` | 5017.77 | 4968.16 | 1.005× | 0.984× |
| `blend/Subtract` | 6291.20 | 6237.86 | 0.984× | 0.996× |
| `byte-write/Add` | 10942.40 | 11105.82 | 0.997× | 0.998× |
| `byte-write/Alpha` | 11714.71 | 11626.26 | 0.995× | 1.000× |
| `byte-write/AlphaOpaque` | 8766.94 | 8877.90 | 1.003× | 1.001× |
| `byte-write/AlphaTransparent` | 13176.15 | 12965.21 | 1.003× | 1.005× |
| `byte-write/Multiply` | 10746.28 | 10559.38 | 0.994× | 0.998× |
| `byte-write/NoEffect` | 9828.55 | 1092.89 | 8.962× | 1.002× |
| `byte-write/Subtract` | 12490.87 | 12368.06 | 1.010× | 1.000× |
| `copy/1024/false/false/false` | 1452376.50 | 1448316.00 | 1.009× | 1.003× |
| `copy/1024/false/false/true` | 2912051.50 | 2901570.50 | 0.999× | 1.000× |
| `copy/1024/false/true/false` | 1928417.25 | 1904820.75 | 1.012× | 1.001× |
| `copy/1024/false/true/true` | 3662741.50 | 3533675.50 | 1.057× | 1.008× |
| `copy/1024/true/false/false` | 48737011.75 | 21738236.75 | 2.242× | 0.999× |
| `copy/1024/true/false/true` | 50071407.25 | 23265354.50 | 2.139× | 1.003× |
| `copy/1024/true/true/false` | 50603694.00 | 22652784.50 | 2.212× | 1.008× |
| `copy/1024/true/true/true` | 52031157.00 | 25570871.75 | 2.110× | 0.995× |
| `copy/16/false/false/false` | 457.16 | 476.26 | 0.995× | 1.005× |
| `copy/16/false/false/true` | 732.80 | 704.21 | 0.980× | 0.999× |
| `copy/16/false/true/false` | 557.26 | 563.62 | 0.989× | 0.995× |
| `copy/16/false/true/true` | 876.35 | 740.92 | 1.183× | 0.990× |
| `copy/16/true/false/false` | 2433.82 | 2454.12 | 0.990× | 1.003× |
| `copy/16/true/false/true` | 2133.59 | 2120.92 | 1.006× | 1.055× |
| `copy/16/true/true/false` | 2507.02 | 2521.01 | 0.962× | 1.000× |
| `copy/16/true/true/true` | 2475.03 | 2242.80 | 1.041× | 1.042× |
| `copy/256/false/false/false` | 94726.85 | 96141.00 | 0.985× | 0.997× |
| `copy/256/false/false/true` | 228456.33 | 228690.21 | 0.999× | 0.991× |
| `copy/256/false/true/false` | 124312.57 | 124276.57 | 1.000× | 0.998× |
| `copy/256/false/true/true` | 240856.26 | 236444.69 | 1.004× | 1.001× |
| `copy/256/true/false/false` | 3316652.20 | 1490685.67 | 2.261× | 1.001× |
| `copy/256/true/false/true` | 3135399.18 | 1412039.93 | 2.250× | 1.008× |
| `copy/256/true/true/false` | 3112783.44 | 1383091.10 | 2.241× | 1.003× |
| `copy/256/true/true/true` | 3132430.49 | 1418423.39 | 2.211× | 0.993× |
| `depth-raster/16` | 14519.92 | 13029.08 | 1.094× | 0.973× |
| `depth-raster/64` | 247444.87 | 216959.56 | 1.096× | 0.965× |
| `half-byte/0` | 4903.08 | 5106.56 | 0.958× | 0.998× |
| `half-byte/1` | 7306.61 | 3931.90 | 1.838× | 1.010× |
| `half-byte/2` | 4860.06 | 3933.66 | 1.196× | 0.994× |
| `half-byte/3` | 5474.55 | 4201.56 | 1.354× | 0.992× |
| `half-byte/4` | 3285.26 | 3929.05 | 0.830× | 0.989× |
| `half-write/Add/false` | 8519.89 | 8552.38 | 0.999× | 1.004× |
| `half-write/Add/true` | 9201.99 | 9267.84 | 0.989× | 1.023× |
| `half-write/Alpha/false` | 8748.94 | 8787.90 | 1.001× | 0.998× |
| `half-write/Alpha/true` | 9851.93 | 9718.69 | 1.014× | 0.979× |
| `half-write/Multiply/false` | 9220.87 | 9351.25 | 0.976× | 0.984× |
| `half-write/Multiply/true` | 10163.07 | 10517.01 | 0.970× | 1.028× |
| `half-write/NoEffect/false` | 7005.45 | 7052.20 | 0.996× | 0.976× |
| `half-write/NoEffect/true` | 7607.65 | 7609.63 | 1.003× | 1.001× |
| `half-write/Subtract/false` | 8619.98 | 8606.49 | 0.993× | 0.991× |
| `half-write/Subtract/true` | 9614.87 | 9656.90 | 1.009× | 1.019× |
| `pack/0` | 18651.57 | 18322.21 | 1.018× | 0.996× |
| `pack/1` | 27241.16 | 26933.20 | 1.015× | 1.015× |
| `pack/2` | 21734.67 | 21204.36 | 1.025× | 0.986× |
| `pack/3` | 19053.92 | 18859.77 | 1.003× | 1.007× |
| `pack/4` | 19230.50 | 18993.56 | 1.015× | 1.000× |
| `rows/1024/1` | 300.50 | 214.01 | 1.278× | 1.364× |
| `rows/1024/256` | 371663.10 | 326637.72 | 1.141× | 1.004× |
| `rows/16/1` | 32.27 | 23.05 | 1.400× | 0.974× |
| `rows/16/256` | 6237.09 | 2572.76 | 2.413× | 1.432× |
| `rows/256/1` | 82.75 | 74.63 | 1.109× | 1.118× |
| `rows/256/256` | 93116.51 | 85619.93 | 1.085× | 1.002× |
| `rows/4/1` | 28.97 | 17.78 | 1.588× | 1.000× |
| `rows/4/256` | 1089.16 | 220.60 | 5.072× | 0.942× |
| `unpack/control/0` | 20374.65 | 20300.79 | 1.003× | 0.997× |
| `unpack/control/1` | 21297.43 | 21221.48 | 0.996× | 0.999× |
| `unpack/control/2` | 16732.05 | 16646.66 | 1.018× | 0.996× |
| `unpack/control/3` | 14920.84 | 14927.54 | 1.004× | 1.003× |
| `unpack/control/4` | 15006.56 | 14906.24 | 1.001× | 0.999× |
| `unpack/control/5` | 19061.27 | 19627.80 | 1.009× | 0.993× |
| `unpack/current/0` | 22752.82 | 22373.85 | 0.973× | 0.983× |
| `unpack/current/1` | 21174.18 | 21998.51 | 0.962× | 0.969× |
| `unpack/current/2` | 16600.35 | 18309.81 | 0.904× | 0.916× |
| `unpack/current/3` | 15175.02 | 16933.00 | 0.897× | 0.915× |
| `unpack/current/4` | 14974.18 | 16608.80 | 0.903× | 0.894× |
| `unpack/current/5` | 19019.78 | 19480.08 | 0.967× | 0.964× |

Control medians span **0.894×–1.432×**, with individual process pairs **0.478×–1.573×**. Some tiny helper and standalone row comparisons have substantial compiler-context bias even before optimization. Their raw ratios alone cannot establish a gain or a regression. Unchanged pack, blend and unpack-control workloads are retained to expose this. The actual-path columns of both TSVs allow direct before/after inspection, subject to cross-process variation.

Retained low-ratio results are explicit below. The percentages describe each final reference pairing; a biased baseline pairing can already have that cost, so these are not automatically costs caused by this change:

- `copy/16/true/true/false`: final paired 0.962× (4.0% more cycles against its frozen reference); identical-source control 1.000×. Actual-path medians are 2656.94 cycles in the baseline and 2521.01 in the final build.
- `half-byte/0`: final paired 0.958× (4.4% more cycles against its frozen reference); identical-source control 0.998×. Actual-path medians are 6913.48 cycles in the baseline and 5106.56 in the final build.
- `half-byte/4`: final paired 0.830× (20.5% more cycles against its frozen reference); identical-source control 0.989×. Actual-path medians are 5226.40 cycles in the baseline and 3929.05 in the final build.

The negative-encoding helper difference is about **0.6 additional cycles per channel** from the final independent medians. Normal byte-writer modes measure 0.994×–1.010×, near their own controls; no normal-blend gain is claimed.

## Exact behavior and allocations

All six allocation counters match and remain zero for **all 78 workloads in every process**. Read-only static storage is additional memory even though allocation counters remain zero. No allocation savings are claimed.

Both release and package opt-level-zero checks cover the following actual test targets. The latter enables debug assertions and overflow checks for **deadlib-render-backend-software** while dependencies retain cached release builds. Each integration binary embeds the backend and repeats its 25 library tests; the **113 executions represent 38 distinct normal tests**, including five new differential corpora.

| Target | Release passed | Ignored | Opt-level-zero passed |
| --- | ---: | ---: | ---: |
| `deadlib_render_backend_software` | 25 | 3 | 25 |
| `half_kernels` | 28 | 7 | 28 |
| `texture_pipeline` | 30 | 6 | 30 |
| `pixel_paths` | 30 | 7 | 30 |

- Exhaust all **65,536 binary16 encodings**, both byte quantization and all f32 bits, with **262,144 packed lane placements**. Compare 131,072 additional arbitrary packed words and color pairs, signed zeros, infinities, NaNs and rounding boundaries. The old conversion bodies are frozen from the stated Git baseline.
- Compare **655,360 whole writer operations** over arbitrary input bits, both alpha policies and all five blend modes. Every packed result matches within each compiler configuration. The cross-profile writer trace uses finite arithmetic; arbitrary NaN arithmetic is still checked bit-for-bit within each profile rather than normalized away.
- Compare **7,605 row/storage combinations**, including incomplete tails and unequal strides, plus **160 complete viewport cases** across float/byte color, depth and alpha preservation. Compare all affected buffers, f32 depth bits, untouched padding and capacities. Existing half-kernel and texture-pipeline integration tests also pass.
- Compare **327,680 arbitrary-color byte writes**, 1,024 exhaustive byte/lane placements and all six depth policies on 16²/64² textured triangles. Destination pixels and every depth bit match; depth-writing policies still change depth. The original out-of-bounds panic message and unmodified buffer are checked.
- Four traces match exactly across the clean control, final release and opt-level-zero builds. The copy trace additionally serializes 120 complete target states. Source, compiler profiles, all artifact/log/trace hashes, every raw timing row and allocation field are checked by the accompanying audit JSON.

| Trace | Bytes | SHA-256 |
| --- | ---: | --- |
| half | 327,680 | `d2e899dc76e244e3ea07476316ed4157c22d4ff29ed10f26d7ed67055c53f169` |
| writer | 5,242,880 | `970a3834ce70b880ad63adac1ae34e1ad8c6d5bd6f9a1b90cfe5087384da90fd` |
| copy | 1,862,112 | `1109f95178f67d7eb8bff41499aa1cdb3be2013214e3e0f17f8de46ab308fafb` |
| byte | 1,419,264 | `54c00ceeebb84392ba6c5a99729d9dead51e59a103b9ace8d149a647b0fe50a9` |

The production-only release check passes. Scoped Clippy reports no diagnostics in changed functions/constants or new test logic; 73 existing diagnostics are recorded. The new integration binary follows the repository's embedded-backend pattern. It explicitly allows duplicate timing-helper modules because existing embedded unit tests keep their copy private; no production visibility is broadened.

## Discarded trials and retained records

Mode dispatch and the half-writer no-effect branch slowed common blend modes and were discarded. Grouped normal-only encode/decode paths improved selected helpers but penalized zeros, subnormals or evolving blend buffers. Forced inlining did not demonstrate a consistent standalone benefit. The retained byte-writer guard follows the existing opaque-alpha fast path and has separate normal-mode/control measurements.

**The half decode table was rejected for exactness.** Every standalone binary16 bit matched, and its opt-level-zero writer checks passed, but release whole-writer testing exposed packed NaN-alpha sign changes: `0x7e00fe00fe00fe00` versus `0xfe00fe00fe00fe00`. The complete failure artifacts and source hashes are preserved in rejected-decode.manifest.json. No NaN normalization was used to accept that change; both 128/256 KiB decode-table variants were withdrawn. The original scalar decoder remains unchanged, and the final arbitrary-bit writer checks pass in both configurations.

The exploratory full-table ablation briefly overlapped the first baseline's pinned CPU. Its numbers and that first control are not used as primary evidence; complete clean controls were rerun sequentially. Early manifests remain available for those discarded implementations.

The first baseline serialized arbitrary writer bits; subsequent cross-profile writer traces use finite arithmetic and retain the arbitrary-bit normal test. Half and copy trace domains remain unchanged. The final corpus adds the byte/depth trace and nine byte/raster workloads. All exploratory manifests and earlier controls remain preserved. The authoritative stages are **final-baseline**, **final-debug** and **final-release**. Raw data is never substituted with favorable individual rounds.

## Reproduction

```powershell
$env:PYTHONUTF8='1'
$env:DEADSYNC_SOFTWARE_PIXELS_ARCHIVE='D:\deadsync-software-pixels-repro'
python scripts/benchmark_software_pixels.py final-baseline --baseline ecfd04050aa748c724b182cf6835924ca7baa9da --runs 5 --reference final-baseline
python scripts/benchmark_software_pixels.py final-debug --debug --reference final-baseline
python scripts/benchmark_software_pixels.py final-release --runs 5 --reference final-baseline
```

The driver extends the verified partner-chart source archive with the package dependency closure and current source snapshots. It builds the library and the pixel_paths, half_kernels and texture_pipeline integration targets through Cargo. The baseline backfills the original row loop under the new private helper's test-only name, without changing baseline call sites. All baseline production bodies otherwise come from Git. Compilation uses D: temporary storage; actual tests use the host temporary directory. Binaries, source snapshots and logs remain under `D:\deadsync-software-pixels-20261104`. Lua source, device I/O and the user's main checkout are unchanged. No cross-platform or whole-application speedup is claimed.
