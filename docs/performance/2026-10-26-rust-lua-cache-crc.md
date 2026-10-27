# Safe cache CRC measurements

Two comparisons use committed c47b2d415 and the final tree: separate rlibs,
identical opt-level 3/codegen-units 16/debug-info 1/static-CRT flags and one fat-LTO
executable. Each records nine alternating pairs for 16 checksum inputs and ten
cache operations (468 samples), plus five native-cache process CPU pairs.

The method processes eight-byte blocks with independent lookups. Our safe Rust
implementation generates the tables from the existing reflected polynomial at
compile time. The general method is described by
[Kounavis and Berry](https://static.aminer.org/pdf/PDF/000/432/446/a_systematic_approach_to_building_high_performance_software_based_crc.pdf).
No code or dependency is imported. Logical table storage grows from 1 KiB to
8 KiB (7 KiB additional read-only data), shared across instances. It adds no
runtime table allocation or per-VM field. Binary/RSS high-water marks are not
measured.

Isolated source copies receive identical private checksum forwarding and table
size accessors, plus the same explicit benchmark-only build ID. The public cache
path still performs its full identity/source/name/integrity checks, bounded
decoding and verification. The shared ID lets each engine read the other's
entries and prove exact serialized-byte equality; normal Cargo builds use their
automatic identity. Parent/current allocations, reallocations, requested bytes
and recorded heap estimates match in every paired fixture/sample.

Median nanoseconds per checksum, first / confirmation. Ratios are medians of
nine paired current/parent durations. Inputs use deterministic binary bytes;
the final suffix is the unaligned offset:

| Fixture | Parent ns | Current ns | Current/parent |
| --- | ---: | ---: | ---: |
| crc_1_0 | 1.99 / 1.87 | 2.73 / 2.75 | 1.417 / 1.445 |
| crc_7_0 | 9.64 / 9.71 | 10.81 / 10.55 | 1.113 / 1.073 |
| crc_8_0 | 11.39 / 11.48 | 6.49 / 6.18 | 0.575 / 0.540 |
| crc_33_0 | 61.86 / 73.87 | 18.36 / 22.41 | 0.295 / 0.305 |
| crc_512_0 | 1,225.46 / 1,486.85 | 346.46 / 420.17 | 0.285 / 0.284 |
| crc_4096_0 | 10,001.22 / 12,243.70 | 2,866.99 / 3,428.17 | 0.276 / 0.279 |
| crc_65536_0 | 155,903.91 / 195,109.38 | 43,978.12 / 54,245.31 | 0.280 / 0.279 |
| crc_1048576_0 | 2,746,212.50 / 3,112,775.00 | 719,337.50 / 875,412.50 | 0.286 / 0.283 |
| crc_65536_3 | 161,516.41 / 187,332.81 | 46,016.41 / 53,903.91 | 0.275 / 0.285 |

All 16 lengths/offsets remain in JSON/TSV. The one- and seven-byte cases are
slower here; tiny timings are also noisy/quantized. Valid cache entries are
larger than one block. The cache checksum covers a complete serialized entry.
Checksum loops have no per-call timer and
allocate nothing. Setup and equality checks are outside timing.

Complete cache calls below include their own timers and Program drop. Hits
include path hashing, file read, full checks, decoding and verification. Stores
include compilation, encoding, writing and replacement. Forced-miss file deletion
is outside timing/counting; the OS file cache and cache directories are warm.
VM results/layouts/heaps are checked outside timing. Native input for this
comparison comes from the committed parent's existing Lua 5.1 dumper.

Median microseconds per complete call, first / confirmation:

| Fixture | Parent us | Current us | Current/parent |
| --- | ---: | ---: | ---: |
| hit_tiny | 124.90 / 127.67 | 114.02 / 125.66 | 0.878 / 0.996 |
| hit_records_1024 | 668.82 / 723.62 | 567.64 / 570.24 | 0.683 / 0.797 |
| hit_closures_512 | 2,076.40 / 2,085.42 | 1,760.04 / 1,627.04 | 0.800 / 0.742 |
| hit_binary_strings_256 | 858.86 / 881.96 | 555.82 / 502.42 | 0.707 / 0.583 |
| hit_native_closures_512 | 2,632.56 / 2,732.10 | 1,889.30 / 2,066.14 | 0.718 / 0.773 |
| store_tiny | 1,488.10 / 1,445.12 | 1,474.03 / 1,438.34 | 0.991 / 0.988 |
| store_records_1024 | 7,086.36 / 7,414.84 | 6,947.46 / 6,990.34 | 0.991 / 0.940 |
| store_closures_512 | 8,257.20 / 8,202.20 | 7,867.04 / 8,009.84 | 0.947 / 0.956 |
| store_binary_strings_256 | 3,351.52 / 3,332.08 | 2,958.30 / 2,897.18 | 0.878 / 0.877 |
| store_native_closures_512 | 5,538.28 / 5,403.34 | 4,969.52 / 4,832.90 | 0.897 / 0.880 |

Each CPU child generates the same native input, primes one cache and performs
500 hits. Five alternating pairs per run include process startup, generation,
priming and cleanup; allocator counter increments are disabled, while the
forwarding allocator's enabled check remains.

| Counter | Parent first / confirm | Current first / confirm | Current/parent |
| --- | ---: | ---: | ---: |
| Wall ms | 1,395.819 / 1,454.017 | 1,117.563 / 1,147.380 | 0.801 / 0.789 |
| Process CPU ms | 1,359.375 / 1,421.875 | 1,046.875 / 1,109.375 | 0.770 / 0.780 |
| Process cycles, billion | 2.972 / 3.095 | 2.364 / 2.424 | 0.795 / 0.783 |

CPU combines user/kernel [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes);
cycles use [QueryProcessCycleTime](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime)
after validated child exit. These counters and isolated function measurements
are not sampled hotspot profiles. The earlier
[WPR policy denial](2026-10-26-rust-lua-intern-hash-profile-attempt.json) was not
retried. The host is shared and not CPU-isolated.

Two ordinary Cargo cache runs independently compare compilation and reloads
using the public API, 135 samples each. Their native fixture is generated by the
development Lua 5.1.5 binding, unlike the parent-dumped input above. The two
harnesses' absolute timings/native inputs are not interchangeable.

| Fixture | Compile us first / confirm | Hit us first / confirm | Hit/compile |
| --- | ---: | ---: | ---: |
| tiny | 10.30 / 9.50 | 109.49 / 104.75 | 10.714 / 11.257 |
| records_1024 | 5,249.29 / 5,176.84 | 580.98 / 594.40 | 0.112 / 0.110 |
| closures_512 | 5,671.33 / 5,552.99 | 1,529.22 / 1,671.69 | 0.260 / 0.268 |
| binary_strings_256 | 1,026.99 / 1,019.35 | 418.76 / 399.55 | 0.383 / 0.380 |
| native_closures_512 | 2,078.21 / 2,050.84 | 1,897.89 / 1,898.04 | 0.888 / 0.870 |

This does not create an automatic profitability policy. Tiny programs and some
native inputs can still compile faster than a disk reload. All five existing
layouts/cache-file sizes are preserved. Both Cargo runs retain allocation/store
costs and identical compiled-versus-cached VM heap observations. Estimates cover
retained object/payload capacities, excluding allocator metadata, transient
compiler/cache buffers, CRT storage and RSS.

Two full reference runs retain 81 runtime and 22 frontend workloads, five paired
samples each. All 103 instruction/register layouts match the preceding
milestone; Vm is 4,432 B and Value 16 B. Their JSON/TSV records include throughput,
startup/drop, compilation, allocations and before/after collection heaps.
Other Rust-engine comparisons, exact live/peak allocator/RSS, cold physical-disk
latency and complete engine gates remain open.

~~~sh
python scripts/benchmark_rust_lua_cache_crc.py --parent c47b2d415 --output docs/performance/2026-10-26-rust-lua-cache-crc-compare.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-cache-crc-cache.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-cache-crc.json
python scripts/benchmark_rust_lua_cache_crc.py --parent c47b2d415 --output docs/performance/2026-10-26-rust-lua-cache-crc-compare-confirm.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-cache-crc-cache-confirm.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-cache-crc-confirm.json
~~~

All ten current source maps and both parent archive maps were verified. Direct
comparisons retain their exact wrappers, fixed test ID, original-source hashes,
build commands, harness, binary hash and raw results. Cargo records retain build
JSONL, executable/build identities, source/reference hashes and raw observations.
[Correctness evidence](../testing/2026-10-26-rust-lua-cache-crc.md) retains the expected strict CLI and
official-suite failures. The full engine/integration objective remains active.
