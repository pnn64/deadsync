# Strict series, shop and scoring choices without lowercase allocations — 2026-11-05

Baseline: `227c4b435914b0c1a053c6d13e74ef3eda0d0c47`. Three independently measured `FromStr` implementations in `deadsync-config/src/theme.rs`:

1. `SelectMusicSeriesSource` dispatches on trimmed byte length, then matches the original `packini`, `pack.ini`, `metadata`, `folder`, `folders`, `directory` and `filesystem` aliases with ASCII case-insensitive equality.
2. `SrpgShopFolder` uses length dispatch for the seven existing folder aliases, including `srpg10 unlocks`, `srpg10 - shops` and `by faction`. Internal spaces, periods and hyphens retain their exact strict grammar.
3. `TournamentScoringSystem` directly matches the trimmed two-byte EX or three-byte ITG spelling, including all ASCII case combinations.

All three remove the temporary lowercase String and its destruction. They keep borrowed `&str`/byte views and allocate nothing. Source changes are **+21/−10 lines** in the three bodies (net +11), excluding one private test registration. No helper abstraction, lookup table, buffer, unsafe, dependency, cache or public API is added. Config lookup order, defaults, canonical labels and other parser implementations are unchanged; the existing unit-test allocator and CPU clock are reused.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package unit-test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Identical-source controls expose context/timing bias. Builds and our benchmark processes run sequentially; unrelated user activity is not controlled.

All fixtures use 50,000 repetitions and return consumed enum results. Immutable strings are prepared outside timing. The counted-System allocator stays installed with counting disabled during CPU measurements, retaining its inactive TLS check. Lowercase String allocation/destruction is included for the frozen functions. Valid aliases, mixed case, Unicode whitespace, punctuation rejection and empty strings are measured for each implementation.

These are steady-state in-memory preference parsing microbenchmarks. The functions run during config loading. Startup, filesystem, frame rate and whole-application gains are not measured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `scoring/empty` | 10.87 | 10.00 | 1.108× | 1.071× | 0 → 0 | 0 → 0 |
| `scoring/ex` | 153.87 | 16.32 | 9.400× | 0.992× | 1 → 0 | 2 → 0 |
| `scoring/invalid` | 146.73 | 15.02 | 9.758× | 0.988× | 1 → 0 | 5 → 0 |
| `scoring/itg` | 155.36 | 15.85 | 9.802× | 0.972× | 1 → 0 | 3 → 0 |
| `scoring/space` | 158.33 | 33.22 | 4.606× | 0.988× | 1 → 0 | 2 → 0 |
| `series/directory` | 172.02 | 29.52 | 5.947× | 0.988× | 1 → 0 | 9 → 0 |
| `series/dotted` | 167.20 | 27.60 | 6.095× | 0.995× | 1 → 0 | 8 → 0 |
| `series/empty` | 17.32 | 10.12 | 1.674× | 1.000× | 0 → 0 | 0 → 0 |
| `series/filesystem` | 178.32 | 31.29 | 5.567× | 1.001× | 1 → 0 | 10 → 0 |
| `series/folder` | 180.90 | 23.89 | 7.651× | 1.016× | 1 → 0 | 6 → 0 |
| `series/folders` | 191.21 | 26.56 | 7.209× | 0.987× | 1 → 0 | 7 → 0 |
| `series/invalid` | 160.90 | 23.09 | 7.139× | 0.996× | 1 → 0 | 8 → 0 |
| `series/metadata` | 163.72 | 27.48 | 6.024× | 0.989× | 1 → 0 | 8 → 0 |
| `series/packini` | 192.90 | 25.95 | 7.284× | 1.000× | 1 → 0 | 7 → 0 |
| `series/space` | 172.98 | 43.09 | 4.015× | 0.964× | 1 → 0 | 8 → 0 |
| `shop/by-faction` | 175.65 | 29.55 | 5.867× | 0.993× | 1 → 0 | 10 → 0 |
| `shop/empty` | 19.47 | 11.03 | 1.737× | 1.080× | 0 → 0 | 0 → 0 |
| `shop/faction` | 188.17 | 25.08 | 7.465× | 0.990× | 1 → 0 | 7 → 0 |
| `shop/factions` | 160.10 | 27.58 | 5.717× | 0.994× | 1 → 0 | 8 → 0 |
| `shop/invalid` | 165.69 | 19.32 | 8.775× | 0.996× | 1 → 0 | 12 → 0 |
| `shop/legacy-shops` | 196.24 | 44.75 | 4.389× | 1.003× | 1 → 0 | 14 → 0 |
| `shop/legacy-unlocks` | 189.39 | 34.75 | 5.454× | 0.987× | 1 → 0 | 14 → 0 |
| `shop/shops` | 177.46 | 22.21 | 7.990× | 0.994× | 1 → 0 | 5 → 0 |
| `shop/space` | 222.25 | 60.46 | 3.676× | 1.000× | 1 → 0 | 14 → 0 |
| `shop/unlocks` | 181.67 | 25.04 | 7.278× | 1.002× | 1 → 0 | 7 → 0 |

Process throughput ranges (minimum–maximum):

- `scoring/empty`: 1.022×–1.270×; control 1.020×–1.082×.
- `scoring/ex`: 7.373×–10.213×; control 0.975×–1.008×.
- `scoring/invalid`: 8.560×–10.176×; control 0.970×–1.012×.
- `scoring/itg`: 8.239×–10.105×; control 0.940×–1.012×.
- `scoring/space`: 4.484×–5.091×; control 0.962×–0.998×.
- `series/directory`: 5.520×–5.993×; control 0.974×–1.018×.
- `series/dotted`: 5.604×–6.114×; control 0.964×–1.022×.
- `series/empty`: 1.662×–1.757×; control 0.994×–1.005×.
- `series/filesystem`: 5.272×–6.583×; control 0.975×–1.012×.
- `series/folder`: 7.345×–7.830×; control 0.919×–1.028×.
- `series/folders`: 6.897×–7.892×; control 0.929×–0.992×.
- `series/invalid`: 6.665×–7.442×; control 0.862×–1.007×.
- `series/metadata`: 5.727×–6.304×; control 0.983×–1.041×.
- `series/packini`: 6.346×–7.756×; control 0.959×–1.021×.
- `series/space`: 3.889×–4.155×; control 0.887×–1.024×.
- `shop/by-faction`: 5.566×–6.222×; control 0.973×–1.023×.
- `shop/empty`: 1.690×–1.811×; control 1.040×–1.104×.
- `shop/faction`: 6.804×–8.021×; control 0.968×–1.024×.
- `shop/factions`: 5.420×–5.831×; control 0.969×–1.029×.
- `shop/invalid`: 8.532×–9.060×; control 0.959×–0.997×.
- `shop/legacy-shops`: 4.292×–4.511×; control 0.979×–1.010×.
- `shop/legacy-unlocks`: 5.021×–5.617×; control 0.973×–1.007×.
- `shop/shops`: 7.729×–8.109×; control 0.985×–1.024×.
- `shop/space`: 3.625×–3.731×; control 0.982×–1.023×.
- `shop/unlocks`: 6.926×–7.512×; control 0.988×–1.190×.

No raw paired median is below 0.970× in this cohort. This does not rule out costs on other inputs.

Small empty-input differences near the controls are inconclusive. Large nonempty gains combine allocation removal with direct matching; the experiment does not attribute a precise fraction to either mechanism.

## Exactness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **251 unit + 13 integration tests**, covering all five integration targets. Ordinary test-name/status sets are identical. Existing load/default/legacy-key and enum serialization tests remain included. Dependencies retain cached release profiles in the assertion-enabled gate. New timing and trace tests run explicitly outside normal gates.
- All ASCII case combinations of all sixteen aliases compare against frozen pre-change bodies, with Unicode whitespace wrapping and every ASCII insertion at every alias boundary. Every Unicode scalar is checked alone, before `pack.ini`, and after `srpg10 - shops`. Another 16,384 deterministic mixed Unicode/punctuation strings exercise rejection, trimming and byte-length dispatch. No punctuation normalization is introduced.
- The three outputs are compared as actual enum values, not just success/failure. Config defaults and canonical output labels are unchanged by exact source-scope verification. The scoring patterns match exact slice lengths and ASCII bytes; the other two parsers compare original trimmed text without punctuation normalization.
- A **7,140,924-byte input/result trace** matches across all three profiles, SHA-256 `e4905bc9cbb292028825ceb99ebc94ff139c0d64fb50fad3cd8632f442d574a9`. It records every alias/case/insertion/random cohort plus a stride-17 scalar sample, input byte lengths/content and all three exact enum outcomes. Ordinary differential tests independently check every scalar.
  - `choice` part: 7,140,916 bytes, SHA-256 `001e3402dd7ecee3430901567ede8dfe583dab84354c45fdb9a160d6baac800d`.
- Every one of the **500 raw rows** matches its process log. For every nonempty measured input, allocation/free counts fall **1 → 0** with all requested/peak bytes eliminated. Empty inputs already allocated zero. All current variants have zero in all six counters; all reallocations remain zero, and every frozen allocation/byte count balances its free. These counters exclude allocator metadata, stack use and total RSS.
- Scoped Clippy has zero changed-scope diagnostics, and production-only `cargo check --offline --locked --lib --release` passes. Its Cargo artifact explicitly retains opt-level 3 with assertions/overflow checks disabled. Audits verify all **53 package inputs**, exact three-body source scope and signatures, frozen Git implementations, source/dependency archives, compiler/lock/profiles, tests, binary/log identities, trace and every timing/allocation field. Other out-of-scope messages remain recorded.

## Reproduction

`scripts/benchmark_config_choices.py` builds the actual config package and all five integration targets in the existing six-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_CONFIG_CHOICES_ARCHIVE = 'C:\deadsync-config-choices-20261105'
python scripts/benchmark_config_choices.py baseline --baseline 227c4b435914b0c1a053c6d13e74ef3eda0d0c47 --runs 5 --reference baseline
python scripts/benchmark_config_choices.py final --runs 5 --reference baseline
python scripts/benchmark_config_choices.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. Baseline restores the original theme module plus only the new test registration; frozen references/fixtures are common to all gates. Each TSV retains 250 rows; checks JSON verifies 500 total rows and all process/control ranges. Separate standalone source/binary/log cohorts are archived and hash-verified.

Standalone prototype measurements passed the same differential cases before implementation; the final package data above is authoritative.
