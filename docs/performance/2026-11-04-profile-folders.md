# Profile folder naming — 2026-11-04

Baseline: `9b0ffa6aae31fbd9535b678a1c9b49b36c416555`. Measurements and validation continue overnight into 2026-11-05. Three safe changes in `deadsync-profile/src/lib.rs`:

1. Classify the exact existing Windows reserved-name set by length and folded three-byte prefixes. The four-byte forms accept only COM/LPT plus a final digit 1–9. This replaces a scan of 22 case-insensitive string literals. ASCII case rules and the set of accepted/rejected names are unchanged.
2. Return the sanitizer’s existing string when its capacity equals the trimmed result length. That equality guarantees the entire buffer is already the exact untrimmed result, with the same public capacity as the old copied result. Other cases keep the original final copy. Character filtering, whitespace collapse, removal order, trimming, the whole-character overshoot at 48 bytes and reserved-name rejection are unchanged.
3. After a base collision, keep the original scalar search for lists below 32 entries. Larger lists probe suffixes 2–8 first. Only when all seven are occupied, scan the existing names once into a 157-word occupancy bitmap and select the first free canonical suffix through 9999. This replaces repeated list scans and decimal formatting for dense collisions. Leading zeros, signs, whitespace, non-ASCII digits and suffixes outside the original range do not occupy canonical slots.

The bitmap is **1,256 bytes of stack storage**, lives in an outlined helper and is initialized only after the guarded collision path is selected. It adds no heap index, persistent state, dependency or unsafe code. Public signatures, strings, returned capacities, case rules, fallback selection and the existing suffix bound are preserved. The changes affect profile creation, renaming and migration name selection; timings cover these pure helpers. The source diff adds 72 production lines and removes six, plus four test-registration lines; execution removes one string copy/allocation on the ownership path and repeated linear scans on the indexed path.

## Measurements

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.99.1. Real Cargo package binaries use release opt-level 3, fat LTO and the static CRT. Five fresh processes per measured stage execute twelve alternating original/current rounds on CPU 2, discard the first and retain the median of eleven. Windows `QueryThreadCycleTime` supplies thread-cycle counts. Our CPU benchmark processes run sequentially; other user builds may still run.

The baseline control compiles unchanged production beside the frozen reference and isolated ablations. Its ratios expose code-generation context and timing bias. Cycle columns below are independent process medians; paired throughput is the median of the five within-process ratios. Every process ratio, raw nanoseconds and all six allocation fields are retained in the TSV/checks data. All fixture construction and allocation tracking remain outside CPU timing; result destruction is timed and counted. The shared counted-System allocator remains installed with counting disabled during CPU samples, so those samples retain its inactive TLS check overhead. Allocation reductions are reported independently of those timings.

Reserved-name fixtures process lists of 2–8 strings for 100,000 iterations per round. Sanitizer fixtures process one string for 20,000 iterations. Collision fixtures process one complete public call; repetitions scale as `10000 / max(n,1)^2`, clamped to 1–100. Sparse/gap3 fixtures use 20 repetitions and contain 2,048 unrelated names plus the base and optionally suffix 2. Dense lists contain the base and every suffix through n; 9999 exhausts the original range and returns the fallback. These dense fixtures intentionally exercise worst-case behavior; they are not estimates of typical profile counts.

`sanitize/reuse` holds the new reserved-name classifier fixed and restores only the final-copy sanitizer body. `folder/index` and sparse/gap3 `index` hold the current sanitizer and classifier fixed and restore only the original repeated-search body. The ablations isolate each retained mechanism; baseline controls verify the corresponding identical implementations.

Selected gains and costs (paired medians; specific controls are shown in the complete table):

- `reserved/reserved`: **6.176x**; control **0.998x**.
- `reserved/short`: **7.385x**; control **0.993x**.
- `reserved/near`: **15.083x**; control **0.996x**.
- `sanitize/clean`: **1.887x**; control **0.981x**.
- `sanitize/reuse/clean`: **1.645x**; control **1.015x**.
- `sanitize/unicode`: **1.782x**; control **0.912x**.
- `sanitize/reuse/unicode`: **1.624x**; control **1.000x**.
- `folder/32`: **4.396x**; control **0.988x**.
- `folder/index/32`: **4.296x**; control **0.997x**.
- `folder/256`: **47.185x**; control **0.994x**.
- `folder/index/256`: **46.769x**; control **1.001x**.
- `folder/2048`: **418.641x**; control **0.998x**.
- `folder/index/2048`: **446.178x**; control **1.009x**.
- `folder/9999`: **3469.980x**; control **1.000x**.
- `folder/index/9999`: **3599.689x**; control **0.994x**.

Raw paired ratios below 0.970x are retained as qualifications; the controls distinguish code context from source effects:

- `folder/8`: **0.961x**, about **4.1% more cycles**; control **1.008x**.
- `folder/index/31`: **0.848x**, about **18.0% more cycles**; control **0.997x**.
- `reserved/long`: **0.939x**, about **6.5% more cycles**; control **0.996x**.

The 31-entry isolated fixture never enters the index (the threshold is 32). Its five paired ratios are **0.848x, 0.817x, 0.989x, 0.831x, 1.002x**; independent cycle medians are 13,279.80 to 13,433.40. The 0.848x paired median remains a reported compiled dispatch/context cost. The combined 31-entry public call measures **1.013x**, control **0.989x**. Sparse/gap3 combined calls measure **1.008x/1.027x** (controls **1.007x/1.001x**), so these common-gap timings are inconclusive. The reserved/long difference is about 1.1 cycles per two-name fixture (0.6 per name). Small sanitizer gains also need their controls: dirty **1.086x** versus **0.953x**, long **1.043x** versus **1.056x**, and Unicode **1.782x** versus **0.912x**. The held-fixed buffer-reuse ablations (**1.645x** clean, **1.624x** Unicode) and exact allocation reduction provide clearer support for that mechanism.

| Workload | Frozen/isolated reference cycles | Current cycles | Paired throughput | Identical-source control |
| --- | ---: | ---: | ---: | ---: |
| `folder/0` | 449.97 | 265.59 | 1.694× | 0.985× |
| `folder/2048` | 32672354.00 | 78800.00 | 418.641× | 0.998× |
| `folder/256` | 546775.00 | 11634.00 | 47.185× | 0.994× |
| `folder/31` | 13565.10 | 13411.40 | 1.013× | 0.989× |
| `folder/32` | 14365.11 | 3268.11 | 4.396× | 0.988× |
| `folder/64` | 55423.50 | 4829.00 | 11.450× | 0.996× |
| `folder/8` | 2504.49 | 2458.39 | 0.961× | 1.008× |
| `folder/9999` | 1294823043.00 | 373150.00 | 3469.980× | 1.000× |
| `folder/gap3/combined` | 8812.90 | 8582.45 | 1.027× | 1.001× |
| `folder/gap3/index` | 9032.45 | 9219.00 | 1.006× | 1.008× |
| `folder/index/0` | 267.79 | 263.40 | 1.017× | 1.004× |
| `folder/index/2048` | 31777673.00 | 74191.00 | 446.178× | 1.009× |
| `folder/index/256` | 533824.00 | 11634.00 | 46.769× | 1.001× |
| `folder/index/31` | 13279.80 | 13433.40 | 0.848× | 0.997× |
| `folder/index/32` | 14145.56 | 3292.44 | 4.296× | 0.997× |
| `folder/index/64` | 53448.00 | 4829.00 | 11.256× | 1.000× |
| `folder/index/8` | 2295.97 | 2276.21 | 1.015× | 1.019× |
| `folder/index/9999` | 1297880011.00 | 372930.00 | 3599.689× | 0.994× |
| `folder/sparse/combined` | 5893.55 | 5805.80 | 1.008× | 1.007× |
| `folder/sparse/index` | 5849.70 | 5838.70 | 1.011× | 1.000× |
| `reserved/empty` | 241.36 | 35.21 | 6.861× | 1.004× |
| `reserved/long` | 16.52 | 17.64 | 0.939× | 0.996× |
| `reserved/near` | 919.58 | 61.18 | 15.083× | 0.996× |
| `reserved/reserved` | 553.92 | 89.02 | 6.176× | 0.998× |
| `reserved/short` | 326.06 | 43.69 | 7.385× | 0.993× |
| `reserved/unicode` | 357.37 | 44.73 | 8.049× | 0.996× |
| `sanitize/clean` | 445.71 | 240.41 | 1.887× | 0.981× |
| `sanitize/dirty` | 483.03 | 450.49 | 1.086× | 0.953× |
| `sanitize/empty` | 232.63 | 207.03 | 1.047× | 1.021× |
| `sanitize/limit` | 845.82 | 620.54 | 1.377× | 1.052× |
| `sanitize/long` | 825.79 | 781.04 | 1.043× | 1.056× |
| `sanitize/reserved` | 248.30 | 215.30 | 1.127× | 0.992× |
| `sanitize/reuse/clean` | 389.40 | 230.30 | 1.645× | 1.015× |
| `sanitize/reuse/dirty` | 413.69 | 420.01 | 0.995× | 1.034× |
| `sanitize/reuse/empty` | 215.32 | 222.02 | 1.002× | 0.986× |
| `sanitize/reuse/limit` | 795.22 | 630.26 | 1.284× | 0.995× |
| `sanitize/reuse/long` | 792.45 | 773.36 | 1.012× | 1.000× |
| `sanitize/reuse/reserved` | 220.32 | 209.46 | 1.051× | 0.993× |
| `sanitize/reuse/spaces` | 410.67 | 416.39 | 0.986× | 0.994× |
| `sanitize/reuse/unicode` | 380.51 | 228.68 | 1.624× | 1.000× |
| `sanitize/spaces` | 473.77 | 414.95 | 1.142× | 0.973× |
| `sanitize/unicode` | 421.40 | 233.69 | 1.782× | 0.912× |

All reserved-name fixtures record zero in all six allocator counters. Returned strings are included in the other complete-operation samples and dropped before tracking ends. All fixtures have zero reallocations; allocation/free counts and allocated/freed bytes balance.

| Workload | Allocations, before → current | Allocated bytes, before → current | Peak bytes, before → current |
| --- | ---: | ---: | ---: |
| `folder/0` | 2 → 1 | 12 → 6 | 12 → 6 |
| `folder/2048` | 3 → 2 | 23 → 17 | 17 → 17 |
| `folder/32` | 3 → 2 | 23 → 17 | 17 → 17 |
| `folder/8` | 3 → 2 | 23 → 17 | 17 → 17 |
| `folder/9999` | 4 → 3 | 31 → 25 | 25 → 25 |
| `folder/gap3/index` | 2 → 2 | 17 → 17 | 17 → 17 |
| `folder/index/32` | 2 → 2 | 17 → 17 | 17 → 17 |
| `folder/index/9999` | 3 → 3 | 25 → 25 | 25 → 25 |
| `folder/sparse/index` | 2 → 2 | 17 → 17 | 17 → 17 |
| `sanitize/clean` | 2 → 1 | 12 → 6 | 12 → 6 |
| `sanitize/dirty` | 2 → 2 | 15 → 15 | 15 → 15 |
| `sanitize/empty` | 1 → 1 | 6 → 6 | 6 → 6 |
| `sanitize/limit` | 2 → 1 | 102 → 51 | 102 → 51 |
| `sanitize/long` | 2 → 2 | 208 → 208 | 208 → 208 |
| `sanitize/reserved` | 1 → 1 | 4 → 4 | 4 → 4 |
| `sanitize/spaces` | 2 → 2 | 16 → 16 | 16 → 16 |
| `sanitize/unicode` | 2 → 1 | 18 → 9 | 18 → 9 |

## Correctness and qualification

The baseline, final release, and target-package opt-level 0 with debug assertions and overflow checks each pass **240 unit tests and 8 integration tests**. The four existing integration targets are `name_comparison`, `noteskin_parts`, `pad_lookup_growth`, and `pad_config_migration`. Six unit and three integration benchmarks/dumps remain ignored during normal gates; the new folder benchmark and trace run explicitly. Cached dependencies remain release builds during the target debug gate. Doctests are outside these gates.

Frozen originals are compiled beside the actual private/public functions. Differential checks cover all 2,097,152 three-byte ASCII strings; case permutations and fourth-byte mutations of reserved-name prefixes; invalid near names and Unicode; **every Unicode scalar** through the sanitizer; whitespace/control filtering order; all byte-length limits around 48 with multi-byte characters; 8,192 randomized strings; exact output capacities; small/large collision lists, reordered lists and gaps at word boundaries; canonical versus malformed suffixes; and complete exhaustion through suffix 9999. Existing package gates additionally cover profile storage and migration behavior. A supplemental actual-library Cargo test fills a coverage gap in the original malformed-suffix fixture, which returned before the bitset dispatch: it occupies suffixes 2–8, then tests malformed aliases of 9/10, mixed ASCII prefix case, Unicode prefixes and valid suffix 9999 and out-of-range 10000 entries. It passes separately in release and package-debug configurations (one additional test per mode). Its temporary Cargo target is removed and the release workspace manifest restored afterward; it does not alter the primary benchmark sources or four integration targets.

The deterministic trace reruns the primary differential checks and records every scalar’s sanitized bytes/capacity plus collision results at the dispatch boundaries and range exhaustion. No normalization of results is used. The supplemental indexed-parser check has separate test logs and is outside this trace.

- `baseline`: 25,511,931 bytes; SHA-256 `65ab5c8967bdc17ad68fdea1725a4e5cdde986540516661c5b00e5d16d086bbc`.
- `debug`: 25,511,931 bytes; SHA-256 `65ab5c8967bdc17ad68fdea1725a4e5cdde986540516661c5b00e5d16d086bbc`.
- `final`: 25,511,931 bytes; SHA-256 `65ab5c8967bdc17ad68fdea1725a4e5cdde986540516661c5b00e5d16d086bbc`.

Scoped Clippy and production-only release `cargo check --lib` pass. Source/input fingerprints match the measured code. The audit verifies exact production scope, independent frozen reference and held-fixed ablations against Git, all stage archives/source overlays/profiles/compiler/lockfiles/binaries/logs/fixtures/traces, all **840 timing rows**, and every allocation field. Public profile-name comparison functions from the preceding batch remain unchanged.

The first primitive trial reused dirty buffers by shrinking them, which introduced reallocations and slowed those fixtures. That path was withdrawn. The retained ownership fast path applies only when capacity is already exact; dirty/long buffers keep the old copy. Early index experiments exposed a sparse small-gap cost, so suffixes 2–8 are probed before building the index. The first trial’s index ablation also retained a local copied sanitizer; the final ablation resolves the current parent sanitizer explicitly by giving the copy variant a distinct name. Exploratory source/binary/log hashes are archived, and the final Cargo runs use the corrected isolated references.

## Reproduce and artifacts

The driver uses the verified earlier source archive plus the current package dependency closure, applies current overlays, restores required fixtures and archives executables before another build can replace them. The minimal workspace registers the profile package explicitly. All three measured builds use the same compiler and lockfile with `--offline --locked`; dependencies were cached by the preceding batch. Compilation TEMP/TMP uses scratch storage; tests retain the host NTFS temporary directory.

```powershell
$env:DEADSYNC_PROFILE_FOLDERS_ARCHIVE='D:\deadsync-profile-folders-20261104'
python scripts/benchmark_profile_folders.py baseline --baseline 9b0ffa6aae31fbd9535b678a1c9b49b36c416555 --runs 5 --reference baseline
python scripts/benchmark_profile_folders.py debug --debug --reference baseline
python scripts/benchmark_profile_folders.py final --runs 5 --reference baseline
python scripts/check_profile_folder_index.py
```

The drivers, private test/reference/ablation modules, supplemental index-parser test, stage manifests, raw baseline/final TSVs and checks JSON are checked in. Large source snapshots, executables, logs and prototype artifacts remain under `D:\deadsync-profile-folders-20261104`, with SHA-256 values and local prerequisites recorded in the manifests.
