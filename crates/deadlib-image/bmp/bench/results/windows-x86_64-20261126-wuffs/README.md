# Wuffs decode parity measurements

The final `decode_rgba8` build met the 1% cycle target on **all 155 original
byte-exact Wuffs pairs** on the Windows x64 Xeon E5-2696 v4. Its highest
median paired ratio was **0.9898×** Wuffs cycles. See the final section for
the five full passes and reproducible validation.

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

## Eight-pixel shuffle and grouped palette lookups

AVX2 processes eight BGR or BGRA pixels per shuffle. SSSE3 and portable Rust
paths remain available. The ordinary `decode` API also uses the new kernel for
standard explicit BGRA masks. Indexed input uses a four-pixel lookup group and
fixed-width palette reads. On two full passes, 150 of 155 shared exact cases
met the 1% target using the median paired ratio. The largest remaining gap was
6.2%, on the suite's OS/2 v2 40-byte indexed file. Three other indexed cases
and top-down 1920×1080 RGB24 also exceeded 1%. [Raw AVX2 passes](direct-avx2.tsv.gz).

## Direct packed and small-palette expansion

The 8-bit scalar path fills the final raster in place. Full 1-bit and 4-bit
palettes expand packed source bytes through a lookup table. AVX2 expands
8-bit index streams with palettes of up to 16 colors by shuffling each color
plane and interleaving the four output bytes. It validates the index range
while processing the raster.

Five 8 ms passes preserved all 155 original exact-output Wuffs pairs and met
the 1% target on 152. The only remaining cases were top-down 1920×1080 RGB24,
RGBA32, and full-palette indexed8 at 1.0252×, 1.0257×, and 1.0235× Wuffs cycles.
Both Deadlib entry points agreed byte for byte on all 174 accepted inputs and
returned the same errors on the other 29. [Raw palette passes](direct-palettes.tsv.gz).

## Large tightly packed output

For top-down, tightly packed inputs that produce at least 4 MiB of RGBA,
the AVX2 path can stream complete cache lines into the newly allocated
raster. It handles alignment and trailing pixels separately, and completes
the streaming stores before returning the buffer. The three remaining
1080p cases used median paired ratios of 0.873× for indexed8, 0.806× for
RGB24, and 0.770× for RGBA32 across five focused passes with 20 ms samples.
[Raw focused large-image passes](direct-streaming-large.tsv.gz).

The five full passes put all three 1080p cases below the target and met it
on 154 of 155 exact pairs. The remaining 511×257 top-down full-palette
indexed8 input had a median ratio of 1.0113×. [Raw full streaming passes](direct-streaming.tsv.gz).

## Final full-palette row loop

The final decoder constructs full palette tables directly, checks short
palettes before the scalar expansion loop, and selects row direction once
per raster. The last indexed8 case moved from 1.0113× to 0.9860× Wuffs cycles.

| Input | Median Deadlib cycles / Wuffs cycles |
| --- | ---: |
| OS/2 CORE indexed8 511×257 bottom-up, highest ratio in the final corpus | 0.9898× |
| Indexed8 511×257 top-down | 0.9860× |
| Indexed8 1920×1080 top-down | 0.8800× |
| RGB24 1920×1080 top-down | 0.8273× |
| RGBA32 1920×1080 top-down | 0.7665× |

The final comparison used five full passes over 203 files with at least 8 ms
per sample. It retained exactly the original 155 matching pairs. The direct
and staged Deadlib APIs agreed on every decoded byte and every error in the
corpus. The performance gate uses the median of the five paired cycle ratios
for each file. Individual pass ratios fluctuate. Windows x64 and x86 crate
tests passed, including explicit AVX2 and SSSE3 tail checks on x64. RLE and
other files without a byte-exact pair are recorded in the data with their
verification outcomes.

[Raw final passes](direct-final.tsv.gz) · [Reproduction and parity checker](../../contenders/README.md)
