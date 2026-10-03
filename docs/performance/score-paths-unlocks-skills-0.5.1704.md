# Score paths, unlock updates and event skills performance 0.5.1704

Parent: `8a2ecf968` (0.5.1703). Date: 2026-10-03.

Three optimizations remove temporary copies in score handling:

1. `ScoreProfilePaths` clones the profile root once, reserves the known suffix
   space and pushes each component into that buffer. Its original helpers
   repeatedly joined and copied the whole growing path. `PathBuf::push` still
   supplies the original separator, rooted-component and Windows prefix rules.
2. `itl_mark_unlock_folders` uses hashbrown's borrowed entry lookup. Existing
   folder keys are retained; only a missing key needs an owned string. Existing
   false flags still become true, duplicate folders retain their original
   changed/not-changed result, and callers retain their cache side effects.
3. The owned event-progress path transfers the SRPG skill vector and strings
   into its result instead of cloning them and destroying the input copies.
   The online response adapter supplies compact clones, so that normal path
   transfers every skill allocation. Direct callers with spare vector or
   string capacity retain the original cloning path, preserving the compact
   output footprint without expensive shrinking reallocations. The borrowed
   API still clones the data needed for its independent result.

## Method

Windows x86_64, Intel Xeon E5-2696 v4, 22 cores / 44 logical processors.
Rust/Cargo 1.98.1, release opt-level 3 with full LTO. The original path impl,
unlock function and five event construction functions are frozen from the
parent in test-only baseline modules. Their function bodies are unchanged
apart from test visibility and the reference path type name. Shared
formatting helpers remain unchanged. Both variants run in the same binary.

[Raw paired measurements](score-paths-unlocks-skills-0.5.1704.csv) contain
204 rows: 17 workloads, two variants and six serial rounds. Variant order
alternates by round using `DEADSYNC_BENCH_NEW_FIRST`. No compiler or other
test process runs during the benchmarks. Each measurement has three warmups
and seven timing samples, followed by a separate allocation-counted operation.

Path measurements use 1024 iterations for all eight helpers and 4096 for one
chart directory. Roots are empty, a normal Windows profile path, and a long
Unicode path. Existing unlock checks use 128 iterations; locked/missing folder
transactions use 256 fresh fixtures per timing sample. Complete event
construction uses 128 fresh inputs per timing sample. Setup and fixture
destruction are outside timing/accounting; event result/input destruction is
inside the measured operation. The event fixture includes summary text,
statistics, a quest, an achievement and leaderboard data.

`QueryThreadCycleTime` measures CPU cycles on the calling thread. Tables show
the median of six paired reductions and throughput ratios; ranges span all
six rounds. Allocation/free calls and requested/freed byte traffic include
measured destruction. Reallocations count old and new byte sizes. Requested
bytes measure allocator traffic, not resident memory. Other host activity is
not isolated. These synthetic results do not establish gameplay FPS or gains
on other platforms.

## Score paths

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Reallocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|---:|
| `root=empty/bundle` | 3061.4 -> 1269.1 | 56.0% (51.5..61.5) | 2.29x | 19 -> 8 | 13 -> 0 | 379 -> 116 |
| `root=empty/chart` | 455.1 -> 188.9 | 57.8% (54.7..61.1) | 2.40x | 3 -> 1 | 2 -> 0 | 53 -> 13 |
| `root=profile/bundle` | 4201.2 -> 2331.0 | 44.4% (39.4..70.4) | 1.80x | 19 -> 8 | 19 -> 8 | 2100 -> 628 |
| `root=profile/chart` | 577.0 -> 277.6 | 52.1% (51.2..55.8) | 2.10x | 3 -> 1 | 3 -> 1 | 339 -> 77 |
| `root=long/bundle` | 4658.9 -> 2442.9 | 48.0% (40.9..51.9) | 1.92x | 19 -> 8 | 19 -> 8 | 69726 -> 19620 |
| `root=long/chart` | 725.8 -> 326.2 | 54.3% (50.7..55.5) | 2.21x | 3 -> 1 | 3 -> 1 | 11016 -> 2451 |

Every path workload saves CPU cycles in all six rounds. A normal chart path
reduces median cycles 52.1%, fresh allocations from three to one, reallocations
from three to one and requested/freed bytes from 339 to 77. The eight-helper
bundle saves 44.4% cycles and reduces 19 allocations plus 19 reallocations to
eight of each. Long Unicode roots retain positive cycle savings in every round.

## Unlock updates

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Reallocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|---:|
| `count=0/existing` | 2.3 -> 2.3 | 0.0% (-12.4..0.0) | 1.00x | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| `count=64/existing` | 4824.2 -> 1097.7 | 77.3% (76.3..78.1) | 4.44x | 64 -> 0 | 0 -> 0 | 2550 -> 0 |
| `count=1024/existing` | 90922.6 -> 23697.7 | 73.7% (71.9..74.6) | 3.83x | 1024 -> 0 | 0 -> 0 | 41898 -> 0 |
| `count=1024/locked` | 96512.3 -> 29765.0 | 68.3% (66.9..69.0) | 3.23x | 1024 -> 0 | 0 -> 0 | 41898 -> 0 |
| `count=1024/missing` | 104678.6 -> 95455.9 | 7.8% (4.0..13.1) | 1.08x | 1034 -> 1034 | 0 -> 0 | 177094 -> 177094 |

Existing-folder updates remove every temporary string allocation/free: 64
folders save 77.3% median cycles, and 1024 save 73.7% while removing 1024
allocation/free pairs and 41898 requested/freed bytes. Updating 1024 existing
false flags saves 68.3% cycles with the same zero-allocation behavior. The
all-missing control retains identical allocation traffic and saves 7.8% cycles
(positive in every round). The empty control is roughly two nanoseconds and
shows no meaningful timing change.

## Complete event progress

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Reallocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|---:|
| `count=0/long=false/spare=false` | 2946.9 -> 2805.8 | 1.3% (-3.3..5.9) | 1.02x | 9 -> 9 | 3 -> 3 | 950 -> 950 |
| `count=1/long=false/spare=false` | 2571.5 -> 2426.9 | 3.4% (-6.1..8.7) | 1.04x | 11 -> 9 | 3 -> 3 | 996 -> 950 |
| `count=8/long=false/spare=false` | 3566.4 -> 2906.7 | 16.5% (11.9..23.5) | 1.24x | 18 -> 9 | 4 -> 4 | 1630 -> 1262 |
| `count=64/long=false/spare=false` | 9048.4 -> 5394.5 | 38.8% (33.1..42.7) | 1.70x | 74 -> 9 | 7 -> 7 | 8628 -> 5630 |
| `count=64/long=true/spare=false` | 130356.2 -> 63021.9 | 52.8% (22.7..73.0) | 2.13x | 74 -> 9 | 10 -> 10 | 225916 -> 153350 |
| `count=8/long=false/spare=true` | 3339.8 -> 3321.5 | -1.6% (-8.0..22.6) | 0.98x | 18 -> 18 | 4 -> 4 | 1630 -> 1630 |

Eight compact skill strings save 16.5% median cycles, improve throughput 1.24x
and remove nine allocation/free pairs, reducing complete-operation allocations
from 18 to nine. At 64 strings, the corresponding counts fall from 74 to nine;
short text saves 38.8% cycles and long text saves 52.8%. All populated cases
with at least eight compact strings improve cycles in every round. One string
removes two allocations but has no clear timing change. Empty and spare-capacity
controls keep identical allocation traffic and their cycle ranges cross zero.
The spare-capacity control has a -1.6% paired cycle median and 0.98x throughput.
All produced skill buffers retain the original compact capacity; the optimized
path does not keep oversized input buffers alive in the result.

No behavioral regression or increased allocation traffic is found. The tables
include empty, all-missing, long-path and oversized-buffer controls; small
timing differences in the displayed noisy controls are not claimed as gains.

## Behavioral validation

- The full debug score/online suite passes 570 tests when excluding the
  single verified pre-existing allowlist failure. All three new regression
  tests pass in debug and release; the unfiltered release score library has
  273 passes and only that known failure.
- Unfiltered score debug/release tests retain the known
  `tests::lua_submit_allowlist_requires_known_hash` failure: the unchanged
  allowlist accepts `f95bc209c6f2cbfe`, while its test expects rejection.
  The existing isolated parent-source test reproduces the same assertion;
  its constant, policy functions and test are unchanged by either this pass
  or the parent encoding pass.
- Path tests compare exact `OsStr` values, covering empty/relative roots,
  slash/backslash endings, drive-relative roots, UNC/verbatim prefixes, long
  Unicode roots, non-UTF-8 Windows surrogate input and rooted/Unicode shards.
  Existing score index and file-writing tests pass.
- Unlock tests compare maps and change flags against the original for existing
  true/false keys, new keys, duplicates, Unicode, blank and trimmed folders.
  Repeated updates assert zero allocation churn; existing runtime write/skip
  tests preserve persistence and invalidation behavior.
- Event tests compare every derived-debug field plus leaderboard float bits
  against the original, including both event types, score-added/updated paths,
  page text/order, reward pages, missing progress and ITL eligibility. Borrowed
  inputs remain unchanged. Compact buffer pointers prove ownership transfer;
  output-capacity checks cover exact, oversized and empty buffers.
- Architecture checks: 141 pass and the exact same 12 existing failures remain.
  Their baseline reproduction is documented in
  [the previous baseline report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Score/online Clippy for all targets completes with existing warnings;
  no warning points to the new modules. Targeted rustfmt and git diff --check
  pass.
- Cargo.toml and Cargo.lock change exactly 0.5.1703 -> 0.5.1704. The lockfile
  changes only the three packages inheriting the workspace version.

## Reproduce

```powershell
cargo test -p deadsync-score -p deadsync-online -- --skip tests::lua_submit_allowlist_requires_known_hash
# This unfiltered run includes the documented existing allowlist failure.
cargo test --release -p deadsync-score --lib
cargo clippy -p deadsync-score -p deadsync-online --all-targets
cargo test --release -p deadsync-score --lib local_store::score_paths_perf::score_paths_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-score --lib itl::unlocks_perf::unlock_updates_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-score --lib event_progress::skills_perf::event_skills_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run benchmarks serially after builds/tests finish. Repeat six rounds, setting
`$env:DEADSYNC_BENCH_NEW_FIRST = '1'` for even rounds and removing it for odd
rounds. Each command measures its frozen original and current implementation.
