# Compact profile keys and ASCII name comparisons — 2026-11-05

Baseline: `a6f3d013f296b2257b54178e264373fd293987a4`. Three measured changes in `deadsync-profile`:

1. `with_compact_option_key` replaces per-byte ASCII classification and folding with a compile-time 256-entry byte table. Zero entries mean the byte is discarded; ASCII letters/digits map to their exact original lowercase bytes. Both the 64-byte stack path and longer fallback retain the old normalization, allocation capacity, callback count and error text. The table's logical data size is 256 bytes; it adds no heap allocation or mutable cache.
2. `ascii_case_insensitive_cmp` skips folding equal raw bytes on its existing short-input path (shorter side below 20 bytes) when the first bytes match. If the first bytes differ, it retains the original folded iterator. Differing later bytes use the original ASCII lowercase comparison; prefixes retain length ordering.
3. A private `cmp_ascii_prefix_blocks` routine skips identical leading 16-byte blocks for long inputs. The wrapper calls it only when the shorter complete string has at least 68 bytes. At the first raw difference, the original `cmp_ascii_blocks` handles the remaining suffix. Its body stays unchanged. Isolating the probe keeps its setup out of ordinary folded-block comparisons and avoids probing every case-different block.

Production source changes: **+56/−9 lines** (net +47), excluding one private test registration. Existing signatures, Unicode-lowercase comparison, ordinary lowercase normalization, error formats, public types, config defaults and serialization stay unchanged. One private prefix routine isolates the long-input loop. No unsafe, dependency or heap cache is introduced.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo profile-package unit executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` provides thread CPU cycles. Cycle columns are independent process medians; throughput is the median of five within-process ratios. Identical-source controls expose code-context and timing bias. Owned builds/benchmarks run sequentially; unrelated user activity is not controlled.

Primitive key/comparator and public parser fixtures use 20,000 repetitions. Keys return a consumed byte checksum; public parsers return consumed enum/error results. Sorting fixtures use 64 repetitions of stable sorting 1,024 prepared string references. The common reference-buffer reset is included in both sort timings; constructing source strings is excluded. Sorting scratch allocation/destruction and parser error allocation/destruction are included. The counted-System allocator remains installed with counting disabled during CPU timing, retaining its inactive TLS check.

These are in-memory normalization, comparison and sort measurements. Filesystem, startup, frame rate and whole-application gains are unmeasured. Short and long comparison changes apply to disjoint input-length paths; public parser and complete-sort workloads check callers separately.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `ascii/case256` | 313.02 | 319.43 | 0.975× | 1.000× | 0 → 0 | 0 → 0 |
| `ascii/case32` | 83.19 | 86.19 | 0.999× | 1.015× | 0 → 0 | 0 → 0 |
| `ascii/early` | 8.54 | 11.08 | 0.769× | 1.031× | 0 → 0 | 0 → 0 |
| `ascii/late` | 513.99 | 134.78 | 3.791× | 1.015× | 0 → 0 | 0 → 0 |
| `ascii/partial` | 41.05 | 39.22 | 1.025× | 1.027× | 0 → 0 | 0 → 0 |
| `ascii/same256` | 313.33 | 110.34 | 2.883× | 0.991× | 0 → 0 | 0 → 0 |
| `ascii/same32` | 79.69 | 81.52 | 1.007× | 0.998× | 0 → 0 | 0 → 0 |
| `ascii/short` | 23.90 | 24.34 | 0.982× | 1.000× | 0 → 0 | 0 → 0 |
| `ascii/short-late` | 48.99 | 29.84 | 1.648× | 0.998× | 0 → 0 | 0 → 0 |
| `ascii/short-prefix` | 38.82 | 24.98 | 1.555× | 1.001× | 0 → 0 | 0 → 0 |
| `ascii/short-same` | 49.44 | 30.31 | 1.626× | 1.011× | 0 → 0 | 0 → 0 |
| `ascii/unicode` | 439.80 | 152.72 | 2.961× | 1.006× | 0 → 0 | 0 → 0 |
| `combo/canonical` | 124.73 | 81.11 | 1.510× | 0.994× | 0 → 0 | 0 → 0 |
| `combo/punctuation` | 131.33 | 83.48 | 1.557× | 0.997× | 0 → 0 | 0 → 0 |
| `key/compact` | 118.16 | 74.20 | 1.590× | 0.997× | 0 → 0 | 0 → 0 |
| `key/digits` | 77.08 | 58.34 | 1.351× | 0.952× | 0 → 0 | 0 → 0 |
| `key/empty` | 22.87 | 22.44 | 1.034× | 1.035× | 0 → 0 | 0 → 0 |
| `key/len256` | 3092.04 | 2349.78 | 1.316× | 0.973× | 1 → 1 | 256 → 256 |
| `key/len64` | 566.27 | 323.95 | 1.752× | 0.960× | 0 → 0 | 0 → 0 |
| `key/len65` | 1155.15 | 759.29 | 1.521× | 0.985× | 1 → 1 | 65 → 65 |
| `key/lower` | 83.49 | 56.55 | 1.476× | 0.940× | 0 → 0 | 0 → 0 |
| `key/space` | 97.59 | 72.16 | 1.343× | 0.947× | 0 → 0 | 0 → 0 |
| `key/tail-upper` | 577.55 | 331.96 | 1.740× | 0.971× | 0 → 0 | 0 → 0 |
| `key/title` | 81.12 | 57.31 | 1.461× | 0.943× | 0 → 0 | 0 → 0 |
| `key/unicode` | 67.13 | 43.93 | 1.463× | 1.010× | 0 → 0 | 0 → 0 |
| `key/upper` | 82.72 | 55.45 | 1.465× | 0.942× | 0 → 0 | 0 → 0 |
| `sort/long` | 3309507.84 | 1099039.92 | 3.011× | 0.998× | 1 → 1 | 16384 → 16384 |
| `sort/mixed` | 824085.34 | 828379.31 | 1.011× | 0.997× | 1 → 1 | 16384 → 16384 |
| `sort/short` | 597506.45 | 378795.25 | 1.600× | 0.989× | 1 → 1 | 16384 → 16384 |
| `turn/canonical` | 92.41 | 62.66 | 1.442× | 1.008× | 0 → 0 | 0 → 0 |
| `turn/invalid` | 330.64 | 292.63 | 1.122× | 0.974× | 1 → 1 | 60 → 60 |
| `turn/punctuation` | 100.17 | 67.08 | 1.525× | 1.002× | 0 → 0 | 0 → 0 |

Process throughput ranges (minimum–maximum):

- `ascii/case256`: 0.964×–0.984×; control 0.983×–1.018×.
- `ascii/case32`: 0.960×–1.011×; control 0.997×–1.079×.
- `ascii/early`: 0.744×–0.800×; control 1.003×–1.164×.
- `ascii/late`: 3.701×–3.903×; control 0.970×–1.051×.
- `ascii/partial`: 1.009×–1.060×; control 0.991×–1.081×.
- `ascii/same256`: 2.340×–2.946×; control 0.983×–1.012×.
- `ascii/same32`: 0.977×–1.011×; control 0.966×–1.166×.
- `ascii/short`: 0.965×–1.002×; control 0.990×–1.008×.
- `ascii/short-late`: 1.528×–1.777×; control 0.992×–1.085×.
- `ascii/short-prefix`: 1.492×–1.569×; control 0.988×–1.035×.
- `ascii/short-same`: 1.509×–1.652×; control 0.937×–1.078×.
- `ascii/unicode`: 2.876×–3.050×; control 0.965×–1.060×.
- `combo/canonical`: 1.481×–1.555×; control 0.981×–1.063×.
- `combo/punctuation`: 1.492×–1.666×; control 0.995×–1.079×.
- `key/compact`: 1.547×–1.630×; control 0.937×–1.028×.
- `key/digits`: 1.321×–1.384×; control 0.942×–1.036×.
- `key/empty`: 1.019×–1.047×; control 1.033×–1.092×.
- `key/len256`: 1.302×–1.323×; control 0.957×–0.986×.
- `key/len64`: 1.721×–1.777×; control 0.935×–1.007×.
- `key/len65`: 1.504×–1.551×; control 0.963×–1.002×.
- `key/lower`: 1.436×–1.491×; control 0.919×–1.023×.
- `key/space`: 1.339×–1.355×; control 0.905×–1.019×.
- `key/tail-upper`: 1.714×–1.793×; control 0.931×–0.991×.
- `key/title`: 1.368×–1.476×; control 0.928×–0.943×.
- `key/unicode`: 1.454×–1.528×; control 0.994×–1.012×.
- `key/upper`: 1.422×–1.492×; control 0.938×–0.987×.
- `sort/long`: 2.998×–3.034×; control 0.985×–1.003×.
- `sort/mixed`: 0.994×–1.016×; control 0.952×–1.014×.
- `sort/short`: 1.571×–1.608×; control 0.974×–1.025×.
- `turn/canonical`: 1.386×–1.479×; control 0.875×–1.016×.
- `turn/invalid`: 1.120×–1.130×; control 0.861×–1.023×.
- `turn/punctuation`: 1.468×–1.563×; control 0.987×–1.008×.

Raw paired medians below 0.970× are retained explicitly:

- `ascii/early`: 8.54 → 11.08 cycles; 0.769×, control 1.031×.

The identical-source controls are not subtracted from reported ratios. Small changes near their controls are inconclusive; the table and process ranges retain both gains and costs. The optimizations are not universal speedups.

## Exactness and resources

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **245 unit + 8 integration tests**, including all four existing integration targets. Ordinary test-name/status sets match. Dependencies retain cached release profiles in the assertion-enabled gate. Timing and trace tests run explicitly outside ordinary gates.
- The compact normalizer compares frozen strings and actual Turn/Combo enum/error outcomes for every Unicode scalar alone, before `Abc9`, and after `abc9`; 16,384 seeded mixed UTF-8 strings and long boundary cases cover 63/64/65 filtered-byte transitions, punctuation, controls, Greek letters, trimming and fallback capacities. Callback panics still occur after exactly one invocation.
- Ordering checks every 0..=255 character pair in short, common-prefix and bulk contexts, all Unicode scalars against themselves and two reference strings, and lengths around 4/16/20/64/68-byte dispatch boundaries. Stable sorting retains complete ordered string-reference results. Existing integration regressions continue to exercise ASCII/Unicode/profile comparators, profile tie-breaking and complete sort results.
- The **10,132,289-byte trace** matches all three gates, SHA-256 `602384ff1318dfce7147e07754ece29ccbd4d2408a64d6409c56b3083a153a92`. It records normalized bytes and ordering for the stride-17 scalar sample, seeded/boundary cohorts, primitive fixtures and complete sorted datasets. Ordinary differential tests independently cover every scalar.
- All **640 raw timing/allocation rows** match process logs. All six allocation counters match old/current for every measured fixture. Primitive comparisons and normal stack keys remain allocation-free; long keys, invalid parser errors and stable-sort scratch retain their previous allocation behavior. Allocation/free bytes balance and no fixture reallocates. The lookup table's fixed data does not appear in heap counters; stack use and RSS are not measured.
- Scoped Clippy has zero changed-scope diagnostics. Production-only `cargo check --offline --locked --lib --release` passes, with its artifact explicitly retaining opt-level 3 and assertions/overflow disabled. Audits verify **26 package inputs**, exact changed bodies/private prefix routine, signatures, frozen routines/callers, source/dependency archives, compiler/lock/profiles, binaries/logs, test names, traces and every raw field. Other out-of-scope messages are recorded.

## Reproduction and discarded probes

`scripts/benchmark_profile_kernels.py` builds the actual profile package and all four integration targets in the six-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo manifests/dependency versions are unchanged; lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_PROFILE_KERNELS_ARCHIVE = 'C:\deadsync-profile-kernels-20261105'
python scripts/benchmark_profile_kernels.py baseline --baseline a6f3d013f296b2257b54178e264373fd293987a4 --runs 5 --reference baseline
python scripts/benchmark_profile_kernels.py final --runs 5 --reference baseline
python scripts/benchmark_profile_kernels.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. The baseline restores both original production modules plus the common private test registration. Each timing TSV retains 320 rows; checks JSON verifies 640 total rows and all process/control ranges.

Earlier standalone probes were withdrawn: cycle-based BPM reordering slowed shuffled input, unrestricted canonical borrowing slowed long strings, bounded borrowing slowed title-case values, and raw equality before every folded block slowed case-only differences. No BPM reorder or ordinary lowercase-normalizer change is retained. Sources, binaries and logs for rejected and retained prototype cohorts are hash-verified; final package measurements above are authoritative.

The first actual-package version placed prefix probing in the ordinary block routine and checked equality on every short byte. It improved short/long sorts but slowed short case-only and medium comparisons. That version's release/assertion gates and 640 rows were verified and archived before refinement. The final version isolates prefix probing and retains the original short iterator when first bytes differ. The baseline was reused only after proving its restored sources, common test harness, driver and manifest unchanged. Both superseded gates and the pre-refinement verification log remain hash-verified in checks JSON.
