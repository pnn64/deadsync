# deadlib-audio-decode-flac

Independent pure-Rust FLAC implementation under development, based on
[RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html). Registered in the Cargo
workspace only; it is not used by DeadSync playback. No production dependencies.

Current APIs:

- Checked MSB-first bit reader/writer and bounded unary decoding.
- FLAC CRC-8/CRC-16 and native streaming MD5 (RFC 1321).
- STREAMINFO, SEEKTABLE, VORBIS_COMMENT, PICTURE and CUESHEET parsing.
- Borrowed metadata blocks retain ordering, unknown/application payloads and tag
  bytes. Tags and picture text are interpreted separately from structural parsing;
  non-UTF-8 contents are retained. Picture URIs are never fetched.
- Configurable metadata and frame byte budgets; checked metadata lengths.
- `FrameHeader` and `FrameDecoder`: independent raw frames, all fixed/LPC orders,
  both partitioned Rice methods and escapes, wasted bits, stereo decorrelation,
  header/frame CRCs, checked integer reconstruction and reusable flat buffers.
- `Reader<R: Read>`: bounded native streaming, frame numbering/sample counts,
  whole-stream MD5 verification and borrowed planar `i32` PCM. Frame properties
  can change mid-stream; consume each returned header's rate/channels/depth.
- `Decoder<'a>`: borrowed native input and exact seeking using a lazy frame index.
- `FrameDecoder::find_frame`: CRC-validated synchronization in headerless input.

```rust
use deadlib_audio_decode_flac::{Reader, Error};
fn decode(input: impl std::io::Read) -> Result<(), Error> {
    let mut reader = Reader::new(input)?;
    while let Some(frame) = reader.read_next()? {
        // Right-aligned original integer samples, no float conversion.
        for plane in frame.pcm.planes() { std::hint::black_box(plane); }
    }
    // Full sequential reads verify any supplied MD5 by default.
    println!("{:?}", reader.integrity());
    Ok(())
}
```

`Decoder::seek(sample)` makes the next PCM block begin at that exact sample,
including within-frame targets. The first forward seek decodes through its
target to discover frame boundaries; later seeks reuse the index. Input must
already be in memory. A file-backed seek adapter and direct SEEKTABLE acceleration
remain future work. Seeking to zero restarts MD5 verification; other seeks report
`Integrity::Partial` while retaining frame CRC and sequence checks.

Default resource limits: 64 MiB total metadata, 4,096 metadata blocks, 32 MiB per
frame. Increase `DecodeOptions::limits` for larger valid inputs. The official
combined-metadata vector needs a 128 MiB metadata budget. Compatibility with
pre-2007 variable-block streams is enabled by default and can be disabled through
`allow_legacy_variable_blocks`. Raw frame headers always expose the actual flag.

Validation: 13 local integration tests plus a documentation example; external
gates compare all 75 valid official corpus files exactly with libFLAC, check
1,533 seek positions and exercise all 11 faulty files. Symphonia 0.6.1 matches 72
files; three explicitly recorded reference differences remain. See
[COVERAGE.md](COVERAGE.md) for exact scope, commands, provenance and limitations.

See [PLAN.md](PLAN.md) for the full scope and unfinished milestones. Encoding,
Ogg mapping, fuzz campaigns and performance measurements remain to be implemented.
No performance comparison is claimed yet.
