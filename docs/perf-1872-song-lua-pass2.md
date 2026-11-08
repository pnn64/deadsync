# Song Lua performance, second pass

This pass starts from `a333c7813`, including all optimizations in the
[first pass](perf-1872-song-lua.md). Its measurements compare against that
already optimized baseline.

Following the profiling, allocation reuse and throughput guidance in
[rust-performance.md](../rust-performance.md), fresh CPU profiles identified
remaining VM lock traffic, class lookup allocations and replay bookkeeping.
The changes in this pass target six additional sources of repeated work:

1. Resolve actor identities and parent delta slots when building the immutable
   update plan. Replay uses a reusable contiguous array instead of hashing
   actor pointers and entering the VM to resolve them on every job.
2. Read short actor class names into inline storage, preserving String's
   coercions, long values and conversion errors.
3. Use the locked raw-state API for read-only environment and scalar-state
   batches, avoiding temporary protected C closures. Raw writes and authored
   lookup hooks retain their protected paths.
4. Move trace detail maps into their events instead of cloning the maps and
   nested argument/result arrays.
5. Normalize captured source strings in their existing storage, retaining the
   original filtering and path rules.
6. Look up previously observed query handle identities using borrowed strings.
   Scene epochs and global first-observation numbering remain unchanged.

The six changes are committed in `f8095aae6`. The results measure them together;
they do not assign individual speedups to each change.

## Results

The [raw measurements and build receipts](perf-1872-song-lua-pass2-results.json)
record every sample, output fingerprint, source/executable hash and counter.
Values below are medians of three interleaved pairs, before → after. G cycles
means billions of user CPU cycles; M allocations means millions of successful
allocation calls, excluding reallocations.

| Workload | Wall seconds | User CPU cycles (G) | Allocations (M) | Throughput gain |
| --- | ---: | ---: | ---: | ---: |
| 7th Gear preparation, null renderer | 26.39 → 21.35 | 86.28 → 69.07 | 44.05 → 18.92 | 23.6% |
| 7th Gear preparation, OpenGL branch | 63.56 → 55.48 | 209.21 → 181.66 | 86.59 → 55.24 | 14.6% |
| VTubers Never Die, full 67,455-frame replay | 90.15 → 86.50 | 286.98 → 275.16 | 60.68 → 56.80 | 4.2% |

Supplemental heavy workloads use one new candidate run and reuse the retained
first-pass *after* sample as this pass's baseline. The executable SHA-256,
inputs, CPU affinity and instrumentation are identical to the current baseline;
the raw results identify the original samples explicitly.

| Supplemental workload | Wall seconds | User CPU cycles (G) | Allocations (M) | Throughput gain |
| --- | ---: | ---: | ---: | ---: |
| Mountain King, full 56,068-frame replay | 220.00 → 210.34 | 693.12 → 660.25 | 146.28 → 124.52 | 4.6% |
| Mawaru7, diagnostic 21,438-frame prefix | 526.14 → 498.33 | 1546.19 → 1455.16 | 497.98 → 456.94 | 5.6% |

All five workload fingerprints match their baselines. Mawaru7's prefix ends at
music second 30 and remains a diagnostic result; it does not accept the pending
chart. Mawaru9 is excluded.

These are measurements on selected workloads, not a corpus-wide speedup claim.
The OpenGL argument selects the production compilation branch; no window or
GPU rendering is involved. The machine and build profile match the first pass:
40-vCPU Xeon E5-2697 v2 KVM guest, Linux 6.8.0-142, rustc 1.98.1, release
optimization with LTO disabled. Timed comparisons use CPU 2 without concurrent
builds or other benchmarks.

## Correctness

All 49 native replay tests and seven focused checks pass. The compiler suite
has 309 passing tests and the same seven existing failures as `a333c7813`;
their failure diagnostics match after normalizing Rust source locations and
test process/thread identifiers. No acceptance rule or tolerance changes.

Environment reads are compared against the original reader, including false
values, nil, rebinding, table/function identities, metamethod side effects and
errors. New raw-stack checks cover successful reads, Rust errors and unwinding.
Actor update checks retain aliased visits, parent deltas and plan rebuilds.

## Verification and measurement

The production compiler benchmark still calls `prepare_song_lua` without
`test-support` and checks that preparation retains no Lua VM. The fixture
runner uses the existing comparisons and acceptance criteria. All work is
headless; native captures are reused and reports are compared uncompressed.

Each primary workload uses three interleaved baseline/candidate pairs; the
second pair reverses the run order. Counters include hardware user CPU cycles,
instructions and CPU time. Allocation accounting
includes Rust GlobalAlloc calls from all process threads, including mlua;
direct native malloc calls remain outside that count. Throughput is measured
in preparations or replayed frames per second and per CPU cycle.

Use the [first-pass reproduction commands](perf-1872-song-lua.md#reproduction)
with the before/after executable paths recorded in this pass's build receipts.
Compiled fingerprints include all compiled layer data except preparation
timings. Complete fixture report hashes normalize temporary extraction paths
and process-specific addresses in typed actor handles, preserving identities,
names, ancestry, values and event order.

Fixtures already support independent worker processes through `--jobs N`.
Cargo compilation uses eight jobs. A chart's Lua callbacks retain their
chronological order because they share mutable state.

The updated runner passes the same GREAT GREAT and freg. archives with one
and two workers, retaining all 484 comparisons. This is one pair, using CPUs
2 and 3; aggregate cycles and allocations include parent and child processes.

| Workers | Wall seconds | User CPU cycles (G), all processes | Allocations (M), all processes | Fixtures/second |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 39.93 | 129.76 | 27.795 | 0.0501 |
| 2 | 30.43 | 129.67 | 27.829 | 0.0657 |

Two workers improve throughput by 31.2% in this pair, with 0.07% fewer aggregate
CPU cycles and 0.12% more allocations. Worker support predates this pass and is
not counted among its six optimizations. Missing coverage in the accepted
fixtures remains reported by the runner without changing acceptance criteria.
