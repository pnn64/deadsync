# Font fallback caches and metric rounding — 2026-11-04

Baseline: `74a02c1808bd693fe4fcdab41ae7b26be339c54d`. Three measured safe Rust optimizations change two existing functions:

1. Deep ASCII-cache construction resolves each fallback font once, instead of resolving its name again for every missing character. Resolved slots are skipped and the first authored glyph wins. Chains with fewer than three reachable fonts keep their original per-character construction.
2. Fallback maps with at most 128 entries are iterated directly, filling unresolved ASCII slots. Larger Unicode maps retain at most 128 ASCII probes per level. The initial font retains its direct construction. Separate chain-only and sparse-map comparisons measure these changes independently.
3. Font metric rounding uses a bounded f64 bias and reads the resulting integer mantissa bits. This removes the floor call, fractional subtraction and half/parity branches. Nonfinite values still return zero; large finite values keep the saturating i32 cast.

The cache function is explicitly inlined into its existing refresh caller. A measured trial of this attribute improved the integrated isolated-font result; final measurements include it. Public signatures, all Glyph/Font fields, cache generation rules, texture ownership and table storage remain unchanged. No new production helper, field, dependency or unsafe block is added. The crate retains `#![forbid(unsafe_code)]`. CPU/allocation instrumentation reuses existing test support in the standalone harness, outside that crate prohibition.

## Final measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1, GenuineIntel, rustc 1.99.1 / LLVM 23.1.1, opt-level 3, fat LTO and static CRT. Five fresh processes alternate original/current samples over twelve rounds on CPU 2, discard the first and retain the median of eleven. Cache fixtures use 2,000 iterations per sample, refresh fixtures 200 and numeric/metric fixtures 10,000. Font maps and numeric inputs are prepared outside timing. Function pointers/arguments are black-boxed. Cache measurements include output destruction; refresh measurements replace tables in existing FontMaps. Background development builds were present; thread CPU cycles and identical-source controls are recorded with raw nanoseconds.

Cycle columns are independent process medians. Throughput is the median of paired old/current ratios and can differ from their column ratio. Cache fixture names encode font count, authored ASCII entries per font, additional Unicode entries and starting-default presence. Numeric fixtures contain 128 values except `arbitrary` (114). Metric fixtures perform the parser's scaled-width/extra-advance arithmetic over 128 widths; they do not measure complete font-file parsing.

| Workload | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `ascii/0/0/0/false` | 706.35 | 721.83 | 0.992× |
| `ascii/1/0/0/true` | 5496.28 | 5714.35 | 0.962× |
| `ascii/1/128/0/true` | 8856.39 | 8816.55 | 1.002× |
| `ascii/1/128/5000/true` | 8937.82 | 8884.70 | 0.998× |
| `ascii/1/16/0/false` | 2776.24 | 2666.38 | 1.031× |
| `ascii/1/16/0/true` | 5807.20 | 6015.40 | 0.958× |
| `ascii/2/16/0/true` | 7017.52 | 7004.47 | 0.989× |
| `ascii/4/16/0/true` | 13433.07 | 10515.81 | 1.288× |
| `ascii/64/1/0/true` | 255435.99 | 11953.53 | 20.923× |
| `ascii/8/1/5000/true` | 35713.31 | 17582.06 | 2.014× |
| `ascii/8/128/0/true` | 8928.49 | 10180.52 | 0.885× |
| `ascii/8/16/0/true` | 22089.82 | 11695.84 | 1.889× |
| `ascii/8/8/129/true` | 31003.72 | 18553.90 | 1.671× |
| `chain/1/16/0/false` | 2912.77 | 2881.49 | 1.010× |
| `chain/8/16/0/true` | 23084.81 | 16739.51 | 1.364× |
| `chain/8/8/129/true` | 32863.43 | 19538.03 | 1.698× |
| `metrics/font` | 1140.39 | 614.36 | 1.851× |
| `metrics/large` | 1074.41 | 708.22 | 1.568× |
| `metrics/ties` | 1318.32 | 638.70 | 2.064× |
| `refresh/1/0/0/true` | 6923.03 | 6632.19 | 0.992× |
| `refresh/1/16/0/true` | 7726.40 | 7514.58 | 1.062× |
| `refresh/33/1/0/true` | 2641565.07 | 456534.76 | 6.005× |
| `refresh/8/1/5000/true` | 167462.03 | 120903.88 | 1.385× |
| `refresh/8/128/0/true` | 106050.33 | 110286.68 | 0.953× |
| `refresh/8/16/0/true` | 145729.35 | 100085.42 | 1.456× |
| `refresh/8/8/129/true` | 168630.87 | 133531.71 | 1.284× |
| `round/arbitrary` | 1297.53 | 764.17 | 1.688× |
| `round/font` | 1386.52 | 810.35 | 1.710× |
| `round/halves` | 1427.54 | 816.36 | 1.749× |
| `round/large` | 1313.53 | 823.67 | 1.605× |
| `sparse/1/16/0/false` | 2922.97 | 2909.47 | 1.004× |
| `sparse/8/16/0/true` | 17330.18 | 12015.65 | 1.363× |
| `sparse/8/8/129/true` | 18888.41 | 19252.35 | 0.981× |

Standalone chain walking improves 1.364×–1.698× in its independent comparisons. Sparse-map iteration adds 1.363× on the eight-font sparse fixture relative to chain walking with probes. Its large-Unicode comparison and shallow no-default comparison are unchanged within variation. Combined deep-cache fixtures improve 1.288×–2.014× at four/eight fonts; the 64-font fixture improves 20.923×. Complete chained refreshes improve 1.284×–6.005×. Rounding improves 1.605×–1.749× and the scaled-metric callers improve 1.568×–2.064×.

Identical-source primary-control medians span **0.979×–1.033×**, with individual process pairs 0.940×–1.070×. The six separate chain/sparse comparisons do not run as identical-source controls. Small changes near the control spread are inconclusive. In particular, no gain is claimed for isolated/empty caches, short chains or the no-default ablation controls.

## Retained costs

The eight-font fixture whose primary map already contains all 128 ASCII characters has no repeated fallback work to remove. Its direct cache result is **0.885×** throughput, or **13.0% more paired CPU**, with an independent median difference of about 1252 cycles per constructed table. The complete eight-font refresh is **0.953×**, or **4.9% more paired CPU**; its independent difference is about 530 cycles per font. This cost is retained alongside the larger chained-font gains, and is measured at the real public caller as well as the private helper.

The empty/sparse isolated-default helper fixtures are 0.962× / 0.958×, about 3.9% / 4.4% more paired CPU. The complete empty-font refresh is 0.992× (inconclusive); sparse isolated-font refresh is 1.062×, without a claim of a general isolated-font gain. These are cache construction/registration costs; the glyph-drawing lookup uses the retained ASCII table and is unchanged. Application code-size impact from inlining is unmeasured.

## Allocations and exact behavior

- All six allocation counters match for every original/current fixture in every process. **32 of 33 workloads are allocation-free**. The 33-font refresh retains `1` allocation, `0` reallocations, `1` free, `528` allocated/freed bytes and `528` peak bytes for the existing SmallVec name list. There is no new cache or retained allocation.
- The full crate passes **191 release tests**, with seven ignored benchmark/trace/child fixtures. Four new crate tests and two additional standalone tests pass; all six standalone tests pass in debug too. Normal production-only release checking and focused Clippy with warnings denied pass. The unsafe prohibition is unchanged.
- The cache corpus compares **645 complete ASCII tables**, including fallback precedence, defaults from only the starting font, missing fonts, small/large Unicode maps, terminating cycles, every glyph float bit and Arc pointer identity.
- **96 complete map refreshes** compare all table outputs, tags, chain keys, retained table pointers and other Font fields. Cases include 0–64 fonts, tag wraparound and repeated refreshes. Unresolved-cycle child probes are confirmed live after readiness and terminated after a bounded observation; the unchanged missing-slot invariant keeps those cycles nonterminating. Cycles covering every ASCII slot still return exact tables.
- Numeric checks compare **895,779 inputs**, including half-neighbors, both signs, exponent/mantissa boundaries, nonfinite values and generated float encodings. Explicit exhaustive release checks pass **2,650,800,128 encodings**: every signed f32 magnitude below 2^31, the entire range using the bias. Higher finite f32 values are already integers and keep the old saturation behavior; nonfinite values return zero.
- Every recorded original/current stage and both profiles share a **29,797,483-byte** serialized glyph/tag/rounding trace, SHA-256 `33038e06553cb29704197876354222acbcd3021a588c071e0809ff01b98a2454`. Traces preserve NaN payload bits in glyph fields. Arc addresses are checked within each process and excluded from the portable trace.

## Why the rounding bits are exact

The bias is 3 × 2^51 (`0x4338000000000000` as f64 bits). Adding any f32 with magnitude below 2^31 keeps the f64 exponent at 52, where adjacent representable values differ by exactly one. Converting f32 to f64 is exact; the addition performs round-to-nearest, ties-to-even at the integer boundary. The bias starts with 32 zero low bits. The low word therefore encodes the rounded signed integer, including negative integers through the wrapping mantissa decrement. The range guard ensures it fits i32; the final safe integer cast reads that low word. No rounding-mode change or unsafe operation is used. Existing documentation already assumes nearest-even arithmetic.

The first always-streamed cache candidate slowed isolated fonts, so it was replaced by the short-chain path. A standard round_ties_even variant had no useful standalone gain and was discarded. A return avoids an unnecessary default-fill scan when every slot is resolved; the remaining measured cost is disclosed above. No claim depends on the earlier two-run experiments.

## Source and reproduction

Only compute_ascii_glyphs and round_half_to_even_i32 bodies, one inline attribute and the four-line test registration change production source. The original cache body remains the shallow-chain path. Frozen references come from the baseline's four functions, with test-only visibility promotions; the original rounding attributes are preserved. The sparse ablation is the current cache body with its map-iteration condition disabled. The harness uses source-extracted, shared Glyph/Font/map definitions and real rustc_hash/smallvec dependencies. Full crate tests exercise the actual types and private helpers.

`checked-final`, `checked-control`, `checked-debug` and `inline-full-release` are authoritative for current source. Earlier successful stages remain immutable, including two preceding 191-test full runs. The audit JSON verifies every recorded source/compiler context, binary/log/trace hash, raw timing row and six allocation fields, plus current-source full/normal checks and reference provenance. Earlier stages use their own frozen harness versions and contain fewer fixtures.

```powershell
$env:PYTHONUTF8='1'
$env:DEADSYNC_FONT_CACHE_ARCHIVE='D:\deadsync-font-cache-20261104'
python scripts/benchmark_font_cache.py inline-full-release --full
python scripts/benchmark_font_cache.py checked-final --runs 5 --exhaustive --reference baseline
python scripts/benchmark_font_cache.py checked-control --baseline 74a02c1808bd693fe4fcdab41ae7b26be339c54d --runs 5 --reference baseline
python scripts/benchmark_font_cache.py checked-debug --debug --reference baseline
```

The full driver verifies and reuses the prior partner-chart source archive, extends its Git path-dependency closure, and overlays current sources. It retains the preparer's shipped cel PNG archive; these font unit tests need no external ignored JSON fixtures. `DEADSYNC_FONT_DEPS` can point at another matching dependency directory. Rlib paths and SHA-256 values are recorded in manifests. Compilation uses D: temporary files; full tests use host C: temporary files. Focused debug builds use opt-level 0 for these functions and link the recorded cached release dependencies with fat LTO. Source snapshots, executables, logs and traces are archived under `D:\deadsync-font-cache-20261104`; raw TSVs, manifests and audit results accompany this report. No full-application or cross-platform speedup is claimed.
