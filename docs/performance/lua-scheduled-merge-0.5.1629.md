# Lua scheduled-sample merging - 0.5.1629

Parent: `b015e8b27` (0.5.1628). Date: 2026-09-30.

This pass applies the local performance guide's M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT recommendations to the scheduled tween merger. It makes three
changes:

1. Step-only writes no longer construct an unused start value. Positive-duration
   tweens still retain their required start anchor. Color steps avoid temporary
   Arc clones and, when no historical sample qualifies, avoid allocating and
   discarding an owning baseline-color snapshot.
2. Historical-value lookup checks the last sample first, uses a reverse scan
   for histories of up to 32 samples, and searches larger ordered histories
   with `partition_point`. Older queries in larger histories change from a
   linear scan to a logarithmic search. Neither search repeats the rejected
   last sample. The partition predicate includes the leading
   negative-NaN prefix before filtering the result; it uses numerical comparison
   to include both signs of zero. The selected sample matches the old reverse
   scan, including stable ties and NaN payloads.
3. The merger omits its final scan over every track. The initial pass already
   canonicalizes all tracks, ordered appends compact their final ties, and
   out-of-order appends canonicalize their own track. Untouched tracks and an
   empty scheduled batch still receive the initial canonicalization pass.

There are no new dependencies, public API changes, unsafe code or persistent
cache allocations. Output samples retain ownership. Zero temporary churn is
verified for prepared step writes and borrowed lookups; required anchors,
out-of-order sorting and new output storage can still allocate.

## Measurements

The [raw CSV](lua-scheduled-merge-0.5.1629.csv) contains 320 rows: 40 workloads,
two implementations and four independent runs. Percentage ranges below cover
all four paired runs. Absolute cycle/allocation/byte figures are from run 1;
allocation and requested-byte counts are identical across runs.

| Workload | Thread cycles/batch, old -> new | CPU reduction | Throughput gain | Allocations/batch, old -> new | Requested bytes/batch, old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| 16 color steps before stored history (16 samples/track) | 9,195.1 -> 5,305.6 | 39.6% to 42.3% | 83.2% to 92.1% | 16 -> 0 | 1,280 -> 0 |
| 16 color steps after stored history (16 samples/track) | 4,489.5 -> 2,949.6 | 30.9% to 34.3% | 74.3% to 77.6% | 0 -> 0 | 0 -> 0 |
| 32 history queries before 8,192 stored samples | 282,374.8 -> 3,203.3 | 98.8% to 99.2% | 8241.7% to 13236.6% | 0 -> 0 | 0 -> 0 |
| 32 history queries midway through 8,192 stored samples | 156,688.7 -> 2,373.3 | 98.3% to 98.5% | 5858.7% to 6538.1% | 0 -> 0 | 0 -> 0 |
| Empty merge, 1 track x 1,024 samples | 7,229.8 -> 3,611.5 | 49.4% to 55.6% | 97.9% to 135.0% | 0 -> 0 | 0 -> 0 |
| Empty merge, 16 tracks x 1,024 samples | 144,371.0 -> 74,678.0 | 47.8% to 51.3% | 91.7% to 105.4% | 0 -> 0 | 0 -> 0 |
| Empty merge, 64 tracks x 1,024 samples | 561,021.4 -> 277,703.5 | 46.7% to 50.9% | 87.6% to 103.7% | 0 -> 0 | 0 -> 0 |
| 16 color tweens midway through 1,024 samples/track | 384,111.3 -> 323,697.4 | 15.7% to 20.6% | 18.4% to 26.4% | 16 -> 16 | 525,312 -> 525,312 |
| 16 color tweens after 1,024 samples/track | 155,529.5 -> 81,331.6 | 44.7% to 48.6% | 83.0% to 96.9% | 0 -> 0 | 0 -> 0 |
| 16 color steps before 1,024 samples/track | 489,135.1 -> 362,065.2 | 18.2% to 26.0% | 22.7% to 35.6% | 32 -> 16 | 526,080 -> 524,800 |

Short before-history color steps eliminate 16 temporary allocations and frees,
or 1,280 requested/freed bytes, per batch. Long before-history steps eliminate
the same snapshots while retaining 16 temporary buffers for out-of-order sorting.
After-history steps avoid unused color-owner clones; their storage was already
allocation-free. Required baseline-color anchors retain their allocation costs.
Borrowed lookups and warm canonicalization batches have zero allocation churn
in both versions. Newly created tracks still allocate and grow their output:
16 reallocations per new-track batch in both versions. No other workload reallocates.

The remaining cases are included below. Negative percentages mean more CPU
work or lower throughput. The final implementation uses a short reverse scan
through 32 samples; after-last queries keep their direct last-sample check.

| Workload (CSV name) | CPU reduction | Throughput gain | Allocations/batch, old -> new | Requested bytes/batch, old -> new |
| --- | ---: | ---: | ---: | ---: |
| `merge_lookup_0_before` | -9.3% to 14.3% | -8.4% to 23.8% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_0_middle` | -31.9% to -2.8% | -24.3% to -2.6% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_0_after` | -43.7% to 5.2% | -30.5% to -0.2% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_16_before` | 0.4% to 17.9% | 0.4% to 21.8% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_16_middle` | -8.2% to 13.5% | -6.8% to 15.6% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_16_after` | 7.4% to 12.9% | 8.0% to 14.8% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_256_before` | 77.3% to 82.4% | 341.3% to 471.9% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_256_middle` | 67.6% to 71.9% | 208.8% to 260.5% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_256_after` | 3.3% to 15.7% | 2.4% to 18.8% | 0 -> 0 | 0 -> 0 |
| `merge_lookup_8192_after` | 1.1% to 11.6% | -1.2% to 15.4% | 0 -> 0 | 0 -> 0 |
| `merge_canonical_1_16` | 24.2% to 28.9% | 32.8% to 46.0% | 0 -> 0 | 0 -> 0 |
| `merge_canonical_16_16` | 18.1% to 45.0% | 22.3% to 83.1% | 0 -> 0 | 0 -> 0 |
| `merge_canonical_64_16` | 36.8% to 52.5% | 58.4% to 110.9% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_false_step_before` | 21.4% to 22.9% | 34.6% to 37.4% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_false_step_after` | 5.3% to 30.9% | 48.7% to 78.2% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_false_tween_before` | 9.1% to 16.1% | 18.9% to 25.4% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_false_tween_middle` | 13.4% to 19.2% | 19.2% to 29.8% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_false_tween_after` | 25.6% to 32.0% | 54.9% to 62.8% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_true_tween_before` | 9.3% to 23.7% | 11.0% to 35.5% | 16 -> 16 | 1,280 -> 1,280 |
| `merge_full_16_colors_true_tween_middle` | 12.3% to 12.6% | 15.4% to 17.8% | 0 -> 0 | 0 -> 0 |
| `merge_full_16_colors_true_tween_after` | 22.9% to 26.2% | 47.0% to 54.3% | 0 -> 0 | 0 -> 0 |
| `merge_full_1024_colors_false_step_before` | 21.6% to 23.7% | 27.9% to 31.1% | 16 -> 16 | 524,800 -> 524,800 |
| `merge_full_1024_colors_false_step_after` | 42.2% to 47.5% | 75.0% to 93.0% | 0 -> 0 | 0 -> 0 |
| `merge_full_1024_colors_false_tween_before` | 19.0% to 23.0% | 23.6% to 29.8% | 16 -> 16 | 525,312 -> 525,312 |
| `merge_full_1024_colors_false_tween_middle` | 18.5% to 21.2% | 22.8% to 27.3% | 16 -> 16 | 525,312 -> 525,312 |
| `merge_full_1024_colors_false_tween_after` | 40.1% to 53.5% | 68.1% to 114.5% | 0 -> 0 | 0 -> 0 |
| `merge_full_1024_colors_true_step_after` | 43.3% to 48.7% | 78.3% to 97.5% | 0 -> 0 | 0 -> 0 |
| `merge_full_1024_colors_true_tween_before` | 12.8% to 31.3% | 14.6% to 46.2% | 32 -> 32 | 526,592 -> 526,592 |
| `merge_new_tracks_colors_false` | -2.8% to 5.6% | -2.7% to 7.5% | 16 -> 16 | 2,560 -> 2,560 |
| `merge_new_tracks_colors_true` | 1.7% to 11.1% | 1.7% to 9.3% | 32 -> 32 | 3,840 -> 3,840 |

Empty-history controls have slower CPU medians in some or all runs, including
up to 43.7% more cycles in the after-history empty case. The 16-sample middle
case ranges from 8.2% more to 13.5% fewer cycles; new scalar-track construction
ranges from 2.8% more to 5.6% fewer. No consistent gain is established for
those controls. All 20 full-merge workloads use fewer cycles in every run.
The benchmark does not measure peak memory,
full-song compilation time or game frame time.

## Method and reproduction

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.1. The parent and
current implementations run in one release test executable with optimization
level 3 and fat LTO. QueryThreadCycleTime counts calling-thread CPU cycles.
The existing scoped System allocator records calling-thread allocations,
reallocations, frees and requested/freed bytes. Allocation accounting is separate
from timing. Byte totals describe allocator churn, not peak RSS or resident memory.

Each row contains the median of seven timing samples after warmup, their elapsed
range, throughput and a separately counted operation. Four runs alternate
old-first and new-first. Timing inputs are black-boxed. All measured code runs
on the benchmark thread, with tests run serially.

The 40 workloads cover:

- Lookup: 32 borrowed queries per batch over 0, 16, 256 or 8,192 ordered samples,
  before the first sample, midway through the history or after its last sample.
  Inputs and storage exist outside measurement. Small cases use 1,024 iterations
  per timing sample; larger cases use 128.
- Canonicalization: empty scheduled batches with 1, 16 or 64 existing tracks and
  16 or 1,024 samples per track. Both versions still canonicalize the entire
  input. The repeated warm operations measure the removed second pass without
  changing output. Each timing sample uses 128 iterations.
- Full merge: 16 existing tracks with 16 or 1,024 samples each, scalar or color
  values, and one scheduled write per track. Cases include steps before/after
  the stored history and positive-duration tweens before/midway/after it.
  Each operation starts from a fresh prepared fixture; setup and final fixture
  destruction are excluded. Temporary clones, fallback snapshots, sorting and
  destruction of replaced values inside the merger are included. Output capacity
  is equally preallocated. Each timing sample uses 64 iterations.
- New tracks: 16 scalar or color tweens create their output tracks and samples.
  Output allocation and growth are included, while fixture setup and final
  destruction are excluded. Each timing sample uses 128 iterations.

Fresh-fixture measurements include per-operation clock overhead equally in both
variants. CPU and elapsed-time medians are computed independently.

Before-history steps use beat -2 with stored samples starting at beat 0. This
exercises the baseline-color snapshot that the old merger constructed and then
discarded. After-history steps exercise unused cloning of an existing color
owner. Positive-duration cases retain owning start samples in both variants.
Metadata strings are absent in timing fixtures. Existing/new track writes report
scheduled writes per second; lookup reports queries per second; canonicalization
reports input samples per second.

The test-only full merge is frozen from the parent, with visibility and formatting
changes only. Its shared sorting and ordered-append helpers are unchanged. The
test-only lookup wrapper contains the parent's reverse-scan expression. Both
were audited against the parent. Full-merge measurements include all three
optimizations, while lookup and empty-batch measurements isolate history search
and the removed final pass. These are merge costs, not full song compilation or
whole-game frame rate.

```powershell
cargo test -p deadsync-song-lua --lib scheduled_merge
cargo test -p deadsync-song-lua --lib
cargo test -p deadsync-song-lua --release --lib
cargo test -p deadsync-song-lua --release --lib scheduled_merge -- --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib scheduled_merge -- --ignored --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

## Behavior validation

Eight new regressions compare the current code to the parent. They check the
exact selected sample address in sorted histories, random f32 bit patterns,
both signs of zero, signaling/quiet NaNs and payloads, infinities, stable ties,
epsilon-connected runs, rewinds and nonfinite schedule bounds. Full merges compare
sample beat/value bits, track order, lookup maps, drained-buffer capacity, empty
and newly created tracks, retained color-owner identity and repeated batches.
Allocation checks require no temporary churn for prepared before-history steps,
empty canonicalization batches and borrowed history queries. The focused filter
also runs the two existing scheduled-merge regressions.

- All eight new regressions pass in debug and release. The full suites also
  pass the two existing scheduled-merge regressions.
- Full debug and release library suites each report **660 passed, 4 failed,
  62 ignored**. The parent reported 652 passed, 4 failed, 61 ignored. Failure
  names, source locations and assertion values match the parent's recorded
  results: actor proxy visibility, local/hidden screen proxy visibility,
  queued command-builder visibility, and notefield-column position
  (`-96:-135` versus expected `-96:-125`). `lib.rs` is unchanged.
- Four serial release benchmark runs pass, alternating old-first and new-first.
  Each executes one ignored benchmark test and produces 80 data rows.
- `cargo clippy -p deadsync-song-lua --all-targets` succeeds with the existing
  warnings (75 library-test warnings, matching the parent). New code and fixtures
  have no Clippy warnings.
- Formatting, `git diff --check`, frozen-baseline verification and
  `cargo metadata --locked --no-deps` pass.

The workspace patch version advances exactly once, 0.5.1628 -> 0.5.1629.
Cargo.lock updates the three inheriting packages. The four excluded files are
omitted from the commit.
