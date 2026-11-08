# Resumable parsing and reader loading measurements

The harness now has 22 paired execution workloads and six frontend compilation
workloads. Two new scripts repeatedly compile and execute text using `load`:
`reader_load_single` loads `return 1+2` 200 times per invocation, and
`reader_load_bytes` loads `local t={x=7};return t.x*3` 100 times with one-byte reader
results. Both engines check the numeric results before timing. Each timing row
contains ten script invocations for these workloads, and each workload has five
paired samples in alternating engine order. Automatic collection stays enabled.

The reference is vendored official Lua 5.1.5 on Windows x86-64 MSVC, static CRT,
C locale. The machine is not CPU-isolated. Ratios below are medians of paired
Rust/reference wall times; lower is better. Each JSON retains source hashes,
compiler/machine metadata, samples and matching TSV/build logs. The runner rejects
source changes during a measurement. Both final reports and the official-suite
report have source hashes matching the tested implementation.

| Execution workload | First run | Confirmation |
| --- | ---: | ---: |
| `reader_load_single` | 1.979 | 2.090 |
| `reader_load_bytes` | 2.598 | 2.588 |
| `numeric_for` | 6.704 | 6.920 |
| `numeric_while` | 6.314 | 6.157 |
| `array_table` | 3.201 | 3.069 |
| `recursive_fib` | 4.345 | 4.357 |
| `closure_calls` | 3.936 | 3.840 |
| `byte_concat` | 4.970 | 4.815 |
| `pattern_find` | 0.899 | 0.914 |
| `pattern_replace` | 2.770 | 2.874 |
| `pattern_callback` | 2.901 | 2.914 |
| `string_format` | 1.156 | 1.123 |
| `table_sort` | 2.182 | 2.189 |
| `table_sort_callback` | 2.255 | 2.201 |
| `coroutine_resume` | 2.532 | 2.697 |
| `coroutine_reuse` | 3.551 | 3.577 |
| `coroutine_wrap` | 3.302 | 3.404 |
| `coroutine_deep_yield` | 2.695 | 2.740 |
| `gc_cycles` | 1.288 | 1.310 |
| `gc_weak_values` | 1.187 | 1.199 |
| `userdata_proxy` | 1.427 | 1.388 |
| `userdata_finalize` | 1.305 | 1.309 |

Single-chunk loading has median Rust script durations of 1.008/0.930 ms versus
reference 0.481/0.445 ms; one-byte loading measures 2.072/2.218 ms versus
0.787/0.857 ms. These include reader callbacks, compilation, execution, host
entry/return and automatic collection. Most execution workloads remain slower
than the reference. There is no claim of overall runtime improvement.

Fixed-source compilation uses the same flat parser as reader loading. The six
existing scaling workloads continue to compile full comments and quoted strings,
including destruction, with source construction outside measurement:

| Compilation payload | First run | Confirmation |
| --- | ---: | ---: |
| Comment, 10 KB | 0.412 | 0.425 |
| Comment, 100 KB | 0.312 | 0.303 |
| Comment, 1 MB | 0.298 | 0.304 |
| Quoted text, 10 KB | 0.297 | 0.292 |
| Quoted text, 100 KB | 0.280 | 0.274 |
| Quoted text, 1 MB | 0.298 | 0.295 |

The scanner's bulk comment/string paths retain their earlier advantage on these
payloads. These timings do not cover token-dense programs or establish complete
frontend performance parity. One-byte large-input behavior is checked separately
in the [reader validation](../testing/2026-10-25-rust-lua-reader.md).

For each 200-load single-chunk script invocation, warmed Rust allocation counts
are 4,000 allocations and 2,000 reallocations, totaling 534,000 requested bytes.
For each 100-load one-byte script invocation they are approximately 3,485–3,486
allocations, 2,100 reallocations and 636,085 requested bytes. These totals count
Rust `GlobalAlloc` requests across the complete script, including VM execution;
they are neither live bytes nor an isolated compiler cost. Direct native C
allocation calls are excluded from the instrumentation. Parser continuations
retain partial state without replaying prefixes, but scratch allocation reuse
still has substantial room for improvement.

Both runs report identical Rust reader heap estimates:

| Workload | After-collection live bytes | Reserved bytes | Observed peak reserved | Collections | Reclaimed objects |
| --- | ---: | ---: | ---: | ---: | ---: |
| Single chunk | 15,460 | 28,356 | 43,189 | 205 | 40,800 |
| One byte | 16,463 | 28,751 | 50,082 | 243 | 29,926 |

These counts cover correctness warmup and all timed invocations in the persistent
VM. Estimates include active reader source/parser state and reusable vectors;
they are capacity-based and are not allocator live/peak measurements or process
RSS. The observed peak is sampled at collector checkpoints, so it can miss
transient peaks. Values remain 16 bytes and call frames 64 bytes on this target.
Startup/drop and per-workload compilation samples are retained in the raw data;
startup opens different library sets in the two engines.

- [First final run](2026-10-25-rust-lua-reader.json)
- [Confirmation](2026-10-25-rust-lua-reader-confirm.json)

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The implementation remains safe Rust with no normal/build dependencies. Binary
loading, full library/resource compatibility, incremental GC pauses, other Rust
engine comparisons and exact allocator/RSS measurements remain unfinished.
