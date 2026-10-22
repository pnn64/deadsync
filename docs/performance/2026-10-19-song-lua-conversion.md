# Song Lua draw-frame conversion

This pass starts from `276198421`, following the [compiled rendering and clocks pass](2026-10-19-song-lua-render-clocks.md). The three changes follow M-HOTPATH, M-INITIAL-CAPACITY, M-MEM-REUSE and M-THROUGHPUT in [rust-performance.md](../../rust-performance.md):

1. Reserve the converted-frame vector from the known normal owner union and immediate-frame owner counts. Authored duplicates can still grow the vector; output order and quiet tails remain unchanged.
2. Count multi-owner operations into reusable scratch storage, then reserve each destination command vector exactly. Keep the existing one-owner reservation and skip scratch preparation for empty frames.
3. Route each operation through a reusable map from resolved owner index to the first matching output slot, replacing the repeated linear search. Duplicate pointers and aliased overlay bindings retain their first-match behavior.

Counting is allocation planning only. It skips unresolved owners and never returns a validation error; the consuming operation loop still reports owner, render-target and missing-source errors in the original order. No new unsafe code or floating-point arithmetic is introduced.

## Measurements

The [raw results and validation receipt](2026-10-19-song-lua-conversion.json) retain six alternating-order rounds and 54 samples per variant/workload. Values below show medians before → after. Results combine the three changes; they do not attribute separate gains to each.

| Case | CPU cycles/raw frame | Raw frames/second | Throughput change |
| --- | ---: | ---: | ---: |
| One owner, 32 calls | 12,749.0 → 11,157.6 | 170,895 → 195,599 | +14.5% |
| Two owners, 64 calls | 44,058.6 → 31,563.4 | 49,511 → 69,354 | +40.1% |
| Eight owners, 128 calls | 99,097.4 → 77,555.5 | 22,032 → 28,149 | +27.8% |
| 32 owners, 256 calls | 235,562.9 → 158,360.7 | 9,277 → 13,838 | +49.2% |
| 128 owners, 256 calls | 242,465.2 → 188,331.0 | 9,004 → 11,633 | +29.2% |
| 32 owners, four calls | 10,265.9 → 10,140.7 | 212,819 → 214,783 | +0.9% |
| 32 owners, no calls | 7,884.0 → 7,428.7 | 276,279 → 292,171 | +5.8% |

A unit is one captured input frame. Each input frame produces one output `DrawFrame` per owner, so these rates are conversion throughput, not gameplay FPS or complete-song compilation rates. All cases remain within the 256-operation capture limit.

The busy two- through 128-owner cases improve throughput in every round. Sparse and quiet throughput changes vary in sign across rounds; their allocation-traffic reductions are consistent.

| Case | Allocations/raw frame | Reallocations/raw frame | Requested bytes/raw frame |
| --- | ---: | ---: | ---: |
| One owner, 32 calls | 3.0156 → 3.0156 | 0.0391 → 0.0000 | 38,077.5 → 37,992.2 |
| Two owners, 64 calls | 6.0156 → 6.0312 | 10.0469 → 0.0000 | 149,565.5 → 75,985.2 |
| Eight owners, 128 calls | 24.0312 → 24.0625 | 32.1250 → 0.0156 | 295,164.0 → 152,391.0 |
| 32 owners, 256 calls | 96.0312 → 96.0625 | 96.1875 → 0.0469 | 574,466.0 → 306,460.8 |
| 128 owners, 256 calls | 384.0625 → 384.1250 | 128.4688 → 0.1562 | 479,284.0 → 316,647.5 |
| 32 owners, four calls | 68.0156 → 68.0312 | 0.1016 → 0.0234 | 10,881.0 → 8,078.4 |
| 32 owners, no calls | 64.0156 → 64.0156 | 0.1016 → 0.0234 | 6,145.0 → 3,331.8 |

The map and count vector add two scratch allocations per complete conversion for fixed-size nonempty multi-owner cases, then reuse their capacity across frames. One-owner and empty-frame cases allocate no routing scratch. Growing owner counts may resize it. Requested bytes include new and reallocated Rust buffer traffic, not RSS; input buffers are prepared outside the measured scope. Their consumption/frees occur during conversion, while converted-output destruction is excluded.

## Behavior and validation

Four new release checks pass against frozen conversion code from `276198421`. They cover sparse and reordered owner sets, callback quiet tails, immediate frames, duplicate owners, distinct pointers resolving to one overlay, all draw-source variants, begin/finish target operations, retained parent/vertex Arcs, missing owners/sources/targets and competing error order. Varied frame sequences include nonfinite and signed-zero timestamps and `usize::MAX` overlay indices. Retained payload checks verify Arc identity and exact NaN/signed-zero bits.

The default-feature release library suite has 693 passes, 13 failures and four ignored performance benchmarks. Every failure matches the preceding commit's normalized diagnostic, and its retained log hash matches the committed receipt. All four new conversion checks pass. Normalization removes Rust source locations, workspace prefixes, temporary process/thread identifiers and the backtrace hint. The JSON records each failure and diagnostic hash, source/executable hashes and the build-receipt hash. Frozen conversion was checked against `git show 276198421` after its sole visibility adapter. Native fixtures, acceptance criteria and tolerances are unchanged.

## Reproduction and scope

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 276198421 --test-filter draw_capture::perf::conversion::benchmark_conversion --out docs/performance/2026-10-19-song-lua-conversion.json
```

Run the executable with `--test-threads=1` for the full library suite. The seven cases use one through 128 owners and zero through 256 operations per input frame. Fixtures retain 32–128 input frames per conversion. Each round performs a warmup and nine retained samples per variant/workload, pinned to logical CPU 2. Both versions run in one executable with the same counted System allocator on Windows, Xeon E5-2696 v4 (44 logical CPUs), rustc 1.98.1, release optimization with LTO disabled.

Timing covers conversion and consumption of captured inputs, with fixture construction and returned-output destruction excluded. QueryThreadCycleTime records scheduled current-thread CPU cycles including kernel execution, not a user-only hardware PMU counter. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua, and excludes direct native malloc. Accounting overhead is included in timing/cycles. Builds and other benchmarks do not overlap measurement. These isolated conversion results do not establish whole-song or corpus-wide speedups.
