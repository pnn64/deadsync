# Lua string interning measurements

Two before/after runs compare committed parent 2a4dbb99c with the final tree in
one executable: separately compiled dependency-free rlibs, identical optimization
flags, static MSVC CRT and fat LTO. Each run records nine alternating paired
samples across 27 fixtures: nineteen hit lengths, four miss lengths, one
middle-change input set and three complete Lua scripts. Input/VM setup,
compilation and correctness checks are outside timing. Each miss sample uses a
fresh VM and 128 distinct inputs. Automatic GC is timed for Lua scripts.

Median nanoseconds per lookup/insert or complete Lua script, first / confirmation.
The final column is the median of nine paired current/parent duration ratios:

| Fixture | Parent ns | Current ns | Current/parent |
| --- | ---: | ---: | ---: |
| `intern_hit_1` | 10 / 10 | 8 / 8 | 0.800 / 0.800 |
| `intern_hit_8` | 37 / 40 | 11 / 11 | 0.286 / 0.275 |
| `intern_hit_16` | 39 / 32 | 37 / 33 | 0.946 / 1.031 |
| `intern_hit_32` | 51 / 55 | 12 / 12 | 0.220 / 0.218 |
| `intern_hit_512` | 609 / 641 | 66 / 63 | 0.108 / 0.100 |
| `intern_hit_4096` | 4,747 / 5,029 | 356 / 382 | 0.075 / 0.076 |
| `intern_hit_1048576` | 1,271,781 / 1,320,706 | 123,425 / 126,387 | 0.098 / 0.095 |
| `intern_miss_4096` | 12,198 / 12,405 | 2,306 / 2,389 | 0.187 / 0.197 |
| `intern_middle_4096` | 12,267 / 12,852 | 2,239 / 2,401 | 0.186 / 0.182 |
| `gsub_callback_wide` | 631,029 / 642,632 | 285,925 / 285,669 | 0.452 / 0.446 |
| `long_concat` | 5,570,246 / 5,703,828 | 962,332 / 965,116 | 0.172 / 0.167 |
| `long_rep` | 27,567,258 / 27,965,454 | 22,268,156 / 22,353,050 | 0.807 / 0.804 |

Lookup timings include the full identity lookup, not just a copied hash function.
Misses include object allocation/copying and table growth. Long strings are fully
hashed and compared. Middle-change inputs share their leading/trailing bytes.
Small nanosecond values are integer-quantized and depend on the fixed corpus's
probe paths; they do not establish a universal tiny-string speedup. The host is
shared and not CPU-isolated.

Each run also measures five alternating pairs of fresh processes executing
400,000 six-match substitutions with 513-byte callback results. Assertions verify
the aggregate result. These include spawn, compiler/VM startup, execution and
shutdown. Median values and paired current/parent ratios are:

| Counter | Parent first / confirm | Current first / confirm | Current/parent |
| --- | ---: | ---: | ---: |
| Wall ms | 2,549.756 / 2,617.478 | 1,181.828 / 1,177.098 | 0.469 / 0.449 |
| Process CPU ms | 2,500.000 / 2,531.250 | 1,171.875 / 1,140.625 | 0.466 / 0.446 |
| Process cycles, billion | 5.504 / 5.572 | 2.533 / 2.526 | 0.463 / 0.444 |

Process CPU combines user/kernel time across threads using [GetProcessTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes).
[QueryProcessCycleTime](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryprocesscycletime) supplies process cycle counts. Cycles are retained as counts, not
converted into elapsed time. Both queries run after each validated child exits.
Windows blocked WPR CPU call-stack sampling with policy error 0xc5585011;
[the failed attempt](2026-10-26-rust-lua-intern-hash-profile-attempt.json) records that no samples were collected.
The process counters are separate measurements; no sampled hotspot attribution
is claimed.

All paired allocation/reallocation/requested-byte totals match the parent for
every fixture and sample in both runs. Every recorded live/reserved/observed-peak
heap estimate also matches, including collection/reclamation counts. No string,
VM, frame or instruction field is added. The VM remains 4,432 B, Value 16 B and
the x64 frame 64 B. The safe block loads reduce the serial byte-multiply chain;
the storage and full byte equality check remain unchanged.

Selected first-run estimates, identical between parent/current:

| Fixture | Live B | Reserved B | Observed peak B | Collections |
| --- | ---: | ---: | ---: | ---: |
| `intern_hit_1048576` | 1,065,996 | 1,071,084 | 1,071,084 | 0 |
| `intern_miss_4096` | 544,764 | 552,972 | 552,972 | 0 |
| `gsub_callback_wide` | 43,754 | 91,946 | 94,678 | 107 |
| `long_concat` | 31,886 | 37,262 | 37,262 | 0 |
| `long_rep` | 154,918 | 160,326 | 160,326 | 0 |

These are capacity-based VM estimates, excluding VM headers, transient compiler
buffers, allocator metadata and OS/CRT storage. They are not process RSS or exact
allocator peaks. Hit loops allocate nothing; miss/script allocation totals and
all other heaps are retained in JSON/TSV. Exact live allocator/RSS, call-stack
profiles and other Rust-engine comparisons remain open.

Two independent reference runs retain 81 runtime and 22 frontend workloads,
five alternating paired samples each. Selected complete-script medians below
are nanoseconds; C uses the development official Lua 5.1.5 binding:

| Fixture | Rust first / confirm | C first / confirm | Rust/C duration |
| --- | ---: | ---: | ---: |
| `gsub_callback_wide` | 311,010 / 305,070 | 115,605 / 110,200 | 2.702 / 2.768 |
| `pattern_replace` | 2,646,570 / 2,736,136 | 653,866 / 650,636 | 4.096 / 4.143 |
| `pattern_callback` | 2,819,500 / 2,932,596 | 716,350 / 700,260 | 3.899 / 4.261 |
| `string_numeric_args` | 3,008,004 / 2,917,002 | 1,077,424 / 1,042,924 | 2.774 / 2.843 |
| `entry_args` | 161 / 178 | 144 / 142 | 1.118 / 1.246 |

These compare Rust/C durations under this harness, not the separate parent
executable comparison. All 103 prior instruction/register layouts match.
Cold compile/startup samples, allocations and heap observations remain in the
full JSON/TSV. Substantial interpreter, native callback and numeric formatting
costs remain; this does not establish overall engine superiority or complete
compatibility.

```sh
python scripts/benchmark_rust_lua_intern.py --parent 2a4dbb99c --cpu-time --output docs/performance/2026-10-26-rust-lua-intern-hash-compare.json
python scripts/benchmark_rust_lua_intern.py --parent 2a4dbb99c --cpu-time --output docs/performance/2026-10-26-rust-lua-intern-hash-compare-confirm.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-intern-hash.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-intern-hash-confirm.json
```

Raw comparisons: [JSON](2026-10-26-rust-lua-intern-hash-compare.json), [TSV](2026-10-26-rust-lua-intern-hash-compare.tsv), [harness](2026-10-26-rust-lua-intern-hash-compare.rs), [build](2026-10-26-rust-lua-intern-hash-compare.build.log),
[confirmation JSON](2026-10-26-rust-lua-intern-hash-compare-confirm.json), [TSV](2026-10-26-rust-lua-intern-hash-compare-confirm.tsv), [harness](2026-10-26-rust-lua-intern-hash-compare-confirm.rs), [build](2026-10-26-rust-lua-intern-hash-compare-confirm.build.log).
Reference runs: [JSON](2026-10-26-rust-lua-intern-hash.json), [TSV](2026-10-26-rust-lua-intern-hash.tsv), [build](2026-10-26-rust-lua-intern-hash.build.log),
[confirmation JSON](2026-10-26-rust-lua-intern-hash-confirm.json), [TSV](2026-10-26-rust-lua-intern-hash-confirm.tsv), [build](2026-10-26-rust-lua-intern-hash-confirm.build.log).
All seven current-source maps match the final tree; parent source maps match
Git archive 2a4dbb99c80298b8337ee7ed695955da6767ee5c. Reference-run source digest:
4a26b054e73f923e8582c09b3b3e1a12ac02e56ac00bd7d52d67199dbbe75cfb. See [validation](../testing/2026-10-26-rust-lua-intern-hash.md) for raw failures
and the unfinished full engine/integration objective.
