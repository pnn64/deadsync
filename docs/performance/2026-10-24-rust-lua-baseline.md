# Independent Rust Lua: initial baseline

This is an early compiler/interpreter baseline, not a compatibility or performance
completion gate. The engine still lacks GC and much of Lua. It is currently
slower than the official 5.1 interpreter on every execution workload below.

Run from the workspace root:

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-24-rust-lua-baseline.json
```

The [JSON](2026-10-24-rust-lua-baseline.json) records source hashes, reference
source digest, compiler, machine, dirty revision, raw samples and missing metrics.
The [TSV](2026-10-24-rust-lua-baseline.tsv) retains stdout and the
[build log](2026-10-24-rust-lua-baseline.build.log) records compilation.
The command uses Cargo's release benchmark profile, workspace LTO and static
MSVC CRT configuration. The host reports x86-64 Intel family 6, model 79.

Each workload first checks the same numeric result against vendored PUC 5.1.5.
Five paired execution samples alternate measurement order. Compilation is outside
execution timing; repeated host entry and scalar result extraction are included.
Ratios are medians of paired Rust/reference times, so lower is better.

| Workload | Rust median ns/run | Lua 5.1 median ns/run | Median paired ratio |
| --- | ---: | ---: | ---: |
| Numeric for, 100,000 iterations | 10,966,823 | 790,425 | 13.51 |
| Numeric while, 100,000 iterations | 20,604,184 | 1,658,564 | 12.43 |
| Array writes and reads, 10,000 elements | 3,321,866 | 471,752 | 7.04 |
| Recursive Fibonacci, n=20 | 5,176,620 | 958,510 | 5.43 |
| Captured-cell calls, 10,000 calls | 2,257,252 | 437,448 | 5.05 |
| Byte-string concatenation, 1,000 steps | 1,426,143 | 602,226 | 2.38 |

The warmed numeric loops performed zero measured Rust allocations or
reallocations. `Value` is 16 bytes. Concatenation performs 2,999 allocation or
reallocation events per run, exposing a substantial future allocation target.
Table and closure allocations are retained without collection in the new engine.
Counts are Rust `GlobalAlloc` events: they include Lua allocations routed through
mlua's Rust allocator, and exclude direct native C `malloc` outside that allocator.
Allocated-byte totals are requested bytes, not live memory or peak RSS.

Compiler creation and destruction measured approximately 4.7–8.0 microseconds
for these small Rust chunks and 6.3–9.2 microseconds for the reference binding.
State creation/destruction measured 8.8 versus 139.7 microseconds, but these states
open different library sets, so that comparison does not establish a startup win.
Allocation instrumentation affects timing. These samples are preliminary and
there is no regression threshold yet.

Further baseline work must measure live/peak Rust heap and process memory, CPU
cycles, larger and scaling workloads, complete libraries, and other Rust engines.
Full correctness and compatibility remain the priority. The current code visibly
performs repeated frame/program lookups, extra register moves and conversions;
profiles should determine their cost before changing dispatch or value layout.
