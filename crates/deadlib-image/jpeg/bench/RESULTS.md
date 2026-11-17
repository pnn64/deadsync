# Decoder results, 2026-11-17

Windows x86-64, Intel Xeon E5-2696 v4 (Broadwell, AVX2), Rust 1.100.0,
release/LTO, baseline `x86-64` target with runtime AVX2 dispatch, logical CPU 0.
References: zune-jpeg 0.5.15 (default features, runtime AVX2), jpeg-decoder
0.3.2 and image 0.25.10 (zune-jpeg backend).

Two independent processes each measured five batches of at least 60 ms per
image/codec. Medians of process cycles are reported. Other builds were running
on the host, so single-file ratios vary by several percent between runs.

Ratios below 1 favor native decoding. Each value is the geometric mean of both
runs' per-image process-cycle ratios.

| Corpus | Images | Native / zune-jpeg | Native / image | Native / jpeg-decoder |
| --- | ---: | ---: | ---: | ---: |
| all | 31 | 0.755 | 0.634 | 0.444 |
| assets | 5 | 0.869 | 0.853 | 0.607 |
| sequential fixtures | 13 | 0.692 | 0.485 | 0.345 |
| progressive fixtures | 13 | 0.779 | 0.738 | 0.505 |

The overall native/zune-jpeg ratios in the individual runs are 0.753 and 0.756.
Native is faster in both runs on 28 of 31 files against zune-jpeg and 30 of 31
against image and jpeg-decoder. The exceptions are within run-to-run noise: the
31x25 progressive CMYK fixture (1.050x zune-jpeg, 1.018x image), the 31x25
sequential multi-scan fixture (1.059x zune-jpeg) and the sparse 503x446 font
texture (0.952x zune-jpeg on average, slower in one run). The 1920x1080 shop
photographs measure 0.82-0.91x zune-jpeg.

| Allocator requests across 31 decodes | Calls | Cumulative requested bytes |
| --- | ---: | ---: |
| native | 147 | 26,308,980 |
| zune-jpeg | 497 | 27,199,774 |
| image | 619 | 30,014,963 |
| jpeg-decoder | 26,019 | 119,728,072 |

Native requests fewer bytes than every reference on all 31 files and makes
fewer allocation calls on 30; the 31x25 grayscale progressive fixture ties
zune-jpeg at four calls (with fewer bytes).

**The decoder benchmark target is met on this Windows x86-64 host**: native
uses fewer cycles than zune-jpeg, image and jpeg-decoder in every corpus group
in both runs, and fewer allocation calls and bytes overall. These are corpus
averages; three individual files remain within noise of zune-jpeg.

| Progress on this corpus (native / zune-jpeg) | Geomean |
| --- | ---: |
| start of this iteration | 1.467 |
| SSE2-only, unsafe forbidden ([data](results/windows-x86_64-20261117/)) | 0.908 |
| runtime AVX2 dispatch (this report) | 0.755 |

How it gets there:

- 8-bit samples use libjpeg's float AAN IDCT (`jidctflt`) in its exact
  operation order, so libjpeg-turbo and mozjpeg `JDCT_FLOAT` reference planes
  match exactly. An explicit AVX2 kernel transforms eight columns per register
  with a shuffle transpose; DC-only blocks fill directly, and the portable
  path keeps a reduced transform for top-left-4x4 blocks.
- An eight-byte Huffman bit reader with a combined AC run/magnitude lookup,
  built once per DHT, serves every sequential scan. Markers and the end of
  data append virtual zero bits whose consumption is a sticky truncation
  error, so acceptance matches the strict reader on all mutation tests. On
  the shop photo, entropy decoding takes about 16M cycles versus zune-jpeg's
  21M.
- YCbCr conversion runs in 64-pixel chunks whose 32-bit fixed-point products
  decompose exactly into 16-bit `pmulhw`/`pmaddwd` shapes (all 2^24 inputs
  tested in both builds). An explicit AVX2 color kernel was measured and
  dropped: LLVM's AVX2 compilation of the portable code was faster.
- Hot kernels implement `simd::Kernel` with `#[inline(always)]` bodies;
  `simd::dispatch` runs them inside an AVX2-compiled wrapper after
  `is_x86_feature_detected!`. Production code denies `unsafe` outside the
  `simd` module, whose loads read fixed-size arrays and whose stores target
  checked slices.
- Every 8-bit DCT process reconstructs into a one- or three-row MCU window;
  progressive and multi-scan images skip full-resolution planes, and pixel
  decoding keeps color hints instead of metadata segments.

Pixel diagnostics are unchanged from the original native pipeline across all
93 file/reference pairs. The 4:1:0/4:1:1 fixtures differ from every reference
by design (centered linear interpolation versus replicated samples), and
zune-jpeg/image mis-decode the sequential multi-scan fixture.

Validation: 176 tests pass, including official ITU T.83 streams, C-oracle
coefficient and sample fixtures, truncation/mutation acceptance against the
strict decoder, sparse and dispatched IDCT equivalence tests (200,000 and
100,000 blocks), and the exhaustive color test through both dispatch modes.
Strict Clippy passes for the crate and the benchmark workspace. Sanitizer
fuzzing, CPUs without AVX2 (beyond the forced-portable tests) and other
architectures were not run in this iteration.

Raw data: [timing run 1](results/windows-x86_64-20261117-avx2/timing-run1.tsv),
[timing run 2](results/windows-x86_64-20261117-avx2/timing-run2.tsv),
[allocations](results/windows-x86_64-20261117-avx2/allocations.tsv) and
[pixel verification](results/windows-x86_64-20261117-avx2/verification.log).
[Reproduction instructions](README.md) describe the measurement definitions.
