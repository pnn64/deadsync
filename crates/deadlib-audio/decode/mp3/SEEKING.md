# Gapless files and exact seeking

`IndexedDecoder<R: Read + Seek>` scans physical frames, metadata, side information
and optionally CRC into an exact index. It does not cache decoded PCM. The source
must remain unchanged after opening. Entropy errors are detected while decoding;
opening is not a full-file PCM validation pass. The index is bounded by
`limits.index_frames`, and offsets include the source's initial seek position.

The default `SampleMode::Gapless` removes recognized Xing/Info frames and uses
recognized LAME-compatible encoder delay/padding. Its retained length is the
physical audio length minus encoder delay and padding. The PCM start additionally
accounts for the native synthesis delay of 529 samples. Tagged output ends at
physical EOF if padding is smaller than that delay, matching mpg123/Symphonia's
file behavior on real FFmpeg/Shine files.
Without recognized trim metadata, only the Xing/Info frame is removed. Unknown
encoder signatures do not supply invented delay/padding. `SampleMode::Trim`
allows an explicit caller-supplied trim and can drain a zero-spectrum granule
after physical EOF when that trim requests the finite DSP tail;
`SampleMode::Raw` retains all physical
samples, matching `StreamDecoder`.

Tagged frame-count mismatches and trim lengths exceeding the audio length error.
Tagged gapless mapping across sample-rate/channel changes is unsupported; the raw
coordinate mode supports those changes. VBRI timing tables are retained as
metadata. Seeking uses the physical index rather than approximate Xing/VBRI byte
estimates, so corrupt or imprecise seek hints cannot cause timing drift.

`samples()` and `position()` count sample instants per channel in the selected
coordinate mode. `sample_range()` maps them to raw coordinates. Each returned
`FileFrame` carries both coordinates and its physical byte offset/index; a DSP
drain has no physical byte offset. PCM is borrowed, planar, normalized and
unclipped. `Pcm::slice` preserves the physical plane stride.

`seek_sample` accepts every position through EOF. It primes the main-data
reservoir from earlier physical payloads and decodes two prior granules to
rebuild hybrid/synthesis history. It respects synchronization discontinuities
and uses the same strict/concealment and truncation policies as raw streaming.
Out-of-range requests preserve state. IO/decode failures require a successful
seek before further reads; a successful seek also resets that failure state.

```rust
use deadlib_audio_decode_mp3::IndexedDecoder;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder = IndexedDecoder::new(std::fs::File::open("input.mp3")?)?;
    decoder.seek_sample(decoder.samples() / 2)?;
    while let Some(frame) = decoder.next_frame()? {
        let _ = (frame.position, frame.raw_position, frame.pcm);
    }
    Ok(())
}
```
