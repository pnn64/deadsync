# Compiler constant-pool measurements

The 4,096-field record compiles 6.72x / 6.83x
faster than the parent in the two runs; the 4,096-number list improves
4.07x / 4.26x.
The repeated-string fixture saves 8,195 allocations per compilation. Complete
compiled artifacts and retained runtime heap observations remain unchanged.

Two comparisons use committed 0bb97ec97 and the final sources: separate rlibs,
identical opt-level 3/codegen-units 16/debug-info 1/static-CRT flags and one fat-LTO
executable. Each records nine alternating pairs across 19 source programs (342
samples), plus five fresh-process CPU pairs. Timings include compile_named and
Program drop; input generation, execution, artifact and layout checks occur
outside timing. Allocation counter updates are included in the timed runs.

Constants remain in first-use order. A temporary flat u32 index replaces repeated
full-pool scans once a function has 64 constants. Full typed equality resolves
collisions. Borrowed string lookups avoid allocating duplicate owned strings.
The index drops at function finalization. Both compilers generate byte-identical
complete cache files and cross-read them under the same benchmark-only build ID
for all 19 fixtures. Identical size accessors and the fixed ID exist only in
isolated benchmark source copies; production Cargo build identities remain
automatic. No runtime or cache-schema field changes.

Median microseconds per compilation, first / confirmation. Ratios are medians
of nine paired current/parent durations; smaller is faster. Separate engine
medians need not divide to the median paired ratio:

| Fixture | Parent us | Current us | Current/parent |
| --- | ---: | ---: | ---: |
| records_8 | 27.49 / 29.37 | 25.05 / 29.64 | 0.951 / 0.984 |
| records_16 | 38.65 / 39.98 | 39.06 / 41.76 | 1.028 / 1.015 |
| records_32 | 66.31 / 61.04 | 67.95 / 62.90 | 1.031 / 1.034 |
| records_64 | 117.81 / 122.82 | 113.28 / 115.49 | 0.956 / 0.943 |
| records_256 | 532.04 / 569.98 | 414.76 / 436.57 | 0.770 / 0.761 |
| records_1024 | 4,999.04 / 5,085.38 | 1,887.18 / 1,919.82 | 0.379 / 0.387 |
| records_4096 | 49,053.00 / 55,811.30 | 7,205.00 / 8,190.30 | 0.149 / 0.146 |
| numbers_16 | 26.34 / 27.47 | 25.89 / 27.86 | 1.056 / 1.025 |
| numbers_64 | 58.83 / 66.90 | 59.38 / 65.40 | 0.998 / 0.972 |
| numbers_512 | 365.74 / 406.41 | 296.80 / 296.74 | 0.737 / 0.729 |
| numbers_4096 | 10,954.15 / 12,200.60 | 2,723.75 / 2,861.05 | 0.245 / 0.235 |
| repeated_string_16 | 53.65 / 61.08 | 51.95 / 57.81 | 0.953 / 0.942 |
| repeated_string_256 | 565.63 / 640.15 | 512.56 / 601.90 | 0.892 / 0.904 |
| repeated_string_4096 | 8,798.30 / 10,191.90 | 7,639.75 / 9,173.55 | 0.884 / 0.900 |
| globals_1024 | 4,572.64 / 5,084.86 | 1,793.88 / 2,051.50 | 0.399 / 0.417 |
| nested_methods_64 | 5,457.30 / 5,528.70 | 4,962.02 / 5,597.16 | 1.008 / 1.005 |
| zero_positive | 860.88 / 842.32 | 690.78 / 626.77 | 0.802 / 0.790 |
| zero_negative | 827.94 / 803.91 | 670.75 / 615.26 | 0.810 / 0.794 |
| booleans_nil | 832.97 / 790.03 | 647.58 / 633.09 | 0.782 / 0.786 |

Record/global programs scale with many distinct constants; the number lists
isolate numeric pooling. Repeated-string programs reuse one 128-byte literal.
Nested-method functions each have a separate pool. First-positive/negative zero
and boolean/nil fixtures also check complete serialized output. Small pools and
some boundary/nested cases do not establish a speedup. Paired ratios put the
16-number list 5.6% / 2.5% slower and the 32-field record boundary
3.1% / 3.4% slower. The shared host is not CPU-isolated;
absolute results include timer and allocator instrumentation.

Allocation observations below are parent / current and agree across both runs
and all nine samples. Counts and requested bytes are per compilation; the peak
is the maximum logical live Rust allocation size during compilation/drop:

| Fixture | Allocations | Reallocations | Requested bytes | Peak tracked live bytes |
| --- | ---: | ---: | ---: | ---: |
| records_8 | 61 / 59 | 41 / 41 | 16,335 / 16,323 | 6,780 / 6,780 |
| records_32 | 113 / 112 | 69 / 69 | 51,005 / 51,504 | 18,182 / 18,437 |
| records_64 | 179 / 178 | 81 / 82 | 96,381 / 97,904 | 34,790 / 34,790 |
| records_4096 | 8,255 / 8,254 | 148 / 155 | 5,637,481 / 5,768,026 | 2,039,260 / 2,039,260 |
| numbers_4096 | 50 / 51 | 127 / 133 | 2,589,171 / 2,654,195 | 988,908 / 1,021,676 |
| repeated_string_4096 | 28,718 / 20,523 | 4,224 / 4,224 | 6,486,052 / 5,957,784 | 2,856,748 / 2,856,746 |
| nested_methods_64 | 9,254 / 9,191 | 4,429 / 4,429 | 4,248,767 / 4,281,823 | 1,243,948 / 1,244,556 |

Requested bytes sum allocation and replacement requests, so they include memory
that is subsequently released. Large distinct pools request temporary index
storage; duplicate strings avoid copies and allocations. The compilation-only
Function descriptor grows from 488 B to 512 B because of the additional Vec.
This is a compiler descriptor, separate from the runtime call frame. Vm remains
4,432 B and Value 16 B. Retained runtime heap observations match for all 19
parent/current fixtures, and immutable compiled artifacts match byte for byte.
The number-list fixture's tracked peak grows by 32,768 B. The record fixture's
tracked peak is unchanged despite additional transient index requests; the
repeated-string peak falls by 2 B.

Logical live/peak counters add successful Rust Layout sizes, subtract deallocated
sizes and update reallocations by their net size change. Every measured interval
asserts zero live balance with no subtraction underflow after Program drop.
The peak excludes allocator headers, internal realloc overlap, native/CRT
allocations, input setup and RSS. It is not the physical allocator high-water
mark. Descriptor stack/storage outside the timed allocation interval is not
included. VM heap estimates are a separate retained object/payload model.

Each CPU child compiles the 4,096-field record 32 times. Five alternating pairs
per run include process startup, source generation and cleanup. Allocator counter
increments are disabled; the forwarding allocator's enabled check remains.

| Counter | Parent first / confirm | Current first / confirm | Current/parent |
| --- | ---: | ---: | ---: |
| Wall ms | 1,810.964 / 1,799.456 | 297.778 / 297.675 | 0.167 / 0.164 |
| Process CPU ms | 1,781.250 / 1,781.250 | 265.625 / 265.625 | 0.148 / 0.149 |
| Process cycles, billion | 3.910 / 3.882 | 0.594 / 0.596 | 0.153 / 0.149 |

CPU combines user/kernel GetProcessTimes; cycles use QueryProcessCycleTime after
validated child exit. These are process counters, not sampled hotspot profiles.
The earlier [WPR policy denial](2026-10-26-rust-lua-intern-hash-profile-attempt.json)
was not retried. Process counters reduce wall-clock scheduling ambiguity but
do not remove host contention or frequency effects.

Two ordinary Cargo cache runs independently compare source/native compilation
and reloads through the public API, 135 samples each. Their native fixture comes
from the development Lua 5.1.5 binding. The direct compiler experiment above
uses source programs; these harnesses have different inputs/build arrangements.

| Fixture | Compile us first / confirm | Hit us first / confirm | Hit/compile |
| --- | ---: | ---: | ---: |
| tiny | 9.37 / 9.12 | 108.38 / 111.97 | 11.736 / 11.912 |
| records_1024 | 1,826.50 / 1,976.18 | 562.71 / 581.16 | 0.317 / 0.251 |
| closures_512 | 5,483.43 / 5,542.42 | 1,669.38 / 1,582.42 | 0.291 / 0.287 |
| binary_strings_256 | 846.13 / 915.47 | 398.31 / 374.63 | 0.482 / 0.398 |
| native_closures_512 | 1,994.14 / 2,181.23 | 1,823.81 / 2,147.94 | 0.914 / 0.873 |

Faster compilation can increase hit/compile ratios without making reloads
slower. Tiny programs and some native inputs can still compile faster than a
disk reload. No automatic cache-profitability policy is added. Hits include
identity/source/name/integrity checks, file read, bounded decode and verification.
Stores include compilation, encoding and file replacement; forced-miss deletion
is outside timing. Filesystem caches are warm. Both runs retain allocation/store
measurements and ten fresh-process counter samples each (five alternating
compile/hit pairs, 300 source loads of the 512-closure fixture per child). All
five prior layouts/file sizes and compiled-versus-cached VM heap observations
are preserved.

Two full C-reference runs retain 81 runtime and 22 frontend workloads, five
paired samples each. All 103 instruction/register layouts match the preceding
milestone. Their JSON/TSV records include throughput, startup/drop, compilation,
allocations and before/after collection heaps. No universal runtime-speed claim
follows from this compiler change. Other Rust-engine comparisons, exact physical
allocator/RSS peaks, cold physical-disk latency and complete engine gates remain
open.

~~~sh
python scripts/benchmark_rust_lua_constants.py --parent 0bb97ec97 --output docs/performance/2026-10-26-rust-lua-constant-pool-compare.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-constant-pool-cache.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-constant-pool.json
python scripts/benchmark_rust_lua_constants.py --parent 0bb97ec97 --output docs/performance/2026-10-26-rust-lua-constant-pool-compare-confirm.json --cpu-time
python scripts/benchmark_rust_lua_cache.py --output docs/performance/2026-10-26-rust-lua-constant-pool-cache-confirm.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-constant-pool-confirm.json
~~~

All ten current-source maps and both parent archive maps were verified. Direct
[first](2026-10-26-rust-lua-constant-pool-compare.json) and [confirmation](2026-10-26-rust-lua-constant-pool-compare-confirm.json) runs
retain exact wrappers, fixed test ID, original-source hashes, build commands,
harness, binary hash, TSV and build logs. Cargo cache [first](2026-10-26-rust-lua-constant-pool-cache.json)
and [confirmation](2026-10-26-rust-lua-constant-pool-cache-confirm.json) runs retain build JSONL and binary,
source and reference identities. Full reference [first](2026-10-26-rust-lua-constant-pool.json) and
[confirmation](2026-10-26-rust-lua-constant-pool-confirm.json) runs retain their raw results and build logs.
[Correctness evidence](../testing/2026-10-26-rust-lua-constant-pool.md) records expected strict CLI and
official-suite failures. The full engine/integration objective remains active.
