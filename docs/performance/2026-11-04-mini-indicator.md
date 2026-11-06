# Mini-indicator scoring and combo colors — 2026-11-04

Baseline: `aa6d41352f5d776ce1c14ed0c623ff1efeec802a`. Three safe Rust changes in `mini_indicator.rs` reduce per-frame scoring and color work:

1. EX and Hard EX losses accumulate in u64 and clamp once. This replaces twelve and eleven saturating operations, respectively, with ordinary bounded arithmetic and one final clamp. The ITG branch is unchanged.
2. Glow colors return white directly for grades whose glow pair is white, provided the divided elapsed time is finite. Invalid elapsed times and division overflow retain the original sine/FMA calculation.
3. Rainbow hue dispatch uses its existing bounded sector range to replace signed remainder with a conditional addition.

Exactly three existing function bodies change. No state fields, types, public signatures, dependencies, heap operations or unsafe blocks are added. The scoring body loses nine lines; the guarded glow return adds nine, and the hue-range comment adds one. There is also a four-line test-module declaration. Rejected flag and stream-loop changes are restored.

## Final measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1, rustc 1.99.1 / LLVM 23.1.1. Release kernels use opt-level 3, fat LTO and the static CRT. Five fresh processes alternate original/current samples on CPU 2 for twelve rounds, discard the first and retain the median of eleven. Each final sample performs **10,000 fixtures**. Inputs are prepared outside timing; function pointers and arguments are black-boxed. The original implementation is frozen from the baseline revision.

CPU columns are independent process medians. Throughput is the median of paired original/current cycle ratios and can differ from the ratio of those columns. Raw TSVs retain nanoseconds as well. These are kernel and small caller measurements, not whole-frame results.

| Workload / inputs per fixture | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `counter/clean/64` | 610.58 | 601.25 | 1.016× |
| `counter/early/64` | 670.77 | 613.59 | 1.035× |
| `counter/late/64` | 583.47 | 625.73 | 0.938× |
| `counter/mixed/64` | 663.31 | 684.99 | 1.004× |
| `glow/active/64` | 3795.20 | 3761.99 | 1.000× |
| `glow/exception/64` | 2538.30 | 2772.86 | 0.936× |
| `glow/mixed/64` | 4079.06 | 2425.41 | 1.671× |
| `glow/white/64` | 3953.39 | 924.93 | 4.237× |
| `lookup/build/512` | 1744.87 | 1820.45 | 1.029× |
| `output/ex/64` | 2144.41 | 1691.09 | 1.248× |
| `output/hard/64` | 2117.30 | 1803.92 | 1.195× |
| `points/ex/limits/64` | 1011.54 | 454.54 | 1.985× |
| `points/ex/normal/64` | 808.37 | 490.08 | 1.670× |
| `points/hard/limits/64` | 909.48 | 519.97 | 1.737× |
| `points/hard/normal/64` | 757.85 | 521.38 | 1.455× |
| `quint/clear/64` | 1254.66 | 1212.32 | 1.029× |
| `quint/mixed/64` | 1187.98 | 1169.34 | 1.017× |
| `rainbow/fixed/64` | 2532.50 | 2509.28 | 1.039× |
| `rainbow/scroll/64` | 2490.62 | 2168.64 | 1.108× |
| `stream/breaks/64` | 7.11 | 7.11 | 1.000× |
| `stream/mixed/512` | 1242.04 | 1138.11 | 0.986× |
| `stream/runs/512` | 1279.88 | 1372.36 | 1.014× |
| `stream/runs/64` | 194.13 | 183.63 | 1.000× |

Score-loss kernels improve 1.455×–1.985× across the listed fixtures. The output fixtures include construction of the actual subtractive indicator result, using ordinary finite score/color values. White and mixed glows improve 4.237× and 1.671×; active colored and exceptional-time cases retain their original calculation. Rainbow fixed/scroll results are 1.039× / 1.108×.

The fixed-rainbow result is inconclusive near the control spread; the scrolling result is the demonstrated benefit for the bounded-sector change. The exceptional-time glow fixture is a retained tradeoff: 0.936× throughput, or 6.8% more paired CPU. Its independent median difference is about 3.7 cycles per call. The new eligibility guard precedes the unchanged fallback on these invalid times; ordinary white/mixed glows remove substantially more work. No gain is claimed for that fallback.

The identical-source control medians span **0.950×–1.051×**, with individual process pairs 0.901×–1.159×. Controls and unchanged function fixtures expose layout and sampling variation. Small or inconsistent differences are inconclusive; no gain is claimed for unchanged judgment checks, stream scans, lookup construction, or active/exceptional glows. Earlier short-sample results remain archived; the final table uses only `validated-final` and the matching `validated-control`.

All twenty-two non-constructor fixtures have zero allocation, reallocation and free events in both implementations. The unchanged 512-segment constructor preserves every counter: one allocation, one reallocation and one free; 4,112 allocated/freed bytes and a 4,096-byte peak. No allocation reduction is claimed.

## Exact behavior

All score weights are positive integers. Saturating a nonnegative intermediate sum or product at u32::MAX has the same final result as clamping the full weighted sum at u32::MAX: an intermediate overflow already makes the mathematical final sum at least that limit. EX is bounded by 34 × u32::MAX and Hard EX by 38 × u32::MAX, both far below u64::MAX. Widening before each sum/product therefore cannot overflow, including debug builds.

The white glow path checks `(elapsed / 0.8).is_finite()`. A finite quotient has a finite fractional part; its bounded sine and midpoint produce a finite interpolation weight. Interpolating two white colors then returns exactly `[1.0; 4]`. NaNs, infinities and finite elapsed times whose division overflows use the untouched original arithmetic, preserving the tested output bits and exceptional behavior.

For a finite hue, `fract()` lies between -1 and 1. Multiplication by six and flooring yield a sector in -6..=5; nonfinite input produces NaN, whose integer cast is zero. Adding six only to a negative sector is thus identical to `rem_euclid(6)`. Hue arithmetic and interpolation remain in their original order.

## Validation and provenance

- Four new focused tests pass in release and debug; together with the five existing module tests, each focused binary passes nine tests. Score comparisons cover 302,025 progress records across all three score types, including ordinary values, sparse/full-width counts, multiplier thresholds and signed ITG extremes. Count checks cover 210,000 records with occupancy patterns and full-width values.
- Color comparisons check **19,737,200 returned RGBA arrays** against the frozen implementation, bit for bit. They span all five color styles, grade/mode/quint combinations, small and large combo values, 20,140 elapsed values, NaN payloads, infinities, signed zero, and neighbors around the elapsed-division overflow boundary. Stream tests also retain reference comparisons of results, debug overflow outcomes, boxed spans, cursor state and type size from the discarded candidate.
- Latest release trace: **326,938,940 bytes**, SHA-256 `f3da80d4aaae275bde9fcb899c9a8b27012c629e55533444a4a034a27df6c883`. Latest debug trace: **326,938,904 bytes**, SHA-256 `a5168e58f5fe70aebdd7979d81bd7040a870a2e4fb6926c5795aae0a4a928fe9`. Original/current traces match within each profile; the explicit overflow cases have profile-specific outcomes. Earlier corpus versions are identified by their own trace hashes.
- Full notefield release suite: **511 passed**, thirteen benchmark/trace tests ignored. The final production and benchmark source fingerprints match that full-suite snapshot. The normal release library check passes with before/after source fingerprints. Focused Clippy passes with warnings denied against the final measured source context.
- The audit checks all fifteen recorded kernel stages, source fragments, generated compiler context, binary/log/trace hashes, frozen reference code, precise production scope, raw timings and allocation counters. Selected Rust formatting, Python compilation and whitespace checks pass.

Full testing reuses the isolated dependency workspace and verified overlays from earlier batches. It restores shipped PNGs and snapshots sixteen existing JSON fixtures absent from Git; full reproduction needs those fixtures (`DEADSYNC_NOTEFIELD_FIXTURE_ROOT` can point at their checkout). Main-checkout content is read only. Compilation uses D: temporary storage; full tests retain the normal host temporary directory for NTFS-dependent fixture behavior.

## Discarded variants and harness corrections

The OR-reduction experiment slowed counter checks to 0.889×–0.981× and quint checks to 0.582×–0.624× in its exploratory run; both bodies are restored. Carrying the preceding stream flag sped long scans but slowed lookup construction to 0.774×; that body is restored too.

An expanded correctness corpus was initially compared against the previous corpus, so the trace equality check stopped before timings. A new matching control was then captured. The first constructor control hit the runner's overly broad zero-allocation assertion after its successful benchmark; constructor counts are now explicitly compared, and fresh complete controls are reported. Both aborted stages remain identifiable in the audit.

A report write also stopped when C: filled. Only this isolated worktree's generated PDB files were moved to D:, with hashes checked after relocation. Trace writing and comparison now use bounded buffers; measured work is unaffected. Full tests were run once against the final source snapshot; lengthening ignored benchmark samples did not require another full build.

Reproduce focused measurements from the isolated checkout:

```powershell
$env:DEADSYNC_MINI_INDICATOR_ARCHIVE = 'D:\deadsync-mini-indicator-repro'
python scripts/benchmark_mini_indicator.py control --baseline aa6d41352f5d776ce1c14ed0c623ff1efeec802a --reference control --runs 5
python scripts/benchmark_mini_indicator.py final --reference control --runs 5
python scripts/benchmark_mini_indicator.py baseline-debug --baseline aa6d41352f5d776ce1c14ed0c623ff1efeec802a --debug
python scripts/benchmark_mini_indicator.py debug --reference baseline-debug --debug
python scripts/benchmark_mini_indicator.py full-release --full
```

Snapshots, compiled binaries, raw run logs and traces are retained under `D:\deadsync-mini-indicator-20261104`. The checked-in manifests, TSVs and checks JSON identify their hashes and sources.
