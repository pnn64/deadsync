# Use the observed tween prefix during frame updates

Baseline: `6513c22e2`.

`deadlib-present::runtime` already moves each actor into a contiguous prefix
when it is first materialized in a frame. Repeated lookups leave the cursor
alone, and new actors displace unseen actors into the suffix. At the next tick,
the prefix therefore contains exactly the actors that should advance; the
suffix contains exactly the actors that should expire.

Previously `tick` checked every entry's frame stamp, then removed stale entries
with `swap_remove` and repaired the index of each moved entry. Those moved
entries were also stale and would immediately be removed themselves.

The new tick updates the prefix and drains the suffix, removing each expired
ID from the index once. Surviving entries retain their order and indices. This
removes redundant age checks, moves and index repairs without adding storage.
Frame stamps remain in use by materialization. Entry size is unchanged at 1,248
bytes on this target. Expired programs may be destroyed in a different order;
they contain owned internal data, with no user callbacks or custom destructors.

## Behavior checks

The integration test includes the production runtime and a frozen copy of the
baseline, both using the same production animation implementation. It compares:

- Every component of returned/stored tween states by exact float bits.
- Entry order, traversal cursor, ID-to-index mappings, and sequence debug snapshots.
- Lazy builder invocation counts, including recursive creation of the same or
  another actor, a tick during construction, and a clear during construction.

There are 73,728 deterministic mixed operations across 72 traces, plus 16
completion frames and two expiry ticks per trace. Cases include insertion,
duplicates, reordering, partial retention, consecutive ticks, clear, baseline
frame wraparound, sleeps, instant/relative/timed operations, negative and zero
delta time, NaN, infinity, and signed zero. Existing unit tests also check the
one-unseen-frame lifetime and state progression explicitly.

The 11 active integration tests passed before the edit and with the final
change. Debug validation passed 171 library tests and the 11 integration tests;
nine integration tests repeat the source unit tests. Warmed active and idle
frames, with alternating traversal order and duplicate lookups, allocate and
free nothing. Formatting, diff checks and performance Clippy checks passed;
existing non-performance Clippy warnings remain.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during
measurement. Both variants use equivalent black-boxed function pointers in
the same executable. Windows `QueryThreadCycleTime` supplies thread cycles;
allocation accounting runs separately.

Ordinary operations tick and materialize all actors. Reordered frames alternate
forward and reverse order; duplicate frames materialize each actor twice.
Active tweens have a long duration and stay active throughout measurement.
Seven batches of 8,192 operations follow three warmups. Throughput counts
distinct actors per frame, including in the duplicate case.

Expiry cases start with 128 active actors and retain either zero or 64. Each
measured operation is one tick, advancing retained actors and dropping expired
actors. Setup and later registry destruction are excluded. Seven batches of
512 operations follow three warmups. Per-operation clock overhead is included
equally, and throughput counts 128 original actors inspected/handled per tick.

| Case | Before ns/op | After ns/op | Median paired cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| idle16 | 888.6 | 908.0 | -2.2% | -7.0 to 7.1% | -2.1% |
| idle128 | 8097.7 | 7979.2 | 1.1% | -0.3 to 1.5% | 1.0% |
| idle1024 | 110653.5 | 107809.3 | 1.2% | 0.4 to 2.9% | 1.2% |
| active128 | 7070.2 | 6902.6 | 2.4% | 0.1 to 3.7% | 2.4% |
| reorder128 | 14615.1 | 14626.9 | 0.5% | -0.1 to 3.9% | 0.5% |
| duplicate128 | 12811.3 | 12747.0 | 1.2% | -0.1 to 1.3% | 1.2% |
| expire128_keep0 | 14362.9 | 11249.4 | 20.7% | 13.9 to 23.0% | 27.7% |
| expire128_keep64 | 8116.2 | 6492.8 | 18.2% | 16.2 to 18.6% | 25.1% |

Times are medians across runs; percentages are medians of paired ratios.
Negative reductions indicate a slower candidate. The reproducible benefit is
expiry: about 3.11 microseconds saved when all 128 actors disappear and 1.62
microseconds when half disappear. Ordinary-frame differences are small and
mixed; no ordinary-frame speedup is claimed. An initial unchanged-code control
also showed timing/layout variation between otherwise equivalent modules, so
small percentages should not be interpreted as reliable improvements.

All measured operations allocate/reallocate zero times. Ordinary frames also
free nothing. Expiry frees are identical: 128 frees / 241,664 bytes when all
actors expire, or 64 frees / 120,832 bytes when half expire. This is CPU registry
measurement, not a full-frame/FPS or rendered-pixel measurement.

### Rejected broader change

Removing frame stamps entirely also passed the behavior tests, but changed
entry size from 1,248 to 1,240 bytes and made several ordinary-frame cases slower.
The cause of the timing change was not isolated. That variant was discarded;
the final change preserves entry layout and materialization bookkeeping.
The [raw CSV](tween-registry-prefix-0.5.1525.csv) includes the unchanged-code
control, rejected variant, and all final paired samples, with absolute cycles,
throughput, timing ranges, and allocation counts.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-present --test tween_registry
cargo test --locked -p deadlib-present --lib --test tween_registry
cargo clippy --locked -p deadlib-present --lib --test tween_registry -- -D clippy::perf
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed release test executable directly:
# <executable> benchmark_tween_registry --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Local final logs are in `target/tween-registry-pass/run1.txt` through `run3.txt`.
The rejected source, executable and logs are in `target/tween-registry-stamps`.
