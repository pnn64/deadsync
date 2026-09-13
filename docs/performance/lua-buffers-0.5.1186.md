# Lua array capacity, formatting buffers and recursive discovery — 0.5.1186

Date: 2026-09-13. Parent: `eeb449635856c265a7e337c4cb7c171904065d4a`
(0.5.1185). The workspace patch version advances once to 0.5.1186;
Cargo.lock updates the three packages inheriting that version.

## Representative results

- **1,024 short strings:** 15.13% fewer CPU cycles; allocation/reallocation calls 2/10 → 2/0; requested bytes 32,808 → 16,440.
- **Actor settextf callback:** 11.17% fewer CPU cycles; allocation/reallocation calls 5/0 → 3/0; requested bytes 322 → 146.
- **128-function upvalue chain:** 25.74% fewer CPU cycles; allocation/reallocation calls 1407/124 → 1025/5; requested bytes 258,832 → 50,464.

See the complete tables and control discussion below; these figures are not universal speedups.

## Changes

1. **Reserve dense string-table capacity.** Both borrowed and owned string-array
   builders know the exact number of entries. They now allocate the Lua array
   storage at that size instead of growing it during insertion. All input
   strings occupy a sequence slot, including empty strings, so the capacity
   matches the resulting dense sequence. Output tables remain fresh and own
   their strings.

2. **Reuse `settextf`'s argument buffer.** The actor callback already owns a
   `MultiValue`. A private helper temporarily removes the receiver, passes the
   remaining buffer by reference to mlua, and restores the receiver on success
   or error. This avoids the second argument vector and handle clones, while
   retaining the original arguments through the subsequent `Text` setter.
   The public borrowed `lua_format_text` API and implementation are unchanged.

3. **Append recursive upvalue discovery to one output vector.** Each recursive
   level previously allocated a result vector and copied its elements into its
   parent. A private collector now appends to one shared output while retaining
   depth-first discovery order and the existing seen-table/function sets.
   Function handles are borrowed for `getupvalue` calls, and accepted table
   handles move directly into the result instead of being cloned. This removes
   per-level result buffers and repeated copying during recursion unwinding.

These changes apply the local `rust-performance.md` guidance on initial
capacity (M-INITIAL-CAPACITY), resource reuse (M-MEM-REUSE), and measuring
allocation/copy costs in frequently used paths (M-HOTPATH). There is no new
dependency, unsafe code or public API change. The measurements below cover
specific helpers and the actual actor text callback; they do not establish
whole-song compile-time or frame-rate gains.

## Behavior and allocation checks

Four complete parent function bodies and the original `settextf` installation
block are frozen in the test fixtures. A source audit checks them against the
parent commit and reconstructs the parent production file from the final diff.
The baseline text-callback fixture wraps the exact original installation block;
both old and new actors receive the same other visual/text methods before the
benchmark. Shared helper code outside this pass remains unchanged.

Fifteen new regression tests cover:

- Dense sequence order and length at capacity boundaries; empty and duplicate
  strings; Unicode and NUL bytes; long strings; fresh table identities;
  independent mutation and append; and output ownership after source drop and
  Lua garbage collection.
- Zero-to-64 formatting arguments, method and plain calls, displaced deque
  storage, exact argument restoration, custom formatter returns, invalid UTF-8,
  missing formatter tables, conversion errors and empty-input behavior.
- Receiver lifetime during formatter garbage collection and argument lifetime
  during a `Text` metamethod; real `settextf` return identity, text updates and
  preservation of the previous text when formatting fails.
- Recursive discovery before and after visiting children, cycles, shared
  tables/functions, existing seen sets, invalid UTF-8 names, non-string names,
  callback order, partial seen-set state on errors, and retained table identity
  after dropping the source graph.
- Dense 128-string table creation stays within two allocations and performs no
  reallocations. In-place formatting with a nil-returning callback has zero
  allocation, reallocation, free or byte churn. Re-visiting a seen function root
  also has zero churn.

Validation:

- Fresh parent debug library suite: **566 passed, 5 failed, 48 ignored**.
- Final debug and release library suites: **581 passed, 5 failed, 51 ignored**.
- All 15 new regression tests pass in both profiles. The three new manual
  benchmarks are ignored in ordinary test runs; all 39 paired fixtures also
  passed a debug smoke run before release measurements.
- `cargo check --all-targets --offline`: root application check passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed; existing non-performance warnings remain.
- Rustfmt checks for all seven changed Rust files, baseline/source/version
  audits and Git whitespace checks passed.

The same five parent assertions fail in the final suites:

- `compile_song_lua_extracts_actorproxy_targets`: visibility assertion.
- `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0 versus 123.
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: visibility assertion.
- `compile_song_lua_runs_cmd_queuecommand_builders`: visibility assertion.
- `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

The full suite is therefore not green. There was no separate parent release
build; frozen parent implementations were compiled and tested alongside the
new implementation in the final release test executable.

## Benchmark method

- Five serial release runs of **39 old/new pairs**, with seven timing samples
  after three warmups per side. Runs 2 and 4 reverse old/new order. Tables show
  medians across runs and all five per-run paired CPU-cycle comparisons.
- The existing `tests/support/perf.rs` harness measures wall time and Windows
  `QueryThreadCycleTime` on the calling thread. A separate warmed operation
  counts allocation/reallocation/free calls and requested/freed bytes through
  the scoped global allocator. Reallocation sizes count toward byte churn;
  timing runs do not enable allocation counting.
- Lua states, source strings, input graphs and callbacks are prepared outside
  timing. Rust output destruction is included. Lua collection runs before each
  pair and is then stopped; deferred Lua collection and VM teardown are excluded.
  These byte counts are allocator requests, not peak live memory, retained RSS
  or measured hardware memory traffic.
- String-array cases use 16 iterations per sample and report input strings/s.
  String data and borrowed views are prepared once; Lua output construction is
  included. Short strings can use Lua interning after warmup, while long strings
  still allocate on both sides.
- Formatting uses 128 iterations and reports complete operations/s. Helper
  cases include the same incoming `MultiValue` clone on both sides. The
  `settextf` case invokes the actual installed callback; registration is outside
  timing and actor table contents are otherwise equivalent.
- Upvalue discovery uses 32 iterations and reports input functions/s. Seen-set
  capacities are prepared outside timing and retained across iterations; their
  clearing, traversal, result allocation and result destruction are included.
  Repeated/shared-node and unmatched-name controls exercise those paths. The
  already-seen control counts the nominal graph size, not functions visited.
  Empty inputs use one nominal throughput unit.
- No Rust compiler or linker process was running when a benchmark run started.
  Relevant source files and the release binary were hashed before measurement
  and checked afterwards.

The in-place helper restores the original argument buffer on ordinary success
and returned errors. Lua formatting and conversion can still allocate output
strings. Recursive discovery still uses stack recursion, seen sets and one
growable result vector, and Lua debug queries have their own costs. These
changes do not make all three complete operations allocation-free.

## Reproduction

```powershell
cargo test -p deadsync-song-lua --lib lua_buffer_pass_ --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo test -p deadsync-song-lua --lib --release lua_buffer_pass_bench --locked -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release lua_buffer_pass_bench --locked -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
```

The full library command reports the five known failures above. The filtered
regression and manual benchmark commands pass. Repeated measurements invoke
the completed release executable directly.

## Environment

- CPU: Intel(R) Xeon(R) CPU E5-2696 v4 @ 2.20GHz.
- Windows x86_64, Rust release profile (`opt-level = 3`, full LTO).
- `rustc 1.98.0 (88d9e12ae 2026-08-18)`, LLVM 22.1.8.
- Final release test binary SHA-256: `d2f18176218956a6e66ceeaacfb069e6fe011a1d47351276062c92344b4cc3e4`.

## Paired release results

Medians across five runs. Positive cycle savings indicate an improvement for this workload on this machine. All allocation counts and byte totals were identical across the five repetitions.

| Scenario | Old µs/op | New µs/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| string_arrays_borrowed_short_0 | 0.1000 | 0.1000 | 288.1 | 301.8 | -4.76% |
| string_arrays_borrowed_short_1 | 0.2562 | 0.2312 | 685.9 | 644.8 | 5.99% |
| string_arrays_borrowed_short_4 | 0.7812 | 0.3812 | 1,783.4 | 919.2 | 48.46% |
| string_arrays_borrowed_short_16 | 1.9062 | 1.2438 | 4,252.8 | 2,798.6 | 34.19% |
| string_arrays_borrowed_short_128 | 9.6062 | 8.2125 | 21,181.8 | 18,108.8 | 14.51% |
| string_arrays_borrowed_short_1024 | 76.0000 | 64.6812 | 166,545.6 | 141,344.2 | 15.13% |
| string_arrays_borrowed_long_128 | 26.6000 | 26.1312 | 58,537.9 | 56,370.3 | 3.70% |
| string_arrays_borrowed_long_1024 | 203.1625 | 201.8125 | 442,745.2 | 436,036.8 | 1.52% |
| string_arrays_owned_short_0 | 0.0938 | 0.0938 | 274.4 | 274.4 | 0.00% |
| string_arrays_owned_short_1 | 0.2500 | 0.2188 | 644.8 | 548.8 | 14.89% |
| string_arrays_owned_short_4 | 0.6688 | 0.3750 | 1,550.2 | 919.1 | 40.71% |
| string_arrays_owned_short_16 | 1.7250 | 1.1625 | 3,868.7 | 2,620.2 | 32.27% |
| string_arrays_owned_short_128 | 9.5375 | 8.0812 | 21,044.6 | 17,820.6 | 15.32% |
| string_arrays_owned_short_1024 | 71.0312 | 66.4875 | 155,502.1 | 145,569.6 | 6.39% |
| string_arrays_owned_long_128 | 25.6938 | 24.0062 | 55,931.4 | 52,803.4 | 5.59% |
| string_arrays_owned_long_1024 | 197.9125 | 203.1812 | 434,033.8 | 442,937.2 | -2.05% |
| text_arguments_plain_0 | 0.0281 | 0.0352 | 72.0 | 85.7 | -19.03% |
| text_arguments_plain_1 | 0.6492 | 0.5719 | 1,435.3 | 1,263.8 | 11.95% |
| text_arguments_plain_2 | 0.8523 | 0.7492 | 1,881.2 | 1,654.8 | 12.03% |
| text_arguments_plain_4 | 1.1305 | 1.0031 | 2,490.0 | 2,210.4 | 11.23% |
| text_arguments_plain_8 | 1.9445 | 1.4609 | 4,282.0 | 3,215.3 | 24.91% |
| text_arguments_plain_64 | 9.5688 | 8.1852 | 20,977.7 | 17,966.4 | 14.35% |
| text_arguments_method_0 | 0.0898 | 0.0969 | 205.8 | 221.2 | -7.48% |
| text_arguments_method_1 | 0.6609 | 0.6391 | 1,461.0 | 1,411.3 | 3.40% |
| text_arguments_method_2 | 0.8695 | 0.8008 | 1,918.9 | 1,766.3 | 7.95% |
| text_arguments_method_4 | 1.1477 | 1.0305 | 2,531.1 | 2,270.5 | 10.30% |
| text_arguments_method_8 | 1.8578 | 1.5961 | 4,088.2 | 3,512.0 | 14.09% |
| text_arguments_method_64 | 9.4859 | 8.1859 | 20,833.6 | 17,940.7 | 13.89% |
| text_arguments_settextf | 1.3453 | 1.1945 | 2,963.2 | 2,632.3 | 11.17% |
| upvalue_output_chain_0 | 0.3281 | 0.3250 | 754.5 | 754.5 | 0.00% |
| upvalue_output_chain_1 | 1.9438 | 1.9156 | 4,300.8 | 4,239.1 | 1.43% |
| upvalue_output_chain_4 | 8.1094 | 7.3406 | 17,841.2 | 16,147.0 | 9.50% |
| upvalue_output_chain_16 | 35.1844 | 30.0438 | 77,167.9 | 65,870.6 | 14.64% |
| upvalue_output_chain_64 | 155.4781 | 124.0281 | 341,034.4 | 272,063.4 | 20.22% |
| upvalue_output_chain_128 | 347.8344 | 258.2406 | 761,911.9 | 565,768.2 | 25.74% |
| upvalue_output_postorder_128 | 422.4562 | 255.8062 | 925,261.1 | 559,958.2 | 39.48% |
| upvalue_output_shared_128 | 265.9688 | 249.9562 | 582,937.2 | 547,755.4 | 6.04% |
| upvalue_output_unmatched_128 | 261.9219 | 250.7906 | 573,875.8 | 549,003.8 | 4.33% |
| upvalue_output_seen_128 | 0.0656 | 0.0719 | 178.3 | 192.1 | -7.74% |

A/R/F means allocation/reallocation/free calls. Requested/freed bytes include reallocations. Lua garbage collection is excluded from timing; see methodology.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| string_arrays_borrowed_short_0 | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 10,000,000.0 | 10,000,000.0 |
| string_arrays_borrowed_short_1 | 2/0/0 | 2/0/0 | 72/0 | 72/0 | 3,902,439.0 | 4,324,324.3 |
| string_arrays_borrowed_short_4 | 2/2/0 | 2/0/0 | 168/48 | 120/0 | 5,120,000.0 | 10,491,803.3 |
| string_arrays_borrowed_short_16 | 2/4/0 | 2/0/0 | 552/240 | 312/0 | 8,393,442.6 | 12,864,321.6 |
| string_arrays_borrowed_short_128 | 2/7/0 | 2/0/0 | 4,136/2,032 | 2,104/0 | 13,324,658.4 | 15,585,997.0 |
| string_arrays_borrowed_short_1024 | 2/10/0 | 2/0/0 | 32,808/16,368 | 16,440/0 | 13,473,684.2 | 15,831,481.3 |
| string_arrays_borrowed_long_128 | 130/7/0 | 130/0/0 | 28,218/2,032 | 26,186/0 | 4,812,030.1 | 4,898,349.7 |
| string_arrays_borrowed_long_1024 | 1026/10/0 | 1026/0/0 | 226,258/16,368 | 209,890/0 | 5,040,300.3 | 5,074,016.7 |
| string_arrays_owned_short_0 | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 10,666,666.7 | 10,666,666.7 |
| string_arrays_owned_short_1 | 2/0/0 | 2/0/0 | 72/0 | 72/0 | 4,000,000.0 | 4,571,428.6 |
| string_arrays_owned_short_4 | 2/2/0 | 2/0/0 | 168/48 | 120/0 | 5,981,308.4 | 10,666,666.7 |
| string_arrays_owned_short_16 | 2/4/0 | 2/0/0 | 552/240 | 312/0 | 9,275,362.3 | 13,763,440.9 |
| string_arrays_owned_short_128 | 2/7/0 | 2/0/0 | 4,136/2,032 | 2,104/0 | 13,420,707.7 | 15,839,133.8 |
| string_arrays_owned_short_1024 | 2/10/0 | 2/0/0 | 32,808/16,368 | 16,440/0 | 14,416,190.1 | 15,401,391.2 |
| string_arrays_owned_long_128 | 130/7/0 | 130/0/0 | 28,218/2,032 | 26,186/0 | 4,981,756.3 | 5,331,944.8 |
| string_arrays_owned_long_1024 | 1026/10/0 | 1026/0/0 | 226,258/16,368 | 209,890/0 | 5,174,003.7 | 5,039,835.1 |
| text_arguments_plain_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 35,555,555.6 | 28,444,444.4 |
| text_arguments_plain_1 | 3/0/3 | 2/0/2 | 216/216 | 56/56 | 1,540,312.9 | 1,748,633.9 |
| text_arguments_plain_2 | 4/0/4 | 3/0/3 | 264/264 | 104/104 | 1,173,235.6 | 1,334,723.7 |
| text_arguments_plain_4 | 4/0/4 | 3/0/3 | 344/344 | 184/184 | 884,588.8 | 996,884.7 |
| text_arguments_plain_8 | 4/1/4 | 3/0/3 | 829/829 | 349/349 | 514,262.8 | 684,492.0 |
| text_arguments_plain_64 | 5/4/4 | 4/0/3 | 7,919/7,715 | 2,959/2,755 | 104,506.9 | 122,172.4 |
| text_arguments_method_0 | 1/0/1 | 1/0/1 | 40/40 | 40/40 | 11,130,434.8 | 10,322,580.6 |
| text_arguments_method_1 | 3/0/3 | 2/0/2 | 256/256 | 96/96 | 1,513,002.4 | 1,564,792.2 |
| text_arguments_method_2 | 4/0/4 | 3/0/3 | 304/304 | 144/144 | 1,150,044.9 | 1,248,780.5 |
| text_arguments_method_4 | 4/0/4 | 3/0/3 | 384/384 | 224/224 | 871,341.0 | 970,432.1 |
| text_arguments_method_8 | 4/1/4 | 3/0/3 | 869/869 | 389/389 | 538,267.5 | 626,529.6 |
| text_arguments_method_64 | 5/4/4 | 4/0/3 | 7,959/7,755 | 2,999/2,795 | 105,419.2 | 122,160.7 |
| text_arguments_settextf | 5/0/5 | 3/0/3 | 322/322 | 146/146 | 743,321.7 | 837,148.5 |
| upvalue_output_chain_0 | 2/0/2 | 2/0/2 | 123/123 | 123/123 | 3,047,619.0 | 3,076,923.1 |
| upvalue_output_chain_1 | 10/0/10 | 9/0/9 | 459/459 | 443/443 | 514,469.5 | 522,022.8 |
| upvalue_output_chain_4 | 43/0/43 | 33/0/33 | 1,884/1,884 | 1,484/1,484 | 493,256.3 | 544,912.7 |
| upvalue_output_chain_16 | 175/12/175 | 129/2/129 | 10,752/10,752 | 6,224/6,224 | 454,747.3 | 532,556.7 |
| upvalue_output_chain_64 | 703/60/703 | 513/4/513 | 80,208/80,208 | 25,184/25,184 | 411,633.5 | 516,012.0 |
| upvalue_output_chain_128 | 1407/124/1407 | 1025/5/1025 | 258,832/258,832 | 50,464/50,464 | 367,991.2 | 495,661.7 |
| upvalue_output_postorder_128 | 1407/124/1407 | 1025/5/1025 | 633,664/633,664 | 50,464/50,464 | 302,990.0 | 500,378.7 |
| upvalue_output_shared_128 | 1153/0/1153 | 1025/0/1025 | 46,560/46,560 | 44,512/44,512 | 481,259.5 | 512,089.6 |
| upvalue_output_unmatched_128 | 1151/0/1151 | 1024/0/1024 | 46,448/46,448 | 44,416/44,416 | 488,695.3 | 510,385.9 |
| upvalue_output_seen_128 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 1,950,476,190.5 | 1,780,869,565.2 |

## Repeatability and controls

Per-run paired cycle savings; positive is faster. These ratios use each run’s own old/new measurements.

| Scenario | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 |
|---|---:|---:|---:|---:|---:|
| string_arrays_borrowed_short_0 | -4.76% | 4.76% | -4.57% | 4.54% | -4.76% |
| string_arrays_borrowed_short_1 | 12.24% | 39.44% | -12.00% | 32.85% | 0.00% |
| string_arrays_borrowed_short_4 | 50.75% | 23.08% | 50.00% | 25.43% | 50.00% |
| string_arrays_borrowed_short_16 | 30.14% | 37.74% | 28.05% | 38.60% | 26.32% |
| string_arrays_borrowed_short_128 | 14.51% | 0.44% | 27.90% | 4.11% | 33.43% |
| string_arrays_borrowed_short_1024 | 13.11% | 22.63% | 8.01% | 11.41% | 28.93% |
| string_arrays_borrowed_long_128 | -3.32% | 9.25% | 3.70% | 11.65% | 3.14% |
| string_arrays_borrowed_long_1024 | 9.45% | 5.99% | -7.90% | 7.85% | -1.83% |
| string_arrays_owned_short_0 | 0.00% | 4.99% | 0.00% | 0.00% | 0.00% |
| string_arrays_owned_short_1 | 19.99% | 11.11% | 12.76% | 11.11% | 16.66% |
| string_arrays_owned_short_4 | 40.18% | 40.70% | 41.59% | 40.71% | 42.48% |
| string_arrays_owned_short_16 | 33.57% | 41.22% | 32.27% | 34.96% | 34.13% |
| string_arrays_owned_short_128 | 23.09% | 13.95% | 12.64% | 21.79% | 28.20% |
| string_arrays_owned_short_1024 | 9.01% | 7.04% | 4.07% | 6.28% | 7.05% |
| string_arrays_owned_long_128 | 3.07% | 9.81% | 5.59% | 3.00% | 10.91% |
| string_arrays_owned_long_1024 | 4.24% | 6.35% | -2.05% | -1.10% | -4.28% |
| text_arguments_plain_0 | -21.03% | -30.97% | -17.49% | -20.48% | -19.03% |
| text_arguments_plain_1 | 10.08% | 15.87% | 14.70% | 14.09% | 15.26% |
| text_arguments_plain_2 | 12.67% | 13.14% | 5.59% | 12.85% | 12.71% |
| text_arguments_plain_4 | 11.30% | 10.59% | 9.56% | 15.93% | 11.23% |
| text_arguments_plain_8 | 20.95% | 19.91% | 25.06% | 16.63% | 24.91% |
| text_arguments_plain_64 | 16.21% | 14.23% | 13.45% | 16.60% | 14.80% |
| text_arguments_method_0 | -6.66% | -6.71% | -15.84% | -5.67% | -6.60% |
| text_arguments_method_1 | -13.66% | 13.68% | 13.51% | 17.62% | -14.56% |
| text_arguments_method_2 | 12.26% | 11.44% | 6.19% | 9.74% | 13.06% |
| text_arguments_method_4 | 11.20% | 8.68% | 10.84% | 10.80% | 10.77% |
| text_arguments_method_8 | 12.67% | 13.44% | 14.30% | 13.97% | 14.32% |
| text_arguments_method_64 | 16.69% | 9.31% | 20.40% | 13.86% | 13.29% |
| text_arguments_settextf | 12.79% | 12.82% | -31.82% | 11.12% | 7.12% |
| upvalue_output_chain_0 | 0.00% | 0.00% | 8.46% | -6.42% | 4.35% |
| upvalue_output_chain_1 | -1.93% | 1.28% | 27.55% | 3.67% | 22.76% |
| upvalue_output_chain_4 | 3.35% | 8.65% | 9.11% | 8.87% | 10.83% |
| upvalue_output_chain_16 | 14.89% | 14.64% | 15.40% | 9.95% | -5.90% |
| upvalue_output_chain_64 | 20.95% | 19.99% | 19.43% | 24.94% | 21.88% |
| upvalue_output_chain_128 | 27.13% | 20.68% | 23.98% | 26.93% | 26.90% |
| upvalue_output_postorder_128 | 37.68% | 38.41% | 38.32% | 41.43% | 47.08% |
| upvalue_output_shared_128 | 3.87% | 6.13% | 4.93% | 7.75% | 4.11% |
| upvalue_output_unmatched_128 | 4.39% | 4.52% | 2.29% | 4.57% | 6.17% |
| upvalue_output_seen_128 | -3.73% | -14.79% | -3.87% | -7.74% | -7.74% |

Cases with higher aggregate median CPU cycles:

- `string_arrays_borrowed_short_0`: 4.76% more cycles; wall time 100.0 → 100.0 ns/op.
- `string_arrays_owned_long_1024`: 2.05% more cycles; wall time 197912.5 → 203181.2 ns/op.
- `text_arguments_plain_0`: 19.03% more cycles; wall time 28.1 → 35.2 ns/op.
- `text_arguments_method_0`: 7.48% more cycles; wall time 89.8 → 96.9 ns/op.
- `upvalue_output_seen_128`: 7.74% more cycles; wall time 65.6 → 71.9 ns/op.

### Interpretation and tradeoffs

- The 1,024-short-string borrowed case improves in all five runs (8.01% to
  28.93% fewer cycles), with median throughput increasing from 13.47 to 15.83
  million strings/s. The corresponding owned case also improves in all five
  runs. Both remove ten table reallocations and about half the requested bytes.
- Long strings still require Lua string allocations. At 1,024 owned long
  strings, aggregate cycles increase by 2.05% and wall time by about 5.27 us/op;
  individual paired savings range from -4.28% to +6.35%. There is no established
  CPU gain for this input. Ten reallocations are still removed and requested
  bytes fall from 226,258 to 209,890. The borrowed long-string result also varies
  across runs, so its small aggregate CPU gain should not be generalized.
- The actual `settextf` callback has 11.17% fewer aggregate cycles and throughput
  increases from about 743,322 to 837,149 operations/s. Four runs improve by
  7.12% to 12.82%; run 3 is 31.82% slower. This variability remains in the table.
  The allocation reduction from five to three calls and requested/freed byte
  reduction from 322 to 146 are identical in every run. One-argument method
  formatting is also variable, despite the consistent buffer allocation saving.
- The 128-function chain improves in every run (20.68% to 27.13% fewer cycles).
  Median throughput increases from about 367,991 to 495,662 input functions/s,
  and requested/freed bytes fall by 80.50%. Postorder traversal benefits more
  because the old implementation repeatedly grows each ancestor's result
  vector while unwinding. Shared-table and unmatched-name traversals also
  improve in all five runs, with smaller allocation savings.
- Empty formatting is consistently slower: plain calls rise from 28.1 to
  35.2 ns/op and receiver-only calls from 89.8 to 96.9 ns/op. These paths have
  no redundant formatting buffer to remove. The new helper retains general
  receiver removal/restoration, accepting this roughly 7 ns cost for these
  degenerate inputs. The already-seen discovery control also rises by about
  6.3 ns/op and remains allocation-free. Neither is reported as a speedup.
- Empty borrowed arrays have unchanged median wall time (100 ns/op) and
  unchanged allocation counts; the small cycle difference changes sign with
  execution order. Empty owned arrays have unchanged aggregate cycles. These
  controls provide no evidence of a performance improvement.

The report preserves all five runs, including outliers and slower controls.
These are targeted helper measurements with Lua collection excluded, not a
claim of universal cycle savings or measured end-to-end application speedups.
