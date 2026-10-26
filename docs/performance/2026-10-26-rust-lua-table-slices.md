# Incremental table propagation measurements

Two runs compare committed 6ec627329 with the final tree: independently compiled
dependency-free rlibs in one executable, identical optimization flags, static
MSVC CRT and fat LTO. Each has nine alternating paired samples across seven
fixtures (126 samples), plus five fresh-process CPU pairs.

Each sample starts from a fresh stopped VM, preloads a step program and builds a
pinned table outside timing. Public execute calls run collectgarbage('step',0)
until one cycle completes at default step multiplier 200. Every call has an
Instant timer; total time includes timing/bookkeeping. Correctness checks every
surviving numeric value or child payload and total count/sum outside timing.
Allocation counters surround collection only. There is no private phase
instrumentation: maximum step duration includes atomic work.

Number/hole arrays have 1,048,576 slots; number/tombstone hashes have 262,144
entries; child-table cases have 32,768 references. Holes/tombstones retain only
the final value. The long-mode case has 262,144 numeric slots and a metatable
whose __mode is 1 MiB of 'x' (strong contents). It exposes the final atomic
metatable rescan that remains.

Median of each sample's maximum public step, milliseconds, first / confirm.
Ratios are medians of nine paired current/parent durations:

| Fixture | Parent ms | Current ms | Current/parent |
| --- | ---: | ---: | ---: |
| array_children | 1.180 / 1.344 | 0.244 / 0.298 | 0.206 / 0.219 |
| array_holes | 0.731 / 1.467 | 0.016 / 0.031 | 0.023 / 0.020 |
| array_long_mode | 2.908 / 3.213 | 3.424 / 3.136 | 1.147 / 0.996 |
| array_numbers | 7.667 / 7.629 | 0.117 / 0.126 | 0.016 / 0.016 |
| hash_children | 1.193 / 1.317 | 0.271 / 0.299 | 0.236 / 0.234 |
| hash_numbers | 1.749 / 2.328 | 0.097 / 0.108 | 0.048 / 0.045 |
| hash_tombstones | 0.273 / 0.455 | 0.027 / 0.029 | 0.098 / 0.068 |

Complete-cycle medians, milliseconds:

| Fixture | Parent ms | Current ms | Current/parent |
| --- | ---: | ---: | ---: |
| array_children | 2.546 / 2.864 | 2.146 / 2.701 | 0.839 / 0.924 |
| array_holes | 0.746 / 1.489 | 0.942 / 1.863 | 1.270 / 1.117 |
| array_long_mode | 5.397 / 5.831 | 6.001 / 6.339 | 1.122 / 1.087 |
| array_numbers | 7.683 / 7.648 | 7.588 / 9.520 | 0.990 / 1.239 |
| hash_children | 2.530 / 2.908 | 2.208 / 2.819 | 0.878 / 0.966 |
| hash_numbers | 1.763 / 2.353 | 1.851 / 3.037 | 1.023 / 1.292 |
| hash_tombstones | 0.289 / 0.478 | 0.334 / 0.526 | 1.148 / 1.123 |

Primitive-only tables exchange additional slice dispatch/API steps for shorter
propagation pauses. This is not a universal throughput improvement. Child graphs
avoid queueing their full width. Long-mode tables still have atomic traversal;
the change does not establish bounded collector pauses. Per-step median/p99/max
and all raw samples remain in JSON/TSV. This shared host is not CPU-isolated;
observed maxima are not deadline guarantees.

Retained reserved-byte estimates after each cycle, first / confirm:

| Fixture | Parent B | Current B | Current/parent |
| --- | ---: | ---: | ---: |
| array_children | 9,813,412 / 9,813,412 | 8,687,332 / 8,687,332 | 0.885 / 0.885 |
| array_holes | 16,808,929 / 16,808,929 | 16,808,929 / 16,808,929 | 1.000 / 1.000 |
| array_long_mode | 6,323,627 / 6,323,627 | 6,323,627 / 6,323,627 | 1.000 / 1.000 |
| array_numbers | 16,808,508 / 16,808,508 | 16,808,508 / 16,808,508 | 1.000 / 1.000 |
| hash_children | 10,599,916 / 10,599,916 | 9,473,836 / 9,473,836 | 0.894 / 0.894 |
| hash_numbers | 10,517,124 / 10,517,124 | 10,517,124 / 10,517,124 | 1.000 / 1.000 |
| hash_tombstones | 10,517,585 / 10,517,585 | 10,517,585 / 10,517,585 | 1.000 / 1.000 |

First-run medians, Rust GlobalAlloc only:

| Fixture | Steps P/C | Allocations P/C | Reallocations P/C | Requested B P/C |
| --- | ---: | ---: | ---: | ---: |
| array_children | 518 / 522 | 0 / 0 | 8 / 0 | 2,252,160 / 0 |
| array_holes | 6 / 134 | 0 / 0 | 0 / 0 | 0 / 0 |
| array_long_mode | 6 / 38 | 0 / 0 | 0 / 0 | 0 / 0 |
| array_numbers | 6 / 134 | 0 / 0 | 0 / 0 | 0 / 0 |
| hash_children | 518 / 522 | 0 / 0 | 8 / 0 | 2,252,160 / 0 |
| hash_numbers | 6 / 38 | 0 / 0 | 0 / 0 | 0 / 0 |
| hash_tombstones | 6 / 38 | 0 / 0 | 0 / 0 | 0 / 0 |

Frontiers remain flat reusable Vec storage, without per-table heap allocations.
Live-byte deltas across 1,024 empty tables give 88 B each in both engines; VM is
4,432 B and Value 16 B. Heap estimates include retained working capacities,
objects and payloads, excluding allocator metadata, compiler scratch, CRT
storage and process RSS. Requested bytes count reallocation requests and do not
measure net live or peak allocator memory.

Each CPU child builds 100,000 child tables then performs 32 complete incremental
cycles. Counters include startup, setup, final validation and shutdown. The same
executable selects the engine through an environment variable; five alternating
pairs per run:

| Counter | Parent first / confirm | Current first / confirm | Current/parent |
| --- | ---: | ---: | ---: |
| Wall ms | 301.267 / 321.443 | 300.793 / 327.439 | 1.025 / 1.013 |
| Process CPU ms | 281.250 / 296.875 | 281.250 / 296.875 | 1.000 / 1.000 |
| Process cycles, billion | 0.605 / 0.650 | 0.603 / 0.664 | 1.033 / 1.016 |

CPU comes from [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes);
cycles from [QueryProcessCycleTime](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime)
after validated child exit. CPU timer granularity affects short processes.
No call-stack profile was captured; the earlier
[WPR policy denial](2026-10-26-rust-lua-intern-hash-profile-attempt.json) remains
unresolved and was not retried. Counter totals do not establish sampled hotspots.

Two full reference runs retain 81 runtime and 22 frontend workloads, five
alternating paired samples each. All 103 instruction/register layouts match
the preceding milestone. Selected complete-script medians, nanoseconds; C uses
the development official Lua 5.1.5 binding:

| Fixture | Rust first / confirm | C first / confirm | Rust/C duration |
| --- | ---: | ---: | ---: |
| gc_cycles | 2,794,030 / 3,070,445 | 1,897,455 / 2,199,895 | 1.463 / 1.393 |
| gc_steps | 397,405 / 377,180 | 341,020 / 331,360 | 1.188 / 1.173 |
| gc_mark_mutation | 3,322,200 / 3,492,060 | 442,640 / 425,330 | 7.374 / 7.515 |
| gc_weak_values | 1,106,215 / 1,056,650 | 872,615 / 822,135 | 1.278 / 1.304 |
| array_table | 1,438,206 / 1,260,370 | 587,376 / 557,238 | 2.469 / 2.297 |
| gsub_callback_wide | 302,520 / 330,745 | 116,620 / 116,950 | 2.592 / 2.828 |

Full JSONs retain startup/drop, compilation, allocations, requested bytes and
before/after collection heap estimates. These Rust/C ratios use a different
harness from the committed-parent experiment. Other Rust-engine comparisons,
exact allocator live/peak/RSS, call-stack profiles and fully bounded collection
remain open.

~~~sh
python scripts/benchmark_rust_lua_gc_tables.py --parent 6ec627329 --output docs/performance/2026-10-26-rust-lua-table-slices-compare.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-table-slices.json
python scripts/benchmark_rust_lua_gc_tables.py --parent 6ec627329 --output docs/performance/2026-10-26-rust-lua-table-slices-compare-confirm.json --cpu-time
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-table-slices-confirm.json
~~~

JSONs pin source hashes, platform and toolchain. Comparisons retain archived
parent hashes, exact rustc commands, generated harnesses, binary hashes and raw
samples. All seven current maps and both parent maps were verified.
[Correctness results](../testing/2026-10-26-rust-lua-table-slices.md) retain the expected strict CLI and
official-suite failures. This collector milestone leaves the full engine and
integration objective unfinished.
