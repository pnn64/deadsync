# Saturating frame conversion and fixed text kernels — 2026-11-05

Baseline: `e79ff01306d8a3cc5af795f6db2e69e7df8b1919`. Three independently measured safe Rust changes in `deadsync-config`:

1. `frame_pacing.rs::seconds_to_us_u32` retains the original f32 multiplication and uses the saturating float-to-u32 cast directly. The cast already maps NaN/negative values to zero and positive overflow to u32::MAX, so the explicit max-with-zero and overflow branch are redundant. Returned integer values remain identical, including signed zero, subnormals, infinities, NaNs and the rounded u32 limit.
2. `null_or_die.rs::parse_null_or_die_kernel_type` maps each byte through a compile-time lowercase ASCII-alphanumeric table and lazily discards zero markers. Existing Rising/Loudest first-byte dispatch and exact suffix iteration remain, preserving ignored punctuation/Unicode, early mismatches and rejection of extra retained characters. The existing target parser is untouched.
3. `coin.rs::CoinMode::from_str` matches the trimmed byte-slice length. Three-byte Pay uses ASCII bit folding; four-byte Free/Home use an explicitly little-endian packed word and a per-byte case bit. Only ASCII upper/lower forms of the original literal letters can match; high UTF-8 bits remain set. Unicode trimming, strict punctuation rejection, errors and enum outcomes are unchanged.

The three bodies add **30 lines and remove 15** (net +15), excluding one private test registration. Kernel-type lookup adds **256 logical bytes of compile-time data**; identical tables may be compiler-merged, and physical binary-size change is unmeasured. Coin matching uses three/four-byte value arrays. No unsafe, runtime initialization, heap allocation, production helper, dependency, cache or public API is added. Defaults, serialization, labels and unrelated callers are unchanged. Existing test allocation/clock support is reused.

`stutter_severity` and `parse_null_or_die_kernel_target` are **unchanged source-context controls**, not retained optimizations. Their frozen/current bodies are checked against Git. Raw ratios for those workloads can reflect call-site/layout/timing bias; they must not be attributed to an algorithm change in either function.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Identical-body baseline controls expose context/timing bias. Numerical frozen functions retain the original always-inline policy. Builds and our benchmark processes run sequentially; unrelated user activity is not controlled.

Scalar numeric fixtures use 100,000 repetitions; mixed/shuffled numeric fixtures use **256 samples per operation** and 2,000 repetitions. Their reported cycles are per complete 256-value fixture, not per value. The shuffled severity workload visits a deterministic permutation. Type/target text fixtures use 20,000 repetitions; coin fixtures use 50,000. Inputs and owned strings/vectors are prepared outside timing. Integer/enum results and batch checksums are black-boxed. The counted-System allocator stays installed with counting disabled during CPU timing, retaining its inactive TLS check.

There are 21 optimized-path fixtures and 11 unchanged controls (nine severity and two target fixtures). These are in-memory conversion/parser microbenchmarks. Frame rate, frame latency, startup, config-loading I/O and whole-application gains are not measured.

| Workload | Scope | Frozen cycles/fixture | Current cycles/fixture | Paired throughput | Identical-body baseline control | Allocations old → new |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `coin/empty` | optimized | 8.68 | 8.35 | 1.043× | 1.000× | 0 → 0 |
| `coin/free` | optimized | 20.47 | 15.76 | 1.299× | 1.000× | 0 → 0 |
| `coin/home` | optimized | 21.25 | 15.76 | 1.348× | 1.000× | 0 → 0 |
| `coin/invalid` | optimized | 14.97 | 14.45 | 1.008× | 1.000× | 0 → 0 |
| `coin/pay` | optimized | 18.92 | 17.33 | 1.092× | 1.000× | 0 → 0 |
| `coin/space` | optimized | 33.39 | 28.10 | 1.190× | 0.992× | 0 → 0 |
| `seconds/boundary` | optimized | 5.15 | 3.77 | 1.366× | 0.976× | 0 → 0 |
| `seconds/mixed256` | optimized | 1446.72 | 1027.59 | 1.407× | 1.012× | 0 → 0 |
| `seconds/nan` | optimized | 5.52 | 4.00 | 1.366× | 0.972× | 0 → 0 |
| `seconds/negative` | optimized | 5.52 | 4.01 | 1.376× | 0.972× | 0 → 0 |
| `seconds/normal` | optimized | 5.52 | 4.00 | 1.380× | 0.971× | 0 → 0 |
| `seconds/overflow` | optimized | 4.95 | 3.77 | 1.313× | 0.978× | 0 → 0 |
| `seconds/zero` | optimized | 5.52 | 4.00 | 1.377× | 0.972× | 0 → 0 |
| `stutter/level0` | unchanged control | 3.95 | 3.95 | 1.000× | 1.000× | 0 → 0 |
| `stutter/level1` | unchanged control | 4.73 | 4.73 | 1.000× | 1.000× | 0 → 0 |
| `stutter/level2` | unchanged control | 5.52 | 5.52 | 1.000× | 1.000× | 0 → 0 |
| `stutter/level3` | unchanged control | 6.30 | 6.30 | 1.000× | 1.000× | 0 → 0 |
| `stutter/mixed256` | unchanged control | 1336.97 | 1296.59 | 1.010× | 0.886× | 0 → 0 |
| `stutter/nan` | unchanged control | 3.94 | 3.94 | 1.000× | 1.000× | 0 → 0 |
| `stutter/negative` | unchanged control | 3.16 | 3.16 | 1.000× | 1.000× | 0 → 0 |
| `stutter/nonmonotone` | unchanged control | 3.95 | 3.95 | 1.000× | 1.000× | 0 → 0 |
| `stutter/shuffled256` | unchanged control | 1307.23 | 1396.02 | 0.936× | 0.970× | 0 → 0 |
| `target/accumulator` | unchanged control | 33.88 | 33.10 | 1.015× | 1.227× | 0 → 0 |
| `target/digest` | unchanged control | 20.52 | 19.74 | 1.038× | 1.180× | 0 → 0 |
| `type/empty` | optimized | 6.55 | 3.23 | 2.028× | 0.891× | 0 → 0 |
| `type/extra` | optimized | 44.50 | 20.53 | 2.244× | 0.992× | 0 → 0 |
| `type/invalid` | optimized | 11.87 | 4.94 | 2.403× | 0.934× | 0 → 0 |
| `type/loudest` | optimized | 47.03 | 24.49 | 1.897× | 1.015× | 0 → 0 |
| `type/punct` | optimized | 77.90 | 41.90 | 1.859× | 0.990× | 0 → 0 |
| `type/rising` | optimized | 41.33 | 20.97 | 2.013× | 0.981× | 0 → 0 |
| `type/tail` | optimized | 978.37 | 358.59 | 2.732× | 0.996× | 0 → 0 |
| `type/unicode` | optimized | 64.82 | 33.10 | 1.968× | 0.984× | 0 → 0 |

Process throughput ranges (minimum–maximum):

- `coin/empty`: 0.833×–1.086×; baseline control 1.000×–1.000×.
- `coin/free`: 1.274×–1.300×; baseline control 0.832×–1.066×.
- `coin/home`: 1.321×–1.350×; baseline control 1.000×–1.000×.
- `coin/invalid`: 0.838×–1.056×; baseline control 1.000×–1.008×.
- `coin/pay`: 1.092×–1.147×; baseline control 1.000×–1.001×.
- `coin/space`: 1.180×–1.210×; baseline control 0.954×–1.006×.
- `seconds/boundary`: 1.366×–1.415×; baseline control 0.976×–0.979×.
- `seconds/mixed256`: 1.323×–1.409×; baseline control 0.988×–1.014×.
- `seconds/nan`: 1.366×–1.383×; baseline control 0.970×–0.972×.
- `seconds/negative`: 1.366×–1.377×; baseline control 0.970×–0.972×.
- `seconds/normal`: 1.360×–1.410×; baseline control 0.971×–0.975×.
- `seconds/overflow`: 1.269×–1.316×; baseline control 0.977×–0.978×.
- `seconds/zero`: 1.369×–1.383×; baseline control 0.972×–1.000×.
- `stutter/level0`: 0.984×–1.003×; baseline control 1.000×–1.000×.
- `stutter/level1`: 1.000×–1.007×; baseline control 1.000×–1.000×.
- `stutter/level2`: 1.000×–1.009×; baseline control 0.837×–1.000×.
- `stutter/level3`: 1.000×–1.070×; baseline control 1.000×–1.000×.
- `stutter/mixed256`: 1.000×–1.039×; baseline control 0.875×–0.915×.
- `stutter/nan`: 0.997×–1.003×; baseline control 1.000×–1.000×.
- `stutter/negative`: 0.997×–1.000×; baseline control 1.000×–1.000×.
- `stutter/nonmonotone`: 1.000×–1.003×; baseline control 1.000×–1.000×.
- `stutter/shuffled256`: 0.930×–0.941×; baseline control 0.969×–0.990×.
- `target/accumulator`: 0.998×–1.024×; baseline control 1.178×–1.238×.
- `target/digest`: 1.037×–1.040×; baseline control 1.149×–1.181×.
- `type/empty`: 2.023×–2.033×; baseline control 0.891×–0.916×.
- `type/extra`: 2.168×–2.267×; baseline control 0.991×–0.993×.
- `type/invalid`: 2.309×–2.408×; baseline control 0.933×–1.102×.
- `type/loudest`: 1.709×–2.011×; baseline control 1.008×–1.020×.
- `type/punct`: 1.752×–1.862×; baseline control 0.961×–0.993×.
- `type/rising`: 1.938×–2.095×; baseline control 0.981×–0.988×.
- `type/tail`: 2.700×–3.843×; baseline control 0.989×–1.041×.
- `type/unicode`: 1.951×–2.034×; baseline control 0.980×–0.992×.

No optimized-path raw paired median is below 0.970× in this cohort. This does not rule out costs on other inputs.

All unchanged-control drifts remain visible above. Differences near their baseline controls are inconclusive; isolated primitive results do not establish application-level improvements.

## Exactness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **254 unit + 13 integration tests**, covering all five config integration targets. Ordinary test-name/status sets are identical. Dependencies retain cached release profiles in the assertion-enabled gate. New timing and trace tests run explicitly outside normal gates.
- The float corpus covers every 16-bit high word combined with three low-mantissa patterns, giving 196,608 binary32 encodings. Each drives fifteen frame/expected contexts, including both severity threshold neighbors, 0.1 neighbors, signed zero, infinities and a signaling NaN. Another 8,192 probes surround the rounded microsecond overflow boundary, followed by 131,072 seeded arbitrary-bit triples. Seconds conversion compares exact u32 results; severity compares exact u8 results. Inputs retain raw f32 bits in failure messages and traces.
- Text comparisons check actual enum/error outcomes for all three parsers. Every Unicode scalar is checked alone, inside Loudest and after Rising; all ASCII case combinations and insertions cover Rising/Loudest/Digest/Accumulator/Pay/Free/Home, including Unicode whitespace wrapping, long retained/discarded suffixes and 16,384 seeded mixed UTF-8 strings. Coin parsing remains strict while kernel parsing still ignores non-alphanumeric bytes.
- A **10,674,523-byte deterministic trace** matches across all three profiles, SHA-256 `0b94e1bd9865bd0c024ba7078ca6ec9aa8f6638e2c1ebcfb4a27afb774d1164a`. It stores raw float input bits with exact integer/severity outputs, and raw text bytes with exact type/target/coin outcomes. It uses stride-17 high-word/scalar samples plus the boundary/random/alias cohorts; ordinary tests cover the full high-word and scalar cohorts.
  - `frame` part: 10,674,515 bytes, SHA-256 `ad60278cc2bfa7986b9418d5e17478a863495806fecd418da966b140b3e022a0`.
- Every one of the **640 raw rows** matches its process log, including all timing fields and six allocation counters. Every variant has zero allocations, reallocations, frees, allocated bytes, freed bytes and peak requested bytes. Allocation behavior is unchanged. Counters exclude stack/value-array use, immutable table storage, allocator metadata and total RSS.
- Scoped Clippy has zero changed-scope diagnostics. Production-only `cargo check --offline --locked --lib --release` passes with a verified opt-level-3 non-test artifact and assertions/overflow checks disabled. Audits verify **59 package inputs**, exact three-body/signature scope, unchanged severity/target controls, frozen Git/selected-prototype bodies, source/dependency archives, compiler/lock/profiles, tests, binaries/logs, traces and all measurement fields.

## Reproduction and rejected experiments

`scripts/benchmark_frame_kernels.py` builds the actual config package and all five integration targets in the existing six-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_FRAME_KERNELS_ARCHIVE = 'C:\deadsync-frame-coin-20261105'
python scripts/benchmark_frame_kernels.py baseline --baseline e79ff01306d8a3cc5af795f6db2e69e7df8b1919 --runs 5 --reference baseline
python scripts/benchmark_frame_kernels.py final --runs 5 --reference baseline
python scripts/benchmark_frame_kernels.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. Baseline restores all three original production modules plus only the new frame test registration. Frozen references/common fixtures remain identical across gates. Each TSV retains 320 rows; checks JSON verifies 640 total rows and all process/control ranges.

Four standalone source/binary/log cohorts are archived and hash-verified. Simply unrolling severity did not show a clear gain. A fully masked variant regressed across every severity cohort; a scalar-prefix variant preserved some early cases but slowed deeper/mixed cases. Both were rejected before production edits. Moving the existing target table to a shared constant also produced an undesirable target fixture drift; the existing target body was restored before implementation, and the selected type parser uses its own local table. Later prototypes matched the original numeric inlining policy. Fixed-width coin parsing supplied the third supported optimization.

The selected prototype passed the complete correctness corpus and three fresh benchmark processes. Its separate coin benchmark emitted an unused-Result fixture warning; the actual-package common fixture explicitly consumes that Result before any package measurement. Prototype source/binary/logs remain unchanged. Package measurements above are authoritative.
