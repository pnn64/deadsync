# Score and profile index encoding performance 0.5.1703

Parent: `be7c69c5d` (0.5.1702). Date: 2026-10-03.

Three persistence optimizations remove data copied solely for encoding:

1. Local score indexes serialize their version and borrowed index directly.
   The original cloned five hash tables and all their owned chart-hash keys
   into a temporary file record, then destroyed that clone after encoding.
2. Profile stats sort borrowed pack-name strings, preserving their original
   ordering. The original copied every name into a `Vec<String>` before
   sorting and encoding. The borrowed vector has smaller elements and owns
   no name strings.
3. ITL self indexes encode their hashbrown cache directly using the existing
   map layout: a u64 entry count followed by key/value pairs. The original
   collected a second standard-library hash map of references, including
   allocation and rehashing. A private `Encode` view replaces that table.

Existing public APIs, wire versions, decoders and file error handling are
retained.
Score/profile encodings match the parent's bytes exactly. ITL map entry order
can differ because the discarded temporary table had randomized iteration
order; length, key/value contents and decoding remain identical. No cache,
dependency or unsafe code is added.

## Measurement method

Windows x86-64, Intel Xeon E5-2696 v4 (22 cores/44 logical processors),
Rust/Cargo 1.98.1, release opt-level 3 with full LTO. The three original
function bodies are frozen from the parent, verified identical modulo
formatting/visibility. Original/current comparisons run in the same final
release test binaries. The ITL preparation baseline is the parent's exact
two-line table construction/encoding block, with an unwrap for measurement;
the complete file-write benchmark invokes the frozen original save function.

Inputs are synthetic, prepared outside measurement, and remain unchanged.
Each encoding operation includes output destruction and the original's
temporary-clone/table destruction. Local fixtures fill all five score maps
with unique chart hashes. Pack names and ITL keys include short and long
UTF-8 strings; ITL values exercise integer encoding boundaries. Inputs contain
0, 1, 128 or 1024 entries; long-string fixtures contain 128 entries.

Six serial rounds alternate original-first/current-first after compilation
and tests finish. Each measurement has three warmups and seven samples of
128 encodes, or 16 complete file writes. `QueryThreadCycleTime` reports
calling-thread CPU cycles. Other host activity is not isolated. Tables show
separate six-round time medians and medians of paired cycle reductions and
throughput ratios. Negative reductions mean more cycles; ranges span all
six paired rounds.

The [raw CSV](index-encoding-0.5.1703.csv) contains 204 measurements:
17 workloads times six rounds times two implementations. Allocation counters
use the existing thread-local System allocator wrapper in a separate complete
operation. Requested/freed bytes measure allocator traffic, not peak RSS.
Remaining allocations own the output buffer and, for pack stats, the vector
of sorted references. Empty inputs are controls with identical heap traffic.

## Local score indexes

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `entries=0/long=false` | 182.1 -> 127.8 | 27.8% (25.9..31.8) | 1.41x | 1 -> 1 | 6 -> 6 |
| `entries=1/long=false` | 1991.8 -> 417.2 | 79.5% (77.6..81.5) | 4.92x | 11 -> 1 | 1102 -> 156 |
| `entries=128/long=false` | 123890.2 -> 12663.2 | 89.7% (89.1..90.9) | 9.70x | 646 -> 1 | 81750 -> 19206 |
| `entries=1024/long=false` | 676058.2 -> 145952.3 | 78.4% (76.0..80.2) | 4.63x | 5126 -> 1 | 654954 -> 155162 |
| `entries=128/long=true` | 146803.1 -> 45929.7 | 68.9% (67.7..70.0) | 3.22x | 646 -> 1 | 799830 -> 378886 |

## Profile pack stats

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `packs=0/long=false` | 91.4 -> 92.2 | 0.8% (-4.2..4.7) | 1.00x | 1 -> 1 | 7 -> 7 |
| `packs=1/long=false` | 391.4 -> 231.2 | 37.2% (12.2..54.6) | 1.64x | 3 -> 2 | 164 -> 102 |
| `packs=128/long=false` | 17282.8 -> 6878.5 | 58.6% (55.2..60.9) | 2.41x | 130 -> 2 | 10887 -> 6023 |
| `packs=1024/long=false` | 245077.8 -> 89919.6 | 63.0% (57.5..66.5) | 2.71x | 1026 -> 2 | 87049 -> 48137 |
| `packs=128/long=true` | 72712.1 -> 15973.1 | 77.6% (72.4..86.1) | 4.65x | 130 -> 2 | 149895 -> 75655 |

## ITL self-index preparation

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `entries=0/long=false` | 85.9 -> 75.8 | 11.4% (7.7..13.3) | 1.13x | 1 -> 1 | 1 -> 1 |
| `entries=1/long=false` | 214.1 -> 89.8 | 57.2% (55.1..61.0) | 2.41x | 2 -> 1 | 124 -> 40 |
| `entries=128/long=false` | 8590.7 -> 3151.9 | 63.0% (60.9..69.8) | 2.70x | 2 -> 1 | 9553 -> 5185 |
| `entries=1024/long=false` | 131436.0 -> 33543.8 | 74.8% (69.0..80.6) | 4.01x | 2 -> 1 | 76307 -> 41475 |
| `entries=128/long=true` | 42120.7 -> 17706.7 | 56.4% (52.5..65.3) | 2.30x | 2 -> 1 | 81489 -> 77121 |

## Complete ITL file writes

The complete original/current save functions overwrite the same file in each
fixture directory. Directory creation/checks, encoding, temporary writes,
rename/cleanup and buffer destruction are inside measurement. Fixture setup
and final cleanup are outside it. Both variants are warmed before samples;
neither performs an additional durability flush. Throughput counts completed
save calls, rather than encodes.

| Workload | Original -> current ns/op | CPU-cycle reduction (round range) | Throughput ratio | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `entries=128` | 1206965.6 -> 1201503.1 | -0.2% (-3.5..0.7) | 1.00x | 8 -> 7 | 10216 -> 5848 |
| `entries=1024` | 1337909.4 -> 1248490.6 | 6.7% (3.8..8.3) | 1.07x | 8 -> 7 | 76970 -> 42138 |

Populated local score indexes reduce median CPU cycles 68.9-89.7% and
improve throughput 3.22x-9.70x. At 1024 entries, allocations/frees fall from
5126 to one and requested/freed bytes from 654954 to 155162. This removes
5120 chart-hash copies and five temporary tables per encode. The empty-index
case also reduces cycles while retaining its single output allocation.

Populated pack stats reduce median cycles 37.2-77.6%. At 1024 packs,
allocations/frees fall from 1026 to two and requested/freed bytes from
87049 to 48137. The remaining allocations own the sorted reference vector
and encoded output. Empty-pack controls keep identical traffic and their
cycle range crosses zero.

Populated ITL preparation reduces median cycles 56.4-74.8%, with 2.30x-4.01x
throughput. Each removes one temporary-table allocation/free; the 1024-entry
case removes 34832 requested/freed bytes. Complete 1024-entry saves reduce
median cycles 6.7% (positive in every round) and improve throughput 1.07x.
The 128-entry full-write control has a -0.2% paired cycle median with a range
of -3.5..0.7%; it shows no clear timing change. Both full-write sizes reduce
allocation/free calls from eight to seven, with fewer requested/freed bytes.

Every populated encoding workload has positive cycle savings in every round.
No behavioral regression or increased allocation traffic is found. These
measurements cover the displayed encodes, complete writes and empty controls.

These synchronous synthetic results establish encoding/allocation changes
and the displayed file-write measurements. They do not establish gameplay
FPS, whole-profile save latency on other storage, or gains on other platforms.

## Behavioral validation

- The complete debug score/profile suite passes 518 tests when excluding the
  single verified pre-existing allowlist failure. Unfiltered debug/release
  library runs pass 497 tests and fail that same test; all four new regression
  tests pass in both modes.
- `tests::lua_submit_allowlist_requires_known_hash` expects
  `f95bc209c6f2cbfe` to be rejected although the unchanged allowlist includes
  it. An isolated Rust test reproduces the failure using the exact allowlist,
  both policy functions and original test extracted from the parent commit.
- Score-index tests compare exact bytes, decoded maps, unchanged inputs,
  spare/deleted buckets, integer-length boundaries, truncated buffers and
  float bits including signed zero, infinities and a NaN payload. Existing
  version rejection, score-file append and index rebuild tests pass.
- Pack-stat tests compare exact sorted bytes and both existing decoder shapes
  for empty, case/Unicode, emoji, NUL/newline and long names, plus combo/count
  boundaries. Existing legacy stats decoding and file load/write tests pass.
- ITL tests compare decoded maps, consumed/encoded lengths, exact count/pair
  layout, untouched source caches and reduced heap churn. Full file tests
  compare overwrites, empty saves, nested parent creation, parent-file errors,
  temporary-directory errors, rename-to-directory errors, error paths/kinds,
  temporary cleanup and the existing no-parent no-op. Existing ITL/SRPG
  profile-file and runtime-cache tests pass.
- The new filesystem fixture resolves its target under the workspace before
  creation and verifies the absolute descendant path before recursive cleanup.
- Architecture checks: 141 pass and the exact same 12 existing failures
  remain. Their baseline reproduction is documented in
  [the preceding baseline report](actor-song-timing-0.5.1699.md#behavioral-validation).
- Score/profile Clippy for all targets completes with existing warnings.
  Targeted rustfmt and `git diff --check` pass.
- Cargo.toml and Cargo.lock change exactly 0.5.1702 -> 0.5.1703. The lockfile
  changes only the three packages inheriting that workspace version.

## Reproduce

```powershell
# Unfiltered runs include the documented existing allowlist failure.
cargo test -p deadsync-score -p deadsync-profile
cargo test -p deadsync-score -p deadsync-profile -- --skip tests::lua_submit_allowlist_requires_known_hash
cargo test --release -p deadsync-score -p deadsync-profile --lib
cargo clippy -p deadsync-score -p deadsync-profile --all-targets
cargo test --release -p deadsync-score --lib index_encoding_perf::local_index_encoding_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-score --lib itl::index_encoding_perf::itl_index_encoding_benchmark -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-profile --lib stats_encoding_perf::profile_stats_encoding_benchmark -- --exact --ignored --nocapture --test-threads=1
```

Run benchmarks serially after builds/tests finish. Repeat six rounds, setting
`$env:DEADSYNC_BENCH_NEW_FIRST = '1'` for even rounds and removing it for odd
rounds. Each command measures the frozen original and current implementation.
The ITL command includes both preparation and complete file writes.
