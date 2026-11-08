# Notefield judgment rows, tick alpha and combo zoom — 2026-11-04

Baseline: `1afea83f573e59a4d337327006de9b289b2dbb46`. Three safe Rust changes remove repeated work in HUD drawing:

1. `tap_judgment_rows` resolves the grade once, returns legacy/non-Fantastic rows directly, and evaluates FA+ options only for a seven-row Fantastic judgment. This removes a second grade match and two redundant conditions.
2. `error_bar_tick_alpha` classifies nonnegative finite ages by their bits, explicitly accepting negative zero. Its fade arithmetic and duration handling are unchanged.
3. `combo_actor_zoom` replaces two floating validations with the positive finite nonzero bit interval. Its `0.5_f32.powf(mini)` expression and fallback remain unchanged. The front judgment-zoom caller benefits without a body change.

The three production bodies contain ten fewer lines overall, including the new comments. There are no new dependencies, fields, unsafe blocks or public signatures. Other production code remains unchanged. Test-only visibility lets the new HUD tests share the existing allocation counter rather than install a second global allocator.

## Final measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1; rustc 1.99.1, LLVM 23.1.1. Release kernels use opt-level 3, fat LTO and the static CRT. Each of five fresh processes alternates original/current sampling on CPU 2 for twelve rounds, drops the first, and takes the median of eleven. A sample runs 1,000 fixtures; each fixture processes 512 black-boxed values or parameter records. Function pointers are black-boxed once per fixture. Fixture construction and allocation counting remain outside timing.

The two CPU columns are independent process medians; throughput is the median of five paired original/current cycle ratios. These can differ from the ratio of the CPU columns. TSVs retain paired nanoseconds and allocation counters. Measurements cover these kernels, not whole-application frame rate.

| Kernel / input set | Original cycles / 512 | Current cycles / 512 | Paired throughput |
| --- | ---: | ---: | ---: |
| Combo zoom / ordinary | 10898.39 | 7617.75 | 1.387× |
| Combo zoom / inactive | 4849.19 | 2516.79 | 1.930× |
| Combo zoom / zero | 3977.34 | 1956.84 | 1.963× |
| Combo zoom / saturated | 9965.30 | 7126.73 | 1.398× |
| Combo zoom / exceptional | 6033.18 | 3886.47 | 1.479× |
| Front judgment caller / ordinary | 10021.27 | 7103.90 | 1.377× |
| Front judgment caller / inactive | 4765.78 | 2419.55 | 2.002× |
| Front judgment caller / zero | 4141.74 | 2043.77 | 1.999× |
| Front judgment caller / saturated | 8499.92 | 6252.68 | 1.359× |
| Front judgment caller / exceptional | 5961.84 | 3722.94 | 1.475× |
| Judgment rows / ordinary | 12721.78 | 11972.19 | 1.065× |
| Judgment rows / inactive | 10617.43 | 8447.46 | 1.281× |
| Judgment rows / zero | 10057.71 | 8477.53 | 1.280× |
| Judgment rows / saturated | 7773.81 | 7135.07 | 1.050× |
| Judgment rows / exceptional | 11327.08 | 8619.76 | 1.281× |
| Tick alpha / ordinary | 7125.63 | 4572.19 | 1.558× |
| Tick alpha / inactive | 3957.59 | 3341.89 | 1.157× |
| Tick alpha / zero | 5485.52 | 4399.66 | 1.259× |
| Tick alpha / saturated | 5807.53 | 3949.90 | 1.539× |
| Tick alpha / exceptional | 5136.08 | 4630.57 | 1.152× |

Ordinary floats span approximately 0–0.77; inactive values cycle through negative zero and seven negative values; zero alternates both signed zeros; saturated values span approximately 1–16; exceptional values include infinities, NaNs, subnormals and large magnitudes. Ordinary row fixtures use Fantastic judgments with varied FA+ flags. Other row fixtures mix all six grades; the saturated set uses legacy six-row sprites. Tick fixtures use a 0.3-second duration and alternate single/multiple-tick mode. Correctness tests exercise many more durations and flags than these timing fixtures.

All 30 workloads and both variants have zero allocations, reallocations, frees, allocated/freed bytes and peak bytes. The ten unchanged glow/back-judgment controls in the final binary span 0.978×–1.031×. A separate unchanged-source five-process control spans **0.939×–1.016×** across its workload medians, with individual pairs **0.901×–1.094×**. Small changes, including the 5–6.5% ordinary/legacy row results, are inconclusive near that spread. Mixed seven-row judgment throughput improves about 28%. Every paired range and raw-log hash is retained in the checks JSON.

## Exact behavior and validation

The tick guard accepts exactly bit patterns `0..=0x7f7fffff` plus `0x80000000`: all nonnegative finite f32 values and negative zero. Other negatives, infinities and NaNs still return positive zero before duration logic. In particular, multi-tick ages below 0.03 still return one even for unusual durations. Combo validation accepts exactly encodings `1..0x7f800000`; either signed zero, negatives and nonfinite inputs retain the fallback one. Arithmetic in both functions is unchanged.

Judgment tests compare all six grades, seven optional-window states, four sprite row counts including `usize::MAX`, all sixteen FA+ flag combinations, and source error values around 10/15ms with both signs, neighboring encodings, exceptional values and generated values. An early non-Fantastic return is equivalent because its old overlay was always absent. Fantastic base-row selection is the same boolean choice, with already-proven grade/row conditions removed.

- Four focused tests pass in debug and release. They cover **100,540 f32 encodings**, **561,792 judgment parameter sets**, and **29,036 boundary parameter sets**. The boundary function is an unchanged control; all six returned array elements are compared as bits, including unused zeros and NaN payloads.
- All eight release kernel stages produce the same **17,558,864-byte** trace, SHA-256 `7112d3cae675a401f7655aacf15ada852c546302c0e5c5d7ad7f9ede26c12d85`. It includes float input bits and exact outputs; structured row/boundary input generators are frozen in source snapshots.
- Original and optimized debug produce the same trace, SHA-256 `f7556a28038bca77388aceac3975be3dc58abfd16c36d7e19c4392fce93e80c8`. A preliminary cross-profile comparison identified a pre-existing difference in `powf` at input bits `0x43156da5` (149.4282989501953): release returns subnormal bits `0x00000001`, debug returns zero. Only combo zoom and its front-judgment caller differ at that input; all other trace words are identical. Both optimized profiles preserve their own original baseline exactly. The runner now selects a baseline from the matching profile.
- The full notefield release suite passes **496 tests**, with seven benchmark/trace tests ignored. The normal release library check also passes.
- Focused Clippy passes with warnings denied. All stage source snapshots, exact extracted functions/types/constants, generated harnesses, executables, trace hashes and raw timing rows are audited. Selected Rust formatting, Python compilation and whitespace checks pass.

Full testing reuses the earlier isolated dependency workspace, restores shipped PNGs and snapshots the sixteen existing JSON fixtures absent from Git. Full-suite reproduction needs those fixtures; `DEADSYNC_NOTEFIELD_FIXTURE_ROOT` can identify their checkout. The main checkout is read only. Builds use temporary storage on D:, with normal host temporary storage retained for actual tests. Source archive, overlay, fixture, compiler, executable and log hashes are in the full-release manifest.

## Rejected candidates and evidence

Ten successful stages are retained: `baseline`, `rows-dispatch`, `zoom-guard`, `glow-bits`, `tick-guard-mask`, `combo-guard`, `final`, `control`, `baseline-debug`, `debug`. Exploratory release stages use three processes; final/control use five. The first five release stages have 24 workloads; `combo-guard` adds explicit signed-zero fixtures for 30. All use the same correctness corpus and trace format.

The judgment-back zoom bit guard slowed ordinary/saturated inputs to 0.924×/0.893×, so its body was restored. A branch-based glow shortcut improved ordinary inputs but regressed inactive/exceptional inputs to 0.761×/0.812×. A branchless integer-mask variant had no consistent gain. Both glow candidates were removed. A first attempt to use integer `min` in the const glow helper failed to compile; the tested variant used a const-compatible conditional and still lost to the original. No glow, back-judgment or boundary optimization is retained.

The post-measurement driver change only selects the trace reference by profile. The audit verifies that the remaining driver AST, generated timed Rust and measured source snapshots are unchanged. The failed cross-profile comparison did not require a production change.

## Reproduction

Use a fresh archive directory to preserve existing artifacts:

```powershell
$env:DEADSYNC_NOTEFIELD_HUD_ARCHIVE='D:\deadsync-notefield-hud-reproduction'
python scripts/benchmark_notefield_hud.py baseline --baseline 1afea83f573e59a4d337327006de9b289b2dbb46
python scripts/benchmark_notefield_hud.py final --runs 5
python scripts/benchmark_notefield_hud.py control --runs 5 --baseline 1afea83f573e59a4d337327006de9b289b2dbb46
python scripts/benchmark_notefield_hud.py baseline-debug --debug --baseline 1afea83f573e59a4d337327006de9b289b2dbb46
python scripts/benchmark_notefield_hud.py debug --debug
python scripts/benchmark_notefield_hud.py full-release --full
```

Kernel reproduction uses tracked source and rustc. Full testing also reuses the source archive referenced by the earlier partner-chart full-release manifest. Measured local artifacts live at `D:\deadsync-notefield-hud-20261104`; tracked manifests preserve their hashes and exact commands. See [`final.tsv`](2026-11-04-notefield-hud-final.tsv), [`control.tsv`](2026-11-04-notefield-hud-control.tsv), the stage manifests sharing this prefix, and [`checks.json`](2026-11-04-notefield-hud-checks.json).
