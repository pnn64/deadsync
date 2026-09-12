# Lua table splitting, formatting, and deduplication - 0.5.1183

Parent: `cc26826a28f9e9da991cd78a4b8cc2b78a2a90a9` (`0.5.1182`).
This pass follows `rust-performance.md`: measure CPU work and allocator traffic
(M-HOTPATH), reserve known output sizes (M-INITIAL-CAPACITY), and reuse storage
instead of constructing temporary strings and argument vectors (M-MEM-REUSE).

## Three changes

1. **Character splitting.** For an empty separator, `create_split_table` counts
   Unicode scalar values, reserves the Lua array once, and encodes each character
   into a reusable four-byte stack buffer. This removes the Rust String per
   character and incremental Lua array growth. Empty text still produces one
   empty string. Combining characters, embedded NULs, and multi-byte scalars
   retain their original meaning. Nonempty separators use the existing path.
2. **Numeric stringification.** `stringify_lua_table` passes `(form, value)`
   directly into the Lua formatting function, removing a temporary MultiValue
   allocation for each formatted number. It also borrows the input table.
   Format lookup and conversion still happen at the same points; callbacks can
   mutate the input, return other value types, or error as before. Output strings,
   output table growth, and the owned format string can still allocate.
3. **Deduplication.** `deduplicate_lua_table` switches from repeated linear
   comparisons to a temporary hash index after accepting 33 values, provided
   the input's raw length is at least 128. Short lists and lists with fewer than
   33 distinct accepted values retain the linear path. The index uses the
   existing 32-value prefix without growing the temporary vector for value 33.
   Its keys use Rust's standard randomized hasher for script-controlled keys.
   Exact integer keys,
   float keys, and rounded integer-as-float keys are separate: mixed numeric
   equality is not transitive above 2^53. Only accepted values enter the index,
   retaining first-occurrence order and order-dependent comparison behavior.
   NaNs, invalid UTF-8 strings, and opaque values continue to compare unequal;
   table equality remains pointer identity. Lua string bytes are immutable even
   though mlua reference counts are mutable, which explains the two narrowly
   scoped Clippy expectations. The index trades temporary storage for less CPU
   work on larger lists; it is not an allocation-free implementation.

No dependencies, public API, global caches, or production unsafe code changed.
These are targeted host-helper measurements; they do not establish a whole-song
compilation, frame-time, or process-memory improvement.

## Behavior and validation

Three parent function bodies are frozen in test-only modules. A source audit
compares their bodies with the parent, reconstructs the original production
file by replacing the three changed bodies and removing the index helper and
test declarations,
and verifies that shared conversion/comparison helpers are unchanged. It also
checks the single patch increment in Cargo.toml and the three workspace-version
entries in Cargo.lock, with no dependency changes.

Sixteen new tests cover:

- Empty and generated Unicode input, combining marks, NULs, maximum scalars,
  overlapping and absent separators, independent output tables, and allocation
  budgets. Splitting 64 already-interned characters performs only the two Lua
  output-table allocations, with no reallocations or per-character Rust strings.
- Mixed formatting values, invalid formats, absent and invalid inputs, numeric
  format output, exact callback argument count, callback order, input mutation,
  replacement of the global formatter, custom return conversions, and matching
  callback/conversion/lookup errors.
- Deduplication threshold boundaries, output order, unchanged inputs, nil holes,
  ignored hash fields and metamethods, table identity, opaque values, invalid
  UTF-8, long strings, signed zero, infinities, NaN payloads, all permutations of
  non-transitive integer/float examples (including i64 limits), and generated
  mixed sequences through 2,048 values.

Validation on this machine:

- Targeted tests: **16 passed**, 3 manual benchmarks ignored.
- Fresh parent debug suite: **525 passed, 5 failed, 39 ignored**.
- Updated debug and release suites: **541 passed, 5 failed, 42 ignored** each.
- The five failing test names and assertion text match the fresh parent debug
  run: actorproxy targets, shared init globals, local/hidden proxy targets,
  cmd queuecommand builders, and the notefield column API. A separate parent
  release build was not run. The suite is not fully green.
- `cargo check --all-targets --offline` passed for the root package.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`
  passed. The crate retains its 53 existing warnings (45 duplicates);
  this pass adds no warnings.
- Rustfmt checked the changed Rust files; the staged diff passed whitespace
  checks. Unrelated formatting changes were restored.

## Measurement method

Measured on 2026-09-13 (Europe/Paris), Windows x86_64, Intel Xeon E5-2696 v4
at 2.20 GHz; rustc 1.98.0 (`88d9e12ae`, LLVM 22.1.8), release optimization
level 3 with full LTO. Each pair calls the frozen parent and current function
in the same executable, on equivalent inputs, after checking equal outputs.

There are 30 paired scenarios, run three times serially. The runner waited for
compiler and linker processes to finish before each run. The middle run
reverses old/new order. Each side has three
warmups and seven timing samples; the tables show medians of the three run
medians. Windows QueryThreadCycleTime measures calling-thread CPU cycles;
elapsed time and item throughput are measured separately. Each operation uses
128 repetitions per sample for splitting, 64 for stringification, and 32 for
deduplication. A unit is an output split field, an input stringify value, or an
input deduplication value; empty cases use one nominal unit.

The existing thread-local allocator harness counts allocations, reallocations,
frees, and requested/freed bytes in a separate operation. It includes Rust and
mlua-backed Lua allocator traffic. Output Rust handle destruction is inside the
measured operation. Lua GC is collected before each pair and then stopped, so
deferred Lua collection and VM teardown are excluded; no claim about peak live
memory or RSS follows from requested bytes. Input construction, warm interned
strings, Lua stack growth during warmup, and fixture setup are excluded. Lua
output tables still allocate; these are not end-to-end zero-allocation claims.

## Interpretation

- Splitting 256 ASCII characters uses **58.10% fewer
  cycles**, with allocation calls **258 -> 2**,
  reallocation calls **8 -> 0**, and requested
  bytes **8,488 -> 4,152**. The 1,024-character
  mixed Unicode case uses **56.64% fewer cycles**.
- Stringifying 256 formatted numbers uses
  **15.93% fewer cycles**, allocation calls
  **771 -> 515**, and requested bytes
  **55,344 -> 14,384**. Exactly one temporary
  argument-vector allocation (160 bytes on this build) disappears per formatted
  number. Output and format-string ownership still account for remaining churn.
- Deduplicating 512 unique strings uses
  **97.62% fewer cycles**; 1,024 unique
  integers use **82.74% fewer cycles**;
  512 unique tables use **92.71% fewer
  cycles**. Allocation calls for the table workload fall
  **515 -> 39** for that complete operation.

The index is a CPU/storage tradeoff. For 1,024 unique integers, allocation calls
are **3 -> 9** and requested bytes
**114,568 -> 301,416**. For 512 strings,
requested bytes are **65,416 -> 90,440**.
The output size is unchanged, but the temporary index uses more storage than a
linear vector. Large table lists save reference-handle churn while also paying
for the index. The complete allocator table makes these increases explicit.

An initial trial activated the index after 32 accepted values. A 1,024-item list
with exactly 32 distinct integers then requested 7,704 bytes instead of 3,464
and measured 2.71% more cycles. The final cutoff requires 33 accepted values;
the same case now requests **3,464 -> 3,464**
bytes and uses **2.25% more cycles** in
the final aggregate. Superseded logs are kept as `initial-*` and `cutoff-*` and are
excluded from every final result table below.

All 30 scenarios, including higher cycle counts and unchanged-path controls,
are reported below. Small timing deltas are not evidence of a general speedup:
the empty and nonempty-separator cases do no optimized per-item work, and the
unformatted/string-only stringify cases do not build numeric argument vectors.
Compiler layout, frequency changes, allocator state, and timing noise can affect
these controls. Gains on unchanged control paths are not attributed to the
optimization. No improvement is claimed for every possible input distribution.


## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| deduplicate_integer_0_1 | 0.1656 | 0.1656 | 425.3 | 425.3 | 0.00% |
| deduplicate_integer_8_8 | 2.4875 | 2.4188 | 5,508.1 | 5,364.0 | 2.62% |
| deduplicate_integer_32_32 | 8.1188 | 7.9688 | 17,683.5 | 17,614.9 | 0.39% |
| deduplicate_integer_127_127 | 51.1594 | 51.0938 | 109,509.9 | 112,171.4 | -2.43% |
| deduplicate_integer_128_128 | 50.5625 | 49.3094 | 111,053.3 | 108,035.2 | 2.72% |
| deduplicate_integer_256_256 | 164.8000 | 99.1312 | 361,297.0 | 217,277.6 | 39.86% |
| deduplicate_integer_1024_1024 | 2,258.4344 | 389.4406 | 4,947,001.8 | 853,724.7 | 82.74% |
| deduplicate_integer_1024_8 | 74.3719 | 76.7750 | 163,170.8 | 168,438.8 | -3.23% |
| deduplicate_integer_1024_32 | 126.3562 | 129.2156 | 277,125.6 | 283,353.9 | -2.25% |
| deduplicate_string_8_8 | 5.4906 | 5.5406 | 12,127.4 | 12,209.7 | -0.68% |
| deduplicate_string_128_128 | 792.6125 | 103.1188 | 1,736,965.3 | 226,023.2 | 86.99% |
| deduplicate_string_512_512 | 12,493.5438 | 296.5188 | 27,357,094.5 | 650,131.6 | 97.62% |
| deduplicate_string_1024_8 | 591.0125 | 588.0844 | 1,295,351.8 | 1,288,856.0 | 0.50% |
| deduplicate_table_128_128 | 150.9406 | 43.7344 | 330,848.2 | 95,297.2 | 71.20% |
| deduplicate_table_512_512 | 2,007.3281 | 146.0062 | 4,394,808.4 | 320,168.2 | 92.71% |
| split_empty | 0.4016 | 0.3602 | 893.4 | 804.3 | 9.97% |
| split_one | 0.3914 | 0.3656 | 874.6 | 814.5 | 6.87% |
| split_ascii_32 | 5.9797 | 2.3031 | 13,139.1 | 5,039.9 | 61.64% |
| split_ascii_256 | 38.7836 | 16.2383 | 85,006.5 | 35,613.9 | 58.10% |
| split_unicode_256 | 40.5297 | 18.3641 | 88,842.6 | 40,278.2 | 54.66% |
| split_unicode_1024 | 165.0789 | 71.6023 | 361,717.1 | 156,844.8 | 56.64% |
| split_separator_256 | 24.5070 | 24.9266 | 53,599.2 | 54,634.9 | -1.93% |
| split_absent_1024 | 1.9273 | 1.5867 | 4,251.1 | 3,501.7 | 17.63% |
| stringify_empty | 0.6656 | 0.6469 | 1,485.0 | 1,443.9 | 2.77% |
| stringify_one | 1.7531 | 1.5297 | 3,872.1 | 3,388.5 | 12.49% |
| stringify_numeric_32 | 38.6422 | 32.9422 | 84,744.1 | 72,256.7 | 14.74% |
| stringify_numeric_256 | 308.1875 | 259.1578 | 675,260.9 | 567,664.7 | 15.93% |
| stringify_numeric_1024 | 1,243.2547 | 1,060.3172 | 2,724,145.9 | 2,323,318.4 | 14.71% |
| stringify_unformatted_256 | 91.8516 | 93.3516 | 201,377.5 | 204,694.0 | -1.65% |
| stringify_strings_256 | 106.0156 | 105.7766 | 232,484.8 | 231,990.9 | 0.21% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| deduplicate_integer_0_1 | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 6,037,735.8 | 6,037,735.8 |
| deduplicate_integer_8_8 | 3/4/1 | 3/4/1 | 776/592 | 776/592 | 3,216,080.4 | 3,307,493.5 |
| deduplicate_integer_32_32 | 3/8/1 | 3/8/1 | 3,464/2,896 | 3,464/2,896 | 3,941,493.5 | 4,015,686.3 |
| deduplicate_integer_127_127 | 3/12/1 | 3/12/1 | 14,216/12,112 | 14,216/12,112 | 2,482,438.5 | 2,485,626.9 |
| deduplicate_integer_128_128 | 3/12/1 | 6/10/4 | 14,216/12,112 | 36,152/34,048 | 2,531,520.4 | 2,595,855.3 |
| deduplicate_integer_256_256 | 3/14/1 | 7/11/5 | 28,552/24,400 | 74,056/69,904 | 1,553,398.1 | 2,582,434.9 |
| deduplicate_integer_1024_1024 | 3/18/1 | 9/13/7 | 114,568/98,128 | 301,416/284,976 | 453,411.4 | 2,629,412.4 |
| deduplicate_integer_1024_8 | 3/4/1 | 3/4/1 | 776/592 | 776/592 | 13,768,645.7 | 13,337,675.0 |
| deduplicate_integer_1024_32 | 3/8/1 | 3/8/1 | 3,464/2,896 | 3,464/2,896 | 8,104,070.8 | 7,924,738.2 |
| deduplicate_string_8_8 | 11/4/9 | 11/4/9 | 904/720 | 904/720 | 1,457,029.0 | 1,443,880.4 |
| deduplicate_string_128_128 | 131/12/129 | 133/10/131 | 16,264/14,160 | 21,288/19,184 | 161,491.3 | 1,241,287.4 |
| deduplicate_string_512_512 | 515/16/513 | 519/12/517 | 65,416/57,168 | 90,440/82,192 | 40,981.2 | 1,726,703.6 |
| deduplicate_string_1024_8 | 1027/4/1025 | 1027/4/1025 | 17,160/16,976 | 17,160/16,976 | 1,732,619.9 | 1,741,246.7 |
| deduplicate_table_128_128 | 131/12/129 | 37/10/35 | 16,264/14,160 | 19,752/17,648 | 848,015.6 | 2,926,759.6 |
| deduplicate_table_512_512 | 515/16/513 | 39/12/37 | 65,416/57,168 | 82,760/74,512 | 255,065.4 | 3,506,699.2 |
| split_empty | 2/0/0 | 2/0/0 | 72/0 | 72/0 | 2,490,272.4 | 2,776,572.7 |
| split_one | 3/0/1 | 2/0/0 | 73/1 | 72/0 | 2,554,890.2 | 2,735,042.7 |
| split_ascii_32 | 34/5/32 | 2/0/0 | 1,096/528 | 568/0 | 5,351,450.2 | 13,894,165.5 |
| split_ascii_256 | 258/8/256 | 2/0/0 | 8,488/4,336 | 4,152/0 | 6,600,729.2 | 15,765,215.3 |
| split_unicode_256 | 258/8/256 | 2/0/0 | 8,872/4,720 | 4,152/0 | 6,316,357.6 | 13,940,270.6 |
| split_unicode_1024 | 1026/10/1024 | 2/0/0 | 35,368/18,928 | 16,440/0 | 6,203,094.2 | 14,301,207.8 |
| split_separator_256 | 2/9/0 | 2/9/0 | 16,424/8,176 | 16,424/8,176 | 10,486,786.3 | 10,310,286.5 |
| split_absent_1024 | 3/0/0 | 3/0/0 | 1,121/0 | 1,121/0 | 518,848.8 | 630,231.4 |
| stringify_empty | 2/0/1 | 2/0/1 | 64/8 | 64/8 | 1,502,347.4 | 1,545,893.7 |
| stringify_one | 6/0/4 | 5/0/3 | 264/192 | 104/32 | 570,410.0 | 653,728.3 |
| stringify_numeric_32 | 99/5/97 | 67/5/65 | 6,960/6,392 | 1,840/1,272 | 828,110.5 | 971,398.8 |
| stringify_numeric_256 | 771/8/769 | 515/8/513 | 55,344/51,192 | 14,384/10,232 | 830,663.2 | 987,815.1 |
| stringify_numeric_1024 | 3075/10/3073 | 2051/10/2049 | 221,232/204,792 | 57,392/40,952 | 823,644.6 | 965,748.8 |
| stringify_unformatted_256 | 258/8/256 | 258/8/256 | 10,280/6,128 | 10,280/6,128 | 2,787,105.6 | 2,742,321.5 |
| stringify_strings_256 | 515/8/513 | 515/8/513 | 14,384/10,232 | 14,384/10,232 | 2,414,738.4 | 2,420,195.9 |

Per-run paired cycle savings for representative cases:

- `split_ascii_256`: 59.41%, 57.56%, 58.10%.
- `split_unicode_1024`: 57.11%, 54.56%, 58.16%.
- `stringify_numeric_256`: 15.03%, 15.98%, 18.55%.
- `deduplicate_integer_1024_1024`: 82.56%, 76.07%, 82.81%.
- `deduplicate_string_512_512`: 97.62%, 97.55%, 97.86%.
- `deduplicate_table_512_512`: 92.59%, 93.34%, 92.70%.

Cases with higher median cycle counts:

- `deduplicate_integer_127_127`: 2.43% more cycles; elapsed 51159.4 -> 51093.8 ns/op.
- `deduplicate_integer_1024_8`: 3.23% more cycles; elapsed 74371.9 -> 76775.0 ns/op.
- `deduplicate_integer_1024_32`: 2.25% more cycles; elapsed 126356.2 -> 129215.6 ns/op.
- `deduplicate_string_8_8`: 0.68% more cycles; elapsed 5490.6 -> 5540.6 ns/op.
- `split_separator_256`: 1.93% more cycles; elapsed 24507.0 -> 24926.6 ns/op.
- `stringify_unformatted_256`: 1.65% more cycles; elapsed 91851.6 -> 93351.6 ns/op.


## Repeatability checks

Six additional runs of all 30 pairs used the same final executable, alternating
old/new order. They investigate the slower controls in the primary three runs;
they do not replace those results. All allocator counts and byte totals match
the primary runs. Positive percentages below mean fewer cycles.

| Scenario | Cycle savings in runs 1 / 2 / 3 / 4 / 5 / 6 | Median paired savings |
|---|---:|---:|
| deduplicate_integer_0_1 | 0.00% / -1.65% / -3.50% / 0.00% / 0.00% / 0.00% | 0.00% |
| deduplicate_integer_8_8 | 0.71% / -0.39% / -21.09% / -30.75% / -0.12% / 12.01% | -0.26% |
| deduplicate_integer_32_32 | -5.33% / -2.50% / 16.84% / -2.11% / -0.51% / -6.70% | -2.30% |
| deduplicate_integer_127_127 | -3.30% / -0.98% / -2.97% / 5.47% / -1.84% / 1.82% | -1.41% |
| deduplicate_integer_128_128 | 3.77% / 4.25% / 1.20% / 8.43% / 3.35% / 1.38% | 3.56% |
| deduplicate_integer_256_256 | 41.55% / 41.36% / 38.04% / 35.64% / 40.41% / 43.41% | 40.89% |
| deduplicate_integer_1024_1024 | 81.86% / 74.71% / 82.61% / 82.97% / 82.09% / 74.39% | 81.97% |
| deduplicate_integer_1024_8 | 1.60% / -3.56% / -9.38% / 0.18% / -3.71% / 4.39% | -1.69% |
| deduplicate_integer_1024_32 | -10.57% / -1.66% / -2.60% / -7.20% / -13.17% / -6.13% | -6.67% |
| deduplicate_string_8_8 | 6.16% / -5.61% / -0.65% / -0.06% / 28.29% / 0.89% | 0.42% |
| deduplicate_string_128_128 | 86.88% / 86.15% / 85.85% / 86.66% / 87.41% / 86.42% | 86.54% |
| deduplicate_string_512_512 | 97.63% / 97.48% / 97.51% / 97.73% / 97.87% / 97.66% | 97.64% |
| deduplicate_string_1024_8 | -0.86% / -6.89% / -7.91% / -1.64% / 4.32% / 0.05% | -1.25% |
| deduplicate_table_128_128 | 71.40% / 68.21% / 74.18% / 70.62% / 69.77% / 71.25% | 70.94% |
| deduplicate_table_512_512 | 93.01% / 92.19% / 91.90% / 92.76% / 92.65% / 92.75% | 92.70% |
| split_empty | -7.23% / -0.45% / -6.96% / 5.43% / 23.07% / 24.17% | 2.49% |
| split_one | 40.72% / 40.58% / -7.21% / 26.28% / 16.54% / 30.71% | 28.50% |
| split_ascii_32 | 61.25% / 66.12% / 66.98% / 63.62% / 65.72% / 66.00% | 65.86% |
| split_ascii_256 | 57.15% / 60.12% / 54.65% / 58.68% / 60.24% / 59.52% | 59.10% |
| split_unicode_256 | 57.87% / 59.02% / 58.46% / 57.70% / 54.99% / 60.06% | 58.17% |
| split_unicode_1024 | 56.40% / 55.28% / 58.02% / 55.18% / 55.00% / 56.59% | 55.84% |
| split_separator_256 | -2.83% / -2.68% / -19.31% / 3.15% / 2.80% / 5.64% | 0.06% |
| split_absent_1024 | 16.71% / -57.42% / 21.66% / -23.25% / 6.91% / -21.23% | -7.16% |
| stringify_empty | 11.16% / -8.01% / 8.55% / 3.19% / 8.29% / 1.61% | 5.74% |
| stringify_one | 14.29% / 13.25% / 10.57% / 14.32% / 23.32% / 14.23% | 14.26% |
| stringify_numeric_32 | 13.97% / 14.31% / 15.18% / 14.57% / 17.20% / 15.23% | 14.88% |
| stringify_numeric_256 | 9.99% / 16.66% / 15.21% / 15.90% / 15.72% / 16.01% | 15.81% |
| stringify_numeric_1024 | 15.43% / 13.13% / 12.63% / 12.87% / 15.04% / 14.05% | 13.59% |
| stringify_unformatted_256 | -0.92% / -2.66% / 3.33% / -1.43% / 1.32% / -2.78% | -1.17% |
| stringify_strings_256 | 0.62% / 5.94% / -1.94% / 1.15% / -2.74% / 1.30% | 0.88% |

For the controls that were slower in the primary aggregate:

- `deduplicate_integer_127_127`: median paired cycle savings -1.41%; 4/6 runs slower; elapsed medians 50317.1 -> 51040.6 ns/op.
- `deduplicate_integer_1024_8`: median paired cycle savings -1.69%; 3/6 runs slower; elapsed medians 75984.4 -> 77484.4 ns/op.
- `deduplicate_integer_1024_32`: median paired cycle savings -6.67%; 6/6 runs slower; elapsed medians 124003.1 -> 131218.8 ns/op.
- `deduplicate_string_8_8`: median paired cycle savings 0.42%; 3/6 runs slower; elapsed medians 5492.2 -> 5340.6 ns/op.
- `split_separator_256`: median paired cycle savings 0.06%; 3/6 runs slower; elapsed medians 24973.8 -> 24762.5 ns/op.
- `stringify_unformatted_256`: median paired cycle savings -1.17%; 4/6 runs slower; elapsed medians 92696.1 -> 93513.3 ns/op.

These checks quantify variation and fixed costs; they do not establish a speedup
for unchanged paths or for unmeasured workloads.

Repeatable control costs in this build include `deduplicate_integer_32_32` (2.30% more cycles, 5/6 runs slower), `deduplicate_integer_1024_32` (6.67% more cycles, 6/6 runs slower). These measured regressions are retained explicitly; no gain is claimed for these controls.

The optimized character, numeric-formatting, and large deduplication cases retain substantial gains across these repetitions.

## Reproduction

The committed tests and baseline modules reproduce the old/new comparison:

```powershell
cargo test -p deadsync-song-lua --lib lua_cleanup_ --locked
cargo test -p deadsync-song-lua --release --lib lua_cleanup_bench --locked -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --release --lib lua_cleanup_bench --locked -- --ignored --test-threads=1 --nocapture
Remove-Item Env:\DEADSYNC_PERF_REVERSE
```

Local raw logs, parsed results, parent/version/source audits, and orchestration
scripts are in ignored `target/lua-cleanup-perf/`. The report and six test/baseline
files are committed. `deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`,
and `optimize.ps1` are excluded from the commit.

Benchmark executable SHA-256: `bae5242c50c3c9914881096871199994000c5d5d599451b7f04f193d89ff779d`.
