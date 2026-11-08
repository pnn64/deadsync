# Paired abbreviated output streaming

`AbbreviatedStreamEncoder<T: Write, I: Write>` emits table-only and image streams
to separate logical destinations. It supports all thirteen JPEG-1 frame
processes, prepared coefficients/sample planes/hierarchies, native/shared rasters
and source pyramids. It shares table, marker and entropy emission with the vector
and interchange-stream APIs. Source preparation remains in memory.

The table destination uses direct writes and no encoded-packet allocation. Only
the image has reusable codec staging: 8 KiB by default, configurable down to zero,
and reduced by the available storage budget. No frame is encoded twice. External
writer buffering and the persistent inline table cache are separate from this
staging; full allocator/stack/cache/peak accounting remains open.

`max_table_bytes` defaults to 65,536 and includes table SOI/EOI. Image options'
`max_output_bytes` bounds the sum of both streams, including nested frame limits.
Both successful byte counts are returned in `AbbreviatedBytes`. Unchanged tables
are omitted; a fully reused packet is the four-byte SOI/EOI stream. Redefinitions
after the first scan stay at their original image position.

Writes to the two destinations can interleave. After each successful call the
receiver must consume that pair's table stream and then its image, in order,
using one persistent table cache. Subsequent calls append streams to each
destination; returned counts delimit pairs. The two writers must represent
separate logical destinations. Directly aliasing them to one channel does not
serialize table-first transport. A single-channel table-first API is not provided.

The codec completes short writes, retries `Interrupted`, rejects zero progress
and never calls either destination's `flush`. `AbbreviatedWriteError::Tables` and
`Image` retain the original I/O error, its custom payload and the failing
destination; `Codec` retains the native error category. Any failed call clears
receiver-cache knowledge, including failures during source preparation. Pending
image staging is discarded. Either destination can contain a prefix; handle
those prefixes before retrying. The codec does not drain staging on Drop, and
each destination retains its own destructor behavior. Clear cache knowledge
when the receiver resets or a successful pair is discarded.

`tables`, `clear`, `get_ref`, `get_mut`, `into_inner` and `buffer_capacity` expose
state without implicit writes. `from_parts`/`into_parts` transfer known receiver
destinations, table writer and existing image `StreamEncoder`, retaining its
buffer. Construction resets `max_table_bytes` to its default.

## Native tests

Four added integration tests cover 520 exact vector/stream comparisons over 65
process/precision representatives, four image staging sizes (0, 1, 31, 8,192), and
two emissions of each source. Both sender implementations and a real native
receiver cache persist across the mixed-family session. Reconstructed native
coefficients, quantizers and predictive/residual integers are exact.

Additional checks cover every failing table/image destination offset, every
total byte ceiling and every table ceiling for a small model, three-byte writes,
periodic interruption, original custom error payloads, zero progress, no flush,
cache reset and exact retry output. All public source conveniences, seeded cache
parts, buffer retention and source-preparation failure are exercised.

Twenty-four more comparisons use 255 components, both DCT precisions/entropy
families, varied sampling, distinct wide quantizers, DNL, restarts, a 60,000-byte
APP7 payload and a 70,000-byte ICC profile. A dense 256x256 model writes to two
digest-only destinations with 64 bytes of image staging and zero table staging;
encoded image size exceeds the source/encoder/staging storage budget, and both
digests exactly match generous-budget vectors.

The existing five interchange-stream tests remain in the same test target.
The paired API also has an executable documentation example.
All 162 unit/integration tests and both doctests pass in release and isolated
debug builds. Formatting, all-target Clippy and rustdoc with warnings denied,
Python compilation and report/source identity audits pass.

## Independent reference gates

`examples/abbreviated.rs encode-stream BUFFER DNL PREFIX FIXED RESTART REPEAT
INPUT...` writes real files through the paired API. Its probe keeps image staging
and sender cache across destination changes, closes files before checking them,
and asserts exact table/image vector equality and the staging bound. Each source
is decoded once in its original input-table context before repeating the prepared
model. This matters for abbreviated inputs whose later scans redefine tables.
Ordinary `encode` and `decode` commands remain available.

`reference_abbreviated.py --stream-buffer` runs the existing mixed-session matrix
with 0, 31 and 8,192 image staging bytes, fixed/optimized Huffman and restart
intervals 0/1/3. All 180 sessions pass:

| Check | Image outputs across three buffer configurations |
| --- | ---: |
| Exact file/vector output identity | 10,368 |
| Genuine Turbo persistent-object B.5 coefficient/sample checks | 4,572 |
| JPEG XT rare-process equivalent-interchange frame-data checks | 5,796 |

Turbo consumes both separate files and concatenated table/image streams. JPEG
XT receives an explicitly reconstructed external-table copy, so those checks do
not establish its abbreviated parsing. They check 29,952 DCT blocks and 3,874,716
sample/residual integers exactly. All 288 inserted EXP00 C-input copies are
recorded individually for its stale expansion state. Every earlier session's
output hash is unchanged; the C checks in this gate are fresh.

`reference_paired_stream.py` adds 148 sessions/296 DNL image outputs using 31-byte
and 8 KiB staging, both table policies, restarts, all thirteen processes, the real
official T.83 B1/B2 input pair, four official predictive originals and generic
255-component table-reuse inputs. A real native receiver checks original
abbreviated/DNL bytes against source values. IJG checks 56 ordinary DCT images;
JPEG XT checks 280 images, totaling 106,944 DCT blocks and 974,284 sample/residual
integers. Sixteen fractional JPEG XT exclusions retain exact IJG checks.
For these 296 C inputs the driver explicitly applies the raw external table bank,
fills known SOF/DHP heights and removes DNL without changing entropy bytes. This
is entropy/frame-data evidence, not genuine C B.5 or original-header DNL parsing.
No EXP00 copies are needed in this supplementary gate.

The main C gate uses a frozen adapter build preceding the source-context
preparation adjustment; those sources are self-contained. `replay_paired_stream.py`
bridges the final adapter to all 180 earlier session hashes/10,368 outputs with
all three buffer sizes. That replay performs native file/vector checks and
reuses the C evidence by exact bytes; it is not counted as fresh C decoding.
The DNL/genuine abbreviated-input supplement uses the final adapter directly.

The configured references are Turbo 3.2.1's public B.5 API helper, IJG libjpeg 10
and JPEG XT `702114c8130ae818ccd2354cc03e879dac69e579`. Executable/source/report
hashes and per-case copies are retained. IJG configuration and JPEG XT patches
are described in `tests/oracle/README.md`, `ijg10-build.json` and
`components-jpegxt-build.json`. No production C code or dependency is added.

## Reproduction and limits

From this package, with the configured reference executables:

```text
cargo build --release --example abbreviated
python tests/reference_abbreviated.py --native ABBREVIATED --turbo TURBO_B5 --reference JPEGXT --part native --stream-buffer 31 --out experiments/paired-native
python tests/reference_abbreviated.py --native ABBREVIATED --turbo TURBO_B5 --reference JPEGXT --part rare --stream-buffer 31 --out experiments/paired-rare
python tests/reference_paired_stream.py --native ABBREVIATED --ijg IJG --reference JPEGXT --out experiments/paired-dnl
python tests/replay_paired_stream.py --native ABBREVIATED --out experiments/paired-replay
cargo test -p deadlib-image-jpeg --test stream_encode
python tests/independent.py
```

Repeat the main gate with buffer sizes 0 and 8192. Results are retained in
`paired-stream-sessions.json`, `paired-stream-dnl.json`, `paired-stream-replay.json`
and `paired-stream-validation.json` under `tests/reports`.

Incremental input, full memory instrumentation, broader conformance/rare
combinations, coverage-guided fuzzing and measured performance/optimization remain
required. This is correctness and bounded encoded-output evidence, with no
complete-conformance or competitive-speed claim and no application integration.
