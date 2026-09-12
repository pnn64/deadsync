# Score ranking performance, 0.5.1139

Baseline: `db7a654b3` / 0.5.1138. This pass applies the supplied Rust guide's
M-HOTPATH, M-MEM-REUSE and M-INITIAL-CAPACITY guidance to ITL score updates and
music-selection popularity rankings. Public signatures, ordering rules and
serialized score data are preserved.

## Three optimizations

1. **Search sorted points instead of scanning them for every rank.** A lower-bound
   search returns the first equal point value, preserving competition ranks for
   ties. This removes quadratic rank lookup work on large profiles. Maximum
   scores retain a constant-time lookup, and an all-equal profile receives rank
   one without sorting or performing any per-entry lookup.
2. **Reuse and size the ITL reconstruction buffers.** The three points vectors
   already owned by `ItlFileData` are cleared and refilled. A counting pass sizes
   the style partitions, and unknown-style points go directly into the dominant
   style (doubles on a tie), eliminating the temporary unknown-points vector.
   Known styles receive only their final style rank; their intermediate global
   ranks were overwritten. Warm rebuilds need no allocation or free. Buffers
   retain capacity across updates, including shrinkage of the score map.
3. **Select the popular-song prefix before sorting.** When at most half the songs
   are requested, partition around the requested count and sort just that prefix.
   Larger limits retain the full-sort path; a zero limit requires no sorting.
   The comparator's song-order/index tie-breakers preserve exact output order.
   The existing workspace continues to allocate nothing once warmed.

ITL rebuilding is used while loading and updating profile score data. The
popular-song method feeds both machine and per-profile Most Popular lists.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.0 (`88d9e12ae`, LLVM 22.1.8),
`x86_64-pc-windows-msvc`, release with full LTO. No dependencies or allocator in
production were changed. Tests use the existing counted System allocator helper.

Each operation uses three warm-ups and seven timing batches. Three independent
invocations alternate variant order, reversing the middle invocation. Reported
values are medians of the three invocation medians. Windows QueryThreadCycleTime
counts calling-thread CPU cycles. Allocation counters run separately from timing.
Requested bytes include allocation and reallocation traffic, not peak RSS.

The ITL benchmark includes three variants: **old** is the frozen reconstruction
with linear lookups; **search** changes only its lookup function to binary search;
**new** is the complete production implementation, including buffer reuse and
eliminating redundant work. This separates lookup savings from reconstruction
savings. The frozen body takes a generic lookup callback, so it can inline like
the original. The popular-song comparison uses the original method body.

Fixtures are built outside timing. Warm ITL measurements include the entire
rebuild and output assignments on the existing data object. The cold fixture
moves its prebuilt score map into fresh output storage, measures reconstruction
and output destruction, then retains the map for the next sample; it does not
clone input scores inside the timer. Popularity measurements include play-count
aggregation, candidate construction, selection/sorting and truncation; index
creation and optional song-order preparation are outside the timer, matching
reuse of those objects by the selection screen.

## Results

ITL timings include the complete reconstruction for each variant. Throughput
is millions of input scores per second; times are microseconds per operation.

| Case | us old | us search | us new | Thread cycles old / search / new | M scores/s old -> new |
| --- | ---: | ---: | ---: | ---: | ---: |
| small | 1.49 | 1.79 | 1.06 | 3,276 / 3,929 / 2,327 | 21.453 -> 30.206 |
| medium | 1,392.71 | 213.74 | 192.12 | 3,051,132 / 468,756 / 420,871 | 1.471 -> 10.660 |
| large | 19,837.33 | 1,030.50 | 1,001.02 | 43,421,434 / 2,229,023 / 2,192,201 | 0.413 -> 8.184 |
| ties | 80.61 | 79.87 | 73.76 | 176,828 / 175,044 / 161,676 | 25.406 -> 27.766 |
| unknown | 1,012.86 | 117.21 | 126.90 | 2,217,135 / 257,034 / 277,647 | 2.022 -> 16.139 |
| cold | 1,323.23 | 221.55 | 206.27 | 2,898,319 / 485,205 / 452,033 | 1.548 -> 9.929 |

ITL small/medium/large contain 32/2,048/8,192 scores. Other ITL fixtures contain
2,048 scores. Ties gives every entry 1,000 points; unknown assigns every entry
an unknown step type. The remaining fixtures mix single/double/unknown types
with deterministic distinct point values. All cases are warm except cold.

| Case | Allocations + reallocations old/search | Allocations + reallocations new | Requested bytes old/search -> new |
| --- | ---: | ---: | ---: |
| small | 4 + 2 | 0 + 0 | 496 -> 0 |
| medium | 4 + 8 | 0 + 0 | 32,752 -> 0 |
| large | 4 + 10 | 0 + 0 | 131,056 -> 0 |
| ties | 4 + 8 | 0 + 0 | 32,752 -> 0 |
| unknown | 4 + 9 | 0 + 0 | 40,944 -> 0 |
| cold | 4 + 8 | 3 + 0 | 32,752 -> 16,384 |

Free counts match allocation counts, and freed-byte traffic matches requested
traffic in every measured operation. New warm rebuilds have zero allocations,
reallocations, frees and byte traffic. Cold output uses three exact-sized
buffers totaling 16,384 bytes, versus 32,752 bytes of old allocation traffic.
Existing buffer capacities are retained across shrinking inputs; this avoids
future churn but is not a request to return that capacity to the allocator.

| Popularity case | us/op old -> new | Thread cycles/op old -> new | M songs/s old -> new |
| --- | ---: | ---: | ---: |
| small | 0.82 -> 0.85 | 1,804 -> 1,863 | 38.944 -> 37.676 |
| medium | 116.07 -> 66.90 | 253,147 -> 146,674 | 17.644 -> 30.615 |
| large | 587.99 -> 323.98 | 1,287,537 -> 709,036 | 13.932 -> 25.286 |
| uncached | 1,041.01 -> 241.06 | 2,278,722 -> 527,769 | 1.967 -> 8.496 |
| ties | 168.93 -> 87.64 | 369,588 -> 192,071 | 12.124 -> 23.369 |
| half | 118.70 -> 92.67 | 260,149 -> 203,118 | 17.254 -> 22.100 |
| full | 119.50 -> 118.38 | 261,927 -> 259,110 | 17.138 -> 17.301 |
| zero | 112.75 -> 48.44 | 246,113 -> 106,235 | 18.164 -> 42.277 |

Popularity small uses 32 songs and a limit of 30; large uses 8,192 songs.
The other cases use 2,048 songs. Normal limits are 50, matching the selection
screen; half/full/zero request 1,024/all/none. Uncached uses the supplied song
comparator with tied play counts; the other cases precompute song-order ranks.
All old and new popularity operations have zero allocation churn once warmed.

Binary lookup alone removes about 85% of cycles on the medium profile and 95%
on the large one. The complete ITL refactor uses about 86%/95% fewer cycles,
raising throughput from 1.47 to 10.66 million scores/s for the medium fixture
and from 0.41 to 8.18 million scores/s for the large one. Buffer reuse eliminates
all warm heap traffic and halves cold requested bytes. Medium/cold reconstruction
uses about 10%/7% fewer cycles than the lookup-only variant. All-equal profiles
also improve with the explicit rank-one shortcut.

The 50-song popularity lists use about 42-45% fewer cycles on medium/large
libraries, with about 1.7-1.8 times the throughput. Prepared ties save about 48%;
unprepared ties save about 77%. Their warm allocation count remains zero.

The measurements do not show a win for every isolated variant/control. Lookup
alone costs about 20% more cycles on 32 scores, while the complete version saves
29%. The all-unknown complete rebuild uses about 8% more cycles than lookup-only,
but saves 87% against the original and eliminates allocation churn. The small
popularity control, which still fully sorts, is about 3% slower (28 ns/op);
paired invocation comparisons for that small control ranged from 3% to 11%
slower. Full-list timing is essentially unchanged. Small differences should be
read in light of batch and invocation variability. These are operation benchmarks, not
whole-game frame-rate or end-to-end profile-loading measurements.

## Validation

- Baseline: 223 score unit tests passed.
- Five new regression/allocation tests check lower-bound ranks with ties, missing
  values, zero and u32::MAX, complete serialized ITL output and unrelated fields,
  unknown-style assignment, case/whitespace handling, equal-score profiles,
  shrinking/empty maps, repeated updates, cold allocation limits, and warm
  allocation-free rebuilding.
- Popularity checks compare exact ordered results and aggregated play counts,
  with prepared/unprepared comparators, duplicate/shared chart hashes,
  saturating play counts, optional zero-play songs, and limits from zero through
  the selection boundary to usize::MAX. Warm workspaces must remain allocation-free.
- Final score unit suite: 228 passed in debug and release, zero failures; two
  manual benchmarks are ignored. Three score integration tests passed.
- Profile consumers: 215 deadsync-profile and 17 deadsync-profile-gameplay tests
  passed, zero failures.
- All-target Clippy with `-D clippy::perf` passed. Existing unrelated warnings
  remain; none were reported in the changed routines or new benchmark files.
- `cargo check --workspace --bins` passed.
- Frozen reference bodies were verified against `db7a654b3`, apart from the
  explicit generic lookup parameter used to isolate the ITL search change.
- Changed production sections and new benchmark modules pass Rustfmt.
- Cargo.toml and Cargo.lock bump the patch exactly once: 0.5.1138 -> 0.5.1139.

## Reproduction

```powershell
cargo test -p deadsync-score --all-targets -- --test-threads=1
cargo test --release -p deadsync-score --lib -- --test-threads=1
cargo test -p deadsync-profile -p deadsync-profile-gameplay --lib -- --test-threads=1
cargo clippy -p deadsync-score --all-targets -- -D clippy::perf
cargo check --workspace --bins
cargo test --release -p deadsync-score --lib score_ranking_bench -- --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-score --lib popularity_bench -- --ignored --nocapture --test-threads=1
```

Run the manual benchmarks three times with `DEADSYNC_PERF_REVERSE=1` only for the
middle run. Avoid other builds/tests during timing. Raw outputs, comparison
script, machine-readable metrics and validation logs are retained locally in the
ignored `target/score-perf/` directory.
