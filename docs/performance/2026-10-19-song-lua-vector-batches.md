# Song Lua predicates and vector batches

This pass starts from `8c2c68c77`, after the
[state-read pass](2026-10-19-song-lua-state-reads.md). It targets three repeated
operations identified by a baseline probe, following the profiling and
throughput guidance in [rust-performance.md](../../rust-performance.md):

1. Classify Sprite strings directly in mlua's checked stack conversion,
   avoiding an owning Value/String handle and borrowed-byte refcount allocation.
   Non-strings, arbitrary binary strings and case-insensitive matching retain
   the preceding predicate's behavior.
2. Read two- through five-component vectors in one locked VM batch. Numeric
   coercions, first-invalid-component returns and ignored extra entries remain
   unchanged.
3. Read complete four-by-four vertex-color tables in one VM batch, removing
   temporary row handles and repeated lock/stack setup for each row/component.

The new raw readers use the existing stack-restoring guard. Three slots cover
the root, current row and component; rows are type-checked before indexing.
Only raw reads and Lua's non-allocating numeric conversion run inside the
batch. Every exit restores the caller's stack; no pointer escapes.

## Results and correctness

The [raw results](2026-10-19-song-lua-vector-batches.json) record 54 samples
per variant/workload. Values below show medians before → after. Capture
numbers measure all three changes together, not separate attributed speedups.

| Capture case | CPU cycles/frame | Allocations/frame | Scene frames/second |
| --- | ---: | ---: | ---: |
| Shallow | 161,717 → 155,437 | 29.94 → 22.94 | 13,530 → 14,057 |
| Wrapped | 3,242,975 → 3,174,034 | 320.33 → 135.33 | 674.7 → 689.8 |
| Dense | 3,123,403 → 3,006,930 | 572.75 → 441.75 | 701.3 → 728.1 |
| Populated properties | 6,245,936 → 4,568,243 | 318.67 → 133.67 | 350.7 → 478.1 |

Capture throughput rises 2.2–36.3% and CPU cycles fall 2.1–26.9% on these
inputs. Allocation calls fall 22.9–58.1%; requested byte traffic falls
0.19–0.46%. Capture reallocations are unchanged except for one fewer
reallocation per twelve-frame populated-properties batch.

| Direct read | CPU cycles/read | Allocations/read | Reads/second |
| --- | ---: | ---: | ---: |
| Sprite predicate | 872.51 → 272.96 | 1 → 0 | 2,512,373 → 7,990,890 |
| Two-value vector | 440.06 → 187.16 | 0 → 0 | 4,979,212 → 11,706,518 |
| Three-value vector | 635.86 → 225.27 | 0 → 0 | 3,443,971 → 9,710,623 |
| Four-value vector | 883.14 → 255.10 | 0 → 0 | 2,482,468 → 8,594,019 |
| Five-value vector | 1,060.97 → 309.45 | 0 → 0 | 2,066,073 → 7,084,536 |
| Vertex colors | 5,085.22 → 902.10 | 0 → 0 | 431,074 → 2,429,101 |

The predicate removes one 16-byte allocation per read. Valid vector/color
reads retain zero allocation traffic and run 2.35–5.63× as fast. Direct reads
have no reallocations on either version. The host is Windows on a 44-logical-
CPU Xeon E5-2696 v4, using rustc 1.98.1 and release optimization with LTO off.

Focused checks pass for complete populated capture frames, all Lua value
types in the predicate, numeric coercions, holes, extra entries, binary/NUL
strings, NaN/signed-zero bits, nested row failures and aliases, raw lookup
semantics, stack restoration and coroutine use. All 19 draw checks and seven
state-reader checks pass. The default-feature release library suite has 685
passes, 13 failures and one ignored performance benchmark, versus 681 passes
and the same 13 failures before this pass. All four new tests pass.

Every failure matches the preceding commit's retained normalized diagnostic;
the baseline log's hash matches its committed result receipt. Normalization
removes Rust source locations, workspace prefixes, temporary process/thread
identifiers and the backtrace hint. The result JSON records failure names,
diagnostic hashes, source/executable hashes and the build receipt hash.
Native fixtures, acceptance criteria and comparison tolerances are unchanged.

## Reproduction and scope

```powershell
cargo test -p deadsync-song-lua --lib --release --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_draw_capture.py --binary <test-executable> --baseline-commit 8c2c68c77 --out docs/performance/2026-10-19-song-lua-vector-batches.json
```

Run the executable with `--test-threads=1` for the library suite. The frozen
baseline includes the preceding draw implementation, decoder, scalar batch,
flat-vector helpers and nested-color helper. Shared unchanged enums/key storage
do not share any of the optimized readers.

The harness runs six alternating rounds with one warmup plus nine samples per
variant/workload, pinned to logical CPU 2. It measures four synthetic capture
cases (shallow, wrapped, dense and populated properties), plus the predicate,
four flat-vector sizes and vertex colors. The properties case includes colors,
shadows, rotation, sizes, texture coordinates and effect timing. Direct read
samples perform 20,000 reads of unchanged tables. Fixture setup and teardown
are excluded. Throughput units are scene frames or reads, as marked in the JSON;
these are not whole-song or corpus-wide speedup claims.

Both variants use the same counted System allocator. Rust GlobalAlloc traffic
includes mlua and excludes direct native malloc. Windows QueryThreadCycleTime
records scheduled thread cycles, including kernel execution. Timing/cycle
measurements include accounting overhead. Requested bytes describe allocation
traffic, not RSS. Timed runs have no concurrent build or other benchmark.
