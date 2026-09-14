# UI and history preparation performance: 0.5.1222

Baseline: `c065be34c` / 0.5.1221. This pass follows `rust-performance.md`:
measure CPU/allocation work (M-HOTPATH), reserve known capacities
(M-INITIAL-CAPACITY), and eliminate repeated work (M-THROUGHPUT).

## Three changes

1. **Scorebox pane preparation.** A new borrowed-reference API stores up to
   eight source panes in a `SmallVec` inline buffer. Larger inputs use the
   existing preallocated `Vec` collector and transfer its allocation to the
   result. The existing Vec-returning API remains available. All three scorebox
   view/pane builders use the new API. Primary-pane selection classifies each
   pane once, remembering fallbacks, instead of making up to three passes.
   Filter order, duplicate identity, first-match preference and fallbacks are
   unchanged. The remaining actor/row/output allocations are outside this change.
2. **Local history traversal.** Recent plays, play counts, combined history,
   and total score counts now enumerate the root once and each immediate shard
   once. Regular files need only an owned filename from `DirEntry`; no full
   path or separate metadata query is needed. Full paths remain necessary for
   directory traversal and following symlinks. Depth stays limited to root
   files plus one directory level. Filename rules, saturation, recent-time
   maxima and final ranking are unchanged. Results do not depend on whether
   root files or shard files are aggregated first.
3. **Unicode name comparison.** Compare and lowercase the shared ASCII prefix
   directly as bytes. At the first non-ASCII byte, continue the existing
   per-character Unicode lowercase iterator at valid character boundaries.
   This avoids Unicode conversion machinery for ordinary profile/device names.
   The comparator is used by import detection, pad configuration lists and
   profile management. Both old and new comparators allocate nothing.

## Measurement

Windows x86-64; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(`88d9e12ae`, 2026-08-18), LLVM 22.1.8, x86_64-pc-windows-msvc. Release uses
opt-level 3 and full LTO, without extra target CPU features.

Five independent release executable invocations are pinned to logical CPU 4.
Runs 1/3/5 time old first and runs 2/4 time new first. Each measurement has
three warmups and seven batches. Batch sizes are 10,000 pane operations,
50,000 comparisons, 32 sorts, and four filesystem scans. Compilation and
validation complete before final timing. Old/new functions and inputs pass
through `black_box`; allocation tracking runs in a separate complete operation
including result destruction. Both variants use the same System-delegating
counting allocator, whose lightweight TLS checks remain in allocation calls
when its counters are disabled during timing.

Eleven frozen baseline function bodies match the baseline commit after
ignoring whitespace. Pane benchmarks include filtering, primary selection and
result destruction. Sort benchmarks stably sort 1,024 indices into existing
names; both retain the same two allocations for index output and sort scratch.
Recent-history benchmarks use the public generic collector with the same
standard HashMap type in both variants. Flat histories have 512 files; sharded
histories have four directories of 128 files, each spanning 32 chart hashes.
File contents need not be read for history/count operations. Setup is untimed.

These are warm-cache synthetic preparation benchmarks, not end-to-end frame
rate, cold-disk latency, process RSS or peak resident-memory measurements.
Filesystem/security-software costs participate in the timings. CPU cycles use
Windows `QueryThreadCycleTime`; throughput units are pane operations,
comparisons, sorted names, or scanned files (empty scans count as one operation).
Tables report medians of the five per-invocation medians; the CSV retains all
220 rows and each seven-batch timing range.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | Units/s old -> new |
|---|---:|---:|---:|---:|
| `panes/0-empty-15` | 18.3 -> 18.7 | 40.3 -> 41.1 | -2.0% | 54,674,685.6 -> 53,590,568.1 |
| `panes/1-gs-15` | 124.7 -> 44.6 | 273.9 -> 97.9 | 64.3% | 8,018,603.2 -> 22,416,498.5 |
| `panes/5-mixed-15` | 152.7 -> 76.6 | 334.8 -> 168.4 | 49.7% | 6,547,502.1 -> 13,049,719.4 |
| `panes/8-tournaments-15` | 1,257.0 -> 594.1 | 2,756.2 -> 1,302.8 | 52.7% | 795,551.3 -> 1,683,331.7 |
| `panes/8-mixed-0` | 167.5 -> 85.5 | 367.5 -> 187.3 | 49.0% | 5,968,723.9 -> 11,695,906.4 |
| `panes/9-mixed-15` | 202.6 -> 221.0 | 444.4 -> 483.6 | -8.8% | 4,936,321.5 -> 4,524,886.9 |
| `panes/64-tournaments-15` | 9,753.8 -> 4,959.1 | 21,377.4 -> 10,871.3 | 49.1% | 102,524.2 -> 201,647.9 |
| `compare/ascii-early` | 30.7 -> 7.3 | 67.1 -> 15.9 | 76.3% | 32,533,021.0 -> 137,136,588.0 |
| `compare/ascii-prefix` | 570.5 -> 48.2 | 1,250.6 -> 105.7 | 91.5% | 1,752,799.2 -> 20,733,123.2 |
| `compare/equal` | 243.1 -> 24.5 | 532.9 -> 53.9 | 89.9% | 4,113,432.0 -> 40,743,155.1 |
| `compare/unicode-first` | 61.7 -> 59.9 | 135.2 -> 131.2 | 3.0% | 16,199,578.8 -> 16,703,414.2 |
| `compare/unicode-late` | 254.0 -> 116.7 | 556.8 -> 256.1 | 54.0% | 3,936,573.9 -> 8,567,658.8 |
| `compare/empty` | 9.2 -> 5.7 | 20.3 -> 12.3 | 39.4% | 108,436,347.9 -> 176,678,445.2 |
| `sort/ascii` | 2,922,228.1 -> 346,781.2 | 6,405,380.4 -> 759,895.3 | 88.1% | 350,417.5 -> 2,952,870.1 |
| `sort/mixed` | 2,870,453.1 -> 767,256.2 | 6,289,724.4 -> 1,681,603.2 | 73.3% | 356,738.1 -> 1,334,625.8 |
| `sort/unicode` | 3,076,815.6 -> 3,092,225.0 | 6,741,153.7 -> 6,776,314.9 | -0.5% | 332,811.6 -> 331,153.1 |
| `history/flat` | 53,547,400.0 -> 671,325.0 | 117,212,287.2 -> 1,470,101.2 | 98.7% | 9,561.6 -> 762,670.8 |
| `count/flat` | 54,018,250.0 -> 695,725.0 | 118,273,787.8 -> 1,526,677.5 | 98.7% | 9,478.3 -> 735,923.0 |
| `history/sharded` | 28,566,450.0 -> 1,044,425.0 | 62,555,358.5 -> 2,288,836.2 | 96.3% | 17,923.1 -> 490,221.9 |
| `count/sharded` | 27,219,175.0 -> 1,048,200.0 | 59,571,312.8 -> 2,297,397.0 | 96.1% | 18,810.3 -> 488,456.4 |
| `history/empty` | 201,800.0 -> 96,875.0 | 441,579.0 -> 209,293.2 | 52.6% | 4,955.4 -> 10,322.6 |
| `count/empty` | 241,525.0 -> 134,525.0 | 530,641.2 -> 290,508.0 | 45.3% | 4,140.4 -> 7,433.6 |

The five-pane mixed scorebox fixture uses **49.7% fewer cycles** and doubles
throughput, while eliminating its allocation/free pair and 40 bytes of heap
churn. The one-pane case saves 64.3% of cycles and eight tournament panes save
52.7%. All measured inputs with at most eight source panes have zero heap
churn. The 64-pane tournament case still saves 49.1% of cycles by avoiding
repeated classification, with unchanged heap allocation.

Flat history scanning uses **98.7% fewer cycles**, with throughput increasing
from **9,562 to 762,671 files/s**. The sharded fixture saves **96.3%** of cycles;
count-only scans save 96.1-98.7%. The flat history scan reduces allocation/free
calls **5,169 -> 555**, reallocations **5,128 -> 4**, and requested/freed bytes
**1,191,748 -> 13,604**. Savings include removing the root's second enumeration,
metadata path conversions and full-path construction for regular files.

ASCII name sorting uses **88.1% fewer cycles**, increasing throughput from
**350,418 to 2,952,870 names/s**. Mixed ASCII/Unicode sorting saves **73.3%**.
An individual shared-ASCII-prefix comparison saves **91.5%**. Scalar comparisons
remain allocation-free, and the sort's two output/scratch allocations are
unchanged. Unicode suffixes still use the prior iterator after the ASCII prefix.

| Operation | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
|---|---:|---:|---:|
| Five mixed panes | 1 -> 0 | 0 -> 0 | 40 -> 0 |
| Eight tournament panes | 1 -> 0 | 0 -> 0 | 64 -> 0 |
| Nine mixed panes | 1 -> 1 | 0 -> 0 | 72 -> 72 |
| 64 tournament panes | 1 -> 1 | 0 -> 0 | 512 -> 512 |
| ASCII-prefix comparison | 0 -> 0 | 0 -> 0 | 0 -> 0 |
| Sort 1,024 ASCII names | 2 -> 2 | 0 -> 0 | 16,384 -> 16,384 |
| Flat history, 512 files | 5,169 -> 555 | 5,128 -> 4 | 1,191,748 -> 13,604 |
| Sharded history, 512 files | 2,673 -> 595 | 2,630 -> 45 | 624,240 -> 22,703 |
| Flat count, 512 files | 5,133 -> 519 | 5,128 -> 4 | 1,187,446 -> 9,302 |
| Sharded count, 512 files | 2,637 -> 559 | 2,630 -> 45 | 619,944 -> 18,407 |
| Empty history scan | 12 -> 6 | 10 -> 5 | 2,452 -> 1,226 |

Counters are identical across all five invocations for each workload/variant.
Requested/freed bytes include reallocation traffic. Source hashes remained
unchanged during these final measurements. The small pane list uses bounded
stack storage instead of a temporary heap allocation; it adds no retained
cache. History outputs still own their map keys/arrays, and directory filename
extraction still allocates. No claim of a fully allocation-free filesystem scan
or renderer is made.

The controls are mixed. **Nine mixed panes use 8.8% more cycles (18.4 ns more
elapsed time)**, despite unchanged allocations. That case exceeds the inline
capacity and finds its preferred pane near the start, so it has little benefit
from single-pass fallback selection. Empty panes cost 0.4 ns more (2.0% more
cycles). The Unicode-heavy sort uses **0.5% more cycles (15.4 us per 1,024-name
sort)**, with overlapping sample ranges. Unicode-first scalar comparison differs
by 1.8 ns; no substantive Unicode-first speedup is claimed. The retained wins
are small-pane preparation, ASCII-prefix comparisons and history traversal;
these measurements do not establish a speedup for every input. Full per-run
ranges are in the [raw samples](ui-preparation-0.5.1222.csv).


## Behavior and validation

Six new regression tests compare against frozen old implementations:

- Pane filtering/order and primary pointer identity over all 16 filter masks,
  both preferred modes, shuffled/tied pane lists and sizes 0/1/2/5/8/9/16/64.
  Fixtures exercise named/explicit ArrowCloud kinds, GS, tournaments, fallback
  names, disabled flags and personalization. Allocation checks require zero
  churn at up to eight source panes and at most one exactly sized allocation
  after spilling.
- All 16,384 pairs of ASCII bytes, Unicode expansions and boundary cases
  (Kelvin sign, dotted I, sigma variants, sharp S, non-BMP letters), common
  ASCII prefixes, 10,000 deterministic random UTF-8 pairs and stable sort ties.
  Comparators must agree with the existing per-character mapping; contextual
  `str::to_lowercase` is a different operation and is not used as the oracle.
  Comparisons retain zero allocation churn.
- Root/shard/deeper directory layouts, malformed/overflowing timestamps,
  `.bin`/`.BIN` distinctions, directories named like files, Unicode chart
  names, missing paths, file-as-directory paths, saturation and merging into
  prepopulated maps. Combined history and ranked recent/count output are
  checked against independently collected old maps. A strict counter assertion
  checks reduced churn during a complete history scan.
- File links, broken links and duplicate directory aliases are covered when
  the host permits symlink creation. **This Windows host returned error 1314
  (missing privilege), so the symlink fixture returned early.** The previous
  storage suite has the same limitation. The invalid-byte filename fixture is
  Unix-only and was not exercised here. Concurrent filesystem mutation is not
  part of the comparison.

The locked score/profile suite passes **483 tests**, with nine manual benchmarks
ignored. The existing scorebox UI suite passes **12 tests**. All six new
regression tests pass in release mode, under the symlink limitation above.
The locked application check, scoped rustfmt/whitespace checks, and
`cargo clippy -p deadsync-score -p deadsync-profile --lib --locked -- -D clippy::perf`
pass. Existing non-performance warnings remain in unchanged code/dependencies.
The final inline hint was followed by release regression tests and fresh
five-run measurements.

The workspace version and all three inheriting lockfile package versions move
exactly once from **0.5.1221 to 0.5.1222**. There are no dependency changes.

Reproduce with:

```powershell
cargo test -p deadsync-score -p deadsync-profile --locked -- --test-threads=1
cargo test -p deadsync-theme-simply-love --lib gs_scorebox --locked -- --test-threads=1
cargo test -p deadsync-profile --release --test ui_preparation --locked -- --test-threads=1
cargo test -p deadsync-profile --release --test ui_preparation --locked -- --ignored --exact benchmark_ui_preparation --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-profile --release --test ui_preparation --locked -- --ignored --exact benchmark_ui_preparation --nocapture --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo check --locked
cargo clippy -p deadsync-score -p deadsync-profile --lib --locked -- -D clippy::perf
```

For comparable timing, invoke the built test executable five times on the same
logical CPU, alternating order, with other build/test work idle. The CPU,
allocator, name distribution, pane count, history layout and filesystem affect
the measured savings.
