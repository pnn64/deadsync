# Profile identity helpers — 2026-11-05

Baseline: `f825df1d7f414ee94efe0d60ac4ca27e664826e7`. Three safe changes in `deadsync-profile/src/lib.rs`:

1. Filter player initials as UTF-8 bytes, accepting only the same ASCII alphanumeric, `?` and `!` set. Stop immediately after writing the fourth accepted byte. This removes decoding of rejected Unicode characters and the limit check for rejected bytes. UTF-8 continuation bytes are never ASCII, so they cannot create accepted characters. The original four-byte allocation, uppercase mapping, output bytes and capacity are unchanged.
2. Validate GUIDs by exact 36-byte length, four fixed dash positions and hexadecimal reductions over the five fixed spans. This removes splitting, delimiter searches and variable group traversal. The hexadecimal reductions use bitwise AND to allow processing a whole fixed span without per-byte early exits. Uppercase/lowercase ASCII hex are still accepted; all other bytes and layouts remain rejected. There is no allocation.
3. Fold trimmed derivation inputs of at most 64 UTF-8 bytes into a bounded stack buffer before UUID v5 hashing. This replaces the temporary heap string for the common 16-hex ITGmania GUID, and for other short inputs. Longer strings keep the original allocating fallback. Trimming remains Unicode-aware; folding changes ASCII uppercase bytes only, without Unicode case conversion. The namespace, SHA-1 input bytes, UUID version/variant, canonical output and returned string capacity are unchanged.

The diff adds 38 production lines and removes 20, plus four test-registration lines. There is no new unsafe code, dependency, cache, persistent state or change to public signatures. The 64-byte stack buffer is local to derivation. These helper timings cover profile identity parsing/import and initials normalization; they are not end-to-end filesystem or gameplay measurements.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz. Actual Cargo package test binaries use release opt-level 3, fat LTO, static CRT, and the exact compiler/lockfile recorded in each manifest. Five fresh processes per stage run twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Windows `QueryThreadCycleTime` records CPU cycles. Our builds and CPU benchmarks run sequentially; unrelated user activity is not controlled.

The baseline control compiles unchanged production beside frozen pre-change function bodies in the same benchmark. These functions do not call one another, so each comparison isolates one change. Baseline control ratios expose code layout and timing bias. Cycle columns are independent medians across five processes; paired throughput is the median of the five within-process ratios. All process ratios, nanoseconds and six allocation counters remain in the TSV/checks data. Fixtures and tracking setup stay outside timing; result destruction is timed and counted. The counted-System allocator remains installed with counting disabled in CPU samples, retaining its inactive TLS check; allocation reductions are therefore reported independently.

Each fixture is one complete public function call. Initials use 20,000 repetitions (2,000 for long rejected-input fixtures); GUID validation uses 200,000; derivation uses 2,000. `initials/invalid` repeats four Unicode characters 256 times; `ascii-invalid` repeats six rejected ASCII bytes 256 times; `early` starts with four accepted letters followed by a long Unicode tail. Validation includes valid lowercase/uppercase GUIDs, invalid early/late hex, wrong delimiter, empty/short/4,096-byte input and a non-ASCII byte sequence. Derivation includes 16-byte IDs, Unicode trimming, blank values, non-ASCII strings, byte lengths 63/64/65, and a 1,536-byte fallback input.

| Workload | Frozen reference cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `derive/blank` | 40.17 | 41.05 | 0.979× | 0.998× |
| `derive/empty` | 26.89 | 28.43 | 0.946× | 1.000× |
| `derive/len63` | 1281.88 | 1095.52 | 1.171× | 0.953× |
| `derive/len64` | 1215.37 | 1040.98 | 1.184× | 1.008× |
| `derive/len65` | 1232.27 | 1176.19 | 1.015× | 0.994× |
| `derive/long` | 9421.60 | 9296.48 | 1.004× | 1.000× |
| `derive/lower16` | 810.28 | 675.40 | 1.196× | 0.977× |
| `derive/trimmed` | 844.42 | 712.72 | 1.208× | 0.975× |
| `derive/unicode` | 829.38 | 711.84 | 1.210× | 0.967× |
| `derive/upper16` | 815.99 | 680.45 | 1.209× | 1.006× |
| `initials/ascii-invalid` | 7595.91 | 5807.09 | 1.308× | 1.003× |
| `initials/dirty` | 203.18 | 179.51 | 1.148× | 1.023× |
| `initials/early` | 183.05 | 164.04 | 1.103× | 1.033× |
| `initials/empty` | 163.37 | 149.41 | 1.080× | 0.975× |
| `initials/invalid` | 5366.34 | 3610.99 | 1.496× | 0.984× |
| `initials/short` | 179.96 | 161.73 | 1.103× | 1.043× |
| `initials/unicode` | 209.30 | 189.25 | 1.106× | 0.997× |
| `validate/early` | 38.59 | 13.69 | 2.828× | 0.991× |
| `validate/empty` | 16.36 | 3.16 | 5.177× | 0.992× |
| `validate/hyphen` | 37.21 | 3.94 | 9.373× | 1.011× |
| `validate/late` | 231.97 | 64.00 | 3.624× | 1.001× |
| `validate/long` | 912.52 | 3.18 | 288.278× | 0.981× |
| `validate/lower` | 234.33 | 64.35 | 3.627× | 1.000× |
| `validate/short` | 60.09 | 3.16 | 19.016× | 1.001× |
| `validate/unicode` | 38.10 | 13.86 | 2.767× | 1.022× |
| `validate/upper` | 235.97 | 64.35 | 3.634× | 0.991× |

Selected gains and controls:

- `initials/short`: **1.103×**; control **1.043×**.
- `initials/dirty`: **1.148×**; control **1.023×**.
- `initials/invalid`: **1.496×**; control **0.984×**.
- `initials/ascii-invalid`: **1.308×**; control **1.003×**.
- `validate/lower`: **3.627×**; control **1.000×**.
- `validate/upper`: **3.634×**; control **0.991×**.
- `validate/long`: **288.278×**; control **0.981×**.
- `derive/lower16`: **1.196×**; control **0.977×**.
- `derive/upper16`: **1.209×**; control **1.006×**.
- `derive/len64`: **1.184×**; control **1.008×**.

Costs and qualifications:

- `derive/empty`: **0.946×**, **5.7% more cycles**; control **1.000×**. Independent cycle medians are 26.89 → 28.43; five paired ratios are 0.946×, 0.946×, 0.946×, 0.946×, 0.946×.

The few-cycle blank-derivation path can be sensitive to the larger function's call/frame layout; the source returns before the buffer declaration. Short initials and Unicode fixtures must be read with their source-identical controls. In particular, the 1.103× short-initials gain has a 1.043× positive control; dirty and long rejected inputs provide clearer CPU evidence. Allocation-free validation and the exact derivation allocation reduction provide separate evidence. Long derivation inputs retain the original body, so their small timing differences do not demonstrate an algorithmic improvement. The very large ratio for rejecting a 4,096-byte malformed GUID comes from the fixed length check and does not describe valid-ID parsing.

## Allocation and memory evidence

All six counters are checked against raw logs for all 520 rows. Byte counters record requested allocation layouts, excluding allocator bookkeeping and process RSS. Every workload has zero reallocations, balanced allocation/free counts and balanced allocated/freed bytes. All GUID-validation counters are zero. Initials always retain exactly one four-byte allocation and matching free, including empty and rejected input.

| Derivation fixture | Allocations, original → current | Allocated bytes | Peak bytes |
| --- | ---: | ---: | ---: |
| `derive/blank` | 0 → 0 | 0 → 0 | 0 → 0 |
| `derive/empty` | 0 → 0 | 0 → 0 | 0 → 0 |
| `derive/len63` | 2 → 1 | 99 → 36 | 63 → 36 |
| `derive/len64` | 2 → 1 | 100 → 36 | 64 → 36 |
| `derive/len65` | 2 → 2 | 101 → 101 | 65 → 65 |
| `derive/long` | 2 → 2 | 1572 → 1572 | 1536 → 1536 |
| `derive/lower16` | 2 → 1 | 52 → 36 | 36 → 36 |
| `derive/trimmed` | 2 → 1 | 52 → 36 | 36 → 36 |
| `derive/unicode` | 2 → 1 | 48 → 36 | 36 → 36 |
| `derive/upper16` | 2 → 1 | 52 → 36 | 36 → 36 |

The stack buffer removes a temporary allocation without changing the 36-byte returned GUID allocation. It is not included in heap counters. For short inputs of at most 36 bytes, the original temporary is freed before output allocation, so heap peak may remain 36 bytes despite fewer allocations/total bytes. The original fallback remains at 65 bytes and above; empty/blank inputs remain allocation-free.

## Exactness and validation

- Baseline, final optimized release and package opt-level 0 with debug assertions/overflow checks each pass **243 unit + 8 integration tests**. All four integration targets run: name comparison (3), noteskin parts (2), pad lookup growth (2), pad configuration migration (1). Dependencies stay in their cached release profile during the package-debug gate. The exact ignored counts are retained in checks JSON; new timing and trace tests run explicitly.
- New differential tests compare every Unicode scalar in initials at empty, three-character and full four-character prefixes, preserving exact bytes and returned capacities. Long invalid and early-stop inputs are included. GUID checks mutate every ASCII byte at every position of a valid 36-byte GUID, plus non-ASCII replacements and length/layout cases. Derivation covers all ASCII single-byte mutations, Unicode whitespace and ASCII-only folding, embedded NULs, 0–139 repeated strings across byte thresholds, 16,384 deterministic random UTF-8 strings, and an independent UUID v5 vector (`99f55b745304ebcf` → `f0b6ee3d-8535-5826-96eb-15fd8ab16ab0`, also checked with Python's standard UUID implementation).
- An **18,998,019-byte deterministic public-output trace** is identical across all three profiles: SHA-256 `ac498f8346822075df85ccda74b1abb8b61fd635fece33593088a29678634ad4`. It contains complete initials/validation/derivation outputs for fixtures and random UTF-8 strings, including capacities, and initials output/capacity for every Unicode scalar. This trace does not claim exhaustive UUID derivation over every Unicode scalar; differential derivation tests cover the stated mutations, length boundaries and randomized strings.
- Scoped Clippy and a production-only `cargo check --offline --locked --lib --release` pass. The source audit verifies exactly three changed function bodies plus test registration, and matches frozen function bodies to Git. It verifies original/dependency archives, 859 external fixtures, 17 shipped cel PNGs, compiler, lockfile, profiles, source snapshots, binaries, logs, traces and all **520 raw timing rows**. All **24 package input files** match the tested source; every baseline allocation counter matches its current-source control.
- Two exploratory trials are archived separately from formal Cargo results. The first used a per-byte early exit in GUID hex validation and checked the initials limit before every byte. The retained second version uses fixed-span reductions and terminates initials immediately after the fourth accepted byte; trial timings selected this version. Exploratory binaries, source snapshots and logs are hashed, and their results are not substituted for formal measurements.

## Reproduction and artifacts

`scripts/benchmark_profile_identity.py` derives the real local dependency closure, snapshots its Git archive, overlays exact checkout bytes, builds the library and all package integration tests, and archives every input/result. It computes overlay paths before archive extraction so identical final source is written only once, preserving cache mtimes across LF/CRLF differences. This preparation change does not change compiled content. It retains locked offline dependency resolution and the existing minimal workspace. Fixture data and build outputs live outside the isolated worktree; the main user's working tree is not modified.

```powershell
$env:DEADSYNC_PROFILE_IDENTITY_ARCHIVE = 'D:\deadsync-profile-identity-20261105'
python scripts/benchmark_profile_identity.py baseline --baseline f825df1d7f414ee94efe0d60ac4ca27e664826e7 --runs 5 --reference baseline
python scripts/benchmark_profile_identity.py debug --debug --reference baseline
python scripts/benchmark_profile_identity.py final --runs 5 --reference baseline
```

Run these stages sequentially on the same compiler/host. The baseline requires this batch's test harness alongside the specified pre-change production revision. Manifests contain the exact workspace manifest, build flags, resolved lockfile, source/archive/fixture hashes, profiles, compiler identity, executable hashes, commands and trace digests. `profile-identity-baseline.tsv` and `profile-identity-final.tsv` retain raw per-process cycles, nanoseconds and allocation counts; `profile-identity-checks.json` retains process ratios, audits, validation logs, scoped diagnostics and exploratory snapshot identities.
