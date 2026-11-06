# Combo flow and hold-feedback geometry — 2026-11-05

Source baseline: `7becb497857a0296ea923f4b835b1eea2687d4ad`. Three safe Rust changes in `deadsync-notefield`:

1. `combo_feedback.rs::compose_combo_milestones` checks asset availability and an empty milestone slice before calculating mini zoom. Visible milestones keep their existing arithmetic and insertion order.
2. `compose_combo_number` contains its former single-use `append_combo_number` body directly, checking show/font/threshold before mini zoom. Miss-combo precedence and every prepared-text field remain unchanged. A targeted inline hint removes the active-path cost observed in the first trial.
3. `judgment_feedback.rs::append_hold_indicators` returns before geometry setup when neither available sprite has feedback in the bounded lane prefix. Active feedback retains the original geometry, expiry checks and sprite calculations. The guard adds a presence scan on nonempty paths.

Production code adds **47 lines and removes 42** (net +5), including one inline attribute and excluding the two private test registrations. One single-use helper is removed. The hundred-combo easing helper, text-slot cursor and all production types, fields, dependencies and allocation policies retain their original implementations. No unsafe code, production helper, cache, buffer or lookup table is added. Machine-code size and stack changes are unmeasured.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz; actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes run twelve alternating original/current rounds pinned to CPU 2, discard the first and retain the median of eleven. QueryThreadCycleTime measures thread CPU cycles. Cycle columns are separate process medians; paired throughput is the median of five within-process ratios. Identical-body controls use the same fixture/reference source with the original production bodies restored.

Combo fixtures repeat 30,000 times and hold fixtures 10,000 times. Inputs and Vec capacity 16/64 are prepared outside timing; outputs are consumed. The counted-System allocator remains installed with counting disabled during CPU timing. All six heap counters are measured separately and stay zero in every warm fixture, for both variants. Cold capacities, existing output prefixes and all six counters match exactly in the correctness tests. Owned builds and timing jobs run sequentially; unrelated user activity is uncontrolled.

The `milestones`, `number` and `holds` fixtures invoke actual production methods. `caller` combines the two combo entry points. `hold-caller` measures a borrowed extracted sequence of the blind check, tap helper and hold helper, with tap absent; it is not a measurement of the complete by-value HUD/application caller. `hundred` fixtures retain unchanged easing arithmetic as additional controls. The mini inputs are 0 and 0.5. Application-wide input frequencies, frame rate, GPU work and application-wide gains are unmeasured.

## Before/after results

| Workload | Frozen cycles | Current cycles | Paired CPU throughput | Identical-body control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `caller/active-positive` | 446.21 | 464.06 | 0.962× | 0.981× | 0 → 0 | 0 → 0 |
| `caller/active-zero` | 428.81 | 427.15 | 0.977× | 1.007× | 0 → 0 | 0 → 0 |
| `hold-caller/active` | 963.76 | 961.43 | 0.991× | 1.014× | 0 → 0 | 0 → 0 |
| `hold-caller/empty` | 98.73 | 34.35 | 2.874× | 1.000× | 0 → 0 | 0 → 0 |
| `holds/active` | 904.45 | 948.20 | 0.977× | 1.001× | 0 → 0 | 0 → 0 |
| `holds/empty` | 106.63 | 35.56 | 2.908× | 1.029× | 0 → 0 | 0 → 0 |
| `holds/empty-effects` | 327.56 | 34.37 | 8.783× | 1.042× | 0 → 0 | 0 → 0 |
| `holds/empty-wide` | 207.27 | 92.63 | 2.343× | 1.028× | 0 → 0 | 0 → 0 |
| `holds/expired` | 100.42 | 99.15 | 1.037× | 1.008× | 0 → 0 | 0 → 0 |
| `holds/late` | 598.55 | 586.99 | 0.995× | 1.009× | 0 → 0 | 0 → 0 |
| `holds/missing-assets` | 18.11 | 18.20 | 0.995× | 0.993× | 0 → 0 | 0 → 0 |
| `holds/prepared-active` | 399.23 | 385.90 | 1.056× | 0.991× | 0 → 0 | 0 → 0 |
| `holds/prepared-empty` | 44.82 | 34.72 | 1.303× | 1.020× | 0 → 0 | 0 → 0 |
| `holds/prepared-expired` | 46.16 | 50.84 | 0.908× | 1.090× | 0 → 0 | 0 → 0 |
| `holds/unmatched` | 90.39 | 30.82 | 3.024× | 1.015× | 0 → 0 | 0 → 0 |
| `holds/zero-cols` | 70.90 | 18.77 | 3.777× | 1.048× | 0 → 0 | 0 → 0 |
| `hundred/early` | 363.58 | 366.52 | 1.002× | 1.004× | 0 → 0 | 0 → 0 |
| `hundred/late` | 113.57 | 119.85 | 0.980× | 1.010× | 0 → 0 | 0 → 0 |
| `milestones/active-positive` | 399.79 | 399.63 | 0.971× | 0.969× | 0 → 0 | 0 → 0 |
| `milestones/active-zero` | 390.96 | 394.10 | 0.966× | 1.009× | 0 → 0 | 0 → 0 |
| `milestones/empty-positive` | 30.44 | 24.09 | 1.300× | 1.000× | 0 → 0 | 0 → 0 |
| `milestones/empty-zero` | 24.25 | 27.06 | 1.001× | 1.018× | 0 → 0 | 0 → 0 |
| `milestones/hidden-positive` | 23.44 | 23.46 | 1.000× | 1.000× | 0 → 0 | 0 → 0 |
| `milestones/hidden-zero` | 28.66 | 28.42 | 1.000× | 1.000× | 0 → 0 | 0 → 0 |
| `milestones/hundred-late-positive` | 156.13 | 156.39 | 0.997× | 0.989× | 0 → 0 | 0 → 0 |
| `milestones/hundred-late-zero` | 156.92 | 152.72 | 1.028× | 0.981× | 0 → 0 | 0 → 0 |
| `milestones/missing-positive` | 30.26 | 23.41 | 1.294× | 0.992× | 0 → 0 | 0 → 0 |
| `milestones/missing-zero` | 25.21 | 24.93 | 0.998× | 1.000× | 0 → 0 | 0 → 0 |
| `milestones/thousand-positive` | 208.20 | 208.88 | 0.992× | 1.007× | 0 → 0 | 0 → 0 |
| `milestones/thousand-zero` | 210.79 | 218.96 | 1.015× | 1.036× | 0 → 0 | 0 → 0 |
| `number/active-positive` | 76.66 | 69.85 | 1.048× | 0.998× | 0 → 0 | 0 → 0 |
| `number/active-zero` | 59.47 | 61.82 | 1.056× | 0.986× | 0 → 0 | 0 → 0 |
| `number/below-positive` | 28.16 | 16.54 | 1.676× | 0.994× | 0 → 0 | 0 → 0 |
| `number/below-zero` | 19.20 | 16.03 | 1.224× | 0.995× | 0 → 0 | 0 → 0 |
| `number/hidden-positive` | 19.05 | 13.57 | 1.413× | 1.000× | 0 → 0 | 0 → 0 |
| `number/hidden-zero` | 18.06 | 12.99 | 1.401× | 0.999× | 0 → 0 | 0 → 0 |
| `number/miss-positive` | 69.53 | 68.35 | 1.039× | 0.998× | 0 → 0 | 0 → 0 |
| `number/miss-zero` | 61.65 | 57.81 | 1.086× | 1.091× | 0 → 0 | 0 → 0 |
| `number/missing-positive` | 27.13 | 14.05 | 1.977× | 1.000× | 0 → 0 | 0 → 0 |
| `number/missing-zero` | 18.25 | 13.23 | 1.378× | 1.000× | 0 → 0 | 0 → 0 |

Process throughput ranges (minimum–maximum), without control normalization:

- `caller/active-positive`: 0.947×–0.976×; control 0.965×–1.033×.
- `caller/active-zero`: 0.953×–1.004×; control 0.970×–1.068×.
- `hold-caller/active`: 0.981×–1.015×; control 0.975×–1.062×.
- `hold-caller/empty`: 2.635×–3.019×; control 0.941×–1.081×.
- `holds/active`: 0.951×–0.993×; control 0.947×–1.056×.
- `holds/empty`: 2.905×–3.059×; control 1.024×–1.043×.
- `holds/empty-effects`: 8.437×–9.787×; control 1.034×–1.082×.
- `holds/empty-wide`: 2.179×–2.352×; control 0.919×–1.053×.
- `holds/expired`: 0.959×–1.046×; control 0.943×–1.026×.
- `holds/late`: 0.947×–1.020×; control 0.966×–1.020×.
- `holds/missing-assets`: 0.994×–1.148×; control 0.990×–1.190×.
- `holds/prepared-active`: 0.961×–1.065×; control 0.963×–1.033×.
- `holds/prepared-empty`: 1.264×–1.331×; control 1.011×–1.048×.
- `holds/prepared-expired`: 0.908×–0.944×; control 1.041×–1.164×.
- `holds/unmatched`: 2.792×–3.089×; control 0.924×–1.015×.
- `holds/zero-cols`: 3.490×–3.994×; control 1.018×–1.160×.
- `hundred/early`: 0.992×–1.006×; control 0.959×–1.074×.
- `hundred/late`: 0.923×–1.100×; control 0.971×–1.054×.
- `milestones/active-positive`: 0.954×–1.045×; control 0.938×–1.005×.
- `milestones/active-zero`: 0.901×–1.018×; control 0.945×–1.033×.
- `milestones/empty-positive`: 1.205×–1.322×; control 0.931×–1.063×.
- `milestones/empty-zero`: 0.859×–1.013×; control 1.000×–1.080×.
- `milestones/hidden-positive`: 0.999×–1.000×; control 0.817×–1.089×.
- `milestones/hidden-zero`: 0.876×–1.008×; control 0.989×–1.020×.
- `milestones/hundred-late-positive`: 0.970×–1.016×; control 0.985×–1.030×.
- `milestones/hundred-late-zero`: 1.004×–1.045×; control 0.964×–1.015×.
- `milestones/missing-positive`: 1.291×–1.311×; control 0.936×–1.004×.
- `milestones/missing-zero`: 0.955×–1.011×; control 1.000×–1.001×.
- `milestones/thousand-positive`: 0.963×–1.009×; control 0.972×–1.015×.
- `milestones/thousand-zero`: 0.956×–1.018×; control 0.981×–1.063×.
- `number/active-positive`: 0.998×–1.125×; control 0.956×–1.072×.
- `number/active-zero`: 0.953×–1.169×; control 0.940×–1.019×.
- `number/below-positive`: 1.565×–1.720×; control 0.902×–1.045×.
- `number/below-zero`: 1.155×–1.232×; control 0.911×–1.029×.
- `number/hidden-positive`: 1.285×–1.505×; control 0.998×–1.054×.
- `number/hidden-zero`: 1.277×–1.415×; control 0.934×–1.091×.
- `number/miss-positive`: 0.983×–1.094×; control 0.958×–1.002×.
- `number/miss-zero`: 0.975×–1.261×; control 0.997×–1.190×.
- `number/missing-positive`: 1.902×–2.202×; control 1.000×–1.000×.
- `number/missing-zero`: 1.377×–1.632×; control 0.975×–1.074×.

The largest measured benefits are empty positive-mini milestones **1.300×** (control 1.000×), missing-font positive-mini numbers **1.977×** (control 1.000×), and empty hold feedback **2.908×** (control 1.029×). Empty hold feedback with visual effects improves **8.783×**.

The extra feedback scan has a cost: prepared expired holds rise **46.16 → 50.84 cycles**, 0.908× (control 1.090×). Active fallback holds are 0.977× (control 1.001×). Combined active positive-mini combo composition is 0.962× (control 0.981×), and zero-mini composition is 0.977× (control 1.007×). These small active changes are mixed and do not establish a general caller improvement. The primary gains apply to the measured empty/disabled paths.

Raw paired medians below 0.970×:

- `caller/active-positive`: 446.21 → 464.06 cycles; 0.962×, control 0.981×.
- `holds/prepared-expired`: 46.16 → 50.84 cycles; 0.908×, control 1.090×.
- `milestones/active-zero`: 390.96 → 394.10 cycles; 0.966×, control 1.009×.

## Correctness and profile-specific traces

- Release baseline/candidate and assertion-enabled package baseline/candidate each pass **520 unit tests**, with zero integration targets and identical ordinary test-name/status sets. The assertion profile uses package opt-level 0 with debug assertions and overflow checks enabled; dependencies retain cached release settings.
- Frozen originals compare all sprite/text fields, float bits, texture keys, order and Vec capacity. Combo cases cover signed zero, exponent classes, boundary neighbors, infinities, NaNs, seeded binary32 values, absent assets/fonts, empty/hidden requests, threshold zero, u32 limits and miss precedence. Hold cases cover absent/unmatched sprites, empty/short/full lane slices, zero/oversized lane counts, prepared/fallback geometry, visual effects, active/expired feedback and existing prefixes.
- The **39,885,136-byte release traces** match before/after, SHA-256 `a6e168dc5d0588cf30b06c1364f88ed4ce4317b1aac554e4de51e2f230539c41`. The **39,885,136-byte assertion traces** independently match before/after, SHA-256 `e3ec1d8c12ee4d53abeb26f3e1bca3bcf8f9999c2d4054c2659ca2f8baca9dfe`. Every output bit is retained; there is no tolerance or output normalization.
- Release versus assertion traces differ in 128 combo words for two seeded inputs: 64 differences also occur in the unchanged frozen original, and 64 in current output. The hold trace matches across all four gates. A matching old-source assertion baseline was added to verify exact before/after behavior under that profile. The initial cross-profile checkpoint failure and its archived metadata remain recorded. Rust documents unspecified precision for [f32::powf](https://doc.rust-lang.org/std/primitive.f32.html#method.powf); profile-dependent power evaluation is a possible explanation, rather than a demonstrated root cause.
- The initially failed assertion manifest was completed from verified archived source/driver files and the successful same-profile comparison. Its recovery record preserves the original manifest hash, initial release-reference failure and all 128 differing words. The existing assertion candidate executable, test log and output bytes were reused unchanged.
- All **800 current timing/allocation rows** match raw logs; the audit verifies **167 package input snapshots**, compiler, lock, source/dependency archives, binaries and traces. Sixteen frozen external fixtures and all 19 shipped PNGs (17 top-level and two nested) retain their hashes/signatures.
- Scoped Clippy passes with zero diagnostics in the changed methods and four new fixture/reference files. A production-only offline locked opt-level-3 check passes with assertions/overflow checks disabled. The scratch workspace is restored to its normal release manifest and current source bytes after the assertion baseline.

Trace parts:

- release `combo`: 38,513,728 bytes; `5bee81f7b276fcbdc498636f105ad74a76865b8486ff5b5dbe719b2d15ce5b2f`.
- release `holds`: 1,371,392 bytes; `311d310dcba5344bd3e8fba2b0a5be5b3da73592b3228fb7267a8397c3b7302b`.
- assertion `combo`: 38,513,728 bytes; `58c7130e6034790e3850a5e7a3cf7a3b12e5f2502bd3d8447690c3a407b5517c`.
- assertion `holds`: 1,371,392 bytes; `311d310dcba5344bd3e8fba2b0a5be5b3da73592b3228fb7267a8397c3b7302b`.

## Rejected trials

All **1,180 earlier timing/allocation rows**, source snapshots, executables, logs and traces remain preserved in the archive and manifests/TSVs below. Their stage labels predate the retained hold-guard cohort; the legacy file named `final` is the rejected cursor trial.

- Removing the repeated hundred-combo easing clamp gave only small/inconclusive isolated improvements (1.016×/1.037× without a matching isolated control). The original easing helper and arithmetic were restored.
- The first number-flow rewrite slowed active number rendering; the retained targeted inline hint removed that observed cost in subsequent package measurements.
- Capping the edit-text cursor at construction produced cursor timings near their controls and mixed full-emitter results. It was rejected; `measure_actors.rs` is restored exactly to the source baseline. Its two temporary test/reference files were archived and removed from the worktree.
- The first screening package passed 518 tests but failed its cross-build trace checkpoint on 16 NaN sign words, eight also in unchanged frozen output. Its five-process timing data is explicitly screening-only; the failed checkpoint was preserved. The later cursor cohort passed 520 tests and matched its fresh release control trace.
- One hold-harness setup failed while extracting a generic fixture function. The prematurely launched owned build was stopped, the incomplete setup archived, and only the missing setup steps were corrected before the retained cohort ran.

| Stage | Unit tests | Raw rows | Manifest | Timings |
| --- | ---: | ---: | --- | --- |
| `baseline` | 518 | 240 | [manifest](./2026-11-05-notefield-combo-baseline.manifest.json) | [TSV](./2026-11-05-notefield-combo-baseline.tsv) |
| `candidate` | 518 | 260 | [manifest](./2026-11-05-notefield-combo-candidate.manifest.json) | [TSV](./2026-11-05-notefield-combo-candidate.tsv) |
| `control` | 520 | 340 | [manifest](./2026-11-05-notefield-combo-control.manifest.json) | [TSV](./2026-11-05-notefield-combo-control.tsv) |
| `final` | 520 | 340 | [manifest](./2026-11-05-notefield-combo-final.manifest.json) | [TSV](./2026-11-05-notefield-combo-final.tsv) |
| `hold-control` | 520 | 400 | [manifest](./2026-11-05-notefield-combo-hold-control.manifest.json) | [TSV](./2026-11-05-notefield-combo-hold-control.tsv) |
| `hold-trial` | 520 | 400 | [manifest](./2026-11-05-notefield-combo-hold-trial.manifest.json) | [TSV](./2026-11-05-notefield-combo-hold-trial.tsv) |
| `hold-debug-control` | 520 | 0 | [manifest](./2026-11-05-notefield-combo-hold-debug-control.manifest.json) | — |
| `hold-debug` | 520 | 0 | [manifest](./2026-11-05-notefield-combo-hold-debug.manifest.json) | — |

The [checks artifact](./2026-11-05-notefield-combo-checks.json) records exact inputs, per-process measurements, profile comparisons, failed-checkpoint provenance and rejected trials. Full archived artifacts are under `C:\deadsync-combo-flow-20261105`.

## Reproduction

Run stages serially in the established six-member scratch workspace. Source/target remain on D:, compiler temporary storage on C:, and the source archive/dependency versions remain fixed. Scratch lock SHA-256: `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`. The driver supports explicit same-profile trace references.

```powershell
$env:DEADSYNC_COMBO_FLOW_ARCHIVE = 'C:\deadsync-combo-flow-20261105'
$env:DEADSYNC_FEEDBACK_FIXTURE_ROOT = 'C:\deadsync-feedback-mini-20261105\frozen-fixtures'
python scripts/benchmark_notefield_combo.py hold-control --baseline 7becb497857a0296ea923f4b835b1eea2687d4ad --runs 5 --reference hold-control
python scripts/benchmark_notefield_combo.py hold-trial --runs 5 --reference hold-control
python scripts/benchmark_notefield_combo.py hold-debug-control --baseline 7becb497857a0296ea923f4b835b1eea2687d4ad --debug --reference hold-debug-control
python scripts/benchmark_notefield_combo.py hold-debug --debug --reference hold-debug-control
```

Use a new archive directory when reproducing to preserve the recorded measurements. Each release timing TSV has 400 rows. Restore the normal manifest and measured current source overlays before production checks. The baseline option restores both original production modules and adds identical private test registrations; fixture/reference source remains identical across the retained profile pairs.
