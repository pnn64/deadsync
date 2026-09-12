# Song grouping performance - 0.5.1150

Baseline: `b5093e110` / 0.5.1149. This pass applies `M-HOTPATH`,
`M-INITIAL-CAPACITY`, `M-MEM-REUSE`, and `M-THROUGHPUT` from the supplied
`rust-performance.md` to the grouping functions used by Select Music.

## Three changes

1. **Size contiguous genre/length groups before filling them.** Multi-group
   output vectors and each song vector have their final capacities. This
   removes repeated growth and copying and avoids retaining spare capacity.
   Raw genre spelling still defines boundaries; whitespace-only names share
   the unknown group, exactly as before.
2. **Reuse the input buffer for a single output group.** Once group counting
   finds a single run, the already sorted input vector becomes that group's
   storage. This removes another complete allocation/copy pass. Spare input
   capacity is shrunk before retaining the vector.
3. **Deduplicate larger meter lists with a native-word bit set.** A local
   64-bit set represents meters 0 through 63. Only larger meters need the
   temporary sorting vector; common values are emitted once in ascending
   order. Lists of at most 16 charts retain the small-list routine. Existing
   scratch vectors remain reusable. Chart-type, note-data and edit-preference
   rules are unchanged, including support for every u32 meter value.

No public API, dependency, allocator, or persistent cache changes. Production
changes use safe Rust. Nonempty returned groups and meter lists still own
heap storage. Zero-allocation assertions apply to empty meter output and warmed
meter scratch, not every complete grouping operation.

## Method

Nine baseline function bodies are frozen in
`crates/deadsync-simfile/tests/perf/song_grouping/baseline.rs`, verified against
the baseline commit ignoring whitespace. Imports and test visibility differ. They
use the same song/group types and unchanged comparators/metadata helpers as
the production routines, and call their own frozen grouping/meter helpers.
The benchmark compiles inside the simfile unit-test binary and invokes the
actual production functions through opaque function pointers.

Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0 (`88d9e12ae`,
LLVM 22.1.8), x86_64-pc-windows-msvc. Repository release profile: opt-level 3,
full LTO. Each workload uses three warmups and seven timing batches. Tables
show medians of three invocation medians, alternating old/new, new/old,
old/new order. No concurrent builds or tests run during measurement.
`QueryThreadCycleTime` counts the calling thread's CPU cycles; these operations
launch no workers. Throughput is input songs/s, or input charts/s for the
standalone meter cases. Empty input has zero useful units.

Allocation accounting uses the existing scoped System-allocator wrapper,
separately from timing. Counts include result disposal and the input Arc-vector
copy required by both consuming APIs. Fixture construction and pre-sorting for
`genre_runs` fixtures happen outside measurement. Requested/freed
bytes describe allocator traffic, not resident or peak memory. These are
operation measurements, not whole-game FPS or disk-loading benchmarks.

Fixtures cover empty, single and 16-song inputs; 4,096 shuffled songs with
mixed, single, singleton-per-song, or case/whitespace genre groups. Equal keys
belong to distinct Arcs to exercise stable ordering. Length groups use mixed lengths,
one minute bucket, or one bucket per song. `genre_runs` measures group assembly
alone, after sorting the same input by raw ASCII-folded genre and title.

`single_group_reuse_control` is explicitly adapted, rather than frozen: its
baseline uses the new exact-sizing algorithm with the single-group reuse
branch disabled. It isolates buffer reuse from the sizing improvement; the
other cases use frozen original implementations. These gains overlap and
must not be added together.

Meter fixtures contain zero, five, 16, 32, 128 or 512 charts. Common duplicate
fixtures cycle through 16 meters; overflow fixtures cycle through 257 meters;
the 128-chart unique fixture contains meters 0 through 127. Complete meter
libraries cover 4,096 songs with five charts each and 128 songs with 128 edit
charts each, cycling through 16 unique meters. Grouping batches contain 128
operations below 64 songs and eight otherwise; standalone meter batches use
512 operations.

## Timing and throughput

Positive cycle savings indicate improvement. Component results and complete
sort/group results overlap and must not be added together. Small differences
can be noise; all measured regressions are retained below.

| Workload | Old ns/op | New ns/op | Old cycles/op | New cycles/op | Cycles saved | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|---:|
| length_empty | 18.8 | 19.5 | 51.4 | 53.2 | -3.5% | 0.0 | 0.0 |
| genre_empty | 26.6 | 26.6 | 68.6 | 68.6 | 0.0% | 0.0 | 0.0 |
| length_single | 348.4 | 146.1 | 773.4 | 331.0 | 57.2% | 2,869,955.2 | 6,844,919.8 |
| genre_single | 407.8 | 203.9 | 905.4 | 459.6 | 49.2% | 2,452,107.3 | 4,904,214.6 |
| length_small | 2,641.4 | 1,939.1 | 5,813.3 | 4,211.7 | 27.6% | 6,057,379.5 | 8,251,410.2 |
| genre_small | 4,284.4 | 3,837.5 | 9,409.4 | 8,433.6 | 10.4% | 3,734,500.4 | 4,169,381.1 |
| length_mixed | 1,457,375.0 | 1,467,237.5 | 3,184,643.1 | 3,196,413.9 | -0.4% | 2,810,532.6 | 2,791,640.8 |
| genre_mixed | 3,637,575.0 | 3,605,412.5 | 7,968,261.8 | 7,902,631.1 | 0.8% | 1,126,024.9 | 1,136,069.7 |
| length_one | 1,974,212.5 | 2,013,825.0 | 4,317,894.4 | 4,404,404.6 | -2.0% | 2,074,751.3 | 2,033,940.4 |
| genre_one | 5,253,400.0 | 5,183,112.5 | 11,499,742.2 | 11,337,312.4 | 1.4% | 779,685.5 | 790,258.7 |
| length_many | 1,187,900.0 | 1,148,050.0 | 2,604,614.6 | 2,513,659.4 | 3.5% | 3,448,101.7 | 3,567,788.9 |
| genre_many | 3,102,187.5 | 3,078,312.5 | 6,786,390.9 | 6,740,351.1 | 0.7% | 1,320,358.6 | 1,330,599.2 |
| length_case | 1,460,875.0 | 1,495,162.5 | 3,192,161.2 | 3,276,174.5 | -2.6% | 2,803,799.1 | 2,739,501.6 |
| genre_case | 5,333,612.5 | 5,415,762.5 | 11,687,798.6 | 11,860,627.9 | -1.5% | 767,959.8 | 756,310.9 |
| genre_runs_mixed | 270,100.0 | 272,450.0 | 592,951.9 | 596,381.5 | -0.6% | 15,164,753.8 | 15,033,951.2 |
| genre_runs_one | 265,212.5 | 175,037.5 | 578,986.1 | 384,564.0 | 33.6% | 15,444,219.3 | 23,400,699.9 |
| genre_runs_many | 960,037.5 | 902,475.0 | 2,104,483.6 | 1,977,338.2 | 6.0% | 4,266,500.0 | 4,538,629.9 |
| genre_runs_case | 456,550.0 | 463,225.0 | 1,001,166.9 | 1,013,678.4 | -1.2% | 8,971,635.1 | 8,842,355.2 |
| single_group_reuse_control | 257,912.5 | 186,675.0 | 565,898.4 | 410,108.4 | 27.5% | 15,881,355.1 | 21,941,877.6 |
| meters_empty | 12.5 | 10.7 | 30.0 | 26.2 | 12.7% | 0.0 | 0.0 |
| meters_typical | 161.3 | 162.1 | 356.3 | 358.8 | -0.7% | 30,992,736.1 | 30,843,373.5 |
| meters_threshold | 390.8 | 363.5 | 860.4 | 801.7 | 6.8% | 40,939,530.2 | 44,019,344.4 |
| meters_medium | 790.0 | 627.5 | 1,736.3 | 1,379.6 | 20.5% | 40,504,326.3 | 50,992,841.6 |
| meters_duplicates | 13,406.8 | 9,820.7 | 29,379.6 | 21,517.4 | 26.8% | 38,189,473.1 | 52,134,760.0 |
| meters_overflow | 15,172.7 | 14,625.0 | 33,275.7 | 31,924.4 | 4.1% | 33,744,915.3 | 35,008,547.0 |
| meters_unique | 2,443.2 | 2,673.2 | 5,358.5 | 5,826.2 | -8.7% | 52,391,078.4 | 47,881,931.8 |
| meter_dense_library | 1,552,862.5 | 1,337,375.0 | 3,404,747.0 | 2,929,940.9 | 13.9% | 82,428.4 | 95,709.9 |
| meter_library | 12,431,825.0 | 12,510,012.5 | 27,219,344.1 | 27,400,679.0 | -0.7% | 329,477.0 | 327,417.7 |

## Allocation churn

`A/R/F` means allocation/reallocation/free calls per complete operation.

| Workload | Old A/R/F | New A/R/F | Old requested bytes | New requested bytes | Old freed bytes | New freed bytes |
|---|---:|---:|---:|---:|---:|---:|
| length_empty | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 |
| genre_empty | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 |
| length_single | 3/0/3 | 2/0/2 | 232 | 56 | 232 | 56 |
| genre_single | 4/0/4 | 3/0/3 | 238 | 62 | 238 | 62 |
| length_small | 3/2/3 | 2/0/2 | 544 | 176 | 544 | 176 |
| genre_small | 14/1/14 | 14/0/14 | 932 | 580 | 932 | 580 |
| length_mixed | 16/85/16 | 16/0/16 | 160,736 | 98,928 | 160,736 | 98,928 |
| genre_mixed | 51/147/51 | 51/0/51 | 166,110 | 99,614 | 166,110 | 99,614 |
| length_one | 4/10/4 | 3/0/3 | 131,232 | 65,584 | 131,232 | 65,584 |
| genre_one | 5/10/5 | 4/0/4 | 131,237 | 65,589 | 131,237 | 65,589 |
| length_many | 4098/10/4098 | 4098/0/4098 | 589,600 | 294,864 | 589,600 | 294,864 |
| genre_many | 8195/10/8195 | 8195/0/8195 | 630,592 | 335,872 | 630,592 | 335,872 |
| length_case | 16/85/16 | 16/0/16 | 160,736 | 98,928 | 160,736 | 98,928 |
| genre_case | 3631/203/3631 | 3631/0/3631 | 357,437 | 202,381 | 357,437 | 202,381 |
| genre_runs_mixed | 50/147/50 | 50/0/50 | 133,342 | 66,846 | 133,342 | 66,846 |
| genre_runs_one | 4/10/4 | 3/0/3 | 98,469 | 32,821 | 98,469 | 32,821 |
| genre_runs_many | 8194/10/8194 | 8194/0/8194 | 597,824 | 303,104 | 597,824 | 303,104 |
| genre_runs_case | 2464/38/2464 | 2464/0/2464 | 314,234 | 129,594 | 314,234 | 129,594 |
| single_group_reuse_control | 4/0/4 | 3/0/3 | 65,589 | 32,821 | 65,589 | 32,821 |
| meters_empty | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 |
| meters_typical | 1/0/1 | 1/0/1 | 20 | 20 | 20 | 20 |
| meters_threshold | 1/0/1 | 1/0/1 | 64 | 64 | 64 | 64 |
| meters_medium | 1/0/1 | 1/0/1 | 128 | 64 | 128 | 64 |
| meters_duplicates | 1/0/1 | 1/0/1 | 2,048 | 64 | 2,048 | 64 |
| meters_overflow | 1/0/1 | 1/0/1 | 2,048 | 2,048 | 2,048 | 2,048 |
| meters_unique | 1/0/1 | 1/0/1 | 512 | 512 | 512 | 512 |
| meter_dense_library | 19/80/19 | 19/80/19 | 34,560 | 34,112 | 34,560 | 34,112 |
| meter_library | 43/167/43 | 43/167/43 | 639,316 | 639,316 | 639,316 | 639,316 |

## Interpretation and tradeoffs

For 4,096 mixed-genre songs, exact sizing removes 147 reallocations and reduces
requested/freed bytes from 166,110 to 99,614 (40.0%). Allocation counts remain
51 because the returned groups still own separate vectors and genre names.
Mixed length grouping removes 85 reallocations and reduces bytes from 160,736
to 98,928 (38.5%). Singleton-per-song length and genre fixtures reduce byte
churn by 50.0% and 46.7%, respectively. Every measured genre/length case has
zero output-construction reallocations with the fixture's exactly sized input.

The single-group assembly case reduces bytes from 98,469 to 32,821 (66.7%)
and uses 33.6% fewer cycles. The adapted control isolates buffer reuse alone:
65,589 to 32,821 bytes (50.0%), four to three allocations/frees, and 27.5%
fewer cycles. Whole genre sorting for one group saves only 1.4% of cycles,
since comparison/sorting still dominates that complete operation. Oversized
caller vectors may require one shrink reallocation before their storage is
retained; the dedicated regression test covers this path.

For 512 charts repeating 16 common meters, deduplication reduces allocated
bytes from 2,048 to 64 (96.9%) and thread cycles by 26.8%. A 32-chart duplicate
list halves requested bytes and saves 20.5% of cycles. The complete dense edit
library saves 14.0% of cycles, while byte churn falls only 1.3% because song
bucket/output storage dominates that operation. Nonempty standalone meter
results still allocate once; warmed internal scratch produces no heap churn.

These are workload-specific gains. The 128-chart list of unique meters uses
8.7% more cycles with unchanged allocation traffic: building the bit set and
handling overflow pays off most when meters repeat. The ordinary five-chart
case and complete ordinary meter library use about 0.7% more cycles, with
unchanged churn. Nonempty full length cases range from gains to a 2.6% cycle increase;
full case/whitespace genre sorting uses 1.5% more cycles despite 43.4% fewer
requested bytes. The tables retain these measured regressions/near-flat cases;
there is no claim of a CPU improvement for every input. Empty-input timing
differences are sub-nanosecond to a few nanoseconds and retain zero allocations.

## Behavior and validation

Four new regression tests compare exact group labels, ordering, song counts,
and Arc identity with the frozen implementations. Cases include empty input,
small and large groups, case/Unicode/whitespace genre boundaries, unknown-label
collisions, stable equal-key ties, invalid/fractional music lengths, display
BPM metadata and the uncached BPM grouping fallback. Meter checks cover the
16/17-chart boundary, empty and filtered results, edit preference, repeated
meters, reversed chart order, the 63/64 and 127/128 boundaries, and u32::MAX.
Both standalone meter lists and complete meter groups are compared.

Storage tests verify exact group/vector capacities and that single-group
results retain the original input buffer when it already fits. Oversized
inputs are trimmed. Allocation guards limit a 512-chart duplicate list to one
allocation/free and 64 bytes with no reallocation; repeated calls using warmed
scratch and an empty standalone meter result must have no allocator activity.

- `cargo test -p deadsync-simfile --locked`: 198 passed, 4 manual benchmarks ignored; doc-tests passed.
- `cargo test -p deadsync-simfile --release --locked`: 198 passed, 4 ignored; doc-tests passed.
- `cargo test -p deadsync-theme-simply-love --lib --locked -- --test-threads=1`: 1,263 passed, 4 ignored.
- `cargo check -p deadsync --all-targets --locked`: passed.
- `cargo clippy -p deadsync-simfile --all-targets --locked -- -D clippy::perf`: passed; pre-existing non-performance warnings remain.
- Targeted rustfmt checks, `git diff --check`, frozen-body verification and unchanged source hashes across all three final benchmark rounds: passed.
- All 28 old/new pairs ran three times; allocation/reallocation/free counts and byte totals were identical across the three rounds.

Reproduce the focused tests and benchmark:

```powershell
cargo test -p deadsync-simfile --lib grouping_perf --locked
cargo test -p deadsync-simfile --lib grouping_perf --release --locked
cargo test -p deadsync-simfile --release --locked song_grouping_bench -- --ignored --nocapture --test-threads=1
```

Run the benchmark three times, setting `DEADSYNC_PERF_REVERSE=1` only for the
middle invocation. Run with builds/tests idle. Timing thresholds are not test
assertions; the allocation guards are deterministic regression checks.
