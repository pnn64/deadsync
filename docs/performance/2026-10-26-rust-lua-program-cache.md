# Persistent Lua program measurements

Two runs compare ordinary compile_named, persistent cache hits and cache stores
within the same Cargo-built executable. Each has nine alternating compile/hit
pairs for five fixtures; a store sample follows each pair (135 samples per run).
The cache is opt-in and uses the complete shared compiler representation.

Each iteration includes Program drop and its own timer. Source generation,
directory setup, forced-miss file deletion, VM correctness/layout checks and heap
observations are outside timing/counting. Hits include filename hashing, file
open/read, full source/name/build/locale/integrity checks, bounded decoding and
Program verification. OS file caches are warm. Stores include compilation,
encoding, temporary-file writes and replacement in an existing directory.
The native input is generated with the development Lua 5.1.5 binding outside
timing; C allocation/execution is not part of these measurements.

Median microseconds per operation, first / confirmation. Ratios are medians of
nine paired durations, rather than ratios of independent medians:

| Fixture | Compile us | Hit us | Hit/compile | Store/compile |
| --- | ---: | ---: | ---: | ---: |
| tiny | 10.31 / 9.67 | 111.51 / 113.29 | 11.483 / 11.682 | 158.445 / 168.468 |
| records_1024 | 5,108.95 / 5,121.48 | 707.54 / 729.30 | 0.136 / 0.146 | 1.468 / 1.520 |
| closures_512 | 5,911.74 / 5,786.81 | 2,024.33 / 1,986.27 | 0.350 / 0.343 | 1.467 / 1.487 |
| binary_strings_256 | 1,068.52 / 1,051.73 | 735.56 / 742.31 | 0.690 / 0.706 | 3.255 / 3.236 |
| native_closures_512 | 2,261.89 / 2,110.76 | 2,620.70 / 2,559.16 | 1.157 / 1.229 | 2.559 / 2.682 |

Large record/closure source programs benefit substantially in these fixtures.
Tiny programs and the imported native-chunk case reload more slowly than they
compile. Cache writes add encoding and filesystem work. This does not establish
a universal cache benefit or an automatic profitability policy. Native entries
retain both shared code and original re-export metadata; their files can be
larger than the input chunk. All raw store timings remain in JSON/TSV.

| Fixture | Source B | Cache file B | Shared instructions | Entry registers |
| --- | ---: | ---: | ---: | ---: |
| tiny | 20 | 582 | 5 | 2 |
| records_1024 | 13,178 | 97,696 | 3,080 | 3 |
| closures_512 | 22,859 | 205,404 | 3,596 | 4 |
| binary_strings_256 | 131,912 | 173,746 | 284 | 51 |
| native_closures_512 | 76,984 | 333,422 | 3,596 | 6 |

Rust System allocation events and requested bytes per iteration, first /
confirmation. The allocator counter is enabled only during measured operations;
native CRT allocations are excluded. Requested bytes include reallocation
requests and are not net live or peak allocator memory.

| Fixture / operation | Allocations | Reallocations | Requested B |
| --- | ---: | ---: | ---: |
| tiny / compile | 33.0 / 33.0 | 21.0 / 21.0 | 5,327 / 5,327 |
| tiny / hit | 17.0 / 17.0 | 2.0 / 2.0 | 1,796 / 1,796 |
| tiny / store | 44.0 / 44.0 | 26.0 / 26.0 | 11,615 / 11,615 |
| records_1024 / compile | 2,107.0 / 2,107.0 | 126.0 / 126.0 | 1,416,550 / 1,416,550 |
| records_1024 / hit | 1,041.0 / 1,041.0 | 2.0 / 2.0 | 217,692 / 217,692 |
| records_1024 / store | 2,118.0 / 2,118.0 | 135.0 / 135.0 | 1,623,742 / 1,623,742 |
| closures_512 / compile | 13,876.0 / 13,876.0 | 4,743.0 / 4,743.0 | 3,246,015 / 3,246,015 |
| closures_512 / hit | 5,141.0 / 5,141.0 | 2.0 / 2.0 | 506,913 / 506,913 |
| closures_512 / store | 13,887.0 / 13,887.0 | 4,753.0 / 4,753.0 | 3,967,462 / 3,967,462 |
| binary_strings_256 / compile | 574.0 / 574.0 | 1,118.0 / 1,118.0 | 305,027 / 305,027 |
| binary_strings_256 / hit | 279.0 / 279.0 | 2.0 / 2.0 | 221,249 / 221,249 |
| binary_strings_256 / store | 585.0 / 585.0 | 1,125.0 / 1,125.0 | 707,921 / 707,921 |
| native_closures_512 / compile | 11,301.0 / 11,301.0 | 1,086.0 / 1,086.0 | 1,127,946 / 1,127,946 |
| native_closures_512 / hit | 7,196.0 / 7,196.0 | 2.0 / 2.0 | 752,444 / 752,444 |
| native_closures_512 / store | 11,312.0 / 11,312.0 | 1,095.0 / 1,095.0 | 2,292,354 / 2,292,354 |

For all five fixtures in both runs, compiled and cached programs have identical
recorded VM live/reserved estimates before and after collection, including
collection/reclamation counts. The input buffer is released after decoding.
No cache field is added to Vm or Value; their sizes remain 4,432 B and 16 B.
These capacity-based VM estimates exclude allocator metadata, transient compiler
and cache buffers, CRT storage and process RSS. Exact live/peak allocator memory,
RSS and cold physical-disk latency remain unmeasured.

Five alternating fresh-process pairs per run each prime a cache, then perform
300 ordinary compilations or 300 disk hits of the 512-closure source. CPU
counting includes process startup, source/setup, priming and cleanup. Allocation
counter increments are disabled for these children, although the forwarding
allocator's enabled check remains.

| Counter | Compile first / confirm | Hit first / confirm | Hit/compile |
| --- | ---: | ---: | ---: |
| Wall ms | 1,810.493 / 1,889.743 | 718.027 / 718.222 | 0.398 / 0.376 |
| Process CPU ms | 1,781.250 / 1,812.500 | 625.000 / 640.625 | 0.357 / 0.351 |
| Process cycles, billion | 3.900 / 4.034 | 1.410 / 1.398 | 0.365 / 0.343 |

CPU combines user/kernel time from [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes);
cycles come from [QueryProcessCycleTime](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime)
after each validated child exits. The shared host is not CPU-isolated. These
counter totals do not attribute sampled hotspots. No call-stack samples were
captured; the previous [WPR policy denial](2026-10-26-rust-lua-intern-hash-profile-attempt.json)
was not retried.

Two independent full reference runs retain 81 runtime and 22 frontend workloads,
five alternating pairs each. All 103 existing instruction/register layouts
match the preceding milestone. Selected complete-script medians, nanoseconds;
C uses the development official Lua 5.1.5 binding:

| Fixture | Rust first / confirm | C first / confirm | Rust/C duration |
| --- | ---: | ---: | ---: |
| entry_args | 190 / 175 | 163 / 173 | 1.123 / 1.075 |
| gc_steps | 457,200 / 506,255 | 403,830 / 380,330 | 1.153 / 1.332 |
| gc_cycles | 3,225,200 / 3,388,505 | 2,226,970 / 2,258,470 | 1.448 / 1.505 |
| gsub_callback_wide | 377,375 / 387,170 | 138,415 / 131,960 | 2.911 / 2.965 |

Full reference JSONs also retain startup/drop, compilation, allocation totals
and before/after collection heap estimates. These Rust/C execution ratios use a
different harness from the cache API measurements. Other Rust-engine comparisons
and complete compatibility/performance gates remain open.

~~~sh
python scripts/benchmark_rust_lua_cache.py --cpu-time --output docs/performance/2026-10-26-rust-lua-program-cache-cache.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-program-cache.json
python scripts/benchmark_rust_lua_cache.py --cpu-time --output docs/performance/2026-10-26-rust-lua-program-cache-cache-confirm.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-program-cache-confirm.json
~~~

Cache JSONs pin source/reference hashes, Cargo commands and build JSONL, runtime
build identity, executable hash, platform/toolchain, every raw sample, layouts,
heap observations and process counters. Full reference records pin their own
source/build inputs. All eight current-source maps and both cache binary/build
identities were verified. [Correctness evidence](../testing/2026-10-26-rust-lua-program-cache.md) retains the
expected strict CLI and unchanged official-suite failures. The complete engine
and integration objective remains unfinished.
