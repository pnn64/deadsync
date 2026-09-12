# Chart modifier performance - 0.5.1168

Baseline: `76d58fdc9` (`0.5.1167`), measured 2026-09-12. This pass bumps the patch exactly once to `0.5.1168`, including all three inherited package versions in Cargo.lock.

## Three optimizations

The local `rust-performance.md` recommendations on CPU/allocation measurement (M-HOTPATH), batching work (M-THROUGHPUT), reusing memory (M-MEM-REUSE), and useful capacity reservations (M-INITIAL-CAPACITY) inform these changes.

1. **Batch intelligent tap insertion.** Big, Quick, BMRize and Skippy use nonoverlapping insertion windows. For unique sorted cells, successful additions are appended to the owned chart and sorted once, replacing a tail move per inserted note. Reservations are deferred until an addition qualifies. Each new tap lies inside an otherwise empty player-row interval with no active hold crossing it, before the next candidate; original notes therefore suffice for subsequent endpoint, spacing and hold queries. Overlapping windows, duplicate cells, endpoint insertions and other compatibility cases retain the parent implementation.
2. **Summarize generated mines by lane.** A stack array records the largest appended mine row per ordinary column. It answers collision queries immediately when all prior mines are behind the queried interval or the newest mine is inside it. Backward hold ends whose interval precedes the largest row, and columns outside the fixed lane domain, use the original complete scan. Source/context searches, periodic tap-row conversion, output order and allocation policy are unchanged. No heap index or persistent cache is added.
3. **Filter simple attack windows in place.** Little, NoMines, NoHolds, NoFakes, NoLifts, NoRolls and HoldsToRolls can act independently on each note when insertion and simultaneous-note/hold-creation masks are absent. Their filters and conversions are fused into one pass over the attacked interval. Retained notes are compacted in the existing vector, and its unaffected tail moves once if necessary. Turn modifiers run afterward in the same order as before. This removes the temporary attacked-note buffer, its copies, and its allocation. Interdependent masks keep the existing scratch-buffer path.

These operations run during chart preparation and attack application. Results concern those operations, not whole-game frame rate or input/audio latency. All production changes use safe Rust and add no dependencies or file-format changes.

## Behavior and project checks

- 791 gameplay tests pass in debug and release; seven manual benchmarks are ignored by ordinary runs.
- Seven new tests compare full note output against frozen parent implementations. Coverage includes known inserted cells, missing timing rows, nonoverlapping/overlapping/endpoint windows, fake notes, holds/rolls, judgment metadata, foreign lanes, duplicates, reversed charts, backward and saturated hold ends, collision context, every combination of the five independent removal masks and two hold-conversion masks, five turn modes, partial/empty/reversed attack bounds, repeated attacks, and interdependent modifier combinations.
- Explicit allocation tests verify that productive preallocated intelligent and mine transforms do not allocate, reallocate or free memory. Dense intelligent input needs no reservation. Simple attack filters retain their source pointer and capacity and have no heap churn, including repeated windows. A conversion-order assertion verifies that NoRolls followed by NoHolds produces taps even when HoldsToRolls is also set.
- Tests honor the parent's sorted-context requirement for mine insertion and row-sorted requirement for simultaneous-note limits. Reversed-input comparisons exercise combinations that accept that input shape.
- `cargo test -p deadsync-gameplay --lib --locked`, its `--release` equivalent, `cargo clippy -p deadsync-gameplay --all-targets --locked -- -D clippy::perf`, and root `cargo check --all-targets --offline` pass. Existing unrelated style warnings remain.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (LLVM 22.1.8). Release opt-level 3 with full LTO. Frozen dispatchers and implementations live in `crates/deadsync-gameplay/tests/perf/chart_modifiers/baseline.rs`; their other shared helpers are unchanged from the parent. The frozen mask dispatcher also calls the frozen intelligent/mine transforms, so composed comparisons retain the old behavior throughout those paths.

Reproduce with:

```text
cargo test -p deadsync-gameplay --lib --release --locked chart_modifier_bench -- --ignored --test-threads=1 --nocapture
```

Run three times, setting `DEADSYNC_PERF_REVERSE=1` for the middle run and unsetting it for the others. Builds and other Cargo activity finish before timing. Each old/new pair executes in the same binary with three warmups, seven timing batches, and a separately allocation-counted operation. Tables report the median of the three per-run medians. The test defines iteration counts from 10 to 20,000 according to workload.

Timing data and fixture construction are outside measurement. Both measured closures clone the same input into a fresh owned vector, transform it, and destroy the output; allocation traffic includes that common input clone. Separate no-churn tests measure the transformation alone. The benchmark asserts full output equality before every pair.

Intelligent fixtures cycle four lanes at one-beat spacing for Big/Skippy and half-beat spacing for Quick; dense input uses quarter-beat spacing and admits no additions. The generic overlapping-window control retains the old path. Mine fixtures alternate holds and rolls in one column every four beats with one-beat holds; backward fixtures reverse the tail-row sequence, and foreign fixtures use a column outside MAX_COLS. Attack fixtures cycle taps, lifts, holds, rolls, mines and fakes, with occasional fake/unjudgable flags and missing hold data; the middle-window fixture attacks only rows 40,000 through 50,000. The simultaneous-note control retains the old scratch path. The 4,096-note fixtures are stress workloads; smaller and inactive controls are included.

Throughput counts source notes/s (operations/s for empty inputs). `QueryThreadCycleTime` measures calling-thread CPU cycles, not retired instructions or timestamp-counter ticks. The shared allocator reports calls and cumulative requested/freed bytes, including reallocations. These byte totals are allocation traffic, not RSS, peak live memory, or measured memory-copy bandwidth. Allocation counting is disabled during timing. These are warmed, single-threaded synthetic benchmarks.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| intelligent_empty | 0.0127 | 0.0123 | 28.0 | 27.1 | 3.21% |
| intelligent_tiny | 0.4895 | 0.4252 | 1,073.0 | 932.7 | 13.08% |
| big_512 | 584.8050 | 162.1490 | 1,281,307.2 | 355,359.5 | 72.27% |
| big_4096 | 43,897.8700 | 1,528.3600 | 96,190,716.9 | 3,349,526.3 | 96.52% |
| quick_4096 | 22,108.3100 | 1,071.4200 | 48,454,098.6 | 2,346,476.9 | 95.16% |
| skippy_4096 | 44,171.0700 | 1,290.5900 | 96,807,228.1 | 2,827,269.9 | 97.08% |
| intelligent_dense | 420.4590 | 319.9170 | 921,785.8 | 701,162.0 | 23.93% |
| intelligent_overlap | 529.4500 | 530.7190 | 1,160,356.0 | 1,163,292.9 | -0.25% |
| mines_empty | 0.0081 | 0.0108 | 17.9 | 23.7 | -32.40% |
| mines_tiny | 0.3186 | 0.3182 | 698.9 | 698.0 | 0.13% |
| mines_512 | 139.8800 | 61.8740 | 306,582.2 | 135,697.1 | 55.74% |
| mines_4096 | 17,419.4700 | 769.1900 | 38,176,690.9 | 1,686,374.6 | 95.58% |
| mines_backward | 152.1290 | 154.6240 | 333,442.5 | 338,767.5 | -1.60% |
| mines_foreign | 131.9910 | 133.2620 | 289,290.0 | 291,967.9 | -0.93% |
| attack_empty | 0.0253 | 0.0250 | 55.6 | 54.8 | 1.44% |
| attack_tiny | 0.2808 | 0.1453 | 615.7 | 318.8 | 48.22% |
| attack_nomines | 241.4490 | 131.6270 | 529,093.8 | 288,133.3 | 45.54% |
| attack_noholds | 225.9340 | 99.7490 | 495,167.9 | 218,606.6 | 55.85% |
| attack_filter | 203.4090 | 120.1380 | 445,826.4 | 263,332.0 | 40.93% |
| attack_middle | 115.3310 | 92.9570 | 252,760.8 | 203,834.3 | 19.36% |
| attack_simultaneous | 266.9730 | 267.5290 | 585,050.9 | 586,262.5 | -0.21% |

A/R/F means allocation/reallocation/free calls. Byte totals include reallocations and the shared input clone.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| intelligent_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 78,585,461.7 | 81,366,965.0 |
| intelligent_tiny | 1/1/1 | 1/1/1 | 1,440/1,440 | 1,440/1,440 | 8,172,271.5 | 9,408,001.5 |
| big_512 | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 875,505.5 | 3,157,589.6 |
| big_4096 | 1/1/1 | 1/1/1 | 1,474,560/1,474,560 | 1,474,560/1,474,560 | 93,307.5 | 2,679,996.9 |
| quick_4096 | 1/1/1 | 1/1/1 | 1,474,560/1,474,560 | 1,474,560/1,474,560 | 185,269.7 | 3,822,963.9 |
| skippy_4096 | 1/1/1 | 1/1/1 | 1,474,560/1,474,560 | 1,474,560/1,474,560 | 92,730.4 | 3,173,742.2 |
| intelligent_dense | 1/1/1 | 1/0/1 | 1,474,560/1,474,560 | 491,520/491,520 | 9,741,734.6 | 12,803,320.9 |
| intelligent_overlap | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 967,041.3 | 964,729.0 |
| mines_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 123,001,230.0 | 93,023,255.8 |
| mines_tiny | 1/1/1 | 1/1/1 | 1,440/1,440 | 1,440/1,440 | 12,556,898.4 | 12,570,710.2 |
| mines_512 | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 3,660,280.2 | 8,274,881.2 |
| mines_4096 | 1/1/1 | 1/1/1 | 1,474,560/1,474,560 | 1,474,560/1,474,560 | 235,139.2 | 5,325,082.2 |
| mines_backward | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 3,365,564.8 | 3,311,258.3 |
| mines_foreign | 1/1/1 | 1/1/1 | 184,320/184,320 | 184,320/184,320 | 3,879,052.4 | 3,842,055.5 |
| attack_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 39,541,320.7 | 40,072,129.8 |
| attack_tiny | 2/0/2 | 1/0/1 | 960/960 | 480/480 | 14,242,985.3 | 27,525,461.1 |
| attack_nomines | 2/0/2 | 1/0/1 | 983,040/983,040 | 491,520/491,520 | 16,964,245.0 | 31,118,235.6 |
| attack_noholds | 2/0/2 | 1/0/1 | 983,040/983,040 | 491,520/491,520 | 18,129,188.2 | 41,063,068.3 |
| attack_filter | 2/0/2 | 1/0/1 | 983,040/983,040 | 491,520/491,520 | 20,136,768.8 | 34,094,125.1 |
| attack_middle | 2/0/2 | 1/0/1 | 541,560/541,560 | 491,520/491,520 | 35,515,169.4 | 44,063,384.1 |
| attack_simultaneous | 2/0/2 | 2/0/2 | 983,040/983,040 | 983,040/983,040 | 15,342,375.4 | 15,310,489.7 |

Per-run paired cycle savings for representative productive cases:

- `big_4096`: 96.52%, 96.30%, 96.52%.
- `mines_4096`: 95.58%, 94.83%, 95.66%.
- `attack_nomines`: 40.21%, 44.25%, 47.39%.

Cases with higher median cycle counts are reported explicitly:

- `intelligent_overlap`: 0.25% more cycles; elapsed 529450.0 to 530719.0 ns/op.
- `mines_empty`: 32.40% more cycles; elapsed 8.1 to 10.8 ns/op.
- `mines_backward`: 1.60% more cycles; elapsed 152129.0 to 154624.0 ns/op.
- `mines_foreign`: 0.93% more cycles; elapsed 131991.0 to 133262.0 ns/op.
- `attack_simultaneous`: 0.21% more cycles; elapsed 266973.0 to 267529.0 ns/op.

## Tradeoffs and limits

Intelligent insertion still sorts the completed chart and grows its vector when required; its productive allocation budgets are unchanged. The mine summary adds a fixed stack array and does not reduce the output storage needed for mines. Backward/foreign mine cases may pay for summary checks before the old scan. Simple attack compaction copies retained Note values after the first removal, but avoids scratch storage and repeated tail shifts. Unsupported fast-path shapes retain the parent algorithms and their allocation policies.

Raw runs, source audits and machine-local scripts remain under ignored `target/chart-modifier-perf/`. The committed tests and frozen baselines reproduce the comparisons. The four excluded files remain outside the commit.
