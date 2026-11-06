# Mini scaling, long-average feedback and atlas coordinates — 2026-11-05

Source baseline: `91d43132fcd23024928a32f54cfb0cdf886d1b7b`. Three safe Rust changes in `deadsync-notefield`:

1. `placement.rs::average_error_bar_mini_scale` returns exact 1.1 for both signs of zero using a sign-masked bit check. Every other input keeps the original fused expression and clamp. A targeted inline hint keeps this scalar path available to callers.
2. `error_bar.rs::append_long_average` rejects invisible tick alpha before calculating mini and width scales. Visible output retains the same arithmetic, insertion order and fields.
3. `judgment_feedback.rs::frame_uv` uses integer bit operations and exact cell widths for one/two-column layouts, with a targeted inline hint. Other grids retain their original formulas. Zero-dimension normalization, usize-to-u32 casts, clamping and profile-specific panic behavior are preserved.

The production changes add **27 lines and remove 6** (net +21), including two inline attributes and excluding private test registrations. The average drawing loop retains its original control flow. No unsafe, heap allocation, production helper, lookup table, normalized buffer, dependency, public API or serialization change is added. Physical code-size and stack changes are unmeasured.

## Method and measured results

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes run twelve alternating old/current rounds pinned to CPU 2, discard the first and retain the median of eleven. QueryThreadCycleTime measures thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. All 40 baseline comparisons execute identical old function bodies and helper bindings.

Feedback/caller fixtures use 30,000 repetitions; scalar and atlas fixtures use 100,000. Request objects, inputs and Vec capacity 64 are prepared outside timing. Outputs are consumed. The counted-System allocator stays installed with counting disabled during CPU timing; all six counters are measured separately. Builds and owned benchmark/maintenance jobs run sequentially. Unrelated user activity is uncontrolled.

These fixtures measure individual methods and complete average/long feedback composition with mini=0. Direct scalar fixtures also cover nonzero inputs. Application-wide mini frequency, nonzero complete-caller performance, frame rate, GPU work and application-wide gains are unmeasured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-body control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `atlas/eight` | 17.40 | 16.78 | 1.041× | 0.956× | 0 → 0 | 0 → 0 |
| `atlas/four` | 17.37 | 17.20 | 1.018× | 0.978× | 0 → 0 | 0 → 0 |
| `atlas/large` | 17.40 | 17.13 | 1.018× | 0.957× | 0 → 0 | 0 → 0 |
| `atlas/max-frame` | 17.36 | 10.92 | 1.590× | 0.967× | 0 → 0 | 0 → 0 |
| `atlas/one` | 17.45 | 9.60 | 1.818× | 1.007× | 0 → 0 | 0 → 0 |
| `atlas/one-single` | 17.40 | 9.62 | 1.809× | 0.902× | 0 → 0 | 0 → 0 |
| `atlas/three` | 17.40 | 16.96 | 1.026× | 0.950× | 0 → 0 | 0 → 0 |
| `atlas/two` | 17.40 | 11.00 | 1.574× | 0.969× | 0 → 0 | 0 → 0 |
| `atlas/two-seven` | 17.38 | 10.95 | 1.595× | 0.991× | 0 → 0 | 0 → 0 |
| `atlas/wide` | 17.42 | 17.31 | 1.006× | 1.020× | 0 → 0 | 0 → 0 |
| `atlas/zero-cols` | 17.39 | 9.62 | 1.810× | 0.940× | 0 → 0 | 0 → 0 |
| `atlas/zero-rows` | 17.43 | 10.90 | 1.603× | 1.013× | 0 → 0 | 0 → 0 |
| `average/active` | 431.84 | 440.93 | 0.978× | 0.977× | 0 → 0 | 0 → 0 |
| `average/empty` | 79.65 | 75.89 | 1.000× | 1.020× | 0 → 0 | 0 → 0 |
| `average/expired` | 36.44 | 33.12 | 1.080× | 0.952× | 0 → 0 | 0 → 0 |
| `average/future` | 36.59 | 33.33 | 1.076× | 0.996× | 0 → 0 | 0 → 0 |
| `average/missing` | 33.33 | 31.68 | 1.024× | 1.002× | 0 → 0 | 0 → 0 |
| `average/nan-width` | 35.66 | 33.36 | 1.040× | 0.986× | 0 → 0 | 0 → 0 |
| `average/off` | 34.28 | 33.69 | 1.032× | 1.003× | 0 → 0 | 0 → 0 |
| `average/zero-width` | 34.55 | 33.36 | 1.048× | 0.995× | 0 → 0 | 0 → 0 |
| `caller/active` | 523.28 | 517.82 | 0.988× | 0.964× | 0 → 0 | 0 → 0 |
| `caller/expired` | 75.03 | 63.55 | 1.199× | 1.052× | 0 → 0 | 0 → 0 |
| `caller/future` | 73.12 | 59.66 | 1.246× | 1.047× | 0 → 0 | 0 → 0 |
| `caller/long` | 122.20 | 121.98 | 1.007× | 1.047× | 0 → 0 | 0 → 0 |
| `caller/missing` | 57.01 | 55.01 | 1.044× | 1.000× | 0 → 0 | 0 → 0 |
| `caller/off` | 65.10 | 61.96 | 1.071× | 1.001× | 0 → 0 | 0 → 0 |
| `long/active` | 77.64 | 79.14 | 0.991× | 1.007× | 0 → 0 | 0 → 0 |
| `long/expired` | 32.34 | 26.52 | 1.188× | 1.017× | 0 → 0 | 0 → 0 |
| `long/future` | 31.74 | 21.55 | 1.471× | 1.029× | 0 → 0 | 0 → 0 |
| `long/hidden` | 20.33 | 20.15 | 1.011× | 0.999× | 0 → 0 | 0 → 0 |
| `long/missing` | 20.43 | 20.33 | 1.005× | 1.000× | 0 → 0 | 0 → 0 |
| `long/zero-width` | 38.70 | 36.64 | 1.060× | 0.983× | 0 → 0 | 0 → 0 |
| `scale/infinite` | 3.95 | 7.09 | 0.557× | 1.002× | 0 → 0 | 0 → 0 |
| `scale/large` | 3.95 | 7.09 | 0.557× | 1.000× | 0 → 0 | 0 → 0 |
| `scale/nan` | 3.95 | 5.52 | 0.716× | 1.000× | 0 → 0 | 0 → 0 |
| `scale/negative` | 3.95 | 7.19 | 0.556× | 0.986× | 0 → 0 | 0 → 0 |
| `scale/negative-zero` | 3.95 | 2.37 | 1.667× | 1.000× | 0 → 0 | 0 → 0 |
| `scale/one` | 3.95 | 7.09 | 0.557× | 1.000× | 0 → 0 | 0 → 0 |
| `scale/subnormal` | 124.18 | 125.69 | 0.988× | 1.005× | 0 → 0 | 0 → 0 |
| `scale/zero` | 3.95 | 2.38 | 1.660× | 1.008× | 0 → 0 | 0 → 0 |

Process throughput ranges (minimum–maximum):

- `atlas/eight`: 1.009×–1.076×; control 0.917×–0.987×.
- `atlas/four`: 1.003×–1.031×; control 0.962×–1.022×.
- `atlas/large`: 1.005×–1.032×; control 0.875×–1.003×.
- `atlas/max-frame`: 1.571×–1.616×; control 0.902×–1.040×.
- `atlas/one`: 1.508×–1.824×; control 0.942×–1.018×.
- `atlas/one-single`: 1.805×–1.815×; control 0.873×–0.978×.
- `atlas/three`: 1.003×–1.094×; control 0.909×–1.025×.
- `atlas/two`: 1.539×–1.590×; control 0.925×–0.990×.
- `atlas/two-seven`: 1.586×–1.602×; control 0.965×–1.010×.
- `atlas/wide`: 1.006×–1.038×; control 0.937×–1.041×.
- `atlas/zero-cols`: 1.808×–1.853×; control 0.906×–0.957×.
- `atlas/zero-rows`: 1.593×–1.646×; control 0.896×–1.056×.
- `average/active`: 0.960×–1.002×; control 0.945×–0.984×.
- `average/empty`: 0.906×–1.070×; control 0.900×–1.039×.
- `average/expired`: 1.019×–1.194×; control 0.882×–1.119×.
- `average/future`: 1.034×–1.395×; control 0.978×–1.070×.
- `average/missing`: 0.934×–1.201×; control 0.930×–1.041×.
- `average/nan-width`: 1.022×–1.276×; control 0.885×–1.004×.
- `average/off`: 0.914×–1.092×; control 0.990×–1.205×.
- `average/zero-width`: 1.001×–1.121×; control 0.967×–1.061×.
- `caller/active`: 0.985×–1.020×; control 0.938×–0.986×.
- `caller/expired`: 1.106×–1.224×; control 0.979×–1.106×.
- `caller/future`: 1.095×–1.310×; control 0.872×–1.121×.
- `caller/long`: 0.996×–1.021×; control 0.995×–1.077×.
- `caller/missing`: 1.031×–1.171×; control 0.946×–1.045×.
- `caller/off`: 1.026×–1.122×; control 0.956×–1.008×.
- `long/active`: 0.823×–1.021×; control 0.928×–1.041×.
- `long/expired`: 1.159×–1.243×; control 0.841×–1.079×.
- `long/future`: 1.428×–1.659×; control 1.000×–1.039×.
- `long/hidden`: 0.997×–1.034×; control 0.928×–1.000×.
- `long/missing`: 0.992×–1.031×; control 0.959×–1.086×.
- `long/zero-width`: 1.047×–1.392×; control 0.956×–1.057×.
- `scale/infinite`: 0.510×–0.575×; control 1.000×–1.247×.
- `scale/large`: 0.557×–0.777×; control 0.974×–1.061×.
- `scale/nan`: 0.689×–0.716×; control 0.964×–1.070×.
- `scale/negative`: 0.548×–0.566×; control 0.949×–1.035×.
- `scale/negative-zero`: 1.660×–1.667×; control 0.852×–1.059×.
- `scale/one`: 0.552×–0.609×; control 0.955×–1.013×.
- `scale/subnormal`: 0.986×–1.007×; control 0.998×–1.040×.
- `scale/zero`: 1.660×–1.667×; control 0.944×–1.083×.

Raw paired medians below 0.970×:

- `scale/infinite`: 3.95 → 7.09 cycles; 0.557×, control 1.002×.
- `scale/large`: 3.95 → 7.09 cycles; 0.557×, control 1.000×.
- `scale/nan`: 3.95 → 5.52 cycles; 0.716×, control 1.000×.
- `scale/negative`: 3.95 → 7.19 cycles; 0.556×, control 0.986×.
- `scale/one`: 3.95 → 7.09 cycles; 0.557×, control 1.000×.

The scalar zero fast path has a measured tradeoff: normal nonzero examples add about three cycles per call, and the NaN example adds about 1.6 cycles. Subnormal results stay near their controls. Active average drawing is **0.978×** against **0.977×** control, and the complete active caller is **0.988×** against **0.964×**. These active results are inconclusive. Generic atlas layouts are also close to their controls. The reported gains apply to the measured zero-scale, invisible-tick and one/two-column cases.

## Correctness and provenance

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **516 unit tests** via `cargo test --lib --tests`. All three ordinary test-name/status sets and traces match. Dependencies keep cached release profiles in the assertion gate.
- Complete sprite fingerprints compare every float bit in center, world-z, size, tint, glow, UV, fade and rotations, plus source, flip flags, blend, z, order and Vec capacity. Both individual helpers and complete composition use frozen references, including a local copy of the original mini helper so old callers retain fused math.
- Cold Vec capacities 0/1/16, both with and without prefixes, compare all six allocation counters exactly. All warm timing fixtures retain zero allocations, reallocations, frees, requested/freed bytes and peak bytes.
- Request cases cover float sign/exponent classes, boundaries, signed zeros, NaNs, subnormals, seeded values and visibility/mode flags. Direct mini tests cover all 65,536 high words with four low patterns plus 100,000 seeded binary32 values. Atlas tests cover ordinary/zero/huge dimensions, cast wraps, frame boundaries and panic payloads within each profile.
- The shared atlas trace excludes wrapped-zero row counts: the original release calculation wraps while assertion-enabled code panics. Ordinary tests verify old/current parity for these profile-specific cases. Common-domain outputs and shared division panics are traced. Standard zero dimensions normalize to one and remain included.
- All three **5,919,380-byte traces** match, SHA-256 `18eeea5adc629637d6e925ec7fa3a6f15124c95bb630fbd700c49a652fd44472`. Parts retain separate fingerprints below.
  - `feedback`: 2,407,040 bytes; `e8cf98de3e08f78210afb8d86b1c5cc742e8fb46ea58b514039302cf3093c762`.
  - `atlas`: 3,512,324 bytes; `693e5778dbbf5ec3544721bee20925eb5363bf6a156359a725706645d57de66c`.
- All **800 current timing/allocation rows** match raw logs and exact input identities. The audit verifies **46 package inputs**, source/dependency archives, compiler/lock/profiles, binaries, logs and traces.
- Sixteen external fixtures were frozen before measurement and reused unchanged. Each stage records their hashes and the shipped asset archive, containing 17 top-level PNGs plus two nested texture PNGs; all 19 signatures are checked.
- Scoped Clippy exits successfully with zero diagnostics in the changed methods and four new fixture/reference files. The production-only offline locked release check passes with an opt-level-3 non-test artifact and assertions/overflow checks disabled.

## Rejected average-loop rewrite

The first actual package cohort moved average-loop scale work after visibility checks. It passed 515 tests in each of three gates and improved inactive cases, but slowed active averages. Its complete sources, binaries, logs, traces, three manifests and two TSVs remain immutable under `C:\deadsync-feedback-inline-20261105`; the checks JSON verifies all **640 preserved trial rows** and includes its results/provenance.

| Rejected workload | Frozen cycles | Trial cycles | Paired throughput | Control |
| --- | ---: | ---: | ---: | ---: |
| `average/off` | 33.55 | 22.87 | 1.467× | 0.990× |
| `average/active` | 398.75 | 431.37 | 0.939× | 0.987× |
| `caller/active` | 491.91 | 512.88 | 0.933× | 0.990× |

The final implementation restores the original average-loop control flow and specializes the existing scalar helper instead. A direct-guard/inlining alternative retained the active slowdown. General power-of-two atlas and non-inlined variants showed fallback costs; the retained atlas specialization targets one/two columns. Six model prototype cohorts retain source/executable/log fingerprints. Their reduced drawing representation was used for screening; the table above uses actual package measurements.

## Reproduction

`scripts/benchmark_notefield_feedback.py` snapshots the actual package, dependency closure, shipped assets and frozen external fixtures in the established six-member scratch workspace. Compiler/test temporary storage is on C:, with source/target on D:. Root Cargo files/dependency versions remain unchanged; scratch lock SHA is `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_NOTEFIELD_FEEDBACK_ARCHIVE = 'C:\deadsync-feedback-mini-20261105'
python scripts/benchmark_notefield_feedback.py baseline --baseline 91d43132fcd23024928a32f54cfb0cdf886d1b7b --runs 5 --reference baseline
python scripts/benchmark_notefield_feedback.py final --runs 5 --reference baseline
python scripts/benchmark_notefield_feedback.py debug --debug --reference baseline
```

Run stages sequentially with the frozen fixture set in the archive's `frozen-fixtures` directory. After the assertion gate restore the final normal workspace manifest before production checks. Each timing TSV contains 400 rows. Baseline restores the three original production modules and adds identical private test registrations to the two feedback modules; reference/fixture source is identical across stages.
