# deadlib-audio-decode-mp3

Independent pure-Rust MPEG Audio Layer III implementation in development.
Registered in the Cargo workspace only; no playback integration and no
production dependencies. References are confined to tests and benchmarks.

The current foundation implements checked MSB bit fields, MPEG-1/2/2.5 Layer III
headers, CBR frame boundaries, free-format length inference, side information,
MPEG protection CRC, bounded main-data reservoir assembly, ID3v1/v2 framing,
ID3 unsynchronization, Xing/Info/LAME delay metadata and VBRI table framing.
Unknown ID3 frame payloads remain available. Compressed/encrypted tag payload
interpretation is not provided; these payloads are retained verbatim.

Native raw-frame PCM decoding is now implemented. `FrameDecoder` returns
borrowed planar, normalized, unclipped f32 PCM with its frame header and byte
count. It retains reservoir, hybrid overlap and synthesis history across calls.
`DecodeOptions` controls CRC verification and MPEG-2.5 acceptance. A physically
incomplete frame can be retried without consuming state; ordinary decode errors
require `reset()`. Missing-reservoir errors retain payload for later recovery.

`PacketReader<R: Read>` and `StreamDecoder<R: Read>` now read files and blocking
streams, retaining metadata and exact raw byte/sample coordinates. Recovery and
truncation are explicit policies; [STREAMING.md](STREAMING.md) documents them.
`IndexedDecoder<R: Read + Seek>` adds raw/gapless sample coordinates, exact
physical indexing and sample-accurate repeated seeks; see [SEEKING.md](SEEKING.md).
Encoding remains unfinished.
No complete standards-conformance or performance-completion claim is made.
See [PLAN.md](PLAN.md) for the full requested scope and verification gates.
The foundation's eight tests and 84-file/732-frame libmad comparison are recorded
in [COVERAGE.md](COVERAGE.md).
MPEG-1 SCFSI/scalefactors and all MPEG-2 LSF scalefactor partitions are now
implemented and tested separately, including intensity parameter widths.
All Layer III Huffman pairs, escapes, signs and count1 quadruples are implemented;
see [HUFFMAN.md](HUFFMAN.md) for numeric-table provenance and independent evidence.
Native hybrid/IMDCT and polyphase synthesis kernels now pass independent
all-frequency checks; [SYNTHESIS.md](SYNTHESIS.md) records their scope.
Raw PCM comparisons, pinned legacy conformance vectors, stereo disagreements
and current limitations are recorded in [RAW_PCM.md](RAW_PCM.md).
The independent [stereo equation audit](STEREO.md) verifies all 90 constructed
streams, including the intensity edge cases on which reference decoders differ.
Additional Shine, Windows Media Foundation and FFmpeg/LAME comparisons are
recorded in [ENCODER_COMPATIBILITY.md](ENCODER_COMPATIBILITY.md), including the
short-padding gapless correction and unusable encoder configurations.

```sh
cargo test --locked -p deadlib-audio-decode-mp3 --target-dir target/deadlib-mp3
```

MPEG-2.5 is an explicitly separate compatibility extension: header version bits
`00` select quarter-rate MPEG-1 frequencies (8, 11.025 and 12 kHz) with MPEG-2
Layer III granule and side-information syntax. Reserved version bits `01` are
rejected. Ordinary LAME-generated files pass PCM comparisons at all nine rates.
The 8 kHz mixed-block profile uses four long subbands and 72 long lines, following
the 1997 Layer3 syntax document. An independent all-frequency equation oracle
confirms this profile. Reference decoders differ on constructed mixed blocks;
see [MPEG25.md](MPEG25.md) for the exact evidence and compatibility scope.
