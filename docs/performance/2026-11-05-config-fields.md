# Boolean, thread-count and tournament fields — 2026-11-05

Source baseline: `c572dc9c05526572068d88ca2f872bad541229d9`. Three safe Rust optimizations in `deadsync-config`:

1. `bools.rs::parse_bool_str` dispatches on trimmed byte-slice width, matching 1/0 and fixed ASCII words with packed little-endian values or small value arrays. All eight named/numeric aliases, strict rejection and Unicode trimming are preserved. ASCII case folding cannot remove UTF-8 high bits or turn punctuation into a literal letter.
2. `numbers.rs::parse_auto_threads_u8` recognizes empty, one-character, three-digit and Auto widths directly. Three digits are accumulated in u16, then checked when converting to u8. Every other spelling still uses the original standard u8 parser, preserving leading plus, arbitrarily many leading zeroes, errors and overflow behavior.
3. `theme.rs::load_theme_presentation_options` matches StepStats Show/Hide from borrowed trimmed text and retains the original loose-boolean fallback. This removes a temporary lowercase String and its destruction. Complete theme options, defaults, precedence and all other loader fields remain unchanged.

The three bodies add **55 lines and remove 22** (net +33), excluding private test registration. No production helper, lookup table, cache, unsafe, dependency, public API or serialization change is added. Packed comparisons use two-to-five-byte value arrays; their physical stack/code-size effects are unmeasured. Ring production code is unchanged.

## Evidence and measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Builds and our benchmark processes run sequentially. Unrelated user activity is not controlled.

There are **26 public-parser/caller fixtures with identical-body clean controls**, plus **six extracted StepStats comparisons**. The extracted old/new decoders intentionally differ in both control and final builds; their control-stage ratio is a repeated comparison, not a source-identical control. Both extracted decoders use the frozen old loose/strict boolean fallback, isolating String removal from boolean-parser speed changes. Their bodies are checked against the production closure with that explicit fallback binding. Complete caller fixtures execute the real public loader and consume the entire returned options record. Caller CPU results combine boolean-matching and allocation-removal effects; the one-allocation reduction specifically comes from StepStats.

Parser/extracted fixtures use 50,000 repetitions; complete-loader fixtures use 10,000. Raw strings, INI objects and default records are prepared outside timing. The counted-System allocator stays installed with counting disabled during CPU timing, retaining its inactive TLS check. All six allocation counters are sampled separately. These are in-memory microbenchmarks; file I/O, startup, UI latency and whole-application gains are unmeasured.

**Initial timings excluded:** archive compression overlapped the first baseline measurements. The original baseline's source, correctness, trace and 320 timing rows remain preserved and audited. A separate `control` stage reruns the old source after maintenance and after the release/assertion pipeline. Only clean-control + final measurements (**640 rows**) support retention. The audit additionally verifies all 320 excluded rows, for **960 audited rows** total. No measured source or earlier artifact was overwritten. Maintenance completion precedes final and clean-control correctness completion, before their benchmarks. After clean controls, the exact current source overlays and normal release workspace are restored before Clippy/production checks.

| Workload | Scope | Frozen cycles | Current cycles | Paired throughput | Clean-control ratio / extracted repeat | Allocations old → new | Requested bytes old → new |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `bool/empty` | public | 11.04 | 8.83 | 1.167× | 1.000× | 0 → 0 | 0 → 0 |
| `bool/false` | public | 25.46 | 19.57 | 1.296× | 1.037× | 0 → 0 | 0 → 0 |
| `bool/invalid` | public | 16.13 | 14.88 | 1.092× | 1.000× | 0 → 0 | 0 → 0 |
| `bool/no` | public | 21.93 | 18.27 | 1.212× | 1.005× | 0 → 0 | 0 → 0 |
| `bool/off` | public | 22.04 | 19.28 | 1.099× | 1.005× | 0 → 0 | 0 → 0 |
| `bool/on` | public | 21.67 | 18.14 | 1.173× | 1.005× | 0 → 0 | 0 → 0 |
| `bool/one` | public | 17.47 | 15.46 | 1.100× | 1.022× | 0 → 0 | 0 → 0 |
| `bool/space` | public | 36.56 | 30.80 | 1.194× | 1.039× | 0 → 0 | 0 → 0 |
| `bool/true` | public | 23.89 | 18.05 | 1.317× | 1.035× | 0 → 0 | 0 → 0 |
| `bool/unicode` | public | 21.25 | 20.35 | 1.044× | 1.000× | 0 → 0 | 0 → 0 |
| `bool/yes` | public | 22.04 | 19.68 | 1.101× | 1.006× | 0 → 0 | 0 → 0 |
| `bool/zero` | public | 17.98 | 15.88 | 1.110× | 1.043× | 0 → 0 | 0 → 0 |
| `caller/empty` | public | 824.07 | 869.26 | 1.020× | 1.005× | 0 → 0 | 0 → 0 |
| `caller/hide` | public | 1113.85 | 938.38 | 1.192× | 1.021× | 1 → 0 | 4 → 0 |
| `caller/invalid` | public | 1147.09 | 872.29 | 1.267× | 1.001× | 1 → 0 | 7 → 0 |
| `caller/missing` | public | 627.64 | 632.97 | 0.992× | 0.983× | 0 → 0 | 0 → 0 |
| `caller/populated` | public | 1720.70 | 1527.79 | 1.131× | 0.991× | 1 → 0 | 4 → 0 |
| `caller/show` | public | 1019.60 | 852.76 | 1.212× | 1.017× | 1 → 0 | 4 → 0 |
| `number/auto` | public | 21.30 | 24.25 | 0.948× | 0.965× | 0 → 0 | 0 → 0 |
| `number/empty` | public | 10.25 | 10.26 | 0.999× | 0.936× | 0 → 0 | 0 → 0 |
| `number/one` | public | 17.89 | 13.10 | 1.210× | 1.000× | 0 → 0 | 0 → 0 |
| `number/overflow` | public | 30.26 | 21.25 | 1.396× | 0.975× | 0 → 0 | 0 → 0 |
| `number/plus` | public | 19.29 | 18.44 | 1.048× | 0.960× | 0 → 0 | 0 → 0 |
| `number/three` | public | 27.96 | 19.39 | 1.412× | 0.941× | 0 → 0 | 0 → 0 |
| `number/unicode` | public | 20.68 | 21.62 | 0.957× | 1.032× | 0 → 0 | 0 → 0 |
| `number/zeros` | public | 34.04 | 34.68 | 0.966× | 0.906× | 0 → 0 | 0 → 0 |
| `step/bool` | extracted | 199.25 | 51.89 | 3.840× | 3.983× | 1 → 0 | 4 → 0 |
| `step/empty` | extracted | 21.30 | 16.47 | 1.341× | 1.090× | 0 → 0 | 0 → 0 |
| `step/hide` | extracted | 169.87 | 22.39 | 7.794× | 7.948× | 1 → 0 | 4 → 0 |
| `step/invalid` | extracted | 201.87 | 49.22 | 4.179× | 3.959× | 1 → 0 | 7 → 0 |
| `step/number` | extracted | 204.27 | 59.75 | 3.460× | 3.426× | 1 → 0 | 3 → 0 |
| `step/show` | extracted | 167.83 | 20.62 | 7.423× | 7.832× | 1 → 0 | 4 → 0 |

Process throughput ranges (minimum–maximum):

- `bool/empty`: 1.076×–1.250×; clean control / extracted repeat 0.999×–1.001×.
- `bool/false`: 1.250×–1.309×; clean control / extracted repeat 1.013×–1.065×.
- `bool/invalid`: 1.069×–1.100×; clean control / extracted repeat 0.994×–1.000×.
- `bool/no`: 1.172×–1.302×; clean control / extracted repeat 0.974×–1.055×.
- `bool/off`: 1.076×–1.143×; clean control / extracted repeat 0.970×–1.005×.
- `bool/on`: 1.113×–1.195×; clean control / extracted repeat 0.942×–1.052×.
- `bool/one`: 1.082×–1.130×; clean control / extracted repeat 0.954×–1.169×.
- `bool/space`: 1.187×–1.209×; clean control / extracted repeat 1.000×–1.061×.
- `bool/true`: 1.302×–1.362×; clean control / extracted repeat 1.024×–1.075×.
- `bool/unicode`: 1.024×–1.100×; clean control / extracted repeat 1.000×–1.005×.
- `bool/yes`: 1.085×–1.120×; clean control / extracted repeat 0.991×–1.046×.
- `bool/zero`: 1.029×–1.132×; clean control / extracted repeat 0.905×–1.138×.
- `caller/empty`: 0.927×–1.099×; clean control / extracted repeat 0.936×–1.018×.
- `caller/hide`: 1.181×–1.233×; clean control / extracted repeat 0.985×–1.072×.
- `caller/invalid`: 1.173×–1.315×; clean control / extracted repeat 0.985×–1.083×.
- `caller/missing`: 0.966×–1.065×; clean control / extracted repeat 0.956×–1.004×.
- `caller/populated`: 1.116×–1.200×; clean control / extracted repeat 0.977×–1.006×.
- `caller/show`: 1.196×–1.233×; clean control / extracted repeat 0.979×–1.062×.
- `number/auto`: 0.778×–0.971×; clean control / extracted repeat 0.959×–0.966×.
- `number/empty`: 0.902×–1.000×; clean control / extracted repeat 0.929×–1.000×.
- `number/one`: 1.203×–1.366×; clean control / extracted repeat 0.966×–1.006×.
- `number/overflow`: 1.330×–1.702×; clean control / extracted repeat 0.939×–0.994×.
- `number/plus`: 1.031×–1.110×; clean control / extracted repeat 0.953×–1.065×.
- `number/three`: 1.385×–1.467×; clean control / extracted repeat 0.931×–0.950×.
- `number/unicode`: 0.944×–0.966×; clean control / extracted repeat 1.012×–1.037×.
- `number/zeros`: 0.851×–1.000×; clean control / extracted repeat 0.827×–0.935×.
- `step/bool`: 3.796×–3.986×; clean control / extracted repeat 3.738×–4.081×.
- `step/empty`: 1.261×–1.399×; clean control / extracted repeat 1.081×–1.137×.
- `step/hide`: 7.263×–8.142×; clean control / extracted repeat 7.888×–9.048×.
- `step/invalid`: 3.944×–4.246×; clean control / extracted repeat 3.903×–4.193×.
- `step/number`: 3.316×–3.663×; clean control / extracted repeat 3.334×–3.515×.
- `step/show`: 7.219×–8.139×; clean control / extracted repeat 7.333×–8.259×.

Public-path raw paired medians below 0.970×:

- `number/auto`: 21.30 → 24.25 cycles; 0.948×, clean control 0.965×.
- `number/unicode`: 20.68 → 21.62 cycles; 0.957×, clean control 1.032×.
- `number/zeros`: 34.04 → 34.68 cycles; 0.966×, clean control 0.906×.

Differences near their controls are inconclusive. Width specialization primarily benefits the measured one/three-digit cases; fallback and Auto results remain visible. The extracted StepStats speedup is not a claim about the complete loader's speedup.

## Correctness and allocation checks

- Selected clean-control, final release and package opt-level 0 with assertions/overflow checks each pass **255 unit + 13 integration tests**, covering all five config integration targets. The initial baseline also passes and remains archived. All four ordinary test-name/status sets and traces match. Dependencies retain cached release profiles in the assertion-enabled gate.
- Differential inputs include every Unicode scalar in three contexts; all ASCII two-byte combinations and all 2,097,152 ASCII three-byte combinations for thread counts; numeric values 0–1024 with signs, zero prefixes and Unicode whitespace; Auto and every boolean/Show/Hide case combination and ASCII insertion; 32,768 seeded mixed UTF-8 strings. These compare exact Option/bool/u8 values and original numeric fallback behavior.
- Complete public-loader records compare with both tournament defaults, missing/empty/invalid keys, minimal/populated INI maps, targeted aliases and a stride-257 Unicode/random corpus. The public loader's source differs only in the StepStats match expression, apart from test registration. All other fields and helper calls retain their source.
- A **4,980,049-byte trace** matches across all four gates, SHA-256 `f4154e32a8b1f5fab04eafc760d99f074a2e239ad8f028631113fe5f355e34df`. It records raw parser inputs and exact strict-boolean/thread/isolated-step results for stride-17 scalar samples plus numeric/random/ASCII cohorts; targeted complete-loader records include all returned fields, two defaults and two INI shapes. Ordinary tests cover the full scalar/three-byte spaces.
  - `fields` part: 4,980,041 bytes, SHA-256 `9844b87c675ec08fff44d68cb19d63b0566ef31120e5f7b941090199993f416d`.
- Every timing and all six allocation fields match their logs across **960 rows**. Boolean/thread kernels remain allocation-free. Extracted nonempty StepStats drops one allocation/free to zero in both builds by design; empty extracted input already allocates zero. Complete public-loader fixtures with nonempty StepStats drop one allocation/free in final, while empty/missing fields retain parity. Requested/freed/peak byte values and reallocations remain available in raw TSVs/checks; no total-RSS or stack claim is made.
- Clippy exits successfully with zero changed-production-scope diagnostics. One `field_reassign_with_default` warning in the new test-only `check_loaded` fixture is reviewed and retained: it adjusts multiple defaults, including nested tournament fields, before comparing complete records. Measured source bytes remain unchanged. Production-only `cargo check --offline --locked --lib --release` passes with an opt-level-3 non-test artifact and assertions/overflow checks disabled. The audit verifies **61 package inputs**, exact production scope, frozen source/prototypes, archives, compiler/lock/profiles, tests, binaries/logs, traces, maintenance timing and all measurement fields.

## Reproduction and rejected experiments

`scripts/benchmark_config_fields.py` snapshots the actual config package and dependency closure in the existing six-member scratch workspace. It preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Root Cargo files/dependency versions are unchanged; lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_CONFIG_FIELDS_ARCHIVE = 'C:\deadsync-text-words-20261105'
python scripts/benchmark_config_fields.py baseline --baseline c572dc9c05526572068d88ca2f872bad541229d9 --runs 5 --reference baseline
python scripts/benchmark_config_fields.py final --runs 5 --reference baseline
python scripts/benchmark_config_fields.py debug --debug --reference baseline
python scripts/benchmark_config_fields.py control --baseline c572dc9c05526572068d88ca2f872bad541229d9 --runs 5 --reference baseline
```

Run stages sequentially; after control, restore the three final production overlays and final normal workspace manifest before further checks. Keep the original baseline timing exclusion documented. Each timing TSV has 320 rows. Baseline/control restore the original three modules plus the private test registration; common reference/fixture source is identical across stages.

Ring traversal/index/push trials passed extensive correctness checks but showed regressions or limited gains; no ring change was retained. The first thread-width dispatch slowed several fallbacks and was refined before production edits. A boolean trial omitted Off; its correctness failure was preserved and corrected in a separate cohort before any production edit. Four successful prototype cohorts and the failed alias trial retain source/binary/log fingerprints. Actual package measurements above determine retention.

Completed dependency archives were transparently NTFS-compressed to reclaim space. Before/after hashes verified identical contents. The original overlapping timing cohort is excluded explicitly; compression is not part of the optimized program.
