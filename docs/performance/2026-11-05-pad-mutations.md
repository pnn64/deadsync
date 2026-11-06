# Pad-config updates and migrated names — 2026-11-05

Baseline: `3fed54203393396b976e539ece4453cd06a60026`. Three independently measured safe Rust changes in `deadsync-profile/src/pad_config.rs`:

1. `set_default_config` keeps the position of the first matching name. Every earlier record is already known not to match, so its serials can be retained without comparing that name again. The remaining records use the original comparison/retain/push logic. Missing names still return false without mutation; duplicate matching names all receive the serial, retaining exact list order and capacities.
2. `rename_config` retains the first matching index for the final assignment and checks new-name equality first in its conflict pass. This removes the second search for the old name and avoids old-name comparisons when the new name does not match. It still trims only the new name, rejects conflicts, permits duplicate old names and changes only the first old record.
3. `migrated_config_name` appends owner, the fixed separator and name directly to a String. It removes general formatting machinery while preserving owner trimming, the empty-owner branch, output bytes and observed capacity/allocation requests on the measured toolchain.

The three bodies add **22 lines and remove 13** (net +9), excluding the private test registration. No new production helper, lookup table, buffer, unsafe, dependency, cache or public API is added. Struct fields, unrelated callers, serialization, defaults and collection order are unchanged. Existing test allocation counters and CPU-clock support are reused.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test executables use opt-level 3, fat LTO and static CRT. Five fresh processes execute twelve alternating frozen/current rounds pinned to CPU 2, discard the first and retain the median of eleven. `QueryThreadCycleTime` supplies thread CPU cycles. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Identical-source baseline controls expose context/timing bias. Builds and our benchmarks run sequentially; unrelated user activity is not controlled.

Fixtures and independent mutable lists are prepared outside timing. Rename fixtures perform **two calls per operation**, old→new→old, returning the list to its original name; their cycles represent that round trip. Successful rename/default fixtures run 5,000 repetitions with 1/4/16/64 records and a first/last matching name. At one record, first and last are the same input, measured in separate positions. Missing/empty-list fixtures use 10,000 repetitions. Default timings measure repeated updates, so nonmatching serial removals happen before the retained timing rounds. Name fixtures perform String construction, black-box consumption and destruction, using 20,000 repetitions.

Allocation counters measure **one invocation of the prepared fixture** before timing; default fixtures can release pre-existing serial strings on that invocation. Those freed allocations were created outside counting, so allocation/free totals need not balance. Their allocation counts do not describe a fresh default list on every timed repetition. Rename round trips include two replacements. Counting is disabled during CPU timing, retaining the installed counted-System allocator's inactive TLS check. Complete return values are consumed.

These are in-memory pad-config operation microbenchmarks. File I/O, migration directory traversal, UI latency and whole-application gains are not measured.

| Workload | Frozen cycles | Current cycles | Paired throughput | Identical-source control | Allocations old → new | Reallocations old → new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `default/1-first` | 260.50 | 286.32 | 0.923× | 0.958× | 1 → 1 | 0 → 0 |
| `default/1-last` | 286.89 | 291.58 | 0.943× | 0.913× | 1 → 1 | 0 → 0 |
| `default/16-first` | 975.19 | 995.87 | 0.987× | 0.986× | 1 → 1 | 0 → 0 |
| `default/16-last` | 1392.11 | 937.22 | 1.482× | 0.982× | 1 → 1 | 0 → 0 |
| `default/4-first` | 401.64 | 395.89 | 1.015× | 1.014× | 1 → 1 | 0 → 0 |
| `default/4-last` | 486.24 | 397.82 | 1.208× | 1.021× | 1 → 1 | 0 → 0 |
| `default/64-first` | 3089.68 | 3232.14 | 0.973× | 0.972× | 1 → 1 | 0 → 0 |
| `default/64-last` | 4984.27 | 2956.53 | 1.684× | 1.003× | 1 → 1 | 0 → 0 |
| `default/empty` | 27.74 | 27.61 | 1.003× | 1.011× | 0 → 0 | 0 → 0 |
| `default/missing` | 45.66 | 45.85 | 0.918× | 1.001× | 0 → 0 | 0 → 0 |
| `name/empty-owner` | 169.97 | 174.07 | 0.974× | 1.060× | 1 → 1 | 0 → 0 |
| `name/long` | 807.80 | 716.40 | 1.130× | 1.014× | 1 → 1 | 2 → 2 |
| `name/normal` | 763.46 | 682.25 | 1.095× | 1.014× | 1 → 1 | 2 → 2 |
| `name/short` | 320.35 | 225.55 | 1.383× | 1.043× | 1 → 1 | 0 → 0 |
| `name/unicode` | 562.51 | 474.73 | 1.188× | 0.988× | 1 → 1 | 1 → 1 |
| `rename/1-first` | 565.34 | 522.23 | 1.093× | 1.054× | 2 → 2 | 0 → 0 |
| `rename/1-last` | 538.35 | 495.63 | 1.090× | 1.002× | 2 → 2 | 0 → 0 |
| `rename/16-first` | 2286.05 | 1347.77 | 1.709× | 1.004× | 2 → 2 | 0 → 0 |
| `rename/16-last` | 4031.34 | 2172.22 | 1.870× | 1.001× | 2 → 2 | 0 → 0 |
| `rename/4-first` | 905.00 | 683.87 | 1.323× | 1.042× | 2 → 2 | 0 → 0 |
| `rename/4-last` | 1241.01 | 856.80 | 1.448× | 1.116× | 2 → 2 | 0 → 0 |
| `rename/64-first` | 7776.01 | 4108.51 | 1.902× | 0.963× | 2 → 2 | 0 → 0 |
| `rename/64-last` | 15056.78 | 7721.13 | 1.950× | 0.985× | 2 → 2 | 0 → 0 |
| `rename/empty` | 37.18 | 37.91 | 0.981× | 0.987× | 0 → 0 | 0 → 0 |
| `rename/missing` | 56.81 | 55.25 | 1.043× | 0.983× | 0 → 0 | 0 → 0 |

Process throughput ranges (minimum–maximum):

- `default/1-first`: 0.872×–1.000×; control 0.927×–1.045×.
- `default/1-last`: 0.815×–1.034×; control 0.797×–0.974×.
- `default/16-first`: 0.920×–1.033×; control 0.943×–1.017×.
- `default/16-last`: 1.456×–1.576×; control 0.925×–1.078×.
- `default/4-first`: 0.991×–1.044×; control 0.932×–1.040×.
- `default/4-last`: 1.171×–1.285×; control 0.933×–1.065×.
- `default/64-first`: 0.947×–0.981×; control 0.926×–1.019×.
- `default/64-last`: 1.646×–1.689×; control 0.970×–1.045×.
- `default/empty`: 0.953×–1.048×; control 0.910×–1.146×.
- `default/missing`: 0.842×–1.938×; control 1.000×–1.067×.
- `name/empty-owner`: 0.924×–1.057×; control 0.864×–1.087×.
- `name/long`: 1.098×–1.206×; control 0.955×–1.045×.
- `name/normal`: 1.029×–1.177×; control 0.991×–1.053×.
- `name/short`: 1.310×–1.502×; control 0.957×–1.113×.
- `name/unicode`: 1.154×–1.227×; control 0.976×–1.060×.
- `rename/1-first`: 0.989×–1.136×; control 0.784×–1.066×.
- `rename/1-last`: 1.040×–1.114×; control 0.883×–1.125×.
- `rename/16-first`: 1.635×–1.736×; control 0.980×–1.064×.
- `rename/16-last`: 1.707×–1.883×; control 0.946×–1.017×.
- `rename/4-first`: 1.275×–1.415×; control 0.928×–1.082×.
- `rename/4-last`: 1.424×–1.499×; control 0.992×–1.126×.
- `rename/64-first`: 1.748×–1.948×; control 0.956×–1.030×.
- `rename/64-last`: 1.933×–1.986×; control 0.960×–1.022×.
- `rename/empty`: 0.861×–0.982×; control 0.953×–1.133×.
- `rename/missing`: 1.009×–1.068×; control 0.909×–1.018×.

Observed raw paired medians below 0.970×:

- `default/1-first`: 260.50 → 286.32 cycles; 0.923×, control 0.958×.
- `default/1-last`: 286.89 → 291.58 cycles; 0.943×, control 0.913×.
- `default/missing`: 45.66 → 45.85 cycles; 0.918×, control 1.001×.

Differences close to identical-source controls are inconclusive. Default selection mainly benefits when the match follows a substantial prefix; first-record results show little benefit, and small-input costs remain visible alongside their controls. Large lists and long shared name prefixes make comparison savings more visible. This does not establish how often those inputs occur in deployed pad configs.

## Exactness and allocations

- Baseline, optimized release and package opt-level 0 with assertions/overflow checks each pass **246 unit + 8 integration tests**, covering all four profile integration targets. Ordinary test-name/status sets match. Dependencies retain cached release profiles in the assertion-enabled gate. New timing and trace tests run explicitly outside normal gates.
- Differential checks cover complete records and their String/Vec capacities, return booleans, serial order, missing names, duplicate ASCII-case names, empty names/new names, whitespace, NUL, Unicode names and aliases that remain distinct under ASCII-only matching. They run on 0/1/2/3/8/16/64 records and 8,192 seeded mixed lists, with both ordinary and deliberately reserved spare capacities. Existing migration, lookup and serialization regressions remain included.
- Name construction checks all pairs from a targeted text cohort and every pair of lengths 0–99 for ASCII, multibyte Unicode and Unicode whitespace. Output bytes and String capacity match the original formatter. No special Unicode folding or trimming of the config name is introduced. Collection/string capacities remain an implementation detail of the measured standard library; future toolchains must be reverified.
- A **17,010,961-byte deterministic trace** matches across all three profiles, SHA-256 `3d6f9fe8314d1944b920939212cbd1c6e0894c6fd3c912151f92df143e17fafc`. It records rename/default outcomes, complete records, serial order and capacities across first/middle/last/missing positions, conflicts and spare capacity; it also records migrated-name inputs/output/capacity across boundary lengths through 4,096. The ordinary differential corpus is broader than this trace.
  - `pad` part: 17,010,953 bytes, SHA-256 `12aabced8c267b6c787abcfda81d153a8fdebdc20d7e5352e103a2fef0e495d5`.
- Every one of the **500 raw rows** matches its process log, including all timing fields and all six allocation counters. Original/current counters have exact parity for every fixture and process, including allocations, reallocations, frees, requested/freed bytes and peak requested bytes. This batch removes CPU work without claiming allocation savings. Counters exclude stack use, allocator metadata and total RSS.
- Clippy exits successfully with zero diagnostics in changed production code. One reviewed fixture-only style warning remains: `manual_is_multiple_of` on the constructor's `global_default: index % 2 == 0`. The complete diagnostic, source line and rationale are recorded; the measured fixture source is unchanged. Production-only `cargo check --offline --locked --lib --release` passes with a verified opt-level-3 non-test artifact and assertions/overflow checks disabled. Audits verify **28 package inputs**, exact three-body/signature scope, frozen Git references, selected prototypes, source/dependency archives, compiler/lock/profiles, test names, binaries/logs, traces and all measurement fields.

## Reproduction

`scripts/benchmark_pad_mutations.py` builds the actual profile package and all four integration targets in the existing six-member scratch workspace, snapshots the Git dependency closure and exact overlays, preserves cache mtimes, uses C: compiler/test temporary storage and resolves offline with `--locked`. Repository Cargo files and dependency versions are unchanged; resolved lock SHA remains `1ea90eddd4bbd96306a7c4e58d6142e5ab1ec9837fd79aeb69b0d6eb0e5c8211`.

```powershell
$env:DEADSYNC_PAD_MUTATIONS_ARCHIVE = 'C:\deadsync-pad-mutations-20261105'
python scripts/benchmark_pad_mutations.py baseline --baseline 3fed54203393396b976e539ece4453cd06a60026 --runs 5 --reference baseline
python scripts/benchmark_pad_mutations.py final --runs 5 --reference baseline
python scripts/benchmark_pad_mutations.py debug --debug --reference baseline
```

Run stages sequentially with archived compiler/lock/workspace inputs. Baseline restores the original pad-config module plus only the new test registration. Frozen references and common fixtures remain identical across all gates. Each timing TSV retains 250 rows; checks JSON verifies 500 rows and all process/control ranges.

The selected standalone prototype passed correctness and three fresh benchmark processes before production edits. An initial checker failure is archived separately: that checker cloned one input but moved the other, giving them different starting capacities. Both inputs were then cloned identically; production candidate bodies were unchanged. The failed checker source/log hashes and corrected prototype source/binary/log hashes remain verified. Prototype source snapshots use Windows CRLF; their raw hashes and normalized Git-content hashes are recorded separately. Actual package data above is authoritative.
