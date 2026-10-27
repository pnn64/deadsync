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

## Paired literal lookup trials (2026-10-27)

Two buffered-only experiments replace a single literal lookup with a lookup
that can return two literal bytes. The first adds a 512-entry u32 table
(2 KiB of stack) for pairs totaling at most nine bits. The second adds a
4,096-entry table (16 KiB) with twelve-bit single/pair entries. Both preserve
the canonical fallback, output limits and exact logical prefix consumption.
Incremental decoding retains its original tables. Neither trial changes public
APIs, heap allocations, production dependencies or encoding behavior.

Both candidates are rejected. Exact patches/source hashes are retained in
`experiments/primary-pairs{,-wide}.{patch,json}`; both patches reconstruct all
21 recorded production source hashes from `23a46c8ed`. The retained runtime
matches that commit byte for byte. `experiments/primary-pairs-selection.json`
links the reconstruction proofs, fourteen timing groups and four final summaries.

Each comparison uses baseline/candidate/candidate/baseline order, three
processes per position/corpus, twelve iterations, three warmups and CPU 0.
The first trial uses `before`, `trial`, `repeat`, `control`; the wider trial
uses `control`, `wide`, `wide_repeat`, `control2`. Builds finish before their
timed processes, and no tests, sanitizer jobs or profiling from this task run
during timing. The corpus, harness, toolchain, binary, sample-size, encoded-size
and pipeline-capacity invariants are verified. The host remains shared.

| Trial / PNG decode corpus | Original MiB/s | Trial MiB/s | Change relative to Rust png |
| --- | ---: | ---: | ---: |
| Nine-bit / 24 photos | 97.70 | 94.62 | -4.71% |
| Nine-bit / 60 formats | 233.25 | 245.74 | -1.54% |
| Twelve-bit / 24 photos | 96.39 | 94.63 | -1.99% |
| Twelve-bit / 60 formats | 237.46 | 242.75 | -7.67% |

Raw-inflate changes relative to flate2 are -1.44% / -5.78% for the nine-bit
trial and +1.86% / -17.97% for twelve bits (photos / formats). Unchanged
reader controls also move, by -2.65% / -4.17% and -4.82% / -5.96% relative to
Rust png. Reference normalization does not eliminate host or code-generation
variation. The measurements support rejecting the added complexity; they do
not establish portable regression percentages or a new decoder speedup.
Final summaries are `benchmark-primary-pairs{,-wide}-{photos,formats}-summary.json`.
Initial summaries are exploratory and excluded from those rates.

The independent fixtures remain as regression coverage: 65,536 ordered byte
pairs plus 132 code-width/padding cases, with flate2/C zlib exact answers,
fragmented native output, EOF/suffix consumption, limits and truncated trailers.
`primary-pairs-validation.*` and `primary-pairs-independent.log` retain the
final normal/standalone checks. Competitive decoder performance remains open.

## Initialized output-slice trial (2026-10-27)

A buffered-only trial fills the already allocated Vec tail with zeroes, clips
it to the configured output limit and decodes into a slice using a separate
written cursor. At least 258 writable bytes admit any legal match, replacing
per-token limit/capacity checks. The slice contains initialized bytes, and
history checks use the written cursor rather than the temporary Vec length.
The trial adds no allocation, unsafe code, production dependency or table/refill
change; streaming and encoding logic are unchanged. It passes 27 focused tests
with three optional checks ignored, but is rejected on measured performance.

`experiments/output-spare.{patch,json}` retain the exact candidate.
`experiments/output-spare-selection.json` reconstructs all 21 source hashes from
`861a15e00` and links eight timing groups / two final summaries. The selected
production files match that baseline byte for byte.

The comparison uses baseline/candidate/candidate/baseline order, three processes
per position/corpus, twelve iterations, three warmups and CPU 0 affinity.
The 24-photo and 60-format corpora, harness, toolchain, source/binary identities
and reported size fields are verified. The harness reports zero in its pipeline
memory column; equality of that column is not an allocation or peak-memory
measurement. Builds precede their timed processes. No tests, sanitizer jobs or
profiling from this task run during timing; external workloads and frequency
remain uncontrolled, including Cargo-lock contention between timed groups.

| Buffered operation | Original MiB/s | Trial MiB/s | Change relative to reference |
| --- | ---: | ---: | ---: |
| Photo PNG decode | 101.24 | 99.03 | -2.59% |
| Photo IDAT inflate | 128.67 | 126.08 | -2.32% |
| All-format PNG decode | 248.66 | 256.62 | -3.45% |
| All-format IDAT inflate | 904.35 | 952.67 | -5.34% |

Decode is normalized against Rust png, inflation against flate2. Unchanged
reader controls are -1.57% / -1.96% relative on photos / formats; unchanged
streaming inflation is -1.26% / -8.93%. This drift limits precise regression
estimates. The results do not establish a benefit for the added output loop,
so it is not retained and no speedup is claimed. Final summaries are
`benchmark-output-spare-{photos,formats}-summary.json`; the initial photo
comparison is exploratory. Competitive decoder performance remains open.

## Streaming literal-run state fusion (2026-10-27)

The streaming inflater now writes consecutive literals directly from its symbol
state, updating the existing history ring and output counter. This avoids a
second state dispatch and a one-byte ring-copy operation for every literal.
A decoded literal still enters the pending Literal state when output is full;
input suspension, logical limits, checksum batching, advertised windows and
prefix consumption retain their existing rules. The struct, 32 KiB allocation,
Huffman tables and refill rules are unchanged. Only `src/zlib/inflate.rs` changes
in production; buffered inflation and encoding logic remain unchanged.

The exact patch/source hashes are in `experiments/stream-literals.{patch,json}`.
`experiments/stream-literals-selection.json` reconstructs all 21 selected source
hashes from `eb9d4ef9d` and links eight timing groups / two final summaries.
The comparison uses baseline/candidate/candidate/baseline order, three process
repetitions per position/corpus, twelve iterations, three warmups and CPU 0.
Identical 24-photo and 60-format corpora, source/binary identities, harness,
toolchain and reported size fields are verified. The pipeline memory column
is zero and is not a peak-allocation measurement. Source inspection and fixed
working-memory assertions establish the unchanged inflater allocation model.

| Streaming operation | Original MiB/s | Selected MiB/s | Change relative to reference |
| --- | ---: | ---: | ---: |
| Photo PNG reader decode | 76.20 | 79.16 | +5.81% |
| Photo IDAT streaming inflate | 82.09 | 86.97 | +8.29% |
| All-format PNG reader decode | 232.66 | 246.08 | +4.74% |
| All-format IDAT streaming inflate | 474.81 | 557.47 | +11.53% |

Reader changes are normalized against Rust png, streaming inflation against
flate2. Unchanged buffered PNG controls are -0.16% / -0.51% relative on photos /
formats; unchanged buffered inflation is +0.46% / -2.02%. Absolute rates drift
along with references. No builds, tests, sanitizer jobs or profiling from this
task run during timed processes. External workloads and CPU frequency remain
uncontrolled, so these measurements support this local change without proving
broad decoder parity. Final comparisons are
`benchmark-stream-literals-{photos,formats}-summary.json`; initial comparisons
are exploratory and excluded from those rates.

Independent fixed/dynamic fixtures check long literal runs, back-references,
two history wraps, empty output calls, one-byte fragments, limits, checksums
and exact prefix consumption against flate2 and C zlib. The language-extension
suite also verifies compressed/uncompressed metadata and native PNG readers.
`stream-literals-validation.*` retains full release, scoped independent offline,
strict main/fuzz lint and C-reference checks. Competitive decoder performance
remains open.

The selected Linux reader-path cycle sample is retained in
`profiling/reader-stream-literals.*`. It runs after all comparative timing and
sanitizer jobs, with no lost samples. The host caps the requested 499 Hz at an
initial 250 Hz. Direct inflater-loop instructions account for 28.15% of samples,
Huffman decoding 16.43%, refill 6.01% and incremental Adler updates 3.83%.
The profile links exact sources, corpus, binary and raw data hashes; its shares
are diagnostic and do not establish an isolated kernel timing improvement.

## Streaming match-header dispatch fusion (2026-10-27)

The streaming symbol loop now reads length extra bits, a distance code and its
extra bits directly. It saves LengthExtra, DistanceCode and DistanceExtra before
each potentially incomplete read, preserving the existing resumption paths.
Complete match headers bypass three outer state dispatches. Both paths call the
same inlined copy validation, preserving history/window checks before output
limits. Copying, literal runs, checksums, bit refill and tables are unchanged.
Only `src/zlib/inflate.rs` changes in production, with no struct, allocation,
unsafe-code, dependency or encoding changes.

The baseline is the selected literal-loop source in `79ccce5be`, using its
verified saved executable. `experiments/stream-matches.{patch,json}` retain the
candidate; `stream-matches-selection.json` reconstructs all selected source hashes
and links eight timing groups / two final summaries. Measurements use
baseline/candidate/candidate/baseline order, three processes per position/corpus,
twelve iterations, three warmups and CPU 0. The 24-photo / 60-format corpora,
harness, toolchain, exact sources/binaries and reported size fields are verified.
The zero-valued pipeline-memory column does not measure peak allocation.

| Streaming operation | Original MiB/s | Selected MiB/s | Change relative to reference |
| --- | ---: | ---: | ---: |
| Photo PNG reader decode | 74.71 | 86.43 | +6.88% |
| Photo IDAT streaming inflate | 82.44 | 94.58 | +8.31% |
| All-format PNG reader decode | 239.78 | 243.62 | +3.08% |
| All-format IDAT streaming inflate | 537.86 | 549.32 | +4.80% |

Reader comparisons use Rust png; inflation uses flate2. The unchanged buffered
photo controls are -0.35% for PNG decode and +0.86% for inflation, supporting
the photo-path improvement. Format controls improve more than the changed
streaming paths: +4.72% / +5.34% versus +3.08% / +4.80%. The format comparison
is therefore inconclusive; no independent format speedup is claimed.
External load, frequency, cache and code-generation variation remain uncontrolled.
These results support the local photo improvement rather than broad parity.
No tests, sanitizer jobs or profiling from this task run during timed processes.
Final rates are in `benchmark-stream-matches-{photos,formats}-summary.json`;
initial comparisons remain exploratory.

Validation reuses existing independently assembled fixtures covering every
legal length/distance value, all eight bit offsets, fixed/dynamic trees and
fragmented input/output. `stream-matches-validation.*` retains full release,
strict lint, explicit C zlib and fresh focused offline checks. Fresh native-
feedback campaigns reuse the exact immutable seed ZIP from the literal-loop
milestone with new mutable corpus copies. Decoder performance remains open.

## Streaming complete-match copies (2026-10-27)

Complete LZ77 matches now copy inside the streaming symbol loop, avoiding the
outer Copy/Codes dispatch when the caller's output space holds the whole match.
Partial matches still suspend in Copy. Both paths use the same inlined history
seed, doubling copy and ring update, with the existing window and output-budget
checks before copying. Only `src/zlib/inflate.rs` changes in production; state
layout, allocations, checksum batching, Huffman tables and bit refill stay fixed.

The baseline is `788659fb4`, using its verified saved match-header executable.
`experiments/stream-copy.{patch,json}` retain the candidate, and
`experiments/stream-copy-selection.json` verifies its reconstruction and all
eight timing groups. Baseline/candidate/candidate/baseline positions each use
three processes per corpus, twelve iterations, three warmups and CPU 0. The
24-photo and 60-format corpora, harness, toolchain, binary/source bytes and
reported size fields agree. No task tests, fuzzing or profiling overlap timing.
The zero pipeline-memory column is not a peak allocation measurement.

| Streaming operation | Baseline MiB/s | Selected MiB/s | Change relative to reference |
| --- | ---: | ---: | ---: |
| Photo PNG reader decode | 88.23 | 91.09 | +6.31% |
| Photo IDAT streaming inflate | 96.92 | 100.90 | +9.76% |
| All-format PNG reader decode | 248.19 | 251.13 | -0.92% |
| All-format IDAT streaming inflate | 552.09 | 567.17 | +1.28% |

Reader comparisons use Rust png; inflation uses flate2. The unchanged buffered
photo controls move -0.02% / +1.46% relative to those references, supporting a
local photo-stream improvement. Absolute photo reader/inflate-stream gains are
+3.25% / +4.10%; reference normalization reflects a slower host during candidate
runs. The format controls move -2.26% / -2.09%, and changed paths move -0.92% /
+1.28%. These smaller mixed results do not establish an independent format
speedup. Shared-host scheduling, frequency, cache and code-generation effects
remain uncontrolled; broad decoder parity is still unproven.

The first baseline invocation completed all three photo measurements, then its
current-source guard stopped because the candidate had already been edited.
Only the missing format group was resumed, verifying the baseline manifest
against committed source bytes and the same saved executable. No completed
measurement was overwritten. Final combined comparisons are in
`benchmark-stream-copy-{photos,formats}-summary.json`; the first-pair summaries
remain exploratory.


`stream-copy-validation.*` retains the complete 134-test release run, strict
lint, the 19,645-case C zlib audit and a fresh offline six-test Huffman run.
All 885 copied source/fixture/example/benchmark files match. Fresh decoder ASan
campaigns complete 70,852 observed executions; their count-check recovery and
exact source/binary/corpus evidence are documented in `COVERAGE.md` and
`fuzz/reports/stream-copy-source.json`.

## Rejected full-lookahead Huffman split (2026-10-27)

The nine-bit primary table only contains code lengths 1-9. Two experiments
return lookahead completeness from the unchanged refill and move the original
short-code/count check into an outlined partial-input helper. The complete-input
path checks only for a nonzero table entry. The second experiment additionally
forces only the primary `Huffman::decode` function inline. Both patches are
independently reconstructible from `76d754d2a`; only `src/zlib.rs` differs in each.
Neither experiment is selected, and all 21 production sources are restored.

Measurements run baseline/split/baseline/forced-inline/baseline, with three
processes per corpus at every position, twelve iterations, three warmups and
CPU 0. Each final comparison combines its two adjacent baseline positions
(six processes) against its three candidate processes. The native sample,
reader and raw outputs agree with references on 24 photos and 60 format files.
Ten manifests bind all thirty processes to exact sources, harnesses, corpora,
toolchains and saved binaries. No task builds, tests, fuzzing or profiling run
during timed processes. Shared-host scheduling and frequency remain uncontrolled.

All entries below are percentage changes relative to Rust png for PNG operations
and flate2 for raw inflation; they do not claim absolute gains.

| Candidate / corpus | Buffered PNG | Reader PNG | Buffered inflate | Streaming inflate |
| --- | ---: | ---: | ---: | ---: |
| Split / photos | -9.77% | -0.26% | -12.10% | -1.06% |
| Split / formats | -0.03% | +2.63% | -5.31% | +4.81% |
| Forced inline / photos | -1.28% | +1.08% | -1.85% | -4.07% |
| Forced inline / formats | -0.07% | -2.18% | +0.31% | +2.07% |

The first variant emits a 323-byte outlined primary decoder, called by the
buffered decompression loop. The forced-inline variant removes that outlined
function and grows buffered `decompress_prefix` from 3,225 to 3,700 code bytes.
It recovers much of the first variant's buffered loss, but still establishes
no consistent decoder benefit. Exact PE/PDB hashes and extracted assembly are
retained in `experiments/huffman-lookahead*-codegen.json` and companion text.
This is generated-code evidence, not a standalone kernel timing.

`experiments/huffman-lookahead-build-inputs.json` also checks declared build
inputs against the saved baseline's recorded source commit. Profiles, patches,
package manifests and native/reference lock nodes agree; an unrelated workspace
release-version increment is recorded separately. The raw run manifests retain
their original source bindings.

`experiments/huffman-lookahead-selection.json` reconstructs both candidates,
recomputes all four final summaries and verifies restored source bytes. The
first candidate passes 29 focused tests with three external checks ignored;
the second changes only its inline annotation and is checked on the benchmark
corpora. No full second-candidate suite or fresh sanitizer campaign is claimed.
Existing full validation remains applicable to the byte-identical restored
sources. Initial pair summaries remain exploratory. Decoder performance remains
open; encoding remains outside the deliverable.
