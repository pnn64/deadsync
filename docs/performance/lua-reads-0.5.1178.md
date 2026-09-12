# Lua restoration, color reads, and sprite keys - 0.5.1178

Parent: `e7501fe239507bde3eaa96cd30c4affaacad10db` (`0.5.1177`).
This pass follows `rust-performance.md` guidance on measuring hot paths
(M-HOTPATH), avoiding temporary ownership and repeated string formatting
(M-MEM-REUSE), and retaining useful collection capacity.

## Three changes

1. **Clear restored function-action tables directly.** Restoration calls mlua's
   raw `Table::clear` before replaying the saved entries. This removes the Rust
   vector of keys and the Lua-to-Rust key/value traversal used only to clear the
   table. The installed Lua 5.4 implementation deletes entries directly while
   traversing them. It retains capacity and does not invoke metamethods, matching
   the previous raw writes. Table identity, metatables, entry restoration order,
   and shallow snapshot ownership remain unchanged.
2. **Borrow color inputs.** Method and function color readers share a borrowed
   iterator. They inspect table/string inputs directly and only convert numeric
   components when needed. Valid unique table handles no longer acquire a
   shared heap counter just to read their components. Method receiver detection,
   table/string precedence, white fallback, alpha defaults, numeric conversion,
   and raw table access remain unchanged. Public signatures are preserved.
3. **Keep sprite-state field keys on the stack.** Sprite scans format each
   `FrameNNNN` key into a 16-byte buffer and reuse its numeric suffix for the
   `DelayNNNN` lookup. This removes two temporary formatted strings per state,
   plus the terminal frame key. Four digits remain a minimum width; larger
   indices are not truncated. The original signed i32 index range is preserved,
   and the buffer also fits its signed endpoints. Output vectors still allocate.

No dependency, cache, public export, or production unsafe code was added. These
paths serve song-Lua capture/restoration, color setters/helpers, and sprite
animation loading. The benchmarks do not establish gameplay FPS or whole-song
load-time gains.

## Behavior and validation

Four parent functions are frozen in test-only modules. A source audit verifies
those bodies, checks that only the intended functions/helpers changed, and
confirms shared numeric conversion, color parsing, snapshot construction,
public color-value conversion, and public exports remain unchanged.

Ten new tests cover:

- Empty, dense, sparse and multiple restored tables; numeric, string, boolean,
  table and function keys; invalid-UTF-8 string bytes; nonfinite values; aliases;
  metatables including weak-table metadata; raw access without metamethods;
  repeated snapshots of the same table; and function environment/global rollback.
- Method/function color conventions, omitted/default alpha, raw table access,
  malformed tables, numeric strings, invalid UTF-8, nonfinite numeric bit
  patterns, and 5,000 generated argument lists. Valid table and scalar reads,
  plus rejection of table-valued numeric components, have no allocation churn
  even when the table handles have not previously been cloned. Malformed color
  tables still incur conversion-error allocation costs.
- Sprite field formatting through 20,004, wider decimal boundaries, and both
  signed i32 endpoints; full scans beyond 9,999; sequence holes; default delays;
  numeric conversion edge cases; invalid values; and matching metamethod lookup
  traces and error order. Formatting and warmed empty scans have zero churn.

Validation:

- Fresh parent debug suite before edits: **471 passed, 5 failed, 24 ignored**.
- New targeted tests: **10 passed**, three manual benchmarks ignored.
- New full debug and release suites: **481 passed, 5 failed, 27 ignored** each.
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
The existing helper takes seven timing samples after three warmups, then counts
allocation churn in a separate operation. Three isolated serial executable runs
alternate old/new order: old first, new first, old first. The runs start after
compiler work ends. Tables aggregate the median of the three per-run medians.
All 33 paired cases are reported. Allocation/reallocation/free counts and bytes
agree across all three runs.

Windows `QueryThreadCycleTime` counts calling-thread CPU cycles, not retired
instructions. Thread-local counters cover allocations routed through Rust's
System allocator, including Lua allocations in this mlua 0.12.1 / vendored Lua
5.4 build. Other threads and allocations bypassing that allocator are outside
the counters. Requested/freed bytes are cumulative, not peak live memory or RSS.
Lua garbage collection is stopped for these isolated measurements; newly
created Lua tables remain owned by the VM after their Rust handles drop, so
their reclamation is outside timing. Lua setup and final teardown are excluded.

Operation boundaries and throughput units:

- **Restoration:** one full snapshot/mutation/restore cycle per operation, 64
  operations/sample. Fixtures have 0, 8, 64 or 512 original entries, using either
  integer or string keys. Work includes the unchanged snapshot-vector creation,
  a temporary field write, the old/new restoration and all Rust input drops.
  Thus remaining input ownership is included, rather than reporting only a raw
  clear microbenchmark. Throughput counts original entries/s, or round trips/s for
  the zero-entry case. Lua table capacity and lookup keys are warmed.
- **Colors:** method and function calls are measured for RGB, RGBA, table,
  malformed table, text, invalid UTF-8 and invalid numeric-component inputs.
  Warm cases reuse argument lists and their handles: 64 reads/op and 128
  operations/sample. Cold table cases construct fresh color tables and complete
  argument lists inside timing: 16 reads/op and 64 operations/sample. This
  includes their input allocation costs, not just the borrowed read. Cold
  tables are not cloned before use. Results are consumed as numeric bit arrays.
  Throughput is color reads/s, including rejected reads.
- **Sprites:** one complete scan/op and 64 operations/sample over 0, 1, 8, 64,
  256 or 1,024 states. Every third explicit delay is omitted to exercise the
  default. A separate 16-state case resolves fields through a Lua `__index`
  function. Work includes both lookups, key construction, output vector growth
  and destruction. Lua keys are warmed. Throughput is states/s, or scans/s for
  the empty case. The terminal missing-frame lookup is included.

Old/new output and state are compared before timing. Baselines compile in the
same binary with the same dependencies; black-boxed inputs and consumed results
limit constant folding. These are microbenchmarks with machine and ordering
noise, especially for tiny or already allocation-free paths.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| restore_0_strings_false | 0.8828 | 0.3125 | 1,961.8 | 709.9 | 63.81% |
| restore_0_strings_true | 0.8844 | 0.3125 | 1,975.5 | 706.5 | 64.24% |
| restore_8_strings_false | 4.7641 | 2.2625 | 10,491.4 | 4,990.2 | 52.44% |
| restore_8_strings_true | 6.9703 | 3.5094 | 15,323.8 | 7,727.1 | 49.57% |
| restore_64_strings_false | 25.5578 | 13.4031 | 56,061.7 | 29,447.3 | 47.47% |
| restore_64_strings_true | 46.1188 | 23.0547 | 101,079.8 | 50,632.5 | 49.91% |
| restore_512_strings_false | 253.0859 | 97.3062 | 554,343.8 | 213,227.1 | 61.54% |
| restore_512_strings_true | 442.1969 | 201.2266 | 967,569.7 | 440,838.3 | 54.44% |
| color_rgb_method_false_warm | 3.8578 | 3.1656 | 8,479.9 | 6,964.0 | 17.88% |
| color_rgba_method_false_warm | 3.9938 | 3.7430 | 8,778.3 | 8,227.8 | 6.27% |
| color_table_method_false_warm | 12.8578 | 12.1156 | 28,173.2 | 26,593.8 | 5.61% |
| color_table_method_false_cold | 14.4578 | 13.9781 | 31,765.8 | 30,894.6 | 2.74% |
| color_invalid_table_method_false_warm | 17.6195 | 14.9836 | 38,609.7 | 32,871.8 | 14.86% |
| color_invalid_table_method_false_cold | 12.5062 | 11.7172 | 27,382.6 | 25,753.5 | 5.95% |
| color_text_method_false_warm | 6.5906 | 5.6188 | 14,478.4 | 12,345.2 | 14.73% |
| color_invalid_text_method_false_warm | 44.0836 | 20.8961 | 96,634.9 | 45,818.9 | 52.59% |
| color_invalid_component_method_false_warm | 2.8547 | 1.7656 | 6,247.2 | 3,885.8 | 37.80% |
| color_rgb_method_true_warm | 4.3711 | 3.0977 | 9,604.8 | 6,811.4 | 29.08% |
| color_rgba_method_true_warm | 4.8531 | 3.5562 | 10,652.6 | 7,818.0 | 26.61% |
| color_table_method_true_warm | 11.2492 | 11.6570 | 24,326.8 | 25,558.0 | -5.06% |
| color_table_method_true_cold | 14.4391 | 15.3578 | 31,734.9 | 33,628.1 | -5.97% |
| color_invalid_table_method_true_warm | 15.6727 | 13.1141 | 34,382.6 | 28,761.4 | 16.35% |
| color_invalid_table_method_true_cold | 12.9469 | 11.3578 | 28,353.2 | 24,951.0 | 12.00% |
| color_text_method_true_warm | 5.7539 | 4.8836 | 12,641.8 | 10,728.1 | 15.14% |
| color_invalid_text_method_true_warm | 44.3930 | 21.9328 | 95,009.2 | 48,121.9 | 49.35% |
| color_invalid_component_method_true_warm | 3.6383 | 1.6320 | 7,848.8 | 3,568.6 | 54.53% |
| sprite_0 | 0.2234 | 0.1703 | 514.5 | 394.4 | 23.34% |
| sprite_1 | 0.6406 | 0.4484 | 1,426.8 | 1,008.3 | 29.33% |
| sprite_8 | 3.5828 | 2.6469 | 7,884.9 | 5,833.9 | 26.01% |
| sprite_64 | 28.9891 | 17.5438 | 63,586.4 | 38,470.8 | 39.50% |
| sprite_256 | 118.8500 | 70.4125 | 260,385.3 | 154,456.0 | 40.68% |
| sprite_1024 | 508.5750 | 293.0188 | 1,114,703.3 | 641,492.2 | 42.45% |
| sprite_virtual_16 | 23.0891 | 18.8141 | 50,642.8 | 41,262.6 | 18.52% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| restore_0_strings_false | 3/0/3 | 1/0/1 | 224/224 | 48/48 | 1,132,743.4 | 3,200,000.0 |
| restore_0_strings_true | 3/0/3 | 1/0/1 | 224/224 | 48/48 | 1,130,742.0 | 3,200,000.0 |
| restore_8_strings_false | 4/3/4 | 2/1/2 | 2,144/2,144 | 1,008/1,008 | 1,679,239.1 | 3,535,911.6 |
| restore_8_strings_true | 20/3/20 | 10/1/10 | 2,400/2,400 | 1,136/1,136 | 1,147,724.7 | 2,279,608.2 |
| restore_64_strings_false | 4/9/4 | 2/4/2 | 20,064/20,064 | 9,968/9,968 | 2,504,126.7 | 4,775,005.8 |
| restore_64_strings_true | 132/9/132 | 66/4/66 | 22,112/22,112 | 10,992/10,992 | 1,387,721.9 | 2,776,008.1 |
| restore_512_strings_false | 4/15/4 | 2/7/2 | 163,424/163,424 | 81,648/81,648 | 2,023,028.2 | 5,261,738.1 |
| restore_512_strings_true | 1028/15/1028 | 514/7/514 | 179,808/179,808 | 89,840/89,840 | 1,157,855.3 | 2,544,395.7 |
| color_rgb_method_false_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 16,589,712.4 | 20,217,176.7 |
| color_rgba_method_false_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 16,025,039.1 | 17,098,726.8 |
| color_table_method_false_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 4,977,518.5 | 5,282,434.9 |
| color_table_method_false_cold | 80/0/48 | 64/0/32 | 5,376/3,456 | 5,120/3,200 | 1,106,668.1 | 1,144,645.7 |
| color_invalid_table_method_false_warm | 128/0/128 | 128/0/128 | 3,072/3,072 | 3,072/3,072 | 3,632,332.7 | 4,271,338.4 |
| color_invalid_table_method_false_cold | 112/0/80 | 96/0/64 | 5,376/4,224 | 5,120/3,968 | 1,279,360.3 | 1,365,515.4 |
| color_text_method_false_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 9,710,763.4 | 11,390,433.8 |
| color_invalid_text_method_false_warm | 256/128/256 | 128/64/128 | 10,496/10,496 | 5,248/5,248 | 1,451,787.3 | 3,062,773.4 |
| color_invalid_component_method_false_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 22,419,266.6 | 36,247,787.6 |
| color_rgb_method_true_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 14,641,644.3 | 20,660,781.8 |
| color_rgba_method_true_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 13,187,379.3 | 17,996,485.1 |
| color_table_method_true_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 5,689,284.0 | 5,490,248.6 |
| color_table_method_true_cold | 80/16/48 | 64/16/32 | 6,016/4,096 | 5,760/3,840 | 1,108,105.2 | 1,041,815.0 |
| color_invalid_table_method_true_warm | 128/0/128 | 128/0/128 | 3,072/3,072 | 3,072/3,072 | 4,083,545.2 | 4,880,257.4 |
| color_invalid_table_method_true_cold | 112/16/80 | 96/16/64 | 6,016/4,864 | 5,760/4,608 | 1,235,819.5 | 1,408,722.0 |
| color_text_method_true_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 11,122,878.5 | 13,105,103.2 |
| color_invalid_text_method_true_warm | 256/128/256 | 128/64/128 | 10,496/10,496 | 5,248/5,248 | 1,441,669.7 | 2,918,002.4 |
| color_invalid_component_method_true_warm | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 17,590,723.6 | 39,214,935.4 |
| sprite_0 | 1/0/1 | 0/0/0 | 10/10 | 0/0 | 4,475,524.5 | 5,871,559.6 |
| sprite_1 | 4/0/4 | 1/0/1 | 62/62 | 32/32 | 1,560,975.6 | 2,229,965.2 |
| sprite_8 | 18/1/18 | 1/1/1 | 266/266 | 96/96 | 2,232,882.7 | 3,022,432.1 |
| sprite_64 | 130/4/130 | 1/4/1 | 2,282/2,282 | 992/992 | 2,207,729.2 | 3,648,022.8 |
| sprite_256 | 514/6/514 | 1/6/1 | 9,194/9,194 | 4,064/4,064 | 2,153,975.6 | 3,635,718.1 |
| sprite_1024 | 2050/8/2050 | 1/8/1 | 36,842/36,842 | 16,352/16,352 | 2,013,469.0 | 3,494,656.9 |
| sprite_virtual_16 | 34/2/34 | 1/2/1 | 554/554 | 224/224 | 692,968.8 | 850,427.7 |

Per-run paired cycle savings for representative cases:

- `restore_512_strings_true`: 56.44%, 44.51%, 54.44%.
- `color_table_method_true_cold`: 12.60%, -23.44%, -11.51%.
- `sprite_64`: 33.58%, 30.60%, 44.63%.

Cases with higher median cycle counts:

- `color_table_method_true_warm`: 5.06% more cycles; elapsed 11249.2 -> 11657.0 ns/op.
- `color_table_method_true_cold`: 5.97% more cycles; elapsed 14439.1 -> 15357.8 ns/op.


## Interpretation and limits

**Restoration:** full snapshot/mutation/restore cycles use 47.47-64.24% fewer
cycles across these fixtures. For 512 string-keyed entries, cycles fall by
54.44%, throughput rises from 1.16 to 2.54 million original entries/s, allocation
and free calls fall from 1,028 to 514, reallocations from 15 to 7, and requested
bytes from 179,808 to 89,840. The unchanged input snapshot and its ownership
still allocate. The raw clear avoids the second Lua-to-Rust traversal, temporary
key vector, and transient key handles; it does not replace tables or add caches.

**Colors:** warmed RGB/RGBA method reads use 29.08%/26.61% fewer cycles in the
initial three runs. RGBA throughput rises from 13.19 to 18.00 million reads/s,
with zero allocation churn both before and after. Cold table-input cases remove
one allocation/free and 16 bytes per read; the measured 16-read batches remove
16 allocation/free calls and 256 requested/freed bytes, including input setup.
Method-style valid table timing is inconclusive, as the follow-up below shows.
No table-method speedup is claimed. Invalid-UTF-8 inputs avoid a redundant
conversion attempt, halving measured error-path churn. Malformed table
conversion errors still allocate; they are reported rather than excluded.

**Sprites:** a 64-state scan uses 39.50% fewer cycles and processes 3.65 million
states/s versus 2.21 million. It removes 129 allocations/frees and 1,290 requested
bytes, leaving one allocation and four reallocations for the output vector.
The 1,024-state case removes 2,049 allocations/frees and 20,490 requested bytes,
with 42.45% fewer cycles. A warmed empty scan has zero churn, and the virtual
16-state scan through `__index` uses 18.52% fewer cycles. The fixed buffer uses
16 stack bytes plus formatter bookkeeping; no retained key cache is introduced.

These are scoped microbenchmarks. Small timing differences vary by run and
machine, and the complete application still owns Lua/output data. No whole-song
profile with normal garbage collection, hardware instruction count, peak-live
memory or RSS measurement was performed. Those measurements are needed to
quantify application-wide gains.

## Focused color follow-up

The initial method-style table cases showed a small slowdown amid wide sample
ranges. Six additional runs used the same unchanged executable and fixtures,
alternating old/new order equally, after waiting for compiler/linker activity
to finish. All 18 color cases are shown. Their allocation counters and byte
counts match the first three runs exactly. These are additional observations;
the original results above are retained.

| Scenario | Fewer aggregate cycles | Paired run range | Faster runs |
|---|---:|---:|---:|
| color_rgb_method_false_warm | 8.77% | 2.17% to 17.48% | 6/6 |
| color_rgba_method_false_warm | 2.54% | -20.29% to 13.80% | 5/6 |
| color_table_method_false_warm | 9.41% | -3.73% to 19.54% | 5/6 |
| color_table_method_false_cold | 11.85% | -6.03% to 26.23% | 5/6 |
| color_invalid_table_method_false_warm | 8.62% | -0.29% to 21.52% | 5/6 |
| color_invalid_table_method_false_cold | 6.14% | -8.46% to 19.23% | 4/6 |
| color_text_method_false_warm | 6.62% | -7.17% to 22.27% | 5/6 |
| color_invalid_text_method_false_warm | 51.37% | 48.19% to 53.27% | 6/6 |
| color_invalid_component_method_false_warm | 43.54% | 31.22% to 57.48% | 6/6 |
| color_rgb_method_true_warm | 19.69% | 2.94% to 31.98% | 6/6 |
| color_rgba_method_true_warm | 25.42% | 15.83% to 31.65% | 6/6 |
| color_table_method_true_warm | 3.69% | -8.42% to 18.63% | 3/6 |
| color_table_method_true_cold | 2.72% | -8.61% to 22.12% | 4/6 |
| color_invalid_table_method_true_warm | 16.31% | -2.61% to 22.90% | 5/6 |
| color_invalid_table_method_true_cold | 8.51% | 0.56% to 15.46% | 6/6 |
| color_text_method_true_warm | 9.96% | -3.68% to 25.04% | 4/6 |
| color_invalid_text_method_true_warm | 53.37% | 48.92% to 56.77% | 6/6 |
| color_invalid_component_method_true_warm | 52.07% | 48.29% to 58.34% | 6/6 |

Warm/cold method-style table reads have 3.69%/2.72% fewer aggregate cycles in
this follow-up, with both positive and negative individual results. The first
three-run aggregate was 5.06%/5.97% slower. A table-method timing improvement or
regression is not established here. The one fewer allocation/free per fresh
color-table input is deterministic. RGBA methods improve in all six follow-up
runs, with 25.42% fewer aggregate cycles versus 26.61% in the initial runs.


## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib lua_read_ --locked
cargo test -p deadsync-song-lua --lib --locked
cargo test -p deadsync-song-lua --lib --release --locked
cargo check --all-targets --offline
cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf
cargo test -p deadsync-song-lua --lib --release --locked lua_read_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked lua_read_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked lua_read_bench -- --ignored --test-threads=1 --nocapture
```

The recorded runs invoke the built release executable directly. Raw logs and
machine-readable results are local to `target/lua-read-perf/`; baselines,
fixtures and this report are committed so the comparison can be reproduced
without those ignored logs.
