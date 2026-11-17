# Decoder results, 2026-11-17

Windows x86-64, Intel Xeon E5-2696 v4 (Broadwell; AVX2 available, unused by
native code), Rust 1.100.0, release/LTO, baseline `x86-64` target (SSE2), logical
CPU 0. References: zune-jpeg 0.5.15 (default features, runtime AVX2),
jpeg-decoder 0.3.2 and image 0.25.10 (zune-jpeg backend).

Two independent processes each measured five batches of at least 60 ms per
image/codec. Medians of process cycles are reported. Other builds were running
on the host, so single-file ratios vary by several percent between runs;
compare ratios within a run rather than absolute cycles.

Ratios below 1 favor native decoding. Each value is the geometric mean of both
runs' per-image process-cycle ratios.

| Corpus | Images | Native / zune-jpeg | Native / image | Native / jpeg-decoder |
| --- | ---: | ---: | ---: | ---: |
| all | 31 | 0.908 | 0.782 | 0.528 |
| assets | 5 | 1.269 | 1.268 | 0.911 |
| sequential fixtures | 13 | 0.865 | 0.642 | 0.422 |
| progressive fixtures | 13 | 0.837 | 0.792 | 0.536 |

The overall native/zune-jpeg ratios in the individual runs are 0.911 and 0.904.
Native is faster in both runs on 22 of 31 files against zune-jpeg, 24 against
image and 29 against jpeg-decoder. At the start of this iteration the same
corpus measured 1.467, 1.241 and 0.858.

| Allocator requests across 31 decodes | Calls | Cumulative requested bytes |
| --- | ---: | ---: |
| native | 147 | 26,308,980 |
| zune-jpeg | 497 | 27,199,774 |
| image | 619 | 30,014,963 |
| jpeg-decoder | 26,019 | 119,728,072 |

Native requests fewer bytes than every reference on all 31 files and makes
fewer allocation calls on 30; the 31x25 grayscale progressive fixture ties
zune-jpeg at four calls (with fewer bytes).

**Remaining gap: larger 4:2:0 images.** The four 1920x1080 shop photographs
remain 1.21-1.23x zune-jpeg, and the sparse 503x446 font texture 1.50x. A
sampling profile of a shop photo shows native entropy decoding already ahead
(about 16M cycles versus zune-jpeg's 21M), while zune-jpeg's runtime-dispatched
AVX2 IDCT (about 7M versus 16M) and color conversion (about 9M versus 15M)
account for the difference. On the texture almost every block is sparse, so
color conversion (about 7 cycles per pixel, with three-channel interleaving
costly without SSSE3 shuffles) dominates. The crate forbids `unsafe`, so these
stages rely on SSE2 autovectorization; runtime AVX2 dispatch as in the PNG
crate would require relaxing that policy.

Changes in this iteration:

- 8-bit samples use libjpeg's float AAN IDCT in its exact operation order;
  libjpeg-turbo and mozjpeg `JDCT_FLOAT` reference planes now match exactly
  (previously within one level). DC-only and top-left-4x4 blocks use exactly
  reduced transforms.
- An eight-byte Huffman bit reader with a combined AC run/magnitude lookup,
  built once per DHT, serves every sequential scan. Markers and the end of
  data append virtual zero bits whose consumption is a sticky truncation
  error, so acceptance matches the strict reader on all mutation tests.
- YCbCr conversion runs in 64-pixel chunks that SSE2 vectorizes: the 32-bit
  fixed-point products decompose exactly into `pmulhw`/`pmaddwd` shapes (all
  2^24 inputs tested). Other sampling grids interpolate incrementally.
- Every 8-bit DCT process reconstructs into a one- or three-row MCU window;
  progressive and multi-scan images skip full-resolution planes, and pixel
  decoding keeps color hints instead of metadata segments.

Pixel diagnostics are unchanged from the original native pipeline across all
93 file/reference pairs (no RMSE increase). The 4:1:0/4:1:1 fixtures differ from
every reference by design (centered linear interpolation versus replicated
samples), and zune-jpeg/image mis-decode the sequential multi-scan fixture.

Validation: 175 tests pass, including official ITU T.83 streams, C-oracle
coefficient and sample fixtures, truncation/mutation acceptance against the
strict decoder, a 200,000-block sparse-IDCT equivalence test and the
exhaustive color test. Strict Clippy passes for the crate and the benchmark
workspace. Sanitizer fuzzing and other architectures were not run in this
iteration.

Raw data: [timing run 1](results/windows-x86_64-20261117/timing-run1.tsv),
[timing run 2](results/windows-x86_64-20261117/timing-run2.tsv),
[allocations](results/windows-x86_64-20261117/allocations.tsv) and
[pixel verification](results/windows-x86_64-20261117/verification.log).
[Reproduction instructions](README.md) describe the measurement definitions.
