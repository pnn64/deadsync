# Lua 5.1 debug command console measurements

Two complete-process runs use five alternating paired samples of ten workloads
on shared Windows x64 MSVC hardware, static CRT, binary64 and C locale. They
include process creation, VM startup, command compilation/execution, pipes and
shutdown. Outputs are independently checked before and after timing. Four new
fixtures run 1/100/1000 successful debug commands or 100 failing commands, then
resume the caller. The official Lua 5.1.5 CLI is the pinned reference. Ordinary
interactive fixtures remove only their known leading banner for validation;
all new debug fixtures match raw bytes.

Median milliseconds per complete process, first / confirmation; ratios are
medians of five paired Rust/C durations:

| Workload | Rust ms | C CLI ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `empty_command` | 12.926 / 13.030 | 12.092 / 12.052 | 1.063 / 1.105 |
| `console_1000` | 35.376 / 35.768 | 18.710 / 19.163 | 1.852 / 1.865 |
| `debug_console_1` | 13.052 / 13.373 | 11.924 / 12.288 | 1.079 / 1.074 |
| `debug_console_100` | 15.176 / 15.596 | 12.387 / 12.520 | 1.225 / 1.244 |
| `debug_console_1000` | 30.982 / 32.319 | 16.979 / 17.636 | 1.879 / 1.877 |
| `debug_console_errors_100` | 16.086 / 16.345 | 13.430 / 13.496 | 1.193 / 1.211 |

Process measurements do not isolate command execution or establish a before/after
speedup. The host is not CPU-isolated. Spawn and pipe costs contribute materially,
especially for short fixtures. Process allocation counts, RSS, cycles, terminal
echo and signals are unmeasured.

Two allocation-instrumented VM runs retain 78 runtime and 22 frontend workloads,
five paired samples each. Selected medians below are nanoseconds per complete
call/script, first / confirmation; C timings include the development binding:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `entry_args` | 185 / 143 | 173 / 132 | 1.095 / 1.068 |
| `debug_sort_args` | 163,310 / 127,555 | 79,185 / 63,565 | 2.066 / 2.005 |
| `debug_foreach_args` | 157,895 / 118,720 | 73,150 / 57,010 | 2.162 / 2.071 |
| `table_sort` | 6,897,395 / 7,677,335 | 2,925,040 / 3,097,815 | 2.358 / 2.537 |

The new console uses the existing frame/register vectors, a 249-byte stack input
buffer and shared compilation/input helpers. It adds no VM, frame, value or
instruction fields. VM metadata remains 4,432 B, Value 16 B and the x64 frame
64 B. All 100 previous program instruction/register layouts match.
Compilation still allocates per debug command; this is not allocation-free
execution. Borrowed string diagnostics and prompts need no temporary byte buffer.

Rust startup/drop measurements instrument 1,000 complete Vm::new/drop operations
per run. Each row is one aggregate sample, not five paired startup samples:

| Run | ns/VM | Allocations/VM | Reallocations/VM | Requested B/VM |
| --- | ---: | ---: | ---: | ---: |
| first | 60,552 | 243 | 63 | 39,651 |
| confirmation | 56,792 | 243 | 63 | 39,651 |

GlobalAlloc counts omit C malloc and allocator metadata; requested bytes are not
live memory. Runtime sample allocation/reallocation/byte totals are in JSON/TSV.
Entry-argument workload VM capacity snapshots are:

| Run | Phase | Live B | Reserved B | Observed peak B |
| --- | --- | ---: | ---: | ---: |
| first | before_collect | 18,387 | 23,827 | 23,827 |
| first | after_collect | 23,827 | 29,267 | 29,267 |
| confirmation | before_collect | 18,387 | 23,827 | 23,827 |
| confirmation | after_collect | 23,827 | 29,267 | 29,267 |

These estimates exclude VM headers, transient compiler/export buffers, OS/CRT
storage and allocator metadata. They are not process RSS or exact allocator peaks;
all other heap snapshots and collected C-heap observations remain in JSON.
Broader native/host scaling and other Rust engines still require measurements.

```sh
python scripts/benchmark_rust_lua_cli.py --output docs/performance/2026-10-26-rust-lua-debug-console-process.json
python scripts/benchmark_rust_lua_cli.py --output docs/performance/2026-10-26-rust-lua-debug-console-process-confirm.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-debug-console.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-debug-console-confirm.json
```

Raw data: [process JSON](2026-10-26-rust-lua-debug-console-process.json), [process confirmation](2026-10-26-rust-lua-debug-console-process-confirm.json),
[process build](2026-10-26-rust-lua-debug-console-process.build.log), [process confirmation build](2026-10-26-rust-lua-debug-console-process-confirm.build.log),
[VM JSON](2026-10-26-rust-lua-debug-console.json), [VM TSV](2026-10-26-rust-lua-debug-console.tsv), [VM build](2026-10-26-rust-lua-debug-console.build.log),
[VM confirmation JSON](2026-10-26-rust-lua-debug-console-confirm.json), [VM confirmation TSV](2026-10-26-rust-lua-debug-console-confirm.tsv),
[VM confirmation build](2026-10-26-rust-lua-debug-console-confirm.build.log). Both VM runs have source digest
66e16588829d482772071d963f0e582133acc66b7c17fc7b44d9c457b0be6f9c. All seven recorded source-hash maps match the final tree.
Executable hashes identify observed builds, which later Cargo invocations may
replace. The [validation report](../testing/2026-10-26-rust-lua-debug-console.md) retains all failures and
contract limits. The complete engine/integration goal remains open.
