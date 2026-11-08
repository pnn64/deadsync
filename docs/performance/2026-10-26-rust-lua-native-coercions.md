# Lua 5.1 native coercion and substitution measurements

Two runs use five alternating paired samples across 81 runtime and 22 frontend
workloads on shared Windows x64 MSVC hardware, static CRT, binary64 and C locale.
The development official Lua 5.1.5 binding is the reference. The host is not
CPU-isolated; these runs establish neither a before/after speedup nor rankings
against other Rust engines. Compilation and initial warmup are separate.

The new debug_gsub_args fixture performs 100 three-match substitutions, replacing
its callback after the first match. gsub_callback_wide performs 100 six-match
substitutions with 513-byte callback results. string_numeric_args performs 1,000
numeric-source substrings and radix conversions. Inputs, closures, result strings,
debug mutation and host/library adapters are included. Both engines check the
aggregate result before measurement; the C adapter opens debug for debug_* cases.

Median nanoseconds per complete call/script, first / confirmation; ratios are
medians of five paired Rust/C durations:

| Workload | Rust ns | C binding ns | Rust/C duration |
| --- | ---: | ---: | ---: |
| `debug_gsub_args` | 199,615 / 162,030 | 63,260 / 50,990 | 3.156 / 3.176 |
| `gsub_callback_wide` | 762,260 / 651,035 | 126,070 / 107,355 | 6.092 / 6.105 |
| `string_numeric_args` | 2,987,214 / 2,683,808 | 1,064,422 / 954,692 | 2.813 / 2.832 |
| `pattern_replace` | 2,919,863 / 2,530,663 | 734,263 / 617,403 | 4.028 / 4.119 |
| `pattern_callback` | 3,072,890 / 2,686,493 | 759,900 / 656,076 | 4.052 / 4.071 |
| `debug_hook` | 82,700 / 85,200 | 36,620 / 37,110 | 2.271 / 2.307 |
| `entry_args` | 174 / 146 | 156 / 130 | 1.154 / 1.123 |

Rust GlobalAlloc events per complete call/script, ranges across five samples:

| Workload | Run | Allocations | Reallocations | Requested B |
| --- | --- | ---: | ---: | ---: |
| `debug_gsub_args` | first | 2.000 | 0.000-0.800 | 12.000-933.600 |
| `debug_gsub_args` | confirmation | 2.000 | 0.000-0.800 | 12.000-933.600 |
| `gsub_callback_wide` | first | 100.000-100.150 | 0.000-1.300 | 400.000-5,318.000 |
| `gsub_callback_wide` | confirmation | 100.000-100.150 | 0.000-1.300 | 400.000-5,318.000 |
| `string_numeric_args` | first | 6,000.000 | 6,000.000 | 169,000.000 |
| `string_numeric_args` | confirmation | 6,000.000 | 6,000.000 | 169,000.000 |
| `pattern_replace` | first | 0.000 | 0.000 | 0.000 |
| `pattern_replace` | confirmation | 0.000 | 0.000 | 0.000 |
| `pattern_callback` | first | 0.000 | 0.000 | 0.000 |
| `pattern_callback` | confirmation | 0.000 | 0.000 | 0.000 |
| `debug_hook` | first | 52.000-52.400 | 35.000-37.100 | 10,094.000-12,258.000 |
| `debug_hook` | confirmation | 52.000-52.400 | 35.000-37.100 | 10,094.000-12,258.000 |
| `entry_args` | first | 0.000 | 0.000 | 0.000 |
| `entry_args` | confirmation | 0.000 | 0.000 | 0.000 |

Counts omit C malloc and allocator metadata. Requested bytes are requests, not
live memory. Numeric formatting still allocates temporary strings; cold native
coercion and matching are not allocation-free. Partial buffer strings and their
reductions are represented explicitly for observable C-stack behavior. The old
replacement root is removed; stack heights and a flat pending-action vector are
held in substitution jobs. Its capacity is included in heap estimates. Pending
actions allocate only for buffer concatenation callbacks, not ordinary matches.
Inspected/partial native result calls stage arguments/results in existing register
storage. VM metadata remains 4,432 B and Value 16 B; the x64 frame stays 64 B.
All 100 prior instruction/register layouts match, with three new program layouts.

First-run capacity estimates after samples and explicit collection:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections |
| --- | --- | ---: | ---: | ---: | ---: |
| `debug_gsub_args` | before_collect | 39,280 | 49,984 | 49,984 | 0 |
| `debug_gsub_args` | after_collect | 30,501 | 56,261 | 56,261 | 1 |
| `gsub_callback_wide` | before_collect | 68,276 | 94,124 | 94,632 | 12 |
| `gsub_callback_wide` | after_collect | 32,635 | 87,675 | 94,632 | 13 |
| `string_numeric_args` | before_collect | 19,696 | 25,088 | 25,088 | 0 |
| `string_numeric_args` | after_collect | 25,075 | 30,531 | 30,531 | 1 |
| `pattern_callback` | before_collect | 23,545 | 28,873 | 28,873 | 0 |
| `pattern_callback` | after_collect | 28,878 | 34,318 | 34,318 | 1 |
| `entry_args` | before_collect | 18,387 | 23,827 | 23,827 | 0 |
| `entry_args` | after_collect | 23,827 | 29,267 | 29,267 | 1 |

These estimates exclude VM headers, transient compiler/export buffers, OS/CRT
storage and allocator metadata. They are not process RSS or exact allocator peaks.
Counts include warmup and are cumulative. Confirmation snapshots and all other
samples remain in JSON. C allocations, RSS, CPU cycles and broader host/native
scaling are unmeasured. Fully bounded native execution and collection remain open.

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-native-coercions.json
python scripts/benchmark_rust_lua.py --output docs/performance/2026-10-26-rust-lua-native-coercions-confirm.json
```

Raw data: [JSON](2026-10-26-rust-lua-native-coercions.json), [TSV](2026-10-26-rust-lua-native-coercions.tsv), [build log](2026-10-26-rust-lua-native-coercions.build.log),
[confirmation JSON](2026-10-26-rust-lua-native-coercions-confirm.json), [confirmation TSV](2026-10-26-rust-lua-native-coercions-confirm.tsv),
[confirmation build](2026-10-26-rust-lua-native-coercions-confirm.build.log). Both VM runs have source digest
1c5300b210a5ae618466d200b22f3aed89fce8166f47095b671f52950f996d0a; all five recorded source-hash maps match the final tree.
Executable hashes identify observed builds, which later Cargo invocations may
replace. The [validation report](../testing/2026-10-26-rust-lua-native-coercions.md) retains raw failures,
contract limits and the unfinished full engine/integration objective.
