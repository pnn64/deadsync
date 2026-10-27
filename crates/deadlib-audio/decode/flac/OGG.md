# Ogg FLAC mapping and evidence

Implemented against RFC 9639 section 10.1 using `deadlib-audio-decode-ogg` for
physical pages, CRC, lacing, packet assembly, logical stream validation, writing,
indexing and seek capture. The raw FLAC codec does not depend on a container API.
No external codec is used in production. The application playback path is unchanged.

## APIs and mapping

- `OggDecoder::push_page` consumes verified borrowed pages.
- `OggReader<R: Read>::read_next` processes one page with borrowed event callbacks.
- `OggStreamDecoder::push` accepts arbitrary byte splits and consumes at most one
  page per call. Repeat with the remaining chunk; call `finish` at physical EOF.
- `OggSeekReader<R: Read + Seek>` validates and indexes a complete physical file
  from byte zero, selects a logical stream, and returns borrowed PCM frames.
- `OggEncoder<W: Write>` accepts exact planar/interleaved PCM through the same
  streaming scheduler and native frame encoder as native FLAC.
- `OggMuxer<W: Write>` validates complete encoded frames and metadata before
  remuxing. Existing comments are placed first; a vendor comment is supplied when
  absent. Other blocks, including unknown blocks and native SEEKTABLE payloads,
  are preserved. Native seek offsets are not reinterpreted as Ogg file offsets.

The decoder checks the 51-byte identification packet on its own 79-byte page,
mapping version 1.0, native signature and STREAMINFO, header packet counts (zero
means unknown), one metadata block per packet, last-block flags, a fresh page
before audio, one complete frame per packet, all checksums, contiguous coded
frame/sample numbers, granules, EOS and known sample totals. Each frame's actual
rate, channels and depth govern PCM reconstruction, including property changes.

Metadata pages use granule zero. The reader also accepts generic Ogg's unknown
granule on pages with no completed metadata packet; this exception is restricted
to FLAC metadata. Unfinished audio pages require the unknown-granule sentinel.
Writing always uses zero for metadata pages, including spanning metadata.

Multiplexed FLAC streams, physically validated ignored codecs and chained streams
are supported. A stream is identified by serial and chain. The underlying Ogg
implementation requires globally unique serials, including across chains.
`start_sample` is relative to the first available sample; `start_granule` includes
the first completed audio page's possibly positive origin. Coded frame numbers
and granule origins are checked independently, including partial broadcasts.
An initial encoded page is buffered until its granule is known, then released.
Callback output is provisional until successful logical EOS and physical finish.
Read errors are terminal until reset or an explicit seek starts a fresh capture.

Forward-only `OggEncoder` retains unknown STREAMINFO count and MD5 in its output,
and returns finalized values separately. Every page and frame has a checksum.
`OggMuxer` retains known STREAMINFO values and verifies the digest on complete
streams. A seekable Ogg encoder that patches its first page is not implemented.

## Exact file seeking

```rust
use deadlib_audio_decode_flac::{OggEncoder, OggSeekReader, EncodeOptions};
use std::io::Cursor;
let mut encoder = OggEncoder::new(Vec::new(), EncodeOptions::default(), Default::default())?;
encoder.write_interleaved(&[1, -1, 2, -2, 3, -3])?;
let encoded = encoder.finish()?.output;
let mut reader = OggSeekReader::new(Cursor::new(encoded), Default::default())?;
let stream = reader.streams()[0].id;
reader.seek(stream, 1)?;
let frame = reader.read_next()?.unwrap();
assert_eq!(frame.start_sample, 1);
assert_eq!(frame.pcm.plane(0), &[2, 3]);
# Ok::<(), deadlib_audio_decode_flac::Error>(())
```

Construction makes one forward pass without decompressing audio. It validates
Ogg structure, FLAC headers and packet CRCs, numbering, metadata and timestamps,
retaining page entries and a compact per-frame packet/sample index. It does not
validate subframe syntax or PCM MD5 until those frames are decoded. A seek binary
searches frame positions and resets the container packet capture at the packet's
starting page. Long packet continuations and multiple frames on one page work
without decoding earlier audio. PCM/predictor and assembly capacities survive
repeated seeks and stream changes. A nonzero seek reports `Integrity::Partial`;
a complete pass after seeking to zero checks a supplied MD5. Partial broadcasts
cannot verify a whole-source digest. EOF is a valid seek target.

Default budgets are 16 active FLAC streams, 128 MiB aggregate retained metadata,
the native per-stream limits, and the Ogg crate's container budgets. Seek indexing
additionally permits at most 1,048,576 pages and 4,194,304 frames. Indexed metadata
is retained across completed streams, so its aggregate budget covers the entire
file. Streaming metadata is released at logical EOS. Budgets are configurable.
The index and byte source must remain identical; external source mutation is not
supported. Startup remains a complete file scan, with index memory proportional
to pages and frames; no unmeasured constant-time or memory claim is made.

## Validation

Eight local Ogg tests cover every bit depth 4..32 and channel count 1..8,
every byte split for a small multi-frame file, 997-byte chunked reads, 140,000-byte
metadata and a 65,535-sample 8-channel 32-bit frame spanning pages, all-sample
seeks across multiplexed streams, chain selection, ignored codecs, empty streams,
nil pages, nonzero granule origins, nonzero fixed/variable coded starting numbers,
native MD5 preservation, unknown metadata, unknown header counts, truncations,
CRC errors, malformed mapping fields, budgets and error lifecycle.

External evidence (2026-10-27):

- **696 generated Ogg cases**: 29 depths x 8 channels x levels 0, 5 and 8;
  1,025 samples/channel, block size 256, deterministic noise or sinusoid plus
  noise, fixed and variable numbering. Native encode -> libFLAC decode and
  libFLAC encode -> native streaming/indexed decode are exact in every case.
  Five seeks per reference file verify the complete returned tail (3,480 total).
  Symphonia 0.6.1's Ogg demuxer plus FLAC decoder is exact for all 696. These
  vectors do not trigger the distinct 32-bit stereo regressions recorded in the
  native matrix; those remain explicitly tested by the native gates.
- **73 official native files remuxed to Ogg**: libFLAC's Ogg output PCM exactly
  matches its native output; this crate's indexed linear PCM and 21 seeks/file
  (1,533 total) match it. All 69 available MD5s verify, and four changing-property
  streams report unavailable. Two headerless official files are excluded because
  they do not supply native metadata for remuxing; native frame tests cover them.

Reports: [ogg-matrix.tsv](validation/ogg-matrix.tsv),
[ogg-corpus.tsv](validation/ogg-corpus.tsv),
[ogg-manifest.json](validation/ogg-manifest.json).
Input provenance and native corpus hashes are in [COVERAGE.md](COVERAGE.md) and
its linked manifest. Byte sizes are correctness-corpus observations, not
representative music compression or speed measurements. No Ogg fuzz campaign
has run yet.

## Reproduce

Prepare the pinned libFLAC and corpus checkouts as described in COVERAGE.md.
The C oracle also needs libogg 1.3.6, commit
`be05b13e98b048f0b5a0f5fa8ce514d56db5f822`. Existing checkouts may be reused:

```powershell
git clone --branch v1.3.6 https://github.com/xiph/ogg.git target/deadlib-audio-reference/ogg
git -C target/deadlib-audio-reference/ogg checkout be05b13e98b048f0b5a0f5fa8ce514d56db5f822
$reference = (Resolve-Path target/deadlib-flac-reference/flac).Path
$oggReference = (Resolve-Path target/deadlib-audio-reference/ogg).Path
cmake -S crates/deadlib-audio/decode/flac/tests/oracle -B target/deadlib-flac-reference/oracle-build "-DREFERENCE_ROOT=$reference" "-DOGG_REFERENCE_ROOT=$oggReference" -A x64
cmake --build target/deadlib-flac-reference/oracle-build --config Release --target deadlib_flac_oracle --parallel 4
$env:DEADLIB_FLAC_ORACLE = (Resolve-Path target/deadlib-flac-reference/oracle-build/Release/deadlib_flac_oracle.exe).Path
$env:DEADLIB_FLAC_CORPUS = (Resolve-Path target/deadlib-flac-reference/conformance).Path
$env:DEADLIB_FLAC_GENERATED = "$pwd/target/deadlib-flac-reference/generated"
$env:DEADLIB_FLAC_OGG_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/ogg-matrix.tsv"
$env:DEADLIB_FLAC_OGG_CORPUS_REPORT = "$pwd/crates/deadlib-audio/decode/flac/validation/ogg-corpus.tsv"
cargo test -p deadlib-audio-decode-flac --target-dir target/deadlib-flac --release --test ogg_reference -- --ignored --test-threads=1
```

On other platforms omit `-A x64` and adjust the oracle executable path. Explicit
external gates fail when prerequisites are missing. The oracle's `ogg INPUT`
and `encode-ogg INPUT.i32le OUTPUT.oga RATE CHANNELS BITS LEVEL BLOCK` commands
use libFLAC's public Ogg APIs. The C sources are validation-only dependencies.
