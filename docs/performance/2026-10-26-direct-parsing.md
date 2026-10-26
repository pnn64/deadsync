# Direct simfile parsing — 2026-10-26

Three safe Rust optimizations in `deadsync-simfile`, following the hot-path, allocation-reuse, indirection, and throughput guidance in `rust-performance.md`. No dependency, public API, CPU feature requirement, or production `unsafe` is added. Neither Lua crate nor unrelated working-tree changes are part of this change.

## Changes and work removed

1. **Background-change delimiters:** replace the byte-at-a-time `=`/`,` predicate with the existing `memchr2` implementation, which selects SIMD when available. This removes the scalar search loop. Absolute byte offsets, the first matching delimiter, UTF-8 boundaries, and EOF behavior stay unchanged.
2. **Keysound projection:** search directly for `[` and inspect only the containing row. Copy retained source spans straight into the output as annotations are encountered. This removes scans of annotation-free rows, a second scan for each row delimiter, the annotation-range vector, its reallocations, sorting, and a separate range-consumption pass. RSSP's `extract_sections` visits disjoint SM/SSC note blocks in source order; the private projection function's only production caller passes that order directly. Rows still use the original column, whitespace, comment, malformed-bracket, and row-end rules. Annotation-free input remains borrowed, and owned output retains the original source-sized capacity.
3. **Unplayed-chart timing:** reject supported chart types before UTF-8/lowercase conversion, and pass lossy-decoded timing text as `Cow<str>` directly to the timing parser. Valid UTF-8 no longer passes through temporary owned strings. Invalid UTF-8 retains the same replacement characters and owned fallback. Timing selection, arithmetic, hold resolution, and returned bounds are unchanged.

Production changes total **one net line** after excluding test-module wiring: 27 added and 26 removed. Execution removes the intermediate range list, its sort and second traversal, and four temporary timing strings per unplayed chart in the timed fixture. The extra source lines express the direct annotation search and its source-order invariant.

## Isolated measurements

Each change was built and measured before the next production change. The initial bodies match `77ed78898c0e87c942699407f9e873983ca8ecb0`. Archived executables, initial logs, source snapshots, and validation logs are under `target/direct-parsing`.

| Change / workload | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Delimiter scan, 128-byte field | 187.48 | 20.53 | 9.132x |
| Complete background parsing, 4096-byte path prefix | 13,777.52 | 9,267.73 | 1.487x |
| Keysound projection, dense | 389,462.70 | 245,275.33 | 1.588x |
| Keysound projection, sparse | 179,889.03 | 16,473.47 | 10.920x |
| Keysound projection, no annotations | 4,075.02 | 788.56 | 5.168x |
| Unplayed bounds, eight supported charts skipped | 1,778.39 | 300.06 | 5.927x |
| Unplayed bounds, eight native charts | 66,282.63 | 60,888.53 | 1.089x |

Raw isolated data: [delimiters](2026-10-26-direct-parsing-delimiters.tsv), [projection](2026-10-26-direct-parsing-projection.tsv), [timing](2026-10-26-direct-parsing-timing.tsv). Unchanged functions are recorded as controls in the latter two files. The timing-stage projection control also includes moving the source-pointer subtraction inside the nonempty annotation branch, so an SSC block with no NOTES tag never subtracts an unrelated empty-slice pointer.

## Allocations per operation

All probes are outside timed batches and include destruction of temporary/output storage. Counts are identical across the isolated and final comparisons.

| Workload | Allocations before → after | Reallocations before → after | Requested bytes before → after | Peak live bytes before → after |
| --- | ---: | ---: | ---: | ---: |
| Delimiter scans | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| Complete background parsing, 4096-byte prefix | 9 → 9 | 0 → 0 | 4,358 → 4,358 | 4,358 → 4,358 |
| Projection, plain | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| Projection, sparse | 2 → 1 | 0 → 0 | 20,913 → 20,849 | 20,913 → 20,849 |
| Projection, dense | 2 → 1 | 11 → 0 | 300,301 → 38,221 | 169,293 → 38,221 |
| Bounds, eight supported charts | 8 → 0 | 0 → 0 | 96 → 0 | 12 → 0 |
| Bounds, eight native charts | 240 → 208 | 0 → 0 | 7,712 → 7,472 | 606 → 606 |

## Final before/after comparison and limits

The complete [final TSV](2026-10-26-direct-parsing-final.tsv) records all workloads, including slower controls. These are local parsing-kernel results, not whole-game or full-library-scan speedups.

| Workload | Before cycles/op | After cycles/op | CPU throughput | Before ns/op | After ns/op |
| --- | ---: | ---: | ---: | ---: | ---: |
| `bgchanges/len=128` | 2,219.42 | 2,178.15 | 1.019x | 1,016.83 | 993.25 |
| `bgchanges/len=16` | 1,697.99 | 1,841.22 | 0.922x | 775.40 | 839.58 |
| `bgchanges/len=4096` | 13,584.91 | 9,278.15 | 1.464x | 6,204.10 | 4,233.15 |
| `delimiter/len=128` | 181.06 | 21.21 | 8.537x | 82.85 | 9.65 |
| `delimiter/len=32` | 52.66 | 14.98 | 3.515x | 24.00 | 6.83 |
| `delimiter/len=4096` | 4,716.47 | 196.79 | 23.967x | 2,155.14 | 89.88 |
| `delimiter/len=8` | 16.66 | 23.64 | 0.705x | 7.61 | 10.77 |
| `projection/dense` | 488,109.28 | 246,652.70 | 1.979x | 223,703.50 | 112,686.75 |
| `projection/plain` | 3,836.86 | 770.44 | 4.980x | 1,752.25 | 347.75 |
| `projection/sparse` | 180,753.86 | 3,482.37 | 51.905x | 82,733.25 | 1,583.25 |
| `unplayed/dance-single/charts=8` | 1,729.66 | 299.62 | 5.773x | 789.65 | 135.85 |
| `unplayed/techno-single4/charts=1` | 8,518.80 | 7,881.91 | 1.081x | 3,898.50 | 3,612.00 |
| `unplayed/techno-single4/charts=8` | 67,891.90 | 61,269.59 | 1.108x | 31,029.35 | 28,001.95 |

SIMD dispatch costs about **3.2 ns** on the eight-byte scan. The complete short-path fixture regresses about **64 ns**, while the 128-byte complete fixture is effectively unchanged and the 4096-byte fixture improves 1.46x. This tradeoff is retained for the large reduction in longer scans without adding dispatch/scan machinery of our own.

The sparse projection result depends strongly on invocation order: 10.920x in its isolated stage versus 51.905x when the delimiter benchmark precedes it in the final executable. The cause is not established; use the conservative isolated result when estimating this kernel's benefit. Both invocation modes and all process results are retained rather than averaging them together.

Two trials were discarded. Streaming output while still scanning every row removed allocations but regressed sparse CPU throughput to 0.749x and dense throughput to 0.927x ([data](2026-10-26-direct-parsing-rejected-projection.tsv)). Adding a scalar 16-byte prefix to delimiter search reduced long-field gains, slowed the 32-byte scan, and failed to remove the eight-byte penalty ([data](2026-10-26-direct-parsing-rejected-prefix.tsv)); its extra code was removed.

## Method and verification

Intel Xeon E5-2696 v4, Windows x86_64, rustc 1.99.1 / LLVM 23.1.1. All versions use `--release --locked --config profile.release.lto=false`, with the repository's static MSVC CRT and baseline CPU target. LTO is explicitly disabled consistently; these results do not establish gains for other build profiles or CPUs.

Each worker is pinned to logical processor 2 using the existing benchmark helper. Windows `QueryThreadCycleTime` supplies thread CPU cycles. Each process discards one warmup batch and reports the median of eleven batches; tables report the median of six fresh processes per version, alternating execution order. Throughput is before/after cycles per operation. Elapsed nanoseconds are secondary. Fixture creation and allocation probes stay outside timing. No compilation or second benchmark launched by this task overlaps measured runs; unrelated activity on the shared machine can still affect results.

Delimiter fixtures search `x * len + '=next,'` and separately parse two complete change records. Projection fixtures contain four SM charts, each with 1024 plain/dense rows; sparse charts add one annotated row. Bounds fixtures contain one/eight SSC charts, four note rows, and BPM/stop/delay/warp tags. Production functions receive opaque inputs and their outputs are consumed. The six TSVs contain **612 process/workload records**, including controls and rejected trials, with executable SHA-256 and all allocation counters.

Correctness: **267 tests passed**, including both library/pack reload integration tests; 18 benchmark tests were ignored in the normal suite. New differential checks cover every tested delimiter offset with Unicode surroundings, 1,536 generated SM/SSC inputs with three chart blocks each, annotation/comment/whitespace/malformed-bracket cases, borrowed output, absent NOTES tags, global/own timing, and invalid UTF-8. Projection bytes and floating-point bound bits are compared with the original implementations. Existing song parsing, native note bounds, keysound columns, chart caching, and reload tests pass.

Clippy completed for the changed package and its test targets with existing repository warnings and no warning in the added tests or changed function bodies. The final log is `target/direct-parsing/clippy-final.log`. Final harness lint fixes affect only the untimed allocation probe calls; timed workloads are unchanged.

```powershell
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib --tests
cargo clippy --release --locked --config profile.release.lto=false -p deadsync-simfile --lib --tests
cargo test --release --locked --config profile.release.lto=false -p deadsync-simfile --lib benchmark_direct_parsing -- --ignored --nocapture --test-threads=1
python scripts/benchmark_direct_parsing.py --before target/direct-parsing/before/simfile.exe --after target/direct-parsing/after/simfile.exe --output docs/performance/2026-10-26-direct-parsing-final.tsv
```

For an isolated projection run, pass `--filter benchmark_direct_parsing_projection`. To reconstruct the archives, keep the added benchmark/test modules in both trees, restore just the three production function bodies from the baseline commit for `before`, and build with the options above. `stage1` contains delimiter search only; `stage2` also contains direct projection; `after` contains all three changes. Save the `deadsync_simfile` test executable reported by Cargo after each build, before changing the next function.

| Archive | SHA-256 |
| --- | --- |
| `before/simfile.exe` | `2186e0c9e4cdce40d27939968e995d11def590b7017d00d48ec409356e1c37f9` |
| `stage1/simfile.exe` | `40cb26c6028013163db6d513c732c459d539bb8fa0a2e4768afdbae0ae973a24` |
| `stage2/simfile.exe` | `041da18145316c27762a9bb184dc8a63db79a0546bf951490fe45999ddceadf7` |
| `after/simfile.exe` | `7438831dc7f26081af8157d0fd35a0f1dec87fc0b2b8ff538874ffe223083c00` |
