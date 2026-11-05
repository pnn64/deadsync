# Config preference allocations — 2026-11-05

Baseline: `6c782aee5e007955874182839403df5e6fbf5a1c`. Three separately measured routines in `deadsync-config`:

1. `normalize_locale` classifies Chinese locale bases up to 64 bytes in a fixed stack buffer and allocates the final seven-byte canonical string directly. This removes the temporary normalized heap string. Underscore replacement cannot change the ASCII letter-only script/region matches. Non-Chinese inputs and longer Chinese inputs keep the original replacement/lowercase path. The redundant simplified-Chinese checks are removed: every remaining Chinese case already returns `zh-Hans`. Traditional matches still take precedence over simplified ones, including unusual prefix/substring inputs accepted before the change.
2. `MachineFont::from_str` matches the trimmed byte length and compares the existing aliases with ASCII case-insensitive equality. It removes the lowercase String allocation while preserving strict punctuation rejection and Unicode whitespace trimming.
3. `parse_legacy_present_mode` applies the same direct strategy to its independent generic `Option<P>` result, preserving `balanced`, `unhinged`, `maxfps`, `max_fps` and `max-fps`. It returns the supplied copyable policy unchanged. Config lookup order, callbacks and defaults are untouched.

The three production bodies change by **+32/−16 lines** (net +16), excluding test-only module registrations/visibility. Execution removes one temporary allocation from normal Chinese locales, both parser allocations, and a redundant locale branch. The locale fast path uses a **64-byte source-level stack buffer**; this is not a measurement of total stack-frame or binary size. No production unsafe, dependency, public API or cache is added. Four test-only visibility changes reuse the existing unit-test allocation counter and CPU clock instead of installing another allocator.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use release opt-level 3, fat LTO and static CRT. Five fresh processes per namespace run twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Builds and our benchmark processes run sequentially; unrelated user activity is not controlled. The baseline compiles unchanged production alongside frozen references and exposes code-generation/timing bias through identical-source controls.

Cycle columns are independent process medians; paired throughput is the median of five within-process ratios and can differ from the quotient of those medians. TSVs retain all cycles, nanoseconds and six allocation counters for all five processes. The counted-System allocator remains installed during timing with counting disabled, retaining its inactive TLS check. Fixture construction is outside timing; returned strings are materialized, consumed and destroyed inside timing. Locale cases use 10,000 repetitions; each parser case uses 20,000. Timing and trace tests run explicitly outside ordinary correctness gates.

These are steady-state in-memory preference microbenchmarks. The routines run during preference/locale loading; application startup, filesystem work, frame rate and end-to-end config loading are not measured. No gameplay throughput improvement is inferred.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `font/empty` | 18.15 | 10.36 | 1.708× | 0.904× | 0 → 0 | 0 → 0 |
| `font/invalid` | 177.63 | 19.15 | 9.350× | 0.996× | 1 → 0 | 5 → 0 |
| `font/short` | 171.75 | 23.52 | 7.070× | 1.011× | 1 → 0 | 4 → 0 |
| `font/spaced` | 205.81 | 45.28 | 4.536× | 1.009× | 1 → 0 | 5 → 0 |
| `font/valid` | 187.60 | 35.59 | 5.106× | 1.009× | 1 → 0 | 14 → 0 |
| `locale/empty` | 49.85 | 56.02 | 0.890× | 1.012× | 0 → 0 | 0 → 0 |
| `locale/en` | 225.76 | 223.54 | 0.997× | 1.005× | 1 → 1 | 5 → 5 |
| `locale/len4096` | 10154.97 | 8572.48 | 1.167× | 1.001× | 2 → 2 | 4103 → 4103 |
| `locale/len62` | 710.81 | 508.63 | 1.397× | 0.993× | 2 → 1 | 69 → 7 |
| `locale/len63` | 726.15 | 524.85 | 1.377× | 1.005× | 2 → 1 | 70 → 7 |
| `locale/len64` | 671.98 | 470.56 | 1.426× | 1.013× | 2 → 1 | 71 → 7 |
| `locale/len65` | 677.42 | 575.68 | 1.173× | 1.008× | 2 → 2 | 72 → 72 |
| `locale/len66` | 684.42 | 573.82 | 1.191× | 1.013× | 2 → 2 | 73 → 73 |
| `locale/unicode` | 712.85 | 243.58 | 2.927× | 1.002× | 2 → 1 | 16 → 7 |
| `locale/zh-default` | 796.94 | 261.40 | 3.049× | 1.000× | 2 → 1 | 18 → 7 |
| `locale/zh-hant` | 411.74 | 222.51 | 1.831× | 1.005× | 2 → 1 | 17 → 7 |
| `locale/zh-tw` | 421.24 | 233.04 | 1.806× | 0.989× | 2 → 1 | 12 → 7 |
| `mode/balanced` | 210.25 | 28.84 | 7.380× | 1.065× | 1 → 0 | 8 → 0 |
| `mode/empty` | 23.06 | 14.23 | 1.618× | 0.995× | 0 → 0 | 0 → 0 |
| `mode/invalid` | 250.43 | 27.61 | 9.340× | 1.255× | 1 → 0 | 7 → 0 |
| `mode/max` | 233.94 | 39.80 | 6.090× | 1.165× | 1 → 0 | 7 → 0 |
| `mode/unhinged` | 226.33 | 31.53 | 7.385× | 1.234× | 1 → 0 | 8 → 0 |

All process throughput ranges (minimum–maximum) are retained below; small changes near the controls should be treated as inconclusive.

Observed raw paired medians below 0.970×:

- `locale/empty`: 49.85 → 56.02 cycles, 0.890×; identical-source control 1.012×. This cost is retained alongside the demonstrated allocation and CPU benefits in other inputs of the same routine.

Legacy-mode identical-source control medians span 0.995×–1.255×. Those material context effects limit attribution of the raw legacy-mode ratios to source changes alone.

- `font/empty`: 1.707×–1.871×; control 0.833×–1.021×.
- `font/invalid`: 8.656×–10.225×; control 0.974×–1.023×.
- `font/short`: 6.724×–8.257×; control 0.991×–1.044×.
- `font/spaced`: 4.043×–5.201×; control 0.985×–1.016×.
- `font/valid`: 4.768×–5.274×; control 0.983×–1.016×.
- `locale/empty`: 0.766×–1.019×; control 0.921×–1.030×.
- `locale/en`: 0.878×–1.065×; control 1.000×–1.042×.
- `locale/len4096`: 1.164×–1.198×; control 0.992×–1.018×.
- `locale/len62`: 1.357×–1.406×; control 0.977×–1.051×.
- `locale/len63`: 1.369×–1.385×; control 0.989×–1.010×.
- `locale/len64`: 1.419×–1.452×; control 0.985×–1.024×.
- `locale/len65`: 1.160×–1.185×; control 0.997×–1.036×.
- `locale/len66`: 1.168×–1.231×; control 0.962×–1.018×.
- `locale/unicode`: 2.873×–2.980×; control 0.977×–1.023×.
- `locale/zh-default`: 3.020×–3.211×; control 0.958×–1.015×.
- `locale/zh-hant`: 1.737×–1.885×; control 0.992×–1.011×.
- `locale/zh-tw`: 1.780×–1.894×; control 0.976×–1.032×.
- `mode/balanced`: 6.035×–7.521×; control 1.046×–1.367×.
- `mode/empty`: 1.553×–1.639×; control 0.992×–1.006×.
- `mode/invalid`: 8.280×–10.026×; control 1.143×–1.389×.
- `mode/max`: 5.873×–6.319×; control 1.068×–1.317×.
- `mode/unhinged`: 7.176×–7.533×; control 1.191×–1.351×.

## Correctness and allocation evidence

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **249 unit tests and 13 integration tests**, across all five integration targets (`controller_text`, `frame_rings`, `ini_uuid_kernels`, `light_text`, `preference_codecs`). Existing config load/default/legacy-key tests remain included. Dependencies retain their cached release profiles in the assertion-enabled gate. All three gates have identical ordinary test-name/status sets.
- The locale differential test compares exact bytes and public String capacities for every Unicode scalar in three prefix contexts, 0–79 repeats of ASCII/underscore/multibyte/script tokens, mixed script/region precedence, suffix delimiters, NUL/BOM, and 16,384 deterministic mixed Unicode strings. This covers the 64-byte boundary from both sides. It preserves the old broad `zh` prefix and substring semantics rather than interpreting a stricter locale grammar.
- Font and legacy-mode tests cover every ASCII case combination of their aliases, Unicode whitespace wrapping, all ASCII insertions at every alias boundary, every Unicode scalar as a prefix, malformed punctuation and invalid/empty strings. The generic mode test uses opaque two-word policies containing arbitrary bit patterns, verifying the returned payload without floating-point transformations.
- A **15,808,640-byte combined trace** matches byte-for-byte across all three gates: SHA-256 `1d7f991a9000c87599481f45b4bf12ec26967e2213a888dcca9846b9bd1b4f45`. It includes input bytes, canonical locale bytes/capacities, exact font outcomes, and full policy option/payload bits. Traces sample the scalar range at stride 17 and retain the complete alias/boundary/random cohorts; the ordinary tests independently cover every scalar. Part identities:
  - `theme`: 14,368,818 bytes, SHA-256 `839e04544efa9cc106f95af04f36599d6120440f2756786f1335f73017c7ff07`.
  - `mode`: 1,439,806 bytes, SHA-256 `2e5688ec21ba4c26de498903436735ea1c8a9812e014c3af84daec546212178c`.
- All **440 raw rows** are checked against their process logs. Allocation/frees and requested allocated/freed bytes balance; every case has zero reallocations. Both nonempty parsers go from one allocation to zero; empty strings already allocate nothing. Short Chinese locale results go from two allocations to one seven-byte allocation. English, empty, and longer-than-64-byte locale allocation counts/capacities are preserved. Peak requested bytes equal the table's requested byte totals for these fixtures; allocator metadata/RSS and the stack buffer are outside these counters.
- Scoped Clippy has zero changed-scope diagnostics; production-only `cargo check --offline --locked --lib --release` passes with the package restored to opt-level 3 and assertions/overflow checks disabled. The audit verifies all **49 package inputs**, exact three-routine production scope, test-only support changes, frozen Git bodies, source/dependency archives, compiler/lock/profiles, complete ordinary test sets, binary/log hashes, trace parts and every timing/counter field. It also verifies archived exploratory source/binary/log identities. Other out-of-scope Clippy messages remain recorded.

## Reproduction and withdrawn experiment

`scripts/benchmark_config_preferences.py` snapshots the Git dependency closure and exact checkout overlays into the existing scratch workspace, now with five members (simfile, notefield, profile, input-fsr, config). It preserves dependency mtimes, compiles with temporary storage on C:, uses host C: temporary storage for filesystem tests, and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; the resolved lock hash remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_CONFIG_PREFERENCES_ARCHIVE = 'C:\deadsync-config-preferences-20261105'
python scripts/benchmark_config_preferences.py baseline --baseline 6c782aee5e007955874182839403df5e6fbf5a1c --runs 5 --reference baseline
python scripts/benchmark_config_preferences.py final --runs 5 --reference baseline
python scripts/benchmark_config_preferences.py debug --debug --reference baseline
```

Run stages sequentially using the archived compiler, lock and workspace inputs. Baseline overlays the original theme/options modules plus only the new test registrations; test-only visibility and new reference/benchmark files are common to all stages. Manifests retain exact sources, flags, profiles, lock, binary/log hashes and trace-part identities. TSVs retain 220 rows per measured stage; checks JSON records controls, process ranges, all 440 verified rows and validation test sets.

A second standalone locale experiment scanned borrowed bytes case-insensitively without a stack buffer or size cutoff. It passed differential checks and helped short locales, but the 4,096-byte Chinese case fell to about 0.47× throughput in one process. It is withdrawn. The retained bounded stack path preserves the long-input fallback and is measured in the final Cargo package above. The initial standalone trials are exploratory evidence, separate from final package measurements.
