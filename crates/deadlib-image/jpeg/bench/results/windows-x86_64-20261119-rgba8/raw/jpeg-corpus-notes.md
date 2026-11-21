# JPEG corpus and behavior-oracle audit

Prepared before decoder executions or performance measurements for baseline commit
`9a6f4ebabe71934202b55df110dae03883ca512c`. This audit performed no builds,
decoder runs, tests, profiles, timings, source edits, staging, or commits.

## Inventory and fixed selection

`jpeg-corpus-inventory.json` records all 596 shipped/fixture JPEG paths,
SHA-256 values, file lengths, first-frame/process/sampling descriptions, scan
and restart counts, and ordered APPn/COM payload lengths/hashes. Total encoded
size is 5,957,920 bytes: five shipped assets and 591 fixture JPEG files.
Case-insensitive extensions matter: the 21 T.83 JPEG files use `.JPG`.

`jpeg-corpus-runtime-inventory.json` records 582 existing JPEG-named paths under
`target/debug` and `target/release`, totaling 180,778,966 encoded bytes and 292
distinct hashes. Five hashes are copies of the shipped assets. Three distinct
files do not begin with JPEG SOI and are explicitly listed as structural
exclusions; a `.jpg` extension alone is not a valid JPEG identifier. The other
284 unique runtime images supply the stratified sample. Artwork bytes were
read in place; no artwork was generated, copied, or modified.

`jpeg-corpus-selection.json` freezes 45 speed candidates:

| Group | Count | Selection |
| --- | ---: | --- |
| Shipped assets | 5 | Every shipped JPEG |
| Runtime artwork | 19 | Deduplicate by SHA-256; exclude shipped copies; stratify by process/color/sampling; order by pixel count, encoded length, hash, path; take minimum, median and maximum when population is at least ten, otherwise extrema |
| Original sequential fixtures | 10 | Every original 8-bit case except the three declared diagnostic names |
| Original progressive fixtures | 10 | Same rule |
| Metadata/YCCK fixture | 1 | `metadata/ycck-8.jpg` |

The range is 31x25 through 6000x3375. The sample covers ordinary baseline,
extended sequential, and progressive processes; 4:4:4, 4:4:0, 4:2:2 and 4:2:0;
gray, encoded RGB, YCbCr, CMYK and YCCK; optimized/default Huffman tables,
restarts, wide quantization, EXIF, ICC, and APP metadata from 12 to 25,149 bytes.
The two real grayscale images are 3738x2912 with 1x1 and 2x2 sampling.

`jpeg-corpus-speed.txt` contains exactly these 45 relative paths, one per line.
The file is an argument list, not an input format accepted by the original
benchmark. A caller must read it and pass the individual paths, or add explicit
list-file support. Spaces and apostrophes in artwork paths require argument
passing without shell string interpolation.

The six `8-410.jpg`, `8-411.jpg`, and `8-multiscan.jpg` cases from sequential and
progressive directories are listed separately in `jpeg-corpus-diagnostic.txt`.
This preserves the former benchmark's special sampling/scan coverage without
mixing known interpolation/multiscan differences into a single undifferentiated
aggregate. `jpeg-corpus-native-correctness.txt` lists the other 564 fixtures.
All three lists together include every shipped and fixture JPEG plus the 19
selected real artworks, a 615-file behavior-oracle input set.

The 45 entries remain candidates until an **untimed** preflight records both
native and image acceptance and output shape. No timing result influenced
selection. Any unavailable decoder or non-equivalent output contract must be
reported by input and reason; do not silently trim slow inputs. Preserve
separate group results so twenty 31x25 synthetic images do not obscure the
real-asset result. Add legacy diagnostics to their own report if measured.

## Existing corpus coverage and provenance

| Fixture directory | JPEG files | Encoded bytes |
| --- | ---: | ---: |
| abbreviated | 109 | 67,807 |
| arithmetic | 15 | 21,562 |
| arithmetic-model | 2 | 9,270 |
| arithmetic-progressive | 15 | 33,799 |
| components | 105 | 843,489 |
| dnl | 26 | 11,861 |
| hierarchical | 42 | 49,300 |
| hierarchical-native | 62 | 56,099 |
| jpegxt-limitations | 5 | 1,464 |
| lossless | 45 | 27,220 |
| lossless-arithmetic | 45 | 54,726 |
| lossless-special | 1 | 1,320 |
| metadata | 2 | 6,602 |
| mozjpeg | 30 | 43,912 |
| mozjpeg-limitations | 4 | 5,459 |
| progressive | 25 | 42,467 |
| pyramid-native | 12 | 30,164 |
| sequential | 25 | 39,283 |
| t83 | 21 | 1,897,514 |

The sequential/progressive manifests pin libjpeg-turbo 3.2.1, producer
arguments, SHA-256, exact coefficients and separate IDCT numerical results.
The 50 inputs include 8/12-bit samples and CMYK. Metadata fixtures include
native-produced YCCK; a separate pinned sRGB ICC profile supplies metadata
tests. Mozjpeg fixtures pin source or native-rewrite provenance and distinguish
exact C coefficient comparisons from bounded numerical color/IDCT comparisons.
Arithmetic fixtures similarly retain source/coefficient hashes and C-producer
commands. The abbreviated manifest groups actual stateful sessions and table
packets; standalone decoding of each packet cannot replace its session tests.
The T.83 manifest contains original official streams (twenty images and one
table packet), archive identity and original provenance, with no claim of
complete formal conformance.

Existing compatibility mutations are generated in memory by
`tests/compatibility.rs`; there is no separate on-disk compatibility JPEG
directory. `target/jpeg-compat-20261119-reference/src/main.rs` records a prior
seven-source marker/padding/Adobe/truncation mutation harness and its sibling
logs. Reusing those input definitions is more meaningful than treating the
old log's `OK/ERR` strings as a new exact baseline.

Other target directories mostly contain copied fixtures/assets or unrelated
application investigations. `jpeg-corpus-target-groups.json` provides counts
and examples without hashing or copying those directories. The repository
also has three JPEGs under `save`; they were not selected or inspected.

## Existing benchmark scope and gaps

The checked-in benchmark README/RESULTS describe 31 inputs: five shipped
assets, thirteen sequential and thirteen progressive 8-bit fixtures. The
native mode calls public `decode`; the image mode calls
`image::load_from_memory_with_format(..., Jpeg)` and consumes raw L8/RGB8
bytes. It does not time the application's final RGBA conversion or its native
compatibility option. The historical result is not automatically a claim
about the current complete image-loading contract.

Current strengths include process-wide CPU cycles, affinity, five batches,
independent wall/cycle medians, reference warmup, varying codec order by file,
destruction inside timing, and separately calibrated allocation instrumentation.
These align with rust-performance.md M-HOTPATH's CPU/allocation guidance.

Current limitations relevant to this optimization:

1. Precision is restricted to 8-bit through a coefficient preflight; lossless,
   hierarchical, abbreviated, unsupported, malformed and high-precision cases
   cannot serve as full behavior coverage in this harness.
2. Cross-decoder `PIXELS` prints maximum error and RMSE only. It provides no
   declared mathematical tolerance and is not a pixel-equality assertion.
3. Snapshots contain only raw native pixel bytes and are keyed by sorted file
   ordinal. They omit input identity, dimensions, color type, sample precision,
   metadata, errors, options, and resource limits. Changing the sorted corpus
   can compare the wrong inputs.
4. The default native decode is strict. The application's compatibility input
   policy and final output type need explicit matched benchmark adapters.
5. All original generated speed fixtures are 31x25. Real progressive and large
   grayscale/color artwork were absent. This selection addresses that gap.
6. Historical timings acknowledged other builds on the host. New repeated
   comparisons should reserve the host and keep heavy inventory/hash work
   outside measurement windows. Inventory/hash work for this audit is complete.

## Exact native before/after oracle

JPEG decoders can legally differ in IDCT and interpolation rounding. The
checked-in RESULTS explicitly records centered interpolation differences for
4:1:0/4:1:1 and incorrect zune/image samples on the sequential multiscan case.
Therefore external reference pixel differences cannot decide whether this
optimization changed native behavior. Preserve a binary/source snapshot of
the starting native revision and compare its observable results with each
candidate on identical bytes and options.

A stable observation should contain:

- Relative input path, input SHA-256/length, API/mode, strictness, upsampling,
  and every resource-limit value; identify by content/path rather than ordinal.
- On success, width, height, color type, bit depth, sample variant, sample
  count and every sample byte. Serialize U16 in an explicit byte order.
  Compare native pixels before RGBA conversion and the final public loading
  output when that adapter changes.
- On failure, exact `Error` variant and payload (`Debug` or a tagged form).
  `Error` implements `Eq`; acceptance alone is weaker than preserving errors.
  Do not use full derived `Debug` for decoded structures because it includes
  private memory bookkeeping that is not part of the public result.
- For metadata-retaining coefficient/lossless/hierarchical APIs, exact public
  frame fields, component specs, quantizers and padded blocks or native sample
  planes, successive-approximation/predictor state, scan counts, consumed byte
  count, and ordered `MetadataSegment { marker, data, offset, preceding_scans }`.
  Original metadata bytes and order matter, including malformed/opaque payloads.
- For abbreviated streams, the established manifest session order and table
  state are part of the input. The existing integration tests should remain
  the stateful oracle even if a simpler standalone snapshot records errors.

Use both strict and compatible policies and both nearest and linear output
for the complete fixture corpus. Include deterministic malformed/truncated
cases and boundary limits (pixels, coefficient/sample storage, metadata and
scans), with mutation recipes and seeds fixed before comparison. The existing
compatibility tests cover empty DHT/DQT, padding, inter-marker noise, absent EOI,
partial markers, missing entropy, restart sequences, progressive EOB overflow,
Adobe precedence, and resource limits. The independent C/T.83 coefficients,
lossless samples and transform checks remain valuable alongside exact native
snapshots; two native implementations can share a bug.

The current fuzz harness compares identical pixels and acceptance under bounded
limits but accepts any `Err` against any other `Err`, plus one reference-storage
limit exception. It therefore does not itself prove unchanged exact error
variants or metadata. Tests should cover that material gap without expanding
performance sampling into an unrelated conformance project.

## Reproduction

From the repository root, before timings:

```powershell
python target/jpeg-perf-20261119/jpeg-corpus-inventory.py --include-target --include-runtime
python target/jpeg-perf-20261119/jpeg-corpus-select.py
```

The scripts use Python's standard library and `rg`. They only write
`jpeg-corpus-*` artifacts under `target/jpeg-perf-20261119`. The structural
parser is not a JPEG validator or a decoder: generalized component labels,
hierarchies and concatenated abbreviated packets require native APIs to
interpret fully. Every encoded file hash covers the original complete bytes.

## Publicly reproducible Kodak supplement

After freezing the 45-entry selection, the coordinator requested a separate
photographic supplement derived from the 24 already pinned Kodak PNGs under
`target/png-kodak-20261117`. The existing PNG benchmark's `raw/kodak.json`
provides the original source URLs, byte counts and hashes. All 24 source
lengths and SHA-256 values were verified before encoding; no download occurred.

`jpeg-corpus-kodak.py` generated 60 JPEGs under `photos`, totaling 7,104,918
encoded bytes. Its `jpeg-corpus-kodak.json` records the generator hash, original
manifest hash, source and output hashes, encoder versions, exact save options,
source conversion, header properties, and explicit group labels. The separate
`jpeg-corpus-kodak.txt` contains their 60 paths. This did not change the
45-entry selection, six diagnostic inputs or 564 remaining fixture inputs.

| Group | Count | Sources |
| --- | ---: | --- |
| kodak-baseline-420-q85 | 24 | All 24 |
| kodak-progressive-420-q85 | 24 | All 24 |
| kodak-baseline-gray-q85 | 4 | 01, 06, 13, 23 |
| kodak-baseline-cmyk-444-q85 | 4 | 01, 06, 13, 23 |
| kodak-baseline-rgb-444-q85 | 4 | 01, 06, 13, 23 |

Installed versions were Python 3.13.14, Pillow 12.3.0, libjpeg API 8.0 and
libjpeg-turbo 3.1.4.1. Pillow's local JPEG saver exposes `keep_rgb`; every RGB
variant's header was checked for encoded RGB component IDs and 4:4:4 sampling.
Every output was also checked structurally for its declared baseline/progressive
process, color mode, sampling, 8-bit precision and EOI. This was encoder output
inspection, not execution of the native or reference JPEG decoders.

Quality is 85, `optimize=False`, no copied EXIF/ICC, explicit subsampling and
progressive settings. The base photographs retain source RGB samples;
grayscale and CMYK use the installed Pillow's `RGB.convert('L')` and
`RGB.convert('CMYK')`. Keep-RGB uses `keep_rgb=True, subsampling=0`.
Image metadata is cleared before saving, so emitted metadata consists only of
the codec's generated JFIF/Adobe headers. The script checks existing generated
files byte-for-byte and refuses to overwrite them if reproduction differs.

```powershell
python target/jpeg-perf-20261119/jpeg-corpus-kodak.py --inspect
python target/jpeg-perf-20261119/jpeg-corpus-kodak.py
```

The supplement depends on the pinned PNG manifest, those 24 local PNG files,
the installed Pillow encoder, and `jpeg-corpus-inventory.py` beside the script.
It performs no package installation or external build. Source/output images
remain ignored local measurement inputs. Combining this supplement with the
original selection yields 105 speed candidates; keep all five Kodak groups
distinct in reports, and preserve the original runtime artwork group regardless
of its measured performance.
