# Texture preparation - 0.5.1656

Parent: `57ece088f` (0.5.1655). Date: 2026-10-01.

Three changes follow `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance in mine/noteskin texture loading:

1. Compute only the neighboring source columns required by the final mine
   color samples. The original sampler averaged every column into a temporary
   vector before resampling it. Two inline column averages now cover monotonic
   output positions, including repeated positions when upsampling. Vertical
   accumulation order, alpha-weighted RGB arithmetic and final interpolation
   stay unchanged. The result is the only allocation. A 1024x64 image reduced
   to 64 colors uses 86.7% to 89.2% fewer measured CPU cycles and
   reduces requested bytes from 17,408 -> 1,024.
2. Precompute the 32 rotated radial-layer colors once per generated mine frame.
   Pixels select that stack table instead of performing two variable-divisor
   modulo operations per interior pixel. The existing radial profile, edge
   alpha rounding, color saturation and output layout are unchanged. The
   64-color fixture uses 33.0% to 40.8% fewer CPU cycles. Allocation
   counts/bytes are unchanged; common palettes still allocate only the output
   image, with no heap scratch added for this table.
3. Reuse the decoded and resized RGBA image for consecutive atlas frames with
   the exact same path spelling. The cache borrows its path and retains one
   prepared frame during that build. A different path drops the previous
   image before decoding the next, keeping decoder working memory bounded.
   Nonconsecutive repeats and unique paths still decode normally. The repeated
   resized-frame fixture uses 84.1% to 86.0% fewer CPU
   cycles and requested bytes fall from 2,203,952 -> 504,870.

The combined mine sampling/generation fixture uses
43.5% to 51.3% fewer CPU cycles. These are asset preparation helpers;
rendering frame rate, texture upload and cold-disk loading were not measured.
No new dependencies or unsafe production code were added.

Sampling now owns only its requested output colors, rather than also owning
one color per source column. Gradient generation still owns the pixel image
and uses the existing inline palette representation; palettes over 64 colors
retain its original heap spill. Atlas construction still owns the complete
atlas and decoder/resize buffers. Its new cache lives only for one build and
does not retain pixels or paths globally. Later builds reread changed files.
The existing generated-texture registry's hit behavior is unchanged.

## Measurement method

The [raw CSV](texture-preparation-0.5.1656.csv) retains 192 measurements:
16 workloads, two implementations and six serial runs alternating
old-first/new-first. Each median uses seven timing batches and their elapsed
minimum/maximum values remain in the CSV. Windows `QueryThreadCycleTime`
measures the calling thread's CPU cycles. Allocation counts run separately
for one complete operation, including result destruction, through the
existing counting `System` allocator; counting is disabled during timing.

Environment: Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, 22 cores / 44 logical
processors, Rust 1.98.1, LLVM 22.1.8. Release builds use opt-level 3 and full LTO,
with test unwinding. The two test executables run serially on logical processor
6 (affinity mask 64), with `--test-threads=1`, after compiler/check processes
finish. The harness verifies each child process's affinity.

Images, PNG files and animation declarations are created before timing.
Behavior comparisons and warmups precede measurements. Gradient benchmarks
start from an already decoded image or supplied palette; they include all
sampling/generation work and owning output destruction. The shared radial
profile is initialized before timing. Atlas benchmarks include file reads,
PNG decode, RGBA conversion, resizing, tile copying and output destruction.
Files/metadata are warm in the OS cache. Fixed-width temporary path IDs keep
all allocator counters and byte totals stable across the six runs.

Requested/freed bytes include reallocation sizes and describe allocator
churn, rather than peak live memory or process RSS. The atlas retains one
prepared frame and drops it before decoding a different path; peak memory
was not instrumented. Operation timings shown in tables are from run 1;
every run's medians and batch ranges remain in the CSV.

Three gradient functions are frozen exactly from the parent in
`crates/deadsync-noteskin/tests/gradient_preparation/baseline.rs`: source-column
sampling, resampling and texture generation. A source audit verifies their
bodies and attributes. Unchanged radial-profile and color helpers are shared.

The atlas baseline in
`crates/deadsync-assets/tests/model_atlas_preparation/baseline.rs` extracts the
parent `model_animation_source` atlas-building block verbatim. Harness-only
adaptations provide its existing dimension/grid locals and return the built
image. A source audit verifies the complete block and verifies the production
source wrapper is unchanged except for calling the extracted optimized helper.
Atlas benchmarks omit first-header dimension probing, generated-texture
registry queries/registration, source-plan construction and upload; they
measure the production atlas-building work on a registry miss.

12 of 16 workloads improve CPU cycles and throughput in every run.
All measurements are retained. Timing gains are not consistent across all
runs for: `tex_sample_upsample`, `tex_atlas_single`, `tex_atlas_unique`,
`tex_atlas_alternating`.
Comparisons slower in every run: `tex_atlas_alternating`.
Negative CPU reduction values mean higher measured CPU cost. Allocation
savings by themselves do not establish a timing improvement for every input.

The alternating-path control uses 0.6% to 7.6% more measured CPU cycles and
unchanged allocation counts in these six runs. It has no consecutive frames
to reuse. This is a recorded cost, not a claimed improvement; this change
targets consecutive repeated image/resize work. Upsampling a narrow image
also has mixed timing results despite removing its intermediate allocation.

## Mine color sampling

Throughput is output color samples/s. Batches contain 256 operations.
`wide` reduces a 1024x64 image to 64 colors; `square` samples a 64x64 image;
`upsample` grows 8 columns to 64 samples; `single` repeats one averaged column;
`one_sample` requests only one color from 1024 columns. Inputs vary RGB and
alpha bytes, including transparent and nearly transparent pixels. Source
regions are checked in full even when only a few columns are needed, retaining
invalid-region rejection. The separate public resampling API is unchanged.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `tex_sample_wide` | 511,052.7 -> 60,604.3 | 1,119,612.9 -> 132,748.6 | 86.7% to 89.2% | 652.3% to 829.7% | 2/0/2 -> 1/0/1 | 17408 -> 1024 |
| `tex_sample_square` | 30,651.2 -> 29,325.4 | 67,177.3 -> 64,248.3 | 4.4% to 12.6% | 4.5% to 14.5% | 2/0/2 -> 1/0/1 | 2048 -> 1024 |
| `tex_sample_upsample` | 4,355.1 -> 4,423.0 | 9,551.7 -> 9,694.9 | -9.6% to 10.5% | -8.6% to 11.9% | 2/0/2 -> 1/0/1 | 1152 -> 1024 |
| `tex_sample_single` | 614.5 -> 520.3 | 1,354.7 -> 1,147.2 | 0.3% to 22.9% | 0.2% to 29.8% | 2/0/2 -> 1/0/1 | 1040 -> 1024 |
| `tex_sample_one_sample` | 524,681.6 -> 1,560.2 | 1,149,286.6 -> 3,431.4 | 99.7% to 99.8% | 33530.4% to 48846.6% | 2/0/2 -> 1/0/1 | 16400 -> 16 |

## Mine radial color selection

Throughput is generated pixels/s, including transparent corners. Batches
contain 64 operations. Palettes of 1/7/64/129 colors exercise constant-looking,
non-power-of-two, normal and heap-spilled palette lengths. Values include
negative, signed-zero, ordinary, out-of-range and nonfinite channels. Every
generated byte is compared with the parent. The 32-entry table is stack
storage; output and existing large-palette allocation counts are unchanged.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `tex_radial_1` | 32,367.2 -> 10,823.4 | 70,946.5 -> 23,709.4 | 54.9% to 71.2% | 120.9% to 245.9% | 1/0/1 -> 1/0/1 | 16384 -> 16384 |
| `tex_radial_7` | 252,285.9 -> 81,771.9 | 553,033.7 -> 178,621.5 | 64.0% to 69.1% | 174.4% to 223.4% | 1/0/1 -> 1/0/1 | 114688 -> 114688 |
| `tex_radial_64` | 2,102,243.8 -> 1,311,409.4 | 4,603,168.8 -> 2,872,809.1 | 33.0% to 40.8% | 49.1% to 68.8% | 1/0/1 -> 1/0/1 | 1048576 -> 1048576 |
| `tex_radial_129` | 4,159,659.4 -> 2,582,421.9 | 9,109,911.9 -> 5,656,881.9 | 35.4% to 39.6% | 54.9% to 65.5% | 2/0/2 -> 2/0/2 | 2115584 -> 2115584 |

## Combined mine preparation

Each operation samples a 1024x64 decoded image into 64 colors, generates the
64-frame gradient sheet and destroys both owning outputs. Throughput is output
pixels/s (64x64x64), with 64 operations per batch. The comparison combines
both gradient changes against both frozen parent functions. File decoding,
texture-key hashing and registry/upload work are outside this fixture.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `tex_mine_full` | 2,384,615.6 -> 1,185,667.2 | 5,224,058.8 -> 2,597,827.0 | 43.5% to 51.3% | 76.7% to 105.4% | 3/0/3 -> 2/0/2 | 1065984 -> 1049600 |

## Animated model atlas preparation

Throughput is authored frame pixels/s, excluding unused blank tiles in the
atlas's rectangular grid. Batches contain 32 operations. PNG pixels vary all
RGBA channels and alpha, exercising replacement rather than alpha blending.
`single` has one 64x64 image; `unique` has eight different 64x64 images;
`repeat` has two consecutive runs of eight images each; `resize_repeat` uses
the same runs with the second source at a different size; `alternating`
switches paths on every frame and controls cache-check overhead; `large_repeat`
uses 256x128 tiles and a differently sized second source. Triangle resizing,
tile order and transparent padding are identical to the parent.

| Workload | ns/op, old -> new (run 1) | Thread cycles/op, old -> new (run 1) | CPU reduction, six runs | Throughput gain, six runs | Alloc/realloc/free, old -> new | Requested bytes, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `tex_atlas_single` | 279,906.2 -> 270,765.6 | 612,878.3 -> 592,876.3 | -4.8% to 3.3% | -4.7% to 3.4% | 11/1/11 -> 11/1/11 | 111537 -> 111537 |
| `tex_atlas_unique` | 1,492,971.9 -> 1,599,287.5 | 3,269,953.3 -> 3,501,800.1 | -7.1% to 4.2% | -6.6% to 4.3% | 81/8/81 -> 81/8/81 | 908680 -> 908680 |
| `tex_atlas_repeat` | 2,774,596.9 -> 452,281.2 | 6,075,993.2 -> 989,931.3 | 82.3% to 85.8% | 464.5% to 605.5% | 161/16/161 -> 21/2/21 | 1784592 -> 452450 |
| `tex_atlas_resize_repeat` | 4,115,300.0 -> 652,296.9 | 9,009,624.3 -> 1,428,396.2 | 84.1% to 86.0% | 530.9% to 615.3% | 193/24/193 -> 25/3/25 | 2203952 -> 504870 |
| `tex_atlas_alternating` | 3,859,175.0 -> 3,881,259.4 | 8,448,301.2 -> 8,497,057.7 | -7.6% to -0.6% | -7.0% to -0.6% | 193/24/193 -> 193/24/193 | 2203952 -> 2203952 |
| `tex_atlas_large_repeat` | 21,113,450.0 -> 3,897,850.0 | 46,213,297.2 -> 8,529,070.4 | 80.6% to 83.5% | 415.5% to 507.6% | 193/32/193 -> 25/4/25 | 14689584 -> 3671206 |

## Behavior and validation

Eight new ordinary tests compare production with frozen parent behavior:

- 864 sampling fixtures combine widths 0/1/2/3/7/32/65/129/513, heights
  0/1/3/17, three alpha patterns and requested counts 0/1/2/3/7/64/129/1024.
  Color float bits, result counts and generated texture keys match exactly.
- Empty regions return None; invalid nonempty regions still panic even for
  a one-sample request. The tests do not assert a particular panic message.
- Generated images match every RGBA byte for palette sizes 1/2/3/7/16/32/64/65/129,
  including ordinary and nonfinite/out-of-range color channels. Empty palettes
  still panic. Combined sampling/generation outputs match the parent.
- Sampling allocates exactly one result buffer; normal generated palettes add
  no heap scratch and allocate only the image. Consecutive atlas reuse reduces
  owning allocation churn for both same-size and resized images.
- Atlas byte comparisons cover 1x1, odd/asymmetric and larger frame sizes,
  mixed/repeated/unique path sequences, Triangle resizing, alpha and grid
  padding. Missing/corrupt image errors match, and a later build sees file edits.

Final validation:

- Eight new debug behavior/allocation tests passed. The filtered preparation
  suites also passed: 3 assets tests and 31 noteskin tests, with manual
  benchmarks ignored.
- Release noteskin library: 318 passed, 15 manual benchmarks ignored.
- Release assets library: 160 passed, five manual benchmarks ignored.
- Six paired release rounds passed for both executables (192 measurements).
- Workspace check passed, offline and locked.
- Noteskin/assets Clippy passed with `clippy::perf` denied and the existing
  `large_enum_variant` exception; unrelated existing warnings remain.
- Changed-file rustfmt, frozen-source audit and `git diff --check` passed.

These checks cover the affected libraries and workspace compilation; they
do not constitute a full workspace test run.

Reproduce correctness and benchmark preparation:

```powershell
cargo test -p deadsync-noteskin -p deadsync-assets --lib preparation --offline --locked
cargo test -p deadsync-noteskin -p deadsync-assets --lib --release --offline --locked
cargo check --workspace --offline --locked
cargo clippy -p deadsync-noteskin -p deadsync-assets --lib --offline --locked --no-deps -- -D clippy::perf -A clippy::large_enum_variant
```

After compiler/check processes finish, pin PowerShell and run both ignored
benchmarks serially six times, alternating implementation order:

```powershell
$taskProcess = Get-Process -Id $PID
$taskProcess.ProcessorAffinity = [IntPtr]64
foreach ($taskRun in 1..6) {
    $env:DEADSYNC_PERF_ORDER = if ($taskRun % 2 -eq 0) { 'new-first' } else { 'old-first' }
    cargo test -p deadsync-noteskin --lib --release --offline --locked benchmark_gradient_preparation -- --ignored --nocapture --test-threads=1
    cargo test -p deadsync-assets --lib --release --offline --locked benchmark_model_atlas_preparation -- --ignored --nocapture --test-threads=1
}
Remove-Item Env:DEADSYNC_PERF_ORDER
```

Recorded runs invoke the built executables directly, without Cargo in the
measurement processes. The patch advances exactly 0.5.1655 -> 0.5.1656 in
Cargo.toml and Cargo.lock. The user-excluded reference/input/scripts are
absent from this commit.
