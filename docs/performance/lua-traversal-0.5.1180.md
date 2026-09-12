# Lua table traversal - 0.5.1180

Parent: `4b877a764bb48b98856243b3455d4c221deab37e` (`0.5.1179`).
This pass follows `rust-performance.md` guidance to measure hot paths
(M-HOTPATH) and remove short-lived allocations and repeated ownership work
(M-MEM-REUSE). It uses the existing mlua 0.12.1 safe callback traversal API.

## Three changes

1. **Recursive Lua value cloning.** `clone_lua_value` collects raw table entries
   with `Table::for_each`. The traversal cursor stays on Lua's stack instead of
   cloning every reference-valued key into a Rust iterator cursor. The change
   applies at every copied table depth, including the snapshot paths that call
   this shared copier. Keys and values still copy recursively, key copying still
   precedes value copying, non-table values retain their identity, and source
   metatables are not copied or invoked. Shared input subtables still produce
   separate copies for separate occurrences.
2. **Actor-child table reads.** `actor_named_children` uses raw callback traversal
   to copy the named child map before its existing sequence merge.
   `actor_direct_children` uses it for the named-map portion of enumeration.
   This removes iterator key clones, including keys unused by direct enumeration.
   Array children still come first; group flattening, first-seen order, identity
   deduplication, missing child-map creation, and shared-group append semantics
   remain unchanged. The existing small/large deduplication storage is unchanged.
3. **Global actor-reference discovery.** The outer globals scan and nested table
   scans use the same raw callback API. They no longer clone cursor keys or the
   globals handle just for traversal. This retains the exact two-level search,
   registry membership checks, identity deduplication, and exclusions for the
   globals table, registry aliases, and interiors of directly referenced actors.
   Keys themselves still do not count as actor references.

Both mlua traversal APIs use raw Lua `next` without `__pairs`. The installed
`TablePairs::next` clones the key for its Rust cursor; `Table::for_each` holds
the current key on the Lua stack. The recursive copier therefore keeps Lua
stack entries across nested copies. Stack capacity may grow with nesting depth;
the warmed measurements below do not measure its cold peak memory cost.

No dependency, cache, public API, or production unsafe code changed. These
paths serve compilation snapshots, actor hierarchy queries, and overlay actor
discovery. The benchmarks do not establish whole-song load-time or gameplay FPS
gains.

## Behavior and validation

Four functions are frozen from the parent in test-only modules. The recursive
baseline calls itself, so it does not accidentally use the optimized copier.
A source audit verifies those bodies and reconstructs both production source
files from the four intended changes. Other helpers and public exports remain
unchanged. The workspace patch increases exactly once to 0.5.1180, with all three
workspace-versioned lockfile entries updated and no dependency changes.

Ten new regression tests cover:

- Deep and broad acyclic copies, numeric/string/boolean/table/function keys,
  invalid UTF-8 string bytes, NaN/infinity/signed zero, function/thread identity,
  source metatables and metamethod traps, independent copied values and keys,
  repeated shared subtables, and nesting through 97 tables. Scalar-only copying
  has no allocator churn.
- Named and array children, duplicate identities, empty/ignored entries, groups,
  mixed key types, binary string keys, both sides of the 32-child deduplication
  threshold, shared-group mutation, missing/invalid child maps, group lookup
  errors, and matching traversal order while a metatable callback replaces an
  existing named value. Scanning 128 non-child fields has zero churn.
- Direct/nested references, repeated globals and registry entries, keys that are
  actor tables, third-level exclusions, registry/globals self-aliases, directly
  referenced actors containing other actors, ignored metamethods, registry holes,
  creation errors, malformed registries, and dense/sparse membership. Warmed
  scans without registered actors have zero churn even with nested globals.

Validation:

- Fresh parent debug suite before edits: **491 passed, 5 failed, 30 ignored**.
- New targeted tests: **10 passed**, three manual benchmarks ignored.
- New full debug and release suites: **501 passed, 5 failed, 33 ignored** each.
  Failure names and assertion text match the fresh parent debug run. The parent
  release suite was not rebuilt in this pass.
- Root `cargo check --all-targets --offline`: passed.
- `cargo clippy -p deadsync-song-lua --all-targets --locked -- -D clippy::perf`:
  passed, with 53 existing nonperformance warnings and none in the new tests.
- Frozen-source/shared-helper audit, exact +1 version/lock audit, and staged
  whitespace check: passed.

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
Each helper call takes seven timing samples after three warmups, then counts a
separate allocation operation. Three serial executable runs alternate old/new
order: old first, new first, old first. Each run starts after compiler/linker
work ends. Tables aggregate the median of the three per-run medians. All 36
paired cases are reported, including numeric-key copies and sequence-only child
controls that do not contain the removed string-key allocation work.
Allocation/reallocation/free counts and requested/freed bytes agree across all three runs.

Windows `QueryThreadCycleTime` measures calling-thread CPU cycles, not retired
instructions. Thread-local counters cover allocations routed through Rust's
System allocator, including Lua allocations in this mlua / vendored Lua 5.4
build. Other threads and allocations bypassing that allocator are outside the
counters. Requested/freed bytes are cumulative, not peak live memory or RSS.
Input setup and VM teardown are excluded. Rust results are consumed and dropped
inside each operation. Lua GC is collected after behavior comparisons, then
stopped for timing/counting; copied Lua tables remain until VM teardown. Thus
table allocation is counted, but later Lua GC and object reclamation are not.
Inputs, lookup keys, Lua reference storage, and stack capacity are warmed.

Operation boundaries and throughput units:

- **Recursive copy:** one full copy and Rust output drop, 32 operations/sample.
  Flat tables have 0, 8, 64 or 256 numeric or string keys. Nested fixtures include
  85 tables with branching factor four, and a 33-table chain. Units count all
  copied entries, including entries inside table keys and values, or operations
  for empty input. The common source-handle clone is included.
- **Child reads:** one complete direct enumeration or named-map construction and
  Rust result drop, 64 operations/sample. Fixtures include 0 through 512 named
  entries, mixed named/array children, two-entry groups containing duplicate
  children, ignored scalar entries, repeated identities, and sequence-only
  controls. Units count fixture child slots (or reads for empty fixtures), not
  deduplicated output size. Named reads include the unchanged sequence merge and
  any output group construction it requires.
- **Global references:** one full registry-membership collection and two-level
  scan, 128 operations/sample. Fixtures vary 0 through 1,024 registry entries,
  0 through 512 global fields, and zero/eight nested tables. Every third field
  holds an actor when references are enabled. Units count registry entries plus
  scalar/actor-valued fields across scanned tables; container-link fields are
  excluded from the throughput unit count. HashSet construction and destruction
  remain included.

Old/new outputs, identities, or table state are compared before timing. Frozen
baselines and new code compile in the same binary with the same dependencies.
Black-boxed inputs and consumed results limit constant folding. Small timing
differences remain subject to machine noise and code layout effects.

## Representative results

- **Recursive copy, 256 string-keyed fields:** 28.18% fewer cycles, 83.191 -> 59.778 us/op; A/R/F 266/0/264 -> 10/0/8, requested bytes 16,416 -> 12,320.
- **Direct child enumeration, 64 named actors:** 34.37% fewer cycles, 16.766 -> 10.983 us/op; A/R/F 66/4/66 -> 2/4/2, requested bytes 5,168 -> 4,144.
- **Global references, 64 registered actors and eight nested tables:** 44.36% fewer cycles, 137.905 -> 76.765 us/op; A/R/F 598/0/598 -> 12/0/12, requested bytes 14,104 -> 4,728.

Lua table outputs, child vectors and membership sets still allocate where required. The complete compilation/capture pipeline is not allocation-free. All scenarios and any slower cases follow.

## Results

Positive cycle savings indicate an improvement on this workload and machine.

| Scenario | Old us/op | New us/op | Old cycles/op | New cycles/op | Fewer cycles |
|---|---:|---:|---:|---:|---:|
| clone_0_0_false | 0.1594 | 0.1469 | 404.7 | 370.4 | 8.48% |
| clone_8_0_false | 2.1031 | 1.8688 | 4,650.7 | 4,136.2 | 11.06% |
| clone_64_0_false | 11.0875 | 10.1812 | 24,378.2 | 22,121.5 | 9.26% |
| clone_256_0_false | 40.6344 | 33.7156 | 89,103.3 | 74,088.1 | 16.85% |
| clone_8_0_true | 3.0594 | 2.1125 | 6,756.5 | 4,684.9 | 30.66% |
| clone_64_0_true | 20.5938 | 14.3375 | 45,244.4 | 31,354.2 | 30.70% |
| clone_256_0_true | 83.1906 | 59.7781 | 182,514.2 | 131,082.7 | 28.18% |
| clone_4_3_true | 155.6438 | 114.9406 | 341,747.8 | 252,020.3 | 26.26% |
| clone_4_3_false | 105.8625 | 99.5625 | 232,093.8 | 218,388.8 | 5.90% |
| clone_1_32_true | 18.7125 | 15.3281 | 41,128.8 | 33,631.5 | 18.23% |
| children_named_0_named_false | 0.2125 | 0.2109 | 483.6 | 480.2 | 0.70% |
| children_named_0_named_true | 0.3656 | 0.3016 | 823.1 | 685.9 | 16.67% |
| children_named_8_named_false | 2.2812 | 1.4719 | 5,027.9 | 3,254.8 | 35.27% |
| children_named_8_named_true | 3.3953 | 2.6703 | 7,473.3 | 5,881.9 | 21.29% |
| children_named_64_named_false | 16.7656 | 10.9828 | 36,762.8 | 24,127.8 | 34.37% |
| children_named_64_named_true | 23.2531 | 16.6562 | 51,064.6 | 36,522.7 | 28.48% |
| children_named_512_named_false | 133.4422 | 81.9016 | 292,065.3 | 179,592.2 | 38.51% |
| children_named_512_named_true | 173.3781 | 120.4828 | 380,204.9 | 263,941.9 | 30.58% |
| children_mixed_64_named_false | 18.8203 | 13.1484 | 41,266.0 | 28,826.5 | 30.14% |
| children_mixed_64_named_true | 85.1344 | 77.4203 | 186,640.2 | 169,793.5 | 9.03% |
| children_groups_64_named_false | 34.6656 | 29.2484 | 76,056.8 | 64,162.6 | 15.64% |
| children_groups_64_named_true | 22.1266 | 15.8016 | 48,595.2 | 34,629.6 | 28.74% |
| children_ignored_128_named_false | 22.6594 | 10.5312 | 49,703.0 | 23,119.5 | 53.48% |
| children_ignored_128_named_true | 40.5234 | 27.4172 | 88,859.8 | 60,091.6 | 32.37% |
| children_sequence_64_named_false | 5.8641 | 5.6422 | 12,895.6 | 12,405.2 | 3.80% |
| children_sequence_64_named_true | 28.9672 | 32.9328 | 63,545.2 | 72,164.1 | -13.56% |
| children_duplicates_128_named_false | 29.7891 | 20.0688 | 65,352.7 | 44,074.9 | 32.56% |
| children_duplicates_128_named_true | 42.6062 | 32.2828 | 93,404.1 | 70,805.9 | 24.19% |
| references_0_0_0_false | 0.5484 | 0.3914 | 1,212.4 | 867.7 | 28.43% |
| references_0_512_0_false | 99.0578 | 45.8812 | 217,003.2 | 100,621.9 | 53.63% |
| references_16_64_0_true | 17.9758 | 10.9555 | 39,439.7 | 24,033.5 | 39.06% |
| references_64_512_0_true | 149.0148 | 81.0148 | 326,684.6 | 177,630.4 | 45.63% |
| references_64_64_8_true | 137.9055 | 76.7648 | 302,423.0 | 168,269.0 | 44.36% |
| references_0_64_8_false | 107.6688 | 48.8492 | 236,110.0 | 107,134.9 | 54.63% |
| references_64_64_8_false | 114.4219 | 59.8883 | 250,811.3 | 131,375.9 | 47.62% |
| references_1024_64_8_true | 257.1430 | 194.5016 | 563,657.1 | 426,287.9 | 24.37% |

A/R/F means allocation/reallocation/free calls. Bytes include reallocations and common input/output ownership costs.

| Scenario | Old A/R/F | New A/R/F | Old requested/freed B | New requested/freed B | Old units/s | New units/s |
|---|---:|---:|---:|---:|---:|---:|
| clone_0_0_false | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 6,274,509.8 | 6,808,510.6 |
| clone_8_0_false | 2/3/0 | 2/3/0 | 296/112 | 296/112 | 3,803,863.3 | 4,280,936.5 |
| clone_64_0_false | 2/6/0 | 2/6/0 | 2,088/1,008 | 2,088/1,008 | 5,772,266.1 | 6,286,065.1 |
| clone_256_0_false | 2/8/0 | 2/8/0 | 8,232/4,080 | 8,232/4,080 | 6,300,084.6 | 7,592,918.7 |
| clone_8_0_true | 13/0/11 | 5/0/3 | 544/296 | 416/168 | 2,614,913.2 | 3,786,982.2 |
| clone_64_0_true | 72/0/70 | 8/0/6 | 4,128/2,536 | 3,104/1,512 | 3,107,739.0 | 4,463,818.7 |
| clone_256_0_true | 266/0/264 | 10/0/8 | 16,416/10,216 | 12,320/6,120 | 3,077,269.8 | 4,282,503.0 |
| clone_4_3_true | 680/0/510 | 340/0/170 | 24,480/11,560 | 19,040/6,120 | 2,184,475.8 | 2,958,049.0 |
| clone_4_3_false | 170/170/0 | 170/170/0 | 14,280/4,080 | 14,280/4,080 | 3,211,713.3 | 3,414,940.4 |
| clone_1_32_true | 99/0/33 | 66/0/0 | 3,168/528 | 2,640/0 | 1,763,527.1 | 2,152,905.2 |
| children_named_0_named_false | 0/0/0 | 0/0/0 | 0/0 | 0/0 | 4,705,882.4 | 4,740,740.7 |
| children_named_0_named_true | 1/0/0 | 1/0/0 | 56/0 | 56/0 | 2,735,042.7 | 3,316,062.2 |
| children_named_8_named_false | 9/1/9 | 1/1/1 | 416/416 | 288/288 | 3,506,849.3 | 5,435,244.2 |
| children_named_8_named_true | 13/0/11 | 5/0/3 | 544/296 | 416/168 | 2,356,189.6 | 2,995,904.0 |
| children_named_64_named_false | 66/4/66 | 2/4/2 | 5,168/5,168 | 4,144/4,144 | 3,817,334.6 | 5,827,287.0 |
| children_named_64_named_true | 72/0/70 | 8/0/6 | 4,128/2,536 | 3,104/1,512 | 2,752,318.2 | 3,842,401.5 |
| children_named_512_named_false | 517/7/517 | 5/7/5 | 50,016/50,016 | 41,824/41,824 | 3,836,867.6 | 6,251,407.0 |
| children_named_512_named_true | 523/0/521 | 11/0/9 | 32,800/20,456 | 24,608/12,264 | 2,953,083.0 | 4,249,568.8 |
| children_mixed_64_named_false | 66/4/66 | 2/4/2 | 5,168/5,168 | 4,144/4,144 | 3,400,581.2 | 4,867,498.5 |
| children_mixed_64_named_true | 328/32/134 | 264/32/70 | 15,899/4,067 | 14,875/3,043 | 751,752.7 | 826,656.4 |
| children_groups_64_named_false | 66/4/66 | 2/4/2 | 5,168/5,168 | 4,144/4,144 | 1,846,209.3 | 2,188,151.1 |
| children_groups_64_named_true | 72/0/70 | 8/0/6 | 4,128/2,536 | 3,104/1,512 | 2,892,451.1 | 4,050,232.4 |
| children_ignored_128_named_false | 128/0/128 | 0/0/0 | 2,048/2,048 | 0/0 | 5,648,876.0 | 12,154,302.7 |
| children_ignored_128_named_true | 137/0/135 | 9/0/7 | 8,224/5,096 | 6,176/3,048 | 3,158,665.9 | 4,668,604.3 |
| children_sequence_64_named_false | 2/4/2 | 2/4/2 | 4,144/4,144 | 4,144/4,144 | 10,913,935.5 | 11,343,118.2 |
| children_sequence_64_named_true | 72/0/70 | 72/0/70 | 3,606/2,014 | 3,606/2,014 | 2,209,396.4 | 1,943,350.6 |
| children_duplicates_128_named_false | 129/0/129 | 1/0/1 | 2,144/2,144 | 96/96 | 4,296,879.1 | 6,378,075.4 |
| children_duplicates_128_named_true | 137/0/135 | 9/0/7 | 8,224/5,096 | 6,176/3,048 | 3,004,254.1 | 3,964,958.1 |
| references_0_0_0_false | 2/0/2 | 0/0/0 | 32/32 | 0/0 | 1,823,361.8 | 2,554,890.2 |
| references_0_512_0_false | 514/0/514 | 0/0/0 | 8,224/8,224 | 0/0 | 5,168,698.8 | 11,159,242.6 |
| references_16_64_0_true | 74/0/74 | 8/0/8 | 2,264/2,264 | 1,208/1,208 | 4,450,432.4 | 7,302,289.1 |
| references_64_512_0_true | 526/0/526 | 12/0/12 | 12,952/12,952 | 4,728/4,728 | 3,865,386.7 | 7,109,808.2 |
| references_64_64_8_true | 598/0/598 | 12/0/12 | 14,104/14,104 | 4,728/4,728 | 4,640,860.2 | 8,337,149.8 |
| references_0_64_8_false | 586/0/586 | 0/0/0 | 9,376/9,376 | 0/0 | 5,349,741.7 | 11,791,386.1 |
| references_64_64_8_false | 592/0/592 | 6/0/6 | 11,740/11,740 | 2,364/2,364 | 5,593,336.1 | 10,686,564.8 |
| references_1024_64_8_true | 602/0/602 | 16/0/16 | 48,728/48,728 | 39,352/39,352 | 6,222,219.5 | 8,226,155.0 |

Per-run paired cycle savings for representative cases:

- `clone_256_0_true`: 28.64%, 28.76%, 28.18%.
- `children_named_64_named_false`: 34.37%, 33.50%, 35.65%.
- `references_64_64_8_true`: 44.51%, 43.46%, 42.94%.

Cases with higher median cycle counts:

- `children_sequence_64_named_true`: 13.56% more cycles; elapsed 28967.2 -> 32932.8 ns/op.


## Child-read timing follow-up

The initial sequence-only named-read control was 13.56% slower in the three-run aggregate, with identical allocation counts and bytes. Six additional serial runs repeat **all 18 child cases**, alternating old/new order with no source changes and no concurrent compiler/linker work. The original 36-case results above remain intact. Positive percentages mean fewer cycles. All allocation counters match the initial runs.

| Scenario | Extra-run cycle savings | Extra-run median old/new us | Extra-run median old/new cycles |
|---|---|---:|---:|
| children_named_0_named_false | 16.96%, 5.73%, 10.33%, 21.02%, 6.15%, 6.39% | 0.2250 / 0.2047 | 516.1 / 469.9 |
| children_named_0_named_true | -19.60%, 10.24%, -75.98%, 4.81%, -1.98%, 5.57% | 0.3039 / 0.3054 | 696.2 / 692.8 |
| children_named_8_named_false | 30.43%, 31.86%, 33.31%, 32.82%, 34.07%, 33.78% | 2.3415 / 1.5500 | 5161.7 / 3422.8 |
| children_named_8_named_true | 18.01%, 24.27%, 23.54%, 21.82%, 16.28%, 21.93% | 3.3445 / 2.6070 | 7344.7 / 5744.8 |
| children_named_64_named_false | 36.98%, 33.02%, 35.97%, 37.58%, 34.82%, 37.88% | 15.4672 / 9.8046 | 33892.2 / 21514.4 |
| children_named_64_named_true | 29.96%, 27.53%, 29.21%, 32.72%, 14.75%, 27.77% | 20.7429 / 14.8672 | 45496.6 / 32640.3 |
| children_named_512_named_false | 38.39%, 37.73%, 35.79%, 39.36%, 39.03%, 41.60% | 126.3047 / 77.9399 | 276806.7 / 170772.8 |
| children_named_512_named_true | 28.76%, 29.12%, 29.03%, 29.69%, 27.52%, 30.94% | 162.4797 / 114.8344 | 356294.8 / 251565.9 |
| children_mixed_64_named_false | 31.94%, 32.06%, 34.67%, 30.52%, 32.35%, 33.19% | 17.4429 / 11.7992 | 38256.4 / 25877.0 |
| children_mixed_64_named_true | 11.51%, 13.51%, 8.25%, 8.64%, 5.71%, 10.94% | 78.7844 / 70.4243 | 172681.3 / 154447.4 |
| children_groups_64_named_false | 15.01%, 19.18%, 18.91%, 16.76%, 17.35%, 18.00% | 32.1305 / 26.6157 | 70492.1 / 58385.2 |
| children_groups_64_named_true | 28.88%, 29.02%, 28.56%, 32.86%, 30.11%, 31.65% | 21.1461 / 14.6118 | 46372.8 / 32052.1 |
| children_ignored_128_named_false | 54.36%, 55.18%, 56.79%, 51.61%, 54.75%, 55.35% | 21.9922 / 9.9219 | 48240.3 / 21745.9 |
| children_ignored_128_named_true | 31.56%, 32.42%, 30.93%, 31.64%, 29.68%, 33.05% | 37.3141 / 25.5063 | 81777.5 / 55933.1 |
| children_sequence_64_named_false | 4.80%, -1.19%, -2.13%, -1.98%, -29.20%, -0.37% | 5.6226 / 5.5492 | 12374.3 / 12199.4 |
| children_sequence_64_named_true | -1.81%, -0.12%, 0.64%, -5.24%, 5.38%, -1.00% | 27.5320 / 27.6539 | 60425.9 / 60643.8 |
| children_duplicates_128_named_false | 40.98%, 44.76%, 40.56%, 39.87%, 41.91%, 43.30% | 29.2023 / 16.8445 | 64059.7 / 36918.9 |
| children_duplicates_128_named_true | 29.56%, 25.61%, 26.80%, 34.40%, 30.51%, 28.82% | 40.2906 / 28.9821 | 88391.6 / 63562.4 |

The sequence-only named-read slowdown did not repeat at its initial magnitude: the follow-up median is 0.36% more cycles, with gains and losses between runs. Its timing effect is inconclusive, and its allocation work is unchanged. Empty/sequence-only controls also show noisy isolated timing outliers. Every child case with named entries improved in all six follow-up runs; the allocation reductions reproduced exactly.


## Reproduce

```powershell
cargo test -p deadsync-song-lua --lib lua_traversal_ --locked
cargo test -p deadsync-song-lua --lib --release --locked lua_traversal_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-song-lua --lib --release --locked lua_traversal_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-song-lua --lib --release --locked lua_traversal_bench -- --ignored --test-threads=1 --nocapture
```

Run benchmarks without concurrent compiler/linker work. Machine-local raw logs
and audit scripts are under ignored `target/lua-traversal-perf/`. Frozen
baselines, tests, benchmark cases and this report are committed. The four
excluded files (`deadsync-song.json.gz`, `rust-performance.md`, `optimize.sh`, and
`optimize.ps1`) are not part of the commit.
