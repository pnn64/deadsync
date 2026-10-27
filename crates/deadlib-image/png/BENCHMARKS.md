# Reproducible pipeline benchmarks

The user clarified on 2026-10-27 that decoding is the deliverable. Decoder
throughput, memory use and native sample parity are completion requirements.
Historical encoder timings and sizes below remain recorded as experimental
context; encoder speed and compression ratio are outside the completion gates.

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

Baseline native decode is 82–201 MiB/s versus Rust png's 680–1324 MiB/s on these
fixtures. Native encode is 21–41 MiB/s versus 22–163 MiB/s. The fixed-Huffman
encoder also produces larger files (especially the 16-bit gradient). Competitive
performance has **not** been reached. Initial candidate bottlenecks to measure:
byte-at-a-time stored-block and back-reference copying, per-sample scanline
scatter on contiguous rows, CRC throughput, and compression match selection.

First scalar optimization: `benchmark-stages-before.tsv` adds inflation timings
and shows roughly 40–64% of native decode time spent in zlib on these fixtures.
The implementation now copies stored blocks directly, expands overlapping
matches in bulk, uses slicing-by-eight CRC, and copies contiguous scanline samples
without per-channel scatter or a second previous-row buffer copy. It stays safe
Rust. No SIMD has been introduced.

`benchmark-bulk-after.tsv` (20 iterations) and `benchmark-bulk-confirm.tsv`
(40 iterations) record the results. The confirmation run measures native decode
at 165–353 MiB/s, approximately 1.8–2.3 times the stage baseline. Rust png still
measures 695–1085 MiB/s; encoding ratios and match selection remain unresolved.
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
host, 20 iterations measured collected streaming decode at 248–413 MiB/s and
row consumption at 288–519 MiB/s, with 40,762 bytes of image-pipeline storage for
RGBA8 and 48,954 for RGBA16, independent of image height. These are local
directional results with concurrent activity on the host, not controlled parity
claims. Rust png still measured 727–1310 MiB/s on these fixtures.

`benchmark-streaming-encode.tsv` adds `encode-writer` (retain the output vector)
and `encode-discard` (write to a sink). Native rows, 32 KiB DEFLATE blocks and a
64 KiB chunk buffer replace whole filtered/compressed image allocations. The
default Fast writer reported 700,666 bytes of image-pipeline storage for RGBA8
and 708,858 for RGBA16 at width 512; caller images, destination storage and
metadata validation are excluded. Identical-width tests confirm capacity does
not grow with height. Native writer throughput was 23–45 MiB/s locally and the
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

## Huffman inflation optimization

The next measurement isolates original IDAT inflation on the same 24-photo
corpus. `examples/inflate_bench.rs` checks exact filtered bytes against flate2,
then times buffered native inflation, native inflation into 8 KiB output
fragments, and flate2. It uses identical compressed bytes for all three. Its
byte rate includes filter bytes; the full PNG benchmark counts native samples.

The selected decoder refills Huffman lookahead with a portable 32-bit load and
compares long canonical prefixes directly, skipping lengths already excluded by
the nine-bit primary lookup. General field/checksum reads stay conservative.
The refill bounds leave at most the four Adler bytes buffered after EOB and
alignment, preserving the streaming prefix contract. No unsafe code or SIMD
was needed for this measured change.

`benchmark-inflate-before-*`, `benchmark-inflate-final-*` and
`benchmark-inflate-before-confirm-*` contain three warmed repetitions on logical
CPU 0. Raw inflation uses ten iterations; the photo pipeline uses five. The
confirmation reruns byte-identical saved baseline executables after selection.
`scripts/benchmark_saved.ps1` verifies their original binary hashes and every
corpus file, then carries forward the source/toolchain metadata belonging to
those binaries. All timing runs finished before the new sanitizer, coverage
and validation builds started. The host and CPU frequency remain uncontrolled.

| Operation, MiB/s | Before | Selected | Later old-binary control |
| --- | ---: | ---: | ---: |
| Native buffered inflation | 93.16 | 102.72 | 97.81 |
| Native streaming inflation | 70.17 | 74.56 | 74.15 |
| flate2 inflation | 143.22 | 137.82 | 150.67 |
| Native PNG decode | 68.19 | 78.98 | 70.81 |
| Native PNG reader | 56.69 | 64.03 | 58.54 |
| Rust png decode | 198.02 | 202.58 | 204.23 |

Each rate is total bytes divided by the sum of per-case medians of run-average
nanoseconds. The relative native/reference ratios suggest approximately 13%
better photo decoding and 10% better reader decoding against both old-code
groups. Raw inflation suggests about 15% buffered and 10% streamed. These are
inferences from a shared host, with reference drift visible in the table.
They are not controlled causal estimates or evidence of broad competitive parity.
Rust png remains over twice as fast on these photo inputs.

Separate one-run `wide-trial`, `canonical-trial` and `combined-trial` artifacts
retain the exploratory measurements. The selected combined implementation then
has the three-run measurements above. Three synthetic repetitions of ten
iterations compare the same selected and saved old binaries across RGBA8/16
gradient/noise fixtures. Native decode ranges from nearly flat on RGBA8 noise
to about 16% faster; reference timings also move. The four-case aggregate is
263.46 MiB/s versus the old control's 237.35, while Rust png measures 910.66
versus 873.02. These inputs still expose substantial filter/sample/pipeline work.
Encoded byte counts and measured pipeline capacities match between implementations.

`scripts/summarize_inflate.py` reproduces the rates, reference-relative ratios,
artifact hashes, size/capacity checks and `benchmark-inflate-summary.json`:

```powershell
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -Target inflate -CorpusDirectory ./target/deadlib-image-reference/kodak -LogPrefix ./target/inflate-current -Iterations 10 -Runs 3 -Cpu 0
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -Target corpus -CorpusDirectory ./target/deadlib-image-reference/kodak -LogPrefix ./target/png-current -Iterations 5 -Runs 3 -Cpu 0
python crates/deadlib-image/png/scripts/summarize_inflate.py
```

The summary script reads the checked-in artifact prefixes. Use the reverse
patches in `experiments/inflate-checkpoints.json` in a separate checkout to
rebuild the baseline or either component trial with the same harness. All three
patches reconstruct the exact measured source bytes. The old LZ77 experiments
retain their separate base commit and patches.

Validation records are in `inflate-validation.log` / `.json`: all 81 tests,
including the five optional C checks, strict Clippy, strict-window/compatibility
C configurations and the isolated offline build pass. The selected decoder also
passes 20,000 fresh sanitizer executions across four targets. Native coverage
and source/binary hashes are retained separately in `coverage-reports/` and
`fuzz/reports/inflate-source.json`. Broader corpora, comparable libpng timing,
filter/sample profiling and compression-ratio work remain open.

## Huffman symbol call sites

Linux cycle sampling of the current 24-photo decoder attributed 33.80% of
samples to `Huffman::decode`, 32.71% to the containing inflation loop, and
20.35% to unfiltering. The primary nine-bit lookup and canonical fallback shared
one function. The selected change lets the primary lookup inline at symbol call
sites while keeping the unchanged long-code/error path in a separate function.
Tables, refill widths, public errors, bounds and decoded bytes are unchanged.

`examples/decode_profile.rs` loads and validates the corpus before repeating
40 passes. The before/selected reports in `profiling/` retain commands, source,
corpus, executable and raw-profile hashes; neither report lost samples. In the
selected report the primary lookup appears inline inside `decompress_prefix`,
while the outlined fallback accounts for 3.42% of samples. Unfiltering remains
22.35%. Sampling attribution is separate from the Windows timing comparisons.
To reproduce a new Linux profile, with `perf` access and an unused evidence name:

```sh
python3 crates/deadlib-image/png/scripts/profile_decode.py --name local-decode --corpus target/deadlib-image-reference/kodak --target-dir /tmp/png-profile-target --raw /tmp/png-local-decode.data
```

`benchmark-huffman-call-before-*`, `split-trial-*`, `narrow-trial-*`,
`combined-trial-*` and `control-*` retain three repetitions of each of the raw
inflate, photo and synthetic benchmarks. The smaller private symbol-result
trial, alone and combined with outlining, did not retain the split-only
buffered-photo gain. The three-run split-only synthetic group initially showed
a slowdown, so selection also used saved binaries in before/split/split/before
order for **each** benchmark target. These twelve later processes verify binary
and corpus hashes through `scripts/benchmark_saved.ps1`. Their manifests retain
the original source metadata. All timing and sampling finished before final
correctness, lint, offline-build and sanitizer work began.

The interleaved groups use two process repetitions per implementation, three
warmups per operation, logical CPU 0, twenty iterations for raw/synthetic work
and five for photos. Rates divide total sample bytes by the sum of per-case
medians of run-average nanoseconds. The split-only trial is the selected source;
its historical artifact names are retained.

| Operation, MiB/s | Saved baseline | Selected split |
| --- | ---: | ---: |
| Photo IDAT native inflation | 110.50 | 125.81 |
| Photo IDAT native streaming inflation | 75.83 | 89.52 |
| Photo IDAT flate2 inflation | 145.27 | 145.16 |
| Photo native PNG decode | 84.02 | 97.76 |
| Photo native PNG reader | 70.59 | 79.63 |
| Photo Rust png decode | 218.22 | 227.68 |
| Synthetic native PNG decode | 262.81 | 275.40 |
| Synthetic Rust png decode | 925.54 | 931.25 |

Relative native/reference ratios suggest 14% better raw buffered inflation,
18% better streamed inflation, 12% better photo decoding and 8% better reader
decoding. Synthetic buffered decode suggests 4% improvement; its reader and row
paths change by approximately 1%. The initial synthetic slowdown did not recur
in the interleaved check. These remain shared-host inferences: CPU frequency
and other workloads are uncontrolled, and the table shows reference drift.
Photo Rust png decoding is still over twice as fast. This change does not
establish competitive parity, and encoding/compression-ratio work remains open.

`scripts/summarize_huffman_calls.py` reproduces
`benchmark-huffman-call-summary.json`, including all group rates,
reference-relative ratios, artifact hashes and exact case/encoded-size/sample-
size/pipeline-capacity invariants across all measurements. The selected reverse
patch and exact source hashes are in
`experiments/huffman-call-split-checkpoints.json`; applying its patch reconstructs
the `f1b22d891` baseline. The separate narrow/combined checkpoints reconstruct
those trial sources from the baseline by reversing their patches. Use
`git -c core.autocrlf=false apply` so Windows EOL conversion does not alter the
recorded bytes. Existing benchmark runners can measure each reconstructed
variant with the same harness and corpus.

`huffman-call-validation.log` / `.json` record all 114 release tests (including
the twelve optional checks), strict main/fuzz linting, strict-window zlib and
compatibility Huffman C audits, and the isolated offline 102-test run. Fresh
ASan campaigns add 80,000 successful executions across four targets; exact
source/corpus/binary hashes and logs are in `fuzz/reports/huffman-call-source.json`.

## Byte-stride filter reconstruction

The next profile attributed 22.35% of photo decode cycles to unfiltering. The
selected change rewrites the Paeth predictor as equivalent minimum/maximum
regions using a signed 16-bit threshold. All 16,777,216 byte triplets match an
independent implementation of the [Third Edition predictor](https://www.w3.org/TR/2025/REC-png-3-20250624/#9Filter-type-4-Paeth),
including ties. Another 2,550 row cases independently verify filtered bytes,
adaptive scores and reconstruction across all six byte strides, all five filter
types, short rows and first-row boundaries; 251 invalid filter bytes retain the
original error without changing the row.

Average and Paeth reconstruction use fixed-size pixel arrays, carrying the
previous pixel and its upper neighbor in registers. Two-byte pixels retain the
measured faster scalar recurrence. There are no new allocations, explicit
intrinsics or unsafe blocks. The inspected Linux executable contains packed
byte minima/maxima and word arithmetic in `unfilter`, demonstrating compiler
SIMD for the regional predictor. Commands, binary hashes, disassembly and
instruction counts are in `profiling/filter-selected-assembly.*`. The final
cycle profile in `profiling/decode-filter-selected.*` attributes 11.89% to
unfiltering and 66.47% to `decompress_prefix`, with no lost samples. These are
sampling shares, not isolated kernel timing or a portable speed guarantee.

`examples/filter_bench.rs` constructs 80 independent 513x257 inputs: eight
8-/16-bit formats, gradient/noise samples and every filter. The eight formats
cover all six byte strides. It applies the normative predictor independently,
uses test-only flate2 stored DEFLATE and CRCs, and checks both native decoding
paths and Rust png against the original bytes/samples before timing. Stored
DEFLATE emphasizes filtering while retaining PNG parsing, CRCs, copies and
allocation costs. Native output retains U8/U16 samples; Rust png's benchmark
output retains its normal wire bytes, including big-endian 16-bit values.
Sub-byte and Adam7 cases remain covered by the independent conformance suite.

The baseline and `scalar-trial`, `pixels-trial`, `hybrid-trial` and
`split-encoder-trial` groups each have three repetitions. The selected hybrid
combines fixed-stride loops with the scalar two-byte path. A subsequent
split-encoder experiment restored the original predictor for encoding; it did
not consistently remove the encoding slowdown or establish a broad benefit,
so the simpler shared predictor is retained. Exact reverse patches and source
hashes for all four variants are in `experiments/filter-*-checkpoints.json`.
Each patch reconstructs the exact `8c5713343` native source, with the same new
benchmark harness left in place.

The selected saved binaries were measured in before/after/after/before order
for the filter, synthetic and photo targets. Each process has three warmups per
operation and logical CPU 0 affinity. The filter/synthetic runs use twenty
iterations and photos use five, with two process repetitions per implementation.
Rates divide total sample bytes by the sum of per-case medians of run-average
nanoseconds. The following table uses those selected interleaved groups:

| Operation, MiB/s | Saved baseline | Selected hybrid |
| --- | ---: | ---: |
| Stored-DEFLATE filter matrix, native decode | 270.88 | 346.36 |
| Stored-DEFLATE filter matrix, Rust png | 1183.68 | 1139.68 |
| Photo native PNG decode | 97.19 | 109.49 |
| Photo native PNG reader | 79.71 | 87.31 |
| Photo Rust png decode | 225.08 | 226.46 |
| Synthetic native PNG decode | 259.69 | 305.42 |
| Synthetic Rust png decode | 941.20 | 948.89 |
| Photo native encode | 28.28 | 29.34 |
| Synthetic native encode | 48.49 | 44.46 |

Relative to the reference timings, these groups suggest approximately 33%
better filter-matrix decoding, 12% better photo decoding, 9% better photo reader
decoding and 17% better synthetic decoding. **Synthetic encoding regresses**:
buffered encoding is about 10% slower relative to the reference, and writer
paths about 5% slower. Photo encoding suggests a small gain or nearly flat
throughput, depending on the path. Restoring the original encoder predictor
did not consistently fix the synthetic loss. Native raw inflation also moves
between binaries despite unchanged inflation source, showing why these local
shared-host comparisons cannot isolate causes or establish broad parity.
CPU frequency, other workloads and linker/code-generation effects are not
controlled. The remaining decoder gap stays open;
Rust png is still over twice as fast on the photos in this table.

All raw artifacts are retained. Historical `final-*` / `interleave-final*`
filenames refer to the selected hybrid; later `selected-*` filenames refer to
the rejected split-encoder experiment. `scripts/summarize_filters.py` makes the
selection explicit with `selected_variant: hybrid`, separates both interleaved
comparisons, and reproduces all rates, per-filter/format partitions, hashes and
case/encoded-size/sample-size/pipeline-capacity invariants. Comparative timing and sampling
finished before the final validation and sanitizer campaigns started.

```powershell
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -Target filter -LogPrefix ./target/filter-current -Iterations 10 -Runs 3 -Cpu 0
python crates/deadlib-image/png/scripts/summarize_filters.py
```

Final evidence in `filter-validation.log` / `.json` records 117 release tests,
strict main/fuzz linting, strict-window/compatibility C audits, scoped formatting
and the isolated offline 105-test run. The isolated copy also builds and runs
`cargo run -p deadlib-image-png --offline --release --example filter_bench`,
checking all 80 fixtures. Its one-iteration smoke output is excluded from the
comparative timing data. Fresh ASan campaigns add 80,000 successful executions
across structured encoding, PNG streaming and buffered decoding; source, corpus,
binary hashes and logs are in `fuzz/reports/filter-source.json`.

## Decoder-only inflate experiments (2026-10-27)

`examples/decode_bench.rs` times six operations on identical static inputs:
native buffered/reader decode, Rust png decode, native buffered/streaming IDAT
inflate and flate2 inflate. Native channel counts, unpacked sub-byte samples and
u16 precision are compared before timing. No encoder runs in this harness.
The independent Python fixture builder supplies sixty 257x129 files: all fifteen
formats, plain/Adam7 and gradient/noise data, with the five filter types
distributed across images. Each image uses one filter type. Inputs use Python
zlib 1.3.1; framing, filters, packing and Adam7 are constructed independently.
`experiments/decode-corpus.json` retains generator and input hashes.

Eight trials explored wider buffered refill, literal pairs and packed Huffman
tokens with fused extra-bit decoding. **None is retained.** Every trial's patch
reconstructs its native source from `39e258611`, and recorded hashes were checked
against that reconstruction. `experiments/decoder-evidence.json` indexes all
nine reconstructions, including the separate failing arithmetic regression.
Linux cycle samples in `profiling/decode-packed-{trial,split}.*` explain inlining
decisions; samples are diagnostic and are not comparative speed claims.

The final forced-inline token candidate and saved original binary were run in
candidate/original/original/candidate order, two process repetitions at each
position and on each corpus, seven iterations and three warmups per operation,
pinned to CPU 0. No builds, profiling or fuzz campaigns from this task ran during
timing. The host and CPU frequency remain shared and uncontrolled. Weighted
rates divide total sample bytes by the sum of per-case median run-average times.

| Operation | Original MiB/s | Rejected token MiB/s | Change relative to reference |
| --- | ---: | ---: | ---: |
| 24 photos, native decode | 105.89 | 93.74 | -2.71% |
| 24 photos, native IDAT inflate | 136.64 | 116.88 | -4.26% |
| 60 formats, native decode | 222.63 | 188.81 | -7.12% |
| 60 formats, native IDAT inflate | 682.32 | 612.09 | -6.50% |

Decode changes are normalized against Rust png, inflation against flate2. The
unchanged streaming inflater's relative control changes are -0.04% on photos
and -0.13% on the format corpus. Absolute reference rates also drift, so the
relative results support rejecting the added complexity, not a precise portable
regression estimate. Raw groups and reproducible summaries are
`benchmark-token-{control,forced}-{photos,formats}-{b,c}*` and
`benchmark-token-forced-{photos,formats}-summary.json`. Earlier `refill56`,
`literals`, `packed` and `token-trial` groups are exploratory, not selected gains.
The retained runtime uses the original refill and symbol loop with a separately
verified overflow fix in consumed-byte arithmetic. Decoder performance remains
incomplete; no speedup is claimed for this milestone.

```powershell
python crates/deadlib-image/png/scripts/generate_decode_corpus.py --output target/decode-corpus-fresh
./crates/deadlib-image/png/scripts/benchmark_pipeline.ps1 -Target decode -CorpusDirectory target/decode-corpus-fresh -LogPrefix target/decode-current -Iterations 7 -Runs 2 -Cpu 0
python crates/deadlib-image/png/scripts/summarize_decoder.py --before crates/deadlib-image/png/benchmark-token-control-photos-b crates/deadlib-image/png/benchmark-token-control-photos-c --after crates/deadlib-image/png/benchmark-token-forced-photos-b crates/deadlib-image/png/benchmark-token-forced-photos-c --output target/decode-comparison-fresh.json
```

## Combined Huffman validity predicate (2026-10-27)

A smaller trial replaced `n != 0 && n <= buffered_bits` with the equivalent
unsigned comparison `n.wrapping_sub(1) < buffered_bits`. It passes all 25 focused
library/Huffman/prefix/inflate/header tests; the two C checks remain optional.
The trial changes no tables, refill rules, memory capacities or encoder code.
Its patch and exact source hashes are in `experiments/huffman-predicate.json`.

The `benchmark-predicate-*` groups use baseline/candidate/saved-baseline order,
three process repetitions per corpus at each position, twelve iterations, three
warmups and CPU 0 affinity. The final summaries combine six original processes
against three candidate processes on each corpus, with exact size, capacity,
source and binary invariants. Photo decode moves from 93.22 to 108.11 MiB/s in
absolute terms, but is **-0.77% relative to Rust png**, which also speeds up.
The all-format decode change is -0.19% relative; reader/raw-inflate results are
mixed. The shared host does not establish a consistent benefit, so the original
predicate is retained. All 21 production files match `fae2e5bd2` exactly.
No new speedup is claimed. Timing finished before native-feedback fuzzing and
coverage builds began.

## Buffered Adler-32 lanes (2026-10-27)

Buffered inflation now accumulates Adler-32 in sixteen independent u32 lanes
for blocks of at least 64 bytes. Each lane tracks its byte sum and the sum of
its running prefixes. For M groups, byte (g, i) contributes
`16 * (M - g) - i` to the second accumulator. Reducing those nonnegative lane
contributions before adding them preserves the 5,552-byte u32 bound, including
initial accumulators of 65,520. The maximum second accumulator before modulus
is 4,294,690,200. Short inputs and tails retain scalar updates. The arrays add
no heap allocation, unsafe code, explicit intrinsics or production dependencies.

The first trial applied the lanes to incremental updates too. Its six-process
interleaved comparisons favored buffered inflation but lowered photo reader
throughput by 3.64% relative to Rust png. It is not selected. The retained
candidate calls a separate bulk method from buffered `adler32`; the incremental
`Adler32::update` body exactly matches `ded016feb`. The exact patches and source
hashes for both versions are in `experiments/adler-{lanes,buffered}.*`.

The selected comparison uses saved original / buffered candidate / buffered
candidate / saved original order, three process repetitions per position and
per corpus, twelve iterations and three warmups per operation, pinned to CPU 0.
It reuses the decoder-only harness and identical 24-photo / 60-format corpora.
No profiling, builds from this task or sanitizer campaigns ran during timed
processes. Builds precede their benchmark processes. Source, binary, corpus,
sample-size, encoded-size and pipeline-capacity invariants are retained and
checked. Rates divide total sample bytes by the sum of per-case median
run-average nanoseconds.

| Buffered operation | Original MiB/s | Selected MiB/s | Change relative to reference |
| --- | ---: | ---: | ---: |
| Photo PNG decode | 91.41 | 96.47 | +1.91% |
| Photo IDAT inflate | 115.54 | 123.65 | +3.26% |
| All-format PNG decode | 228.29 | 245.90 | +5.04% |
| All-format IDAT inflate | 678.41 | 889.91 | +26.53% |

Decode changes are normalized against Rust png, inflation against flate2.
The unchanged reader path is 1.15% / 1.22% lower relative to Rust png on photos /
formats; unchanged streaming inflation is 1.71% / 2.19% lower relative to flate2.
Their absolute rates rise, along with references. These controls show remaining
shared-host and code-generation variation; no streaming speedup is claimed.
CPU frequency and external workloads are uncontrolled. These local results
support the buffered optimization, not broad parity with established decoders.

`benchmark-adler-split-{photos,formats}-summary.json` reproduces the selected
comparison from the `control`, `split`, `split_repeat` and `control2` groups.
The earlier `before`, `trial` and `repeat` groups describe the rejected shared
update; `benchmark-adler-{photos,formats}-summary.json` summarizes that trial.
Initial summaries are exploratory and excluded from the selected rates.

The Linux cycle sample in `profiling/decode-adler-buffered.*` attributes 2.09%
to buffered Adler-32 and 61.58% to buffered inflation, with no lost samples.
Huffman lookup remains 26.62% of samples. These shares are diagnostic, not
isolated kernel timings. `profiling/adler-buffered-assembly.*` records the
selected binary, disassembly command and packed widening/addition instructions,
confirming compiler SIMD for the lane accumulation. Comparative timing finished
before sampling and validation started.

`adler-validation.json` / `.log` and `adler-independent.log` retain strict
main/fuzz Clippy, scoped formatting, 128 passing tests in each normal release and
standalone offline suite, and three explicit C oracle audits. Each default suite
still ignores fifteen optional checks. Independent u64 modular arithmetic checks
both checksum paths across tail lengths, arbitrary fragmentation, varied bytes
and maximum valid accumulators. Fresh native-feedback ASan campaigns pass
100,000 decoder inputs. `experiments/adler-selection.json` links the exact final
source, reconstruction proofs, timings, profile and validation artifacts. Only
`src/checksum.rs` changes from `ded016feb`; public APIs, heap allocations and decoder
sample/metadata behavior are unchanged. Competitive decoder performance and
remaining conformance audits stay open.
