# Keymap preparation performance - 0.5.1152

This pass follows `M-HOTPATH`, `M-MEM-REUSE`, and `M-THROUGHPUT` from the supplied
`rust-performance.md`: avoid short-lived strings and collections, repeated
hashing, and copies on paths that can borrow or reuse existing storage. The
changes affect input configuration loading, editing, change detection, and
persistence. These measurements do not claim a gameplay FPS improvement.

## Three changes

1. **Repair default bindings only when needed.** Restoration borrows each action's
   binding list and uses the existing keyboard reverse index to check whether a
   default key is already taken. Already-correct or unavailable defaults allocate
   nothing. Actions that need repair share a single lazily allocated scratch
   vector. The existing action order, default priority, duplicates, and conflicts
   remain unchanged; reverse mappings are still updated through `Keymap::bind`.
2. **Compare bindings directly when detecting edits.** `Keymap::has_same_bindings`
   compares ordered binding slices instead of formatting both maps as INI lines.
   The configuration publisher uses this method. Missing and explicitly empty
   action lists compare equally, while order, duplicate entries, device filters,
   UUID filters, and binding variants remain significant. Reverse-index storage
   is intentionally excluded, just as it was by serialized comparison.
3. **Write INI tokens into their destination.** Section writing appends names,
   separators, and formatted bindings directly into the caller's `String`.
   It no longer creates a vector of lines, a vector of token strings for each
   action, and joined copies. The owned-line API shares the direct list writer,
   and the existing gamepad formatter now supports appending to a destination.
   A sufficiently sized caller buffer needs no allocations or growth. Fresh
   output strings and owned line lists still allocate when storage is needed.

No dependencies, unsafe code, or global caches were added. Existing public token
and INI formats remain byte-for-byte compatible.

## Method and fixtures

Windows x86-64, Intel Xeon E5-2696 v4, rustc 1.98.0, repository release profile
(opt-level 3, full LTO). Twelve baseline function bodies are frozen from
`9b46fda7b` / 0.5.1151 and were checked against that commit ignoring whitespace.
Unchanged parsing, binding-update utilities, defaults, and data types are shared.
The comparison baseline reproduces the former serialized equality expression.
Both implementations run in the same executable with identical fixtures and
opaque function dispatch/inputs. The test allocator delegates to `System`.

Each workload has three warmups and seven timed batches. Three invocations use
old/new, new/old, old/new order. Tables report medians of the three invocation
medians. Counters are disabled during timing and enabled for a separate complete
operation. CPU cycles are calling-thread cycles from Windows
`QueryThreadCycleTime`; all measured work is synchronous on that thread. No
builds or other tests ran during benchmarking. Source hashes were checked before
and after the three invocations and after validation.

Four fixture families cover an empty keymap, built-in defaults, a mixed map with
four bindings per action, and a larger map with sixteen bindings per action.
Mixed bindings include keyboard keys, wildcard and device-specific directions,
and raw gamepad codes with optional device and UUID filters. All 32 actions are
represented. Early changes replace the first action; late changes replace the
last action in serialized order. Equal maps are distinct clones.

Warmed section writing clears and reuses a buffer sized before timing; cold
writing starts with a new `String`. Owned-line output and cold sections are
destroyed inside each measured operation. Stable restoration starts with defaults
already repaired, including for the originally empty/mixed fixtures. Fresh
restoration includes cloning the input map, repairing it, and destroying it.
The complete keyboard-edit benchmark builds a replacement map and restores its
defaults. The loading benchmark parses the mixed map's INI entries and fills
missing defaults. These two benchmarks exclude publication and disk I/O; they
also exclude the separate change-detection comparison.

Most batches contain 256 operations. Fresh restoration, loading, and editing
contain 128. Writer throughput counts output bytes/second; owned-line throughput
counts action lines/second; comparison, restoration, loading, and editing count
complete operations/second.

Allocation and free call counts match for every workload, as do requested and
freed bytes. Reallocations are reported separately. Byte traffic includes each
full reallocation request and measures allocator churn, not peak live memory,
RSS, or allocator metadata.

## CPU time, cycles, and throughput

Each paired cell is old / new. Negative cycle changes indicate improvement.

| Workload | ns/op, old / new | Cycles/op, old / new | Cycle change | Throughput/s, old / new |
| --- | ---: | ---: | ---: | ---: |
| write_warm_empty | 1,914.8 / 263.7 | 4,210.8 / 586.5 | -86.07% | 193,227,254 / 1,403,259,259 |
| write_cold_empty | 2,862.9 / 884.4 | 6,290.9 / 1,947.2 | -69.05% | 129,240,006 / 418,374,558 |
| lines_empty | 459.0 / 565.2 | 1,012.6 / 1,245.8 | +23.03% | 69,719,149 / 56,613,684 |
| compare_late_changed_empty | 1,538.7 / 506.2 | 3,388.5 / 1,116.4 | -67.05% | 649,911 / 1,975,309 |
| compare_equal_empty | 1,157.4 / 59.8 | 2,547.4 / 135.5 | -94.68% | 863,989 / 16,732,026 |
| compare_changed_empty | 1,557.4 / 260.2 | 3,401.4 / 577.0 | -83.04% | 642,087 / 3,843,844 |
| restore_stable_empty | 2,608.2 / 463.3 | 5,587.0 / 1,022.0 | -81.71% | 383,406 / 2,158,516 |
| restore_fresh_empty | 21,740.6 / 6,811.7 | 47,701.8 / 14,963.7 | -68.63% | 45,997 / 146,806 |
| write_warm_default | 7,620.7 / 1,315.2 | 16,713.7 / 2,892.9 | -82.69% | 100,515,659 / 582,405,702 |
| write_cold_default | 8,899.2 / 2,116.4 | 19,506.3 / 4,612.9 | -76.35% | 86,074,972 / 361,934,293 |
| lines_default | 6,016.4 / 4,863.7 | 13,197.4 / 10,644.0 | -19.35% | 5,318,790 / 6,579,391 |
| compare_late_changed_default | 12,910.2 / 1,016.0 | 28,312.1 / 2,235.3 | -92.10% | 77,458 / 984,237 |
| compare_equal_default | 11,954.3 / 1,296.9 | 26,162.5 / 2,852.6 | -89.10% | 83,652 / 771,084 |
| compare_changed_default | 12,107.4 / 382.8 | 26,550.9 / 844.6 | -96.82% | 82,594 / 2,612,245 |
| restore_stable_default | 2,225.8 / 478.1 | 4,895.0 / 1,055.5 | -78.44% | 449,280 / 2,091,503 |
| restore_fresh_default | 6,008.6 / 4,778.1 | 13,159.7 / 10,501.7 | -20.20% | 166,428 / 209,287 |
| write_warm_mixed | 30,846.5 / 12,600.4 | 67,323.9 / 27,650.1 | -58.93% | 93,949,118 / 229,992,870 |
| write_cold_mixed | 32,086.7 / 14,829.7 | 69,988.8 / 32,522.9 | -53.53% | 90,317,742 / 195,418,818 |
| lines_mixed | 32,397.3 / 27,842.2 | 71,000.5 / 61,049.3 | -14.02% | 987,738 / 1,149,335 |
| compare_late_changed_mixed | 59,669.5 / 1,298.8 | 130,545.9 / 2,859.5 | -97.81% | 16,759 / 769,925 |
| compare_equal_mixed | 59,860.2 / 1,347.3 | 130,996.9 / 2,962.4 | -97.74% | 16,706 / 742,244 |
| compare_changed_mixed | 57,832.0 / 496.5 | 126,532.3 / 1,094.9 | -99.13% | 17,292 / 2,014,162 |
| restore_stable_mixed | 5,419.9 / 480.9 | 11,898.4 / 1,059.8 | -91.09% | 184,504 / 2,079,610 |
| restore_fresh_mixed | 83,990.6 / 18,843.8 | 184,186.2 / 41,314.0 | -77.57% | 11,906 / 53,068 |
| write_warm_large | 100,946.1 / 44,102.3 | 221,029.6 / 96,291.9 | -56.43% | 100,192,089 / 229,330,216 |
| write_cold_large | 100,205.5 / 45,750.0 | 219,237.6 / 100,102.3 | -54.34% | 100,932,615 / 221,071,038 |
| lines_large | 98,185.2 / 66,221.5 | 215,175.2 / 145,171.0 | -32.53% | 325,915 / 483,227 |
| compare_late_changed_large | 193,594.1 / 2,399.6 | 423,614.4 / 5,280.0 | -98.75% | 5,165 / 416,734 |
| compare_equal_large | 198,398.0 / 2,629.7 | 434,284.2 / 5,778.2 | -98.67% | 5,040 / 380,273 |
| compare_changed_large | 189,951.6 / 690.2 | 415,989.4 / 1,520.2 | -99.63% | 5,264 / 1,448,783 |
| restore_stable_large | 15,001.6 / 650.0 | 32,900.1 / 1,433.6 | -95.64% | 66,660 / 1,538,462 |
| restore_fresh_large | 270,029.7 / 47,989.1 | 590,789.4 / 104,608.9 | -82.29% | 3,703 / 20,838 |
| edit_keyboard | 92,372.7 / 30,403.9 | 202,595.1 / 66,666.3 | -67.09% | 10,826 / 32,890 |
| load_mixed | 108,760.9 / 46,451.6 | 237,826.5 / 101,856.6 | -57.17% | 9,194 / 21,528 |

## Allocation traffic

Each paired cell is old / new. Allocations equal frees; requested bytes equal
freed bytes in all these operations.

| Workload | Allocations (= frees) | Reallocations | Requested (= freed) bytes |
| --- | ---: | ---: | ---: |
| write_warm_empty | 1 / 0 | 0 / 0 | 1,280 / 0 |
| write_cold_empty | 2 / 1 | 6 / 6 | 2,550 / 1,270 |
| lines_empty | 1 / 1 | 0 / 0 | 1,280 / 1,280 |
| compare_late_changed_empty | 5 / 0 | 0 / 0 | 2,686 / 0 |
| compare_equal_empty | 2 / 0 | 0 / 0 | 2,560 / 0 |
| compare_changed_empty | 5 / 0 | 0 / 0 | 2,686 / 0 |
| restore_stable_empty | 20 / 0 | 0 / 0 | 3,200 / 0 |
| restore_fresh_empty | 65 / 46 | 0 / 0 | 10,860 / 7,820 |
| write_warm_default | 66 / 0 | 4 / 0 | 4,190 / 0 |
| write_cold_default | 67 / 1 | 11 / 7 | 6,740 / 2,550 |
| lines_default | 66 / 21 | 4 / 25 | 4,190 / 2,010 |
| compare_late_changed_default | 135 / 0 | 8 / 0 | 8,506 / 0 |
| compare_equal_default | 132 / 0 | 8 / 0 | 8,380 / 0 |
| compare_changed_default | 131 / 0 | 8 / 0 | 8,344 / 0 |
| restore_stable_default | 20 / 0 | 0 / 0 | 3,200 / 0 |
| restore_fresh_default | 67 / 47 | 0 / 0 | 9,953 / 6,753 |
| write_warm_mixed | 193 / 0 | 0 / 0 | 9,776 / 0 |
| write_cold_mixed | 194 / 1 | 7 / 9 | 16,925 / 10,230 |
| lines_mixed | 193 / 33 | 0 / 112 | 9,776 / 7,912 |
| compare_late_changed_mixed | 383 / 0 | 0 / 0 | 19,377 / 0 |
| compare_equal_mixed | 386 / 0 | 0 / 0 | 19,552 / 0 |
| compare_changed_mixed | 383 / 0 | 0 / 0 | 19,381 / 0 |
| restore_stable_mixed | 20 / 0 | 20 / 0 | 9,600 / 0 |
| restore_fresh_mixed | 159 / 140 | 20 / 0 | 32,712 / 23,312 |
| write_warm_large | 577 / 0 | 64 / 0 | 43,608 / 0 |
| write_cold_large | 578 / 1 | 71 / 10 | 64,806 / 20,470 |
| lines_large | 577 / 33 | 64 / 192 | 43,608 / 36,840 |
| compare_late_changed_large | 1139 / 0 | 126 / 0 | 85,976 / 0 |
| compare_equal_large | 1154 / 0 | 128 / 0 | 87,216 / 0 |
| compare_changed_large | 1139 / 0 | 126 / 0 | 85,994 / 0 |
| restore_stable_large | 20 / 0 | 60 / 0 | 48,000 / 0 |
| restore_fresh_large | 327 / 308 | 60 / 0 | 117,120 / 69,800 |
| edit_keyboard | 197 / 178 | 20 / 1 | 45,108 / 36,028 |
| load_mixed | 250 / 231 | 21 / 1 | 53,848 / 44,448 |

## Interpretation and limits

For built-in defaults, unchanged restoration removes 20 allocations/frees and
3,200 bytes of churn, using 78.4% fewer CPU cycles. Equal-map change detection
removes 132 allocations/frees plus eight reallocations and 8,380 bytes of churn,
using 89.1% fewer cycles. Warmed INI writing removes 66 allocations/frees plus four
reallocations and 4,190 bytes of churn, using 82.7% fewer cycles. All three paths
have zero measured allocation traffic after the change.

Complete mixed-keymap loading uses 57.2% fewer cycles and reduces growth from
21 reallocations to one. Complete keyboard editing uses 67.1% fewer cycles and
reduces growth from twenty reallocations to one. These measurements include the
remaining map construction and binding copies; those operations are not claimed
to be allocation-free.

One isolated case regressed: materializing owned lines for a completely empty
keymap increased from 459.0 to 565.2 ns (23.0% more cycles), with the same single
1,280-byte allocation. Empty section writing and empty-map comparison both
improve substantially because they bypass that owned-line materialization.
The other 33 paired workloads improved in the three-run medians. Results vary
with machine, allocator, hash-map seeds, and workload; the zero-churn assertions
are deterministic, while timing percentages are measurements rather than a
universal speed guarantee. Cold output strings still grow, and serialization
still spends CPU time formatting device IDs and UUIDs.

## Validation

- Input debug and release suites: 94 tests passed in each mode; two manual
  benchmarks ignored in each.
- Configuration suite: 230 tests passed.
- Gameplay suite: 772 tests passed; four manual benchmarks ignored.
- Clippy for input and configuration, all targets, with `-D clippy::perf`: passed;
  existing style warnings remain.
- `cargo check -p deadsync --all-targets --locked`: passed. The first attempt
  exhausted disk space; cleaning only generated development artifacts for the
  Simply Love theme freed space and the unchanged sources passed on retry.
- All 34 benchmark pairs completed three invocations with unchanged source hashes.

The four added regression tests exercise all binding variants, exact INI output
including append prefixes and blank lines, maximum numeric device/code values,
UUIDs, duplicates and ordering, missing versus explicitly empty actions, conflicts
and default-priority restoration, repeated restoration, malformed/duplicate INI
entries, and keyboard/gamepad edits and clears. Parity checks compare serialized
bindings, reverse keyboard queries, mapped keyboard events, and raw-pad match
results. Allocation assertions cover direct comparisons, already-correct default
restoration, and writing into a prepared output buffer.

The workspace patch increases exactly once from 0.5.1151 to 0.5.1152. `Cargo.toml`
and the three corresponding lockfile package versions are updated.

Reproduce the benchmark with:

```powershell
cargo test -p deadsync-input --release --locked keymap_preparation_bench -- --ignored --nocapture --test-threads=1
```

Run three times, setting `DEADSYNC_PERF_REVERSE=1` only for the middle invocation
and removing it for the others. The shared harness reports sample ranges as well
as medians, thread cycles, throughput, allocations, reallocations, frees, and bytes.
