# Gapless PCM without seeking

`GaplessDecoder<R: Read>` applies the default indexed file mapping to blocking
non-seekable streams. `StreamDecoder` remains the raw-coordinate API. Neither
reader registers a codec with DeadSync's playback pipeline.

```rust
use deadlib_audio_decode_mp3::GaplessDecoder;
fn decode(source: impl std::io::Read) -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = GaplessDecoder::new(source);
    while let Some(frame) = decoder.next_frame()? {
        let _ = (frame.position, frame.raw_position, frame.pcm);
    }
    Ok(())
}
```

Recognized Xing/Info frames are removed. Known LAME-compatible trim fields add
encoder delay plus the 529-sample decoder offset to the leading skip and retain
only `max(encoder_padding - 529, 0)` samples at the end. Tagged output stops at
physical EOF for short/zero padding, matching the default indexed behavior and
the independently checked Shine files. No DSP drain is invented at EOF. Unknown
encoder signatures supply no trim; tagless and VBRI streams retain raw PCM.

The reader does not wait for the complete file before returning PCM. Only the
trailing holdback and at most two physical PCM frames are stored, including a
borrowed/partly emitted front frame. Trim fields are 12-bit values, so PCM storage
is bounded independently of file length. Encoded buffering and retained metadata
use the existing separate `StreamOptions` limits.

Output is borrowed planar, normalized, unclipped f32. Audible `position` counts
samples per channel; `raw_position`, physical byte offset, skipped bytes and
recovery status remain available on each output piece. A physical frame may be
returned in multiple pieces while trailing padding is held back. `stats()` uses
raw physical counts; `position()` uses emitted coordinates. EOF is stable.

CRC, truncation, recovery, IO and version policies are inherited from
`StreamOptions`. Every error is terminal until `reset(new_source)`. Reset clears
queued PCM, trim state, positions, metadata and raw DSP/reservoir history.
Tagged trim across rate/channel changes is rejected, while tagless format
changes retain the raw reader's behavior.

False declared frame counts and impossible trim lengths are detected when EOF
establishes the physical length. A valid prefix may already have been returned;
streaming cannot retract it. Consumers must handle the terminal error rather
than treating that prefix as a successfully completed file. Seekable callers
can use `IndexedDecoder` to validate those counts during opening. Empty byte
streams are ordinary streaming EOF; the seekable constructor still rejects a
file containing no Layer III frames.
