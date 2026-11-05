# Texture luminance and single-axis resizing — 2026-11-04

Baseline: `a2d215ee50f2a90cf7424e9c314b6fd8380704a1`. Three safe Rust optimizations change two existing production functions:

1. Luminance uses u16 weighted arithmetic before widening its result to u32. The largest sum is 255 × (30 + 59 + 11) = 25,500, so every intermediate fits. This reduces arithmetic width without changing the integer division.
2. Vertical-only resize passes blend two source pixels directly and stream their row coordinates. This skips horizontal blending, its intermediate truncation, and its coordinate Vec.
3. Horizontal-only resize passes blend two source pixels directly. This skips the vertical blend while retaining horizontal truncation and the existing coordinate table.

The two resize cases share one local two-pixel blend closure. At source level each output uses four packed products instead of twelve and reads two source pixels instead of four. The existing general and exact-half paths remain. Vertical rounding and horizontal truncation stay distinct. The production change adds 44 lines inside zoom_image and six inside apply_texture_hints, plus three test-registration lines. Public APIs, types, fields and dependencies are unchanged; no unsafe blocks are added. This adds explicit arithmetic paths to remove work from the pixel loop.

## Measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1, GenuineIntel, rustc 1.99.1 / LLVM 23.1.1, opt-level 3, fat LTO and static CRT. Five fresh processes use twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Zoom performs 100 fixtures per sample; luminance performs 16–100,000 according to image size. Fixture creation stays outside timing. Zoom includes the identical input clone, intermediate allocations, output construction and destruction. Luminance transforms identical evolving buffers. Function pointers and arguments are black-boxed. Background development builds were present; thread CPU cycles and identical-source controls provide the comparison, with raw nanoseconds retained in the TSVs.

Cycle columns are independent process medians. Throughput is the median of paired original/current ratios. These are texture-preparation kernels, not whole-application startup or GPU throughput results.

| Workload | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `luma/alpha/1` | 17.70 | 18.15 | 0.988× |
| `luma/alpha/1024` | 2246953.00 | 2029524.38 | 1.107× |
| `luma/alpha/16` | 561.87 | 521.13 | 1.075× |
| `luma/alpha/256` | 137172.87 | 126132.00 | 1.089× |
| `luma/both/1` | 17.67 | 17.73 | 1.011× |
| `luma/both/1024` | 2257283.19 | 2048374.00 | 1.104× |
| `luma/both/16` | 555.44 | 520.91 | 1.069× |
| `luma/both/256` | 137948.40 | 127566.07 | 1.087× |
| `luma/gray/1` | 19.73 | 19.77 | 1.006× |
| `luma/gray/1024` | 3055700.69 | 2576861.38 | 1.184× |
| `luma/gray/16` | 765.05 | 650.38 | 1.183× |
| `luma/gray/256` | 191125.93 | 159781.37 | 1.193× |
| `luma/none/1` | 10.45 | 10.12 | 1.027× |
| `luma/none/1024` | 10.38 | 10.12 | 1.025× |
| `luma/none/16` | 10.33 | 10.22 | 1.011× |
| `luma/none/256` | 10.38 | 10.06 | 1.018× |
| `zoom/1024x512/1024x64` | 12949046.86 | 6468649.58 | 1.973× |
| `zoom/1024x512/128x512` | 12807129.20 | 8423323.53 | 1.528× |
| `zoom/1024x512/64x32` | 2567871.22 | 2575185.00 | 0.999× |
| `zoom/16x16/16x32` | 13505.84 | 5573.10 | 2.380× |
| `zoom/16x16/32x16` | 13545.35 | 8420.02 | 1.613× |
| `zoom/1x256/1x32` | 12796.84 | 10239.67 | 1.262× |
| `zoom/256x1/32x1` | 11234.01 | 8977.55 | 1.250× |
| `zoom/256x256/128x128` | 117164.71 | 118205.14 | 0.988× |
| `zoom/256x256/256x256` | 46731.55 | 46946.66 | 0.997× |
| `zoom/256x256/256x512` | 3004897.99 | 1222972.77 | 2.471× |
| `zoom/256x256/512x256` | 3095145.38 | 1888397.96 | 1.617× |
| `zoom/256x256/512x512` | 6437456.52 | 6192538.37 | 1.040× |
| `zoom/257x129/129x129` | 405585.53 | 246292.17 | 1.643× |
| `zoom/257x129/257x65` | 404404.61 | 167166.82 | 2.398× |
| `zoom/257x129/513x259` | 5987874.40 | 4096502.12 | 1.471× |

Identical-source control medians span **0.985×–1.026×**, with individual process pairs 0.928×–1.146×. Changes near that spread are inconclusive. In particular, no gain is claimed for no-op luminance, one-pixel luminance, no-op resize or the unchanged two-axis exact-half path. The 257×129 → 513×259 fixture reaches its width first and uses the vertical shortcut on its final pass. The ordinary 256×256 → 512×512 fixture remains on the general path; its 1.040× result is treated as placement/sampling variation, not another optimization.

## Allocations

All sixteen luminance fixtures remain allocation-free. Horizontal-only resize counts match in all six tracked fields. Vertical-only passes remove one coordinate-table allocation each; output image dimensions, pixels and capacities remain exact.

| Fixture | Allocs | Reallocs | Frees | Allocated bytes | Freed bytes | Peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `zoom/1024x512/1024x64` | 7 → 4 | 0 → 0 | 7 → 4 | 3969024 → 3932160 | 3969024 → 3932160 | 3158016 → 3145728 |
| `zoom/16x16/16x32` | 3 → 2 | 0 → 0 | 3 → 2 | 3264 → 3072 | 3264 → 3072 | 3264 → 3072 |
| `zoom/1x256/1x32` | 7 → 4 | 0 → 0 | 7 → 4 | 1956 → 1920 | 1956 → 1920 | 1548 → 1536 |
| `zoom/256x256/256x512` | 3 → 2 | 0 → 0 | 3 → 2 | 789504 → 786432 | 789504 → 786432 | 789504 → 786432 |
| `zoom/257x129/257x65` | 3 → 2 | 0 → 0 | 3 → 2 | 202516 → 199432 | 202516 → 199432 | 202516 → 199432 |
| `zoom/257x129/513x259` | 5 → 4 | 0 → 0 | 5 → 4 | 1205808 → 1199652 | 1205808 → 1199652 | 1067040 → 1060884 |

The one-pass 256×256 → 256×512 case drops three allocations to two and removes 3,072 allocated bytes. Three-pass 1024×512 → 1024×64 drops seven allocations to four, removes 36,864 allocated bytes and reduces peak storage by 12,288 bytes. Input cloning is counted in both.

## Exact behavior and checks

- **112 full release tests pass**, with 22 ignored benchmark/fixture tests. Four new focused tests pass in release and debug; focused Clippy passes with warnings denied, and the production-only release check passes.
- Exhaustive RGB verification checks 67,108,864 RGB/hint combinations, varying alpha, plus 272 incomplete/oversized storage tails. All bytes and in-place capacities match. Luminance still transforms every complete four-byte chunk, including storage beyond logical dimensions, and leaves incomplete tails unchanged.
- Resize verification compares 491 images: tiny axes, odd sizes, up/down and multi-pass resizes, mixed-axis resizes, all 65,536 byte pairs at 2 → 3/4 weights, excess storage and exact output capacities.
- The unchanged axis shortcut is limited to dimensions **at most 2^22**. Every coordinate through this limit is checked for identity and an in-bounds right sample. Beyond it, f32 rounding can make the right coordinate out of bounds even when its weight is zero. Both orientations at 2^22 + 1 preserve the original bounds panic and message through the general path. The earlier 2^23 trial limit was rejected during review before final timing.
- Original/current values agree within both profiles. Release and debug traces agree byte-for-byte: **276,312,335 bytes**, SHA-256 `ce67d9d89cae5dbc4dba7704a7fb78016704b2ebba6e58eeaffcc8caacf879b5`. Traces serialize pixels and dimensions, not the intentionally different allocation counts.
- The initial full run passed 111 tests before the tightened guard and fourth regression test. `checked-full-release`, `final`, `control`, and `final-debug` are authoritative for current source. Earlier completed baseline/debug artifacts remain immutable. A first debug link using cached bitcode without LTO failed; the recorded debug stages link with fat LTO and opt-level 0 for these Rust functions, using the cached release image dependency.

The frozen reference is extracted from the baseline's four texture functions; its only functional-access change makes zoom_image visible to its enclosing test module. The harness uses the actual image crate and shared, source-extracted TextureHints and sampler enum fields. Full-package tests exercise the real crate types and callers. `2026-11-04-texture-axis-checks.json` audits source scope, snapshots, compiled fragments, binary/log/trace hashes, every raw measurement and all six allocation counters.

## Reproduction

From the isolated worktree in PowerShell, set `PYTHONUTF8=1` and `DEADSYNC_TEXTURE_AXIS_ARCHIVE=D:\deadsync-texture-axis-20261104`, then run:

```powershell
python scripts/benchmark_texture_axis.py checked-full-release --full
python scripts/benchmark_texture_axis.py final --runs 5 --reference baseline
python scripts/benchmark_texture_axis.py control --baseline a2d215ee50f2a90cf7424e9c314b6fd8380704a1 --runs 5 --reference baseline
python scripts/benchmark_texture_axis.py final-debug --debug --reference baseline
```

The full driver reuses the prior partner-chart source archive after checking its SHA-256, extends its Git path-dependency closure and overlays current source. Shipped cel PNGs and both `itgmania-primary-bitmaps` and `itgmania-song-lua-micro` fixture directories are snapshotted and hashed from the main checkout; `DEADSYNC_NOTEFIELD_FIXTURE_ROOT` can supply that fixture root. These existing fixture names do not indicate Lua code changes. Compilation uses D: temporary files; full tests use the host C: temporary directory. External fixture file count: 859.

Focused builds require the image rlib and transitive dependency directory recorded in each manifest. Override `DEADSYNC_IMAGE_DEPS` and `DEADSYNC_IMAGE_RLIB` for another matching cached build. Its SHA-256 is recorded. Source snapshots, compiler context, executables, test logs, raw cycle/allocation logs and traces are under `D:\deadsync-texture-axis-20261104`. Raw TSVs and manifests are checked in beside this report. No full-workspace or cross-platform throughput gain is claimed.
