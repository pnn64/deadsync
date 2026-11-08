# ALSA enumeration without temporary C-string allocations

Three changes remove temporary Rust allocations while preserving PCM names,
device ordering, default selection, capabilities, routing and public errors:

1. Build control names in a 14-byte stack buffer and pass the borrowed `CStr`
   directly to `Ctl::open`, removing a formatted `String` and a `CString` per card.
2. Build physical PCM names in a 34-byte stack buffer and probe that `CStr`
   directly with `PCM::open`. Allocate the final public ID once, at its actual
   length, instead of allocating another C string for every probe.
3. Resolve default devices to their native `(card, device)` pair. Compare that
   pair directly during enumeration and use static C literals for default
   aliases. Format an owned hardware ID only when exclusive alias routing needs it.

The two bounded decimal writers share a small suffix-writing function. All new
production code is safe Rust and uses existing dependencies. Ten decimal digits
cover every `u32`; both helpers explicitly terminate their buffers and validate
the resulting C string. Public IDs retain the same text with smaller capacity.

## Allocation measurements

The fixture exposes two cards with four devices each. Counts cover a separate
warmed operation using the Rust `System` allocator; C/libasound allocations are
excluded. Requested bytes include allocation and reallocation requests, not
retained memory or RSS. Counts were identical across all repeats.

| Eight-device scan | Allocations | Reallocations | Requested bytes |
| --- | ---: | ---: | ---: |
| Before | 39 | 1 | 2,192 |
| Stack control names | 35 | 1 | 2,170 |
| Stack device names / direct probes | 27 | 1 | 1,954 |
| Native default address | 25 | 1 | 1,920 |

The combined change removes 14 allocations (35.9%) and 272 requested bytes per
normal scan. Failed probes retain their fallback results and go from 39 to 25
allocations, with nine reallocations unchanged and requested bytes 3,088 to 2,816.
An empty inventory goes from two allocations / 34 bytes to zero. A nine-device
inventory using multi-digit and maximum signed card/device indices goes from
45 allocations / four reallocations / 3,881 bytes to 28 / two / 3,496.

The private real-null-device probe drops from two allocations / 61 bytes to one
allocation / 56 bytes. Public preparation and reopening are controls: allocation
counts remain three for prepare, four for prepare plus reopen, and one for a
reopen alone. Their string ownership and execution paths are unchanged.

## CPU measurements

All comparisons use the same frozen set of 215 dependency artifacts. Each
candidate was archived and compared before the next change. The release test
binaries use Rust 1.99.1 / LLVM 23.1.1, opt-level 3, fat LTO, 16 codegen units,
disabled overflow/debug assertions, stripped debug information and the default
x86-64 target. Measurements run on Debian WSL2, Xeon E5-2696 v4, libasound 1.2.16.1.

Seven runs alternate variant order and pin execution to CPU 0. Tables show
median userspace cycles per operation; throughput is before divided by after.
The per-change regular-scan comparisons are:

| Change | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Stack control name | 261,256 | 260,615 | 1.002x |
| Stack device name / direct probe | 257,373 | 255,815 | 1.006x |
| Native default address | 263,025 | 260,917 | 1.008x |

These individual cycle differences are small. Each change is retained for its
deterministic allocation reduction, supported by reduced instruction counts.
The final comparison uses one identical expanded fixture binary for both builds:

| Workload | Before cycles/op | After cycles/op | CPU throughput | Instructions/op before → after |
| --- | ---: | ---: | ---: | ---: |
| Eight-device scan | 257,289 | 253,170 | 1.016x | 452,413 → 444,983 |
| Scan with failed probes | 41,094 | 38,450 | 1.069x | 84,693 → 78,363 |
| Empty inventory | 19,949 | 18,933 | 1.054x | 34,931 → 33,868 |
| Nine large-index devices | 290,334 | 282,707 | 1.027x | 506,810 → 499,309 |
| Real null-device probe | 53,935 | 53,849 | 1.002x | 101,172 → 100,978 |

Normal scans improve in five of seven paired runs; paired throughput ranges
from 0.942x to 1.071x. Failed probes and large-index scans improve in all seven.
The private null probe has essentially unchanged cycle cost. Prepare/reopen
controls have unchanged instructions and allocations, with cycle ratios ranging
from 0.981x to 1.017x across their medians; no gain is claimed for those paths.

Hardware counters cover the entire test process, divided by operation count;
the TSVs separately record timed-loop wall time and scheduled CPU time. Batches
contain 15,000–100,000 enumeration operations or 10,000–30,000 null-device
operations. Allocation counting is disabled during timing, while its enabled
check remains. These measure backend work with null PCMs, not physical driver
latency or whole-game performance.

## Correctness and evidence

Final release, debug and original-implementation reference builds pass 11 unit
checks, 46 fixture scenarios and every one of the 2,048 common-rate support masks.
The original runtime enumeration/routing code is retained in the reference; a
test-only adapter supplies the new private probe signature. Formatting-based
helper prototypes in the benchmark baseline are used only by tests, leaving its
original runtime paths intact. Exact patches are recorded in the provenance.

The name oracle compares 10,060 boundary/random card-device pairs with standard
decimal formatting, reusing nonzero-filled buffers. The C fixture independently
validates the actual names passed to ALSA, including zero, ten and `i32::MAX`.
Scenarios cover missing/unbacked/unmatched/default aliases, invalid or absent
names, probe failures, rate boundaries, both host clocks and timing failures.
Existing arithmetic and clock-map oracles remain enabled. Additional checks
compare embedded-NUL panic payloads, missing-PCM error strings and ready/reopen
metadata against the existing library behavior. Panic source locations are not
asserted. Default lookup skips constructing error strings that are discarded.

Scoped rustfmt, all-target ALSA Clippy, Python compilation, strict C compilation
with `-Wall -Wextra -Werror`, and `git diff --check` pass. The final release
snapshots and debug/current sources differ only in LF versus CRLF line endings.
All frozen dependency hashes were verified again after measurement.

The [provenance](2026-10-26-alsa-c-name-provenance.json) records build identities,
source patches, test/fixture versions, dependency hashes, validation logs,
benchmark commands and all 243 observations with dataset/raw-log hashes.
The [original-reference checks](2026-10-26-alsa-c-name-reference.json) preserve
their exact adapter, compile command and results.

The primary dataset is the [combined comparison](2026-10-26-alsa-c-name-final.tsv).
Per-change datasets are [control names](2026-10-26-alsa-stack-control-name.tsv),
[device names](2026-10-26-alsa-stack-device-name.tsv) and
[default addresses](2026-10-26-alsa-native-default-address.tsv). Separate pre-edit
observations remain in [initial](2026-10-26-alsa-c-name-before.tsv),
[device](2026-10-26-alsa-stack-device-before.tsv) and
[default](2026-10-26-alsa-native-address-before.tsv) datasets. Binaries, snapshots
and raw subprocess logs remain under `target/alsa-performance`; the immutable
dependency snapshot is in the Linux target directory.

To reproduce a new pass, use fresh stage names with
`scripts/benchmark_alsa.py --build NAME --frozen-deps PATH`, then
`--compare BEFORE AFTER --fixture-stage AFTER --runs 7 --workloads ... --output PATH`.
Use a normal Cargo `--build NAME --debug` archive for debug validation.
