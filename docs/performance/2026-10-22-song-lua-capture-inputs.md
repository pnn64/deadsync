# Song Lua borrowed capture inputs

Capture preparation borrows existing actor handles and transfers owned sound paths into output events. CPU changes and allocation traffic for every control are reported below; these measurements do not establish a whole-game or frame-rate improvement.

The independent quiet confirmation improves the 128-long-sound-path scope by 35.57% in pooled throughput and reduces worker cycles by 26.12%, with 128 fewer allocations and 526,592 fewer allocated bytes per capture. Actor input scopes remove one allocation per nonempty capture and up to five reallocations, but their timing changes are mixed. For example, the 128-actor untouched indexed scope is 7.18% slower by pooled medians and 1.62% slower by paired medians; its paired band spans zero. These changes are retained for the measured allocation reduction, without claiming a CPU win for each actor scope.

The complete offline capture fixture removes 49 allocations and 15,456 allocated bytes per compile. Its pooled throughput is +3.77%; the live control is -1.00%. Every complete workload's paired band spans zero, so startup speed remains uncertain.

Baseline: `d418006b78d0f62b1c398cfd339116d6c04d660b`. Three ownership changes follow the transient-allocation and repeated-copy guidance in [rust-performance.md](../../rust-performance.md):

1. Indexed actor snapshots consume a borrowed iterator instead of first cloning every handle into a temporary vector. The retained semantic snapshots still own their required handles and state.
2. Touched-actor capture and full function-action capture feed their filtered actor references directly into the existing collector. Actor order, aliases, filtering, flushes, errors and restoration remain in their original sequence.
3. Compiled function actions move captured `PathBuf` values into sound events instead of cloning them and discarding the originals.

No unsafe code is added. The snapshot helper accepts borrowed iterables; existing slice callers remain supported. No cache, extra production wrapper or alternate behavior path is introduced.

Validation: the saved baseline passes 823 tests with zero failures and 36 ignored benchmarks. The candidate passes 825 tests with zero failures and 37 ignored benchmarks, including new independent frozen-body comparisons for empty/partial/full actor sets, aliases, errors, clock/state restoration and sound event order/metadata. Complete fixture snapshots match exactly across all timed variants and selected pairs.

The new full fixture captures 256 overlay function eases, actor actions and 32 repeated sound calls with no unsupported captures. Both offline and live compilation are measured. Six alternating paired rounds retain 54 samples per variant/workload: 15 scoped workloads and eight complete fixtures (2,484 selected samples). All attempted batches remain in the evidence.

| Scoped workload | Before captures/s | After captures/s | Throughput | Worker cycles/capture before -> after | Allocations/capture before -> after | Reallocations/capture before -> after | Allocated bytes/capture before -> after |
|---|---:|---:|---:|---:|---:|---:|---:|
| indexed_empty | 349,440 | 377,804 | +8.12% | 6,221.5 -> 5,770.4 | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| indexed_one | 31,517 | 32,086 | +1.80% | 69,351.7 -> 68,206.2 | 31.766 -> 30.766 | 0.016 -> 0.016 | 5,125.9 -> 5,101.9 |
| indexed_128_untouched | 630 | 585 | -7.18% | 3,476,185.5 -> 3,744,387.1 | 3,586.9 -> 3,585.9 | 5 -> 5 | 595,053.5 -> 591,981.5 |
| indexed_128_all | 297 | 293 | -1.54% | 7,371,449.7 -> 7,483,346.7 | 3,578.4 -> 3,577.4 | 10.016 -> 10.016 | 666,593.2 -> 663,521.2 |
| touched_empty | 166,191 | 170,077 | +2.34% | 13,164.9 -> 12,799.6 | 6.047 -> 6.047 | 0 -> 0 | 298.672 -> 298.672 |
| touched_one | 23,548 | 23,979 | +1.83% | 93,026.8 -> 91,296.6 | 52.781 -> 51.781 | 2 -> 2 | 5,862.5 -> 5,734.5 |
| touched_128_one | 20,232 | 21,410 | +5.82% | 108,086.6 -> 102,288.7 | 52.656 -> 51.656 | 2 -> 2 | 5,829.8 -> 5,701.8 |
| touched_128_all | 247 | 256 | +3.81% | 8,875,489.4 -> 8,543,085.7 | 4,920.7 -> 4,919.7 | 280 -> 275 | 743,328.3 -> 735,264.3 |
| action_empty | 21,452 | 21,373 | -0.37% | 101,992.0 -> 102,482.5 | 25.641 -> 25.641 | 7 -> 7 | 5,219.0 -> 5,219.0 |
| action_128_one | 10,278 | 10,803 | +5.11% | 213,141.4 -> 202,459.6 | 73.391 -> 72.391 | 9 -> 9 | 10,824.3 -> 10,696.3 |
| action_128_all | 257 | 248 | -3.74% | 8,500,732.6 -> 8,835,785.5 | 4,945.8 -> 4,944.8 | 287 -> 282 | 750,316.5 -> 742,252.5 |
| sound_none | 23,794 | 23,760 | -0.14% | 92,064.8 -> 92,232.9 | 25.641 -> 25.641 | 7 -> 7 | 5,219.0 -> 5,219.0 |
| sound_one | 23,148 | 22,730 | -1.81% | 94,611.4 -> 96,197.6 | 29.641 -> 28.641 | 7 -> 7 | 5,399.0 -> 5,381.0 |
| sound_128 | 7,407 | 8,012 | +8.17% | 295,570.5 -> 273,289.5 | 283.297 -> 155.297 | 19 -> 19 | 21,952.1 -> 19,648.1 |
| sound_long_128 | 4,135 | 5,606 | +35.57% | 528,724.1 -> 390,614.0 | 283.266 -> 155.266 | 19 -> 19 | 1,070,526.4 -> 543,934.4 |

| Scoped workload | Median paired throughput | 95% pair-bootstrap band |
|---|---:|---:|
| indexed_empty | +4.74% | +1.25% to +20.91% |
| indexed_one | +1.65% | -5.54% to +6.71% |
| indexed_128_untouched | -1.62% | -10.41% to +1.12% |
| indexed_128_all | +0.16% | -3.74% to +4.26% |
| touched_empty | -0.85% | -12.15% to +6.26% |
| touched_one | +1.69% | -8.18% to +10.13% |
| touched_128_one | +1.18% | -3.29% to +6.59% |
| touched_128_all | +0.72% | -2.38% to +8.84% |
| action_empty | +0.50% | -7.73% to +5.22% |
| action_128_one | +0.52% | -0.54% to +7.89% |
| action_128_all | -0.70% | -1.93% to +2.11% |
| sound_none | -0.26% | -5.99% to +1.77% |
| sound_one | -0.18% | -1.96% to +2.03% |
| sound_128 | +7.67% | +2.78% to +10.57% |
| sound_long_128 | +33.54% | +31.17% to +38.83% |

| Complete library workload | Before compiles/s | After compiles/s | Throughput | Worker cycles/compile before -> after | Allocations/compile before -> after | Allocated bytes/compile before -> after |
|---|---:|---:|---:|---:|---:|---:|
| offline_basic | 5.06 | 4.96 | -1.84% | 432,350,246 -> 440,511,364 | 252,087 -> 252,087 | 39,435,494 -> 39,435,494 |
| offline_capture | 2.16 | 2.25 | +4.54% | 1,011,518,453 -> 965,913,698.5 | 382,835 -> 382,827 | 48,283,384 -> 48,282,360 |
| live_wrappers | 1.66 | 1.70 | +2.50% | 1,311,587,800.5 -> 1,276,940,930 | 710,003 -> 710,003 | 120,011,915 -> 120,011,915 |
| live_actors | 2.05 | 2.06 | +0.37% | 1,064,277,915 -> 1,060,259,409 | 595,935 -> 595,935 | 97,414,961 -> 97,414,961 |
| live_two_layers | 1.22 | 1.22 | -0.23% | 1,787,122,572.5 -> 1,791,508,510 | 1,164,446 -> 1,164,446 | 201,642,899 -> 201,642,899 |
| live_timing4096 | 1.71 | 1.73 | +0.96% | 1,278,852,767 -> 1,266,519,610.5 | 718,029 -> 718,029 | 121,119,850 -> 121,119,850 |
| offline_capture_inputs | 1.99 | 2.06 | +3.77% | 1,102,402,968.5 -> 1,060,053,522 | 451,611 -> 451,562 | 61,600,557 -> 61,585,101 |
| live_capture_inputs | 2.95 | 2.92 | -1.00% | 741,602,702.5 -> 749,471,668 | 297,726 -> 297,726 | 47,753,597 -> 47,753,597 |

| Complete workload | Median paired throughput | 95% pair-bootstrap band |
|---|---:|---:|
| offline_basic | +0.08% | -7.02% to +9.96% |
| offline_capture | +3.94% | -1.32% to +6.28% |
| live_wrappers | +0.80% | -0.87% to +7.13% |
| live_actors | +0.40% | -2.31% to +5.55% |
| live_two_layers | -0.61% | -1.58% to +10.91% |
| live_timing4096 | +0.96% | -2.01% to +2.74% |
| offline_capture_inputs | +2.74% | -3.52% to +9.64% |
| live_capture_inputs | -1.76% | -5.60% to +1.80% |

The paired table compares the medians within each short alternating batch. Its deterministic bootstrap resamples six pair effects 10,000 times; the band is descriptive and does not account for all desktop or build-layout effects. Pooled and paired values are both retained.

Compiler audit: selected batches with observed compiler overlap or audit errors: scopes. Each batch uses the first attempt without observed overlap/error, or its third attempt if none is clean; selection never uses performance values.

Independent scoped confirmation: first clean attempt 1. The scoped table uses that confirmation when available. The original three overlapping scoped attempts and every confirmation attempt remain in JSON; the original campaign is unchanged.

Reproduction: preserve the `compile_pipeline` example and library test executable from the baseline. Build the candidate with `cargo +1.98.1`, `--release --locked --offline -j 2 --config profile.release.lto=false`. Run `scripts/benchmark_song_lua_compile.py --suite capture_inputs --binary TEST_EXE --rounds 6 --out SCOPES_JSON`. Compare saved pipeline executables using `scripts/benchmark_song_lua_pipeline.py --suite startup` and `--suite capture_inputs`, with six alternating pairs and nine samples per variant/case. Single-pair batches alternate `--reverse-first`.

Measurement limits:

- Windows `QueryThreadCycleTime` records scheduled worker CPU cycles including kernel execution, not user-only PMU instructions. CPU 2, 32 MiB stacks, rustc 1.98.1, release opt-level 3, LTO disabled and static MSVC runtime are shared across variants.
- Counted System allocation overhead is included. Scoped counters cover current-thread Rust GlobalAlloc, including mlua; whole-compile counters cover all-process Rust allocation traffic. Native malloc, retained memory and RSS are excluded. Allocated bytes include reallocation traffic, rather than only the final buffer size.
- Scoped measurements prepare VMs, actors, clocks, native callbacks and path strings outside timing. One warmup and full collection precede timing; GC is stopped. Each retained sample performs 64 captures, including callback effects, snapshots/restoration and temporary/result destruction. VM/input destruction is excluded.
- Sound scopes inject prepared strings into the real action-capture log and run the production action compiler. Event vector capacity is warmed; event destruction is included. Filesystem lookup and audio decoding/playback are excluded from those scopes.
- Complete fixtures measure compilation/startup, excluding output formatting and final DTO/VM destruction. Unit scopes use panic unwinding; normal compiler executables use panic abort. Steady-state frame speed and game FPS are not measured.
- One-second Toolhelp snapshots audit visible cargo/rustc/cl/link/cmake/ninja processes. Shorter overlaps and other desktop activity may be missed; version probes can conservatively flag completed timing batches.

Evidence: [JSON](2026-10-22-song-lua-capture-inputs.json) contains raw retained measurements, source/binary/build receipt hashes, full-output fingerprints and all compiler audits. Raw reports and logs remain under `target/song-lua-capture-inputs-campaign-20261022`.
