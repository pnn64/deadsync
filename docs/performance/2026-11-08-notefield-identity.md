Note alpha/glow evaluation now returns `(1.0, 0.0)` immediately when the appearance cache identifies a field with no appearance effects. The previous path already returned visibility 1.0, but the generated code merged it into the general glow calculation and called `fmaf` for each note. Returning the final pair avoids that work and the coordinate read. This applies to CPU note preparation used by GPU backends and hold geometry.

The production change is three lines in `appearance_note_alpha_glow_cached`. The identity cache is already maintained by `note_appearance_cache`. For identity, the old glow expression evaluates `1.0.mul_add(-1.3, 1.3)`, exactly positive zero, and actor alpha is exactly 1.0. The change preserves those bits regardless of position, including NaNs, infinities and signed zero. It adds no unsafe code, CPU feature requirement, allocation or approximate math.

The initial copied-source experiment measured **3.375x** faster default alpha/glow evaluation, with six process ratios from **3.270x to 3.453x**. CPU cost fell from **1,658.1 to 492.6 cycles per 256 notes**, about **70.4% less CPU cost**. Independent median wall times were **756.0 to 224.25 ns**. The absolute saving is about **0.53 microseconds per 256 notes**, not a whole-frame or FPS speedup. The [prototype raw data](2026-11-08-notefield-identity-prototype-raw.csv) retains all 1,440 observations and the [summary](2026-11-08-notefield-identity-prototype-summary.csv) retains all 12 cases. The combined Hidden/Sudden fade-only control measured 0.981x [0.973, 1.012], about 1.9% more CPU cost; this result is retained.

The integrated Cargo release test executable measures **3.665x** faster default alpha/glow evaluation (**72.7% less CPU cost**), with paired ratios from **3.603x to 3.771x**. Independent median wall times are **811.47 to 221.42 ns per 256 notes**. Its [raw measurements](2026-11-08-notefield-identity-raw.csv) retain all 1,440 observations, and the [summary](2026-11-08-notefield-identity-summary.csv) retains all controls and ranges. This executable uses the release test profile (including panic unwinding), so its results are kept separate from the original panic-abort prototype. No builds or other tests from this change overlap these timed runs.

| Workload (256 notes) | Before cycles | After cycles | Median speedup | Six-process range |
| --- | ---: | ---: | ---: | ---: |
| identity | 1,776.5 | 484.7 | 3.665x | 3.603-3.771x |
| hidden | 2,602.5 | 2,408.8 | 1.070x | 1.043-1.114x |
| sudden | 2,619.0 | 2,523.2 | 1.028x | 0.995-1.075x |
| hidden-sudden | 3,211.2 | 3,155.9 | 1.013x | 1.004-1.023x |
| stealth-control | 1,990.3 | 1,785.2 | 1.119x | 1.062-1.162x |
| mixed | 6,831.5 | 6,640.4 | 1.029x | 1.020-1.053x |
| identity-fade | 1,774.8 | 485.9 | 3.628x | 3.281-3.835x |
| hidden-fade-control | 4,628.9 | 4,376.4 | 1.059x | 1.035-1.092x |
| sudden-fade-control | 4,576.4 | 4,390.4 | 1.043x | 1.024-1.047x |
| hidden-sudden-fade-control | 6,434.8 | 6,321.7 | 1.013x | 0.992-1.026x |
| stealth-fade-control | 1,946.6 | 1,737.7 | 1.114x | 1.094-1.163x |
| mixed-fade | 7,518.1 | 7,319.5 | 1.027x | 1.013-1.046x |

The source snapshots were compiled together with opt-level 3, fat LTO, one codegen unit, generic x86-64 CPU settings and static CRT on an Intel Xeon E5-2696 v4, Windows x86-64, rustc 1.99.1 / LLVM 23.1.1. Inspection of the measured assembly confirms that the original identity branch reaches `fmaf`; the new branch stores the final pair without that call. The initial attempt to check visibility endpoints inside the glow helper was rejected because some fade controls slowed by 10-18%. Moving the identity return earlier avoids adding those endpoint checks to the fade calculation.

The committed opt-in `measure_alpha_glow` benchmark compares the actual helper with the original composition of its unchanged visibility, actor-alpha and glow helpers. Both loops are in the same executable. Each invocation uses 256 positions, one prebuilt appearance cache, and retained output buffers. It measures 3,000 invocations per round, alternating implementation order for ten rounds and excluding only round zero. Coordinates span the visible field or focus on fade regions. Six fresh processes use rotations 0, 5, 10, 15, 20 and 25. Threads are pinned to logical CPU 2; `QueryThreadCycleTime` measures CPU cycles and `Instant` measures wall time. Setup is outside timing. Every case checks exact output bits before timing. Speedups are medians of paired process ratios; cycle/time columns are independent medians of process medians. These are helper measurements, not full note composition. Unrelated compiler activity was present during the investigation.

The regular regression test compares **2,097,152 complete alpha/glow pairs**, including 1,048,576 identity cases, against the original composition. It varies effect strengths, signed-zero settings, stealth flags, independent position coordinates, mini, elapsed time, finite values, arbitrary float patterns, subnormals, infinities and NaNs. Existing note and hold tests exercise the production call sites.

Validation: **521 notefield release tests pass**, with 24 opt-in benchmark, trace and exhaustive checks ignored. This includes the 2,097,152-pair regression and existing native-vector, note and hold coverage. All six ignored benchmark invocations pass separately. A standalone harness including the actual production module also matches all 2,097,152 pairs with opt-level 0 and debug assertions enabled; it uses the cached release gameplay dependency, fat LTO and panic abort, rather than a full Cargo debug build. Clippy completes with diagnostics in unchanged code; the new test/benchmark module has none. The new module's formatting and staged whitespace checks pass.

Reproduction from the repository root:

```powershell
cargo test --release --locked -j 2 -p deadsync-notefield --lib -- --test-threads=1
cargo test --locked -j 2 -p deadsync-notefield --lib transforms::appearance_performance::alpha_glow_matches_original_with_float_patterns_and_independent_positions -- --exact --test-threads=1
$env:DEADSYNC_APPEARANCE_BENCH_ROTATION = '0'
cargo test --release --locked -p deadsync-notefield --lib transforms::appearance_performance::measure_alpha_glow -- --exact --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_APPEARANCE_BENCH_ROTATION
cargo clippy --release --locked -j 2 -p deadsync-notefield --all-targets --no-deps
```

Repeat the benchmark command with the six rotations above to obtain paired process medians. Local source snapshots, the original measured executable and assembly, rejected experiment, runner and hashes are retained in `target/notefield-fma-audit-2026-11-08/`. Integrated logs and results are retained in `target/notefield-identity-integrated-2026-11-08/`.
