# Lua 5.1 console and engine measurements

Two complete-process runs and two engine runs use five alternating paired samples
on shared Windows x86-64 MSVC hardware. The host is not CPU-isolated. Comparisons
include the recorded adapters and do not establish a before/after optimization
claim, CPU-cycle equivalence or performance against other Rust Lua engines.

Complete-process timing includes spawn, startup, compilation, execution, captured
stdout/stderr pipes and shutdown. Each pair uses identical input/options and
validates outputs outside the measured interval. The official unmodified C CLI
uses /O2 /MT and the default 512-byte input buffer without readline. Only the
known leading project/version banner is removed for validation; both raw warmup
observations and banner bytes remain in JSON. Console batches recompile each
entry, invoke the handler-bearing protected driver and flush each prompt. Error
batches include traceback construction and a full collection per failure.

Median milliseconds per complete process, first / confirmation; ratios are medians
of five paired durations:

| Workload | Rust ms | C CLI ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `empty_command` | 13.200 / 12.880 | 11.868 / 11.960 | 1.118 / 1.093 |
| `stdin_source` | 13.306 / 13.301 | 11.883 / 11.921 | 1.132 / 1.105 |
| `console_1` | 13.365 / 13.336 | 12.020 / 12.109 | 1.095 / 1.103 |
| `console_100` | 15.940 / 15.634 | 12.973 / 12.829 | 1.214 / 1.240 |
| `console_1000` | 35.400 / 35.884 | 19.082 / 19.114 | 1.853 / 1.910 |
| `console_errors_100` | 22.227 / 22.220 | 16.188 / 16.540 | 1.375 / 1.347 |

The 1/100/1000 console fixtures evaluate =6*7 and print each result. The error
fixture raises 100 errors with level zero. The JSON records repetitions and
per-chunk times, which include amortized process startup and pipe/output cost;
they are not isolated interpreter timings. Process allocations, RSS, CPU cycles
and exact OS/CRT buffering overhead remain unmeasured. The console reuses three
local Vec buffers and borrows string/default prompt bytes; its transient parser,
program and reporting allocations are included in wall time, not allocator counts.

The existing engine harness covers 66 runtime and 22 frontend workloads. Median
nanoseconds per call/script, first / confirmation:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `entry_args` | 146 / 142 | 141 / 136 | 1.035 / 1.048 |
| `standalone_entry` | 239 / 243 | 131 / 135 | 1.838 / 1.750 |
| `debug_hook` | 83,150 / 82,460 | 38,590 / 38,010 | 2.155 / 2.251 |
| `dump_load` | 1,975,800 / 1,903,720 | 381,570 / 363,450 | 5.219 / 5.239 |
| `concat_chain_128` | 996,870 / 804,720 | 197,160 / 153,470 | 4.882 / 5.255 |

standalone_entry measures 10,000 three-argument calls per sample after compilation,
context creation and warmup. Each Rust call creates a logical handler closure and
includes collector safe points/entry cleanup. The C binding entry has no extra
standalone main frame; it is not an equal-context CLI comparison. entry_args has
no standalone context. All 88 prior instruction/register layouts match.
This milestone adds no VM-header, frame or instruction field; Value remains 16 B
and the x86-64 frame remains 64 B. Entry modes are a local enum and share the
existing continuations/native arena. The legacy production engine is unchanged.

Rust GlobalAlloc events per call, ranges across the five samples:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `entry_args` | first | 0.000000 | 0.000000 | 0.000000 |
| `entry_args` | confirmation | 0.000000 | 0.000000 | 0.000000 |
| `standalone_entry` | first | 0.000000-0.000200 | 0.000000-0.003500 | 0.000000-20.711200 |
| `standalone_entry` | confirmation | 0.000000-0.000200 | 0.000000-0.003500 | 0.000000-20.711200 |

Logical handler creation is distinct from GlobalAlloc storage requests. Counts
exclude C allocations and allocator metadata; requested bytes are not live memory.
First engine-run snapshots after samples and explicit collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections |
| --- | --- | ---: | ---: | ---: | ---: |
| `entry_args` | before_collect | 18,387 | 23,827 | 23,827 | 0 |
| `entry_args` | after_collect | 23,827 | 29,267 | 29,267 | 1 |
| `standalone_entry` | before_collect | 50,835 | 127,955 | 127,955 | 48 |
| `standalone_entry` | after_collect | 23,995 | 127,955 | 127,955 | 49 |

These are VM capacity estimates, excluding headers, transient compiler/export
buffers, OS/CRT memory and allocator metadata. They are not process RSS or exact
allocator peaks. Confirmation snapshots and other workloads remain in raw data.

```sh
python scripts/benchmark_rust_lua_cli.py --output docs/performance/2026-10-26-rust-lua-console-process.json
python scripts/benchmark_rust_lua_cli.py --output docs/performance/2026-10-26-rust-lua-console-process-confirm.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-console.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-console-confirm.json
```

Raw complete-process data: [first JSON](2026-10-26-rust-lua-console-process.json),
[first build log](2026-10-26-rust-lua-console-process.build.log), [confirmation JSON](2026-10-26-rust-lua-console-process-confirm.json),
[confirmation build log](2026-10-26-rust-lua-console-process-confirm.build.log).
Raw engine data: [first JSON](2026-10-26-rust-lua-console.json), [TSV](2026-10-26-rust-lua-console.tsv),
[build log](2026-10-26-rust-lua-console.build.log), [confirmation JSON](2026-10-26-rust-lua-console-confirm.json),
[confirmation TSV](2026-10-26-rust-lua-console-confirm.tsv), [confirmation build log](2026-10-26-rust-lua-console-confirm.build.log).
Both engine runs have source digest 1c9fb9f38cbe6aa17eca803a6e8396954398933c3486c972c8dcd4ca291fbbb4; all final hashes match
the tree. The [validation report](../testing/2026-10-26-rust-lua-console.md) retains 415 tests,
140 process passes, 17/18 official files in both engines and 72/110 raw CLI matches
(110/110 after only the known banner removal). Full engine/standalone compatibility
and the integrations remain unfinished.
