# Wuffs decode parity measurements

Wuffs writes into the requested pixel format during decoding. Its BGR24 path
uses an SSE byte shuffle for four pixels at a time on this build. Indexed
8-bit pixels are expanded directly through a prepared four-byte palette, with
four lookups per iteration. [Pinned Wuffs BGR conversion](https://github.com/google/wuffs/blob/a87f0bb85b9406d29f48a1c03b6f42c4585145f6/release/c/wuffs-v0.4.c#L29284)
and [indexed conversion](https://github.com/google/wuffs/blob/a87f0bb85b9406d29f48a1c03b6f42c4585145f6/release/c/wuffs-v0.4.c#L30071).

The first direct `decode_rgba8` implementation fused 24-bit channel conversion
and alpha expansion. It preserved the existing `decode` API for callers that
need the palette or native pixel layout. The second milestone groups BGR24 by
three input words and writes opaque 32-bit bitfields and 8-bit palette input
straight into RGBA. The short-palette path checks used indices.

The full 203-file corpus was run twice with `decode-wuffs`, reversing the
second pass. The benchmark used logical CPU 10 on the same Xeon E5-2696 v4 as
the [contender comparison](../windows-x86_64-20261126-contenders/README.md),
with five samples of at least 10 ms per measurement. Both Deadlib paths
returned the same verification status on all 203 files. The table gives the
sum of geometric-mean cycle counts divided by the Wuffs sum for matching
cases. Ratios below one favor Deadlib.

| Cohort | `decode` + `into_rgba8` | `decode_rgba8` |
| --- | ---: | ---: |
| All 155 shared exact cases | 0.375 | 0.307 |
| Generated RGB24, all sizes | 1.522 | 0.754 |
| Kodak RGB24 | 1.317 | 0.657 |
| Full 8-bit palettes | 0.919 | 0.735 |
| Short 8-bit palettes | 0.911 | 0.817 |
| Opaque 32-bit bitfields | 1.022 | 0.478 |
| BMP Suite 40-byte RGB24 | 3.416 | 1.542 |
| BMP Suite 124-byte RGB24 | 3.588 | 1.550 |

The direct decoder was within 1% or faster on 123 of 155 shared exact cases.
The largest remaining case ratio was 1.685 for 511×257 top-down RGB24.
The 511×257 and suite RGB24 cases still favor Wuffs. Some 8-bit palette and
32-bit alpha cases also exceed the 1% target. [Raw milestone passes](direct-word-groups.tsv.gz).

## x86-64 RGB24 shuffle

The SSSE3 kernel reduced the suite RGB24 ratios from roughly 1.54 to 0.65
on two focused repetitions. For all 26 matched RGB24 cases, 25 were within
1% or faster on the combined cycle estimate. The 511×257 top-down image
varied between 3.016× and 0.957× across the passes. The complete-pixel and
padding checks also exercised the scalar path and the SIMD tail.
[Raw RGB24 passes](direct-ssse3-rgb24.tsv.gz).
