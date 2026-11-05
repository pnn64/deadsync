# Routine hash buffers and partner dispatch, 2026-11-04

Three retained optimizations in `deadsync-simfile::partner_charts`, starting at
`4d7ae14e6` on isolated branch `perf/non-lua-kernels-20261104`. Lua and the
main worktree's existing changes are outside this batch.

## Changes and exactness

1. Reuse the partner-width lookup in `project`. Partner charts need neither a
   separate RSSP supported-type check nor a second width lookup. Short-field
   rejection, supported native types, unsupported entries and playable-chart
   ordinals retain their behavior. RSSP's lookup is a pure constant function.
2. Clone the generated grid once and mask the other participant's owned cells.
   Restore that buffer for the second participant. The generated grid contains
   only zero or owned cells; its commas/newlines and every unowned zero already
   have the required representation. This removes a tree lookup for every cell,
   two byte-by-byte buffer builders and their growth reallocations. Drop the
   reusable buffer before transferring ownership records, preserving peak memory.
3. Borrow note data when it contains no slash and ends in a byte other than
   comma/ampersand. The existing cleanup produces exactly the input plus a final
   newline in this case; that newline contributes only an ignored empty line.
   Empty input and trailing separators can gain a nonempty raw measure/part from
   that newline, so they retain the original cleanup. Any slash also retains
   the original comment-removal path, including single slashes and large comments.

All production changes use safe Rust. There are no new public APIs, state
fields or production dependencies; `memchr` was already a direct dependency.
Only two production function bodies change (32 added / 28 removed lines).
The old hold resolver, row rounding, ownership construction and RSSP hash
minimizer are unchanged. The generated-grid indexing uses the same row/column
layout as the existing grid writer.

## Final paired measurements

| Workload | Before cycles/fixture | After cycles/fixture | CPU throughput |
| --- | ---: | ---: | ---: |
| Couple projection, 16 charts | 4,357.95 | 3,049.73 | 1.434x |
| Native projection, 16 charts | 1,142.50 | 1,156.77 | 1.029x |
| Mixed projection, 16 charts | 1,373,426.07 | 302,134.47 | 4.546x |
| Sparse routine projection, four charts | 973,336.20 | 172,029.47 | 5.638x |
| Dense routine projection, four charts | 2,026,467.93 | 548,120.80 | 3.727x |
| Hold/mine routine projection, four charts | 1,336,886.73 | 257,049.13 | 5.179x |
| Plain merge, 16 measures | 479,197.80 | 346,370.93 | 1.382x |
| Commented merge, 16 measures | 688,703.13 | 688,322.67 | 1.000x |
| Trailing-separator merge, 16 measures | 483,368.33 | 504,264.67 | 0.969x |

Cycle columns are independent five-process medians. Throughput is the median
of five paired ratios and can differ from the ratio of those columns.
Native-only and commented cases do not establish a CPU improvement.

Paired ranges: couple 1.394–1.504x; mixed 4.489–4.729x; sparse/dense/hold
routine 5.104–5.725x, 3.627–3.750x and 5.034–5.413x; plain merge
1.339–1.392x. Trailing-separator throughput ranges 0.935–0.993x: its
paired median is about 3.2% more cycles. The unchanged-source control is already
0.975x for that case, so a small additional fallback cost is inconclusive.
The fallback cases have no allocation benefit. The borrowed path is retained
for its demonstrated plain-input CPU and memory improvements.

| Workload | Allocations before → after | Reallocations before → after | Allocated bytes before → after |
| --- | ---: | ---: | ---: |
| Couple projection | 2 → 2 | 2 → 2 | 11,274 → 11,274 |
| Mixed projection | 235 → 227 | 128 → 28 | 283,576 → 223,776 |
| Sparse routine projection | 170 → 162 | 128 → 28 | 242,738 → 182,938 |
| Dense routine projection | 454 → 446 | 128 → 28 | 321,746 → 261,946 |
| Hold/mine routine projection | 235 → 227 | 128 → 28 | 256,754 → 196,954 |
| Plain merge | 191 → 190 | 11 → 0 | 115,525 → 78,670 |
| Commented merge | 191 → 191 | 11 → 11 | 119,620 → 119,620 |
| Trailing-separator merge | 191 → 191 | 11 → 11 | 117,255 → 117,255 |

Native-only projection remains allocation-free. Counts include output
destruction; input construction and entry parsing are outside measurement.
Allocated bytes include every allocation/reallocation request, rather than
retained memory. Plain-merge peak falls from 77,262 to 58,830 bytes. Routine
projection peaks remain unchanged in these fixtures, despite reduced churn.
The commented fixture verifies that large comments do not trigger an input-sized
temporary reservation.

## Behavior verification

All **296 actual simfile unit tests pass** in release (24 remain ignored as
marked). The seven focused tests also pass in debug and release; they are
included in that full-unit count. Full-crate verification covers optimized unit
tests, while the focused checks cover both profiles. The source overlays and
copied full-unit executable are hashed in the full-release manifest.

Focused production/test Clippy passes with `-D warnings`. The full compiler
reports existing dead-code warnings in unchanged frozen song-search helpers.
Formatting, Python compilation and whitespace checks pass.

Four existing tests retain their frozen reference at `f7f34d930`; three new
tests also compare with `4d7ae14e6`. New checks cover mixed native/partner/unsupported
ordinals and field counts 0/4/5/255; exact projected bytes, owners and hash inputs
for 294 generated/directed routine bodies in SM/SSC; and 160 combinations of
empty parts, comments, single slashes, holds, whitespace and trailing separators.
Both dance and pump widths are covered. Borrowed-result identity is checked.

All seven kernel stages emit the same deterministic grid/ownership/projection/
hash-input trace as the preceding batch: **1,709,939 bytes**, SHA-256
`678bd61a339559ac1dfbfd56d889dd706a74723a4a1de69f26c10091e62df032`.
The frozen pre-batch production prefix differs only in test visibility for its
merge function, after normalizing newlines.

## Evidence and limits

Adjacent TSVs/manifests retain these cumulative stages:

- `baseline`: unchanged pre-batch production and all focused cases.
- `type-reuse`: couple projection 1.442x; no heap change.
- `hash-mask`: sparse/dense/hold routine projection 4.314x/3.390x/4.598x;
  hash-buffer reuse removes four allocations and 72 reallocations per four charts.
- `comment-borrow`: plain merge 1.351x with zero cleanup-buffer allocation/growth.
- `debug`: focused correctness and trace in the unoptimized profile.
- `final` / `control`: five fresh processes each, retained sources versus
  exact pre-batch production respectively.
- `full-release`: actual crate unit suite, using the prior dependency snapshot
  plus verified source overlays.

Unchanged-source control medians span 0.975–1.016x; individual pairs span
0.898–1.078x. Plain-merge control is 0.997x, couple 1.016x, and routine
controls 0.989–0.996x. Absolute cycle levels vary across stages; paired ratios
are the primary comparison. Small differences are inconclusive. Results cover
projection/merge components, including canonical hash preparation, rather than
filesystem scanning, complete song analysis or application startup.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1, CPU identifier
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. Real locked RSSP and
`memchr` dependencies, optimization level 3, LTO, static MSVC runtime, no debug
information or incremental compilation. The harness compiles the complete
production prefix and the real private-helper tests. Its shared runner now
archives all partner-test modules and accepts the selected report/benchmark;
the previous benchmark defaults remain available.

Threads are pinned to CPU 2. Twelve rounds alternate variants; the first is
warmup and the median of eleven remaining rounds is reported. Five fresh
processes provide final/control paired medians. Thread-local `System`
allocation counters run separately from timing. Projection fixtures use eight
lanes; routine fixtures have four charts, two measures and sixteen authored
rows per measure. Merge fixtures have sixteen measures and 48 authored rows.
Mixed projection includes unsupported, native, couple and routine entries.

Compiler/commands, source and binary hashes, dependency lockfiles, traces,
correctness logs and raw CPU/ns/allocation rows are recorded. Exact artifacts
are archived at `D:\deadsync-partner-hashes-20261104`. Full-unit validation
reuses `D:\deadsync-partner-charts-20261104\full-workspace`: its base archive
hash and Git delta are checked before overlaying the five partner source files.
Other production/dependency sources remain unchanged. The host's normal NTFS
temporary directory is retained for existing junction fixtures.

```powershell
$env:DEADSYNC_PARTNER_HASH_ARCHIVE = 'D:\deadsync-partner-hashes-20261104'
python scripts/benchmark_partner_hashes.py baseline --baseline 4d7ae14e6
python scripts/benchmark_partner_hashes.py control --baseline 4d7ae14e6 --runs 5
python scripts/benchmark_partner_hashes.py final --runs 5
python scripts/benchmark_partner_hashes.py debug --debug
python scripts/benchmark_partner_hashes.py full-release --full
```
