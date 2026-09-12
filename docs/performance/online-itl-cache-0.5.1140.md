# Online ITL cache performance, 0.5.1140

Baseline: `3494eee4b` / 0.5.1139. The supplied Rust guide's M-HOTPATH,
M-MEM-REUSE and M-INITIAL-CAPACITY guidance applies here: avoid short-lived
key copies, retain existing owned keys, and size temporary results for the
data needed by the operation.

## Three optimizations

1. **Borrow keys when updating online scores/ranks.** Existing ITL/SRPG cache
   entries are updated in place. Missing-key deletes and repeated values create
   no owned keys. New entries allocate their two strings once. A changed loaded
   profile still returns the same independent persistence snapshot.
2. **Borrow IDs when probing the overall-rank cache.** Hits compare API/profile
   slices and both generation counters without creating owned strings. Only a
   replacement entry constructs an owned key. The public owned-key query still
   works, and cached results retain the same Arc identity and side mapping.
3. **Sort borrowed hashes by points for overall ranks.** Single/double scratch
   entries hold string slices instead of owned strings. Equal points share a
   rank, so comparing chart hashes inside a tie is unnecessary. Only distinct
   output hashes are cloned. Later lower ranks for a duplicate hash and the
   double-style overwrite retain their original behavior. Output-table capacity
   is bounded by the number of scored hashes, even when charts repeat.

The update routine serves ITL scores, ITL ranks and SRPG scores. Overall-rank
queries reuse the cache until score or library generations change; misses rebuild
ranks from the song cache. These changes do not alter serialized data or public APIs.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4, Rust 1.98.0 (`88d9e12ae`, LLVM 22.1.8),
`x86_64-pc-windows-msvc`, release with full LTO. Production allocator and
dependencies are unchanged; tests use the existing counted System allocator.

Frozen baseline functions run alongside production functions in the same test
binary. Fixture setup is outside timing. Each measurement has three warm-ups
and seven timing batches. Three invocations alternate old/new order, reversing
the middle invocation. Values below are medians of invocation medians. CPU
cycles come from Windows QueryThreadCycleTime; allocation counting is separate
from timing. Allocation bytes mean requested traffic, not peak RSS.

Cache update batches contain 64 operations, except changed-profile measurements,
which contain one update and include creation/destruction of the 256-entry
persistence snapshot. Insert batches include creating, growing and destroying a
fresh session cache. Other update batches reuse existing caches. Cache-hit measurements
include the runtime wrapper, locks, loaded-profile checks, key comparison, Arc
cloning and result drop. Their invalidation control changes the library generation
every operation, using an empty library. Ranking measurements include complete
result construction and destruction. Chart fixture hashes are 16 bytes and update
API keys are six bytes; cache-query IDs vary as described below.

## Results

Cache updates (M updates/s; times/cycles per batch):

| Case | us/op old -> new | Thread cycles/op old -> new | M units/s old -> new |
| --- | ---: | ---: | ---: |
| cache_unchanged | 23.857 -> 7.350 | 52,330 -> 16,125 | 2.683 -> 8.707 |
| cache_session_change | 19.202 -> 3.239 | 41,912 -> 7,114 | 3.333 -> 19.761 |
| cache_missing_delete | 12.840 -> 4.805 | 28,174 -> 10,551 | 4.984 -> 13.320 |
| cache_profile_change | 33.419 -> 34.172 | 73,019 -> 74,928 | 0.030 -> 0.029 |
| cache_insert | 26.244 -> 15.000 | 57,520 -> 32,904 | 2.439 -> 4.267 |

Runtime rank-cache queries (M queries/s; one query per operation):

| Case | us/op old -> new | Thread cycles/op old -> new | M units/s old -> new |
| --- | ---: | ---: | ---: |
| rank_hit_profile | 0.213 -> 0.096 | 468 -> 212 | 4.696 -> 10.412 |
| rank_hit_session | 0.099 -> 0.042 | 217 -> 94 | 10.154 -> 23.594 |
| rank_hit_long_ids | 0.296 -> 0.111 | 638 -> 243 | 3.380 -> 9.050 |
| rank_hit_invalidating | 0.405 -> 0.387 | 864 -> 850 | 2.467 -> 2.583 |
| rank_hit_not_joined | 0.017 -> 0.026 | 38 -> 58 | 59.535 -> 38.715 |

Complete overall-rank rebuilds (M input charts/s):

| Case | us/op old -> new | Thread cycles/op old -> new | M units/s old -> new |
| --- | ---: | ---: | ---: |
| overall_small | 7.602 -> 7.182 | 16,323 -> 15,410 | 4.209 -> 4.456 |
| overall_medium | 553.913 -> 546.128 | 1,213,979 -> 1,194,690 | 3.697 -> 3.750 |
| overall_large | 3,187.725 -> 2,541.238 | 6,974,208 -> 5,558,872 | 2.570 -> 3.224 |
| overall_ties | 485.978 -> 511.812 | 1,060,446 -> 1,121,741 | 4.214 -> 4.001 |
| overall_duplicates | 527.056 -> 362.812 | 1,149,563 -> 792,683 | 3.886 -> 5.645 |
| overall_filtered | 284.525 -> 285.934 | 621,555 -> 625,431 | 7.198 -> 7.162 |

The rank-cache profile query uses 13-byte API and 17-byte profile IDs;
the long-ID case uses 94 bytes total including multibyte UTF-8 characters.
Rank fixtures contain 32/2,048/8,192 charts for small/medium/large; the other
rank fixtures contain 2,048. Mixed fixtures vary points and step types. Ties
give all entries the same points, with hashes already ordered within each
style. Duplicates repeat each of 512 hashes four times, including across styles.
Filtered inputs mix charts without note data, missing scores and invalid point
names. All fixtures are synthetic; library/score setup is outside timing.

| Case | Allocations + reallocations old -> new | Requested bytes/op old -> new |
| --- | ---: | ---: |
| cache_unchanged | 256 + 0 -> 0 + 0 | 2,816 -> 0 |
| cache_session_change | 256 + 0 -> 0 + 0 | 2,816 -> 0 |
| cache_missing_delete | 128 + 0 -> 0 + 0 | 832 -> 0 |
| cache_profile_change | 518 + 0 -> 514 + 0 | 34,883 -> 34,839 |
| cache_insert | 262 + 0 -> 134 + 0 | 17,276 -> 15,868 |
| rank_hit_profile | 2 + 0 -> 0 + 0 | 30 -> 0 |
| rank_hit_session | 1 + 0 -> 0 + 0 | 13 -> 0 |
| rank_hit_long_ids | 2 + 0 -> 0 + 0 | 94 -> 0 |
| rank_hit_invalidating | 3 + 0 -> 3 + 0 | 78 -> 78 |
| rank_hit_not_joined | 0 + 0 -> 0 + 0 | 0 -> 0 |
| overall_small | 35 + 4 -> 35 + 4 | 4,432 -> 3,984 |
| overall_medium | 2051 + 16 -> 2051 + 16 | 298,768 -> 266,064 |
| overall_large | 8195 + 20 -> 8195 + 20 | 1,195,792 -> 1,064,784 |
| overall_ties | 2051 + 16 -> 2051 + 16 | 298,768 -> 266,064 |
| overall_duplicates | 2051 + 16 -> 515 + 16 | 298,768 -> 140,112 |
| overall_filtered | 939 + 14 -> 939 + 14 | 147,856 -> 131,536 |

Free counts match allocation counts; freed-byte traffic matches requested
traffic in every measured operation. Reallocation traffic is included in bytes.
Unchanged updates, session-only changes, missing deletes and rank-cache hits
have zero heap churn. New insertions and changed-profile persistence snapshots
still own data and therefore still allocate.

Borrowed ranking scratch entries are 24 bytes on this target versus 32 bytes
previously. The duplicate fixture reduces output-table capacity from 3,584 to
896 entries, while preserving all 512 output ranks and the contribution of
every duplicate to competition rank. Duplicate key copies fall from 2,048 to
512. Buffer growth remains in cold ranking construction; the report does not
claim zero allocations or a measured peak-RSS reduction for that operation.

Unchanged cache updates save 69% of cycles and session-only
changes save 83%. Profile/session rank-cache hits save
55%/57%. Duplicate-heavy rank rebuilding saves
31% of cycles, 53% of requested bytes and about 75%
of allocation calls. These are operation-level gains, not measurements of
whole-game frame rate or network/persistence latency.

The large mixed fixture uses about 20% fewer cycles and 11% fewer requested
bytes. Medium and filtered rebuilding are approximately flat in CPU cost.
The already-ordered all-ties fixture costs about 6% more cycles (26 us), while
using 11% fewer requested bytes. The extra duplicate-key probe trades some
CPU work on unique hashes for eliminating duplicate allocations. Changed-profile
updates cost about 3% more cycles (0.75 us); snapshot cloning remains dominant.
The no-player control measured 26 ns versus 17 ns, with zero allocations in both
versions. Cache invalidation is approximately flat and still allocates an owned
entry. The retained changes target cache reuse and larger/duplicate-heavy rank
workloads; they do not make every control faster. Small differences and exact
percentages vary across batches and invocations. Paired invocation cycle savings
were 66-73% for unchanged updates, 40-55% for profile cache hits, 3-21% for
large mixed rebuilding and 28-34% for duplicate-heavy rebuilding.

## Validation

- Baseline score suite: 228 unit tests and three integration tests passed.
- Five new regression/allocation tests compare frozen reference state transitions
  and full snapshots, including repeated and missing values, trimming, Unicode,
  loaded/unloaded profiles, deletion and u32::MAX values.
- Rank-cache checks preserve Arc identity, both player slots/default-side mapping,
  exact API/profile matching, None versus an empty profile ID, and independent
  score/library generation invalidation, including u64::MAX. Cached probes must
  remain allocation-free; the benchmark also measures full runtime cache queries.
- Overall ranks match the frozen implementation for empty/small/large inputs,
  tied points, mixed styles, filtered charts, missing scores and repeated hashes
  within/across styles. Allocation budgets check distinct-key cloning and bounded
  output-table capacity.
- Final score tests: 233 passed in debug and release; three manual benchmarks
  ignored. Three integration tests passed; no failures.
- Profile consumers: 215 deadsync-profile and 17 deadsync-profile-gameplay tests
  passed with no failures.
- All-target Clippy with `-D clippy::perf` passed. Existing unrelated warnings
  remain; none are in the changed routines or the new benchmark file.
- `cargo check --workspace --bins`, targeted Rustfmt and `git diff --check` passed.
- Five reference function bodies were verified against `3494eee4b`, allowing only
  renamed reference calls and formatting changes.
- Cargo.toml and Cargo.lock bump the patch exactly once: 0.5.1139 -> 0.5.1140.

## Reproduction

```powershell
cargo test -p deadsync-score --all-targets -- --test-threads=1
cargo test --release -p deadsync-score --lib -- --test-threads=1
cargo test -p deadsync-profile -p deadsync-profile-gameplay --lib -- --test-threads=1
cargo clippy -p deadsync-score --all-targets -- -D clippy::perf
cargo check --workspace --bins
cargo test --release -p deadsync-score --lib online_itl_bench -- --ignored --nocapture --test-threads=1
```

Run the manual benchmark three times, with `DEADSYNC_PERF_REVERSE=1` only on the
middle run. Avoid other builds/tests during timing. Raw benchmark outputs,
comparison script, JSON metrics and validation logs remain locally in the
ignored `target/online-itl-perf/` directory.
