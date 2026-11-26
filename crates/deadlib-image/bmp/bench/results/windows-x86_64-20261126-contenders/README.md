# BMP decoder and encoder benchmark

**Deadlib led on encoding and several decode formats.** It had the lowest
decode cycle count on 90 of its 163 exact-output cases. With a 3% band against
the best exact-output rival, Deadlib won 87 cases, was within the band on 13,
and lost 63. On the 54 files all 11 decoders matched, Windows WIC used 7.3%
fewer total cycles than Deadlib, which placed second. Deadlib won 25 of 29
encode cases by more than 3%, with two within the band and two losses.

Measured 2026-11-26 on an Intel Xeon E5-2696 v4 and Windows 11 Pro
24H2 (build 26100.8457), using the Deadlib code at `986bb6b4756b`. The
benchmark used one pinned logical processor. Rust 1.100.0 ran the default
Windows x64 release build. The C and C++ bridge for Wuffs, stb, and libnsbmp
used MSVC 19.44 and `/arch:AVX2`. SDL3, FFmpeg, OpenCV, and Pillow used their
pinned binary distributions. OpenCV and Pillow timings include their Python
3.13 API overhead. All labels refer to the measured builds and API paths.

## Decode throughput

Each row uses the same images for Deadlib and its fastest eligible rival. A
decoder qualifies for a row when it matches the reference RGBA8 output on
every case in that row. The synthetic rows use 511×257 and 1920×1080 images,
with both row directions where the format allows them. Numbers are geometric
mean megapixels per second. The two generated RLE rows cover literal and
replicated runs at both sizes.

| BMP input | Cases | Deadlib MP/s | Fastest other exact decoder | MP/s |
| --- | ---: | ---: | --- | ---: |
| 1-bit indexed | 4 | **746.6** | Wuffs | 546.5 |
| 4-bit indexed | 4 | **654.4** | Wuffs | 529.5 |
| 8-bit indexed | 4 | 731.6 | Wuffs | **888.7** |
| 8-bit indexed, short palette | 4 | 703.7 | Wuffs | **831.5** |
| OS/2 CORE, 8-bit indexed | 2 | 728.5 | Wuffs | 738.1 |
| OS/2 CORE, RGB24 | 2 | 372.9 | Wuffs | **844.5** |
| RGB24, generated | 4 | 364.8 | Wuffs | **981.3** |
| RGB24, Kodak photos | 8 | 238.7 | Windows WIC | **364.4** |
| RGB555 | 4 | 377.6 | FFmpeg | **428.2** |
| RGB565 | 4 | 364.5 | SDL3 | **547.2** |
| RGB333 masks | 4 | **283.1** | image | 97.1 |
| RGBA4444 masks | 4 | **709.1** | oxideav-bmp | 160.1 |
| BGRX32, opaque bitfields | 4 | 350.7 | Windows WIC | **445.5** |
| RGBA32, explicit masks | 4 | **991.8** | Wuffs | 890.2 |
| RGBA32, swapped masks | 4 | **1,137.8** | SDL3 | 874.3 |
| BGRA32, implicit alpha | 4 | **985.1** | SDL3 | 260.9 |
| BGRA32, all-zero implicit alpha | 4 | **810.8** | Windows WIC | 405.0 |
| RLE4, generated | 4 | Unsupported | Wuffs | 499.4 |
| RLE8, generated | 4 | Unsupported | libnsbmp | 506.1 |

The RLE4 and RLE8 leaders apply to these generated streams. The eight Kodak
images are 768×512 or 512×768 RGB24. OpenCV reached 241.0 MP/s on that cohort,
within 1% of Deadlib. The common-corpus cycle total weights larger images more
heavily. The individual case counts give every file equal weight, including
1×1 and 17×9 cases.

## Decode coverage and pairwise comparison

The corpus contains 203 inputs. It includes 120 generated inputs with exact
expected pixels, 67 BMP Suite files, eight additional generated RLE inputs,
and eight converted Kodak photos. The suite adds V2 through V5, OS/2 v1 and
v2, bitmap arrays, 2 bpp, alternate or wide masks, 64 bpp, RLE24,
Huffman 1D, color profile headers, and BMPs containing embedded PNG or JPEG.
[BMP Suite 2.8 test gallery](https://entropymine.com/jason/bmpsuite/bmpsuite/html/bmpsuite.html).

Deadlib matched all 120 generated native layouts, 35 of 67 BMP Suite files,
and all Kodak files. It rejected 29 files across the full corpus, including
RLE4/RLE8, 2 bpp, OS/2 v2 extended headers, embedded JPEG/PNG, 64 bpp,
BI_ALPHABITFIELDS, and bitfield channels wider than 8 bits. It returned
different pixels on 11 suite files, including RGB555/565, implicit alpha,
and explicit-alpha renderings. The synthetic reference uses bit repetition
for narrow channels. The suite reference uses its published renderings where
applicable. The two references can differ in sample scaling or alpha handling.
The raw log distinguishes rejected files from accepted files with different
pixels. Wuffs was measured with its BMP module, which reports redirection for
embedded JPEG and PNG. This path did not follow those redirects.

The speed ratio is the geometric mean of competitor cycles divided by Deadlib
cycles on the exact matches shared by that pair. Counts use a 3% band. The
matched sets differ between rows.

| Decoder | Exact / 203 | Shared exact | Deadlib wins / within 3% / losses | Deadlib cycle speed |
| --- | ---: | ---: | ---: | ---: |
| Wuffs | 174 | 155 | 103 / 4 / 48 | 1.96× |
| SDL3 | 161 | 152 | 140 / 4 / 8 | 4.34× |
| image | 160 | 140 | 140 / 0 / 0 | 3.25× |
| oxideav-bmp | 160 | 145 | 145 / 0 / 0 | 2.03× |
| FFmpeg | 156 | 144 | 131 / 5 / 8 | 11.83× |
| Windows WIC | 142 | 130 | 109 / 5 / 16 | 4.68× |
| Pillow | 134 | 124 | 124 / 0 / 0 | 35.55× |
| libnsbmp | 131 | 118 | 108 / 5 / 5 | 2.41× |
| OpenCV | 111 | 98 | 95 / 0 / 3 | 8.04× |
| stb_image | 99 | 98 | 98 / 0 / 0 | 3.28× |

## Encode throughput

All timed output passed an exact RGBA8 round trip through an independent
decoder. The three-case synthetic groups span 17×9, 511×257, and 1920×1080.
The palette-expanded rows give every encoder the same packed RGB pixels.
Deadlib and oxideav-bmp exposed top-down output in the tested APIs.

| Output and source | Cases | Deadlib MP/s | Fastest other exact encoder | MP/s |
| --- | ---: | ---: | --- | ---: |
| RGB24, Kodak photos | 8 | **467.3** | oxideav-bmp | 393.0 |
| RGB24, generated bottom-up | 3 | **476.4** | oxideav-bmp | 392.5 |
| RGB24, generated top-down | 3 | **472.7** | oxideav-bmp | 395.3 |
| RGB24, palette-expanded bottom-up | 3 | **546.0** | oxideav-bmp | 378.2 |
| RGB24, palette-expanded top-down | 3 | **506.7** | oxideav-bmp | 396.5 |
| RGBA32, bottom-up | 3 | **512.6** | image | 190.7 |
| RGBA32, top-down | 3 | **527.7** | oxideav-bmp | 99.1 |
| RGBA32, fully transparent | 3 | **532.7** | image | 167.9 |

Per-image results differ from the group means. At 511×257, OpenCV used
19.6% fewer cycles for generated RGB24 and 6.3% fewer for RGB24 from
palette-expanded pixels. The latter case swapped order between passes.
At 1920×1080, both RGB24 directions were within 1% of oxideav-bmp by cycles.
The separate native indexed-input benchmark measured 32 cases. At 1920×1080,
Deadlib wrote RGB24 from indexed input at 454–572 MP/s. Deadlib's encoder
supports RGB24 and RGBA32 output, so packed palette, bitfield 16-bit, and RLE
output are outside the direct encode comparison.

| Encoder | Verified / 29 | Deadlib wins / within 3% / losses | Deadlib cycle speed |
| --- | ---: | ---: | ---: |
| oxideav-bmp | 29 | 27 / 2 / 0 | 1.96× |
| image | 20 | 20 / 0 / 0 | 2.43× |
| OpenCV | 20 | 18 / 0 / 2 | 3.12× |
| FFmpeg | 20 | 20 / 0 / 0 | 3.17× |
| Windows WIC | 20 | 20 / 0 / 0 | 5.67× |
| stb_image_write | 20 | 20 / 0 / 0 | 6.37× |
| Pillow | 14 | 14 / 0 / 0 | 6.52× |
| SDL3 | 20 | 20 / 0 / 0 | 27.52× |

RGB24 encode output sizes were identical across codecs for each case. In
RGBA32, Deadlib, image, and stb emitted V4 headers with explicit masks. WIC,
SDL3, and OpenCV emitted V5 headers with explicit masks, adding 16 bytes.
Oxideav-bmp used BI_ALPHABITFIELDS, saving 52 bytes, and Wuffs verified its
round trips. FFmpeg used BI_RGB32, saving 68 bytes and relying on decoder
alpha policy. For the all-zero alpha output from FFmpeg, oxideav-bmp
preserved alpha while Deadlib's BI_RGB32 reader rendered opacity. Pillow's
BI_RGB32 output failed the independent RGBA round-trip comparison and was
excluded from the RGBA32 timing table.

## Data and reproduction

- [Decode results, both passes](decode.tsv.gz) and [encode results, both passes](encode.tsv.gz)
- [Native indexed-input timings](indexed.tsv)
- [Pinned dependencies and reproduction commands](../../contenders/README.md)

Each run used five samples of at least 6 ms per timed case. The aggregate uses
the geometric mean of the two run medians. The larger exact-output decode
measurements had a median 5.2% run-to-run cycle spread. Differences near that
magnitude depend on the case and measurement order.

Contenders were Wuffs `a87f0bb85b94`, stb `6b5799bae8a1`, libnsbmp
`3406e7e0bfb5`, SDL 3.4.20, FFmpeg 9.1 (`74e4393595`), WIC on Windows
24H2, image 0.25.10, oxideav-bmp 0.1.9, OpenCV 4.13.0, and Pillow 12.3.0.
[Wuffs](https://github.com/google/wuffs), [stb](https://github.com/nothings/stb),
[libnsbmp](https://github.com/netsurf-browser/libnsbmp), [SDL](https://github.com/libsdl-org/SDL/releases/tag/release-3.4.20),
[FFmpeg distribution](https://www.gyan.dev/ffmpeg/builds/), [Windows WIC BMP](https://learn.microsoft.com/en-us/windows/win32/wic/bmp-format-overview),
[image](https://docs.rs/image/0.25.10/image/codecs/bmp/index.html), [oxideav-bmp](https://github.com/OxideAV/oxideav-bmp),
[OpenCV BMP API](https://legacy-docs.opencv.org/4.x/d8/d6a/group__imgcodecs__flags.html), and [Pillow BMP mode](https://pillow.readthedocs.io/en/stable/handbook/image-file-formats.html#bmp).
