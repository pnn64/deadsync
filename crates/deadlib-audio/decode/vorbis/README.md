# deadlib-audio-decode-vorbis

Raw Vorbis packet APIs remain usable independently of Ogg:

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

`OggSeekReader` provides sample-accurate indexed seeking. Frames are relative to
the retained start of one logical stream; original broadcast timestamps remain
available in each `OggPcm`. Streams in multiplexed/chained input can be selected
independently, with their own rates and layouts.

```rust
use std::io::Cursor;
use deadlib_audio_decode_vorbis::{OggEvent, OggSeekReader};
# fn seek_example(ogg_bytes: &[u8]) -> Result<(), deadlib_audio_decode_vorbis::Error> {
let mut reader = OggSeekReader::new(Cursor::new(ogg_bytes))?;
let stream = reader.selected().info.id;
let frame = reader.selected().end.frames / 2;
reader.seek(stream, frame)?;
while reader.read_next(|event| {
    if let OggEvent::Pcm(data) = event {
        // The first data.start_frame equals frame, including within a packet.
        std::hint::black_box(data.pcm.plane(0));
    }
})? {}
# Ok(())
# }
```

Construction validates the complete container, then scans Vorbis headers, packet
modes and granules without synthesizing PCM. `VorbisIndex` caches metadata/setup
packets, one timing landmark per completed audio page, and a compact packet-mode
index. Two logical bits per packet identify short/long blocks or discarded
prefixes. Seek refines a page landmark to the nearest valid pre-roll packet
using those modes and exact overlap increments, avoiding synthesis of earlier
packets in the page interval. `SeekOptions::packet_preroll` defaults to true;
false retains page-only landmarks to save this storage. Both return the same
sample-accurate PCM. Vector capacity is included in `max_index_memory`.
The index can be built
separately and passed to `with_index`; `into_parts` preserves it for reuse with
the exact same immutable source. Audio floor/residue errors are still detected
when those packets are decoded. Use the streaming reader for low-latency startup
when a complete index is unnecessary.

`seek` resets overlap and selects the preceding audio packet; `read_next` primes
it, decodes forward and discards to the exact target. The exact stream end is a
valid target. Repeated seeks within one stream retain setup, transform tables
and synthesis buffers. Changing streams rebuilds the selected codec. Start/end
events repeat after each seek; `StreamEnd` describes the whole stream. A read
processes at most one selected-stream page and may emit several PCM blocks.
For short reads, `read_packet` uses the same overlap/trim logic while synthesizing
at most one packet per call. It retains a cursor into the verified page without
copying or rereading it. Calls may return progress with no PCM (pre-roll,
discarded packet, or spanning fragment). Consume each complete returned overlap
block before the next call; the method does not take a requested frame count.
Calls can alternate with `read_next`, which drains the current page's remainder.
Page granule/EOS checks complete when that page is consumed, so PCM remains
provisional until those checks and StreamEnd. Seeking abandons a paused page.
Invalid seek arguments preserve the cursor; read/seek IO failures are terminal
until a successful seek resets state. As with streaming, output is provisional
until end validation. Wrap unbuffered file sources in `BufReader`.

Packet prefix truncation discards the packet without advancing overlap. A
truncated floor produces a zero current window for every channel (spec 4.3.2)
and still overlaps preceding data. Residue truncation retains completed work;
type zero commits only complete partitions. Other errors preserve previous
overlap for caller recovery.
`Limits::max_decode_memory` bounds owned buffer/table elements separately from
setup storage. Nonfinite spectral/transform/overlap results are rejected.

Run `cargo test -p deadlib-audio-decode-vorbis`, or use its manifest path.
The parent `scripts/verify_independent.ps1` copies only the two crates into a
separate workspace, runs their tests offline and checks production dependency
trees. Optional [external tests](tests/oracle/README.md) require pinned C oracle
builds. Resource limits are configurable application policies, not format limits.
