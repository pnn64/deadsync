# Rust native callback measurements

The new callback path dispatches a Rust function pointer by native ID, borrows
the original argument registers and writes separate results. It avoids temporary
argument/result vectors, per-call reference counting and the duplicated C argument
scratch copy used by other dynamic native closures. All measured warmed Rust
paths assert zero System allocations, reallocations and requested bytes.

Two runs compare this engine with the development mlua bridge to official C Lua
5.1.5. Each has nine alternating pairs across four workloads (72 samples).
Three Lua loops invoke the callback 5,000 times per iteration; twenty iterations
are timed per sample. The direct case makes one Rust-to-native entry per iteration,
20,000 iterations per sample. Setup, three warmups, correctness/layout/heap checks
are outside timing. Rust allocation counters remain inside timing.

The C comparison uses typed mlua Rust callbacks performing equivalent arithmetic
and return values. Its captured multiplier resides in a Rust closure; this
engine stores a traced Lua value. These measurements include both interpreters
and the callback bridges. They do not isolate indirect function-pointer cost or
compare two implementations of the same C ABI.
The Rust entry returns a borrowed Value slice; the C binding decodes a Rust f64.
The direct row therefore compares those public interfaces and result shapes.

Median nanoseconds per callback-equivalent, first / confirmation. Lua rows divide
whole-loop time by 5,000, including loop/arithmetic/result-adjustment work. Ratios
are medians of nine paired Rust/reference durations; separate engine medians
need not divide to the median paired ratio. Smaller is faster:

| Workload | Rust ns | C bridge ns | Rust/reference |
| --- | ---: | ---: | ---: |
| add_lua_loop | 155.66 / 156.99 | 77.49 / 75.24 | 2.071 / 1.982 |
| echo_lua_loop | 309.97 / 281.83 | 140.94 / 125.15 | 2.243 / 2.265 |
| capture_lua_loop | 150.57 / 137.00 | 64.73 / 60.71 | 2.322 / 2.232 |
| add_direct | 109.00 / 104.00 | 163.00 / 148.00 | 0.663 / 0.703 |

Direct Rust entry and Lua-driven callback execution have different costs. The
Lua loops remain slower than this C-bridge comparison. No universal callback or
interpreter speedup is claimed. The echo row returns four results including nil
and false; the capture row reads one scalar capture; the addition row uses two
numeric arguments. Return values are checked exactly before timing.

Each Rust sample reports zero allocations, reallocations and requested bytes
after warmup. This covers only these fixed numeric/result paths; constructing
new strings, tables, userdata or captures can allocate. Reference counters cover
Rust bridge allocations, excluding native C allocator calls. No complete C-versus-
Rust allocation comparison follows. Registration time is not measured.

One registered callback is live in each Lua-loop fixture. Heap observations
agree across the two runs:

| Fixture | Rust live estimate B | Rust reserved estimate B | C Lua allocator B |
| --- | ---: | ---: | ---: |
| add_lua_loop | 19,416 | 25,008 | 32,091 |
| echo_lua_loop | 20,248 | 25,824 | 33,873 |
| capture_lua_loop | 19,308 | 24,900 | 32,067 |

Rust estimates include retained object/payload capacities and traced capture
storage; allocator headers, native storage, RSS and exact high-water marks are
excluded. C's allocator count excludes Rust callback/bridge storage. The two
models are not exact physical heap comparisons. Vm stays 4,432 B and Value 16 B;
no VM field is added. Program/cache layouts are independent of callback bindings.

Five alternating fresh-process CPU pairs per run use the addition Lua loop.
Every child performs 5,000 warmup callbacks and 2,000,000 workload callbacks
(400 executions). Counters include startup, VM creation, registration, compilation
and cleanup. Allocator increments are disabled; the forwarding allocator remains.

| Counter | Rust first / confirm | C bridge first / confirm | Rust/reference |
| --- | ---: | ---: | ---: |
| Wall ms | 370.688 / 377.962 | 202.113 / 197.994 | 1.834 / 1.905 |
| Process CPU ms | 328.125 / 343.750 | 156.250 / 171.875 | 2.100 / 2.000 |
| Process cycles, billion | 0.759 / 0.768 | 0.382 / 0.379 | 1.988 / 2.014 |

Process CPU combines GetProcessTimes user/kernel values; cycles use
QueryProcessCycleTime after validated child exit. These are process counters,
not sampled call stacks. The earlier WPR policy denial was not retried. The
shared host is not CPU-isolated; scheduling, frequency and timer quantization
remain relevant, especially for the smallest cases.

Two standard Cargo cache runs retain 135 samples each, using the same public
compile/hit/store API. Median microseconds per call, first / confirmation:

| Fixture | Compile us | Hit us | Hit/compile |
| --- | ---: | ---: | ---: |
| tiny | 9.15 / 9.10 | 104.70 / 104.16 | 11.468 / 11.580 |
| records_1024 | 1,903.07 / 2,038.56 | 540.15 / 535.96 | 0.284 / 0.274 |
| closures_512 | 5,547.18 / 5,526.05 | 1,585.87 / 1,614.89 | 0.279 / 0.274 |
| binary_strings_256 | 890.76 / 915.45 | 426.94 / 383.05 | 0.469 / 0.395 |
| native_closures_512 | 2,130.89 / 2,184.99 | 1,915.75 / 2,011.04 | 0.886 / 0.864 |

Cache timings retain source/name/build/locale/integrity checks, bounded decoding
and verification. Stores include compilation, encoding and replacement; forced-
miss deletion is outside timing. Filesystem caches are warm. Tiny programs and
some native chunks can still compile faster than reloading; no automatic
profitability policy is added. All five preceding layouts/file sizes and
compiled-versus-cached VM heap observations are preserved. Each run also retains
five CPU compile/hit pairs of 300 source loads of the 512-closure fixture.

Two full reference runs retain 81 runtime and 22 frontend workloads, five paired
samples each. All 103 instruction/register layouts match the preceding compiler
milestone. Raw results retain throughput, startup/drop, compile cost, allocation
counts and before/after collection heap estimates. Reentrant/yielding callbacks,
richer host objects, sampled CPU stacks, exact physical allocator/RSS peaks and
complete standalone engine gates remain open.

~~~sh
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-host-functions-callbacks.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-host-functions-cache.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-host-functions.json
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-host-functions-callbacks-confirm.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-host-functions-cache-confirm.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-host-functions-confirm.json
~~~

All ten current-source maps match the tested tree. Callback
[first](2026-10-26-rust-lua-host-functions-callbacks.json) and [confirmation](2026-10-26-rust-lua-host-functions-callbacks-confirm.json)
runs retain TSV, Cargo build JSONL/logs, binary/build identities and reference
source hashes. Cache [first](2026-10-26-rust-lua-host-functions-cache.json) and
[confirmation](2026-10-26-rust-lua-host-functions-cache-confirm.json) and full reference
[first](2026-10-26-rust-lua-host-functions.json) and [confirmation](2026-10-26-rust-lua-host-functions-confirm.json) runs retain their raw
observations. [Correctness evidence](../testing/2026-10-26-rust-lua-host-functions.md) records callback
behavior, raw-handle scope, panic handling and known official/CLI limitations.
