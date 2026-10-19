# Song Lua draw capture

This pass optimizes three sources of repeated work in custom draw capture,
following M-HOTPATH, M-INITIAL-CAPACITY and M-MEM-REUSE in
[rust-performance.md](../../rust-performance.md):

1. Reserve parent-stack capacity for the inherited states, wrappers and actor.
   Construct frozen parent slices directly, avoiding temporary leaf/screen
   vectors and their copies.
2. Read actor classes and external child names into the existing inline
   `LuaFieldText` type. Numeric coercions, UTF-8 errors, long names and lookup
   hooks retain String-compatible behavior.
3. Write converted frames directly into the output buffer, removing the
   intermediate vector allocated for every captured frame and the subsequent
   transfer into the output. Preserve owner order, duplicate visits and quiet
   tail frames.

The baseline is `3d8094d9e`. The test executable contains the frozen baseline
and candidate, sharing the same public payload types, allocator, dependencies
and build settings. No acceptance criteria or draw budgets change.

## Measurement

The [raw results](2026-10-19-song-lua-draw-capture.json) include all 54 measured
samples per variant/workload. Values below are medians normalized per scene
frame, before → after. The three changes are measured together; these numbers
do not assign separate speedups to individual changes.

| Workload | CPU cycles/frame | Allocations/frame | Reallocations/frame | Scene frames/second |
| --- | ---: | ---: | ---: | ---: |
| Shallow capture | 237,029 → 218,749 | 48.41 → 36.41 | 8.19 → 2.19 | 9,241 → 10,008 |
| Wrapped capture | 4,877,290 → 4,735,520 | 602 → 505 | 83.25 → 7.25 | 448.9 → 462.4 |
| One-owner conversion | 8,598 → 7,122 | 4.03 → 3.03 | 2.06 → 2.06 | 253,415 → 301,887 |
| 64-owner conversion | 463,412 → 443,719 | 193.06 → 192.06 | 68.28 → 64.41 | 4,723 → 4,937 |

Throughput improves 8.3%, 3.0%, 19.1% and 4.5%, respectively. CPU cycles fall
7.7%, 2.9%, 17.2% and 4.2%. Requested allocation bytes fall 37.8% and 40.5%
for capture, and 4.0% and 4.3% for conversion. Conversion removes one allocation
per scene frame. The wrapped case removes 76 reallocations per scene frame.

These are selected synthetic capture/conversion workloads, not a whole-song
or corpus-wide speedup claim. The host is Windows on a 44-logical-CPU Intel
Xeon E5-2696 v4. Builds use rustc 1.98.1, release optimization and disabled LTO.
No builds or other benchmarks run concurrently with these measurements.

## Correctness

Differential checks compare every emitted frame and operation with the frozen
baseline, including parent states, cameras, source bindings, order and quiet
tails. They cover duplicate owners, immediate Draw, nested render targets,
String coercions, long/invalid UTF-8 values and source-binding errors.

All four new differential checks and all 16 draw checks pass. The complete
default-feature release library suite has 679 passes, 13 failures and one
ignored performance benchmark. All 13 failures are rerun with the original
draw module against the same current sources, dependencies and allocator;
each reproduces the same normalized diagnostic. The raw result file names
every failure and records the baseline/candidate executable hashes and
diagnostic hashes. Normalization removes source locations, the copied crate
root's filename, temporary process/thread identifiers and the backtrace hint.

The unchanged failures concern compiler compatibility, actor-host setup and
sound/reload controls. No acceptance rule, tolerance or retained native fixture
changes in this pass.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
```

Use the Song Lua test executable reported by Cargo:

```powershell
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --out docs/performance/2026-10-19-song-lua-draw-capture.json
```

The benchmark uses logical CPU 2 and six rounds with alternating variant order.
Each variant/workload has nine measured samples per round after one warmup.
Fixture setup and Lua destruction are outside measurement. Capture measures
the actual traversal and retained frame creation; conversion measures consuming
those frames, building their output and dropping that output. Throughput is
frames per second for that phase, not whole-song gameplay throughput.

The shallow capture has one callback owner and four leaves. Wrapped capture
has three owners, twelve leaves per owner and four wrappers per leaf.
Conversion cases use one owner/four leaves and 64 owners/two leaves each.
Each case uses fresh Lua tables, real draw callbacks, actor-state reads and
the production capture/conversion routines. No timing region uses mocks.

Windows `QueryThreadCycleTime` records scheduled thread cycles, including
kernel execution; these are not Linux user-only PMU counts. Allocation counts
cover the measured thread's Rust global allocator, including mlua. Direct
native `malloc` calls are excluded. Requested bytes describe allocation
traffic, not resident memory. The raw JSON records every sample and the
executable, source and compiler identities.
Both variants use the same counted `System` allocator. Cycle and timing
measurements include the allocation-accounting overhead.
