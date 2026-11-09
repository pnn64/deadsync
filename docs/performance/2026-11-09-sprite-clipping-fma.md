Rotated sprite clipping now dispatches to an FMA-enabled Rust helper when the CPU supports it. The helper keeps corner transforms and all four rectangle-edge loops together, allowing LLVM to pack position, UV and color interpolation. The fallback retains the existing compact edge calls. Geometry, interpolation order, buffer ownership and output construction remain unchanged; no inline assembly, intrinsics or extra allocations are introduced. The crate continues to deny unsafe code by default, with local exceptions for the guarded target-feature call and helper.

Archived Cargo release binaries measure **1.518x** faster partial clipping with reused output storage, about **34.1% fewer CPU cycles**. Partial clipping with fresh allocation measures **1.374x**. The [integrated raw data](2026-11-09-sprite-clipping-fma-raw.csv) retains all 3120 observations, and the [summary](2026-11-09-sprite-clipping-fma-summary.csv) includes runtime and forced-portable results. Mode 0 is runtime dispatch; mode 1 forces portable geometry.

| Workload | Before cycles/sprite | After cycles/sprite | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| rotated-contained | 570.4 | 443.4 | 1.289x | 1.018-1.403x |
| rotated-partial | 894.4 | 583.9 | 1.518x | 1.379-1.668x |
| rotated-rejected | 295.4 | 194.9 | 1.532x | 1.353-1.752x |
| axis-contained | 88.2 | 78.4 | 1.137x | 0.976-1.503x |
| axis-partial | 118.6 | 123.8 | 0.993x | 0.879-1.233x |
| axis-rejected | 89.0 | 80.4 | 1.150x | 0.949-1.428x |
| rotated-edge | 974.2 | 672.4 | 1.443x | 1.258-1.528x |
| rotated-thin | 739.3 | 493.4 | 1.505x | 1.097-1.732x |
| rotated-partial-fresh | 1,114.2 | 814.6 | 1.374x | 1.208-1.439x |
| rotated-contained-fresh | 770.6 | 647.4 | 1.163x | 1.076-1.310x |
| mixed-partial | 222.1 | 190.6 | 1.165x | 1.087-1.387x |
| quarter-turn-partial | 822.1 | 534.8 | 1.549x | 1.229-1.662x |
| half-turn-partial | 828.7 | 558.5 | 1.524x | 1.275-1.542x |

Partial rotated pooled wall medians are **408.3 ns to 266.6 ns** per sprite. These measure the complete `clipped_sprite_instance_to_world_rect` operation through a non-inlined wrapper: offset/corner calculation, runtime dispatch, polygon clipping, output construction, observation and either buffer recycling or fresh allocation/free. Fixture generation is outside timing. They are synthetic helper workloads, not whole-frame or gameplay CPU measurements.

This native composition path is used by hardware rendering backends, outside the software renderer and Lua. A concrete caller is the mode-selection demo in `crates/deadsync-theme-simply-love/src/screens/select_mode.rs`: receptors and layered arrows use `MaskDest()` with `rotationz`, including animated marathon spins. The benchmark covers varied positions/sizes, arbitrary rotation, thin clipping rectangles, fresh/reused storage, a mix with one rotated sprite in eight, and quarter/half turns. The unrotated path performs no new feature check.

Forced-portable controls compare the old geometry with the new shared body forced to `FMA = false`, on the same FMA-capable machine:

| Portable control | Median speedup | Six-pair range |
| --- | ---: | ---: |
| rotated-contained | 1.027x | 0.917-1.203x |
| rotated-partial | 0.987x | 0.922-1.134x |
| rotated-rejected | 1.089x | 0.961-1.250x |
| axis-contained | 1.045x | 0.904-1.481x |
| axis-partial | 0.943x | 0.786-1.253x |
| axis-rejected | 1.076x | 0.823-1.369x |
| rotated-edge | 1.002x | 0.945-1.071x |
| rotated-thin | 1.054x | 0.970-1.165x |
| rotated-partial-fresh | 0.997x | 0.929-1.075x |
| rotated-contained-fresh | 1.007x | 0.929-1.104x |
| mixed-partial | 0.973x | 0.875-1.165x |
| quarter-turn-partial | 1.036x | 0.960-1.067x |
| half-turn-partial | 1.001x | 0.957-1.121x |

Unrotated partial clipping has a **0.993x** runtime median, while its forced-portable control has a **0.943x** median (about **5.7% lower throughput**, or **6% more CPU cycles**). Both invoke unchanged unrotated arithmetic; no cause for the control difference was established. All observations and process ranges are retained, including regressions. Ranges are observed extrema, not confidence intervals or guarantees. These results support the measured rotated-path improvement without guaranteeing that every control is faster.

Compiler-presence records were collected at every process-pair boundary. One unrelated `rustc` process was present before the first pair; none was observed before pairs 1-5 or after the final pair. These snapshots are not continuous monitoring, and contention may affect timings despite thread affinity and alternating order. No unrelated process was modified or terminated. Separate non-FMA hardware, other architectures, and full application/GPU runtime measurements were not available.

The preceding extracted prototype measured **1.774x** for partial clipping (1.548-2.046x), **1.533x** with fresh allocation, and **1.300x** for contained sprites. Its axis-aligned contained median was **0.970x**, and forced-portable partial/thin controls were approximately **0.95x**. Those prompted integrated validation rather than being discarded. The [prototype raw data](2026-11-09-sprite-clipping-fma-prototype-raw.csv), [runtime summary](2026-11-09-sprite-clipping-fma-prototype-summary.csv) and [portable summary](2026-11-09-sprite-clipping-fma-prototype-portable-summary.csv) remain separate. An initial shorter run measured 1.754x partial clipping; its [raw data](2026-11-09-sprite-clipping-fma-initial-prototype-raw.csv), [runtime summary](2026-11-09-sprite-clipping-fma-initial-prototype-summary.csv) and [portable summary](2026-11-09-sprite-clipping-fma-initial-prototype-portable-summary.csv) are also retained.

The extracted assembly shows the source of the gain: the portable edge loop calls scalar `fmaf` eight times per generated intersection (two position, two UV and four color components). The specialized loop emits one eight-lane `vfmadd213ps` for that interpolation. Its complete rotated-clipping helper has ten scalar FMA instruction sites for corner calculations and four vector sites, one per rectangle edge, with no `fmaf` calls. The outer sprite entry still has its unchanged offset calculation and unrotated/portable branches. Static instruction sites are not dynamic calls per sprite. The specialized helper adds code (1091 lines of extracted assembly). The prototype's forced-portable entry has the same normal-path instructions as baseline; only labels and panic-location pointers differ.

Validation:

- **199 library release tests pass** before and after, with twelve opt-in benchmarks ignored. Every archived-binary benchmark invocation passes.
- The new differential compares **102,880 sprites / 6,211,500 floating fields** against frozen geometry from `aa242cf0f`, testing runtime and forced-portable paths. It checks output kind, count, vertex order, all instance/vertex fields, sprite index, mesh flags, sampler/cache metadata and unused pool entries. Retained output preserves the preallocated pointer and capacity; rejected/unrotated calls leave the unused buffer intact.
- Cases include rotation thresholds, quarter/half/full turns, contained/partial/rejected/zero/infinite rectangles, signed zeros, subnormals, extreme values, infinities, NaNs, arbitrary float bit patterns and both fresh/reused storage. Every non-NaN bit matches. **Zero NaN-bit differences** were observed in integrated release and unoptimized runs; unspecified NaN payload/sign selection is not guaranteed across compilers or architectures and is documented in the production helper.
- **176 masked-render tests pass**, including nested clips, sprite-to-mesh conversion, rejected sprites, render-frame comparisons and allocation/pool regressions.
- The new regression passes at opt-level 0 with debug assertions and overflow checks, directly compiling the presentation crate against cached release dependencies with fat LTO. This is not a full Cargo debug build. Final test-only Clippy cleanups do not alter its cases or expected results.
- Cargo check and Clippy for all crate targets complete. Existing warnings remain, including shared benchmark support loaded in multiple test modules; the new code's warnings were addressed. Changed production code and new test/reference files pass formatting and whitespace checks.

Measurements use Windows x86_64 MSVC on an Intel Xeon E5-2696 v4 (22 cores / 44 logical processors), rustc 1.99.1 / LLVM 23.1.1, release opt-level 3, fat LTO, static CRT and generic CPU settings. FMA is enabled only inside the runtime-guarded helper. Cargo test binaries use panic unwinding, while the extracted prototype used panic abort and one codegen unit, so their results are reported separately.

Six fresh before/after process pairs alternate binary order and rotate workloads by 0, 5, 10, 15, 20 and 25. Each workload measures ten rounds of 1024 batches of 64 sprites (65,536 operations), discarding round zero. Runtime/portable order rotates within each process. The benchmark thread is pinned to logical CPU 2; `QueryThreadCycleTime` measures CPU cycles and `Instant` wall time. Reported speedups are medians of six paired process-median ratios; absolute columns are independent medians and need not divide to the reported ratio. Own builds and correctness checks do not overlap timing.

The timing function, wrappers, fixtures and recycling body are identical in the archived before/after executables. The test-only portable adapter switches from frozen baseline geometry to the shared production body; final non-timed test cleanups remove a redundant conversion and express the same even-case condition with `is_multiple_of`. The frozen reference reproduces the original arithmetic; verification checks its function bodies against the baseline and confirms the shared production geometry only changes the clipper selection.

SHA-256:

- Before executable: `db1c0c1046eae99af1628fa7f48b743835189d2cbe6bc7194c54da754affbd16`.
- After executable: `4c24af6f1f2277f4ae85d3eac758af570819d6c165999a5ff9776a9f18eee89c`.
- Timing function: `2da0fdd92f9d080f93d739b4ff230f16270448b2dffc0aea66b0a19f735443fd`.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test masked_render -- --test-threads=1
$env:SPRITE_CLIP_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib compose::sprite_clip_performance::benchmark_sprite_clipping -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:SPRITE_CLIP_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

For before/after comparison, archive the test executable reported by `cargo test --release --locked -j 2 -p deadlib-present --lib --no-run --message-format=json` on each side of the production change, keeping the benchmark and fixtures identical and using the frozen geometry for the baseline portable adapter. Run the exact ignored benchmark from archived executables with all six rotations and alternating process order. Binaries, manifests, runners, source snapshots, assembly, compiler-presence records and validation logs remain in `target/sprite-clip-fma-audit-2026-11-09/`.
