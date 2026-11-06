# Borrowed pattern, play and game mode parsing — 2026-11-05

Baseline: `e105d114a735202b5e1deaa884bcc9235fd2f614`. Three independently measured `FromStr` implementations in `deadsync-config/src/theme.rs`:

1. `SelectMusicPatternInfoMode` dispatches on trimmed byte length and compares the existing `tech`, `stamina` and `auto` names with ASCII case-insensitive equality.
2. `MachinePreferredPlayMode` directly recognizes `regular` and `marathon` from borrowed trimmed text, with the same exact enum outcomes.
3. `GameFlag` directly recognizes `dance` and `pump`, preserving strict punctuation rejection and Unicode whitespace trimming.

Each removes the temporary lowercase String and its destruction. Source changes are **+13/−10 lines** in the three bodies (net +3), excluding one private test registration. No helper abstraction, lookup table, buffer, unsafe, dependency, cache or public API is added. Config lookup order, defaults, canonical labels, serialization and other parsers are unchanged. The existing unit-test allocator and CPU clock are reused.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package unit-test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Identical-source controls expose context/timing bias. Builds and our benchmark processes run sequentially; unrelated user activity is not controlled.

All fixtures use 50,000 repetitions and return consumed enum results. Immutable strings are prepared outside timing. The counted-System allocator stays installed with counting disabled during CPU measurements, retaining its inactive TLS check. Lowercase String allocation/destruction is included for the frozen functions. Valid aliases, mixed case, Unicode whitespace, punctuation rejection and empty strings are measured for each implementation.

These are steady-state in-memory preference parsing microbenchmarks. The functions run during config loading. Startup, filesystem, frame rate and whole-application gains are not measured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `game/dance` | 132.63 | 18.63 | 6.935× | 0.982× | 1 → 0 | 5 → 0 |
| `game/empty` | 7.16 | 6.58 | 1.090× | 1.142× | 0 → 0 | 0 → 0 |
| `game/invalid` | 127.28 | 11.92 | 10.643× | 0.990× | 1 → 0 | 6 → 0 |
| `game/pump` | 130.42 | 17.84 | 7.206× | 0.992× | 1 → 0 | 4 → 0 |
| `game/space` | 148.07 | 32.73 | 4.594× | 1.014× | 1 → 0 | 5 → 0 |
| `pattern/auto` | 128.00 | 17.28 | 7.408× | 1.001× | 1 → 0 | 4 → 0 |
| `pattern/empty` | 9.20 | 11.92 | 0.812× | 1.011× | 0 → 0 | 0 → 0 |
| `pattern/invalid` | 125.11 | 14.90 | 8.153× | 1.002× | 1 → 0 | 8 → 0 |
| `pattern/space` | 139.24 | 29.76 | 4.623× | 0.982× | 1 → 0 | 4 → 0 |
| `pattern/stamina` | 133.90 | 20.84 | 6.425× | 1.021× | 1 → 0 | 7 → 0 |
| `pattern/tech` | 126.49 | 17.95 | 6.977× | 0.985× | 1 → 0 | 4 → 0 |
| `play/empty` | 8.56 | 7.24 | 1.167× | 0.958× | 0 → 0 | 0 → 0 |
| `play/invalid` | 126.53 | 11.98 | 10.663× | 0.984× | 1 → 0 | 9 → 0 |
| `play/marathon` | 125.14 | 22.06 | 5.645× | 1.017× | 1 → 0 | 8 → 0 |
| `play/regular` | 142.87 | 20.38 | 6.658× | 0.996× | 1 → 0 | 7 → 0 |
| `play/space` | 160.67 | 34.68 | 4.579× | 1.025× | 1 → 0 | 7 → 0 |

Process throughput ranges (minimum–maximum):

- `game/dance`: 6.577×–7.192×; control 0.850×–1.064×.
- `game/empty`: 1.085×–1.091×; control 1.066×–1.152×.
- `game/invalid`: 10.128×–11.286×; control 0.903×–1.082×.
- `game/pump`: 6.424×–7.854×; control 0.978×–1.008×.
- `game/space`: 4.517×–4.636×; control 0.966×–1.049×.
- `pattern/auto`: 7.339×–7.552×; control 0.968×–1.005×.
- `pattern/empty`: 0.772×–0.875×; control 0.947×–1.121×.
- `pattern/invalid`: 7.982×–8.515×; control 0.980×–1.051×.
- `pattern/space`: 4.387×–4.731×; control 0.927×–1.080×.
- `pattern/stamina`: 6.330×–6.645×; control 0.988×–1.091×.
- `pattern/tech`: 6.902×–7.078×; control 0.949×–1.010×.
- `play/empty`: 1.034×–1.196×; control 0.928×–1.131×.
- `play/invalid`: 10.042×–10.920×; control 0.941×–1.046×.
- `play/marathon`: 5.518×–5.964×; control 0.980×–1.023×.
- `play/regular`: 6.420×–7.056×; control 0.983×–1.013×.
- `play/space`: 4.521×–4.641×; control 0.977×–1.079×.

Observed raw paired medians below 0.970×:

- `pattern/empty`: 9.20 → 11.92 cycles; 0.812×, control 1.011×.

Small empty-input differences near the controls are inconclusive. Large nonempty gains combine allocation removal with direct matching; the experiment does not attribute a precise fraction to either mechanism.

## Exactness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **252 unit + 13 integration tests**, covering all five integration targets. Ordinary test-name/status sets are identical. Existing load/default/legacy-key and enum serialization tests remain included. Dependencies retain cached release profiles in the assertion-enabled gate. New timing and trace tests run explicitly outside normal gates.
- All ASCII case combinations of all seven names compare against frozen pre-change bodies, with Unicode whitespace wrapping and every ASCII insertion at every alias boundary. Every Unicode scalar is checked alone, before `tech`, and after `dance`. Another 16,384 deterministic mixed Unicode/punctuation strings exercise rejection, trimming and byte-length dispatch. No punctuation normalization is introduced.
- The three outputs are compared as actual enum values, not just success/failure. Config defaults and canonical output labels are unchanged by exact source-scope verification. All three compare original trimmed text with exact byte lengths and ASCII case-insensitive equality.
- A **4,063,148-byte input/result trace** matches across all three profiles, SHA-256 `8af4db9e06726ae145412ee673a94ce1cb95de23c6b1a1564ff88f0287d415d0`. It records every alias/case/insertion/random cohort plus a stride-17 scalar sample, input byte lengths/content and all three exact enum outcomes. Ordinary differential tests independently check every scalar.
  - `mode` part: 4,063,140 bytes, SHA-256 `d3369351a75ee11af0ec68935568d1b8f7bd3903623f33ea144bbb60c102a630`.
- Every one of the **320 raw rows** matches its process log. For every nonempty measured input, allocation/free counts fall **1 → 0** with all requested/peak bytes eliminated. Empty inputs already allocated zero. All current variants have zero in all six counters; all reallocations remain zero, and every frozen allocation/byte count balances its free. These counters exclude allocator metadata, stack use and total RSS.
- Scoped Clippy has zero changed-scope diagnostics, and production-only `cargo check --offline --locked --lib --release` passes. Its Cargo artifact explicitly retains opt-level 3 with assertions/overflow checks disabled. Audits verify all **55 package inputs**, exact three-body source scope and signatures, frozen Git implementations, source/dependency archives, compiler/lock/profiles, tests, binary/log identities, trace and every timing/allocation field. Other out-of-scope messages remain recorded.

## Reproduction

`scripts/benchmark_config_modes.py` builds the actual config package and all five integration targets in the existing six-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_CONFIG_MODES_ARCHIVE = 'C:\deadsync-config-modes-20261105'
python scripts/benchmark_config_modes.py baseline --baseline e105d114a735202b5e1deaa884bcc9235fd2f614 --runs 5 --reference baseline
python scripts/benchmark_config_modes.py final --runs 5 --reference baseline
python scripts/benchmark_config_modes.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. Baseline restores the original theme module plus only the new test registration; frozen references/fixtures are common to all gates. Each TSV retains 160 rows; checks JSON verifies 320 total rows and all process/control ranges. Separate standalone source/binary/log cohorts are archived and hash-verified.

Standalone prototype measurements passed the same differential cases before implementation; the final package data above is authoritative.
