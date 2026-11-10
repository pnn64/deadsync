# Raw file and streaming decoding

`StreamDecoder<R: Read>` reads a file, socket, cursor or other blocking byte
source and returns borrowed planar raw PCM through `next_frame()`. It handles
arbitrary short reads and retries interrupted reads. `Read::read()` returning
zero means EOF; other IO errors are terminal until `reset(new_source)`.
The PCM borrow ends before the next mutable decoder operation.

```rust
use deadlib_audio_decode_mp3::StreamDecoder;
fn main() -> Result<(), Box<dyn std::error::Error>> {
let file = std::fs::File::open("input.mp3")?;
let mut decoder = StreamDecoder::new(file);
while let Some(frame) = decoder.next_frame()? {
    // Positions count samples per channel, including metadata frames and delay.
    let _ = (frame.byte_offset, frame.position, frame.header.sample_rate());
    for plane in frame.pcm.planes() {
        let _ = plane;
    }
}
Ok(())
}
```

Raw coordinates include Info/Xing frames, encoder and decoder delay, and final
padding. They count per-channel sample instants even if the channel count or
sampling rate changes; they are not time durations or gapless coordinates.
`GaplessDecoder<R: Read>` adds bounded tagged trimming without seeking; see
[GAPLESS_STREAMING.md](GAPLESS_STREAMING.md). For seekable sources,
`IndexedDecoder` adds exact seeking as described in [SEEKING.md](SEEKING.md).
This raw streaming API retains all samples.

`PacketReader<R>` exposes complete physical packets without decoding PCM.
Packets include their original header, byte offset, free-format size and skipped
junk-byte count. Acquisition checks a compatible following header where
available, or validates the complete candidate's side information when the next
boundary contains junk. A lone final frame is accepted. Protection CRC and main
data are checked by the decoder, including the first packet. Buffered lookahead
is validated even when free-format probing has already discovered source EOF;
this keeps acquisition/recovery independent of the read schedule. Plausible embedded
headers in arbitrary junk remain inherently ambiguous; further adversarial and
fuzz validation is required.

Free-format size inference requires three compatible headers and accounts for
padding independently on each frame. An explicit `free_format` option permits
one- and two-frame inputs when their unpadded size is already known.

## Metadata and bounds

The packet buffer logically holds at most `2 * limits.frame_bytes + 4` bytes
(or 128 bytes for small frame limits). Capacity grows as needed and is reused.
The first compressed frame is retained separately for borrowed Xing/Info/LAME
or VBRI access through `metadata().vbr()`; malformed optional VBR fields are
reported when that metadata is parsed, independently of raw audio decoding.

ID3v2 headers, payloads and footers are preserved byte for byte with their
original offsets. Large tags read directly into their retained metadata buffer;
audio sync is never searched inside their declared payloads. ID3v1 is available
after its trailer is read. The cumulative encoded ID3 bytes are bounded by
`limits.metadata_bytes`. Unknown, compressed and encrypted metadata payloads remain opaque.

Sync searches are bounded by `limits.sync_bytes` between packets. Leading and
trailing junk is counted. A gap between audio packets is an error in strict
decoding mode. Recognized ID3 blocks between packets preserve decoder state.
`into_parts()` returns the original read-ahead source together with its unread
lookahead, so those bytes need not be lost. `byte_position()` on PacketReader
tracks consumed bytes rather than the source's read-ahead position.

## Errors, concealment and truncation

The default `Recovery::Strict` stops on bad side information, entropy errors,
protection failures, missing reservoir or loss of synchronization between
packets. `Recovery::Conceal` replaces a complete damaged frame with silence of
exactly that frame's sample count. `FrameStatus` distinguishes decoded,
missing-reservoir and corrupt output; statistics and the latest recovered error
remain available. Configuration/unsupported-format, allocation, resource-limit
and IO errors remain fatal in both modes.

Concealment retains the known preceding history and the damaged packet's main-data
bytes, exactly once, for later reservoir dependencies and resets DSP history.
Missing-reservoir errors retain the history
already accumulated by the raw decoder. After recovery, synthesis warmup affects
subsequent samples. Recovery preserves known physical-frame timing; skipped junk
does not supply enough information to invent missing-frame durations. Consumers
must use the reported gap and concealment flags when continuity matters.

An incomplete final audio frame defaults to `Error::Truncated`.
`TruncatedPolicy::Discard` explicitly drops that incomplete tail and reports its
byte count. Incomplete or malformed recognized ID3 metadata still errors in
either mode. Errors are terminal until reset; EOF is stable across repeated
calls. Reset replaces the source, clears metadata/counters and resets reservoir
and DSP history while retaining the packet buffer's allocation.
