# Cabinet-light report packing and source lookup, 2026-11-04

Three retained optimizations in `deadsync-lights`, starting at `67f1fff88` on
isolated branch `perf/non-lua-kernels-20261104`. Lua and the main worktree's
existing changes remain outside this batch.

## Changes and exactness

1. Blue Dot pad reports permute both packed player bytes together with `u16`
   masks/shifts. Swapping each nibble's Left/Up bits changes L,D,U,R into the
   hardware U,D,L,R order; folding the two nibbles produces the report byte.
   High button bits stay ignored. The cabinet builder retains its original code.
2. MiniMaid constructs its nine report bytes directly from cabinet, menu and
   permuted pad bits. This removes the temporary 32-bit light accumulator and
   the per-player/per-bit setters. Marquee order, both pad-enable bits, merged
   bass/neon output, blue LED, keyboard enable and reserved zeros stay exact.
3. Linux LED mapping entries store a source-byte index and precomputed mask.
   The fixed constructors resolve these at compile time. Building outputs reads
   five source bytes once and tests the stored masks, removing enum dispatch
   and per-entry mask construction. Output order and the existing OR merge stay
   intact, including duplicate bass mappings and unmapped `None` outputs.

All production changes use safe Rust. No dependency, public interface, driver
state field or runtime cache is added. Mapping entries remain 16 bytes on this
x86-64 target, checked against the frozen original; both static mapping arrays
retain their entry counts. Unused hardware-position constants move into their
existing unit tests. Test-only access bridges and the existing allocation
counter support direct comparisons without adding production work.

## Final paired CPU measurements

`formatted-final` measures the final source in five fresh processes. Cycle
columns are independent medians; throughput is the median of five paired
before/after ratios, so it need not equal the ratio of those columns.

| Fixture, 256 states | Before cycles | After cycles | Throughput | Paired range |
| --- | ---: | ---: | ---: | ---: |
| blue_pad/sparse/256 | 2,886.86 | 1,513.89 | 1.836x | 1.759-1.931x |
| blue_pad/dense/256 | 2,856.13 | 1,549.89 | 1.843x | 1.742-1.860x |
| blue_pad/mixed/256 | 2,888.40 | 1,547.91 | 1.819x | 1.793-1.945x |
| minimaid/sparse/256 | 6,550.10 | 5,958.77 | 1.121x | 1.082-1.143x |
| minimaid/dense/256 | 6,463.18 | 5,714.90 | 1.152x | 1.069-1.155x |
| minimaid/mixed/256 | 6,533.42 | 6,031.20 | 1.088x | 1.017-1.136x |
| linux_piuio/sparse/256 | 8,646.54 | 7,466.07 | 1.158x | 1.108-1.249x |
| linux_piuio/dense/256 | 8,194.81 | 7,174.80 | 1.142x | 1.110-1.159x |
| linux_piuio/mixed/256 | 8,139.28 | 7,217.16 | 1.136x | 1.128-1.206x |
| linux_itgio/sparse/256 | 24,188.02 | 7,599.75 | 3.158x | 2.819-3.538x |
| linux_itgio/dense/256 | 22,937.75 | 7,384.86 | 3.036x | 2.852-3.642x |
| linux_itgio/mixed/256 | 22,782.34 | 7,383.54 | 3.023x | 2.885-3.365x |

Each fixture builds 256 reports/output arrays from sparse, dense or generated
mixed states. Set-up and fixture allocation are outside timing. The pad and
MiniMaid measurements include complete report construction; Linux measurements
include all 32 output slots. Every one of the 24 final workloads asserts zero
allocations, reallocations, frees and byte churn for both variants.

The twelve unchanged cabinet/GPB/PacDrive comparison workloads have final
median ratios of 0.985-1.012x. Fresh unchanged-source control medians across all
24 workloads span 0.988-1.013x. Individual control pairs span 0.785-1.360x; the
largest outlier is an ITGIO sparse pair. Absolute cycle counts also vary across
runs. Small differences are inconclusive; retained gains recur across the
candidate, final and formatted-source measurements. The preceding five-process
run records Blue Dot pads 1.780-1.882x, MiniMaid 1.117-1.135x, PIUIO
1.174-1.182x and ITGIO 3.430-3.563x.

These Windows x86-64 measurements cover pure packing and mapping. Native Linux
sysfs access, real HID/serial devices, USB throughput and whole-game performance
were not measured. Device discovery, sends, reconnects, paths, writes and worker
scheduling retain their existing code.

## Correctness and build checks

The actual crate passes **48 release unit tests**, with three benchmarks/trace
tests ignored. Five focused tests also pass in debug and release. The final
source passes focused Clippy with warnings denied and a normal release library
check. Cargo emits only its existing workspace unused-patch notices.

For each retained builder, tests cover every combination of its 16 meaningful
input bits: 65,536 states combining six cabinet bits, four directions per player
and both menu-start bits. Another 2,560 states vary each raw byte independently
with other fields all zero/all one, and 50,000 generated states combine arbitrary
raw bytes. That is **118,096 states** per device comparison. Linux compares both
complete arrays (236,192 comparisons), preserving `None`, `Some(false)` and
`Some(true)` separately. GPB's unchanged control additionally checks all 65,536
pairs of raw menu bytes. PacDrive's unchanged control checks both orderings.

The final seven kernel stages share the same **13,108,656-byte** deterministic
input/wire/output trace, SHA-256
`48a8e5e5439b9cddd0b8ad66073914609d0ff63e39d3d8d3e186d4799d452195`.
Linux output options are encoded explicitly as 0/1/2, avoiding representation or
padding assumptions. Debug and release traces match exactly. Frozen production
definitions are verified against the starting commit, and artifact/source hashes
are checked against the final working files and staged sources.

## Probes and rejected work

- `baseline` / `blue-bitfields`: initial twelve-workload probe. Combined Blue Dot
  report construction improves 1.392-1.421x.
- `minimaid-bytes`: MiniMaid improves 1.086-1.108x in its isolated probe.
- `pacdrive-bitfields`: rejected combined 16-bit packing, only 0.936-0.975x.
- `pacdrive-bytes`: rejected byte-level packing; 0.987-1.033x is inconclusive.
- `split-probe`: fifteen workloads isolate Blue Dot's two builders. Pad packing
  improves 1.810-1.865x; cabinet packing has no measured benefit (0.965-0.990x),
  so its original code is restored.
- `baseline-expanded`: eighteen workloads add GPB and full menu-pair coverage.
- `gpb-expansion`: rejected nibble multiplication expansion, 0.877-0.887x.
- `gpb-lookup`: rejected 256-byte lookup, 0.967-0.983x.
- `gpb-direct-scatter`: rejected direct multiplication scatter, 0.872-0.892x.
  GPB and PacDrive production code are both restored in the retained result.
- `baseline-linux` / `indexed-maps`: twenty-four workloads add both Linux maps.
  Indexed maps improve PIUIO 1.142-1.148x and ITGIO 2.918-2.967x.
- `final` / `control`: five fresh processes each. `formatted-final` repeats the
  retained result after removing surplus blank lines with rustfmt.
- `debug` / `formatted-debug`: unoptimized correctness and trace checks.
- `full-release` / `full-formatted`: complete actual-crate release tests.

Trace scope grows with coverage. The initial six stages share 3,542,880 bytes
(SHA-256 `dbc43aa1d79ef51d10334553d3e72d1dc044d29a0694ee1d090011eee86541d2`);
the four GPB stages share 5,550,512 bytes
(`1d5ce8831ab5b29e675ce9c92c71a8af65180bf41bdab5959f2781da982665a2`).
The Linux baseline starts the final 13,108,656-byte scope. Every stage's manifest
records its actual test count, scope, source, compiler and trace hashes.

## Method and reproduction

Windows x86-64, Rust 1.99.1 / LLVM 23.1.1,
`Intel64 Family 6 Model 79 Stepping 1, GenuineIntel`. The runner extracts actual
State/enums/accessors and complete packing/mapping definitions, including static
board maps. Generated access bridges match the actual crate's test-only bridges.
No hardware API is stubbed into those pure kernels.

Release uses optimization level 3, fat LTO, static MSVC runtime and no debug
information. The benchmark thread is pinned to CPU 2. Twelve rounds alternate
variants; the first is warmup and the median of eleven remaining rounds is
reported. Both variants use the same black-boxed function-pointer dispatch per
256-state fixture. Allocation accounting runs separately from timing.

Adjacent TSVs preserve raw thread cycles, elapsed nanoseconds and allocation
counts. Manifests preserve exact source/definition hashes, commands, binaries,
correctness logs and traces. Full validation overlays all current source deltas
onto the verified cached dependency workspace and records every overlay hash;
its temporary directory stays on the normal host volume. Exact artifacts are
archived at `D:\deadsync-light-protocol-20261104`.

```powershell
$env:DEADSYNC_LIGHT_PROTOCOL_ARCHIVE = 'D:\deadsync-light-protocol-20261104'
python scripts/benchmark_light_protocols.py baseline-linux --baseline 67f1fff88
python scripts/benchmark_light_protocols.py control --baseline 67f1fff88 --runs 5
python scripts/benchmark_light_protocols.py formatted-final --runs 5
python scripts/benchmark_light_protocols.py formatted-debug --debug
python scripts/benchmark_light_protocols.py full-formatted --full
```
