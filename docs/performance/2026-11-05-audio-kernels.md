# Audio clock, overlay anchor and numeric boolean kernels — 2026-11-05

Source baseline: `e9236afd59a74cb4726b500660bfde3f8c0422c0`. Three safe Rust changes in `deadsync-config`:

1. `audio.rs::NoteScrollClock::from_str` handles numeric/empty spellings directly and compares the complete eight-byte RawAudio/Smoothed and eleven-byte FrameStable aliases using packed little-endian values. Other aliases retain their original case-insensitive comparisons. This removes repeated canonical-name comparisons from the general path, preserving every accepted spelling, Unicode trim and exact enum/error result.
2. `machine.rs::canonical_frame_stats_overlay_anchor` replaces the key-array iterator and repeated comparisons with six exact-length packed matches. Case-fold masks affect letter positions only; separator bytes must remain exact, so carriage returns cannot become hyphens. Every unknown value still returns the original static Auto token.
3. `bools.rs::parse_u8_bool_str` trims once, removes one optional leading plus and skips leading zeroes. It checks one/two/three significant digits directly; three-digit arithmetic uses u16 and rejects values above 255. It preserves empty/error, negative, plus-only, arbitrary zero-prefix and overflow behavior while producing the same optional boolean without constructing the intermediate parsed u8 result.

The changed bodies add **119 lines and remove 40** (net +79), excluding private test registration. Packed comparisons use small value arrays, with no normalized-text buffer or lookup table. No new unsafe, heap allocation, dependency, production helper, public API or serialization change is introduced. Physical stack/code-size changes are unmeasured.

## Measurement method and results

Windows x86-64 on Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo test executables use opt-level 3, fat LTO and static CRT. Five fresh processes run twelve alternating old/current rounds, pinned to CPU 2; the first round is discarded and the median of eleven retained. QueryThreadCycleTime measures thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. All 32 baseline comparisons execute public paths with identical old bodies.

Parser/default fixtures use 50,000 repetitions and full-audio fixtures use 10,000. Raw inputs, INI maps and defaults are prepared outside timing, and all returned fields are consumed. Complete audio callers combine clock and numeric-boolean effects. The six heap counters are collected separately; the counted-System allocator remains installed with counting disabled during CPU timing. Builds, owned maintenance and benchmark processes do not run concurrently. Unrelated user activity is uncontrolled.

These are in-memory microbenchmarks. Audio playback, rendering speed, I/O, startup and application-wide performance are unmeasured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-body control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `anchor/empty` | 13.46 | 8.03 | 1.676× | 1.048× | 0 → 0 | 0 → 0 |
| `anchor/first` | 26.65 | 15.75 | 1.702× | 1.086× | 0 → 0 | 0 → 0 |
| `anchor/invalid` | 26.23 | 17.63 | 1.399× | 1.001× | 0 → 0 | 0 → 0 |
| `anchor/last` | 32.72 | 16.37 | 1.999× | 1.087× | 0 → 0 | 0 → 0 |
| `anchor/middle` | 37.89 | 19.12 | 1.982× | 1.109× | 0 → 0 | 0 → 0 |
| `anchor/unicode` | 45.66 | 34.30 | 1.327× | 1.032× | 0 → 0 | 0 → 0 |
| `caller/invalid` | 937.57 | 909.52 | 1.027× | 0.973× | 0 → 0 | 0 → 0 |
| `caller/missing` | 489.57 | 447.71 | 1.007× | 0.983× | 0 → 0 | 0 → 0 |
| `caller/populated` | 1315.64 | 1302.80 | 0.990× | 0.976× | 0 → 0 | 0 → 0 |
| `caller/raw` | 889.90 | 920.56 | 0.998× | 0.999× | 0 → 0 | 0 → 0 |
| `caller/stable` | 939.09 | 914.88 | 1.027× | 1.017× | 0 → 0 | 0 → 0 |
| `caller/zeros` | 1431.18 | 1345.75 | 1.036× | 1.030× | 0 → 0 | 0 → 0 |
| `clock/empty` | 10.72 | 9.74 | 1.074× | 0.970× | 0 → 0 | 0 → 0 |
| `clock/invalid` | 21.85 | 20.04 | 1.090× | 0.965× | 0 → 0 | 0 → 0 |
| `clock/one` | 18.92 | 16.64 | 1.174× | 0.961× | 0 → 0 | 0 → 0 |
| `clock/raw` | 27.71 | 18.55 | 1.463× | 1.008× | 0 → 0 | 0 → 0 |
| `clock/separated` | 38.97 | 38.24 | 1.000× | 0.962× | 0 → 0 | 0 → 0 |
| `clock/smooth` | 28.10 | 20.38 | 1.379× | 1.025× | 0 → 0 | 0 → 0 |
| `clock/stable` | 33.43 | 22.29 | 1.488× | 1.000× | 0 → 0 | 0 → 0 |
| `clock/unicode` | 39.39 | 32.64 | 1.174× | 0.943× | 0 → 0 | 0 → 0 |
| `default/missing` | 3.96 | 3.47 | 1.000× | 1.020× | 0 → 0 | 0 → 0 |
| `default/present` | 13.89 | 14.70 | 1.039× | 0.855× | 0 → 0 | 0 → 0 |
| `numeric/empty` | 7.18 | 8.45 | 0.850× | 1.177× | 0 → 0 | 0 → 0 |
| `numeric/invalid` | 18.72 | 16.35 | 1.145× | 1.000× | 0 → 0 | 0 → 0 |
| `numeric/one` | 15.69 | 13.10 | 1.134× | 1.000× | 0 → 0 | 0 → 0 |
| `numeric/other` | 16.85 | 13.18 | 1.198× | 1.000× | 0 → 0 | 0 → 0 |
| `numeric/overflow` | 21.42 | 19.35 | 1.154× | 1.041× | 0 → 0 | 0 → 0 |
| `numeric/plus` | 15.12 | 15.54 | 1.043× | 0.895× | 0 → 0 | 0 → 0 |
| `numeric/three` | 22.61 | 21.72 | 1.175× | 1.075× | 0 → 0 | 0 → 0 |
| `numeric/unicode` | 24.20 | 23.93 | 1.015× | 1.030× | 0 → 0 | 0 → 0 |
| `numeric/zero` | 16.64 | 13.78 | 1.199× | 1.000× | 0 → 0 | 0 → 0 |
| `numeric/zeros` | 35.23 | 23.67 | 1.457× | 1.133× | 0 → 0 | 0 → 0 |

Process throughput ranges (minimum–maximum):

- `anchor/empty`: 1.644×–1.813×; identical-body control 0.975×–1.121×.
- `anchor/first`: 1.559×–1.744×; identical-body control 1.059×–1.246×.
- `anchor/invalid`: 1.332×–1.488×; identical-body control 0.984×–1.055×.
- `anchor/last`: 1.998×–2.210×; identical-body control 1.007×–1.134×.
- `anchor/middle`: 1.856×–2.035×; identical-body control 0.925×–1.167×.
- `anchor/unicode`: 1.255×–1.444×; identical-body control 0.960×–1.044×.
- `caller/invalid`: 0.983×–1.048×; identical-body control 0.957×–1.052×.
- `caller/missing`: 0.969×–1.107×; identical-body control 0.981×–1.081×.
- `caller/populated`: 0.930×–1.044×; identical-body control 0.960×–1.078×.
- `caller/raw`: 0.922×–1.041×; identical-body control 0.898×–1.095×.
- `caller/stable`: 0.955×–1.153×; identical-body control 0.969×–1.083×.
- `caller/zeros`: 1.034×–1.084×; identical-body control 0.980×–1.062×.
- `clock/empty`: 1.053×–1.133×; identical-body control 0.926×–1.206×.
- `clock/invalid`: 1.067×–1.119×; identical-body control 0.917×–1.013×.
- `clock/one`: 1.085×–1.297×; identical-body control 0.907×–0.974×.
- `clock/raw`: 1.372×–1.494×; identical-body control 0.884×–1.129×.
- `clock/separated`: 0.984×–1.053×; identical-body control 0.935×–1.016×.
- `clock/smooth`: 1.267×–1.568×; identical-body control 0.969×–1.071×.
- `clock/stable`: 1.243×–1.527×; identical-body control 0.944×–1.008×.
- `clock/unicode`: 1.088×–1.210×; identical-body control 0.870×–1.064×.
- `default/missing`: 0.924×–1.141×; identical-body control 0.972×–1.070×.
- `default/present`: 0.917×–1.068×; identical-body control 0.832×–0.925×.
- `numeric/empty`: 0.834×–0.852×; identical-body control 1.107×–1.177×.
- `numeric/invalid`: 0.958×–1.170×; identical-body control 0.935×–1.135×.
- `numeric/one`: 1.095×–1.198×; identical-body control 1.000×–1.139×.
- `numeric/other`: 1.157×–1.441×; identical-body control 0.945×–1.074×.
- `numeric/overflow`: 1.033×–1.221×; identical-body control 0.988×–1.071×.
- `numeric/plus`: 0.973×–1.158×; identical-body control 0.790×–1.007×.
- `numeric/three`: 0.998×–1.256×; identical-body control 0.977×–1.133×.
- `numeric/unicode`: 1.008×–1.169×; identical-body control 0.912×–1.159×.
- `numeric/zero`: 1.157×–1.311×; identical-body control 1.000×–1.111×.
- `numeric/zeros`: 1.270×–1.646×; identical-body control 1.046×–1.170×.

Raw paired medians below 0.970×:

- `numeric/empty`: 7.18 → 8.45 cycles; 0.850×, control 1.177×.

Differences close to their controls are inconclusive. Complete audio-loader paired medians range from 0.990× to 1.036× and remain close to their controls. Short, invalid, empty, fallback and missing cases are included above.

## Correctness and provenance

- Baseline, final release and package opt-level 0 with assertions/overflow checks each pass **256 unit + 13 integration tests**, covering all five config integration targets. Their ordinary test-name/status sets and traces match. Dependencies retain cached release profiles during the assertion gate.
- Exact differential inputs cover every Unicode scalar in three contexts, every alias capitalization (including Unicode trim wrappers), ASCII insertion and substitution at every word position, all ASCII two-byte inputs, all 2,097,152 ASCII three-byte numeric inputs, 0–1024 with signs and up to 1,024 leading zeroes, plus 32,768 seeded mixed UTF-8 strings.
- Full public audio-loader records compare against a frozen original loader with frozen clock/numeric helpers, both boolean/clock defaults, missing/minimal/populated maps and a stride-257 scalar/alias/random corpus. Visual-delay f32 bits are also checked, including negative zero. Source checks prove all other production functions and callers unchanged.
- A **12,114,922-byte trace** matches in all three gates, SHA-256 `39cbc0f10375912cabf3f0c59cb76a4a4321fa39fea7ef527b29b489c7343796`. It records raw parser input and exact enum/error, optional boolean and anchor text on stride-17 scalar/alias/random samples, plus complete audio records, both defaults and both INI shapes. Full scalar and ASCII-three-byte spaces are covered by ordinary tests.
- All six heap counters remain exactly zero for old and current fixtures. All **640 timing/allocation rows** match raw logs and exact input identities. RSS and stack usage are unmeasured.
- Clippy exits successfully with zero diagnostics in the changed production methods. The new ignored clock timing fixture has one reviewed `unused_must_use` compiler warning: `black_box` consumes the full Result, then its returned copy is deliberately discarded. Ordinary differential tests assert exact results separately. This warning and its raw log remain recorded; measured fixture source is unchanged. Production-only `cargo check --offline --locked --lib --release` passes with an opt-level-3 non-test artifact and assertions/overflow checks disabled. The audit verifies **63 package inputs**, source archives, profiles, compiler, lock, binaries/logs and every measurement field.

## Reproduction and rejected trials

`scripts/benchmark_audio_kernels.py` snapshots the actual package and dependency closure in the established six-member workspace, preserving cache mtimes. It uses C: temporary storage and offline locked resolution. Root Cargo files/dependency versions are unchanged; the scratch lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_AUDIO_KERNEL_ARCHIVE = 'C:\deadsync-audio-direct-20261105'
python scripts/benchmark_audio_kernels.py baseline --baseline e9236afd59a74cb4726b500660bfde3f8c0422c0 --runs 5 --reference baseline
python scripts/benchmark_audio_kernels.py final --runs 5 --reference baseline
python scripts/benchmark_audio_kernels.py debug --debug --reference baseline
```

Run stages sequentially; after the assertion gate restore the final normal release workspace manifest before Clippy/production checks. Each timing TSV contains 320 rows. Baseline restores all three original production modules plus the same private test registration; reference and fixture source is identical across stages.

The first width-only clock trial misplaced a fourteen-byte alias and failed correctness before timing. A corrected width dispatch showed little clock benefit, and its single-digit boolean shortcut slowed long zero prefixes. A fully packed clock improved named inputs but regressed short/invalid cases; the retained version specializes the common long names and preserves original comparisons elsewhere. A one-pass fourteen-byte lowercase buffer was slower and rejected; its initial mutable-pattern compile failure and corrected timings are retained. Five successful prototype cohorts plus the alias failure preserve source/binary/log fingerprints. Actual package results above determine retention.
