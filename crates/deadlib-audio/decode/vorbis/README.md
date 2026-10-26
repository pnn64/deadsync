# deadlib-audio-decode-vorbis

Independent, in-development Vorbis I implementation. The only production
dependency is the sibling `deadlib-audio-decode-ogg` container. Symphonia and
libvorbis serve only as test oracles. Neither new crate is integrated into
DeadSync's decoder or playback pipeline.

Raw Vorbis packet APIs remain usable independently of Ogg:

* `bitpack::{BitReader, BitWriter}`: native LSB-first arbitrary-width integers
  (0..=32), lookahead and sticky end-of-packet semantics.
* `Identification::{parse, encode}`: stream parameters, bitrate hints, block
  sizes and framing validation. `channel_order` exposes mapping-zero speaker
  order for one through eight channels; larger counts remain explicitly unassigned.
* `Comments::{parse, encode, to_owned}`: borrowed eight-bit-clean metadata, duplicates and
  unknown/unstructured vectors preserved. `fields` and `values` expose valid
  field names and ASCII-insensitive lookup. `Metadata` owns fields in two flat
  allocations. Strict parsing is separate from nonfatal stream comment handling.
* `codebook::{Codebook, Huffman}`: ordered/unordered/sparse entry lengths,
  entry-order Huffman assignment, scalar and VQ decode, lookup types 0/1/2,
  exact integer lattice sizes and the specification's single-entry erratum.
* `Setup::parse`: codebooks, time placeholders, floor-zero/one configuration,
  all residue configurations, mapping-zero submaps/coupling, modes and framing.
  Index/range/tree validation precedes use. Floor-one neighbors and X ordering
  are precomputed. `read_audio_header` parses a raw packet's mode/window prefix.
* `PacketDecoder`: floor-zero/one curves, residue-zero/one/two, inverse channel
  coupling in reverse order, native FFT-based inverse MDCT, Vorbis windows and
  overlap-add. Output is borrowed planar, unclipped f32 in Vorbis channel order.
  The first block primes overlap and returns zero frames. `reset` keeps tables
  and allocations; decode/reset perform zero allocations after construction.
* `Imdct`: reusable native inverse transform for all legal 64..=8192 block sizes.

```rust
use deadlib_audio_decode_vorbis::{Identification, Limits, PacketDecoder, Setup};
# fn example(id_packet: &[u8], setup_packet: &[u8]) -> Result<(), deadlib_audio_decode_vorbis::Error> {
let id = Identification::parse(id_packet)?;
let setup = Setup::parse(setup_packet, &id, &Limits::default())?;
assert!(!setup.modes().is_empty());
let mut decoder = PacketDecoder::new(setup)?;
// decoder.decode(audio_packet)? returns borrowed PcmBlock data, or None for
// a non-audio packet / truncated mode prefix. Consume PCM before the next call.
decoder.reset();
# Ok(())
# }
```

Ogg Vorbis decoding uses the independent native container:

* `OggDecoder::push_page`: validates the header lifecycle and mapping, maintains
  independent overlap/timing/metadata for multiplexed streams and chains, and
  skips unknown codecs while validating their physical framing.
* `OggStreamDecoder::push`: accepts arbitrary byte chunks, consuming at most one
  page per call. Repeat until the supplied chunk is consumed, then call `finish`.
* `OggReader::read_next` and `decode_reader`: synchronous `Read` adapters. Use a
  `BufReader` for files/sockets to reduce small underlying reads.
* `OggEvent`: stream start (parameters and metadata), borrowed PCM, stream end,
  ignored codec and nonfatal comment error events. No PCM copying or queues are
  required. Codec storage is released at each logical EOS.

```rust
use deadlib_audio_decode_vorbis::{decode_reader, OggEvent, OggOptions};
# fn example(ogg_bytes: &[u8]) -> Result<(), deadlib_audio_decode_vorbis::Error> {
decode_reader(ogg_bytes, OggOptions::default(), |event| {
    if let OggEvent::Pcm(data) = event {
        // data.start_frame: position relative to this stream's retained start.
        // data.start_granule: original sample timestamp, including broadcast offset.
        for plane in data.pcm.planes() {
            // Consume each borrowed, unclipped f32 channel before returning.
            std::hint::black_box(plane);
        }
    }
})?;
# Ok(())
# }
```

Initial edits discard samples before time zero; a positive starting granule
preserves broadcast timestamps. EOS clips trailing padding, with exact retained
counts at `StreamEnd`. Rates/layouts/positions are per logical stream, rather
than a fabricated combined timeline across incompatible chains. A short stream
whose first audio page is also EOS interprets a reduced granule as tail padding;
its initial and final trim cannot be distinguished from that one landmark.

Mapping errors are terminal until reset. Callback output is provisional until
`StreamEnd` and final `finish` validation. Truncated comments or a missing comment
framing bit produce an empty metadata set and `CommentError`; wrong header
signatures, setup errors and explicit resource-limit failures remain fatal.
`OggOptions` bounds active streams and their aggregate retained setup, metadata
and synthesis elements; container packet assembly has independent limits.

**Seeking and native audio encoding remain pending.**
Raw decoder positions count overlap output before container initial/final trim.
There is no synthetic final flush; the container mapping supplies final length.
Header writers are not audio encoders. Do not replace the existing decoding
pipeline until the remaining conformance/performance gates pass.
See [COVERAGE.md](COVERAGE.md) and the parent work plan for the remaining scope.

Packet prefix truncation discards the packet without advancing overlap. A
truncated floor produces a zero current window for every channel (spec 4.3.2)
and still overlaps preceding data. Residue truncation retains complete vectors
decoded so far. Other errors preserve previous overlap for caller recovery.
`Limits::max_decode_memory` bounds owned buffer/table elements separately from
setup storage. Nonfinite spectral/transform/overlap results are rejected.

Run `cargo test -p deadlib-audio-decode-vorbis`, or use its manifest path.
The parent `scripts/verify_independent.ps1` copies only the two crates into a
separate workspace, runs their tests offline and checks production dependency
trees. Optional [external tests](tests/oracle/README.md) require pinned C oracle
builds. Resource limits are configurable application policies, not format limits.
