# Lua binary decoding stage measurements

The harness retains 54 runtime and twenty-two frontend workloads, five
alternating paired Rust/C samples and a separate confirmation run. The oracle
is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT, binary64 and
initial C locale. The shared host is not CPU-isolated. No paired parent timing
claim is made.

`binary_header_reject` constructs a twelve-byte native-shaped header with an
invalid version. One thousand calls to `loadstring` must return nil plus the
exact `binary: bad header in precompiled chunk` diagnostic. Ten scripts run per
sample. Compilation and scalar checks occur outside timing; header construction,
all loader calls, diagnostic construction/interning and interpreted control flow
remain inside. This measures malformed-header rejection, not prototype decoding,
instruction translation or binary execution.

Durations are milliseconds per complete script, first run / confirmation.
Engine durations are medians; ratios are medians of the five paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `binary_header_reject` | 1.086 / 1.075 | 2.256 / 2.048 | 0.473 / 0.511 |
| `reader_load_single` | 1.400 / 1.391 | 0.547 / 0.497 | 2.665 / 2.804 |
| `reader_load_bytes` | 3.412 / 2.776 | 1.080 / 0.933 | 3.207 / 3.093 |
| `gc_steps` | 0.382 / 0.370 | 0.343 / 0.351 | 1.163 / 1.105 |
| `string_format` | 0.806 / 0.872 | 0.737 / 0.678 | 1.106 / 1.301 |

Header rejection is faster than C in this workload. Reader and other controls
remain slower. Neither the narrow rejection result nor historical samples
establish general speed or binary-loader parity. Full prototype decoding,
one-byte binary-reader scaling and per-field latency are not timed here.

Every rejection script requests 5,002 Rust allocations, 4,001 reallocations and
251,024 bytes in both runs. Temporary source/name buffers, the decoder task
vector and diagnostic buffers contribute. Counters cover Rust `GlobalAlloc`;
native C allocations and allocator metadata are excluded. No dependency or
unsafe code is added. Decoder storage is proportional to materialized input
and pending nesting, rather than untrusted declared vector sizes. Production
decoding and prototype traversal use flat vectors; only native binary readers
instantiate the optional decoder. Loading jobs also retain saved depth/mode
state, adding per-job storage for text loaders.

Startup/drop remains 243 allocations, 63 reallocations and 39,651 bytes per Rust
state. Calendar results remain 2,000 allocations, zero reallocations and 352,000
bytes per 1,000-table script. All 75 prior program layouts match; the new fixture
emits 41 instructions and uses ten entry registers. `Value` remains sixteen
bytes and the x86-64 frame-size assertion remains 64 bytes.

Heap observations match across both runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `binary_header_reject` | before_collect | 20,450 | 25,810 | 25,810 | 0 | 0 |
| `binary_header_reject` | after_collect | 25,838 | 31,246 | 31,246 | 1 | 2 |
| `reader_load_bytes` | before_collect | 40,174 | 45,806 | 47,163 | 509 | 32629 |
| `reader_load_bytes` | after_collect | 29,774 | 37,886 | 47,163 | 510 | 32694 |

Snapshots follow all samples and explicit full collection; counts include
warmup and are cumulative. Three standard-stream userdata remain. First
collection can grow retained GC work buffers despite reclaiming objects.
These capacity estimates exclude VM headers, native storage and allocator
overheads; they are not exact live/peak allocator usage or RSS.

Both benchmark runs have source digest `879726bcbf1938d3d84a77bd6140bb0f20ecfc2a17aa07ef71319bc7ba8fc17a`; all
82 recorded source/configuration hashes match the final tree.
Raw data: [JSON](2026-10-25-rust-lua-chunk-codec.json), [TSV](2026-10-25-rust-lua-chunk-codec.tsv), [build log](2026-10-25-rust-lua-chunk-codec.build.log),
[confirmation JSON](2026-10-25-rust-lua-chunk-codec-confirm.json), [confirmation TSV](2026-10-25-rust-lua-chunk-codec-confirm.tsv),
[confirmation build log](2026-10-25-rust-lua-chunk-codec-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-chunk-codec.md) records 375 tests and 129 process probes.

Valid precompiled chunks still report unsupported execution; native instruction
translation and dump emission are required next. The persistent compiled cache,
native modules, wider platform/resource coverage, remaining GC/diagnostic work,
host APIs, later dialects, optional JIT and integrations remain unfinished.
