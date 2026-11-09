The final float-to-i16 output loop now explicitly clamps scaled samples to the i16 range, maps NaN to zero, and performs a bounded unchecked conversion. This preserves Rust's saturating-cast behavior while allowing LLVM to convert four samples together. The safe private function contains one documented unsafe cast; the crate-wide unsafe lint remains denied elsewhere except for the existing CPU dispatch module. No inline assembly, architecture intrinsics, dependencies, allocations or CPU feature requirements are added.

The integrated Cargo release benchmark improves **1.868x** for a music callback with gain 0.8 and 1,024 interleaved samples (512 stereo frames), about **46.5% less CPU cost**. An SFX callback with gain 0.9 and 4,096 samples improves **2.044x**, about **51.1% less CPU cost**. Pure unity-gain music uses an existing direct i16 path, while silence and float output also bypass this conversion.

The [raw measurements](2026-11-08-pcm-conversion.csv) retain all 1,680 rows. The [summary](2026-11-08-pcm-conversion-summary.csv) includes cycle and wall-time medians, paired ranges and CPU-cost changes for all 14 workloads.

| Callback workload | Samples | Before cycles | After cycles | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: | ---: |
| Music, gain 0.8 | 32 | 472.5 | 407.4 | 1.163x | 1.091-1.813x |
| Music, gain 0.8 | 128 | 782.6 | 546.8 | 1.416x | 1.391-1.716x |
| Music, gain 0.8 | 512 | 2,043.5 | 1,137.7 | 1.787x | 1.695-3.003x |
| Music, gain 0.8 | 1,024 | 3,898.4 | 2,077.2 | 1.868x | 1.800-2.164x |
| Music, gain 0.8 | 4,096 | 15,220.8 | 8,076.2 | 1.885x | 1.834-1.996x |
| Unity music control | 512 | 385.0 | 386.1 | 0.998x | 0.966-1.045x |
| Unity music control | 4,096 | 2,426.7 | 2,460.6 | 0.974x | 0.919-1.029x |
| SFX, gain 0.9 | 512 | 2,155.7 | 1,193.9 | 1.802x | 1.729-1.831x |
| SFX, gain 0.9 | 4,096 | 14,604.5 | 7,158.8 | 2.044x | 2.012-2.048x |
| Music plus SFX | 512 | 2,545.4 | 1,568.2 | 1.632x | 1.583-1.661x |
| Music plus SFX | 4,096 | 18,334.5 | 10,785.4 | 1.703x | 1.673-1.724x |
| Float music control | 4,096 | 4,217.1 | 4,372.5 | 0.964x | 0.948-1.017x |
| Float SFX control | 4,096 | 4,992.8 | 5,009.2 | 0.995x | 0.993-1.058x |
| Silence control | 4,096 | 399.3 | 386.8 | 1.016x | 0.934-1.087x |

The float-music control shows about 3.7% more median CPU cost and the large unity-music control about 2.7% more, with both paired ranges crossing parity. Small shifts in unchanged paths are inconclusive; these results do not establish gains for those paths or prove all controls are unaffected. Every pair improves for each affected callback workload. All outliers, including a substantially slower baseline round/process for smaller music buffers, remain in the raw data. These measurements concern callback CPU work, not whole-game CPU, frame rate, device latency or physical-device behavior.

The baseline is `13fc88c8c`, including both previous SFX mixer optimizations. The same committed `render_performance` example was built and archived before and after applying the production change. Both use the repository's unchanged release profile: opt-level 3, fat LTO, panic abort, generic x86-64 CPU settings and static MSVC CRT. Six pairs alternate executable order and rotate fixture order equally. Each process pins its thread to logical CPU 2 and runs ten rounds, dropping only the first as warmup. Measurements use `QueryThreadCycleTime` and `Instant`. The displayed cycle costs are independent medians of process medians; speedups are medians of paired process ratios, so their quotients can differ.

Fixture allocation is outside timing. Music queue writes, optional SFX queuing, the complete public `RenderState::render` call, played-map draining and output consumption are inside timing. Buffers contain deterministic PCM values; SFX fixtures retain their source allocation. Each case runs `(12_000_000 / samples).clamp(3000, 60000)` iterations per round. No builds or tests for this change overlapped timing. Unrelated compiler jobs were present; process inventories before and after the run are retained with the local evidence.

Host: Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.99.1 / LLVM 23.1.1. Disassembly of the exact measured executables confirms four scalar `cvttss2si` conversions, scalar bounds operations and lane reassembly in the original loop. The new loop uses packed `maxps`/`minps`, a NaN mask, one `cvttps2dq`, and `packssdw` per four samples; scalar conversion handles the tail.

Earlier isolated-kernel measurements are retained separately in the [prototype raw CSV](2026-11-08-pcm-conversion-kernel.csv) and [summary](2026-11-08-pcm-conversion-kernel-summary.csv). They measured 2.864x at 1,024 samples and 2.953x at 4,096 samples across six processes, with variants alternating inside each round. Those are conversion-only results, not the integrated callback results above. The exploratory callback measurements, source snapshots and rejected safe-cast reformulations remain under `target/pcm-cast-audit-2026-11-08/`.

Safety follows from the explicit bounds: clamping maps infinities and out-of-range samples to -32,768 or 32,767; the NaN branch supplies zero. Every value reaching `to_int_unchecked::<i16>()` is therefore finite and within range, and truncation matches the original cast. Scaling and sample traversal order are unchanged. Unequal input/output lengths preserve the original zip behavior, and silence still clears the whole destination.

The committed tests compare every PCM truncation boundary and its neighboring floats, explicit signed zeros, subnormals, finite extrema, infinities and NaN payloads, plus 200,000 deterministic float patterns. Another test covers 71,824 source/destination-length, offset, tail, guard and silence combinations. The opt-in release test compares all 4,294,967,296 f32 bit patterns against the original saturating cast. The existing rendering trace covers output samples and timing/state changes across channel counts, gain changes, scheduling, generations and underruns.

Validation passes **140 release test executions**, with 16 opt-in benchmarks/exhaustive/trace tests ignored by the normal suite. The existing callback integration harness includes the render module, so it also runs the new regular tests. The exhaustive float-pattern test passes separately in 9.40 seconds, and both new regular tests pass in debug mode. Before/after render traces are byte-identical: **2,588,580 bytes**, SHA-256 `d20cd6571f3061a9330779228c16490a3e8ce04137a1c0144961c8defa255389`. ARM64 and i686 Windows release compile checks pass. Clippy completes with only the existing unchanged panned-fallback traversal and telemetry argument-count warnings. Formatting and whitespace checks pass. Full workspace tests and physical audio-device validation were not run.

Reproduction from the repository root:

```powershell
cargo build --release --locked -p deadlib-audio-core --example render_performance
& target/release/examples/render_performance.exe
cargo test --release --locked -j 2 -p deadlib-audio-core -- --test-threads=1
cargo test --release --locked -p deadlib-audio-core --lib render::pcm_conversion_tests::pcm_conversion_matches_every_f32_bit_pattern -- --exact --ignored --test-threads=1
cargo test --locked -p deadlib-audio-core --lib pcm_conversion_tests -- --test-threads=1
cargo check --release --locked -p deadlib-audio-core --target aarch64-pc-windows-msvc
cargo check --release --locked -p deadlib-audio-core --target i686-pc-windows-msvc
cargo clippy --release --locked -p deadlib-audio-core --all-targets --no-deps
```

For a paired rerun, archive builds of the same example against the baseline and updated source, then alternate them with matching rotation arguments `0, 3, 6, 9, 12, 15`. The local runner, executables, per-process CSVs, source snapshots, disassembly, logs and hashes are retained under `target/pcm-cast-integrated-2026-11-08/`.
