# Spline work performance 0.5.1683

Parent: `d209a1380` (0.5.1682). Date: 2026-10-02.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance to three remaining loader costs:

1. Solve all varying spline axes against one tridiagonal matrix. The old solver
   rebuilt/factored that matrix for every varying axis and allocated separate
   diagonal and slope vectors even for empty, two-point and constant splines.
   Slopes now occupy their existing output coefficient slots; one shared
   diagonal buffer is allocated only for a varying spline with at least three
   points. A single varying axis uses a dedicated loop without inner axis checks.
   Arithmetic order within each axis is retained.
2. Resolve players' NoteField from their stored named-child registry directly.
   The previous lookup copied the entire registry into a temporary Lua table
   on every capture, zoom-hide read and column snapshot. Players with sequence
   children or absent stored registries retain the existing lookup/merge path,
   including scripted getter/setter effects.
   The shortcut only reads a registry already stored on the actor; getters
   can add sequence children while resolving an absent registry.
   mlua sequence traversal is raw, so player metatables do not prevent the shortcut. Direct raw lookup
   matches the old fresh table's treatment of registry metatables, observes
   registry replacement/removal and retains conversion errors.
3. Deduplicate reused coefficient Arcs by identity. Captured lanes retain two
   booleans indicating whether the preceding position/zoom buffers contain
   NaNs. Reflexive buffers shared with the previous frame need only an identity
   and metadata comparison; different buffers retain value comparison. NaN
   buffers remain non-reflexive exactly as under the parent's PartialEq.
   Metadata-only frames reuse this property without rescanning coefficients.

No dependency or unsafe production code was added. A varying cubic solver
allocates its owned output plus one scratch buffer: 52 requested bytes/point,
previously 56. One/two-point or constant splines allocate only their 48-byte
output coefficient per point. Empty input allocates nothing. These are owned
load results; no persistent solver cache was added. Capture's two cached
booleans add 8 bytes to each lane entry on this 64-bit target, including padding;
BTreeMap node capacity also affects measured allocation traffic. They expire
with capture scratch at the end of compilation. Existing coefficient/track
limits and logical byte accounting are retained.

## Measurement method

Measurements use Windows 11 Pro 10.0.26100, Intel Xeon E5-2696 v4,
22 cores/44 logical processors, Rust/Cargo 1.98.1 and the repository release
profile (opt-level 3, full LTO). All builds/checks finish before timing.
Six serial rounds alternate old-first/new-first. Each row in the
[raw CSV](spline-work-0.5.1683.csv) contains the median and min/max of seven
batches; there are 288 measurements across 24 paired workloads.
Windows QueryThreadCycleTime records calling-thread CPU cycles. Tables show
medians of paired round percentage changes, with the full six-round CPU range.
Negative reductions mean slower. Throughput improvements are percentage
increases, with large ratios shown as multiples. Throughput units are points,
lookups, comparisons or lane captures as specified below.

Allocator accounting runs separately from timing through the repository's
thread-local System wrapper. mlua routes its Lua allocator through the same
wrapper, so counts include temporary Lua tables. Requested/freed bytes describe
allocator traffic rather than peak RSS or allocator metadata. Every operation
ends with balanced requested/freed bytes. Counts are identical across all six
rounds for each workload/variant. The disabled TLS tracking check is present
in both timed implementations.

The solver baseline is frozen from the parent. Lookup's baseline calls the
unchanged actor_named_children implementation. Dedup's baseline freezes the
parent capture while both variants use the same new reader and solver; this
isolates the third change from the first two. These CPU microbenchmarks do not
establish end-to-end song-load, gameplay FPS or GPU improvements.

## Shared cubic solver

One operation solves and drops the complete coefficient vector. Inputs are
prepared outside timing/allocation accounting; throughput counts points.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput improvement | Allocations/op | Requested bytes/op |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2 points, 3 varying axes | 204.2 -> 78.2 | 61.3% (56.2..71.5%) | 159.5% | 3 -> 1 | 112 -> 96 |
| 32 points, 0 varying axes | 290.2 -> 172.7 | 40.6% (20.0..44.2%) | 68.8% | 3 -> 1 | 1,792 -> 1,536 |
| 32 points, 1 varying axes | 650.1 -> 615.4 | 5.2% (-1.1..8.7%) | 5.6% | 3 -> 2 | 1,792 -> 1,664 |
| 32 points, 3 varying axes | 1,394.6 -> 787.0 | 44.2% (40.3..51.5%) | 80.9% | 3 -> 2 | 1,792 -> 1,664 |
| 256 points, 1 varying axes | 4,210.0 -> 4,242.2 | 0.6% (-3.7..10.7%) | 0.2% | 3 -> 2 | 14,336 -> 13,312 |
| 256 points, 2 varying axes | 7,129.5 -> 4,968.3 | 30.9% (22.0..34.1%) | 46.3% | 3 -> 2 | 14,336 -> 13,312 |
| 256 points, 3 varying axes | 9,845.2 -> 5,576.0 | 43.4% (42.8..46.0%) | 77.5% | 3 -> 2 | 14,336 -> 13,312 |
| 4,096 points, 3 varying axes | 177,253.1 -> 129,246.9 | 28.1% (16.7..31.4%) | 39.1% | 3 -> 2 | 229,376 -> 212,992 |
| 65,536 points, 1 varying axes | 2,520,404.7 -> 2,307,615.6 | 8.8% (7.2..13.6%) | 9.6% | 3 -> 2 | 3,670,016 -> 3,407,872 |
| 65,536 points, 3 varying axes | 4,233,781.2 -> 2,839,895.3 | 32.2% (29.2..33.9%) | 47.4% | 3 -> 2 | 3,670,016 -> 3,407,872 |

## NoteField lookup

One operation performs 128 lookups and collects Lua garbage. Actor/Lua fixture
construction is excluded. Collection is included in both variants to give
stable allocation/free accounting. Throughput counts lookups. The dummy fixture
uses the production actor factory and its metatable.
Sequence children exercise the preserved fallback; actor metatables retain
raw sequence semantics and benefit from the shortcut.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput improvement | Allocations/op | Requested bytes/op |
| --- | ---: | ---: | ---: | ---: | ---: |
| dummy | 214,885.1 -> 29,456.7 | 85.9% (85.8..87.0%) | 614.2% | 512 -> 0 | 28,672 -> 0 |
| plain | 198,336.8 -> 28,536.3 | 85.7% (83.8..86.3%) | 600.2% | 512 -> 0 | 28,672 -> 0 |
| missing | 130,589.4 -> 24,046.5 | 81.8% (80.0..82.4%) | 448.7% | 385 -> 1 | 16,418 -> 34 |
| sequence | 188,825.0 -> 189,595.3 | -2.1% (-17.3..0.3%) | -2.0% | 640 -> 640 | 18,176 -> 18,176 |
| metatable | 103,401.9 -> 29,127.8 | 71.6% (68.9..72.2%) | 253.9% | 256 -> 0 | 10,240 -> 0 |

## Shared-buffer deduplication

The comparison operations process 128 repeated shared-buffer comparisons.
The cached reflexivity scan occurs outside timing, as it is reused across
captures. Both versions allocate nothing. These timings isolate the equality
work; the complete capture timings below show its impact with Lua traversal.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput improvement | Allocations/op | Requested bytes/op |
| --- | ---: | ---: | ---: | ---: | ---: |
| 32 points, shared Arc | 21,919.5 -> 447.6 | 98.0% (97.5..98.1%) | 50.4x | 0 -> 0 | 0 -> 0 |
| 256 points, shared Arc | 169,910.2 -> 429.7 | 99.7% (99.7..99.8%) | 401.0x | 0 -> 0 | 0 -> 0 |
| 65,536 points, shared Arc | 41,589,976.5 -> 477.4 | 99.9989% (99.9981..99.9990%) | 89,252.2x | 0 -> 0 | 0 -> 0 |

Each complete capture operation starts empty, performs 120 samples, drops
capture output, and collects Lua garbage. Fixtures use the production dummy-player
actor factory. Construction is excluded; metadata and point edits are included.
Both position and zoom are captured. Throughput
counts lane samples. Both variants use the same optimized reader/solver, so
allocation counts match; the new lane metadata increases BTreeMap bytes.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput improvement | Allocations/op | Requested bytes/op |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 lane x 2 points, steady | 417,775.0 -> 465,628.1 | -2.7% (-16.0..0.3%) | -2.7% | 9 -> 9 | 1,590 -> 1,678 |
| 8 lanes x 32 points, steady | 14,351,031.2 -> 14,139,496.8 | 3.0% (-0.3..19.0%) | 3.3% | 60 -> 60 | 55,710 -> 55,798 |
| 8 lanes x 256 points, steady | 104,471,246.8 -> 100,958,846.8 | 2.3% (-1.3..20.7%) | 2.7% | 60 -> 60 | 416,798 -> 416,886 |
| 1 lane x 256 points, metadata | 12,406,668.7 -> 12,436,603.1 | -0.4% (-4.4..2.3%) | -0.4% | 11 -> 11 | 73,310 -> 73,398 |
| 1 lane x 32 points, changing | 2,335,296.9 -> 2,343,259.4 | -0.6% (-15.0..6.6%) | -0.1% | 725 -> 725 | 791,230 -> 791,318 |
| 8 lanes x 0 points, disabled | 1,188,271.9 -> 1,165,628.1 | 1.6% (-5.7..4.9%) | 1.7% | 11 -> 11 | 3,870 -> 3,958 |

The 256-point single-axis solver is effectively neutral in CPU cycles
(0.6% median reduction, -3.7..10.7% across rounds), while allocating one fewer
buffer and 7.1% fewer bytes. The sequence fallback adds a raw-length check and
measured 2.1% more cycles (range -17.3..0.3% reduction); its allocation traffic
is unchanged. The shortcut requires a stored registry and no sequence children.
Missing registries retain the full lookup/merge path and its scripted effects.

Small two-point capture measured 2.7% more CPU cycles. Metadata edits (-0.4%)
and changing coordinates (-0.6%) are effectively neutral; their round ranges
include both improvements and regressions. Larger steady captures show the
2.3..3.0% median CPU reductions above, with one slower round in each workload.
The much larger equality-only gains apply to that comparison work, while Lua
field traversal remains the main cost of complete capture. All capture variants
have matching allocation counts and the new cache adds 88 requested bytes to
the first BTreeMap node (11 value slots x 8 extra bytes), freed at operation end.

A missing NoteField is different from a stored NoteField: after each benchmark's
full Lua collection, the unrooted lookup-key string must be interned again.
That fixture therefore retains one allocation/34 bytes per 128-lookup operation;
the production dummy and present-field fixtures retain zero allocation traffic.
Warmed production lookups with collection stopped also pass the zero-churn test.

## Behavior and checks

- 2,214 affected debug tests passed (92 ignored manual benchmarks).
- 1,589 release unit tests passed; optimized builds also check exact
  non-NaN solver coefficient bits, including signed zero, subnormal values,
  infinities and overflow, up to the 65,536-point limit. NaNs retain their
  classification; optimized arithmetic may select a different NaN operand's
  payload for synthetic non-finite inputs, which Lua rejects. Capture tests
  preserve the observable non-reflexive equality behavior.
- Native ITGmania Position spline semantic parity: 2 tests passed.
- Lookup regressions cover registry edits/removal/replacement, registry
  metatables, sequence duplicates/groups/holes, actor metatables, malformed
  child names and conversion errors, including registry getters/setters that
  add sequence children during lookup. Sequence lookup still visits unrelated
  malformed children and retains their errors.
- Capture regressions compare finished tracks with the parent through metadata
  and coordinate edits; prior capture tests retain disabled modes, resize,
  replacements, player/lane identity, validation errors and the 128 MiB cap.
  Explicit identity tests retain NaN non-reflexivity and signed-zero equality.
- Warmed production dummy-player lookup and unchanged eight-lane capture
  perform zero allocations, reallocations, frees and requested/freed bytes. Solver allocation
  budgets require output-only storage for constant/short splines and only one
  scratch buffer for varying cubic splines, including the size limit.
- Clippy completed for all three affected libraries/tests; existing warnings
  are recorded in the check log. rustfmt of changed Rust files and
  `git diff --check` passed.

Reproduce benchmarks after completing all builds/checks:

```powershell
cargo test --release -p deadsync-gameplay -p deadsync-song-lua --lib --no-run --locked
cargo test --release -p deadsync-gameplay --lib spline_solver_perf::benchmark_spline_solver --locked -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --lib lua_util::note_field_lookup_perf::benchmark_note_field_lookup --locked -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --lib perframe::spline_dedup_perf::benchmark_spline_dedup --locked -- --exact --ignored --nocapture --test-threads=1
```

Set `DEADSYNC_PERF_REVERSE=1` for new-first rounds, removing it for old-first
rounds. The recorded six rounds invoke the already-built test executables
serially to exclude Cargo overhead and competing build work.
