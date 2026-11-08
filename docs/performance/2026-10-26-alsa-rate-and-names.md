# ALSA rate probing and device names

This pass follows `f6bac7311` and retains three further optimizations:

1. Combine preferred-rate selection and supported-rate collection. An eleven-bit
   mask records each common rate's result once. Its first set bit preserves the
   existing preference order; the returned list remains sorted and deduplicated.
   This removes one helper and the duplicate common-rate array. Previously,
   successful common-rate discovery made 12–22 rate tests; it now makes eleven.
   Minimum/maximum fallback rules, errors and the vector's capacity are retained.
2. Borrow the card-info name while enumerating that card. Keep its `CardInfo`
   owner alive instead of cloning the name. Failed card-info/name queries still
   use the owned `Card::get_name()` result or `Card {index}` fallback.
3. Reserve each valid display name's complete byte length and append its three
   components directly. This removes generic formatting and growth copies from
   the successful name path. Invalid device names retain their original fallback.

All production changes use safe Rust. Explicit ownership lifetimes and direct
appends add five production lines overall, while removing a helper, duplicated
rate queries, card-name copies and display-name growth.

ALSA's `test_rate` delegates to `SND_TEST`, which operates on a configuration
copy rather than committing constraints. This supports reusing the test results
within one hardware-parameter probe. See the pinned upstream
[rate API](https://raw.githubusercontent.com/alsa-project/alsa-lib/v1.2.16.1/src/pcm/pcm.c)
and [parameter implementation](https://raw.githubusercontent.com/alsa-project/alsa-lib/v1.2.16.1/src/pcm/pcm_params.c).

## CPU and allocation measurements

Rust 1.99.1 / LLVM 23.1.1, libasound 1.2.16.1, Debian WSL2 on a Xeon E5-2696
v4. Actual Cargo release binaries use opt-level 3, LTO and the default target
CPU. Each comparison has seven rotating/alternating runs, pinned to CPU 0.
Values below are medians; CPU throughput is before cycles divided by after.

| Operation | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Rate capabilities, real null PCM | 29,116.56 | 26,898.29 | 1.082x |
| Complete real null-device probe | 56,231.11 | 53,656.97 | 1.048x |
| Eight-device enumeration fixture | 264,835.50 | 255,963.22 | 1.035x |
| Eight devices with failed probes | 39,913.27 | 37,540.41 | 1.063x |

The first two workloads run **without the preload fixture**, exercising real
libasound constraint testing. Capability-query setup opens the PCM and constructs
`HwParams` outside timing; complete device probing includes those operations.
Their instruction counts fall from 58,451 to 53,989 and from 105,635 to 101,172
per operation, respectively. Rust allocation counts remain 1 and 2.

The separate rate change measured 1.078x capability throughput and 1.047x full
probe throughput before the name changes. Direct display-name construction
measured 1.057x CPU throughput on failed probes initially and 1.049x in the
four-stage confirmation. Normal enumeration's cycle gains are smaller and more
variable; its allocation improvements below are deterministic in every sample.

| Eight-device scan stage | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: |
| Baseline | 41 | 9 | 2,390 |
| Single rate pass | 41 | 9 | 2,390 |
| Borrow card names | 39 | 9 | 2,352 |
| Reserve display names | 39 | 1 | 2,192 |

The name changes remove two allocations and eight growth reallocations per
eight-device scan. Borrowing extends each existing C `CardInfo` allocation's
lifetime through that card's iteration; it does not change its allocation count.
The error-probe scan drops from 41 allocations/17 reallocations/3,286 requested
bytes to 39/9/3,088. Error-string construction remains part of that workload.

`perf stat` records userspace hardware cycles/instructions and task-clock for
the entire process; loop wall time is separate. Batches perform 200,000 capability
queries, 20,000 complete probes or normal scans, or 100,000 failed-probe scans.
Rust allocator increments are disabled during timing, with its enabled check
still present. Allocation counts cover a separate warmed operation, exclude C
allocations, and count requested bytes rather than peak/live heap or RSS.

The enumeration fixture supplies two cards/four devices each and fixed rate
responses; its successful PCM opens use real null PCMs. It is not a hardware
driver latency benchmark. The unchanged playback-timing and shared-ID controls
vary by 1.8% and 1.0% in cycles; neither is claimed as an optimization here.

## Correctness and evidence

The frozen baseline and final source pass five unit checks, 32 fixture scenarios
and every one of the 2,048 common-rate support masks. The same checks pass in
the final debug build, including checked arithmetic. Rate cases preserve
preference order, sorted output, minimum/maximum errors, rejected bounds, zero
minimum, reversed bounds, duplicate bounds and complete failure. Name cases
cover valid/invalid/missing card names, failed card-info queries, invalid device
names, empty names and long UTF-8 names. Existing stream shape, timing quality,
clock health, routing and default-selection checks also pass.

Scoped rustfmt, Clippy on all ALSA targets, Python compilation and C compilation
with `-Wall -Wextra -Werror` pass. Physical hardware and a whole-game render were
not exercised. Unrelated worktree changes are excluded.

[Provenance](2026-10-26-alsa-rate-and-names-provenance.json) retains six stage
identities, source patches, configuration/dependency hashes, fixture/test source
versions and all 206 observations with dataset hashes.
[Reference results](2026-10-26-alsa-capabilities-reference.json) retain the
independent frozen-source build command and all scenario results. Its small
adapter patch forwards the baseline's two original rate functions so every
version can use the same benchmark/test interface; the exact patch is included.
The verification script is embedded in the provenance. Binaries and raw logs
remain under `target/alsa-performance`.

Every change had a measured baseline before editing, followed by its own paired
comparison: [rate queries](2026-10-26-alsa-single-rate-pass.tsv),
[card names](2026-10-26-alsa-borrowed-card-name.tsv) and
[display names](2026-10-26-alsa-sized-display-name.tsv). Confirmation retains
[all four name stages](2026-10-26-alsa-name-stages-confirm.tsv) and
[final probe/control results](2026-10-26-alsa-probes-final.tsv).

```sh
export CARGO_TARGET_DIR=/home/perfecttaste/deadsync-alsa-target
python3 scripts/benchmark_alsa.py --build new-before
# Apply one change, then archive under another unused name.
python3 scripts/benchmark_alsa.py --build new-after
python3 scripts/benchmark_alsa.py --compare new-before new-after --fixture-stage new-after --workloads rate_capabilities probe_null enumerate enumerate_failed --runs 7 --output target/alsa-next.tsv
python3 scripts/benchmark_alsa.py --build new-debug --debug
cargo clippy --locked -p deadlib-audio-backend-alsa --all-targets --no-deps
```

The runner preserves existing archives, rejects mixed debug/release comparisons,
and runs all fixture scenarios and rate masks during each new archive build.
