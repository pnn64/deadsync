# PNG decoder results, 2026-11-19

The native decoder passes the aggregate performance gate on this Windows x86-64
host for the measured buffered RGBA8 workload. Across two independent processes,
it uses **7.2–7.5% fewer CPU cycles than the current png asset path** and
**9.7–10.0% fewer than image**, with identical RGBA8 output. All three corpus
groups also have lower native total cycles in both runs. Individual files still
have losses; this is not a claim about every PNG or every architecture.

The final implementation uses **2.1–3.1% fewer native total cycles** than the
frozen starting implementation, including the four retained optimizations and
the separately committed compatibility correction. Per-file gains are larger on
packed samples and two-byte filter strides. Runtime loader integration is a
separate step.

## Equivalent RGBA8 measurements

The frozen corpus contains **116 images / 54,994,264 pixels**: 60 generated format
cases, 24 pinned Kodak photos and 32 distinct bundled assets. The asset selection
was fixed before measuring, from 1,016 runtime PNG paths / 735 distinct byte
contents. It includes large atlases, fonts, packed palettes, metadata-rich icons
and a sixteen-bit RGBA asset. Input hashes, original asset paths and selection
reasons are retained in the [experiment archive](results/windows-x86_64-20261119-rgba8/README.md).

Totals below represent one decode of every input. They are not a measured
distribution of deadsync loading frequency. Each row includes output destruction.
The two final runs are reported separately rather than selecting the better one.

| Implementation / run | Total CPU cycles | Cycles/pixel | Throughput, MP/s |
| --- | ---: | ---: | ---: |
| Native before | 880,484,332 | 16.01 | 136.79 |
| Native final, run 1 | 862,133,190 | 15.68 | 139.71 |
| Native final, run 2 | 853,363,677 | 15.52 | 141.15 |
| png asset path, run 1 | 932,476,100 | 16.96 | 129.16 |
| png asset path, run 2 | 919,161,480 | 16.71 | 131.04 |
| image, run 1 | 954,747,359 | 17.36 | 126.16 |
| image, run 2 | 948,640,721 | 17.25 | 126.97 |

The reference implementations are measured within each final process. Their own
variation is visible in the table; the performance decision uses native/reference
ratios from the same run, alongside the separate native before/after comparison.

Ratios below 1 favor native. Each pair is run 1 / run 2:

| Corpus | Images | Native/png total cycles | Native/image total cycles | Native/png equal-file geometric mean | Native/image equal-file geometric mean |
| --- | ---: | ---: | ---: | ---: | ---: |
| All | 116 | 0.9246 / 0.9284 | 0.9030 / 0.8996 | 0.7370 / 0.7427 | 0.5144 / 0.5169 |
| Formats | 60 | 0.6590 / 0.6636 | 0.3442 / 0.3457 | 0.6132 / 0.6159 | 0.3235 / 0.3242 |
| Photos | 24 | 0.9206 / 0.9248 | 0.9249 / 0.9258 | 0.9181 / 0.9224 | 0.9221 / 0.9235 |
| Selected assets | 32 | 0.9406 / 0.9442 | 0.9439 / 0.9373 | 0.8822 / 0.8970 | 0.7922 / 0.8019 |

Native wins against the png asset path on **100/116 inputs in both runs**:
58/60 formats, 16/24 photos and 26/32 assets. Against image it wins on 101/116
and 103/116. Native itself improves from the starting implementation on 95/116
and 98/116 inputs. Equal-file geometric means must not be read as total CPU
savings; the two largest png asset costs alone account for 37.1% of the baseline
reference total.

## Remaining individual losses

These examples use the arithmetic mean of each file's two final cycle medians.
Ratios divide that native mean by the corresponding png mean. Each listed file
is slower than png in both runs; the table includes the largest relative gaps
and the largest absolute cycle deficit.

| Input | Mean native cycles | Mean png cycles | Native/png |
| --- | ---: | ---: | ---: |
| `icon-32.png` (`asset-03`) | 51,798.5 | 27,379.5 | 1.8919 |
| `icon-16.png` (`asset-02`) | 47,869.5 | 34,626 | 1.3825 |
| Delta-note center texture (`asset-22`) | 3,831,228 | 3,495,435 | 1.0961 |
| GrayAlpha8 noninterlaced noisy fixture | 293,961 | 269,742 | 1.0898 |
| `kodim18.png` | 14,690,771 | 13,528,601 | 1.0859 |
| Catjam atlas (`asset-23`) | 155,864,055 | 151,052,353.5 | 1.0319 |

Catjam improved by 6.3% against the native starting version, but still has the
largest absolute deficit against png, about 4.81 million cycles. The small icons
have much larger ratios but much smaller absolute costs. The archived per-file
analysis retains all inputs, including losses and increases from the native
baseline. These measurements establish an aggregate advantage for the selected
workload, not an unconditional replacement advantage for every asset.

The full runs also raised three possible increases against the native starting
version. A targeted check repeated the immutable baseline and final executables
in both orders, with five 80 ms batches per case:

| Input | Native cycle change, before → final, order 1 | Reversed executable order |
| --- | ---: | ---: |
| GameplayIn texture (`asset-20`) | −1.27% | +2.37% |
| Snoop atlas (`asset-24`) | −3.20% | −3.79% |
| Rin Cat atlas (`asset-31`) | +7.12% | −1.80% |

A consistent worsening was not reproduced in those pairs; reference timings also
varied. The full runs remain part of the evidence, including asset-24 being 1.27%
slower than image in full run 2. These follow-up checks do not establish a speed
guarantee for any file. The archive retains all four targeted TSVs, their process
record and both summaries.

## Retained changes and isolated measurements

The retained changes have paired CPU-cycle measurements and correctness checks.
These targeted subsets exercise particular paths; their percentage changes are
not additive and do not predict the complete workload's improvement. Historical
source-to-executable lineage limits are recorded below and in the archive.

| Change | Targeted comparison | Native cycles before → after | Result |
| --- | --- | ---: | ---: |
| Portable two-byte Average/Paeth reconstruction | 8 stride-2 format cases | 3,534,254 → 3,159,914 | 10.6% fewer |
| Packed sample expansion | 2-/4-bit noninterlaced cases | Per-file pairs retained in `packed-unpack-*.tsv` | 28.2–65.5% fewer |
| Simpler literal hot loop, first comparison | 8 selected photos/assets | 300,810,280 → 292,277,378 | 2.84% fewer |
| Simpler literal hot loop, reversed-order repeat | Same 8 inputs | 301,165,971 → 293,700,705 | 2.48% fewer |
| Deferred stored-block initialization | 23 first-stored-block cases, after compatibility fix | 6,517,543 → 6,338,438 | 2.75% fewer |

The stride-2 change removes the 13-line indexed recurrence and uses the existing
fixed-lane portable helper. It adds no instruction-set requirement. Packed
expansion separates complete output groups from the tail: two- and four-bit
samples become fixed arrays, while one-bit samples use an immutable 2 KiB table.
Independent bit-at-a-time tests cover every byte, partial groups, offsets,
padding, short input and extra input.

The inflater hot loop removes 21 lines of speculative four-literal lookups and
stores. One checked literal store feeds the existing next-entry lookup/refill
pipeline; the bounded fallback keeps its existing batching and guards. The new
maximum-token fixture covers bit alignments, output slack, overlapping history,
prefix consumption and fragmented IDAT input.

Stored blocks now append directly into reserved capacity until the first Huffman
block needs an initialized output slice. This removes redundant zeroing of stored
payload bytes while preserving match history, limits and unsized metadata budget
accounting. Reservation sizes and measured allocation counts do not increase.

The recorded one-bit table candidate comparison improved all ten targeted cases,
from 5,438,497 to 4,886,381 cycles. Both packed-unpack comparisons had the temporary
fixed-Huffman table candidate on both sides. The preceding one-bit implementation
in the table comparison is not independently established as the original loop or
the arithmetic fixed-group candidate, so this is not claimed as a proven
incremental gain over the latter. Those temporary fixed-Huffman tables were
subsequently removed. The archive also records the partially reconstructed
packed-length candidate lineage. The final full-corpus comparison uses frozen
final sources and contains no rejected candidates.

All four retained optimizations are safe Rust. There are no new production
dependencies, unsafe blocks, public API changes or CPU-dispatch requirements.
The benchmark-only flate2 dev dependency lets the stage diagnostic compile the
production module's independent regression oracle; it changes no resolved
package version or production dependency.

Measured candidates removed from production were SSE4.1 stride-2 specialization
(less than 1% additional benefit), fixed-Huffman wide tables (the apparent gain
did not survive repetition), packed length descriptors (about 1.6% worse total
cycles), fused Up reconstruction/compaction (no total benefit), and the arithmetic
one-bit expansion superseded by the table. A contiguous-chunk CRC proposal was
prepared but never applied or measured; it is not a measured rejection.

## Raw-output diagnostic

A separate 40 ms/batch diagnostic retains the original output APIs. Native raw
decoding falls from 816,775,041 to 772,038,149 total cycles (5.5% fewer) and reaches
156.02 MP/s. Its final total-cycle ratios are 0.9096 versus normalized png, 0.8866
versus image, and 0.9123 versus png's packed-byte output. Each corpus group also
has lower native total cycles in this diagnostic. The API representations differ,
so these numbers are supplementary and are not substituted for the repeated
equivalent-RGBA8 gate above.

## Allocations

The separate instrumented executable reports identical before/after native
allocation counts and requested bytes. The improvements remove CPU and memory
write work; they do not reduce the allocator request totals.

| Equivalent RGBA8, one decode per input | Allocation/reallocation calls | Cumulative requested bytes |
| --- | ---: | ---: |
| Native, before and final | 548 | 265,945,823 |
| png asset path | 1,533 | 290,155,337 |
| image | 1,670 | 288,171,969 |

Native makes 64.3% fewer calls than png and 67.2% fewer than image. Its cumulative
requested-byte totals are 8.3% and 7.7% lower, respectively. These are corpus
totals: native requests more bytes on 11 inputs versus png and five versus image.
Requested bytes count replacement sizes on reallocation; they are not peak live
memory or RSS. Allocation instrumentation is excluded from the timing executable.

## Compatibility correction and validation

Commit `3fc2deedb61f6655641f8f235c32ec5b53a0e7e9` separately fixes stale
unaccounted prefetch bits before a direct stored-block copy. A valid mixed
stored/Huffman stream previously failed with `Checksum`; the same independently
constructed stream was accepted by flate2. The regression was reproduced with
the original production algorithm before applying the correction. This is an
intentional compatibility fix alongside the behavior-preserving optimizations.

Final validation:

- 174 release tests passed, zero failed; 21 existing external-oracle tests remain
  ignored. The log has 44 suite result footers, including documentation tests.
- All 1,132 supplied PNG paths matched png native samples, image RGBA16 output,
  all three RGBA8 adapters, and the complete native buffered/reader results,
  including metadata.
- 22 release library tests passed on `i686-pc-windows-msvc`.
- The library compiled for `aarch64-unknown-linux-gnu`; this was not ARM execution
  or a speed measurement.
- Strict Clippy passed for the PNG crate and all standalone benchmark targets,
  with both default and all features. Scoped rustfmt and diff checks passed.

No new Miri or sanitizer result is claimed. Historical sanitizer evidence remains
attached to its earlier source hashes in the November 17 archive.

## Measurement scope and reproduction

Host: Windows x86-64, Intel Xeon E5-2696 v4, 22 cores / 44 logical processors.
Toolchain: Rust 1.100.0 (`0ef1bc386`, 2026-11-09), LLVM 23.1.5. The standalone
workspace uses release opt-level 3, full LTO, one codegen unit, panic abort and
the repository's static CRT flag, without `target-cpu=native`. Reference versions
are pinned to png 0.18.1 and image 0.25.10.

Each final RGBA8 process measures five batches of at least 80 ms per image/codec,
with thread affinity set to logical CPU 0. Windows `QueryThreadCycleTime` and
wall-time medians are selected independently. Codec order rotates between files.
Builds and validation finished before final timing; process records show no
compiler at the beginning or end of each run. These boundary checks are not
continuous monitoring of other software on the host.

The comparison includes decoding and direct conversion to identical RGBA8 pixels.
The png adapter mirrors the production asset loader with color keying disabled.
Native retains complete CRC, metadata and stream validation; the png asset path
stops after its first `next_frame`, as the loader does. Pixel equivalence on this
corpus does not establish identical malformed-input or resource-limit behavior.
A future native loader integration must preserve the consumer's final RGBA
allocation guard and color-key behavior.

Disk I/O, color keying, resizing, texture upload, streaming/APNG throughput and
other CPUs are outside this performance claim. The [benchmark README](README.md)
defines the adapters, counters and commands. The
[archive manifest](results/windows-x86_64-20261119-rgba8/manifest.json) retains raw
measurements, source/binary/input hashes, candidate decisions and validation logs.
The earlier [November 17 buffered-decoder results](results/windows-x86_64-20261117-pipelined/manifest.json)
remain separate; they use a smaller corpus and different output comparisons.
