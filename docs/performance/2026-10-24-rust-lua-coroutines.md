# Coroutine measurements

The reference harness now includes coroutine resume, wrapper, deep-yield and
persistent-thread reuse workloads. Each script checks its result against the
official vendored Lua 5.1.5 interpreter before timing. Five paired samples
alternate Rust/reference order on Windows x86-64 MSVC, static CRT, in the C locale.
Ratios are medians of paired wall times; lower is better.

| Coroutine workload | Stack-scan boundary check | Cached boundary check | Confirmation | Reuse run |
| --- | ---: | ---: | ---: | ---: |
| Create, resume and yield 1,000 times | 2.28 | 2.42 | 2.30 | 2.34 |
| Create wrapper and yield 1,000 times | 2.95 | 2.94 | 2.92 | 2.95 |
| Yield under 128 Lua frames | 2.61 | 2.38 | 2.38 | 2.39 |
| Resume an existing thread 1,000 times | — | — | — | 2.82 |

The cached check removes a scan of all active frames on each yield. Frames inherit
their native-boundary depth; fixed result counts use a 16-bit field with an open-
result sentinel. A compile-time assertion preserves 64-byte frames on 64-bit
targets. The deep-yield ratio improved by about 9% in these samples, while the
shallow coroutine ratios are broadly similar. The machine is not CPU-isolated;
unrelated compiler activity affected the initial run, so absolute before/after
times and changes in unrelated workloads do not establish improvements.

The persistent-thread script records zero warmed Rust allocation/reallocation
events in all five samples. It creates the thread before measured execution,
retains it globally, then resumes/yields 1,000 times per invocation. Scripts that
create a new coroutine record approximately four allocation/reallocation events
per invocation; the deep variant records approximately eighteen, including stack
and arena growth. This is allocation instrumentation, not a unit-test assertion.
Values remain 16 bytes. Numeric loops and the warmed pattern search/replacement
workloads also retain zero events.

The reuse run includes sixteen total workloads. Its other Rust/reference ratios
are 5.85 (numeric `for`), 5.18 (numeric `while`), 3.39 (arrays), 3.59 (recursive
Fibonacci), 3.41 (captured-cell calls), 2.61 (byte concatenation), 0.83 (pattern
search), 2.78 (replacement), 2.81 (replacement callback), 1.07 (formatting),
1.87 (numeric sorting) and 1.96 (Lua comparator sorting). Most workloads remain
slower than the reference; these are partial engine microbenchmarks.

The final run includes the completed overflow-lifecycle and native-environment
fixes. Its coroutine ratios are 2.35 (resume), 2.78 (wrap), 2.43 (deep yield) and
2.89 (persistent reuse); reuse again records zero allocation/reallocation events
in every sample. The deep-yield ratio remains below the initial 2.61, with normal
run-to-run variation visible in the other workloads. No overall speedup is claimed.

Raw samples and source/build/machine metadata:

- [Initial coroutine implementation](2026-10-24-rust-lua-coroutines-before.json)
- [Cached boundary check](2026-10-24-rust-lua-coroutines.json)
- [Confirmation](2026-10-24-rust-lua-coroutines-confirm.json)
- [Persistent-thread workload added](2026-10-24-rust-lua-coroutines-reuse.json)
- [Final implementation](2026-10-24-rust-lua-coroutines-final.json)

Each JSON has a matching TSV and build log. The first three reports contain
fifteen workloads; the reuse/final reports contain sixteen. The final report also
includes overflow-lifecycle and native-environment correctness fixes discovered
by the expanded differential tests. Reproduce with:

```sh
python scripts/benchmark_rust_lua.py --output docs/performance/rust-lua-current.json
```

The allocator counts Rust allocations only, excluding native C malloc. Rust
threads/heap objects currently remain alive until VM drop, while the reference
has GC. Startup library surfaces still differ. Live/peak Rust heap, process RSS,
CPU cycles, full-library/gameplay workloads and other Rust engines remain
unmeasured. Broader compatibility and performance gates remain open; see the
[coroutine validation report](../testing/2026-10-24-rust-lua-coroutines.md).
