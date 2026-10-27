# CoreAudio callback timing

Baseline: `b4ecd1e987608c129d1f7bdd96fd009b33e842e5`. This batch changes only
`deadlib-audio-backend-coreaudio` production code. Three independently measured
changes preserve the integer timing values published by the callback.

| Change and workload | Before cycles/update | After cycles/update | CPU throughput |
| --- | ---: | ---: | ---: |
| Narrow Mach scaling, 1/1 timebase | 30.527 | 25.400 | 1.202x |
| Narrow Mach scaling, 125/3 timebase | 30.638 | 24.020 | 1.276x |
| Narrow latency conversion, 160 ms at 48 kHz | 8.962 | 3.900 | 2.298x |
| Narrow latency conversion, overflowing product | 9.229 | 2.696 | 3.423x |
| Reuse buffer duration, 128-frame callback/buffer | 49.776 | 30.060 | 1.656x |
| Reuse buffer duration, 256-frame callback/128-frame buffer | 49.605 | 30.173 | 1.644x |

Each row compares consecutive stages, with twelve alternating before/after
process pairs. A separate sixteen-pair baseline/final comparison gives:

| Workload | Before cycles/update | After cycles/update | CPU throughput |
| --- | ---: | ---: | ---: |
| Mach 1/1 | 33.265 | 28.179 | 1.181x |
| Mach 125/3 | 32.575 | 24.666 | 1.321x |
| Mach overflowing product | 97.580 | 97.266 | 1.003x |
| Latency, normal | 10.611 | 4.355 | 2.437x |
| Latency, overflowing product | 10.475 | 2.906 | 3.604x |
| Callback timing, fixed period | 55.969 | 33.251 | 1.683x |
| Callback timing, larger period | 56.363 | 34.113 | 1.652x |

These measure the production arithmetic on Windows x86_64, including the actual
callback telemetry calculation block. They do **not** measure rendering, OS calls,
CoreAudio playback, or native Mac performance. No end-to-end playback speedup is
claimed. Both callback workloads allocate and free zero bytes before and after.

## Removed work and exactness

1. `scale_mach_time` uses a checked 64-bit multiply and 64-bit division whenever
   the product fits. That result is exactly the original 128-bit quotient and
   cannot need saturation. Overflow retains the original calculation, including
   its saturation. Zero denominators still panic; calibration already rejects
   them. The overflow control is 1.6% slower in the staged comparison and flat in
   the final comparison; no overflow-path improvement is claimed.
2. `nanos_to_frames` removes 128-bit multiplication, division and saturation.
   A fitting product uses the identical quotient and clamp in 64 bits. If the
   product overflows 64 bits, its quotient by one billion is at least
   18,446,744,073, which exceeds `u32::MAX`; returning `u32::MAX` is exact.
   Multiplication by a zero sample rate still returns zero.
3. The immutable buffer duration is calculated once before callback registration
   and reused for startup publication. Each callback now computes only its period
   duration. Integer frame-to-nanosecond conversion is monotone, so
   `f(max(buffer, period)) == max(f(buffer), f(period))`, including the zero-rate
   case. This removes one runtime division per callback and reuses the existing
   `buffered_frames` value instead of spelling out the same maximum twice.

Production code changes are 12 added and 5 removed lines after formatting. The
execution path is shorter despite the small source increase needed for checked
arithmetic. No dependencies, public interfaces, shared state, or unsafe blocks are
added. Atomic latency loads, clock fallback, logging, and publication order are
unchanged.

## Verification and measurement

`scripts/benchmark_coreaudio.py` extracts all three timing functions and the
callback arithmetic directly from `lib.rs`; it does not benchmark handwritten
optimized substitutes. It archives source, generated harness, support files,
executable hashes and test output for each stage. Frozen original formulas in
`scripts/coreaudio_timing_tests.rs` provide the differential reference.

Five checks pass in debug and optimized builds: Mach scaling, zero-denominator
panic, latency conversion, all seven callback telemetry arguments, and allocation
freedom. The differential checks include one million randomized comparisons plus
explicit boundaries: multiplication overflow, saturation thresholds, rounding,
zero sample rate, zero/large/wrapping callback frame counts, and differing callback
and device buffer sizes. The original device-latency conversion test remains.

`cargo check --all-targets --locked --offline` passes for both
`aarch64-apple-darwin` and `x86_64-apple-darwin`. ARM64 Clippy passes with the
existing `collapsible_if` warning in untouched device enumeration; strict
`-D warnings` fails on that same existing warning. Rust formatting and Python
compilation pass. Native Mac execution is still unverified.

Measurements use an Intel Xeon E5-2696 v4, Windows 11, rustc 1.99.1 / LLVM 23.1.1,
baseline CPU target, optimization level 3, thin LTO, one codegen unit, and the static
MSVC runtime. The existing benchmark helper pins its thread to logical processor
2 and uses `QueryThreadCycleTime`; wall time is secondary. Each process discards
one warmup and takes the median of eleven samples. Each sample runs 1,024 batches
of 256 updates. TSV `cycles_per_op` and `ns_per_op` refer to a **256-update batch**;
tables above divide the cycle median by 256. Setup, allocation probes, compilation
and process creation are outside measured regions. This task's compilations had
finished before retained comparisons; unrelated shared-machine work is
uncontrolled. Unchanged controls vary by several percent, so no small incidental
gains are claimed. Early exploratory measurements were replaced with these final
runs after formatting the harness identically for every archive.

The four TSVs retain all 728 process/workload records, wall time, allocation
counts and executable hashes. `2026-10-26-coreaudio-timing.json` records all stage
source hashes, exact patches from the baseline, compiler commands, test output
and normalized summaries. Raw logs and binaries remain in
`target/coreaudio-timing/`.

## Reproduction

Save the baseline crate source from the named commit to a file, then use
`--source PATH` when archiving it. For intermediate stages, apply only the Mach
change, then the latency change; the JSON contains their exact patches. The final
stage uses the current source by default. Keep the same harness and support files
for every stage.

```powershell
python scripts/benchmark_coreaudio.py --archive target/coreaudio-timing/final-before --source PATH_TO_BASELINE
python scripts/benchmark_coreaudio.py --archive target/coreaudio-timing/final-mach --source PATH_TO_MACH_STAGE
python scripts/benchmark_coreaudio.py --archive target/coreaudio-timing/final-latency --source PATH_TO_LATENCY_STAGE
python scripts/benchmark_coreaudio.py --archive target/coreaudio-timing/final-after
python scripts/benchmark_coreaudio.py --debug --archive target/coreaudio-timing/debug
python scripts/benchmark_sampling.py --before target/coreaudio-timing/final-before/timing.exe --after target/coreaudio-timing/final-mach/timing.exe --test-filter benchmark_coreaudio_timing --runs 12 --log-dir target/coreaudio-timing/runs --output docs/performance/2026-10-26-coreaudio-mach.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-timing/final-mach/timing.exe --after target/coreaudio-timing/final-latency/timing.exe --test-filter benchmark_coreaudio_timing --runs 12 --log-dir target/coreaudio-timing/runs --output docs/performance/2026-10-26-coreaudio-latency.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-timing/final-latency/timing.exe --after target/coreaudio-timing/final-after/timing.exe --test-filter benchmark_coreaudio_timing --runs 12 --log-dir target/coreaudio-timing/runs --output docs/performance/2026-10-26-coreaudio-buffer.tsv
python scripts/benchmark_sampling.py --before target/coreaudio-timing/final-before/timing.exe --after target/coreaudio-timing/final-after/timing.exe --test-filter benchmark_coreaudio_timing --runs 16 --log-dir target/coreaudio-timing/runs --output docs/performance/2026-10-26-coreaudio-final.tsv
cargo check -p deadlib-audio-backend-coreaudio --all-targets --locked --offline --target aarch64-apple-darwin --target-dir target/coreaudio-check
cargo check -p deadlib-audio-backend-coreaudio --all-targets --locked --offline --target x86_64-apple-darwin --target-dir target/coreaudio-intel-check
cargo clippy -p deadlib-audio-backend-coreaudio --all-targets --no-deps --locked --offline --target aarch64-apple-darwin --target-dir target/coreaudio-check
```
