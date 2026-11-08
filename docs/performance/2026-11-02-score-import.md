# Score import optimizations (2026-11-02)

Three measured changes in `deadsync-score`, based on `3fe56b047`:

1. Pack collection uses the existing `FxBuildHasher` for its internal filter and
   deduplication sets. These sets are queried for membership; output still follows
   catalog order. The caller's existing-score set and public signatures are unchanged.
2. `grade_from_itg` replaces 17 separate tier-string matches with two checked byte
   patterns and the existing `grade_from_code` mapping. Prefix stripping, Unicode
   trimming, case sensitivity, `Failed`, and invalid-input results are preserved.
3. Pack progress/completion messages append optional failure counts directly into
   their output buffer. This removes the temporary suffix `String` and its
   allocation while preserving the complete output, including punctuation.

The production diff removes 14 net lines (27 added / 41 removed). It adds no unsafe
code, dependencies, generalized wrappers, or persistent state.

## Measurements

Seven final runs compare frozen originals with actual production functions in the
same executable, alternating variants on one pinned thread. Cycle columns are
medians across runs; throughput is the median of each run's paired ratio, so it
need not equal the quotient of the two independent medians. Allocations cover one
whole operation; fixture construction is excluded.

| Workload | Before cycles/op | After cycles/op | Paired CPU throughput | Allocations/op |
| --- | ---: | ---: | ---: | ---: |
| Pack collection, 3,072 charts | 582,662.47 | 479,317.25 | 1.249x | 1410 -> 1410 |
| Imported grades, 128 tier strings | 3,013.65 | 2,179.73 | 1.380x | 0 -> 0 |
| Completion messages, 128 packs with failures | 79,976.84 | 63,553.88 | 1.268x | 256 -> 128 |

Pack collection improves by 1.245?1.273x across all-pack, single-pack and
multiple-filter workloads; its allocation counts are unchanged. Grade parsing
improves by 1.240?1.380x across bare tiers, prefixed tiers and invalid/failed
grades, with zero allocations.

All six message workloads improve in the final paired runs (1.064?1.268x).
Failure messages remove 128 allocations per batch. Completion messages with
failures also reduce allocated bytes from 14,080 to 11,264 and peak live bytes
from 110 to 88. Normal messages retain 128 allocations per batch. Progress and
completion messages, short/long Unicode names, and zero/nonzero failure counts
are measured separately.

These are CPU and allocation measurements of these functions; they do not
measure total import latency or gameplay frame rate. Absolute cycles varied
substantially under host load, so conclusions use paired comparisons within each
run rather than comparing absolute timings across different builds.

## Stages and discarded changes

The consolidated [raw measurements](2026-11-02-score-import.tsv) and
[source/compiler/binary manifests](2026-11-02-score-import.manifest.json) record:

- `before` -> `hash`: the internal hasher change.
- `rows` -> `grades`: the grade-decoding change; grade inputs and the original
  grade function were unchanged through the intervening row experiments.
- `before-messages` -> `messages`: the message-buffer change.
- `verified` and `verified-messages`: seven final runs of the retained changes.

Single-note shortcuts, cached first-note lanes, and branchless early counters
were discarded. Initial full-row results suggested improvements, but some row
shapes regressed and the isolated counter check was slower: paired throughput
0.946x for mixed timing signs, 0.763x for all early hits, and 0.623x for all late
hits. This exposed a misleading full-row timing result. No production column
judgment changes remain. Their experimental records are retained in the raw
data; early row workloads used grade-correlated timing signs, which were
decoupled before the counter experiment.

Archived executables, source snapshots, correctness output and run logs remain
locally under `target/score-import-kernels/<stage>`.

## Method and reproduction

Windows; Intel64 Family 6 Model 154 Stepping 4; rustc 1.97.1, LLVM 22.1.6,
x86_64-pc-windows-msvc. Existing Cargo release settings remain opt-level 3, LTO
and static MSVC runtime. The shared benchmark helper pins logical CPU 2 and
reads `QueryThreadCycleTime`; it measures 12 rounds, discards warmup, and uses
the median of the remaining 11. Inputs, outputs and selected function pointers
use `black_box`. Raw wall-clock nanoseconds are retained as a secondary metric.

```text
python scripts/benchmark_score_import.py reproduce --runs 7
python scripts/benchmark_score_import.py reproduce-messages --messages --runs 7
```

The script builds the real integration-test executable, runs correctness tests,
archives source/binary hashes and compiler details, then records paired cycles,
nanoseconds and scoped System-allocator counts. It writes per-stage TSV and
manifest files; the recorded session above consolidates those files.

## Correctness and checks

Differential tests retain the original production functions. They cover catalog
filters, duplicate/case-sensitive hashes, aliases, trimming, output ordering and
existing scores; all 65,536 pairs of Latin-1 suffix characters with bare/prefixed
tier names; and complete message strings with Unicode/control characters,
punctuation, zero/maximum counts and optional failure suffixes.

Both final debug and release suites pass: **274 tests each**, with ten benchmark
tests ignored by the default runner. The new paired benchmarks were run
separately for the measurements above.

```text
cargo test -p deadsync-score --locked
cargo test -p deadsync-score --release --locked
cargo clippy -p deadsync-score --all-targets --locked --no-deps
```

Clippy completes with existing diagnostics and none in the new test files.
New Rust files and changed production functions pass rustfmt checks, and
`git diff --check` passes. The pre-existing dirty fixtures submodule is unchanged.
