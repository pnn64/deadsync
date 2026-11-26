# PNG codec comparison on Windows x86-64

## General-purpose decode

Deadlib has the lowest total cycles in both static workloads. On 238
RGBA8 files it uses **19% fewer cycles than Blazediff**, the closest
overall contender, and has the lowest cycles on **181/238** individual
files among codecs that matched the reference pixels. On the 38 RGBA16
files it uses **27% fewer cycles than Blazediff** and leads on **33/38**
files. These results combine two independent runs. The RGBA8 cycle
advantage over Blazediff is 18.5% and 19.2% in the individual runs.

| Reference | RGBA8 Deadlib/reference cycles | RGBA8 wins | RGBA16 Deadlib/reference cycles | RGBA16 wins |
| --- | ---: | ---: | ---: | ---: |
| Blazediff | 0.812 | 189/238 | 0.732 | 33/38 |
| Rust png | 0.807 | 236/238 | 0.598 | 38/38 |
| image | 0.777 | 236/238 | 0.365 | 38/38 |
| libspng | 0.717 | 237/238 | 0.254 | 38/38 |
| Wuffs | 0.693 | 227/233 | 0.251 | 35/35 |
| libpng | 0.691 | 238/238 | 0.315 | 38/38 |
| Zune PNG | 0.687 | 229/234 | 0.682 | 34/35 |
| stb_image | 0.446 | 233/238 | 0.383 | 38/38 |

Deadlib uses **17% fewer cycles than Rust png on the 32 Deadsync
assets**, the closest reference there. On Kodak the lead over Blazediff
is **2.6%** and Deadlib leads on 13 of 24 photos. Two generated
noninterlaced RGB8 files take **1.40× Wuffs' cycles** combined. Blazediff
also leads on some RGB16, gray-alpha16 and interlaced RGBA16 cases.

In PNGSuite, Wuffs differs from the canonical pixels on five RGBA8 and
three RGBA16 cases. Zune differs on four RGBA8 and three RGBA16 cases.
Those rows are excluded from the corresponding pairwise speed comparisons.

## fpng-produced PNGs

| Decoder | 30 files, M cycles | 8 game captures, M cycles | 6 photos, M cycles |
| --- | ---: | ---: | ---: |
| Deadlib | 433.67 | 96.81 | 55.44 |
| fpng | 509.44 | 86.35 | 84.13 |
| libspng | 406.38 | 107.74 | 81.95 |

libspng leads by total cycles on the mixed corpus, including the large
synthetic cases. fpng takes the lowest cycles on 16 of the 30 files,
including seven captures. Deadlib takes the lowest cycles on all six photos
and one capture.

## Still-image encode

FPNGE level 1 has the lowest total cycles on the 30 RGBA8 cases, the eight
captures, and the six photos. Deadlib RLE/Up is second by total cycles on the
30-file corpus and uses **17% more cycles** with **23% fewer output bytes**
than FPNGE level 1. On the captures, RLE/Up wins **7 of 8** individual files
against FPNGE level 1 and uses **12% more total cycles**. The one FPNGE win
is larger than the seven Deadlib wins combined.
The 12 ms screenshot confirmation again records seven RLE/Up wins and
one FPNGE win. FPNGE leads by total cycles there too, with RLE/Up using
20% more cycles. [Screenshot confirmation](screenshot-confirm.tsv.gz)

| Encoder | 30 RGBA8, M cycles | 30 RGBA8, MB | 8 captures, M cycles | 8 captures, MB |
| --- | ---: | ---: | ---: | ---: |
| FPNGE 1 | 383 | 68.60 | 48.21 | 3.092 |
| Deadlib RLE/Up | 447 | 52.71 | 54.05 | 3.260 |
| Deadlib RLE/Adaptive | 544 | 51.55 | 78.78 | 2.578 |
| FPNGE 4 | 609 | 66.99 | 88.45 | 2.315 |
| Rust png Fastest | 758 | 52.28 | 67.93 | 3.049 |
| Rust png Fast/Adaptive | 922 | 50.92 | 116.38 | 2.346 |
| fpng default | 952 | 51.98 | 151.54 | 2.867 |
| Deadlib default Fast/Adaptive | 1,452 | 52.42 | 381.68 | 2.928 |

On captures, Deadlib RLE/Adaptive uses **32% fewer total cycles** than
Rust png Fast/Adaptive and produces **10% more bytes**. Deadlib's default
Fast/Adaptive uses **3.28×** the Rust encoder's cycles and **7.92×**
FPNGE level 1's cycles on those same files.

On the 60 original-format encoding cases, RLE/Up has the lowest cycles of
all verified modes on **41/60 files**. It uses 28.51 million cycles,
compared with 63.42 million for Deadlib Fast/Adaptive and 171.64 million
for libpng level 1. Its 4.14 MB of output compares with 2.28 MB for Deadlib
Fast/Adaptive and 2.34 MB for libpng level 1. Rust png Fastest leads the
noninterlaced packed grayscale and indexed cases. Rust png 0.18.1's
encoder covers the 30 noninterlaced cases. FPNGE covers 16 cases, and fpng
covers four. See the per-format and per-codec coverage in the summary.

## APNG

On the nine raw-frame workloads, Deadlib decodes all nine fastest,
using **27% fewer cycles than Rust png** and **39% fewer than Zune PNG**.
Deadlib RLE/Adaptive wins six of nine encode cases and uses **36% fewer
total cycles** and **9% fewer bytes** than Rust png Fast/Adaptive.
Deadlib's default Fast/Adaptive uses **2.46×** Rust png's cycles.

| Operation | Mode | M cycles | Encoded bytes |
| --- | --- | ---: | ---: |
| Decode | Deadlib | 106.63 | — |
| Decode | Rust png | 145.38 | — |
| Decode | Zune PNG | 175.31 | — |
| Encode | Deadlib RLE/Adaptive | 41.63 | 477,157 |
| Encode | Rust png Fast/Adaptive | 65.01 | 526,298 |
| Encode | Deadlib default Fast/Adaptive | 160.16 | 603,916 |

## Workloads

- **Static decode to RGBA8:** 238 files: 60 independently generated cases, all
  24 SHA-256-verified Kodak photos, 32 Deadsync assets, and 122 PNGSuite
  fixtures. The generated cases cover all 15 legal PNG color-type and
  sample-depth combinations, both plain and Adam7, all five PNG filters,
  and compressible and noisy inputs. PNGSuite exercises transparency,
  different filters and zlib encodings, and ancillary chunks.
- **Static decode to RGBA16:** The 38 files with 16-bit source samples, with
  all 16 bits of each channel preserved. Static comparison uses straight
  alpha and matches reference sample values without color management.
- **fpng decode:** 30 additional RGBA8 files produced by fpng. fpng's
  decoder reads the subset written by its own encoder. The same inputs
  also ran through all general-purpose decoders. [fpng upstream](https://github.com/richgel999/fpng)
- **Still-image encoding:** The 60 format cases encode at their original
  sample depth, color type and interlace mode. Another 30 inputs encode as
  RGBA8, with eight game captures, six photos, and 16 synthetic images
  covering flat color, gradients, transparent UI patterns and
  noise from 17×13 through 3840×2160. Every timed result is a complete,
  independently decoded PNG with matching pixels.
- **APNG:** Nine generated RGBA8 files span full-size frames, cropped
  subframes, and a separate poster image. Checks cover the raw samples,
  frame positions, frame delays, blend and disposal. The workload measures
  animation frame decoding and encoding. Rendering or compositing the
  frames is outside the measurement.

## Measurement

Windows 11 Pro build 26100, Intel Xeon E5-2696 v4 (AVX2), logical CPU 2,
rustc 1.100.0 / LLVM 23.1.5 and MSVC 19.44.35230. Rust uses release
optimization, whole-program LTO, one codegen unit and the static CRT.
The C/C++ wrapper uses MSVC Release and AVX2. libpng and libspng link
zlib-ng 2.3.90 in compatibility mode.

The two static runs used five batches of at least 8 ms for each case and
codec. The animation repeats used five batches of at least 20 ms. Each
batch measures one thread. Times include construction and destruction of
the owned result. Input I/O, pixel comparison and output reporting take
place outside the measurement. Smaller numbers are better for both
cycles and bytes. Total cycles represent one operation per file. Win counts
give each file one vote. The summary also records wall time, comparisons on
matched files, and equal-file geometric means.

The static encoder runs kept one verified PNG buffer alive during each
codec's timing. The final harness releases that buffer before timing.
The screenshot confirmation uses the final harness and reproduces the
top two aggregate positions and individual winner counts.

The APIs do differing amounts of validation. Deadlib uses its standard
public decoder and complete stream checks. The Rust png asset adapter
stops after the decoded first image. Zune uses its fast SIMD preset
with CRC and Adler verification disabled. A codec that produced a different
pixel value or output shape received no timing for that case.

Static decoder versions include png 0.18.1, image 0.25.10, zune-png 0.5.2,
blazediff-png 6.1.2, Wuffs 0.4.0-alpha.12, libpng 1.8.0.git, libspng 0.7.4,
and stb_image 2.30. The external library selection includes the main
contenders in the image-rs upstream comparison and the SIMD codecs
Blazediff, fpng and FPNGE. [Image-rs comparison](https://blog.image-rs.org/2026/06/18/png-adoption.html) · [Upstream source revisions](../../contenders/upstream.tsv)

See the [benchmark instructions](../../contenders/README.md) for the build,
run commands and summarizer.

## Raw evidence

- [Summary and per-format results](summary.json.gz) · [Input paths and hashes](inputs.json.gz) · [Build and source identities](manifest.json)
- Static decode [run 1](decode-run1.tsv.gz) and [run 2](decode-run2.tsv.gz)
- Static encode [run 1](encode-run1.tsv.gz) and [run 2](encode-run2.tsv.gz)
- fpng-produced decode [run 1](fpng-run1.tsv.gz) and [run 2](fpng-run2.tsv.gz)
- APNG [run 1](animation-run1.tsv.gz) and [run 2](animation-run2.tsv.gz)
- Final harness [static decoder verification](decode-final-check.tsv.gz) and [screenshot confirmation](screenshot-confirm.tsv.gz)
