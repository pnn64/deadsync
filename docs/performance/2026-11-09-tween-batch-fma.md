Active tween segments with at least four prepared operations now select an FMA-enabled helper on supported x86 CPUs. The helper evaluates the existing easing function and interpolates the prepared properties, allowing LLVM to pack paired and RGBA interpolation. Smaller segments use the portable body; completed and sleeping segments return before dispatch. The formulas, property order, queue semantics, endpoint handling and prepared-storage reuse remain unchanged. There is no inline assembly, explicit intrinsic, approximation, global CPU-feature requirement or new allocation.

This is native animation in `deadlib-present`, outside the software renderer and Lua. The change follows `076196e8f`. An XY or RGBA operation counts as one prepared operation, so the threshold is four operations, not four scalar components. One feature-specific call is amortized across the entire active segment.

Archived Cargo release binaries measure **1.494x** faster large linear batches, **1.472x** faster large decelerating batches and **1.347x** faster large Bezier-eased batches: about **33.1%**, **32.1%** and **25.8%** fewer CPU cycles. Four-operation linear/decelerating fixtures improve **1.202x/1.215x**. These are batches of complete public `TweenSeq::update` calls with already prepared segments, not complete application/frame time or queue preparation. The [integrated raw data](2026-11-09-tween-batch-fma-raw.csv) retains all 1680 observations; the [summary](2026-11-09-tween-batch-fma-summary.csv) includes every workload and process range.

| Workload | Before cycles/64 actors | After cycles/64 actors | Median speedup | Six-pair range |
| --- | ---: | ---: | ---: | ---: |
| idle | 834.3 | 780.5 | 1.068x | 1.041-1.150x |
| sleep | 1,999.0 | 1,956.7 | 1.021x | 0.922-1.222x |
| linear-x | 2,511.7 | 2,535.4 | 1.003x | 0.921-1.038x |
| linear-xy | 2,654.6 | 2,554.3 | 1.047x | 0.932-1.078x |
| decelerate-xy | 3,013.6 | 3,056.6 | 1.003x | 0.954-1.163x |
| ease-x | 8,113.4 | 8,145.3 | 0.994x | 0.955-1.129x |
| ease-mixed | 9,723.2 | 9,649.0 | 1.002x | 0.956-1.057x |
| linear-many | 12,976.4 | 8,641.4 | 1.494x | 1.415-1.719x |
| decelerate-many | 13,761.8 | 9,119.9 | 1.472x | 1.314-1.731x |
| ease-many | 19,229.1 | 14,402.4 | 1.347x | 1.301-1.503x |
| linear-four | 5,046.1 | 4,363.7 | 1.202x | 0.907-1.361x |
| decelerate-four | 5,502.6 | 4,511.9 | 1.215x | 1.155-1.289x |
| linear-three | 3,743.7 | 3,514.9 | 1.038x | 0.808-1.246x |
| decelerate-three | 4,234.3 | 4,130.4 | 1.025x | 0.905-1.104x |

All small-workload medians are near parity or faster; single-property Bezier easing is 0.994x. Observed ranges are broad, and four-operation linear includes a 0.907x pair. An unrelated `rustc` process compiling `unit_host` was present immediately before and after timing. No builds or correctness checks launched for this change overlapped timing. The alternating process order reduces systematic order bias but does not remove contention or code-layout effects. The gains in the larger cases are consistent across all six pairs; the extrema are not confidence intervals or guarantees on other hardware. This measurement does not establish a net whole-engine CPU improvement for a particular mix of active animations.

The preceding standalone prototype measured 1.440x/1.492x for the four-operation fixtures and 1.529x-1.659x for the larger ones. Its final measurements used logical CPU 32, panic abort, one codegen unit and three source modules in one executable; the integrated Cargo test binaries use logical CPU 2, panic unwinding and separate before/after processes. The integrated results above are the production measurements. The [prototype raw data](2026-11-09-tween-batch-fma-prototype-raw.csv), [runtime summary](2026-11-09-tween-batch-fma-prototype-summary.csv) and [portable summary](2026-11-09-tween-batch-fma-prototype-portable-summary.csv) remain separate.

The final prototype's smaller controls included linear-X at 0.952x and three-operation decelerate at 0.935x (about 5% and 7% more cycles). Those costs motivated checking complete Cargo binaries. Its forced-portable medians range from 0.976x to 1.067x. Compiling feature detection false on an FMA-capable CPU is a control, not measurement on a physical non-FMA CPU. The [earlier four-operation threshold summary](2026-11-09-tween-batch-fma-earlier-threshold-four-summary.csv) and [three-operation experiment](2026-11-09-tween-batch-fma-earlier-threshold-three-summary.csv) are also retained; their local raw files remain in the audit directory. The three-operation experiment's linear-three result was 0.995x and its decelerate-three result was 1.044x with ranges crossing parity, so the committed threshold remains four.

The audited prototype RGBA arm contains four scalar `call fmaf` instructions plus shuffles. Its specialized counterpart uses two vector loads, `vsubps`, one `vfmadd213ps` and one vector store. XY/size/scale pairs also become packed FMA; scalar properties use scalar FMA. The specialized body has no `fmaf` calls; spring easing still calls `cosf`. Source verification confirms the three production dispatch/interpolation bodies match that measured prototype after comments and whitespace are removed. Preparation, property formulas, easing and the public queue-update body match the baseline unchanged. The complete assembly and extracted arms remain in the audit directory.

The crate still denies unsafe code by default. Only the dispatcher and private feature-specific helper add local exceptions, matching existing presentation FMA conventions. The call is guarded by `is_x86_feature_detected!("fma")`, which checks CPU and OS support. The helper uses ordinary arithmetic and safe iteration without unchecked memory access. A separate test-only allowance covers the existing Windows affinity/cycle-counter helper.

Validation:

- **197 baseline and 197 updated library release tests pass**, with ten opt-in benchmarks ignored. All twelve archived-binary benchmark invocations pass.
- **72,920 property batches** compare runtime-selected and explicitly portable interpolation against generic baseline control flow, with **zero observed bit differences**, including NaN payloads. Cases cover all 31 build-operation variants, nine easing variants, lengths 0/1/2/3/4/5/12/16/17/32, inline and spilled storage, zero-duration and completed segments, signed zero, subnormals, infinities, signaling/quiet NaNs and deterministic random IEEE inputs. State fields, flags, elapsed time, completion and prepared-storage pointer/capacity are checked. The regression requires exact non-NaN bits and tracks NaN payload differences separately; zero observed differences is not a cross-platform guarantee about unspecified NaN payloads.
- **4,320 queued tween states** match generic baseline execution, with zero observed bit differences. These cover relative targets, ordered overlapping properties, sleeps, instantaneous segments, completion into following segments, active state mutation and zero/negative/NaN/infinite delta-time handling. Queue length, active elapsed time and emptiness also match. Reference control flow is frozen from `076196e8f`; unchanged preparation, easing, interpolation formulas and endpoint writes are shared with production.
- **Three tween-completion integration tests pass**, covering relative-target capture, ordered writes, leftover time, spilled operations, cloning and clearing.
- **Eight animation tests pass at opt-level 0 with debug assertions and overflow checks**, including the two new regressions and zero observed NaN-bit differences. This directly compiles the production presentation crate with cached release dependencies and fat LTO; it is not a full Cargo debug build.
- `cargo check --release` and Clippy for all crate targets complete. Existing diagnostics remain, including the unchanged many-argument Bezier signature; no new helper/test diagnostics occur. The changed animation source and both new test/reference files pass rustfmt and whitespace checks.
- The earlier prototype separately matched 281,728 tween states and 1,361,282 unchanged curve evaluations. Its checks, logs and source snapshots are retained locally.
- Full application runtime/GPU, physical non-FMA and non-x86 runtime testing were not performed.

Measurements used an Intel Xeon E5-2696 v4, Windows x86_64 MSVC, rustc 1.99.1 / LLVM 23.1.1, release opt-level 3, fat LTO, static CRT and generic CPU settings. The committed ignored benchmark calls the public production update through a non-inlined wrapper. It black-boxes the mutable sequence slice, elapsed time, delta time and complete state. The same timing function, wrapper and fixtures are used in both archived binaries. Only the test-only portable comparison wrapper changes from the original interpolation body to the extracted production portable body.

Fixtures contain 64 actors. Allocation and initial preparation happen before timing. Each timed iteration resets active elapsed time to one of .125/.25/.375/.5/.625, then invokes `TweenSeq::update(1/120)` on every actor; the reset and output observation are included. Four-operation fixtures contain XY, RGBA tint, XY zoom and Z rotation. Large fixtures contain 18 operations: XY, size, tint, glow, zoom, three rotations, two skews, four crops and four fades. Three-operation fixtures contain XY, alpha and Z rotation. The `ease-mixed` three-operation control uses bias -37; other Bezier fixtures use bias 37.

The benchmark thread is pinned to logical CPU 2. `QueryThreadCycleTime` measures CPU cycles and `Instant` measures wall time. Each process measures ten rounds of 2000 batches per case, discarding round zero. Six fresh before/after process pairs alternate binary order and rotate cases by 0, 5, 10, 15, 20 and 25. Speedups are medians of the six paired process-median ratios; absolute columns are independent medians, so their ratio need not exactly equal the paired speedup.

SHA-256:

- Before executable: `471ace443fe7a319ad0b3e92dbd0f55ba6082536e49474696bcfeee745563e55`.
- After executable: `a3e3611bfe174a788d22857fe0d435d542dd50f18790832b1a96c234a8e7a197`.
- Timing function body: `a2e9efafec82d02a43e0482044dee779f74890dc185d281ab8879b30a376eab4`.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadlib-present --lib --test tween_completion -- --test-threads=1
$env:TWEEN_BENCH_ROTATION = '0'
cargo test --release --locked -j 2 -p deadlib-present --lib anim::performance::benchmark_tween_batches -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:TWEEN_BENCH_ROTATION
cargo check --release --locked -j 2 -p deadlib-present
cargo clippy --release --locked -j 2 -p deadlib-present --all-targets --no-deps
```

For comparative measurements, archive the library test executable from `cargo test --release --locked -j 2 -p deadlib-present --lib --no-run --message-format=json` before and after the production change, retaining the same benchmark fixtures and timing function. The baseline test-only `portable_apply` wrapper must use the original easing/property loop until the new production method exists. Run the exact ignored benchmark directly from each archived executable with all six rotations and alternating process order. Binaries, manifests, runners, before/after source snapshots, assembly, source verification, compiler-presence records and validation logs remain in `target/tween-batch-fma-audit-2026-11-09/`.
