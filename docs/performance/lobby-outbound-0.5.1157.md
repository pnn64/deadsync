# Lobby outbound preparation - 0.5.1157

Baseline: `04f769207` (0.5.1156). This pass applies the supplied
`rust-performance.md` guidance on measured CPU/allocation costs (M-HOTPATH),
reuse of owned storage (M-MEM-REUSE), and sufficient initial capacity
(M-INITIAL-CAPACITY). The guide itself is excluded from the commit.

All 40 comparisons use fewer median thread cycles in this run. Two-player
command encoding with full statistics uses 82.57% fewer cycles, populated song
selection uses 73.49% fewer, and two-player state construction uses 12.09% fewer.
Complete two-player update preparation drops from 132 allocation calls and four
reallocations to 61 calls and three reallocations, using 48.27% fewer cycles
and 44.56% less requested-byte churn.

## Changes

1. **Borrow machine state when encoding command envelopes.** Create, join and
   update commands serialize small borrowed payload structs directly. The old
   json! wrappers cloned the complete existing Value tree, allocated wrapper
   keys and copied code/password strings before producing the final text.
   The new path only allocates the output string and its growth buffers.
2. **Serialize song selections directly.** Borrow the song's string fields and
   emit explicit nulls without first constructing a Value tree. Field order
   matches the old output with this application's locked preserve_order feature
   set. Optional f32 values still widen to f64 before serialization, preserving
   the previous decimal representations, signed zero and non-finite nulls.
3. **Move machine-player strings into their JSON objects.** State conversion
   consumes its players, so player ID, profile name and screen name transfer
   their existing allocations into Value::String. Object maps reserve the known
   field counts. Profile-name construction uses the existing borrowed prefix
   helper and reserves exactly the result length, eliminating the temporary
   four-character string and output growth.

## Method

Windows x86_64 MSVC; Intel Xeon E5-2696 v4 @ 2.20 GHz; rustc 1.98.0
(88d9e12ae, 2026-08-18), LLVM 22.1.8. Cargo release uses optimization level 3
and full LTO. Seven original functions are frozen in a test-only module;
only their visibility differs. Common types, generic Value-envelope encoding,
command routing, socket work and inbound parsing remain unchanged.

All 40 old/new pairs run in the same release binary with black-boxed inputs.
Three invocations alternate old/new, new/old, old/new. Each invocation uses
three warmups, seven timing batches and a separate allocation-counted operation.
Tables report the median of the three per-invocation medians. Batches contain
512 operations, except profile names (1,024) and long-name state construction
(256). No Cargo build runs alongside the benchmarks.

CPU cycles use Windows QueryThreadCycleTime for the calling thread. The existing
scoped allocator delegates to System and counts allocations, reallocations,
frees and requested bytes separately from timing. Returned values are dropped
inside every measured operation. Byte totals include replacement buffers and
describe allocation churn, not peak RSS, committed heap pages, cache misses or
performance of the production allocator.

Command fixtures contain zero, one or two players, with and without full
judgment/score statistics. The first player has an unprefixed Unicode name;
the second has a prefixed name and different screen/ready state. Create and
join use four-character passwords/codes. Existing machine Values are built
outside the command timing; their old wrapper copies are included. Zero-player
plain/stats cases intentionally produce the same empty-player payload.

State cases include player creation and Value conversion, with the old profile
name implementation on the old side. Pipeline cases additionally encode the
resulting update command, including destruction of its temporary machine Value.
The state_long fixture uses a 2,560-byte escaped Unicode name for player one.
Song cases cover all-default fields, populated Unicode/escaped text, non-finite
floats, and strings repeated 128 times. Song fixture creation is outside timing.
Name cases cover whitespace, ordinary text, an existing prefix, Unicode and
long text. All outputs are constructed during measurement on both sides.

These are local preparation benchmarks. They exclude the runtime cache,
connection, transport, server work and rendering. They do not establish an
end-to-end frame-rate or network-throughput gain. Commands still require an
owned output string, and machine Values still own keys and object storage;
these paths are not completely allocation-free.

## Timing, CPU and throughput

Throughput below is complete operations per second, calculated from ns/op.
Raw output also reports input bytes/s for name cases. Positive CPU savings
mean fewer cycles. Tiny/control cases can be sensitive to code layout and
timer overhead.

| Case | Old ns/op | New ns/op | Old cycles/op | New cycles/op | CPU saved | Old -> new ops/s |
|---|---:|---:|---:|---:|---:|---:|
| create_0_plain | 961.3 | 186.5 | 2,116.5 | 411.6 | 80.55% | 1,040,258 -> 5,361,930 |
| join_0_plain | 1,141.8 | 195.9 | 2,397.4 | 432.6 | 81.96% | 875,810 -> 5,104,645 |
| update_0_plain | 768.2 | 158.4 | 1,688.3 | 350.3 | 79.25% | 1,301,744 -> 6,313,131 |
| state_0_plain | 370.1 | 294.7 | 814.6 | 649.5 | 20.27% | 2,701,972 -> 3,393,281 |
| pipeline_0_plain | 1,053.7 | 462.9 | 2,310.8 | 1,014.8 | 56.08% | 949,037 -> 2,160,294 |
| create_0_stats | 864.8 | 166.4 | 1,900.5 | 367.8 | 80.65% | 1,156,337 -> 6,009,615 |
| join_0_stats | 1,028.3 | 190.8 | 2,255.9 | 421.0 | 81.34% | 972,479 -> 5,241,090 |
| update_0_stats | 711.3 | 146.9 | 1,563.5 | 324.5 | 79.25% | 1,405,877 -> 6,807,352 |
| state_0_stats | 351.2 | 293.8 | 773.4 | 646.9 | 16.36% | 2,847,380 -> 3,403,676 |
| pipeline_0_stats | 1,085.0 | 455.7 | 2,358.8 | 998.9 | 57.65% | 921,659 -> 2,194,426 |
| create_1_plain | 2,294.1 | 393.8 | 5,037.8 | 868.6 | 82.76% | 435,901 -> 2,539,360 |
| join_1_plain | 2,477.0 | 431.1 | 5,443.3 | 948.3 | 82.58% | 403,714 -> 2,319,647 |
| update_1_plain | 2,022.3 | 372.5 | 4,438.0 | 820.1 | 81.52% | 494,486 -> 2,684,564 |
| state_1_plain | 1,712.7 | 1,346.9 | 3,755.1 | 2,955.5 | 21.29% | 583,873 -> 742,446 |
| pipeline_1_plain | 3,716.4 | 1,757.6 | 8,121.1 | 3,856.3 | 52.52% | 269,078 -> 568,958 |
| create_1_stats | 4,989.8 | 839.3 | 10,945.8 | 1,844.3 | 83.15% | 200,409 -> 1,191,469 |
| join_1_stats | 5,013.9 | 856.2 | 11,000.3 | 1,878.6 | 82.92% | 199,446 -> 1,167,951 |
| update_1_stats | 4,157.2 | 792.0 | 9,119.5 | 1,741.0 | 80.91% | 240,547 -> 1,262,626 |
| state_1_stats | 3,470.5 | 3,014.3 | 7,611.3 | 6,611.6 | 13.13% | 288,143 -> 331,752 |
| pipeline_1_stats | 8,037.9 | 3,922.7 | 17,459.7 | 8,486.8 | 51.39% | 124,411 -> 254,926 |
| create_2_plain | 3,981.4 | 621.9 | 8,716.1 | 1,367.6 | 84.31% | 251,168 -> 1,607,976 |
| join_2_plain | 4,074.8 | 658.8 | 8,939.1 | 1,443.5 | 83.85% | 245,411 -> 1,517,911 |
| update_2_plain | 3,721.7 | 599.0 | 8,151.5 | 1,317.4 | 83.84% | 268,694 -> 1,669,449 |
| state_2_plain | 3,356.4 | 2,456.8 | 7,363.5 | 5,386.3 | 26.85% | 297,938 -> 407,034 |
| pipeline_2_plain | 6,879.3 | 3,041.4 | 15,075.2 | 6,670.3 | 55.75% | 145,364 -> 328,796 |
| create_2_stats | 8,361.5 | 1,420.3 | 18,192.3 | 3,113.3 | 82.89% | 119,596 -> 704,077 |
| join_2_stats | 8,510.5 | 1,467.6 | 18,524.2 | 3,218.8 | 82.62% | 117,502 -> 681,385 |
| update_2_stats | 8,135.0 | 1,399.2 | 17,604.2 | 3,069.1 | 82.57% | 122,926 -> 714,694 |
| state_2_stats | 6,771.1 | 6,001.8 | 14,848.8 | 13,053.0 | 12.09% | 147,686 -> 166,617 |
| pipeline_2_stats | 15,265.4 | 7,822.7 | 33,154.8 | 17,151.0 | 48.27% | 65,508 -> 127,833 |
| song_empty | 1,723.8 | 356.4 | 3,782.9 | 784.5 | 79.26% | 580,114 -> 2,805,836 |
| song_full | 2,154.7 | 572.5 | 4,749.3 | 1,259.1 | 73.49% | 464,102 -> 1,746,725 |
| song_nonfinite | 1,602.1 | 334.2 | 3,511.1 | 735.7 | 79.05% | 624,181 -> 2,992,220 |
| song_long | 11,389.8 | 9,099.6 | 24,835.7 | 19,960.4 | 19.63% | 87,798 -> 109,895 |
| name_blank | 162.5 | 68.5 | 358.0 | 151.5 | 57.68% | 6,153,846 -> 14,598,540 |
| name_plain | 254.8 | 68.8 | 561.8 | 152.4 | 72.87% | 3,924,647 -> 14,534,884 |
| name_prefixed | 131.9 | 74.5 | 290.9 | 164.8 | 43.35% | 7,581,501 -> 13,422,819 |
| name_unicode | 326.7 | 71.3 | 718.3 | 157.6 | 78.06% | 3,060,912 -> 14,025,245 |
| name_long | 298.0 | 116.1 | 655.3 | 254.2 | 61.21% | 3,355,705 -> 8,613,264 |
| state_long | 6,475.4 | 5,781.6 | 14,204.9 | 12,625.5 | 11.12% | 154,431 -> 172,963 |

## Allocation churn

Allocation/free call counts and allocated/freed byte totals match in every
measured operation. They are identical across all three runs. The table shows
old -> new counts; reallocations are separate from allocation/free calls.

| Case | Allocations / frees | Reallocations | Allocated / freed bytes |
|---|---:|---:|---:|
| create_0_plain | 10 -> 1 | 0 -> 0 | 785 -> 128 |
| join_0_plain | 12 -> 1 | 0 -> 0 | 793 -> 128 |
| update_0_plain | 8 -> 1 | 0 -> 0 | 773 -> 128 |
| state_0_plain | 4 -> 4 | 0 -> 0 | 274 -> 274 |
| pipeline_0_plain | 12 -> 5 | 0 -> 0 | 1,047 -> 402 |
| create_0_stats | 10 -> 1 | 0 -> 0 | 785 -> 128 |
| join_0_stats | 12 -> 1 | 0 -> 0 | 793 -> 128 |
| update_0_stats | 8 -> 1 | 0 -> 0 | 773 -> 128 |
| state_0_stats | 4 -> 4 | 0 -> 0 | 274 -> 274 |
| pipeline_0_stats | 12 -> 5 | 0 -> 0 | 1,047 -> 402 |
| create_1_plain | 22 -> 1 | 1 -> 1 | 1,942 -> 384 |
| join_1_plain | 24 -> 1 | 1 -> 1 | 1,950 -> 384 |
| update_1_plain | 20 -> 1 | 1 -> 1 | 1,930 -> 384 |
| state_1_plain | 20 -> 16 | 1 -> 0 | 1,223 -> 1,175 |
| pipeline_1_plain | 40 -> 17 | 2 -> 1 | 3,153 -> 1,559 |
| create_1_stats | 38 -> 1 | 2 -> 2 | 4,195 -> 896 |
| join_1_stats | 40 -> 1 | 2 -> 2 | 4,203 -> 896 |
| update_1_stats | 36 -> 1 | 2 -> 2 | 4,183 -> 896 |
| state_1_stats | 36 -> 32 | 1 -> 0 | 2,964 -> 2,916 |
| pipeline_1_stats | 72 -> 33 | 3 -> 2 | 7,147 -> 3,812 |
| create_2_plain | 34 -> 1 | 2 -> 2 | 3,351 -> 896 |
| join_2_plain | 36 -> 1 | 2 -> 2 | 3,359 -> 896 |
| update_2_plain | 32 -> 1 | 2 -> 2 | 3,339 -> 896 |
| state_2_plain | 36 -> 28 | 1 -> 0 | 2,154 -> 2,072 |
| pipeline_2_plain | 68 -> 29 | 3 -> 2 | 5,493 -> 2,968 |
| create_2_stats | 66 -> 1 | 3 -> 3 | 7,857 -> 1,920 |
| join_2_stats | 68 -> 1 | 3 -> 3 | 7,865 -> 1,920 |
| update_2_stats | 64 -> 1 | 3 -> 3 | 7,845 -> 1,920 |
| state_2_stats | 68 -> 60 | 1 -> 0 | 5,636 -> 5,554 |
| pipeline_2_stats | 132 -> 61 | 4 -> 3 | 13,481 -> 7,474 |
| song_empty | 14 -> 1 | 1 -> 1 | 1,809 -> 384 |
| song_full | 20 -> 1 | 2 -> 2 | 2,423 -> 896 |
| song_nonfinite | 15 -> 1 | 1 -> 1 | 1,824 -> 384 |
| song_long | 20 -> 1 | 6 -> 6 | 22,340 -> 16,256 |
| name_blank | 1 -> 1 | 1 -> 0 | 24 -> 11 |
| name_plain | 2 -> 1 | 1 -> 0 | 32 -> 10 |
| name_prefixed | 2 -> 1 | 0 -> 0 | 16 -> 8 |
| name_unicode | 2 -> 1 | 2 -> 0 | 49 -> 17 |
| name_long | 2 -> 1 | 1 -> 0 | 2,580 -> 2,564 |
| state_long | 68 -> 60 | 1 -> 0 | 10,734 -> 8,104 |

## Results and limits

- Two-player command encoding: 82.57% fewer median cycles, 64 -> 1 allocations and 7,845 -> 1,920 requested bytes per operation.
- Populated song selection: 73.49% fewer median cycles, 20 -> 1 allocations and 2,423 -> 896 requested bytes per operation.
- Two-player state construction: 12.09% fewer median cycles, 68 -> 60 allocations and 5,636 -> 5,554 requested bytes per operation.
- Complete two-player update preparation: 48.27% fewer median cycles, 132 -> 61 allocations and 13,481 -> 7,474 requested bytes per operation.

All cases with increased median cycles:

None in these measurements; results still depend on hardware and workload.

## Behavior and verification

- 278 online library tests pass in debug and release; six remain ignored by
  default, including the manual benchmarks and live download test.
- Four new tests compare exact command text, state Values and full update
  pipelines to the frozen implementations. They cover empty and one/two-player
  states, a second-player-only state, absent/partial statistics, escaped/Unicode
  strings, arbitrary existing machine Values, explicit nulls and non-finite
  values.
- Song tests cover all 256 field-presence combinations, numeric extremes,
  subnormals and 1,024 deterministic generated f32 bit patterns. They compare
  output bytes, including key order and number representation.
- Name tests cover mixed-case prefixes, Unicode boundaries, whitespace and
  128 generated inputs. Allocation budgets enforce one exactly sized output
  allocation for representative names. Pointer checks prove that consumed
  player strings become the corresponding JSON string storage.
- `cargo clippy -p deadsync-online --all-targets --locked -- -D clippy::perf`
  passes; existing non-performance warnings remain.
- `cargo check --all-targets --locked`, targeted rustfmt checks and
  `git diff --check` pass.
- The audit verifies seven frozen baseline functions, unchanged shared helpers,
  final benchmark/source hashes, main, and exactly one patch increment in
  Cargo.toml and all three version-inheriting Cargo.lock packages.

Reproduce from the repository root (PowerShell):

```powershell
cargo test -p deadsync-online --lib --locked -- --test-threads=1
cargo test -p deadsync-online --release --lib --locked -- --test-threads=1
Remove-Item Env:DEADSYNC_PERF_REVERSE -ErrorAction SilentlyContinue
cargo test -p deadsync-online --release --lib --locked lobby_outbound_bench -- --ignored --test-threads=1 --nocapture
$env:DEADSYNC_PERF_REVERSE = '1'
cargo test -p deadsync-online --release --lib --locked lobby_outbound_bench -- --ignored --test-threads=1 --nocapture
Remove-Item Env:DEADSYNC_PERF_REVERSE
cargo test -p deadsync-online --release --lib --locked lobby_outbound_bench -- --ignored --test-threads=1 --nocapture
```

Raw local runs and source hashes are retained under `target/lobby-outbound-perf/`
(ignored). The committed test-only harness and frozen baselines can reproduce
the comparisons without those local scripts.
