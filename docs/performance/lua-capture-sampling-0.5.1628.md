# Lua capture sampling - 0.5.1628

Parent: `a082c2239` (0.5.1627). Date: 2026-09-30.

This pass removes temporary ownership work in actor reads and captured writes,
and repeated timing work when a tween writes several properties. It applies
the local guide's M-HOTPATH, M-MEM-REUSE and M-THROUGHPUT recommendations:

1. The complete actor-state reader uses the existing 32-byte inline Lua text
   conversion for text alignment, effect clock, text glow, blend and effect mode.
   Known names no longer create five temporary Rust strings per state read.
   Longer names retain heap storage. Lookup order, coercions, errors and live
   metatable behavior remain unchanged.
2. Captured writes borrow their current state and next value. A new track emits
   its first write as a step, and an unchanged write emits no sample. Neither
   needs an owning current snapshot. Changed writes create that snapshot only
   when an earlier anchor is needed to bridge a gap. Output samples still own
   their values; captured writes still take precedence over restored messages.
3. Adjacent scheduled properties with identical timing inputs share the two
   seconds-to-beat conversions. The cache holds one preceding pair on the stack
   and resets on every call; single-property schedules bypass it. Keys use the
   bits of the original f32 conversion inputs, preserving signed zero and NaNs.
   Each property's f64 start/end seconds are still computed independently and
   stored unchanged.

There are no new dependencies, public API changes or new unsafe code. The state
reader reuses the existing validated owning inline-text adapter. Zero allocation
churn applies to the measured warm state-name and unchanged-value operations.
Actual Lua writes, new tracks, gap anchors and owning tween outputs still require
storage.

## Measurements

The [raw CSV](lua-capture-sampling-0.5.1628.csv) contains 512 rows: 32 workloads,
two variants and eight independent runs. Each percentage range below covers all
eight paired runs. Absolute cycle, allocation and byte figures are from run 1;
allocation counts are identical across runs for every workload.

| Workload | Thread cycles/batch, old -> new | CPU reduction | Throughput gain | Allocations/batch, old -> new | Requested bytes/batch, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 64 complete reads, five known names | 1,069,625.2 -> 995,098.1 | 0.8% to 7.0% | 0.8% to 7.5% | 320 -> 0 | 3,904 -> 0 |
| 16 unchanged color writes, one track | 3,213.6 -> 800.8 | 71.1% to 75.3% | 222.8% to 309.7% | 16 -> 0 | 1,280 -> 0 |
| 512 unchanged color writes, 32 tracks | 102,319.6 -> 21,356.7 | 76.9% to 79.9% | 332.8% to 396.1% | 512 -> 0 | 40,960 -> 0 |
| 2,048 unchanged color writes, 128 tracks | 441,802.1 -> 97,771.8 | 74.6% to 79.0% | 292.8% to 376.5% | 2,048 -> 0 | 163,840 -> 0 |
| 512 changing color writes, contiguous frames | 101,803.4 -> 27,178.6 | 72.5% to 75.5% | 279.7% to 324.1% | 512 -> 0 | 40,960 -> 0 |
| Complete capture, 64 Lua actors x 16 frames | 9,564,767.4 -> 8,904,295.2 | 1.0% to 6.9% | 0.9% to 7.4% | 14,384 -> 13,360 | 843,136 -> 761,216 |
| 4 properties, shared bounds, 8 BPM segments | 620.8 -> 358.4 | 36.0% to 42.3% | 58.1% to 74.9% | 0 -> 0 | 0 -> 0 |
| 32 properties, shared bounds, 8 BPM segments | 4,221.9 -> 1,843.5 | 51.4% to 60.1% | 106.3% to 151.5% | 0 -> 0 | 0 -> 0 |
| 4 properties, shared bounds, 128 BPM segments | 4,288.8 -> 1,351.3 | 62.7% to 72.4% | 169.3% to 263.7% | 0 -> 0 | 0 -> 0 |
| 32 properties, shared bounds, 128 BPM segments | 32,726.1 -> 2,560.3 | 91.1% to 92.4% | 1029.7% to 1224.3% | 0 -> 0 | 0 -> 0 |

The normal state-reader and unchanged color-write cases eliminate temporary
allocations, frees and requested/freed bytes. Contiguous changing writes also
eliminate temporary current-color snapshots because no gap anchor is needed.
All measured variants have zero reallocations. Tween output storage is reused,
so the timing optimization reduces CPU work while both variants have zero churn.
Complete Lua capture still allocates for setter and table work; requested bytes
fall by 9.7% in every complete-capture case. Those are warm captures with existing
tracks, not first-track construction or complete song compilation.

Controls remain part of the comparison. Negative values
mean more CPU work or lower throughput. Single-property schedules bypass the
cache because they have no next property with which to share timing work.
Long names still spill to heap storage; gap anchors still need owning color
values. The complete-capture controls include Lua GC, so their independently
counted frees can vary even when the allocated-byte reduction is constant.

| Control workload (CSV name) | CPU reduction | Throughput gain | Allocations/batch, old -> new | Requested bytes/batch, old -> new |
| --- | ---: | ---: | ---: | ---: |
| `state_names_empty` | -1.4% to 5.2% | -1.5% to 5.5% | 0 -> 0 | 0 -> 0 |
| `state_names_long` | -3.0% to 9.8% | -2.9% to 10.8% | 576 -> 576 | 294,912 -> 294,912 |
| `captured_values_1_colors_false` | -10.9% to 28.9% | -9.7% to 40.6% | 0 -> 0 | 0 -> 0 |
| `captured_values_32_colors_false` | -22.6% to 5.9% | -18.2% to 6.2% | 0 -> 0 | 0 -> 0 |
| `captured_values_128_colors_false` | -1.2% to 22.1% | -1.2% to 28.3% | 0 -> 0 | 0 -> 0 |
| `captured_changes_32_gaps_true` | -34.1% to 6.1% | -25.6% to 6.6% | 512 -> 512 | 40,960 -> 40,960 |
| `complete_capture_1` | -7.9% to 8.6% | -7.5% to 8.2% | 224 -> 208 | 13,136 -> 11,856 |
| `complete_capture_16` | -8.9% to 5.2% | -8.1% to 5.5% | 3,614 -> 3,358 | 211,696 -> 191,216 |
| `tween_clock_0_1_shared_true` | -17.2% to 7.4% | -15.9% to 6.9% | 0 -> 0 | 0 -> 0 |
| `tween_clock_0_1_shared_false` | -12.8% to 1.3% | -10.8% to 0.0% | 0 -> 0 | 0 -> 0 |
| `tween_clock_0_4_shared_true` | -9.2% to 47.2% | -11.0% to 92.1% | 0 -> 0 | 0 -> 0 |
| `tween_clock_0_4_shared_false` | -11.4% to 6.2% | -10.1% to 6.9% | 0 -> 0 | 0 -> 0 |
| `tween_clock_0_32_shared_true` | 3.8% to 22.9% | 3.9% to 29.9% | 0 -> 0 | 0 -> 0 |
| `tween_clock_0_32_shared_false` | -7.0% to 14.1% | -6.5% to 16.5% | 0 -> 0 | 0 -> 0 |
| `tween_clock_8_1_shared_true` | -9.3% to 0.0% | -8.2% to -0.9% | 0 -> 0 | 0 -> 0 |
| `tween_clock_8_1_shared_false` | -13.7% to 3.2% | -11.9% to 2.6% | 0 -> 0 | 0 -> 0 |
| `tween_clock_8_4_shared_false` | -52.0% to 12.7% | -34.7% to 21.9% | 0 -> 0 | 0 -> 0 |
| `tween_clock_8_32_shared_false` | -35.3% to 3.4% | -25.5% to 3.5% | 0 -> 0 | 0 -> 0 |
| `tween_clock_128_1_shared_true` | -40.1% to 5.5% | -28.8% to 6.0% | 0 -> 0 | 0 -> 0 |
| `tween_clock_128_1_shared_false` | -20.9% to 18.2% | -21.0% to 22.2% | 0 -> 0 | 0 -> 0 |
| `tween_clock_128_4_shared_false` | -14.9% to 4.3% | -12.1% to 4.4% | 0 -> 0 | 0 -> 0 |
| `tween_clock_128_32_shared_false` | -0.2% to 11.8% | -0.2% to 13.4% | 0 -> 0 | 0 -> 0 |

The single-property, eight-segment shared-bound control uses up to 9.3% more CPU
and has 0.9% to 8.2% lower throughput. Distinct-bound and gap controls show mixed
results, including higher CPU medians: one four-property/eight-segment distinct
run uses 52.0% more cycles, while other runs improve. No consistent CPU gain is
established for those cases. The targeted savings do not establish a speedup for
every workload or a reduction in peak memory, full-song compilation time or game
frame time.

## Method and reproduction

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.1. Both implementations
run in the same release test executable with optimization level 3 and fat LTO.
The scoped System allocator counts calling-thread allocations, reallocations,
frees, and requested/freed bytes. QueryThreadCycleTime counts calling-thread CPU
cycles. Timing and allocation accounting are separate. Byte totals describe
allocator churn, not peak resident memory or process RSS.

Each row reports the median of seven timing samples after warmup and a separate
allocation-counted operation. Eight runs alternate old-first and new-first.
State-reader batches perform 64 complete reads with no names, five known aliases,
or five 512-byte names that spill to the heap. Lua fixtures and their strings are
created outside measurement.

Captured-value batches run 16 frames with 1, 32 or 128 existing scalar or color
tracks. Prepared owning writes, state buffers, maps and sample capacity exist
before measurement. The changing-value control uses 32 color tracks with a fresh
fixture per operation, either contiguous frames or gaps that require anchors.
Setup and fixture destruction are excluded from that control; allocation and
destruction of temporary current snapshots are included. The control's output
capacity is preallocated equally in both variants.

Complete capture batches run 16 frames with 1, 16 or 64 Lua actors. They call the
real vertex-color setter, record writes, synchronize update/message states,
manage existing tracks and reset actor capture tables. Normal Lua GC stays on.
Lua setter/table work and temporary-value destruction are included, while setup
and final fixture destruction are excluded. Collector phase can change the
separately counted frees and bytes because earlier garbage is also collected.

Tween batches process 1, 4 or 32 properties against maps with 0, 8 or 128 BPM
segments, at 48 seconds and music rate 1.25. The shared case uses one timing pair;
the distinct case changes each property's delay. Output capacity is preallocated
and metadata strings are absent, so these timing cases remain allocation-free.
Throughput units are complete state reads, captured writes, or scheduled
properties, depending on the workload.

The test-only baselines freeze the parent's complete state reader, owning-value
helper, complete capture helper and complete scheduled-property helper. All four
functions were checked against the parent, allowing visibility and formatting
only. The frozen capture helper resolves its value and timing helpers to the
frozen functions. Shared Lua setters, capture storage and other helpers are
unchanged in the measured paths. These results measure reads and capture, not
complete song compilation or whole-game frame rate.

```powershell
cargo test -p deadsync-song-lua --lib capture_sampling -- --test-threads=1
cargo test -p deadsync-song-lua --lib
cargo test -p deadsync-song-lua --release --lib
cargo test -p deadsync-song-lua --release --lib capture_sampling -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib capture_sampling -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

## Behavior validation

Eight new tests compare the parent and current implementations. State-reader
tests cover aliases, numeric coercions, wrong types, Unicode/NUL, invalid UTF-8,
31/32/33-byte spill boundaries, long names, dynamic lookup order, GC and partial
errors. Captured-value tests cover all 77 targets, first-write steps, equal writes,
gaps, repeated and nonfinite beats, NaN payloads, retained color identity, missing
current states and captured-write precedence over restored messages. Tween tests
compare exact f32/f64 bits for repeated and distinct bounds, signed zero, infinities,
NaNs, empty/single/multiple-property schedules, missing states, changed BPM
maps/rates and prior color values. Allocation assertions require zero warm
normal state reads and unchanged-value churn, and
zero churn with shared and distinct tween bounds when output capacity exists.

- All eight new behavior/allocation regressions pass in debug and release.
- Full debug and release library suites each report **652 passed, 4 failed,
  61 ignored**. The parent reported 644 passed, 4 failed, 59 ignored. The same
  four tests fail with unchanged assertion values: actor proxy visibility,
  local/hidden screen proxy visibility, queued command-builder visibility and
  the notefield-column position (`-96:-135` versus expected `-96:-125`).
  Failure names, source locations and assertions were compared to the parent's
  recorded debug and release results. These changes do not touch `lib.rs`.
- All eight serial release benchmark runs pass (two ignored benchmark tests per
  run), alternating old-first and new-first. Each produces 64 data rows.
- `cargo clippy -p deadsync-song-lua --all-targets` exits successfully with the
  existing warnings and two extra argument-count warnings in frozen baseline
  functions. The new production code and fixtures have no Clippy warnings.
- Formatting, `git diff --check`, frozen-baseline verification and
  `cargo metadata --locked --no-deps` pass.

The workspace version advances exactly once, 0.5.1627 -> 0.5.1628. Cargo.lock
updates the three packages that inherit it. The four excluded files are not
part of the commit.
