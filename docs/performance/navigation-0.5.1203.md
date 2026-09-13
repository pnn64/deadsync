# Chart navigation and settings search — 0.5.1203

Baseline: `91b7bcfca` / 0.5.1202. This pass applies `M-HOTPATH`,
`M-MEM-REUSE`, `M-INITIAL-CAPACITY`, and `M-THROUGHPUT` from
`rust-performance.md` to three repeated navigation operations.

1. **Edit-chart lookup:** selecting a later edit no longer allocates and sorts
   the entire edit list. It collects indices in a `SmallVec` with room for 16
   edits on the stack, then selects the requested rank. The comparison includes
   the original chart index to preserve stable ordering for tied keys. Standard
   difficulties and the first edit keep their existing lookup paths. Oversized
   edit sets spill to the heap; an impossible rank returns early.
2. **Ghost completion:** each search result lazily retains its original-text
   prefix and shares the existing full-label allocation with actors. Repeated
   frames avoid folding, counting, collecting, and copying those strings.
   Absent completions are also retained. A rebuilt query creates fresh results;
   changing focus selects that result's own retained text. Accepting completion
   copies into the query buffer only when the user accepts it.
3. **Focused help:** each search result lazily retains joined help. A single
   untrimmed line shares the row's allocation immediately. Multiple lines join
   through 256 bytes of inline scratch before creating the final shared string;
   larger text reserves exact scratch capacity on the heap. Subsequent frames
   clone the shared handle without allocating or joining again.

Chart lookup serves song selection, courses, synchronization, and gameplay
entry. The two text helpers run while the settings search overlay renders.
These benchmarks measure those operations, not complete UI frames, game FPS,
audio latency, or total application CPU usage.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 at 2.20 GHz, Rust 1.98.0
(`88d9e12ae`), system allocator, repository release profile with opt-level 3
and LTO. The chart benchmark calls the production public API and a frozen
parent implementation in the same binary. Search benchmarks compile the exact
production helper and fuzzy-matching modules beside the old helper bodies;
state/row access is replaced by prebuilt inputs. The unchanged chart paths
serve as controls for compiler and timing variation.

Each measurement has three warmups and seven timing samples, followed by a
separate allocation-counted operation. Five complete runs alternate old/new
order. Builds and checks finish before timing. CPU cycles come from Windows
`QueryThreadCycleTime` for the calling thread; these workloads are single
threaded. Tables use medians of the five per-run medians. The
[CSV](navigation-0.5.1203.csv) records all 280 measurements, including timing
ranges, throughput, allocations, reallocations, frees, and requested bytes.

Chart fixtures mix standard difficulties, chart types, case variants, duplicate
ordering keys, and Unicode descriptions. Lookup benchmarks select later edits
from sets of 2, 8, 16, 17, and 128 matching edits. One operation is one lookup.
Search fixtures cover ASCII, decomposed accents, long Unicode text, alias-only
matches, exact matches, empty input, and short/long help. Cold operations create
and drop the retained state every time. Warm operations retain state across
frames. Each processes one completion or help result. The 60-frame workload
creates state, reads both text results 60 times, and destroys state within each
operation; throughput counts text-preparation frames. Input construction is
outside timing. Output creation, reference counting, and destruction are inside.

Allocation bytes count allocator requests, including reallocation sizes; they
measure churn, not RSS or allocator overhead. CPU cycles do not measure cache
misses or retired instructions. The benchmarks do not estimate total retained
application memory.

## Results

| Workload | Old ns/op | New ns/op | Old cycles/op | New cycles/op | Fewer cycles | Throughput old/new (M units/s) |
|---|---:|---:|---:|---:|---:|---:|
| Second edit / 2 edits | 234.6 | 199.7 | 513.9 | 437.5 | 14.9% | 4.262 / 5.006 |
| Middle edit / 8 edits | 554.1 | 467.3 | 1,213.2 | 1,022.4 | 15.7% | 1.805 / 2.140 |
| Middle edit / 16 edits | 1,116.3 | 1,019.0 | 2,443.4 | 2,232.1 | 8.6% | 0.896 / 0.981 |
| Middle edit / 17 edits | 1,263.9 | 1,032.2 | 2,769.2 | 2,260.6 | 18.4% | 0.791 / 0.969 |
| Middle edit / 128 edits | 17,299.0 | 6,989.1 | 37,863.6 | 15,297.4 | 59.6% | 0.058 / 0.143 |
| Ghost / ASCII / warm | 166.7 | 28.4 | 365.0 | 62.2 | 83.0% | 5.999 / 35.165 |
| Ghost / Unicode / warm | 191.1 | 28.5 | 418.4 | 62.5 | 85.1% | 5.233 / 35.106 |
| Ghost / long / warm | 9,044.7 | 28.7 | 19,796.5 | 63.3 | 99.7% | 0.111 / 34.843 |
| Help / single / warm | 125.1 | 14.2 | 274.2 | 31.0 | 88.7% | 7.996 / 70.611 |
| Help / multiline / warm | 153.8 | 15.2 | 337.2 | 33.3 | 90.1% | 6.503 / 65.617 |
| Help / Unicode / warm | 157.9 | 14.2 | 346.3 | 31.1 | 91.0% | 6.334 / 70.274 |
| Help / long / warm | 151.9 | 14.7 | 332.0 | 32.3 | 90.3% | 6.582 / 67.953 |
| Both text results / 60 frames | 19,920.1 | 2,782.1 | 43,627.8 | 6,085.9 | 86.1% | 3.012 / 21.566 |

| Workload | Allocations old/new | Reallocations old/new | Requested bytes old/new |
|---|---:|---:|---:|
| Second edit / 2 edits | 1 / 0 | 0 / 0 | 32 / 0 |
| Middle edit / 8 edits | 1 / 0 | 1 / 0 | 96 / 0 |
| Middle edit / 16 edits | 1 / 0 | 2 / 0 | 224 / 0 |
| Middle edit / 17 edits | 1 / 1 | 3 / 0 | 480 / 256 |
| Middle edit / 128 edits | 1 / 1 | 5 / 2 | 2,016 / 1,792 |
| Ghost / ASCII / warm | 2 / 0 | 0 / 0 | 17 / 0 |
| Ghost / Unicode / warm | 2 / 0 | 0 / 0 | 19 / 0 |
| Ghost / long / warm | 2 / 0 | 1 / 0 | 2,576 / 0 |
| Help / single / warm | 2 / 0 | 0 / 0 | 122 / 0 |
| Help / multiline / warm | 2 / 0 | 0 / 0 | 180 / 0 |
| Help / Unicode / warm | 2 / 0 | 0 / 0 | 95 / 0 |
| Help / long / warm | 2 / 0 | 0 / 0 | 1,070 / 0 |
| Both text results / 60 frames | 240 / 2 | 0 / 0 | 10,260 / 136 |

Frees equal allocations, and freed bytes equal requested bytes, in every
measured operation. Retained warm text reads have zero allocation, reallocation,
or free calls. The CSV includes each metric separately.

First-focus costs, including creating and dropping the retained state:

| Cold workload | Old ns/op | New ns/op | CPU cycles old/new | Allocations old/new | Requested bytes old/new |
|---|---:|---:|---:|---:|---:|
| Ghost / ASCII | 165.4 | 123.1 | 362.1 / 267.6 | 2 / 1 | 17 / 24 |
| Ghost / Unicode | 190.5 | 140.3 | 416.5 / 307.0 | 2 / 1 | 19 / 24 |
| Ghost / long | 9,025.7 | 8,156.3 | 19,786.3 / 17,834.3 | 2 / 1 | 2,576 / 752 |
| Help / single | 126.2 | 35.5 | 275.3 / 77.9 | 2 / 0 | 122 / 0 |
| Help / multiline | 155.1 | 164.8 | 338.1 / 360.2 | 2 / 1 | 180 / 136 |
| Help / Unicode | 158.6 | 188.4 | 347.6 / 411.9 | 2 / 1 | 95 / 48 |
| Help / long | 154.1 | 256.7 | 336.7 / 561.7 | 2 / 2 | 1,070 / 2,030 |
| Help / empty | 20.6 | 23.5 | 45.1 / 49.9 | 0 / 0 | 0 / 0 |

First-focus multiline help still costs 9.7 ns more for the ASCII fixture,
29.8 ns more for Unicode, and 102.6 ns more for the long spill fixture. Empty
help costs 2.9 ns more on first access. In these fixtures, savings on a second
read exceed that setup cost. The complete 60-frame text workload reduces CPU
cycles by 86.1%, allocations/frees from 240 to 2, and requested/freed bytes from
10,260 to 136. This includes initial preparation and final cache destruction.

The unchanged standard-chart control measured 104.4 / 101.6 ns and the
first-edit control 555.9 / 547.2 ns (old/new). Their small differences are not
claimed as optimizations. Invalid ranks, empty queries, exact matches, and
alias-only matches are included in the CSV as additional controls.

## Memory and scope

The chart scratch payload is 128 stack bytes on x86-64. It adds no retained
song data. Larger edit sets still allocate, with fewer growth operations than
the original vector. The existing workspace `smallvec` dependency is now also
used by `deadsync-chart`. All dependency versions remain unchanged, and this
pass adds no unsafe code.

The two lazy text cells add 48 inline bytes per search result on this platform.
Only focused results populate them. Prefixes and joined multiline help remain
allocated until the result list is replaced or the overlay closes. Existing
single-line help and full labels share their original allocations. The retained
strings use exact-size `Arc<str>` storage. This exchanges bounded retained text
and metadata for eliminating repeated allocation/free churn; it is not a claim
that the search overlay's resident memory decreases. Long multiline help may
need both scratch and output allocations on first focus. Short ghost prefixes
also pay an Arc header: the cold ASCII fixture requests 24 bytes instead of
17 across the old two allocations. Warm reads request zero bytes.

Other search work, including ranking, actor-vector construction, current-value
formatting, and query rendering, remains outside these targeted measurements.

## Behavior and checks

- Chart library: 32 tests passed. The new chart suite adds three tests comparing
  every rank across nine edit-set sizes, ten data seeds, five chart-type queries,
  out-of-range indices, and pointer identity under Unicode sorting ties. It
  verifies zero churn for up to 16 matching edits. Debug and release passed.
- Theme library: 1,275 tests passed; five existing manual tests ignored. The new
  state-level test exercises typing, backspace, focus changes, ghost acceptance,
  empty/no-match queries, closing, and reopening against uncached text behavior.
- Search integration suite: 22 tests passed in debug and release; one manual
  benchmark ignored. This includes existing fuzzy tests, differential Unicode
  completion cases, help trimming/joining, independent cells and clones,
  allocation identity, inline/spill boundaries, and allocation budgets.
- Scoped rustfmt and `git diff --check` passed. Performance Clippy passed with
  the existing `large_enum_variant` diagnostic allowed for the unrelated
  `SimplyLoveRuntimeRequest` enum. The theme test build retains its existing
  unused `song_lua_overlay_camera_state` warning.
- The workspace patch increases exactly once, 0.5.1202 → 0.5.1203. Cargo.lock
  records the same version for all three workspace-version packages.

Reproduce from the repository root:

```powershell
cargo test -p deadsync-chart --lib --test edit_lookup_perf
cargo test -p deadsync-theme-simply-love --lib --test search_text_perf -- --test-threads=1
cargo test --release -p deadsync-chart -p deadsync-theme-simply-love --test edit_lookup_perf --test search_text_perf -- --test-threads=1
cargo clippy -p deadsync-chart -p deadsync-theme-simply-love --lib --test edit_lookup_perf --test search_text_perf --no-deps -- -A clippy::all -D clippy::perf -A clippy::large_enum_variant
cargo test --release -p deadsync-chart -p deadsync-theme-simply-love --test edit_lookup_perf --test search_text_perf -- --ignored --nocapture --test-threads=1
```

Repeat the last command five times; set `DEADSYNC_PERF_REVERSE=1` for runs two
and four and remove it for the others. The manual tests perform no filesystem,
network, audio, or rendering work. Cycle reporting is Windows-specific; other
platforms report zero for that unavailable metric.
