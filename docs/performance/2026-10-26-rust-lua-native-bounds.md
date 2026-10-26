# Lua 5.1 native result and integer measurements

Two runs use five alternating paired samples across **72 runtime and 22 frontend
workloads** on shared Windows x86-64 MSVC hardware, static CRT, binary64 and C locale.
The official 5.1.5 development binding is the reference. The shared host is not
CPU-isolated; no before/after optimization or cross-engine ranking is established.

Six new workloads measure unpack widths 4/512/7,999, byte width 7,997, native integer
narrowing and catchable result-limit errors. Unpack/byte scripts perform 100 native
calls per script and count their results through a Lua vararg function. Input table
or string construction occurs during the excluded warmup; local counting functions,
host entry/return, result copying and Lua/C adapters remain included. The integer
script makes 1,000 char/rep/select triplets, and the error script makes 1,000 protected
unpack failures. Compilation is separate. These are complete scripts, not isolated
raw table-access or dispatch timings.

Median nanoseconds per complete call/script, first / confirmation; ratios are
medians of five paired Rust/C durations:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `unpack_4` | 42,540 / 44,675 | 19,005 / 20,515 | 2.253 / 2.228 |
| `unpack_512` | 541,050 / 502,240 | 413,000 / 401,310 | 1.309 / 1.249 |
| `unpack_7999` | 7,725,970 / 7,431,280 | 5,659,430 / 5,728,210 | 1.334 / 1.307 |
| `byte_7997` | 2,193,710 / 1,915,060 | 4,468,530 / 3,950,230 | 0.491 / 0.493 |
| `native_int_narrow` | 597,550 / 565,400 | 202,255 / 182,765 | 2.954 / 3.093 |
| `native_result_error` | 527,700 / 506,595 | 2,426,500 / 2,359,735 | 0.217 / 0.214 |
| `entry_args` | 148 / 144 | 141 / 163 | 1.051 / 0.939 |

Rust GlobalAlloc events per complete call/script, ranges across five samples:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `unpack_4` | first | 0.000 | 0.000-0.300 | 0.000-281.600 |
| `unpack_4` | confirmation | 0.000 | 0.000-0.300 | 0.000-281.600 |
| `unpack_512` | first | 0.000 | 0.000-0.400 | 0.000-281.600 |
| `unpack_512` | confirmation | 0.000 | 0.000-0.400 | 0.000-281.600 |
| `unpack_7999` | first | 0.000 | 0.000-0.500 | 0.000-310.400 |
| `unpack_7999` | confirmation | 0.000 | 0.000-0.500 | 0.000-310.400 |
| `byte_7997` | first | 0.000-0.100 | 0.000-0.500 | 0.000-312.000 |
| `byte_7997` | confirmation | 0.000-0.100 | 0.000-0.500 | 0.000-312.000 |
| `native_int_narrow` | first | 0.000 | 0.000 | 0.000 |
| `native_int_narrow` | confirmation | 0.000 | 0.000 | 0.000 |
| `native_result_error` | first | 1,000.000 | 0.000-0.100 | 26,000.000-26,588.800 |
| `native_result_error` | confirmation | 1,000.000 | 0.000-0.100 | 26,000.000-26,588.800 |
| `entry_args` | first | 0.000 | 0.000 | 0.000 |
| `entry_args` | confirmation | 0.000 | 0.000 | 0.000 |

Counts exclude C malloc and allocator metadata. Requested bytes are allocation
requests, not live memory; zero storage events do not imply zero logical object
creation. Native argument narrowing precedes string repetition sizing. Result
checks precede register growth, and unpack borrows a checked table for its loop.
No VM-header, frame, instruction or value field is added. Value remains 16 B;
the x86-64 frame remains 64 B. All 88 previous instruction/register layouts match.

First-run heap snapshots after samples and explicit collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections |
| --- | --- | ---: | ---: | ---: | ---: |
| `unpack_4` | before_collect | 24,928 | 31,664 | 31,664 | 0 |
| `unpack_4` | after_collect | 26,328 | 37,616 | 37,616 | 1 |
| `unpack_512` | before_collect | 47,376 | 53,296 | 53,296 | 0 |
| `unpack_512` | after_collect | 50,776 | 58,992 | 58,992 | 1 |
| `unpack_7999` | before_collect | 415,280 | 421,200 | 421,200 | 1 |
| `unpack_7999` | after_collect | 413,240 | 421,456 | 421,456 | 2 |
| `byte_7997` | before_collect | 300,146 | 306,178 | 306,178 | 1 |
| `byte_7997` | after_collect | 298,146 | 306,418 | 306,418 | 2 |
| `native_int_narrow` | before_collect | 19,990 | 25,382 | 25,382 | 0 |
| `native_int_narrow` | after_collect | 25,394 | 30,834 | 30,834 | 1 |
| `native_result_error` | before_collect | 28,858 | 34,226 | 34,226 | 0 |
| `native_result_error` | after_collect | 25,410 | 40,178 | 40,178 | 1 |
| `entry_args` | before_collect | 18,387 | 23,827 | 23,827 | 0 |
| `entry_args` | after_collect | 23,827 | 29,267 | 29,267 | 1 |

These capacity estimates exclude VM headers, transient compiler/export buffers,
OS/CRT memory and allocator metadata; they are not process RSS or exact allocator
peaks. Counts are cumulative and include warmup. Confirmation snapshots and all
other workload samples remain in JSON. C allocation counts, process RSS, CPU cycles,
other Rust engines and broad native-call/resource scaling remain unmeasured.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-native-bounds.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-native-bounds-confirm.json
```

Raw data: [JSON](2026-10-26-rust-lua-native-bounds.json), [TSV](2026-10-26-rust-lua-native-bounds.tsv), [build log](2026-10-26-rust-lua-native-bounds.build.log),
[confirmation JSON](2026-10-26-rust-lua-native-bounds-confirm.json), [confirmation TSV](2026-10-26-rust-lua-native-bounds-confirm.tsv),
[confirmation build log](2026-10-26-rust-lua-native-bounds-confirm.build.log). Both source digests are
cea26840585b0a92f45f0ccbeae9803920090936ce8bc2a4fecd1afa3e2de1ee; all recorded final hashes match the tree. Binary hashes
identify the executables observed during their respective audits; later Cargo
builds can replace local executables. The [validation report](../testing/2026-10-26-rust-lua-native-bounds.md)
retains 422 tests, 140 process passes, 17/18 official files in both engines and
88/126 raw CLI matches (126/126 after only known leading banner removal).
Complete standalone compatibility and the engine/integration objective remain open.
