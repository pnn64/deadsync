Stereo-panned SFX now use a runtime-selected hardware FMA loop for general gains on supported x86/x86-64 CPUs. This path routes two-player mine sounds to the owning player's channel. The exact-product fast path returns before feature detection; the original portable fallback remains. The 21-line production addition reuses the existing private FMA module and its scoped unsafe boundary, with safe slice traversal and no inline assembly, explicit SIMD intrinsics, dependency or allocation.

The actual Cargo-built public mixer improves **2.773×** for one left-panned voice at default gain (approximately 0.9) and 1,024 interleaved samples, equivalent to 512 stereo frames. That is about **64% less CPU cost** for this workload. Six paired process medians range from **2.685× to 2.931×**. Independent median cycle costs are **2,806.4 before → 988.4 after**; the quotient differs from the median of paired ratios. Full audio-callback time, latency and application frame rate are unmeasured.

The [integrated raw data](2026-11-08-panned-mixer-fma.csv) contains all 1,800 rows. The [summary](2026-11-08-panned-mixer-fma-summary.csv) retains cycles, wall time, paired ratios, ranges and CPU-cost changes for all 15 cases. The existing benchmark's historical `panned-control` label is the optimized target in this comparison. Other cases exercise centered mixing or no active voices.

| Integrated workload | Median speedup | Six-pair range |
| --- | ---: | ---: |
| Panned target, 1,024 samples, one voice | 2.773× | 2.685–2.931× |
| Centered default gain, 32 samples | 0.989× | 0.950–1.116× |
| Centered default gain, 128 samples | 1.015× | 0.911–1.042× |
| Centered default gain, 1,024 samples | 0.994× | 0.946–1.018× |
| Centered default gain, 4,096 samples | 0.998× | 0.967–1.030× |
| Centered default gain, eight voices | 1.003× | 0.946–1.028× |
| Centered default gain, 32 voices | 0.990× | 0.934–1.031× |
| Centered gain 0.7, 1,024 samples | 1.025× | 0.998–1.036× |
| Centered unity, 32 samples | 1.074× | 0.991–1.158× |
| Centered unity, 128 samples | 1.020× | 0.987–1.047× |
| Centered unity, 1,024 samples | 0.953× | 0.934–1.018× |
| Centered unity, eight voices | 0.979× | 0.916–1.009× |
| Centered exact-product gain 0.375 | 1.001× | 0.931–1.040× |
| Centered zero gain | 1.007× | 0.933–1.029× |
| No active voices | 1.039× | 0.996–1.088× |

The centered unity/1,024-sample median is roughly 5% more CPU cost, with a range crossing parity. Other unchanged controls also move in both directions. These small shifts are retained and do not establish additional gains or prove every unchanged path is unaffected. An unrelated compiler job was present during the integrated measurements; process inventories are retained. No builds or tests from this change overlapped timing.

Earlier [isolated-crate prototype measurements](2026-11-08-panned-mixer-fma-prototype.csv) cover 20 workloads and 2,400 rows. They found 2.911× left and 2.808× right at 1,024 samples/one voice, and 3.207× left and 3.073× right with eight voices. These are separate exploratory measurements, not additional integrated results. Production mixer source matches that prototype byte-for-byte.

The baseline is commit `5be1c4d54`, including the previous centered-mixer optimization. The unchanged `mixer_performance` example was built and archived before the production patch, then rebuilt afterward using the same Cargo workspace, benchmark and compiler settings. Six process pairs alternate executable order and rotate fixture order equally. Each process pins its thread to logical CPU 2, runs ten rounds and excludes the first as warmup. CPU cycles use `QueryThreadCycleTime`; wall time uses `Instant`. Allocation/setup is outside timing; voice cursor reset, output clearing, the complete public `mix_active_sfx` call and output consumption are inside. All retained rounds and outliers remain in the CSVs.

Host: Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.99.1 / LLVM 23.1.1. The repository's release settings remain opt-level 3, fat LTO, generic x86-64 CPU and static CRT. Disassembly of the measured production executable confirms `vfmadd231ps` processing four stereo frames at a time, followed by stores only to the selected channel. A scalar FMA loop handles the tail.

Validation passes **136 release tests**, with 14 opt-in benchmarks/trace tests ignored. The new debug boundary test also passes. Expanded exhaustive-i16 tests cover general gains and exceptional float bits. The new test checks 52,272 combinations of left/right routing, source offsets, source/destination lengths 0–65 and gains 0.7/0.9/1.3, comparing exact bits in both channels, odd tails and both guard samples. It also checks zero heap churn during mixing. Existing voice-progress, centered-mixer and callback tests pass. ARM64 and i686 Windows release compile checks pass. Clippy completes with only the existing unchanged fallback traversal and telemetry warnings.

Reproduction from the repository root:

```powershell
cargo build --release --locked -p deadlib-audio-core --example mixer_performance
& target/release/examples/mixer_performance.exe
cargo test --release --locked -j 2 -p deadlib-audio-core -- --test-threads=1
cargo test --locked -p deadlib-audio-core --test panned_mixer mixer::kernel::general_gain_preserves_channels_tails_and_guard_samples -- --exact --test-threads=1
cargo check --release --locked -p deadlib-audio-core --target aarch64-pc-windows-msvc
cargo check --release --locked -p deadlib-audio-core --target i686-pc-windows-msvc
cargo clippy --release --locked -p deadlib-audio-core --all-targets --no-deps
```

Local before/after executables, source snapshots, per-process CSVs, test logs, disassembly, process inventories, the alternating-run script and hashes are retained under `target/panned-mixer-integrated-2026-11-08/`. The earlier exploratory workspace is under `target/panned-mixer-fma-2026-11-08/`. Baseline mixer SHA-256: `2d5b6658934ac44b7045cee875550e7d31ad6c9c69dc8c37d6f8d9950b0550ed`; updated mixer: `30814144803cc48adac01a900340b4a2a6f0ec0130729bb44c99ead6d6159c2a`.
