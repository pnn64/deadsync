# Lua 5.1 source concatenation measurements

The harness records five alternating Rust/C samples per workload over 64 runtime
and 22 frontend workloads. A baseline uses compiler commit `77ed78898c0e87c942699407f9e873983ca8ecb0`
with the two new benchmark fixtures and workload-count update. Its recorded hashes
were checked against that commit (accounting for Git line endings); the benchmark
and script fixtures match the final run. Two final runs use source digest
`c301d392282567ff646595d83612620c0d822bc2ce0ff314fc53f7e19b094c1b`; all final report hashes match the tree. The official C
source digest is identical in all three runs.

The oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT,
binary64 and initial C locale. The shared host is not CPU-isolated. Baseline then
final then confirmation is an ordered comparison, not a randomized crossover or
a whole-engine speed guarantee. Allocation counts and instruction layouts provide
additional evidence for the removed work.

`concat_chain_16` and `concat_chain_128` each concatenate sixteen-byte strings in
a 200-iteration Lua loop and return the accumulated output length (51,200 and
409,600). Each paired sample runs ten scripts; compilation and warmup are excluded.
Source scalar checks run before timing. The old compiler emits pairwise operations;
the final compiler extends one range operation. Buffered copying processes a run
of strings once instead of repeatedly copying and interning its suffixes. Metamethod
chains retain the existing resumable range path and are validated separately.

Durations are milliseconds per complete script: baseline / first / confirmation.
Ratios are medians of five paired Rust/C durations. Speedups divide the baseline
Rust median by each final Rust median; C durations expose host variation.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration | Rust speedup first / confirm |
| --- | ---: | ---: | ---: | ---: |
| `concat_chain_16` | 1.765 / 0.161 / 0.158 | 0.040 / 0.041 / 0.046 | 43.860 / 3.849 / 3.628 | 10.96 / 11.20 |
| `concat_chain_128` | 140.446 / 1.095 / 0.984 | 0.212 / 0.213 / 0.201 | 640.385 / 5.253 / 4.866 | 128.22 / 142.67 |
| `byte_concat` | 2.819 / 3.398 / 3.518 | 0.430 / 0.476 / 0.484 | 6.473 / 7.122 / 7.176 | 0.83 / 0.80 |
| `capture_limit` | 8.722 / 9.737 / 9.261 | 6.190 / 6.766 / 6.547 | 1.446 / 1.424 / 1.413 | 0.90 / 0.94 |
| `function_dump` | 0.279 / 0.353 / 0.351 | 0.121 / 0.133 / 0.129 | 2.393 / 2.701 / 2.684 | 0.79 / 0.79 |
| `dump_long_loop` | 36.406 / 46.043 / 44.024 | 4.505 / 6.914 / 6.698 | 8.696 / 6.972 / 6.611 | 0.79 / 0.83 |

The two-operand `byte_concat` workload retains its pairwise opcode. This change
does not optimize every concatenation or establish a uniform speedup. Other
workloads and all frontend costs remain in the raw data.

Control timings also moved: `byte_concat`, `function_dump` and `dump_long_loop`
took longer in both final runs. The ordered runs on a shared host do not isolate
the cause. Unchanged pairwise instruction/allocation counts do not establish
unchanged throughput, and these measurements do not prove a regression-free
improvement across workloads.

Rust allocation counters per complete script, ranges across execution samples:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `concat_chain_16` | before | 6,000.000 | 3,000.000 | 864,000.000 |
| `concat_chain_16` | first | 0.000 | 0.000 | 0.000 |
| `concat_chain_16` | confirmation | 0.000 | 0.000 | 0.000 |
| `concat_chain_128` | before | 76,200.000 | 25,400.000 | 79,248,000.000 |
| `concat_chain_128` | first | 0.000 | 0.000 | 0.000 |
| `concat_chain_128` | confirmation | 0.000 | 0.000 | 0.000 |
| `byte_concat` | before | 2,999.333-2,999.533 | 999.000 | 2,001,384.333-2,002,203.533 |
| `byte_concat` | first | 2,999.333-2,999.533 | 999.000 | 2,001,384.333-2,002,203.533 |
| `byte_concat` | confirmation | 2,999.333-2,999.533 | 999.000 | 2,001,384.333-2,002,203.533 |

These GlobalAlloc counters exclude native C allocations and allocator metadata.
Requested bytes include repeated requests, not live memory. The compiler may
retain unused interned GC-map entries; no claim of minimal stored metadata is made.
All 86 workload entry-register bounds are unchanged, and none of their instruction
counts increases. Selected shared-VM instruction/register layouts, before / final:

| Workload | Instructions | Entry registers |
| --- | ---: | ---: |
| `concat_chain_16` | 45 / 31 | 22 / 22 |
| `concat_chain_128` | 269 / 143 | 134 / 134 |
| `byte_concat` | 15 / 15 | 7 / 7 |

`Value` remains 16 bytes and the x86-64 frame remains 64 bytes. First-final-run
heap observations after samples and explicit full collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `concat_chain_16` | before_collect | 20,764 | 26,172 | 26,172 | 0 | 0 |
| `concat_chain_16` | after_collect | 25,932 | 31,372 | 31,372 | 1 | 1 |
| `concat_chain_128` | before_collect | 31,964 | 37,372 | 37,372 | 0 | 0 |
| `concat_chain_128` | after_collect | 35,340 | 40,780 | 40,780 | 1 | 1 |
| `byte_concat` | before_collect | 42,644 | 54,996 | 79,296 | 1971 | 150833 |
| `byte_concat` | after_collect | 26,508 | 39,116 | 79,296 | 1972 | 150849 |

Capacity estimates exclude VM headers, OS/CRT storage, transient compiler/export
buffers and allocator overhead. They are not exact allocator peaks or process RSS.
Counts include warmup and are cumulative. Startup/drop counts and all confirmation
snapshots remain in the raw data. Native C allocation counts, process peak RSS,
CPU cycles, other Rust engines and wider workloads remain unmeasured.

Raw data: [baseline JSON](2026-10-26-rust-lua-concat-before.json), [baseline TSV](2026-10-26-rust-lua-concat-before.tsv),
[baseline build log](2026-10-26-rust-lua-concat-before.build.log), [final JSON](2026-10-26-rust-lua-concat.json),
[final TSV](2026-10-26-rust-lua-concat.tsv), [final build log](2026-10-26-rust-lua-concat.build.log),
[confirmation JSON](2026-10-26-rust-lua-concat-confirm.json), [confirmation TSV](2026-10-26-rust-lua-concat-confirm.tsv),
[confirmation build log](2026-10-26-rust-lua-concat-confirm.build.log).
The [validation report](../testing/2026-10-26-rust-lua-concat.md) records 398 tests, 140 process probes
and 15/16 isolated official-file passes in both engines. Broader standalone
compatibility and the full engine/integration objective remain open.
