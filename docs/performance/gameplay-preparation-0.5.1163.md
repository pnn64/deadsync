# Gameplay preparation performance - 0.5.1163

Baseline: `0301eb86a` (`0.5.1162`), measured 2026-09-12. This pass increments the workspace patch exactly once to `0.5.1163`, including the three inherited-version entries in Cargo.lock.

## Three changes

The local `rust-performance.md` guidance on profiling CPU and allocation costs (M-HOTPATH), reusing owned storage (M-MEM-REUSE), initial capacity (M-INITIAL-CAPACITY) and avoiding work on discarded items (M-THROUGHPUT) motivated these changes:

1. **Consume the replay input buffer during runtime setup.** `init_gameplay_runtime` passes its owned vector to `build_replay_input_edges_owned`. A consuming filter/map reuses that allocation for the equally sized/aligned recorded-edge type. Per-player offset shifts are computed once. Invalid source times/lanes, saturating arithmetic and stable equal-time ordering are preserved. The borrowed public builder remains available. Ordered conversion, including filtering, performs no allocation or deallocation; out-of-order conversion still uses the standard stable sort and may allocate its scratch storage.
2. **Resume crossover-cue timing searches.** `build_crossover_cues_from_annotations` lazily initializes the existing stack-only `BeatTimeCache` when a cue needs a time. Batches smaller than 16 annotations, timing with no BPM changes, and unsupported BPM boundaries retain independent conversions. `supports_row_time_cache` rejects ambiguous subrow/duplicate and invalid BPM boundaries. Beat rewinds reset the cursor. No cache is created or validated for inputs producing no timing queries. Cue selection, overlap/merge rules, fade arithmetic, ordering and output allocation policy are unchanged.
3. **Skip column masks for discarded cue rows.** The first eligible note determines a row's time and whether the 1.5-second cue-gap rule admits that row. Rejected rows then only advance to the next row instead of checking and inserting every remaining column. The previous time still advances even for discarded rows. A preliminary search finds the first eligible note before allocating the output vector, retaining the old requested initial capacity exactly, including tiny charts. Processing resumes at that note, so the filtered prefix is not rescanned. All-fake or out-of-field input can return with no heap churn.

These paths run during gameplay initialization: replay preparation for loaded recordings, column cues when the profile enables them, and crossover cues from parity annotations when enabled. This is chart preparation work, not a claimed per-frame/audio throughput improvement.

## Behavior and project checks

- 778 gameplay unit tests pass in both debug and release; five manual benchmarks are ignored by ordinary runs, one added here.
- Six new tests cover crossover cue options, exact float-bit/column-mask output, quantization and duration extremes, bracket choices, nonfinite beats and first-visible times, timing events and ambiguous BPMs, reversed annotation order, all note types, partial note ranges, per-column filtering and invalid cached song times.
- Replay comparisons preserve the full edge sequence across invalid lanes/times, filtering, one/two-player and zero-width configurations, offset saturation, duplicate timestamps, reversals and per-player shifts. A playback test drains the prepared old/new recordings in bounded batches and compares events and cursor state, then verifies cursor reset.
- Allocation tests verify no churn for no-cue inputs and identical input/output pointers and capacities for ordered owned replay conversion at sizes up to 65,536 edges, including partially and fully filtered inputs. Input creation and output destruction occur outside this no-churn assertion.
- `cargo test -p deadsync-gameplay --lib --locked`, the equivalent `--release` run, `cargo clippy -p deadsync-gameplay --all-targets --locked -- -D clippy::perf`, and root `cargo check --all-targets --locked` pass. Changed-function/new-test rustfmt checks, diff checks, exact version/lockfile audits, frozen-baseline comparisons and benchmark-source identity checks pass. Existing non-performance style warnings remain.

## Measurement method

Windows x86_64, Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0 (88d9e12ae, LLVM 22.1.8). Release opt-level 3, full LTO. The old replay builder, column-cue builder and crossover-cue entry/core functions are frozen from the parent with only imports/formatting adapted. Old and new execute in the same test binary, with builds and other Cargo activity finished first.

Run `cargo test -p deadsync-gameplay --lib --release --locked preparation_bench -- --ignored --test-threads=1 --nocapture` three times, setting `DEADSYNC_PERF_REVERSE=1` for the middle run. Each case has three warmups and seven timing batches. Tables report the median of the three per-run medians; iteration counts are embedded in the benchmark (8-50,000 depending on workload). Fixture/timing construction is outside measurement. Cue output allocation and destruction are included.

Replay cases clone the input **inside both measured closures**, so both get a fresh owned allocation per operation. The old closure then calls the frozen borrowed builder and drops its source; the new closure consumes its source. Both outputs are dropped inside measurement. Thus raw replay churn includes a fixture clone that production setup already owns; the separate pointer/no-churn tests establish that ordered conversion itself needs no allocation. The measured timings include this shared cloning cost rather than subtracting an estimated cost.

The shared `tests/support/perf.rs` allocator records allocation/reallocation/free calls and requested/freed bytes separately for one operation; counting is disabled during timing. Windows `QueryThreadCycleTime` measures calling-thread cycles, not retired instructions, system-wide CPU, cache misses or peak memory. Byte churn is cumulative requested/freed allocation traffic, not RSS. These warmed synthetic fixtures are not whole-song load benchmarks or physical input latency measurements. Safe Vec collection reuse depends on standard-library specialization and compatible element sizes/alignment; regression tests protect that property on the tested toolchain.

Crossover fixtures alternate inner/outer column annotations at quarter-beat spacing, with occasional bracket flags. Dense timing has a BPM change every eight beats, plus stops, delays, warps and fake segments; the largest case has 4,096 annotations and 128 BPM points. Ambiguous fixtures add duplicate subrow BPMs; reverse fixtures reverse annotations; inactive fixtures require no timing queries. Column fixtures have 4,096 rows with 1, 4 or 16 notes each. Dense rows are 0.1 seconds apart; sparse rows are two seconds apart so all qualify. Ignored columns are fake. Replay fixtures have up to 65,536 edges, eight lanes, four-edge timestamp ties, keyboard/gamepad sources and press/release edges. Filtered inputs contain invalid times/lanes, shifted inputs move player two by -10 ms, and reverse inputs reverse event order.

Throughput units are annotations/s, parsed notes/s or source replay edges/s, including filtered items; empty controls use operations/s.

## CPU and elapsed time

Positive cycle savings mean improvement on this machine and workload.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| cross_empty | 0.0147 | 0.0139 | 32.4 | 30.6 | 5.56% |
| cross_tiny | 0.2412 | 0.2470 | 529.0 | 535.7 | -1.27% |
| cross_simple | 177.4660 | 170.3430 | 388,822.3 | 373,220.2 | 4.01% |
| cross_dense | 27,014.2375 | 298.7875 | 59,223,239.4 | 655,591.5 | 98.89% |
| cross_ambiguous | 1,749.8300 | 1,715.6000 | 3,835,148.0 | 3,761,549.4 | 1.92% |
| cross_reverse | 1,680.0500 | 1,726.5900 | 3,684,044.2 | 3,785,584.9 | -2.76% |
| cross_inactive | 3.8712 | 4.4131 | 8,483.8 | 9,676.5 | -14.06% |
| column_empty | 0.0132 | 0.0125 | 28.8 | 27.4 | 4.86% |
| column_tiny | 0.0685 | 0.0687 | 150.4 | 150.7 | -0.20% |
| column_single | 25.8641 | 22.4785 | 56,702.8 | 49,252.1 | 13.14% |
| column_chords | 73.5694 | 53.5892 | 161,262.3 | 117,220.0 | 27.31% |
| column_wide | 313.4000 | 173.5040 | 686,690.4 | 380,088.4 | 44.65% |
| column_sparse | 79.4600 | 84.0960 | 174,118.4 | 184,222.0 | -5.80% |
| column_ignored | 55.2360 | 47.0302 | 121,060.0 | 103,117.6 | 14.82% |
| replay_empty | 0.0174 | 0.0151 | 38.2 | 33.1 | 13.35% |
| replay_tiny | 0.1348 | 0.0810 | 295.7 | 177.7 | 39.91% |
| replay_ordered | 1,054.0662 | 605.3600 | 2,308,478.7 | 1,326,290.4 | 42.55% |
| replay_filtered | 880.9550 | 570.2788 | 1,930,291.2 | 1,248,891.9 | 35.30% |
| replay_ignored | 411.0540 | 446.4500 | 900,474.6 | 977,231.6 | -8.52% |
| replay_shifted | 3,090.2675 | 2,651.7650 | 6,768,337.3 | 5,808,941.3 | 14.17% |
| replay_reverse | 3,163.3925 | 2,738.1025 | 6,928,775.3 | 5,998,161.3 | 13.43% |

## Allocation churn and throughput

A/R/F means allocation/reallocation/free calls. Byte totals include reallocation traffic, replay fixture clones and returned-output destruction as described above.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| cross_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 67,833,401.2 | 71,777,203.6 |
| cross_tiny | 1/0/1 | 1/0/1 | 48/48 | 48/48 | 16,586,842.5 | 16,191,709.8 |
| cross_simple | 1/0/1 | 1/0/1 | 22,344/22,344 | 22,344/22,344 | 23,080,477.4 | 24,045,602.1 |
| cross_dense | 1/0/1 | 1/0/1 | 22,344/22,344 | 22,344/22,344 | 151,623.8 | 13,708,739.5 |
| cross_ambiguous | 1/0/1 | 1/0/1 | 5,580/5,580 | 5,580/5,580 | 585,199.7 | 596,875.7 |
| cross_reverse | 1/0/1 | 1/0/1 | 5,580/5,580 | 5,580/5,580 | 609,505.7 | 593,076.5 |
| cross_inactive | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 1,058,056,183.4 | 928,135,232.2 |
| column_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 76,045,627.4 | 80,243,941.6 |
| column_tiny | 1/0/1 | 1/0/1 | 12/12 | 12/12 | 14,591,723.6 | 14,556,888.3 |
| column_single | 1/0/1 | 1/0/1 | 768/768 | 768/768 | 158,366,229.6 | 182,218,564.4 |
| column_chords | 1/0/1 | 1/0/1 | 768/768 | 768/768 | 222,701,286.1 | 305,733,244.8 |
| column_wide | 1/0/1 | 1/0/1 | 768/768 | 768/768 | 209,112,954.7 | 377,720,398.4 |
| column_sparse | 1/6/1 | 1/6/1 | 97,536/97,536 | 97,536/97,536 | 206,191,794.6 | 194,824,961.9 |
| column_ignored | 1/0/1 | 0/0/0 | 768/768 | 0/0 | 296,618,147.6 | 348,371,897.2 |
| replay_empty | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 57,431,656.3 | 66,401,062.4 |
| replay_tiny | 2/0/2 | 1/0/1 | 128/128 | 64/64 | 29,672,269.8 | 49,360,777.9 |
| replay_ordered | 2/0/2 | 1/0/1 | 2,097,152/2,097,152 | 1,048,576/1,048,576 | 62,174,460.1 | 108,259,548.0 |
| replay_filtered | 2/0/2 | 1/0/1 | 2,097,152/2,097,152 | 1,048,576/1,048,576 | 74,391,995.1 | 114,919,239.1 |
| replay_ignored | 2/0/2 | 1/0/1 | 2,097,152/2,097,152 | 1,048,576/1,048,576 | 159,434,040.3 | 146,793,593.9 |
| replay_shifted | 3/0/3 | 2/0/2 | 3,145,728/3,145,728 | 2,097,152/2,097,152 | 21,207,225.6 | 24,714,105.5 |
| replay_reverse | 3/0/3 | 2/0/2 | 3,145,728/3,145,728 | 2,097,152/2,097,152 | 20,716,999.2 | 23,934,823.5 |

## Tradeoffs and limits

- `cross_tiny`: 1.27% more cycles; elapsed 241.2 to 247.0 ns/op.
- `cross_reverse`: 2.76% more cycles; elapsed 1680050.0 to 1726590.0 ns/op.
- `cross_inactive`: 14.06% more cycles; elapsed 3871.2 to 4413.1 ns/op.
- `column_tiny`: 0.20% more cycles; elapsed 68.5 to 68.7 ns/op.
- `column_sparse`: 5.80% more cycles; elapsed 79460.0 to 84096.0 ns/op.
- `replay_ignored`: 8.52% more cycles; elapsed 411054.0 to 446450.0 ns/op.

Per-round paired cycle savings for representative cases:

- `cross_dense`: 98.80%, 98.89%, 98.91%.
- `column_chords`: 31.43%, 27.31%, 19.96%.
- `column_ignored`: 13.25%, 13.69%, 18.59%.
- `replay_ordered`: 44.93%, 42.58%, 39.23%.
- `replay_filtered`: 38.66%, 33.99%, 31.43%.

Timing-cache setup and the column row-admission branch can add work on tiny inputs, unsupported timing and sparse rows whose cues all qualify. Timing with many stops but no BPM changes deliberately retains independent queries; it is not covered by the cursor optimization. Output cues still own storage and can grow. Replay sorting can still allocate, and filtered replay output retains the original capacity just as the old builder reserved the full source length. The benefit of owned conversion is avoiding the second buffer and its copy/allocation traffic; it does not remove the storage needed to retain the replay. No unsafe layout conversion, new dependency, persistent timing cache or file-format change is introduced.

Raw runs, per-run medians, source hashes, commands and logs remain in ignored `target/gameplay-preparation-perf/`. Committed tests and frozen baselines reproduce the fixtures and comparisons. All four user-excluded files remain outside the commit.
