# Lua 5.1 long-branch export measurements

The harness retains five alternating paired samples per workload and a separate
confirmation run over 62 runtime and 22 frontend workloads. The oracle is vendored
official Lua 5.1.5 on Windows x86-64 MSVC, static CRT, binary64 and initial C locale.
The shared host is not CPU-isolated. No paired timing comparison against the
previous commit is established.

Two additions prepare a 120,000-increment function outside execution timing:
`dump_long_skip` skips its body and returns zero; `dump_long_loop` executes two
numeric-for iterations and returns 240,000. Each timed script exports, reloads
and calls that function. Each paired sample repeats the script twice. Rust and
C use the same source name; Rust's native expansion produces a longer instruction
stream. This measures complete export/load/execute paths, not equal-size copying.
Warmup and scalar validation precede timing. Compilation of the prepared large
function is excluded, while export scratch allocation and imported verification
and translation are included.

Durations are milliseconds per complete script, first run / confirmation.
Durations are medians; ratios are medians of the five paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `dump_long_skip` | 45.167 / 39.046 | 6.376 / 5.627 | 7.108 / 7.003 |
| `dump_long_loop` | 46.367 / 41.991 | 6.899 / 5.977 | 6.588 / 6.810 |
| `function_dump` | 0.362 / 0.287 | 0.143 / 0.110 | 2.638 / 2.610 |
| `native_redump` | 0.173 / 0.167 | 0.104 / 0.103 | 1.489 / 1.520 |
| `dump_load` | 2.257 / 2.081 | 0.444 / 0.380 | 4.915 / 5.136 |
| `native_chunk_numeric` | 0.451 / 0.420 | 0.173 / 0.173 | 2.580 / 2.446 |

Source export builds temporary native words, lines, source-PC maps and patch
vectors. Overflowing functions additionally allocate batched relocation buffers,
safe-cut maps, relay edges and split local ranges. Short functions bypass the
relay pass; source export still scans capture bindings to remove ineffective
CLOSE instructions. The transient serializer task now carries an optional local
range vector. These costs are measured, not assumed free. No new fields are added
to stored VM values or frames: `Value` remains 16 bytes and the x86-64 frame remains
64 bytes. All 82 previous source instruction/register layouts remain unchanged.

Rust allocation counters per complete script (ranges span execution samples):

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `dump_long_skip` | first | 71.000-72.000 | 113.000-115.500 | 62,498,431.000-62,499,807.000 |
| `dump_long_skip` | confirmation | 71.000-72.000 | 113.000-115.500 | 62,498,431.000-62,499,807.000 |
| `dump_long_loop` | first | 111.000-112.000 | 123.000-125.000 | 47,147,926.000-47,148,630.000 |
| `dump_long_loop` | confirmation | 111.000-112.000 | 123.000-125.000 | 47,147,926.000-47,148,630.000 |
| `function_dump` | first | 1,444.000-1,444.300 | 7.000-8.400 | 103,634.000-104,826.800 |
| `function_dump` | confirmation | 1,444.000-1,444.300 | 7.000-8.400 | 103,634.000-104,826.800 |
| `native_redump` | first | 444.000-444.300 | 7.000-8.400 | 81,834.000-83,026.800 |
| `native_redump` | confirmation | 444.000-444.300 | 7.000-8.400 | 81,834.000-83,026.800 |
| `dump_load` | first | 7,819.000 | 3,503.000 | 1,097,958.000 |
| `dump_load` | confirmation | 7,819.000 | 3,503.000 | 1,097,958.000 |

Counters cover Rust GlobalAlloc, excluding native C allocations and allocator
metadata. Requested bytes include repeated requests, not live memory. Serialized
code is not cached. Imported debug ranges include the split relay scopes when
the result is loaded; ordinary source programs retain their existing metadata.

First-run heap observations after samples and explicit full collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `dump_long_skip` | before_collect | 40,353,030 | 40,359,078 | 47,080,021 | 2 | 13 |
| `dump_long_skip` | after_collect | 4,825,516 | 4,832,060 | 47,080,021 | 3 | 24 |
| `dump_long_loop` | before_collect | 53,808,710 | 53,814,582 | 53,814,582 | 2 | 9 |
| `dump_long_loop` | after_collect | 4,825,877 | 4,832,389 | 53,814,582 | 3 | 24 |
| `function_dump` | before_collect | 34,469 | 47,677 | 66,252 | 1 | 126 |
| `function_dump` | after_collect | 26,459 | 40,779 | 66,252 | 2 | 154 |
| `native_redump` | before_collect | 33,928 | 47,136 | 65,711 | 1 | 126 |
| `native_redump` | after_collect | 25,918 | 40,238 | 65,711 | 2 | 154 |
| `dump_load` | before_collect | 35,133 | 45,429 | 77,374 | 425 | 45932 |
| `dump_load` | after_collect | 27,952 | 39,040 | 77,374 | 426 | 45952 |

Capacity estimates exclude VM headers, OS/CRT storage, transient export buffers
and allocator overhead; they are not exact allocator peaks or process RSS.
Counts include warmup and are cumulative. Confirmation snapshots and startup/drop
measurements remain in the raw data. Native C allocation counts, process peak RSS,
CPU cycles, other Rust engines and wider workloads remain unmeasured.

Both runs have source digest `c3632d56760ac7c95592cb0ab8f04863ccb4681f8fdeb748af2ac29b3ea20709`; all recorded hashes match
the final tree. Raw data: [JSON](2026-10-25-rust-lua-chunk-jumps.json), [TSV](2026-10-25-rust-lua-chunk-jumps.tsv),
[build log](2026-10-25-rust-lua-chunk-jumps.build.log), [confirmation JSON](2026-10-25-rust-lua-chunk-jumps-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-chunk-jumps-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-chunk-jumps-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-chunk-jumps.md) records 393 tests, 138 process probes
and 15/16 isolated official-file passes in both engines. Broader native encoding,
debug/collector compatibility and the remaining standalone-engine gates stay open.
