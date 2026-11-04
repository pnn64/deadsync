# Name comparison and profile-ID sorting — 2026-11-04

Baseline: `4502d2587fab5fc775a0fb9a89f6c30ab9afce0d`. Three safe changes in `deadsync-profile`:

1. Compare long ASCII names in fixed sixteen-byte blocks after preserving four scalar prefix bytes. Independent `to_ascii_lowercase` operations expose packed work to the compiler; lexicographic block comparison and the original scalar remainder preserve byte order for all UTF-8 inputs. Short shared lengths below twenty bytes retain the scalar iterator. The private block loop is outlined to keep its temporary arrays out of early-return paths.
2. Keep the first eight bytes of Unicode name comparison on the scalar ASCII path. When that prefix matches and both complete suffixes are ASCII, reuse the block comparator. Non-ASCII suffixes retain the original ASCII-prefix scan and `char::to_lowercase` expansion iterator. This preserves the baseline’s context-free Unicode mapping.
3. Reuse the Unicode name comparator for profile IDs, retaining the original raw-string tie-break. This removes the duplicate character-by-character lowercase traversal from the common ASCII ID path.

These functions run in favorites, music-wheel and profile list sorting. The profile-ID comparator is also used when enumerating local profiles. Public signatures and return values are unchanged. The production diff adds 56 lines and removes eight: the new block/dispatch code reduces executed byte-wise work, and the profile-ID wrapper removes its duplicate lowercase iterator chain. No unsafe production code, lookup table, heap storage, dependency or shared state is added. Fixed block temporaries live on the stack. Standalone release assembly inspection confirms sixteen-byte SSE2 case folding (`paddb`, `pminub`, `pcmpeqb`, `pand`, `por`); the assembly inputs, commands and hashes are archived. This is source/code-generation inspection, separate from the actual Cargo binary measurements. All other production code, including the withdrawn color experiments, matches the baseline.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.99.1. The real Cargo-built library and integration binaries use release opt-level 3, fat LTO and the static CRT. Five fresh processes per stage execute twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Windows `QueryThreadCycleTime` supplies thread-cycle counts. Our benchmark processes run sequentially; other user builds may still run.

The frozen function bodies are compiled beside the actual public library calls. The copied profile-ID body omits its library `inline(always)` annotation; benchmark function pointers are black-boxed, and the identical-production-source profile-ID controls measure 0.996x-1.003x. An identical-production-source baseline control exposes compiler context, indirect-call and timing bias. Cycle columns are independent process medians; paired throughput is the median of five within-process ratios. Each individual ratio and its range is retained in the checks JSON, and the raw TSVs retain nanoseconds and all six allocator counters. These measurements cover the comparator and sort fixtures shown below.

Each comparison fixture processes 256 string pairs for 500 repetitions per round. Each full-sort round times ten operations; every operation resets a warmed vector of 1,024 string references and runs `sort_unstable_by`. The reset and complete sort are both timed; string creation, allocation tracking and correctness assertions are outside timing. Calls use black-boxed function pointers and inputs. Stable sorts are checked separately for correctness. All measured operations include result consumption.

The cohorts are: `short` (eight-character pack names), `early` (long names differing in the first byte), `near` (long names differing in the second byte), `shared` (long mixed-case compilation names with a common ASCII prefix), `uuid` (36-byte IDs sharing a prefix, including case-only ties), `unicode` (an ASCII prefix followed by Greek, accented Latin, Turkish dotted I and Japanese), `equal` (identical long ASCII names), and `random` (independent 40-letter names). These are synthetic stress/cohort fixtures; shared-prefix UUIDs are not a claim about the distribution of generated UUIDs. Random names and second-byte differences are included to expose early-exit costs.

The retained long-prefix ASCII compare fixtures measure **1.611x-2.790x** paired throughput; complete shared-prefix ASCII sorts measure **2.129x-2.596x**. Unicode comparison on long ASCII cohorts measures **1.153x-1.542x**; Unicode-name shared/UUID sorts measure **1.130x-1.197x**. Profile-ID comparison benefits from the shared ASCII path across all cohorts; complete profile-ID sorts measure **1.925x-12.423x**. Consult each specific identical-source control before interpreting small ratios.

Identical-source control medians span **0.871x-1.086x**. In particular, the Unicode early/near/random controls already measure **0.871x/0.904x/0.881x**; the final raw ratios **0.876x/0.874x/0.848x** include timing differences of a similar scale. Their entire raw cost cannot be assigned to the algorithm change. ASCII early/random ratios **0.957x/0.944x** have controls **1.000x/1.003x**, about **4.5%/5.9% more cycles** (roughly 0.7 additional cycle per comparison from the independent medians). The ASCII short fixture measures **0.972x**, about **2.9% more cycles**; control **0.998x**. Unicode random-name sorting measures **0.913x**, against a **0.955x** control. Shared/UUID Unicode sorting gains (**1.130x/1.197x**) also have positive controls (**1.086x/1.049x**); the direct long-prefix comparisons provide clearer evidence of that retained path.

The extra path selection and ASCII admission can cost cycles on short prefixes, early mismatches or mixed-Unicode inputs. The following final ratios below 0.970x are explicitly retained as qualifications, including compiler-context variation shown by their controls:

- `compare/ascii/early`: 0.957x, about 4.5% more cycles; control 1.000x. Median cycles per 256-pair fixture: 2835.50 to 3018.56.
- `compare/ascii/random`: 0.944x, about 5.9% more cycles; control 1.003x. Median cycles per 256-pair fixture: 2970.27 to 3146.31.
- `compare/unicode/early`: 0.876x, about 14.1% more cycles; control 0.871x. Median cycles per 256-pair fixture: 3999.29 to 4492.73.
- `compare/unicode/near`: 0.874x, about 14.4% more cycles; control 0.904x. Median cycles per 256-pair fixture: 5106.45 to 5866.80.
- `compare/unicode/random`: 0.848x, about 17.9% more cycles; control 0.881x. Median cycles per 256-pair fixture: 4079.19 to 4847.00.
- `compare/unicode/short`: 0.924x, about 8.2% more cycles; control 0.948x. Median cycles per 256-pair fixture: 10665.50 to 11493.46.
- `sort/unicode/random`: 0.913x, about 9.5% more cycles; control 0.955x. Median cycles per sort: 248386.20 to 274572.50.

| Workload | Frozen reference cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `compare/ascii/early` | 2835.50 | 3018.56 | 0.957× | 1.000× |
| `compare/ascii/equal` | 120303.56 | 43570.75 | 2.790× | 0.995× |
| `compare/ascii/near` | 4290.79 | 4204.30 | 1.021× | 1.000× |
| `compare/ascii/random` | 2970.27 | 3146.31 | 0.944× | 1.003× |
| `compare/ascii/shared` | 43626.94 | 19320.83 | 2.261× | 1.004× |
| `compare/ascii/short` | 9165.44 | 9502.59 | 0.972× | 0.998× |
| `compare/ascii/unicode` | 26036.65 | 16584.98 | 1.611× | 0.998× |
| `compare/ascii/uuid` | 37423.44 | 16540.21 | 2.274× | 0.996× |
| `compare/profile/early` | 23330.22 | 5716.66 | 4.087× | 0.998× |
| `compare/profile/equal` | 1536732.69 | 93229.11 | 16.483× | 1.001× |
| `compare/profile/near` | 35446.61 | 7119.26 | 5.108× | 0.996× |
| `compare/profile/random` | 23599.32 | 5710.07 | 4.133× | 1.000× |
| `compare/profile/shared` | 485596.78 | 39334.41 | 12.352× | 1.000× |
| `compare/profile/short` | 95007.50 | 11904.80 | 7.837× | 1.000× |
| `compare/profile/unicode` | 538034.44 | 283625.17 | 1.894× | 1.001× |
| `compare/profile/uuid` | 447772.09 | 36248.23 | 12.356× | 0.999× |
| `compare/unicode/early` | 3999.29 | 4492.73 | 0.876× | 0.871× |
| `compare/unicode/equal` | 135388.04 | 86576.95 | 1.542× | 1.001× |
| `compare/unicode/near` | 5106.45 | 5866.80 | 0.874× | 0.904× |
| `compare/unicode/random` | 4079.19 | 4847.00 | 0.848× | 0.881× |
| `compare/unicode/shared` | 47940.55 | 39165.38 | 1.225× | 1.009× |
| `compare/unicode/short` | 10665.50 | 11493.46 | 0.924× | 0.948× |
| `compare/unicode/unicode` | 279513.07 | 282810.82 | 0.999× | 1.018× |
| `compare/unicode/uuid` | 40174.65 | 35080.49 | 1.153× | 1.011× |
| `sort/ascii/random` | 207273.80 | 207537.30 | 1.008× | 1.001× |
| `sort/ascii/shared` | 171912.40 | 70020.50 | 2.383× | 0.997× |
| `sort/ascii/unicode` | 1741732.50 | 814037.90 | 2.129× | 1.008× |
| `sort/ascii/uuid` | 154857.20 | 57926.10 | 2.596× | 1.003× |
| `sort/profile/random` | 1332584.50 | 334561.90 | 4.013× | 1.003× |
| `sort/profile/shared` | 17849168.80 | 1490975.70 | 11.886× | 0.999× |
| `sort/profile/unicode` | 19996033.00 | 10416197.00 | 1.925× | 1.002× |
| `sort/profile/uuid` | 16507629.30 | 1328918.90 | 12.423× | 0.997× |
| `sort/unicode/random` | 248386.20 | 274572.50 | 0.913× | 0.955× |
| `sort/unicode/shared` | 164690.90 | 149786.80 | 1.130× | 1.086× |
| `sort/unicode/unicode` | 1155140.80 | 1162559.70 | 0.981× | 1.017× |
| `sort/unicode/uuid` | 168071.20 | 142828.60 | 1.197× | 1.049× |

Every one of the 36 workloads records **zero allocations, reallocations, frees, allocated bytes, freed bytes and peak bytes**, for both implementations in every process. The baseline control also records all six zeros. The retained improvements remove CPU work rather than heap traffic.

## Profile-ID delegation in isolation

An additional five-process primitive ablation compiles the exact new profile-ID function body while binding its delegated name helper to the independently frozen **baseline** Unicode comparator. Both block comparison and whole-ASCII admission stay absent from that helper. This measures the delegation change independently of the first two optimizations. The actual public library behavior and complete combined performance remain covered by the Cargo tests and primary measurements above. Each operation processes 256 pairs, 500 repetitions per alternating round; all six counters remain zero.

| Cohort | Old character traversal cycles | Delegate to baseline helper cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| `reuse/early` | 23629.62 | 4685.45 | 5.046x |
| `reuse/equal` | 1685762.18 | 145935.89 | 11.539x |
| `reuse/near` | 38133.30 | 6358.92 | 6.046x |
| `reuse/random` | 23903.99 | 4949.73 | 4.879x |
| `reuse/shared` | 529088.07 | 46699.07 | 11.319x |
| `reuse/short` | 102602.64 | 11676.08 | 8.776x |
| `reuse/unicode` | 583990.72 | 298558.63 | 1.956x |
| `reuse/uuid` | 490188.28 | 44350.85 | 11.053x |

The ablation driver, source inputs, generated source, executable, command, compiler and five logs are fingerprinted in the separate reuse manifest; its 80 raw rows are audited alongside the 720 primary rows.

## Correctness and qualification

The baseline, final release, and target-package opt-level 0 with debug assertions and overflow checks each pass **237 unit tests and 8 integration tests** across all four package integration targets (`name_comparison`, `noteskin_parts`, `pad_lookup_growth`, `pad_config_migration`). Four unit and three integration benchmarks/dumps remain ignored in the normal gates; the new benchmark and trace run explicitly. The opt-level 0 gate keeps cached release dependency builds, so it is a target-package debug check rather than an all-dependency debug build. Doctests are not included in these gates.

Differential tests compile the frozen original comparators and compare exact `Ordering` values: every ASCII-byte pair at thirteen prefix lengths around block boundaries; empty strings and unequal lengths; case-only ties; Unicode lowercase expansions, final sigma forms, accented characters, CJK, emoji and embedded NULs; 32,768 random UTF-8 pairs and their reversals; and 1,024 case-varied profile IDs with the original byte tie-break. Both stable and unstable complete sort outputs match across eight cohorts. Allocation tests exercise the real public functions.

The deterministic trace records pair orderings and every original position in the complete unstable sort order for all eight cohorts and all three comparator families. It also reruns the full differential checks. Trace identity is byte-exact across baseline, release and package-debug builds.

- `baseline`: 417,792 bytes; SHA-256 `2a10c21f5ac078083525a8a7f651a557fde41851b9b0ff98a23e71621c2e6540`.
- `debug`: 417,792 bytes; SHA-256 `2a10c21f5ac078083525a8a7f651a557fde41851b9b0ff98a23e71621c2e6540`.
- `final`: 417,792 bytes; SHA-256 `2a10c21f5ac078083525a8a7f651a557fde41851b9b0ff98a23e71621c2e6540`.

Scoped Clippy for the library and tests passes with no diagnostics in the changed comparator functions or new harness/reference files. A production-only release `cargo check --lib` passes. Their source and build-input fingerprints match the measured code. The audit verifies the production diff’s exact scope, frozen original against Git after canonical Rust formatting, all archived source overlays/dependency archives/fixtures/binaries/profiles/logs/traces, and all 800 raw timing rows (720 primary and 80 isolated reuse).

The retained implementation was selected after five archived primitive trials: direct ASCII blocks exposed early-exit costs; initial per-block ASCII checks did not justify their overhead; whole-suffix ASCII admission helped long names but hurt short-prefix cases; scalar prefixes and an outlined block loop reduced those costs. Earlier trial timings are retained as exploratory evidence, not substituted for the final measurements. Three unrelated color lookup/formatter candidates were fully withdrawn after paired throughput regressed across normal tested modes (roughly 0.63–0.93×); their sources, executables, logs and checksums remain archived.

## Reproduce and artifacts

Set `DEADSYNC_NAME_COMPARISON_ARCHIVE` to a scratch directory with sufficient free space. The driver verifies the original source archive, reconstructs the package dependency closure from Git, overlays current sources and prior committed changes, restores required fixtures, and copies each built executable into its stage before another build can overwrite it. Compilation temporary files use scratch storage; tests keep the host NTFS temporary directory. The minimal workspace explicitly registers the profile package. Its lockfile was seeded from repository HEAD, resolved once for that workspace, then held fixed with `--locked` during every measured build; all three stage manifests preserve the lockfile. Missing locked registry dependencies were fetched before the formal run.

```powershell
$env:DEADSYNC_NAME_COMPARISON_ARCHIVE='D:\deadsync-name-comparison-20261104'
python scripts/benchmark_name_comparison.py baseline --baseline 4502d2587fab5fc775a0fb9a89f6c30ab9afce0d --runs 5 --reference baseline
python scripts/benchmark_name_comparison.py debug --debug --reference baseline
python scripts/benchmark_name_comparison.py final --runs 5 --reference baseline
python scripts/benchmark_name_reuse.py
```

The checked-in drivers, integration harness/frozen reference, stage manifests, raw baseline/final/reuse TSVs and checks JSON retain inputs and measurements. Large source snapshots, executables and logs are local under `D:\deadsync-name-comparison-20261104`; rejected color artifacts are under `D:\deadsync-color-codecs-20261104`. The manifests record local archive/fixture prerequisites and SHA-256 fingerprints.
