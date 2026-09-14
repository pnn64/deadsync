# Noteskin pack preparation - 0.5.1230

Baseline: `a463bbc40` (0.5.1229). The supplied `rust-performance.md` calls for
measuring CPU and allocation costs (M-HOTPATH), reusing temporary storage
(M-MEM-REUSE), reserving known capacities (M-INITIAL-CAPACITY), and increasing
useful work per CPU cycle (M-THROUGHPUT).

## Changes

1. **Stream selections directly into cache key hashes.** Runtime keys hash the
   same skin, delimiters, slots, and IDs without formatting an intermediate
   selection string. Compiler keys filter borrowed options as they hash,
   removing the cloned selection and BTreeMap. The returned key has one exact
   allocation; the hash and filtering need no temporary heap storage. Existing
   keys, PNG-only filtering, unknown choices, and revision fingerprints match.
2. **Reuse canonical roots during pack validation.** Resolve the pack root once
   and each base root once, then reuse those paths when checking references.
   Every source and target still undergoes canonicalization, containment, file
   type, and duplicate-target checks. Reuse the per-choice target set and reserve
   the skin/choice ID sets from their known sizes. This removes repeated path
   resolution and temporary allocations across large manifests.
3. **Reuse Workshop label buffers and sort within slots.** Borrow path components
   instead of collecting a vector, remove family names into reusable scratch
   storage, and assemble labels in a reusable string. Slugs lowercase characters
   into the output buffer, trim in place, and shrink to avoid retaining unused
   space in the manifest (M-SHRINK-TO-FIT). The grouping map
   already orders slots, so stable label sorting within each slot eliminates
   cloned slot sort keys. Complete manifests, errors, and Unicode IDs match.

These changes affect noteskin selection, installed-pack discovery/loading, and
Workshop manifest compilation. They do not make file I/O, JSON parsing, complete
manifests, or the application allocation-free.

## Results

| Complete operation | Fewer thread cycles | Throughput gain | Allocation/free calls old -> new | Allocated/freed bytes old -> new |
|---|---:|---:|---:|---:|
| Compiler key, 11 options | 63.6% | 2.74x | 26 -> 1 | 1,454 -> 27 |
| Load pack, 512 choices and 4 replacements each | 36.1% | 1.57x | 40,681 -> 31,455 | 7,553,151 -> 6,442,067 |
| Workshop manifest, 1,024 files in nested folders | 30.3% | 1.43x | 25,440 -> 9,446 | 1,467,106 -> 776,424 |

All 17 tested workloads used fewer median thread cycles. The empty Workshop
control has no allocation churn in either implementation.

Shallow Workshop fixtures can grow their two reusable scratch strings once or
twice; the old implementation had no reallocations in those fixtures but made
many more separate allocations. Deep fixtures also shrink IDs to preserve
their exact retained capacity. The complete allocation/free and byte totals
below include those reallocations and the destruction of returned values.

### All workloads

| Workload | ns/op old -> new | Thread cycles/op old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `runtime_key/0` | 380.3 → 142.4 | 834.3 → 312.3 | 62.6% | 2,629,517.9 → 7,023,319.6 |
| `compiler_key/0` | 484.1 → 142.9 | 1,042.7 → 313.5 | 69.9% | 2,065,661.4 → 6,998,120.6 |
| `runtime_key/1` | 653.1 → 184.1 | 1,387.5 → 402.0 | 71.0% | 1,531,157.7 → 5,430,920.2 |
| `compiler_key/1` | 1,000.9 → 332.2 | 2,168.7 → 739.4 | 65.9% | 999,121.9 → 3,010,657.8 |
| `runtime_key/6` | 1,255.7 → 336.2 | 2,715.5 → 737.5 | 72.8% | 796,391.4 → 2,974,150.5 |
| `compiler_key/6` | 2,364.5 → 751.8 | 5,133.1 → 1,624.4 | 68.4% | 422,930.8 → 1,330,172.4 |
| `runtime_key/11` | 1,493.3 → 531.4 | 3,235.7 → 1,164.5 | 64.0% | 669,664.0 → 1,881,920.5 |
| `compiler_key/11` | 3,691.3 → 1,349.1 | 8,047.1 → 2,931.7 | 63.6% | 270,908.4 → 741,236.7 |
| `pack_load/0x0` | 960,637.5 → 754,525.0 | 2,099,901.6 → 1,639,171.1 | 21.9% | 1,041.0 → 1,325.3 |
| `pack_load/1x1` | 1,516,412.5 → 1,119,825.0 | 3,280,427.2 → 2,419,082.0 | 26.3% | 659.5 → 893.0 |
| `pack_load/64x4` | 129,592,625.0 → 80,033,025.0 | 280,987,493.9 → 173,492,252.9 | 38.3% | 493.9 → 799.7 |
| `pack_load/512x4` | 1,013,925,500.0 → 647,142,650.0 | 2,192,739,028.5 → 1,400,497,466.0 | 36.1% | 505.0 → 791.2 |
| `workshop_choices/0x0` | 50.0 → 12.1 | 110.1 → 26.9 | 75.6% | 19,980,487.8 → 82,580,645.2 |
| `workshop_choices/1x0` | 3,687.5 → 2,643.8 | 8,162.6 → 5,871.6 | 28.1% | 542,372.9 → 756,501.2 |
| `workshop_choices/64x0` | 298,300.0 → 226,768.8 | 650,378.5 → 497,085.2 | 23.6% | 429,098.2 → 564,451.7 |
| `workshop_choices/512x0` | 2,532,593.8 → 2,014,368.8 | 5,499,737.0 → 4,378,311.5 | 20.4% | 404,328.6 → 508,347.8 |
| `workshop_choices/512x4` | 3,955,000.0 → 2,759,712.5 | 8,600,462.4 → 5,993,091.1 | 30.3% | 258,912.8 → 371,053.1 |

### Allocation churn

| Workload | Allocations/frees old → new | Reallocations old → new | Allocated/freed bytes old → new |
|---|---:|---:|---:|
| `runtime_key/0` | 2 → 1 | 2 → 0 | 80 → 27 |
| `compiler_key/0` | 3 → 1 | 2 → 0 | 90 → 27 |
| `runtime_key/1` | 2 → 1 | 4 → 0 | 140 → 27 |
| `compiler_key/1` | 6 → 1 | 4 → 0 | 708 → 27 |
| `runtime_key/6` | 2 → 1 | 6 → 0 | 380 → 27 |
| `compiler_key/6` | 16 → 1 | 6 → 0 | 1,045 → 27 |
| `runtime_key/11` | 2 → 1 | 7 → 0 | 700 → 27 |
| `compiler_key/11` | 26 → 1 | 7 → 0 | 1,454 → 27 |
| `pack_load/0x0` | 51 → 47 | 25 → 25 | 167,286 → 166,874 |
| `pack_load/1x1` | 76 → 68 | 35 → 35 | 171,635 → 170,796 |
| `pack_load/64x4` | 5,134 → 3,975 | 2,589 → 2,589 | 1,089,653 → 950,777 |
| `pack_load/512x4` | 40,681 → 31,455 | 20,512 → 20,512 | 7,553,151 → 6,442,067 |
| `workshop_choices/0x0` | 0 → 0 | 0 → 0 | 0 → 0 |
| `workshop_choices/1x0` | 32 → 20 | 0 → 0 | 2,896 → 2,518 |
| `workshop_choices/64x0` | 2,157 → 1,187 | 0 → 1 | 94,745 → 66,275 |
| `workshop_choices/512x0` | 17,248 → 9,446 | 0 → 2 | 779,490 → 546,660 |
| `workshop_choices/512x4` | 25,440 → 9,446 | 2,048 → 1,028 | 1,467,106 → 776,424 |


## Measurement

- Windows x86-64; Intel Xeon E5-2696 v4 at 2.20 GHz, 44 logical processors.
- Rust 1.98.0 (`88d9e12ae`, 2026-08-18), LLVM 22.1.8. Release opt-level 3,
  full LTO, repository defaults.
- Both implementations are compiled into the same release test executable.
  Frozen baseline function bodies were checked against `a463bbc40`, allowing
  only whitespace, test visibility, and the wrapper's `Self` type spelling.
- Five fresh processes per suite, pinned to logical CPU 4 (affinity mask 16).
  Even-numbered processes reverse old/new order. No builds or other tests ran
  during recorded measurements. Each operation uses three warmups and seven
  timed batches; tables report the median of five process medians.
- Key batches contain 4,096 operations; pack loads contain eight (two for the
  512-choice fixture); Workshop batches contain 16 (4,096 for the empty control).
  Fixture path lengths are fixed across processes. Fixtures and input construction are outside timing.
  Complete returned values are destroyed inside the measured operation.
- Windows `QueryThreadCycleTime` measures calling-thread cycles separately from
  elapsed wall time. Scoped allocator counters record allocation/free calls,
  reallocations, and requested/freed bytes in a separate operation outside the
  timing samples. Raw CSV rows retain each process's median, range, and counters.
- Key throughput is keys/s. Pack-load throughput is choices/s (loads/s for the
  empty control). Workshop throughput is input files/s (operations/s for empty).
  Workshop fixtures have two files per option folder and zero or four nested
  folders (512 option folders yield 640 choices because holds split into active
  and inactive slots);
  pack fixtures validate zero to 512 choices with zero to four replacements.
  Replacement paths are shared between choices, as the format permits.
- Filesystem fixtures use a warm OS cache. This measures local preparation
  operations, not cold-disk latency or complete application startup. Requested
  allocation bytes measure churn, not peak RSS; hardware cache misses, energy,
  and whole-application throughput were not measured.

## Behavior checks

- `cargo test -p deadsync-noteskin -p deadsync-assets --locked --no-fail-fast`:
  430 passed, zero failed, 16 ignored (manual benchmarks or optional installed assets).
- The five new behavior tests passed again in release mode; the two new manual
  benchmarks are ignored by ordinary test runs and were each run five times.
- `cargo check --locked` passed for the application.
- `cargo clippy -p deadsync-noteskin --lib --locked -- -D clippy::perf` passed.
  Its 21 non-performance warnings concern unchanged code outside the edited modules.
- Scoped `rustfmt --check`, `git diff --check`, frozen-baseline comparisons,
  source/executable hashes, and the exact one-patch version check passed.
- Ten successful benchmark processes produced 170 old/new measurement rows.
  Paired old/new allocation deltas were identical across all five runs.
  Allocation/free call counts were stable; run 2 of the 64-choice pack case had
  one extra reallocation and eight extra requested/freed bytes in both versions.
  This is consistent with the unchanged fingerprint formatter: a separate
  allocator probe of `format!("{:016x}", value)` uses 16 bytes without leading
  zero padding, or 24 bytes and one reallocation with padding. Fingerprints
  include the per-process fixture path. The raw variation is retained in CSV;
  tables show medians, and this shared eight-byte difference does not change
  the paired improvement.


The new comparisons cover:

- Byte-identical runtime/compiler keys across empty/full selections, all 2,048
  option subsets, unknown skins/choices, Unicode/NUL strings, and hash block
  boundaries. Keys enforce a one-allocation budget for their returned string.
- Complete loaded manifests, canonical root paths, revision fingerprints, and
  error signatures. Cases include duplicate IDs/targets, path aliases, missing
  assets, directories in file slots, invalid schemas/metrics/cells/labels, and
  absolute/parent/backslash/drive paths.
- Internal directory aliases and escaping targets. Windows symlink privilege
  1314 triggers a directory-junction fixture, so containment checks execute
  without requiring elevated privileges.
- Byte-identical Workshop manifests across both families, empty/small/large
  packs, deep paths, unknown categories, shared groups, slug collisions, family
  exclusions, and RGB/DDR metric overrides. Every Unicode scalar is checked in
  ASCII context against the old slug implementation, plus casing and separator
  edge cases. Allocation tests require reduced churn for complete pack loads
  and large Workshop manifests; normalized IDs retain no excess capacity.
- The downstream pack-refresh test now asserts that catalog refresh retains
  atlas pixels on disk. Its former eager-upload expectation was stale: commit
  `fad771795` had already removed that behavior from `refresh_packs`. Catalog
  activation and immutable snapshot checks remain, alongside an atlas-header
  dimension check. No production preview behavior changed in this pass.

## Reproduce

```powershell
cargo test -p deadsync-noteskin -p deadsync-assets --locked --no-fail-fast
cargo test -p deadsync-noteskin --release --lib --locked --no-run
cargo test -p deadsync-noteskin --release --lib --locked benchmark_pack_preparation -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-noteskin --release --lib --locked benchmark_workshop_preparation -- --ignored --nocapture --test-threads=1
```

For the five-process comparison, run each ignored benchmark five times with
affinity mask 16, setting `DEADSYNC_PERF_REVERSE=1` only on even runs. Remove that
environment variable for odd runs. Run regression checks and builds separately
from benchmarks. Raw observations are in
[`pack-preparation-0.5.1230.csv`](pack-preparation-0.5.1230.csv).
