# ALSA prepared names, rate storage and deferred probe errors

This pass retains three changes, using `066a912d4` as the production baseline:

1. Store the prepared PCM name as an owned `CString`. Consume the selected input
   string during preparation and pass its borrowed `CStr` directly when reopening
   the stream. This removes the temporary reopen allocation and the intermediate
   string copy. Public ready metadata, routing, error text and NUL panic payloads
   are preserved.
2. Assemble, sort and deduplicate the bounded rate list on the stack, then copy
   only its unique values into the public vector. A supported default rate is
   already in the common-rate mask, so it is not inserted twice. This removes
   duplicate sorting work and the fixed 14-element heap reservation.
3. Return a static reason and optional ALSA error from private device probes.
   The caller formats these directly into the warning after the probe's resources
   have closed. This removes intermediate error strings and their reallocations,
   including when warnings are enabled. The two warning forms retain the exact
   original messages, severity, target module, fallback values and cleanup order.

No production unsafe code or dependencies are added. The private probe error is
a tuple alias, with no formatting object, boxed error or additional allocation.
Rate probing still calls ALSA in the original order, including boundary queries.
The fixed array has space for a default, all 11 common rates and both boundaries;
at least one rate is present before sorting and compaction.

## Allocation measurements

Counts cover one separate warmed operation using the Rust `System` allocator.
C/libasound allocations are excluded. Requested bytes sum allocation and
reallocation requests; they are not retained memory or RSS. Each row's counts
are identical across all seven repeats.

| Operation | Allocations before → after | Reallocations before → after | Requested bytes before → after |
| --- | ---: | ---: | ---: |
| Prepare real null PCM | 3 → 2 | 0 → 1 | 16 → 16 |
| Prepare and reopen | 4 → 2 | 0 → 1 | 21 → 16 |
| Reopen prepared PCM | 1 → 0 | 0 → 0 | 5 → 0 |
| Rate list, all common rates plus distinct bounds | 1 → 1 | 0 → 0 | 56 → 52 |
| Rate list, one common rate plus bounds | 1 → 1 | 0 → 0 | 56 → 12 |
| Rate list, bounds only | 1 → 1 | 0 → 0 | 56 → 8 |
| Eight-device scan, successful probes | 25 → 25 | 1 → 1 | 1,920 → 1,824 |
| Eight-device scan, failed opens | 25 → 17 | 9 → 1 | 2,816 → 1,472 |

Prepared-name reuse removes a reopen allocation. Consuming an exactly sized
input string introduces a NUL-terminator reallocation during preparation, so
prepare alone still makes three allocation/reallocation requests. Prepare plus
reopen makes three instead of four. No standalone preparation CPU gain is claimed.

The enumeration fixture has two cards and four devices per card. Its successful
probes report boundary rates already in the common set, yielding 11 unique values
and a 44-byte vector instead of 56 bytes. Failed probes save eight allocations,
eight reallocations and 1,344 requested bytes per scan. These counts also hold
with the warning-enabled benchmark logger, which formats and fingerprints every
message without allocating itself.

## CPU measurements and tradeoffs

The final comparison uses unchanged production source from `066a912d4` and the
final candidate, with the same expanded test harness and one identical fixture
binary. The original test adapter changes only the private null-open argument
from a C literal to a string literal; production source is unchanged.

All timed variants use the same frozen set of 215 Rust dependency artifacts.
Release flags are opt-level 3, fat LTO, 16 codegen units, disabled overflow/debug
assertions, stripped debug information and the default x86-64 target. The host
is Debian WSL2, Xeon E5-2696 v4, Rust 1.99.1 / LLVM 23.1.1, libasound 1.2.16.1.

Seven runs alternate variant order and pin execution to CPU 0. Results are
median userspace cycles per operation. CPU throughput is before divided by after.

| Final comparison | Before cycles/op | After cycles/op | CPU throughput |
| --- | ---: | ---: | ---: |
| Prepare real null PCM | 47,947 | 48,337 | 0.992x |
| Prepare and reopen | 67,282 | 66,940 | 1.005x |
| Reopen prepared PCM | 18,300 | 17,561 | 1.042x |
| Real null-PCM rate capabilities | 26,929 | 26,884 | 1.002x |
| Full common-rate support fixture | 493.98 | 472.32 | 1.046x |
| One supported common rate | 321.68 | 324.38 | 0.992x |
| Boundary rates only | 346.93 | 355.26 | 0.977x |
| Successful eight-device scan | 254,769 | 252,598 | 1.009x |
| Failed opens, warnings disabled | 39,139 | 32,656 | 1.199x |
| Failed opens, warnings enabled | 45,876 | 40,974 | 1.120x |
| Private real null-device probe | 52,966 | 53,402 | 0.992x |

Both failed-probe workloads improve in all seven paired runs. Their instruction
counts fall from 78,822 to 64,400 with warnings disabled and from 93,563 to 85,565
with warnings enabled. Full common-rate support improves in all seven runs and
uses 39 fewer instructions per operation. Ordinary scan and null-device CPU
differences are small; they do not support a broad throughput claim.

Sparse rate cases exchange about 0.8–2.3% CPU throughput for 79–86% fewer requested
heap bytes. The bounds-only case is slower in all seven final pairs. This is a
measured memory/CPU tradeoff, not a universal rate-query speedup. Prepared-name
reuse is retained for allocation removal: reopening cycles varied across earlier
comparisons, and its final 4.2% median gain is not treated as a reliable CPU result.

Each change also has a separate before/after comparison. The retained rate-list
variant's initial full-support comparison was 477.10 → 424.19 cycles/op (1.125x);
the expanded final fixture adds wrapper overhead to both variants. The isolated
deferred-error comparison was 39,941 → 33,615 cycles/op without warnings (1.188x)
and 45,733 → 40,203 with warnings (1.138x). Earlier name-cache, rate-list and
pre-edit observations remain archived as well.

Hardware counters cover the whole test process, divided by operation count;
timed-loop wall time and scheduled CPU time are recorded separately. Batches use
10,000–30,000 null-device operations, 20,000 successful scans, 100,000 failed
scans, 30,000 warning-enabled scans or 200,000 rate queries. Allocation counting
is disabled during timing while its enabled check remains. Logger setup and
prepared-stream setup for reopen-only work occur outside the timed loop. These
are backend/null-PCM measurements, not physical-driver or whole-game results.

## Correctness and retained evidence

Final frozen-release, frozen-dependency debug, Cargo debug and unchanged-original
reference builds pass 11 unit checks, 52 fixture scenarios and every one of the
2,048 common-rate masks. Debug validates overflow checks and debug assertions;
the frozen debug build retains release dependency artifacts, while Cargo debug
also validates the current workspace dependency integration.

Six added failure scenarios compare all eight warnings and fallback devices for
open, hardware-parameter, access, format, channel and rate failures. A test-only
close log verifies that successful default resolution and each failed probe have
closed before its warning is emitted. Text, default flags, sample rates, channel
counts and device names match the original implementation. Existing checks cover
10,060 formatted ID pairs, routing, missing-PCM errors, embedded-NUL panic
payloads, ready/reopen metadata, frame conversion and clock mapping.

The early direct-warning trial was discarded because it logged before resource
cleanup. The final small error value preserves cleanup ordering without retaining
the intermediate string. An initial stack-list trial still inserted the default
twice; the retained variant removes that duplicate as well.

Scoped rustfmt, all-target ALSA Clippy, Python compilation, strict C compilation
with `-Wall -Wextra -Werror`, and `git diff --check` pass. A transient unrelated
missing GIF benchmark prevented the first Cargo debug attempt; the later Cargo
debug build and Clippy retry pass. The verified ALSA sources were restored from
their immutable snapshots after concurrent workspace activity reverted them.
Source identities and all frozen dependency hashes are checked again before commit.

The [provenance](2026-10-26-alsa-prep-rates-provenance.json) contains all 581
observations with dataset/raw-log hashes, build flags, source patches, test and
fixture snapshots, validation logs, reference adapters and reproduction scripts.
The primary dataset is the [final comparison](2026-10-26-alsa-prep-rates-final.tsv).
The per-change comparisons include [prepared names](2026-10-26-alsa-owned-prep-name.tsv),
[distinct rate lists](2026-10-26-alsa-stack-rates-distinct.tsv) and
[deferred probe errors](2026-10-26-alsa-lazy-probe-error.tsv).
Binaries and raw logs remain in `target/alsa-performance`; the immutable
dependency snapshot remains in the Linux target directory.
