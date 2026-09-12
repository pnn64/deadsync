# Playlist preparation performance - 0.5.1149

This pass follows `M-HOTPATH`, `M-MEM-REUSE`, `M-INITIAL-CAPACITY`, and
`M-THROUGHPUT` from the supplied `rust-performance.md`. Playlist preparation
runs while building the song-selection library. These measurements cover CPU
selection/index work, not disk I/O, frame rate, UI latency, RSS, or peak heap.

## Three changes

1. **Reuse normalization storage while building playlist indexes.** Group,
   folder, and song keys use three local scratch strings. Existing groups are
   found with borrowed keys; new owned strings are stored only for new keys.
   Equal group/folder aliases skip the redundant song insertion, while duplicate
   song keys retain their first winner. The full-path map reserves its source
   bound only when a lobby path is actually present. No-path sources need no
   full-path hash table.
2. **Build the music wheel from section batches.** A new section visitor
   borrows header names and transfers a drain of the resolved song handles to
   its consumer. The section buffer is reused. The existing Vec API uses the
   visitor; single-section wheel construction consumes batches directly,
   removing the full intermediate playlist-entry vector and the temporary
   owned header string. Multi-section text retains whole-playlist sizing to
   avoid growing the larger wheel vector. A conservative marker check may
   select the buffered path for song names containing `---`; semantics remain
   identical. Unused headers allocate nothing.
3. **Skip index preparation when there are no playlists.** The music-selection
   library builder returns an empty vector before gathering song sources or
   indexing the library. A nonempty playlist list, including empty playlist
   files, follows the existing menu-building and sorting behavior.

Changes use safe Rust and introduce no dependencies, benchmark-only public API,
long-lived scratch cache, or allocator changes. Nonempty output still needs
owned headers, song handles, and vectors; hash tables/group vectors may grow.
Scratch storage lives only for one index build. The UI playlist preparation
helpers moved into a private module so integration tests compile the production
implementation directly, using the real UI entry/view types.

## Method and workloads

Baseline: `9541f9eeb` / 0.5.1148. Twelve frozen simfile function bodies and six
frozen UI helper bodies match that commit ignoring whitespace. Imports and test
visibility differ; the baseline UI's lookup/parser calls are rebound to the
frozen simfile routines. Source and output data types are shared. The new UI
module is compiled unchanged in the test binary and used by the actual screen.

Windows x64, Intel Xeon E5-2696 v4 at 2.20 GHz, rustc 1.98.0 (`88d9e12ae`,
LLVM 22.1.8); repository release profile with opt-level 3 and full LTO. Both
versions run in one executable through opaque function pointers, with input
and output black boxes and the shared System allocator wrapper.

Each workload warms three times, then times seven batches with allocation
counters disabled. A separate operation records allocations/reallocations/frees
and all requested/freed bytes. Windows `QueryThreadCycleTime` measures this
thread's CPU cycles; the routines launch no workers. Tables are medians of
three invocation medians in old/new, new/old, old/new order, without concurrent
builds or tests. Small timing differences can be noise. Output disposal is
included; fixture generation is outside the measurement.

- Index fixtures: zero or one song; 4,096 songs in 32 contiguous groups with
  matching folder/display names, distinct aliases, or absent lobby paths;
  and 1,024 songs in 1,024 groups. The consuming API gets cloned sources during
  each timed operation, so their string/Arc cloning and disposal are included
  for both versions. Index/group/key ownership and disposal are also included.
- Parsing uses the 4,096-song lookup built outside timing: empty text; 4,096
  unused headers; 4,096 literal paths; one section per song; 32 group wildcards
  expanding to 4,096 songs; and 4,096 missing paths. Output creation/disposal
  is included. The parser scratch string starts cold for each call.
- Wheel-only cases use the same prebuilt lookup and cover literal paths, many
  sections and wildcards, including output creation/conversion/disposal.
- Complete preparation uses the grouped wheel (32 headers and 4,096 songs),
  zero playlists or one playlist containing every literal path. Source
  gathering, index creation/disposal, parser output, wheel conversion, menu
  creation, sorting, and final library disposal are included.
- `empty_guard_control` is explicitly adapted: it forces the optimized index
  builder to run and drop its result before returning empty. This isolates the
  early return from the other two changes. `library_none` uses the full frozen
  baseline instead.

Large index and complete-library batches use four operations; empty/single index
batches use 1,024, parsing/wheel conversion uses 16, and the fast empty guard uses 1,024. Empty
complete-library output uses 4,096 operations for the new routine. Throughput
counts input songs for indexes, input lines for parsing (a wildcard is one line),
and completed calls for library preparation. Empty input has zero useful units.
Allocation bytes are allocator requests/churn, not resident or peak memory.

## Timing and throughput

Positive cycle savings indicate improvement. Component and complete-library
gains overlap and should not be added together.

| Workload | Old ns/op | New ns/op | Old cycles/op | New cycles/op | Cycles saved | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|---:|
| index_empty | 31.1 | 28.2 | 69.7 | 63.2 | 9.3% | 0.0 | 0.0 |
| index_single | 1,555.3 | 1,508.1 | 3,410.4 | 3,306.2 | 3.1% | 642,973.8 | 663,083.6 |
| index_grouped | 5,607,825.0 | 4,218,600.0 | 12,267,799.8 | 9,244,407.0 | 24.6% | 730,408.0 | 970,938.2 |
| index_aliases | 5,671,675.0 | 4,913,800.0 | 12,429,077.2 | 10,749,683.5 | 13.5% | 722,185.2 | 833,570.8 |
| index_no_lobby | 4,059,975.0 | 2,784,600.0 | 8,898,365.5 | 6,101,222.2 | 31.4% | 1,008,873.2 | 1,470,947.4 |
| index_many_groups | 2,035,400.0 | 1,777,825.0 | 4,462,325.2 | 3,898,046.0 | 12.6% | 503,095.2 | 575,984.7 |
| parse_empty | 31.2 | 31.2 | 137.2 | 137.2 | 0.0% | 0.0 | 0.0 |
| parse_unused_headers | 369,018.8 | 114,618.8 | 809,392.5 | 251,450.9 | 68.9% | 11,099,707.0 | 35,735,863.5 |
| parse_literal | 583,543.8 | 603,662.5 | 1,277,983.8 | 1,323,612.5 | -3.6% | 7,019,182.4 | 6,785,248.4 |
| parse_sections | 1,392,168.8 | 1,078,143.8 | 3,050,364.1 | 2,361,682.7 | 22.6% | 5,884,344.1 | 7,598,244.7 |
| parse_wildcards | 157,750.0 | 149,412.5 | 346,110.3 | 327,795.9 | 5.3% | 202,852.6 | 214,172.2 |
| parse_misses | 481,431.2 | 472,731.2 | 1,055,863.6 | 1,036,753.4 | 1.8% | 8,507,964.5 | 8,664,542.5 |
| wheel_literal | 657,393.8 | 665,818.8 | 1,440,029.8 | 1,455,202.7 | -1.1% | 6,230,664.7 | 6,151,824.4 |
| wheel_sections | 1,944,162.5 | 1,656,268.8 | 4,247,366.1 | 3,624,877.8 | 14.7% | 4,213,639.5 | 4,946,057.2 |
| wheel_wildcards | 175,562.5 | 160,775.0 | 384,948.1 | 343,846.7 | 10.7% | 182,271.3 | 199,035.9 |
| library_none | 7,264,925.0 | 18.4 | 15,911,609.5 | 40.7 | >99.99% | 137.6 | 54,395,750.3 |
| library_populated | 8,164,675.0 | 6,975,575.0 | 17,864,995.5 | 15,141,165.0 | 15.2% | 122.5 | 143.4 |
| empty_guard_control | 5,731,750.0 | 22.4 | 12,551,284.0 | 50.6 | >99.99% | 174.5 | 44,716,157.2 |

## Allocation churn

Counts include result disposal. Bytes mean allocator requests, not live heap or
peak memory. `A/R/F` means allocations/reallocations/frees per operation.

| Workload | Old A/R/F | New A/R/F | Old requested bytes | New requested bytes | Old freed bytes | New freed bytes |
|---|---:|---:|---:|---:|---:|---:|
| index_empty | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 |
| index_single | 14/0/14 | 14/0/14 | 841 | 850 | 841 | 850 |
| index_grouped | 37131/160/37131 | 16718/160/16718 | 1,186,096 | 1,061,642 | 1,186,096 | 1,061,642 |
| index_aliases | 37389/320/37389 | 21136/321/21136 | 1,841,104 | 1,724,304 | 1,841,104 | 1,724,304 |
| index_no_lobby | 28939/160/28939 | 8525/160/8525 | 1,067,996 | 673,190 | 1,067,996 | 673,190 |
| index_many_groups | 11285/0/11285 | 8216/0/8216 | 765,942 | 744,720 | 765,942 | 744,720 |
| parse_empty | 0/0/0 | 0/0/0 | 0 | 0 | 0 | 0 |
| parse_unused_headers | 4096/0/4096 | 0/0/0 | 53,248 | 0 | 53,248 | 0 |
| parse_literal | 4/12/4 | 4/11/4 | 196,777 | 196,649 | 196,777 | 196,649 |
| parse_sections | 8195/12/8195 | 4099/12/4099 | 620,309 | 572,267 | 620,309 | 572,267 |
| parse_wildcards | 4/6/4 | 4/5/4 | 195,760 | 195,632 | 195,760 | 195,632 |
| parse_misses | 1/0/1 | 1/0/1 | 15 | 15 | 15 | 15 |
| wheel_literal | 6/12/6 | 4/11/4 | 524,561 | 393,321 | 524,561 | 393,321 |
| wheel_sections | 12292/12/12292 | 8196/12/8196 | 1,406,741 | 1,358,699 | 1,406,741 | 1,358,699 |
| wheel_wildcards | 6/6/6 | 4/5/4 | 523,544 | 392,304 | 523,544 | 392,304 |
| library_none | 37164/4266/37164 | 0/0/0 | 1,684,060 | 0 | 1,684,060 | 0 |
| library_populated | 37175/4278/37175 | 16760/4277/16760 | 2,536,508 | 2,280,817 | 2,536,508 | 2,280,817 |
| empty_guard_control | 16751/4266/16751 | 0/0/0 | 1,559,606 | 0 | 1,559,606 | 0 |

## Interpretation and limits

Grouped index construction saves 24.6% of thread cycles, reducing allocation/free
calls from 37,131 to 16,718. The no-lobby case saves 31.4% of cycles and 37.0%
of requested bytes. Aliased groups save 13.5% of cycles; scratch-string growth
adds one reallocation, while removing 16,253 allocation/free pairs. Single-song
cold setup requests nine additional bytes (850 versus 841), with unchanged
allocation count. These are local scratch-capacity costs, not retained caches.

Single-section wheel construction eliminates the intermediate playlist vector
and header string: requested/freed bytes fall by about 25%, from 524,561 to
393,321 for literal paths, and 523,544 to 392,304 for wildcards. Wildcard wheel
preparation saves 10.7% of cycles; multi-section wheel preparation saves 14.7%
of cycles and 4,096 allocation/free pairs through borrowed section names.

The literal Vec-returning API uses 3.6% more cycles in the aggregate; the literal
wheel case uses 1.1% more, with timing ranges overlapping across samples/runs.
Treat those as measured small regressions/near-flat controls, not demonstrated
CPU improvements. The final implementation keeps bulk section transfer; the
rejected direct-append and per-song-iterator drafts had larger memory or CPU
regressions. Empty-input and missing-path controls show no allocation regression.

Complete populated-library preparation saves 15.2% of cycles, 55.0% of allocation
calls, and 10.1% of requested/freed bytes. The empty-library case removes 37,164
allocations, 4,266 reallocations and 1,684,060 requested/freed bytes per call,
leaving zero heap churn. It still executes a small branch/return; CPU savings
round above 99.99%, not to literal zero work. The adapted guard-only control
confirms the short circuit independently of index-key/parser improvements.

## Behavior and validation

Six new tests compare exact header text, song counts, song Arc identity and
order, menu labels/IDs, stable menu sorting, original header indexes, and
optional wheel fields. They cover group/folder aliases, duplicate path and
song-directory precedence, case handling, Unicode paths, whitespace-only
names, missing lobby paths, rootless paths, Windows non-UTF-8 directories,
wildcards, repeated songs, empty/trailing sections, and every UTF-8 truncation
of a mixed syntax corpus. Complete library comparison includes empty input,
empty playlist files, equal sort keys and duplicate IDs. Partial section-drain
consumption checks exact prefixes and releases all unconsumed song handles.

Allocation guards require no allocator activity for an empty playlist library
and for 1,024 unused headers. A 1,024-song wildcard section requires four
allocations/frees and no reallocation: one section buffer, one output vector,
one owned header, and one normalization buffer.

- `cargo test -p deadsync-simfile --locked`: 194 passed, 3 manual benchmarks ignored; doc-tests passed.
- `cargo test -p deadsync-theme-simply-love --lib --locked -- --test-threads=1`: 1,263 passed, 4 ignored.
- Focused playlist integration tests: 6 passed in debug and release, 1 manual benchmark ignored.
- `cargo check -p deadsync --all-targets --locked`: passed.
- Simfile Clippy performance lints: passed with `-D clippy::perf`.
- Theme/library and playlist-test performance lints: passed with `-D clippy::perf -A clippy::large_enum_variant`. The exception is for the pre-existing large `SimplyLoveRuntimeRequest` enum in `effects.rs`, verified unchanged from the baseline; ordinary style warnings remain.
- Targeted rustfmt checks, `git diff --check`, frozen-baseline verification, and unchanged source hashes across the final three benchmark rounds: passed.

Reproduce the focused tests and benchmark:

```powershell
cargo test -p deadsync-theme-simply-love --test playlist_preparation --locked
cargo test -p deadsync-theme-simply-love --test playlist_preparation --release --locked
cargo test -p deadsync-theme-simply-love --test playlist_preparation --release --locked playlist_preparation_bench -- --ignored --nocapture --test-threads=1
```

Repeat the benchmark three times, setting `DEADSYNC_PERF_REVERSE=1` only for the
middle run. Run without concurrent builds/tests. Timing thresholds are not test
assertions; the allocation guards are deterministic regression checks.
