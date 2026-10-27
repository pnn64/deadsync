# Lua locale measurements

The harness retains 53 runtime and twenty-two frontend workloads, five
alternating paired Rust/C samples and a separate confirmation run. The oracle
is official vendored Lua 5.1.5 on Windows x86-64 MSVC, static CRT and binary64.
Ordinary workloads start in C locale; each locale fixture selects its category
and restores the previous setting. The shared host is not CPU-isolated. There
is no paired parent timing claim: the parent cannot execute the new fixtures.

`locale_numeric` selects `ptb` for numeric conversion, then parses `12,5` and
formats it to `12,50` one thousand times. It returns 1,000 successful byte checks.
`locale_ctype` selects `ptb` for character operations, repeatedly uppercases four
bytes (233, 225, 65, 97) and replaces their `%a` matches. One thousand iterations
return 8,000 total bytes/matches. Ten scripts run per sample. Locale changes,
native cache setup, interpreted work and restoring the old locale remain inside
timing. Compilation and scalar checks occur outside it.

Durations are milliseconds per complete script, first run / confirmation.
Engine durations are medians; ratios are medians of the five paired ratios.

| Workload | Rust ms | C Lua 5.1 ms | Rust/C duration |
| --- | ---: | ---: | ---: |
| `locale_numeric` | 1.132 / 1.166 | 1.090 / 0.973 | 1.238 / 1.172 |
| `locale_ctype` | 1.576 / 1.706 | 0.737 / 0.757 | 2.107 / 2.259 |
| `string_format` | 0.994 / 0.920 | 0.809 / 0.728 | 1.235 / 1.260 |
| `gc_steps` | 0.370 / 0.421 | 0.321 / 0.365 | 1.128 / 1.094 |
| `gc_mark_mutation` | 3.019 / 3.078 | 0.440 / 0.449 | 7.086 / 7.072 |
| `constant_left` | 1.046 / 0.973 | 0.262 / 0.249 | 3.998 / 3.971 |

Both locale kernels remain slower than C. These are whole-script measurements,
not per-call latency or full locale/standard-library parity. Existing slower
controls remain visible; no overall speedup is inferred from historical samples.

Every sample requests 5,007 Rust allocations, zero reallocations and 20,032 bytes
per numeric script; the character script requests seven allocations, two
reallocations and 214 bytes. Decimal normalization and locale punctuation own
temporary buffers on the non-C path. Character maps are prepared at locale
change and reused for ordinary byte work; numeric/time-only changes do not
rebuild them. C-locale number formatting, case conversion and ordering retain
Rust fast paths. Non-C string ordering calls the CRT through a locked boundary.
No third-party dependency or C Lua runtime is linked into the new engine.

Startup/drop now requests 243 allocations, 63 reallocations and 39,651 bytes per
Rust state: one allocation and nine bytes more than the prior milestone for
the new builtin name. Calendar-table samples remain 2,000 allocations, zero
reallocations and 352,000 bytes per 1,000-table script. Counters cover Rust
`GlobalAlloc`; CRT/native C allocations, allocator headers and global static
storage are excluded. The shared atomic character map occupies 1,024 static
bytes, with additional revision/flags and decimal-cache storage. A per-VM
pattern revision adds header storage that heap estimates exclude.

Heap observations match across both runs:

| Workload | Phase | Live B | Reserved B | Observed peak B | Collections | Reclaimed |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `locale_numeric` | before_collect | 20,915 | 26,243 | 26,243 | 0 | 0 |
| `locale_numeric` | after_collect | 26,300 | 31,676 | 31,676 | 1 | 2 |
| `locale_ctype` | before_collect | 24,502 | 29,798 | 29,798 | 0 | 0 |
| `locale_ctype` | after_collect | 29,827 | 35,235 | 35,235 | 1 | 5 |

Snapshots follow all samples and explicit full collection; counts include
warmup and are cumulative. Three standard-stream userdata remain. The collector
allocates retained working storage during that first collection, so its capacity
estimate can rise even while objects are reclaimed. Estimates are not allocator
high-water marks or RSS. Global locale caches and CRT-owned storage are outside
these per-VM estimates.

All 73 previous program layouts match. Numeric/character fixtures emit
50/58 instructions and use 10/13 entry registers. `Value` remains sixteen bytes;
the x86-64 frame-size assertion remains 64 bytes. Both benchmark runs have source
digest `9c357a4173a9736e4ef81992fb2828dc8b4ec77f53e3aa3f43a7e6816dc5c614`; all 79 recorded source/configuration
hashes match the final tree.

Raw data: [JSON](2026-10-25-rust-lua-locale.json), [TSV](2026-10-25-rust-lua-locale.tsv), [build log](2026-10-25-rust-lua-locale.build.log),
[confirmation JSON](2026-10-25-rust-lua-locale-confirm.json), [confirmation TSV](2026-10-25-rust-lua-locale-confirm.tsv),
[confirmation build log](2026-10-25-rust-lua-locale-confirm.build.log).
The [validation report](../testing/2026-10-25-rust-lua-locale.md) records 366 tests, 126 process
probes and fourteen official Rust passes out of sixteen files.

Other locale/encoding matrices, external host locale changes, non-Windows native
locales, exact allocator peaks, RSS, cycles, native allocation counts, pause
distributions and other Rust engines remain unmeasured. Binary chunks/modules,
remaining GC/diagnostic/resource work, caches, host APIs, later dialects, optional
JIT and integrations remain unfinished.
