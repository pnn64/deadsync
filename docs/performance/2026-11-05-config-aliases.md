# Strict config aliases without lowercase allocations — 2026-11-05

Baseline: `8a36344a9287a2ff5588314e577648d3d63e771b`. Three independently measured `FromStr` implementations in `deadsync-config/src/theme.rs`:

1. `BreakdownStyle` matches the two trimmed ASCII bytes directly, preserving every case combination of `sl` and `sn` and rejecting every other shape.
2. `DefaultSyncOffset` dispatches on trimmed byte length and compares the existing `itg`, `off`, `null` and `none` aliases with ASCII case-insensitive equality. It preserves the supplied enum outcomes and the strict punctuation grammar.
3. `VersionOverlaySide` handles the one-byte L/R aliases with a single ASCII fold, then dispatches the full words and `lhs`/`rhs` aliases by length. It retains all Unicode whitespace trimming and rejection behavior. The single-byte indexing is guarded by the exact length arm.

All three remove the temporary lowercase String. They retain borrowed `&str`/byte views and allocate nothing. Other parsing, config lookup order, defaults, serialization labels and public types are unchanged. Source changes are **+19/−9 lines** in the three bodies (net +10), excluding one private test registration. No helper abstraction, lookup table, buffer, unsafe, dependency, cache or public API is added. The existing unit-test allocator and CPU clock are reused without additional visibility changes.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package unit-test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Identical-source controls expose context/timing bias. Builds and our benchmark processes run sequentially; unrelated user activity is not controlled.

All fixtures use 50,000 repetitions and return consumed enum results. Immutable strings are prepared outside timing. The counted-System allocator stays installed with counting disabled during CPU measurements, retaining its inactive TLS check. Lowercase String allocation/destruction is included for the frozen functions. Valid aliases, mixed case, Unicode whitespace, punctuation rejection and empty strings are measured for each implementation.

These are steady-state in-memory preference parsing microbenchmarks. The functions run during config loading. Startup, filesystem, frame rate and whole-application gains are not measured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `breakdown/empty` | 8.83 | 7.57 | 1.166× | 1.143× | 0 → 0 | 0 → 0 |
| `breakdown/invalid` | 125.23 | 12.60 | 9.875× | 1.005× | 1 → 0 | 3 → 0 |
| `breakdown/sl` | 132.82 | 13.97 | 9.308× | 1.003× | 1 → 0 | 2 → 0 |
| `breakdown/sn` | 129.61 | 14.49 | 8.945× | 1.011× | 1 → 0 | 2 → 0 |
| `breakdown/space` | 136.98 | 27.97 | 4.892× | 0.993× | 1 → 0 | 2 → 0 |
| `side/empty` | 18.89 | 9.46 | 1.997× | 0.992× | 0 → 0 | 0 → 0 |
| `side/invalid` | 145.93 | 17.47 | 8.535× | 0.993× | 1 → 0 | 5 → 0 |
| `side/l` | 144.47 | 13.31 | 10.752× | 1.001× | 1 → 0 | 1 → 0 |
| `side/left` | 144.80 | 18.38 | 7.775× | 0.999× | 1 → 0 | 4 → 0 |
| `side/lhs` | 141.51 | 18.26 | 8.076× | 0.995× | 1 → 0 | 3 → 0 |
| `side/r` | 136.28 | 13.32 | 10.231× | 0.993× | 1 → 0 | 1 → 0 |
| `side/rhs` | 144.15 | 17.95 | 7.991× | 0.987× | 1 → 0 | 3 → 0 |
| `side/right` | 156.34 | 21.39 | 7.309× | 0.993× | 1 → 0 | 5 → 0 |
| `side/space` | 150.86 | 35.70 | 4.219× | 0.999× | 1 → 0 | 4 → 0 |
| `sync/empty` | 8.20 | 8.20 | 1.000× | 1.143× | 0 → 0 | 0 → 0 |
| `sync/invalid` | 128.72 | 12.60 | 10.216× | 1.001× | 1 → 0 | 5 → 0 |
| `sync/itg` | 132.63 | 16.38 | 8.092× | 0.963× | 1 → 0 | 3 → 0 |
| `sync/none` | 135.90 | 17.63 | 7.483× | 0.990× | 1 → 0 | 4 → 0 |
| `sync/null` | 131.60 | 17.67 | 7.378× | 1.009× | 1 → 0 | 4 → 0 |
| `sync/off` | 130.27 | 16.41 | 7.913× | 0.973× | 1 → 0 | 3 → 0 |

Process throughput ranges (minimum–maximum):

- `breakdown/empty`: 1.097×–1.166×; control 1.142×–1.149×.
- `breakdown/invalid`: 9.727×–10.688×; control 0.997×–1.017×.
- `breakdown/sl`: 9.139×–10.075×; control 0.990×–1.008×.
- `breakdown/sn`: 8.815×–9.448×; control 0.994×–1.013×.
- `breakdown/space`: 4.804×–5.044×; control 0.980×–1.009×.
- `side/empty`: 1.966×–2.053×; control 0.953×–0.997×.
- `side/invalid`: 7.957×–8.964×; control 0.965×–1.012×.
- `side/l`: 10.208×–11.213×; control 0.980×–1.032×.
- `side/left`: 7.688×–7.972×; control 0.973×–1.002×.
- `side/lhs`: 7.707×–8.324×; control 0.931×–1.013×.
- `side/r`: 10.084×–10.651×; control 0.939×–1.003×.
- `side/rhs`: 7.785×–8.293×; control 0.961×–1.013×.
- `side/right`: 6.376×–7.369×; control 0.965×–1.023×.
- `side/space`: 4.094×–4.280×; control 0.994×–1.015×.
- `sync/empty`: 0.966×–1.063×; control 1.063×–1.143×.
- `sync/invalid`: 10.122×–10.619×; control 0.997×–1.006×.
- `sync/itg`: 8.043×–8.212×; control 0.958×–0.986×.
- `sync/none`: 7.426×–8.022×; control 0.981×–1.040×.
- `sync/null`: 7.176×–7.540×; control 0.987×–1.075×.
- `sync/off`: 7.846×–7.967×; control 0.957×–0.989×.

No raw paired median is below 0.970× in this cohort. This does not rule out costs on other inputs.

Small empty-input differences near the controls are inconclusive. Large nonempty gains combine allocation removal with direct matching; the experiment does not attribute a precise fraction to either mechanism.

## Exactness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **250 unit + 13 integration tests**, covering all five integration targets. Ordinary test-name/status sets are identical. Existing load/default/legacy-key and enum serialization tests remain included. Dependencies retain cached release profiles in the assertion-enabled gate. New timing and trace tests run explicitly outside normal gates.
- All ASCII case combinations of all twelve aliases compare against frozen pre-change bodies, with Unicode whitespace wrapping and every ASCII insertion at every alias boundary. Every Unicode scalar is checked alone, before `sl`, and after `right`. Another 16,384 deterministic mixed Unicode/punctuation strings exercise rejection, trimming and byte-length dispatch. No punctuation normalization is introduced.
- The three outputs are compared as actual enum values, not just success/failure. Config defaults and canonical output labels are unchanged by exact source-scope verification. The direct byte fold can reach only the L/R arm on a one-byte string; all non-ASCII single Unicode scalars have multi-byte UTF-8 encodings.
- A **3,908,240-byte input/result trace** matches across all three profiles, SHA-256 `3d7c188ee39dd283e55a186ec6918adc2476bd5ce845091c3b701620f3e42341`. It records every alias/case/insertion/random cohort plus a stride-17 scalar sample, input byte lengths/content and all three exact enum outcomes. Ordinary differential tests independently check every scalar.
  - `alias` part: 3,908,232 bytes, SHA-256 `bcec9ddee7580c7f2d62a4ac2c940aa93e95a114b6e9032be6823c52a71c7152`.
- Every one of the **400 raw rows** matches its process log. For every nonempty measured input, allocation/free counts fall **1 → 0** with all requested/peak bytes eliminated. Empty inputs already allocated zero. All current variants have zero in all six counters; all reallocations remain zero, and every frozen allocation/byte count balances its free. These counters exclude allocator metadata, stack use and total RSS.
- Scoped Clippy has zero changed-scope diagnostics, and production-only `cargo check --offline --locked --lib --release` passes. Its Cargo artifact explicitly retains opt-level 3 with assertions/overflow checks disabled. Audits verify all **51 package inputs**, exact three-body source scope and signatures, frozen Git implementations, source/dependency archives, compiler/lock/profiles, tests, binary/log identities, trace and every timing/allocation field. Other out-of-scope messages remain recorded.

## Reproduction and rejected color experiments

`scripts/benchmark_config_aliases.py` builds the actual config package and all five integration targets in the existing six-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_CONFIG_ALIASES_ARCHIVE = 'C:\deadsync-config-aliases-20261105'
python scripts/benchmark_config_aliases.py baseline --baseline 8a36344a9287a2ff5588314e577648d3d63e771b --runs 5 --reference baseline
python scripts/benchmark_config_aliases.py final --runs 5 --reference baseline
python scripts/benchmark_config_aliases.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. Baseline restores the original theme module plus only the new test registration; frozen references/fixtures are common to all gates. Each TSV retains 200 rows; checks JSON verifies 400 total rows and all process/control ranges. Separate standalone source/binary/log cohorts are archived and hash-verified.

Earlier color-conversion candidates passed correctness but failed the performance filter: the parser lookups stayed near parity, while fixed-buffer color formatting slowed to roughly 0.87×–0.91× in one process. They were fully withdrawn before any repository source changes. No color lookup tables or formatter changes are retained. Final package measurements above are the evidence for this batch.
