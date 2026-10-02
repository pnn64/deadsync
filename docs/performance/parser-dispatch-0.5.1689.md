# Parser dispatch performance 0.5.1689

Parent: `05256d0214d6c7ece0524999cd49e2831b069656` (0.5.1688).
Date: 2026-10-02.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE and
M-THROUGHPUT guidance to three string-processing paths:

1. Find chart attack `TIME=` markers by searching for the two ASCII field
   initials (`T`/`t`) in bulk, then checking the following four bytes. Keep a
   small scalar path for inputs of at most 32 bytes and check the first window
   directly, preserving the common immediate hit. When consecutive false
   initials are at most eight bytes apart, resume the scalar scan to avoid
   repeated byte-search setup on dense candidates. Search offsets and embedded/
   adjacent markers keep their parent behavior. The bulk helper stays out of
   the immediate-hit path even under LTO. Gameplay now depends on the already-locked
   `memchr` 2.8.3 package, using its portable search implementation and platform
   acceleration; no new package version was added to Cargo.lock.
2. Reject ordinary modifier names before trying to parse a C/X/M speed value.
   Recognize an ASCII speed prefix first, then borrow the remainder directly
   instead of validating its UTF-8 again. Suffix `x` parsing keeps its original
   precedence and finite/positive checks; prefix parsing retains its distinct
   NaN, infinity and zero behavior. Both chart and Lua modifier parsers benefit.
3. Borrow Lua selector keys with no uppercase ASCII bytes. Short keys avoid
   stack copying, case conversion and UTF-8 validation; keys longer than the
   32-byte stack buffer also avoid the owning lowercase fallback. Mixed-case
   keys retain their stack/heap paths and Unicode bytes retain their original
   ASCII-only folding. Short keys use an early-exit uppercase check. Long keys
   starting with uppercase ASCII take the existing fallback immediately; other
   long keys use a bitwise boolean reduction that LLVM can scan in bulk. This
   shared helper serves effect modes/clocks, text
   alignment/glow, theme preferences and screen fallbacks.

All public signatures remain unchanged. No new unsafe code was added. Attack
scanning and modifier parsing already avoided heap traffic; these changes
reduce their CPU work. Folded Lua keys now avoid the old long-key allocation.

## Measurement method

Intel Xeon E5-2696 v4 (22 cores/44 logical processors), Windows x86-64,
Rust/Cargo 1.98.1, release opt-level 3 and full LTO. Six serial rounds alternate
old-first/new-first after validation and builds finish. Each round reports
medians and ranges of seven timing samples. QueryThreadCycleTime records the
calling thread's CPU cycles; measured operations do not spawn worker threads.
Other host activity was not isolated.
Three warm-up operations precede each timed case.

The [raw CSV](parser-dispatch-0.5.1689.csv) contains 252 measurements for 21
paired workloads across gameplay and song Lua: elapsed time, cycles,
useful-item throughput, allocations, reallocations, frees, requested bytes and
freed bytes. The existing thread-local System allocator wrapper counts heap
traffic in a separate operation after timing. Every heap sample balances,
counts/bytes are stable across rounds, and no case reallocates. Requested bytes
measure allocator traffic rather than peak RSS or allocator metadata. Negative
reductions mean slower. CPU reductions and throughput increases are medians
of paired round ratios; ns/op values are separate six-round medians. Their
ratios can differ. Tables include complete cycle-change round ranges.

Fixtures are prepared outside timing/counting. Every operation destroys any
owning result inside both measurements. Eleven original function bodies are
frozen from the parent and verified modulo function naming/formatting. The
original inline annotations are retained on the search, speed and folding
helpers. Old full modifier parsers call the old speed decoder; old Lua
selectors/callbacks call the old folding helper. Shared numeric/modifier
application, chunk parsing and normalized selector handlers are unchanged.

These synthetic cases isolate dispatch and allocation behavior. They do not
measure Lua VM execution, end-to-end song loading, frame latency or gameplay
FPS. Timing/cycle overhead affects tiny helpers equally in both variants.

## Chart attack marker search

One operation finds the first `TIME=` marker, returning its byte offset.
Hits are at the end of each input except the early-hit control, which places
the marker first. Misses inspect the complete input. Throughput counts searches
per second. Cases include tiny spans, 4096-byte spans, 64 KiB spans, dense false
`XIME=` markers, dense false `TIME_` initials and an early hit. These searches feed both owned and borrowed
chart attack preparation paths. Unicode is covered by behavior comparisons.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `scan_16_hit` | 15.2 -> 15.6 | -4.8% (-8.7..-1.5) | -4.7% | 0 -> 0 | 0 -> 0 |
| `scan_32_miss` | 26.2 -> 26.7 | -5.2% (-9.1..-1.9) | -5.0% | 0 -> 0 | 0 -> 0 |
| `scan_4096_hit` | 3720.2 -> 104.7 | 97.2% (96.8..97.3) | 35.0x | 0 -> 0 | 0 -> 0 |
| `scan_65536_miss` | 59596.1 -> 1668.8 | 97.1% (96.7..97.3) | 35.2x | 0 -> 0 | 0 -> 0 |
| `scan_65536_false` | 59098.1 -> 1880.0 | 96.8% (96.4..97.1) | 31.7x | 0 -> 0 | 0 -> 0 |
| `scan_65536_false_initials` | 96894.6 -> 102263.3 | -5.7% (-8.0..-2.9) | -5.4% | 0 -> 0 | 0 -> 0 |
| `scan_16_front` | 4.9 -> 4.7 | 3.5% (0.0..4.1) | 3.6% | 0 -> 0 | 0 -> 0 |

## Speed dispatch and full modifier parsers

One decoder operation handles an ordinary modifier, a CMod, an XMod suffix or
an invalid numeric speed. Throughput counts tokens per second. Full chart and
Lua parser operations handle eight ordinary modifiers: drunk, reverse, tiny,
bumpy, boost, flip, hidden and sudden. These full-result cases include unchanged
token splitting, normalization and modifier application as well as the faster
speed rejection. Every measured decoder/parser operation has zero heap churn.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `scroll_ordinary` | 36.0 -> 8.8 | 74.7% (69.5..78.2) | 307.3% | 0 -> 0 | 0 -> 0 |
| `scroll_prefixed` | 28.5 -> 25.8 | 11.3% (-3.9..39.5) | 11.3% | 0 -> 0 | 0 -> 0 |
| `scroll_suffix` | 23.9 -> 25.6 | -1.7% (-14.4..0.4) | -1.5% | 0 -> 0 | 0 -> 0 |
| `scroll_invalid` | 28.5 -> 18.8 | 34.7% (16.9..45.8) | 54.2% | 0 -> 0 | 0 -> 0 |
| `mods_chart` | 1033.1 -> 758.0 | 26.9% (5.0..33.3) | 36.6% | 0 -> 0 | 0 -> 0 |
| `mods_lua` | 885.4 -> 743.0 | 20.5% (-5.1..33.6) | 26.2% | 0 -> 0 | 0 -> 0 |

## Lua selector dispatch

One direct key operation folds/borrows its string, invokes a borrowed-key
callback and drops any internal owning string. Inputs are a short lowercase
key, its mixed-case spelling, UTF-8 text without uppercase ASCII, and 128-byte
lowercase/mixed-case keys. Throughput counts keys per second. Full selector
operations call effect-mode, effect-clock, text-alignment and text-glow parsers
once each; throughput counts four selector calls per operation. The long clock
is a 128-byte name ending in `beat`, which the existing substring fallback
accepts as a beat clock. Its complete parser now avoids allocation. Names are
not truncated or restricted by a new length limit. Mixed-case long names keep
the parent one-allocation budget.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `overlay_key_short_lower` | 23.5 -> 13.3 | 42.2% (40.5..50.2) | 74.0% | 0 -> 0 | 0 -> 0 |
| `overlay_key_short_mixed` | 22.8 -> 25.2 | -7.2% (-31.0..-7.2) | -6.9% | 0 -> 0 | 0 -> 0 |
| `overlay_key_unicode` | 27.9 -> 12.6 | 54.9% (49.0..56.6) | 124.0% | 0 -> 0 | 0 -> 0 |
| `overlay_key_long_lower` | 82.0 -> 10.8 | 86.9% (86.7..87.5) | 675.7% | 1 -> 0 | 128 -> 0 |
| `overlay_key_long_mixed` | 76.8 -> 82.8 | -4.2% (-32.1..1.6) | -4.1% | 1 -> 1 | 128 -> 128 |
| `overlay_selectors_lower` | 151.1 -> 106.5 | 30.3% (10.1..33.7) | 44.6% | 0 -> 0 | 0 -> 0 |
| `overlay_selectors_mixed` | 152.8 -> 154.7 | 0.3% (-20.4..16.6) | 0.4% | 0 -> 0 | 0 -> 0 |
| `overlay_clock_long` | 131.2 -> 46.6 | 64.8% (61.3..67.5) | 187.8% | 1 -> 0 | 128 -> 0 |

## Control costs and scope

The short tail-hit/miss scans have about 5% median CPU overhead from dispatch;
the immediate-hit control improves slightly. Dense false `TIME_` initials
fall back to scalar scanning and have 5.7% median CPU overhead, bounding the
cost of candidate-search setup. Short and long mixed-case key controls have
7.2% and 4.2% median CPU overhead respectively; their parent heap budgets are
preserved. The complete mixed-case selector result is within timing noise.
The suffix-speed control has a 1.7% median cycle regression and no heap change.

Long sparse/delimiter-dense scans and folded-key gains hold in every measured
round. Full modifier parser timings vary more: the Lua parser has one round
with a cycle regression despite its positive six-round median. These results
support the three targeted changes; they do not establish a universal speedup
for every input or an end-to-end application improvement. All 21 workloads
retain or reduce heap traffic.

## Behavior checks and reproduction

2248 debug/profile cases pass (101 ignored), including six new
regression tests. Byte search is compared at all offsets of short inputs,
SIMD-size boundaries, invalid starts, dense delimiters/initials, all 256 byte
values at every position of the marker, random arbitrary bytes,
mixed case, Unicode and embedded markers; complete chart chunks preserve their
parent boundaries. Speed decoders compare variant/float bits for ordinary
tokens, malformed speeds, whitespace, signed zero, subnormals, overflow, NaN
and infinities. Both full modifier parsers preserve ordering, aliases,
clearall and amount/approach semantics. Lua folding is compared across the
32-byte threshold, Unicode and 256 deterministic varied strings; all four
selectors, screen fallback and theme preference callbacks match the parent.
Heap checks cover the unchanged zero-allocation search/parsers, borrowed long
keys/clocks and the mixed-case allocation budget.

860 gameplay and 763 song Lua release unit cases pass (94
ignored). Clippy completes for gameplay, notefield and song Lua with existing
repository warnings and no warnings from the new test/baseline files.
3 available native multitap parity cases pass. The external
`multitap::edgar_countdown_onsets_and_hit_commands` fixture remains unavailable
because `C:\GitHub\lua-songs` is absent; the command below excludes that case.
Authored native trace fixtures are unchanged.

```powershell
cargo test -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --locked
cargo clippy -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --all-targets --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity multitap --locked -- --skip edgar_countdown_onsets_and_hit_commands
cargo test --release -p deadsync-gameplay -p deadsync-song-lua --lib --locked
cargo test --release -p deadsync-gameplay --lib parser_dispatch_perf::benchmark_parser_dispatch -- --exact --ignored --nocapture --test-threads=1
cargo test --release -p deadsync-song-lua --lib key_dispatch_perf::benchmark_key_dispatch -- --exact --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat both benchmarks in reversed order; alternate for six rounds.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
