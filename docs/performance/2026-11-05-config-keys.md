# Shared byte parsers — 2026-11-05

Baseline: `3816123080ebd42bb2bbbd16f03744488090e860`. Three independently measured safe Rust changes in `deadsync-config`:

1. `theme.rs::NormalizedAsciiKey::parse` maps each byte directly to lowercase ASCII alphanumeric or a discarded zero marker. A local `usize` cursor advances through the existing 32-byte buffer and writes its `u8` length once. Removing `trim()` is exact because all whitespace bytes are already discarded. The 32/33 retained-byte boundary and unused zeroed bytes remain unchanged.
2. `streaming.rs::TwitchChannel::parse` trims once, removes at most one leading `#`, then validates and lowercases in a single copy into its existing 25-byte buffer. It retains the byte-length limit, underscores, empty record, and rejection of Unicode/internal whitespace. In particular, `# abc` stays invalid: there is no second trim after `#`.
3. `null_or_die.rs::parse_null_or_die_kernel_target` replaces separate lazy classification and lowercase adapters with one lazy mapped-byte filter. Existing first-byte dispatch and exact suffix iteration for Digest/Accumulator remain, including early mismatch and rejection of extra retained characters. Kernel-type parsing is untouched.

Source changes are **+56/−12 lines** in the three bodies (net +44), excluding one private test registration. Each function has a local compile-time 256-entry byte table: **768 bytes of logical lookup data**. Identical tables may be merged by the compiler; physical binary-size change is unmeasured. No new production helper, heap allocation, buffer, cache, unsafe, dependency or public API is introduced. Structs, capacities, defaults, labels, serialization and caller bodies are unchanged. The existing test allocator and CPU clock are reused.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Identical-source baseline controls expose context/timing bias. Builds and our benchmarks run sequentially; unrelated user activity is not controlled.

All 28 fixtures use 20,000 repetitions and consume complete return values through black boxes. Strings are prepared outside timing. The counted-System allocator stays installed with counting disabled during CPU measurements, retaining its inactive TLS check. The shared-key fixture consumes its complete buffer/length tuple. Channel fixtures consume their actual complete fixed records separately without converting to a shared DTO. Five caller fixtures also measure unchanged ThemeFlag and MachinePreferredPlayStyle parsing with the old or new shared normalizer.

These are steady-state in-memory parsing microbenchmarks. Config loading, Twitch networking, streaming throughput, startup, frame rate and whole-application gains are not measured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Requested bytes old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `caller/style-invalid` | 97.89 | 51.54 | 1.893× | 1.024× | 0 → 0 | 0 → 0 |
| `caller/style-plain` | 98.36 | 61.41 | 1.663× | 1.016× | 0 → 0 | 0 → 0 |
| `caller/style-punct` | 101.15 | 52.93 | 1.884× | 0.981× | 0 → 0 | 0 → 0 |
| `caller/theme-plain` | 111.32 | 57.96 | 1.935× | 0.944× | 0 → 0 | 0 → 0 |
| `caller/theme-punct` | 136.22 | 60.58 | 2.227× | 0.976× | 0 → 0 | 0 → 0 |
| `channel/empty` | 24.92 | 21.40 | 1.167× | 1.021× | 0 → 0 | 0 → 0 |
| `channel/hash` | 122.25 | 49.09 | 2.478× | 1.006× | 0 → 0 | 0 → 0 |
| `channel/invalid` | 42.12 | 21.60 | 1.995× | 1.030× | 0 → 0 | 0 → 0 |
| `channel/len25` | 143.41 | 58.60 | 2.474× | 1.020× | 0 → 0 | 0 → 0 |
| `channel/len26` | 31.62 | 15.80 | 2.044× | 0.966× | 0 → 0 | 0 → 0 |
| `channel/lower` | 81.02 | 39.87 | 2.032× | 0.997× | 0 → 0 | 0 → 0 |
| `channel/unicode` | 34.65 | 17.40 | 1.869× | 1.024× | 0 → 0 | 0 → 0 |
| `channel/upper` | 77.24 | 40.28 | 1.918× | 1.047× | 0 → 0 | 0 → 0 |
| `key/empty` | 14.44 | 7.98 | 1.751× | 0.999× | 0 → 0 | 0 → 0 |
| `key/ignored` | 1199.77 | 356.83 | 3.303× | 1.007× | 0 → 0 | 0 → 0 |
| `key/len32` | 240.62 | 69.15 | 3.551× | 1.010× | 0 → 0 | 0 → 0 |
| `key/len33` | 241.09 | 59.68 | 4.122× | 1.037× | 0 → 0 | 0 → 0 |
| `key/punct` | 116.16 | 40.09 | 2.895× | 0.982× | 0 → 0 | 0 → 0 |
| `key/short` | 92.73 | 36.26 | 2.518× | 1.000× | 0 → 0 | 0 → 0 |
| `key/space` | 170.22 | 79.31 | 2.199× | 1.021× | 0 → 0 | 0 → 0 |
| `key/unicode` | 129.97 | 41.07 | 3.114× | 0.979× | 0 → 0 | 0 → 0 |
| `target/accumulator` | 70.93 | 26.93 | 2.619× | 1.014× | 0 → 0 | 0 → 0 |
| `target/digest` | 42.03 | 18.77 | 2.229× | 1.021× | 0 → 0 | 0 → 0 |
| `target/empty` | 3.29 | 2.93 | 1.116× | 0.998× | 0 → 0 | 0 → 0 |
| `target/invalid` | 9.24 | 4.79 | 1.959× | 1.000× | 0 → 0 | 0 → 0 |
| `target/late` | 66.73 | 25.76 | 2.596× | 1.000× | 0 → 0 | 0 → 0 |
| `target/punct` | 70.38 | 27.49 | 2.595× | 0.996× | 0 → 0 | 0 → 0 |
| `target/tail` | 985.69 | 328.42 | 3.009× | 1.007× | 0 → 0 | 0 → 0 |

Process throughput ranges (minimum–maximum):

- `caller/style-invalid`: 1.829×–2.075×; control 1.000×–1.058×.
- `caller/style-plain`: 1.536×–1.736×; control 0.947×–1.040×.
- `caller/style-punct`: 1.780×–2.202×; control 0.938×–1.022×.
- `caller/theme-plain`: 1.618×–2.087×; control 0.880×–1.031×.
- `caller/theme-punct`: 2.072×–2.460×; control 0.931×–1.009×.
- `channel/empty`: 1.123×–1.217×; control 0.932×–1.031×.
- `channel/hash`: 2.196×–2.649×; control 0.995×–1.043×.
- `channel/invalid`: 1.890×–2.046×; control 1.000×–1.146×.
- `channel/len25`: 2.172×–2.610×; control 0.988×–1.045×.
- `channel/len26`: 2.000×–2.105×; control 0.963×–0.981×.
- `channel/lower`: 2.000×–2.096×; control 0.973×–1.076×.
- `channel/unicode`: 1.613×–2.105×; control 0.949×–1.064×.
- `channel/upper`: 1.777×–2.093×; control 0.950×–1.122×.
- `key/empty`: 1.534×–1.810×; control 0.932×–1.090×.
- `key/ignored`: 3.277×–3.395×; control 0.991×–1.047×.
- `key/len32`: 3.072×–3.688×; control 0.963×–1.049×.
- `key/len33`: 4.026×–4.201×; control 1.005×–1.078×.
- `key/punct`: 2.800×–2.980×; control 0.974×–1.005×.
- `key/short`: 2.366×–2.729×; control 0.945×–1.058×.
- `key/space`: 2.083×–2.234×; control 1.010×–1.060×.
- `key/unicode`: 3.032×–3.301×; control 0.938×–0.996×.
- `target/accumulator`: 2.249×–2.634×; control 0.999×–1.036×.
- `target/digest`: 2.011×–2.278×; control 0.985×–1.069×.
- `target/empty`: 1.114×–1.123×; control 0.996×–0.998×.
- `target/invalid`: 1.919×–2.263×; control 0.852×–1.022×.
- `target/late`: 2.218×–2.818×; control 0.962×–1.040×.
- `target/punct`: 2.462×–2.749×; control 0.964×–1.045×.
- `target/tail`: 2.962×–3.091×; control 0.981×–1.027×.

No raw paired median is below 0.970× in this cohort. This does not rule out costs on other inputs.

Differences close to identical-source controls are inconclusive. The package measurements above determine retention; standalone prototype numbers are not substituted for them.

## Exactness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **253 unit + 13 integration tests**, covering all five integration targets. Ordinary test-name/status sets are identical. Dependencies retain cached release profiles in the assertion-enabled gate. Timing and trace tests run explicitly outside normal gates.
- Differential tests check every Unicode scalar alone, before `Abc9` and after `accumulator`; every ASCII insertion at every boundary of five representative names; lengths 0/1/24/25/26/31/32/33/64/4096 with retained/discarded ASCII, multibyte Unicode, `#` prefixes and Unicode whitespace; targeted NUL/BOM/punctuation/hash/suffix cases; and 16,384 seeded mixed UTF-8 strings. All ASCII case combinations of simplylove, twoplayer, channel_09, digest and accumulator also run with Unicode whitespace wrapping.
- Equality covers the full normalized-key buffer/length, channel presence/string/empty state and its derived Debug representation of every buffer byte and length, and the actual KernelTarget enum. ThemeFlag and MachinePreferredPlayStyle callers compare actual enum/error outcomes against frozen pre-change callers and normalizer. Frozen implementations and their structs/methods are checked against Git baseline source.
- A **13,057,776-byte input/result trace** matches across all three profiles, SHA-256 `65a78380b09080133c01d14140627bca3714e35ed46d31dc5836392b4a8f1fd9`. It contains input bytes/lengths, complete key buffer/length, complete channel Debug record and target outcome for the insertion/boundary/targeted/random cohorts and stride-17 scalar sample. Caller enum equality and exhaustive capitalization run in ordinary tests, not in this trace. Ordinary tests independently check every scalar.
  - `key` part: 13,057,768 bytes, SHA-256 `ed46906bef0d4a120ddda84b05420751d33e6f1ac85e019a55f5ee8189c6d137`.
- All **560 raw rows** match their process logs, including every timing field and all six allocation counters. Every measured variant has **zero allocations, reallocations, frees, allocated bytes, freed bytes and peak requested bytes**. Allocation behavior is unchanged; stack use, allocator metadata, lookup-table storage and total RSS are not represented by these counters.
- Scoped Clippy has zero changed-scope diagnostics, and production-only `cargo check --offline --locked --lib --release` passes. Its Cargo artifact retains opt-level 3 with assertions/overflow checks disabled. Audits verify all **57 package inputs**, exact three-body scope/signatures, frozen Git references, source/dependency archives, compiler/lock/profiles, test names, binaries/logs, trace and every timing/allocation field. Out-of-scope diagnostics remain recorded.

## Reproduction

`scripts/benchmark_config_keys.py` builds the actual config package and all five integration targets in the existing six-member scratch workspace. It snapshots the Git dependency closure and exact checkout overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_CONFIG_KEYS_ARCHIVE = 'C:\deadsync-key-parsers-20261105'
python scripts/benchmark_config_keys.py baseline --baseline 3816123080ebd42bb2bbbd16f03744488090e860 --runs 5 --reference baseline
python scripts/benchmark_config_keys.py final --runs 5 --reference baseline
python scripts/benchmark_config_keys.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. Baseline restores the original three production modules plus only the new theme test registration. Frozen references/fixtures are common to all gates. Each TSV retains 280 rows; checks JSON verifies 560 total rows and all process/control ranges.

Two standalone source/binary/log cohorts are archived and hash-verified. The first table prototype improved ordinary key cases but slowed the 32/33-byte boundary fixtures; that key implementation was rejected before production edits. Moving its loop cursor from the record's u8 field to a local usize and setting the record length once removed that regression in three fresh prototype processes. Channel and target prototype bodies were retained. Both cohorts passed exact differential checks; the final package data above is authoritative.
