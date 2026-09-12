# Lua alignment, speed-mod, and capture access - 0.5.1176

Parent: `a82c06196b06e11cd809dd5cd8972ecc007d6999` (`0.5.1175`).
This pass applies `rust-performance.md` guidance on measuring hot paths
(M-HOTPATH) and reusing allocations (M-MEM-REUSE). The targets are repeated
string ownership/formatting and temporary collections in song-Lua work.

## Three changes

1. **Borrow alignment text.** Horizontal/vertical alignment conversion reads a
   Lua string once, tries numeric parsing first, then trims quotes and repeated
   alignment prefixes on a borrowed slice. ASCII case-insensitive comparisons
   replace temporary owned strings. Text alignment also borrows its input.
   Numeric conversion, prefix-stripping order, Unicode trimming, and rejection
   of invalid UTF-8 remain unchanged. The public owned-string token helper
   retains its signature and implementation.
2. **Reuse speed-mod field names.** Installed methods use the complete field key
   already held by their closure. Direct writes from `PlayerOptions:FromString`
   use static field names for its five known speed-mod keys. Unknown direct
   keys retain formatting. Set/clear order, raw table writes, callback returns,
   and getter behavior remain unchanged. No extra strings are retained.
3. **Borrow tracked actors while resetting/collecting captures.** Reset walks
   the actor slice directly. Indexed collection consumes a borrowed iterator,
   avoiding the temporary vector of cloned Lua table handles. The public
   indexed-table wrapper remains available. Traversal, duplicate indices,
   invalid-index skipping, flushing, output order, and partial errors are
   preserved. Resets still create fresh Lua tables so aliases to earlier
   captures remain valid.

These paths are used by actor alignment setters, player options, and capture
replay during song-Lua evaluation/compilation. No dependencies or production
unsafe code were added. These measurements do not establish whole-song load
or gameplay FPS gains.

## Behavior and validation

Ten functions from the parent are frozen in test-only baseline modules. A
source audit verifies equality apart from formatting and test visibility.
Shared numeric conversion, text-alignment parsing, capture reset/flush/read
helpers, and public exports remain unchanged.

Ten new tests cover:

- Numeric bit patterns, signed zero, NaN/infinity, numeric overflow/underflow,
  quoted strings, repeated/mixed prefixes, Unicode, invalid UTF-8, long inputs,
  and 10,000 generated alignment token combinations.
- All five speed-mod keys plus unknown/empty/Unicode keys, set/clear/switch
  behavior, nonfinite values, raw writes bypassing `__newindex`, defaults,
  invalid arguments, getter errors, and setter return identity.
- Empty/sparse/dense capture outputs, reversed/repeated/missing indices,
  flushing, public indexed access, retained Lua table aliases, partial reset
  errors, and collection metamethod lookup/error order.
- Zero warm allocation churn for common alignment inputs and direct known-key
  speed writes, and zero empty-capture collection churn even with initially
  unique handles. Installed speed setters preserve the existing argument-list
  budget: one allocation/free of `2 * size_of::<Value>()` (80 bytes here).

Validation:

- Fresh parent debug suite before edits: **451 passed, 5 failed, 18 ignored**.
- New targeted tests: **10 passed**, three manual benchmarks ignored.
- New full debug and release suites: **461 passed, 5 failed, 21 ignored** each.
  Both have identical failure names and assertion text to the fresh parent
  debug run. The parent release suite was not rebuilt in this pass.
- Root `cargo check --all-targets --offline`: passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed, with 53 existing nonperformance warnings and none in the new tests.
- Frozen-source/shared-helper/public-export audit, exact +1 version/lock audit,
  and staged whitespace check: passed.

The five existing failures are:

- `compile_song_lua_extracts_actorproxy_targets`: initial visibility.
- `compile_song_lua_layers_share_init_globals_and_actor_refs`: 0 versus 123.
- `compile_song_lua_resolves_local_and_hidden_screen_proxy_targets`: initial visibility.
- `compile_song_lua_runs_cmd_queuecommand_builders`: initial visibility.
- `compile_song_lua_supports_notefield_column_api`: `-96:-135` versus `-96:-125`.

Those failures keep the full-suite commands from returning success.

## Benchmark method

Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz, Rust 1.98.0
(`88d9e12ae`, LLVM 22.1.8), release optimization level 3 with full LTO.
The existing helper takes seven timing samples after three warmups, then
counts allocation churn in a separate operation. Three isolated serial
executable runs alternate old/new order: old first, new first, old first.
No Cargo compiler runs during measurement. Tables aggregate the median of the
three per-run medians. All 27 paired scenarios, including controls, are shown.
Allocation/reallocation/free calls and byte counts agree across all three runs.

Windows `QueryThreadCycleTime` measures calling-thread CPU cycles, not retired
instructions. Thread-local counters cover allocations routed through Rust's
System allocator, including Lua allocations in this mlua 0.12.1 / vendored
Lua 5.4 build. Other threads and allocations bypassing that allocator are outside
these counters. Requested/freed bytes are cumulative, not peak live memory or
RSS. Lua garbage collection is stopped in these isolated fixtures; reset's
fresh capture tables are reclaimed after measurement, so its counted allocation
and free totals do not balance. Setup and Lua teardown are outside timing.

Operation boundaries and throughput units:

- Alignment: 16 repetitions of four inputs (64 value triplets/op), except the
  long-string case with one input (16 triplets/op). Each triplet invokes all
  three conversions and consumes their results. Warm cases reuse handles; cold
  cases create/drop fresh Lua string handles inside timing, including their
  cost. Initial shared-counter promotion can still allocate in mlua. Cold long
  strings also include Lua string storage. The unchanged text-alignment parser
  can still allocate for long text; native numeric/nil values are a control.
  Each timing sample contains 128 operations, or 512 for the native control.
- Speed: 64 writes or getter calls/op, 128 operations/sample. Direct cases
  include both Lua field writes. Method cases include actual Lua callback
  dispatch, argument ownership, and return destruction. Methods/keys are
  installed outside timing. Getters and the unknown direct-key fallback are
  controls. Throughput is writes or reads/s.
- Capture: one reset or collection over the named actor count/op, 64
  operations/sample. Collection indices are reversed with an extra invalid
  index. Density 0 is empty, 1 populates every 32nd actor, and 2 populates every
  actor. Timed work includes fresh reset tables and construction/destruction of
  owned collection output. Nonempty output still allocates. Throughput counts
  valid actors visited/s, or calls/s for zero actors. Handles are warmed; the
  separate no-churn regression test covers initially unique handles.

Old and new outputs/state are compared before timing. Frozen baselines compile
in the same binary with the same dependency versions. Black-boxed inputs and
consumed results limit constant folding; this remains a microbenchmark, with
process-order and machine noise, especially for tiny controls.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| align_names_cold_false | 65.4953 | 12.5297 | 143,587.3 | 27,485.5 | 80.86% |
| align_names_cold_true | 72.3586 | 19.9680 | 158,624.8 | 43,817.7 | 72.38% |
| align_numbers_cold_false | 18.8156 | 10.0289 | 41,266.0 | 22,001.4 | 46.68% |
| align_numbers_cold_true | 25.3867 | 17.1938 | 55,701.6 | 37,726.6 | 32.27% |
| align_invalid_cold_false | 55.3227 | 10.8156 | 121,350.9 | 23,721.4 | 80.45% |
| align_invalid_cold_true | 63.2203 | 17.7578 | 138,585.1 | 38,939.0 | 71.90% |
| align_long_cold_false | 63.7883 | 28.7703 | 139,922.7 | 63,108.0 | 54.90% |
| align_long_cold_true | 74.4008 | 39.9461 | 162,987.3 | 87,611.4 | 46.25% |
| align_native | 1.8559 | 0.7568 | 4,069.8 | 1,663.8 | 59.12% |
| tracked_reset_0 | 0.0062 | 0.0031 | 30.9 | 24.0 | 22.33% |
| tracked_reset_2 | 1.9266 | 1.9109 | 4,249.4 | 4,215.1 | 0.81% |
| tracked_reset_16 | 15.8656 | 15.2781 | 34,855.9 | 33,559.5 | 3.72% |
| tracked_reset_64 | 60.8328 | 59.1781 | 133,377.1 | 129,854.8 | 2.64% |
| tracked_collect_0_density_0 | 0.0219 | 0.0156 | 65.2 | 51.4 | 21.17% |
| tracked_collect_2_density_0 | 0.6781 | 0.4781 | 1,505.6 | 1,070.1 | 28.93% |
| tracked_collect_16_density_0 | 4.1938 | 3.6250 | 9,222.4 | 7,974.0 | 13.54% |
| tracked_collect_64_density_0 | 16.1531 | 14.6312 | 35,377.2 | 32,112.2 | 9.23% |
| tracked_collect_64_density_1 | 27.7500 | 26.2266 | 60,880.4 | 57,522.7 | 5.52% |
| tracked_collect_16_density_2 | 93.6328 | 92.4109 | 205,356.0 | 202,639.6 | 1.32% |
| speed_write_common | 16.4227 | 11.2109 | 36,022.0 | 24,585.7 | 31.75% |
| speed_write_camod | 16.2945 | 10.6812 | 35,751.1 | 23,428.2 | 34.47% |
| speed_write_clear | 16.6016 | 10.7422 | 36,438.7 | 23,565.4 | 35.33% |
| speed_write_unknown | 16.0586 | 17.1078 | 35,221.2 | 37,537.9 | -6.58% |
| speed_method_XMod_read_false | 36.6016 | 30.1211 | 80,297.6 | 66,078.1 | 17.71% |
| speed_method_XMod_read_true | 29.3414 | 29.3719 | 64,349.5 | 64,407.8 | -0.09% |
| speed_method_UnusualMethod_read_false | 52.7406 | 47.7875 | 115,690.2 | 104,792.4 | 9.42% |
| speed_method_UnusualMethod_read_true | 30.2688 | 31.0281 | 66,403.9 | 67,909.5 | -2.27% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| align_names_cold_false | 448/0/448 | 0/0/0 | 4,368/4,368 | 0/0 | 977,169.2 | 5,107,868.8 |
| align_names_cold_true | 512/0/512 | 64/0/64 | 5,392/5,392 | 1,024/1,024 | 884,483.7 | 3,205,133.2 |
| align_numbers_cold_false | 64/0/64 | 0/0/0 | 512/512 | 0/0 | 3,401,428.3 | 6,381,553.3 |
| align_numbers_cold_true | 128/0/128 | 64/0/64 | 1,536/1,536 | 1,024/1,024 | 2,521,003.2 | 3,722,282.8 |
| align_invalid_cold_false | 336/0/336 | 0/0/0 | 1,728/1,728 | 0/0 | 1,156,849.7 | 5,917,364.9 |
| align_invalid_cold_true | 400/0/400 | 64/0/64 | 2,752/2,752 | 1,024/1,024 | 1,012,332.9 | 3,604,047.5 |
| align_long_cold_false | 128/0/128 | 16/0/16 | 106,112/106,112 | 17,664/17,664 | 250,829.8 | 556,128.8 |
| align_long_cold_true | 160/0/144 | 48/0/32 | 124,432/106,368 | 35,984/17,920 | 215,051.5 | 400,539.8 |
| align_native | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 34,485,371.5 | 84,562,580.6 |
| tracked_reset_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 160,000,000.0 | 320,000,000.0 |
| tracked_reset_2 | 3/0/1 | 2/0/0 | 128/16 | 112/0 | 1,038,118.4 | 1,046,606.7 |
| tracked_reset_16 | 17/0/1 | 16/0/0 | 1,024/128 | 896/0 | 1,008,469.6 | 1,047,248.9 |
| tracked_reset_64 | 65/0/1 | 64/0/0 | 4,096/512 | 3,584/0 | 1,052,063.8 | 1,081,480.7 |
| tracked_collect_0_density_0 | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 45,714,285.7 | 64,000,000.0 |
| tracked_collect_2_density_0 | 1/0/1 | 0/0/0 | 128/128 | 0/0 | 2,949,308.8 | 4,183,006.5 |
| tracked_collect_16_density_0 | 1/2/1 | 0/0/0 | 896/896 | 0/0 | 3,815,201.2 | 4,413,793.1 |
| tracked_collect_64_density_0 | 1/4/1 | 0/0/0 | 3,968/3,968 | 0/0 | 3,962,081.6 | 4,374,199.1 |
| tracked_collect_64_density_1 | 6/4/6 | 5/0/5 | 9,676/9,676 | 5,708/5,708 | 2,306,306.3 | 2,440,274.1 |
| tracked_collect_16_density_2 | 34/4/34 | 33/2/33 | 46,432/46,432 | 45,536/45,536 | 170,880.3 | 173,139.7 |
| speed_write_common | 64/0/64 | 0/0/0 | 2,432/2,432 | 0/0 | 3,897,055.3 | 5,708,710.8 |
| speed_write_camod | 64/0/64 | 0/0/0 | 2,432/2,432 | 0/0 | 3,927,698.1 | 5,991,808.1 |
| speed_write_clear | 64/0/64 | 0/0/0 | 2,432/2,432 | 0/0 | 3,855,058.8 | 5,957,818.2 |
| speed_write_unknown | 64/0/64 | 64/0/64 | 2,432/2,432 | 2,432/2,432 | 3,985,405.0 | 3,740,980.9 |
| speed_method_XMod_read_false | 128/0/128 | 64/0/64 | 7,552/7,552 | 5,120/5,120 | 1,748,559.2 | 2,124,756.8 |
| speed_method_XMod_read_true | 128/0/128 | 128/0/128 | 2,816/2,816 | 2,816/2,816 | 2,181,217.9 | 2,178,955.2 |
| speed_method_UnusualMethod_read_false | 128/0/128 | 64/0/64 | 7,552/7,552 | 5,120/5,120 | 1,213,485.8 | 1,339,262.4 |
| speed_method_UnusualMethod_read_true | 128/0/128 | 128/0/128 | 3,392/3,392 | 3,392/3,392 | 2,114,391.9 | 2,062,644.8 |

Per-run paired cycle savings for representative cases:

- `align_names_cold_false`: 83.34%, 80.52%, 81.49%.
- `speed_write_common`: 32.12%, 31.61%, 33.72%.
- `tracked_collect_64_density_0`: 9.67%, 0.22%, 9.23%.

Cases with higher median cycle counts:

- `speed_write_unknown`: 6.58% more cycles; elapsed 16058.6 -> 17107.8 ns/op.
- `speed_method_XMod_read_true`: 0.09% more cycles; elapsed 29341.4 -> 29371.9 ns/op.
- `speed_method_UnusualMethod_read_true`: 2.27% more cycles; elapsed 30268.8 -> 31028.1 ns/op.


## Interpretation and limits

Warm named alignment eliminates 448 allocations/frees and 4,368 requested
bytes per 64 value triplets. It uses 80.86% fewer cycles and processes 5.11
million triplets/s versus 0.98 million. Cold named inputs still use 72.38% fewer
cycles and remove 448 allocations per operation; 64 shared-counter allocations
remain. Long text retains the unchanged text-parser allocation, but the warm
case removes 112 allocations and 88,448 requested bytes per operation.

Known direct speed writes eliminate 64 allocations/frees and 2,432 requested
bytes per 64 writes. Cycle savings are 31.75-35.33%. Actual XMod setter calls
use 17.71% fewer cycles and halve allocation/free calls from 128 to 64, while
requested bytes fall from 7,552 to 5,120. The remaining 80 bytes per call belong
to the existing two-argument callback list. Custom installed setters also
reuse their captured key and show 9.42% fewer cycles.

Empty capture collection eliminates its temporary vector entirely. At 64
actors this removes one allocation, four reallocations, one free, and 3,968
cumulative requested/freed bytes. The aggregate uses 9.23% fewer cycles, with
per-run savings of 9.67%, 0.22%, and 9.23%; the smaller second-run benefit is
retained in the report. The two-actor case improves by 28.93%. Sparse 64-actor
collection removes the same vector churn and shows 5.52% fewer cycles. Dense
16-actor output retains most of its owned-output cost, with only 1.32% fewer
cycles. Reset saves one allocation/free and `8 * actor_count` bytes here;
its cycle gains are small and variable (the 64-actor reset was 3.83% slower in
one run). Fresh Lua table creation remains the dominant reset cost.

The unknown direct-key fallback keeps its existing allocation/formatting
behavior and has a measured 6.58% cycle penalty in the aggregate (1.71-6.68%
per run). The static-key dispatch adds work before that fallback. The sole
production caller of this private helper passes one of the five keys returned
by `parse_player_speed_option`; installed methods use their captured field key
directly. This fallback cost must be reconsidered if future callers pass
arbitrary keys. The unchanged XMod/custom getters have 0.09%/2.27% higher median
cycles with identical churn, so no getter speedup is claimed.

Tiny zero-actor controls are near timing overhead and are not meaningful
application speedups. Allocation counts are deterministic in these fixtures;
cycle and throughput results are machine/workload dependent. A whole-song
profile with normal Lua garbage collection is needed to quantify end-to-end
time and peak memory. The changes remove transient ownership without adding
retained caches or changing public Lua table identity.


## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib lua_access --locked
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo check --all-targets --offline
cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf
cargo test -p deadsync-song-lua --lib --release --locked lua_access_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked lua_access_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked lua_access_bench -- --ignored --test-threads=1 --nocapture
```

The recorded benchmark runs invoked the built release executable directly after
all compiler work ended. Local raw logs and machine-readable results are under
`target/lua-access-perf/`; frozen baselines, fixtures, and these measurements are
committed so the comparison can be rerun without those ignored logs.
