# Incremental encoded output

`StreamEncoder<W: std::io::Write>` writes interchange images for all thirteen
JPEG-1 frame processes and standalone table streams. The public entry points
accept prepared coefficients, predictive planes, hierarchy frames, native/shared
rasters and source-pyramid inputs. Raster conversion and pyramid/model preparation
still happen in memory. Incremental input and paired abbreviated output with a
persistent omission cache remain future work.

## Storage and error behavior

Marker, Huffman and arithmetic emission use one statically dispatched output
abstraction shared with the existing `Vec` APIs. The `Vec` path retains its growth
strategy. The writer path retains a reusable staging window, defaults to 8 KiB,
and accepts zero for direct writes. Large borrowed segments bypass staging after
draining it. No full encoded image, encoded frame, or metadata payload copy is
created by the writer path. The external writer may allocate its own storage;
that is outside the codec's limits.

The requested staging size is reduced by the encoded byte ceiling and remaining
memory after source/encoder storage. A retained allocation that exceeds a later
memory budget is released before preparing output. Logical encoded size remains
subject to `max_output_bytes`, including nested frame limits. A stream can exceed
its staging memory budget while staying within its encoded byte ceiling.

This refactor also recalculates actual DCT model capacities and charges added
metadata and Huffman symbol storage against the memory available for output.
Previously a standalone DCT output vector was not charged against retained model
storage. Consequently tighter limits can now reject an allocation previously
accepted. Existing legal default outputs remain byte-identical. These are storage
budgets; a complete allocator high-water measurement, stack accounting and cache
audit remain pending.

Successful calls drain codec staging, return bytes written for that call and
never call `W::flush`. Short writes are completed, `Interrupted` is retried, and
zero progress returns `WriteZero`. A destination returning an impossible length
is rejected. `WriteError::Io` preserves the original `io::Error` and its custom
payload. Core `Error` now also has an `Io(ErrorKind)` category for propagation;
the streaming wrapper returns the original error. Codec failures use
`WriteError::Codec`.

Errors can leave an already written prefix and discard pending staging. The
codec never drains staging on Drop; the destination retains its own destructor
behavior. Reusing an encoder starts a new JPEG at the destination's current
position; the caller must first handle any failed prefix. Consecutive successful
calls append complete streams. `get_ref`, `get_mut` and `into_inner` expose the
destination between calls without implicit writes or flushes.

## Native checks

Five integration tests cover:

- 260 exact `Vec`/writer comparisons: 65 process/precision representatives with
  staging windows of 0, 1, 17 and 8,192 bytes. Destinations accept three bytes per
  write and periodically return `Interrupted`. All thirteen markers are present.
  Each comparison also fails at header, body and final-drain offsets and checks
  the exact prefix and original custom I/O error.
- Twelve 255-component cases at 8/12 bits, both entropy families, varied sampling,
  distinct wide quantizers, DNL, restart markers, a 60,000-byte APP7 payload and a
  70,000-byte ICC profile. Stream bytes and native decoded coefficients are exact.
- Every destination failure offset and every encoded byte ceiling for a small
  coefficient model with three buffer sizes, retry after failure, `WriteZero`,
  codec-error staging discard and absence of flush/Drop output.
- A dense 256x256 coefficient model whose encoded image exceeds the source plus
  encoder/staging memory budget. A digest-only destination receives exactly the
  generous-budget vector bytes with 64 bytes of staging. Reuse first allocates an
  8 KiB window, then verifies its release under the smaller budget.
- Every public raster/shared, lossless, hierarchy/pyramid and table-only entry
  point, both entropy families, ICC chunking and consecutive calls on one writer.

The executable documentation example also runs as a Cargo doctest.
All 158 unit/integration tests and that doctest pass in both release and isolated
debug builds. Package formatting, all-target Clippy and rustdoc with warnings
denied pass, as do Python driver compilation and fixture/tool identity audits.

## Independent entropy checks

`tests/reference_stream_encode.py` snapshots the native example, IJG coefficient
observer and JPEG XT executable, pins all source hashes, and writes resumable
case checkpoints. `examples/stream_encode.rs` writes each image through a file
destination and a vector, and dumps source frame values before entropy writing.
The gate uses 74 source sessions with three configurations: one-byte staging,
31-byte staging with DNL, and 8 KiB staging with fixed Huffman tables. Restart
intervals are one or seven. It includes the official T.83 B1/B2 input pair and
four lossless originals, plus 255-component quantizer-reuse fixtures.

All 222 file outputs exactly equal the vector outputs. IJG checks 42 ordinary
DCT images, including 255 components. JPEG XT checks 210 cases across all thirteen
processes: 80,208 DCT blocks and 730,713 sample/residual integers. Small ordinary
lossless images use its raw PGX planes; other cases use its frame observer.
These checks are exact and use no numerical tolerance.

Twelve JPEG XT cases are explicitly excluded for fractional sampling; all twelve
have exact IJG coefficient checks. The driver also detects an observer limitation
for repeated same-kind, same-size frames; no selected case requires that exclusion.
The report records each exclusion.
For 74 DNL cases the C input is an explicit known-height copy: SOF/DHP heights
are filled and DNL markers removed, without changing entropy bytes. This checks
the emitted entropy, not those C parsers' original DNL handling. EXP00 copy
support is retained in the driver; zero cases needed it here. Native tests read
the original emitted headers. Paired abbreviated input is decoded with a real
persistent native table cache, then emitted as interchange output; it is not
counted as abbreviated streaming output.

Reference builds/configuration and JPEG XT observer/cleanup patches are pinned
in `tests/reports/ijg10-build.json` and `components-jpegxt-build.json`, with source
and patch instructions in `tests/oracle/README.md`. References remain test-only.
No throughput, allocation high-water, or complete conformance claim is made.

The shared writer refactor preserves all 10,612 earlier default output hashes:
4,448 lossless, 3,100 authored hierarchy, 2,100 hierarchy rewrite, 172 pixel,
192 CMYK and 600 ordinary rewrite cases. The last three matrices also pass 964
fresh Turbo C decodes. Hash-identity reuse for the other 9,648 cases is labeled
explicitly; their earlier C observations are not counted as fresh decodes.

From this package, with the configured references available:

```text
cargo build --release --example stream_encode
python tests/reference_stream_encode.py --native PATH/stream_encode.exe --ijg PATH/ijg-coefficients.exe --reference PATH/jpeg.exe --out experiments/stream-c
cargo test -p deadlib-image-jpeg --test stream_encode
python tests/independent.py
```

Reports are `tests/reports/stream-encode.json`, `stream-encode-regressions.json`
and `stream-encode-validation.json`. Broader conformance, incremental input and
abbreviated output, coverage-guided fuzzing and measured optimization remain
required. No application integration or production dependency is added.
