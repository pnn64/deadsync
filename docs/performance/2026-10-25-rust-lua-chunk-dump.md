# Lua 5.1 function dump measurements

The harness retains 60 runtime and twenty-two frontend workloads, five
alternating paired execution samples and a separate confirmation run. The
oracle is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT,
binary64 and initial C locale. The shared host is not CPU-isolated. No paired
timing change against the previous commit is established.

Three workloads are added:

- `function_dump`: 200 exports of a small source-compiled arithmetic function,
  with signature-byte checks and one final load/call validation.
- `native_redump`: one source export/load, followed by 200 exports of the
  imported function; every result must equal that engine's first byte stream.
- `dump_load`: one export of an iterator/nested-closure function followed by
  100 load/execute cycles. Each call's scalar result contributes to a checked sum.

Each paired sample runs ten complete scripts. Function creation, dump/load
setup, output interning, checks and interpreted loops remain inside script
timing. Scalar verification and warmup precede timing. Both engines explicitly
use `=(dump-bench)`, avoiding different source-name sizes in exported metadata.
Their source-compiled native instructions and output lengths can still differ.
The benchmark therefore measures complete implementations, not equal-size
buffer copying. Native re-export separately uses each engine's own stream.

Durations are milliseconds per script, first run / confirmation. Durations are
medians; ratios are medians of the five paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `function_dump` | 0.326 / 0.341 | 0.120 / 0.131 | 2.513 / 2.587 |
| `native_redump` | 0.181 / 0.173 | 0.101 / 0.106 | 1.465 / 1.453 |
| `dump_load` | 2.211 / 2.269 | 0.440 / 0.433 | 4.958 / 5.216 |
| `debug_hook` | 0.098 / 0.098 | 0.044 / 0.039 | 2.311 / 2.311 |
| `native_chunk_numeric` | 0.437 / 0.406 | 0.164 / 0.163 | 2.667 / 2.451 |
| `reader_load_single` | 1.674 / 1.705 | 0.566 / 0.606 | 2.922 / 2.817 |

The new dump workloads remain slower than C. Source lowering allocates temporary
word/line/patch/scope data for each export; native re-export bypasses lowering
and reads retained native words/debug offsets. Serialized code is not cached.
Both paths allocate an output buffer before interning it. Further optimization
requires profiling and equivalent-behavior checks rather than treating these
ratios as a general engine speed result.

Rust allocation counters per complete script (ranges span execution samples):

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `function_dump` | first | 1,444.000–1,444.300 | 7.000–8.400 | 98,810.000–100,002.800 |
| `function_dump` | confirmation | 1,444.000–1,444.300 | 7.000–8.400 | 98,810.000–100,002.800 |
| `native_redump` | first | 444.000–444.300 | 7.000–8.400 | 77,010.000–78,202.800 |
| `native_redump` | confirmation | 444.000–444.300 | 7.000–8.400 | 77,010.000–78,202.800 |
| `dump_load` | first | 7,818.000 | 3,503.000 | 1,097,806.000 |
| `dump_load` | confirmation | 7,818.000 | 3,503.000 | 1,097,806.000 |
| `debug_hook` | first | 52.000–52.400 | 35.000–37.100 | 10,094.000–12,258.000 |
| `debug_hook` | confirmation | 52.000–52.400 | 35.000–37.100 | 10,094.000–12,258.000 |

Counters cover Rust `GlobalAlloc`, excluding native C allocations and allocator
metadata. Requested bytes include repeated allocations, not live memory. Rust
startup/drop counters (allocations, reallocations, bytes) are `(243.0, 63.0, 39651.0)`
and `(243.0, 63.0, 39651.0)` in the two runs. Source-program instruction/register layouts
match all previous fixtures. `Value` remains sixteen bytes; the x86-64 frame
assertion remains 64 bytes.

Imported programs retain native code/line/local/child vectors for export, so
their resident code metadata grows. Source prototypes retain the empty optional
native field. The registry hook table replaces a private Rust map and uses the
normal table allocator/barriers; it is created lazily by hook APIs and remains
registered after callbacks are removed, matching the reference's ownership.

First-run heap observations after samples and explicit full collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `function_dump` | before_collect | 34,469 | 47,677 | 66,252 | 1 | 126 |
| `function_dump` | after_collect | 26,459 | 40,779 | 66,252 | 2 | 154 |
| `native_redump` | before_collect | 33,928 | 47,136 | 65,711 | 1 | 126 |
| `native_redump` | after_collect | 25,918 | 40,238 | 65,711 | 2 | 154 |
| `dump_load` | before_collect | 35,133 | 45,429 | 77,374 | 425 | 45932 |
| `dump_load` | after_collect | 27,952 | 39,040 | 77,374 | 426 | 45952 |
| `debug_hook` | before_collect | 48,786 | 61,786 | 68,562 | 1 | 128 |
| `debug_hook` | after_collect | 26,118 | 42,398 | 68,562 | 2 | 205 |

These capacity estimates exclude VM headers, OS/CRT storage, transient export
buffers and allocator overhead; they are not exact allocator peaks or process RSS. Counts include
warmup and are cumulative. Three standard-stream userdata remain. Confirmation
heap snapshots are retained in the raw data.

Both runs have source digest `ae5e5ac364fc9140502251094a919fa7b61a2d71e07e3d32541e238f3b801caf`; all recorded hashes match
the final source tree. Raw data: [JSON](2026-10-25-rust-lua-chunk-dump.json), [TSV](2026-10-25-rust-lua-chunk-dump.tsv),
[build log](2026-10-25-rust-lua-chunk-dump.build.log), [confirmation JSON](2026-10-25-rust-lua-chunk-dump-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-chunk-dump-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-chunk-dump-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-chunk-dump.md) records 388 tests, 136 process
probes and 15/16 isolated official-file passes in both engines.

Broader encoding/resource limits, debug/collector equivalence, cross-platform
runtime validation, persistent caching, native modules, later dialects, host
APIs, optional JIT and integrations remain unfinished.
