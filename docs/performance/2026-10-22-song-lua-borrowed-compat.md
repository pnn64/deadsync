# Song Lua borrowed compatibility inputs

Borrowing compatibility inputs improves measured callback throughput by 4.30% to 92.92% and removes temporary ownership. Initial complete fixture pooled throughput changes range from -5.31% to +0.82%; the short-pair recheck centers near zero. Every complete output snapshot matches exactly. Complete-compile and whole-game speedups are not established.

Baseline: `be95399d371c1ae946725cad47b7194cedb4868d`. This pass follows the transient-allocation and repeated-copying guidance in [rust-performance.md](../../rust-performance.md). Three production callbacks change:

1. `cleanGSub` borrows its three validated UTF-8 inputs before using the existing literal replacement operation.
2. `Var` borrows the first argument and returns a handle to the same valid Lua string. `GameCommand` still constructs its native table. All arguments remain held through the callback, including non-string fallback inputs.
3. `GetVersionParts` borrows its input or the product-version constant before using the existing parser and table constructor.

The existing string reader is shared with the compatibility string helpers. No unsafe code is added. Missing values, non-string inputs and invalid UTF-8 retain their original defaults; no new coercion or `__tostring` call occurs. Literal replacement, embedded NULs and Unicode behavior are unchanged.

The callback oracle compares the production globals against frozen original callback bodies, including all combinations of replacement inputs, long strings, unsupported value kinds, `GameCommand` result shape and explicit userdata collection. The complete fixture uses all three callbacks and makes their results observable in actor names and positions.

Validation: baseline and after library suites each pass 823 tests with zero failures and 36 ignored benchmarks. The normal-library compiler snapshots are compared exactly for every timed fixture run and across selected pairs. Additional finalizer and offline/live smoke checks passed; smoke timings are excluded.

The scoped table uses independent clean confirmation attempt 1. Six alternating paired rounds retain 54 samples per variant/workload. Scoped timings use the actual callbacks through prepared Lua loops of 8192 calls. VM setup, warmup, full collection and VM destruction are excluded; GC is stopped during timing and temporary/result destruction is included. Complete fixture timings cover normal-library compilation/startup; output formatting and final DTO/VM destruction are excluded.

| Callback workload | Before calls/s | After calls/s | Throughput | Worker cycles/call before -> after | Allocations/call before -> after | Bytes/call before -> after |
|---|---:|---:|---:|---:|---:|---:|
| gsub_short | 750,166 | 1,013,692 | +35.13% | 2,918.1 -> 2,162.0 | 8.000 -> 5.000 | 214.005 -> 183.005 |
| gsub_unicode | 687,482 | 923,521 | +34.33% | 3,184.2 -> 2,363.9 | 8.000 -> 5.000 | 228.005 -> 201.005 |
| gsub_long | 443,145 | 596,302 | +34.56% | 4,940.2 -> 3,667.5 | 8 -> 5 | 8,379 -> 4,264 |
| gsub_empty | 962,225 | 1,177,020 | +22.32% | 2,276.6 -> 1,862.5 | 4 -> 4 | 168 -> 168 |
| gsub_defaults | 1,697,296 | 2,055,993 | +21.13% | 1,286.4 -> 1,062.9 | 3 -> 2 | 200 -> 184 |
| var_short | 1,806,315 | 2,467,135 | +36.58% | 1,212.5 -> 885.342 | 3 -> 2 | 64 -> 56 |
| var_long | 1,029,598 | 1,986,276 | +92.92% | 2,125.5 -> 1,098.7 | 3 -> 2 | 4,152 -> 56 |
| var_empty | 2,155,846 | 2,601,791 | +20.69% | 1,013.2 -> 840.421 | 2 -> 2 | 56 -> 56 |
| var_invalid | 1,376,171 | 1,435,317 | +4.30% | 1,591.6 -> 1,519.5 | 4 -> 4 | 138 -> 138 |
| version_short | 747,493 | 831,245 | +11.20% | 2,919.4 -> 2,631.2 | 5 -> 4 | 243 -> 232 |
| version_long | 550,140 | 634,830 | +15.39% | 3,976.3 -> 3,445.1 | 5 -> 4 | 4,328 -> 232 |
| version_default | 999,000 | 1,071,018 | +7.21% | 2,192.7 -> 2,041.8 | 3 -> 2 | 181 -> 176 |

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles/compile before -> after | Allocations/compile before -> after | Bytes/compile before -> after |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 5.08 | 5.10 | +0.44% | 430,791,466.5 -> 428,138,702.5 | 252,087 -> 252,087 | 39,435,494 -> 39,435,494 |
| offline_capture | 2.29 | 2.28 | -0.32% | 956,482,115 -> 959,514,839.5 | 382,219 -> 382,219 | 48,222,660 -> 48,222,660 |
| live_wrappers | 1.75 | 1.71 | -2.49% | 1,249,555,228 -> 1,282,091,050.5 | 709,387 -> 709,387 | 119,951,191 -> 119,951,191 |
| live_actors | 2.06 | 2.07 | +0.47% | 1,058,959,858.5 -> 1,055,275,443 | 595,319 -> 595,319 | 97,354,237 -> 97,354,237 |
| live_two_layers | 1.23 | 1.24 | +0.82% | 1,783,286,380 -> 1,768,627,061.5 | 1,163,830 -> 1,163,830 | 201,582,175 -> 201,582,175 |
| live_timing4096 | 1.75 | 1.74 | -0.98% | 1,247,166,185 -> 1,258,559,991 | 717,413 -> 717,413 | 121,059,126 -> 121,059,126 |
| offline_borrowed_compat | 2.86 | 2.70 | -5.31% | 766,059,828 -> 809,214,624.5 | 300,597 -> 290,356 | 42,254,130 -> 42,137,383 |
| live_borrowed_compat | 3.13 | 3.05 | -2.49% | 698,005,281.5 -> 716,001,427.5 | 289,822 -> 279,581 | 42,149,715 -> 42,032,968 |

The initial complete callback fixture pooled medians regress despite the allocation reduction. That concern was investigated before accepting the pass. Existing compiler stage timers show host creation dominates the workload; script execution itself falls from 10.94 ms to 10.17 ms in a diagnostic run. Diagnostic logging timings are excluded from the performance tables.

A separate run alternated variants more frequently: six clean chunks, three pairs per chunk, three samples per variant/case (54 per variant total). Its median paired effects are near zero and the exploratory pair-level intervals span zero. Neither a complete-compile speedup nor a consistent slowdown is established. The negative pooled observations remain visible in both tables. An earlier interleaved run with compiler overlap is retained in the JSON but is not used as clean confirmation.

| Target fixture recheck | Pooled throughput | Median paired throughput | Exploratory 95% paired bootstrap interval |
|---|---:|---:|---:|
| offline_borrowed_compat | -3.72% | +0.19% | -3.27% to +0.51% |
| live_borrowed_compat | -1.87% | -0.10% | -1.41% to +0.96% |

Production callback throughput improves in every scoped control, temporary allocation traffic falls, and the complete callback fixture removes 10,241 allocations and 116,747 allocated bytes per compile. Startup performance remains uncertain; no whole-game gain is claimed.

Compiler audit: 20 original campaign batch attempts and 1 independent scope confirmation attempts, all retained in the JSON evidence. Campaign selection uses the first attempt with no observed compiler overlap or audit error, or the third attempt if none is clean. An independently clean scope confirmation is used for the scoped table when available. Reported batches with observed interference: none. No selection uses performance values.

Reproduction: build the default-feature `compile_pipeline` example and library test executable with `cargo +1.98.1`, `--release --locked --offline -j 2 --config profile.release.lto=false`; preserve both pipeline executables. Run `scripts/benchmark_song_lua_compile.py --suite borrowed_compat --binary TEST_EXE --rounds 6 --out SCOPES_JSON`. Run `scripts/benchmark_song_lua_pipeline.py` against the saved executables with `--suite startup` and `--suite borrowed_compat`, six alternating pairs and nine samples per variant/case. Single-pair batches alternate `--reverse-first`.

Measurement limits:

- Windows `QueryThreadCycleTime` counts scheduled worker CPU cycles, including kernel execution; it is not a user-only hardware-PMU instruction counter.
- Scoped allocation counters cover current-thread Rust GlobalAlloc, including mlua's normal Rust-backed allocator. Complete-compile counters cover all-process Rust allocation traffic. Native malloc, retained memory and RSS are not measured. Counted System allocator overhead is included.
- Both variants use rustc 1.98.1, release opt-level 3, LTO disabled, static MSVC runtime, logical CPU 2 and 32 MiB worker stacks. Unit scopes use panic unwinding; normal compiler executables use panic abort.
- One-second Toolhelp process snapshots audit visible cargo/rustc/cl/link/cmake/ninja processes. Shorter overlaps and other desktop interference may be missed; version probes can conservatively mark an otherwise completed timing batch.
- Empty, default and long-input controls are included. Full fixture work measures startup compilation, not steady-state gameplay, audio playback or game FPS.

Evidence: [JSON](2026-10-22-song-lua-borrowed-compat.json) contains selected summaries, all batch attempts, source/binary/build-receipt hashes, full-output fingerprints, diagnostics and compiler audits. Raw uncompressed reports and logs remain under `target/song-lua-borrowed-compat-campaign-20261022`, `target/song-lua-borrowed-compat-confirmation-20261022` and `target/borrowed-compat-*`.
