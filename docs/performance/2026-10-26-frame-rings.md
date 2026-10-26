# Frame diagnostic rings - 2026-10-26

Three measured optimizations in `deadsync-config::frame_pacing`, following `rust-performance.md`. Production changes are confined to three existing method bodies: 22 lines added and nine removed. No public API, type, dependency, or unsafe operation is added. Lua changes and unrelated worktree edits are excluded.

## Execution simplified

1. **Wrap the five-entry stutter cursor directly.** Replace constant remainder with increment and reset at five. The sample write and float calculations are unchanged.
2. **Collect recent history from two contiguous spans.** Replace the per-sample modular index with iteration over the physical tail and head of the ring. The start calculation, filtering, callback calls, and individual output pushes remain unchanged; there is no intermediate collection.
3. **Specialize generic cursor wrapping by capacity.** Power-of-two capacities use a mask; other capacities increment and reset at the bound. The const-generic choice folds during compilation. The zero-capacity return and saturating length update are unchanged. The preceding checked array write establishes `cursor < N`, so advancing cannot overflow and reaches at most `N`.

## Isolated measurements

All measurements use the default release profile, including LTO. Consecutive stages compare original -> stutter cursor -> recent-history spans -> generic cursor, keeping the final benchmark harness fixed. Each comparison uses eight fresh processes per version. CPU throughput is before cycles divided by after cycles; paired ranges are minimum/maximum corresponding-process ratios, not confidence intervals.

| Change / fixture | Before cycles | After cycles | CPU throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| Stutter cursor / 8,192 pushes | 87,436.44 | 65,825.99 | 1.328x | 1.232-1.630x |
| Recent spans / 128 scalar entries | 417.99 | 383.27 | 1.091x | 1.026-1.284x |
| Recent spans / 129 scalar entries | 753.67 | 357.12 | 2.110x | 1.948-2.774x |
| Recent spans / 128 wide entries | 2,865.93 | 2,759.19 | 1.039x | 0.907-1.266x |
| Generic cursor / 8,192 scalar pushes, capacity 129 | 86,777.96 | 48,190.55 | 1.801x | 1.721-2.350x |
| Generic cursor / 8,192 wide pushes, capacity 129 | 311,175.55 | 300,169.67 | 1.037x | 0.967-1.120x |
| Generic cursor / 8,192 scalar pushes, capacity 128 | 44,301.26 | 45,745.16 | 0.968x | 0.915-1.090x |
| Generic cursor / 8,192 wide pushes, capacity 128 | 303,863.47 | 299,847.31 | 1.013x | 0.971-1.209x |
| Generic cursor / new ring and 64 scalar pushes | 379.41 | 391.19 | 0.970x | 0.780-1.304x |

Raw data retain every workload and control: [stutter cursor](2026-10-26-ring-push.tsv), [recent-history spans](2026-10-26-ring-recent.tsv), and [generic cursor](2026-10-26-ring-wrap.tsv). In the stutter-only comparison, unchanged capacity-129 collection slows from 620.78 to 711.23 cycles (0.873x, paired 0.710-0.887x). The later span change recovers this loss in the combined comparison. The data do not establish the cause of changes in untouched methods.

## Combined measurements and limitations

The original/final comparisons use twelve fresh processes per version. [Collection and stutter data](2026-10-26-ring-final.tsv) and [history insertion data](2026-10-26-ring-history-final.tsv) include the following complete set of workloads.

| Fixture | Before cycles | After cycles | CPU throughput | Before -> after ns |
| --- | ---: | ---: | ---: | ---: |
| 8,192 stutter pushes | 90,914.15 | 68,926.42 | 1.319x | 41,423.44 -> 31,475.00 |
| Recent 128 scalar entries | 411.99 | 388.84 | 1.060x | 184.96 -> 174.81 |
| Recent 129 scalar entries | 629.35 | 357.55 | 1.760x | 284.18 -> 160.55 |
| Recent 128 wide entries | 3,116.72 | 2,874.07 | 1.084x | 1,417.19 -> 1,306.84 |
| 8,192 scalar pushes, capacity 128 | 47,775.54 | 49,449.22 | 0.966x | 21,776.57 -> 22,564.06 |
| 8,192 wide pushes, capacity 128 | 334,562.58 | 341,003.52 | 0.981x | 152,687.50 -> 155,576.56 |
| 8,192 scalar pushes, capacity 129 | 97,413.39 | 55,759.85 | 1.747x | 44,420.31 -> 25,515.62 |
| 8,192 wide pushes, capacity 129 | 338,715.93 | 330,700.75 | 1.024x | 154,434.38 -> 150,823.43 |
| New ring and 64 scalar pushes | 408.13 | 410.06 | 0.995x | 184.66 -> 185.64 |
| Visible / 512 empty rings | 12,312.58 | 12,082.78 | 1.019x | 5,589.07 -> 5,493.75 |
| Visible / 512 populated rings | 24,189.58 | 24,937.25 | 0.970x | 11,000.00 -> 11,362.50 |
| Visible / 512 filtered rings | 13,622.70 | 14,620.75 | 0.932x | 6,184.38 -> 6,640.63 |
| Visible / 512 mixed rings | 17,988.72 | 17,947.54 | 1.002x | 8,210.94 -> 8,157.81 |
| Snapshot / 128 scalar entries | 69.88 | 71.16 | 0.982x | 29.69 -> 29.88 |
| Snapshot / 128 wide entries | 2,222.87 | 2,246.02 | 0.990x | 1,011.13 -> 1,020.70 |

All fixtures report **zero allocations, reallocations, frees, requested/freed bytes, and peak bytes before and after**, with caller output buffers warmed. These are CPU improvements; no allocation reduction is claimed. The five TSVs retain all **760 process/workload records**, elapsed time, allocator counters, and executable SHA-256 values.

The shell's frame-statistics and stutter-diagnostic histories currently use **capacity 128** (`frame_stats.rs` and `stutter_diag.rs`). Capacity 129 exercises the supported generic non-power-of-two case; it is not the current shell configuration. Capacity-128 collection improves modestly, with combined paired ranges of 0.781-1.165x for scalar records and 0.901-1.908x for wide records. Its insertion medians are slower by 3.4% and 1.9% in throughput, respectively. Unchanged visibility and snapshot controls also have slower medians, especially filtered visibility (0.932x, paired 0.729-1.082x). These are retained observations. Stutter insertion improves in every combined pair (1.142-1.402x), as do capacity-129 scalar collection (1.574-2.037x) and insertion (1.115-2.029x). These microbenchmarks do not establish gameplay FPS or whole-application latency gains.

## Method and correctness

Windows x86_64 MSVC, Intel Xeon E5-2696 v4, rustc 1.99.1 / LLVM 23.1.1. Default release LTO, baseline CPU target, and static MSVC runtime settings from `.cargo/config.toml` apply to every measured archive. The existing helper pins the benchmark thread to logical processor 2 and reads `QueryThreadCycleTime`. Each process discards one warmup sample and reports the median of eleven samples. The driver alternates before/after order and reports medians of process medians. No compilation started by this task overlaps retained comparisons; other shared-machine activity is uncontrolled.

Stutter and full-history push fixtures contain 8,192 prepared records and repeat 32 times per sample. Stutter reads visit 512 rings spanning all cursor positions and repeat 32 times. Recent/snapshot fixtures repeat 256 times, with a preallocated output vector and a ring populated with `N + 37` records. Timestamps include zeros and expired entries. Wide records are `[u64; 24]` (192 bytes); the timestamp callback consumes the full opaque record through `black_box`, rather than modeling an exact shell record layout. The growing-ring fixture includes initialization and 64 pushes, repeated 512 times. Fixture construction otherwise stays outside measurement; inputs, mutable ring references in push probes, and outputs use `black_box`.

Two new differential checks compare against production source frozen from `bab7ef065`. They cover repeated stutter overwrites and clears, float output bits including NaNs/infinities/signed zeros, and time-window edges. Generic capacities 0, 1, 2, 3, 5, 7, 8, 127, 128, and 129 cover empty/partial/full/wrapped rings, clears, snapshots, zero/future timestamps, saturated time arithmetic, output capacity, callback order, and partial output when a timestamp callback panics. Callback evaluation still occurs once per visited sample in chronological order, including filtered records. No eager output reservation was added.

Validation passes: **484 release tests** across config/profile (eight ignored), **245 debug tests** across config (four ignored), and Clippy for all config targets with existing warnings and no diagnostics in changed code. The measured default-LTO archives each pass both new differential checks. All three changed/new Rust files pass formatting checks. Source audits verify the frozen reference, restrict production edits to the three methods, and match final production bytes to the measured source and all raw records to archived executable digests. Logs and archives remain under `target/frame-rings/`.

Visibility span, narrower-modulo, and bounded-subtraction trials regressed one or more visibility controls and were discarded. Replacing the saturating length update with a conditional increment measured 0.950x scalar full-ring throughput and 0.710x growing-ring throughput, so it was also discarded. Visibility and length-update code retain their original forms. Trial artifacts remain under `target/frame-rings/`.

```powershell
cargo test --release --locked -p deadsync-config --test frame_rings --no-run --message-format=json
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_frame_rings --log-dir target/frame-rings/runs-final --before target/frame-rings/before-final/config.exe --after target/frame-rings/after/config.exe --output docs/performance/2026-10-26-ring-final.tsv
python scripts/benchmark_sampling.py --runs 12 --test-filter benchmark_history_push --log-dir target/frame-rings/runs-final --before target/frame-rings/before-final/config.exe --after target/frame-rings/after/config.exe --output docs/performance/2026-10-26-ring-history-final.tsv
cargo test --release --locked --config profile.release.lto=false -p deadsync-config -p deadsync-profile -- --test-threads=1
cargo test --locked -p deadsync-config -- --test-threads=1
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-config --all-targets
```

To reconstruct archives, keep the committed integration harness fixed. Restore only the three modified methods from `bab7ef065` for `before-final`; apply the stutter cursor change for `push-final`, the collection change for `wrap-before`, and the generic cursor change for `wrap-after`/`after`. Build/archive the reported integration executable at each stage using the first command. Compare consecutive stages with eight runs each: `benchmark_frame_rings` for the first two changes and `benchmark_history_push` for the third. The two combined comparisons use twelve runs each.
