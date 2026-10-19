# Song Lua state reads and command buffers

This pass starts at `add695545`, after the
[draw-capture allocation pass](2026-10-19-song-lua-draw-capture.md).
Following M-HOTPATH, M-MEM-REUSE and M-INITIAL-CAPACITY in
[rust-performance.md](../../rust-performance.md), it makes three changes:

1. Use the existing locked scalar batch reader for baked draw states. Sprites
   without an explicit texture-state flag retain sequential reads, preserving
   authored Texture/AFT lookup hooks, their mutations and error order.
2. Read remaining state-decoder actor classes and texture-presence values
   into inline storage, preserving String coercions, long values and errors.
3. Reserve command storage from the known call count when a frame has one
   owner, and reserve the single owner-end entry. Quiet frames allocate no
   command storage; duplicate/multiple owners retain their original routing.

The test executable freezes the preceding draw implementation and state
decoder separately. Scalar-field enums and unchanged helpers are shared.
The before/after measurement therefore includes both decoder and draw changes.

## Results

The [raw results](2026-10-19-song-lua-state-reads.json) record all 54 samples
per variant/workload. Values below are medians normalized per scene frame,
before → after. Results measure the three changes together and do not assign
individual speedups to each change.

| Workload | CPU cycles/frame | Allocations/frame | Reallocations/frame | Scene frames/second |
| --- | ---: | ---: | ---: | ---: |
| Shallow capture | 216,019 → 134,022 | 36.41 → 29.94 | 2.19 → 2.19 | 10,135 → 16,205 |
| Wrapped capture | 4,432,245 → 2,741,070 | 505 → 320.33 | 7.25 → 7.25 | 493.7 → 797.2 |
| Dense capture | 3,782,314 → 2,596,548 | 703.25 → 572.75 | 12.63 → 12.75 | 577.5 → 842.8 |
| One-owner conversion | 6,600 → 6,143 | 3.03 → 3.03 | 2.06 → 0.06 | 329,812 → 354,178 |
| Dense-owner conversion | 224,929 → 184,490 | 3.06 → 3.06 | 7.09 → 0.09 | 9,542 → 11,825 |
| 64-owner conversion | 417,036 → 383,717 | 192.06 → 192.06 | 64.41 → 64.41 | 5,229 → 5,681 |

Capture throughput improves 59.9%, 61.5% and 45.9%, with 31.4–38.2% fewer CPU
cycles and 17.8–36.6% fewer allocation calls. Requested bytes are +0.13%, -0.15%
and +0.13%, respectively, including the per-VM scalar-key cache allocation.
The dense capture has one additional reallocation per eight-frame batch.

Conversion throughput improves 7.4%, 23.9% and 8.7%. Requested bytes fall
42.1%, 49.8% and 0.6%. Dense single-owner conversion removes seven reallocations
per scene frame; the small single-owner case removes two.

Measurements use a Windows host with 44 logical CPUs (Xeon E5-2696 v4), rustc
1.98.1, release optimization and disabled LTO. Source/executable hashes and
all individual samples are retained in the result JSON.

## Correctness

Complete frame/operation comparisons retain state values, source bindings,
cameras, duplicate owner visits, quiet tails and nested render targets. The
new state checks compare authored Texture/AFT lookup event order, mutations,
errors, class/texture coercions, invalid UTF-8 and long strings against the
frozen preceding decoder. Existing numeric/boolean batch-reader checks pass.

All 17 draw checks and five state-reader checks pass. The full default-feature
release library suite has 681 passes, 13 failures and one ignored performance
benchmark, compared with 679 passes and the same 13 failures before this pass.
Both new tests pass. Every failure has the same normalized diagnostic as the
preceding commit's retained suite log; that log's hash matches its committed
measurement receipt. Normalization removes source locations, workspace
prefixes, temporary process/thread identifiers and the backtrace hint.
The result JSON records all failure names and normalized diagnostic hashes.

Native fixtures, comparison tolerances and acceptance rules are unchanged.

## Reproduction and scope

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit add695545 --out docs/performance/2026-10-19-song-lua-state-reads.json
```

Run the reported executable with `--test-threads=1` for the library suite.
The benchmark uses six alternating rounds, each with a warmup followed by
nine samples per variant/workload. It pins its measurement thread to logical
CPU 2 and runs without concurrent builds or benchmarks.

Shallow capture has one owner/four leaves; wrapped capture has three owners,
twelve leaves each and four wrappers per leaf. Dense capture has one owner
and 128 leaves. Conversion cases have one owner/four leaves, one owner/128
leaves and 64 owners/two leaves each. Setup and Lua teardown are excluded;
conversion includes consuming the capture, constructing and dropping output.
These synthetic workloads measure the actual capture/conversion routines,
not whole-song gameplay throughput.

Both variants use the same counted System allocator. Allocation traffic
covers the current thread's Rust GlobalAlloc, including mlua, and excludes
direct native malloc calls. CPU cycles come from Windows QueryThreadCycleTime
and include scheduled kernel execution. Timing/cycle measurements include
allocation-accounting overhead. Requested bytes describe traffic, not RSS.
