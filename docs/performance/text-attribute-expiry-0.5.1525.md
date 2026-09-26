# Recompute text-attribute precedence once per expiry group

Baseline: `303dab2c6`.

`deadlib-present::TextAttrCursor` removes ended ranges before choosing the color
for a visible glyph. Previously removing the highest-priority active range
immediately rescanned the remaining active indices. If several winning ranges
expired between glyphs, the cursor repeatedly computed intermediate winners
that were never used.

The cursor now removes all ranges due at the current character index, checks
whether the previous winner expired, and rescans the survivors at most once.
It then activates new ranges. This preserves the original slice-order priority:
a surviving higher-index range still overrides a newly starting lower-index one.
The expiry work is guarded by the existing next-end boundary check, so glyphs
without expiry do not carry a separate invalidation flag. No new cache, buffer,
or traversal pass is introduced.

## Behavior checks

The integration test includes production composition code and a frozen copy of
the baseline cursor. Both are compared with a reverse scan of the caller's
attributes, checking every returned color component by its exact float bits.

- 336 traces / 59,556 character queries, with 0, 1, 8, 9, 32, 128, and 257 ranges.
- Ordered, overlapping, grouped equal boundaries, nested ranges, identical ranges,
  and ranges that cover the entire text.
- Rotated/reversed slice order, skipped character indices, zero-length ranges,
  saturating ends near `usize::MAX`, signed zero, and corner colors.
- Reused scratch storage across sizes and arrangements.
- A focused case checks surviving precedence after the old winner expires,
  newly starting lower-priority ranges, never-activated skipped ranges, and
  repeated queries at the same character index.

The integration suite passed before the edit (136 active tests) and after it
(136 active tests, including the expanded no-expiry trace coverage). Existing
composition tests also check attributed glyph vertices and texture batches.
The frozen baseline is the original cursor with only test visibility changed.
The final code also passed 171 library tests plus the 136-test integration suite
in debug, formatting, and performance Clippy checks. Style warnings remain.
Many integration tests repeat source unit tests, so the counts are not distinct
coverage totals. The manual benchmarks are ignored by default.

## Before/after benchmark

Intel Xeon E5-2696 v4, Windows x86-64, Rust 1.98.1 / LLVM 22.1.8, repository
release profile with LTO. Three paired runs pinned to logical CPU 6, reversing
variant order in run 2. No builds or other tests from this pass ran during
measurement. Both versions run through equivalent black-boxed walk function
pointers in the same executable.

One operation constructs a cursor, including both event sorts, and queries
character indices below 256. The no-expiry, ordered-8, overlap-8, and
nested-32-single cases query every index; other cases query every fourth index.
The grouped nested cases have start zero and descending ends, so four successive
winners expire between queries. Input construction is excluded and scratch
capacity is warmed. Seven batches of 8,192 operations follow three warmups.
Windows `QueryThreadCycleTime` supplies thread cycles; allocation accounting
runs separately.

| Case | Before ns/op | After ns/op | Median paired cycle reduction | Reduction range | Median throughput increase |
| --- | ---: | ---: | ---: | ---: | ---: |
| no_expiry_1 | 1575.7 | 1503.8 | 7.1% | 2.6 to 17.2% | 7.7% |
| ordered_8 | 1272.2 | 1158.1 | 7.6% | 1.0 to 9.6% | 8.3% |
| overlap_8 | 1528.4 | 1473.1 | 4.1% | 3.6 to 4.3% | 4.2% |
| overlap_32 | 1763.2 | 1779.9 | 1.2% | -1.0 to 2.0% | 1.1% |
| tied_groups_32 | 1739.8 | 1715.1 | 1.4% | 1.4 to 6.8% | 1.4% |
| nested_32 | 1654.4 | 1174.4 | 30.1% | 27.7 to 30.4% | 43.3% |
| nested_32_single | 2476.6 | 2515.8 | 0.0% | -2.7 to 2.8% | 0.1% |
| identical_128 | 5092.4 | 5058.1 | -1.2% | -2.1 to 2.9% | -1.2% |
| overlap_128 | 7371.4 | 7316.5 | 0.7% | -1.9 to 0.7% | 0.7% |
| nested_128 | 15390.2 | 8865.1 | 42.4% | 32.2 to 42.8% | 74.0% |

Times are medians across runs. Percentages are medians of paired ratios, so they
need not equal ratios of displayed time medians. Negative percentages indicate
a slower candidate result. All samples, including absolute cycle counts and
cursor operations per second, are in the [raw CSV](text-attribute-expiry-0.5.1525.csv).

The clear gains are the grouped nested cases: median cycle reductions of 30.1%
and 42.4%, saving about 0.48 and 6.53 microseconds per complete cursor operation.
The single-expiry, identical-range and ordinary large-overlap controls show
small mixed differences, with no demonstrated improvement in those cases.
Smaller ordinary cases also improved in these runs, but the no-expiry result
has no algorithmic work reduction and should be treated as compiler/layout
and measurement effects, not an additional claimed optimization.

Every warmed operation used zero allocations, reallocations, frees, allocated
bytes, and freed bytes before and after. These are synthetic CPU cursor timings.
No full-frame/FPS improvement or live GPU pixel comparison was measured. Text
without attributes does not construct this cursor.

## Reproduce

```powershell
cargo test --locked --release -p deadlib-present --test attribute_order
cargo test --locked -p deadlib-present --lib --test attribute_order
cargo clippy --locked -p deadlib-present --lib --test attribute_order -- -D clippy::perf
(Get-Process -Id $PID).ProcessorAffinity = 64
# Run the printed release test executable directly:
# <executable> benchmark_attribute_expiry --ignored --nocapture --test-threads=1
# Repeat three times; set DEADSYNC_PERF_REVERSE=1 for the middle run.
```

Capture stderr directly. Local logs are retained under
`target/attribute-expiry-pass/run1.txt` through `run3.txt`.
