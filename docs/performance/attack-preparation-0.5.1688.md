# Attack preparation performance 0.5.1688

Parent: `cd3fc61634525c0b85552474d9986185839d5090` (0.5.1687).
Date: 2026-10-02.

This pass applies `rust-performance.md`'s M-HOTPATH, M-MEM-REUSE,
M-INITIAL-CAPACITY and M-THROUGHPUT guidance to three attack preparation paths:

1. Borrow modifier names that already begin with a lowercase ASCII letter
   and contain only lowercase ASCII letters and digits. This removes stack
   copying and UTF-8 validation from normalized short keys, and removes the
   heap fallback for normalized keys longer than 128 bytes. Other spellings
   retain the original leading-digit stripping, ASCII filtering and lowercase
   conversion. Both chart and song Lua modifier parsers use the borrowed keys.
2. Defer chart mask output reservation and the chunk-counting scan until the
   first usable window. Malformed, zero-length and unsupported attacks can
   produce an empty vector without any heap allocation. When output exists,
   reserve exactly the original raw-chunk count before the first push, keeping
   the parent capacity and allocation budget even for a one-window result.
3. Extend column offset tails directly when windows are already ordered by
   column, target and total-ordered start time. This removes identity index
   construction, sorting and heap scratch above the 256-index stack threshold.
   Unordered inputs retain the original index sort and original-index tie
   order; nonfinite starts retain the original scalar fallback. The shared
   tail loop preserves the parent's epsilon, default-end and minimum arithmetic.

These operations prepare gameplay attacks and song Lua column windows.
Public signatures, dependencies and the private key enum's two-variant shape
remain unchanged. No new unsafe code was added.

## Measurement method

Intel Xeon E5-2696 v4 (22 cores/44 logical processors), Windows x86-64,
Rust/Cargo 1.98.1, release opt-level 3 and full LTO. Six serial rounds alternate
old-first/new-first after validation and builds finish. Each round reports
medians and ranges of seven timing samples. QueryThreadCycleTime records the
calling thread's CPU cycles. Other host activity was not isolated.

The [raw CSV](attack-preparation-0.5.1688.csv) contains 240 measurements for 20
paired workloads: elapsed time, cycles, useful-item throughput, allocations,
reallocations, frees, requested bytes and freed bytes. The existing thread-local
System allocator wrapper counts heap traffic in a separate operation after
timing. Every heap sample balances, counts/bytes are stable across rounds, and
none reallocates. Requested bytes measure allocator traffic rather than peak
RSS or allocator metadata. Negative reductions mean slower. Cycle reductions
and throughput increases are medians of paired round ratios; ns/op values are
separate six-round medians. Their ratios can differ. Tables include complete
cycle-change round ranges for faster paths and controls.

Six original function bodies are frozen from the parent and verified modulo
function/enum variant naming and formatting. The old mask builder calls the
old modifier parser, and the old chart/Lua parsers call the old normalization
helpers. Shared scalar parsing, modifier application, index allocation and
nonfinite fallback helpers are unchanged. Benchmarked mask construction uses
attack mode On; Off and Random are covered by behavior tests.

All input fixtures are prepared outside timing and counting. Mutable column
fixtures are freshly cloned for each operation and destroyed outside both.
The equal per-operation clock overhead affects tiny column-cycle measurements.
These synthetic cases isolate scaling and allocation behavior. They do not
measure end-to-end song-load time, frame latency or gameplay FPS.

## Modifier keys and full parsers

One key operation normalizes a name and destroys any owning result. Short and
medium keys are `drunk` and `confusionoffset20`; the 512-byte all-lowercase key
is a normalization stress case, not a supported gameplay modifier. Uppercase
and punctuation cases exercise the original fallback. Throughput counts keys
per second. Each full parser operation handles five modifier tokens, including
amounts, disabled modifiers, an approach prefix and a scroll-speed override;
throughput counts input tokens per second. Complete owning results are dropped
inside measurement. Normalized short/full-parser inputs already had zero heap
traffic; their benefit is less normalization work.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `key_short` | 17.8 -> 9.2 | 48.0% (45.9..51.9) | 93.7% | 0 -> 0 | 0 -> 0 |
| `key_medium` | 41.8 -> 20.5 | 51.3% (50.2..57.7) | 106.4% | 0 -> 0 | 0 -> 0 |
| `key_long` | 1176.6 -> 461.3 | 60.7% (54.0..62.0) | 154.0% | 1 -> 0 | 512 -> 0 |
| `key_uppercase` | 26.9 -> 25.2 | 6.0% (0.8..10.5) | 6.5% | 0 -> 0 | 0 -> 0 |
| `key_punctuation` | 32.8 -> 29.3 | 7.2% (4.6..14.2) | 8.1% | 0 -> 0 | 0 -> 0 |
| `parse_normalized` | 701.4 -> 656.4 | 5.5% (-8.3..6.6) | 6.0% | 0 -> 0 | 0 -> 0 |
| `parse_fallback` | 759.6 -> 820.6 | -2.8% (-14.9..7.6) | -2.6% | 0 -> 0 | 0 -> 0 |
| `parse_lua` | 567.0 -> 488.6 | 12.7% (10.6..17.2) | 14.7% | 0 -> 0 | 0 -> 0 |

## Chart mask construction

One operation parses all raw chunks, builds the owning output and drops it.
Throughput counts input chunks per second, including rejected entries.
Invalid cases use nonfinite start times, zero cases use zero duration, and
unsupported cases use unknown modifier names. Mixed inputs have one rejected
zero-length entry out of every three. Valid/mixed fixtures use uppercase names
to exercise the unchanged normalizer fallback. Valid output retains one
allocation with the parent's exact capacity. Empty outputs need none.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `masks_1_valid` | 1000.0 -> 1021.8 | -0.9% (-3.9..0.8) | -0.9% | 1 -> 1 | 2400 -> 2400 |
| `masks_1024_valid` | 1551971.1 -> 1570757.8 | -0.8% (-4.3..2.4) | -0.8% | 1 -> 1 | 2457600 -> 2457600 |
| `masks_1024_invalid` | 241395.4 -> 193764.8 | 19.7% (19.0..20.7) | 24.6% | 1 -> 0 | 2457600 -> 0 |
| `masks_1024_zero` | 522010.2 -> 458685.2 | 12.2% (4.6..14.2) | 13.9% | 1 -> 0 | 2457600 -> 0 |
| `masks_1024_unsupported` | 561316.4 -> 492650.8 | 12.4% (9.3..15.1) | 14.1% | 1 -> 0 | 2457600 -> 0 |
| `masks_1024_mixed` | 1229931.2 -> 1226290.6 | 0.4% (-3.2..11.5) | 0.4% | 1 -> 1 | 2457600 -> 2457600 |

## Column offset tail extension

One operation extends every window's sustain tail in place. Heap counts include
only index scratch; owning window vectors belong to setup. Throughput counts
windows per second. Ordered and tied starts use direct traversal; reversed and
mixed-column/target inputs retain the original sort scratch. The small control
fits the parent's stack index buffer, so both versions already avoid heap churn.

| Workload | Old -> new ns/op | CPU-cycle reduction (round range) | Throughput increase | Allocations/op | Requested bytes/op |
|---|---:|---:|---:|---:|---:|
| `columns_8_ordered` | 159.0 -> 105.5 | 7.5% (3.2..9.3) | 51.3% | 0 -> 0 | 0 -> 0 |
| `columns_1024_ordered` | 13042.2 -> 6912.1 | 45.0% (36.1..51.5) | 87.4% | 1 -> 0 | 8192 -> 0 |
| `columns_8192_ordered` | 87080.1 -> 61169.6 | 28.3% (22.3..33.2) | 40.1% | 1 -> 0 | 65536 -> 0 |
| `columns_1024_ties` | 10151.6 -> 6624.6 | 33.7% (19.2..37.3) | 55.3% | 1 -> 0 | 8192 -> 0 |
| `columns_1024_reverse` | 8969.5 -> 8888.3 | 1.1% (-22.7..12.2) | 1.3% | 1 -> 1 | 8192 -> 8192 |
| `columns_1024_mixed` | 57346.4 -> 55491.4 | 4.9% (-0.5..8.0) | 5.4% | 1 -> 1 | 8192 -> 8192 |

The gains depend on input shape. Some parser and valid-mask control medians
are slightly slower, with CPU-change round ranges crossing zero. No measured
workload increases heap traffic. The reversed-column control retains its
allocation budget and has a small positive paired CPU median.

## Behavior checks and reproduction

2242 debug/profile cases pass (99 ignored), including five new
regression tests. They compare parent/new keys across 128-byte buffer boundaries,
Unicode and 256 deterministic varied strings; both full parsers for ordering,
aliases, clearall and unusual float amounts; mask output for attack modes,
players, malformed chunks and tiny capacity; and bit-exact column fields across
sizes, groups, ties, repeated extension, epsilon boundaries, signed zero and
nonfinite starts. Allocation checks prove normalized long keys, rejected chart
output and ordered column tails have zero heap traffic, while valid masks and
unordered columns retain their parent budgets.

857 release unit cases pass (14 ignored). Clippy completes for
gameplay, notefield and song Lua with existing repository warnings and no
warnings in the new test/baseline files. 3 available native multitap
parity cases pass. The external `multitap::edgar_countdown_onsets_and_hit_commands`
fixture remains unavailable because `C:\GitHub\lua-songs` is absent; the native
command below excludes that case. Authored native trace fixtures are unchanged.

```powershell
cargo test -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --locked
cargo clippy -p deadsync-gameplay -p deadsync-notefield -p deadsync-song-lua --all-targets --locked
cargo test -p deadsync --test song_lua_itgmania_semantic_parity multitap --locked -- --skip edgar_countdown_onsets_and_hit_commands
cargo test --release -p deadsync-gameplay --lib --locked
cargo test --release -p deadsync-gameplay --lib attack_preparation_perf::benchmark_attack_preparation -- --exact --ignored --nocapture --test-threads=1
$env:DEADSYNC_PERF_REVERSE = '1'
# Repeat with reversed order; alternate for six rounds.
Remove-Item Env:DEADSYNC_PERF_REVERSE
```
