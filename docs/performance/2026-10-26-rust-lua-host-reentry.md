# Protected Rust-to-Lua callback measurements

HostCall::call and call_and_return retain the live native callback and invoke
Lua through a protected continuation using the same contiguous registers and
shared instruction counter. Results can be forwarded without an intermediate
vector. No argument/result heap buffer or reference-count operation is added.
Vm grows by eight bytes, from 4,432 to 4,440 B; Value stays 16 B and Frame 64 B.

Two runs compare this engine with official C Lua 5.1.5 through its development
binding. Each has nine alternating pairs across six workloads (108 samples).
Five loops invoke a host callback 5,000 times per iteration; twenty iterations
are timed per sample. Two loops make one nested Lua call per host callback,
returning one addition result or four echo results including nil and false.
Child functions are compiled and bound once before timing. The direct case
makes one host entry per iteration, 20,000 iterations per sample. Setup, three
warmups, correctness/layout/heap checks are outside timing; Rust allocation
counters remain inside timing.

For the two reentry loops the C comparison is a raw lua_CFunction copying the
same three native argument slots and calling lua_pcall(LUA_MULTRET). It forwards
the resulting stack slots without typed bridge decoding in that callback.
This preserves native arguments as the Rust implementation does. Remaining
leaf callbacks use typed mlua Rust callbacks; the captured multiplier resides
in a Rust closure rather than traced Lua captures. The direct Rust entry returns
a borrowed Value slice while the C bridge decodes f64. These are whole public
boundary/interpreter measurements, not isolated function-pointer latency.

Median nanoseconds per host callback-equivalent, first / confirmation. Loop
rows divide whole-loop time by 5,000, including interpreter execution and result
adjustment. Ratios are medians of nine paired durations; separate engine medians
need not divide to the paired ratio. Smaller is faster:

| Workload | Rust ns | C reference ns | Rust/reference |
| --- | ---: | ---: | ---: |
| add_lua_loop | 168.41 / 166.45 | 95.55 / 85.48 | 1.785 / 1.970 |
| echo_lua_loop | 308.76 / 292.31 | 138.59 / 135.91 | 2.228 / 2.159 |
| capture_lua_loop | 152.79 / 151.61 | 68.11 / 67.58 | 2.250 / 2.243 |
| add_direct | 113.00 / 106.00 | 151.00 / 160.00 | 0.734 / 0.673 |
| reenter_add_lua_loop | 322.44 / 319.57 | 99.70 / 100.41 | 3.234 / 3.231 |
| reenter_echo_lua_loop | 450.50 / 467.09 | 136.65 / 135.93 | 3.290 / 3.424 |

Nested calls and the Lua-driven leaf callback loops remain slower than these
C comparisons. No universal callback or interpreter speedup is claimed. All
54 measured Rust samples per run assert zero System allocations, reallocations
and requested bytes after warmup. This covers the fixed numeric paths, not new
strings, tables, captures, userdata or repeated closure construction. Reference
counters cover Rust bridge allocations and exclude native C allocator calls.

Each loop retains one registered Rust callback; heap observations agree across
the two runs:

| Fixture | Rust live estimate B | Rust reserved estimate B | C Lua allocator B |
| --- | ---: | ---: | ---: |
| add_lua_loop | 19,416 | 25,008 | 32,091 |
| echo_lua_loop | 20,248 | 25,824 | 33,873 |
| capture_lua_loop | 19,308 | 24,900 | 32,067 |
| reenter_add_lua_loop | 20,684 | 26,140 | 32,911 |
| reenter_echo_lua_loop | 21,688 | 27,128 | 33,993 |

Rust estimates model retained capacities and traced capture storage, excluding
allocator headers, native storage, RSS and exact peaks. C's count excludes Rust
bridge storage. These are different accounting models, not exact physical heap
comparisons. Cache/program layouts remain independent of runtime host bindings.

Five alternating fresh-process CPU pairs per run cover both a leaf addition
loop and nested addition. Each child performs 5,000 warmup host callbacks and
2,000,000 timed-workload host callbacks (400 executions). Process counters
include VM creation, registration, compilation, startup and cleanup. Allocation
counter increments are disabled; the forwarding allocator remains.

| Fixture / counter | Rust first / confirm | C first / confirm | Rust/reference |
| --- | ---: | ---: | ---: |
| add_lua_loop: Wall ms | 377.988 / 379.426 | 215.786 / 214.203 | 1.777 / 1.777 |
| add_lua_loop: Process CPU ms | 359.375 / 359.375 | 187.500 / 171.875 | 2.000 / 2.091 |
| add_lua_loop: Process cycles, billion | 0.767 / 0.765 | 0.405 / 0.402 | 1.919 / 1.893 |
| reenter_add_lua_loop: Wall ms | 711.019 / 694.665 | 249.384 / 241.723 | 2.888 / 2.837 |
| reenter_add_lua_loop: Process CPU ms | 687.500 / 656.250 | 234.375 / 218.750 | 3.071 / 3.000 |
| reenter_add_lua_loop: Process cycles, billion | 1.496 / 1.453 | 0.486 / 0.467 | 3.120 / 3.109 |

CPU combines GetProcessTimes user/kernel values; cycles use
QueryProcessCycleTime after a validated child exit. These are process counters,
not sampled call stacks. The earlier WPR policy denial was not retried. The
shared host is not CPU-isolated; frequency, scheduling and timer quantization
limit fine-grained timing claims. No isolated fresh-parent speedup is claimed.

Two standard Cargo cache runs retain 135 samples each. Median microseconds
per public API call, first / confirmation:

| Fixture | Compile us | Hit us | Hit/compile |
| --- | ---: | ---: | ---: |
| tiny | 9.63 / 9.24 | 112.24 / 109.75 | 11.533 / 11.809 |
| records_1024 | 2,012.55 / 2,019.17 | 506.64 / 544.25 | 0.248 / 0.275 |
| closures_512 | 5,562.57 / 5,588.91 | 1,582.31 / 1,593.63 | 0.284 / 0.285 |
| binary_strings_256 | 920.64 / 923.61 | 375.77 / 409.89 | 0.408 / 0.436 |
| native_closures_512 | 2,189.81 / 2,178.81 | 1,878.83 / 1,930.52 | 0.858 / 0.880 |

Cache timings include source/name/build/locale/integrity checks, bounded
decoding and verification. Stores include compilation, encoding and replacement;
forced-miss deletion is outside timing. Filesystem caches are warm. Tiny programs
and some native chunks can compile faster than reload. No automatic profitability
policy is added. All five prior layouts/file sizes and compiled-versus-cached
VM heap observations are preserved. Each run also retains five CPU compile/hit
pairs of 300 loads of the 512-closure fixture.

Two full reference runs retain 81 runtime and 22 frontend workloads, five paired
samples each. All 103 instruction/register layouts match the prior compiler
milestone. Raw results include throughput, startup/drop, compile cost, allocation
counts and before/after collection heap estimates. Yielding Rust callback stacks,
richer host objects, sampled CPU stacks, exact physical heap/RSS peaks and
complete standalone engine gates remain open.

~~~sh
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-host-reentry-callbacks.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-host-reentry-cache.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-host-reentry.json
python scripts/benchmark_rust_lua_host_functions.py --output docs/performance/2026-10-26-rust-lua-host-reentry-callbacks-confirm.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-host-reentry-cache-confirm.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-host-reentry-confirm.json
~~~

All ten current-source maps match the tested tree. Callback
[first](2026-10-26-rust-lua-host-reentry-callbacks.json) and [confirmation](2026-10-26-rust-lua-host-reentry-callbacks-confirm.json)
runs retain TSV, Cargo build JSONL/logs, binary/build identities and reference
source hashes. Cache [first](2026-10-26-rust-lua-host-reentry-cache.json) and
[confirmation](2026-10-26-rust-lua-host-reentry-cache-confirm.json) and full reference
[first](2026-10-26-rust-lua-host-reentry.json) and [confirmation](2026-10-26-rust-lua-host-reentry-confirm.json) retain their raw
observations. [Correctness evidence](../testing/2026-10-26-rust-lua-host-reentry.md) records nested-call
semantics and the known official/CLI limitations.
