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

## Current buffered profiles and rejected marker reservoirs (2026-10-27)

Fresh Linux profiles bind the selected `b19a1d1b3` production bytes, shared
primitives, entry point and 24-photo corpus to the saved executable and raw
recordings. Buffered decode captures 5,936 samples and raw inflation 4,016,
with zero lost samples. The requested 499 Hz is capped by the host at 250 Hz.
Setup validates the corpus, including native/reference raw checks for inflation,
before eighty repeated corpus passes. These are diagnostic cycle attributions;
setup, sampling skid and shared-host effects prevent isolated-kernel timing claims.

Buffered decode attributes 61.95% to decompression, including 27.21% to inlined
primary Huffman lookup and 7.40% to refill; unfiltering accounts for 11.97%.
Raw inflation attributes 78.18% to the containing decompressor, including 34.36%
to primary lookup and 9.64% to refill. `perf annotate` exits with SIGSEGV on the
saved trace. The fallback maps original `perf script` symbol offsets to matching
static disassembly, retaining commands, raw samples, assembly and hashes in
`profiling/inflate-b19a1d1b3.json`. Hot-address attribution is not instruction
latency evidence. The raw recordings and dedicated Linux binary remain at the
paths in both profile manifests.

Three decoder-only prototypes replace the separate bit count with a marker above
the available data: an explicit marker replacement, an equivalent one-shift
addition refill, and that addition variant with only the primary lookup forced
inline. The arithmetic form uses the old marker as part of the addition; its
representation and overflow/partial-input proof are retained in the checkpoint.
All byte/32-bit refill boundaries remain unchanged. Each patch reconstructs both
changed source files from `b19a1d1b3`. All three candidates are rejected; the
explicit bit-count implementation is restored exactly.

Comparisons run baseline/marker/baseline/addition/baseline/inline/baseline.
Every position uses three processes per corpus, twelve iterations, three warmups
and CPU 0. Each final comparison combines its adjacent baseline positions
(six processes) against three candidate processes. Fourteen manifests verify
all forty-two processes, with 24 photos / 60 format inputs and identical harness,
toolchain, corpus and reported size fields. Peak allocation is not measured by
the zero pipeline-memory column. No task builds, tests, fuzzing or profiling
overlap timed processes; host frequency and unrelated work remain uncontrolled.

Percentage changes below are relative to Rust png for PNG operations and flate2
for raw inflation. Absolute rates and exploratory first pairs remain retained.

| Candidate / corpus | Buffered PNG | Reader PNG | Buffered inflate | Streaming inflate |
| --- | ---: | ---: | ---: | ---: |
| Marker / photos | -3.14% | -2.27% | -3.43% | -1.37% |
| Marker / formats | -3.07% | -3.08% | -6.88% | -2.49% |
| Addition / photos | -11.09% | -1.40% | -13.89% | -1.78% |
| Addition / formats | -2.35% | +0.97% | -12.14% | +4.05% |
| Forced inline / photos | -5.93% | -2.78% | -2.32% | -0.27% |
| Forced inline / formats | -3.79% | -3.51% | -8.41% | +2.60% |

The addition candidate emits a 322-byte outlined primary lookup called twice
from buffered decompression. Forcing it inline removes that function and grows
`decompress_prefix` from 4,030 to 4,754 code bytes, but does not establish a
consistent decoder benefit. PE/PDB and extracted assembly hashes bind this
observation to the timed executables. Neither shorter source nor fewer logical
state fields establishes faster generated code.

Both reservoir/refill algorithms pass 29 focused tests, with three external C
checks ignored; the addition candidate also passes strict main all-target Clippy.
The third changes only its inline annotation and agrees with references on all
84 benchmark files. No full candidate suite or new sanitizer campaign is claimed.
All 21 production sources and 885 source/fixture/example/benchmark files match the
previous tested standalone copy, preserving its 134-test / 70,852-execution
validation. `experiments/sentinel-selection.json` verifies reconstruction, exact
sources, fourteen timing groups, six recomputed summaries and both profiles.
The measured decoder performance requirement remains open.

## Buffered literal runs and coded-block frames (2026-10-27)

The buffered decoder now appends complete primary-table literals directly from
already-prefetched bits, up to the smaller of existing output capacity and the
remaining output limit. The run performs no refill or allocation, and stops
before an empty/long-code entry, match or end-of-block. The first literal and all
slow/error paths retain their existing behavior. Fixed/dynamic table construction
and token decoding move into one private function called once per coded block,
keeping those table frames out of stored-block decoding. Streaming code and
public APIs are unchanged.

Four independently reconstructible candidates retain their source hashes,
patches, PE/PDB hashes and generated code. Eight-byte staging with variable-width
`memcpy` regresses photo decoding/inflation. Constant-width staged copies grow
the containing function from 4,365 to 5,556 code bytes without a convincing
overall gain. Direct literal runs shrink it to 3,874 bytes, but longer checks
expose a stored-only regression even though the new literal loop is not entered.
The selected per-block extraction leaves a 2,079-byte outer decoder and a
4,502-byte coded-block helper. Primary lookup remains inlined in all four builds.

Normal comparisons retain 24 timing groups / 72 processes across 24 photos and
60 format files. Each group uses three processes, twelve iterations, three warmups
and CPU 0. The selected comparison combines six baseline processes bracketing
six candidate processes. Twelve longer groups / 36 processes examine the earlier
per-case regressions: two photo and ten format inputs for the direct-run variant,
then the two stored-only files for the selected extraction. The final stored
probe uses 4,096 iterations per file per process. These subsets retain source-only
provenance templates and exact original input bytes; no timing is attributed to
the derived templates themselves.

| Selected corpus | Buffered PNG | Unchanged reader | Buffered inflate | Unchanged streaming inflate |
| --- | ---: | ---: | ---: | ---: |
| Photos, reference-relative change | +2.65% | +0.89% | +2.64% | -1.68% |
| Formats, reference-relative change | -0.25% | -1.04% | +9.43% | -0.60% |

PNG changes are normalized to Rust png; raw inflation uses flate2. Absolute photo
decode throughput rises from 98.98 to 108.58 MiB/s, while Rust png rises from
198.58 to 212.21. Photo raw inflation rises from 126.70 to 141.28 MiB/s; format
raw inflation rises from 946.43 to 1,091.43. The unchanged controls support a
local photo and raw-format benefit. End-to-end format results remain inconclusive,
and photo decoding remains substantially behind Rust png.

The longer stored-only probe does not repeat the direct-run variant's 10.23%
loss relative to unchanged streaming inflation. For gray8 Adam7 noise, selected
raw inflation is -5.06% relative to flate2 and -2.58% relative to streaming
inflation; the other stored case is -0.88% / -0.29%. Both stored cases improve
end-to-end relative to both controls. No stored-only speedup is claimed.

`experiments/literal-selection.json` verifies four reconstructed candidates,
all 108 timed processes, nineteen recomputed final/interim summaries, per-case
comparisons and generated-code artifacts. Timed runs overlap no task builds,
tests, fuzzing or profiling. Shared-host frequency and unrelated workloads remain
uncontrolled; normalization does not eliminate their effects. Reported pipeline
byte fields are invariant checks, not measured peak memory. The broader decoder
performance requirement remains open.

## Owned 8-bit reconstruction and bounded growth requests (2026-10-27)

Ordinary noninterlaced 8-bit images now retain the inflated vector as their native
sample storage. Each row is unfiltered against the already compacted previous
row, then moved left over its filter byte. The destination ends before the next
unread row. This removes the second full-image allocation and its zero fill;
row movement remains. Packed, 16-bit and Adam7 reconstruction keep their existing
sample representation and algorithms. Buffered APNG frames use the same owned
reconstruction entry point; public APIs and incremental decoding are unchanged.

Buffered inflation still allocates only as output is produced. Its geometric
growth requests are capped at the logical output limit rather than retaining
unused space from the last doubling. No allocation based on the entire declared
PNG size is added. On the measured global allocator, final inflation capacity
equals filtered size on all 84 corpus images. The reused sample vector retains
only the removed filter-byte overhead beyond its exact sample length.

An untimed probe builds exact baseline/current native sources in separate
projects and saves native sample bytes for byte-for-byte comparison. Its four
tables and source/binary hashes are retained in `experiments/row-reuse-buffers.json`.
The original incomplete-copy manifest parse error and successful continuation
are separate logs; no measurement preceded the copy correction.

| Summed capacities across 24 photos | Baseline | Selected |
| --- | ---: | ---: |
| Inflation buffer, bytes | 51,036,160 | 28,325,376 |
| Returned sample buffer, bytes | 28,311,552 | 28,325,376 |
| Separate sample buffer allocated during reconstruction | Yes | No |

These are observed vector capacities from separate calls, not a measured
simultaneous allocation peak or process RSS. The selected sample vector is the
inflation allocation itself. Native sample bytes are identical for all 84 files;
the other 50 format-corpus layouts retain their previous sample capacity.

Baseline/candidate/candidate/baseline timing uses three processes per position,
twelve iterations, three warmups and CPU 0. Each final comparison combines six
baseline and six candidate processes. Rates include allocations. No task builds,
tests, fuzzing or profiling overlap timed runs; unrelated host load and frequency
remain uncontrolled.

| Corpus | Buffered PNG | Unchanged reader | Buffered inflate | Unchanged streaming inflate |
| --- | ---: | ---: | ---: | ---: |
| Photos, reference-relative change | +5.28% | +1.86% | -0.50% | -1.48% |
| Formats, reference-relative change | +10.28% | +0.26% | +18.36% | +2.78% |

PNG uses Rust png normalization; raw inflation uses flate2. Photo decode changes
from 96.90 to 103.77 MiB/s, while Rust png changes from 190.47 to 193.74. Format
decode changes from 258.11 to 263.25 MiB/s. The controls support local PNG gains;
photo raw inflation remains inconclusive, and broad parity remains unproved.

Longer checks cover every case flagged below -5% against both controls: one
photo and nine format files, using 48 / 4,096 iterations. None retains a PNG
decode loss beyond that threshold against both controls. One stored-only raw
case, gray8 Adam7 noise, remains slower: 9.02 to 11.02 microseconds, -28.49%
relative to flate2 and -12.03% relative to unchanged streaming inflation.
The lower-capacity allocation policy has a documented per-input tradeoff;
no uniform raw-inflate speedup is claimed.

`experiments/row-reuse-selection.json` reconstructs all 21 production source
hashes, verifies sixteen timing groups / 48 processes and recomputes six
final/interim summaries. It links per-case checks, buffer probes, generated code,
full validation and sanitizer evidence. The final decoder performance and
requirements audits remain open.

## Stored-block allocation requests and allocator observations (2026-10-27)

Nonfinal buffered stored blocks now request room for up to one additional payload,
clipped to the remaining output limit. Final blocks keep their previous request.
LEN/NLEN and logical output checks still precede allocation; no complete declared-
image allocation is introduced. Only `src/zlib.rs` changes in production.

An untimed, single-threaded [System](https://doc.rust-lang.org/std/alloc/struct.System.html)
wrapper observes successful [GlobalAlloc](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html)
calls and requested layout bytes. Its four methods are calibrated before each
probe process. One fewer realloc call is observed for 21 format-corpus images,
in both PNG decoding and raw inflation. Photos have unchanged counts. All 84
images retain identical native sample bytes and logical peak/retained requested
bytes. The gray8 Adam7 stored case changes from two realloc calls to one, with
the same 33,397-byte raw peak and retained allocation.

The instrumented binaries are never timed. Logical peaks exclude allocator
overhead, usable-size rounding, internal realloc overlap and process RSS; compiler
allocation elision can affect observed calls. Inputs and prior caller-owned
results are outside each measured API call. The standalone instrumentation uses
System forwarding; the native codec retains `forbid(unsafe_code)`.
`experiments/stored-growth-allocations.{rs,py,json,lock}` retain its source,
as-run Windows driver, dependency lock, binary/source hashes and four result tables.

| Corpus | Buffered PNG | Unchanged reader | Buffered inflate | Unchanged streaming inflate |
| --- | ---: | ---: | ---: | ---: |
| Photos, reference-relative change | +0.41% | -1.41% | +1.39% | +0.76% |
| Formats, reference-relative change | -0.63% | +0.14% | +2.56% | +4.22% |

PNG uses Rust png normalization and raw inflation uses flate2. Broad throughput
results are inconclusive; retain the change for the verified allocation reduction.
The longer three-file stored subset improves raw inflation 3.22% relative to
flate2 while unchanged streaming inflation moves -4.24%. Gray8 raw inflation
changes from 7.59 to 6.60 microseconds, but normalized gains there are only
1.73% / 1.58% against flate2 / unchanged streaming inflation. Palette results
remain inconclusive. Two additional longer outlier checks do not repeat the
short-run gray8 PNG or RGB16 raw regressions; their unchanged-control changes
are -1.52% and +3.50%, respectively. No uniform or broad speedup is claimed.

Sixteen final ABBA groups / 48 processes use three runs per position, CPU 0,
three warmups, twelve iterations for the full corpora and 4,096 for subsets.
Task builds/tests/fuzzing/profiling do not overlap timed runs; shared-host load
and frequency remain uncontrolled. The old zero pipeline-byte column is not
used as a memory measurement.

The saved-binary runner now replaces inherited control lineage with the immediate
parent and current runner hashes. Previously, deriving an already saved manifest
ran its three processes but failed to write metadata. Those three stdout files
are retained and excluded; the corrected comparison uses fresh `-b` names.
`experiments/stored-growth-focus-recovery.json` binds the archived/fixed runners,
failure/completion logs and exact parent lineage. `stored-growth-selection.json`
in the same directory reconstructs all 21 native source hashes and verifies four
summaries, per-case results, allocator observations and full validation evidence.

## Rejected refilling literal-run loop (2026-10-28)

A buffered-only trial keeps primary-table literal runs in their tight loop when
the bit reservoir needs refilling. It loads four bytes only below nine available
bits, preserves the existing lookahead bound, and leaves matches, long codes,
end-of-block and input tails on the original symbol path. Output capacity and
logical limits still bound each run. The trial changes only `src/zlib.rs`; it
adds no allocation, dependency or unsafe code.

Fresh baseline/candidate binaries use the same toolchain, workspace profile,
harness and dependency lock. All builds and correctness checks finish before
baseline/candidate/candidate/baseline timing. Each position runs three processes
per corpus with twelve iterations, three warmups and CPU 0 affinity: 24 timed
processes across 24 photos and 60 generated format files. Every process checks
native samples and raw output against independent references before timing.
Shared-host frequency and unrelated work remain uncontrolled.

| Corpus | Buffered PNG | Unchanged reader | Buffered inflate | Unchanged streaming inflate |
| --- | ---: | ---: | ---: | ---: |
| Photos, reference-relative change | -1.92% | -0.70% | -0.89% | +0.74% |
| Formats, reference-relative change | +1.94% | +0.75% | +6.36% | +0.00% |

PNG is normalized against Rust png, raw inflation against flate2. Absolute photo
decoding changes from 103.09 to 97.34 MiB/s, while the reference and unchanged
controls also slow down. Photo inflation changes from 127.33 to 119.30 MiB/s.
Format inflation rises from 1,040.52 to 1,096.73 MiB/s. The local format result
does not establish an overall PNG benefit; no photo speedup is demonstrated.
The candidate is rejected and all 21 production files are restored exactly to
`b2b3a745`. No precise kernel-regression or uniform-throughput claim is made.

The candidate passes 41 focused tests with five optional C checks ignored,
strict all-target Clippy and scoped rustfmt. No full candidate suite or new
sanitizer campaign is claimed. The restored source and all recorded regression,
fixture, harness and tool inputs match the previously validated conformance
milestone. Zero pipeline-byte columns are not allocation or peak-memory data.
`experiments/literal-refill-selection.json` verifies patch reconstruction, both
saved binaries, all eight timing groups and both recomputed summaries. The
source-only parent templates contain no measurements; their scope text remains
in the saved-run manifests alongside actual process measurements and parent
lineage. The selected decoder's performance requirement remains open.
## Reconstruction CPU cycles and allocations (2026-10-28)

Three sequential changes simplify native sample reconstruction and explicit
RGBA16 expansion. Production remains safe Rust under `forbid(unsafe_code)`.

1. Compute the RGBA scale once per image/frame. Every legal sample mask divides
   65535 exactly, so multiplication replaces division for each output component.
   Palette expansion and transparency comparisons retain their existing rules.
2. Unpack contiguous 1/2/4-bit rows in fixed groups per encoded byte. This removes
   the general pixel/channel scatter loop and its runtime source-index arithmetic
   from those rows, including Adam7's contiguous final pass. Other Adam7 passes
   retain their existing scatter path; partial final bytes are bounded by the
   destination sample count.
3. Reconstruct each pass's first row directly: Up is identity, Paeth is Sub, and
   Average adds half the reconstructed left byte. This removes one allocation,
   zero fill and predictor-row read per nonempty pass. Invalid filters still
   return the same error before modifying samples.

Measurements use the Intel Xeon E5-2696 v4, Windows x86-64 and Rust 1.99.1,
with the workspace bench profile (optimization level 3, LTO, debug info 1).
The new `examples/reconstruction_bench.rs` uses Windows `QueryThreadCycleTime`
and pins the current thread to logical CPU 2. Each checkpoint has three original
processes and three saved-binary controls, replayed in reverse checkpoint order.
Every process warms each operation three times, then takes the median of seven
12-iteration batches. The table reports medians across all six processes per
checkpoint. Allocations/reallocations and requested byte totals come from a
separate untimed operation; counters are disabled during timed batches. Bytes
are allocation requests, including full replacement sizes on reallocations,
rather than live memory or RSS. Throughput counts native output sample bytes.

| Change and operation | Before cycles/image | After cycles/image | CPU throughput | Before MiB/s | After MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| Scale: RGBA8 to RGBA16, 513x257 | 6,716,828 | 2,742,890 | 2.45x | 327.9 | 802.0 |
| Scale: RGBA16 to RGBA16, 513x257 | 6,893,809 | 2,658,182 | 2.59x | 319.2 | 827.2 |
| Scale: RGBA8 APNG SOURCE, 513x257 | 6,196,110 | 1,816,984 | 3.41x | 355.3 | 1,211.8 |
| Unpack: grayscale1 decode, 4097x257 | 6,663,023 | 2,827,983 | 2.36x | 331.1 | 775.5 |
| Unpack: grayscale2 decode, 4097x257 | 7,198,064 | 3,408,890 | 2.11x | 306.5 | 644.3 |
| Unpack: grayscale4 decode, 4097x257 | 8,409,905 | 4,684,587 | 1.80x | 261.3 | 469.2 |

Scaling's RGBA16 expansion gain spans 1.38x to 2.59x across grayscale, GA, RGB
and RGBA at 8/16 bits; APNG SOURCE spans 1.61x to 3.41x. These changes preserve
allocation counts. Unpacking's inputs use identical fixed-seed samples and the
None filter; timings include the entire native buffered decoder.

| First-row change: buffered decode | Before allocations | After allocations | Before requested bytes | After requested bytes |
| --- | ---: | ---: | ---: | ---: |
| RGBA8 33x1, each of all five filters | 3 | 2 | 409 | 277 |
| Grayscale1 4097x257 | 4 | 3 | 1,840,900 | 1,840,387 |
| Grayscale8 4097x257 | 3 | 2 | 7,217,667 | 7,213,570 |
| Grayscale16 4097x257 | 4 | 3 | 16,672,007 | 16,663,813 |

First-row reallocations are unchanged. Short-image cycle medians improve
14–30% in these measurements, but their very small batches and the shared host
make exact timing percentages less reliable than the deterministic allocation
reduction. Small changes in unrelated cases are not attributed to an
optimization. Host frequency and competing workloads remain uncontrolled.

Raw TSVs, source/binary/toolchain identities, original build logs, reverse-order
controls, incremental patches and full summaries are in
[`experiments/reconstruction-20261028`](experiments/reconstruction-20261028).
`reconstruction_checkpoints.py` verifies every archived transition against every
production source hash recorded at each build, accounting for checkout line
endings. All 26 output fingerprints agree across all 24 measured processes.
The decoder fixtures also compare complete native images with their generated
sample vectors outside timing.

`measured-benchmark.rs` preserves the exact harness used for those measurements.
The current example resolves the identical CPU helper inside the PNG package,
so standalone package builds also work. The helper's bytes and the measured
production source are unchanged by that path relocation.

The source diff adds 32 production lines net (48 added, 16 removed), plus ten
lines extending independent filter and invalid-filter checks. The removed work
is per-channel division, runtime packed-sample scatter arithmetic and per-pass
predictor allocation/initialization. No public API or dependency changes.

The complete release suite, including the optional C/reference checks, passes
166 tests with zero failures or skips. Focused debug unit/static/compositing
checks also pass, including the 3,240-case static matrix and every Paeth byte
triplet. All-target Clippy with warnings denied and scoped formatting pass.
Commands, source/reference binary hashes and output are retained in
`validation.json`, `release.log`, `debug.log`, `clippy.log` and `fmt.log` in the
evidence directory.

The standalone offline debug suite passes 145 tests. Its 21 optional reference
checks ran separately in the complete release suite above. All 894 copied
source/test/example/benchmark files match the current package, and the normal
dependency tree contains only `deadlib-image`; see `independent.log` and
`independent.json` in the same evidence directory.

## Native pixel expansion and opaque blending (2026-10-28)

Three changes accelerate explicit RGBA16 expansion and APNG rendering:

1. Native RGBA8 widens directly into the destination slice; RGBA16 copies its
   native slice. This removes per-pixel color/storage dispatch and channel
   indexing; native RGBA16 also avoids per-channel multiplication.
2. Indexed images with at least 256 pixels build a 2 KiB stack table of RGBA16
   values. Each pixel then needs one indexed load. Missing palette entries stay
   opaque black, and missing transparency entries stay opaque. Smaller images
   keep direct expansion, avoiding the fixed table initialization cost.
3. OVER against an opaque destination uses exact u32 interpolation with the
   constant divisor 65535. The destination alpha stays opaque. Transparent
   source pixels skip the remaining blend work. The shared pixel kernel is
   explicitly inlined to retain fast grayscale expansion and SOURCE rendering.

With M=65535, an opaque destination makes the original alpha denominator M².
Its rounded channel result is exactly `(source*sa + destination*(M-sa) + 32767)
/ M`. M is odd, so removing the common factor preserves rounding. The largest
new numerator is M²+32767=4,294,868,992, which fits u32. This removes variable
u64 channel divisions and repeated alpha normalization from opaque pixels.
Production remains safe Rust under `forbid(unsafe_code)`.

| Change / 513x257 case | Before cycles | After cycles | CPU throughput | Before MiB/s | After MiB/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| Direct RGBA8 expansion | 2,610,093 | 1,132,995 | 2.30x | 843.1 | 1,940.0 |
| Direct RGBA16 expansion | 2,677,690 | 1,223,191 | 2.19x | 815.2 | 1,802.6 |
| Indexed8, two palette entries | 2,264,920 | 1,153,454 | 1.96x | 970.7 | 1,904.1 |
| Indexed8, 256 palette entries | 4,229,866 | 1,179,191 | 3.59x | 520.3 | 1,867.5 |
| Two-frame RGBA8, opaque destination | 13,246,880 | 4,688,419 | 2.83x | 328.4 | 938.2 |
| Two-frame RGBA16, opaque destination | 13,266,690 | 4,573,292 | 2.90x | 326.0 | 963.8 |

The first four rows compare sequential checkpoints using medians from six
processes each: three original runs and three reverse-order saved-binary
controls. The last two use four alternating processes per version, comparing
the palette checkpoint with the selected `inline` checkpoint. Mixed destination
alpha improves CPU throughput 1.28–1.30x in the latter comparison. Grayscale8
expansion improves from 2,601,002 to 1,630,428 cycles; grayscale16 improves from
2,577,936 to 1,714,268 cycles. Full results include the final cumulative source.

The intermediate `opaque` checkpoint showed slower grayscale expansion in
alternating controls (2.37M to 2.71M cycles for grayscale8). It was superseded
by `inline`, which retains the opaque arithmetic and forces the pixel kernel
inline. Both intermediate and selected measurements are retained. Some unchanged
decoder/control cases vary with host load; small differences are not attributed
to these optimizations. CPU cycles take precedence over wall throughput, which
also reflects preemption. Host frequency and competing workloads are uncontrolled.

Measurements use the same Windows x86-64 Xeon E5-2696 v4, Rust 1.99.1 and bench
profile as the preceding experiment. Windows thread-cycle counters run on CPU 2.
Each process warms an operation three times, then takes the median of seven
batches: twelve iterations for large cases, at least 2,048 for outputs smaller
than 4,096 bytes. Static rates count expanded RGBA16 bytes. APNG rates count both
full-canvas frames and include reset, SOURCE background rendering and OVER
foreground rendering. Native parser/inflater cases remain in the harness as
controls.

Every benchmark process compares complete outputs against the existing
independent expansion/rational-compositing model outside timing. All 46 cases
have matching fingerprints across five checkpoints and the alternating controls.
Untimed allocation probes agree throughout: one 1,054,728-byte output allocation
for large static expansion, zero allocations during reused two-frame rendering.
The palette table adds fixed stack scratch space and no heap allocation.

The selected source passes 13 focused release tests with no skips, including
independent C/Python regeneration of 35 WPT and 240 precision animations. Nine
debug checks pass; their optional regeneration check ran in the release suite.
New tests cover every legal palette size at depths 1/2/4/8, short transparency
tables, recovered black indices, 255/256/257-pixel boundaries and exact output
limits. A white-over-opaque-white property checks every source alpha at maximum
channel arithmetic bounds; the existing rational test covers every source alpha
at fifteen destination-alpha boundaries. All-target Clippy with warnings denied
and scoped formatting pass.

The production diff adds 41 lines net (44 added, three removed). It removes
execution steps: per-pixel RGBA interpretation, repeated palette/transparency
lookups and scaling, and variable u64 divisions for opaque blends. Public APIs
and dependencies are unchanged.

Raw measurements, cumulative source patches from `22d57bf31`, source/binary/
oracle hashes, archived original drivers, validation logs and summaries are in
[`experiments/pixels-20261028`](experiments/pixels-20261028). `summary.tsv` contains
the five six-process checkpoints; `selected-paired-summary.tsv` contains the
selected alternating comparison. `selected-validation.json` identifies the final
source and verification commands. The example and all oracle modules are inside
the PNG package. Run the current benchmark with:

```powershell
cargo run --offline -p deadlib-image-png --profile bench --example reconstruction_bench
```

## Owned pixels, packed streaming rows and SOURCE copies (2026-10-28)

Three further changes remove work in expansion, streamed decoding and rendering:

1. RGBA16 expansion builds complete `[u16; 4]` pixels in its output allocation,
   then transfers that allocation to the flat sample vector with the safe
   `Vec::into_flattened` API. Every output sample is initialized by its final
   value, eliminating the preceding full-image zero fill. Exact reservation
   preserves allocation sizes for tiny images. Compact indexed images use a
   direct palette kernel; large indexed images retain their lookup table.
2. Streaming 1/2/4-bit reconstruction reuses the buffered decoder's fixed-width
   unpacker. The native row vector keeps its length within each pass and is
   overwritten directly. This removes per-sample `push`, source division/modulo,
   repeated dynamic shifts and repeated vector-length updates. Buffer growth
   remains fallible and follows pass width; allocation counts/sizes are unchanged.
3. RGBA SOURCE frames dispatch once per frame, widening native bytes or copying
   native u16 rows directly into the destination rectangle. This removes generic
   pixel interpretation, per-pixel destination slicing and blend selection from
   those copies. The general renderer retains its previous loop and pixel kernel.
   Compositor canvases still start transparent and reset by zeroing their samples.

Production remains safe Rust under `forbid(unsafe_code)`. Pixel arrays and flat
u16 samples share the same storage; flattening adds neither a copy nor a second
allocation. The implementation uses standard-library layout/iteration operations
and adds no dependency or public API.

Final baseline/selected alternating comparison:

| Operation / fixture | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| RGB8 expansion, 513x257 | 1,983,182 | 1,548,874 | 1.28x |
| RGBA16 expansion, 513x257 | 1,146,988 | 916,925 | 1.25x |
| Indexed8 expansion, 255x1, full palette | 3,165.98 | 1,740.89 | 1.82x |
| Grayscale1 reader, 4097x257 | 6,711,633 | 2,758,630 | 2.43x |
| Grayscale2 reader, 4097x257 | 7,030,283 | 3,041,246 | 2.31x |
| Grayscale4 reader, 4097x257 | 8,035,749 | 4,093,016 | 1.96x |
| RGBA8 SOURCE, 513x257 | 1,449,313 | 280,274 | 5.17x |
| RGBA16 SOURCE, 513x257 | 1,505,852 | 313,291 | 4.81x |

The packed-reader component comparison before the SOURCE change also measures
6,685,238 to 2,635,143 cycles for grayscale1, 6,991,660 to 2,951,205 for
grayscale2, and 7,912,838 to 3,898,814 for grayscale4 (six processes per position).
The 8/16-bit reader controls are essentially unchanged in the final paired run.
Large expansion still makes one 1,054,728-byte allocation; reused SOURCE rendering
makes none. All measured allocation/reallocation requests match the baseline.

Measured tradeoffs remain visible: RGB16 SOURCE is 1.42M to 1.54M cycles in the
final paired run; a full-palette 256x1 conversion is 1,935 to 2,182 cycles, and
its 1x1 counterpart is 262 to 288 cycles. RGBA8 static expansion's 1.21M to 1.20M
difference is within run variance. These timings concern individual public API
operations and this shared host; they do not establish an application-wide gain.

Earlier output/renderer variants were superseded after broader comparisons.
The first flat-u16 iterator lost short indexed-image throughput. Complete pixel
construction plus direct indexed lookup removes that loss at 255 pixels. A
common row loop and shared palette-call refactoring slowed generic SOURCE
formats; the selected `frame` checkpoint restores the general loop/kernel while
retaining direct RGBA copies. Grayscale and GA SOURCE are around baseline in
the final alternating runs. Intermediate source patches and raw data are retained.

Measurements use Windows thread cycles, logical CPU 2, the Xeon E5-2696 v4,
Rust 1.99.1 and the existing optimized/LTO bench profile. Each process has three
warmups and seven batches; large cases use twelve iterations, small outputs at
least 2,048. Sequential checkpoints have three original and three reverse-order
controls. Final alternating runs use four processes per version in balanced
order. Allocation probes are untimed. Native reader outputs are compared with
the generated native images; every conversion/render case also checks complete
samples against the independent expansion/rational model before timing. All 51
case fingerprints agree. Host frequency, preemption and external workloads remain
uncontrolled; CPU cycles take precedence over wall throughput.

The selected source passes 28 release checks with no skips and 24 debug checks.
Debug's one optional regeneration check ran in release. Checks include the
3,240-case native format/filter/Adam7 matrix, PNGSuite fragmented/interrupted
reads, strict/recovered palette policies, exact resource budgets and all sixteen
bits of compositing precision. The new SOURCE test covers odd and wide native
RGBA rows, nonzero rectangle offsets, transparent RGB preservation, every
disposal mode, completion and reset. Independent C/Python checks regenerate 35
WPT and 240 precision animations. All-target Clippy with warnings denied and
scoped formatting pass.

The production diff is 48 lines net (103 added, 55 removed, including formatting
of the general renderer). Removed execution includes zero fill, per-sample
packed reconstruction bookkeeping and generic interpretation of RGBA SOURCE.
The native conversion helpers and reused row kernel remain local to the crate.

Evidence is in [`experiments/rows-20261028`](experiments/rows-20261028).
`frame-paired-summary.tsv` contains the final comparison;
`summary.tsv` contains the sequential six-process checkpoints. `owned`, `pixels`,
`compact`, `indexed`, `mode`, `leaf`, `local` and `range` identify intermediate
construction/dispatch trials; `frame` is the selected source. Cumulative patches
and source/binary hashes are relative to `e96e16343`. `selected-validation.json`
records the final source and verification commands. The existing measurement,
pairing and validation scripts now accept `DEADLIB_PNG_PERF_SERIES` so independent
series preserve separate evidence; `DEADLIB_PNG_PERF_BASE` selects the patch base.

## Encoder filtering and reusable pass rows (2026-10-28)

Three retained changes remove repeated encoder work:

1. Dispatch the filter once per row into a constant filter kernel, removing the
   dynamic match from the byte loop. Predictor arithmetic and adaptive tie order
   are unchanged.
2. Compile score accumulation only for adaptive selection. Fixed filters no
   longer calculate signed-byte magnitudes and a discarded 64-bit sum.
3. Reuse buffered encoder rows across Adam7 passes and omit the candidate buffer
   for fixed filters in both encoders. Capacity is bounded by the widest nonempty
   pass, including one-row images. Checked equal row spans and zipped writes
   avoid repeated output indexing in the filter kernels; the small dispatch
   wrapper is inline. No unsafe code, runtime dependencies or public APIs were
   added.

The following are isolated, alternating comparisons: each row uses four
processes before and four after its component change. Cycle values are medians
of the process medians. These component ratios must not be multiplied.

| Change and case | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Row dispatch, adaptive gray8 513x257 buffered | 6,904,007 | 4,856,950 | 1.42x |
| Row dispatch, adaptive RGB8 513x257 buffered | 17,879,445 | 13,467,935 | 1.33x |
| Omit score, None RGBA8 513x257 buffered | 5,137,215 | 4,312,151 | 1.19x |
| Omit score, None gray8 513x257 writer | 1,678,919 | 1,480,308 | 1.13x |
| Reuse/check spans, None RGBA8 9x9 Adam7 buffered | 9,452 | 5,380 | 1.76x |
| Reuse/check spans, adaptive RGBA8 33x9 Adam7 writer | 65,852 | 58,055 | 1.13x |

The last component reduces full seven-pass adaptive buffered allocation calls
from 31 to 7, with output reallocations unchanged. For RGBA16 513x257 Adam7,
requested bytes fall from 6,419,218 to 6,390,450. Fixed filters use three row
buffers rather than four: the 9x9 Adam7 case falls from 31 to 6 allocation calls,
with three output reallocations unchanged. Fixed writer paths also remove one
allocation, including its later growth across passes. Requested bytes count
allocator requests, not peak resident memory.

The combined selected implementation (`bounded`) was then replayed directly
against the original source with the same balanced four-process protocol:

| Complete Stored pipeline | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Adaptive RGB8 513x257 buffered | 17,696,859 | 13,365,172 | 1.32x |
| Adaptive RGB8 513x257 writer | 15,096,844 | 14,568,617 | 1.04x |
| None RGBA8 513x257 buffered | 5,913,806 | 2,880,133 | 2.05x |
| None RGBA8 513x257 writer | 5,431,089 | 3,552,571 | 1.53x |
| Adaptive RGBA16 513x257 Adam7 buffered | 51,736,590 | 39,100,524 | 1.32x |
| None RGBA8 9x9 Adam7 buffered | 10,973 | 5,357 | 2.05x |
| Adaptive RGBA8 33x9 Adam7 buffered | 57,276 | 42,126 | 1.36x |

These are targeted filtering/row-layout measurements with Stored compression,
including PNG validation, checksums, allocation and framing. They do not predict
Fast DEFLATE throughput or compression ratios. The 116 cases cover all six
native byte strides, all filters, packed 1/2/4-bit samples, plain/Adam7 layout,
buffered/writer encoding and small rows. Native images are pseudorandom and
fixture construction stays outside timing. Every process compares complete
inflated IDAT bytes against independent normative filtering and adaptive
selection, and verifies decoded native images before measuring.

The first allocation-reuse trial (`buffers`) slowed many large adaptive
buffered cases by about 13%, despite reducing allocations. It is rejected in
favor of checked spans plus bounded capacity (`bounded`). Some tradeoffs remain
relative to the preceding `score` checkpoint: GA8 Average, RGB8 Average and
RGB16 Sub buffered cases have throughput ratios 0.78x, 0.83x and 0.84x. All three
are faster than the original source in the combined comparison, by 1.07x,
1.28x and 1.18x. The final corpus has no median regression above 5%; changes near
1% are within shared-host variance and are not treated as established gains.
Large plain adaptive RGBA16 baseline timings varied substantially across
comparisons, so its unusually high final ratio is not used as a headline result.

Measurements use Windows thread cycles on logical CPU 2, Rust 1.99.1 and the
existing optimized/LTO bench profile. Every measured process has three warmups
and seven batches, with three iterations for large outputs and at least 2,048
for outputs below 4,096 bytes. Allocation probes run separately, untimed.
Frequency, preemption and unrelated host workloads remain uncontrolled; CPU
cycles take precedence over wall-clock throughput. All retained comparisons
run sequentially in alternating A/B, B/A order.

The selected source passes 20 release tests and 19 debug tests, strict
all-target Clippy and scoped formatting. Debug skips the optional C encoder
audit that passed in release. A new independent filter-byte test checks 8,640
format/width/height/pattern/filter/interlace configurations through both
encoders: 17,280 encoded outputs, including one- and two-row images, partial
packed bytes, zero/full rows and deterministic adaptive ties. Existing checks
include every Paeth triplet, filter scores and short boundaries, 1,620 C encoder
reference files, 8,640 native APNG outputs, input/ordering/resource limits,
literal wire answers, and exact writer prefixes under interrupted/short writes.

The production-source diff is 49 lines net (64 added, 15 removed, including
unit-test call formatting). Removed execution includes a per-byte filter
switch, discarded score arithmetic, repeated pass allocation and unused fixed
candidate storage. The five constant filter kernels share one local safe Rust
implementation rather than duplicating predictor logic.

Evidence is in [`experiments/encoding-20261028`](experiments/encoding-20261028).
`dispatch-paired-summary.tsv`, `score-paired-summary.tsv` and
`bounded-paired-summary.tsv` record component comparisons;
`selected-paired-summary.tsv` is the final baseline comparison. Checkpoints
carry cumulative patches relative to `e372c77f0`, source/harness/binary hashes,
build logs and raw process data. `final-validation.json` and
`scanline-validation.json` record checks and oracle hashes. `exclusions.json`
identifies overlapping exploratory runs, the disk-interrupted comparison,
superseded validation failures and intermediate candidates. The measurement
driver's `freeze` mode separates builds from timing; use
`DEADLIB_PNG_PERF_SERIES=encoding-20261028`,
`DEADLIB_PNG_BENCH_STAGE=encode`, and `DEADLIB_PNG_PAIR_ITERS=3` to replay this
protocol without running competing measurement jobs.

## Packed rows, Adam7 gathers and first-row lanes (2026-10-28)

Three retained safe Rust changes remove reconstruction work:

1. Pack contiguous 1/2/4-bit samples in constant groups per output byte. Each
   byte is written once, with zero padding in its partial tail. This removes
   the initial row clear, dynamic bit-offset calculations and repeated byte
   read/modify/write operations.
2. Gather 8-bit Adam7 pixels as fixed channel arrays. Representation and channel
   dispatch happen once per row, rather than inside a per-channel loop. Complete
   byte-aligned output is overwritten directly without an initial zero fill.
   The unsuccessful generic 16-bit gather kernels are removed; 16-bit samples
   retain direct per-channel big-endian writes.
3. Share first-row reconstruction with the streaming decoder, removing unused
   zero-predictor clearing and reading. Paeth reduces to Sub and Average uses
   half of the left byte. Constant shifts and register-carried pixel lanes
   handle these cases, with the measured scalar recurrence retained for
   two-byte strides. Explicit Sub filters reuse the existing reconstruction
   loop. The same first-row helper also serves buffered decoding.

Isolated alternating comparisons use four processes before and four after
each component change; these ratios must not be multiplied:

| Component and complete Stored pipeline | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Grouped 1-bit gray, 4097x129 buffered encode | 3,239,601 | 1,665,164 | 1.95x |
| Grouped 2-bit gray, 4097x129 buffered encode | 3,236,930 | 1,905,077 | 1.70x |
| Grouped 4-bit gray, 4097x129 buffered encode | 3,941,159 | 2,836,416 | 1.39x |
| 8-bit RGB Adam7 gather, 513x257 buffered encode | 3,719,208 | 3,467,039 | 1.07x |
| 8-bit RGBA Adam7 gather, 513x257 buffered encode | 4,209,132 | 3,939,842 | 1.07x |
| First-row Average gray8, 4097x1 reader decode | 34,530 | 30,058 | 1.15x |
| First-row Paeth RGBA16, 4097x1 reader decode | 297,110 | 238,744 | 1.24x |

The selected implementation (`scalar2`) was also compared directly against the
original source with the same balanced four-process protocol:

| Combined pipeline | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| 1-bit gray, 4097x129 buffered encode | 3,642,675 | 1,888,688 | 1.93x |
| 2-bit gray, 4097x129 buffered encode | 3,959,670 | 2,089,677 | 1.89x |
| 4-bit gray, 4097x129 writer encode | 5,054,280 | 3,303,292 | 1.53x |
| RGB8 Adam7, 513x257 writer encode | 3,801,008 | 3,318,584 | 1.15x |
| RGBA8 Adam7, 513x257 buffered encode | 4,360,989 | 4,056,397 | 1.08x |
| Average gray8, 4097x1 reader decode | 36,447 | 28,655 | 1.27x |
| Paeth RGBA8, 4097x1 reader decode | 109,047 | 69,813 | 1.56x |
| Paeth RGBA16, 4097x1 reader decode | 303,016 | 227,114 | 1.33x |
| Paeth RGBA8, 513x257 reader control | 3,156,812 | 3,172,507 | 1.00x |
| Paeth RGBA16, 513x257 reader control | 9,346,603 | 9,368,114 | 1.00x |

First-row improvements are startup/pass-boundary improvements, not a promise
of equivalent gains on tall images. Allocation counts, reallocations and
requested bytes are identical before/after in all 99 final cases. Tradeoffs
remain: gray16 Adam7 buffered encoding rises from 2,306,140 to 2,462,132 cycles,
plain RGB8 buffered encoding from 3,472,746 to 3,693,490, and first-row Sub
gray16 reading from 60,160 to 63,398. These are approximately 6.8%, 6.4% and
5.4% more cycles. Other small changes should be read alongside the raw
shared-host variation rather than treated as established gains.

The first trials exposed regressions rather than being retained unchanged.
Generic 16-bit gathers did not establish a benefit. Reusing the previous scalar
first-row helper slowed Average; register lanes fixed most strides, while
two-byte lanes and explicit Sub required their scalar/existing loops. Initial
short reader timings used only three iterations per batch. All four retained
checkpoints were rebuilt with the same longer short-reader protocol before
the final comparisons. The original harness and all intermediate measurements
are retained separately.

Measurements use logical CPU 2, Windows thread cycles, Rust 1.99.1 and the
existing optimized/LTO bench profile. Each process has three warmups, a separate
untimed allocation probe, and seven batches. Reader cases up to 65,536 native
bytes use at least 64 iterations per batch; other outputs below 4,096 bytes use
at least 2,048; remaining cases use three. Reported values are medians of four
process medians in alternating A/B, B/A order. Measurement jobs run sequentially.
Host frequency, preemption and unrelated workloads remain uncontrolled.

`DEADLIB_PNG_BENCH_STAGE=rows` supplies 50 buffered/writer encoding cases and
49 reader cases: all six native byte strides, packed depths, plain/Adam7
layout, all first-row filters, short images and tall-image controls. Stored
DEFLATE emphasizes packing/reconstruction while retaining PNG framing,
checksums and allocation. These measurements do not predict Fast DEFLATE
compression throughput. Every process checks complete filtered bytes against
the independent normative model and complete decoded native images against
the generated source. Reader fingerprints identify the exact input PNG, so
changed fixture encoding cannot silently change a comparison.

The selected source passes 29 release and 27 debug tests, strict all-target
Clippy and scoped formatting. Debug's two optional C checks passed in release.
The new unit test checks all 256 packed byte values, every partial tail, four
strides and three offsets: 43,008 cases, with poisoned output and unused source
samples. Existing checks include 17,280 independently modeled encoded outputs,
every Paeth triplet, all first-row/short-filter boundaries, the 3,240-case native
format/filter/interlace matrix, 1,620 C encoder references, 3,000 modeled APNG
faults, exact resource thresholds, palette policies and fragmented/I/O failures.

The source diff is 104 lines net (130 added, 26 removed, including the new
exhaustive unit test). The additional local constant-width kernels remove
executed work; they add no allocation, unsafe code, runtime dependency or public
API. Data layout and low-level specialization replace repeated interpretation.

Evidence is in [`experiments/packing-20261028`](experiments/packing-20261028).
`packed64-paired-summary.tsv`, `gather8-paired-summary.tsv` and
`scalar2-paired-summary.tsv` are the retained component comparisons;
`accepted-paired-summary.tsv` is the selected baseline comparison. Cumulative
patches and source/binary/harness hashes are relative to `ae44814be`.
`accepted-final-validation.json` and `accepted-scanline-validation.json` record
the final checks and oracle identities. `protocol.json` identifies superseded
trials and timing protocols; `exploratory-benchmark.rs` preserves the exact
original short-batch harness. The validation driver accepts `*-final` labels
to retain separate complete checks for refinements without overwriting logs.

## Word gathers, stepped bit rows and streaming Adler-32 (2026-10-28)

Three retained changes replace repeated scalar work:

1. Gather eight validated 1-bit samples with a little-endian u64 multiply by
   `0x8040201008040201`, taking the high byte. Four 2-bit samples use a u32
   multiply by `0x40100401`. Within the validated sample ranges, product bit
   positions have unique contributors, so carries cannot corrupt the result.
   Partial groups retain scalar packing and zero padding. The unsuccessful
   direct nibble-pair trial is rejected; contiguous 4-bit rows keep their
   original grouping loop.
2. Pack stepped Adam7 samples in constant groups per output byte, writing each
   byte once. A separate partial group handles padding. This removes clearing
   the packed row, repeated byte read/modify/write operations and dynamic bit
   offsets from the per-sample loop.
3. Route streaming Adler-32 updates of at least 64 bytes to the existing bulk
   kernel. Short calls retain an inline scalar path, including an empty-input
   no-op. The original all-bulk entry point regressed byte fragmentation and
   is rejected. One-shot checksums keep their existing bulk path. Duplicate
   direct/indirect checksum test passes are removed while retaining independent
   per-byte modular oracles, fragmented calls and worst normalized states.

Each component below has a separate alternating four-process comparison
before and after its change. Cycle values are medians of process medians;
component ratios must not be multiplied.

| Component and complete Stored pipeline | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Word gather, 1-bit gray 4097x129 buffered encode | 1,852,031 | 1,507,819 | 1.23x |
| Word gather, 2-bit gray 4097x129 buffered encode | 2,150,625 | 1,880,676 | 1.14x |
| Stepped 1-bit gray 4097x129 Adam7 buffered encode | 2,478,045 | 1,890,627 | 1.31x |
| Stepped 2-bit gray 4097x129 Adam7 buffered encode | 2,936,764 | 2,309,360 | 1.27x |
| Stepped 4-bit gray 4097x129 Adam7 buffered encode | 3,745,438 | 3,021,637 | 1.24x |
| Shared bulk checksum, gray8 4097x257 reader | 4,310,431 | 3,619,957 | 1.19x |
| Shared bulk checksum, RGBA16 4097x257 reader | 38,673,267 | 32,050,769 | 1.21x |

The final baseline comparison was repeated, giving eight processes per version
in two balanced alternating replays. The selected source is `adaptive-nibble`:

| Combined pipeline | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| 1-bit gray 4097x129 buffered encode | 1,935,405 | 1,525,635 | 1.27x |
| 1-bit gray 4097x129 writer encode | 1,988,999 | 1,586,875 | 1.25x |
| 2-bit gray 4097x129 buffered encode | 2,151,686 | 1,907,492 | 1.13x |
| 2-bit gray 4097x129 Adam7 buffered encode | 2,906,985 | 2,245,119 | 1.29x |
| 4-bit gray 4097x129 Adam7 buffered encode | 3,710,209 | 2,992,809 | 1.24x |
| Gray8 4097x257 reader | 4,226,473 | 3,478,746 | 1.21x |
| RGBA8 4097x257 reader | 16,668,684 | 13,919,885 | 1.20x |
| RGBA16 4097x257 reader | 38,067,410 | 32,530,193 | 1.17x |

There are measured control-path tradeoffs. Combined before/after medians for
GA8 Adam7 buffered encoding are 2,355,528 / 2,740,128 cycles; GA16 Adam7 buffered
encoding is 4,205,327 / 4,721,555; plain RGB16 writer encoding is
5,644,296 / 6,201,058; and the small RGBA8 33x9 Adam7 writer case is
29,667 / 35,455. These are throughput ratios of 0.86x, 0.89x, 0.91x and 0.84x.
Outlining the larger kernels produced mixed results and was rejected; no
blanket no-inline attributes are retained. Raw per-process ranges show material
shared-host variation. These targeted gains are not a universal speedup claim.

A separate checksum-only example compares the exact scalar baseline and
candidate implementations in one executable, checking both against an
independent u64 modular oracle before timing. Three processes each retain
eleven alternating batches per variant after a warmup batch. For 8,192 bytes
in 5,552-byte fragments, the selected path improves from 8,341 to 2,984 cycles
(2.80x); seven-byte fragments improve from 17,574 to 15,790 (1.11x). One-byte
fragmentation costs 65,967 / 68,717 cycles, a 4.2% increase. The rejected
all-bulk entry point was substantially worse on tiny fragments. The probe's
64-byte inputs use 1,024 iterations per batch and 8,192-byte inputs use 32.
This is a checksum microbenchmark, not an end-to-end compression measurement.

The whole-pipeline harness uses `DEADLIB_PNG_BENCH_STAGE=bitwords`: the previous
99 row cases plus three large unfiltered reader inputs, all with Stored
DEFLATE. It includes PNG framing, CRCs, allocations and reconstruction while
excluding fixture generation and independent model checks from timing. Every
process verifies complete filtered pass bytes and native decoded images;
reader fingerprints identify exact input PNGs. Allocation calls, reallocations
and requested bytes are unchanged in all 102 cases. No Fast-DEFLATE throughput
or compression-ratio gain is inferred from these Stored measurements.

Measurements use logical CPU 2, Windows thread cycles, Rust 1.99.1 and the
existing optimized/LTO bench profile. Every whole-pipeline process has three
warmups, a separate untimed allocation probe and seven batches. Short reader
cases up to 65,536 native bytes use at least 64 iterations; other outputs below
4,096 bytes use at least 2,048; remaining cases use three. Measurement jobs are
serialized. Frequency, preemption and unrelated workloads remain uncontrolled.

The exact selected source passes 33 release and 32 debug tests, strict
all-target Clippy and scoped formatting. The optional C encoder check skipped
in debug passed in release. Checks include all 43,008 packed-byte/tail/stride
answers, 17,280 independently modeled encoded outputs, the 3,240-case native
format/filter/interlace matrix, 1,620 C encoder references, streamed DEFLATE
fragmentation and invalid streams, writer failures, every Paeth triplet and
Adler modulus/block boundaries with initial states `(1, 0)` and
`(65520, 65520)`. Empty calls, arbitrary split points and 5,551/5,552/5,553-byte
fragments exercise both checksum paths. Production remains safe Rust, with no
new runtime dependencies or public APIs.

Production-source files change by 32 lines net (63 added, 31 removed), including
deduplicated checksum tests. Removed execution includes group-shift loops for
complete 1/2-bit bytes, stepped-row clearing and repeated writes, and the serial
Adler dependency chain for large streaming chunks.

Evidence is in [`experiments/bitwords-20261028`](experiments/bitwords-20261028).
`words-final-paired-summary.tsv`, `steps-final-paired-summary.tsv` and
`checksum-final-paired-summary.tsv` record the retained components.
`accepted-combined-summary.tsv` pools the two final balanced comparisons;
their individual data remains in `accepted-paired*` and `accepted-replay*`.
`micro-summary.tsv` retains checksum-only results. `protocol.json` identifies
rejected nibble, all-bulk and outlining trials. Cumulative patches are relative
to `2414ec857`; immutable binaries, source/harness hashes and raw logs identify
every checkpoint. Final checks are in `accepted-final-validation.json` and
`accepted-library-validation.json`. The new checksum example retains its exact
baseline source snapshot and oracle checks for reproducibility.

## Direct None rows and omitted Sub predictors (2026-10-28)

Three safe Rust changes remove intermediate encoder storage and copying:

1. Buffered None filtering appends the filter byte and writes directly into the
   final filtered vector. Contiguous 8-bit rows extend directly from native
   samples; other rows pack into the initialized final span. Three scratch rows
   and two full-row copies are removed. The total filtered size is reserved
   before writing, as before.
2. Streaming None filtering packs directly into the row output after its filter
   byte. It removes the raw and previous-row buffers, a copy, predictor setup
   and unused swaps while retaining the same row lengths and framing.
3. Sub filtering omits the previous-row buffer in both encoders. Its left
   predictor uses only the current raw row. Constant filter kernels no longer
   slice/read unused previous-row data for None/Sub; Paeth alone computes its
   upper-left byte. Up, Average, Paeth and adaptive selection retain their
   previous rows and exact tie order.

The isolated before/after comparisons use four alternating processes per
version. These are complete Stored pipelines, not standalone packing kernels:

| Component, gray8 513x257 None | Before cycles | After cycles | CPU throughput | Allocation calls |
| --- | ---: | ---: | ---: | ---: |
| Buffered direct output | 709,314 | 405,197 | 1.75x | 6 -> 3 |
| Streaming direct output | 743,520 | 445,695 | 1.67x | 7 -> 5 |

The third change's established benefit is allocation/storage reduction, not a
claimed CPU gain. RGBA16 513x257 Sub drops from six to five buffered allocation
calls and seven to six writer calls, with 4,104 fewer requested bytes in each
case. The previous row is no longer allocated, cleared, resized or swapped.
Sub's in-process CPU ratios below are approximately neutral. Allocation probes
are separate from timing; counts include the full PNG API rather than only rows.

Between-process control timings varied substantially, including reader paths
whose source did not change. An additional caller links an immutable baseline
crate and the current crate into one executable, using the same shared native
image types and copied bench-profile settings. Before timing, both buffered
and writer PNG files must match byte-for-byte. Each process alternates variants
within batches; three processes each retain eleven batches per variant after
one warmup batch. The results corroborate the None fast paths:

| In-process combined comparison | Before cycles | After cycles | CPU throughput |
| --- | ---: | ---: | ---: |
| Gray8 None, 513x257 buffered | 1,323,366 | 1,043,942 | 1.27x |
| Gray8 None, 513x257 writer | 1,391,703 | 1,072,404 | 1.30x |
| GA8 None, 513x257 Adam7 buffered | 2,770,310 | 2,371,990 | 1.17x |
| GA8 None, 513x257 Adam7 writer | 2,771,626 | 2,035,570 | 1.36x |
| RGBA16 None, 513x257 buffered | 9,632,612 | 6,970,295 | 1.38x |
| RGBA16 None, 513x257 writer | 8,913,310 | 6,005,447 | 1.48x |
| RGBA8 None, 33x9 Adam7 buffered | 9,815 | 6,020 | 1.63x |
| RGBA8 None, 33x9 Adam7 writer | 31,459 | 25,436 | 1.24x |
| RGBA16 Sub, 513x257 buffered | 9,175,466 | 9,347,920 | 0.98x |
| RGBA16 Sub, 513x257 writer | 7,784,275 | 7,823,053 | 1.00x |

The four adaptive encoder controls have in-process throughput ratios
1.017x/1.034x (RGBA8 buffered/writer) and 1.001x/1.029x (RGBA16).
Four one-row reader controls range from 0.987x to 1.021x. Small differences are
within observed host/caller variation. The separate-process final comparison
also shows None/Sub encoder gains, but several reader controls are slower by
10-30%; these did not reproduce in the within-process controls. Both datasets
remain visible. The results establish targeted encoding/storage benefits,
not a universal speedup or an isolated decoder regression diagnosis.

`DEADLIB_PNG_BENCH_STAGE=rowbuffers` adds Sub, Up and adaptive controls to the
existing row/large-reader cases: 124 case/operations per process. Every process
checks independently modeled complete filtered bytes and complete decoded
images. Reader fingerprints identify input PNGs. Logical CPU 2, Windows thread
cycles, Rust 1.99.1 and the existing optimized/LTO bench profile are used.
There are three warmups, an untimed allocation probe, and seven batches per
whole-pipeline process. Large cases use three iterations; readers up to 65,536
native bytes use at least 64; other outputs below 4,096 bytes use at least 2,048.
Measurement jobs are serialized; other workloads, frequency and preemption are
uncontrolled. No Fast-DEFLATE throughput or compression-ratio claim is inferred
from the Stored timing cases.

The within-process caller measures 22 encoder case/operations and four reader
controls. Large encodes use three iterations per batch, the small case 1,024,
and reader controls 64. The baseline source is recovered directly from Git,
renaming its package only to link both crate versions. Source hashes, generated
manifest/profile and immutable executable identity are recorded. This extra
probe supplements full-library correctness and allocation checks; it does not
replace them.

The selected source passes 33 release and 32 debug tests, strict all-target
Clippy and scoped formatting. The optional C encoder audit ran in release.
Checks include 17,280 independently modeled encoded outputs, all native
formats/filter types/Adam7 passes, partial packed tails, 1,620 C encoder
references, 109,376 literal scalar files, 308,033 rejected native inputs,
49,419 ordering pairs and 2,616 exact writer failure prefixes. The filter unit
oracle now supplies empty previous slices for None/Sub, testing their absent
dependency alongside exact bytes and adaptive scores. All twelve library tests
also pass, including packed-byte/stride exhaustiveness and checksum boundaries.
Wire bytes and input acceptance/error behavior are preserved; working-memory
statistics truthfully decrease with the removed buffers.

The production-source diff is 54 lines net (75 added, 21 removed, including
the unit-oracle adjustment). The direct paths remove allocations, clears,
copies and swaps without new unsafe code, runtime dependencies or public APIs.

Evidence is in [`experiments/directrows-20261028`](experiments/directrows-20261028).
`buffered-paired-summary.tsv`, `writer-paired-summary.tsv` and
`sub-paired-summary.tsv` retain the three components;
`selected-paired-summary.tsv` retains the whole selected pipeline comparison.
`in-process-summary.tsv` and its raw logs retain the additional paired caller.
`compare.py` regenerates its baseline source and build under the ignored target
directory; `compare.rs` contains the exact API comparisons. Cumulative patches
are relative to `68a288d39`; final checks are in `final-validation.json` and
`library-validation.json`. `protocol.json` records both measurement scopes.
