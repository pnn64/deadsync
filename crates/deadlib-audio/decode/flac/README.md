# deadlib-audio-decode-flac

Independent pure-Rust FLAC implementation under development, based on
[RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html). Registered in the Cargo
workspace only; it is not used by DeadSync playback. No production codec
dependencies; the only production dependency is the sibling Ogg container crate.

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
- `SeekReader<R: Read + Seek>`: native file-backed exact seeking, with validated
  SEEKTABLE hints, a lazy frame index and reusable streaming buffers.
- `FrameDecoder::find_frame`: CRC-validated synchronization in headerless input.
- `FrameEncoder`: reusable constant/verbatim/fixed/LPC encoding with exact Rice
  bit-cost selection, stereo decorrelation and compression levels 0..8.
- `Encoder<W: Write>`: planar or interleaved streaming input, metadata preservation,
  MD5 and seek-point generation. `finish_seekable` patches the native header;
  `finish` supports forward-only outputs with unknown totals/MD5 in their header.
- `OggDecoder`, `OggReader` and `OggStreamDecoder`: page, reader and incremental
  Ogg FLAC decoding with multiplexing, chaining, broadcast offsets and integrity.
- `OggSeekReader<R: Read + Seek>`: bounded full-file indexing and exact seeks
  among FLAC logical streams, without retaining the source or decoded PCM.
- `OggEncoder` and `OggMuxer`: streaming Ogg encoding and validated frame remuxing.

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
already be in memory. `SeekReader` instead reads metadata at construction and uses
file seeks plus validated SEEKTABLE hints; see [SEEKING.md](SEEKING.md).
Seeking to zero restarts MD5 verification; other seeks report
`Integrity::Partial` while retaining frame CRC and sequence checks.

Default resource limits: 64 MiB total metadata, 4,096 metadata blocks, 32 MiB per
frame. Increase `DecodeOptions::limits` for larger valid inputs. The official
combined-metadata vector needs a 128 MiB metadata budget. Compatibility with
pre-2007 variable-block streams is enabled by default and can be disabled through
`allow_legacy_variable_blocks`. Raw frame headers always expose the actual flag.

```rust
use deadlib_audio_decode_flac::{Encoder, EncodeOptions, Decoder};
use std::io::Cursor;
let mut encoder = Encoder::new(Cursor::new(Vec::new()), EncodeOptions::default())?;
encoder.write_interleaved(&[1, -1, 2, -2, 3, -3])?;
let encoded = encoder.finish_seekable()?.output.into_inner();
let mut decoder = Decoder::new(&encoded)?;
assert_eq!(decoder.read_next()?.unwrap().pcm.plane(0), &[1, 2, 3]);
# Ok::<(), deadlib_audio_decode_flac::Error>(())
```

See [ENCODING.md](ENCODING.md) for encoder options, streaming finalization and
independent roundtrip evidence.

Validation: 33 local integration tests plus three documentation examples; external
gates compare all 75 valid official corpus files exactly with libFLAC, check
1,533 seek positions and exercise all 11 faulty files. Symphonia 0.6.1 matches 72
files; three explicitly recorded reference differences remain. Another 696
generated encoder cases pass exact bidirectional libFLAC validation; Symphonia
matches 694, with two named 32-bit stereo reference differences. See
[COVERAGE.md](COVERAGE.md) for exact scope, commands, provenance and limitations.
Ogg adds another 696 bidirectional libFLAC cases, all exact with Symphonia's Ogg
demuxer and FLAC decoder, plus 73 official-file remuxes and 1,533 exact seeks.
See [OGG.md](OGG.md) for mapping behavior and reproducible evidence.
FFmpeg 9.0.2 additionally matches 74 official files and 72 Ogg remuxes; its one
bit-depth-change rejection is pinned. Another 1,279 explicit wire cases and
2,162 accepted fuzz-generated frames match both libFLAC and FFmpeg exactly.
See [PARITY.md](PARITY.md) for the decode contract, reports and reproduction.

See [PLAN.md](PLAN.md) for the full scope and unfinished milestones. Fuzz
targets now have recorded ASan campaigns; see [fuzz/README.md](fuzz/README.md).
Decode performance is competitive on the measured Intel x86-64 host: Windows raw
decoding leads 26 of 27 classes, and Linux/WSL raw decoding takes a median 0.912x
the fastest CRC-checking reference's time. See [PERFORMANCE.md](PERFORMANCE.md)
for streaming, startup, seek and memory measurements, remaining Linux gaps,
first-seek index costs and the limits of these comparisons.
