# JPEG codec comparison on Windows x86-64

Across 53 common images, Deadlib used 23% more cycles than default
libjpeg-turbo and 4% fewer than Zune. It led the four CMYK photos. On nine
4:2:0 photo and capture sources, Deadlib's default optimized-Huffman encoder
used 15.63× the cycles of optimized Turbo at Q75 and 13.87× at Q90. Total
output bytes differed by less than 0.5% and mean RGB PSNR by 0.02 dB.

Measured on 2026-11-26 on an AVX2 Intel Xeon E5-2696 v4, Windows 11 Pro build
26100, and logical CPU 2. The Rust release used 1.100.0 with LTO. The C and C++
release builds used MSVC 19.44 and NASM 3.01. Turbo and MozJPEG included NASM SIMD,
JPEGli used Highway, and sjpeg, Wuffs and jpgd used their enabled x86 SIMD
configurations. The upstream C/C++ sources and Rust package versions are in the
[source pins](../../contenders/upstream.tsv) and [Cargo lockfile](../../Cargo.lock).

## Decode

Ratios are Deadlib cycles divided by the other codec's cycles on the shared inputs. A ratio below 1.00× favors Deadlib.

| Contender on 53 common images | Cycle ratio | Wall-time ratio | Deadlib wins |
| --- | ---: | ---: | ---: |
| libjpeg-turbo-fast | 1.24× | 1.24× | 1/53 |
| libjpeg-turbo | 1.23× | 1.23× | 2/53 |
| mozjpeg | 1.23× | 1.23× | 1/53 |
| zenjpeg | 1.11× | 1.12× | 29/53 |
| zune-jpeg | 0.96× | 0.95× | 35/53 |
| image | 0.95× | 0.96× | 36/53 |
| jpeg-decoder | 0.79× | 0.85× | 51/53 |
| jpegli | 0.59× | 0.59× | 52/53 |
| wuffs | 0.59× | 0.59× | 53/53 |
| jpgd | 0.53× | 0.52× | 53/53 |

### Ordinary decode by image type

| Input type | Cases | Deadlib / Turbo | Deadlib / Zune | Fastest over the complete group |
| --- | ---: | ---: | ---: | ---: |
| Kodak baseline 4:2:0 | 24 | 1.15× | 0.85× | libjpeg-turbo-fast |
| Kodak progressive 4:2:0 | 24 | 1.27× | 1.05× | zenjpeg |
| Game assets | 5 | 1.21× | 0.87× | libjpeg-turbo-fast |
| Grayscale | 4 | 0.91× | 0.71× | wuffs |
| Direct RGB 4:4:4 | 4 | 1.06× | 0.82× | libjpeg-turbo-fast |
| CMYK | 4 | 0.70× | 0.78× | deadlib |
| YCbCr 4:4:4 | 1 | 1.01× | 0.81× | libjpeg-turbo-fast |
| YCbCr 4:2:2 | 1 | 1.19× | 0.92× | libjpeg-turbo-fast |
| YCbCr 4:4:0 | 1 | 0.85× | 0.88× | deadlib |
| YCbCr 4:1:1 | 1 | 2.20× | 1.92× | libjpeg-turbo-fast |
| YCbCr 4:1:0 | 1 | 2.53× | 1.90× | libjpeg-turbo-fast |
| YCCK fixtures | 6 | 0.78× (2) | 0.64× (1) | deadlib |

### Specialized decode coverage

Counts indicate successful packed raster output under the verification rule. The cycle multiplier uses only the shared Deadlib and default Turbo cases.

| Fixture family | Files | Deadlib | Turbo | MozJPEG | Paired with Turbo | Deadlib / Turbo |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sequential Huffman DCT | 25 | 25 | 25 | 25 | 25 | 0.91× |
| Progressive Huffman DCT | 25 | 25 | 25 | 25 | 25 | 1.09× |
| Sequential arithmetic DCT | 15 | 15 | 15 | 15 | 15 | 0.92× |
| Progressive arithmetic DCT | 15 | 15 | 15 | 15 | 15 | 0.97× |
| Predictive lossless Huffman | 5 | 5 | 5 | 3 | 5 | 1.18× |
| Predictive lossless arithmetic | 5 | 5 | 0 | 0 | 0 | — |
| DNL | 6 | 6 | 0 | 0 | 0 | — |
| Hierarchical | 6 | 6 | 0 | 0 | 0 | — |
| Hierarchical native | 6 | 6 | 0 | 0 | 0 | — |
| Pyramid native | 6 | 6 | 0 | 0 | 0 | — |
| ITU T.83 standalone-raster attempts | 20 | 2 | 17 | 17 | 0 | — |

Across all 206 raster attempts, the accepted counts were deadlib 188, libjpeg-turbo 174, mozjpeg 172, libjpeg-turbo-fast 146, zenjpeg 103, jpegli 100, jpeg-decoder 97, image 96, zune-jpeg 96, wuffs 95, jpgd 74.

## Encode

Q75 and Q90 below use the same nine photo and game-capture rasters in 4:2:0.
Each comparison uses Deadlib's default optimized-Huffman encoder unless marked fixed.
Size and quality columns give the Deadlib minus contender difference at the nominal quality setting.

| Contender | Q75 cycles | Q75 bytes | Q75 PSNR | Q90 cycles | Q90 bytes | Q90 PSNR |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| turbo-fixed | 32.13× | -5.4% | -0.02 dB | 30.19× | -4.4% | -0.02 dB |
| sjpeg-fixed | 24.84× | -3.4% | +0.05 dB | 20.69× | -2.5% | +0.05 dB |
| sjpeg-opt | 21.82× | +2.2% | +0.05 dB | 17.58× | +1.6% | +0.05 dB |
| turbo-opt | 15.63× | -0.2% | -0.02 dB | 13.87× | -0.4% | -0.02 dB |
| jpegli | 9.68× | +13.6% | -0.22 dB | 9.06× | +14.2% | -0.12 dB |
| zenjpeg | 8.18× | +15.8% | +0.01 dB | 8.06× | +17.1% | +0.17 dB |
| jpeg-encoder | 7.58× | -0.3% | +0.54 dB | 7.08× | -0.8% | +0.90 dB |
| mozjpeg | 2.10× | +19.2% | +0.66 dB | 1.53× | +13.9% | +0.81 dB |
| turbo-fixed, both encoders fixed | 32.20× | +79.5% | -0.02 dB | 29.80× | +71.5% | -0.02 dB |

### Other encoder configurations against optimized Turbo

| Configuration | Paired cases | Cycle ratio | Wall-time ratio | Size delta | PSNR delta |
| --- | ---: | ---: | ---: | ---: | ---: |
| Q85 4:4:4 RGB | 3 | 12.36× | 12.06× | -0.4% | +0.00 dB |
| Q85 4:2:2 | 3 | 11.91× | 11.65× | -0.4% | -0.03 dB |
| Q85 4:4:0 | 3 | 7.68× | 7.56× | -0.4% | -0.01 dB |
| Q85 4:1:1 | 3 | 8.37× | 8.41× | -0.4% | +0.00 dB |
| Q85 4:1:0 | 3 | 9.18× | 9.03× | -0.4% | -0.00 dB |
| Q85 gray | 3 | 7.53× | 7.61× | -0.4% | -0.00 dB |
| Q85 progressive | 3 | 5.46× | 5.43× | +5.9% | -0.01 dB |
| Q85 arithmetic | 3 | 3.89× | 3.90× | -0.3% | -0.01 dB |
| Q85 12-bit gray | 1 | 5.08× | 5.01× | -0.0% | +0.00 dB |
| 8-bit lossless gray | 3 | 1.75× | 1.73× | -0.0% | exact |
| 16-bit lossless gray | 1 | 2.67× | 2.81× | -0.0% | exact |
| Q75 synthetic gradient | 2 | 21.58× | 21.15× | +0.4% | +0.25 dB |
| Q75 synthetic noise | 2 | 7.90× | 7.91× | +0.1% | -0.00 dB |

The progressive encoders use their own scan scripts. The encode size delta
is calculated from total bytes. PSNR deltas are the unweighted mean over the
paired rasters.

Across all 51 cases, the verified encode counts were deadlib-fixed 51, deadlib-opt 51, turbo-fixed 51, turbo-opt 51, mozjpeg 46, jpeg-encoder 43, jpegli 43, zenjpeg 37, sjpeg-fixed 25, sjpeg-opt 25, image 6.

## Coverage and timing

The decoder corpus contains 207 JPEG files, including 206 with frame headers.
Kodak photos and five game assets anchor the common-image
comparison. The remainder include grayscale, direct RGB, YCbCr with each of
the six tested sampling ratios, CMYK, YCCK, extended and progressive Huffman
DCT, arithmetic DCT, predictive lossless, DNL, hierarchical and ITU T.83
fixtures. T.83 B1 is a tables packet. B2 requires external tables and exposes
255 components, outside the packed gray/RGB comparison.
Seventeen other T.83 fixtures fell below the packed RGB agreement threshold
against Turbo. Their timings are excluded from Deadlib/Turbo pairs.
All three [native T.83 tests](../../../tests/t83.rs) passed, including the
exact coefficient and lossless source-plane checks.

The encoder corpus contains 51 configurations over 21 source rasters. It
includes Kodak photos, game captures, synthetic gradients and noise, six
chroma sampling ratios, grayscale, progressive, arithmetic, 12-bit extended
DCT, and 8-bit and 16-bit predictive lossless. The local screenshot is
identified by the SHA-256 of its source raster. The two other captures are from
the fixture submodule listed in the build record.

Two independent process runs cover the full corpus, with five timing batches
and at least 8 ms per batch. The game assets have a third pass after a wall-time
spike in the second run. Their reported results use the median of all three
passes. Timing includes allocation
and destruction, with input reading and pixel checks outside measured time.
Lossy decode rows require at least 28 dB agreement with default Turbo, or with
Deadlib on processes Turbo cannot render. Self-referenced Deadlib rows measure
decode completion and raster geometry only. All common-image rows use Turbo
as the pixel reference. Encoder size and PSNR correspond to nominal quality factors
and native quantization choices. Lossless encoding requires exact samples.
See the [reproduction commands and verification rules](../../contenders/README.md).

## Archive

- [Decode run 1](decode-run1.tsv.gz) and [decode run 2](decode-run2.tsv.gz)
- [Game-asset decode repeat](decode-assets-repeat.tsv.gz)
- [Encode run 1](encode-run1.tsv.gz) and [encode run 2](encode-run2.tsv.gz)
- [Decode corpus](decode-inputs.tsv), [encode configurations](encode-inputs.tsv),
  and [input SHA-256 hashes](inputs.json)
- [Build and machine identity](build.json)

Run [summarize.py](../../contenders/summarize.py) on each pair of compressed
TSVs for a JSON breakdown by input class and JPEG process. Pass
`--repeat-subset decode-assets-repeat.tsv.gz` for the decode summary. The TSVs include
per-file CPU cycles, wall time, input and output bytes, verification metrics,
and reasons for exclusions.
