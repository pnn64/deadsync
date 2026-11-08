# Native Lua 5.1 instruction bridge measurements

The harness measures 57 runtime and twenty-two frontend workloads in two runs,
with five alternating paired Rust/C samples for execution. The oracle is the
vendored official Lua 5.1.5 build on Windows x86-64 MSVC, static CRT, binary64
and initial C locale. The shared host is not CPU-isolated. These measurements
do not establish a paired timing change against the previous commit.

Three new workloads import official native chunks: `native_chunk_numeric`
sums 1..20,000; `native_chunk_captures` calls a captured accumulator 2,000 times;
`native_chunk_tables` builds and iterates a 500-element table. Official dumping,
loading/translation, scalar verification and warmup happen outside execution
timing. Each execution sample runs 100 complete scripts. The same native bytes
are loaded by both engines. Separate `compile_drop` measurements include native
decoding, structural verification, translation, IR verification and Rust drop;
the C side includes native loading and its own cleanup behavior. Each uses
1,000 iterations. These loader measurements are single samples per run and
should not be interpreted as paired statistical estimates.

Execution durations are milliseconds per script, first run / confirmation.
Durations are medians; ratios are medians of the five paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `native_chunk_numeric` | 0.419 / 0.349 | 0.156 / 0.143 | 2.646 / 2.457 |
| `native_chunk_captures` | 0.298 / 0.264 | 0.096 / 0.075 | 3.056 / 3.560 |
| `native_chunk_tables` | 0.112 / 0.097 | 0.042 / 0.034 | 2.771 / 2.930 |
| `numeric_for` | 3.442 / 3.237 | 0.799 / 0.733 | 4.269 / 4.214 |
| `closure_calls` | 1.675 / 1.567 | 0.444 / 0.418 | 3.771 / 3.948 |
| `reader_load_single` | 1.541 / 1.505 | 0.544 / 0.514 | 2.867 / 2.930 |
| `binary_header_reject` | 1.095 / 1.093 | 2.138 / 2.148 | 0.509 / 0.513 |

All three imported execution workloads remain slower than C in both runs.
Paired duration ratios span 2.46–3.56. The differences between runs are retained;
the host and these few scripts do not establish general execution speed.

Native load/drop durations are microseconds per imported chunk:

| Workload | Rust us | C Lua 5.1 us |
| --- | ---: | ---: |
| `native_chunk_numeric` | 5.9 / 5.4 | 1.9 / 2.0 |
| `native_chunk_captures` | 13.8 / 12.0 | 3.1 / 2.6 |
| `native_chunk_tables` | 11.9 / 9.7 | 2.8 / 2.5 |

The bridge preserves one runtime/object model and translates constant operands
once into register loads. Conditional helpers do not add Lua hook dispatches;
native range concatenation batches contiguous strings and resumes callbacks
through the existing scheduler. Capture bindings and source names are immutable
program metadata. Two scratch registers are reserved for imported prototypes;
liveness masks prevent dead scratch values and stale caller argument copies
from retaining objects. Source-compiled program instruction/register layouts
are unchanged across every previous fixture. Prototype/debug headers grow to
hold optional native metadata and binding/source vectors, including for source
programs; the empty vectors add no allocations. `Value` remains sixteen bytes
and the x86-64 frame assertion remains 64 bytes.

Rust allocation counters per complete imported script (ranges cover samples):

| Workload | Run | Allocations | Reallocations | Requested bytes |
| --- | --- | ---: | ---: | ---: |
| `native_chunk_numeric` | first | 0.000 | 0.000 | 0.000 |
| `native_chunk_numeric` | confirmation | 0.000 | 0.000 | 0.000 |
| `native_chunk_captures` | first | 1.000–1.030 | 0.000–0.230 | 4.000–518.800 |
| `native_chunk_captures` | confirmation | 1.000–1.030 | 0.000–0.230 | 4.000–518.800 |
| `native_chunk_tables` | first | 1.000–1.020 | 7.000–7.060 | 16,320.000–16,424.720 |
| `native_chunk_tables` | confirmation | 1.000–1.020 | 7.000–7.060 | 16,320.000–16,424.720 |

Counters cover Rust `GlobalAlloc`, excluding C native allocations and allocator
metadata. They count all requested bytes, including repeated allocations, and
are not live memory. The native chunk loader is not yet optimized for large
debug/call-provenance corpora or one-byte binary reader throughput.

First-run heap observations after all samples and explicit full collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `native_chunk_numeric` | before_collect | 18,707 | 24,147 | 24,147 | 0 | 0 |
| `native_chunk_numeric` | after_collect | 24,147 | 29,587 | 29,587 | 1 | 0 |
| `native_chunk_captures` | before_collect | 41,779 | 70,371 | 70,371 | 1 | 512 |
| `native_chunk_captures` | after_collect | 25,119 | 71,439 | 71,439 | 2 | 1002 |
| `native_chunk_tables` | before_collect | 66,704 | 71,736 | 71,736 | 99 | 496 |
| `native_chunk_tables` | after_collect | 25,304 | 30,776 | 71,736 | 100 | 501 |

These are capacity-based VM estimates, excluding VM headers, native storage
and allocator overhead. They are not exact allocator peaks or process RSS.
Object counts include warmup and are cumulative. Three standard-stream userdata
remain. Confirmation-run heap observations are retained in the raw JSON.

Both runs have source digest `75eb1f2c933b8ea6e5f8040d689c0d699918408c6a81965033f4a1403edf1406`; all recorded hashes match
the final source tree. Raw data: [JSON](2026-10-25-rust-lua-chunk-exec.json), [TSV](2026-10-25-rust-lua-chunk-exec.tsv),
[build log](2026-10-25-rust-lua-chunk-exec.build.log), [confirmation JSON](2026-10-25-rust-lua-chunk-exec-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-chunk-exec-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-chunk-exec-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-chunk-exec.md) records 380 tests and 132 process probes.

Binary dump emission, persistent caches, native modules, broader resource,
collector and diagnostic coverage, Lua 5.2–5.5, host APIs, optional JIT and
integrations remain unfinished. The standalone-engine objective remains open.
