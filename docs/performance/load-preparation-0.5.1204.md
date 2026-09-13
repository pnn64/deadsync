# Song and noteskin preparation - 0.5.1204

Baseline: `a85e01991` / 0.5.1203. This pass applies the allocation reuse,
unnecessary-copy, and throughput guidance in `rust-performance.md`
(`M-MEM-REUSE`, `M-HOTPATH`, and `M-THROUGHPUT`) to three preparation operations.

1. **Note-hide splines:** the coefficient buffer also holds the temporary
   diagonal and slope values. Slope construction and forward elimination share
   one traversal; backward substitution produces final coefficients in another.
   This removes two temporary arrays and several traversals while preserving
   the original floating-point calculations and terminal segment. Preparation
   allocates only its output. Song Lua setup calls this for authored note-hide
   columns before gameplay.
2. **Noteskin loader requests:** `load_request_ref` borrows button, element,
   and optional command strings from the compiled entry or fallback arguments.
   Runtime resolution, hold/roll parts, and explosion selection use this API.
   The existing owned `load_request` API remains compatible and explicitly
   converts the borrowed result when a caller needs ownership.
3. **First-sprite actor metadata:** `decl_for_path_ref` borrows the compiled
   actor declaration. First-sprite selection no longer copies every sprite,
   model, reference, command map, string, and frame array just to examine the
   declaration's sprites. The consuming `decl_for_path` API remains available;
   it also avoids the previous unnecessary manifest-key clone.

These changes affect song/noteskin preparation, including noteskin previews.
The measurements below cover the targeted operations, not complete loading
time, rendered frames, audio latency, or application-wide CPU usage.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), repository release profile with opt-level 3 and LTO. The shared
counting allocator forwards to `System`. Timing disables allocation counting;
each measurement records churn in a separate operation. Old and production
implementations execute in the same binary with prebuilt, black-boxed inputs.
Frozen parent bodies are committed beside the tests.

Each measurement uses three warmups and seven timing samples. Five complete
runs alternate old/new order. Builds and tests finish before timing. The tables
show medians of five per-run medians. Calling-thread CPU cycles come from
Windows `QueryThreadCycleTime`; the measured work is single threaded. The
[raw CSV](load-preparation-0.5.1204.csv) contains all 150 measurements, including
sample ranges, throughput, allocation/reallocation/free calls, and requested
and freed bytes.

Spline fixtures contain overlapping, reversed, and nonfinite hide windows.
One operation prepares and replaces a column's existing spline; throughput
counts coefficient records produced. The two-point case is an unchanged
control. Sizes range through the accepted 65,536-point limit.

Loader fixtures have 20 sorted entries across four buttons and five elements,
with case-insensitive queries, rotations, blank entries, and optional commands.
One operation returns and drops a request. The missing-entry case uses the
unchanged warning path with no logger installed, so logger work is excluded.

Actor fixtures have 24 manifest entries and resolve the last one. Each graph
group contains one sprite, model, reference, and path reference, with nested
commands and animation metadata. Graphs contain 0, 1, 8, 32, or 128 groups.
One operation looks up and returns/drops the owned or borrowed declaration.
This isolates the metadata copy removed from first-sprite selection; texture
decoding and filesystem work are outside this benchmark. Larger graphs are
scaling cases, not an estimate of the typical noteskin. A missing manifest
entry serves as an additional unchanged control.

Output creation and destruction are timed. Spline replacement frees its prior
output inside the operation. Borrowed paths retain their existing source data.
Requested bytes measure allocation churn, not RSS or allocator overhead.
Thread cycles do not measure cache misses or retired instructions.

## Results

Throughput units are coefficient records for splines, requests for loaders,
and declaration lookups for actor graphs.

| Workload | Old ns/op | New ns/op | CPU cycles old/new | Fewer cycles | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|
| Spline / 2 points (control) | 248.4 | 252.1 | 544.9 / 542.3 | 0.5% | 8.052 / 7.932 |
| Spline / 64 points | 1,115.7 | 737.8 | 2,438.9 / 1,617.0 | 33.7% | 57.363 / 86.746 |
| Spline / 1,024 points | 16,675.6 | 13,164.1 | 36,433.6 / 28,806.0 | 20.9% | 61.407 / 77.787 |
| Spline / 16,384 points | 253,403.3 | 187,306.6 | 552,906.1 / 407,935.3 | 26.2% | 64.656 / 87.472 |
| Spline / 65,536 points | 1,326,784.4 | 913,162.5 | 2,889,628.3 / 1,988,574.2 | 31.2% | 49.395 / 71.768 |
| Request / plain | 200.8 | 89.1 | 438.1 / 194.4 | 55.6% | 4.979 / 11.218 |
| Request / command | 239.3 | 84.1 | 523.6 / 184.6 | 64.7% | 4.179 / 11.885 |
| Request / blank | 230.1 | 119.3 | 504.0 / 261.1 | 48.2% | 4.347 / 8.380 |
| Request / fallback | 141.3 | 42.1 | 307.8 / 92.3 | 70.0% | 7.079 / 23.763 |
| Actor / empty graph | 894.2 | 710.3 | 1,948.1 / 1,557.2 | 20.1% | 1.118 / 1.408 |
| Actor / 1 group | 3,631.9 | 646.7 | 7,963.3 / 1,416.0 | 82.2% | 0.275 / 1.546 |
| Actor / 8 groups | 23,634.4 | 636.2 | 51,728.6 / 1,395.7 | 97.3% | 0.042 / 1.572 |
| Actor / 32 groups | 113,347.4 | 587.2 | 248,060.0 / 1,288.0 | 99.5% | 0.009 / 1.703 |
| Actor / 128 groups | 415,571.4 | 594.4 | 909,502.4 / 1,304.5 | 99.9% | 0.002 / 1.682 |
| Actor / missing (control) | 435.8 | 427.3 | 955.7 / 936.0 | 2.1% | 2.295 / 2.340 |

The small differences in the unchanged controls are not claimed as
optimizations. In particular, the two-point spline measured 3.7 ns more wall
time despite slightly fewer thread cycles; wall-time and cycle medians are
aggregated independently.

| Workload | Allocation/free calls old/new | Requested/freed bytes old/new |
|---|---:|---:|
| Spline / 2 points | 1 / 1 | 32 / 32 |
| Spline / 64 points | 3 / 1 | 1,536 / 1,024 |
| Spline / 1,024 points | 3 / 1 | 24,576 / 16,384 |
| Spline / 16,384 points | 3 / 1 | 393,216 / 262,144 |
| Spline / 65,536 points | 3 / 1 | 1,572,864 / 1,048,576 |
| Request / plain | 2 / 0 | 12 / 0 |
| Request / command | 3 / 0 | 42 / 0 |
| Request / blank | 2 / 0 | 18 / 0 |
| Request / fallback | 2 / 0 | 17 / 0 |
| Actor / empty graph | 2 / 1 | 52 / 26 |
| Actor / 1 group | 46 / 1 | 2,303 / 26 |
| Actor / 8 groups | 326 / 1 | 18,060 / 26 |
| Actor / 32 groups | 1,286 / 1 | 72,084 / 26 |
| Actor / 128 groups | 5,126 / 1 | 288,180 / 26 |
| Actor / missing | 1 / 1 | 25 / 25 |

Every measured operation has zero reallocations. Frees equal allocations,
and freed bytes equal requested bytes, throughout the CSV. Borrowed loader
requests have zero allocation, reallocation, and free calls.

## Memory and compatibility

Spline preparation above two points reduces requested bytes by one third
(`24 * size` to `16 * size`). At the size limit it removes 512 KiB of temporary
scratch per column preparation. The retained 1 MiB coefficient output is
unchanged. The algorithm uses a few scalar locals and reuses output slots;
it adds no retained scratch or persistent cache.

Borrowed noteskin requests and declarations add no retained allocations or
fields to compiled assets. Actor lookup still allocates a manifest key (26
bytes in the matching fixtures). Consumers needing ownership still clone
the output graph. Existing owned APIs, serialized schema, lookup order, and
warning behavior remain compatible. No external dependency versions or unsafe code
are added by this pass.

## Behavior and checks

- Gameplay library: 797 tests passed in debug and release; eight manual tests
  ignored. Three new tests compare coefficients and sampled zoom values bit
  for bit against the parent across sizes, seeds, columns, spacings, nonfinite
  inputs, signed zero, and the size limit. They verify invalid requests leave
  existing state untouched and enforce an output-only allocation budget.
- Noteskin library: 240 tests passed; six manual tests ignored. Five new
  integration tests passed in debug and release, with one manual benchmark
  ignored. They cover mapped and missing requests, blank/command/rotation
  fields, Unicode and case variants, duplicate and search-directory ordering,
  nested actor metadata, owned-copy independence, and allocation budgets.
  An actual first-sprite call matches the parent's loader callback trace:
  animated/frame/texture fallback, selection of the next sprite, and preservation
  of animation metadata.
- Note-rendering library: all 370 tests passed, including authored spline
  visibility behavior.
- `cargo check -p deadsync` and performance Clippy passed. Scoped rustfmt and
  `git diff --check` passed; unrelated existing formatting in `attacks.rs` is
  preserved, with the changed method checked separately.
- The workspace patch increases exactly once: 0.5.1203 -> 0.5.1204. Cargo.lock
  updates all three workspace-version packages with no dependency changes.

Reproduce from the repository root:

```powershell
cargo test -p deadsync-gameplay --lib -- --test-threads=1
cargo test -p deadsync-noteskin --lib --test load_preparation_perf -- --test-threads=1
cargo test -p deadsync-notefield --lib -- --test-threads=1
cargo test --release -p deadsync-gameplay --lib -- --test-threads=1
cargo test --release -p deadsync-noteskin --test load_preparation_perf -- --test-threads=1
cargo check -p deadsync
cargo clippy -p deadsync-gameplay -p deadsync-noteskin --lib --test load_preparation_perf --no-deps -- -A clippy::all -D clippy::perf
cargo test --release -p deadsync-gameplay --lib benchmark_load_preparation -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-noteskin --test load_preparation_perf benchmark_load_preparation -- --ignored --nocapture --test-threads=1
```

Repeat the last two commands five times, setting `DEADSYNC_PERF_REVERSE=1`
for runs two and four and removing it for the others. Finish both release
builds before collecting timing. Cycle reporting is Windows-specific; other
platforms report zero for that unavailable metric.
