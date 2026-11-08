# Song Lua compile finalization: flat wrapper storage and direct output construction

This pass starts from `bacdc3e37`. It prioritizes compiler preparation and complete fixture compilation, following M-HOTPATH, M-MEM-REUSE and M-INITIAL-CAPACITY in [rust-performance.md](../../rust-performance.md). It makes three changes in compile.rs:

1. Store wrapper children in one flat vector with parent offsets, replacing the vector allocated for each populated child list. The existing already-ordered fast path is unchanged. Reverse filling preserves original sibling order before the same stable source-key sort. Source propagation, parent-first traversal and error strings remain unchanged.
2. Move the original message, sound-path and loaded-style buffers into the final output layer. Earlier layers receive independent copies as before. Initialize layer metadata during construction, removing the later metadata pass. This avoids copying an owned payload and then discarding its original allocation.
3. Reserve each output's overlay and live-index vectors using the layer counts already computed for remapping. This removes growth/copying while preserving the same sequence of actors and retained scene indices.

The flat representation removes many heap-backed child-list objects; it adds explicit offset arithmetic, covered by exhaustive small graphs and larger differential cases. No unsafe code, mutable cache, dependency or public API is added. Lua execution, host setup, actor decoding, graph validation messages, index remapping, draw coalescing and capture routing are unchanged. Only the two finalization helpers and their test module differ from the baseline compiler source.

## Measurements

The [raw measurements and validation receipt](2026-10-20-song-lua-compile-finalize.json) contain six alternating-order rounds and 54 retained samples per variant/workload: 1,512 samples across fourteen workloads. Tables show medians before → after. Both variants run in the same release test executable, with pinned rustc 1.98.1 and the same counted System allocator. The complete baseline compiler is frozen from git, with parent visibility added to two private helpers. Both compiler versions call the same unchanged host/runtime implementations.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty order control | 27.4 → 27.0 | 0 → 0 | 0 → 0 | 0 → 0 | 70,136,986.30 → 72,112,676.06 | +2.8% |
| 128 actors already ordered | 433.4 → 451.2 | 1 → 1 | 0 → 0 | 1,024 → 1,024 | 5,002,442.60 → 4,800,750.12 | -4.0% |
| 128 late wrappers + owned actors | 36,754.9 → 12,070.8 | 134 → 6 | 5 → 0 | 17,432 → 9,264 | 59,315.55 → 180,858.02 | +204.9% |
| 1,024 late wrappers + owned actors | 338,208.3 → 98,815.7 | 1,031 → 7 | 8 → 0 | 147,480 → 81,968 | 6,464.87 → 22,161.82 | +242.8% |

An order operation borrows prepared actors, returns the traversal vector and drops it inside timing. A late-wrapper input contains N ordinary actors followed by N wrappers, each ordinary actor owned by its corresponding later wrapper. These inputs force the original child-list path. The 1,024-wrapper case changes throughput by +242.8% and cycles by -70.8%; per-round throughput changes range from +211.8% to +269.3%. Allocations change from 1031 to 7. Empty/already-ordered controls retain their original algorithm. This primitive result is not a complete compile speedup estimate.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Empty layer control | 12,734.4 → 12,520.1 | 3 → 3 | 0 → 0 | 16,115 → 16,115 | 170,144.89 → 174,624.83 | +2.6% |
| 128 messages/paths, one layer | 89,466.8 → 33,578.4 | 261 → 3 | 0 → 0 | 28,823 → 16,115 | 24,438.67 → 65,216.28 | +166.9% |
| 128 messages/paths, three layers | 213,676.4 → 173,590.2 | 779 → 521 | 0 → 0 | 86,469 → 73,761 | 10,228.38 → 12,616.80 | +23.4% |
| 128 actors, one layer | 388,746.5 → 309,801.9 | 7 → 7 | 7 → 0 | 497,171 → 338,675 | 5,630.95 → 7,063.90 | +25.4% |
| 128 wrappers/actors, one layer | 463,086.7 → 362,099.6 | 140 → 12 | 13 → 0 | 994,859 → 668,451 | 4,708.86 → 6,042.52 | +28.3% |
| 128 wrappers/actors + 32 payload pairs, three layers | 498,902.9 → 341,704.9 | 342 → 148 | 26 → 0 | 1,353,525 → 706,977 | 4,360.01 → 6,397.86 | +46.7% |

Split operations consume prepared compiled DTOs and ownership vectors. Input preparation is excluded; output destruction is included. Payload cases contain 128 owned message strings and 128 PathBuf values, with no overlays, isolating the transfer from overlay capacity and wrapper storage. Moving the last copy removes 258 allocations for either one or three layers. Earlier layers still own independent payloads. Payload throughput in one layer changes by +166.9% and cycles by -62.5%, with per-round throughput changes from +134.9% to +213.2%.

The flat 128-actor input has no messages, paths or loaded-style string, and uses the unchanged already-ordered wrapper fast path. It isolates output-vector reservation: allocation count stays the same while reallocations change from 7 to 0. Its throughput changes by +25.4% and cycles by -20.3%, with per-round throughput changes from +12.7% to +144.4%. Wrapper/layer combinations measure the changes together. These gains are not independent additive estimates. Loaded-style ownership transfer is separately checked by pointer identity and mutation independence.

| Case | CPU cycles/operation | Allocations | Reallocations | Requested bytes | Operations/second | Throughput change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Basic offline fixture | 553,875,002.5 → 554,717,432.5 | 430,641 → 430,640 | 13,035 → 13,035 | 46,882,441 → 46,882,435 | 3.95 → 3.94 | -0.2% |
| 128 actors with two wrappers each | 1,487,686,049.5 → 1,484,368,641.0 | 1,247,307 → 1,247,306 | 27,984 → 27,968 | 144,533,896 → 143,733,842 | 1.47 → 1.47 | +0.1% |
| Existing 256-actor live fixture | 1,194,824,648.0 → 1,200,021,641.5 | 1,017,188 → 1,017,187 | 23,968 → 23,952 | 116,963,425 → 116,006,371 | 1.83 → 1.82 | -0.4% |
| Two wrapper-heavy layers | 2,359,173,620.5 → 2,355,834,449.0 | 2,074,683 → 2,074,682 | 41,540 → 41,508 | 242,492,022 → 240,890,664 | 0.93 → 0.93 | +0.2% |

Across these complete fixtures, aggregated throughput changes range from -0.4% to +0.2%. These results do not establish a meaningful whole-compile speedup. The retained improvements reduce finalization work and memory traffic; these measurements do not isolate the time spent in host setup, Lua execution or actor extraction.

Complete fixture operations call compile_song_lua_layers_with_default_host with actual files. The basic offline fixture is the existing tests/fixtures/song_lua/basic.lua. The wrapper fixture creates 128 quads with two AddWrapperState calls each. The actor-heavy fixture is the unchanged tests/perf/live_actors.lua, which has 256 animated actors, getter feedback, effects and recurring queues. The two-layer case compiles the wrapper fixture twice in one shared Lua scene. Context creation, file reads, host creation, Lua execution, startup commands, extraction and finalization are included. Simfile parsing, archive extraction and gameplay preparation outside this compiler entry point are not measured.

Each variant warms one complete compile before ten fresh-VM samples; the first sample is discarded. Every measured output is retained until the clock/counters stop, then its complete Debug DTO snapshot is compared exactly against the frozen compiler's expected output. Formatting and returned DTO/VM destruction are excluded. Debug represents the retained VM by its actor/tracked counts; the separate behavior check also advances each live result through eight identical frames and compares all states and emitted outputs. These timings measure the default-feature library test build, not an application renderer or native-trace replay.

The basic offline case changes throughput by -0.2% and cycles by +0.2%, with per-round throughput changes from -3.0% to +5.3%. The wrapper-heavy live case changes throughput by +0.1% and cycles by -0.2%, with per-round changes from -3.9% to +3.1%. The existing actor-heavy case changes throughput by -0.4% and cycles by +0.4%, with per-round changes from -4.6% to +1.4%. The two-layer case changes throughput by +0.2% and cycles by -0.1%, with per-round changes from -1.3% to +1.2%.

Negative aggregated throughput changes: `order_identity128` -4.0% (rounds -27.2% to +12.9%); `compile_basic_offline` -0.2% (rounds -3.0% to +5.3%); `compile_actors_live` -0.4% (rounds -4.6% to +1.4%)

The selected complete fixtures exercise compiler control flow and real actor state but are not a representative chart corpus. Payload-heavy synthetic splits quantify ownership-copy costs separately. These results do not establish a whole-game FPS improvement or the duration of the full test suite. Raw samples and per-round ranges retain timing variation rather than hiding slower cases.

## Behavior and validation

Five new checks pass. Wrapper checks enumerate every parent graph and wrapper-kind mask through four actors, comparing exact orders or exact error strings. Larger late-wrapper cases cover 1, 2, 8, 65 and 1,024 owned actors, including invalid parents. Split checks compare complete DTO snapshots across one, two and three layers, every primary layer, forward wrappers, actor proxies, screen indices, update tracks, stateful captures and immediate/non-immediate draw coalescing. Invalid ownership lengths/layers keep their original errors.

Ownership checks verify that the final layer retains the original message/path/style allocations and that mutating earlier layers leaves the final layer unchanged. Allocation checks verify removal of per-parent vectors, 258 payload-copy allocations and output-vector growth. Complete fixture checks compare all compiled DTO fields and eight live updates, including states and emitted operations; callback errors remain empty.

A fresh baseline was built from bacdc3e37 before applying the candidate: 769 passes, 15 failures and fifteen ignored benchmarks. The candidate has 774 passes, 15 failures and sixteen ignored benchmarks. All fifteen normalized failure diagnostics match exactly. The baseline executable was copied and hashed before rebuilding; its build receipt, source hashes and full-suite log are retained in the JSON evidence. The candidate receipt verifies source/executable hashes, all five new checks and every failure diagnostic. Normalization removes only Rust source locations, workspace prefixes, test thread/process IDs and the backtrace hint. Existing tests, fixture expectations and tolerances are unchanged.

## Reproduction

```powershell
cargo +1.98.1 test -p deadsync-song-lua --lib --release --locked --offline -j 2 --config profile.release.lto=false --no-run --message-format=json
python scripts/benchmark_song_lua_compile.py --binary <test-executable> --out docs/performance/2026-10-20-song-lua-compile-finalize.json
```

Run the executable with --test-threads=1 and RUST_MIN_STACK=33554432 for the full suite. The benchmark uses the same 32-MiB test-thread stack and pins logical CPU 2. Each retained sample performs 512 wrapper orders, 64 splits or one complete fixture compile. No build or other benchmark overlaps the authoritative measurements. Windows, Xeon E5-2696 v4, 44 logical CPUs; release optimization with LTO disabled. The compiler is pinned because the default toolchain changed during the surrounding work.

CPU cycles use Windows QueryThreadCycleTime, including scheduled kernel execution; these are not user-only PMU counts. Allocation accounting covers current-thread Rust GlobalAlloc, including mlua's normal Rust-backed Lua allocator, and excludes direct native malloc. Accounting overhead is included. Requested bytes are allocation/reallocation traffic, not RSS. Scoped input setup is excluded for primitive benchmarks, and any collection triggered inside timed compiler work is included.
