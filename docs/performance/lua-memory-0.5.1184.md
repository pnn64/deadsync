# Lua string transfer, compact deduplication, and actor restoration - 0.5.1184

Parent: `cbc8023529ef1a076816d3d7dc0d83047aaaab38` (`0.5.1183`).
This pass follows `rust-performance.md`: measure CPU and allocation costs
(M-HOTPATH), size known collections (M-INITIAL-CAPACITY), and avoid temporary
ownership and repeated allocation (M-MEM-REUSE).

## Three changes

1. **Transfer existing Lua strings during stringification.** Values that are
   already Lua strings, including results of numeric formatting callbacks, are
   validated as UTF-8 and stored directly in the output table. They no longer
   pass through an owned Rust String and a second Lua string construction. This
   is particularly useful for long strings, which Lua does not intern like
   short strings. Non-string conversions retain the existing helper. Invalid
   UTF-8, format lookup, callback arguments and side effects, nil termination,
   and conversion/error order are unchanged. The output table still owns its
   strings after input references are dropped and Lua GC runs.
2. **Use compact deduplication keys and separate iteration phases.** The old
   tagged key set is replaced by typed sets for exact integers, float bits,
   rounded integer-as-float bits, Lua strings, and table pointers; booleans use
   two bits. Prefix type counts size only the sets needed by that prefix.
   The first phase keeps the linear scan; after accepting the 33rd value in a
   table with raw length at least 128, a separate helper builds the index and
   handles the rest. This removes the per-item index-mode check and keeps the
   larger index stack frame out of the short-list path. The helper is deliberately
   not inlined and is called once per indexed operation. The threshold, accepted
   value ordering, non-transitive mixed numeric comparisons above 2^53, signed
   zero, NaNs, invalid strings, opaque values, and table identity are preserved.
   The standard randomized hashers remain; no persistent cache is introduced.
3. **Collect actor restoration keys in one raw traversal.** Actor mutable and
   semantic state restoration uses `Table::for_each`, retains matching Lua key
   handles in a SmallVec with 16 inline slots, and avoids cloning the actor.
   Larger key sets spill to the heap. Every string key is still validated before
   any write, including ignored keys. Deletions occur after traversal completes,
   then snapshot entries are applied in their original order. Lua metamethods,
   callback mutations, and partial-write errors keep their existing behavior.
   This also avoids cursor-key reference allocations for ignored non-string keys.

All changes use existing dependencies and safe Rust/mlua APIs. The public API
and dependency versions are unchanged. These targeted helper/capture benchmarks
do not establish a whole-song compilation or frame-time improvement.

## Behavior and validation

Seven named parent function bodies plus the original deduplication enum and
insertion helper are frozen in test-only modules. The actor restoration oracle
includes its mutable/semantic wrappers, the multi-actor wrapper, and the full
preserving-command caller. Its remaining helpers are shared and unchanged.
A source audit reconstructs both parent production files from the three changed
bodies, replaced index support, and new test declarations. Shared conversion and
comparison code is unchanged. Cargo.toml and Cargo.lock are audited for exactly
one patch increment with no dependency changes.

Twelve new tests cover long Unicode/NUL strings and GC lifetimes, formatter
mutation and exact error/callback order, invalid UTF-8, non-string conversions,
late index type transitions, all orderings of non-transitive numeric examples,
threshold boundaries and repetitive inputs, userdata/thread/table retention,
actor key filtering and SmallVec spill boundaries, ignored invalid string keys,
metamethod order and partial failures, and complete command capture/restoration.
Existing earlier differential tests also exercise the new implementations.

Two scoped allocation guards verify:

- Building an index for 512 distinct integers needs at most two allocations and
  20,000 requested bytes, with zero reallocations.
- Restoring an actor with 128 ignored table keys and an empty snapshot has zero
  allocation, reallocation, free, or byte churn after warmup.

Validation on this machine:

- Targeted tests: **12 passed**, 3 manual benchmarks ignored.
- Fresh parent debug suite: **541 passed, 5 failed, 42 ignored**.
- Updated debug and release suites: **553 passed, 5 failed, 45 ignored** each.
- Failing names and assertion text match the fresh parent debug run: actorproxy
  targets, shared init globals, local/hidden proxy targets, cmd queuecommand
  builders, and the notefield column API. No separate parent release build was
  run. The suite is not fully green.
- Root `cargo check --all-targets --offline` passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`
  passed with the same 53 crate warnings (45 duplicates), and no new warnings.
- Changed Rust files pass rustfmt; the staged diff passes whitespace checks.

## Measurement method

Measured on 2026-09-13 (Europe/Paris), Windows x86_64, Intel Xeon E5-2696 v4
at 2.20 GHz; rustc 1.98.0 (`88d9e12ae`, LLVM 22.1.8), release optimization
level 3 with full LTO. Each pair runs frozen parent and current implementations
in the same executable on equivalent inputs, with behavior compared first.

There are 41 paired scenarios across three serial runs. The middle run reverses
old/new order, and the runner waits for compiler/linker processes to finish before
each run. Each side has three warmups and seven timing samples. The tables show
medians of the three run medians. Windows QueryThreadCycleTime measures calling
thread CPU cycles; wall time and throughput are also reported. Samples repeat
64 operations for stringification, 32 for deduplication and complete captures,
and 128 for direct restoration. Units are input values for stringification and
deduplication, ignored + changed + saved fields for restoration, and one complete
command for captures; empty cases use one nominal unit.

The existing thread-local allocator harness separately counts allocation,
reallocation, and free calls plus requested/freed bytes for one operation. It
includes Rust and mlua-backed Lua allocator traffic. Input setup and warmup are
excluded. Restoration refills temporary fields and clones the owned snapshot
inside each measured operation on both sides; these common costs remain in the
results. Output Rust handles are dropped inside timing. Lua GC is collected
before each pair and then stopped, so deferred Lua GC and VM teardown are excluded.
Requested byte totals are not peak live memory or RSS. These paths still allocate
Lua output tables, snapshots, selected key references, and/or temporary indexes.

## Interpretation

- Stringifying 128 long Unicode strings uses
  **37.13% fewer cycles**, with allocation calls
  **386 -> 130** and requested bytes
  **85,196 -> 6,184**.
  Formatting 256 numbers uses **12.86% fewer cycles**
  and allocation calls **515 -> 259**.
  Existing Lua-string reference validation can still allocate reference counters;
  the optimization removes owned string copies and repeated long-string creation.
- Deduplicating 1,024 unique integers uses
  **32.38% fewer cycles** and requested bytes
  **301,416 -> 107,976**. The
  repetitive 1,024-item/32-distinct-integer case uses
  **2.11% more cycles**, with unchanged allocator
  traffic. The index threshold is unchanged; that case stays on the linear path.
- Restoring 64 changed fields with 256 ignored fields uses
  **21.87% fewer cycles**, reallocation calls
  **4 -> 1**, and requested bytes
  **13,334 -> 12,662**. A complete
  preserving command capture with 64 custom state fields uses
  **11.39% fewer cycles**, with requested bytes
  **46,702 -> 45,342**. The capture includes
  substantial shared work, so the direct restoration gain is not its overall gain.

Compact sets do not improve every allocation metric. For 1,024 unique integers,
allocation calls change **9 -> 15**;
for 256 mixed values, they change **78 -> 95**
while requested bytes change **41,400 -> 26,744**.
The separately sized sets can grow independently. Byte traffic and CPU costs
must be weighed against additional allocation/free calls; the tables
include every scenario and any measured timing regressions.

Small/empty and unformatted controls do little or none of the optimized work.
Compiler layout, frequency changes, hash seeds, allocator state, and timer
granularity can affect timing. Improvements on controls are not proof of a
general speedup, and no claim is made that every input distribution improves.
Only final corrected release runs appear below; the initial incomplete run and
debug benchmark smoke test are excluded from the performance aggregates.


## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| actor_restore_0_0_0_string | 0.0625 | 0.0578 | 145.8 | 135.5 | 7.06% |
| actor_restore_0_1_0_string | 0.5352 | 0.3383 | 1,183.2 | 751.1 | 36.52% |
| actor_restore_0_8_8_string | 4.2406 | 3.4664 | 9,318.5 | 7,619.0 | 18.24% |
| actor_restore_0_16_16_string | 9.3875 | 6.8602 | 20,585.0 | 14,999.7 | 27.13% |
| actor_restore_0_17_16_string | 9.5633 | 7.2281 | 20,972.5 | 15,836.6 | 24.49% |
| actor_restore_256_8_8_string | 62.7609 | 50.1430 | 137,638.5 | 109,892.3 | 20.16% |
| actor_restore_256_64_64_string | 96.1250 | 75.1141 | 210,696.0 | 164,609.6 | 21.87% |
| actor_restore_0_256_64_string | 102.1164 | 87.0523 | 223,653.3 | 190,923.8 | 14.63% |
| actor_restore_128_0_0_table | 21.8820 | 10.0945 | 48,010.5 | 22,130.1 | 53.91% |
| actor_capture_0 | 79.0406 | 74.1344 | 173,377.6 | 161,881.2 | 6.63% |
| actor_capture_64 | 224.1750 | 198.3906 | 490,911.8 | 434,994.1 | 11.39% |
| dedup_storage_integer_0_1 | 0.2000 | 0.1344 | 487.0 | 349.8 | 28.17% |
| dedup_storage_integer_8_8 | 1.8219 | 1.8188 | 4,040.2 | 4,033.3 | 0.17% |
| dedup_storage_integer_32_32 | 6.2094 | 6.5219 | 13,670.7 | 14,356.7 | -5.02% |
| dedup_storage_integer_127_127 | 41.3125 | 39.5125 | 90,591.8 | 86,627.1 | 4.38% |
| dedup_storage_integer_128_128 | 35.7000 | 27.6062 | 78,436.9 | 60,527.2 | 22.83% |
| dedup_storage_integer_256_256 | 77.3000 | 54.7312 | 169,577.5 | 120,039.1 | 29.21% |
| dedup_storage_integer_1024_1024 | 310.3719 | 210.0562 | 680,525.5 | 460,174.8 | 32.38% |
| dedup_storage_integer_1024_8 | 57.2969 | 58.3438 | 125,677.5 | 128,009.7 | -1.86% |
| dedup_storage_integer_1024_32 | 98.7562 | 100.8625 | 216,639.6 | 221,201.1 | -2.11% |
| dedup_storage_integer_1024_33 | 108.9781 | 66.4719 | 238,582.8 | 145,727.4 | 38.92% |
| dedup_storage_integer_1024_64 | 113.2469 | 71.3031 | 248,371.1 | 156,386.9 | 37.03% |
| dedup_storage_number_256_256 | 50.1750 | 39.8562 | 110,182.1 | 87,546.2 | 20.54% |
| dedup_storage_number_1024_1024 | 206.5844 | 157.3156 | 453,027.4 | 344,889.3 | 23.87% |
| dedup_storage_mixed_256_256 | 72.0781 | 67.5250 | 157,937.1 | 148,114.5 | 6.22% |
| dedup_storage_string_8_8 | 4.4531 | 4.2469 | 9,815.8 | 9,369.9 | 4.54% |
| dedup_storage_string_128_128 | 79.9500 | 82.5312 | 175,250.2 | 181,053.2 | -3.31% |
| dedup_storage_string_512_512 | 233.4688 | 220.3844 | 511,400.7 | 483,140.1 | 5.53% |
| dedup_storage_string_1024_8 | 463.3156 | 486.4375 | 1,015,880.3 | 1,066,234.9 | -4.96% |
| dedup_storage_table_128_128 | 34.9906 | 32.6719 | 76,667.2 | 71,611.8 | 6.59% |
| dedup_storage_table_512_512 | 114.7000 | 99.5125 | 251,499.0 | 218,162.4 | 13.26% |
| string_transfer_empty | 0.4906 | 0.5625 | 1,100.9 | 1,258.7 | -14.33% |
| string_transfer_one | 1.3688 | 1.2250 | 3,031.8 | 2,709.5 | 10.63% |
| string_transfer_numeric_32 | 27.7938 | 26.3656 | 60,969.6 | 57,896.5 | 5.04% |
| string_transfer_numeric_256 | 216.6750 | 188.8250 | 474,936.2 | 413,877.5 | 12.86% |
| string_transfer_numeric_1024 | 871.0938 | 775.6812 | 1,909,087.5 | 1,699,886.9 | 10.96% |
| string_transfer_unformatted_256 | 76.6250 | 77.7594 | 167,866.0 | 170,369.7 | -1.49% |
| string_transfer_strings_256 | 82.6469 | 59.5906 | 181,224.7 | 130,434.4 | 28.03% |
| string_transfer_long_strings_128 | 90.7406 | 57.0375 | 198,966.5 | 125,091.0 | 37.13% |
| string_transfer_long_strings_1024 | 751.8047 | 453.1375 | 1,647,323.5 | 993,138.0 | 39.71% |
| string_transfer_wide_numeric_256 | 337.3250 | 277.5719 | 738,857.5 | 608,039.0 | 17.71% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| actor_restore_0_0_0_string | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 16,000,000.0 | 17,297,297.3 |
| actor_restore_0_1_0_string | 2/0/2 | 1/0/1 | 112/112 | 16/16 | 1,868,613.1 | 2,956,120.1 |
| actor_restore_0_8_8_string | 18/1/18 | 17/0/17 | 1,064/1,064 | 776/776 | 3,773,028.7 | 4,615,731.4 |
| actor_restore_0_16_16_string | 34/2/34 | 33/0/33 | 2,230/2,230 | 1,558/1,558 | 3,408,788.3 | 4,664,616.8 |
| actor_restore_0_17_16_string | 35/3/35 | 35/0/35 | 3,014/3,014 | 2,342/2,342 | 3,450,698.5 | 4,565,499.4 |
| actor_restore_256_8_8_string | 274/1/274 | 273/0/273 | 5,160/5,160 | 4,872/4,872 | 4,333,905.9 | 5,424,489.4 |
| actor_restore_256_64_64_string | 386/4/386 | 386/1/386 | 13,334/13,334 | 12,662/12,662 | 3,994,798.4 | 5,112,225.2 |
| actor_restore_0_256_64_string | 322/6/322 | 322/3/322 | 21,526/21,526 | 20,854/20,854 | 3,133,678.6 | 3,675,949.3 |
| actor_restore_128_0_0_table | 128/0/128 | 0/0/0 | 2,048/2,048 | 0/0 | 5,849,548.4 | 12,680,133.1 |
| actor_capture_0 | 232/1/214 | 229/1/211 | 7,826/6,828 | 7,618/6,620 | 12,651.7 | 13,489.0 |
| actor_capture_64 | 747/21/600 | 746/15/599 | 46,702/39,048 | 45,342/37,688 | 4,460.8 | 5,040.6 |
| dedup_storage_integer_0_1 | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 5,000,000.0 | 7,441,860.5 |
| dedup_storage_integer_8_8 | 3/4/1 | 3/4/1 | 776/592 | 776/592 | 4,391,080.6 | 4,398,625.4 |
| dedup_storage_integer_32_32 | 3/8/1 | 3/8/1 | 3,464/2,896 | 3,464/2,896 | 5,153,497.7 | 4,906,564.4 |
| dedup_storage_integer_127_127 | 3/12/1 | 3/12/1 | 14,216/12,112 | 14,216/12,112 | 3,074,130.1 | 3,214,172.7 |
| dedup_storage_integer_128_128 | 6/10/4 | 9/10/7 | 36,152/34,048 | 14,696/12,592 | 3,585,434.2 | 4,636,631.2 |
| dedup_storage_integer_256_256 | 7/11/5 | 11/11/9 | 74,056/69,904 | 28,040/23,888 | 3,311,772.3 | 4,677,400.9 |
| dedup_storage_integer_1024_1024 | 9/13/7 | 15/13/13 | 301,416/284,976 | 107,976/91,536 | 3,299,268.0 | 4,874,884.7 |
| dedup_storage_integer_1024_8 | 3/4/1 | 3/4/1 | 776/592 | 776/592 | 17,871,829.8 | 17,551,151.6 |
| dedup_storage_integer_1024_32 | 3/8/1 | 3/8/1 | 3,464/2,896 | 3,464/2,896 | 10,368,964.0 | 10,152,435.2 |
| dedup_storage_integer_1024_33 | 4/9/2 | 5/9/3 | 8,728/7,648 | 5,672/4,592 | 9,396,381.2 | 15,405,011.5 |
| dedup_storage_integer_1024_64 | 5/9/3 | 7/9/5 | 17,192/16,112 | 8,008/6,928 | 9,042,192.1 | 14,361,221.9 |
| dedup_storage_number_256_256 | 6/11/4 | 7/11/5 | 40,248/36,096 | 19,336/15,184 | 5,102,142.5 | 6,423,083.0 |
| dedup_storage_number_1024_1024 | 8/13/6 | 9/13/7 | 166,232/149,792 | 71,592/55,152 | 4,956,812.4 | 6,509,207.2 |
| dedup_storage_mixed_256_256 | 78/11/76 | 95/11/93 | 41,400/37,248 | 26,744/22,592 | 3,551,701.7 | 3,791,188.4 |
| dedup_storage_string_8_8 | 11/4/9 | 11/4/9 | 904/720 | 904/720 | 1,796,491.2 | 1,883,738.0 |
| dedup_storage_string_128_128 | 133/10/131 | 134/10/132 | 21,288/19,184 | 19,832/17,728 | 1,601,000.6 | 1,550,927.7 |
| dedup_storage_string_512_512 | 519/12/517 | 520/12/518 | 90,440/82,192 | 76,696/68,448 | 2,193,013.0 | 2,323,213.7 |
| dedup_storage_string_1024_8 | 1027/4/1025 | 1027/4/1025 | 17,160/16,976 | 17,160/16,976 | 2,210,156.4 | 2,105,100.9 |
| dedup_storage_table_128_128 | 37/10/35 | 38/10/36 | 19,752/17,648 | 11,128/9,024 | 3,658,122.7 | 3,917,742.7 |
| dedup_storage_table_512_512 | 39/12/37 | 40/12/38 | 82,760/74,512 | 37,272/29,024 | 4,463,818.7 | 5,145,082.3 |
| string_transfer_empty | 2/0/1 | 2/0/1 | 64/8 | 64/8 | 2,038,216.6 | 1,777,777.8 |
| string_transfer_one | 5/0/3 | 4/0/2 | 104/32 | 96/24 | 730,593.6 | 816,326.5 |
| string_transfer_numeric_32 | 67/5/65 | 35/5/33 | 1,840/1,272 | 1,584/1,016 | 1,151,338.0 | 1,213,701.6 |
| string_transfer_numeric_256 | 515/8/513 | 259/8/257 | 14,384/10,232 | 12,336/8,184 | 1,181,493.0 | 1,355,752.7 |
| string_transfer_numeric_1024 | 2051/10/2049 | 1027/10/1025 | 57,392/40,952 | 49,200/32,760 | 1,175,533.6 | 1,320,129.9 |
| string_transfer_unformatted_256 | 258/8/256 | 258/8/256 | 10,280/6,128 | 10,280/6,128 | 3,340,946.2 | 3,292,207.5 |
| string_transfer_strings_256 | 515/8/513 | 259/8/257 | 14,384/10,232 | 12,336/8,184 | 3,097,515.8 | 4,295,977.8 |
| string_transfer_long_strings_128 | 386/7/256 | 130/7/128 | 85,196/41,986 | 6,184/4,080 | 1,410,614.0 | 2,244,137.6 |
| string_transfer_long_strings_1024 | 3074/10/2048 | 1026/10/1024 | 682,876/336,794 | 49,192/32,752 | 1,362,055.9 | 2,259,799.7 |
| string_transfer_wide_numeric_256 | 1027/8/513 | 515/8/257 | 98,864/32,760 | 43,312/8,184 | 758,912.0 | 922,283.6 |

Per-run paired cycle savings for representative cases:

- `string_transfer_long_strings_128`: 39.85%, 37.68%, 33.17%.
- `string_transfer_numeric_256`: 13.50%, 11.24%, -0.11%.
- `dedup_storage_integer_1024_1024`: 32.38%, 30.29%, 48.61%.
- `dedup_storage_integer_1024_32`: -2.11%, -6.61%, -1.70%.
- `actor_restore_256_64_64_string`: 17.52%, 30.00%, 21.98%.
- `actor_capture_64`: -4.57%, 11.39%, 12.08%.

Cases with higher median cycle counts:

- `dedup_storage_integer_32_32`: 5.02% more cycles; elapsed 6209.4 -> 6521.9 ns/op.
- `dedup_storage_integer_1024_8`: 1.86% more cycles; elapsed 57296.9 -> 58343.8 ns/op.
- `dedup_storage_integer_1024_32`: 2.11% more cycles; elapsed 98756.2 -> 100862.5 ns/op.
- `dedup_storage_string_128_128`: 3.31% more cycles; elapsed 79950.0 -> 82531.2 ns/op.
- `dedup_storage_string_1024_8`: 4.96% more cycles; elapsed 463315.6 -> 486437.5 ns/op.
- `string_transfer_empty`: 14.33% more cycles; elapsed 490.6 -> 562.5 ns/op.
- `string_transfer_unformatted_256`: 1.49% more cycles; elapsed 76625.0 -> 77759.4 ns/op.


## Repeatability checks

Six additional runs of all 41 pairs used the same final executable, alternating
old/new order. They investigate the slower controls in the primary three runs;
they do not replace those results. All allocator counts and byte totals match
the primary runs. Positive percentages below mean fewer cycles.

| Scenario | Cycle savings in runs 1 / 2 / 3 / 4 / 5 / 6 | Median paired savings |
|---|---:|---:|
| actor_restore_0_0_0_string | 11.93% / 10.90% / 5.51% / 10.32% / 12.11% / 7.86% | 10.61% |
| actor_restore_0_1_0_string | 39.31% / 34.34% / 38.47% / 33.18% / 37.41% / 33.88% | 35.87% |
| actor_restore_0_8_8_string | 19.71% / 11.25% / 21.74% / 15.04% / 15.18% / 12.67% | 15.11% |
| actor_restore_0_16_16_string | 15.04% / 21.27% / 16.46% / 33.68% / 13.58% / 14.35% | 15.75% |
| actor_restore_0_17_16_string | 15.28% / 5.91% / 16.45% / 10.89% / 11.60% / 13.10% | 12.35% |
| actor_restore_256_8_8_string | 32.08% / 25.88% / 22.44% / 20.32% / 22.05% / 19.55% | 22.25% |
| actor_restore_256_64_64_string | 21.69% / 19.30% / 19.59% / 16.19% / 20.07% / 17.97% | 19.44% |
| actor_restore_0_256_64_string | -7.53% / 16.62% / 17.81% / 13.05% / 13.95% / 18.22% | 15.28% |
| actor_restore_128_0_0_table | 58.19% / 53.32% / 51.13% / 51.68% / 55.26% / 54.07% | 53.69% |
| actor_capture_0 | 4.45% / -3.06% / 10.83% / 7.44% / 4.76% / 7.13% | 5.95% |
| actor_capture_64 | 6.21% / 5.25% / -0.85% / 7.07% / 8.52% / -1.74% | 5.73% |
| dedup_storage_integer_0_1 | 23.52% / -36.17% / -16.09% / -32.61% / 36.49% / -47.83% | -24.35% |
| dedup_storage_integer_8_8 | -6.39% / -6.12% / -6.92% / -6.75% / 2.46% / -7.64% | -6.57% |
| dedup_storage_integer_32_32 | 9.56% / 1.45% / 12.72% / 2.09% / -1.83% / 1.66% | 1.87% |
| dedup_storage_integer_127_127 | -1.74% / 1.36% / 11.81% / -0.50% / -13.58% / -9.92% | -1.12% |
| dedup_storage_integer_128_128 | 24.64% / 27.93% / 6.76% / 23.83% / 29.93% / 25.41% | 25.02% |
| dedup_storage_integer_256_256 | 23.36% / 26.44% / 43.03% / 14.95% / 25.93% / 23.36% | 24.65% |
| dedup_storage_integer_1024_1024 | 30.56% / 29.15% / 46.13% / 28.08% / 48.10% / 31.63% | 31.09% |
| dedup_storage_integer_1024_8 | 0.83% / -1.28% / -12.67% / -2.13% / 16.05% / 7.68% | -0.22% |
| dedup_storage_integer_1024_32 | -1.93% / 1.00% / 6.57% / 0.28% / 0.48% / 0.30% | 0.39% |
| dedup_storage_integer_1024_33 | 35.31% / 35.98% / 35.22% / 35.17% / 34.68% / 47.74% | 35.26% |
| dedup_storage_integer_1024_64 | 34.30% / 36.58% / 35.72% / 32.63% / 34.76% / 27.59% | 34.53% |
| dedup_storage_number_256_256 | 24.23% / 22.22% / 19.56% / 21.60% / 18.93% / 22.67% | 21.91% |
| dedup_storage_number_1024_1024 | 40.14% / 27.18% / 28.01% / 17.90% / 25.86% / 25.99% | 26.58% |
| dedup_storage_mixed_256_256 | 6.20% / -3.55% / 3.18% / -0.41% / 3.86% / 9.65% | 3.52% |
| dedup_storage_string_8_8 | 2.15% / 5.88% / 2.24% / -4.23% / 16.90% / 6.76% | 4.06% |
| dedup_storage_string_128_128 | 4.55% / -0.55% / 4.79% / -4.34% / 10.40% / 2.46% | 3.51% |
| dedup_storage_string_512_512 | -0.12% / -2.44% / 5.49% / -2.30% / -1.87% / 5.07% | -1.00% |
| dedup_storage_string_1024_8 | 6.04% / 3.28% / 4.84% / -6.55% / 6.57% / 1.73% | 4.06% |
| dedup_storage_table_128_128 | -1.00% / 6.37% / -0.54% / 19.09% / 3.54% / 1.83% | 2.68% |
| dedup_storage_table_512_512 | 17.02% / 11.15% / 11.26% / 9.99% / 11.60% / 16.71% | 11.43% |
| string_transfer_empty | 17.43% / -1.17% / -0.29% / -5.67% / -12.83% / -3.15% | -2.16% |
| string_transfer_one | -7.37% / -16.37% / 7.51% / -17.08% / 2.92% / -16.44% | -11.87% |
| string_transfer_numeric_32 | 8.73% / 9.46% / 1.35% / 8.29% / 7.13% / 15.62% | 8.51% |
| string_transfer_numeric_256 | 11.15% / 8.70% / 11.20% / 6.30% / 10.72% / 12.57% | 10.94% |
| string_transfer_numeric_1024 | 8.20% / 10.05% / 10.13% / 10.14% / 7.92% / 9.67% | 9.86% |
| string_transfer_unformatted_256 | -4.14% / 2.68% / -0.30% / -0.28% / 9.22% / -5.60% | -0.29% |
| string_transfer_strings_256 | 30.31% / 29.09% / 28.12% / 29.82% / 33.79% / 31.74% | 30.06% |
| string_transfer_long_strings_128 | 37.17% / 38.08% / 41.00% / 37.79% / 37.93% / 37.65% | 37.86% |
| string_transfer_long_strings_1024 | 38.92% / 36.90% / 39.77% / 40.71% / 38.48% / 39.73% | 39.32% |
| string_transfer_wide_numeric_256 | 14.19% / 17.88% / 14.91% / 18.98% / 7.70% / 9.13% | 14.55% |

For the controls that were slower in the primary aggregate:

- `dedup_storage_integer_32_32`: median paired cycle savings 1.87%; 1/6 runs slower; elapsed medians 6539.1 -> 6434.4 ns/op.
- `dedup_storage_integer_1024_8`: median paired cycle savings -0.22%; 3/6 runs slower; elapsed medians 62398.4 -> 60492.1 ns/op.
- `dedup_storage_integer_1024_32`: median paired cycle savings 0.39%; 1/6 runs slower; elapsed medians 101842.2 -> 100765.6 ns/op.
- `dedup_storage_string_128_128`: median paired cycle savings 3.51%; 2/6 runs slower; elapsed medians 83939.1 -> 81357.8 ns/op.
- `dedup_storage_string_1024_8`: median paired cycle savings 4.06%; 1/6 runs slower; elapsed medians 480504.7 -> 464001.6 ns/op.
- `string_transfer_empty`: median paired cycle savings -2.16%; 5/6 runs slower; elapsed medians 519.5 -> 524.2 ns/op.
- `string_transfer_unformatted_256`: median paired cycle savings -0.29%; 4/6 runs slower; elapsed medians 71427.4 -> 73252.4 ns/op.

These checks quantify variation and fixed costs; they do not establish a speedup
for unchanged paths or for unmeasured workloads.

Repeatable control costs in this build include `dedup_storage_integer_8_8` (6.57% more cycles, 5/6 runs slower), `string_transfer_empty` (2.16% more cycles, 5/6 runs slower). These measured regressions are retained explicitly; no gain is claimed for these controls.

Use the per-run results to distinguish stable gains and costs from variation; all primary results remain in the report.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib lua_memory_ --locked
cargo test -p deadsync-song-lua --release --lib lua_memory_bench --locked -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib lua_memory_bench --locked -- --ignored --test-threads=1 --nocapture
Remove-Item Env:\DEADSYNC_PERF_REVERSE
```

Local logs, source/version audits, parsed results, and orchestration scripts are
in ignored `target/lua-memory-perf/`. This report and the six test/baseline files
are committed. `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and
`optimize.ps1` are excluded from the commit.

Benchmark executable SHA-256: `c34a6ad31a77cdd6cb69ad961885d910a400d17c3cb380d2f225729355d28f30`.
