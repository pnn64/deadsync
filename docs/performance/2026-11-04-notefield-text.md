# Modifier-text integer output and fixed labels — 2026-11-04

Baseline: `d07a93399b50b163365fd957d4294b557603a529`. This batch retains three safe Rust changes in `display_mods.rs`:

1. A five-byte stack buffer emits signed i16 decimal values directly at four former formatting call sites: modifier percentages, Mini, Spacing and VisualDelay. Sign, digits and suffix are appended in the original order. `unsigned_abs` handles `i16::MIN` without overflow.
2. Turn selection walks the enabled low eight bits in ascending order and appends its fixed, space-free labels directly. It avoids scanning unset entries and normalizing known labels.
3. Transform selection rejects masks that emit no labels, then appends its fixed single-word labels directly. Only the low five hold bits are meaningful; ignored bits retain their behavior.

General atomic-text and floating formatting helpers are restored to their original bodies. Names supplied through the noteskin parameter still use the existing Unicode-preserving space replacement. The change adds one private bounded integer writer, removes four general integer-formatting call sites, and removes space normalization from the two fixed-label loops. There are no new dependencies, fields, unsafe blocks or public signatures. Returned String capacities and allocation counters are preserved.

## Final CPU and throughput measurements

Windows x86-64, Intel64 Family 6 Model 79 Stepping 1; rustc 1.99.1, LLVM 23.1.1. Release kernels use opt-level 3, fat LTO and the static CRT. Five fresh processes compare original/current on CPU 2, alternating order for twelve rounds, discarding the first and taking the median of eleven. A sample runs 250 fixtures. The input count per fixture is included in each row. Inputs and reused-buffer allocation happen outside timing; whole-string creation and destruction are inside timing. Function pointers and arguments are black-boxed.

The CPU columns are independent medians across the five processes. Throughput is the median of paired original/current cycle ratios, which can differ from the ratio of the CPU columns. TSVs retain paired nanoseconds as well. These are text-building measurements, not frame-rate measurements; the text is rebuilt when its inputs change.

| Workload / inputs per fixture | Original cycles | Current cycles | Paired throughput |
| --- | ---: | ---: | ---: |
| Mini integers / 128 | 7520.07 | 4972.12 | 1.559× |
| Named modifier integers / 128 | 12908.36 | 10248.89 | 1.235× |
| Spacing integers / 128 | 7669.33 | 4981.77 | 1.531× |
| Transforms, dense / 32 | 18297.52 | 7085.46 | 2.541× |
| Transforms, mixed / 32 | 9743.17 | 4189.82 | 2.359× |
| Transforms, single insert / 32 | 2333.72 | 2123.88 | 1.096× |
| Transforms, none / 32 | 1949.16 | 272.18 | 6.950× |
| Turns, all selected / 128 | 22299.44 | 8776.49 | 2.549× |
| Turns, mixed / 128 | 12111.13 | 8433.19 | 1.456× |
| Turns, one selected / 128 | 3722.72 | 1708.59 | 2.179× |
| Turns, none / 128 | 835.86 | 1014.09 | 0.824× |
| Complete text, basic / 16 | 27910.74 | 23918.48 | 1.163× |
| Complete text, default options / 16 | 13526.47 | 12025.96 | 1.136× |
| Complete text, dense / 16 | 83800.71 | 66213.49 | 1.261× |
| Complete text, mixed / 16 | 35580.95 | 31648.39 | 1.110× |
| Complete text, Unicode noteskin / 16 | 37295.68 | 33354.34 | 1.120× |

There is a measured tradeoff: empty turn selection costs about **1.4 additional cycles per call** (raw median CPU cost about 21% higher). An explicit entry-return variant did not remove it and was discarded. Nonempty turn cases improve 1.456×–2.549×, and the complete default-options builder still improves 1.136× alongside the transform change. The empty-turn regression is retained explicitly in the report and raw data.

The final ten unchanged atomic/float/window controls span 0.972×–1.022×. A separate unchanged-source five-process control spans **0.986×–1.016×** across all 26 workload medians, with individual pairs **0.859×–1.160×**. Small differences are inconclusive. All paired ranges are in the checks JSON.

Final complete-text fixtures use typical positive finite C/X/M speeds. Basic fixtures contain a smaller fixed set including Boost, Drunk, Dynamic Sudden and Hide BG; mixed adds signed Mini and average-error-bar values; dense enables all flag groups and nonzero percentages; Unicode supplies a spaced non-ASCII noteskin. The default-options fixture clears modifier arrays, masks and optional displays. Earlier exploratory whole-text fixtures also used raw f32 MMod values for broad coverage; their source snapshots and results remain separate.

## Allocations and returned capacity

All **21 reused-buffer workloads** remain allocation-free. Each whole-text fixture creates and frees sixteen Strings. Original/current counters match exactly in every process, including cumulative allocated/freed bytes and peak bytes:

| Complete text / 16 | Allocations | Reallocations | Frees | Allocated = freed bytes | Peak bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Basic | 16 | 20 | 16 | 4096 | 256 |
| Mixed | 16 | 29 | 16 | 6400 | 256 |
| Dense | 16 | 63 | 16 | 30720 | 1024 |
| Unicode | 16 | 31 | 16 | 6912 | 256 |
| Default options | 16 | 0 | 16 | 1024 | 64 |

The integer writer emits a negative sign separately, followed by the complete digit slice and then the original suffix. The fixed-label loops retain delimiter/name append order. Tests compare final String capacities across deliberately small and larger starting capacities; they also compare complete output byte strings. No preallocation increase or capacity shrink is used to obtain the CPU gains.

## Correctness and source scope

- Six focused tests pass in debug and release, comparing **2,232,398 string/capacity pairs** per profile. Coverage includes 1,014 Unicode/spacing texts, 10,260 f32 encodings, every one of the 65,536 i16 values through three modifier helpers, all 65,536 turn masks, all 256 disabled-window masks, 5,771 transform tuples, and 4,176 complete strings. Prefixes and starting capacities include zero, small boundary capacities and existing text.
- Transform tests apply each possible mask independently to each input field, boundary tuples including ignored hold bits, and 5,000 generated combined tuples. Turn iteration matches the original eight names and ascending bit order; high bits remain ignored. Every literal bypassing space replacement is checked against the original table and contains no ASCII space.
- The eleven latest kernel stages, including original/optimized debug, produce the same **8,473,145-byte** byte/capacity trace, SHA-256 `04df4a190a2781c6f07386b1ea356038995ed8694b5f6e55dfc222bc2e85eb47`. The first five stages used a 4,893,111-byte trace before exhaustive selection results were added; their SHA-256 is `08c701711e35a4b741729e87ff1a46ad5850bd6aed46affca5387fa7a635cb02`. Transform combinations have additional direct comparison tests beyond the trace's complete-string fixtures.
- The full notefield release suite passes **502 tests**, with nine benchmark/trace tests ignored. The normal release library check also passes.
- Focused Clippy passes with warnings denied. The audit verifies all sixteen stage source snapshots, complete generated modules, frozen original functions, compiler/binary/trace hashes, raw timing rows and allocation equality. All measured source files, including the runner, match the final worktree. Selected Rust formatting, Python compilation and whitespace checks pass.

The frozen reference contains all fifteen original functions, with test-only visibility changes. Parameters and the actual scroll enum declaration are shared/copied from production source. Restoring the six changed existing bodies, removing the new integer helper and removing the test-module declaration reproduces the original module. Generic atomic text, float trimming, speed handling, perspective handling and disabled-window formatting are unchanged.

Full testing uses the earlier isolated dependency workspace and recorded source overlays. It restores shipped PNGs and snapshots the sixteen existing JSON fixtures absent from Git. Full-suite reproduction requires those fixtures; `DEADSYNC_NOTEFIELD_FIXTURE_ROOT` can identify their checkout. The main checkout is read only. Compilation uses temporary storage on D:, while tests retain the host's normal temporary directory.

## Candidates and reproduction

Sixteen successful stages are retained: `baseline`, `atomic-fast`, `suffix-trim`, `atomic-loop`, `integer-direct`, `baseline-expanded`, `turn-bits`, `literal-turns`, `turns-guard`, `literal-transforms`, `direct-turn-bits`, `turn-entry`, `final`, `control`, `baseline-debug`, `debug`. Exploratory release stages use three processes; final/control use five. Workloads grow from 13 to 21, then 25 and finally 26 as selection/default-option coverage is added.

A no-space shortcut and a one-pass atomic-text loop had unstable gains across later builds, so the original helper was restored. Fixed-decimal suffix trimming showed only small improvements and was also restored. Bit iteration that still called the general label normalizer slowed mixed turns to 0.900×. Direct appends inside the original eight-entry loop improved dense selections but regressed empty selection to 0.252×; an empty-mask guard recovered that case, while single selections remained slower. The retained turn implementation combines direct appends with set-bit iteration. The final empty-case cost is disclosed above.

Use a fresh archive directory to preserve measured artifacts:

```powershell
$env:DEADSYNC_NOTEFIELD_TEXT_ARCHIVE='D:\deadsync-notefield-text-reproduction'
python scripts/benchmark_notefield_text.py baseline-expanded --baseline d07a93399b50b163365fd957d4294b557603a529
python scripts/benchmark_notefield_text.py final --runs 5
python scripts/benchmark_notefield_text.py control --runs 5 --baseline d07a93399b50b163365fd957d4294b557603a529
python scripts/benchmark_notefield_text.py baseline-debug --debug --baseline d07a93399b50b163365fd957d4294b557603a529
python scripts/benchmark_notefield_text.py debug --debug
python scripts/benchmark_notefield_text.py full-release --full
```

Kernel reproduction uses tracked source and rustc. Full testing also reuses the verified source archive referenced by the earlier partner-chart full-release manifest. Measured local artifacts live at `D:\deadsync-notefield-text-20261104`; tracked manifests retain their hashes and exact commands. See [`final.tsv`](2026-11-04-notefield-text-final.tsv), [`control.tsv`](2026-11-04-notefield-text-control.tsv), the stage manifests/TSVs sharing this prefix, and [`checks.json`](2026-11-04-notefield-text-checks.json).
