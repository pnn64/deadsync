# String and table measurements

The benchmark now includes six string/pattern/sort workloads alongside the six
original interpreter workloads. Every workload checks its scalar result against
the official vendored Lua 5.1.5 interpreter before timing. Five samples alternate
Rust/reference order; ratios are medians of paired wall times, with lower better.
The platform remains x86-64 Windows MSVC, static CRT and the C locale.

| Workload | Initial libraries | Reused buffers/partitions | Confirmation |
| --- | ---: | ---: | ---: |
| Cached pattern search | 0.91 | 0.85 | 0.83 |
| String replacement | 4.62 | 2.60 | 2.59 |
| Native replacement callback | 3.06 | 2.67 | 2.73 |
| String formatting | 1.32 | 1.04 | 1.08 |
| Numeric table sorting | 1.99 | 1.96 | 1.83 |
| Lua comparator sorting | 1.77 | 1.85 | 1.77 |

Literal replacement writes captures directly into an output buffer. Buffers are
reused across calls and kept separately while nested callbacks execute. Sort
partitions use bounded inline storage, avoiding an allocation per sort and growth
during partitioning. The implementation remains safe Rust with zero normal/build
dependencies.

For each script invocation, the replacement workload performs 1,000 `gsub` calls
with two matches each. Its median Rust duration fell from 3.52 ms to 1.99 ms,
about 43%; allocation/reallocation events fell from 6,000 to zero after warming.
The callback replacement workload fell from 3,000 events to zero. Pattern search
also records zero warmed events. Formatting still records 5,000 events per
1,000-call script, down from 7,000. Sorting scripts create 100 fresh 128-element
tables: their events fell from approximately 900 to 600, with table storage still
allocated. These are benchmark allocator observations, not unit-test assertions.

The confirmation ratios for the original workloads are 6.00 (numeric `for`),
5.18 (numeric `while`), 3.40 (arrays), 3.31 (recursive Fibonacci), 3.09 (captured-cell
calls) and 2.60 (byte concatenation). Their implementations were not optimized in
this stage. Uncontrolled machine load and code layout can affect these timings;
the runs are not CPU-isolated, so the shifts do not establish loop improvements.
Warmed numeric loops retain zero allocation/reallocation events.

Raw samples, source hashes, toolchain, machine and reference metadata:

- [Initial libraries](2026-10-24-rust-lua-stdlib-before.json)
- [Reused buffers and partitions](2026-10-24-rust-lua-stdlib.json)
- [Confirmation](2026-10-24-rust-lua-stdlib-confirm.json)

Each report has a matching TSV and build log. Reproduce with:

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The allocator counts Rust allocations only; C Lua allocations are not counted by
that instrumentation. Rust objects are currently retained until VM destruction,
while the reference has GC. This difference affects memory behavior and timing,
particularly table-heavy workloads. Cached patterns and pooled buffers also
remain alive. Live/peak Rust heap, process RSS, CPU cycles, full-library/gameplay
workloads and comparisons with other Rust engines remain unmeasured. These are
partial engine microbenchmarks, not an overall compatibility or speed claim.

All 31 differential/adversarial groups, the verifier test and documentation
example pass. Strict Clippy passes, and the production dependency tree contains
only `deadlib-lua`. The [official suite report](../testing/2026-10-24-rust-lua-stdlib-suite.md)
records three passing files and eight failures at missing functions.
