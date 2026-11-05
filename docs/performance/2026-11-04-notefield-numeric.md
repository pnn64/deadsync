# Notefield numeric quantization — 2026-11-04

Baseline: `12a94507058c8c47980f422eff8d4e8fe60bf900`. This batch retains three safe Rust changes in `deadsync-notefield/src/transforms.rs`: signed hundredths, unsigned hundredths, and the const-capable rounded i16 helper. Each uses a guarded half-unit addition and Rust's truncating, saturating integer cast in place of floating `round`. Unsigned input validation uses one interval of positive finite f64 encodings. `mod_percent_key` benefits through its existing call to the i16 helper; its body is unchanged.

These helpers produce text/cache keys and modifier values. Actual callers include the Simply Love notefield HUD's percentage and signed offset text. The results below measure the kernels, not frame rate or whole-application performance.

## Final measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1; rustc 1.99.1, LLVM 23.1.1. Release uses opt-level 3, fat LTO and the static CRT. Five fresh processes each run paired original/current samples on CPU 2 with alternating order: twelve rounds, discard the first, retain the median of eleven. Each sample performs 1,000 fixtures; one fixture processes 512 black-boxed values through a black-boxed function pointer selected before the value loop. Fixture construction and allocation instrumentation stay outside timing. The frozen original definitions come from the baseline above.

CPU columns are independent medians across the five processes. Throughput is the median of their paired original/current cycle ratios, so it need not equal the ratio of the two CPU columns. The TSVs also retain paired nanoseconds. Normal inputs cover signed ordinary values; positive inputs cover approximately 0–110; inactive inputs repeat signed zero, minimum normal and epsilon; exceptional inputs mix ordinary boundaries, saturation, infinities and NaNs. f64 timing inputs are widened from these f32 fixtures; the correctness corpus separately exercises native f64 boundaries.

| Kernel / input set | Original cycles / 512 | Current cycles / 512 | Paired throughput |
| --- | ---: | ---: | ---: |
| Signed hundredths / normal | 6426.52 | 4694.89 | 1.369× |
| Signed hundredths / positive | 6542.20 | 4533.11 | 1.434× |
| Signed hundredths / inactive | 6674.78 | 3376.79 | 1.947× |
| Signed hundredths / exceptional | 5994.98 | 4065.36 | 1.435× |
| Unsigned hundredths / normal | 7232.74 | 3578.51 | 2.043× |
| Unsigned hundredths / positive | 9093.23 | 3908.42 | 2.381× |
| Unsigned hundredths / inactive | 6488.86 | 3547.12 | 1.819× |
| Unsigned hundredths / exceptional | 6486.01 | 3396.54 | 1.861× |
| Rounded i16 / normal | 6013.86 | 3959.56 | 1.513× |
| Rounded i16 / positive | 6041.74 | 3763.33 | 1.576× |
| Rounded i16 / inactive | 6616.39 | 3639.53 | 1.823× |
| Rounded i16 / exceptional | 5971.50 | 3825.66 | 1.574× |
| Percentage-key caller / normal | 6293.72 | 4287.71 | 1.414× |
| Percentage-key caller / positive | 6350.79 | 4293.64 | 1.475× |
| Percentage-key caller / inactive | 6777.94 | 3713.28 | 1.772× |
| Percentage-key caller / exceptional | 5707.00 | 4118.48 | 1.467× |

All 24 workloads, both original and current, report zero allocations, reallocations, frees, allocated/freed bytes and peak bytes. The other eight workloads are unchanged activity/divisor controls: final activity medians span 0.969×–0.993× and divisor medians span 0.988×–1.018×. A separate five-process unchanged-source control spans 0.952×–1.031× across the 24 workload medians, with individual pairs 0.887×–1.119×. Small differences are inconclusive. The checks JSON includes every paired range and raw-log hash.

## Behavior and code scope

The signed helpers still reject nonfinite input before scaling. The unsigned guard accepts exactly the positive finite nonzero f64 bit interval; it rejects signed zeros, negative numbers, infinities and NaNs. Multiplication by 100 remains in the same position relative to validation. A finite input that overflows during multiplication still saturates to the integer limit, while an originally nonfinite input still returns zero.

The half-unit shortcut needs a guard: the floating predecessor of 0.5 can become 1.0 when adding 0.5, despite being below the original rounding threshold. Returning zero for magnitudes below 0.5 excludes that region. For other half-integer boundaries relevant to these integer result ranges, the input spacing immediately below the half-integer matches the spacing below the following integer. Adding an exactly representable half therefore cannot move a below-threshold input across that integer. Exact ties move away from zero using `copysign`; the integer cast truncates and saturates. Values beyond the result range saturate regardless of the exact rounded floating value. Negative-zero integer results remain zero.

The i16 helper remains `const fn`; compile-time cases cover signed ties, saturation, NaN and infinities. No production dependency, field, unsafe block or public signature changes. Three `round` expressions and the unsigned two-part floating validation are replaced; the source grows by explicit guards/comments while execution avoids general floating rounding. Only those three function bodies and a test-module declaration differ in the production file. Activity, divisor and percentage-key definitions match the baseline exactly.

## Correctness and validation

- Five focused tests pass in debug and release. The corpus contains 697,112 f32 patterns and 2,090,332 f64 patterns, including signed zero/subnormals, NaNs with payloads/signs, infinities, integer saturation and multiplication overflow. f32 tests cover both signs and neighbors of every relevant i16 half-integer. The percentage-key test also probes scaled boundaries. Native f64 inputs add small half-integer boundaries, powers of two and 100,000 generated u32 half-integer centers with neighboring source encodings.
- An explicit exhaustive release test compares all **268,434,946** positive/negative f32 encodings from magnitude 0.5 through 32767.5 against the original i16 helper. It passes. This supplements the small-magnitude, nonfinite and saturation cases in the focused suite.
- Expanded baseline, subsequent candidates, final, unchanged-source control and debug produce the same **42,507,784-byte** input/output trace, SHA-256 `5533674d970396788ce9bb4bfc650175c0fcfc1f5d09d1d5c6319cc2f52c4354`. Its 16-byte count header precedes original input bits and exact integer/classification/divisor outputs. The first four stages used a smaller corpus/trace; their distinct hashes and source versions are retained in their manifests.
- The full notefield release suite passes **492 tests**, with five existing/new benchmark, trace and exhaustive tests ignored by default. The explicit exhaustive test above runs separately. The normal release library check also passes.
- Focused Clippy passes with warnings denied. Source snapshots, generated harnesses, frozen references, executable hashes, trace hashes, raw timing rows, zero-allocation counters, selected Rust formatting, Python compilation and whitespace checks are audited.

The full-crate harness extends an earlier isolated simfile dependency workspace. An initial missing-member error and then a missing noteskin manifest were resolved by including notefield and archiving its complete path-dependency closure, including unchanged Lua dependencies needed by noteskin. Shipped PNG fixtures are checked before testing. The first complete compile also found existing JSON fixtures absent from Git; the harness copies the required files from the main checkout into the isolated workspace, retaining their exact snapshots and hashes. Reproduction of the full suite needs those existing fixtures; `DEADSYNC_NOTEFIELD_FIXTURE_ROOT` can point to a checkout containing them. The main checkout is read only. Compilation uses temporary storage on D:, while actual unit tests retain the host's normal temporary directory. These harness changes happened after measurements: all timed driver function ASTs and measured Rust sources still match the final snapshot. The manifests record both measured/current driver hashes and full-build source archives/overlays.

## Candidates retained and discarded

Twelve kernel stages are archived: `baseline`, `activity-bits`, `divisor-bits`, `integer-i16`, `baseline-expanded`, `integer-u32`, `integer-i32`, `guarded-add`, `positive-bits-add`, `final`, `control`, `debug`. Each has five passing focused tests. Initial stages used 18 workloads; `guarded-add` onward adds the positive-input set for 24. The final/control stages use five processes each; exploratory release stages use three.

Activity bit classification had no stable gain. An early divisor bit-classification candidate appeared faster, then regressed in later runs (roughly 0.855× normal and 0.923× exceptional); both functions were restored. Manual f32 mantissa decoding improved some inputs but added more machinery and lost to the guarded addition. Manual f64 decoding produced inconclusive signed gains and an unsigned exceptional regression around 0.887×; it and its shared helper were removed. The initial unsigned guarded-add variant regressed on inactive values (0.847×); positive-finite bit validation removed that cost and the final candidate improves all four sets. Earlier candidate TSVs retain these observations.

## Reproduction and evidence

From the isolated checkout, PowerShell:

```powershell
$env:DEADSYNC_NOTEFIELD_NUMERIC_ARCHIVE='D:\deadsync-notefield-numeric-20261104'
python scripts/benchmark_notefield_numeric.py baseline-expanded --baseline 12a94507058c8c47980f422eff8d4e8fe60bf900
python scripts/benchmark_notefield_numeric.py final --runs 5
python scripts/benchmark_notefield_numeric.py control --runs 5 --baseline 12a94507058c8c47980f422eff8d4e8fe60bf900
python scripts/benchmark_notefield_numeric.py debug --debug
python scripts/benchmark_notefield_numeric.py full-release --full
& "$env:DEADSYNC_NOTEFIELD_NUMERIC_ARCHIVE\final\numeric.exe" numeric_performance::exhaustive_i16_domain --exact --ignored --nocapture --test-threads=1
```

Fresh reproduction should use a fresh archive directory to preserve the measured snapshots. The full path reuses the verified source archive referenced by the earlier partner-chart full-release manifest. Kernel reproduction needs only rustc and the tracked sources. Historical generated files and binaries live under `D:\deadsync-notefield-numeric-20261104` on this machine; tracked manifests retain their hashes and commands. Paired raw samples are in [`final.tsv`](2026-11-04-notefield-numeric-final.tsv) and [`control.tsv`](2026-11-04-notefield-numeric-control.tsv); all stage manifests and TSVs share this report's prefix. [`checks.json`](2026-11-04-notefield-numeric-checks.json) records the audit and full results.
