# Reproducible pipeline benchmarks

Run from the workspace root:

```powershell
$env:DEADLIB_PNG_BENCH_ITERS = '20'
cargo bench -p deadlib-image-png --bench pipeline
```

The harness creates four deterministic 512x512 RGBA fixtures (gradient and
xorshift noise, at 8 and 16 bits), validates both decoders and both encoders,
warms each operation three times, and prints TSV with time, throughput and
encoded size. Decode inputs are identical Rust png encodings. Both decoders
produce native sample vectors, including conversion to native u16 for 16-bit
images. Encode timings use each codec's default settings; compression effort and
resulting sizes differ and must be considered alongside throughput.

Checked-in TSV artifacts use UTF-8. Windows PowerShell 5 defaults to UTF-16 for
redirection; use `Out-File -Encoding utf8` when capturing results there.

Baseline: 2026-10-26, Windows x86-64, Intel Xeon E5-2696 v4 @ 2.20 GHz
(22 cores, 44 logical processors), Rust 1.99.1, Cargo bench profile (opt-level 3,
LTO enabled, debug info 1, static MSVC runtime), png 0.18.1, flate2 1.1.10.
Raw results are in `benchmark-baseline.tsv`. This is an initial local measurement,
not a general performance claim: the host is shared and CPU affinity/frequency
were not controlled. Larger corpora, repeated samples, allocation measurements,
stage profiles, and a comparable in-process libpng benchmark remain required.

Baseline native decode is 82Ã¢â‚¬â€œ201 MiB/s versus Rust png's 680Ã¢â‚¬â€œ1324 MiB/s on these
fixtures. Native encode is 21Ã¢â‚¬â€œ41 MiB/s versus 22Ã¢â‚¬â€œ163 MiB/s. The fixed-Huffman
encoder also produces larger files (especially the 16-bit gradient). Competitive
performance has **not** been reached. Initial candidate bottlenecks to measure:
byte-at-a-time stored-block and back-reference copying, per-sample scanline
scatter on contiguous rows, CRC throughput, and compression match selection.

First scalar optimization: `benchmark-stages-before.tsv` adds inflation timings
and shows roughly 40Ã¢â‚¬â€œ64% of native decode time spent in zlib on these fixtures.
The implementation now copies stored blocks directly, expands overlapping
matches in bulk, uses slicing-by-eight CRC, and copies contiguous scanline samples
without per-channel scatter or a second previous-row buffer copy. It stays safe
Rust. No SIMD has been introduced.

`benchmark-bulk-after.tsv` (20 iterations) and `benchmark-bulk-confirm.tsv`
(40 iterations) record the results. The confirmation run measures native decode
at 165Ã¢â‚¬â€œ353 MiB/s, approximately 1.8Ã¢â‚¬â€œ2.3 times the stage baseline. Rust png still
measures 695Ã¢â‚¬â€œ1085 MiB/s; encoding ratios and match selection remain unresolved.
Some tool builds ran on the shared host during measurement, so these are local
directional results, not controlled comparative claims. Exact sample parity was
rechecked with the normal suite and all 611 libpng oracle cases after the change.

A subsequent weighted 16-byte Adler-32 grouping trial did not show a material
pipeline improvement (`benchmark-adler-groups.tsv`, 40 iterations) and was
reverted. The simpler scalar checksum remains, with added modulus/block boundary
known-answer checks. SIMD and further filter/inflate profiling remain open.

`benchmark-streaming-decode.tsv` adds the new `decode-reader` (collect final native
images) and `decode-rows` (consume and discard borrowed native rows) operations.
The latter includes an `image_pipeline_bytes` column for the measured decoder
buffer capacities; zero on other operations means unmeasured. This excludes
metadata, caller input, retained image output and process RSS. On the same shared
host, 20 iterations measured collected streaming decode at 248Ã¢â‚¬â€œ413 MiB/s and
row consumption at 288Ã¢â‚¬â€œ519 MiB/s, with 40,762 bytes of image-pipeline storage for
RGBA8 and 48,954 for RGBA16, independent of image height. These are local
directional results with concurrent activity on the host, not controlled parity
claims. Rust png still measured 727Ã¢â‚¬â€œ1310 MiB/s on these fixtures.

`benchmark-streaming-encode.tsv` adds `encode-writer` (retain the output vector)
and `encode-discard` (write to a sink). Native rows, 32 KiB DEFLATE blocks and a
64 KiB chunk buffer replace whole filtered/compressed image allocations. The
default Fast writer reported 700,666 bytes of image-pipeline storage for RGBA8
and 708,858 for RGBA16 at width 512; caller images, destination storage and
metadata validation are excluded. Identical-width tests confirm capacity does
not grow with height. Native writer throughput was 23Ã¢â‚¬â€œ45 MiB/s locally and the
compression-size gap remains, particularly on the 16-bit gradient. The writer
and original encoder need not emit identical bytes because their DEFLATE block
boundaries differ. Exact decoded samples are compared independently instead.

## Bounded match-chain experiment

The `benchmark-lz-*` artifacts record warmed measurements on the host/toolchain
above, pinned to logical CPU 0. Final and original synthetic measurements use
three repetitions of 20 iterations. The host and CPU frequency remain shared
and uncontrolled; these results establish local direction, not general
competitive performance. This task ran no builds or fuzz campaigns concurrently
with timed measurements. Reproduce fresh evidence:

```powershell
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -LogPrefix ./target/png-pipeline -Iterations 20 -Runs 3
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -LogPrefix ./target/png-deflate -Stage deflate -Iterations 20 -Runs 3
```

The `deflate` stage extracts both codecs' filtered scanlines and feeds identical
bytes to native Fast and flate2 levels 1/6. The initial 16-bit gradient took
500,751 zlib bytes natively versus 18,517 at flate2 level 6 on the same
native-filtered input. Match selection was therefore a measured size bottleneck.

The final implementation uses four-byte hash heads, a 32 KiB backward-link ring,
portable eight-byte prefix comparisons, sparse insertion inside long matches,
and search backoff on sustained literal input. It starts with one candidate and
permits up to 16 after the current block reaches 256 bytes and its fixed-code
cost is at most four bits per input byte. It avoids following the final candidate
link. Length/distance codes use constant-time integer mappings; all 33,024 legal
values are checked against the RFC tables. Streaming history is copied once at
the end of a block, since current-block references read the input block directly.

Each 32 KiB block initially emits fixed Huffman codes into the writer buffer,
with a bit/byte checkpoint. If stored coding is cheaper, the block is restored
and replaced before returning output to the caller. The streaming scratch buffer
is bounded. This avoids a token-planning pass on
photos. Dynamic-Huffman encoding and explicit SIMD remain absent. u32 hash-head
positions wrap with bounded-distance and byte checks. Matcher/history and
streaming scratch capacities do not grow with input length.

Medians below compare `benchmark-lz-before-pipeline-run{1,2,3}.tsv` with
`benchmark-lz-final-pipeline-run{1,2,3}.tsv`. Throughput is native sample MiB/s;
sizes are from `encode-writer`. Both final writers emit the listed sizes.

| Fixture | Buffered before / final | Writer before / final | PNG bytes before / final |
| --- | ---: | ---: | ---: |
| Gradient RGBA8 | 47.4 / 47.1 | 46.1 / 58.1 | 30,868 / 14,977 |
| Noise RGBA8 | 24.4 / 31.2 | 22.2 / 36.3 | 1,106,652 / 1,049,508 |
| Gradient RGBA16 | 40.4 / 46.0 | 42.5 / 57.3 | 501,030 / 40,287 |
| Noise RGBA16 | 24.1 / 31.5 | 23.6 / 37.0 | 2,212,494 / 2,098,436 |

At width 512, measured writer capacity falls from 700,666 to 504,082 bytes for
RGBA8 and from 708,858 to 512,274 for RGBA16: 196,584 bytes less in either case.
This covers encoder image-pipeline buffers, excluding caller images, output
storage, metadata validation and RSS. Height independence remains tested.

For identical native-filtered bytes, final zlib sizes are 14,920 / 1,049,259 /
40,230 / 2,097,995 in the table's order. Deflate-only median throughput changes
from 257.0 / 49.0 / 138.8 / 48.0 to 441.4 / 106.1 / 338.4 / 108.5 MiB/s.
The 16-bit gradient still exceeds independent level-6 size by over 2x. Rust png
remains faster on gradients. Decoder code is unchanged and its performance gap
remains open. Four synthetic fixtures alone do not represent natural images.

The token-planning prototype (`benchmark-lz-tokens-*`) was faster on synthetic
noise but slowed the external photo corpus substantially. Its historical
`benchmark-kodak-selected-*` name does **not** identify the final implementation.
The final direct-emission approach trades some synthetic-noise speed for photo
throughput and lower memory. Earlier chain/backoff, search limits, token packing,
hash-width and inline trials remain recorded, including unsuccessful results.
[experiments/](experiments/README.md) maps artifacts to byte-exact source patches.

Baseline codec source is `684326e55` with this milestone's harness. Every
manifest records executable SHA-256, tool versions, CPU/affinity and timestamps.
The first before-deflate manifest predates per-source hashes; its binary hash
matches before-pipeline, which records the source hashes. Later manifests also
record every production Rust source and harness hash. Shared-checkout HEAD can
include unrelated concurrent commits; identify the measured codec with those
hashes and checkpoint patches. Some trial files originally used CRLF; patch bytes
are preserved so reconstruction matches their recorded raw hashes.

## External RGB8 corpus

`scripts/kodak.json` pins the URLs, byte lengths and SHA-256 values of the 24
PNG images from [Rich Franzen's Kodak lossless image collection](https://r0k.us/graphics/kodak/),
retrieved on 2026-10-26 (15,394,305 compressed bytes). Images are downloaded to
ignored build storage and are not redistributed in this repository:

```powershell
python crates/deadlib-image/png/scripts/fetch_corpus.py --output target/deadlib-image-reference/kodak
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -Target corpus -CorpusDirectory ./target/deadlib-image-reference/kodak -LogPrefix ./target/png-kodak -Iterations 10 -Runs 3
```

The corpus harness accepts RGB8 PNGs. Decode timings give both libraries the same
original file, including metadata. Native decoding returns a complete `Png`;
reference decoding returns native sample bytes and dimensions. Encoding gives
both libraries the same RGB samples, without ancillary metadata, using their
default settings. All input and reencoded samples are checked through native and
independent Rust png readers before timing. File sizes must be considered with
speed because the two encoders use different compression effort. Run boundaries,
corpus hashes and source/binary hashes are recorded in the JSON manifests.

An additional ignored external C test checks each original plus both native
writers under Stored/Fast and plain/Adam7 output: 216 exact native-sample libpng
comparisons for these 24 inputs. After building the C oracle, run:

```powershell
$env:DEADLIB_PNG_CORPUS = (Resolve-Path ./target/deadlib-image-reference/kodak).Path
$env:DEADLIB_LIBPNG_ORACLE = 'path/to/deadlib_png_oracle.exe'
cargo test -p deadlib-image-png --release --test libpng libpng_external_corpus -- --ignored --nocapture
```

`benchmark-kodak-before-*` and `benchmark-kodak-final-*` contain three warmed
10-iteration runs of the previous and selected codec. Throughput below is total
native bytes (27 MiB per corpus) divided by the sum of per-image median times;
it is not an arithmetic average of per-image throughput. `before-confirm` is a
later single five-iteration control run of the unchanged old binary.

| Operation | Before, 3 runs | Final, 3 runs | Later old control, 1 run |
| --- | ---: | ---: | ---: |
| Native buffered encode, MiB/s | 27.80 | 25.30 | 26.15 |
| Native writer encode, MiB/s | 28.25 | 25.29 | 25.87 |
| Rust png encode, MiB/s | 10.43 | 9.30 | 9.91 |
| Native decode, MiB/s | 73.46 | 65.95 | 67.57 |
| Native reader decode, MiB/s | 58.32 | 54.46 | 54.10 |
| Rust png decode, MiB/s | 211.78 | 188.28 | 194.91 |

The unchanged decoder and Rust png controls also slow by about 10?11% between
the three-run groups. Together with the later old-code control, this supports
an inference of similar photo encoding throughput, not a measured broad speed
win or regression. CPU frequency and other workloads were not controlled.
The token prototype had a much larger loss (writer 16.44 MiB/s), which motivated
its replacement. The final writer's corpus total drops from 22,385,618 to
21,227,692 bytes, a 5.2% reduction. The Rust png default produces 15,777,098 bytes,
so native files are still about 35% larger despite higher encode throughput in
these runs. Compression effort and filtering differ. Comparable-effort results,
more formats/corpora, and a similarly instrumented libpng benchmark remain open.

Final RGB8 writer buffer capacities are 502,034 / 505,106 bytes at widths
512 / 768, versus 698,618 / 701,690 before. These exclude retained pixels,
destination storage, metadata and process RSS, as in the synthetic measurements.
The selected code improves the local size/memory tradeoff; it does not establish
the requested competitive decoder performance. Native photo decode still trails
Rust png substantially, and filter/literal-inflate profiling remains required.
