# Spline work performance 0.5.1682

Parent: `14ea3817e` (0.5.1681). Date: 2026-10-02.

Three optimizations apply `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance to spline compilation and rendering:

1. Reuse unchanged position/zoom spline coefficients during the loader's
   repeated Lua samples. Previously every sample allocated points, solved three
   axes, allocated the final Arc, and only then discarded unchanged frames.
   Capture now retains one reusable point buffer and one sample buffer. It
   checks authored coordinates against the previous lane frame's constant
   coefficients before deciding to solve. Lua tables may change in place;
   pointer identity is never used as evidence that points are unchanged.
   Short mode strings use the existing inline text conversion, with the same
   numeric coercions and UTF-8 errors. Metadata-only changes share coefficients
   but still emit the same frames. Logical coefficient-byte accounting and the
   128 MiB track limit retain the parent's behavior, including overflow cases.
2. Resolve lane receptor positions once in `prepare_notefield`. Notes, holds
   and feedback reuse these frame-local values. Preparation already evaluated
   receptors for lane placement; the new fixed array retains that result and
   also replaces a second evaluation for absolute lane Y. Track transitions
   and clock rewinds rebuild the values with the next prepared frame.
3. Evaluate hold position and direction together. The previous strip sampler
   requested offsets for direction and then evaluated the spline again for
   position. The combined evaluator uses one sample with the same coordinate
   arithmetic. The strip sampler also reuses its already computed X/depth
   effects for the returned base position.

No dependency or unsafe production code was added. Rendering retains zero
heap churn in the measured sampling paths. The prepared frame adds 120 bytes
of fixed receptor storage (`MAX_COLS = 10`). Loader point scratch reserves
exactly the largest authored point count seen, bounded by 65536 points
(786432 requested bytes), and is dropped when compilation ends. Sample-buffer
capacity follows the authored lane count. This is compile-local reuse, with
no global cache, locking, insertion during gameplay or eviction policy.
The existing named-child lookup still creates two small Lua allocations per
player lookup; complete spline compilation is not allocation-free.

## Measurement method

The paired baselines freeze the parent's spline reader/capture and prepared
sampling methods. The spline solver itself is unchanged. The cached-only hold
variant retains duplicate sampling while using the new receptor values, so
combined-path savings can be assessed separately from receptor caching.

Measurements run on Windows 11 Pro 10.0.26100, Intel Xeon E5-2696 v4,
22 cores/44 logical processors, Rust/Cargo 1.98.1. Executables use the
repository's release profile (opt-level 3, full LTO). All build and test work
finishes before timing. Six serial rounds alternate old-first/new-first.
Each row reports a median of seven batches and the minimum/maximum batch
elapsed time. Windows QueryThreadCycleTime measures calling-thread CPU cycles;
throughput is useful lane samples or note/hold samples per second.

Allocator accounting runs separately from timing using the repository's
thread-local System wrapper. It includes mlua's Lua allocations, because this
vendored Lua configuration routes its allocator through Rust's global allocator.
Requested/freed bytes include the full new/old layouts of reallocations.
These metrics describe allocator traffic, not peak RSS or allocator metadata.
The wrapper's disabled TLS check is present in both timed implementations.

Capture operations start with an empty capture, perform 120 complete samples,
then destroy captured output and collect Lua garbage. Lua fixtures are built
outside timing/accounting; metadata/point edits and final collection are
included equally in both variants. This makes free accounting independent of
an earlier operation's garbage-collection phase. Each fixture captures both
position and zoom. Note/hold operations process 256 queries into black-boxed
outputs with no output allocation. Prepared-frame construction is outside
those timings; its receptor evaluation already existed in the parent.

These are CPU microbenchmarks of compiler capture and renderer sampling.
They do not establish an end-to-end song-load, gameplay FPS or GPU improvement.

## Results

The [raw CSV](spline-work-0.5.1682.csv) contains all **192 measurements**:
14 workloads, two variants each plus four cached-only hold controls, six rounds.
The tables report **medians of the six paired percentage changes**, rather
than a ratio of independently aggregated times. Each variant records the same
allocation counts and byte traffic in all six rounds. CPU ranges show the
smallest/largest paired round reduction; a negative value means that round
was slower. Positive reductions mean improvement.

### Loader capture

Each operation performs 120 samples, including output destruction and Lua
collection. Throughput counts lane samples, with both position and zoom read.

| Workload | Elapsed reduction | CPU-cycle reduction (round range) | Throughput increase |
| --- | ---: | ---: | ---: |
| 1 lane x 2 points, steady | 18.1% | 18.1% (14.4..22.0%) | 22.1% |
| 8 lanes x 32 points, steady | 16.3% | 16.3% (10.9..19.6%) | 19.5% |
| 8 lanes x 256 points, steady | 16.2% | 16.2% (-7.0..22.7%) | 19.3% |
| 1 lane x 256 points, metadata edits | 26.7% | 26.7% (24.2..30.9%) | 36.5% |
| 1 lane x 32 points, point edits | 2.0% | 2.0% (-4.6..10.6%) | 2.1% |
| 8 lanes, empty Position handlers | 11.1% | 11.2% (4.4..14.6%) | 12.5% |

| Workload | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
| --- | ---: | ---: | ---: |
| 1 lane x 2 points, steady | 1,923 -> 253 | 0 -> 0 | 130,638 -> 11,222 |
| 8 lanes x 32 points, steady | 12,010 -> 316 | 120 -> 1 | 7,364,094 -> 67,358 |
| 8 lanes x 256 points, steady | 12,010 -> 316 | 120 -> 1 | 57,253,374 -> 442,782 |
| 1 lane x 256 points, metadata edits | 1,923 -> 253 | 5 -> 5 | 7,219,854 -> 84,958 |
| 1 lane x 32 points, point edits | 1,923 -> 1,205 | 5 -> 5 | 983,694 -> 831,550 |
| 8 lanes, empty Position handlers | 2,410 -> 251 | 120 -> 1 | 206,334 -> 13,470 |

The 8-lane, 32-point steady fixture removes **97.4% of allocation/free calls**
and **99.1% of requested/freed bytes**, with CPU reduction in every round.
Metadata-only edits eliminate repeated solving and retain shared coefficients
across emitted frames: CPU falls 26.7%, requested bytes fall 98.8%.
A warmed 8-lane capture's scoped budget is only **2 allocations / 80 bytes per
sample**, belonging to the existing named-child lookup; spline storage and
mode conversion add no churn.

The large 256-point steady fixture has 16.2% median paired CPU reduction, but
one round is 7.0% slower and timing ranges overlap. Point-edit capture is
approximately CPU-neutral (2.0% median reduction, -4.6%..10.6% across rounds),
while allocations fall 37.3% and requested bytes fall 15.5%. Changed splines
still need freshly owned solved coefficients. These controls do not support
claiming a uniform CPU improvement for every input or run.

### Frame receptor reuse

Each operation evaluates 256 note-offset queries. Both versions allocate,
reallocate and free **zero bytes** in all four workloads.

| Spline | Elapsed reduction | CPU-cycle reduction (round range) | Throughput increase |
| --- | ---: | ---: | ---: |
| Disabled | 24.6% | 24.6% (6.2..32.4%) | 32.7% |
| Two-point relative | 35.8% | 35.8% (25.1..38.4%) | 55.8% |
| Cubic relative | 41.0% | 41.0% (40.3..45.6%) | 69.6% |
| Cubic absolute | 41.3% | 41.3% (39.6..44.1%) | 70.2% |

### Combined hold evaluation

The first comparison isolates the combined evaluator against **cached-only**
receptor sampling. All variants allocate/reallocate/free zero bytes.
The disabled control requires no active spline; its remaining differences
come from call/branch organization and eliminating duplicate disabled samples.

| Spline | Elapsed reduction vs cached-only | CPU-cycle reduction vs cached-only (round range) | Throughput increase vs cached-only |
| --- | ---: | ---: | ---: |
| Disabled | 20.8% | 20.8% (0.9..33.8%) | 26.3% |
| Two-point relative | 40.7% | 40.7% (39.9..44.4%) | 68.8% |
| Cubic relative | 40.3% | 40.2% (39.9..40.8%) | 67.4% |
| Cubic absolute | 43.4% | 43.4% (41.6..44.5%) | 76.7% |

The complete parent-to-new comparison includes both rendering optimizations:

| Spline | Elapsed reduction | CPU-cycle reduction (round range) | Throughput increase |
| --- | ---: | ---: | ---: |
| Disabled | 52.4% | 52.4% (44.2..62.3%) | 111.9% |
| Two-point relative | 61.6% | 61.7% (60.7..62.6%) | 160.7% |
| Cubic relative | 64.9% | 64.9% (64.1..70.6%) | 185.3% |
| Cubic absolute | 58.3% | 58.3% (56.5..64.9%) | 139.6% |

## Behavior checks

Differential capture tests compare frame times, player/lane identities,
metadata, coefficients and track-byte accounting. They cover unchanged inputs,
metadata-only changes, in-place point edits, table replacement, signed zero,
size changes, disabled/Position modes, numeric mode coercion, invalid UTF-8,
missing axes, late malformed points, nonfinite conversions and finite inputs
whose solved coefficients overflow. A scoped allocation budget ensures steady
captures allocate only the existing named-child lookup tables, with no spline
buffer or mode-string churn.

Differential renderer checks compare all components bit-for-bit, allowing NaN
pairs, across linear/cubic, relative/absolute and disabled splines. Cases cover
negative truncation, segment boundaries, clamping, signed zero, invalid beat
conversions, nonfinite beats/zoom and overflowed coefficients. Sampling has a
zero-allocation regression assertion. An actual notefield preparation test
checks receptor refresh across track changes, disabling and clock rewinds.
Existing native hold-orientation and playback suites exercise downstream use.

Validation: all 1371 affected-crate development tests pass (80 opt-in tests
ignored), including playback/integration suites. All 1199 release unit tests
pass (76 ignored). Both root native C++ spline-parity tests pass. Rustfmt and
`git diff --check` pass for changed files. Clippy completes successfully; its
existing warnings are in unchanged code, with none in the new benchmark files
or changed production functions.

## Reproduction

```powershell
cargo test -p deadsync-song-lua -p deadsync-notefield --locked
cargo test -p deadsync-song-lua -p deadsync-notefield --release --lib --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity position_spline --locked
cargo test -p deadsync-song-lua --release --lib --locked benchmark_spline_capture -- --ignored --nocapture --test-threads=1
cargo test -p deadsync-notefield --release --lib --locked benchmark_spline_sampling -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat both benchmark commands, alternating this variable for six rounds.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
