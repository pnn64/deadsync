# Lua process measurements

The harness retains 43 runtime and twenty-two frontend workloads, with five
alternating paired samples per workload and a confirmation run. The oracle is
vendored official Lua 5.1.5, Windows x86-64 MSVC, static CRT and C locale. The
shared machine is not CPU-isolated. These data do not establish overall engine
performance parity.

`os_execute` runs `os.execute('exit 0')` once per script, five scripts per sample.
It includes shell startup/waiting and returns the raw numeric zero status.
`os_shell_query` makes an initial query and 100 more calls, checking a stable
result; ten scripts run per sample. No shell is spawned for Windows queries.
Correctness checks and compilation are outside execution timing.

Durations are milliseconds per complete script, first run / confirmation.
Each duration is its engine median; ratios are medians of paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Paired Rust/C duration |
| --- | ---: | ---: | ---: |
| `os_execute` | 20.573 / 20.317 | 20.325 / 28.668 | 0.993 / 0.712 |
| `os_shell_query` | 4.732 / 4.821 | 3.572 / 3.581 | 1.325 / 1.319 |
| `os_difftime` | 1.473 / 1.261 | 0.577 / 0.485 | 2.491 / 2.524 |
| `constant_left` | 1.138 / 0.937 | 0.288 / 0.225 | 3.948 / 4.184 |

The two new kernels have 7 / 25
instructions and 2 / 9 entry registers,
respectively. Both final runs have identical source digests and all 72 source/
configuration hashes match the retained tree. `constant_left` retains its 21
instructions and eleven entry registers.

The safe standard-library process baseline took
46.157 ms versus
20.580 ms for C Lua in its separate run.
That baseline used `Command::status` with `CREATE_NO_WINDOW`; the final bridge
uses default creation flags, matching the pinned CRT. These are sequential
build/run observations, not randomized parent/native pairs, and console policy
also changed. Do not attribute the entire timing difference to allocations.
The process kernel does not write parent stdout, so the accompanying stdout
buffer correction is outside its timed path.

Every sample shows ten allocations, three reallocations and 539 requested bytes
per safe-baseline shell call; both final runs show six allocations, zero
reallocations and 258 bytes. The bridge removes 40% of allocation calls, all
three reallocations and 52.1% of requested bytes for this fixture. This measured
reduction accompanies fixes to batch-file handling and raw command-line bytes;
it is the justification for the narrowly scoped unsafe Windows system bridge.
It does not add a dependency or call C Lua. UTF-16 buffers and handle ownership
stay inside that bridge; compiler/interpreter storage remains safe Rust.

The query kernel requests 303 allocations,
0 reallocations and
9999 bytes per 101-query script. These temporary
standard-library environment/path conversions remain. The allocator records
Rust/global allocator requests, including reference-binding work; native C
allocation counts are not measured.

Heap estimates agree in both final runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `os_execute` | `before_collect` | 18,072 | 23,576 | 23,576 | 0 | 0 |
| `os_execute` | `after_collect` | 22,040 | 27,544 | 27,544 | 1 | 0 |
| `os_shell_query` | `before_collect` | 19,095 | 24,615 | 24,615 | 0 | 0 |
| `os_shell_query` | `after_collect` | 23,063 | 28,583 | 28,583 | 1 | 0 |

Each retains three standard-stream userdata objects. The first full collection
initializes reusable tracing storage and reclaims no objects. Heap estimates
exclude transient process buffers, OS processes/handles, Rust stdio storage and
the new process-global logical stdout cache. They are not exact allocator peaks
or RSS. The new stdout cache allocates lazily when buffering output and uses
4096 logical bytes by default for piped Windows stdout; `setvbuf` can change it.

Rust startup/drop takes 60.708 /
57.192 microseconds, with one 1,000-state
sample per run. It requests 238 allocations,
62 reallocations and
39111 bytes per VM in the first run.
All runtime compile/drop and twenty-two frontend scaling measurements remain in
the raw data. There is no broad parent/native speedup claim.

Raw data: [final JSON](2026-10-25-rust-lua-process.json), [TSV](2026-10-25-rust-lua-process.tsv),
[build log](2026-10-25-rust-lua-process.build.log), [confirmation JSON](2026-10-25-rust-lua-process-confirm.json),
[confirmation TSV](2026-10-25-rust-lua-process-confirm.tsv), [confirmation build log](2026-10-25-rust-lua-process-confirm.build.log),
[safe baseline JSON](2026-10-25-rust-lua-process-std.json), [safe baseline TSV](2026-10-25-rust-lua-process-std.tsv),
[safe baseline build log](2026-10-25-rust-lua-process-std.build.log).
The [safe source archive](2026-10-25-rust-lua-process-std-source.zip) contains all seventy recorded
source/configuration inputs, each checked against its recorded SHA-256.
The [reverse patch](2026-10-25-rust-lua-process-std.patch) restores the baseline implementation from
the final tree; `git apply --check --unidiff-zero` passes. The archive preserves original bytes
and line endings. Restore it in an isolated copy, with the unchanged workspace
legacy/vendor sources, and omit the final-only Windows/output modules to
reproduce the earlier source inventory.

The [compatibility rebuild](../testing/2026-10-25-rust-lua-process-std-rebuild.json) uses direct
`rustc` with static CRT to repeat observations from that archive. Its executable
is used only for compatibility comparisons, not these Cargo benchmark timings.
See [validation](../testing/2026-10-25-rust-lua-process.md) for retained counterexamples to the safe
launcher and stdout path.

Final source digest: `a10b54733f23c4d19c25c428928ca56568e8da27fd3a4be3d2093352900dae24`.
Exact live/peak allocations, RSS, cycles, native C allocations, other Rust engines,
terminal throughput and full OS/library workloads remain unmeasured.
