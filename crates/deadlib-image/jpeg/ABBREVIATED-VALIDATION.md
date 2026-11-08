# Abbreviated table and image streams

This milestone implements whole-input T.81 B.4/B.5 sessions across all thirteen
JPEG-1 frame processes. Incremental byte input/output, independent conformance
corpora, the remaining IJG/mozjpeg gates and measured performance remain open.
These checks do not establish complete standards conformance or speed parity.

## State and API

`Tables` owns four quantization destinations and eight canonical Huffman
destinations. Quantizers retain both their natural-order values and DQT width;
Huffman views borrow the original counts/symbol order from compiled lookup
tables. Setters validate definitions before replacing an existing destination.
Zero-code definitions permitted by B.2.4.2 are retained and serialized; they
can remain unused, while attempting to decode a symbol from them fails.
No lookup-table clone is needed per image. Decoded models own their quantizer
snapshots and borrow metadata from the current input, independently of the cache.

`load` consumes one SOI/EOI table-specification stream. B.5 permits DQT, DHT,
APPn and COM; DAC, DRI, frames, scans, DNL and DHP are rejected. Empty SOI/EOI
preserves existing definitions. Table metadata is returned with its own input
lifetime, subject to metadata/storage limits. It is never inherited as image
color/ICC metadata. `encode` and `encode_with_metadata` write a bounded table
stream; getters/setters also support application-supplied definitions.

The three `Tables::decode_*` methods reuse the same destinations. Successfully
processed definitions replace earlier values, including inside image streams.
On error, updates already made remain installed; explicitly
clone the cache before a call if the application requires rollback. Arithmetic
conditioning defaults and restart interval zero are reestablished at every SOI.
The existing free decode APIs still begin with an empty table bank.

`AbbreviatedEncoder` shares the ordinary native frame writers. Before the first
SOS, changed DQT/DHT definitions go directly into a companion table stream.
After that SOS, changed definitions stay inline at the scan/frame where they
take effect. Identical cached definitions are suppressed. This avoids moving
later definitions in front of scans that use earlier versions. DAC and DRI are
always handled by the ordinary image writer. APPn/COM and ICC stay in the image.
The encoder neither creates a second complete JPEG nor splits/copies entropy.

All coefficient, native/shared raster, predictive lossless, hierarchical and
pyramid source APIs expose this path. Deliver each successful `tables` vector
followed by its `image` vector to one persistent receiver. `with_tables` accepts
known receiver state; `tables()` exposes its current definitions. Clear state
if delivery is interrupted or a successful pair is discarded. Encoder write
errors clear state so a retry cannot suppress undelivered tables. Preflight
errors before writing may preserve the previous state.

The image output budget includes both returned vectors, including their two
SOI/EOI pairs. Hierarchical frame budget restoration retains deductions for
external table bytes. `max_table_bytes` additionally bounds the table packet.
Existing metadata, frame, scan, pixel and model storage limits remain in force;
the fixed caller-owned table bank is not allocator high-water accounting.

## Independent references

The test-only `tests/oracle/abbreviated.c` uses the public libjpeg-turbo 3.2.1
API: `jpeg_write_tables`, `jpeg_start_compress(FALSE)` and
`jpeg_read_header(FALSE)`. One compressor/decompressor object persists for each
entire session. Separate files and concatenated files exercise the same native
state on both sides. DCT comparisons require identical visible coefficients,
quantizers and geometry. Predictive lossless comparisons require identical
native sample integers; C-authored sources also match an independent source
formula after the declared point transform. No IDCT tolerance is used.

The C setup explicitly assigns Tq=0 after `start_compress` and before frame
header emission. Turbo reinitializes lossless component/color defaults during
that call. Its YCbCr/YCCK defaults also select Huffman table 1 while the lossless
SOS writer writes selector zero. For these two source spaces the client uses
generic native components with table zero and writes explicit Adobe transform
tags. No sample conversion is performed. These are public encoder setup/marker
changes, not reference codec patches or native parser relaxations. The reset
cases first encode custom DAC and
restarts, then encode defaults and omit DAC/DRI from the second image. Both
decoders must reestablish SOI defaults rather than reuse the previous image.

Rare-process checks use the existing pinned JPEG XT build and four documented
test-only patches in `tests/oracle`. That adapter has no genuine B.5 session
API. An independent Python bank stores raw DQT/DHT definitions per destination
and prepends the current definitions to an unchanged abbreviated image. Tables
are captured before image updates; later redefinitions remain inline. Exact
C frame observations/PGX samples check this equivalent interchange stream.
These are entropy/frame-data comparisons, not C abbreviated-parser successes.
The C observer records coefficients/residuals; native checks additionally
require exact quantizer snapshots and reference composition.
The reference's stale EXP state occasionally requires an explicit EXP00 in its
input copy; every such comparison is identified in the report. Original native
streams are always checked unchanged by the persistent Rust decoder. Annex J
reference composition is compared against the native pre-entropy target; no C
final-raster tolerance substitutes for exact frame data.

## Reproduction and retained artifacts

Build the test C client with the existing pinned oracle CMake configuration:

```text
cmake --build experiments/oracle-build --config Release --target jpeg-abbreviated
cargo build --release --example abbreviated --jobs 1
python tests/reference_abbreviated.py --native NATIVE --turbo TURBO --reference JPEGXT --part c --out experiments/abbreviated-c --fixtures experiments/abbreviated-fixtures
python tests/reference_abbreviated.py --native NATIVE --turbo TURBO --reference JPEGXT --part native --out experiments/abbreviated-native --fixtures experiments/abbreviated-fixtures
python tests/reference_abbreviated.py --native NATIVE --turbo TURBO --reference JPEGXT --part rare --out experiments/abbreviated-rare --fixtures experiments/abbreviated-fixtures
python tests/pin_abbreviated.py --raw experiments/abbreviated-fixtures --out tests/fixtures/abbreviated --reports experiments/abbreviated-c/report.json experiments/abbreviated-native/report.json experiments/abbreviated-rare/report.json --report-out tests/reports
```

The drivers snapshot executables and record SHA-256 values, exact source
parameters and checkpoint identities. `--resume` requires the same driver,
helper and tool hashes. Final reports distinguish configured image cases from
distinct encoded byte strings; repeated settings are not independent source
corpora. Separate/concatenated replay is not counted as another image case.

The retained `tests/reports/abbreviated-{c,native,rare}.json` gates contain:

| Gate | Sessions | Image cases | Independent comparison |
| --- | ---: | ---: | --- |
| C-authored | 1,082 | 3,238 | Genuine B.5, C/native coefficients or samples; lossless source formula |
| Native ordinary processes | 24 | 1,524 | Genuine B.5, pre-entropy coefficients/quantizers or samples |
| Native rare processes | 36 | 1,932 | Equivalent interchange, exact C coefficients/residuals/samples |

The C-authored gate covers 624 DCT sessions: both precisions, five colors,
gray/4:4:4/4:2:0/4:2:2 layouts, sequential/progressive, both entropy coders,
restart rows 0/1 and quality 30/85/100. Another 450 predictive sessions cover
P=2..16, five native color layouts, Pt=0/P-1 and restart rows 0/1/3, cycling
all seven predictors. Each session has three distinct source images. Eight
two-image arithmetic sessions verify SOI DAC/DRI defaults explicitly.

Native gates process 127 ordinary and 161 rare source files, each twice under
fixed/optimized Huffman and restart settings 0/1/3. Tables persist across batches
of up to 32 different sources, as well as between repeated images. The rare
C gate matches 9,984 DCT blocks and 1,291,572 sample/residual integers exactly.
It records 96 C-input EXP00 workarounds and zero tolerated C warnings/errors.
No such syntax change is applied to native inputs.

`abbreviated-empty.json` records an additional C/native check of all eight
unused zero-code destinations, both table-only and followed by a real image.
Replacing a used destination with an empty definition fails natively and
produces a C corrupt-data diagnostic; that C warning is an expected rejection,
not a passing image comparison. Run `reference_abbreviated_empty.py` after the
compaction step with `--native`, `--turbo`, `--fixtures` and `--out` paths to
reproduce the check and append its fixture to the manifest.

`pin_abbreviated.py` removes overlapping mixed-session prefixes from retained
fixtures. C sessions remain verbatim. Each retained native image is unchanged;
its starting raw destination bank is independently serialized into a B.5 table
packet. The manifest labels this snapshot and records the source stream hashes.
Expected coefficients/samples were captured before native entropy encoding;
C-authored expectations also pass exact C/native comparisons. These are
regression fixtures, not an independent standards-conformance corpus.

The final set contains 44 sessions and 64 images in 223 files (481,821 bytes,
including the provenance manifest). The Rust regression checks all thirteen
process markers, 34,201 image prefixes and 3,300 bounded image mutations. The
separate table-stream test covers every prefix and 1,000 bounded mutations of
an externally supplied quantization packet.

Rust tests additionally cover all table destinations, 256-symbol definitions,
wide quantizers, table replacement, borrowed metadata, SOI resets, retained
state after malformed definitions, all output-budget boundaries, cache reset
on failure, all source APIs, DNL/zero-height DHP combinations, every existing
fixture and bounded malformed input. Pinned session tests exercise all process
markers and representative truncated prefixes/mutations without C installed.

`abbreviated-regressions.json` records 10,612 unchanged default-writer outputs,
with 964 fresh Turbo C decodes; the remaining cases explicitly reuse previous
C evidence through exact SHA-256 identity. This writer audit was completed
before relaxing the empty-definition parser check; the subsequent zero-Li
change has its own targeted C gate and the final full package suites. No
performance or allocation high-water result is inferred from these checks.

All 139 tests pass in optimized release and an isolated debug package build
with only `deadlib-image`. Formatting, all-target Clippy and rustdoc pass with
warnings denied. `abbreviated-validation.json` records the final gate counts
and report hashes; the full user goal remains active.
