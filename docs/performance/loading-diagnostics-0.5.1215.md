# Loading and diagnostics performance, 0.5.1215

Baseline: `012adc4ce` / 0.5.1214. This pass applies the supplied guide's
M-HOTPATH, M-THROUGHPUT, M-MEM-REUSE, M-INITIAL-CAPACITY and M-LOG-OVERHEAD
recommendations to background parsing and diagnostic text. These measurements
isolate in-memory work, not library-loading I/O, rendering, log sinks or game FPS.

## Three optimizations

1. **Compare background filenames in chunks.** Ordinary filename candidates use
   a slice comparison with ASCII case folding instead of checking newline rules
   before every byte. A bounded 64-byte probe is shared by directory candidates,
   preserving early termination for streaming consumers. Candidates differing at
   their first byte are rejected immediately. Newlines in the probe keep the
   original loop; longer candidates also check their remaining prefix. LF/CRLF
   inside or immediately following a candidate retains the original matcher.
   The legacy directory loop stays separate and inlines its per-entry matcher.
   Directory order, delimiters in filenames, bare CR, Unicode bytes and final
   empty fields remain unchanged.
   Ordinary records already had zero allocation churn and retain that property.
2. **Stream stutter diagnostics through reusable storage.** The application now
   formats and emits one line at a time using one String. It no longer builds a
   Vec of owning strings before logging. Initial capacity is 768 bytes with
   frame samples or 256 without them. The buffer grows for unusually long
   reasons/values and drops at the end of the dump. Line order and every formatted
   byte remain unchanged. Frame/event snapshot allocations and logger costs are
   outside this formatter benchmark.
3. **Reuse timing-overlay text storage.** A timing readout writes into retained
   scratch storage, then allocates only its final Arc when displayed text changes.
   An exact match for the last formatted key bypasses hashing; other retained
   keys still use the existing map. Equal consecutive text shares its previous
   Arc without admitting another raw telemetry key. The last result remains
   reusable after map saturation, where the old implementation allocated and
   formatted it again on every frame. Formatting and replacement stay outside
   the small cache-hit function.

Timing storage remains presentation-thread-local and drops with the thread.
The map starts at capacity 256, retains at most 4,096 keys and never evicts.
Scratch starts at 384 bytes and retains the largest formatted size. The cache
can retain one extra Arc after saturation. This is a bounded metadata/scratch
tradeoff; arbitrary changing IDs still fill the existing map. No dependencies,
production allocator, target CPU or unsafe production code changed.

## Measurement

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), `x86_64-pc-windows-msvc`, release with full LTO.
The prior functions, including inline attributes, are frozen under each
package's `tests/loading_diagnostics/` and compared with included production
code in the same executable. Diagnostic record types are shared and unchanged.

Five independent invocations alternate old/new order (new first on runs 2 and
4). Each invocation uses `tests/support/perf.rs`: three warmups, seven timed
batches, and one separately allocation-counted operation. Tables report medians
of five invocation medians. Windows `QueryThreadCycleTime` measures the calling
thread's CPU cycles; all measured operations are synchronous. Requested/freed
bytes include reallocation traffic and are not process RSS. Throughput derives
from elapsed time. Tests assert behavior and allocation budgets, not timings.

## Results

Times and cycles are per operation. Throughput is millions of background
records/s, stutter lines/s, or timing readouts/s. Early-exit parsing consumes one
record; an empty stutter dump still emits its header. Negative "fewer cycles"
values indicate higher measured CPU use.

| Workload | ns old -> new | Thread cycles old -> new | Fewer cycles | M units/s old -> new |
|---|---:|---:|---:|---:|
| `background/empty-directory` | 27,942.0 -> 28,341.3 | 61,288.8 -> 62,174.1 | -1.4% | 4.581 -> 4.516 |
| `background/two-entries` | 67,345.3 -> 38,446.7 | 147,713.2 -> 84,326.0 | 42.9% | 1.901 -> 3.329 |
| `background/128-entries` | 2,090,354.7 -> 292,731.3 | 4,581,430.4 -> 641,775.5 | 86.0% | 0.061 -> 0.437 |
| `background/512-entries` | 8,185,496.7 -> 1,021,054.7 | 17,935,577.7 -> 2,238,092.2 | 87.5% | 0.016 -> 0.125 |
| `background/diverse-entries` | 209,955.3 -> 100,415.3 | 459,834.9 -> 219,627.3 | 52.2% | 0.610 -> 1.275 |
| `background/multiline` | 1,843,262.0 -> 1,917,134.7 | 4,036,758.7 -> 4,180,033.6 | -3.5% | 0.069 -> 0.067 |
| `background/early-exit` | 16,336.9 -> 2,152.3 | 35,633.5 -> 4,693.7 | 86.8% | 0.061 -> 0.465 |
| `stutter/empty` | 816.3 -> 687.8 | 1,778.3 -> 1,504.5 | 15.4% | 1.225 -> 1.454 |
| `stutter/single-frame` | 3,553.0 -> 2,972.8 | 7,756.4 -> 6,516.5 | 16.0% | 0.563 -> 0.673 |
| `stutter/128-frames` | 418,240.0 -> 318,325.3 | 915,691.8 -> 697,641.3 | 23.8% | 0.308 -> 0.405 |
| `stutter/mixed` | 468,418.3 -> 407,730.3 | 1,026,186.6 -> 893,324.0 | 12.9% | 0.412 -> 0.473 |
| `timing/stable` | 128.1 -> 70.1 | 281.1 -> 151.3 | 46.2% | 7.805 -> 14.269 |
| `timing/alternating` | 130.2 -> 91.2 | 285.5 -> 199.9 | 30.0% | 7.680 -> 10.971 |
| `timing/64-cached` | 147.3 -> 126.8 | 322.5 -> 278.3 | 13.7% | 6.787 -> 7.884 |
| `timing/changing-ids` | 1,744.4 -> 1,714.9 | 3,823.5 -> 3,743.7 | 2.1% | 0.573 -> 0.583 |
| `timing/changing-no-audio` | 803.3 -> 791.9 | 1,759.7 -> 1,722.4 | 2.1% | 1.245 -> 1.263 |
| `timing/jitter` | 1,810.8 -> 1,748.6 | 3,967.6 -> 3,810.6 | 4.0% | 0.552 -> 0.572 |
| `timing/saturated-stable` | 1,751.7 -> 62.4 | 3,835.9 -> 136.9 | 96.4% | 0.571 -> 16.024 |

| Workload | Allocations/frees old -> new | Reallocations old -> new | Requested/freed bytes old -> new |
|---|---:|---:|---:|
| `background/multiline` | 256 -> 256 | 0 -> 0 | 8,960 -> 8,960 |
| `stutter/empty` | 2 -> 1 | 0 -> 0 | 256 -> 256 |
| `stutter/single-frame` | 3 -> 1 | 0 -> 0 | 838 -> 768 |
| `stutter/128-frames` | 130 -> 1 | 0 -> 0 | 74,752 -> 768 |
| `stutter/mixed` | 194 -> 1 | 0 -> 0 | 88,064 -> 768 |
| `timing/changing-ids` | 2 -> 1 | 0 -> 0 | 576 -> 288 |
| `timing/changing-no-audio` | 2 -> 1 | 0 -> 0 | 312 -> 152 |
| `timing/jitter` | 2 -> 0 | 0 -> 0 | 576 -> 0 |
| `timing/saturated-stable` | 2 -> 0 | 0 -> 0 | 576 -> 0 |

Ordinary filename matching uses **42.9-87.5% fewer cycles**, with **86.8% fewer**
for early exit. The empty-directory and newline-heavy controls measure **1.4%**
and **3.5% more cycles** respectively (1.4% and 4.0% higher elapsed medians).
Their per-invocation ranges overlap, and the newline control's individual paired
cycle differences range from a 2.1% decrease to an 8.1% increase. These controls
show no meaningful speedup; the new path's benefit is ordinary filename matching.

Stutter formatting uses **12.9-23.8% fewer cycles**. The mixed report reduces
allocation traffic from **194 calls / 88,064 requested bytes** to **one call /
768 bytes**. The header-only case halves calls while requesting the same bytes.

Timing cache hits use **13.7-46.2% fewer cycles**; stable text absent from a full
map uses **96.4% fewer**. Changing IDs and low-bit jitter have small elapsed/cycle
improvements whose timing ranges overlap; the dependable gain on those cases is
allocation reduction: changed text goes from two allocations to one, and jitter
from two to zero. No whole-application FPS claim follows from these microbenchmarks.

All benchmark cases have zero reallocations. Allocations/frees and requested/
freed bytes match in each measured operation. Ordinary background parsing and
all three retained-key timing controls remain allocation-free. Newline removal
still owns its normalized fields, with unchanged allocation churn.

[All 180 raw measurements, including ranges, cycles, throughput and allocation
counters](loading-diagnostics-0.5.1215.csv) accompany this report.

A cold-cache retention test supplies 4,096 distinct display-error bit patterns
that all print the same value. Retained map entries fall from **4,096 to 1**,
and retained string payloads from **1,085,440 to 265 bytes**. These payloads
exclude hash-table storage and Arc headers. On this target, the cache struct grows from 32 to 248 bytes and reserves 384 scratch
bytes initially: **600 bytes** of fixed metadata/initial scratch overhead.
After saturation it may also retain one additional final Arc. Changing IDs
still use the original 4,096-entry bound; the large retention improvement is
specific to equivalent consecutive displayed text.

## Workloads and behavior

Background workloads stream 128 eleven-field changes with two filename fields,
using directories of 0, 2, 128 or 512 entries. The selected filename is last in
directory order. Controls vary the first letter, insert CRLF within names, and
stop after one record. Each timed batch runs 150 parses or 20,000 early exits;
fixtures are built before timing. All fields are consumed through a black box.
Directory entries are nonempty filenames, as provided by the filesystem.

Stutter workloads emit a header alone, one frame, 128 frames, or 128 frames plus
32 display and 32 audio events. Fields include realistic phases and integer
extremes. Each timed batch runs 20,000 small dumps or 300 large dumps. Both
versions consume every resulting line through the same black-box checksum and
include destruction of output storage; logger/disk I/O is excluded.

Timing workloads build/drop one Arc per operation, with 20,000 operations per
batch. Caches are first filled with 4,096 changing present IDs. Cases cover one,
two and 64 retained keys; continuously advancing IDs with/without audio; low-bit
float jitter; and a stable key absent from the saturated map. There is no
rounding before formatting. The changing cases retain their ID counter across
samples to prevent accidental cache hits.

Eleven new behavior/allocation tests cover:

- Entry ordering and prefixes, filenames containing delimiters, LF/CRLF and bare
  CR, UTF-8, every valid truncation of representative tags, 1,000 randomized
  input/directory combinations, and long names around the chunk boundaries.
- No heap churn for ordinary background parsing and stopping after one record.
- Exact stutter line bytes and order, no-frame/no-event cases, future timestamps,
  zero expected interval, integer extremes, nonfinite floats and buffer growth
  for long reasons; one-buffer allocation budgets and reduced churn.
- Every timing key field, 4,096 random floating bit patterns, large audio values,
  saturation, retained external Arc owners, subprecision jitter, one-allocation
  changed text, zero-allocation repeated text and bounded retained entries.

Validation on the final code:

- Simfile, shell and theme library suites: **1,824 passed**, 13 existing ignored.
- Focused suites: **22 passed**, three manual benchmarks ignored, in debug and
  release (including the included modules' existing tests).
- Application `cargo check --locked` passed.
- Performance Clippy passed with the existing `large_enum_variant` warning in
  unchanged `deadsync-theme-simply-love/src/effects.rs:928`; no new findings.
- Scoped rustfmt, `git diff --check`, frozen-parent-source and version audits passed.
- Cargo.toml bumps **0.5.1214 -> 0.5.1215** exactly once. Cargo.lock changes only
  the three workspace-version inheritors; no dependency versions change.

## Reproduce

```powershell
cargo test -p deadsync-simfile -p deadsync-shell -p deadsync-theme-simply-love --lib --locked -- --test-threads=1
cargo test -p deadsync-simfile -p deadsync-shell -p deadsync-theme-simply-love --test loading_diagnostics --locked -- --test-threads=1
cargo test -p deadsync-simfile -p deadsync-shell -p deadsync-theme-simply-love --release --test loading_diagnostics --locked -- --test-threads=1
cargo check -p deadsync --locked
cargo clippy -p deadsync-simfile -p deadsync-shell -p deadsync-theme-simply-love --lib --test loading_diagnostics --locked --no-deps -- -A clippy::all -W clippy::perf
# Run benchmarks serially, with no other builds running; repeat five times.
cargo test -p deadsync-simfile -p deadsync-shell -p deadsync-theme-simply-love --release --test loading_diagnostics --locked -- --ignored --exact benchmark_loading_diagnostics --nocapture --test-threads=1
# Set $env:DEADSYNC_PERF_REVERSE='1' for runs 2 and 4; remove for runs 1, 3 and 5.
```
